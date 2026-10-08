//! The scope walk behind "Did you mean …?" for an unresolved name.
//!
//! Pinned native operation: `resolveNameForSymbolSuggestion`
//! (`checker.go:966`), a second `binder.NameResolver` whose `Lookup` is
//! `getSuggestionForSymbolNameLookup` (`checker.go:1774`) instead of
//! `getSymbol`. `NameResolver.Resolve` (`nameresolver.go:24`) is unchanged, so
//! every arm consults its table through the *suggestion* lookup and the walk
//! stops at the **first table that yields a near-miss**, with that arm's own
//! meaning mask and that arm's own acceptance rules applied to the suggested
//! symbol. The suggestion is therefore not the closest name in scope: a
//! class's property members are never candidates (the class arm looks with
//! `meaning & Type`), and an inner scope's acceptable near-miss shadows a
//! closer one further out.
//!
//! Called with `nameNotFoundMessage == nil` and `isUse == false`, so the error
//! arms (static type parameter, heritage and computed-name type parameters)
//! answer nil without reporting, and nothing is marked referenced.
//!
//! No cache or side table. The table reads are the binder's; the lookup and
//! the parameter-scope question are the checker's ([`SuggestionHost`]).
//! Reached only after a name failed to resolve, so the work is on the error
//! path.

use tsr_ast::{Node, NodeId, NodeMap, NodeTable, SyntaxKind};

use crate::{BindResult, SymbolFlags, SymbolId, SymbolTable};

/// The checker half of the suggestion resolver.
pub trait SuggestionHost<'a> {
    /// `NameResolver.Lookup` = `getSuggestionForSymbolNameLookup`: the exact
    /// `getSymbol` hit, else the spelling suggestion over the table's symbols.
    /// The globals lookup (`meaning | SymbolFlagsGlobalLookup`) is the
    /// caller's, after [`SuggestionWalk::Globals`].
    fn lookup(
        &mut self,
        table: &SymbolTable<'a>,
        name: &str,
        meaning: SymbolFlags,
    ) -> Option<SymbolId>;

    /// `requiresScopeChange` over `function`'s parameters
    /// (`useOuterVariableScopeInParameter`, `nameresolver.go:345`); it reads
    /// the emit target, which the binder does not have.
    fn declaration_requires_scope_change(&mut self, function: NodeId) -> bool;
}

/// How [`BindResult::resolve_name_for_suggestion`] ended.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SuggestionWalk {
    /// An arm accepted this symbol (`break loop`).
    Found(SymbolId),
    /// An arm returned nil before the globals lookup.
    Declined,
    /// The walk ran off the top: the caller performs the globals lookup.
    Globals,
}

fn id<'a>(node: impl Into<Node<'a>>) -> Option<NodeId> {
    node.into().node_id()
}

