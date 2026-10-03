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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeParameterConstraintKey {
    parameter: TypeId,
    bindings: Vec<(SymbolId, TypeId)>,
}

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
    /// Not ported: `super` and index signatures. Both answer `errorType`.
    ///
    /// **A private name is not a special lookup** (`bd tsr-0opd`). Upstream
    /// reaches `this.#x` through the same `getPropertyOfType`
    /// (`checker.go:11258`) as any other member, because a private field's
    /// symbol is filed under its own text — and `PrivateIdentifier.text`
    /// carries the leading `#`, so `#x` is simply a member name that cannot be
    /// written as a dotted identifier. This port's binder already files it
    /// that way (`crates/tsr-binder/src/binder.rs:3995`), so the arm is the
    /// name extraction below and nothing else.
    ///
    /// The *scope* rule upstream enforces separately —
    /// `lookupSymbolForPrivateIdentifierDeclaration`, which reports when a
    /// private name is used outside its declaring class — is a **diagnostic**
    /// and does not change the type answer (ADR-0040's distinction). What it
    /// would reject, the lookup here misses anyway: a `#x` that is not a
    /// member of the receiver's type answers `errorType`, which is upstream's
    /// type answer too.
    pub fn check_property_access_expression(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        // Direct public reads (including the property's name in a baseline)
        // share the expression's cached result. Rechecking an access after
        // its call exhausted the instantiation budget must not change it.
        if let Some(id) = node.node_id
            && let Some(&cached) = self.node_types.get(&id)
        {
            return cached;
        }
        self.instantiation_count = 0;
        // §944: a property access that is the TARGET of an assignment and names
        // a READONLY property answers `any`.
        //
        // Upstream reports "cannot assign to a read-only property" and the
        // erroneous reference answers `errorType`, which the producer prints as
        // `any`: `bigintWithLib` records `>bigIntArray.length : any` for
        // `bigIntArray.length = 10`. §933 met these rows as 4 `RIGHT->WRONG` and
        // named the diagnostic as their reopening condition — they were right
        // by coincidence before, because the receiver did not resolve at all.
        //
        // The predicate is already here: `property_signature_is_readonly` backs
        // `check_readonly_assignment_target`, the DIAGNOSTIC road. The type road
        // never consulted it — the eighteenth instance this session of a
        // capability present and a caller that does not reach for it.
        if let Some(id) = node.node_id
            && self.is_readonly_assignment_target(id)
        {
            return self.intrinsics.any;
        }
        let computed = self.check_property_access_expression_worker(node);
        // §55.1 (`checker-notes-narrow.md`): a single-member enum's ACCESS
        // prints the enum spelling while its declaration line keeps the
        // per-name fresh form (`Enum.A : Enum` beside `>A : Enum.A`).
        if let Some(&spelled) = self.enum_access_spelling.get(&computed) {
            return spelled;
        }
        computed
    }

    /// SS186: does the class body lexically containing `node` declare the
    /// private name `name`? Upstream's `lookupSymbolForPrivateIdentifier
    /// Declaration` walks containing classes; a name found on an ANCESTOR
    /// class is not accessible, which is what distinguishes this from a
    /// members lookup.
    /// §471 sharpened SS186's bool into the INNERMOST declaring class,
    /// because the shadow rule needs to know WHICH class's `#foo` the site
    /// sees, not merely that one exists: upstream's
    /// `lookupSymbolForPrivateIdentifierDeclaration` returns the first
    /// class's member walking OUTWARD, and the type-side lookup then runs on
    /// that symbol's per-class MANGLED name
    /// (`binder.GetSymbolNameForPrivateIdentifier`), so a same-spelled
    /// private on any other class can never match.
    fn lexical_private_declaring_class(
        &self,
        node: tsr_ast::NodeId,
        name: &str,
    ) -> Option<tsr_ast::NodeId> {
        // SS190 (measured +0, reverted): the decorator exclusion —
        // `getContainingClassExcludingClassDecorators`
        // (utilities.go:994-1011) starts the walk ABOVE a class when the
        // private name is written inside that class's own DECORATOR, so
        // `@dec(this.#x) class C { #x }` must not resolve. Transcribed and
        // measured: zero cases, no corpus fixture writes a private name in a
        // decorator position. Reverted under the unexercised-branch rule;
        // this is the one place SS186's walk is knowingly wrong, and it is
        // recorded here rather than left implicit.
        let mut current = self.nodes.parent(node);
        while let Some(id) = current {
            let members = match self.node_map.get(id) {
                Some(Node::ClassDeclaration(class)) => Some(class.members),
                Some(Node::ClassExpression(class)) => Some(class.members),
                _ => None,
            };
            if let Some(members) = members {
                let declares = members.iter().any(|member| {
                    let member_name = match member {
                        tsr_ast::ClassElement::PropertyDeclaration(p) => Some(p.name),
                        tsr_ast::ClassElement::MethodDeclaration(m) => Some(m.name),
                        tsr_ast::ClassElement::GetAccessorDeclaration(a) => Some(a.name),
                        tsr_ast::ClassElement::SetAccessorDeclaration(a) => Some(a.name),
                        _ => None,
                    };
                    matches!(member_name, Some(tsr_ast::PropertyName::PrivateIdentifier(p))
                        if p.text == name)
                });
                if declares {
                    return Some(id);
                }
            }
            current = self.nodes.parent(id);
        }
        None
    }

    /// §471: whether a member declaration's NAME is a private identifier —
    /// the test that separates a declared `#foo` from a computed `["#foo"]`,
    /// which spell the same property text in this port's by-text member
    /// tables and are different names upstream (mangled vs. plain).
    pub(crate) fn declaration_names_a_private(&self, declaration: tsr_ast::NodeId) -> bool {
        let member_name = match self.node_map.get(declaration) {
            Some(Node::PropertyDeclaration(p)) => Some(p.name),
            Some(Node::MethodDeclaration(m)) => Some(m.name),
            Some(Node::GetAccessorDeclaration(a)) => Some(a.name),
            Some(Node::SetAccessorDeclaration(a)) => Some(a.name),
            _ => None,
        };
        matches!(member_name, Some(tsr_ast::PropertyName::PrivateIdentifier(_)))
    }

    fn check_property_access_expression_worker(
        &mut self,
        node: &tsr_ast::PropertyAccessExpression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let (Some(receiver), Some(member)) = (node.expression, node.name) else {
            return error;
        };
        let name = match member {
            tsr_ast::MemberName::Identifier(name) => name.text,
            tsr_ast::MemberName::PrivateIdentifier(name) => name.text,
        };
        // SS186: a PRIVATE name is lexically scoped to the class body that
        // DECLARES it (`lookupSymbolForPrivateIdentifierDeclaration`,
        // checker.go:11475). The doc comment above claimed the lookup
        // "misses anyway" what that rule would reject — the FOURTH
        // unfalsified redundancy claim this session, and false here: in
        // `class Derived extends Base`, `#prop` IS a member of `Derived`'s
        // type by inheritance, so the lookup succeeds where upstream
        // rejects. The oracle records the same `x.#prop` as `number` inside
        // `Base` and `any` (its errorType) inside `Derived`
        // (`privateNameFieldDerivedClasses`).
        let mut lexical_private_class = None;
        if let tsr_ast::MemberName::PrivateIdentifier(_) = member
            && let Some(access_id) = node.node_id
        {
            lexical_private_class = self.lexical_private_declaring_class(access_id, name);
            if lexical_private_class.is_none() {
                return error;
            }
        }
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
            // §121 (`checker-notes-narrow.md`): the receiver COMPUTED (the
            // gap test above passed) and the non-null strip itself refused.
            // Upstream reports (TS2532/TS18048/TS18047) and returns
            // `errorType`, printed `any`: the deliberate error-answer, the
            // boundary-argument chain's seventh hop. GATED to receivers that
            // are PURELY nullish — the ungated arm measured 64:36 with the
            // adverse concentrated in `unknown` receivers this port fails to
            // narrow (catch variables, assertion predicates); those keep the
            // honest gap.
            if self.receiver_is_purely_nullish(non_optional) {
                // §14.1's split: a JS baseline prints upstream's errorType
                // verbatim (`jsFileClassSelfReferencedProperty`'s
                // `this.testStackOverflow.bind : error`).
                if node.node_id.is_some_and(|id| self.in_js_file(id)) {
                    return error;
                }
                return self.intrinsics.any;
            }
            return error;
        }
        // §471 the shadow half of upstream's mangled-name lookup: the
        // property the receiver's type serves under this spelling must be
        // THE lexical class's member — `getPrivateIdentifierPropertyOfType`
        // asks for `lexicallyScopedSymbol.Name`, the per-class mangled name
        // (`checker.go:11296`), so B's `#foo` seen from inside A can never
        // resolve A's-typed receiver's `#foo`, and
        // `checkPrivateIdentifierPropertyAccess` reports shadowing and
        // answers `errorType` (`:11300`), printed `any`
        // (`privateNamesInNestedClasses-1/-2`,
        // `privateNameNestedClassAccessorsShadowing`,
        // `privateNamesAndStaticFields`). Inheritance survives by symbol
        // identity: `Derived`'s type serves BASE's member symbol for a base
        // private, and that symbol's declaration sits in the lexical class
        // (`privateNameFieldDerivedClasses`).
        if let Some(lexical) = lexical_private_class
            && let Some(property) = self.get_property_of_type(stripped, name)
        {
            let record = self.binder.symbols().get(property);
            let declared_in_lexical = record
                .value_declaration
                .and_then(|declaration| self.containing_class_of(declaration))
                == Some(lexical);
            if !declared_in_lexical {
                return self.intrinsics.any;
            }
        }
        // `isAssignmentToReadonlyEntity` (`checker.go:11377`): a readonly
        // property as an assignment target answers upstream's `errorType`,
        // printed `any` (`checker-notes-narrow.md` §27).
        if node
            .node_id
            .is_some_and(|id| {
                self.assignment_target_kind(id)
                    != crate::expressions::AssignmentTargetKind::None
            })
            // Upstream's constructor exception: `this.x = …` inside a
            // constructor assigns a readonly property legally
            // (`isAssignmentToReadonlyEntity`'s same-class carve-out —
            // approximated as this-receiver-in-constructor, the §27 bar's
            // fired leg). §471: the carve-out is for readonly FIELDS only —
            // upstream reaches it through the `CheckFlagsReadonly`/
            // `readonly`-modifier arms, and a GET-ONLY ACCESSOR write is
            // illegal even there (`isReadonlySymbol`'s accessor arm has no
            // constructor escape; `privateNameAccessors` wants
            // `this.#roProp : any` inside the constructor).
            && !(matches!(
                receiver,
                tsr_ast::Expression::KeywordExpression(keyword)
                    if keyword.kind == tsr_ast::SyntaxKind::ThisKeyword
            ) && node.node_id.is_some_and(|id| {
                self.control_flow_container(id).is_some_and(|container| {
                    self.nodes.kind(container) == tsr_ast::SyntaxKind::Constructor
                })
            }) && !self.get_property_of_type(stripped, name).is_some_and(|property| {
                let flags = self.binder.symbols().get(property).flags;
                flags.intersects(tsr_binder::SymbolFlags::GET_ACCESSOR)
                    && !flags.intersects(tsr_binder::SymbolFlags::SET_ACCESSOR)
            }))
            && self
                .get_property_of_type(stripped, name)
                .is_some_and(|property| self.is_readonly_symbol(property))
        {
            return self.intrinsics.any;
        }
        // `isThisPropertyAccessInConstructor` (`checker.go:27328`): inside the
        // constructor that declares a this-property, the property reads as
        // `autoType` and `getFlowTypeOfAccessExpression` follows its flow.
        if let Some(access) = node.node_id
            && self.in_js_file(access)
            && let Some(constructor) = self.get_this_container(access, true)
            && self.nodes.kind(constructor) == tsr_ast::SyntaxKind::Constructor
            && let Some(property) = self.get_property_of_type(stripped, name)
            && matches!(
                self.is_constructor_declared_this_property(property),
                crate::assignment_declarations::ThisAssignmentDeclaration::Constructor(declaring)
                    if declaring == constructor
            )
        {
            if self.assignment_target_kind(access)
                == crate::expressions::AssignmentTargetKind::Definite
            {
                return self.intrinsics.any;
            }
            return self.get_flow_type_of_property_symbol(access, None, property);
        }
        let result = self.access_member_lookup(stripped, name, node.node_id);
        if result == error {
            return error;
        }
        // §78 (`checker-notes-narrow.md`): `getWriteTypeOfSymbol` under
        // `exactOptionalPropertyTypes` — a WRITE position removes
        // `missingType`, so `obj.a = 'hello'` prints `string` while the read
        // keeps `string | undefined` (`strictOptionalProperties1`).
        // §616: the DIVERGENT-ACCESSOR half of `getWriteTypeOfSymbol`, beside
        // the `exactOptionalPropertyTypes` half below. A write to a property
        // whose setter is annotated sees the SETTER's parameter type
        // (`getWriteTypeOfAccessors`, `checker.go:16447`), which differs from
        // the read type only for a divergent pair — `get x(): string` beside
        // `set x(v: string | number | boolean)`.
        let is_definite_write = node.node_id.is_some_and(|id| {
            self.assignment_target_kind(id) == crate::expressions::AssignmentTargetKind::Definite
        });
        let result = if is_definite_write
            && let Some(property) = self.get_property_of_type(stripped, name)
            && let Some(written) = self.write_type_of_accessors(property)
        {
            written
        } else {
            result
        };
        let result = if self.exact_optional_property_types && is_definite_write {
            self.remove_missing_type(result)
        } else {
            result
        };
        self.propagate_optional_type_marker_at(node.node_id, result, non_optional != receiver_type)
    }

    /// The §45 `Record<string, V>` read: `Some(V)` only for a reference to
    /// the GLOBAL `Record` with a plain-`string` key.
    fn record_string_value(&mut self, receiver: TypeId) -> Option<TypeId> {
        let (target, arguments) = self.type_reference_targets.get(&receiver)?.clone();
        if arguments.len() != 2 || arguments[0] != self.intrinsics.string {
            return None;
        }
        let record = self.binder.global("Record")?;
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(record))
            .then_some(arguments[1])
    }

    /// `checkNonNullType` (`checker.go:7409`), without the diagnostics: an
    /// `unknown` receiver is `errorType` in strict mode; a nullable one is
    /// answered by its non-nullable remainder; a remainder that is itself
    /// nullable or `never` refuses.
    pub(crate) fn check_non_null_type(&mut self, id: TypeId) -> TypeId {
        let error = self.intrinsics.error;
        if self.strict_null_checks && self.store.get(id).flags.intersects(TypeFlags::UNKNOWN) {
            return error;
        }
        if !self
            .get_type_facts(id)
            .intersects(crate::flow::TypeFacts::IS_UNDEFINED | crate::flow::TypeFacts::IS_NULL)
        {
            return id;
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

    /// §121's gate: the receiver is `undefined`, `null`, or a union of only
    /// those — every constituent carries `TypeFlags::NULLABLE`. `unknown`
    /// and mixed remainders are excluded: the ungated arm measured its
    /// adverse there (receivers this port fails to narrow).
    fn receiver_is_purely_nullish(&mut self, id: TypeId) -> bool {
        let non_nullable = self.get_non_nullable_type(id);
        let original = self.store.get(id).flags;
        original.intersects(crate::flags::TypeFlags::NULLABLE.union(crate::flags::TypeFlags::UNION))
            && self.store.get(non_nullable).flags.intersects(crate::flags::TypeFlags::NEVER)
            && !original.intersects(crate::flags::TypeFlags::UNKNOWN)
    }

    /// `GetNonNullableType` (`checker.go:18663`): adjust non-null facts in
    /// strict mode, including semantic intersections for type variables.
    pub(crate) fn get_non_nullable_type(&mut self, id: TypeId) -> TypeId {
        if self.strict_null_checks {
            self.get_adjusted_type_with_facts(id, crate::flow::TypeFacts::NE_UNDEFINED_OR_NULL)
        } else {
            id
        }
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
        if let Some(receiver) = receiver.filter(|&id| self.expression_is_optional_chain(id)) {
            // `removeOptionalTypeMarker` (`checker.go:29073`), done by identity:
            // the receiver link's own pre-union type is precisely what the
            // marker was added to. Filtering `undefined` by flag instead would
            // also take away a genuine one — `this?.a.#b` with `a?: A`, which
            // upstream reports on. §5 of `checker-notes-nnaccess.md`.
            return self.pre_optional_marker.get(&receiver).copied().unwrap_or(expression_type);
        }
        expression_type
    }

    /// Remember an optional-chain link's type before the marker joins it, so
    /// the next link can subtract exactly that. §5 of `checker-notes-nnaccess.md`.
    pub(crate) fn propagate_optional_type_marker_at(
        &mut self,
        node: Option<tsr_ast::NodeId>,
        id: TypeId,
        was_optional: bool,
    ) -> TypeId {
        if was_optional && let Some(node) = node {
            self.pre_optional_marker.insert(node, id);
        }
        self.propagate_optional_type_marker(id, was_optional)
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
    pub(crate) fn expression_is_optional_chain(&self, id: tsr_ast::NodeId) -> bool {
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

    /// Whether a `NonNullExpression` carries `NodeFlagsOptionalChain` — the
    /// predicate `checkNonNullAssertion` (`checker.go:10623`) branches on. §884.
    ///
    /// **This is not "the operand spine contains `?.`".** The parser builds every
    /// `!` with `ast.NodeFlagsNone` (`parser.go:5375`) and the flag is added
    /// *retroactively*, by `tryReparseOptionalChain` (`parser.go:5414`), which is
    /// called from exactly three places — the property-access, element-access and
    /// call rests (`parser.go:5399`, `5444`, `5468`). A `!` is therefore a chain
    /// link only when **another link was parsed on top of it**:
    ///
    /// ```ts
    /// o2?.["b"]!.c   // the `!` IS a chain link — `.c` reparsed it
    /// o2?.["b"]!.c!  // the trailing `!` is NOT — nothing follows it
    /// m?.[0]! && …   // not a link either; `&&` is not one of the three rests
    /// ```
    ///
    /// That asymmetry is the whole observable difference: an inner `!` re-unions
    /// the chain's `undefined` and the outermost `!` removes it. Deriving the flag
    /// from the operand alone marks the trailing `!` as a link too and answers
    /// `string | undefined` where upstream answers `string` — measured as six
    /// `RIGHT->WRONG` on the first build of §884.
    ///
    /// The upward walk climbs a *run* of `!`s because `tryReparseOptionalChain`
    /// stamps the whole run in one go (`o2?.b!!.c`).
    pub(crate) fn non_null_is_optional_chain(&self, node: tsr_ast::NodeId) -> bool {
        // Downward: `expr := node.Expression(); for IsNonNullExpression(expr) …`
        // — the port derives the operand's flag rather than storing it, and the
        // spine walker already passes through `!` links.
        let Some(Node::NonNullExpression(assertion)) = self.node_map.get(node) else {
            return false;
        };
        let Some(operand) = assertion.expression.and_then(|e| e.node_id()) else { return false };
        if !self.expression_is_optional_chain(operand) {
            return false;
        }
        // Upward: did one of the three rests reparse this run?
        let mut current = node;
        loop {
            let Some(parent) = self.nodes.parent(current) else { return false };
            if matches!(self.node_map.get(parent), Some(Node::NonNullExpression(_))) {
                current = parent;
                continue;
            }
            let continues = match self.node_map.get(parent) {
                Some(Node::PropertyAccessExpression(access)) => access.expression,
                Some(Node::ElementAccessExpression(access)) => access.expression,
                Some(Node::CallExpression(call)) => call.expression,
                _ => return false,
            };
            return continues.and_then(|e| e.node_id()) == Some(current);
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
            // §268. A `this`-headed qualified name checks `this` as an
            // EXPRESSION rather than resolving it as a name.
            //
            //     // @noImplicitThis: false
            //     function Test1() { let x: typeof this.no = 1 }
            //     >this.no : any
            //     >x : any
            //
            // Witness `conformance/typeofThisWithImplicitThis`. `this` is not a
            // name, so `resolve_name` finds nothing and this arm answered the
            // error type; `check_this_expression` already answers `any` for a
            // plain function in every mode (`tryGetThisTypeAtEx`'s
            // fallthrough), and `.no` on `any` is `any` — upstream's route.
            //
            // The sibling arm in `get_type_from_type_query_node`
            // (`declared.rs`) stays as it is and is NOT this bug: that one is
            // upstream's `isThisIdentifier` dispatch for a BARE `typeof this`,
            // it is correct, and it is the arm anyone reading this diagnosis
            // would go to first.
            tsr_ast::EntityName::Identifier(identifier) if identifier.text == "this" => {
                match identifier.node_id {
                    Some(id) => self.check_this_expression(id),
                    None => return error,
                }
            }
            tsr_ast::EntityName::Identifier(identifier) => {
                self.check_expression(tsr_ast::Expression::Identifier(identifier))
            }
            tsr_ast::EntityName::QualifiedName(inner) => self.check_qualified_name(inner),
        };
        // `checkQualifiedName` routes the left through `checkNonNullExpression`
        // (`checker.go:8127`) exactly as a property access does its receiver.
        if left_type == error {
            return error;
        }
        let stripped = self.check_non_null_type(left_type);
        if stripped == error {
            // §121: same gate as the property-access road — a computed,
            // purely-nullish left takes upstream's deliberate error-answer.
            if self.receiver_is_purely_nullish(left_type) {
                return self.intrinsics.any;
            }
            return error;
        }
        self.access_member_lookup(stripped, right.text, node.node_id)
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
        // §164/§165: the THIS-ARGUMENT is the ORIGINAL receiver, not the
        // apparent one — `getTypeWithThisArgument(apparentType, receiver)`
        // (`checker.go:19573` at its member call sites) reads members from
        // the apparent type while substituting `this` with what the caller
        // actually wrote. `x: T extends A` calling `x.self(): this` is
        // `T`, not `A` (thisTypeAndConstraints, the §165 first pair's four
        // R→W).
        let this_argument = receiver_type;
        let receiver_type = self.apparent_type(receiver_type);
        if receiver_type == self.intrinsics.any {
            return self.intrinsics.any;
        }
        // resolveAnonymousTypeMembers exposes only runtime global properties;
        // block-scoped bindings are lexical globals, not globalThis members.
        // A missing global property recovers with any in checkPropertyAccess.
        if Some(receiver_type) == self.global_this_type {
            return match self.binder.global(name) {
                Some(symbol)
                    if !self.binder.symbols().get(symbol).flags.intersects(
                        SymbolFlags::BLOCK_SCOPED_VARIABLE | SymbolFlags::CLASS | SymbolFlags::ENUM,
                    ) && self.symbol_is_value(symbol) =>
                {
                    self.get_type_of_symbol(symbol)
                }
                _ => self.intrinsics.any,
            };
        }
        // §45 (`checker-notes-narrow.md`): the global `Record<K, V>` answers
        // V for property reads when K is `string` — the one mapped alias
        // special-cased, everything else mapped stays the subsystem. The
        // `noUncheckedIndexedAccess` refinement was measured at −6 net
        // (10 R→W / 4 W→R): the option's property-vs-element split differs
        // inside this road and stays recorded, not guessed.
        if let Some(value) = self.record_string_value(receiver_type) {
            return value;
        }
        // §32 (`checker-notes-narrow.md`): a receiver minted for an
        // UNRESOLVED type reference is upstream's `errorType`, and member
        // access through it answers the same — printed `any`. The §31
        // structural gate applies: in an import-machinery file the mint may
        // be the PORT's resolution miss.
        if self.unresolved_types.contains(&receiver_type)
            && node_id.is_some_and(|id| !self.file_has_import_machinery(id))
        {
            return self.intrinsics.any;
        }
        // No explicit test for an `errorType` receiver: it is an intrinsic and
        // never carries a members table, so the lookup below misses and answers
        // `errorType` anyway. An earlier draft guarded it and no mutation could
        // make the guard observable, so it was removed rather than kept as
        // decoration. The identity test above is what keeps that true now that
        // an `ANY`-flagged type has a fast path.
        // On a property miss, the `prop == nil` path of
        // `checkPropertyAccessExpressionOrQualifiedName`: an applicable index
        // signature answers the member's type (`noImplicitAny` errors are
        // diagnostics, not types). The key is the name's string literal type.
        // See `checker-notes-narrow.md` §17.
        let property_type = if let Some(found) =
            self.get_type_of_property_with_this_argument(receiver_type, name, this_argument, false)
        {
            found
        } else {
            let key = self.store.intern_literal(
                crate::flags::TypeFlags::STRING_LITERAL,
                crate::types::TypeData::StringLiteral(name.to_string()),
                false,
            );
            if let Some(info) = self.get_applicable_index_info(receiver_type, key) {
                if self.no_unchecked_indexed_access {
                    let undefined = self.intrinsics.undefined;
                    self.get_union_type(&[info.value, undefined])
                } else {
                    info.value
                }
            } else if node_id.is_some_and(|id| !self.in_js_file(id))
                && self.miss_is_established(receiver_type, name)
            {
                // §123 (`checker-notes-narrow.md`): the walk COMPLETED —
                // every base on the chain was followed and the name is
                // established absent — so this is upstream's TS2339/TS2550
                // report and its errorType-printed-any, not a table this
                // port failed to read. A blocked walk keeps the gap.
                // JS positions excluded — unchecked-JS misses answer
                // differently upstream (spellingUncheckedJS's 7 R→W).
                self.intrinsics.any
            } else {
                error
            }
        };
        // §164 (`checker-notes-narrow.md`), §163's decidable slice 1: a
        // member whose type IS the owner's `this` type answers the
        // RECEIVER — `getTypeWithThisArgument` (`checker.go:19573`) reads a
        // member with the receiver as the this-argument, and where the
        // member's type is the this-type itself the substitution's whole
        // effect is that answer. A receiver that IS a this-type substitutes
        // to itself, so it is skipped rather than looped.
        // §166 (`checker-notes-narrow.md`): the port mints `this` in TWO
        // tables — `this_types` per CLASS symbol (`expressions.rs`) and
        // `this_type_nodes` per INTERFACE declaration (`declared.rs:163`) —
        // and §164/§165 reached only the first, so a lib INTERFACE's
        // `valueOf(): this` still printed `this`. Consulting both at the
        // substitution site completes the rule WITHOUT unifying the mints
        // (that unification is rock #3's prerequisite for the
        // representation work, and is deliberately not attempted here).
        let this_minted: Vec<TypeId> =
            self.this_types.values().chain(self.this_type_nodes.values()).copied().collect();
        let property_type =
            if property_type != this_argument && this_minted.contains(&property_type) {
                this_argument
            } else if property_type != this_argument
                && let Some(minted) = this_minted
                    .iter()
                    .copied()
                    .find(|&minted| self.mentions_type_parameter(property_type, &[minted], &[]))
            {
                // §165 (`checker-notes-narrow.md`), §164's embedded half: where
                // the this-type sits INSIDE the member's type — `fn(): this`
                // read off `c` wants `() => C` — the same
                // `getTypeWithThisArgument` substitution runs through
                // `instantiate_type`, which already handles it: a this-type is
                // TYPE_PARAMETER-flagged, so arm 1 maps it and the signature
                // text is REBUILT rather than reused. An instantiation that
                // gaps keeps the original rather than answering error.
                let map = [(minted, this_argument)];
                let names = ["this"];
                let image = self.instantiate_type(property_type, &map, &[minted], &names);
                if image == self.intrinsics.error { property_type } else { image }
            } else {
                property_type
            };
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
        // `getFlowTypeOfAccessExpression` (`checker.go:11396`): a definite
        // assignment target is not narrowed by the flow reaching it.
        // Under `exactOptionalPropertyTypes` the caller's `removeMissingType`
        // still misses members whose optionality this port spells as plain
        // `undefined` (`strictOptionalProperties1`'s `Partial<…>` writes), so
        // that mode keeps the earlier narrowed answer until those members
        // carry `missingType`.
        if !self.exact_optional_property_types
            && self.assignment_target_kind(id) == crate::expressions::AssignmentTargetKind::Definite
        {
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
    /// **AUDITED and HOLDS** (2026-08-11, under checker-1's §203 heuristic —
    /// a refusal naming a prerequisite subsystem is the cheapest kind to
    /// check and the easiest to write carelessly). Four such claims were
    /// checked across both lanes this session and four were FALSE; this one
    /// is true: the port mints no `emptyObjectType` intrinsic at all
    /// (`intrinsics.rs` has none, and `intersections.rs` only tracks an
    /// `empty_object` INCLUDES bit while folding). The arm therefore needs
    /// the intrinsic first, exactly as written. Recorded so the next audit
    /// does not re-run this grep.
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
    /// The `extends` constraint of a type-parameter type, resolved.
    ///
    /// getConstraintOfTypeParameter (checker.go), retaining the effective
    /// outer mapper for written constraints. A polymorphic this parameter
    /// instead has the declaring class or interface as its constraint.
    pub(crate) fn type_parameter_constraint(&mut self, id: TypeId) -> Option<TypeId> {
        if !self.store.get(id).flags.contains(TypeFlags::TYPE_PARAMETER) {
            return None;
        }
        if let Some(symbol) =
            self.this_types.iter().find_map(|(&symbol, &ty)| (ty == id).then_some(symbol)).or_else(
                || {
                    self.this_type_nodes.iter().find_map(|(&node, &ty)| {
                        (ty == id).then(|| self.binder.symbol_of(node)).flatten()
                    })
                },
            )
        {
            return Some(self.get_declared_type_of_symbol(symbol));
        }
        // getConstraintOfTypeParameter keeps a resolved constraint identity.
        // This port evaluates nodes under alias binding frames, so the cache
        // also retains the effective mapper rather than sharing substitutions.
        let bindings: rustc_hash::FxHashMap<_, _> = self
            .alias_evaluation_bindings
            .iter()
            .flat_map(|frame| frame.iter().map(|(&symbol, &ty)| (symbol, ty)))
            .collect();
        let mut bindings: Vec<_> = bindings.into_iter().collect();
        bindings.sort_unstable_by_key(|&(symbol, _)| symbol);
        let key = TypeParameterConstraintKey { parameter: id, bindings };
        if let Some(&constraint) = self.type_parameter_constraint_cache.get(&key) {
            return constraint;
        }
        if let Some(instance) = self.instantiated_type_parameters.get(&id).cloned() {
            self.type_parameter_constraint_cache.insert(key.clone(), None);
            let constraint =
                self.type_parameter_constraint(instance.target).and_then(|constraint| {
                    let names: Vec<_> = instance.names.iter().map(String::as_str).collect();
                    let image = self.instantiate_type(
                        constraint,
                        &instance.map,
                        &instance.parameters,
                        &names,
                    );
                    (image != self.intrinsics.error).then_some(image)
                });
            self.type_parameter_constraint_cache.insert(key, constraint);
            return constraint;
        }
        let symbol = *self.type_parameter_symbols.get(&id)?;
        // getConstraintDeclaration searches all merged declarations; an infer
        // parameter can have its constraint on a later occurrence.
        let constraint_node =
            self.binder.symbols().get(symbol).declarations.iter().find_map(|&declaration| {
                match self.node_map.get(declaration) {
                    Some(Node::TypeParameterDeclaration(parameter)) => parameter.constraint,
                    _ => None,
                }
            })?;
        self.type_parameter_constraint_cache.insert(key.clone(), None);
        let constraint = self.mapped_constraint_type(constraint_node);
        // A constraint that itself gaps leaves the parameter as it was: a gap
        // beats reading members off `errorType`.
        let constraint = (constraint != self.intrinsics.error).then_some(constraint);
        self.type_parameter_constraint_cache.insert(key, constraint);
        constraint
    }

    /// getInferredTypeParameterConstraint (checker.go:17114). Conditional
    /// inference reads this before applying its non-fixing constraint mapper.
    pub(crate) fn inferred_type_parameter_constraint(&mut self, id: TypeId) -> Option<TypeId> {
        let symbol = *self.type_parameter_symbols.get(&id)?;
        let declarations = self.binder.symbols().get(symbol).declarations.clone();
        let mut constraints = Vec::new();
        for declaration in declarations {
            let Some(infer) = self.nodes.parent(declaration) else { continue };
            if !matches!(self.node_map.get(infer), Some(Node::InferTypeNode(_))) {
                continue;
            }
            let mut child = infer;
            let mut parent = self.nodes.parent(child);
            while let Some(node) = parent
                && matches!(self.node_map.get(node), Some(Node::ParenthesizedTypeNode(_)))
            {
                child = node;
                parent = self.nodes.parent(child);
            }
            let Some(parent) = parent else { continue };
            let constraint = match self.node_map.get(parent) {
                Some(Node::TypeReferenceNode(reference)) => {
                    let Some(symbol) = reference
                        .type_name
                        .and_then(|name| self.resolve_entity_name(name, SymbolFlags::TYPE))
                    else {
                        continue;
                    };
                    let Some(index) = reference
                        .type_arguments
                        .iter()
                        .position(|argument| argument.node_id() == Some(child))
                    else {
                        continue;
                    };
                    let Some(parameters) = self.local_type_parameter_types_of(symbol) else {
                        continue;
                    };
                    if reference.type_arguments.len() != parameters.len() {
                        continue;
                    }
                    let Some(&(parameter, _)) = parameters.get(index) else { continue };
                    let Some(constraint) = self.type_parameter_constraint(parameter) else {
                        continue;
                    };
                    let arguments: Vec<_> = reference
                        .type_arguments
                        .iter()
                        .map(|&argument| self.get_type_from_type_node(argument))
                        .collect();
                    if arguments.contains(&self.intrinsics.error) {
                        continue;
                    }
                    let types: Vec<_> = parameters.iter().map(|&(id, _)| id).collect();
                    let names: Vec<_> = parameters.iter().map(|(_, name)| name.as_str()).collect();
                    let map: Vec<_> = types.iter().copied().zip(arguments).collect();
                    let instantiated = self.instantiate_type(constraint, &map, &types, &names);
                    if instantiated == id || instantiated == self.intrinsics.error {
                        continue;
                    }
                    instantiated
                }
                Some(Node::RestTypeNode(_)) => {
                    let target = self.global_type_symbol("Array")?;
                    self.create_type_reference(target, vec![self.intrinsics.unknown])
                }
                Some(Node::ParameterDeclaration(parameter))
                    if parameter.dot_dot_dot_token.is_some() =>
                {
                    let target = self.global_type_symbol("Array")?;
                    self.create_type_reference(target, vec![self.intrinsics.unknown])
                }
                Some(Node::NamedTupleMember(member))
                    if member.dot_dot_dot_token.is_some()
                        || self.nodes.parent(parent).is_some_and(|outer| {
                            matches!(self.node_map.get(outer), Some(Node::RestTypeNode(_)))
                        }) =>
                {
                    let target = self.global_type_symbol("Array")?;
                    self.create_type_reference(target, vec![self.intrinsics.unknown])
                }
                Some(Node::TemplateLiteralTypeSpan(_)) => self.intrinsics.string,
                Some(Node::TypeParameterDeclaration(_))
                    if self.nodes.parent(parent).is_some_and(|outer| {
                        matches!(self.node_map.get(outer), Some(Node::MappedTypeNode(_)))
                    }) =>
                {
                    self.get_union_type(&[
                        self.intrinsics.string,
                        self.intrinsics.number,
                        self.intrinsics.es_symbol,
                    ])
                }
                _ => continue,
            };
            if constraint != self.intrinsics.error {
                constraints.push(constraint);
            }
        }
        if constraints.is_empty() {
            None
        } else {
            Some(self.get_intersection_type(&constraints, None))
        }
    }

    pub(crate) fn apparent_type(&mut self, id: TypeId) -> TypeId {
        let id = self.apparent_mapped_type(id);
        // Upstream's order, arm for arm (`checker.go:21745-21751`). `NUMBER_LIKE`
        // carrying `ENUM` is upstream's too, not a widening added here.
        //
        // A union carries `TypeFlags::UNION` and **not** its constituents'
        // flags (`crate::unions::create_union`), with the single exception of
        // `false | true`, which is given `BOOLEAN` deliberately so that it *is*
        // `boolean`. So `string | number` reaches no arm here and `boolean`
        // reaches the fourth — which is exactly upstream's behaviour, and the
        // reason this needs no union guard of its own.
        // `getApparentType`'s **head**, before the switch (`checker.go:21731`):
        // *"if t.flags&TypeFlagsInstantiable != 0 { t = getBaseConstraintOfType(t);
        // if t == nil { t = unknownType } }"*. A type parameter is read
        // through its constraint, which is what makes `T.x` resolve for
        // `T extends Shape` (`bd tsr-rppd`).
        //
        // An **unconstrained** parameter becomes `unknown`, whose lookup finds
        // nothing — the same gap as today, by a different route, which is why
        // this cannot lose a line on its own.
        //
        // Recursive base constraints also expose indexed and template types;
        // polymorphic this reads through its declaring class/interface.
        let id = self.base_constraint_of_type(id).unwrap_or(id);
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
        self.get_type_of_property_with_this_argument(id, name, id, false)
    }

    /// getTypeWithThisArgument retains the original receiver when member
    /// lookup proceeds through its apparent constraint.
    fn get_type_of_property_with_this_argument(
        &mut self,
        id: TypeId,
        name: &str,
        this_argument: TypeId,
        skip_object_function_augment: bool,
    ) -> Option<TypeId> {
        self.resolve_mapped_type_members(id);
        if let Some(property) = self
            .anonymous_properties
            .get(&id)
            .and_then(|(properties, instantiated)| {
                instantiated
                    .then(|| properties.iter().find(|property| property.name == name))
                    .flatten()
            })
            .cloned()
        {
            // getTypeOfReverseMappedSymbol (inference.go:1145): a reverse
            // mapped property type resolves its own members when read.
            self.complete_reverse_mapped_type(property.r#type);
            return Some(if property.optional {
                self.get_optional_type(property.r#type, true)
            } else {
                property.r#type
            });
        }

        // §829.2, a PROBE and nothing else (`TSR_PROJ_TRACE=<name>`): print what
        // the projection has in hand for one property name, so the 193-line
        // "the property TYPES; the projection fails" bucket can be told apart
        // from the already-refused inference legs. Behaviour-free.
        if std::env::var("TSR_PROJ_TRACE").ok().as_deref() == Some(name) {
            let reference = self.type_reference_targets.get(&id).cloned();
            let arguments = reference.as_ref().map(|(_, args)| {
                args.iter()
                    .map(|&a| crate::printing::type_to_string(self.store.get(a)))
                    .collect::<Vec<_>>()
            });
            let own = self
                .get_property_of_type(id, name)
                .map(|symbol| self.get_type_of_symbol(symbol))
                .map(|t| {
                    if t == self.intrinsics.error {
                        "error".to_string()
                    } else {
                        crate::printing::type_to_string(self.store.get(t))
                    }
                });
            eprintln!(
                "PROJ `{name}` on `{}`: is_reference={} args={:?} property_own_type={:?}",
                crate::printing::type_to_string(self.store.get(id)),
                reference.is_some(),
                arguments,
                own
            );
        }
        // §952: a homomorphic IDENTITY mapped type reads the SOURCE's member
        // and then applies the mapping's optionality modifier — the one thing
        // the reused member owner cannot carry. `Partial<O>`'s `x` is
        // `string | undefined`, `Required<{ x?: string }>`'s is `string`, and
        // `Readonly<O>`'s is `string` unchanged.
        //
        // Upstream sets optionality on a freshly synthesised property symbol in
        // `resolveMappedTypeMembers` (`checker.go`); see
        // `Checker::mapped_identity_optionality` for why this port cannot and
        // what it does instead. The read is here rather than at the mint because
        // the source's member type is only known per name.
        if let Some(&(optionality, _)) = self.mapped_identity_optionality.get(&id) {
            let owner = match &self.store.get(id).data {
                TypeData::Named { members: Some(owner), .. } => *owner,
                _ => return None,
            };
            let source = self
                .type_reference_targets
                .get(&id)
                .and_then(|(_, arguments)| (arguments.len() == 1).then_some(arguments[0]));
            let member = if let Some(source) = source {
                self.get_type_of_property_with_this_argument(
                    source,
                    name,
                    source,
                    skip_object_function_augment,
                )?
            } else {
                let mut visiting = Vec::new();
                let property = self.get_property_of_declared_symbol(owner, name, &mut visiting)?;
                self.get_type_of_symbol(property)
            };
            return Some(match optionality {
                // `?` / `+?`: the property becomes optional, which a READ sees
                // as `| undefined`.
                Some(true) => {
                    let undefined = self.intrinsics.undefined;
                    self.get_union_type(&[member, undefined])
                }
                // `-?`: optionality is removed, so an `| undefined` the source
                // carried is stripped. `remove_undefined` is `getTypeWithFacts`'s
                // job upstream; this port's existing non-nullable road is the
                // same question, and a member with no `undefined` is unchanged.
                Some(false) => self.get_non_nullable_type(member),
                None => member,
            });
        }
        // §49 (`checker-notes-narrow.md`): a UNION projects across its
        // constituents — every one must carry the name, and the answer is
        // the union of the member types.
        if let TypeData::Union { types, .. } = &self.store.get(id).data {
            let constituents = types.clone();
            let mut projected = Vec::with_capacity(constituents.len());
            let mut has_property = false;
            for constituent in constituents {
                // §117 slice 3: each constituent reads through its APPARENT
                // type — upstream's per-constituent getReducedApparentType;
                // `(string | number).constructor` answers through the
                // wrapper interfaces, and slice 1's Object fallback then
                // covers the object constituents.
                let apparent = self.apparent_type(constituent);
                if self.store.get(apparent).flags.contains(TypeFlags::NEVER)
                    || self.intersection_has_never_discriminant(apparent)
                {
                    continue;
                }
                let member = if let Some(member) = self.get_type_of_property_with_this_argument(
                    apparent,
                    name,
                    constituent,
                    skip_object_function_augment,
                ) {
                    has_property = true;
                    member
                } else {
                    // createUnionOrIntersectionProperty accepts an applicable
                    // index in a constituent missing the named property. For
                    // example, [boolean, string] | string[] has a property 0
                    // contributed by the tuple and an index from the array.
                    // A purely indexed union still has no named property.
                    if name.starts_with('[')
                        || self.tuple_element_lists.contains_key(&apparent)
                        || self.variadic_tuple_elements.contains_key(&apparent)
                    {
                        return None;
                    }
                    let key = self.store.intern_literal(
                        TypeFlags::STRING_LITERAL,
                        TypeData::StringLiteral(name.to_string()),
                        false,
                    );
                    self.get_applicable_index_info(apparent, key)?.value
                };
                projected.push(member);
            }
            return has_property.then(|| self.get_union_type(&projected));
        }
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
        if self.variadic_tuple_elements.contains_key(&id) {
            if name == "length" {
                return Some(self.intrinsics.number);
            }
            if let Ok(index) = name.parse::<usize>()
                && index.to_string() == name
                && let Some(element) = self.variadic_tuple_element_type(id, index, false)
            {
                return Some(element);
            }
        }
        if let Some((elements, _)) = self.tuple_element_lists.get(&id)
            && let Ok(index) = name.parse::<usize>()
            && index.to_string() == name
        {
            let element = elements.get(index).copied();
            // §79: an OPTIONAL element reads `| undefined` — the mint's
            // "exact by construction" claim predates optional tuples, and
            // the first measurement without this arm priced it at 100 G→W
            // (`optionalTupleElements1`).
            let optional = self
                .tuple_optional_masks
                .get(&id)
                .is_some_and(|mask| mask.get(index).copied().unwrap_or(false));
            return Some(match element {
                None => self.intrinsics.undefined,
                Some(element) if optional => {
                    let undefined = self.intrinsics.undefined;
                    self.get_union_type(&[element, undefined])
                }
                Some(element) => element,
            });
        }
        // §117 slice 2: the synthetic `.prototype` on a class STATIC side —
        // upstream mints it typed as the INSTANCE (generic classes: the
        // reference over per-parameter `any`); a WRITTEN member named
        // `prototype` would be found by the symbol road, but upstream's mint
        // shadows statics only where no such member exists, and a class
        // cannot declare one (TS2699), so answering first is safe. Function
        // receivers keep the fallback's `any`-family answers.
        if name == "prototype"
            && let TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(symbol))
                .flags
                .contains(SymbolFlags::CLASS)
        {
            let class = self.binder.merged_symbol(symbol);
            let parameters = self.local_type_parameters_of(class);
            if parameters.is_empty() {
                let instance = self.get_declared_type_of_symbol(class);
                if instance != self.intrinsics.error {
                    return Some(instance);
                }
            } else {
                let any = self.intrinsics.any;
                let arguments = vec![any; parameters.len()];
                return Some(self.create_type_reference(class, arguments));
            }
        }
        // §117 slice 4: a TUPLE's `.length` is the LITERAL element count
        // (`createNormalizedTupleType` mints it; plain tuples only — the
        // §37 registry has no rest/optional shapes to miscount... optional
        // masks DO exist: a tuple with optionals answers the union of
        // possible lengths upstream; decline those, plain counts only).
        if name == "length"
            && let Some((elements, _)) = self.tuple_element_lists.get(&id)
        {
            let count = elements.len();
            let min_length = self
                .tuple_optional_masks
                .get(&id)
                .map_or(count, |mask| mask.iter().filter(|&&optional| !optional).count());
            let lengths: Vec<_> = (min_length..=count)
                .map(|length| {
                    self.store.intern_literal(
                        crate::flags::TypeFlags::NUMBER_LITERAL,
                        crate::types::TypeData::NumberLiteral(length.to_string()),
                        false,
                    )
                })
                .collect();
            return Some(self.get_union_type(&lengths));
        }
        if let Some(property) = self.get_property_of_type_ex(id, name, skip_object_function_augment)
        {
            let declared = self.get_type_of_symbol(property);
            let instantiated =
                self.instantiate_for_reference_with_this(id, declared, this_argument);
            // §92: a property the symbol road FINDS but cannot type may
            // still answer through the shape road (chain1's low reads — the
            // alias symbol's table hands back a symbol whose declared type
            // does not compute).
            if instantiated == self.intrinsics.error
                && let Some(shaped) =
                    self.property_type_via_shape(id, name, skip_object_function_augment)
            {
                return Some(shaped);
            }
            return Some(instantiated);
        }
        // §393: a member inherited through a GENERIC heritage entry reads
        // through the INSTANTIATED base — `class T6 extends T5<number>`
        // answers `this.foo : number` for `foo: T` on `T5<T>`
        // (`superCallArgsMustMatch`). The symbol road above cannot see it:
        // `base_symbols_of` refuses type-argument heritage for exactly this
        // reason (§202 — a member table cannot be instantiated), and the
        // instantiation goes through §226's `base_type_of_heritage_entry`
        // plus the reference road's own member typing instead. Cycles are
        // guarded by the walk's visited set.
        if let Some(member) = self.generic_heritage_member(id, name, &mut Vec::new()) {
            return Some(member);
        }
        // §770: a TUPLE's non-numeric members come from `Array<T>`.
        //
        // Upstream's tuple is a REFERENCE to a target synthesised by
        // `createNormalizedTupleType` (`checker.go:24148`) whose base is
        // `Array<union of the element types>` (`ReadonlyArray` when readonly),
        // so `.length`, `.some`, `.every` and the rest resolve through the
        // ordinary base-member road. This port mints a tuple as a bare `Named`
        // object carrying an element list (`create_tuple_type`,
        // `declared.rs:1775`) with NO base at all, so it answered its numeric
        // indices (§8, above) and nothing else.
        //
        // §769 is what found this: giving an IIFE rest parameter its correct
        // TUPLE type turned `noNumbers.some(…)` from RIGHT to GAP, because the
        // parameter had been `any[]` — a type that does have array members —
        // and the tuple did not. That arm was reverted and this is its
        // recorded prerequisite.
        // `length` is NOT one of them. A tuple's own `length` is its element
        // COUNT — a literal for a plain tuple (§117 slice 4, above), and a
        // UNION of possible lengths when elements are optional, which §117
        // declines. Upstream's tuple target declares its own `length` rather
        // than inheriting `Array`'s `number`, so falling back here would
        // replace a deliberate decline with a confidently wrong answer:
        // measured at 2 R→W on `tupleTypes`' `readonly [number?]`, whose
        // baseline wants `0 | 1` and got `number`.
        if name != "length"
            && let Some((elements, readonly)) = self.tuple_element_lists.get(&id)
        {
            let (elements, readonly) = (elements.clone(), *readonly);
            let target = if readonly { "ReadonlyArray" } else { "Array" };
            if let Some(target) = self.global_type_symbol(target) {
                // The element type is the UNION of the tuple's elements, which
                // is what upstream's target is instantiated over. An empty
                // tuple has no elements and takes `never`, so `[].length` is
                // still `number` and `[].some` still resolves.
                let element = if elements.is_empty() {
                    self.intrinsics.never
                } else {
                    self.get_union_type(&elements)
                };
                let array = self.create_type_reference(target, vec![element]);
                if array != self.intrinsics.error
                    && let Some(property) = self.get_property_of_type(array, name)
                {
                    let declared = self.get_type_of_symbol(property);
                    return Some(self.instantiate_for_reference_with_this(array, declared, id));
                }
            }
        }
        self.property_type_via_shape(id, name, skip_object_function_augment)
    }

    /// Resolve inherited members through each instantiated base, guarded by
    /// symbol identity against cyclic heritage.
    fn generic_heritage_member(
        &mut self,
        id: TypeId,
        name: &str,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<TypeId> {
        let TypeData::Named { members: Some(owner), .. } = self.store.get(id).data else {
            return None;
        };
        if visiting.contains(&owner) {
            return None;
        }
        visiting.push(owner);
        let declarations: Vec<tsr_ast::NodeId> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        for declaration in declarations {
            let clauses = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(node)) => node.heritage_clauses,
                Some(Node::ClassExpression(node)) => node.heritage_clauses,
                Some(Node::InterfaceDeclaration(node)) => node.heritage_clauses,
                _ => continue,
            };
            for clause in clauses {
                if clause.token.kind != tsr_ast::SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for entry in clause.types {
                    let Some(base) = self.base_symbol_of_heritage_entry(entry, false) else {
                        continue;
                    };
                    if visiting.contains(&base) {
                        continue;
                    }
                    let Some(base_type) =
                        self.instantiated_heritage_base(base, entry.type_arguments, entry.node_id)
                    else {
                        continue;
                    };
                    // The ordinary found-path pair (symbol, then the
                    // reference's instantiation), with the DEEPER generic
                    // heritage recursing through THIS walk so the visited
                    // set holds across levels — a fresh set would spin on
                    // mutually-generic bases.
                    if let Some(property) = self.get_property_of_type(base_type, name) {
                        let declared = self.get_type_of_symbol(property);
                        let instantiated = self.instantiate_for_reference(base_type, declared);
                        if instantiated != self.intrinsics.error {
                            // §923: the SECOND substitution. The step above maps
                            // the base's parameters onto the heritage entry's
                            // arguments — for `interface D<T> extends C<T>` that
                            // is `C`'s `U := T`, which leaves `T`. The
                            // REFERENCE's own arguments (`D<string>`) are a
                            // separate map and nothing applied them, so
                            // `d.m` answered `(x: T) => T`.
                            //
                            // `extends C<string>` worked and hid it: a concrete
                            // heritage argument needs no second step, so the
                            // road looked complete.
                            //
                            // Instantiating for `id` composes the two. A
                            // non-generic reference maps nothing and the result
                            // is unchanged, so this cannot disturb the shapes
                            // that already worked.
                            let composed = self.instantiate_for_reference(id, instantiated);
                            if composed != self.intrinsics.error {
                                return Some(composed);
                            }
                            return Some(instantiated);
                        }
                    }
                    if let Some(member) = self.generic_heritage_member(base_type, name, visiting) {
                        // Resolve inherited members under every enclosing
                        // reference mapper, including indirect generic bases.
                        let member = self.instantiate_for_reference(id, member);
                        if member != self.intrinsics.error {
                            return Some(member);
                        }
                    }
                }
            }
        }
        None
    }

    /// Root declarations contributing an intersection property, corresponding
    /// to createUnionOrIntersectionProperty's distinct property set.
    pub(crate) fn intersection_property_symbols(
        &mut self,
        id: TypeId,
        name: &str,
    ) -> Vec<SymbolId> {
        let TypeData::Intersection { types, .. } = self.store.get(id).data.clone() else {
            return self.get_property_of_type(id, name).into_iter().collect();
        };
        let mut symbols = Vec::new();
        for part in types {
            let apparent = self.apparent_type(part);
            for symbol in self.intersection_property_symbols(apparent, name) {
                if !symbols.contains(&symbol) {
                    symbols.push(symbol);
                }
            }
        }
        symbols
    }

    /// §92 (`checker-notes-narrow.md`): the property roads the symbol table
    /// cannot answer — an INTERSECTION's constituents (multiple hits
    /// intersect, upstream's synthesized intersection property), `Omit<T, K>`
    /// by its global symbol (a name outside `K` reads through `T`), and an
    /// alias reference with a non-literal body (evaluated under §91's
    /// bindings, then re-asked). `None` stays *no such property*.
    fn property_type_via_shape(
        &mut self,
        id: TypeId,
        name: &str,
        skip_object_function_augment: bool,
    ) -> Option<TypeId> {
        // createUnionOrIntersectionProperty (checker.go): each constituent
        // contributes its apparent property type, with the entire receiver
        // substituted for polymorphic this before intersecting the types.
        if let TypeData::Intersection { types, .. } = &self.store.get(id).data {
            let constituents = types.clone();
            if self.intersection_has_never_discriminant(id) {
                return None;
            }
            for skip_augment in [true, false] {
                if !skip_augment && skip_object_function_augment {
                    break;
                }
                let mut hits = Vec::new();
                for &constituent in &constituents {
                    let apparent = self.apparent_type(constituent);
                    if let Some(member) = self.get_type_of_property_with_this_argument(
                        apparent,
                        name,
                        id,
                        skip_augment,
                    ) {
                        hits.push(member);
                    }
                }
                if !hits.is_empty() {
                    return Some(self.get_intersection_type(&hits, None));
                }
            }
            return None;
        }
        let (target, arguments) = self.type_reference_targets.get(&id).cloned()?;
        if self.global_type_symbol_with_arity("Omit", 2) == Some(target) && arguments.len() == 2 {
            let removed = self.literal_key_texts(arguments[1])?;
            if removed.iter().any(|key| key == name) {
                return None;
            }
            // `Omit<T, K>` is `Pick<T, Exclude<keyof T, K>>`: a key exists
            // only where `keyof T` has one, and getLiteralTypeFromProperty
            // excludes private/protected members. For a generic `T` the
            // resolved mapped member is the template `T[P]` with `P` fixed,
            // which stays a deferred indexed access (`this["publicProp"]`,
            // `destructuringUnspreadableIntoRest`).
            let source = arguments[0];
            let apparent = self.apparent_type(source);
            if let Some(property) = self.get_property_of_type(apparent, name)
                && self.is_non_public_member(property)
            {
                return None;
            }
            if self.store.get(source).flags.intersects(TypeFlags::TYPE_PARAMETER) {
                self.get_property_of_type(apparent, name)?;
                let key = self.store.intern_literal(
                    TypeFlags::STRING_LITERAL,
                    crate::types::TypeData::StringLiteral(name.to_string()),
                    false,
                );
                return self.resolved_indexed_access_type(source, key, false);
            }
            return self.get_type_of_property_with_this_argument(
                source,
                name,
                source,
                skip_object_function_augment,
            );
        }
        if self.binder.symbols().get(target).flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS) {
            let evaluated = self.evaluate_alias_body(target, &arguments)?;
            if evaluated != id {
                return self.get_type_of_property_with_this_argument(
                    evaluated,
                    name,
                    evaluated,
                    skip_object_function_augment,
                );
            }
        }
        None
    }

    /// A member's type as seen through an instantiated reference: `declared`
    /// with the receiver's type arguments substituted in, or `declared`
    /// unchanged when the receiver is not an instantiated reference.
    ///
    /// The instantiation half of `getTypeOfPropertyOfType` reached through
    /// upstream's `instantiateSymbol` (`checker.go:20753`) — upstream
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
        self.instantiate_for_reference_with_this(receiver, declared, receiver)
    }

    fn instantiate_for_reference_with_this(
        &mut self,
        receiver: TypeId,
        declared: TypeId,
        this_argument: TypeId,
    ) -> TypeId {
        // resolveTypeReferenceMembers also supplies a this argument for a
        // non-generic class or interface. The port keeps those as Named types
        // rather than entries in type_reference_targets.
        let (symbol, arguments) =
            if let Some((symbol, arguments)) = self.type_reference_targets.get(&receiver) {
                (*symbol, Some(arguments.clone()))
            } else if self.store.get(receiver).flags.contains(TypeFlags::OBJECT)
                && let TypeData::Named { members: Some(symbol), .. } = self.store.get(receiver).data
                && self
                    .binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .intersects(SymbolFlags::CLASS | SymbolFlags::INTERFACE)
            {
                (symbol, None)
            } else {
                return declared;
            };
        let error = self.intrinsics.error;
        let Some(parameters) = self.local_type_parameter_types_of(symbol) else {
            return error;
        };
        let arguments = arguments
            .unwrap_or_else(|| parameters.iter().map(|(parameter, _)| *parameter).collect());
        if parameters.len() != arguments.len() {
            return error;
        }
        // instantiateSymbol (checker.go:20753) retains the complete receiver
        // mapper. A member's own same-named parameter has a distinct TypeId,
        // so it survives while an outer parameter in the same return is mapped.
        let mut names: Vec<&str> = Vec::new();
        let mut types: Vec<TypeId> = Vec::new();
        let mut map: Vec<(TypeId, TypeId)> = Vec::new();
        for (index, (parameter, name)) in parameters.iter().enumerate() {
            if *parameter != arguments[index] {
                names.push(name.as_str());
                types.push(*parameter);
                map.push((*parameter, arguments[index]));
            }
        }
        // resolveTypeReferenceMembers pads the type arguments with the
        // reference itself for the target's polymorphic this parameter.
        let this_type = self.this_types.get(&symbol).copied().or_else(|| {
            self.binder
                .symbols()
                .get(symbol)
                .declarations
                .iter()
                .find_map(|node| self.this_type_nodes.get(node).copied())
        });
        if let Some(this_type) = this_type
            && this_type != this_argument
        {
            types.push(this_type);
            map.push((this_type, this_argument));
        }
        // A reference without type parameters or a polymorphic this needs no map.
        if map.is_empty() {
            return declared;
        }
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
    /// `new C().T` would answer with the type parameter `T`. **The alias half is
    /// ported too** — see [`Checker::symbol_is_value`]; it used to be a miss, and
    /// a miss on this gate is what `nonexistent_property` reports as TS2339.
    ///
    /// # Two tables, chosen by what the type *is*
    ///
    /// A `TypeData::Named` is the instance side and looks in `members`; a
    /// `TypeData::Anonymous` is `typeof X` and looks in `exports`. See
    /// [`Checker::get_property_of_anonymous_symbol`] for why that is not the same
    /// table with a different name.
    #[must_use]
    pub fn get_property_of_type(&mut self, id: TypeId, name: &str) -> Option<SymbolId> {
        self.get_property_of_type_ex(id, name, false)
    }

    pub(crate) fn get_property_of_type_ex(
        &mut self,
        id: TypeId,
        name: &str,
        skip_object_function_augment: bool,
    ) -> Option<SymbolId> {
        // The borrow of `self.store` has to end before the recursion below, which
        // takes `&mut self`. Both bindings are `Copy`, so this statement copies
        // out what it needs and releases the type. ADR-0013's read-drop-recurse.
        let owner = match &self.store.get(id).data {
            TypeData::Named { members: Some(owner), .. } => Owner::Declared(*owner),
            TypeData::Anonymous { symbol, .. } => Owner::Anonymous(*symbol),
            _ => return None,
        };
        let found = match owner {
            Owner::Declared(owner) => {
                let mut visiting = Vec::new();
                self.get_property_of_declared_symbol(owner, name, &mut visiting)
            }
            Owner::Anonymous(symbol) => self.get_property_of_anonymous_symbol(symbol, name),
        };
        if found.is_some() {
            return found;
        }
        // §117 slice 1 (`getPropertyOfTypeEx`, `checker.go:18918`): an
        // OBJECT-flagged receiver's miss falls back to the FUNCTION interface
        // family when it carries signatures (the strictBindCallApply
        // interfaces, falling to `Function` when unmounted), then to the
        // global `Object` — always, which is what makes `.toString` answer
        // on every object while a genuine `.foo` still misses (Object misses
        // it too, byte-identical by construction).
        // Upstream withholds the fallback from CONST-ENUM objects (their
        // prototype access is TS2748-family territory) —
        // `constEnumNoObjectPrototypePropertyAccess`'s 14 G→W measured
        // without this gate.
        let withheld = match owner {
            Owner::Declared(owner) | Owner::Anonymous(owner) => self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(owner))
                .flags
                .contains(SymbolFlags::CONST_ENUM),
        };
        if !skip_object_function_augment
            && !withheld
            && self.store.get(id).flags.contains(TypeFlags::OBJECT)
        {
            let mut fallbacks: Vec<&str> = Vec::new();
            // §395: a CLASS's static side is a constructor function — its
            // misses fall through the Function interface before Object
            // (`Foo.name : string`, `deleteReadonlyInStrictNullChecks`).
            // The signature-carrying test below cannot see it because a
            // class's anonymous static type registers no entry in
            // `signature_types`.
            let class_static = match owner {
                Owner::Anonymous(symbol) => self
                    .binder
                    .symbols()
                    .get(self.binder.merged_symbol(symbol))
                    .flags
                    .contains(SymbolFlags::CLASS),
                Owner::Declared(_) => false,
            };
            let has_call = self
                .signatures_of_type_kind(id, crate::signatures::SignatureKind::Call)
                .is_some_and(|signatures| !signatures.is_empty());
            let has_construct = self
                .signatures_of_type_kind(id, crate::signatures::SignatureKind::Construct)
                .is_some_and(|signatures| !signatures.is_empty());
            if has_call || has_construct {
                let all_construct = !has_call;
                // §846: `CallableFunction`/`NewableFunction` are
                // `getGlobalStrictFunctionType`'s answer, and that function is
                // gated on the flag:
                //
                // ```go
                // func (c *Checker) getGlobalStrictFunctionType(name string) *Type {
                //     if c.strictBindCallApply {
                //         return c.getGlobalType(name, 0 /*arity*/, true /*reportErrors*/)
                //     }
                //     return c.globalFunctionType
                // }
                // ```
                //
                // With the flag off both ARE `Function`. Reaching for
                // `CallableFunction` unconditionally made `f.bind` resolve to
                // its two generic `this`-parameter overloads instead of
                // `Function.bind`, so relating a function type to `Function`
                // stopped being identity and became a structurally hard
                // question the relater answers `false` — the corpus's
                // `ElementRef & Function` rows, via
                // `narrow_type_by_type_facts`' third arm.
                if self.strict_bind_call_apply {
                    fallbacks.push(if all_construct {
                        "NewableFunction"
                    } else {
                        "CallableFunction"
                    });
                }
                fallbacks.push("Function");
            }
            // Resolved class constructors use NewableFunction before Function,
            // just as getPropertyOfTypeEx selects by signature kind. Retain the
            // previous fallback only when constructor resolution is unsupported.
            if class_static && !has_call && !has_construct {
                fallbacks.push("Function");
            }
            fallbacks.push("Object");
            for global in fallbacks {
                let Some(interface) = self.global_type_symbol_with_arity(global, 0) else {
                    continue;
                };
                let mut visiting = Vec::new();
                if let Some(found) =
                    self.get_property_of_declared_symbol(interface, name, &mut visiting)
                {
                    return Some(found);
                }
            }
        }
        None
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
    fn get_property_of_anonymous_symbol(
        &mut self,
        symbol: SymbolId,
        name: &str,
    ) -> Option<SymbolId> {
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
        let is_class = data.flags.contains(SymbolFlags::CLASS);
        // An own export first, then the module's `export *` re-exports. Reading
        // the table alone is what made every member of a barrel module missing:
        // `import * as z from "zod"` names a module whose entire surface arrives
        // through `export *`, so `z.string` found nothing.
        let found = match data.exports.get(name).copied() {
            Some(found) => Some(found),
            None if is_class => self
                .late_bound_static_members_of(symbol)
                .into_iter()
                .find_map(|(spelled, member)| (spelled == name).then_some(member))
                .or_else(|| self.get_export_from_star(symbol, name)),
            None => self.get_export_from_star(symbol, name),
        };
        let found = match found {
            Some(found) => found,
            // §122 (`checker-notes-narrow.md`): a class's STATIC side is a
            // real inheritance chain — upstream's constructor type takes the
            // base class's constructor type as its base
            // (`getBaseConstructorTypeOfClass`), so `class D extends B` finds
            // `B.x` through `typeof D`. Own exports answered above, which is
            // what makes a derived redeclaration shadow by construction.
            // A `#private` static never inherits: private names are
            // lexically scoped to the declaring class body, and upstream
            // answers its error-any for `Derived.#x` (TS18013 territory) —
            // the walk carrying it measured 4 G→W
            // (privateNameStaticAccessorssDerivedClasses wants `any`).
            None if is_class && !name.starts_with('#') => {
                let mut visiting = vec![symbol];
                self.static_property_of_bases(symbol, name, &mut visiting)?
            }
            None => return None,
        };
        self.symbol_is_value(found).then_some(found)
    }

    /// §123's completeness probe: whether a Named receiver's base-type walk
    /// can be followed to the end — every `base_symbols_of` on the chain
    /// answers. Only then is a member's absence ESTABLISHED rather than
    /// unknown. A cycle answers `false` (decline, honest gap) — upstream
    /// reports a base-cycle diagnostic there, a channel this port lacks.
    fn named_walk_is_complete(&mut self, receiver: TypeId) -> bool {
        // §124: the Anonymous side's analogue. A CLASS owner establishes
        // absence through the same extends-chain walk §122 reads; a FUNCTION
        // or ENUM owner's exports are single-declaration-set and whole by
        // construction. VALUE_MODULE is NOT admitted — `export *` can carry
        // surface this port cannot see (the merged-symbol read happens first,
        // so `function f` merged with `namespace f` is excluded with it).
        if let TypeData::Anonymous { symbol, .. } = self.store.get(receiver).data {
            let merged = self.binder.merged_symbol(symbol);
            let flags = self.binder.symbols().get(merged).flags;
            if flags.contains(SymbolFlags::VALUE_MODULE) {
                return false;
            }
            if flags.contains(SymbolFlags::CLASS) {
                let mut visiting = Vec::new();
                return self.walk_completes(merged, &mut visiting)
                    && !self.chain_declares_index_signature(merged);
            }
            // FUNCTION owners measured 24 G→W (strictBindCallApply1): a
            // "missing" function member may live on the CallableFunction /
            // Function wrapper interfaces with a specialized type this
            // port's fallback answers differently — absence not established.
            return flags.intersects(SymbolFlags::ENUM);
        }
        let TypeData::Named { members: Some(owner), .. } = self.store.get(receiver).data else {
            return false;
        };
        // Only owners declared EXCLUSIVELY by class/interface declarations —
        // the two forms whose members tables the binder populates whole. A
        // Named minted for an alias of a mapped/conditional body carries a
        // table that was never the type's member list (mappedTypes2 21,
        // conditionalTypes1 26, recursiveIntersectionTypes 24 G→W on the
        // ungated pair — upstream computes real members there).
        // §124.2: VALUE-only declarations are IGNORED by the test — a lib
        // wrapper merges `interface Number` with `var Number:
        // NumberConstructor`, and the var contributes nothing to the
        // instance side; requiring all() of the raw list meant §123 never
        // established a single lib-wrapper miss (probe 14's walk=false on
        // `.length`-of-number, the §142 repro's final conjunct).
        let declared_only = {
            let symbol = self.binder.symbols().get(owner);
            let mut type_side = 0usize;
            let mut foreign = 0usize;
            for &declaration in &symbol.declarations {
                match self.nodes.kind(declaration) {
                    tsr_ast::SyntaxKind::ClassDeclaration
                    | tsr_ast::SyntaxKind::ClassExpression
                    | tsr_ast::SyntaxKind::InterfaceDeclaration => type_side += 1,
                    tsr_ast::SyntaxKind::VariableDeclaration
                    | tsr_ast::SyntaxKind::FunctionDeclaration => {}
                    _ => foreign += 1,
                }
            }
            type_side >= 1 && foreign == 0
        };
        if !declared_only {
            return false;
        }
        let mut visiting = Vec::new();
        self.walk_completes(owner, &mut visiting)
    }

    /// §123/§124's combined question: is this name's absence ESTABLISHED?
    /// A `CONST_ENUM` owner skips the `Function`-family gate — upstream
    /// deliberately withholds the prototype road there (TS2748-family, the
    /// §117 fallback's own recorded gate), so `E.toString`'s error-any IS
    /// the established answer (constEnumNoObjectPrototypePropertyAccess 14).
    fn miss_is_established(&mut self, receiver: TypeId, name: &str) -> bool {
        // §125, re-measured 2026-08-10 as §161 (`checker-notes-narrow.md`):
        // the union per-constituent arm re-refused at 63:213 — the same
        // ~1:3.4 as the original 39:164 despite the flow arc's +170. The
        // class is narrowing-owned WHOLE, twice measured.
        let const_enum = if let TypeData::Anonymous { symbol, .. } = self.store.get(receiver).data {
            let merged = self.binder.merged_symbol(symbol);
            self.binder.symbols().get(merged).flags.contains(SymbolFlags::CONST_ENUM)
        } else {
            false
        };
        if !const_enum && self.function_family_declares(receiver, name) {
            return false;
        }
        self.named_walk_is_complete(receiver)
    }

    /// §124 iteration 3: a name any of the Function-family or Object globals
    /// declares is never an established absence — the §117 fallback family
    /// may answer it with a type this port computes differently or not at
    /// all (`C.bind` under strictBindCallApply wants the specialized
    /// signature; strictBindCallApply1 measured 24 G→W through two
    /// iterations before this gate named the mechanism).
    /// §124.1's third callable detector: any declaration of the symbol
    /// carries a call- or construct-signature member.
    fn symbol_declares_signature_member(&self, owner: SymbolId) -> bool {
        self.binder.symbols().get(owner).declarations.iter().any(|&declaration| {
            let members: &[tsr_ast::TypeElement<'_>] = match self.node_map.get(declaration) {
                Some(Node::InterfaceDeclaration(node)) => node.members,
                Some(Node::TypeLiteralNode(node)) => node.members,
                _ => return false,
            };
            members.iter().any(|member| {
                matches!(
                    member,
                    tsr_ast::TypeElement::CallSignatureDeclaration(_)
                        | tsr_ast::TypeElement::ConstructSignatureDeclaration(_)
                )
            })
        })
    }

    fn function_family_declares(&mut self, receiver: TypeId, name: &str) -> bool {
        // §124.1: the Function-family interfaces only gate SIGNATURE-BEARING
        // receivers — the §117 fallback never consults them otherwise, so a
        // `.length` miss on `number` is fully established (§142's probe 5
        // found the over-wide gate erring whole literals through inferred
        // returns). Object's names gate unconditionally, mirroring its
        // unconditional fallback.
        let callable = self.signature_types.get(&receiver).is_some_and(|s| !s.is_empty())
            || matches!(self.store.get(receiver).data,
                crate::types::TypeData::Anonymous { symbol, .. }
                    if self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags.intersects(
                        SymbolFlags::FUNCTION | SymbolFlags::METHOD | SymbolFlags::CLASS))
            // Third detector: a NAMED owner whose declarations carry
            // call/construct SIGNATURE members (`{ (): void; }` type
            // literals and interfaces — objectTypeWithCallSignature*'s 10
            // on the two-detector pair).
            || matches!(self.store.get(receiver).data,
                crate::types::TypeData::Named { members: Some(owner), .. }
                    if self.symbol_declares_signature_member(owner));
        let families: &[&str] = if callable {
            &["CallableFunction", "NewableFunction", "Function", "Object"]
        } else {
            &["Object"]
        };
        for global in families {
            if let Some(interface) = self.global_type_symbol_with_arity(global, 0) {
                let mut visiting = Vec::new();
                if self.get_property_of_declared_symbol(interface, name, &mut visiting).is_some() {
                    return true;
                }
            }
        }
        false
    }

    /// §124: whether any class declaration on the extends chain declares an
    /// index signature — a member this port's static index road cannot
    /// answer, so a miss beside one is not an established absence
    /// (staticIndexSignature4's 20 G→W on the ungated pair).
    fn chain_declares_index_signature(&mut self, owner: SymbolId) -> bool {
        let mut visiting = Vec::new();
        self.chain_declares_index_signature_worker(owner, &mut visiting)
    }

    fn chain_declares_index_signature_worker(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return true;
        }
        visiting.push(owner);
        let declarations: Vec<_> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        for declaration in declarations {
            if let Some(Node::ClassDeclaration(class)) = self.node_map.get(declaration)
                && class.members.iter().any(|member| {
                    matches!(member, tsr_ast::ClassElement::IndexSignatureDeclaration(_))
                })
            {
                return true;
            }
        }
        match self.base_symbols_of(owner) {
            None => true,
            Some(bases) => bases
                .into_iter()
                .any(|base| self.chain_declares_index_signature_worker(base, visiting)),
        }
    }

    fn walk_completes(&mut self, owner: SymbolId, visiting: &mut Vec<SymbolId>) -> bool {
        if visiting.contains(&owner) {
            return false;
        }
        visiting.push(owner);
        match self.base_symbols_of(owner) {
            None => false,
            Some(bases) => bases.into_iter().all(|base| self.walk_completes(base, visiting)),
        }
    }

    /// §122's walk: each base's `exports`, depth-first in declaration order,
    /// first hit wins. `base_symbols_of`'s refusals (instantiated or
    /// non-identifier heritage) gap the whole walk rather than answer a wrong
    /// symbol, exactly as the instance side's walk does.
    fn static_property_of_bases(
        &mut self,
        owner: SymbolId,
        name: &str,
        visiting: &mut Vec<SymbolId>,
    ) -> Option<SymbolId> {
        for base in self.base_symbols_of(owner)? {
            if visiting.contains(&base) {
                continue;
            }
            visiting.push(base);
            if let Some(&found) = self.binder.symbols().get(base).exports.get(name)
                && self.symbol_is_value(found)
            {
                return Some(found);
            }
            if let Some(found) = self
                .late_bound_static_members_of(base)
                .into_iter()
                .find_map(|(spelled, member)| (spelled == name).then_some(member))
            {
                return Some(found);
            }
            if let Some(found) = self.static_property_of_bases(base, name, visiting) {
                return Some(found);
            }
        }
        None
    }

    /// `lateBindMember` / `getPropertyNameFromType` (checker.go, utilities.go):
    /// resolve computed declarations to semantic names. Literal keys use their
    /// values; symbol keys retain this port's existing bracketed identity.
    /// Printing reads the declaration separately. The cache sentinel makes a
    /// recursive key fall back to the early-bound members, as upstream does.
    pub(crate) fn late_bound_members_of(
        &mut self,
        owner: SymbolId,
        is_static: bool,
    ) -> Vec<(String, tsr_ast::NodeId)> {
        let cache_key = (owner, is_static);
        if let Some(members) = self.late_bound_member_names.get(&cache_key) {
            return members.clone();
        }
        self.late_bound_member_names.insert(cache_key, Vec::new());
        let declarations: Vec<tsr_ast::NodeId> =
            self.binder.symbols().get(owner).declarations.iter().copied().collect();
        let mut out = Vec::new();
        for declaration in declarations {
            let member_ids: Vec<tsr_ast::NodeId> = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => {
                    class.members.iter().filter_map(|m| tsr_ast::Node::from(*m).node_id()).collect()
                }
                Some(Node::ClassExpression(class)) => {
                    class.members.iter().filter_map(|m| tsr_ast::Node::from(*m).node_id()).collect()
                }
                Some(Node::InterfaceDeclaration(interface)) => interface
                    .members
                    .iter()
                    .filter_map(|m| tsr_ast::Node::from(*m).node_id())
                    .collect(),
                Some(Node::TypeLiteralNode(literal)) => literal
                    .members
                    .iter()
                    .filter_map(|m| tsr_ast::Node::from(*m).node_id())
                    .collect(),
                _ => continue,
            };
            for member in member_ids {
                let Some(
                    Node::PropertyDeclaration(&tsr_ast::PropertyDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::PropertySignatureDeclaration(&tsr_ast::PropertySignatureDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::MethodDeclaration(&tsr_ast::MethodDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::MethodSignatureDeclaration(&tsr_ast::MethodSignatureDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::GetAccessorDeclaration(&tsr_ast::GetAccessorDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    })
                    | Node::SetAccessorDeclaration(&tsr_ast::SetAccessorDeclaration {
                        name: tsr_ast::PropertyName::ComputedPropertyName(computed),
                        ..
                    }),
                ) = self.node_map.get(member)
                else {
                    continue;
                };
                let member_is_static = self.binder.symbol_of(member).is_some_and(|symbol| {
                    self.property_has_modifier(symbol, tsr_ast::SyntaxKind::StaticKeyword)
                });
                if member_is_static != is_static {
                    continue;
                }
                let Some(expression) = computed.expression else { continue };
                let name_type = self.check_expression(expression);
                let name = self.property_name_from_index(name_type).or_else(|| {
                    self.late_bound_symbol_member_name(computed).map(|(spelled, _)| spelled)
                });
                if let Some(name) = name {
                    out.push((name, member));
                }
            }
        }
        self.late_bound_member_names.insert(cache_key, out.clone());
        out
    }

    /// resolveAnonymousTypeMembers (checker.go): class values own a static side.
    pub(crate) fn class_static_symbol(&self, id: TypeId) -> Option<tsr_binder::SymbolId> {
        let TypeData::Anonymous { symbol, .. } = self.type_of(id).data else { return None };
        let symbol = self.binder.merged_symbol(symbol);
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .contains(tsr_binder::SymbolFlags::CLASS)
            .then_some(symbol)
    }

    /// The names of every property of `id`, own and inherited, or `None` if any
    /// base type could not be followed.
    ///
    /// Ported from `Checker.getPropertiesOfType` → `getPropertiesOfObjectType`
    /// (`internal/checker/checker.go`). Upstream reads a resolved members table
    /// that already has the base types layered in; there is none here, so this
    /// walks the same base-symbol graph [`Checker::get_property_of_declared_symbol`]
    /// walks and collects names instead of resolving one. Names only: the
    /// *symbol* for a name is then taken from
    /// [`Checker::get_property_of_type`], so shadowing is decided in exactly one
    /// place rather than twice.
    ///
    /// The `None`-on-an-unfollowable-base rule is [`Checker::base_symbols_of`]'s
    /// and is why the walk cannot silently under-report a requirement.
    pub(crate) fn get_property_names_of_type(&mut self, id: TypeId) -> Option<Vec<String>> {
        if let Some((properties, true)) = self.anonymous_properties.get(&id) {
            return Some(properties.iter().map(|property| property.name.clone()).collect());
        }
        if let Some(symbol) = self.class_static_symbol(id) {
            let mut names = vec!["prototype".to_owned()];
            return self
                .collect_static_property_names(symbol, &mut names, &mut Vec::new())
                .then_some(names);
        }
        // resolveAnonymousTypeMembers: function, enum and module values expose
        // exports. Class statics were handled above; instance members stay separate.
        if let TypeData::Anonymous { symbol, signature, .. } = self.type_of(id).data
            && (signature
                || self
                    .binder
                    .symbols()
                    .get(symbol)
                    .flags
                    .intersects(SymbolFlags::ENUM | SymbolFlags::VALUE_MODULE))
        {
            let mut names: Vec<_> = self
                .binder
                .symbols()
                .get(symbol)
                .exports
                .iter()
                .filter(|(_, member)| self.symbol_is_value(**member))
                .map(|(&name, _)| name.to_owned())
                .collect();
            if let Some((properties, _)) = self.anonymous_properties.get(&id) {
                for property in properties {
                    if !names.contains(&property.name) {
                        names.push(property.name.clone());
                    }
                }
            }
            return Some(names);
        }
        let TypeData::Named { members: Some(owner), .. } = self.type_of(id).data else {
            return None;
        };
        if let Some(names) = self.mapped_alias_literal_key_names(id, owner) {
            return Some(names);
        }
        let mut names = Vec::new();
        let mut visiting = Vec::new();
        self.collect_structured_property_names(owner, &mut names, &mut visiting).then_some(names)
    }

    /// The property names resolveMappedTypeMembers (checker.go) gives a
    /// reference to a generic alias whose body is `{ [P in K]: … }` over its own
    /// parameter `K`, when the reference supplies string literal keys:
    /// `Record<"name", T>` has `name`. The reference is owned by the alias
    /// symbol, whose table was never the type's member list, so enumerating
    /// that table would answer no names and let a structured target demand
    /// nothing. Other key shapes keep the existing enumeration.
    fn mapped_alias_literal_key_names(&self, id: TypeId, owner: SymbolId) -> Option<Vec<String>> {
        if !self.binder.symbols().get(owner).flags.contains(SymbolFlags::TYPE_ALIAS) {
            return None;
        }
        let (target, arguments) = self.type_reference_targets.get(&id)?;
        if *target != owner {
            return None;
        }
        let declaration = self.binder.symbols().get(owner).declarations.first().copied()?;
        let Some(Node::TypeAliasDeclaration(alias)) = self.node_map.get(declaration) else {
            return None;
        };
        let Some(tsr_ast::TypeNode::MappedTypeNode(mapped)) = alias.r#type else { return None };
        if mapped.name_type.is_some() {
            return None;
        }
        let Some(tsr_ast::TypeNode::TypeReferenceNode(constraint)) =
            mapped.type_parameter.and_then(|parameter| parameter.constraint)
        else {
            return None;
        };
        let Some(tsr_ast::EntityName::Identifier(name)) = constraint.type_name else {
            return None;
        };
        if !constraint.type_arguments.is_empty() {
            return None;
        }
        let position = alias
            .type_parameters
            .iter()
            .position(|parameter| parameter.name.is_some_and(|own| own.text == name.text))?;
        self.literal_key_texts(*arguments.get(position)?)
    }

    /// resolveAnonymousTypeMembers (checker.go): the class's static properties
    /// include base exports. The synthetic prototype is added by the caller.
    fn collect_static_property_names(
        &mut self,
        owner: tsr_binder::SymbolId,
        names: &mut Vec<String>,
        visiting: &mut Vec<tsr_binder::SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return true;
        }
        let inherited = !visiting.is_empty();
        visiting.push(owner);
        for (&name, &symbol) in &self.binder.symbols().get(owner).exports {
            if self.symbol_is_value(symbol)
                && !(inherited && name.starts_with('#'))
                && !names.iter().any(|existing| existing == name)
            {
                names.push(name.to_owned());
            }
        }
        for (name, _) in self.late_bound_static_members_of(owner) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let Some(bases) = self.base_symbols_of_ex(owner, false) else { return false };
        bases.into_iter().all(|base| self.collect_static_property_names(base, names, visiting))
    }

    /// One step of [`Checker::get_property_names_of_type`]'s walk.
    ///
    /// The `visiting` guard is [`Checker::get_property_of_declared_symbol`]'s,
    /// for the same reason: `class A extends B` with `class B extends A` is a
    /// real cycle in the base-type graph. Re-entry contributes nothing rather
    /// than failing — every name reachable through the cycle has already been
    /// collected by the outer visit.
    fn collect_structured_property_names(
        &mut self,
        owner: tsr_binder::SymbolId,
        names: &mut Vec<String>,
        visiting: &mut Vec<tsr_binder::SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return true;
        }
        visiting.push(owner);
        // A members table also holds type parameters, so the value gate is the
        // same one `getPropertyOfType` applies; without it `interface I<T>`
        // would demand a property named `T`.
        let own: Vec<String> = self
            .binder
            .symbols()
            .get(owner)
            .members
            .iter()
            .filter(|&(_, &symbol)| self.symbol_is_value(symbol))
            .map(|(&name, _)| name.to_owned())
            .collect();
        for name in own {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        // getResolvedMembersOrExportsOfSymbol keeps instance members and exports
        // separate, including computed declarations.
        for (name, _) in self.late_bound_members_of(owner, false) {
            if !names.contains(&name) {
                names.push(name);
            }
        }
        let Some(bases) = self.base_symbols_of_ex(owner, false) else {
            return false;
        };
        bases.into_iter().all(|base| self.collect_structured_property_names(base, names, visiting))
    }

    pub(crate) fn late_bound_static_members_of(
        &mut self,
        owner: SymbolId,
    ) -> Vec<(String, SymbolId)> {
        // resolveAnonymousTypeMembers / getLateBoundSymbol (checker.go):
        // computed static members belong to exports, never instance members.
        self.late_bound_members_of(owner, true)
            .into_iter()
            .filter_map(|(name, declaration)| {
                self.binder.symbol_of(declaration).map(|symbol| (name, symbol))
            })
            .collect()
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
        // getPropertyOfType searches the resolved member table for every
        // semantic key, including string and numeric late-bound names.
        for (key, member) in self.late_bound_members_of(owner, false) {
            if key == name
                && let Some(symbol) = self.binder.symbol_of(member)
            {
                return Some(symbol);
            }
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
        self.base_symbols_of_ex(owner, true)
    }

    /// [`Self::base_symbols_of`], with the type-argument refusal made optional.
    ///
    /// **The refusal belongs to the INSTANCE side only.** A heritage entry's
    /// type arguments decide what `extends B<any>` contributes as a member
    /// table, and this port cannot instantiate one, so the member road must
    /// gap. They decide nothing about the base's **static** side:
    /// `getBaseConstructorTypeOfClass` (`checker.go:17434`) types the heritage
    /// entry's *expression*, and `B` is `typeof B` whatever follows it in
    /// angle brackets.
    ///
    /// So `super()` inside `class D extends B<any>` is `typeof B` upstream and
    /// was a gap here — the caller inherited a conservatism it did not need,
    /// which is `docs/conventions.md` corollary 11 one layer in: not a refusal
    /// whose stated reason is wrong, but a shared helper whose reason is right
    /// for its first caller and wider than the second caller requires. §202.
    pub(crate) fn base_symbols_of_ex(
        &mut self,
        owner: SymbolId,
        refuse_type_arguments: bool,
    ) -> Option<Vec<SymbolId>> {
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
                    bases.push(self.base_symbol_of_heritage_entry(base, refuse_type_arguments)?);
                }
            }
        }
        Some(bases)
    }

    /// The symbol one `extends` entry names, or `None` if it is a gap.
    pub(crate) fn base_symbol_of_heritage_entry(
        &mut self,
        entry: &tsr_ast::ExpressionWithTypeArguments<'_>,
        refuse_type_arguments: bool,
    ) -> Option<SymbolId> {
        if refuse_type_arguments && !entry.type_arguments.is_empty() {
            return None;
        }
        let symbol = self.heritage_entity_symbol(entry.expression?, SymbolFlags::TYPE)?;
        // A symbol-only member walk cannot apply either explicit arguments or
        // omitted defaults. Route generic bases through their instantiated type.
        if refuse_type_arguments && !self.local_type_parameters_of(symbol).is_empty() {
            return None;
        }
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

    /// resolveEntityName for the expression-shaped names in heritage clauses.
    /// Intermediate namespaces and imported aliases are resolved before exports.
    fn heritage_entity_symbol(
        &mut self,
        expression: tsr_ast::Expression<'_>,
        meaning: SymbolFlags,
    ) -> Option<SymbolId> {
        let symbol = match expression {
            tsr_ast::Expression::Identifier(name) => self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name.node_id?,
                name.text,
                meaning | SymbolFlags::ALIAS,
            )?,
            tsr_ast::Expression::PropertyAccessExpression(access) => {
                let owner =
                    self.heritage_entity_symbol(access.expression?, SymbolFlags::NAMESPACE)?;
                let tsr_ast::MemberName::Identifier(name) = access.name? else { return None };
                *self.binder.symbols().get(owner).exports.get(name.text)?
            }
            _ => return None,
        };
        let symbol = self.qualified_alias_target(symbol).unwrap_or(symbol);
        let symbol = self.resolve_alias_fully(symbol);
        let symbol = self.binder.merged_symbol(symbol);
        self.binder.symbols().get(symbol).flags.intersects(meaning).then_some(symbol)
    }

    /// `Checker.symbolIsValueEx` (`checker.go:22095`), **both halves**:
    ///
    /// ```go
    /// return symbol.Flags&ast.SymbolFlagsValue != 0 || symbol.Flags&ast.SymbolFlagsAlias != 0 &&
    ///     c.getSymbolFlagsEx(symbol, …)&ast.SymbolFlagsValue != 0
    /// ```
    ///
    /// # The alias half, and why it was missing
    ///
    /// An alias's own flags carry **no** `VALUE` bit — that is the whole reason
    /// upstream's test has a second disjunct. So a value-ness gate over the raw
    /// flags says no to every `export { x } from "./m"`, and
    /// [`Checker::get_property_of_anonymous_symbol`] turned that `no` into
    /// *"property does not exist"*.
    ///
    /// This was recorded as unported with the reason *"nothing follows aliases
    /// yet (`bd tsr-y4u.12`)"*, and that reason expired:
    /// [`Checker::get_symbol_flags`] is upstream's `getSymbolFlagsEx` and has
    /// followed aliases since — `get_type_of_alias` already takes its own
    /// `VALUE` test over it for exactly this reason. The stale note is what let
    /// the gap survive.
    ///
    /// The gap is not exotic. Every `index.parts.d.ts`-style barrel is a file
    /// of nothing but export specifiers, so `import * as P from "…/parts"` then
    /// `P.Root` reported TS2339 on every member — 1,384 diagnostics on a
    /// 22-package repository, against `tsc`'s zero.
    /// `docs/architecture/checker-notes-diag2.md` §400.
    ///
    /// **`&mut self`, because resolving an alias can create types.** The three
    /// callers all had a `&mut` receiver already.
    pub(crate) fn symbol_is_value(&mut self, symbol: SymbolId) -> bool {
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(SymbolFlags::VALUE) {
            return true;
        }
        flags.intersects(SymbolFlags::ALIAS)
            && !self.alias_is_type_only(symbol)
            && self.get_symbol_flags(symbol).intersects(SymbolFlags::VALUE)
    }

    /// `getTypeOnlyAliasDeclaration(symbol) != nil`, as
    /// `getSymbolFlagsEx`'s `excludeTypeOnlyMeanings` guard uses it
    /// (`checker.go:16374`):
    ///
    /// ```go
    /// if excludeTypeOnlyMeanings && c.getTypeOnlyAliasDeclaration(symbol) != nil {
    ///     break
    /// }
    /// ```
    ///
    /// `symbolIsValue` passes `includeTypeOnlyMembers: false`, hence
    /// `excludeTypeOnlyMeanings: true` — so a **type-only** alias does not
    /// contribute its target's value-ness, and `a.A` stays an error even though
    /// `A` is a class.
    ///
    /// # This is not optional, and two corpus cases said so
    ///
    /// Following the alias without this gate silently turned
    /// `conformance/exportNamespace3` and `conformance/importEquals2` from
    /// passing to failing — both write `export type { A }` and both expect the
    /// TS2339 a type-only re-export earns. The suite summary said only
    /// `1,898 → 1,896`; `examples/casequery.rs` named them.
    ///
    /// # The walk already existed
    ///
    /// [`Checker::type_only_alias_declaration`] is §121's port of the same
    /// question, and it follows the alias **chain** rather than reading one
    /// declaration — §120 measured 7 wrong lines for stopping at the first hop.
    /// A second, syntax-only copy was written here before that one was found;
    /// it would have been wrong in exactly the way §120 records.
    fn alias_is_type_only(&mut self, symbol: SymbolId) -> bool {
        self.type_only_alias_declaration(symbol).is_some()
    }
}

impl Checker<'_, '_> {
    /// `isReadonlySymbol` (`checker.go:13849`), reduced to the shapes this
    /// binder represents: a property whose declaration carries a `readonly`
    /// modifier, a get-accessor with no set-accessor, an enum member. The
    /// `CheckFlagsReadonly`, `const`-variable and `Object.defineProperty`
    /// arms have no counterpart here yet — each is a *missing rejection* for
    /// the one caller acting on the positive (`relater.rs`'s strict-subtype
    /// readonly rule, `checker-notes-assign.md` §14).
    pub(crate) fn is_readonly_property(&self, symbol: tsr_binder::SymbolId) -> bool {
        let data = self.binder.symbols().get(symbol);
        if data.flags.intersects(tsr_binder::SymbolFlags::ENUM_MEMBER) {
            return true;
        }
        if data.flags.intersects(tsr_binder::SymbolFlags::GET_ACCESSOR)
            && !data.flags.intersects(tsr_binder::SymbolFlags::SET_ACCESSOR)
        {
            return true;
        }
        data.declarations.iter().any(|&declaration| {
            let modifiers = match self.node_map.get(declaration) {
                Some(Node::PropertySignatureDeclaration(p)) => p.modifiers,
                Some(Node::PropertyDeclaration(p)) => p.modifiers,
                _ => return false,
            };
            modifiers.iter().any(|modifier| {
                matches!(
                    modifier,
                    tsr_ast::ModifierLike::Token(token)
                        if token.kind == tsr_ast::SyntaxKind::ReadonlyKeyword
                )
            })
        })
    }
}

impl Checker<'_, '_> {
    /// Whether a property's declaration carries the postfix `?` — the
    /// optionality test that reads syntax, because this binder never writes
    /// `SymbolFlags::OPTIONAL` (the `acdeed5` trap, which once collapsed
    /// optional-property else-branches to `never` through the flag).
    pub(crate) fn property_is_optional(&self, symbol: tsr_binder::SymbolId) -> bool {
        // The binder sets SymbolFlagsOptional only for a `?` postfix; the
        // definite-assignment `!` (`remainder!: string`) is required.
        let question = |token: Option<&tsr_ast::Token<'_>>| {
            token.is_some_and(|t| t.kind == tsr_ast::SyntaxKind::QuestionToken)
        };
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            match self.node_map.get(declaration) {
                Some(Node::PropertySignatureDeclaration(p)) => question(p.postfix_token),
                Some(Node::PropertyDeclaration(p)) => question(p.postfix_token),
                Some(Node::MethodSignatureDeclaration(m)) => question(m.postfix_token),
                Some(Node::MethodDeclaration(m)) => question(m.postfix_token),
                _ => false,
            }
        })
    }

    /// isObjectLiteralType (utilities.go): the semantic `ObjectLiteral` flag,
    /// retained by regularization and removed by widening.
    pub(crate) fn is_object_literal_type(&self, id: TypeId) -> bool {
        self.object_literal_spread_flags.contains_key(&id)
    }
}

impl Checker<'_, '_> {
    /// Whether a property's declaration carries the given modifier keyword —
    /// the reader behind the relation's privacy arms
    /// (`checker-notes-assign.md` §16), syntactic for the same reason
    /// [`Checker::property_is_optional`] is.
    pub(crate) fn property_has_modifier(
        &self,
        symbol: tsr_binder::SymbolId,
        kind: tsr_ast::SyntaxKind,
    ) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            let modifiers = match self.node_map.get(declaration) {
                Some(Node::PropertySignatureDeclaration(p)) => p.modifiers,
                Some(Node::PropertyDeclaration(p)) => p.modifiers,
                Some(Node::MethodSignatureDeclaration(m)) => m.modifiers,
                Some(Node::MethodDeclaration(m)) => m.modifiers,
                Some(Node::GetAccessorDeclaration(accessor)) => accessor.modifiers,
                Some(Node::SetAccessorDeclaration(accessor)) => accessor.modifiers,
                _ => return false,
            };
            modifiers.iter().any(|modifier| {
                matches!(
                    modifier,
                    tsr_ast::ModifierLike::Token(token) if token.kind == kind
                )
            })
        })
    }
}
