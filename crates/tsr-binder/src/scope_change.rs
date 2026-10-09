//! `useOuterVariableScopeInParameter` (`binder/nameresolver.go:346`) for the
//! resolver walk in `lib.rs`.
//!
//! Native's `resolveNameHelper` (`nameresolver.go:74`) passes over a variable
//! found in a function's `locals` when the walk arrives from a parameter and
//! the variable is declared in the function's body, unless the parameter list
//! *requires a scope change* (`requiresScopeChange`, `:371`): an initializer
//! that a down-level emit moves into the body. `function f(a = () => x) { let
//! x }` therefore looks for `x` outside `f`, and reports TS2304 when nothing
//! outside declares it. The suggestion walk (`suggestion.rs`) already ports
//! the same arm through `SuggestionHost`; this is the resolver's copy.
//!
//! The decision needs two compiler options, which the binder does not hold;
//! the caller passes them ([`ScopeChangeOptions`]). Native caches the per-function
//! answer in `declarationRequiresScopeChange` links; this port computes it on
//! demand, which happens only after a body variable was found through a
//! parameter, so there is no cache, side table or publication state.
//! `docs/parity/notes/r6-names.md` §11.

use tsr_ast::{Node, NodeFlags, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_core::ScriptTarget;

use crate::{BindResult, SymbolFlags, SymbolId};

/// The two options `requiresScopeChange` reads (`nameresolver.go:384`,
/// `:388`): `GetEmitScriptTarget` and `GetEmitStandardClassFields`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScopeChangeOptions {
    /// `CompilerOptions.GetEmitScriptTarget()`.
    pub target: ScriptTarget,
    /// `CompilerOptions.GetEmitStandardClassFields()`.
    pub emit_standard_class_fields: bool,
}

#[allow(dead_code, reason = "hook: docs/parity/notes/r6-names-parameter-scope.diff")]
impl BindResult<'_> {
    /// The `useResult = false` arm of `resolveNameHelper`
    /// (`nameresolver.go:72-75`): `symbol`, found in `location`'s locals on a
    /// walk that came from `last`, is not the answer.
    ///
    /// `None` options decline the arm, keeping this binder's earlier answer
    /// for a caller that has no compiler options.
    #[allow(clippy::too_many_arguments)]
    #[cold]
    #[inline(never)]
    pub(crate) fn use_outer_variable_scope_in_parameter(
        &self,
        options: Option<&ScopeChangeOptions>,
        symbol: SymbolId,
        location: NodeId,
        last: Option<NodeId>,
        meaning: SymbolFlags,
        nodes: &NodeTable,
        node_map: &NodeMap<'_>,
    ) -> bool {
        let (Some(options), Some(last)) = (options, last) else { return false };
        // `ast.IsParameterDeclaration(lastLocation)`.
        if nodes.kind(last) != SyntaxKind::Parameter {
            return false;
        }
        // `ast.IsFunctionLike(location) && lastLocation != location.Body()`;
        // a body-less function-like has no body to be declared in.
        let Some(body) = function_body(node_map, location) else { return false };
        if body == last {
            return false;
        }
        // `lookup_scoped` already answers the merged symbol.
        let symbol = self.symbols.get(symbol);
        // `meaning&result.Flags&SymbolFlagsVariable != 0`.
        if !(meaning & symbol.flags).intersects(SymbolFlags::VARIABLE) {
            return false;
        }
        let Some(declaration) = symbol.value_declaration else { return false };
        let (body, declaration) = (nodes.span(body), nodes.span(declaration));
        declaration.start >= body.start
            && declaration.end <= body.end
            && !declaration_requires_scope_change(*options, location, nodes, node_map)
    }
}

/// `location.Body()` for the function-like kinds that own `locals`.
fn function_body(node_map: &NodeMap<'_>, location: NodeId) -> Option<NodeId> {
    match node_map.get(location)? {
        Node::FunctionDeclaration(f) => f.body.and_then(|b| b.node_id()),
        Node::FunctionExpression(f) => f.body.and_then(|b| b.node_id()),
        Node::ArrowFunction(f) => f.body.and_then(|b| b.node_id()),
        Node::MethodDeclaration(m) => m.body.and_then(|b| b.node_id()),
        Node::ConstructorDeclaration(c) => c.body.and_then(|b| b.node_id()),
        Node::GetAccessorDeclaration(a) => a.body.and_then(|b| b.node_id()),
        Node::SetAccessorDeclaration(a) => a.body.and_then(|b| b.node_id()),
        _ => None,
    }
}

