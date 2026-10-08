//! Element access: `a["b"]`, `a[0]`, `a[k]`.
//!
//! Ported from `Checker.checkIndexedAccess` into `checkElementAccessExpression`
//! (`checker.go:8133`, `:8146`) and `getPropertyTypeForIndexType`
//! (`checker.go:21902`) — both the arm a literal index reaches and the fall-back
//! to the index signatures.
//!
//! # The whole slice is one observation from upstream
//!
//! `a["b"]` is not a second kind of lookup. Upstream derives a **property name
//! from the index's *type*** — `getPropertyNameFromIndex` (`checker.go:21786`)
//! into `getPropertyNameFromType` — and then calls the same `getPropertyOfType`
//! that property access calls. So an element access with a literal index *is* a
//! property access, and this module is that observation plus the cases where the
//! name cannot be derived.
//!
//! Taking the name from the index's **type** rather than from its syntax is
//! upstream's choice and it is worth more than it looks:
//!
//! ```text
//! const k = "b";
//! a[k]            // k's type is the literal "b", so this resolves
//! ```
//!
//! A syntactic reading would see an identifier and gap. Because a `const`
//! initialised with a string literal keeps its literal type (the freshness rule
//! this crate already implements), the type-directed reading answers it.
//!
//! # An index that names no property is not a gap
//!
//! `a[i]` for a `number` `i` names no property, and upstream falls to the index
//! signatures. So does this: both the no-name path and a *named* lookup that
//! misses reach [`Checker::get_applicable_index_info`], which is what makes
//! `{ [k: string]: number }["anything"]` answer `number`. The applicability rule
//! is asymmetric and lives in [`crate::index_signatures`]; what remains gapped
//! there — a `symbol` key, two applicable signatures, `noUncheckedIndexedAccess`
//! — is gapped here by propagation.
//!
//! # What is a gap
//!
//! - **An optional chain**, `a?.[b]`.
//! - **A `unique symbol` index**, the third arm of `getPropertyNameFromType`.
//! - **A name that is not a property of the receiver.** Upstream reports
//!   "Property 0 does not exist" and answers `errorType`; so does this, for the
//!   same reason property access does — including every receiver whose members
//!   this port cannot reach, such as an array or a tuple, which resolve through
//!   `lib.d.ts` (`bd tsr-9or.1`).

use tsr_ast::ElementAccessExpression;

use crate::{
    checker::Checker,
    types::{TypeData, TypeId},
};

