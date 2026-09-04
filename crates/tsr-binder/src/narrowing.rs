//! Which expressions are worth a flow node.
//!
//! Ported from `isNarrowingExpression` and its helpers
//! (`internal/binder/binder.go:2595–2697`) plus the `ast.Is*` predicates they
//! lean on (`internal/ast/utilities.go`) at the pinned commit.
//!
//! # Why these exist at all
//!
//! The flow graph is not a faithful CFG of the program — it is the subset of the
//! CFG that could change a type. `if (foo())` branches, but nothing about the
//! branch tells the checker anything, so upstream records no condition node for
//! it; `if (x)` does, because `x` is a reference the checker can narrow. Every
//! predicate here is a filter with that one purpose, and getting one wrong is
//! not a crash but a silently larger or smaller graph — larger costs memory and
//! analysis time, smaller loses narrowing. That is why they are ported arm for
//! arm rather than reconstructed from what narrowing "obviously" needs.
//!
//! # Reading a node's parent
//!
//! Upstream writes `node.Parent`. The tree here has no back-edges
//! ([ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md)) and
//! [`NodeTable`] answers with a [`NodeId`], not a [`Node`] — there is no
//! id-to-node table, and building one would cost 16 bytes per node to serve a
//! handful of predicates. So the functions that need a parent take it as an
//! argument, and the binder supplies it from the ancestor chain it is already
//! walking. See `binder::Ancestors`.

use tsr_ast::{Expression, Node, NodeFlags, NodeTable, SyntaxKind};

/// Whether narrowing `expr` could tell the checker anything.
///
/// Upstream: `isNarrowingExpression`.
#[must_use]
pub(crate) fn is_narrowing_expression(expr: Node<'_>, nodes: &NodeTable) -> bool {
    match expr {
        Node::Identifier(_) => true,
        Node::KeywordExpression(k) if k.kind == SyntaxKind::ThisKeyword => true,
        Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_) => {
            contains_narrowable_reference(expr, nodes)
        }
        Node::CallExpression(call) => {
            call.arguments
                .iter()
                .any(|argument| contains_narrowable_reference(Node::from(*argument), nodes))
                || matches!(
                    call.expression,
                    Some(Expression::PropertyAccessExpression(access))
                        if access.expression.is_some_and(|inner| {
                            contains_narrowable_reference(Node::from(inner), nodes)
                        })
                )
        }
        Node::ParenthesizedExpression(_)
        | Node::NonNullExpression(_)
        | Node::TypeOfExpression(_) => expression_of(expr)
            .is_some_and(|inner| is_narrowing_expression(Node::from(inner), nodes)),
        Node::BinaryExpression(_) => is_narrowing_binary_expression(expr, nodes),
        Node::PrefixUnaryExpression(unary) => {
            unary.operator.kind == SyntaxKind::ExclamationToken
                && unary
                    .operand
                    .is_some_and(|operand| is_narrowing_expression(Node::from(operand), nodes))
        }
        _ => false,
    }
}

/// Whether `expr` is, or optionally chains onto, a narrowable reference.
///
/// Upstream: `containsNarrowableReference`.
#[must_use]
pub(crate) fn contains_narrowable_reference(expr: Node<'_>, nodes: &NodeTable) -> bool {
    if is_narrowable_reference(expr) {
        return true;
    }
    if !has_flag(expr, nodes, NodeFlags::OPTIONAL_CHAIN) {
        return false;
    }
    match expr {
        Node::PropertyAccessExpression(_)
        | Node::ElementAccessExpression(_)
        | Node::CallExpression(_)
        | Node::NonNullExpression(_) => expression_of(expr)
            .is_some_and(|inner| contains_narrowable_reference(Node::from(inner), nodes)),
        _ => false,
    }
}

