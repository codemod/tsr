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
//! - ~~**Omitted elements.** `[1, , 2]` needs the optional-element flags model,
//!   which arrives with tuples.~~ **Wrong, and it cost fourteen cases (§203).**
//!   It needs nothing: `checkExpressionWorker` (`checker.go:7815`) answers
//!   `undefinedWideningType` for `KindOmittedExpression` in one unconditional
//!   line, and the optional-element flags decide how a **tuple** prints, not
//!   what an array-literal element contributes. `[1, 2, ,]` is
//!   `(number | undefined)[]`.
//! - **Two object-typed elements**, because upstream reduces the element union
//!   with `UnionReductionSubtype` and this port has no assignability. See
//!   [`Checker::check_array_literal`].

use tsr_ast::{ArrayLiteralExpression, Expression};

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

/// §76's context kinds — see `array_literal_tuple_context_kind`.
#[derive(PartialEq, Clone, Copy)]
enum TupleContext {
    No,
    /// A tuple ANNOTATION or declared tuple target supplies the shape.
    Annotated,
    /// A destructuring pattern supplies it; nests through inner literals
    /// index-by-index (`destructuring_array_pattern_slot`).
    Destructured,
}

/// The array pattern a destructured literal is assigned into — either a
/// binding pattern (`var [x] = …`) or an assignment target (`[x] = …`).
enum PatternSlot<'a> {
    Binding(&'a tsr_ast::BindingPattern<'a>),
    Assignment(&'a tsr_ast::ArrayLiteralExpression<'a>),
}