impl Checker<'_, '_> {
    /// The type of an element access expression.
    ///
    /// Ported from `Checker.checkElementAccessExpression` (`checker.go:8146`).
    ///
    /// Const-enum accesses require string-literal-like syntax before index
    /// lookup; invalid syntax has the native error recovery type (`any`).
    /// The `for…in` numeric special case was ported at §477 and the readonly
    /// write answer at §473.
    pub fn check_element_access_expression(
        &mut self,
        node: &ElementAccessExpression<'_>,
    ) -> TypeId {
        let (computed, was_optional) = self.check_element_access_type(node);
        // The flow narrowing `checkIndexedAccess` ends with, the same call
        // `crate::members` makes for `a.b` — `bd tsr-6ka`. Split into a wrapper
        // rather than threaded through the six early returns below, because
        // every one of them is an answer that narrowing applies to and
        // repeating the call at each would be six chances to miss one.
        //
        // A gap is not narrowed: filtering `errorType` would answer `never` for
        // an access this port could not type.
        let error = self.intrinsics.error;
        if computed == error {
            return computed;
        }
        let Some(id) = node.node_id else { return computed };
        // §473: `isAssignmentToReadonlyEntity`'s element-access half — the
        // §27 arm the property-access twin has had since that landing: a
        // READONLY property as an assignment target answers upstream's
        // `errorType` (`checker.go:11377` reports and returns it), printed
        // `any` (`constDeclarations-access3/4/5` record `M["x"] : any` for
        // every write spelling against an exported `const`). The same
        // this-in-constructor carve-out as the twin, fields only.
        if self.assignment_target_kind(id) != crate::expressions::AssignmentTargetKind::None
            && let (Some(receiver), Some(index)) = (node.expression, node.argument_expression)
        {
            let object_type = self.check_expression(receiver);
            let index_type = self.check_expression(index);
            if self.tuple_is_readonly(object_type)
                && (self
                    .store
                    .get(index_type)
                    .flags
                    .intersects(crate::flags::TypeFlags::NUMBER_LIKE)
                    || self.property_name_from_index(index_type).is_some_and(|name| {
                        name == "length"
                            || name.parse::<usize>().is_ok_and(|index| index.to_string() == name)
                    }))
            {
                return self.intrinsics.any;
            }
            let readonly_target = self
                .property_name_from_index(index_type)
                .is_some_and(|name| self.is_readonly_property_of_type(object_type, &name));
            let constructor_field_write = matches!(
                receiver,
                tsr_ast::Expression::KeywordExpression(keyword)
                    if keyword.kind == tsr_ast::SyntaxKind::ThisKeyword
            ) && self.control_flow_container(id).is_some_and(
                |container| self.nodes.kind(container) == tsr_ast::SyntaxKind::Constructor,
            );
            if readonly_target && !constructor_field_write {
                return self.intrinsics.any;
            }
        }
        if self.assignment_target_kind(id) != crate::expressions::AssignmentTargetKind::None
            && let (Some(receiver), Some(index)) = (node.expression, node.argument_expression)
            && self.receiver_alias_is_namespace_import(receiver) == Some(true)
        {
            let source = self.check_expression(receiver);
            let key = self.check_expression(index);
            if self
                .property_name_from_index(key)
                .is_some_and(|name| self.get_property_of_type(source, &name).is_some())
            {
                return self.intrinsics.any;
            }
        }
        // `isThisPropertyAccessInConstructor` (`checker.go:27042`), the
        // element-access twin of the property-access arm: `this["x"]` inside
        // the declaring constructor reads `autoType` through its flow.
        if self.in_js_file(id)
            && let Some(constructor) = self.get_this_container(id, true)
            && self.nodes.kind(constructor) == tsr_ast::SyntaxKind::Constructor
            && let (Some(receiver), Some(index)) = (node.expression, node.argument_expression)
        {
            let object_type = self.check_expression(receiver);
            let index_type = self.check_expression(index);
            if let Some(property) = self
                .property_name_from_index(index_type)
                .and_then(|name| self.get_property_of_type(object_type, &name))
                && matches!(
                    self.is_constructor_declared_this_property(property),
                    crate::assignment_declarations::ThisAssignmentDeclaration::Constructor(declaring)
                        if declaring == constructor
                )
            {
                if self.assignment_target_kind(id)
                    == crate::expressions::AssignmentTargetKind::Definite
                {
                    return self.intrinsics.any;
                }
                let flowed = self.get_flow_type_of_property_symbol(id, None, property);
                return self.propagate_optional_type_marker_at(node.node_id, flowed, was_optional);
            }
        }
        // §12.7's element-access half (the §52 scorecard's recorded residue):
        // a WRITE-position read takes the declared type — upstream's
        // assignment-target dispatch, which the identifier road has had
        // since §12.7 and this road lacked (`x['o'] = true`'s LHS narrowed
        // by a preceding guard, `controlFlowElementAccess`).
        if self.assignment_target_kind(id) == crate::expressions::AssignmentTargetKind::Definite {
            // §617: the element-access half of §616 — `obj['x'] = v` sees the
            // SETTER's annotation for a divergent accessor pair, exactly as
            // `obj.x = v` does. The two roads carry the SAME two halves of
            // `getWriteTypeOfSymbol` and this one had only the
            // `exactOptionalPropertyTypes` one, which is the asymmetry §78.1
            // left behind (`divergentAccessorsTypes8` writes through
            // `obj['x']`).
            let computed = if let (Some(receiver), Some(index)) =
                (node.expression, node.argument_expression)
            {
                let object_type = self.check_expression(receiver);
                let index_type = self.check_expression(index);
                if let TypeData::Union { types, .. } = self.store.get(index_type).data.clone() {
                    // getIndexedAccessTypeOrUndefined (5b1047d checker.go:26993):
                    // writing intersects each key's write type. Declaration
                    // setters and ordinary indexed members share the original
                    // receiver; a failed constituent is not silently skipped.
                    // Query-local traversal adds no cache/publication state.
                    let mut writes = Vec::with_capacity(types.len());
                    for key in types {
                        let written = self
                            .property_name_from_index(key)
                            .and_then(|name| {
                                self.write_type_of_property_of_type(object_type, &name)
                            })
                            .or_else(|| self.resolved_indexed_access_type(object_type, key, false));
                        let Some(written) = written else { return self.intrinsics.error };
                        writes.push(written);
                    }
                    self.get_intersection_type(&writes, None)
                } else {
                    self.property_name_from_index(index_type)
                        .and_then(|name| self.write_type_of_property_of_type(object_type, &name))
                        .unwrap_or(computed)
                }
            } else {
                computed
            };
            // §78.1: the element-access half of `getWriteTypeOfSymbol` —
            // `obj['a'] = x` under `exactOptionalPropertyTypes` removes
            // `missingType`, as the property-access road does.
            let computed = if self.exact_optional_property_types {
                self.remove_missing_type(computed)
            } else {
                computed
            };
            return self.propagate_optional_type_marker_at(node.node_id, computed, was_optional);
        }
        let narrowed = self.get_flow_type_of_reference(id, None, computed);
        // `checkElementAccessChain` wraps the whole access — flow narrowing
        // included — in `propagateOptionalTypeMarker` (`checker.go:8140`),
        // the same ordering as the property-access twin.
        self.propagate_optional_type_marker_at(node.node_id, narrowed, was_optional)
    }

    /// The type `a[b]` computes before flow narrowing, and whether an
    /// optional chain stripped anything on the way (the marker
    /// `check_element_access_expression` propagates).
    fn check_element_access_type(&mut self, node: &ElementAccessExpression<'_>) -> (TypeId, bool) {
        let error = self.intrinsics.error;
        let (Some(receiver), Some(index)) = (node.expression, node.argument_expression) else {
            return (error, false);
        };
        let object_type = self.check_expression(receiver);
        // Upstream returns the object type when it is `errorType`
        // (`checker.go:8154`), which is the same answer by identity — an
        // unreachable receiver takes the access with it.
        if object_type == error {
            return (error, false);
        }
        // The nullable-receiver strip and the chain marker — the same trio a
        // property access runs (`checker-notes-nnaccess.md`): `?.` strips at
        // the root, an inner link removes the marker, the lookup runs on
        // `checkNonNullType`'s remainder.
        let non_optional = self.get_optional_expression_type(
            object_type,
            receiver.node_id(),
            node.question_dot_token.is_some(),
        );
        let stripped = self.check_non_null_type(non_optional);
        if stripped == error {
            return (error, false);
        }
        let was_optional = non_optional != object_type;
        let object_type = stripped;
        (self.element_access_lookup(node, object_type, index), was_optional)
    }

