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
        // §6.3 (`checker-notes-arrays.md`): a literal with a TUPLE spread
        // mints a tuple — plain elements widened in place, tuple spreads
        // spliced. Any non-tuple spread, omission, or gap declines whole.
        let has_tuple_spread = node.elements.iter().any(|element| {
            if let Expression::SpreadElement(spread) = element
                && let Some(operand) = spread.expression
            {
                let operand_type = self.check_expression(operand);
                return self.tuple_element_lists.contains_key(&operand_type);
            }
            false
        });
        if has_tuple_spread {
            // §6.3's narrowing, twice-fired: the context DECIDES the shape.
            // A TUPLE context (annotated initializer or assignment target
            // typing as a tuple) mints the spliced tuple; an UNANNOTATED
            // variable initializer widens — upstream answers the
            // element-union array there (`arrayLiteralExpressionContextualTyping`'s
            // `var spr1 = [1, 2, 3, ...tup]` wants `number[]`); every other
            // context declines whole.
            #[derive(PartialEq)]
            enum Context {
                Tuple,
                Widening,
                Decline,
            }
            let context = 'context: {
                let Some(id) = node.node_id else { break 'context Context::Decline };
                let Some(parent) = self.nodes.parent(id) else { break 'context Context::Decline };
                match self.node_map.get(parent) {
                    Some(tsr_ast::Node::VariableDeclaration(declaration))
                        if declaration.initializer.and_then(|i| i.node_id()) == Some(id) =>
                    {
                        match declaration.r#type {
                            None => Context::Widening,
                            Some(annotation) => {
                                let t = self.get_type_from_type_node(annotation);
                                if self.tuple_element_lists.contains_key(&t) {
                                    Context::Tuple
                                } else {
                                    Context::Decline
                                }
                            }
                        }
                    }
                    Some(tsr_ast::Node::BinaryExpression(binary))
                        if binary
                            .operator_token
                            .is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
                            && binary.right.and_then(|r| r.node_id()) == Some(id) =>
                    {
                        // The target's DECLARED type, not the flowed one — an
                        // assignment read through the flow walk can answer a
                        // narrowed form the tuple table does not hold.
                        let target = match binary.left {
                            Some(Expression::Identifier(identifier)) => identifier
                                .node_id
                                .and_then(|left_id| {
                                    self.binder.resolve_name(
                                        self.nodes,
                                        self.node_map,
                                        left_id,
                                        identifier.text,
                                        tsr_binder::SymbolFlags::VALUE,
                                    )
                                })
                                .map(|symbol| self.get_type_of_symbol(symbol)),
                            other => other.map(|left| self.check_expression(left)),
                        };
                        match target {
                            Some(t) if self.tuple_element_lists.contains_key(&t) => Context::Tuple,
                            _ => Context::Decline,
                        }
                    }
                    _ => Context::Decline,
                }
            };
            if context == Context::Decline {
                return error;
            }
            if context == Context::Widening {
                // The §6.2 union contribution, alive exactly here: tuple
                // spreads contribute their element union and the literal
                // widens to an array like any other.
                let mut union_elements = Vec::with_capacity(node.elements.len());
                for element in node.elements {
                    match element {
                        Expression::SpreadElement(spread) => {
                            let Some(operand) = spread.expression else { return error };
                            let operand_type = self.check_expression(operand);
                            if let Some((elements, _)) = self.tuple_element_lists.get(&operand_type)
                            {
                                let elements = elements.clone();
                                if elements.is_empty() {
                                    return error;
                                }
                                let union = self.get_union_type(&elements);
                                union_elements.push(self.get_widened_literal_type(union));
                            } else if let Some(element_type) =
                                self.array_spread_element_type(operand_type)
                            {
                                union_elements.push(element_type);
                            } else {
                                return error;
                            }
                        }
                        Expression::OmittedExpression(_) => return error,
                        _ => {
                            let element_type = self.check_expression_for_mutable_location(*element);
                            if element_type == error {
                                return error;
                            }
                            union_elements.push(self.get_widened_literal_type(element_type));
                        }
                    }
                }
                let element_type = self.get_union_type(&union_elements);
                let Some(target) = self.global_type_symbol("Array") else { return error };
                return self.create_type_reference(target, vec![element_type]);
            }
            let mut spliced = Vec::with_capacity(node.elements.len());
            for element in node.elements {
                match element {
                    Expression::SpreadElement(spread) => {
                        let Some(operand) = spread.expression else { return error };
                        let operand_type = self.check_expression(operand);
                        let Some((elements, _)) = self.tuple_element_lists.get(&operand_type)
                        else {
                            return error;
                        };
                        spliced.extend(elements.iter().copied());
                    }
                    Expression::OmittedExpression(_) => return error,
                    _ => {
                        let element_type = self.check_expression_for_mutable_location(*element);
                        if element_type == error {
                            return error;
                        }
                        spliced.push(self.get_widened_literal_type(element_type));
                    }
                }
            }
            return self.create_tuple_type(spliced, false);
        }
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
            if let Expression::SpreadElement(spread) = element {
                // `getSpreadElementType`'s array half
                // (`checker-notes-arrays.md` §6): `...xs` over `Array<T>`
                // contributes `T`. Every other spread shape declines whole.
                let Some(operand) = spread.expression else { return error };
                let operand_type = self.check_expression(operand);
                let Some(element_type) = self.array_spread_element_type(operand_type) else {
                    return error;
                };
                elements.push(element_type);
                continue;
            }
            if matches!(element, Expression::OmittedExpression(_)) {
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
                // §7 (`checker-notes-arrays.md`): an element union with no
                // FRESHABLE literal constituent is context-independent —
                // `isLiteralOfContextualType` has nothing to preserve — so
                // the position question dissolves for it.
                let literal_free = elements.iter().all(|&element| {
                    let ty = self.store.get(element);
                    !(ty.fresh || ty.flags.intersects(TypeFlags::UNIT))
                });
                if !uncontextual && !literal_free {
                    return error;
                }
                match self.union_with_subtype_reduction(&elements) {
                    // A DECIDABLE reduction is the answer whatever survives —
                    // the old `count <= 1` gate was §13's conservatism, and
                    // it rejected upstream's own multi-survivor unions
                    // (`(Derived1 | Derived2)[]`, the §7/§17 investigation's
                    // terminus: the reducer decided all along and the ARRAY
                    // arm threw the answer away). Undecidable stays a gap.
                    Some(subtype_reduced) => subtype_reduced,
                    None => return error,
                }
            } else {
                reduced
            }
        };

        let Some(target) = self.global_type_symbol("Array") else { return error };
        self.create_type_reference(target, vec![element_type])
    }

    /// The element type an array spread contributes — `Array<T>` only
    /// (`checker-notes-arrays.md` §6); `None` declines.
    fn array_spread_element_type(&mut self, operand: TypeId) -> Option<TypeId> {
        if operand == self.intrinsics.error {
            return None;
        }
        let (target, arguments) = self.type_reference_targets.get(&operand)?.clone();
        if arguments.len() != 1 {
            return None;
        }
        let array = self.global_type_symbol("Array")?;
        (self.binder.merged_symbol(target) == self.binder.merged_symbol(array))
            .then(|| arguments[0])
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
