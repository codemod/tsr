//! TS2872 / TS2873 — `This kind of expression is always truthy.` and
//! `… always falsy.` — plus TS1345, the `void` early return they share.
//!
//! `checkTruthinessOfType` (`checker.go:12865`) and
//! `getSyntacticTruthySemantics` (`checker.go:12886`).
//!
//! # There are no types in this rule
//!
//! Apart from the `void` early return and one identifier case, the predicate
//! reads **node kinds and literal text**. §14's ordering rule — a rule that
//! reports on a syntactic fact has no incompleteness to leak — applies here
//! more completely than to any rule since TS2369.
//!
//! # The site list *is* the rule
//!
//! Because the predicate is context-free, everything this rule gets right or
//! wrong is the set of positions it is asked about. Upstream calls
//! `checkTruthinessExpression` at seven: `if`, `do`, `while`, a `for`
//! statement's condition, a conditional expression's condition, the operand of
//! `!`, and the **left** operand of `&&` or `||` — the last only for
//! `IsLogicalBinaryOperator`, so `??` is excluded (`checker.go:12356`).
//!
//! `docs/architecture/checker-notes-diag2.md` §47.

use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_diagnostics::{Diagnostic, messages};

use tsr_binder::SymbolId;

use crate::{checker::Checker, flags::TypeFlags, types::TypeId};

/// `PredicateSemantics` (`checker.go:12877`), a two-bit lattice: the union of
/// `Always` and `Never` is `Sometimes`, which is what makes `c ? 1 : 0` silent.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct PredicateSemantics {
    always: bool,
    never: bool,
}

impl PredicateSemantics {
    const ALWAYS: Self = Self { always: true, never: false };
    const NEVER: Self = Self { always: false, never: true };
    const SOMETIMES: Self = Self { always: true, never: true };

    fn union(self, other: Self) -> Self {
        Self { always: self.always || other.always, never: self.never || other.never }
    }
}