    /// # The JS-literal arm, transcribed but NOT built (needs an object flag)
    ///
    /// `getPropertyTypeForIndexType`'s failure path answers `anyType` — twice,
    /// at `checker.go:27130` and `:27189` — when `isJSLiteralType(objectType)`
    /// (`utilities.go:1753`):
    ///
    /// ```go
    /// if c.noImplicitAny { return false }          // meaningless in that mode
    /// if t.objectFlags&ObjectFlagsJSLiteral != 0 { return true }
    /// union: every constituent; intersection: some constituent
    /// ```
    ///
    /// ~~`ObjectFlagsJSLiteral` … so the arm cannot be written faithfully
    /// today. The build is three pieces …~~ **STALE — §185 BUILT IT** (same
    /// session, three hours later). The three pieces are in the tree: the
    /// `js_literal_types` side table on the checker (ADR-0003, not a
    /// `TypeData` widening), the mark at [`Checker::check_object_literal`]
    /// when `in_js_file`, and the consult below under `!no_implicit_any` —
    /// upstream's own first line in `isJSLiteralType`.
    ///
    /// **Kept struck rather than deleted, because the staleness is the
    /// lesson**: this said "cannot be written faithfully today" while the
    /// build it described was three edits long, and the careful scoping is
    /// what made it convincing. Same class as §194, where "I can't build
    /// this safely" survived until the one-grep probe the refusal itself
    /// named answered the whole question. **A refusal you wrote yourself is
    /// not evidence.**
    ///
    /// Head case: `compiler/jsNegativeElementAccessNotBound`, a `.js` file
    /// with `var indexMap = {}; indexMap[-1] = 0;` — upstream records
    /// `>indexMap[-1] : any` where this port answers `errorType`. Sized at
    /// one deficit-1 case there, and the same arm covers every failed element
    /// access on a JS object literal.
    ///
    /// The lookup half: the index type against the receiver's properties and
    /// index signatures.
    fn element_access_lookup(
        &mut self,
        node: &ElementAccessExpression<'_>,
        object_type: TypeId,
        index: tsr_ast::Expression<'_>,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let _ = node;
        let index_type = self.check_expression(index);
        // checkElementAccessExpression (checker.go): this syntactic rejection
        // precedes the enum's otherwise valid numeric reverse index.
        if let TypeData::Anonymous { symbol, .. } = self.type_of(object_type).data
            && self.binder.symbols().get(symbol).flags.contains(tsr_binder::SymbolFlags::CONST_ENUM)
            && !matches!(
                index,
                tsr_ast::Expression::StringLiteral(_)
                    | tsr_ast::Expression::NoSubstitutionTemplateLiteral(_)
            )
        {
            return self.intrinsics.any;
        }
        // §477: `isForInVariableForNumericPropertyNames` (`checker.go:8161`)
        // — an index that is the FOR-IN VARIABLE of a loop over an object
        // with only a NUMERIC index signature reads as `number`, though the
        // variable's own type is `string`: `for (let ix in iobj) iobj[ix]`
        // applies `{ [x: number]: any }`'s signature
        // (`capturedLetConstInLoop1`). The effective-index substitution is
        // upstream's own line; everything below sees `number`.
        let index_type = if self.is_for_in_variable_for_numeric_property_names(index) {
            self.intrinsics.number
        } else {
            index_type
        };
        // An `any` receiver makes the access `any`, whatever the index.
        //
        // This is the largest single cause in the element-access row: of 12,905
        // element-access lines in the baselines, 11,363 (88%) answer `any`, and
        // `conformance/anyPropertyAccess.types` is the canonical case. Upstream
        // reaches it because `getPropertyOfType` finds nothing on `any` and no
        // index info applies, and the property-access path states the same rule
        // outright (`isAnyLike`, `checker.go:11266`).
        //
        // **This is a computed answer, not a gap wearing `any`.** The rule that
        // forbids `anyType` is about forms this port could not compute; here
        // upstream's own answer is `any`, the same footing as `yield`. The
        // The identity test against `intrinsics.any` rather than a
        // `TypeFlags::ANY` test matters because `errorType` also carries `ANY`
        // here, and a flag test would turn every gap into a confident answer.
        //
        // **It is defence in depth and unobservable today**: the `error` guard
        // at the top of this function already returned, so swapping this for a
        // flag test leaves the tests green — confirmed by mutation, not assumed.
        // It stays because it is the only thing standing between a reordering of
        // that guard and a silent flood of wrong `any` answers, which is the
        // most dangerous way this row could produce a large number.
        if object_type == self.intrinsics.any {
            return self.intrinsics.any;
        }
        // §32: an element access through a minted unresolved receiver —
        // upstream's `errorType` — answers `any`, the same one hop as the
        // property twin, behind the same §31 structural gate.
        if self.unresolved_types.contains(&object_type)
            && node.node_id.is_some_and(|id| !self.file_has_import_machinery(id))
        {
            return self.intrinsics.any;
        }
        // §263+§264, landed together (the halves are not independently
        // measurable — either alone reads +0). `noUncheckedIndexedAccess`
        // includes `undefined` in an index-signature result, in EXPRESSION
        // position only.
        //
        // **§264's recorded predicate was BACKWARDS, and upstream's own text
        // settles it.** The record here said: exclude when the parent is a
        // compound assignment ("the LHS of a compound assignment reads the
        // DECLARED type"). Upstream says the opposite
        // (`checker.go:8164-8172`): `getAssignmentTargetKind` DEFINITE — `=`,
        // the logical assignments, a for-in/of target — gets `AccessFlagsWriting`
        // with NO `ExpressionPosition`; COMPOUND gets `Writing |
        // ExpressionPosition`; and `IncludeUndefined` is derived from
        // `ExpressionPosition` alone (`checker.go:26947`). A compound LHS is a
        // read-modify-write, so it DOES include `undefined`; only a definite
        // write is excluded. §263's four damaged lines were definite-write
        // lines present in both fixtures, not compound ones — the misread came
        // from attributing the damage to the fixture's NAME.
        //
        // The union site is `getPropertyTypeForIndexType` (`checker.go:27107`),
        // with one carve-out (`checker.go:27117`): indexing an enum's object
        // type with one of its OWN member literals — `E[E.A]` — stays exact,
        // no `undefined`. `enum_member_owners` is precisely that back-edge.
        let include_undefined = self.no_unchecked_indexed_access
            && node.node_id.is_none_or(|id| {
                self.assignment_target_kind(id)
                    != crate::expressions::AssignmentTargetKind::Definite
            });
        if self.variadic_tuple_elements.contains_key(&object_type)
            && let Some(t) = self.tuple_index_type(object_type, index_type, include_undefined)
        {
            return t;
        }
        // §381: a symbol-typed ENTITY index names a late-bound member — the
        // same chain-and-flags test as `late_bound_symbol_member_name`, so
        // the lookup key and the member's printed spelling cannot drift.
        // `i[Symbol.iterator]` reads the `[Symbol.iterator]` member before
        // any symbol index signature (`symbolProperty17`; and the baseline
        // answers by NAME even for a shadowed `Symbol`, `symbolProperty55` —
        // ADR-0006, the generated Go wins). A miss falls through to the
        // index-signature road unchanged.
        if self.type_of(index_type).flags.intersects(crate::flags::TypeFlags::ES_SYMBOL_LIKE) {
            fn chain_text(expression: &tsr_ast::Expression<'_>) -> Option<String> {
                match expression {
                    tsr_ast::Expression::Identifier(identifier) => {
                        Some(identifier.text.to_string())
                    }
                    tsr_ast::Expression::PropertyAccessExpression(access) => {
                        let base = chain_text(access.expression.as_ref()?)?;
                        let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                            return None;
                        };
                        Some(format!("{base}.{}", name.text))
                    }
                    _ => None,
                }
            }
            if let Some(chain) = chain_text(&index) {
                let name = format!("[{chain}]");
                if let Some(member) = self.get_type_of_property_of_type(object_type, &name) {
                    // getPropertyTypeForIndexType: a known symbol property,
                    // like a known string property, does not use index fallback.
                    return member;
                }
            }
        }
        let writing = node.node_id.is_some_and(|id| {
            self.assignment_target_kind(id) != crate::expressions::AssignmentTargetKind::None
        });
        // checkElementAccessExpression (checker.go:8169): a generic receiver
        // other than polymorphic `this` can read, but cannot write through a
        // non-numeric index signature of its constraint.
        let no_index_signatures = writing
            && self.indexed_access_object_is_generic(object_type)
            && !self
                .this_types
                .values()
                .chain(self.this_type_nodes.values())
                .any(|&ty| ty == object_type);
        // checkElementAccessExpression (`checker.go:8148`) widens the receiver
        // of an assignment target or a called method; a widened object literal
        // has lost ObjectFlagsObjectLiteral.
        let widened = writing || node.node_id.is_some_and(|id| self.is_method_access_for_call(id));
        // `getIndexedAccessTypeOrUndefined` (`checker.go:26975`): a union key
        // other than `boolean` reads each constituent through
        // `getPropertyTypeForIndexType`; one miss fails the whole access, and
        // the reads combine as a literal-reduced union, or an intersection in
        // a write position. Generic keys and objects defer first.
        if let TypeData::Union { types, .. } = self.store.get(index_type).data.clone()
            && !self.store.get(index_type).flags.contains(crate::flags::TypeFlags::BOOLEAN)
            && !self.indexed_access_index_is_generic(index_type)
            && (!self.variadic_tuple_elements.contains_key(&object_type)
                || !self.indexed_access_object_is_generic(object_type))
        {
            let mut values = Vec::with_capacity(types.len());
            for key in types {
                let value = self.element_access_for_index_type(
                    object_type,
                    key,
                    include_undefined,
                    widened,
                    no_index_signatures,
                );
                if value == error {
                    return error;
                }
                // formatUnionTypes compares enum members by their regular
                // types when collapsing the complete enum (printer.go).
                values.push(if self.enum_member_owners.contains_key(&value) {
                    self.get_regular_type_of_literal_type(value)
                } else {
                    value
                });
            }
            return if writing {
                self.get_intersection_type(&values, None)
            } else {
                self.get_union_type(&values)
            };
        }
        self.element_access_for_index_type(
            object_type,
            index_type,
            include_undefined,
            widened,
            no_index_signatures,
        )
    }

    /// `isMethodAccessForCall` (`checker.go:11466`): the access, through any
    /// parentheses, is the callee of a call or `new` expression.
    fn is_method_access_for_call(&self, mut node: tsr_ast::NodeId) -> bool {
        while let Some(parent) = self.nodes.parent(node)
            && self.nodes.kind(parent) == tsr_ast::SyntaxKind::ParenthesizedExpression
        {
            node = parent;
        }
        let Some(parent) = self.nodes.parent(node) else { return false };
        match self.node_map.get(parent) {
            Some(tsr_ast::Node::CallExpression(call)) => {
                call.expression.and_then(|e| e.node_id()) == Some(node)
            }
            Some(tsr_ast::Node::NewExpression(new)) => {
                new.expression.and_then(|e| e.node_id()) == Some(node)
            }
            _ => false,
        }
    }

    /// `getPropertyTypeForIndexType` (`checker.go:27002`) for one
    /// non-union key of an element access expression.
    fn element_access_for_index_type(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        include_undefined: bool,
        widened: bool,
        no_index_signatures: bool,
    ) -> TypeId {
        let error = self.intrinsics.error;
        let Some(name) = self.property_name_from_index(index_type) else {
            // Higher-order accesses defer before applicable index signatures
            // are considered (getIndexedAccessTypeOrUndefined). A constraint
            // can make a generic key assignable to an index signature without
            // making that key concrete.
            if let Some(deferred) =
                self.deferred_indexed_access(object_type, index_type, include_undefined)
            {
                return deferred;
            }
            // getReducedApparentType follows the receiver's base constraint
            // after deferral. Do not use that constraint to admit a generic
            // key rejected by the expression-side validation above.
            let apparent = if self.indexed_access_index_is_generic(index_type) {
                object_type
            } else {
                self.apparent_type(object_type)
            };
            // Not a literal, so it names no property. `getIndexedAccessType`
            // falls to the index signatures (`checker.go:21902`).
            if let Some(info) = self.get_applicable_index_info(apparent, index_type) {
                if no_index_signatures && info.key != self.intrinsics.number {
                    return error;
                }
                return self.include_unchecked_undefined(
                    info.value,
                    include_undefined,
                    object_type,
                    index_type,
                );
            }
            // getPropertyTypeForIndexType keeps applicable index signatures
            // ahead of the never-index fallback (checker.go:27126).
            if index_type == self.intrinsics.never {
                return self.intrinsics.never;
            }
            // §921: the `Array<T>`/tuple numeric road, reached from the branch
            // that actually needs it.
            //
            // `array_or_tuple_element_access` sits at the TAIL of this function,
            // after `let Some(name) = … else { … }` — so it is reachable only
            // when the index NAMES A PROPERTY. A plain `number` index names
            // none, which is precisely the case the road exists for: `a[i]` on
            // `string[]` with `i: number` returned `error` here and never got
            // there. Probed rather than reasoned — the road's own entry never
            // fired for that shape.
            //
            // `a[0]` worked and hid it: a numeric LITERAL does name a property,
            // takes the road above, and answers through the members table.
            if let Some(found) =
                self.array_or_tuple_element_access(object_type, index_type, include_undefined)
            {
                return found;
            }
            return self
                .object_literal_index_fallback(object_type, index_type, widened)
                .unwrap_or(error);
        };
        // Through [`Checker::get_type_of_property_of_type`] rather than
        // `get_property_of_type` + `get_type_of_symbol`, because the symbol
        // carries the *uninstantiated* declaration: `c["a"]` on a `C<number>`
        // whose member is declared `a: T` must answer `number`, and only the
        // seam can know that. It answers identically today (`bd tsr-4qx`).
        // §471: a STRING key never reaches a PRIVATE-IDENTIFIER member —
        // upstream files privates under a per-class mangled name
        // (`binder.GetSymbolNameForPrivateIdentifier`), so `this["#foo"]`
        // cannot collide with `#foo` and falls to the index signature
        // (`privateNameAndIndexSignature` records `this["#foo"] : any` from
        // `[k: string]: any` beside a declared `#foo`). A COMPUTED property
        // written `["#bar"]` is an ordinary string-named member and still
        // matches — the test is the declaration's name kind, not the
        // spelling.
        let names_a_private_member = name.starts_with('#')
            && self.get_property_of_type(object_type, &name).is_some_and(|property| {
                self.binder
                    .symbols()
                    .get(property)
                    .value_declaration
                    .is_some_and(|declaration| self.declaration_names_a_private(declaration))
            });
        // §858: through the APPARENT type. `access_member_lookup` reaches a
        // primitive's members via `get_apparent_type` — that is what makes
        // `x.doStuff` work on a `number` when `interface Number` declares it —
        // and this road passed `object_type` straight through, so the dotted
        // form resolved and `x['doStuff']` gapped
        // (`extendNumberInterface` and its `Boolean`/`String` siblings, 4 gaps
        // and zero wrong lines each).
        //
        // Additive rather than a redirection: this port's `apparent_type` is
        // the primitive arms only (`crate::members` says so), so every
        // non-primitive receiver answers exactly as before.
        let apparent = self.apparent_type(object_type);
        if let Ok(index) = name.parse::<usize>()
            && index.to_string() == name
            && let Some(element) =
                self.variadic_tuple_element_type(object_type, index, include_undefined)
        {
            return element;
        }
        if !names_a_private_member
            && let Some(property_type) = self.get_type_of_property_of_type(apparent, &name)
        {
            return property_type;
        }
        // A named lookup that misses still reaches the index signatures, which is
        // what makes `{ [k: string]: number }["anything"]` answer `number`.
        if let Some(info) = self.get_applicable_index_info(object_type, index_type) {
            if no_index_signatures && info.key != self.intrinsics.number {
                return error;
            }
            return self.include_unchecked_undefined(
                info.value,
                include_undefined,
                object_type,
                index_type,
            );
        }
        // §177 (`checker-notes-narrow.md`): the index signatures of the
        // APPARENT type. `getIndexedAccessType` reads them off
        // `getApparentType(objectType)` — a primitive carries none of its
        // own, and `""[0]` is `string` through `String`'s
        // `[index: number]: string` (`stringHasStringValuedNumericIndexer`).
        // The named lookup above already goes through the members road; this
        // is its index-signature twin.
        let apparent = self.apparent_type(object_type);
        if apparent != object_type
            && let Some(info) = self.get_applicable_index_info(apparent, index_type)
        {
            if no_index_signatures && info.key != self.intrinsics.number {
                return error;
            }
            return self.include_unchecked_undefined(
                info.value,
                include_undefined,
                object_type,
                index_type,
            );
        }
        if let Some(found) =
            self.array_or_tuple_element_access(object_type, index_type, include_undefined)
        {
            return found;
        }
        // SS185: `isJSLiteralType` (`utilities.go:1753`) — the failure path
        // of `getPropertyTypeForIndexType` answers `anyType` for an object
        // literal minted in a JS FILE (`checker.go:27130`, `:27189`). The
        // flag is meaningless under `noImplicitAny`, which is upstream's own
        // first line. Head case: `var indexMap = {}; indexMap[-1] = 0` in a
        // `.js` file records `>indexMap[-1] : any`.
        if !self.no_implicit_any && self.js_literal_types.contains(&object_type) {
            return self.intrinsics.any;
        }
        self.object_literal_index_fallback(object_type, index_type, widened).unwrap_or(error)
    }

    /// `getPropertyTypeForIndexType`'s object-literal arm (`checker.go:27134`),
    /// reached by an element access expression whose key found neither a
    /// property nor an index signature: under `noImplicitAny` a literal key
    /// reads `undefined`; a `string` or `number` key reads the union of every
    /// property's type with `undefined`. Other keys keep the miss.
    fn object_literal_index_fallback(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        widened: bool,
    ) -> Option<TypeId> {
        use crate::flags::TypeFlags;
        let apparent = self.apparent_type(object_type);
        if widened || !self.is_object_literal_type(apparent) {
            return None;
        }
        let flags = self.store.get(index_type).flags;
        if self.no_implicit_any
            && flags.intersects(TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL)
        {
            return Some(self.intrinsics.undefined);
        }
        if !flags.intersects(TypeFlags::STRING | TypeFlags::NUMBER) {
            return None;
        }
        let properties = self.anonymous_properties.get(&apparent)?.0.clone();
        let mut types: Vec<TypeId> =
            properties.iter().map(|property| self.property_type(property)).collect();
        types.push(self.intrinsics.undefined);
        Some(self.get_union_type(&types))
    }

    /// `getIndexedAccessTypeOrUndefined` (checker.go), without an expression
    /// node. Generic operands remain captured; concrete keys project members,
    /// index signatures, and tuple/array elements. Union keys require every
    /// member lookup to succeed before their read types are unioned.
    pub(crate) fn resolved_indexed_access_type(
        &mut self,
        object: TypeId,
        mut index: TypeId,
        include_undefined: bool,
    ) -> Option<TypeId> {
        use crate::flags::TypeFlags;
        if object == self.intrinsics.error || index == self.intrinsics.error {
            return None;
        }
        // getIndexedAccessTypeOrUndefined normalizes generic string/number
        // keys when every object constituent has only a string index signature.
        if !self.store.get(index).flags.intersects(TypeFlags::NULLABLE)
            && self.is_string_index_signature_only_type(object)
        {
            // The source-variable relation explores its base constraint. Keep
            // the two kind checks separate, as isTypeAssignableToKind does.
            let key = self.base_constraint_or_type(index);
            if self.is_type_assignable_to(key, self.intrinsics.number)
                || self.is_type_assignable_to(key, self.intrinsics.string)
            {
                index = self.intrinsics.string;
            }
        }
        if let Some(t) = self.tuple_index_type(object, index, include_undefined) {
            return Some(t);
        }
        let index_generic = self.indexed_access_index_is_generic(index);
        if self.indexed_access_object_is_generic(object) || index_generic {
            if self.store.get(object).flags.intersects(TypeFlags::ANY | TypeFlags::UNKNOWN) {
                return Some(object);
            }
            let key = (object, index, include_undefined);
            if let Some(&cached) = self.deferred_indexed_access_cache.get(&key) {
                return Some(cached);
            }
            let object_text = self.wrap_array_element_text(object, &self.type_to_string(object));
            let text = format!("{object_text}[{}]", self.type_to_string(index));
            let id = self.store.new_named(TypeFlags::INDEXED_ACCESS, text, None);
            self.deferred_indexed_access_types.insert(id, key);
            self.deferred_indexed_access_cache.insert(key, id);
            self.deferred_index_mints.insert(id);
            return Some(id);
        }
        if object == self.intrinsics.any {
            return Some(object);
        }
        if let crate::types::TypeData::Union { types, .. } = &self.store.get(index).data {
            let types = types.clone();
            let mut values = Vec::with_capacity(types.len());
            for index in types {
                let value = self.resolved_indexed_access_type(object, index, include_undefined)?;
                // formatUnionTypes compares enum members by their regular
                // types when collapsing the complete enum (printer.go).
                values.push(if self.enum_member_owners.contains_key(&value) {
                    self.get_regular_type_of_literal_type(value)
                } else {
                    value
                });
            }
            return Some(self.get_union_type(&values));
        }
        if let Some(name) = self.property_name_from_index(index) {
            let apparent = self.apparent_type(object);
            if let Some(value) = self.get_type_of_property_of_type(apparent, &name) {
                return Some(value);
            }
        }
        let apparent = self.apparent_type(object);
        let info = self.get_applicable_index_info(apparent, index).or_else(|| {
            // getPropertyTypeForIndexType (checker.go:27085): a string
            // signature is the fallback for every non-null property key,
            // including symbols. This matters when a generic indexed type's
            // base constraint projects keyof T to string | number | symbol.
            if self.store.get(index).flags.intersects(TypeFlags::NULLABLE) {
                return None;
            }
            let keys = self.get_union_type(&[
                self.intrinsics.string,
                self.intrinsics.number,
                self.intrinsics.es_symbol,
            ]);
            if !self.is_type_assignable_to(index, keys) {
                return None;
            }
            self.get_index_infos_of_type(apparent)?
                .into_iter()
                .find(|info| info.key == self.intrinsics.string)
        });
        if let Some(info) = info {
            return Some(self.include_unchecked_undefined(
                info.value,
                include_undefined,
                object,
                index,
            ));
        }
        // getPropertyTypeForIndexType retains an any index when no property
        // or index signature supplied a type (checker.go). Unresolved alias
        // placeholders may carry ANY in this port but are not computed any.
        if self.store.get(index).flags.contains(TypeFlags::ANY)
            && !self.unresolved_types.contains(&index)
        {
            return Some(index);
        }
        (index == self.intrinsics.never).then_some(self.intrinsics.never)
    }

    /// isStringIndexSignatureOnlyTypeWorker (checker.go:27363).
    fn is_string_index_signature_only_type(&mut self, object: TypeId) -> bool {
        if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.store.get(object).data.clone()
        {
            return types.into_iter().all(|ty| self.is_string_index_signature_only_type(ty));
        }
        self.store.get(object).flags.contains(crate::flags::TypeFlags::OBJECT)
            && !self.indexed_access_object_is_generic(object)
            && self.property_names_of(object).is_empty()
            && self
                .get_index_infos_of_type(object)
                .is_some_and(|infos| infos.len() == 1 && infos[0].key == self.intrinsics.string)
    }

    pub(crate) fn indexed_access_index_is_generic(&self, index: TypeId) -> bool {
        use crate::flags::TypeFlags;
        if self
            .store
            .get(index)
            .flags
            .intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE | TypeFlags::INDEX)
            || self.deferred_keyof_types.contains(&index)
            || self.deferred_indexed_access_types.contains_key(&index)
            || (self
                .store
                .get(index)
                .flags
                .intersects(TypeFlags::TEMPLATE_LITERAL | TypeFlags::STRING_MAPPING)
                && !self.is_pattern_template(index))
        {
            return true;
        }
        match &self.store.get(index).data {
            crate::types::TypeData::Union { types, .. }
            | crate::types::TypeData::Intersection { types, .. } => {
                types.iter().any(|&t| self.indexed_access_index_is_generic(t))
            }
            _ => false,
        }
    }

    /// getGenericObjectFlags/isGenericMappedType (checker.go:24880).
    /// Generic object references such as Box<T> are still concrete objects;
    /// only instantiable types, generic mapped types, and generic tuples defer
    /// member selection. Numeric tuple accesses are handled before this query.
    fn indexed_access_object_is_generic(&mut self, object: TypeId) -> bool {
        use crate::flags::TypeFlags;
        if self.store.get(object).flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE) {
            return true;
        }
        match &self.store.get(object).data {
            TypeData::Union { types, .. } | TypeData::Intersection { types, .. } => {
                let types = types.clone();
                return types.into_iter().any(|ty| self.indexed_access_object_is_generic(ty));
            }
            _ => {}
        }
        if let Some(info) = self.mapped_types.get(&object).cloned() {
            if self.indexed_access_index_is_generic(info.constraint) {
                return true;
            }
            if let Some(name) = info.name_type {
                let name = self.instantiate_type(
                    name,
                    &[(info.parameter, info.constraint)],
                    &[info.parameter],
                    &[],
                );
                if self.indexed_access_index_is_generic(name) {
                    return true;
                }
            }
        }
        self.variadic_tuple_elements.get(&object).cloned().is_some_and(|(elements, _)| {
            elements.iter().any(|element| {
                element.spread && self.tuple_spread_array_element(element.r#type).is_none()
            })
        })
    }

    /// Mint the deferred `Object[Index]` of a generic indexed access, or
    /// `None` when the access is concrete. §786.
    ///
    /// This expression-side path first checks whether a generic key belongs
    /// to the receiver. Annotation-side resolution can defer generic objects
    /// even with concrete keys; expressions preserve native eager constraint
    /// lookup for that case.
    ///
    /// An `errorType` on either side refuses: a deferred print built over a
    /// type this port failed to compute would be a confident answer wearing
    /// the shape of a faithful one.
    fn deferred_indexed_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        include_undefined: bool,
    ) -> Option<TypeId> {
        let error = self.intrinsics.error;
        if object_type == error || index_type == error {
            return None;
        }
        // Native 5b1047d checkIndexedAccessIndexType admits a generic key by
        // relating it to the concrete receiver's index type. This includes
        // keyof T on U extends T and on an unremapped mapped image of T;
        // equality of keyof operands is not the admission contract. The
        // relation preserves receiver/mapper identity and cannot admit an
        // unsupported proof merely because the rendered keys coincide.
        if !self.indexed_access_index_is_generic(index_type)
            || !self
                .resolved_keyof_type(object_type)
                .is_some_and(|keys| self.is_type_assignable_to(index_type, keys))
        {
            return None;
        }
        self.resolved_indexed_access_type(object_type, index_type, include_undefined)
    }

    /// `isForInVariableForNumericPropertyNames` (`checker.go:8179`): the
    /// index is an identifier (parens skipped) naming a VARIABLE that is the
    /// for-in variable of an enclosing loop — reached by walking up through
    /// STATEMENT positions only, exactly upstream's `child ==
    /// node.Statement` test, so the head's own expression never matches
    /// itself — whose iterated object's type has exactly one index info and
    /// it is numeric (`hasNumericPropertyNames`, `:8216`).
    fn is_for_in_variable_for_numeric_property_names(
        &mut self,
        index: tsr_ast::Expression<'_>,
    ) -> bool {
        let mut skipped = index;
        while let tsr_ast::Expression::ParenthesizedExpression(parenthesized) = skipped {
            let Some(inner) = parenthesized.expression else { return false };
            skipped = inner;
        }
        let tsr_ast::Expression::Identifier(name) = skipped else { return false };
        let Some(reference) = name.node_id else { return false };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            reference,
            name.text,
            tsr_binder::SymbolFlags::VALUE,
        ) else {
            return false;
        };
        if !self.binder.symbols().get(symbol).flags.intersects(tsr_binder::SymbolFlags::VARIABLE) {
            return false;
        }
        let Some(start) = index.node_id() else { return false };
        let mut child = start;
        let mut current = self.nodes.parent(start);
        while let Some(ancestor) = current {
            if self.nodes.kind(ancestor) == tsr_ast::SyntaxKind::ForInStatement
                && let Some(tsr_ast::Node::ForInOrOfStatement(statement)) =
                    self.node_map.get(ancestor)
                && statement.statement.and_then(|s| tsr_ast::Node::from(s).node_id()) == Some(child)
                && self.for_in_variable_symbol(statement) == Some(symbol)
                && let Some(iterated) = statement.expression
            {
                let iterated_type = self.check_expression(iterated);
                if let Some(infos) = self.get_index_infos_of_type(iterated_type)
                    && let [info] = infos.as_slice()
                    && info.key == self.intrinsics.number
                {
                    return true;
                }
            }
            child = ancestor;
            current = self.nodes.parent(ancestor);
        }
        false
    }

    /// `getForInVariableSymbol` (`checker.go:8200`): the first declaration of
    /// a declaration-list head (a non-pattern name), or the referenced
    /// symbol of a bare identifier head.
    fn for_in_variable_symbol(
        &mut self,
        statement: &tsr_ast::ForInOrOfStatement<'_>,
    ) -> Option<tsr_binder::SymbolId> {
        let initializer = statement.initializer?;
        if let Some(list) = initializer.node_id()
            && let Some(tsr_ast::Node::VariableDeclarationList(declarations)) =
                self.node_map.get(list)
        {
            let first = declarations.declarations.first()?;
            if matches!(first.name, Some(tsr_ast::BindingName::BindingPattern(_))) {
                return None;
            }
            return self.binder.symbol_of(first.node_id?);
        }
        if let tsr_ast::ForInitializer::Identifier(name) = initializer {
            return self.binder.resolve_name(
                self.nodes,
                self.node_map,
                name.node_id?,
                name.text,
                tsr_binder::SymbolFlags::VALUE,
            );
        }
        None
    }

    /// The `Array<T>`/tuple half of `getIndexedAccessType`'s numeric road
    /// (`checker-notes-narrow.md` §28): a number-like index into `Array<T>`
    /// answers `T` (`| undefined` under `noUncheckedIndexedAccess`); into a
    /// tuple, the element union.
    pub(crate) fn array_or_tuple_element_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
        include_undefined: bool,
    ) -> Option<TypeId> {
        // §921: plain `number`, **or a number LITERAL into an `Array<T>`**.
        //
        // §28's gate was `index_type != number → None`, justified by *"a literal
        // index already answered through the property-name road (in-range)"*.
        // True of a TUPLE — position `0` is a real member — and false of
        // `Array<T>`, which has no property named `"0"`. Both halves were
        // broken and each hid the other: the literal form died on this gate,
        // and the `number` form never reached this function at all (the call
        // site sat past an earlier `return error`).
        //
        // Tuples keep the exclusion: their literal indices are positional, the
        // property road answers them, and the out-of-range `undefined` §28
        // measured (`indexerWithTuple`) belongs to that road.
        let numeric_literal =
            self.store.get(index_type).flags.contains(crate::flags::TypeFlags::NUMBER_LITERAL);
        let is_tuple = self.tuple_element_lists.contains_key(&object_type)
            || self.variadic_tuple_elements.contains_key(&object_type);
        let admits = index_type == self.intrinsics.number || (numeric_literal && !is_tuple);
        if !admits {
            return None;
        }
        let element = if let Some((elements, _)) = self.tuple_element_lists.get(&object_type) {
            let mut elements = elements.clone();
            // Optional tuple elements contribute undefined to the numeric
            // index signature even without noUncheckedIndexedAccess.
            if self.tuple_optional_masks.get(&object_type).is_some_and(|mask| mask.contains(&true))
            {
                elements.push(self.intrinsics.undefined);
            }
            self.get_union_type(&elements)
        } else if let Some(element) = self.variadic_tuple_index_union(object_type) {
            element
        } else {
            let (target, arguments) = self.type_reference_targets.get(&object_type)?.clone();
            if arguments.len() != 1 {
                return None;
            }
            let array = self.global_type_symbol("Array")?;
            let readonly_array = self.global_type_symbol("ReadonlyArray");
            let merged = self.binder.merged_symbol(target);
            if merged != self.binder.merged_symbol(array)
                && readonly_array.map(|s| self.binder.merged_symbol(s)) != Some(merged)
            {
                return None;
            }
            arguments[0]
        };
        // §264: the caller's write-position classification, not the raw flag —
        // `arr[0] = x` keeps the exact element type where `arr[0]` reads
        // `T | undefined` (`checker.go:26947`).
        if include_undefined {
            let undefined = self.intrinsics.undefined;
            return Some(self.get_union_type(&[element, undefined]));
        }
        Some(element)
    }

    /// The `IncludeUndefined` union of `getPropertyTypeForIndexType`
    /// (`checker.go:27107`): an index-signature result in expression read
    /// position under `noUncheckedIndexedAccess` carries `undefined`, EXCEPT
    /// when an enum's object type is indexed with one of its own member
    /// literals (`checker.go:27117`) — `E[E.A]` cannot miss, so it stays
    /// exact. Upstream tests `indexType.symbol`'s parent against
    /// `objectType.symbol`; this port's equivalent back-edge is
    /// `enum_member_owners`, written at the one place a member type is minted.
    pub(crate) fn include_unchecked_undefined(
        &mut self,
        value: TypeId,
        include_undefined: bool,
        object_type: TypeId,
        index_type: TypeId,
    ) -> TypeId {
        if !include_undefined {
            return value;
        }
        if let TypeData::Anonymous { symbol, .. } = self.store.get(object_type).data
            && self.binder.symbols().get(symbol).flags.intersects(
                tsr_binder::SymbolFlags::REGULAR_ENUM | tsr_binder::SymbolFlags::CONST_ENUM,
            )
            && self.enum_member_owners.get(&index_type) == Some(&symbol)
        {
            return value;
        }
        let undefined = self.intrinsics.undefined;
        self.get_union_type(&[value, undefined])
    }

    /// The property name an index type names, if it names one.
    ///
    /// Ported from `getPropertyNameFromIndex` into `getPropertyNameFromType`
    /// (`checker.go:21786`), restricted to the two literal arms. Upstream's
    /// fallback — reading the name off the *node* when the type is unusable — is
    /// for index signatures in type position and is not reachable from an element
    /// access expression.
    ///
    /// The numeric arm takes the literal's **normalised** text, which is the same
    /// string `Number::toString` gives upstream: `a[1.0]` and `a[1]` name the same
    /// property `1`, and the payload is already normalised for exactly this
    /// reason (see [`crate::printing::normalise_number`]).
    pub(crate) fn property_name_from_index(&self, index: TypeId) -> Option<String> {
        match &self.store.get(index).data {
            TypeData::StringLiteral(value)
            | TypeData::NumberLiteral(value)
            | TypeData::EnumLiteral {
                value:
                    crate::types::EnumLiteralValue::String(value)
                    | crate::types::EnumLiteralValue::Number(value),
                ..
            } => Some(value.clone()),
            _ => None,
        }
    }
}