/// `ast.IsFunctionLike` with its `Body()` and `Type()`; `None` for any other
/// node.
fn function_like_parts(node: Option<Node<'_>>) -> Option<(Option<NodeId>, Option<NodeId>)> {
    Some(match node? {
        Node::FunctionDeclaration(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::FunctionExpression(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::ArrowFunction(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::MethodDeclaration(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::ConstructorDeclaration(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::GetAccessorDeclaration(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::SetAccessorDeclaration(n) => (n.body.and_then(id), n.r#type.and_then(id)),
        Node::MethodSignatureDeclaration(n) => (None, n.r#type.and_then(id)),
        Node::CallSignatureDeclaration(n) => (None, n.r#type.and_then(id)),
        Node::ConstructSignatureDeclaration(n) => (None, n.r#type.and_then(id)),
        Node::IndexSignatureDeclaration(n) => (None, n.r#type.and_then(id)),
        Node::FunctionTypeNode(n) => (None, n.r#type.and_then(id)),
        Node::ConstructorTypeNode(n) => (None, n.r#type.and_then(id)),
        Node::JSDocSignature(n) => (None, n.r#type.and_then(id)),
        _ => return None,
    })
}

/// `ast.IsConstAssertion`: `x as const` / `<const>x`.
fn is_const_assertion(node: Option<Node<'_>>) -> bool {
    let ty = match node {
        Some(Node::AsExpression(n)) => n.r#type,
        Some(Node::TypeAssertion(n)) => n.r#type,
        _ => return false,
    };
    matches!(ty.map(Node::from), Some(Node::TypeReferenceNode(reference))
        if matches!(reference.type_name.map(Node::from), Some(Node::Identifier(name)) if name.text == "const"))
}

impl<'a> BindResult<'a> {
    /// `NameResolver.Resolve` (`nameresolver.go:24`) driven by the suggestion
    /// lookup, up to (not including) the globals lookup — see the module docs.
    ///
    /// Arms this port's binder has no table for are declined as
    /// [`BindResult::resolve_name`] declines them: the `CommonJS` module
    /// indicator test on a file's exports, and `arguments` (a failed
    /// resolution of `arguments` inside a function cannot reach a suggestion,
    /// because that arm resolves it).
    #[allow(clippy::too_many_lines, reason = "one arm per native switch case")]
    pub fn resolve_name_for_suggestion(
        &self,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        start: NodeId,
        name: &str,
        meaning: SymbolFlags,
        host: &mut dyn SuggestionHost<'a>,
    ) -> SuggestionWalk {
        let mut last: Option<NodeId> = None;
        let mut current = Some(start);
        while let Some(mut location) = current {
            let typed = node_map.get(location);
            if name == "const" && is_const_assertion(typed) {
                return SuggestionWalk::Declined;
            }
            let name_of = match typed {
                Some(Node::ModuleDeclaration(n)) => n.name.and_then(id),
                Some(Node::EnumDeclaration(n)) => n.name.and_then(id),
                _ => None,
            };
            if last.is_some() && name_of == last {
                last = Some(location);
                let Some(parent) = nodes.parent(location) else { break };
                location = parent;
            }
            let typed = node_map.get(location);
            let kind = nodes.kind(location);
            let global_source_file =
                kind == SyntaxKind::SourceFile && self.symbol_of(location).is_none();
            if !global_source_file
                && let Some(locals) = self.locals.get(&location)
                && let Some(result) = host.lookup(locals, name, meaning)
                && self.use_local_suggestion(result, location, last, meaning, nodes, node_map, host)
            {
                return SuggestionWalk::Found(result);
            }
            match typed {
                Some(Node::SourceFile(_) | Node::ModuleDeclaration(_)) => {
                    if let Some(module) = self.symbol_of(location) {
                        let exports = &self.symbols.get(self.merged_symbol(module)).exports;
                        let external = match typed {
                            Some(Node::ModuleDeclaration(module)) => {
                                self.facts(location).contains(crate::NodeFacts::AMBIENT_CONTEXT)
                                    && module.keyword.kind != SyntaxKind::GlobalKeyword
                            }
                            _ => true,
                        };
                        let mut skip = false;
                        if external {
                            if let Some(&default) = exports.get(crate::binder::INTERNAL_DEFAULT)
                                && self
                                    .local_name_of_export_default(default, location, name, node_map)
                                && self.symbols.get(default).flags.intersects(meaning)
                            {
                                return SuggestionWalk::Found(default);
                            }
                            skip = exports.get(name).is_some_and(|&export| {
                                let entry = self.symbols.get(export);
                                entry.flags == SymbolFlags::ALIAS
                                    && entry.declarations.iter().any(|&d| {
                                        matches!(
                                            nodes.kind(d),
                                            SyntaxKind::ExportSpecifier
                                                | SyntaxKind::NamespaceExport
                                        )
                                    })
                            });
                        }
                        if !skip
                            && name != crate::binder::INTERNAL_DEFAULT
                            && let Some(table) = exports.as_ref()
                            && let Some(result) =
                                host.lookup(table, name, meaning & SymbolFlags::MODULE_MEMBER)
                        {
                            return SuggestionWalk::Found(result);
                        }
                    }
                }
                Some(Node::EnumDeclaration(_)) => {
                    if let Some(enumeration) = self.symbol_of(location)
                        && let Some(table) =
                            self.symbols.get(self.merged_symbol(enumeration)).exports.as_ref()
                        && let Some(result) =
                            host.lookup(table, name, meaning & SymbolFlags::ENUM_MEMBER)
                    {
                        return SuggestionWalk::Found(result);
                    }
                }
                Some(
                    Node::ClassDeclaration(_)
                    | Node::ClassExpression(_)
                    | Node::InterfaceDeclaration(_),
                ) => {
                    if let Some(result) = self.member_suggestion(location, name, meaning, host) {
                        if self.is_type_parameter_declared_in(result, location, nodes) {
                            if last.is_some_and(|last| crate::is_static_member(node_map, last)) {
                                return SuggestionWalk::Declined;
                            }
                            return SuggestionWalk::Found(result);
                        }
                    } else if meaning.intersects(SymbolFlags::CLASS)
                        && let Some(Node::ClassExpression(class)) = typed
                        && class.name.is_some_and(|written| written.text == name)
                        && let Some(symbol) = self.symbol_of(location)
                    {
                        return SuggestionWalk::Found(symbol);
                    }
                }
                Some(Node::ExpressionWithTypeArguments(n)) => {
                    if last.is_some()
                        && n.expression.and_then(id) == last
                        && let Some(heritage) = nodes.parent(location)
                        && matches!(node_map.get(heritage), Some(Node::HeritageClause(clause))
                            if clause.token.kind == SyntaxKind::ExtendsKeyword)
                        && let Some(container) = nodes.parent(heritage)
                        && matches!(
                            nodes.kind(container),
                            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
                        )
                        && self.member_suggestion(container, name, meaning, host).is_some()
                    {
                        return SuggestionWalk::Declined;
                    }
                }
                Some(Node::ComputedPropertyName(_)) => {
                    if let Some(grandparent) = nodes.parent(location).and_then(|p| nodes.parent(p))
                        && matches!(
                            nodes.kind(grandparent),
                            SyntaxKind::ClassDeclaration
                                | SyntaxKind::ClassExpression
                                | SyntaxKind::InterfaceDeclaration
                        )
                        && self.member_suggestion(grandparent, name, meaning, host).is_some()
                    {
                        return SuggestionWalk::Declined;
                    }
                }
                Some(Node::FunctionExpression(function)) => {
                    if meaning.intersects(SymbolFlags::FUNCTION)
                        && function.name.is_some_and(|written| written.text == name)
                        && let Some(symbol) = self.symbol_of(location)
                    {
                        return SuggestionWalk::Found(symbol);
                    }
                }
                Some(Node::Decorator(_)) => {
                    if let Some(parent) = nodes.parent(location)
                        && nodes.kind(parent) == SyntaxKind::Parameter
                    {
                        location = parent;
                    }
                    if let Some(parent) = nodes.parent(location)
                        && (is_class_element(nodes.kind(parent))
                            || nodes.kind(parent) == SyntaxKind::ClassDeclaration)
                    {
                        location = parent;
                    }
                }
                Some(Node::InferTypeNode(infer)) => {
                    if meaning.intersects(SymbolFlags::TYPE_PARAMETER)
                        && let Some(parameter) = infer.type_parameter
                        && parameter.name.is_some_and(|written| written.text == name)
                        && let Some(symbol) = id(parameter).and_then(|p| self.symbol_of(p))
                    {
                        return SuggestionWalk::Found(symbol);
                    }
                }
                Some(Node::ExportSpecifier(specifier)) => {
                    if last.is_some()
                        && specifier.property_name.and_then(id) == last
                        && let Some(declaration) =
                            nodes.parent(location).and_then(|p| nodes.parent(p))
                        && matches!(node_map.get(declaration), Some(Node::ExportDeclaration(export))
                            if export.module_specifier.is_some())
                        && let Some(outer) = nodes.parent(declaration)
                    {
                        location = outer;
                    }
                }
                _ => {}
            }
            last = Some(location);
            current = nodes.parent(location);
        }
        SuggestionWalk::Globals
    }

    /// `r.lookup(getSymbolOfDeclaration(container).Members, name, meaning & Type)`.
    fn member_suggestion(
        &self,
        container: NodeId,
        name: &str,
        meaning: SymbolFlags,
        host: &mut dyn SuggestionHost<'a>,
    ) -> Option<SymbolId> {
        let owner = self.symbol_of(container)?;
        let members = self.symbols.get(self.merged_symbol(owner)).members.as_ref()?;
        host.lookup(members, name, meaning & SymbolFlags::TYPE)
    }

    /// `GetLocalSymbolForExportDefault(result).Name == name`, read as
    /// [`BindResult::resolve_name`]'s default arm reads it: the container's
    /// `locals[name]` marker whose export symbol is that `default`, the first
    /// declaration carrying the syntactic `default` modifier.
    fn local_name_of_export_default(
        &self,
        default: SymbolId,
        container: NodeId,
        name: &str,
        node_map: &NodeMap<'a>,
    ) -> bool {
        self.symbols.get(default).declarations.first().is_some_and(|&declaration| {
            node_map.get(declaration).and_then(crate::binder::modifiers_of).is_some_and(
                |modifiers| crate::binder::has_modifier(modifiers, SyntaxKind::DefaultKeyword),
            )
        }) && self
            .locals
            .get(&container)
            .and_then(|locals| locals.get(name))
            .is_some_and(|&local| self.symbols.get(local).export_symbol == Some(default))
    }

    /// The `useResult` block of `NameResolver.Resolve`
    /// (`nameresolver.go:52`–`:92`) for a locals hit.
    #[allow(clippy::too_many_arguments)]
    fn use_local_suggestion(
        &self,
        result: SymbolId,
        location: NodeId,
        last: Option<NodeId>,
        meaning: SymbolFlags,
        nodes: &NodeTable,
        node_map: &NodeMap<'a>,
        host: &mut dyn SuggestionHost<'a>,
    ) -> bool {
        let entry = self.symbols.get(result);
        let flags = entry.flags;
        let typed = node_map.get(location);
        if let Some(Node::ConditionalTypeNode(conditional)) = typed {
            return last.is_some() && conditional.true_type.and_then(id) == last;
        }
        let Some((body, return_type)) = function_like_parts(typed) else { return true };
        let Some(last) = last else { return true };
        if body == Some(last) {
            return true;
        }
        let last_kind = nodes.kind(last);
        let synthesized = nodes.flags(last).contains(tsr_ast::NodeFlags::SYNTHESIZED);
        let mut use_result = true;
        if meaning.intersects(flags & SymbolFlags::TYPE) && last_kind != SyntaxKind::JSDoc {
            use_result = flags.contains(SymbolFlags::TYPE_PARAMETER)
                && (synthesized
                    || return_type == Some(last)
                    || matches!(
                        last_kind,
                        SyntaxKind::Parameter
                            | SyntaxKind::JSDocParameterTag
                            | SyntaxKind::JSDocReturnTag
                            | SyntaxKind::TypeParameter
                    ));
        }
        if meaning.intersects(flags & SymbolFlags::VARIABLE) {
            if use_outer_variable_scope_in_parameter(
                entry.value_declaration,
                location,
                body,
                last,
                nodes,
                host,
            ) {
                use_result = false;
            } else if flags.contains(SymbolFlags::FUNCTION_SCOPED_VARIABLE) {
                use_result = last_kind == SyntaxKind::Parameter
                    || synthesized
                    || return_type == Some(last)
                        && entry.value_declaration.is_some_and(|declaration| {
                            std::iter::once(declaration)
                                .chain(nodes.ancestors(declaration))
                                .any(|node| nodes.kind(node) == SyntaxKind::Parameter)
                        });
            }
        }
        use_result
    }
}

/// `useOuterVariableScopeInParameter` (`nameresolver.go:345`).
fn use_outer_variable_scope_in_parameter(
    value_declaration: Option<NodeId>,
    location: NodeId,
    body: Option<NodeId>,
    last: NodeId,
    nodes: &NodeTable,
    host: &mut dyn SuggestionHost<'_>,
) -> bool {
    if nodes.kind(last) != SyntaxKind::Parameter {
        return false;
    }
    let (Some(body), Some(declaration)) = (body, value_declaration) else { return false };
    let (body, declaration) = (nodes.span(body), nodes.span(declaration));
    declaration.start >= body.start
        && declaration.end <= body.end
        && !host.declaration_requires_scope_change(location)
}

/// `ast.IsClassElement`.
fn is_class_element(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::Constructor
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::IndexSignature
            | SyntaxKind::ClassStaticBlockDeclaration
            | SyntaxKind::SemicolonClassElement
    )
}