impl<'a> Checker<'a, '_> {
    /// §76's destructuring slot: the array PATTERN this literal is assigned
    /// into, when there is one that makes it a tuple context. `var [x] =
    /// [1, "hello"]` answers the binding pattern; `[x, y] = [1, "hello"]`
    /// answers the target literal; a NESTED literal answers only when the
    /// pattern element at ITS INDEX is itself an array pattern — `var [a3,
    /// b3] = [[x13, y13], …]` does NOT nest, because `a3` is a plain name
    /// and the inner array is assigned whole
    /// (`declarationEmitDestructuringArrayPattern2`). Empty and
    /// rest-bearing patterns widen at every level.
    fn destructuring_array_pattern_slot(
        &mut self,
        literal: tsr_ast::NodeId,
    ) -> Option<PatternSlot<'a>> {
        let parent = self.nodes.parent(literal)?;
        match self.node_map.get(parent)? {
            tsr_ast::Node::VariableDeclaration(declaration)
                if declaration.r#type.is_none()
                    && declaration.initializer.and_then(|i| i.node_id()) == Some(literal) =>
            {
                match declaration.name {
                    Some(tsr_ast::BindingName::BindingPattern(pattern))
                        if !pattern.elements.is_empty()
                            && !pattern.elements.iter().any(|e| e.dot_dot_dot_token.is_some())
                            && pattern.node_id.is_some_and(|p| {
                                self.nodes.kind(p) == tsr_ast::SyntaxKind::ArrayBindingPattern
                            }) =>
                    {
                        Some(PatternSlot::Binding(pattern))
                    }
                    _ => None,
                }
            }
            tsr_ast::Node::BinaryExpression(binary)
                if binary
                    .operator_token
                    .is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
                    && binary.right.and_then(|r| r.node_id()) == Some(literal) =>
            {
                match binary.left {
                    Some(tsr_ast::Expression::ArrayLiteralExpression(target))
                        if !target.elements.is_empty()
                            && !target
                                .elements
                                .iter()
                                .any(|e| matches!(e, Expression::SpreadElement(_))) =>
                    {
                        Some(PatternSlot::Assignment(target))
                    }
                    _ => None,
                }
            }
            // §160 (`checker-notes-narrow.md`): a PARAMETER default is
            // destructured by the parameter's own pattern the same way —
            // `function f([p] = [1])` prints the default `[1]` as `[number]`
            // (destructuringWithLiteralInitializers2), including the empty
            // DEFAULT (`[p] = []` is the empty tuple). The pattern itself
            // must be non-empty, same as the sibling arms: `[] = [1, 2, 3]`
            // gives no tuple context and keeps `number[]`
            // (emptyArrayBindingPatternParameter04, the pair's R→W).
            tsr_ast::Node::ParameterDeclaration(parameter)
                if parameter.r#type.is_none()
                    && parameter.initializer.and_then(|i| i.node_id()) == Some(literal) =>
            {
                match parameter.name {
                    Some(tsr_ast::BindingName::BindingPattern(pattern))
                        if !pattern.elements.is_empty()
                            && !pattern.elements.iter().any(|e| e.dot_dot_dot_token.is_some())
                            && pattern.node_id.is_some_and(|p| {
                                self.nodes.kind(p) == tsr_ast::SyntaxKind::ArrayBindingPattern
                            }) =>
                    {
                        Some(PatternSlot::Binding(pattern))
                    }
                    _ => None,
                }
            }
            // §76.1: a pattern element's DEFAULT initializer is destructured
            // by the element's own pattern — `var [a2, [b2, c2] = ["abc",
            // …]] = …` prints the default `["abc", …]` as a tuple
            // (`declarationEmitDestructuringArrayPattern2`'s row 25).
            tsr_ast::Node::BindingElement(element)
                if element.initializer.and_then(|i| i.node_id()) == Some(literal) =>
            {
                match element.name {
                    Some(tsr_ast::BindingName::BindingPattern(pattern))
                        if !pattern.elements.is_empty()
                            && !pattern.elements.iter().any(|e| e.dot_dot_dot_token.is_some())
                            && pattern.node_id.is_some_and(|p| {
                                self.nodes.kind(p) == tsr_ast::SyntaxKind::ArrayBindingPattern
                            }) =>
                    {
                        Some(PatternSlot::Binding(pattern))
                    }
                    _ => None,
                }
            }
            tsr_ast::Node::ArrayLiteralExpression(outer) => {
                let index = outer.elements.iter().position(|e| e.node_id() == Some(literal))?;
                let outer_id = outer.node_id?;
                match self.destructuring_array_pattern_slot(outer_id)? {
                    PatternSlot::Binding(pattern) => match pattern.elements.get(index)?.name {
                        Some(tsr_ast::BindingName::BindingPattern(inner))
                            if !inner.elements.is_empty()
                                && !inner
                                    .elements
                                    .iter()
                                    .any(|e| e.dot_dot_dot_token.is_some())
                                && inner.node_id.is_some_and(|p| {
                                    self.nodes.kind(p) == tsr_ast::SyntaxKind::ArrayBindingPattern
                                }) =>
                        {
                            Some(PatternSlot::Binding(inner))
                        }
                        _ => None,
                    },
                    PatternSlot::Assignment(target) => match target.elements.get(index)? {
                        Expression::ArrayLiteralExpression(inner)
                            if !inner.elements.is_empty()
                                && !inner
                                    .elements
                                    .iter()
                                    .any(|e| matches!(e, Expression::SpreadElement(_))) =>
                        {
                            Some(PatternSlot::Assignment(inner))
                        }
                        _ => None,
                    },
                }
            }
            _ => None,
        }
    }
}

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
    /// Whether this array literal sits in a TUPLE context — §63/§63.1's
    /// annotated-initializer and assignment-target arms, §76's destructuring
    /// and nesting arms. Only the DESTRUCTURING arms nest (an element of a
    /// destructured literal is itself destructured); an annotation-driven
    /// tuple context does not — its element types, not the pattern, decide
    /// the inner shapes (`arrayLiterals2ES5`'s `[number[], string[]]`).
    fn array_literal_in_tuple_context(&mut self, node: &ArrayLiteralExpression<'_>) -> bool {
        self.array_literal_tuple_context_kind(node) != TupleContext::No
    }

    fn array_literal_tuple_context_kind(
        &mut self,
        node: &ArrayLiteralExpression<'_>,
    ) -> TupleContext {
        if let Some(id) = node.node_id
            && self.destructuring_array_pattern_slot(id).is_some()
        {
            return TupleContext::Destructured;
        }
        'context: {
            let Some(id) = node.node_id else { break 'context TupleContext::No };
            let Some(parent) = self.nodes.parent(id) else {
                break 'context TupleContext::No;
            };
            match self.node_map.get(parent) {
                // §371: a type ASSERTION to a tuple supplies the tuple
                // context — `<[]>[]` / `[] as []` record the literal as the
                // empty tuple (`emptyTuplesTypeAssertion01/02`); upstream's
                // `inTupleContext` reads the contextual type an assertion
                // supplies.
                Some(tsr_ast::Node::TypeAssertion(assertion)) => match assertion.r#type {
                    Some(annotation) => {
                        let t = self.get_type_from_type_node(annotation);
                        if self.tuple_element_lists.contains_key(&t) {
                            TupleContext::Annotated
                        } else {
                            TupleContext::No
                        }
                    }
                    None => TupleContext::No,
                },
                Some(tsr_ast::Node::AsExpression(assertion)) => match assertion.r#type {
                    Some(annotation) => {
                        let t = self.get_type_from_type_node(annotation);
                        if self.tuple_element_lists.contains_key(&t) {
                            TupleContext::Annotated
                        } else {
                            TupleContext::No
                        }
                    }
                    None => TupleContext::No,
                },
                Some(tsr_ast::Node::VariableDeclaration(declaration))
                    if declaration.initializer.and_then(|i| i.node_id()) == Some(id) =>
                {
                    match declaration.r#type {
                        Some(annotation) => {
                            let t = self.get_type_from_type_node(annotation);
                            if self.tuple_element_lists.contains_key(&t) {
                                TupleContext::Annotated
                            } else {
                                TupleContext::No
                            }
                        }
                        // §76: the destructuring legs are decided by
                        // `destructuring_array_pattern_slot` below.
                        None => TupleContext::No,
                    }
                }
                // §63.1: the assignment-target arm — the target's DECLARED
                // type (the §6.3 rule, extended to spread-free literals).
                Some(tsr_ast::Node::BinaryExpression(binary))
                    if binary
                        .operator_token
                        .is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
                        && binary.right.and_then(|r| r.node_id()) == Some(id) =>
                {
                    match binary.left {
                        Some(tsr_ast::Expression::Identifier(identifier)) => {
                            let annotated = identifier
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
                                .map(|symbol| self.get_type_of_symbol(symbol))
                                .is_some_and(|t| self.tuple_element_lists.contains_key(&t));
                            if annotated { TupleContext::Annotated } else { TupleContext::No }
                        }
                        _ => TupleContext::No,
                    }
                }
                // §76.2: a call ARGUMENT under a TUPLE parameter type —
                // `f(["string", 1, 2])` against `x: [string, number,
                // number]` prints the literal as the tuple
                // (`destructuringParameterDeclaration1ES5iterable`).
                Some(tsr_ast::Node::CallExpression(call)) => {
                    let contextual = self.contextual_type_for_argument(call, id);
                    match contextual {
                        Some(t) if self.tuple_element_lists.contains_key(&t) => {
                            TupleContext::Annotated
                        }
                        _ => TupleContext::No,
                    }
                }
                _ => TupleContext::No,
            }
        }
    }

    pub(crate) fn check_array_literal(&mut self, node: &ArrayLiteralExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        // §365: the literal that IS the target of a destructuring assignment
        // mints a TUPLE of its element targets' types — upstream's
        // `inDestructuringPattern` exit of `checkArrayLiteral`
        // (`checker.go:8084`, `createTupleTypeEx`): `[a, b] = new FooIterator`
        // records `>[a, b] : [Bar, Bar]` (`iterableArrayPattern3`). Spreads
        // and omissions need the rest/optional element flags this tuple mint
        // does not carry, so they keep today's road.
        if let Some(id) = node.node_id
            && let Some(parent) = self.nodes.parent(id)
            && let Some(tsr_ast::Node::BinaryExpression(binary)) = self.node_map.get(parent)
            && binary.operator_token.is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
            && binary.left.and_then(|l| l.node_id()) == Some(id)
            && !node.elements.iter().enumerate().any(|(index, element)| {
                // §431: ONE TRAILING spread WITH leading elements is the rest
                // element and prints; a rest-only target prints `T[]` through
                // the ordinary road (`[...obj?.a] = x` is `any[]`,
                // `elementAccessChain.3` — the draft's 9 R->W), and anything
                // else keeps today's road too.
                matches!(element, Expression::SpreadElement(_))
                    && (index + 1 != node.elements.len() || node.elements.len() == 1)
                    || matches!(element, Expression::OmittedExpression(_))
            })
        {
            // §371: the EMPTY target included — `[] = iterable` records
            // `>[] : []` (`emptyAssignmentPatterns01_ES6`); §365's draft
            // excluded it for no upstream reason.
            let mut elements = Vec::with_capacity(node.elements.len());
            let mut rest: Option<String> = None;
            for element in node.elements {
                // §431: the trailing REST target spells `...T` in the tuple —
                // `[a, ...b] = new FooIterator` records `[Bar, ...Bar[]]`
                // (`iterableArrayPattern4/6/8`). Display-only mint: the
                // pattern line is the only consumer, the assignment's own
                // type is the RHS.
                if let Expression::SpreadElement(spread) = element {
                    let Some(operand) = spread.expression else { return error };
                    let operand_type = self.check_expression(operand);
                    if operand_type == error {
                        return error;
                    }
                    rest = Some(format!("...{}", self.type_to_string(operand_type)));
                    continue;
                }
                let element_type = self.check_expression(*element);
                if element_type == error {
                    return error;
                }
                elements.push(element_type);
            }
            if let Some(rest) = rest {
                let mut parts: Vec<String> =
                    elements.iter().map(|&element| self.type_to_string(element)).collect();
                parts.push(rest);
                let printed = format!("[{}]", parts.join(", "));
                return self.store.new_named(TypeFlags::OBJECT, printed, None);
            }
            return self.create_tuple_type(elements, false);
        }
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
        // §105 slice 2a fired leg (A): the literal's OWN line in a const
        // context answers the readonly tuple (`checkArrayLiteral` under
        // `isConstContext`, `checker.go:8021`) — the slice-1 mint lived only
        // in the assertion arm, so `[10] as const`'s literal line kept
        // printing `number[]` (constAssertions 0:184/0:190).
        if node.node_id.is_some_and(|id| self.is_const_context(id)) && !has_tuple_spread {
            let mut elements = Vec::with_capacity(node.elements.len());
            let mut clean = true;
            for element in node.elements {
                match element {
                    Expression::SpreadElement(_) | Expression::OmittedExpression(_) => {
                        clean = false;
                        break;
                    }
                    other => {
                        let checked = self.check_expression(*other);
                        let regular = self.get_regular_type_of_literal_type(checked);
                        if regular == error {
                            clean = false;
                            break;
                        }
                        elements.push(regular);
                    }
                }
            }
            if clean {
                return self.create_tuple_type(elements, true);
            }
            return error;
        }
        // §63 (`checker-notes-arrays.md`): a PLAIN literal under a TUPLE
        // context mints the tuple too — `const y: [number, number] = [0, 0]`
        // prints the literal as `[number, number]` (~170 corpus lines);
        // the §6.3 context test extends to all literals.
        let tuple_context = self.array_literal_in_tuple_context(node);
        if tuple_context && !has_tuple_spread {
            let mut elements = Vec::with_capacity(node.elements.len());
            let mut clean = true;
            for element in node.elements {
                match element {
                    Expression::SpreadElement(_) | Expression::OmittedExpression(_) => {
                        clean = false;
                        break;
                    }
                    _ => {
                        let element_type = self.check_expression_for_mutable_location(*element);
                        if element_type == error {
                            clean = false;
                            break;
                        }
                        elements.push(self.get_widened_literal_type(element_type));
                    }
                }
            }
            if clean {
                // Empty included: `const t: [] = []` prints the empty TUPLE
                // (`[]`, 90 baseline instances per the tuple notes), not
                // `never[]`.
                return self.create_tuple_type(elements, false);
            }
        }
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
            // **An elision is `undefined`, not a gap.** `checkExpressionWorker`
            // (`checker.go:7815`) answers `undefinedWideningType` for
            // `KindOmittedExpression` — one line, no condition — so
            // `[1, 2, ,]` is `(number | undefined)[]`
            // (`compiler/commentOnArrayElement3`). This port refused, which
            // read as caution and was a refusal to transcribe a constant. §203.
            //
            // The widening/non-widening distinction is the one this port does
            // not carry (`undefinedWideningType` and `undefinedType` print the
            // same string and differ only under `getWidenedType`), the same
            // approximation the empty-array arm below already documents.
            if matches!(element, Expression::OmittedExpression(_)) {
                elements.push(self.intrinsics.undefined);
                continue;
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

    /// The element type an array spread contributes — `Array<T>`, or a
    /// custom iterator through §284's seam.
    ///
    /// §285's first draft delegated to [`Checker::for_of_element_type`]
    /// whole; five `arrayLiteralSpread` lines went RIGHT→GAP through the
    /// helper's other arms, so the Array half is kept verbatim and only the
    /// iterator tail is appended — strictly additive by construction.
    pub(crate) fn array_spread_element_type(&mut self, operand: TypeId) -> Option<TypeId> {
        if operand == self.intrinsics.error {
            return None;
        }
        // §397: spreading an `any` contributes `any` — upstream's
        // `checkIteratedTypeOrElementType` answers the anyType straight off
        // (`[...obj?.a]` is `any[]`, `propertyAccessChain.3`,
        // `trailingCommasInBindingPatterns`). Identity against the intrinsic,
        // not a flag test: `errorType` carries ANY and must stay the gap
        // above.
        if operand == self.intrinsics.any {
            return Some(self.intrinsics.any);
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&operand).cloned()
            && arguments.len() == 1
            && let Some(array) = self.global_type_symbol("Array")
            && self.binder.merged_symbol(target) == self.binder.merged_symbol(array)
        {
            return Some(arguments[0]);
        }
        // §285: `[...new SymbolIterator]` reads the same custom-iterator seam
        // the for-of road does (§284), gated by the same syntactic presence
        // test (`iteratorSpreadInArray5`, `iteratorSpreadInCall*`).
        if self.declares_symbol_iterator(operand) {
            if let Some(element) = self.for_of_element_type(operand) {
                return Some(element);
            }
        }
        // §495: a DECIDABLE protocol failure is upstream's reported
        // not-iterable error, and `checkIteratedTypeOrElementType` answers
        // `anyType` there (`checker.go:6103`) — a heritage-free class with no
        // spelling of `[Symbol.iterator]` at all, or whose iterator returns
        // only `this` while `next` is absent (`iteratorSpreadInArray8/10`).
        // Everything short of provable stays the gap.
        if self.iteration_decidably_fails(operand) {
            return Some(self.intrinsics.any);
        }
        None
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