impl Checker<'_, '_> {
    /// Every truthiness-tested position under `node`, dispatched from the
    /// check traversal.
    pub(crate) fn check_truthiness_sites(&mut self, node: NodeId, ambient: bool) {
        if ambient || self.file_has_parse_errors || self.in_js_file(node) {
            return;
        }
        self.check_known_truthy_sites(node);
        let tested = match self.node_map.get(node) {
            Some(Node::IfStatement(statement)) => statement.expression,
            Some(Node::WhileStatement(statement)) => statement.expression,
            Some(Node::DoStatement(statement)) => statement.expression,
            Some(Node::ForStatement(statement)) => statement.condition,
            Some(Node::ConditionalExpression(conditional)) => conditional.condition,
            Some(Node::PrefixUnaryExpression(unary))
                if unary.operator.kind == SyntaxKind::ExclamationToken =>
            {
                unary.operand
            }
            // `IsLogicalBinaryOperator` (`checker.go:12355`) — `&&` and `||`
            // and **not** `??`, whose left operand is tested for nullishness
            // by a different check with different messages.
            Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|token| {
                    matches!(
                        token.kind,
                        SyntaxKind::BarBarToken | SyntaxKind::AmpersandAmpersandToken
                    )
                }) =>
            {
                binary.left
            }
            _ => return,
        };
        let Some(tested) = tested else { return };
        self.check_truthiness_of(tested);
    }

    /// The three callers of `checkTestingKnownTruthyCallableOrAwaitableOrEnumMemberType`
    /// (`checker.go:3814`): `checkIfStatement` (body: the `then` branch,
    /// `checker.go:3806`), `checkConditionalExpression` (body: `whenTrue`,
    /// `checker.go:10937`), and `checkBinaryLikeExpression` for the left
    /// operand of `&&`, `||` and `??` (`checker.go:12345`) — always for `&&`,
    /// and for `||`/`??` only when the enclosing chain is an `if` condition.
    fn check_known_truthy_sites(&mut self, node: NodeId) {
        if !self.strict_null_checks {
            return;
        }
        let (condition, body) = match self.node_map.get(node) {
            Some(Node::IfStatement(statement)) => {
                (statement.expression, statement.then_statement.and_then(|b| b.node_id()))
            }
            Some(Node::ConditionalExpression(conditional)) => {
                (conditional.condition, conditional.when_true.and_then(|b| b.node_id()))
            }
            Some(Node::BinaryExpression(binary)) => {
                let Some(operator) = binary.operator_token.map(|token| token.kind) else {
                    return;
                };
                if !is_logical_or_coalescing(operator) {
                    return;
                }
                let mut parent = self.nodes.parent(node);
                while let Some(id) = parent {
                    let climb = match self.node_map.get(id) {
                        Some(Node::ParenthesizedExpression(_)) => true,
                        Some(Node::BinaryExpression(outer)) => outer
                            .operator_token
                            .is_some_and(|token| is_logical_or_coalescing(token.kind)),
                        _ => false,
                    };
                    if !climb {
                        break;
                    }
                    parent = self.nodes.parent(id);
                }
                let if_body = parent.and_then(|id| match self.node_map.get(id) {
                    Some(Node::IfStatement(statement)) => Some(statement.then_statement),
                    _ => None,
                });
                if operator != SyntaxKind::AmpersandAmpersandToken && if_body.is_none() {
                    return;
                }
                (binary.left, if_body.flatten().and_then(|b| b.node_id()))
            }
            _ => return,
        };
        let Some(condition) = condition.and_then(|c| c.node_id()) else { return };
        let condition_type = self.check_expression_at_node(condition);
        self.check_testing_known_truthy_types(condition, condition_type, body);
    }

    /// `checkTestingKnownTruthyTypes` (`checker.go:3821`).
    fn check_testing_known_truthy_types(
        &mut self,
        condition: NodeId,
        condition_type: TypeId,
        body: Option<NodeId>,
    ) {
        let mut condition = self.skip_parentheses_id(condition);
        self.check_testing_known_truthy_type(condition, condition_type, body);
        for _ in 0..256 {
            let Some(Node::BinaryExpression(binary)) = self.node_map.get(condition) else { return };
            if !binary.operator_token.is_some_and(|token| {
                matches!(token.kind, SyntaxKind::BarBarToken | SyntaxKind::QuestionQuestionToken)
            }) {
                return;
            }
            let Some(left) = binary.left.and_then(|left| left.node_id()) else { return };
            condition = self.skip_parentheses_id(left);
            self.check_testing_known_truthy_type(condition, condition_type, body);
        }
    }

    /// TS2774 / TS2801 / TS2845 — `checkTestingKnownTruthyType`
    /// (`checker.go:3830`).
    ///
    /// Upstream's own heuristic bounds the rule, and its comment is the
    /// justification: *"we de-scope to functions and Promises unreferenced in
    /// the block … there are too many false positives otherwise."* Two
    /// declines are this port's: a type whose call signatures or promised
    /// type cannot be read answers nothing, and a tested name this port cannot
    /// resolve to a symbol is treated as no symbol (upstream's `nil`).
    fn check_testing_known_truthy_type(
        &mut self,
        condition: NodeId,
        condition_type: TypeId,
        body: Option<NodeId>,
    ) {
        let mut location = condition;
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(condition)
            && binary.operator_token.is_some_and(|token| is_logical_or_coalescing(token.kind))
            && let Some(right) = binary.right.and_then(|right| right.node_id())
        {
            location = self.skip_parentheses_id(right);
        }
        if let Some(Node::BinaryExpression(binary)) = self.node_map.get(location)
            && binary.operator_token.is_some_and(|token| is_logical_or_coalescing(token.kind))
        {
            self.check_testing_known_truthy_types(location, condition_type, body);
            return;
        }
        let tested_type = if location == condition {
            condition_type
        } else {
            self.check_expression_at_node(location)
        };
        let access = match self.node_map.get(location) {
            Some(Node::PropertyAccessExpression(access)) => Some(access),
            _ => None,
        };
        // The enum-member arm: `E.A` in a condition is always one of truthy
        // or falsy.
        if self.type_of(tested_type).flags.intersects(TypeFlags::ENUM_LITERAL)
            && let Some(access) = access
            && let Some(receiver) = access.expression.and_then(|receiver| receiver.node_id())
            && self.receiver_symbol_flags(receiver).intersects(tsr_binder::SymbolFlags::ENUM)
        {
            let truthy = match self.enum_member_value(tested_type) {
                Some((_, key)) => !matches!(key.as_str(), "s:" | "n:0" | "n:-0"),
                None => return,
            };
            self.report_known_truthy(
                location,
                &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_0,
                vec![if truthy { "true" } else { "false" }.to_string()],
            );
            return;
        }
        let is_property_expression_cast = access
            .and_then(|access| access.expression)
            .and_then(|receiver| receiver.node_id())
            .is_some_and(|receiver| {
                let receiver = self.skip_parentheses_id(receiver);
                matches!(
                    self.node_map.get(receiver),
                    Some(Node::TypeAssertion(_) | Node::AsExpression(_))
                )
            });
        if is_property_expression_cast || self.all_constituents_primitive(tested_type) {
            return;
        }
        if !self.get_type_facts(tested_type).intersects(crate::flow::TypeFacts::TRUTHY) {
            return;
        }
        let Some(call_signatures) = self.call_signatures_of_type(tested_type) else { return };
        let Some(is_promise) = self.has_awaited_type_of_promise(tested_type) else { return };
        if call_signatures.is_empty() && !is_promise {
            return;
        }
        let tested_node = match self.node_map.get(location) {
            Some(Node::Identifier(_)) => Some(location),
            _ => access
                .and_then(|access| access.name.and_then(|name| name.node_id()))
                .filter(|&name| matches!(self.node_map.get(name), Some(Node::Identifier(_)))),
        };
        let tested_symbol = tested_node.and_then(|node| self.known_truthy_symbol_at(node));
        if tested_symbol.is_none() && !is_promise {
            return;
        }
        let is_used = match (tested_symbol, tested_node) {
            (Some(symbol), Some(tested_node)) => {
                self.nodes.parent(condition).is_some_and(|parent| {
                    self.is_symbol_used_in_binary_expression_chain(parent, symbol)
                }) || body.is_some_and(|body| {
                    self.is_symbol_used_in_condition_body(condition, body, tested_node, symbol)
                })
            }
            _ => false,
        };
        if is_used {
            return;
        }
        if is_promise {
            let text = self.type_to_string(tested_type);
            self.report_known_truthy(
                location,
                &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_0_IS_ALWAYS_DEFINED,
                vec![text],
            );
        } else {
            self.report_known_truthy(
                location,
                &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_FUNCTION_IS_ALWAYS_DEFINED_DID_YOU_MEAN_TO_CALL_IT_INSTEAD,
                Vec::new(),
            );
        }
    }

    /// Report once per node and code. The `if` arm and the `||` arm can both
    /// reach the same operand of `if (a || b)`; upstream's diagnostic
    /// collection drops the identical second report.
    fn report_known_truthy(
        &mut self,
        at: NodeId,
        message: &'static tsr_diagnostics::Message,
        args: Vec<String>,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        let span = self.error_span(at);
        if self.diagnostics().iter().any(|(reported_file, diagnostic)| {
            *reported_file == file
                && diagnostic.span == span
                && diagnostic.message.code() == message.code()
        }) {
            return;
        }
        self.report(file, Diagnostic::with_args(message, span, args));
    }

    /// `getAwaitedTypeOfPromise(t) != nil` (`checker.go:31458`) — whether `t`
    /// has a promised type (`getPromisedTypeOfPromiseEx`, `checker.go:28926`).
    /// `None` where a step cannot be read, including `then` signatures with a
    /// `this` parameter, whose subtype filter is not ported here.
    fn has_awaited_type_of_promise(&mut self, t: TypeId) -> Option<bool> {
        if self.type_of(t).flags.intersects(TypeFlags::ANY) {
            return Some(false);
        }
        if let Some((target, arguments)) = self.type_reference_targets.get(&t).cloned()
            && arguments.len() == 1
            && self.global_type_symbol_with_arity("Promise", 1).is_some_and(|promise| {
                self.binder.merged_symbol(promise) == self.binder.merged_symbol(target)
            })
        {
            return Some(true);
        }
        // Primitives with a `then` member are not unwrapped. Only a type the
        // flags already decide is answered; a generic constraint is not read.
        if self.all_constituents_primitive(t) {
            return Some(false);
        }
        if self.type_of(t).flags.intersects(TypeFlags::INSTANTIABLE) {
            return None;
        }
        let Some(then) = self.get_type_of_property_of_type(t, "then") else { return Some(false) };
        if then == self.intrinsics.error {
            return None;
        }
        if self.type_of(then).flags.intersects(TypeFlags::ANY) {
            return Some(false);
        }
        let signatures = self.call_signatures_of_type(then)?;
        if signatures.is_empty() {
            return Some(false);
        }
        let mut callbacks = Vec::new();
        for signature in &signatures {
            if signature
                .this_parameter
                .as_ref()
                .is_some_and(|this| this.r#type != self.intrinsics.void)
            {
                return None;
            }
            callbacks.push(
                self.signature_type_at_position(signature, 0).unwrap_or(self.intrinsics.never),
            );
        }
        if callbacks.contains(&self.intrinsics.error) {
            return None;
        }
        let callbacks = self.get_union_type(&callbacks);
        let callbacks =
            self.get_type_with_facts(callbacks, crate::flow::TypeFacts::NE_UNDEFINED_OR_NULL);
        if self.type_of(callbacks).flags.intersects(TypeFlags::ANY) {
            return Some(false);
        }
        Some(!self.call_signatures_of_type(callbacks)?.is_empty())
    }

    /// Every constituent is a primitive (or `never`). Such a type has no call
    /// signatures — a union's are the intersection of its members' — and no
    /// promised type, so the rule's answer is already known. Asked **before**
    /// the signature and `then` lookups: those resolve the apparent type's
    /// members, and every `if`, `?:` and `&&` in a program reaches this rule,
    /// almost all of them on `boolean` or a primitive union. Measured on
    /// `domain-model`: without this order the rule cost ~3% wall
    /// (`docs/parity/notes/flow.md` §5).
    fn all_constituents_primitive(&self, t: TypeId) -> bool {
        let primitive = TypeFlags::PRIMITIVE | TypeFlags::NEVER;
        match &self.type_of(t).data {
            crate::types::TypeData::Union { types, .. } => {
                types.iter().all(|&part| self.type_of(part).flags.intersects(primitive))
            }
            _ => self.type_of(t).flags.intersects(primitive),
        }
    }

    /// `getSymbolAtLocation(identifier, false)` for the two positions this rule
    /// compares: a property-access **name** is the accessed property, and any
    /// other identifier is the value it resolves to. Merged, so identity is
    /// comparable.
    fn known_truthy_symbol_at(&mut self, identifier: NodeId) -> Option<SymbolId> {
        let Some(Node::Identifier(name)) = self.node_map.get(identifier) else { return None };
        let text = name.text;
        if let Some(parent) = self.nodes.parent(identifier)
            && let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(parent)
            && access.name.and_then(|name| name.node_id()) == Some(identifier)
        {
            let receiver = access.expression?.node_id()?;
            let receiver_type = self.check_expression_at_node(receiver);
            let receiver_type = self.get_non_nullable_type(receiver_type);
            let receiver_type = self.apparent_type(receiver_type);
            let property = self.get_property_of_type(receiver_type, text)?;
            return Some(self.binder.merged_symbol(property));
        }
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            identifier,
            text,
            tsr_binder::SymbolFlags::VALUE,
        )?;
        Some(self.binder.merged_symbol(symbol))
    }

    /// `getResolvedSymbolOrNil(location.Expression()).Flags` for the enum arm:
    /// an identifier or a property access, aliases followed (`import E =
    /// My.E` then `E.A`). Empty flags where nothing resolves.
    fn receiver_symbol_flags(&mut self, receiver: NodeId) -> tsr_binder::SymbolFlags {
        let name = match self.node_map.get(receiver) {
            Some(Node::Identifier(_)) => Some(receiver),
            Some(Node::PropertyAccessExpression(access)) => {
                access.name.and_then(|name| name.node_id())
            }
            _ => None,
        };
        let Some(symbol) = name.and_then(|name| self.known_truthy_symbol_at(name)) else {
            return tsr_binder::SymbolFlags::empty();
        };
        let symbol = self.resolve_alias(symbol).unwrap_or(symbol);
        self.binder.symbols().get(self.binder.merged_symbol(symbol)).flags
    }

    /// `isSymbolUsedInBinaryExpressionChain` (`checker.go:3897`): the right
    /// operands of the enclosing `&&` chain — their **children**, as upstream
    /// visits them.
    fn is_symbol_used_in_binary_expression_chain(
        &mut self,
        node: NodeId,
        symbol: SymbolId,
    ) -> bool {
        let mut node = node;
        for _ in 0..256 {
            let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else {
                return false;
            };
            if binary
                .operator_token
                .is_none_or(|token| token.kind != SyntaxKind::AmpersandAmpersandToken)
            {
                return false;
            }
            if let Some(right) = binary.right.and_then(|right| right.node_id())
                && self.children_mention_symbol(right, symbol)
            {
                return true;
            }
            let Some(parent) = self.nodes.parent(node) else { return false };
            node = parent;
        }
        false
    }

    /// `ForEachChild(visit)` with upstream's identifier test: `node` itself is
    /// not visited, only what is under it.
    fn children_mention_symbol(&mut self, node: NodeId, symbol: SymbolId) -> bool {
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| {
            (matches!(self.node_map.get(child), Some(Node::Identifier(_)))
                && self.known_truthy_symbol_at(child) == Some(symbol))
                || self.children_mention_symbol(child, symbol)
        })
    }

    /// `isSymbolUsedInConditionBody` (`checker.go:3915`).
    fn is_symbol_used_in_condition_body(
        &mut self,
        condition: NodeId,
        body: NodeId,
        tested_node: NodeId,
        symbol: SymbolId,
    ) -> bool {
        let condition_is_identifier =
            matches!(self.node_map.get(condition), Some(Node::Identifier(_)));
        let tested_parent_is_binary = self.nodes.parent(tested_node).is_some_and(|parent| {
            matches!(self.node_map.get(parent), Some(Node::BinaryExpression(_)))
        });
        let mut stack = Vec::new();
        if let Some(typed) = self.node_map.get(body) {
            tsr_ast::for_each_child_id(typed, |child| stack.push(child));
        }
        stack.reverse();
        while let Some(child) = stack.pop() {
            if matches!(self.node_map.get(child), Some(Node::Identifier(_)))
                && self.known_truthy_symbol_at(child) == Some(symbol)
            {
                if condition_is_identifier || tested_parent_is_binary {
                    return true;
                }
                if let Some(same) = self.same_access_target(tested_node, child) {
                    if same {
                        return true;
                    }
                    // Upstream's `visit` answers `false` here and moves on to
                    // the next sibling: the identifier's own children are
                    // not visited.
                    continue;
                }
            }
            let mut children = Vec::new();
            if let Some(typed) = self.node_map.get(child) {
                tsr_ast::for_each_child_id(typed, |grandchild| children.push(grandchild));
            }
            children.reverse();
            stack.extend(children);
        }
        false
    }

    /// The receiver walk inside `isSymbolUsedInConditionBody`: `Some(answer)`
    /// where upstream's loop returns, `None` where it runs off the tree and
    /// falls through to visiting children.
    fn same_access_target(&mut self, tested: NodeId, child: NodeId) -> Option<bool> {
        let mut tested = self.nodes.parent(tested);
        let mut child = self.nodes.parent(child);
        for _ in 0..256 {
            let (Some(t), Some(c)) = (tested, child) else { return None };
            match (self.node_map.get(t), self.node_map.get(c)) {
                (Some(Node::Identifier(_)), Some(Node::Identifier(_))) => {
                    return Some(self.known_truthy_symbol_at(t) == self.known_truthy_symbol_at(c));
                }
                _ if self.nodes.kind(t) == SyntaxKind::ThisKeyword
                    && self.nodes.kind(c) == SyntaxKind::ThisKeyword =>
                {
                    return Some(true);
                }
                (
                    Some(Node::PropertyAccessExpression(tested_access)),
                    Some(Node::PropertyAccessExpression(child_access)),
                ) => {
                    let tested_name = tested_access.name.and_then(|name| name.node_id());
                    let child_name = child_access.name.and_then(|name| name.node_id());
                    let next = (
                        tested_access.expression.and_then(|e| e.node_id()),
                        child_access.expression.and_then(|e| e.node_id()),
                    );
                    let (Some(tested_name), Some(child_name)) = (tested_name, child_name) else {
                        return Some(false);
                    };
                    if self.known_truthy_symbol_at(tested_name)
                        != self.known_truthy_symbol_at(child_name)
                    {
                        return Some(false);
                    }
                    (tested, child) = next;
                }
                (
                    Some(Node::CallExpression(tested_call)),
                    Some(Node::CallExpression(child_call)),
                ) => {
                    (tested, child) = (
                        tested_call.expression.and_then(|e| e.node_id()),
                        child_call.expression.and_then(|e| e.node_id()),
                    );
                }
                _ => return Some(false),
            }
        }
        None
    }

    fn skip_parentheses_id(&self, node: NodeId) -> NodeId {
        let mut current = node;
        for _ in 0..256 {
            match self.node_map.get(current) {
                Some(Node::ParenthesizedExpression(wrapper)) => {
                    match wrapper.expression.and_then(|e| e.node_id()) {
                        Some(inner) => current = inner,
                        None => return current,
                    }
                }
                _ => return current,
            }
        }
        current
    }

    /// `checkTruthinessOfType` (`checker.go:12865`), against the expression's
    /// own node — which is also the error node.
    fn check_truthiness_of(&mut self, expression: Expression<'_>) {
        let Some(node) = expression.node_id() else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        let tested = self.check_expression(expression);
        if self.type_of(tested).flags.intersects(TypeFlags::VOID) {
            self.report(
                file,
                Diagnostic::new(
                    &messages::AN_EXPRESSION_OF_TYPE_VOID_CANNOT_BE_TESTED_FOR_TRUTHINESS,
                    span,
                ),
            );
            return;
        }
        let semantics = self.syntactic_truthy_semantics(node, 0);
        if semantics == PredicateSemantics::SOMETIMES {
            return;
        }
        let message = if semantics.always {
            &messages::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_TRUTHY
        } else {
            &messages::THIS_KIND_OF_EXPRESSION_IS_ALWAYS_FALSY
        };
        self.report(file, Diagnostic::new(message, span));
    }

    /// `getSyntacticTruthySemantics` (`checker.go:12886`).
    fn syntactic_truthy_semantics(&mut self, node: NodeId, depth: u32) -> PredicateSemantics {
        if depth > 64 {
            return PredicateSemantics::SOMETIMES;
        }
        let node = self.skip_outer_expressions(node);
        match self.node_map.get(node) {
            // `while (0)` and `while (1)` are allowed deliberately.
            Some(Node::NumericLiteral(literal)) => {
                if numeric_literal_is_zero_or_one(literal.text) {
                    PredicateSemantics::SOMETIMES
                } else {
                    PredicateSemantics::ALWAYS
                }
            }
            Some(
                Node::ArrayLiteralExpression(_)
                | Node::ArrowFunction(_)
                | Node::BigIntLiteral(_)
                | Node::ClassExpression(_)
                | Node::FunctionExpression(_)
                | Node::JsxElement(_)
                | Node::JsxSelfClosingElement(_)
                | Node::ObjectLiteralExpression(_)
                | Node::RegularExpressionLiteral(_),
            ) => PredicateSemantics::ALWAYS,
            Some(Node::VoidExpression(_)) => PredicateSemantics::NEVER,
            Some(Node::StringLiteral(literal)) => {
                if literal.text.is_empty() {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::ALWAYS
                }
            }
            Some(Node::NoSubstitutionTemplateLiteral(literal)) => {
                if literal.text.is_empty() {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::ALWAYS
                }
            }
            Some(Node::ConditionalExpression(conditional)) => {
                let branch = |branch: Option<Expression<'_>>| branch.and_then(|e| e.node_id());
                let (Some(when_true), Some(when_false)) =
                    (branch(conditional.when_true), branch(conditional.when_false))
                else {
                    return PredicateSemantics::SOMETIMES;
                };
                self.syntactic_truthy_semantics(when_true, depth + 1)
                    .union(self.syntactic_truthy_semantics(when_false, depth + 1))
            }
            // `getResolvedSymbol(node) == c.undefinedSymbol`: the synthesised
            // global the binder seeds (`declare_synthesised_globals`), so a
            // local or parameter named `undefined` shadowing it is `Sometimes`.
            // This arm used to test for *no* resolution, which never held once
            // the global was seeded — every `undefined && x` was silent.
            Some(Node::Identifier(identifier)) if identifier.text == "undefined" => {
                let resolved = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    node,
                    identifier.text,
                    tsr_binder::SymbolFlags::VALUE,
                );
                if resolved.is_some() && resolved == self.binder.undefined_symbol() {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::SOMETIMES
                }
            }
            _ => {
                if self.nodes.kind(node) == SyntaxKind::NullKeyword {
                    PredicateSemantics::NEVER
                } else {
                    PredicateSemantics::SOMETIMES
                }
            }
        }
    }

    /// `ast.SkipOuterExpressions(node, ast.OEKAll)` — parentheses, the two
    /// assertion spellings, `satisfies`, and `!`-assertions all pass the
    /// question through to what they wrap.
    pub(crate) fn skip_outer_expressions(&self, node: NodeId) -> NodeId {
        let mut current = node;
        for _ in 0..64 {
            let inner = match self.node_map.get(current) {
                Some(Node::ParenthesizedExpression(wrapper)) => wrapper.expression,
                Some(Node::AsExpression(wrapper)) => wrapper.expression,
                Some(Node::TypeAssertion(wrapper)) => wrapper.expression,
                Some(Node::SatisfiesExpression(wrapper)) => wrapper.expression,
                Some(Node::NonNullExpression(wrapper)) => wrapper.expression,
                Some(Node::PartiallyEmittedExpression(wrapper)) => wrapper.expression,
                _ => return current,
            };
            let Some(inner) = inner.and_then(|expression| expression.node_id()) else {
                return current;
            };
            current = inner;
        }
        current
    }
}

