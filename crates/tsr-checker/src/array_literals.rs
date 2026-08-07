//! Array literal **expressions**: `[1, "a"]`.
//!
//! Ported from `Checker.checkArrayLiteral` (`checker.go:8021`), the
//! non-tuple tail of it (`checker.go:8089`–`:8100`).
//!
//! # This is two pieces of existing machinery meeting
//!
//! The element type is the **union of the elements' types**, and the result is
//! an **array type**, which is a reference to the global `Array`. Both already
//! exist, so this module computes almost nothing of its own: `[1, "a"]` is
//! `(string | number)[]` because [`crate::unions`] sorts the constituents by
//! `TypeFlags` and [`crate::declared`] prints an `Array` reference as `T[]` with
//! the element parenthesised. Upstream records exactly that line.
//!
//! # `[]` is `never[]`
//!
//! `checker.go:8098` — `implicitNeverType` under `strictNullChecks` and
//! `undefinedWideningType` without it. The corpus splits 461 `never[]` to 297
//! `undefined[]`, which is the same option this crate assumes **on** throughout
//! (see [`crate::unions`]). It is also the cheapest test that separates a real
//! implementation from one that special-cases the non-empty path, which is why
//! it is first in the test file.
//!
//! # The elements widen, and the declaration widens again
//!
//! `checkExpressionForMutableLocation` (`checker.go:8071`) is the same helper
//! object literals use, so `[1]` is `number[]` and not `1[]` — freshness stops
//! at the element boundary exactly as it stops at the property boundary. The
//! *second* widening, `getWidenedType` at the declaration, is still unported and
//! still belongs to `bd tsr-mli`.
//!
//! # What is gapped, and why each
//!
//! - **Tuples.** `checkArrayLiteral` produces one whenever the literal is in a
//!   destructuring pattern, a const context, or a tuple contextual type
//!   (`checker.go:8083`–`:8087`). All three are unported, so the tuple paths are
//!   unreachable rather than wrong — but a literal that upstream would make a
//!   tuple of is a line this port answers as an array. `bd tsr-cqi`.
//! - **Spread elements.** `[...xs]` needs `isArrayLikeType` and the iterated
//!   type (`checker.go:8036`). A spread makes the whole literal a gap.
//! - **Omitted elements.** `[1, , 2]` needs the optional-element flags model,
//!   which arrives with tuples.
//! - **Two object-typed elements**, because upstream reduces the element union
//!   with `UnionReductionSubtype` and this port has no assignability. See
//!   [`Checker::check_array_literal`].

use tsr_ast::{ArrayLiteralExpression, Expression};

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

impl Checker<'_, '_> {
    /// Ported from `Checker.checkArrayLiteral` (`checker.go:8021`).
    ///
    /// # The element union uses the wrong reduction, deliberately
    ///
    /// Upstream reduces with **`UnionReductionSubtype`** (`checker.go:8096`);
    /// this port has only `UnionReductionLiteral`, because `removeSubtypes`
    /// needs assignability. For every element type this slice can produce the
    /// two agree — widened primitives are mutually unrelated, and literal types
    /// only survive in a const context, which is unported.
    ///
    /// They part company on **object-typed elements**, and there the difference
    /// is not subtle: an object literal type is not interned, so `[{a: 1}, {a: 1}]`
    /// is a union of two *distinct* types that print the same string, which
    /// upstream's subtype reduction collapses to one. Printing
    /// `({ a: number; } | { a: number; })[]` would be a wrong line that looks
    /// like a formatting bug. So **two or more object-typed constituents make
    /// the literal a gap**, and one is fine: `[{a: 1}, 1]` is unaffected,
    /// because subtype reduction would not have merged those either.
    pub(crate) fn check_array_literal(&mut self, node: &ArrayLiteralExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        let mut elements = Vec::with_capacity(node.elements.len());
        for element in node.elements {
            // A spread needs the iterated type; an omission needs the tuple
            // element flags. Both make the whole literal a gap rather than an
            // array of what the other elements happen to be.
            //
            // **Unobservable today and load-bearing under one named edit**:
            // `check_expression` has no `SpreadElement` arm, so the guard below
            // catches these anyway and deleting this changes nothing. The moment
            // `SpreadElement` returns its operand's type, `[...[1]]` would answer
            // `number[][]` instead of a gap. That pair of mutations *was* applied
            // together and turns `a_spread_or_an_omitted_element_makes_the_literal_a_gap`
            // red, so the test is real rather than decorative.
            if matches!(element, Expression::SpreadElement(_) | Expression::OmittedExpression(_)) {
                return error;
            }
            let element_type = self.check_expression_for_mutable_location(*element);
            // A gap in an element is a gap in the array — the same call made for
            // union constituents, type arguments, object members and array
            // *type* elements.
            if element_type == error {
                return error;
            }
            elements.push(element_type);
        }

        let element_type = if elements.is_empty() {
            // `checker.go:8098`: `implicitNeverType` under `strictNullChecks`,
            // `undefinedWideningType` without it. The non-strict branch was
            // skipped while the option was assumed on crate-wide; the flag has
            // been per-case real since `set_strict_null_checks` and the corpus
            // records `undefined[]` on 384 lines — the 212-line `undefined[] ->
            // never[]` W2 row (`checker-notes-arrays.md`, ninth session).
            // Upstream's `implicitNeverType`/`undefinedWideningType` are
            // distinct types printing the same strings as `neverType`/
            // `undefinedType`; this port has only the latter pair — one more of
            // the "distinct types that print the same string", recorded rather
            // than merged silently.
            if self.strict_null_checks { self.intrinsics.never } else { self.intrinsics.undefined }
        } else {
            let reduced = self.get_union_type(&elements);
            if self.object_constituent_count(reduced) > 1 {
                // Upstream reduces the element union with
                // `UnionReductionSubtype` (`checker.go:8096`). In a position
                // WITH a contextual type the members kept their literals and
                // this port's widening already diverged upstream of here
                // (`checker-notes-assign.md` §10.1, `arrayBestCommonTypes`),
                // so only the provably-uncontextual position — an
                // un-annotated variable initialiser — takes the §9 reduction
                // (§13); everything else stays this arm's gap.
                let uncontextual = node.node_id.is_some_and(|id| self.has_no_contextual_type(id));
                if !uncontextual {
                    return error;
                }
                match self.union_with_subtype_reduction(&elements) {
                    Some(subtype_reduced)
                        if self.object_constituent_count(subtype_reduced) <= 1 =>
                    {
                        subtype_reduced
                    }
                    _ => return error,
                }
            } else {
                reduced
            }
        };

        let Some(target) = self.global_type_symbol("Array") else { return error };
        self.create_type_reference(target, vec![element_type])
    }

    /// How many of a type's union constituents are object types — or 1 for a
    /// bare object type, and 0 for anything else.
    ///
    /// The test for whether subtype reduction could have changed the answer.
    fn object_constituent_count(&self, id: TypeId) -> usize {
        let ty = self.store.get(id);
        if let crate::types::TypeData::Union { types, .. } = &ty.data {
            return types
                .iter()
                .filter(|&&constituent| {
                    self.store.get(constituent).flags.contains(TypeFlags::OBJECT)
                })
                .count();
        }
        usize::from(ty.flags.contains(TypeFlags::OBJECT))
    }
}