/// `core.Some(functionLocation.Parameters(), r.requiresScopeChange)`
/// (`nameresolver.go:360`).
fn declaration_requires_scope_change(
    options: ScopeChangeOptions,
    function: NodeId,
    nodes: &NodeTable,
    node_map: &NodeMap<'_>,
) -> bool {
    let Some(typed) = node_map.get(function) else { return false };
    let mut parameters = Vec::new();
    tsr_ast::for_each_child_id(typed, |child| {
        if nodes.kind(child) == SyntaxKind::Parameter {
            parameters.push(child);
        }
    });
    parameters.into_iter().any(|parameter| {
        let Some(Node::ParameterDeclaration(declaration)) = node_map.get(parameter) else {
            return false;
        };
        declaration
            .name
            .and_then(|name| Node::from(name).node_id())
            .is_some_and(|name| requires_scope_change_worker(options, name, nodes, node_map))
            || declaration.initializer.and_then(|init| Node::from(init).node_id()).is_some_and(
                |initializer| requires_scope_change_worker(options, initializer, nodes, node_map),
            )
    })
}

/// `requiresScopeChangeWorker` (`nameresolver.go:376`).
fn requires_scope_change_worker(
    options: ScopeChangeOptions,
    node: NodeId,
    nodes: &NodeTable,
    node_map: &NodeMap<'_>,
) -> bool {
    let name_of = |node: NodeId| node_map.get(node).and_then(|typed| typed.name_id());
    let recurse = |node: NodeId| requires_scope_change_worker(options, node, nodes, node_map);
    match nodes.kind(node) {
        SyntaxKind::ArrowFunction
        | SyntaxKind::FunctionExpression
        | SyntaxKind::FunctionDeclaration
        | SyntaxKind::Constructor => false,
        SyntaxKind::MethodDeclaration
        | SyntaxKind::GetAccessor
        | SyntaxKind::SetAccessor
        | SyntaxKind::PropertyAssignment => name_of(node).is_some_and(recurse),
        SyntaxKind::PropertyDeclaration => {
            let is_static = matches!(node_map.get(node), Some(Node::PropertyDeclaration(property))
                if tsr_ast::has_syntactic_modifier(property.modifiers, SyntaxKind::StaticKeyword));
            if is_static {
                return !options.emit_standard_class_fields;
            }
            name_of(node).is_some_and(recurse)
        }
        kind => {
            // `ast.IsNullishCoalesce(node) || ast.IsOptionalChain(node)`.
            let nullish = matches!(node_map.get(node), Some(Node::BinaryExpression(binary))
                if binary.operator_token.is_some_and(|token| token.kind == SyntaxKind::QuestionQuestionToken));
            let optional_chain = nodes.flags(node).contains(NodeFlags::OPTIONAL_CHAIN)
                && matches!(
                    kind,
                    SyntaxKind::PropertyAccessExpression
                        | SyntaxKind::ElementAccessExpression
                        | SyntaxKind::CallExpression
                        | SyntaxKind::NonNullExpression
                );
            if nullish || optional_chain {
                return options.target < ScriptTarget::ES2020;
            }
            if let Some(Node::BindingElement(element)) = node_map.get(node)
                && element.dot_dot_dot_token.is_some()
                && nodes.parent(node).map(|parent| nodes.kind(parent))
                    == Some(SyntaxKind::ObjectBindingPattern)
            {
                return options.target < ScriptTarget::ES2017;
            }
            if is_type_node_kind(kind) {
                return false;
            }
            let Some(typed) = node_map.get(node) else { return false };
            let mut children = Vec::new();
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
            children.into_iter().any(recurse)
        }
    }
}

/// `ast.IsTypeNodeKind`.
fn is_type_node_kind(kind: SyntaxKind) -> bool {
    (kind >= SyntaxKind::FIRST_TYPE_NODE && kind <= SyntaxKind::LAST_TYPE_NODE)
        || matches!(
            kind,
            SyntaxKind::AnyKeyword
                | SyntaxKind::UnknownKeyword
                | SyntaxKind::NumberKeyword
                | SyntaxKind::BigIntKeyword
                | SyntaxKind::ObjectKeyword
                | SyntaxKind::BooleanKeyword
                | SyntaxKind::StringKeyword
                | SyntaxKind::SymbolKeyword
                | SyntaxKind::VoidKeyword
                | SyntaxKind::UndefinedKeyword
                | SyntaxKind::NeverKeyword
                | SyntaxKind::IntrinsicKeyword
                | SyntaxKind::ExpressionWithTypeArguments
        )
}
