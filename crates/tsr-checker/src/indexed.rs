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
    /// Upstream's `checkNonNullExpression` on the receiver, the widening for an
    /// assignment target, the `const` enum diagnostic and the `for…in` numeric
    /// special case are all absent: each either reports (`bd tsr-5e7.6`) or needs
    /// machinery this port does not have, and **none of them changes the type**
    /// for the shapes answered here.
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
        // §12.7's element-access half (the §52 scorecard's recorded residue):
        // a WRITE-position read takes the declared type — upstream's
        // assignment-target dispatch, which the identifier road has had
        // since §12.7 and this road lacked (`x['o'] = true`'s LHS narrowed
        // by a preceding guard, `controlFlowElementAccess`).
        if self.assignment_target_kind(id) == crate::expressions::AssignmentTargetKind::Definite {
            return self.propagate_optional_type_marker(computed, was_optional);
        }
        let narrowed = self.get_flow_type_of_reference(id, None, computed);
        // `checkElementAccessChain` wraps the whole access — flow narrowing
        // included — in `propagateOptionalTypeMarker` (`checker.go:8140`),
        // the same ordering as the property-access twin.
        self.propagate_optional_type_marker(narrowed, was_optional)
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
        let Some(name) = self.property_name_from_index(index_type) else {
            // Not a literal, so it names no property. `getIndexedAccessType`
            // falls to the index signatures (`checker.go:21902`).
            return self
                .get_applicable_index_info(object_type, index_type)
                .map_or(error, |info| info.value);
        };
        // Through [`Checker::get_type_of_property_of_type`] rather than
        // `get_property_of_type` + `get_type_of_symbol`, because the symbol
        // carries the *uninstantiated* declaration: `c["a"]` on a `C<number>`
        // whose member is declared `a: T` must answer `number`, and only the
        // seam can know that. It answers identically today (`bd tsr-4qx`).
        if let Some(property_type) = self.get_type_of_property_of_type(object_type, &name) {
            return property_type;
        }
        // A named lookup that misses still reaches the index signatures, which is
        // what makes `{ [k: string]: number }["anything"]` answer `number`.
        if let Some(info) = self.get_applicable_index_info(object_type, index_type) {
            return info.value;
        }
        self.array_or_tuple_element_access(object_type, index_type).unwrap_or(error)
    }

    /// The `Array<T>`/tuple half of `getIndexedAccessType`'s numeric road
    /// (`checker-notes-narrow.md` §28): a number-like index into `Array<T>`
    /// answers `T` (`| undefined` under `noUncheckedIndexedAccess`); into a
    /// tuple, the element union.
    fn array_or_tuple_element_access(
        &mut self,
        object_type: TypeId,
        index_type: TypeId,
    ) -> Option<TypeId> {
        // Plain `number` only: a literal index already answered through the
        // property-name road (in-range) or wants `undefined` (out of range —
        // `indexerWithTuple`, the §28 measurement's only movement).
        if index_type != self.intrinsics.number {
            return None;
        }
        let element = if let Some((elements, _)) = self.tuple_element_lists.get(&object_type) {
            let elements = elements.clone();
            self.get_union_type(&elements)
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
        if self.no_unchecked_indexed_access {
            let undefined = self.intrinsics.undefined;
            return Some(self.get_union_type(&[element, undefined]));
        }
        Some(element)
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
    fn property_name_from_index(&self, index: TypeId) -> Option<String> {
        match &self.store.get(index).data {
            TypeData::StringLiteral(value) => Some(value.clone()),
            TypeData::NumberLiteral(text) => Some(text.clone()),
            _ => None,
        }
    }
}
