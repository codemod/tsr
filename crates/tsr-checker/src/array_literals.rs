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
    /// (`declarationEmitDestructuringArrayPattern2`). Binding patterns follow
    /// [`Checker::array_binding_pattern_implies_tuple`].
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
                        if self.array_binding_pattern_implies_tuple(pattern) =>
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
                        if self.array_binding_pattern_implies_tuple(pattern) =>
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
                        if self.array_binding_pattern_implies_tuple(pattern) =>
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
                            if self.array_binding_pattern_implies_tuple(inner) =>
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

    /// Whether `getTypeFromArrayBindingPattern` (`checker.go:17957`) answers a
    /// tuple for this pattern, i.e. whether the implied type that
    /// `getContextualTypeForInitializerExpression` supplies is tuple-like.
    /// Only an empty pattern or a lone rest element yields the iterable/array
    /// of `any`; every other array pattern, rest-bearing included, is a tuple
    /// (`var [x, ...a] = [1, 2, 3]` records `[number, number, number]`).
    fn array_binding_pattern_implies_tuple(&self, pattern: &tsr_ast::BindingPattern<'_>) -> bool {
        pattern
            .node_id
            .is_some_and(|p| self.nodes.kind(p) == tsr_ast::SyntaxKind::ArrayBindingPattern)
            && match pattern.elements {
                [] => false,
                [only] => only.dot_dot_dot_token.is_none(),
                _ => true,
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
    /// isConstContext's contextual-type-variable arm (checker.go:13618).
    /// Read the uninstantiated contextual target before following const carriers.
    pub(crate) fn literal_in_const_type_variable_context(&mut self, id: tsr_ast::NodeId) -> bool {
        // Context queries re-enter argument checking in this port. Resolve the
        // enclosing call first, as the previous argument-context road did, and
        // avoid entering a non-const call's property contexts needlessly.
        let mut child = id;
        let mut enclosing_call = None;
        let mut enclosing_new = None;
        let mut crossed_function = false;
        for ancestor in self.nodes.ancestors(id) {
            match self.node_map.get(ancestor) {
                Some(tsr_ast::Node::CallExpression(call)) => {
                    if call.expression.and_then(|expression| expression.node_id()) == Some(child) {
                        return false;
                    }
                    enclosing_call = Some(call);
                    break;
                }
                Some(tsr_ast::Node::NewExpression(new)) => {
                    if new.expression.and_then(|expression| expression.node_id()) == Some(child) {
                        return false;
                    }
                    enclosing_new = Some(new);
                    break;
                }
                Some(tsr_ast::Node::FunctionDeclaration(_)) => return false,
                Some(tsr_ast::Node::ArrowFunction(_) | tsr_ast::Node::FunctionExpression(_)) => {
                    crossed_function = true;
                    child = ancestor;
                }
                _ => child = ancestor,
            }
        }
        if let Some(new) = enclosing_new {
            let has_const_parameter =
                new.node_id.and_then(|id| self.active_inference_contexts.get(&id)).is_some_and(
                    |context| context.signature.type_parameters.iter().any(|p| p.is_const),
                );
            return has_const_parameter && self.literal_has_const_contextual_target(id);
        }
        let Some(call) = enclosing_call else { return false };
        // An IIFE's parameters derive from its arguments. Computing that
        // callee while checking a spread argument would cache its in-flight
        // parameter as any. A function expression's own parameter declarations
        // already establish whether a const context is possible.
        let mut callee = call.expression;
        while let Some(tsr_ast::Expression::ParenthesizedExpression(parenthesized)) = callee {
            callee = parenthesized.expression;
        }
        let parameters = match callee {
            Some(tsr_ast::Expression::ArrowFunction(function)) => Some(function.type_parameters),
            Some(tsr_ast::Expression::FunctionExpression(function)) => {
                Some(function.type_parameters)
            }
            _ => None,
        };
        if parameters.is_some_and(|parameters| !parameters.iter().any(|parameter|
            parameter.modifiers.iter().any(|modifier| matches!(modifier,
                tsr_ast::ModifierLike::Token(token) if token.kind == tsr_ast::SyntaxKind::ConstKeyword)))) {
            return false;
        }
        {
            let Some(call_id) = call.node_id else { return false };
            let has_const_parameter = if let Some(context) =
                self.active_inference_contexts.get(&call_id)
            {
                context.signature.type_parameters.iter().any(|p| p.is_const)
            } else if crossed_function {
                // Overload selection can check a callback before parking an
                // inference context. A freshness query must not select that
                // overload again and check the same callback recursively.
                callee
                    .and_then(|callee| callee.node_id())
                    .and_then(|callee| self.node_types.get(&callee))
                    .and_then(|callee| self.signature_types.get(callee))
                    .is_some_and(|signatures| {
                        signatures
                            .iter()
                            .any(|signature| signature.type_parameters.iter().any(|p| p.is_const))
                    })
            } else {
                if !self.resolving_signature_calls.insert(call_id) {
                    return false;
                }
                let has_const_parameter = call
                    .expression
                    .map(|callee| self.check_expression(callee))
                    .and_then(|callee| self.resolve_call_signature(callee, Some(call.arguments)))
                    .is_some_and(|signature| signature.type_parameters.iter().any(|p| p.is_const));
                self.resolving_signature_calls.remove(&call_id);
                has_const_parameter
            };
            if !has_const_parameter {
                return false;
            }
        }
        self.literal_has_const_contextual_target(id)
    }

    fn literal_has_const_contextual_target(&mut self, id: tsr_ast::NodeId) -> bool {
        let mut current = id;
        loop {
            let previous = self.contextual_prefers_uninstantiated;
            self.contextual_prefers_uninstantiated = true;
            let contextual = self.get_contextual_type(current);
            self.contextual_prefers_uninstantiated = previous;
            if contextual.is_some_and(|ty| self.is_const_type_variable(ty, 0)) {
                return true;
            }
            let Some(parent) = self.nodes.parent(current) else {
                return false;
            };
            match self.node_map.get(parent) {
                Some(
                    tsr_ast::Node::ArrayLiteralExpression(_)
                    | tsr_ast::Node::ParenthesizedExpression(_)
                    | tsr_ast::Node::ObjectLiteralExpression(_)
                    | tsr_ast::Node::PropertyAssignment(_)
                    | tsr_ast::Node::ShorthandPropertyAssignment(_)
                    | tsr_ast::Node::SpreadElement(_)
                    | tsr_ast::Node::SpreadAssignment(_)
                    | tsr_ast::Node::TemplateSpan(_),
                ) => current = parent,
                _ => return false,
            }
        }
    }

    pub(crate) fn array_literal_in_tuple_context(
        &mut self,
        node: &ArrayLiteralExpression<'_>,
    ) -> bool {
        self.array_literal_tuple_context_kind(node) != TupleContext::No
    }

    /// `contextualType != nil && someType(contextualType,
    /// isMutableArrayLikeType)` for a const-context literal (checkArrayLiteral,
    /// checker.go:8087). `getContextualType` passes a const assertion through
    /// to its own context (checker.go:29368), which `get_contextual_type`
    /// does not, so that arm is walked here; the remaining parents are
    /// `get_contextual_type`'s.
    pub(crate) fn array_literal_contextual_is_mutable(&mut self, id: tsr_ast::NodeId) -> bool {
        let mut current = id;
        while let Some(parent) = self.nodes.parent(current) {
            let annotation = match self.node_map.get(parent) {
                Some(tsr_ast::Node::AsExpression(assertion)) => assertion.r#type,
                Some(tsr_ast::Node::TypeAssertion(assertion)) => assertion.r#type,
                _ => None,
            };
            if !annotation.is_some_and(crate::assertions::is_const_type_reference) {
                break;
            }
            current = parent;
        }
        self.get_contextual_type(current)
            .is_some_and(|contextual| self.const_context_is_mutable_array_like(contextual))
    }

    /// `isSpreadIntoCallOrNew` (`checker.go:8117`). A parenthesis does NOT break
    /// this one — upstream walks up through them explicitly, unlike the optional
    /// chain's flag, which a parenthesis stops.
    fn array_literal_is_spread_into_call_or_new(&self, id: tsr_ast::NodeId) -> bool {
        let mut current = id;
        while let Some(parent) = self.nodes.parent(current) {
            if matches!(self.node_map.get(parent), Some(tsr_ast::Node::ParenthesizedExpression(_)))
            {
                current = parent;
                continue;
            }
            if !matches!(self.node_map.get(parent), Some(tsr_ast::Node::SpreadElement(_))) {
                return false;
            }
            return matches!(
                self.nodes.parent(parent).map(|g| self.nodes.kind(g)),
                Some(tsr_ast::SyntaxKind::CallExpression | tsr_ast::SyntaxKind::NewExpression)
            );
        }
        false
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
        let decided = 'context: {
            let Some(id) = node.node_id else { break 'context TupleContext::No };
            let Some(parent) = self.nodes.parent(id) else {
                break 'context TupleContext::No;
            };
            // §887: `isSpreadIntoCallOrNew` (`checker.go:8117`), the FIRST
            // disjunct of upstream's `inTupleContext` and the one arm of it that
            // reads no contextual type at all:
            //
            // ```go
            // parent := ast.WalkUpParenthesizedExpressions(node.Parent)
            // return ast.IsSpreadElement(parent) && ast.IsCallOrNewExpression(parent.Parent)
            // ```
            //
            // `f(...[1, 2])` records `[number, number]` for the literal. The
            // spread is how a call consumes positions, so the positions have to
            // survive — widening to `number[]` loses exactly the information the
            // call is about to use. Every other arm below reads an ANNOTATION,
            // and a spread argument has none, so this position could not be
            // reached however it was spelled — the same gap §793 found for a
            // variadic parameter.
            if self.array_literal_is_spread_into_call_or_new(id) {
                break 'context TupleContext::Annotated;
            }
            // §793: a CALL ARGUMENT whose parameter is spelled `[...T]` is in
            // TUPLE context. `f<T extends unknown[]>(t: [...T])` called with
            // `f([1, 2])` records `[number, number]` for the literal upstream,
            // not `number[]` — the variadic parameter is exactly how a
            // signature asks for a tuple.
            //
            // Every other arm of this function reads an ANNOTATION (assertion,
            // declaration, assignment target); a call argument had none of
            // them, so this position could never reach tuple context however
            // the parameter was spelled.
            if let Some(tsr_ast::Node::CallExpression(call)) = self.node_map.get(parent)
                && let Some(contextual) = self.contextual_type_for_argument(call, id)
                && (self.variadic_tuple_nodes.contains_key(&contextual)
                    || self.variadic_tuple_elements.contains_key(&contextual))
            {
                break 'context TupleContext::Annotated;
            }
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
                // §654. `return [y, f(x)];` inside `f(...): [T, U]` — the
                // literal's tuple context comes from the ENCLOSING function's
                // return annotation, which no arm above reaches, so the parent
                // fell to `_` and the literal widened to `(T | U)[]`.
                //
                // The branch was printed rather than guessed: the dispatch
                // simply has no `ReturnStatement` arm. The lookup mirrors
                // `enclosing_function_has_a_return_annotation`
                // (`implicit_any.rs:462`) and then applies the same
                // `tuple_element_lists` test every annotated arm above uses.
                Some(tsr_ast::Node::ReturnStatement(_)) => {
                    let annotation = self.nodes.ancestors(id).find_map(|ancestor| {
                        match self.node_map.get(ancestor) {
                            Some(tsr_ast::Node::FunctionDeclaration(n)) => Some(n.r#type),
                            Some(tsr_ast::Node::FunctionExpression(n)) => Some(n.r#type),
                            Some(tsr_ast::Node::ArrowFunction(n)) => Some(n.r#type),
                            Some(tsr_ast::Node::MethodDeclaration(n)) => Some(n.r#type),
                            Some(tsr_ast::Node::GetAccessorDeclaration(n)) => Some(n.r#type),
                            _ => None,
                        }
                    });
                    match annotation.flatten() {
                        Some(annotation) => {
                            let t = self.get_type_from_type_node(annotation);
                            if self.tuple_element_lists.contains_key(&t) {
                                TupleContext::Annotated
                            } else {
                                TupleContext::No
                            }
                        }
                        None => TupleContext::No,
                    }
                }
                _ => TupleContext::No,
            }
        };
        if decided != TupleContext::No {
            return decided;
        }
        // §888: the generic question, behind the seven arms.
        match node.node_id {
            Some(id) if self.array_literal_has_a_tuple_contextual_type(id) => {
                TupleContext::Annotated
            }
            _ => TupleContext::No,
        }
    }

    /// §888: upstream's SECOND `inTupleContext` disjunct (`checker.go:8029`) —
    /// *is the contextual type tuple-like?* — asked once, generically.
    ///
    /// ```go
    /// contextualType != nil && someType(contextualType, func(t *Type) bool {
    ///     return c.isTupleLikeType(t) || …
    /// })
    /// ```
    ///
    /// [`Checker::array_literal_tuple_context_kind`] approximates this with seven
    /// hand-rolled parent-kind arms, each re-deriving an annotation. This asks
    /// [`Checker::get_contextual_type`] instead, which dispatches on the parent
    /// too but covers a **strict superset**: it adds `SatisfiesExpression`,
    /// `PropertyDeclaration`, `NewExpression`, `ConditionalExpression`,
    /// `PropertyAssignment`, `ParenthesizedExpression`, an enclosing
    /// `ArrayLiteralExpression` and an arrow's expression body.
    ///
    /// **Consulted only where the seven arms already answered `No`**, so it can
    /// only widen. That is deliberate and not merely cautious: the two roads
    /// disagree on *which type* a shared parent kind yields (the hand-rolled arms
    /// read the written annotation node; `get_contextual_type` may return an
    /// inferred or instantiated type), and replacing rather than extending would
    /// put those disagreements in play at the same time as the new coverage.
    ///
    /// The tuple test is upstream's `isTupleLikeType`
    /// ([`Checker::is_tuple_like_type`]): a tuple, a `"0"` property, or an
    /// array-like with a literal `length` — not only the `tuple_element_lists`
    /// membership the annotated arms above use (`r4-arrays.md` §1).
    ///
    /// §889 supplies the `someType` half: upstream's predicate is applied to a
    /// union **constituent by constituent** (`someType` short-circuits on the
    /// first that matches), so `[number, string] | undefined` is a tuple context
    /// — which is exactly the shape §885 mints for an optional member.
    /// Homomorphic generic mapped contexts also preserve tuple positions.
    fn array_literal_has_a_tuple_contextual_type(&mut self, id: tsr_ast::NodeId) -> bool {
        let Some(contextual) = self.get_contextual_type(id) else { return false };
        let contextual = self.instantiate_contextual_inference_type(contextual, id);
        let contextual = self.apparent_contextual_type(contextual);
        let mut contexts = match &self.store.get(contextual).data {
            crate::types::TypeData::Union { types, .. } => types.clone(),
            _ => vec![contextual],
        };
        if contexts.iter().any(|&t| {
            self.is_tuple_like_type(t)
                || (self.is_generic_homomorphic_mapped_type(t)
                    && self.mapped_types.get(&t).is_some_and(|info| info.name_type.is_none()))
        }) {
            return true;
        }
        // A pre-inference contextual read: upstream's first inference pass
        // checks the argument against the WRITTEN parameter type, which
        // `instantiateContextualType` (no `ContextFlagsSignature`) maps only
        // through the return mapper, and whose apparent type is the
        // constraint — `all<T extends readonly unknown[] | []>([a, b])` is a
        // tuple context through `[]`. This port's stateless argument road
        // fixes the variable to unknown first, so the written type is read
        // here. Written type arguments still use their instantiated context.
        if self.nodes.parent(id).and_then(|parent| self.node_map.get(parent)).is_some_and(
            |node| matches!(node, tsr_ast::Node::CallExpression(call) if call.type_arguments.is_empty()),
        ) {
            let saved = self.contextual_prefers_uninstantiated;
            self.contextual_prefers_uninstantiated = true;
            let original = self.get_contextual_type(id);
            self.contextual_prefers_uninstantiated = saved;
            if let Some(original) = original {
                let original = self.instantiate_contextual_type_without_signature(original, id);
                let original = self.apparent_contextual_type(original);
                contexts = match &self.store.get(original).data {
                    crate::types::TypeData::Union { types, .. } => types.clone(),
                    _ => vec![original],
                };
                return contexts.iter().any(|t| {
                    self.tuple_element_lists.contains_key(t)
                        || self.variadic_tuple_elements.contains_key(t)
                        || (self.is_generic_homomorphic_mapped_type(*t)
                            && self.mapped_types.get(t).is_some_and(|info| info.name_type.is_none()))
                });
            }
        }
        false
    }

    pub(crate) fn check_array_literal(&mut self, node: &ArrayLiteralExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        // `inDestructuringPattern := ast.IsAssignmentTarget(node)`
        // (`checker.go:8026`): nested targets such as `{ a: [x, ...y] } = o`
        // count, not only the direct left operand of `=`.
        // §365: the literal that IS the target of a destructuring assignment
        // mints a TUPLE of its element targets' types — upstream's
        // `inDestructuringPattern` exit of `checkArrayLiteral`
        // (`checker.go:8084`, `createTupleTypeEx`): `[a, b] = new FooIterator`
        // records `>[a, b] : [Bar, Bar]` (`iterableArrayPattern3`). Spreads
        // and omissions need the rest/optional element flags this tuple mint
        // does not carry, so they keep today's road.
        if let Some(id) = node.node_id
            && self.assignment_target_kind(id) != crate::expressions::AssignmentTargetKind::None
            && !node.elements.iter().enumerate().any(|(index, element)| {
                // A trailing rest-only target normalizes to an array, not a
                // one-element tuple. It must still use the assignment-target
                // index/iteration/unknown fallback rather than spread's any.
                matches!(element, Expression::SpreadElement(_)) && index + 1 != node.elements.len()
                    || matches!(element, Expression::OmittedExpression(_))
            })
        {
            // §371: the EMPTY target included — `[] = iterable` records
            // `>[] : []` (`emptyAssignmentPatterns01_ES6`); §365's draft
            // excluded it for no upstream reason.
            let mut elements = Vec::with_capacity(node.elements.len());
            for element in node.elements {
                let (element_type, spread) = if let Expression::SpreadElement(spread) = element {
                    let Some(operand) = spread.expression else { return error };
                    let operand_type = self.check_expression(operand);
                    if operand_type == error {
                        return error;
                    }
                    // `checkArrayLiteral` classifies an array-like spread as a
                    // Variadic tuple element. Normalization then expands a
                    // fixed tuple, retains a generic variadic, and turns a
                    // plain array into its element Rest type. For a
                    // non-array-like destructuring rest, its numeric index type
                    // (or unknown) is wrapped in the array representation that
                    // this port's normalizer uses for a Rest element.
                    // ADR-0048: upstream's `errorType` is array-like
                    // (`isArrayLikeType`, `checker.go:23520`: assignable to
                    // `ReadonlyArray<any>`), so it stays Variadic and the
                    // normalizer makes it a Rest of itself (`:23372`) —
                    // `[...c] = tupel` records `[...c] : any[]`
                    // (`assignmentRestElementWithErrorSourceType`).
                    if self.tuple_array_like(operand_type)
                        || operand_type == self.intrinsics.native_error
                    {
                        (operand_type, true)
                    } else {
                        let indexed = if self
                            .store
                            .get(operand_type)
                            .flags
                            .intersects(TypeFlags::STRING_LIKE)
                        {
                            self.intrinsics.string
                        } else if let Some(info) =
                            self.get_applicable_index_info(operand_type, self.intrinsics.number)
                        {
                            info.value
                        } else {
                            self.tuple_index_type(operand_type, self.intrinsics.number, false)
                                .or_else(|| self.iterated_element_type(operand_type))
                                .unwrap_or(self.intrinsics.unknown)
                        };
                        let Some(array) = self.global_type_symbol("Array") else {
                            return error;
                        };
                        (self.create_type_reference(array, vec![indexed]), true)
                    }
                } else {
                    // `checkArrayLiteral` reads every element through
                    // checkExpressionForMutableLocation, so a defaulted target
                    // `b = 0` contributes `number`, not the fresh `0`.
                    let element_type = self.check_expression_for_mutable_location(*element);
                    if element_type == error {
                        return error;
                    }
                    (element_type, false)
                };
                elements.push(crate::tuples::TupleElement {
                    r#type: element_type,
                    spread,
                    optional: false,
                    label: None,
                });
            }
            return self.normalize_variadic_tuple(elements, false);
        }
        let checked = self.check_array_literal_value(node);
        self.create_array_literal_type(checked)
    }

    fn check_array_literal_element(
        &mut self,
        node: &ArrayLiteralExpression<'_>,
        element: Expression<'_>,
    ) -> TypeId {
        let ty = self.check_expression_for_mutable_location(element);
        if let Some(id) = element.node_id()
            && self.live_inference_context(id).is_some()
            && self.is_context_sensitive_argument(&element)
            && self.array_literal_in_tuple_context(node)
        {
            self.add_intra_expression_inference_site(id, ty);
        }
        ty
    }

    fn check_array_literal_value(&mut self, node: &ArrayLiteralExpression<'_>) -> TypeId {
        let error = self.intrinsics.error;
        // `checkArrayLiteral` keeps array-like spread operands as variadic
        // tuple arguments. Normalization expands tuples and retains generic
        // operands; outside tuple context they contribute operand[number].
        let has_spread =
            node.elements.iter().any(|element| matches!(element, Expression::SpreadElement(_)));
        let omitted_tuple = !has_spread
            && node
                .elements
                .iter()
                .any(|element| matches!(element, Expression::OmittedExpression(_)))
            && (node.node_id.is_some_and(|id| {
                self.is_const_context(id) || self.literal_in_const_type_variable_context(id)
            }) || self.array_literal_in_tuple_context(node));
        if has_spread || omitted_tuple {
            let mut arguments = Vec::with_capacity(node.elements.len());
            let mut supported = true;
            let mut has_omitted = false;
            for element in node.elements {
                let (t, spread) = if let Expression::SpreadElement(spread) = element {
                    let Some(operand) = spread.expression else { return error };
                    let t = self.check_expression(operand);
                    if self.tuple_array_like(t) {
                        (t, true)
                    } else {
                        // checkArrayLiteral keeps array-like operands Variadic,
                        // but other spreads contribute the iterated element as
                        // a Rest. The normalizer represents Rest with an array.
                        let Some(element) = self.array_spread_element_type(t) else {
                            supported = false;
                            break;
                        };
                        let Some(array) = self.global_type_symbol("Array") else { return error };
                        (self.create_type_reference(array, vec![element]), true)
                    }
                } else if matches!(element, Expression::OmittedExpression(_)) {
                    if self.exact_optional_property_types {
                        has_omitted = true;
                        (self.intrinsics.missing, false)
                    } else {
                        (self.intrinsics.undefined, false)
                    }
                } else {
                    let t = self.check_array_literal_element(node, *element);
                    if self.is_gap(t) {
                        return error;
                    }
                    // checkArrayLiteral marks ordinary elements after an
                    // exact-optional omission optional as well. Spread flags
                    // stay Variadic/Rest regardless of preceding omissions.
                    let t = if has_omitted && self.strict_null_checks {
                        self.get_optional_type(t, true)
                    } else {
                        t
                    };
                    (t, false)
                };
                if self.is_gap(t) {
                    return error;
                }
                arguments.push(crate::tuples::TupleElement {
                    r#type: t,
                    spread,
                    optional: has_omitted && !spread,
                    label: None,
                });
            }
            if supported {
                let const_argument =
                    node.node_id.is_some_and(|id| self.literal_in_const_type_variable_context(id));
                let const_context = node.node_id.is_some_and(|id| self.is_const_context(id));
                if const_context || const_argument || self.array_literal_in_tuple_context(node) {
                    return self
                        .normalize_variadic_tuple(arguments, const_context && !const_argument);
                }
                let mut elements = Vec::with_capacity(arguments.len());
                for argument in arguments {
                    let t = if argument.spread {
                        self.tuple_index_type(argument.r#type, self.intrinsics.number, false)
                            .unwrap_or(error)
                    } else {
                        argument.r#type
                    };
                    if self.is_gap(t) {
                        return error;
                    }
                    elements.push(t);
                }
                let element = self.get_union_type(&elements);
                let Some(array) = self.global_type_symbol("Array") else { return error };
                return self.create_type_reference(array, vec![element]);
            }
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
        // §797: an array literal that is the ARGUMENT of a `const` type
        // parameter is in a CONST CONTEXT — `f(["b", "c"])` on
        // `declare function f<const T>(x: T)` records `["b", "c"]`, not
        // `string[]` (`isConstTypeVariable`, `checker.go`).
        //
        // `is_const_context` cannot answer this: it walks the parent chain
        // SYNTACTICALLY and const-ness here depends on the callee's resolved
        // signature. Same seam §793 used for the tuple-context arm.
        let const_argument =
            node.node_id.is_some_and(|id| self.literal_in_const_type_variable_context(id));
        if (const_argument || node.node_id.is_some_and(|id| self.is_const_context(id)))
            && !has_tuple_spread
        {
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
                // §798: the `readonly` belongs at the INFERENCE site, not the
                // literal, when the const context came from a `const` TYPE
                // PARAMETER. Upstream records `['a', ['b', 'c']]` as
                // `["a", ["b", "c"]]` — a tuple, NOT readonly — while the CALL
                // is `readonly ["a", readonly ["b", "c"]]`; the readonly comes
                // from the const type variable, not from `checkArrayLiteral`.
                //
                // An `as const` assertion is the other way round and keeps the
                // readonly here, which is what `is_const_context` answers —
                // unless the contextual type has a mutable array-like
                // constituent: `inConstContext && !(contextualType != nil &&
                // someType(contextualType, isMutableArrayLikeType))`
                // (checkArrayLiteral, checker.go:8087), so `[1] as const
                // satisfies unknown[]` is the mutable `[1]`.
                let readonly = !const_argument
                    && !node.node_id.is_some_and(|id| self.array_literal_contextual_is_mutable(id));
                return self.create_tuple_type(elements, readonly);
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
                        let element_type = self.check_array_literal_element(node, *element);
                        if element_type == error {
                            clean = false;
                            break;
                        }
                        elements.push(element_type);
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
                                union_elements.push(union);
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
                            let element_type = self.check_array_literal_element(node, *element);
                            if element_type == error {
                                return error;
                            }
                            union_elements.push(element_type);
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
                        let element_type = self.check_array_literal_element(node, *element);
                        if element_type == error {
                            return error;
                        }
                        spliced.push(element_type);
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
            let element_type = self.check_array_literal_element(node, *element);
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
            // Distinct identities let isEmptyArrayLiteralType distinguish an
            // inferred empty array from a written never[] or undefined[].
            if self.strict_null_checks {
                self.intrinsics.implicit_never
            } else {
                self.intrinsics.undefined_widening
            }
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
        // not a flag test: the port's gap carries ANY and must stay the
        // decline above. ADR-0048: upstream's own `errorType` is any-flagged
        // too, so `IsTypeAny(inputType)` returns it unchanged
        // (`checkIteratedTypeOrElementType`, `checker.go:6096`) — the element is
        // `errorType`.
        if operand == self.intrinsics.any || operand == self.intrinsics.native_error {
            return Some(operand);
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&operand).cloned()
            && arguments.len() == 1
            && let Some(array) = self.global_type_symbol("Array")
            && self.binder.merged_symbol(target) == self.binder.merged_symbol(array)
        {
            return Some(arguments[0]);
        }
        // §567: a STRING spreads to `string`. `checkIteratedTypeOrElementType`
        // reaches `getIteratedTypeOrElementType` and a string-like operand
        // yields `stringType` (`checker.go`'s `isTypeAssignableToKind(…,
        // TypeFlagsStringLike)` arm), which is why
        // `for (const ch of "ab")` already reads `string` here through
        // `for_of_element_type`.
        //
        // The spread road could not reach that: its iterator seam is gated on
        // `declares_symbol_iterator`, a SYNTACTIC presence test, and a
        // primitive declares nothing — `String`'s `[Symbol.iterator]` lives in
        // the lib interface, not on the operand. So `[..."ab"]` gapped while
        // `for (const ch of "ab")` answered, the same road split two ways.
        //
        // Widened, not the literal: `[..."ab"]` is `string[]`, because the
        // elements are the string's characters and not the literal itself.
        if self.store.get(operand).flags.intersects(TypeFlags::STRING_LIKE) {
            return Some(self.intrinsics.string);
        }
        // §285: `[...new SymbolIterator]` reads the same custom-iterator seam
        // the for-of road does (§284), gated by the same syntactic presence
        // test (`iteratorSpreadInArray5`, `iteratorSpreadInCall*`).
        if self.declares_symbol_iterator(operand) {
            if let Some(element) = self.for_of_element_type(operand) {
                return Some(element);
            }
        }
        // §569: the syntactic gate above is a SUFFICIENT condition, not a
        // necessary one. `declares_symbol_iterator` asks whether the operand's
        // own declarations spell `[Symbol.iterator]`, which a LIB type never
        // does from the operand's side — a `Generator`, a `Set`, a `Map` all
        // carry it on the interface. So `for (const x of g1())` read `number`
        // while `[...g1()]` gapped: §32.1's shape, two entrances to one road
        // with only one gated correctly.
        //
        // Falling through to the same `for_of_element_type` the for-of road
        // calls is safe by construction — it answers `Option` and DECLINES
        // where it cannot decide, so this can only turn a gap into an answer
        // the for-of road already trusts. It runs BEFORE the decidable-failure
        // arm so a real element type wins over that arm's `any`.
        if let Some(element) = self.for_of_element_type(operand) {
            return Some(element);
        }
        // Iterable unions are valid only when every constituent supports the
        // protocol. The successful resolver above combines absent yields
        // before recovery; only a failed union reaches the constituent errors.
        if let crate::types::TypeData::Union { types, .. } = self.store.get(operand).data.clone() {
            let elements = types
                .into_iter()
                .map(|part| self.array_spread_element_type(part))
                .collect::<Option<Vec<_>>>()?;
            return Some(self.get_union_type(&elements));
        }
        if let Some(symbol) = self.get_property_of_type(operand, "[Symbol.iterator]") {
            if self.resolutions.on_stack(symbol, crate::resolution::PropertyName::Type) {
                return None;
            }
            if self.property_is_optional(symbol) {
                return Some(self.intrinsics.any);
            }
            if let Some(method) = self.get_type_of_property_of_type(operand, "[Symbol.iterator]")
                && !self.is_gap(method)
            {
                // `getIterationTypesOfIterableSlow` (`checker.go:6463`):
                // `IsTypeAny(methodType)` yields `anyType`, upstream's
                // `errorType` included (ADR-0048).
                if self.is_type_any(method) {
                    return Some(self.intrinsics.any);
                }
                if self.store.get(method).flags.intersects(TypeFlags::PRIMITIVE) {
                    return Some(self.intrinsics.any);
                }
                if let Some(signatures) = self.call_signatures_of_type(method) {
                    let returns: Vec<_> = signatures
                        .iter()
                        .filter(|signature| self.signature_min_argument_count(signature) == 0)
                        .map(|signature| signature.r#type)
                        .collect();
                    if returns.is_empty() {
                        return Some(self.intrinsics.any);
                    }
                    if returns.iter().all(|&t| !self.is_gap(t)) {
                        let iterator = self.get_intersection_type(&returns, None);
                        // `getIterationTypesOfIteratorWorker`
                        // (`checker.go:6496`): an any-like iterator yields
                        // `anyType`.
                        if self.is_type_any(iterator)
                            || self.iteration_methods_decidably_absent(iterator)
                        {
                            return Some(self.intrinsics.any);
                        }
                    }
                }
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
        // A complete property-only capture or declaration table proves the
        // absence of Symbol.iterator. Numeric index signatures do not supply
        // an iterator: unlike a destructuring target, an ordinary spread must
        // recover with any rather than borrow operand[number]. Unresolved
        // computed names, aliases and heritage keep the gap.
        let missing_iterator =
            self.anonymous_properties.get(&operand).filter(|(_, complete)| *complete).is_some_and(
                |(properties, _)| {
                    properties.iter().all(|property| property.name != "[Symbol.iterator]")
                },
            ) || self.declared_property_table(operand).is_some_and(|properties| {
                properties.iter().all(|(name, _)| name != "[Symbol.iterator]")
            });
        if missing_iterator
            || self
                .store
                .get(operand)
                .flags
                .intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE | TypeFlags::UNKNOWN)
        {
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
