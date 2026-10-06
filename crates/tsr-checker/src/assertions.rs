//! Type assertions: `x as T` and `<T>x`.
//!
//! Ported from `Checker.checkAssertion` (`checker.go:12287`).
//!
//! # Two unrelated rules behind one node kind
//!
//! An assertion looks like one construct and is two. `x as T` **discards the
//! operand's type entirely** and answers `getTypeFromTypeNode(T)` — the operand
//! is checked only so that the assignability diagnostic can be reported, and
//! upstream defers even that (`checkAssertionDeferred`, `checker.go:12315`).
//! `x as const` does the opposite: it has no type to resolve at all, and answers
//! `getRegularTypeOfLiteralType` of the **operand's** type.
//!
//! That is why `const` must be recognised *before* the type node is resolved.
//! Letting it resolve as a name would look for a type called `const`, find
//! nothing, and answer `errorType` on every const assertion in the corpus.
//! Upstream makes the same point from the other side: `resolveName` has a
//! special case so that the `const` in a const assertion is never resolved
//! (`utilities.go:134`).
//!
//! # The `const` arm was unreachable for 16 sessions — a two-contract seam, closed by §104
//!
//! The parser's `ConstKeyword` arm (`tsr-parser/src/types.rs:471`, its only
//! `TypeReferenceNode::new(None, ..)` site) deliberately encodes `x as const`
//! as a reference with **no name**, while [`is_const_type_reference`] demanded
//! the identifier spelling — two halves of one feature written to different
//! contracts, and every const assertion in the corpus gapped on the mismatch
//! (257 at the time of the original note; the §104 pair converted 72 in
//! `constAssertions` alone). The history above this paragraph previously
//! blamed `bd tsr-0ao` (the parser producing None **with no diagnostic**) and
//! was half right: the None encoding was later made deliberate, and this test
//! was never updated to match it. §104's slice 0 accepts None-with-no-arguments
//! as the const assertion; the once-`#[ignore]`d tests now run.
//!
//! # Nothing here checks that the assertion is legal
//!
//! Upstream's deferred half asks whether the operand is comparable to the target
//! and reports when it is not. This port has no assignability and no
//! diagnostics, so the answer is the asserted type either way — which is
//! upstream's answer too, since the diagnostic does not change the type.

use tsr_ast::{Expression, Node, NodeId, TypeNode};

use crate::{checker::Checker, types::TypeId};

