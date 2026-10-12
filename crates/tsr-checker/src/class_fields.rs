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
    /// after it.` — for a name the function's **body** declares, and TS2372 /
    /// TS2373 under a parameter binding element.
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
    /// below ES2017). A parameter-list name reached from a parameter is
    /// [`Checker::check_parameter_self_reference`]'s; reached from a parameter
    /// binding element (`([c, e = e])`, `([h = i, i])`) it is this check's,
    /// TS2372 when it is the element itself. §2.
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
        let is_variable = symbol.flags.intersects(SymbolFlags::VARIABLE);
        // A name the parameter list declares. With a parameter as the
        // associated declaration that is [`Checker::check_parameter_self_reference`]'s;
        // with a parameter binding element it is this check's, which that one
        // never reaches.
        let declared_by_parameters = self.nodes.kind(value_declaration) == SyntaxKind::Parameter
            || self
                .nodes
                .ancestors(value_declaration)
                .take_while(|&ancestor| ancestor != function)
                .any(|ancestor| self.nodes.kind(ancestor) == SyntaxKind::Parameter);
        let through_binding_element = self.nodes.kind(associated) == SyntaxKind::BindingElement;
        if declared_by_parameters && !through_binding_element {
            return;
        }
        // `useOuterVariableScopeInParameter`: a body variable is passed over
        // unless the parameters need a scope change.
        if !declared_by_parameters
            && is_variable
            && !self.declaration_requires_scope_change(function)
        {
            return;
        }
        let refers_to_itself = through_binding_element
            && self.binder.symbol_of(associated).map(|own| self.binder.merged_symbol(own))
                == Some(candidate);
        if !refers_to_itself
            && self.nodes.span(value_declaration).start <= self.nodes.span(associated).start
        {
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
        // Upstream's `if` / `else if`: the declaration naming itself first.
        let diagnostic = if refers_to_itself {
            Diagnostic::with_args(&messages::PARAMETER_0_CANNOT_REFERENCE_ITSELF, span, [name_text])
        } else {
            Diagnostic::with_args(
                &messages::PARAMETER_0_CANNOT_REFERENCE_IDENTIFIER_1_DECLARED_AFTER_IT,
                span,
                [name_text, text.to_string()],
            )
        };
        self.report(file, diagnostic);
    }

    /// The associated declaration and the function-like whose parameter it
    /// belongs to, for a name read outside every deferred context between.
    pub(crate) fn parameter_initializer_scope(&self, node: NodeId) -> Option<(NodeId, NodeId)> {
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

    /// `DeclarationNameToString` for a parameter or binding element name: the
    /// name's source text (`declaration_name_source_text`, r7-grammar §35), so
    /// a pattern prints as written, computed keys and defaults included.
    /// Without source text a pattern is rebuilt from its element names (§2).
    /// Correction of the record: this used to say the checker cannot reach
    /// the source text, and always rebuilt the pattern (r7-grammar §54).
    fn binding_name_text(&self, name: NodeId) -> String {
        if let Some(text) = self.declaration_name_source_text(name) {
            return text;
        }
        self.rebuilt_binding_name_text(name)
    }

    fn rebuilt_binding_name_text(&self, name: NodeId) -> String {
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
            .map(|name| self.rebuilt_binding_name_text(name))
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
        self.report_skipped_on_no_emit(
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

impl Checker<'_, '_> {
    /// `isPropertyInitializedInConstructor` (`checker.go:4977`): a synthetic
    /// `this.<name>` (`this[<expr>]` for a computed name) parented to the
    /// constructor, flowed from its `ReturnFlowNode` with `propType` declared
    /// and `getOptionalType(propType)` initial; initialized when `undefined`
    /// does not survive. The query is r7-flow's
    /// [`Checker::flow_type_of_this_property_at`], which matches the access
    /// by name. A computed name is spelled when its expression is a string,
    /// numeric or no-substitution template literal; any other computed name
    /// has no spelling there, and keeps §87's decidable part instead: a body
    /// that never names it cannot assign it (`subtree_accesses_this_member`,
    /// whose every uncertain answer declines). No cache: one walk per
    /// property, as native (r7-grammar §29). Lives here rather than beside its check.rs caller, which the lane may edit only inside `check_property_initialization`.
    pub(crate) fn is_property_initialized_in_constructor(
        &mut self,
        body: NodeId,
        name: tsr_ast::PropertyName<'_>,
        text: &str,
        declared: crate::types::TypeId,
    ) -> bool {
        let spelled = match name {
            tsr_ast::PropertyName::ComputedPropertyName(computed) => match computed.expression {
                Some(tsr_ast::Expression::StringLiteral(literal)) => Some(literal.text.to_string()),
                Some(tsr_ast::Expression::NumericLiteral(literal)) => {
                    Some(literal.text.to_string())
                }
                Some(tsr_ast::Expression::NoSubstitutionTemplateLiteral(literal)) => {
                    Some(literal.text.to_string())
                }
                _ => None,
            },
            _ => Some(text.to_string()),
        };
        let (Some(spelled), Some(constructor)) = (spelled, self.nodes.parent(body)) else {
            return self.subtree_accesses_this_member(body, text, 0);
        };
        let initial = self.get_optional_type(declared, false);
        let flowed = self.flow_type_of_this_property_at(constructor, &spelled, declared, initial);
        !self.contains_undefined_type(flowed)
    }
}

impl Checker<'_, '_> {
    /// `superCallShouldBeRootLevel` (`checker.go:2860`): not
    /// `emitStandardClassFields`, and the class has an instance property with
    /// an initializer or any private-named element
    /// (`isInstancePropertyWithInitializerOrPrivateIdentifierProperty`,
    /// `:2906`), or the constructor has a parameter with a parameter-property
    /// modifier (`ModifierFlagsParameterPropertyModifier`: an accessibility
    /// modifier, `readonly` or `override`). The caller,
    /// `check_super_call_is_first` (check.rs), reports TS2376 behind it
    /// (r7-grammar §33).
    pub(crate) fn super_call_should_be_root_level(
        &self,
        members: &[tsr_ast::ClassElement<'_>],
        constructor: &tsr_ast::ConstructorDeclaration<'_>,
    ) -> bool {
        if self.get_emit_standard_class_fields() {
            return false;
        }
        let member_has_state = members.iter().any(|member| {
            let private_named = match member {
                tsr_ast::ClassElement::PropertyDeclaration(n) => {
                    matches!(n.name, tsr_ast::PropertyName::PrivateIdentifier(_))
                }
                tsr_ast::ClassElement::MethodDeclaration(n) => {
                    matches!(n.name, tsr_ast::PropertyName::PrivateIdentifier(_))
                }
                tsr_ast::ClassElement::GetAccessorDeclaration(n) => {
                    matches!(n.name, tsr_ast::PropertyName::PrivateIdentifier(_))
                }
                tsr_ast::ClassElement::SetAccessorDeclaration(n) => {
                    matches!(n.name, tsr_ast::PropertyName::PrivateIdentifier(_))
                }
                _ => false,
            };
            private_named
                || matches!(member, tsr_ast::ClassElement::PropertyDeclaration(property)
                    if property.initializer.is_some()
                        && !crate::check::has_modifier(property.modifiers, SyntaxKind::StaticKeyword))
        });
        member_has_state
            || constructor.parameters.iter().any(|parameter| {
                parameter.modifiers.iter().any(|modifier| {
                    matches!(modifier, tsr_ast::ModifierLike::Token(token) if matches!(
                        token.kind,
                        SyntaxKind::PublicKeyword
                            | SyntaxKind::PrivateKeyword
                            | SyntaxKind::ProtectedKeyword
                            | SyntaxKind::ReadonlyKeyword
                            | SyntaxKind::OverrideKeyword
                    ))
                })
            })
    }

    /// `superCallIsRootLevelInConstructor` (`checker.go:2910`): the call's
    /// parent, above any parentheses, is an expression statement directly in
    /// the constructor body.
    pub(crate) fn super_call_is_root_level_in_constructor(
        &self,
        call: NodeId,
        body: NodeId,
    ) -> bool {
        let mut parent = self.nodes.parent(call);
        while let Some(at) = parent
            && self.nodes.kind(at) == SyntaxKind::ParenthesizedExpression
        {
            parent = self.nodes.parent(at);
        }
        parent.is_some_and(|statement| {
            self.nodes.kind(statement) == SyntaxKind::ExpressionStatement
                && self.nodes.parent(statement) == Some(body)
        })
    }

    /// `nodeImmediatelyReferencesSuperOrThis` (`checker.go:2915`): `super` or
    /// `this` anywhere below, not entering an arrow, function declaration or
    /// expression, or property declaration, nor the body block of a
    /// constructor, method or accessor (whose name and parameters are still
    /// visited).
    pub(crate) fn node_immediately_references_super_or_this(&self, node: NodeId) -> bool {
        match self.nodes.kind(node) {
            SyntaxKind::SuperKeyword | SyntaxKind::ThisKeyword => return true,
            SyntaxKind::ArrowFunction
            | SyntaxKind::FunctionDeclaration
            | SyntaxKind::FunctionExpression
            | SyntaxKind::PropertyDeclaration => return false,
            SyntaxKind::Block
                if self.nodes.parent(node).is_some_and(|parent| {
                    matches!(
                        self.nodes.kind(parent),
                        SyntaxKind::Constructor
                            | SyntaxKind::MethodDeclaration
                            | SyntaxKind::GetAccessor
                            | SyntaxKind::SetAccessor
                    )
                }) =>
            {
                return false;
            }
            _ => {}
        }
        let mut children = Vec::new();
        if let Some(typed) = self.node_map.get(node) {
            tsr_ast::for_each_child_id(typed, |child| children.push(child));
        }
        children.into_iter().any(|child| self.node_immediately_references_super_or_this(child))
    }

    /// `scanner.DeclarationNameToString` (`scanner/utilities.go`) for a
    /// present name: `GetTextOfNode`, the name's source text from its
    /// trivia-free start, so `[x = 0]` prints as written. `None` without
    /// source text or for a missing (zero-width) name, where the caller's
    /// spelling stands (r7-grammar §35).
    pub(crate) fn declaration_name_source_text(&self, name: NodeId) -> Option<String> {
        let span = self.nodes.span(name);
        if span.start == span.end {
            return None;
        }
        let file = self.source_file_of_for_diagnostics(name)?;
        let text = self.module_host.and_then(|host| host.source_text(file, self.nodes))?;
        text.get(span.start as usize..span.end as usize).map(str::to_string)
    }

    /// `GetErrorRangeForNode`'s `KindConstructor` arm (`scanner.go:2632`):
    /// from the constructor's first token (modifiers included) to the end of
    /// the `constructor` keyword, or of a string-literal `"constructor"`
    /// name. Scans the source on the diagnostic path; without source text,
    /// the node's span. The shared `error_span` has no such arm (routed);
    /// this lane's constructor reports use this (r7-grammar §35).
    pub(crate) fn constructor_error_span(&self, constructor: NodeId) -> tsr_core::Span {
        let span = self.nodes.span(constructor);
        let Some(file) = self.source_file_of_for_diagnostics(constructor) else { return span };
        let Some(rest) = self
            .module_host
            .and_then(|host| host.source_text(file, self.nodes))
            .and_then(|text| text.get(span.start as usize..span.end as usize))
        else {
            return span;
        };
        let mut scanner = tsr_scanner::Scanner::new(rest);
        loop {
            let token = scanner.scan();
            match token.kind {
                SyntaxKind::ConstructorKeyword | SyntaxKind::StringLiteral => {
                    return tsr_core::Span::new(span.start, span.start + token.span.end);
                }
                SyntaxKind::EndOfFile => return span,
                _ => {}
            }
        }
    }
}
