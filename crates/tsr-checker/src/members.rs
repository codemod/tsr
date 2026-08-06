//! Looking inside an object type: property access, `getPropertyOfType`, and
//! inherited members.
//!
//! Ported from `checker.go:11244`, `:11258`, `getPropertyOfTypeEx`
//! (`checker.go:18899`) and `getPropertyOfObjectType` (`:21403`).

use tsr_ast::Node;
use tsr_binder::{SymbolFlags, SymbolId};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

/// Which symbol table a property lookup should read, decided by the type's
/// shape before any `&mut self` call borrows the store back.
///
/// The two arms are upstream's two branches of `resolveAnonymousTypeMembers` /
/// `resolveDeclaredMembers`, not a convenience: they read *different tables* of
/// the same symbol, and conflating them is the wrong-answer case
/// `crate::symbols` documents.
#[derive(Clone, Copy)]
enum Owner {
    /// The instance side: read `members`, then walk base types.
    Declared(SymbolId),
    /// The `typeof X` side: read `exports`.
    Anonymous(SymbolId),
}

impl Checker<'_, '_> {
    /// Ported from `Checker.checkPropertyAccessExpression` into
    /// `checkPropertyAccessExpressionOrQualifiedName` (`checker.go:11244`,
    /// `:11258`), reduced to the lookup.
    ///
    /// Upstream takes the receiver's **apparent** type first, which is what makes
    /// `"a".length` work: a primitive's apparent type is its wrapper interface
    /// from `lib.d.ts`. That is [`Checker::apparent_type`], and it is now taken
    /// here.
    ///
    /// > **This comment used to say "there are no lib files (`bd tsr-9or.1`), so
    /// > a primitive receiver has no members here"**, and that has been false
    /// > since the program started loading `internal/bundled/libs`. The gap it
    /// > described was being attributed to lib rather than to this function, and
    /// > it was this function's. Corrected rather than deleted, because
    /// > `docs/conventions.md` records four stale comments outliving their truth
    /// > in one session and this was a fifth.
    ///
    /// Not ported: optional chains, private identifiers, `super`, and index
    /// signatures. All answer `errorType`.
    pub fn check_property_access_expression(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(receiver), Some(tsr_ast::MemberName::Identifier(name))) =
            (node.expression, node.name)
        else {
            return error;
        };
        let receiver_type = self.check_expression(receiver);
        if receiver_type == error {
            return error;
        }
        // `checkPropertyAccessExpression` hands `checkNonNullExpression(expr)`
        // to the lookup (`checker.go:11258`), and a chain link goes through
        // `checkPropertyAccessChain` (`checker.go:11253`) — the receiver is
        // stripped of `null`/`undefined` (the "possibly undefined" report is a
        // *diagnostic*, a channel this port does not have — ADR-0040), the
        // lookup and flow narrowing run on the remainder, and
        // `propagateOptionalTypeMarker` (`checker.go:29082`) unions
        // `undefined` back in when `?.` stripped anything.
        // `docs/architecture/checker-notes-nnaccess.md`.
        let non_optional = self.get_optional_expression_type(
            receiver_type,
            receiver.node_id(),
            node.question_dot_token.is_some(),
        );
        let stripped = self.check_non_null_type(non_optional);
        if stripped == error {
            return error;
        }
        let result = self.access_member_lookup(stripped, name.text, node.node_id);
        if result == error {
            return error;
        }
        self.propagate_optional_type_marker(result, non_optional != receiver_type)
    }

    /// `checkNonNullType` (`checker.go:7409`), without the diagnostics: an
    /// `unknown` receiver is `errorType` in strict mode; a nullable one is
    /// answered by its non-nullable remainder; a remainder that is itself
    /// nullable or `never` refuses.
    pub(crate) fn check_non_null_type(&mut self, id: TypeId) -> TypeId {
        let error = self.intrinsics.error;
        if self.store.get(id).flags.intersects(crate::flags::TypeFlags::UNKNOWN) {
            return error;
        }
        let non_nullable = self.get_non_nullable_type(id);
        if non_nullable == id {
            return id;
        }
        let flags = self.store.get(non_nullable).flags;
        if flags.intersects(crate::flags::TypeFlags::NULLABLE.union(crate::flags::TypeFlags::NEVER))
        {
            return error;
        }
        non_nullable
    }

    /// `GetNonNullableType` (`checker.go:18663`): the `NEUndefinedOrNull`
    /// facts filter, which for the shapes this port builds is the flag test —
    /// a `null`/`undefined` constituent carries `TypeFlags::NULLABLE` and
    /// nothing else does.
    pub(crate) fn get_non_nullable_type(&mut self, id: TypeId) -> TypeId {
        self.filter_type(id, |checker, constituent| {
            !checker.store.get(constituent).flags.intersects(crate::flags::TypeFlags::NULLABLE)
        })
    }

    /// `getOptionalExpressionType` (`checker.go:29064`): a chain **root**
    /// strips nullable outright; an inner link removes the propagated marker.
    ///
    /// Upstream's marker is `optionalType`, an `undefined` distinct from the
    /// real one; this port has one `undefined`, and the divergence is owned in
    /// `checker-notes-nnaccess.md` §2 — the type answers coincide because
    /// upstream's own `checkNonNullType` strips a genuine `undefined` on the
    /// same path.
    pub(crate) fn get_optional_expression_type(
        &mut self,
        expression_type: TypeId,
        receiver: Option<tsr_ast::NodeId>,
        node_is_chain_root: bool,
    ) -> TypeId {
        if node_is_chain_root {
            return self.get_non_nullable_type(expression_type);
        }
        if receiver.is_some_and(|id| self.expression_is_optional_chain(id)) {
            return self.filter_type(expression_type, |checker, constituent| {
                !checker.store.get(constituent).flags.intersects(crate::flags::TypeFlags::UNDEFINED)
            });
        }
        expression_type
    }

    /// `propagateOptionalTypeMarker` (`checker.go:29082`): when the chain
    /// stripped anything, `undefined` joins the result. Upstream distinguishes
    /// the outermost link (`getOptionalType`, a real `undefined`) from an
    /// inner one (the marker); with one `undefined` the two are the same
    /// union.
    pub(crate) fn propagate_optional_type_marker(
        &mut self,
        id: TypeId,
        was_optional: bool,
    ) -> TypeId {
        if !was_optional {
            return id;
        }
        let undefined = self.intrinsics.undefined;
        self.get_union_type(&[id, undefined])
    }

    /// Whether this expression is a link of an optional chain: it, or an
    /// access/call/non-null assertion on its receiver spine, carries `?.`.
    /// Upstream stores this as `NodeFlagsOptionalChain`, stamped by the
    /// parser; this port derives it by walking the spine, which a parenthesis
    /// deliberately breaks — `(a?.b).c` is not a chain link, exactly as
    /// upstream's flag propagation stops at the parenthesis.
    fn expression_is_optional_chain(&self, id: tsr_ast::NodeId) -> bool {
        let mut current = id;
        loop {
            let next = match self.node_map.get(current) {
                Some(Node::PropertyAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    access.expression.and_then(|e| e.node_id())
                }
                Some(Node::ElementAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    access.expression.and_then(|e| e.node_id())
                }
                Some(Node::CallExpression(call)) => {
                    if call.question_dot_token.is_some() {
                        return true;
                    }
                    call.expression.and_then(|e| e.node_id())
                }
                Some(Node::NonNullExpression(assertion)) => {
                    assertion.expression.and_then(|e| e.node_id())
                }
                _ => return false,
            };
            let Some(next) = next else { return false };
            current = next;
        }
    }

    /// Ported from `Checker.checkQualifiedName` (`checker.go:8122`), which is
    /// one call into the same `checkPropertyAccessExpressionOrQualifiedName`
    /// tail as a property access — the reason the lookup below is shared
    /// rather than duplicated. Reached from `typeof A.B` in type position
    /// (`bd tsr-4sc.10`); an expression-position qualified name does not exist
    /// in the grammar outside import assignments.
    ///
    /// `typeof this.x` is refused whole-construct: upstream routes a `this`
    /// left through `checkThisExpression` plus `checkNonNullType`
    /// (`checker.go:8125`), a per-container answer this port only partially
    /// has. `docs/architecture/checker-notes-tquery.md` §4 owns the refusal.
    pub fn check_qualified_name(&mut self, node: &tsr_ast::QualifiedName<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(left), Some(right)) = (node.left, node.right) else {
            return error;
        };
        let left_type = match left {
            tsr_ast::EntityName::Identifier(identifier) if identifier.text == "this" => {
                return error;
            }
            tsr_ast::EntityName::Identifier(identifier) => {
                self.check_expression(tsr_ast::Expression::Identifier(identifier))
            }
            tsr_ast::EntityName::QualifiedName(inner) => self.check_qualified_name(inner),
        };
        // `checkQualifiedName` routes the left through `checkNonNullExpression`
        // (`checker.go:8127`) exactly as a property access does its receiver.
        let left_type = self.check_non_null_type(left_type);
        self.access_member_lookup(left_type, right.text, node.node_id)
    }

    /// The shared tail of `checkPropertyAccessExpressionOrQualifiedName`
    /// (`checker.go:11244`): apparent type, the `any` fast path, the member
    /// lookup, and flow narrowing keyed on the access node itself.
    fn access_member_lookup(
        &mut self,
        receiver_type: TypeId,
        name: &str,
        node_id: Option<tsr_ast::NodeId>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        // `isAnyLike` (`checker.go:11266`) and the branch it guards
        // (`checker.go:11314`): a property access on `any` is `any`, whatever
        // the property name.
        //
        // This is the other half of the same cause as element access — see
        // [`crate::indexed`]. 6,329 of 27,140 property-access lines in the
        // baselines answer `any`, against a gap row of 8,154, so an `any`
        // receiver is most of what this row is. `anyPropertyAccess.types`
        // records both spellings failing together, which is why they are ported
        // together.
        //
        // **Upstream distinguishes `errorType` from `anyType` inside this very
        // branch** — `if c.isErrorType(apparentType) { return c.errorType }`
        // (`checker.go:11318`) sits between `isAnyLike` and the return. This
        // port gets that for free by testing **identity** against
        // `intrinsics.any`: `errorType` is a different type with the same `ANY`
        // flag, so it cannot match, and a `TypeFlags::ANY` test would answer
        // `any` for every gap in the corpus. That is the single most dangerous
        // edit that could be made to this function, and it would look like a
        // large win in the measurement.
        // # This arm produces wrong lines, and they are not its fault
        //
        // **Measured: +555 wrong property-access lines when this landed**, against
        // 894 gaps closed. They are all one shape, and the owner is elsewhere.
        //
        // This port has **no contextual typing**, so an unannotated parameter is
        // the implicit `any` here where upstream infers a real type from the
        // contextual signature — `getContextuallyTypedParameterType`
        // (`checker.go:29458`), reached through `assignContextualParameterTypes`
        // (`checker.go:10349`). Probe: `const y = x => x.foo;` answers
        // `x.foo : any` here, while upstream in `arr.map(x => x.foo)` types `x`
        // from context and answers a real member type.
        //
        // So the receiver is wrong before this arm sees it, and the arm then
        // converts what used to be an honest gap into a confident claim. Those
        // lines cost no gradient and no cases — the receiver's own line was
        // already wrong, so every case containing one was already failing — but
        // they do cost **diagnostic separability**, which is the thing this
        // project's method rests on. Someone reading a property-access histogram
        // will see 555 wrong lines and cannot tell from the instrument that they
        // are contextual-typing lines rather than a defect here.
        //
        // **Closing contextual typing is what removes them.** That is recorded
        // here rather than left to be rediscovered, because a limitation naming
        // no owner is how four stale comments outlived their truth this session.
        // **The apparent type is taken before the `any` test, as upstream takes
        // it** (`checker.go:11265`, `apparentType := c.getApparentType(...)`,
        // one line above `isAnyLike`). The order is not cosmetic: upstream's
        // `isAnyLike` asks about the *apparent* type, so a receiver whose
        // apparent type is `any` takes the `any` path even when the original
        // was not. Reversing it here would answer `any` for a primitive whose
        // global interface is missing — the direction that manufactures
        // confident wrong answers.
        let receiver_type = self.apparent_type(receiver_type);
        if receiver_type == self.intrinsics.any {
            return self.intrinsics.any;
        }
        // No explicit test for an `errorType` receiver: it is an intrinsic and
        // never carries a members table, so the lookup below misses and answers
        // `errorType` anyway. An earlier draft guarded it and no mutation could
        // make the guard observable, so it was removed rather than kept as
        // decoration. The identity test above is what keeps that true now that
        // an `ANY`-flagged type has a fast path.
        let property_type = self.get_type_of_property_of_type(receiver_type, name).unwrap_or(error);
        // `checkPropertyAccessExpressionOrQualifiedName` ends by narrowing the
        // property's declared type by the flow reaching this access
        // (`getFlowTypeOfReference`, `checker.go:11430`) — `bd tsr-6ka`. Until
        // this call existed, *no* property access reached the flow walk, which
        // made every ported narrowing guard dead on the forms the corpus
        // actually writes (`a.b !== undefined`, `o.foo != null`).
        //
        // The binder already records a flow node for a narrowable access
        // (`record_flow`'s `PropertyAccessExpression` arm), so nothing in the
        // binder moves; `get_flow_type_of_reference` returns the declared type
        // unchanged when there is none.
        //
        // A gap is not narrowed: `errorType` carries `TypeFlags::ANY`, and
        // filtering it would answer `never` for a property this port could not
        // type — a confident wrong line out of a missing one.
        let Some(id) = node_id else { return property_type };
        if property_type == error {
            return property_type;
        }
        self.get_flow_type_of_reference(id, None, property_type)
    }

    /// The type whose members a property access should be looked up in.
    ///
    /// Ported from `Checker.getApparentType` (`checker.go:21729`, from `grep -n`
    /// on the declaration), **primitive arms only** — the five `switch` cases
    /// that map a primitive to its global wrapper interface, in upstream's own
    /// order.
    ///
    /// # What is deliberately not ported, and who owns each
    ///
    /// Upstream's `getApparentType` has five arms before these five and three
    /// after. Every one of them is in a file this workstream does not own, and
    /// each returns the type unchanged here rather than guessing:
    ///
    /// | upstream arm | what it needs | owner |
    /// |---|---|---|
    /// | `TypeFlagsInstantiable` → base constraint | `getBaseConstraintOfType` | type parameters |
    /// | `ObjectFlagsMapped` | mapped types | not ported at all |
    /// | `ObjectFlagsReference` → `getTypeWithThisArgument` | instantiation | `bd tsr-4qx` |
    /// | `TypeFlagsIntersection` | `getApparentTypeOfIntersectionType` | `intersections.rs` |
    /// | `TypeFlagsNonPrimitive` / `Index` / `Unknown` | `emptyObjectType`, `stringNumberSymbolType` | `intrinsics.rs` |
    ///
    /// **This is why the slice is 1,165 lines and not 13,156.** The row
    /// `property access, the receiver has no such property` blocks 13,156 gap
    /// lines; measured by `examples/gaproot.rs`, 36.7% of them have an
    /// instantiated-generic receiver and 9.6% an array receiver — the
    /// `ObjectFlagsReference` arm, which is `bd tsr-4qx` — and only 11.3% have a
    /// primitive one. See `docs/architecture/checker-notes-gaproot.md` Part 2.
    ///
    /// # A missing global is a gap, never `any`
    ///
    /// `globals()` may not hold `String` — a lib-less unit test never does. The
    /// original type is returned then, and the lookup below misses and answers
    /// `errorType`. Returning `anyType` or `emptyObjectType` instead would turn
    /// a lib configuration into a confident wrong answer on every primitive
    /// member access in the corpus.
    ///
    /// Likewise a global whose *declared type* gaps: `get_declared_type_of_symbol`
    /// answering `errorType` means the interface is there and unreadable, and the
    /// original primitive is the more honest receiver to fail on.
    fn apparent_type(&mut self, id: TypeId) -> TypeId {
        // Upstream's order, arm for arm (`checker.go:21745-21751`). `NUMBER_LIKE`
        // carrying `ENUM` is upstream's too, not a widening added here.
        //
        // A union carries `TypeFlags::UNION` and **not** its constituents'
        // flags (`crate::unions::create_union`), with the single exception of
        // `false | true`, which is given `BOOLEAN` deliberately so that it *is*
        // `boolean`. So `string | number` reaches no arm here and `boolean`
        // reaches the fourth — which is exactly upstream's behaviour, and the
        // reason this needs no union guard of its own.
        let flags = self.store.get(id).flags;
        let global = if flags.intersects(TypeFlags::STRING_LIKE) {
            "String"
        } else if flags.intersects(TypeFlags::NUMBER_LIKE) {
            "Number"
        } else if flags.intersects(TypeFlags::BIG_INT_LIKE) {
            "BigInt"
        } else if flags.intersects(TypeFlags::BOOLEAN_LIKE) {
            "Boolean"
        } else if flags.intersects(TypeFlags::ES_SYMBOL_LIKE) {
            "Symbol"
        } else {
            return id;
        };
        // Not `crate::declared::global_type_symbol`, which gates on the symbol
        // having **exactly one** type parameter — right for `Array<T>` and
        // `Promise<T>`, and wrong for every interface here, all of which have
        // none. Reusing it would have made this arm silently dead.
        let Some(&symbol) = self.binder.globals().get(global) else { return id };
        let declared = self.get_declared_type_of_symbol(symbol);
        if declared == self.intrinsics.error { id } else { declared }
    }

    /// The **type** of a property of `id`, or `None` if there is no such
    /// property.
    ///
    /// Ported from `Checker.getTypeOfPropertyOfType` (`checker.go:18951`),
    /// which is three lines and the same three: look the property up, and if it
    /// is there, take its type.
    ///
    /// # Why a two-line function is worth having
    ///
    /// Not for the deduplication. `get_property_of_type` returns a
    /// [`SymbolId`], and a symbol is the *uninstantiated* declaration: on a
    /// receiver `C<number>` whose member is declared `a: T`, the symbol's type
    /// is `T` and every caller that turns the symbol into a type on its own
    /// gets `T` where upstream answers `number`. There were three such callers
    /// — property access (here), element access (`crate::indexed`) and both
    /// sides of the relater's structural comparison (`crate::relater`) — and
    /// substituting in one of them would leave the other two answering a
    /// **confident wrong type**. At the relater that is the dangerous
    /// direction: it acts on a `false`, so a wrong member type promotes the
    /// next overload candidate rather than degrading to a gap. See
    /// *"A conservative `false` is safe for one kind of consumer and unsafe for
    /// the other"* in `docs/conventions.md`.
    ///
    /// So this is the single place instantiation can be added once and be true
    /// everywhere, which is `bd tsr-4qx` — and it is now added: on a receiver
    /// that came out of `create_type_reference`, the found property's declared
    /// type is substituted through [`Checker::instantiate_type`] with the
    /// target's own type parameters mapped to the reference's arguments. A
    /// member whose declared type this port cannot rebuild — baked signature
    /// text, a tuple — falls through `instantiate_type`'s arms to `errorType`,
    /// an honest gap rather than the uninstantiated `T`.
    ///
    /// `None` means *no such property*, exactly as upstream's `nil` does. A
    /// property that exists and whose type this port cannot compute answers
    /// `Some(errorType)`, which is what keeps the relater's existence test
    /// (`crate::relater`'s `properties_related_to`) meaning what it did.
    #[must_use]
    pub fn get_type_of_property_of_type(&mut self, id: TypeId, name: &str) -> Option<TypeId> {
        // A tuple's numeric-literal property IS its element. Upstream reaches
        // this through the synthesised tuple target's members
        // (`createNormalizedTupleType`) and the reference's resolved type
        // arguments; this port's tuple stores its element list in
        // `tuple_element_lists` (`bd tsr-5ll`) and answers from it directly.
        // Exact by construction — the modifier tuple forms refuse at the mint,
        // so every recorded element is a plain one. Out of range answers
        // `undefined`: §8's registration guessed a gap there and the baseline
        // corrected it before the code ran — `indexerWithTuple.types` records
        // `>strNumTuple[2] : undefined` (the TS2493 diagnostic lives beside
        // it, not in the type). The round-trip test rejects `"01"`/`"-0"`,
        // which name no element. `checker-notes-tuple.md` §8.
        if let Some((elements, _)) = self.tuple_element_lists.get(&id)
            && let Ok(index) = name.parse::<usize>()
            && index.to_string() == name
        {
            return Some(elements.get(index).copied().unwrap_or(self.intrinsics.undefined));
        }
        let property = self.get_property_of_type(id, name)?;
        let declared = self.get_type_of_symbol(property);
        Some(self.instantiate_for_reference(id, declared))
    }

    /// A member's type as seen through an instantiated reference: `declared`
    /// with the receiver's type arguments substituted in, or `declared`
    /// unchanged when the receiver is not an instantiated reference.
    ///
    /// The instantiation half of `getTypeOfPropertyOfType` reached through
    /// upstream's `instantiateSymbol` (`checker.go:19676`) — upstream
    /// instantiates the *symbol* when members are resolved and the type falls
    /// out; this port has no instantiated symbols, so the same substitution is
    /// applied to the type at the one seam every consumer shares.
    ///
    /// An arity mismatch between the target's parameters and the reference's
    /// arguments — or a parameter list this port cannot resolve — answers
    /// `errorType` rather than substituting partially.
    pub(crate) fn instantiate_for_reference(
        &mut self,
        receiver: TypeId,
        declared: TypeId,
    ) -> TypeId {
        let Some((symbol, arguments)) = self.type_reference_targets.get(&receiver).cloned() else {
            return declared;
        };
        let error = self.intrinsics.error;
        let Some(parameters) = self.local_type_parameter_types_of(symbol) else {
            return error;
        };
        if parameters.len() != arguments.len() {
            return error;
        }
        let names = parameters.iter().map(|(_, name)| name.as_str()).collect::<Vec<_>>();
        let types = parameters.iter().map(|&(id, _)| id).collect::<Vec<_>>();
        let map = types.iter().copied().zip(arguments).collect::<Vec<_>>();
        self.instantiate_type(declared, &map, &types, &names)
    }

    /// Ported from `Checker.getPropertyOfTypeEx` (`checker.go:18899`) through
    /// `getPropertyOfObjectType` (`:21403`).
    ///
    /// # Inherited members are found by walking base types, not by flattening
    ///
    /// Upstream does **not** put a base class's properties in the derived
    /// symbol's members table. `resolveDeclaredMembers` (`checker.go:19612`) takes
    /// exactly `getMembersOfSymbol(t.symbol)` — the declared members and nothing
    /// else — and `resolveObjectTypeMembers` layers the base types' properties
    /// *underneath* them, so a derived declaration shadows the base's and the
    /// answer for an inherited name is still **the base's symbol**. Flattening the
    /// two tables in the binder would merge identities upstream keeps apart and
    /// would answer with the wrong symbol for an overridden member, so this walks
    /// instead: own members first, then each base in declaration order, first hit
    /// wins.
    ///
    /// # The walk is over symbols, not types
    ///
    /// Upstream reaches a base through `resolvedBaseTypes`, which are `*Type`s.
    /// Here `TypeData::Named` carries the owning [`SymbolId`] and
    /// `getDeclaredTypeOfClassOrInterface` is `new_named_type(symbol, …)` — one
    /// type per symbol, with no members of its own yet
    /// (`crate::declared::get_declared_type_of_class_or_interface`). Going
    /// type → symbol → base symbol and back is therefore the same graph with one
    /// indirection removed, and it avoids creating a type for every base merely to
    /// read its symbol back out.
    ///
    /// **The consequence accepted:** the moment `getDeclaredTypeOfClassOrInterface`
    /// grows real member resolution — the instantiated members of `class C extends
    /// B<number>` — this must move to the type level, because a symbol has no place
    /// to hold an instantiated table. That is the falsifier for this shape.
    ///
    /// # `symbolIsValue`
    ///
    /// The gate is upstream's (`symbolIsValueEx`, `checker.go:22095`) and it is not
    /// cosmetic here: a class's or interface's members table also holds its **type
    /// parameters** (`declareSymbolAndAddToSymbolTable` →
    /// `declareClassMember`, `internal/binder/binder.go:429-441`), so without it
    /// `new C().T` would answer with the type parameter `T`. The alias half of
    /// upstream's test is not ported — nothing follows aliases yet
    /// (`bd tsr-y4u.12`) — so an alias member is a miss rather than a wrong answer.
    ///
    /// # Two tables, chosen by what the type *is*
    ///
    /// A `TypeData::Named` is the instance side and looks in `members`; a
    /// `TypeData::Anonymous` is `typeof X` and looks in `exports`. See
    /// [`Checker::get_property_of_anonymous_symbol`] for why that is not the same
    /// table with a different name.
    #[must_use]
    pub fn get_property_of_type(&mut self, id: TypeId, name: &str) -> Option<SymbolId> {
        // The borrow of `self.store` has to end before the recursion below, which
        // takes `&mut self`. Both bindings are `Copy`, so this statement copies
        // out what it needs and releases the type. ADR-0013's read-drop-recurse.
        let owner = match &self.store.get(id).data {
            TypeData::Named { members: Some(owner), .. } => Owner::Declared(*owner),
            TypeData::Anonymous { symbol, .. } => Owner::Anonymous(*symbol),
            _ => return None,
        };
        match owner {
            Owner::Declared(owner) => {
                let mut visiting = Vec::new();
                self.get_property_of_declared_symbol(owner, name, &mut visiting)
            }
            Owner::Anonymous(symbol) => self.get_property_of_anonymous_symbol(symbol, name),
        }
    }

    /// A property of `typeof X` — the static side of a class, the exports of a
    /// namespace, the members of an enum.
    ///
    /// Ported from the tail of `Checker.resolveAnonymousTypeMembers`
    /// (`checker.go:20650`), whose third and last branch is introduced by the
    /// comment *"Combinations of function, class, enum and module"*
    /// (`checker.go:20671`) and reads
    /// `members := c.getExportsOfSymbol(symbol)` (`checker.go:20672`).
    ///
    /// # `exports`, not `members`, and the distinction is the whole point
    ///
    /// `crate::symbols` records why `TypeData::Anonymous` deliberately carried no
    /// members table until now: a class's statics and a namespace's exports live
    /// in the symbol's `exports`, and pointing this lookup at `members` would
    /// resolve `C.x` against the **instance** members — answering the wrong
    /// symbol rather than none. That constraint is why the fix is a second table
    /// consulted for a second type shape, and not a repointing of the existing
    /// walk. `Named` still reads `members`; nothing about the instance side moves.
    ///
    /// # The flags gate is upstream's branch, not a filter
    ///
    /// `resolveAnonymousTypeMembers` reaches the exports line only after two
    /// earlier returns: an instantiated type (`checker.go:20652`) and a
    /// **type-literal** symbol (`checker.go:20662`), which takes `getMembersOfSymbol`
    /// instead. The second one is live here: `crate::function_types` builds an
    /// anonymous type over `bindFunctionOrConstructorType`'s `__type` symbol.
    ///
    /// **This gate is unobservable today, and that is stated rather than
    /// implied.** Deleting it reddens no test in the workspace — I ran that
    /// mutation — because the only symbol it excludes is `__type`, whose
    /// `exports` table is empty, so gated and ungated both miss. It is kept
    /// rather than removed because the two are equal only by accident: the
    /// moment a `__type` or object-literal symbol carries an export, the ungated
    /// form answers from a table upstream never reads, and that is a wrong answer
    /// rather than a gap. The named edit that makes it bite is
    /// `crate::function_types` gaining the `__call`/`__new` member lookup, or
    /// `crate::objects` building `TypeData::Anonymous` for an object literal.
    /// Contrast [`Checker::check_property_access_expression`]'s removed
    /// `errorType` guard, which was dropped because its fallthrough was
    /// *provably* identical, not merely identical for now.
    ///
    /// # What is a miss here, and why a miss is safe
    ///
    /// Unlike [`Checker::base_symbols_of`], an unfollowable case here costs
    /// nothing but a gap: a miss returns `None`, which
    /// [`Checker::check_property_access_expression`] turns into `errorType`. There
    /// is no ordering hazard, because upstream layers inherited statics
    /// *underneath* own ones (`addInheritedMembers`, `checker.go:20690` — it adds
    /// only names not already present), so a name found in the symbol's own
    /// `exports` is always the symbol upstream would have answered with.
    ///
    /// Three things are therefore gaps rather than wrong answers:
    ///
    /// - **Statics inherited from a base class.** `class B { static x = 1 }` with
    ///   `class C extends B {}` gives `C.x` upstream through
    ///   `getBaseConstructorTypeOfClass` (`checker.go:20687`), which needs
    ///   construct signatures (`bd tsr-4sc.8`). `C`'s own statics are unaffected.
    /// - **`globalThis`** (`checker.go:20674`), which has no symbol here.
    /// - **An enum's numeric index signature** (`checker.go:20703`), so `E[0]`
    ///   stays a gap; index signatures are `bd tsr-4sc.8`'s.
    ///
    /// The `symbolIsValue` gate is upstream's own
    /// (`getPropertyOfTypeEx`, `checker.go:18916`) and matters more here than on
    /// the instance side: a namespace's `exports` holds its exported *types* too,
    /// so without it `namespace M { export interface I {} }` would answer `M.I`
    /// with an interface symbol in a value position.
    fn get_property_of_anonymous_symbol(&self, symbol: SymbolId, name: &str) -> Option<SymbolId> {
        let data = self.binder.symbols().get(symbol);
        if !data.flags.intersects(
            SymbolFlags::FUNCTION
                | SymbolFlags::METHOD
                | SymbolFlags::CLASS
                | SymbolFlags::ENUM
                | SymbolFlags::VALUE_MODULE,
        ) {
            return None;
        }
        let found = *data.exports.get(name)?;
        self.symbol_is_value(found).then_some(found)
    }

    /// One step of the walk: `owner`'s own members, then its base types'.
    ///
    /// # The circularity guard is load-bearing
    ///
    /// `class A extends B {}` with `class B extends A {}` is a real cycle in the
    /// base-type graph and the corpus contains such cases deliberately. Upstream
    /// guards it in `resolveBaseTypesOfClass`, which parks a `resolvingEmptyArray`
    /// sentinel in `resolvedBaseTypes` and reports
    /// `Type_0_recursively_references_itself_as_a_base_type` on re-entry. There is
    /// no `resolvedBaseTypes` memo here to park a sentinel in, so the guard is the
    /// **path** — the symbols already on the walk — which is the same question
    /// asked with the state that exists. It is not [`crate::resolution::Resolutions`]:
    /// that stack is keyed on `(symbol, PropertyName)` and is about *type*
    /// resolution, and giving base-type walking a `PropertyName` of its own is a
    /// change to a file this work does not own.
    ///
    /// A cycle answers `None` — a miss — rather than a diagnostic, because the
    /// checker has none (`bd tsr-5e7.6`).
    fn get_property_of_declared_symbol(
        &mut self,
        owner: SymbolId,
        name: &str,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        if let Some(&found) = self.binder.symbols().get(owner).members.get(name)
            && self.symbol_is_value(found)
        {
            return Some(found);
        }
        for base in self.base_symbols_of(owner)? {
            if let Some(found) = self.get_property_of_declared_symbol(base, name, visiting) {
                return Some(found);
            }
        }
        None
    }

    /// The symbols named by `owner`'s `extends` clauses, in declaration order.
    ///
    /// Ported from `getEffectiveBaseTypeNode` plus `resolveBaseTypesOfClass` /
    /// `resolveBaseTypesOfInterface` (`checker.go`), reduced to naming the base.
    ///
    /// # `None` means "this type's bases are a gap", and it stops the whole lookup
    ///
    /// Returning an empty list for a base this port cannot follow would be worse
    /// than answering nothing: for `interface I extends A, B<number>`, skipping
    /// `B<number>` and answering from `A` gives a property that upstream would have
    /// taken from `B` — the wrong *symbol*, not merely a missing one. So any base
    /// that cannot be followed makes the whole lookup a miss. `owner`'s own members
    /// are already answered above, so a gap in a base never costs a declared
    /// member.
    ///
    /// Three things are gaps:
    ///
    /// - **A base with type arguments** (`extends B<number>`). Upstream
    ///   instantiates; nothing here does (`bd tsr-4sc.7`,
    ///   `crate::declared::get_instantiated_type_reference`), and answering `B`'s
    ///   uninstantiated member would give `T` where upstream gives `number`.
    /// - **A base that is not a plain identifier** (`extends M.B`, `extends
    ///   mixin()`). Upstream resolves the first through `resolveEntityName` and the
    ///   second through `getBaseConstructorTypeOfClass`, which needs `typeof` and
    ///   construct signatures.
    /// - **A name that resolves to something with no members**, an alias among
    ///   them.
    ///
    /// # `implements` is not a base type
    ///
    /// Upstream reads only the `extends` clause for base types
    /// (`getEffectiveBaseTypeNode`); `implements` is checked for conformance and
    /// contributes no members. The token is the only thing that distinguishes the
    /// two clauses.
    ///
    /// # The meaning passed to resolution is narrower than upstream's
    ///
    /// A class's `extends` names an **expression**, which upstream resolves in
    /// value meaning and then reduces with `getBaseConstructorTypeOfClass`. That
    /// path needs `typeof C` and construct signatures, so this resolves the name in
    /// `SymbolFlags::TYPE` meaning instead — upstream's meaning for the *interface*
    /// case, and the one that finds a class or interface declaration in both. The
    /// consequence is that `class C extends someExpression` is a gap; the
    /// declaration form, which is what the corpus is mostly made of, is not.
    pub(crate) fn base_symbols_of(&mut self, owner: SymbolId) -> Option<Vec<SymbolId>> {
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        let mut bases = Vec::new();
        for declaration in declarations {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                // A symbol with a declaration that is neither — a class merged
                // with a namespace, say — contributes no bases from it.
                _ => continue,
            };
            for clause in clauses {
                if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for base in clause.types {
                    bases.push(self.base_symbol_of_heritage_entry(base)?);
                }
            }
        }
        Some(bases)
    }

    /// The symbol one `extends` entry names, or `None` if it is a gap.
    fn base_symbol_of_heritage_entry(
        &mut self,
        entry: &tsr_ast::ExpressionWithTypeArguments<'_>,
    ) -> Option<SymbolId> {
        if !entry.type_arguments.is_empty() {
            return None;
        }
        let Some(tsr_ast::Expression::Identifier(name)) = entry.expression else {
            return None;
        };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            name.node_id?,
            name.text,
            SymbolFlags::TYPE,
        )?;
        // Only a class or an interface has a members table to inherit from. A
        // type alias or a type parameter resolving here is a gap, not an empty
        // base: upstream would have expanded the alias.
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            .then_some(symbol)
    }

    /// Ported from `Checker.symbolIsValueEx` (`checker.go:22095`), value half
    /// only — see [`Checker::get_property_of_type`] for why the alias half is not
    /// ported.
    pub(crate) fn symbol_is_value(&self, symbol: SymbolId) -> bool {
        self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::VALUE)
    }
}
