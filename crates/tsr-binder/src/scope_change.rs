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
//! The decision needs two compiler options, which the binder does not hold.
//! They are read only at the three *triggers* (`requiresScopeChangeWorker`'s
//! static property, `??`/`?.`, and object rest arms); a parameter list with no
//! trigger answers the same under every option set. So a caller passes the
//! options if it has them ([`ScopeChangeOptions`], from the checker's
//! `resolve_name_with_export_alias`), and without them the arm still answers
//! every parameter list without a trigger, and declines (keeps the body's
//! variable, this binder's earlier answer) only for one with a trigger. The
//! options are never stored on [`BindResult`]: an `OnceLock` slot there cost
//! domain-model Ir +0.15% at identical call counts
//! (`docs/parity/notes/r6-names2.md` §1).
//!
//! Native caches the per-function answer in `declarationRequiresScopeChange`
//! links; this port computes it on demand, which happens only after a body
//! variable was found through a parameter, so there is no cache, side table or
//! publication state. `docs/parity/notes/r6-names.md` §11.

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

impl BindResult<'_> {
    /// The `useResult = false` arm of `resolveNameHelper`
    /// (`nameresolver.go:72-75`): `symbol`, found in `location`'s locals on a
    /// walk that came from `last`, is not the answer.
    ///
    /// `None` options answer only a parameter list whose answer does not
    /// depend on them; for one with a trigger the arm is declined.
    #[allow(clippy::too_many_arguments)]
    #[cold]
    #[inline(never)]
    pub(crate) fn use_outer_variable_scope_in_parameter(
        &self,
        options: Option<ScopeChangeOptions>,
        symbol: SymbolId,
        location: NodeId,
        last: Option<NodeId>,
        meaning: SymbolFlags,
        nodes: &NodeTable,
        node_map: &NodeMap<'_>,
    ) -> bool {
        let Some(last) = last else { return false };
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
            // An unknown answer (a trigger, no options) declines.
            && declaration_requires_scope_change(options, location, nodes, node_map) == Some(false)
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
/// (`nameresolver.go:360`), three-valued: `None` when the answer depends on
/// options the caller did not pass.
fn declaration_requires_scope_change(
    options: Option<ScopeChangeOptions>,
    function: NodeId,
    nodes: &NodeTable,
    node_map: &NodeMap<'_>,
) -> Option<bool> {
    let typed = node_map.get(function)?;
    let mut parameters = Vec::new();
    tsr_ast::for_each_child_id(typed, |child| {
        if nodes.kind(child) == SyntaxKind::Parameter {
            parameters.push(child);
        }
    });
    some(parameters, |parameter| {
        let Some(Node::ParameterDeclaration(declaration)) = node_map.get(parameter) else {
            return Some(false);
        };
        // `requiresScopeChange`: the name, then the initializer.
        let name = declaration.name.and_then(|name| Node::from(name).node_id());
        let initializer = declaration.initializer.and_then(|init| Node::from(init).node_id());
        some(name.into_iter().chain(initializer), |node| {
            requires_scope_change_worker(options, node, nodes, node_map)
        })
    })
}

/// `core.Some` / `ForEachChild` over a three-valued predicate: `true` as soon
/// as one item is, else unknown if one was, else `false`.
fn some<T>(
    items: impl IntoIterator<Item = T>,
    mut test: impl FnMut(T) -> Option<bool>,
) -> Option<bool> {
    let mut answer = Some(false);
    for item in items {
        match test(item) {
            Some(true) => return Some(true),
            None => answer = None,
            Some(false) => {}
        }
    }
    answer
}

/// `requiresScopeChangeWorker` (`nameresolver.go:376`). Each option read is
/// `None` when the caller passed no options.
fn requires_scope_change_worker(
    options: Option<ScopeChangeOptions>,
    node: NodeId,
    nodes: &NodeTable,
    node_map: &NodeMap<'_>,
) -> Option<bool> {
    let name_of = |node: NodeId| node_map.get(node).and_then(|typed| typed.name_id());
    let recurse = |node: NodeId| match name_of(node) {
        Some(name) => requires_scope_change_worker(options, name, nodes, node_map),
        None => Some(false),
    };
    match nodes.kind(node) {
        SyntaxKind::ArrowFunction
        | SyntaxKind::FunctionExpression
        | SyntaxKind::FunctionDeclaration
        | SyntaxKind::Constructor => Some(false),
        SyntaxKind::MethodDeclaration
        | SyntaxKind::GetAccessor
        | SyntaxKind::SetAccessor
        | SyntaxKind::PropertyAssignment => recurse(node),
        SyntaxKind::PropertyDeclaration => {
            let is_static = matches!(node_map.get(node), Some(Node::PropertyDeclaration(property))
                if tsr_ast::has_syntactic_modifier(property.modifiers, SyntaxKind::StaticKeyword));
            if is_static {
                return options.map(|options| !options.emit_standard_class_fields);
            }
            recurse(node)
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
                return options.map(|options| options.target < ScriptTarget::ES2020);
            }
            if let Some(Node::BindingElement(element)) = node_map.get(node)
                && element.dot_dot_dot_token.is_some()
                && nodes.parent(node).map(|parent| nodes.kind(parent))
                    == Some(SyntaxKind::ObjectBindingPattern)
            {
                return options.map(|options| options.target < ScriptTarget::ES2017);
            }
            if is_type_node_kind(kind) {
                return Some(false);
            }
            let Some(typed) = node_map.get(node) else { return Some(false) };
            let mut children = Vec::new();
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
            some(children, |child| requires_scope_change_worker(options, child, nodes, node_map))
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