impl<'a> Checker<'a, '_> {
    /// Ported from `Checker.checkAssertion` (`checker.go:12287`).
    ///
    /// Takes the node's id rather than the node, because `checkExpression`'s
    /// dispatch has a free lifetime and `getTypeFromTypeNode` needs the
    /// checker's. `NodeMap` is the way back
    /// ([ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)),
    /// and it is the same route the function-expression arm already takes.
    pub(crate) fn check_assertion(&mut self, node: NodeId) -> TypeId {
        let error = self.intrinsics.error;
        let (type_node, operand) = match self.node_map.get(node) {
            Some(Node::AsExpression(node)) => (node.r#type, node.expression),
            Some(Node::TypeAssertion(node)) => (node.r#type, node.expression),
            _ => return error,
        };
        let (Some(type_node), Some(operand)) = (type_node, operand) else {
            return error;
        };
        if is_const_type_reference(type_node) {
            return self.check_const_assertion(operand);
        }
        // The operand is checked for its diagnostics and its type is discarded.
        // Upstream checks it here and defers the comparison
        // (`checker.go:12298`); this port has neither diagnostic, so the call
        // exists only to type the operand's own baseline lines — `>x` still
        // gets a line of its own inside `x as T`.
        let _ = self.check_expression(operand);
        self.get_type_from_type_node(type_node)
    }

    /// The `const` arm of `checkAssertion` (`checker.go:12303`).
    ///
    /// `getRegularTypeOfLiteralType(exprType)` — the operand's type, made
    /// regular. `1 as const` is `1` rather than widening to `number`, which is
    /// the whole point of the form.
    ///
    /// # An object literal operand is a gap, for two independent reasons
    ///
    /// `{ a: 1 } as const` is `{ readonly a: 1; }`
    /// (`baselines/reference/submodule/conformance/es2020IntlAPIs.types:188`),
    /// and this port would answer `{ a: number; }`. Both halves of that are
    /// wrong and only one of them is obvious:
    ///
    /// 1. **The members are `readonly` and unwidened.** That does not happen
    ///    here — it happens inside `checkObjectLiteral`, which asks
    ///    `isConstContext` (`checker.go:13615`) and, when it is true, takes the
    ///    regular type instead of the widened one and sets `CheckFlagsReadonly`.
    ///    `isConstContext` recurses through enclosing parentheses, array
    ///    literals, spreads and property assignments, so a const assertion many
    ///    levels up still reaches every member.
    /// 2. **The member's printed form would use the wrong quotes.** A string
    ///    literal type prints double-quoted standing alone and **preserves the
    ///    source's quote style inside an object type**:
    ///
    ///    ```text
    ///    const options1 = { localeMatcher: 'lookup' } as const;
    ///    >options1 : { readonly localeMatcher: 'lookup'; }
    ///    >'lookup' : "lookup"
    ///    ```
    ///
    ///    The same type, two spellings, because the node builder reuses the
    ///    source type node for the member. This port normalises every string
    ///    literal to double quotes, so it would fail the line even with
    ///    `isConstContext` ported.
    ///
    /// Porting only the first would produce `{ readonly a: 1; }` for numbers and
    /// a wrong line for every string, which is worse than a gap and much harder
    /// to spot. `bd tsr-7ja` owns both halves together.
    ///
    /// An **array literal** operand IS guarded (below): array literals type
    /// fine now, and `['a'] as const` wants a readonly tuple this port cannot
    /// mint — the earlier claim that the operand's error propagates predated
    /// `check_array_literal`.
    fn check_const_assertion(&mut self, operand: Expression<'a>) -> TypeId {
        // Objects for the two documented reasons above; arrays because
        // `['a'] as const` is a READONLY TUPLE and this port has no readonly
        // tuple type yet (§104) — answering the widened array would be a
        // confident wrong where a gap belongs. Parens looked through: the
        // fired leg was `([10]) as const` slipping a direct-shape test
        // (constAssertions 0:182/0:183, 2 G→W on §104's first pair).
        let mut inner = operand;
        while let Expression::ParenthesizedExpression(node) = inner {
            match node.expression {
                Some(next) => inner = next,
                None => return self.intrinsics.error,
            }
        }
        // §105 slice 1: an ARRAY operand mints the readonly tuple of its
        // elements' regular types — `checkArrayLiteral`'s const-context
        // answer (`checker.go:8021` under `isConstContext`), reached here
        // because the assertion IS the const context. Nested arrays recurse
        // through this same arm (inheriting the paren-climb and the object
        // gate); spreads, holes, and object elements decline the operand
        // whole — readonly MEMBERS are slice 2, behind the value-spelling
        // carriage.
        if let Expression::ArrayLiteralExpression(array) = inner {
            if array.elements.iter().any(|element| {
                matches!(element, Expression::SpreadElement(_) | Expression::OmittedExpression(_))
            }) {
                return self.check_array_literal(array);
            }
            let mut elements = Vec::with_capacity(array.elements.len());
            for element in array.elements {
                let element_type = match element {
                    Expression::SpreadElement(_) | Expression::OmittedExpression(_) => {
                        return self.intrinsics.error;
                    }
                    nested @ (Expression::ArrayLiteralExpression(_)
                    | Expression::ParenthesizedExpression(_)) => {
                        self.check_const_assertion(*nested)
                    }
                    other => {
                        let checked = self.check_expression(*other);
                        self.get_regular_type_of_literal_type(checked)
                    }
                };
                if element_type == self.intrinsics.error {
                    return self.intrinsics.error;
                }
                elements.push(element_type);
            }
            // checkArrayLiteral's readonly rule (checker.go:8087): a mutable
            // array-like contextual type keeps the const tuple mutable.
            let readonly =
                !array.node_id.is_some_and(|id| self.array_literal_contextual_is_mutable(id));
            return self.create_tuple_type(elements, readonly);
        }
        let operand_type = self.check_expression(operand);
        self.get_regular_type_of_literal_type(operand_type)
    }
}

impl Checker<'_, '_> {
    /// isConstTypeVariable (internal/checker/checker.go:13645).
    pub(crate) fn is_const_type_variable(
        &mut self,
        id: crate::types::TypeId,
        depth: usize,
    ) -> bool {
        self.const_type_variable_inner(id, depth, &mut Vec::new())
    }

    fn const_type_variable_inner(
        &mut self,
        id: crate::types::TypeId,
        depth: usize,
        stack: &mut Vec<crate::types::TypeId>,
    ) -> bool {
        use crate::{flags::TypeFlags, types::TypeData};
        if depth >= 5 || stack.contains(&id) {
            return false;
        }
        stack.push(id);
        if !self.conditional_constraint_branches.contains_key(&id)
            && let Some((symbol, arguments)) = self.type_reference_targets.get(&id).cloned()
        {
            self.capture_conditional_alias_branches(id, symbol, &arguments);
        }
        let result = if let Some(symbol) = self.type_parameter_symbols.get(&id) {
            self.binder.symbols().get(*symbol).declarations.iter().any(|&declaration|
                matches!(self.node_map.get(declaration),Some(Node::TypeParameterDeclaration(parameter))
                    if parameter.modifiers.iter().any(|modifier| matches!(modifier,
                        tsr_ast::ModifierLike::Token(token) if token.kind==tsr_ast::SyntaxKind::ConstKeyword))))
        } else if let TypeData::Union { types, .. } | TypeData::Intersection { types, .. } =
            self.store.get(id).data.clone()
        {
            types.into_iter().any(|ty| self.const_type_variable_inner(ty, depth, stack))
        } else if let Some((object, _, _)) = self.deferred_indexed_access_types.get(&id).copied() {
            self.const_type_variable_inner(object, depth + 1, stack)
        } else if let Some((yes, no)) = self
            .conditional_constraint_branches
            .get(&id)
            .or_else(|| self.mapped_conditional_branches.get(&id))
            .copied()
        {
            // getDefaultConstraintOfConditionalType elides an any branch.
            (!self.store.get(yes).flags.contains(TypeFlags::ANY)
                && self.const_type_variable_inner(yes, depth + 1, stack))
                || (!self.store.get(no).flags.contains(TypeFlags::ANY)
                    && self.const_type_variable_inner(no, depth + 1, stack))
        } else if let Some(mapped) = self.mapped_types.get(&id) {
            self.deferred_keyof_operands.get(&mapped.constraint).copied().is_some_and(|source| {
                self.store.get(source).flags.contains(TypeFlags::TYPE_PARAMETER)
                    && self.const_type_variable_inner(source, depth, stack)
            })
        } else if let Some((elements, _)) = self.variadic_tuple_elements.get(&id).cloned() {
            elements.iter().any(|element| {
                element.spread && self.const_type_variable_inner(element.r#type, depth, stack)
            })
        } else {
            false
        };
        stack.pop();
        result
    }
    /// `isConstContext` (`checker.go:13615`), minus the const-type-variable
    /// arm (that is §103's inference subsystem): a node is in a const context
    /// when its parent chain reaches a const assertion through parens, array
    /// literals, spreads, property assignments (through their object literal),
    /// shorthand assignments, or template spans.
    pub(crate) fn is_const_context(&self, node: tsr_ast::NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        match self.node_map.get(parent) {
            Some(Node::AsExpression(assertion)) => {
                assertion.r#type.is_some_and(is_const_type_reference)
            }
            Some(Node::TypeAssertion(assertion)) => {
                assertion.r#type.is_some_and(is_const_type_reference)
            }
            // Upstream lists these in two groups — the expression positions and
            // the member/span positions — and both recur into the parent, so
            // they are one arm here.
            Some(
                Node::ParenthesizedExpression(_)
                | Node::ArrayLiteralExpression(_)
                // A node whose parent is the OBJECT LITERAL is a member;
                // upstream never sees this shape because it always asks from
                // the member's INITIALIZER (whose parent is the assignment),
                // while this port also asks from the assignment node itself —
                // the §105 slice-2a fired leg (ts-expect-error 8: the arm was
                // reached with const_context=false, PA-to-object unhandled).
                | Node::ObjectLiteralExpression(_)
                | Node::SpreadElement(_)
                | Node::SpreadAssignment(_)
                | Node::PropertyAssignment(_)
                | Node::ShorthandPropertyAssignment(_),
            ) => self.is_const_context(parent),
            Some(Node::TemplateSpan(_)) => self
                .nodes
                .parent(parent)
                .is_some_and(|template| self.is_const_context(template)),
            _ => false,
        }
    }
}

/// `isConstTypeReference` (`utilities.go:128`).
///
/// A bare `const` in type position: a type reference, no type arguments, whose
/// name is the identifier `const`. The arity test is upstream's and is not
/// decoration — `const<T>` is a reference to a type *named* `const`, which is a
/// legal if perverse declaration, and treating it as a const assertion would
/// silently answer the operand's type.
pub(crate) fn is_const_type_reference(node: TypeNode<'_>) -> bool {
    let TypeNode::TypeReferenceNode(reference) = node else { return false };
    if !reference.type_arguments.is_empty() {
        return false;
    }
    match reference.type_name {
        Some(tsr_ast::EntityName::Identifier(name)) => name.text == "const",
        // §104's slice 0: the parser's ConstKeyword arm encodes `as const`
        // as a reference with NO name (`tsr-parser/src/types.rs:471`, its
        // only None-named creation site), while this test demanded the
        // identifier spelling — the two halves of one feature written to
        // different contracts, and every `as const` in the corpus gapped
        // on the mismatch. A None name with no arguments IS the const
        // assertion.
        None => true,
        _ => false,
    }
}