/// Whether `node` denotes a storage location the checker can track.
///
/// Upstream: `isNarrowableReference`.
#[must_use]
pub(crate) fn is_narrowable_reference(node: Node<'_>) -> bool {
    match node {
        Node::Identifier(_) | Node::MetaProperty(_) => true,
        Node::KeywordExpression(k) => {
            matches!(k.kind, SyntaxKind::ThisKeyword | SyntaxKind::SuperKeyword)
        }
        Node::PropertyAccessExpression(_)
        | Node::ParenthesizedExpression(_)
        | Node::NonNullExpression(_) => {
            expression_of(node).is_some_and(|inner| is_narrowable_reference(Node::from(inner)))
        }
        Node::ElementAccessExpression(access) => {
            let argument = access.argument_expression;
            argument.is_some_and(|a| is_string_or_numeric_literal_like(Node::from(a)))
                || argument.is_some_and(|a| is_entity_name_expression(Node::from(a)))
                    && access
                        .expression
                        .is_some_and(|inner| is_narrowable_reference(Node::from(inner)))
        }
        // `(a, b) = c` narrows `b`; `a.b = c` narrows `a.b`. Both are references
        // that a *later* read can be narrowed against, which is why an
        // assignment counts as one and not merely as the thing that changes one.
        Node::BinaryExpression(binary) => {
            let operator = binary.operator_token.map(|token| token.kind);
            operator == Some(SyntaxKind::CommaToken)
                && binary.right.is_some_and(|right| is_narrowable_reference(Node::from(right)))
                || operator.is_some_and(is_assignment_operator)
                    && binary
                        .left
                        .is_some_and(|left| is_left_hand_side_expression(Node::from(left)))
        }
        _ => false,
    }
}

/// Whether a binary expression is one narrowing understands.
///
/// Upstream: `isNarrowingBinaryExpression`.
fn is_narrowing_binary_expression(expr: Node<'_>, nodes: &NodeTable) -> bool {
    let Node::BinaryExpression(binary) = expr else { return false };
    let Some(operator) = binary.operator_token.map(|token| token.kind) else { return false };
    match operator {
        SyntaxKind::EqualsToken
        | SyntaxKind::BarBarEqualsToken
        | SyntaxKind::AmpersandAmpersandEqualsToken
        | SyntaxKind::QuestionQuestionEqualsToken => {
            binary.left.is_some_and(|left| contains_narrowable_reference(Node::from(left), nodes))
        }
        SyntaxKind::EqualsEqualsToken
        | SyntaxKind::ExclamationEqualsToken
        | SyntaxKind::EqualsEqualsEqualsToken
        | SyntaxKind::ExclamationEqualsEqualsToken => {
            let (Some(left), Some(right)) = (binary.left, binary.right) else { return false };
            let left = skip_parentheses(Node::from(left));
            let right = skip_parentheses(Node::from(right));
            is_narrowable_operand(left, nodes)
                || is_narrowable_operand(right, nodes)
                || is_narrowing_type_of_operands(right, left, nodes)
                || is_narrowing_type_of_operands(left, right, nodes)
                || is_boolean_literal(right) && is_narrowing_expression(left, nodes)
                || is_boolean_literal(left) && is_narrowing_expression(right, nodes)
        }
        SyntaxKind::InstanceOfKeyword => {
            binary.left.is_some_and(|left| is_narrowable_operand(Node::from(left), nodes))
        }
        SyntaxKind::InKeyword | SyntaxKind::CommaToken => {
            binary.right.is_some_and(|right| is_narrowing_expression(Node::from(right), nodes))
        }
        _ => false,
    }
}

/// Upstream: `isNarrowableOperand`.
fn is_narrowable_operand(expr: Node<'_>, nodes: &NodeTable) -> bool {
    match expr {
        Node::ParenthesizedExpression(_) => {
            return expression_of(expr)
                .is_some_and(|inner| is_narrowable_operand(Node::from(inner), nodes));
        }
        Node::BinaryExpression(binary) => match binary.operator_token.map(|token| token.kind) {
            Some(SyntaxKind::EqualsToken) => {
                return binary
                    .left
                    .is_some_and(|left| is_narrowable_operand(Node::from(left), nodes));
            }
            Some(SyntaxKind::CommaToken) => {
                return binary
                    .right
                    .is_some_and(|right| is_narrowable_operand(Node::from(right), nodes));
            }
            _ => {}
        },
        _ => {}
    }
    contains_narrowable_reference(expr, nodes)
}

/// `typeof x === "string"`, in either operand order.
///
/// Upstream: `isNarrowingTypeOfOperands`.
fn is_narrowing_type_of_operands(first: Node<'_>, second: Node<'_>, nodes: &NodeTable) -> bool {
    matches!(first, Node::TypeOfExpression(_))
        && expression_of(first).is_some_and(|inner| is_narrowable_operand(Node::from(inner), nodes))
        && is_string_literal_like(second)
}

