//! The checks whose outcome depends on `target` and `useDefineForClassFields`
//! (`getEmitScriptTarget`, `GetEmitStandardClassFields`): the parts of name
//! resolution and class checking where downlevel emit moves code and so
//! changes what a name may see.
//!
//! `docs/parity/notes/r5-classfields.md`.

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{SuggestionHost, SymbolFlags};
use tsr_diagnostics::{Diagnostic, messages};

use crate::checker::Checker;

impl Checker<'_, '_> {
    /// `CompilerOptions.GetEmitStandardClassFields` (`core/compileroptions.go`):
    /// `useDefineForClassFields` (defaulting on from ES2022) **and** a target
    /// of at least ES2022. `standard_class_fields` holds only the first half.
    pub(crate) fn get_emit_standard_class_fields(&self) -> bool {
        self.standard_class_fields && self.language_version >= tsr_core::ScriptTarget::ES2022
    }

    /// TS2373 — `Parameter '{0}' cannot reference identifier '{1}' declared
    /// after it.` — for a name the function's **body** declares.
    ///
    /// `onSuccessfullyResolvedSymbol` (`checker.go:1850`-`:1858`) with the
    /// walk state of `NameResolver.Resolve` (`binder/nameresolver.go`): the
    /// associated declaration is the first parameter or parameter binding
    /// element reached through its initializer or binding-pattern name, and
    /// `withinDeferredContext` is `getIsDeferredContext` over every location
    /// passed before the function whose locals hold the result.
    ///
    /// The result is a body declaration only when `useResult` keeps it: a
    /// non-variable (class, function, enum) always, a variable only when
    /// `useOuterVariableScopeInParameter` is false — the function's
    /// parameters need a scope change (`requiresScopeChange`: a static field
    /// without standard class fields, `?.`/`??` below ES2020, an object rest
    /// below ES2017). A later *parameter* is
    /// [`Checker::check_parameter_self_reference`]'s. §1.
    pub(crate) fn check_parameter_reference_to_body_declaration(
        &mut self,
        node: NodeId,
        text: &str,
    ) {
        let Some((associated, function)) = self.parameter_initializer_scope(node) else { return };
        if !self.is_value_reference(node) {
            return;
        }
        let Some(locals) = self.binder.locals(function) else { return };
        let Some(&candidate) = locals.get(text) else { return };
        let candidate = self.binder.merged_symbol(candidate);
        let symbol = self.binder.symbols().get(candidate);
        if !symbol.flags.intersects(SymbolFlags::VALUE) {
            return;
        }
        let Some(value_declaration) = symbol.value_declaration else { return };
        // A parameter of the same list is the self-reference check's.
        if self.nodes.kind(value_declaration) == SyntaxKind::Parameter
            || symbol.flags.contains(SymbolFlags::FUNCTION_SCOPED_VARIABLE)
                && self
                    .nodes
                    .ancestors(value_declaration)
                    .take_while(|&ancestor| ancestor != function)
                    .any(|ancestor| self.nodes.kind(ancestor) == SyntaxKind::Parameter)
        {
            return;
        }
        if symbol.flags.intersects(SymbolFlags::VARIABLE)
            && !self.declaration_requires_scope_change(function)
        {
            return;
        }
        if self.nodes.span(value_declaration).start <= self.nodes.span(associated).start {
            return;
        }
        // The walk above stands in for the resolver's; the symbol it found
        // must be the one resolution answers (an IIFE between may shadow it).
        if self
            .binder
            .resolve_name(self.nodes, self.node_map, node, text, SymbolFlags::VALUE)
            .map(|resolved| self.binder.merged_symbol(resolved))
            != Some(candidate)
        {
            return;
        }
        let name = match self.node_map.get(associated) {
            Some(Node::ParameterDeclaration(parameter)) => parameter.name,
            Some(Node::BindingElement(element)) => element.name,
            _ => None,
        };
        let Some(name) = name.and_then(|name| Node::from(name).node_id()) else { return };
        let name_text = self.binding_name_text(name);
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::PARAMETER_0_CANNOT_REFERENCE_IDENTIFIER_1_DECLARED_AFTER_IT,
                span,
                [name_text, text.to_string()],
            ),
        );
    }

    /// The associated declaration and the function-like whose parameter it
    /// belongs to, for a name read outside every deferred context between.
    fn parameter_initializer_scope(&self, node: NodeId) -> Option<(NodeId, NodeId)> {
        let mut associated: Option<NodeId> = None;
        let mut last = node;
        for location in self.nodes.ancestors(node) {
            if self.is_function_like_or_static_block(location)
                && self.nodes.kind(last) == SyntaxKind::Parameter
            {
                return associated.map(|associated| (associated, location));
            }
            if self.is_deferred_context(location, last) {
                return None;
            }
            match self.node_map.get(location) {
                Some(Node::ParameterDeclaration(parameter)) if associated.is_none() => {
                    let through_initializer =
                        parameter.initializer.and_then(|e| e.node_id()) == Some(last);
                    let through_pattern = matches!(
                        self.nodes.kind(last),
                        SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern
                    ) && parameter.name.and_then(|n| Node::from(n).node_id())
                        == Some(last);
                    if through_initializer || through_pattern {
                        associated = Some(location);
                    }
                }
                Some(Node::BindingElement(element)) if associated.is_none() => {
                    let through_initializer =
                        element.initializer.and_then(|e| e.node_id()) == Some(last);
                    let through_pattern = matches!(
                        self.nodes.kind(last),
                        SyntaxKind::ObjectBindingPattern | SyntaxKind::ArrayBindingPattern
                    ) && element.name.and_then(|n| Node::from(n).node_id())
                        == Some(last);
                    if (through_initializer || through_pattern)
                        && self.binding_root_is_parameter(location)
                    {
                        associated = Some(location);
                    }
                }
                _ => {}
            }
            last = location;
        }
        None
    }

    /// `getIsDeferredContext` (`binder/nameresolver.go:459`).
    fn is_deferred_context(&self, location: NodeId, last: NodeId) -> bool {
        let kind = self.nodes.kind(location);
        let named_by_last = self.declaration_name_of(location) == Some(last);
        if !matches!(kind, SyntaxKind::ArrowFunction | SyntaxKind::FunctionExpression) {
            let deferring = match kind {
                SyntaxKind::TypeQuery => return true,
                SyntaxKind::PropertyDeclaration => !matches!(
                    self.node_map.get(location),
                    Some(Node::PropertyDeclaration(property))
                        if tsr_ast::has_syntactic_modifier(property.modifiers, SyntaxKind::StaticKeyword)
                ),
                SyntaxKind::FunctionDeclaration
                | SyntaxKind::MethodDeclaration
                | SyntaxKind::Constructor
                | SyntaxKind::GetAccessor
                | SyntaxKind::SetAccessor => true,
                _ => false,
            };
            return deferring && !named_by_last;
        }
        if named_by_last {
            return false;
        }
        let (asterisk, modifiers) = match self.node_map.get(location) {
            Some(Node::FunctionExpression(function)) => {
                (function.asterisk_token.is_some(), function.modifiers)
            }
            Some(Node::ArrowFunction(function)) => (false, function.modifiers),
            _ => return false,
        };
        if asterisk || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AsyncKeyword) {
            return true;
        }
        self.immediately_invoked_call(location).is_none()
    }

    /// `DeclarationNameToString` for a parameter or binding element name.
    /// Upstream prints the pattern's source text, which the checker cannot
    /// reach; a pattern is rebuilt from its element names (§2).
    fn binding_name_text(&self, name: NodeId) -> String {
        match self.node_map.get(name) {
            Some(Node::Identifier(identifier)) => identifier.text.to_string(),
            Some(Node::BindingPattern(pattern)) => {
                let parts: Vec<String> = pattern
                    .elements
                    .iter()
                    .filter_map(|element| element.node_id)
                    .map(|element| self.binding_element_text(element))
                    .collect();
                if self.nodes.kind(name) == SyntaxKind::ObjectBindingPattern {
                    format!("{{ {} }}", parts.join(", "))
                } else {
                    format!("[{}]", parts.join(", "))
                }
            }
            _ => String::new(),
        }
    }

    fn binding_element_text(&self, element: NodeId) -> String {
        let Some(Node::BindingElement(element)) = self.node_map.get(element) else {
            return String::new();
        };
        let name = element
            .name
            .and_then(|name| Node::from(name).node_id())
            .map(|name| self.binding_name_text(name))
            .unwrap_or_default();
        let rest = if element.dot_dot_dot_token.is_some() { "..." } else { "" };
        format!("{rest}{name}")
    }

    /// `ast.IsPartOfParameterDeclaration`: the root of a binding element's
    /// pattern chain is a parameter.
    fn binding_root_is_parameter(&self, node: NodeId) -> bool {
        let mut at = node;
        while matches!(
            self.nodes.kind(at),
            SyntaxKind::BindingElement
                | SyntaxKind::ObjectBindingPattern
                | SyntaxKind::ArrayBindingPattern
        ) {
            let Some(parent) = self.nodes.parent(at) else { return false };
            at = parent;
        }
        self.nodes.kind(at) == SyntaxKind::Parameter
    }

    /// TS2818 — `Duplicate identifier 'Reflect'. Compiler reserves name
    /// 'Reflect' when emitting 'super' references in static initializers.`
    ///
    /// `recordPotentialCollisionWithReflectInGeneratedCode` and
    /// `checkReflectCollision` (`checker.go:10574`, `:10582`), reached from
    /// `checkCollisionsForDeclarationName` for every declaration kind that
    /// calls it. Upstream marks each block-scope container enclosing a
    /// `super.x` / `super[x]` in a static initializer
    /// (`NodeCheckFlagsContainsSuperPropertyInStaticInitializer`,
    /// `checkSuperExpression`, `checker.go:7946`) and defers the collision
    /// test until checking is done. This port checks in document order, so a
    /// mark made later in the file would be missed; the mark is instead read
    /// off the tree — a container is marked exactly when such a `super` lies
    /// inside it (§3). `name` is the declaration name identifier being checked.
    pub(crate) fn check_reflect_collision(&mut self, name: NodeId, text: &str) {
        if text != "Reflect" || self.language_version > tsr_core::ScriptTarget::ES2021 {
            return;
        }
        let Some(declaration) = self.nodes.parent(name) else { return };
        if !self.declares_collision_checked_name(declaration, name)
            || !self.need_collision_check_for_identifier(declaration)
        {
            return;
        }
        let has_collision = match self.node_map.get(declaration) {
            Some(Node::ClassExpression(class)) => class
                .members
                .iter()
                .filter_map(|member| Node::from(*member).node_id())
                .any(|member| self.contains_super_property_in_static_initializer(member)),
            Some(Node::FunctionExpression(_)) => {
                self.contains_super_property_in_static_initializer(declaration)
            }
            _ => self.enclosing_block_scope(declaration).is_some_and(|container| {
                self.contains_super_property_in_static_initializer(container)
            }),
        };
        if !has_collision {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(declaration) else { return };
        let span = self.error_span(declaration);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::DUPLICATE_IDENTIFIER_0_COMPILER_RESERVES_NAME_1_WHEN_EMITTING_SUPER_REFERENCES_IN_STATIC_INITIALIZERS,
                span,
                [text.to_string(), "Reflect".to_string()],
            ),
        );
    }

    /// Whether `name` is the name `checkCollisionsForDeclarationName` is
    /// called with for `declaration`: the call sites in
    /// `checkFunctionDeclaration`, `checkClassLikeDeclaration`,
    /// `checkEnumDeclaration`, `checkModuleDeclaration`, `checkImportBinding`,
    /// `checkVariableLikeDeclaration` and
    /// `checkFunctionExpressionOrObjectLiteralMethod`.
    fn declares_collision_checked_name(&self, declaration: NodeId, name: NodeId) -> bool {
        let named = |slot: Option<NodeId>| slot == Some(name);
        match self.node_map.get(declaration) {
            Some(Node::FunctionDeclaration(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::FunctionExpression(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::ClassDeclaration(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::ClassExpression(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::EnumDeclaration(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::ModuleDeclaration(n)) => {
                n.name.and_then(|n| Node::from(n).node_id()) == Some(name)
            }
            Some(Node::ImportClause(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::NamespaceImport(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::ImportSpecifier(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::ImportEqualsDeclaration(n)) => named(n.name.and_then(|n| n.node_id)),
            Some(Node::VariableDeclaration(n)) => {
                n.name.and_then(|n| Node::from(n).node_id()) == Some(name)
            }
            Some(Node::BindingElement(n)) => {
                n.name.and_then(|n| Node::from(n).node_id()) == Some(name)
            }
            Some(Node::ParameterDeclaration(n)) => {
                n.name.and_then(|n| Node::from(n).node_id()) == Some(name)
            }
            _ => false,
        }
    }

    /// `needCollisionCheckForIdentifier` (`checker.go:10499`) past the name
    /// test: not ambient, not a type-only import, not an overload's parameter.
    fn need_collision_check_for_identifier(&self, declaration: NodeId) -> bool {
        if self.declaration_is_in_an_ambient_context(declaration) {
            return false;
        }
        let type_only_clause = |clause: Option<NodeId>| {
            matches!(clause.and_then(|clause| self.node_map.get(clause)),
                Some(Node::ImportClause(clause))
                    if clause.phase_modifier.is_some_and(|token| token.kind == SyntaxKind::TypeKeyword))
        };
        match self.node_map.get(declaration) {
            Some(Node::ImportClause(_)) => !type_only_clause(Some(declaration)),
            Some(Node::NamespaceImport(_)) => !type_only_clause(self.nodes.parent(declaration)),
            Some(Node::ImportSpecifier(specifier)) => {
                let clause = self
                    .nodes
                    .parent(declaration)
                    .and_then(|named_imports| self.nodes.parent(named_imports));
                !specifier.is_type_only && !type_only_clause(clause)
            }
            Some(Node::ImportEqualsDeclaration(import)) => !import.is_type_only,
            Some(Node::ParameterDeclaration(_)) => {
                // `ast.NodeIsMissing(root.Parent.Body())`: an overload.
                let body = self.nodes.parent(declaration).and_then(|function| {
                    match self.node_map.get(function)? {
                        Node::FunctionDeclaration(n) => n.body.map(|_| ()),
                        Node::MethodDeclaration(n) => n.body.map(|_| ()),
                        Node::ConstructorDeclaration(n) => n.body.map(|_| ()),
                        Node::GetAccessorDeclaration(n) => n.body.map(|_| ()),
                        Node::SetAccessorDeclaration(n) => n.body.map(|_| ()),
                        Node::FunctionExpression(_) | Node::ArrowFunction(_) => Some(()),
                        _ => None,
                    }
                });
                body.is_some()
            }
            _ => true,
        }
    }

    /// `ast.GetEnclosingBlockScopeContainer`.
    fn enclosing_block_scope(&self, node: NodeId) -> Option<NodeId> {
        self.nodes.ancestors(node).find(|&ancestor| match self.nodes.kind(ancestor) {
            SyntaxKind::Block => !self
                .nodes
                .parent(ancestor)
                .is_some_and(|parent| self.is_function_like_or_static_block(parent)),
            SyntaxKind::SourceFile
            | SyntaxKind::CaseBlock
            | SyntaxKind::CatchClause
            | SyntaxKind::ModuleDeclaration
            | SyntaxKind::ForStatement
            | SyntaxKind::ForInStatement
            | SyntaxKind::ForOfStatement
            | SyntaxKind::Constructor
            | SyntaxKind::MethodDeclaration
            | SyntaxKind::GetAccessor
            | SyntaxKind::SetAccessor
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::FunctionExpression
            | SyntaxKind::ArrowFunction
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::ClassStaticBlockDeclaration => true,
            _ => false,
        })
    }

    /// `NodeCheckFlagsContainsSuperPropertyInStaticInitializer` on the
    /// block-scope container `container`: some `super.x` or `super[x]` inside
    /// it has a static property or static block as its (arrow-adjusted)
    /// `super` container, in a class with a base. A script file is never
    /// marked (`!ast.IsSourceFile(current) || IsExternalOrCommonJSModule`).
    fn contains_super_property_in_static_initializer(&self, container: NodeId) -> bool {
        if let Some(Node::SourceFile(source)) = self.node_map.get(container)
            && !tsr_binder::is_external_module(source)
        {
            return false;
        }
        let mut stack = vec![container];
        while let Some(node) = stack.pop() {
            if self.nodes.kind(node) == SyntaxKind::SuperKeyword
                && self.is_super_property_in_static_initializer(node)
            {
                return true;
            }
            if let Some(typed) = self.node_map.get(node) {
                tsr_ast::for_each_child_id(typed, |child| stack.push(child));
            }
        }
        false
    }

    /// The marking arm of `checkSuperExpression` (`checker.go:7946`) for one
    /// `super` keyword, past the legality tests a static member passes.
    fn is_super_property_in_static_initializer(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let is_property = match self.node_map.get(parent) {
            Some(Node::PropertyAccessExpression(access)) => {
                access.expression.and_then(|e| e.node_id()) == Some(node)
            }
            Some(Node::ElementAccessExpression(access)) => {
                access.expression.and_then(|e| e.node_id()) == Some(node)
            }
            _ => false,
        };
        if !is_property {
            return false;
        }
        // `getSuperContainer(node, true)`, adjusted through arrow functions;
        // a computed property name on the way makes the use illegal.
        for ancestor in self.nodes.ancestors(node) {
            match self.nodes.kind(ancestor) {
                SyntaxKind::ArrowFunction => {}
                SyntaxKind::ComputedPropertyName => return false,
                SyntaxKind::PropertyDeclaration | SyntaxKind::ClassStaticBlockDeclaration => {
                    let is_static = match self.node_map.get(ancestor) {
                        Some(Node::PropertyDeclaration(property)) => {
                            tsr_ast::has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::StaticKeyword,
                            )
                        }
                        _ => true,
                    };
                    let Some(class) = self.nodes.parent(ancestor) else { return false };
                    return is_static
                        && self.class_has_extends_clause(class)
                        && !self.class_declaration_extends_null(class);
                }
                kind if self.is_function_like_or_static_block(ancestor)
                    || matches!(
                        kind,
                        SyntaxKind::ClassDeclaration
                            | SyntaxKind::ClassExpression
                            | SyntaxKind::SourceFile
                    ) =>
                {
                    return false;
                }
                _ => {}
            }
        }
        false
    }

    fn class_has_extends_clause(&self, class: NodeId) -> bool {
        let clauses = match self.node_map.get(class) {
            Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
            Some(Node::ClassExpression(class)) => class.heritage_clauses,
            _ => return false,
        };
        clauses.iter().any(|clause| {
            clause.token.kind == SyntaxKind::ExtendsKeyword && !clause.types.is_empty()
        })
    }
}
