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
use tsr_binder::SymbolId;
use tsr_diagnostics::{Diagnostic, messages};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{EnumLiteralValue, TypeData, TypeId},
};

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
        self.check_testing_known_truthy_site(node);
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
    /// (`checker.go:3814`): `checkIfStatement` with the `then` statement,
    /// `checkConditionalExpression` with `whenTrue`, and
    /// `checkBinaryLikeExpression`'s logical arm (`checker.go:12340`) with the
    /// left operand — for `&&` always, for `||`/`??` only when the enclosing
    /// (paren/logical-skipped) parent is an `if`, whose `then` is the body.
    fn check_testing_known_truthy_site(&mut self, node: NodeId) {
        let (condition, body) = match self.node_map.get(node) {
            Some(Node::IfStatement(statement)) => (
                statement.expression.and_then(|e| e.node_id()),
                statement.then_statement.and_then(|s| s.node_id()),
            ),
            Some(Node::ConditionalExpression(conditional)) => (
                conditional.condition.and_then(|e| e.node_id()),
                conditional.when_true.and_then(|e| e.node_id()),
            ),
            Some(Node::BinaryExpression(binary)) => {
                let Some(operator) = binary.operator_token.map(|token| token.kind) else {
                    return;
                };
                if !matches!(
                    operator,
                    SyntaxKind::AmpersandAmpersandToken
                        | SyntaxKind::BarBarToken
                        | SyntaxKind::QuestionQuestionToken
                ) {
                    return;
                }
                let mut parent = self.nodes.parent(node);
                while let Some(id) = parent
                    && (self.nodes.kind(id) == SyntaxKind::ParenthesizedExpression
                        || self.is_logical_or_coalescing_binary(id))
                {
                    parent = self.nodes.parent(id);
                }
                let enclosing_if = parent.and_then(|id| match self.node_map.get(id) {
                    Some(Node::IfStatement(statement)) => Some(statement),
                    _ => None,
                });
                if operator != SyntaxKind::AmpersandAmpersandToken && enclosing_if.is_none() {
                    return;
                }
                (
                    binary.left.and_then(|e| e.node_id()),
                    enclosing_if.and_then(|s| s.then_statement).and_then(|s| s.node_id()),
                )
            }
            _ => return,
        };
        let Some(condition) = condition else { return };
        if !self.strict_null_checks {
            return;
        }
        let condition_type = self.check_expression_at_node(condition);
        self.check_testing_known_truthy_types(condition, condition_type, body);
    }

    /// `checkTestingKnownTruthyTypes` (`checker.go:3821`): the condition, then
    /// every left operand down a `||`/`??` chain.
    fn check_testing_known_truthy_types(
        &mut self,
        condition: NodeId,
        condition_type: TypeId,
        body: Option<NodeId>,
    ) {
        let mut condition = self.skip_parentheses_for_truthiness(condition);
        self.check_testing_known_truthy_type(condition, condition_type, body);
        while let Some(Node::BinaryExpression(binary)) = self.node_map.get(condition)
            && binary.operator_token.is_some_and(|token| {
                matches!(token.kind, SyntaxKind::BarBarToken | SyntaxKind::QuestionQuestionToken)
            })
            && let Some(left) = binary.left.and_then(|e| e.node_id())
        {
            condition = self.skip_parentheses_for_truthiness(left);
            self.check_testing_known_truthy_type(condition, condition_type, body);
        }
    }

    /// `checkTestingKnownTruthyType` (`checker.go:3830`).
    ///
    /// TS2774 — `This condition will always return true since this function is
    /// always defined. Did you mean to call it instead?` — for a tested value
    /// with call signatures whose symbol the condition's `&&` chain and body
    /// never mention, and TS2845 for an enum member read off its enum.
    ///
    /// The `isPromise` arm (TS2801) needs `getAwaitedTypeOfPromise`, which this
    /// port does not expose; it is treated as `false`, which can only omit a
    /// diagnostic.
    fn check_testing_known_truthy_type(
        &mut self,
        condition: NodeId,
        condition_type: TypeId,
        body: Option<NodeId>,
    ) {
        let location = if self.is_logical_or_coalescing_binary(condition) {
            let Some(Node::BinaryExpression(binary)) = self.node_map.get(condition) else {
                return;
            };
            let Some(right) = binary.right.and_then(|e| e.node_id()) else { return };
            self.skip_parentheses_for_truthiness(right)
        } else {
            condition
        };
        if self.is_module_exports_access(location) {
            return;
        }
        if self.is_logical_or_coalescing_binary(location) {
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
        if self.type_of(tested_type).flags.contains(TypeFlags::ENUM_LITERAL)
            && let Some(access) = access
            && let Some(receiver) = access.expression.and_then(|e| e.node_id())
            && self.resolved_symbol_flags(receiver).intersects(tsr_binder::SymbolFlags::ENUM)
        {
            if let TypeData::EnumLiteral { value, .. } = &self.type_of(tested_type).data {
                let truthy = match value {
                    EnumLiteralValue::String(text) => !text.is_empty(),
                    EnumLiteralValue::Number(text) => !matches!(text.as_str(), "0" | "-0" | "NaN"),
                };
                self.report_known_truthy(
                    location,
                    Diagnostic::with_args(
                        &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_0,
                        self.error_span(location),
                        [if truthy { "true" } else { "false" }.to_string()],
                    ),
                );
            }
            return;
        }
        let is_property_expression_cast = access
            .and_then(|access| access.expression.and_then(|e| e.node_id()))
            .is_some_and(|receiver| {
                matches!(
                    self.nodes.kind(self.skip_parentheses_for_truthiness(receiver)),
                    SyntaxKind::TypeAssertionExpression | SyntaxKind::AsExpression
                )
            });
        if is_property_expression_cast
            || !self.get_type_facts(tested_type).contains(crate::flow::TypeFacts::TRUTHY)
        {
            return;
        }
        let has_call_signatures = self
            .signatures_of_type_kind(tested_type, crate::signatures::SignatureKind::Call)
            .is_some_and(|signatures| !signatures.is_empty());
        if !has_call_signatures {
            return;
        }
        let tested_node = match self.node_map.get(location) {
            Some(Node::Identifier(_)) => Some(location),
            Some(Node::PropertyAccessExpression(access)) => access.name.and_then(|n| n.node_id()),
            _ => None,
        };
        let Some(tested_node) = tested_node else { return };
        let Some(tested_symbol) = self.symbol_at_location_for_truthiness(tested_node) else {
            return;
        };
        let used = self.nodes.parent(condition).is_some_and(|parent| {
            self.symbol_used_in_binary_expression_chain(parent, tested_symbol)
        }) || body.is_some_and(|body| {
            self.symbol_used_in_condition_body(condition, body, tested_node, tested_symbol)
        });
        if used {
            return;
        }
        let span = self.error_span(location);
        self.report_known_truthy(
            location,
            Diagnostic::new(
                &messages::THIS_CONDITION_WILL_ALWAYS_RETURN_TRUE_SINCE_THIS_FUNCTION_IS_ALWAYS_DEFINED_DID_YOU_MEAN_TO_CALL_IT_INSTEAD,
                span,
            ),
        );
    }

    /// Report once per position and message. Two of the three callers can
    /// test the same operand (`if (a || b)` reaches `a` through
    /// `checkIfStatement` and through the `||` arm), and upstream's
    /// `SortAndDeduplicateDiagnostics` collapses the identical pair.
    fn report_known_truthy(&mut self, at: NodeId, diagnostic: Diagnostic) {
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return };
        if self.diagnostics.iter().rev().any(|(seen_file, seen)| {
            *seen_file == file
                && seen.message.code() == diagnostic.message.code()
                && seen.span == diagnostic.span
                && seen.args == diagnostic.args
        }) {
            return;
        }
        self.report(file, diagnostic);
    }

    /// `isSymbolUsedInBinaryExpressionChain` (`checker.go:3890`): up an `&&`
    /// chain, does any right operand's subtree (children only) name it?
    fn symbol_used_in_binary_expression_chain(&mut self, node: NodeId, tested: SymbolId) -> bool {
        let mut node = node;
        loop {
            let Some(Node::BinaryExpression(binary)) = self.node_map.get(node) else {
                return false;
            };
            if binary.operator_token.map(|token| token.kind)
                != Some(SyntaxKind::AmpersandAmpersandToken)
            {
                return false;
            }
            if let Some(right) = binary.right.and_then(|e| e.node_id())
                && self.children_mention_symbol(right, tested)
            {
                return true;
            }
            let Some(parent) = self.nodes.parent(node) else { return false };
            node = parent;
        }
    }

    /// `visit` of `isSymbolUsedInBinaryExpressionChain`, over `root`'s children.
    fn children_mention_symbol(&mut self, root: NodeId, tested: SymbolId) -> bool {
        let mut stack = self.child_ids(root);
        stack.reverse();
        while let Some(child) = stack.pop() {
            if self.nodes.kind(child) == SyntaxKind::Identifier
                && self.symbol_at_location_for_truthiness(child) == Some(tested)
            {
                return true;
            }
            let mut children = self.child_ids(child);
            children.reverse();
            stack.extend(children);
        }
        false
    }

    /// `isSymbolUsedInConditionBody` (`checker.go:3911`).
    fn symbol_used_in_condition_body(
        &mut self,
        condition: NodeId,
        body: NodeId,
        tested_node: NodeId,
        tested: SymbolId,
    ) -> bool {
        let mut stack = self.child_ids(body);
        stack.reverse();
        while let Some(child) = stack.pop() {
            if self.nodes.kind(child) == SyntaxKind::Identifier
                && self.symbol_at_location_for_truthiness(child) == Some(tested)
            {
                // A simple identifier test needs nothing more.
                if self.nodes.kind(condition) == SyntaxKind::Identifier
                    || self.nodes.parent(tested_node).is_some_and(|parent| {
                        self.nodes.kind(tested_node) == SyntaxKind::Identifier
                            && self.nodes.kind(parent) == SyntaxKind::BinaryExpression
                    })
                {
                    return true;
                }
                // Otherwise the symbol must be read off the same target.
                return self.same_access_target(tested_node, child);
            }
            let mut children = self.child_ids(child);
            children.reverse();
            stack.extend(children);
        }
        false
    }

    /// The receiver walk inside `isSymbolUsedInConditionBody`
    /// (`checker.go:3911`): climb both property-access/call chains in step.
    fn same_access_target(&mut self, tested_node: NodeId, child: NodeId) -> bool {
        let mut tested = self.nodes.parent(tested_node);
        let mut used = self.nodes.parent(child);
        while let (Some(t), Some(u)) = (tested, used) {
            let (t_kind, u_kind) = (self.nodes.kind(t), self.nodes.kind(u));
            if t_kind == SyntaxKind::Identifier && u_kind == SyntaxKind::Identifier {
                return self.symbol_at_location_for_truthiness(t)
                    == self.symbol_at_location_for_truthiness(u);
            }
            if t_kind == SyntaxKind::ThisKeyword && u_kind == SyntaxKind::ThisKeyword {
                // `getSymbolAtLocation(this)` is the symbol of `this`'s type.
                let t_type = self.check_expression_at_node(t);
                let u_type = self.check_expression_at_node(u);
                return t_type == u_type;
            }
            match (self.node_map.get(t), self.node_map.get(u)) {
                (
                    Some(Node::PropertyAccessExpression(t_access)),
                    Some(Node::PropertyAccessExpression(u_access)),
                ) => {
                    let (Some(t_name), Some(u_name)) = (
                        t_access.name.and_then(|n| n.node_id()),
                        u_access.name.and_then(|n| n.node_id()),
                    ) else {
                        return false;
                    };
                    if self.symbol_at_location_for_truthiness(t_name)
                        != self.symbol_at_location_for_truthiness(u_name)
                    {
                        return false;
                    }
                    tested = t_access.expression.and_then(|e| e.node_id());
                    used = u_access.expression.and_then(|e| e.node_id());
                }
                (Some(Node::CallExpression(t_call)), Some(Node::CallExpression(u_call))) => {
                    tested = t_call.expression.and_then(|e| e.node_id());
                    used = u_call.expression.and_then(|e| e.node_id());
                }
                _ => return false,
            }
        }
        false
    }

    /// `getSymbolAtLocation` (`checker.go`), for the identifier positions
    /// this rule compares: a property-access name answers the receiver's
    /// property symbol (`checkPropertyAccessExpression`'s resolved symbol), a
    /// declaration name its declared symbol, and any other identifier its
    /// resolved value symbol.
    fn symbol_at_location_for_truthiness(&mut self, node: NodeId) -> Option<SymbolId> {
        let parent = self.nodes.parent(node)?;
        if let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(parent)
            && access.name.and_then(|n| n.node_id()) == Some(node)
        {
            let receiver = access.expression?;
            let name = self.identifier_text(node)?.to_string();
            let receiver_type = self.check_expression(receiver);
            let apparent = self.apparent_type(receiver_type);
            return self.get_property_of_type(apparent, &name);
        }
        if let Some(declared) = self.binder.symbol_of(parent)
            && self.name_node_of(parent) == Some(node)
        {
            return Some(self.binder.merged_symbol(declared));
        }
        let text = self.identifier_text(node)?.to_string();
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            node,
            &text,
            tsr_binder::SymbolFlags::VALUE | tsr_binder::SymbolFlags::EXPORT_VALUE,
        )?;
        Some(self.binder.merged_symbol(symbol))
    }

    fn child_ids(&self, node: NodeId) -> Vec<NodeId> {
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children
    }

    /// `ast.IsLogicalOrCoalescingBinaryExpression`.
    fn is_logical_or_coalescing_binary(&self, node: NodeId) -> bool {
        matches!(self.node_map.get(node), Some(Node::BinaryExpression(binary))
        if binary.operator_token.is_some_and(|token| matches!(
            token.kind,
            SyntaxKind::AmpersandAmpersandToken
                | SyntaxKind::BarBarToken
                | SyntaxKind::QuestionQuestionToken
        )))
    }

    /// `ast.IsModuleExportsAccessExpression`: `module.exports`.
    fn is_module_exports_access(&self, node: NodeId) -> bool {
        let Some(Node::PropertyAccessExpression(access)) = self.node_map.get(node) else {
            return false;
        };
        matches!(access.expression, Some(Expression::Identifier(module)) if module.text == "module")
            && access.name.and_then(|n| n.node_id()).and_then(|n| self.identifier_text(n))
                == Some("exports")
    }

    /// `ast.SkipParentheses`.
    fn skip_parentheses_for_truthiness(&self, mut node: NodeId) -> NodeId {
        while let Some(Node::ParenthesizedExpression(wrapper)) = self.node_map.get(node)
            && let Some(inner) = wrapper.expression.and_then(|e| e.node_id())
        {
            node = inner;
        }
        node
    }

    /// `getResolvedSymbolOrNil(...).Flags` for the receiver of an enum-member
    /// read: an identifier's resolved value symbol; anything else answers no
    /// flags (`unknownSymbol`).
    fn resolved_symbol_flags(&mut self, receiver: NodeId) -> tsr_binder::SymbolFlags {
        if self.nodes.kind(receiver) != SyntaxKind::Identifier {
            return tsr_binder::SymbolFlags::empty();
        }
        self.symbol_at_location_for_truthiness(receiver)
            .map_or_else(tsr_binder::SymbolFlags::empty, |symbol| {
                self.binder.symbols().get(symbol).flags
            })
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
            // `undefined` is a value the global scope declares and no user
            // declaration shadows here; a shadowed one resolves and is
            // `Sometimes`, which is upstream's `symbol != c.undefinedSymbol`.
            Some(Node::Identifier(identifier)) if identifier.text == "undefined" => {
                if self
                    .binder
                    .resolve_name(
                        self.nodes,
                        self.node_map,
                        node,
                        identifier.text,
                        tsr_binder::SymbolFlags::VALUE,
                    )
                    .is_none()
                {
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