/// A name that could be an assertion call's target: `a`, `a.b`, `this.a.b`.
///
/// Upstream: `ast.IsDottedName`.
#[must_use]
pub(crate) fn is_dotted_name(node: Node<'_>) -> bool {
    match node {
        Node::Identifier(_) | Node::MetaProperty(_) => true,
        Node::KeywordExpression(k) => {
            matches!(k.kind, SyntaxKind::ThisKeyword | SyntaxKind::SuperKeyword)
        }
        Node::PropertyAccessExpression(_) | Node::ParenthesizedExpression(_) => {
            expression_of(node).is_some_and(|inner| is_dotted_name(Node::from(inner)))
        }
        _ => false,
    }
}

/// Upstream: `ast.IsEntityNameExpression` with `allowJS` false.
fn is_entity_name_expression(node: Node<'_>) -> bool {
    match node {
        Node::Identifier(_) => true,
        Node::PropertyAccessExpression(access) => {
            matches!(access.name, Some(tsr_ast::MemberName::Identifier(_)))
                && access
                    .expression
                    .is_some_and(|inner| is_entity_name_expression(Node::from(inner)))
        }
        _ => false,
    }
}

/// Upstream: `ast.IsStringLiteralLike`.
fn is_string_literal_like(node: Node<'_>) -> bool {
    matches!(node, Node::StringLiteral(_) | Node::NoSubstitutionTemplateLiteral(_))
}

/// Upstream: `ast.IsStringOrNumericLiteralLike`.
fn is_string_or_numeric_literal_like(node: Node<'_>) -> bool {
    is_string_literal_like(node) || matches!(node, Node::NumericLiteral(_))
}

/// Upstream: `ast.IsBooleanLiteral`.
#[must_use]
pub(crate) fn is_boolean_literal(node: Node<'_>) -> bool {
    matches!(node, Node::KeywordExpression(k)
        if matches!(k.kind, SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword))
}

/// Upstream: `ast.IsLeftHandSideExpression` (kind only; there are no
/// `PartiallyEmittedExpression`s in a parsed tree, only in transformed ones).
#[must_use]
pub(crate) fn is_left_hand_side_expression(node: Node<'_>) -> bool {
    match node {
        Node::PropertyAccessExpression(_)
        | Node::ElementAccessExpression(_)
        | Node::NewExpression(_)
        | Node::CallExpression(_)
        | Node::JsxElement(_)
        | Node::JsxSelfClosingElement(_)
        | Node::JsxFragment(_)
        | Node::TaggedTemplateExpression(_)
        | Node::ArrayLiteralExpression(_)
        | Node::ParenthesizedExpression(_)
        | Node::ObjectLiteralExpression(_)
        | Node::ClassExpression(_)
        | Node::FunctionExpression(_)
        | Node::Identifier(_)
        | Node::PrivateIdentifier(_)
        | Node::RegularExpressionLiteral(_)
        | Node::NumericLiteral(_)
        | Node::BigIntLiteral(_)
        | Node::StringLiteral(_)
        | Node::NoSubstitutionTemplateLiteral(_)
        | Node::TemplateExpression(_)
        | Node::NonNullExpression(_)
        | Node::ExpressionWithTypeArguments(_)
        | Node::MetaProperty(_)
        | Node::MissingDeclaration(_) => true,
        Node::KeywordExpression(k) => matches!(
            k.kind,
            SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::ImportKeyword
        ),
        _ => false,
    }
}

/// Upstream: `ast.IsAssignmentOperator`.
#[must_use]
pub(crate) fn is_assignment_operator(kind: SyntaxKind) -> bool {
    SyntaxKind::FIRST_ASSIGNMENT as u16 <= kind as u16
        && kind as u16 <= SyntaxKind::LAST_ASSIGNMENT as u16
}

/// Upstream: `ast.IsLogicalOrCoalescingBinaryOperator`.
#[must_use]
pub(crate) fn is_logical_or_coalescing_binary_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BarBarToken
            | SyntaxKind::AmpersandAmpersandToken
            | SyntaxKind::QuestionQuestionToken
    )
}

/// Upstream: `ast.IsLogicalOrCoalescingAssignmentOperator`.
#[must_use]
pub(crate) fn is_logical_or_coalescing_assignment_operator(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::BarBarEqualsToken
            | SyntaxKind::AmpersandAmpersandEqualsToken
            | SyntaxKind::QuestionQuestionEqualsToken
    )
}