/// Upstream compares `node.Text()` against `"0"` and `"1"`, and a numeric
/// literal's `Text` is the scanner's **normalised** value rather than the
/// source spelling: `0.0`, `0x0` and `0e5` all read `"0"`.
///
/// This port keeps the written text on the node, so the comparison is done on
/// the value instead. `ifDoWhileStatements`' `if (0.0) { }` was three wrong
/// lines before this — `checker-notes-diag2.md` §47.
fn numeric_literal_is_zero_or_one(text: &str) -> bool {
    let bare = text.replace('_', "");
    match bare.get(..2).map(str::to_ascii_lowercase).as_deref() {
        Some("0x") => matches!(u128::from_str_radix(&bare[2..], 16), Ok(0 | 1)),
        Some("0o") => matches!(u128::from_str_radix(&bare[2..], 8), Ok(0 | 1)),
        Some("0b") => matches!(u128::from_str_radix(&bare[2..], 2), Ok(0 | 1)),
        // The decimal form is normalised the way the scanner would: format the
        // parsed value back out and compare the text, which is upstream's
        // comparison exactly and avoids an equality test on a float.
        _ => {
            matches!(bare.parse::<f64>().map(|value| format!("{value}")).as_deref(), Ok("0" | "1"))
        }
    }
}

/// `ast.IsLogicalOrCoalescingBinaryOperator`: `&&`, `||` and `??`.
fn is_logical_or_coalescing(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::AmpersandAmpersandToken
            | SyntaxKind::BarBarToken
            | SyntaxKind::QuestionQuestionToken
    )
}