/// Whether `node` is `&&`, `||`, or `??` under any number of parentheses and
/// logical negations.
///
/// Upstream: `ast.IsLogicalExpression`.
#[must_use]
pub(crate) fn is_logical_expression(node: Node<'_>) -> bool {
    let mut node = node;
    loop {
        match node {
            Node::ParenthesizedExpression(p) => match p.expression {
                Some(inner) => node = Node::from(inner),
                None => return false,
            },
            Node::PrefixUnaryExpression(u) if u.operator.kind == SyntaxKind::ExclamationToken => {
                match u.operand {
                    Some(operand) => node = Node::from(operand),
                    None => return false,
                }
            }
            Node::BinaryExpression(b) => {
                return b
                    .operator_token
                    .is_some_and(|token| is_logical_or_coalescing_binary_operator(token.kind));
            }
            _ => return false,
        }
    }
}

/// Whether `node` is `&&=`, `||=`, or `??=` under any number of parentheses.
///
/// Upstream: `isLogicalAssignmentExpression`.
#[must_use]
pub(crate) fn is_logical_assignment_expression(node: Node<'_>) -> bool {
    matches!(skip_parentheses(node), Node::BinaryExpression(b)
        if b.operator_token
            .is_some_and(|token| is_logical_or_coalescing_assignment_operator(token.kind)))
}

/// Upstream: `ast.IsNullishCoalesce`.
#[must_use]
pub(crate) fn is_nullish_coalesce(node: Node<'_>) -> bool {
    matches!(node, Node::BinaryExpression(b)
        if b.operator_token.map(|token| token.kind) == Some(SyntaxKind::QuestionQuestionToken))
}

/// Upstream: `ast.SkipParentheses`.
#[must_use]
pub(crate) fn skip_parentheses(node: Node<'_>) -> Node<'_> {
    let mut node = node;
    while let Node::ParenthesizedExpression(p) = node {
        match p.expression {
            Some(inner) => node = Node::from(inner),
            None => return node,
        }
    }
    node
}

/// Upstream: `ast.IsOptionalChain`.
///
/// Live since §748 (`checker-notes-callres.md`): the parser sets
/// [`NodeFlags::OPTIONAL_CHAIN`] exactly as upstream's
/// `tryReparseOptionalChain` does (`parser.go:5414`). Before that the flag
/// was never set, every optional-chain path in the binder was unreachable,
/// and `a?.b` got the flow graph of `a.b` (the gap `tsr-y4u.7` recorded).
#[must_use]
pub(crate) fn is_optional_chain(node: Node<'_>, nodes: &NodeTable) -> bool {
    has_flag(node, nodes, NodeFlags::OPTIONAL_CHAIN)
        && matches!(
            node,
            Node::PropertyAccessExpression(_)
                | Node::ElementAccessExpression(_)
                | Node::CallExpression(_)
                | Node::NonNullExpression(_)
        )
}

/// Upstream: `ast.IsOptionalChainRoot`.
#[must_use]
pub(crate) fn is_optional_chain_root(node: Node<'_>, nodes: &NodeTable) -> bool {
    is_optional_chain(node, nodes)
        && !matches!(node, Node::NonNullExpression(_))
        && question_dot_token(node).is_some()
}

/// Upstream: `ast.IsOutermostOptionalChain`, with the parent passed in.
#[must_use]
pub(crate) fn is_outermost_optional_chain(
    node: Node<'_>,
    parent: Option<Node<'_>>,
    nodes: &NodeTable,
) -> bool {
    let Some(parent) = parent else { return true };
    !is_optional_chain(parent, nodes)
        || is_optional_chain_root(parent, nodes)
        || !is_same_node(expression_of(parent).map(Node::from), node)
}

/// Upstream: `ast.IsExpressionOfOptionalChainRoot`, with the parent passed in.
#[must_use]
pub(crate) fn is_expression_of_optional_chain_root(
    node: Node<'_>,
    parent: Option<Node<'_>>,
    nodes: &NodeTable,
) -> bool {
    parent.is_some_and(|parent| {
        is_optional_chain_root(parent, nodes)
            && is_same_node(expression_of(parent).map(Node::from), node)
    })
}

/// The `?.` of an optional-chain link.
fn question_dot_token(node: Node<'_>) -> Option<&tsr_ast::Token<'_>> {
    match node {
        Node::PropertyAccessExpression(n) => n.question_dot_token,
        Node::ElementAccessExpression(n) => n.question_dot_token,
        Node::CallExpression(n) => n.question_dot_token,
        _ => None,
    }
}

/// Upstream's `node.Expression()`, for the kinds the flow binder asks about.
///
/// Deliberately partial: an exhaustive accessor would be a second copy of the
/// generated AST, and a kind this returns `None` for is one no caller reaches.
#[must_use]
pub(crate) fn expression_of(node: Node<'_>) -> Option<Expression<'_>> {
    match node {
        Node::PropertyAccessExpression(n) => n.expression,
        Node::ElementAccessExpression(n) => n.expression,
        Node::CallExpression(n) => n.expression,
        Node::NewExpression(n) => n.expression,
        Node::NonNullExpression(n) => n.expression,
        Node::ParenthesizedExpression(n) => n.expression,
        Node::TypeOfExpression(n) => n.expression,
        Node::VoidExpression(n) => n.expression,
        Node::AwaitExpression(n) => n.expression,
        Node::DeleteExpression(n) => n.expression,
        Node::SpreadElement(n) => n.expression,
        Node::SpreadAssignment(n) => n.expression,
        Node::AsExpression(n) => n.expression,
        Node::SatisfiesExpression(n) => n.expression,
        Node::ExpressionStatement(n) => n.expression,
        Node::ReturnStatement(n) => n.expression,
        Node::ThrowStatement(n) => n.expression,
        Node::SwitchStatement(n) => n.expression,
        Node::WithStatement(n) => n.expression,
        Node::IfStatement(n) => n.expression,
        Node::WhileStatement(n) => n.expression,
        Node::DoStatement(n) => n.expression,
        Node::ForInOrOfStatement(n) => n.expression,
        Node::CaseOrDefaultClause(n) => n.expression,
        Node::ExpressionWithTypeArguments(n) => n.expression,
        Node::PartiallyEmittedExpression(n) => n.expression,
        _ => None,
    }
}

/// Node identity, by id.
///
/// Upstream compares `*ast.Node` pointers. Two distinct nodes never share an id,
/// so this is the same test; an unregistered node compares equal to nothing,
/// including itself, which is the conservative answer.
fn is_same_node(left: Option<Node<'_>>, right: Node<'_>) -> bool {
    match (left.and_then(|n| n.node_id()), right.node_id()) {
        (Some(left), Some(right)) => left == right,
        _ => false,
    }
}

/// Read a node's flags through the side table, tolerating an unregistered node.
fn has_flag(node: Node<'_>, nodes: &NodeTable, flag: NodeFlags) -> bool {
    node.node_id().is_some_and(|id| nodes.flags(id).contains(flag))
}

/// Whether reaching `node` at runtime would actually run something.
///
/// Upstream: `ast.IsPotentiallyExecutableNode`. This is what decides whether
/// unreachable code is *reported*: a type alias after a `return` is unreachable
/// and harmless, an assignment after a `return` is unreachable and a mistake.
#[must_use]
pub(crate) fn is_potentially_executable_node(node: Node<'_>, nodes: &NodeTable) -> bool {
    let Some(id) = node.node_id() else { return false };
    let kind = nodes.kind(id);
    if SyntaxKind::FIRST_STATEMENT as u16 <= kind as u16
        && kind as u16 <= SyntaxKind::LAST_STATEMENT as u16
    {
        let Node::VariableStatement(statement) = node else { return true };
        let Some(list) = statement.declaration_list else { return true };
        // `let`/`const`/`using` are temporal-dead-zone observable even without an
        // initializer, so an unreachable one is still worth reporting; a bare
        // `var x;` is not.
        let list_flags = list.node_id.map_or_else(NodeFlags::empty, |list_id| nodes.flags(list_id));
        if list_flags.intersects(NodeFlags::BLOCK_SCOPED) {
            return true;
        }
        return list.declarations.iter().any(|declaration| declaration.initializer.is_some());
    }
    matches!(
        node,
        Node::ClassDeclaration(_) | Node::EnumDeclaration(_) | Node::ModuleDeclaration(_)
    )
}

/// Names whose call on an array is treated as a mutation of it.
///
/// Upstream: `ast.IsPushOrUnshiftIdentifier`.
#[must_use]
pub(crate) fn is_push_or_unshift_identifier(name: &str) -> bool {
    name == "push" || name == "unshift"
}
