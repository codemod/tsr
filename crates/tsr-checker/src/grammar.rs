//! Grammar checks ported from typescript-go's `internal/checker/grammarchecks.go`
//! that the parser lane owns.
//!
//! Each one is a `grammarError*` report: it stands only in a file without
//! parse diagnostics, which the caller (`check_grammar_modifier_shapes`)
//! already guarantees.

use tsr_ast::{
    Expression, ModifierFlags, ModifierLike, Node, NodeFlags, NodeId, ObjectLiteralElementLike,
    Statement, SyntaxKind, TypeNode,
};
use tsr_diagnostics::{Diagnostic, messages};

use crate::check::has_modifier;
use crate::checker::Checker;

impl Checker<'_, '_> {
    /// The modifier arm of typescript-go's
    /// `Checker.checkGrammarObjectLiteralExpression` (`grammarchecks.go`):
    /// modifiers are never allowed on object literal members except `async`
    /// on a method, and each one is reported as TS1042.
    pub(crate) fn check_grammar_object_literal_modifiers(&mut self, typed: Node<'_>) {
        let Node::ObjectLiteralExpression(literal) = typed else { return };
        for property in literal.properties {
            let (modifiers, is_method) = match property {
                ObjectLiteralElementLike::MethodDeclaration(method) => (method.modifiers, true),
                ObjectLiteralElementLike::GetAccessorDeclaration(accessor) => {
                    (accessor.modifiers, false)
                }
                ObjectLiteralElementLike::SetAccessorDeclaration(accessor) => {
                    (accessor.modifiers, false)
                }
                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    (assignment.modifiers, false)
                }
                ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    (shorthand.modifiers, false)
                }
                ObjectLiteralElementLike::SpreadAssignment(_) => continue,
            };
            for modifier in modifiers {
                let ModifierLike::Token(token) = modifier else { continue };
                if is_method && token.kind == SyntaxKind::AsyncKeyword {
                    continue;
                }
                let Some(id) = token.node_id else { continue };
                self.report_modifier_cannot_be_used_here(id, token.kind);
            }
        }
    }

    /// `ast.NodeCanBeDecorated(c.legacyDecorators, node, node.Parent,
    /// node.Parent.Parent)` (`ast/utilities.go:4254`), with `CanHaveDecorators`
    /// folded in as the kinds it lists. Shared by `checkGrammarModifiers`'
    /// decorator arm (below) and `markDecoratorAliasReferenced`
    /// (`isolated_alias.rs`); r6-isolated's copy there was folded into this
    /// one (r6-modules2 §8).
    pub(crate) fn node_can_be_decorated(&self, node: NodeId, typed: Node<'_>) -> bool {
        let legacy = self.legacy_decorators;
        let parent = self.nodes.parent(node);
        let parent_kind = parent.map(|parent| self.nodes.kind(parent));
        let parent_is_class_declaration = parent_kind == Some(SyntaxKind::ClassDeclaration);
        let parent_is_class_like =
            matches!(parent_kind, Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression));
        let private_name = |name: tsr_ast::PropertyName<'_>| {
            matches!(name, tsr_ast::PropertyName::PrivateIdentifier(_))
        };
        let class_member = |body: bool| {
            body && (if legacy { parent_is_class_declaration } else { parent_is_class_like })
        };
        match typed {
            Node::ClassDeclaration(_) => true,
            Node::ClassExpression(_) => !legacy,
            Node::PropertyDeclaration(property) => {
                !(legacy && private_name(property.name))
                    && ((legacy && parent_is_class_declaration)
                        || (!legacy
                            && parent_is_class_like
                            && !tsr_ast::has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::AbstractKeyword,
                            )
                            && !tsr_ast::has_syntactic_modifier(
                                property.modifiers,
                                SyntaxKind::DeclareKeyword,
                            )))
            }
            Node::MethodDeclaration(method) => {
                !(legacy && private_name(method.name)) && class_member(method.body.is_some())
            }
            Node::GetAccessorDeclaration(accessor) => {
                !(legacy && private_name(accessor.name)) && class_member(accessor.body.is_some())
            }
            Node::SetAccessorDeclaration(accessor) => {
                !(legacy && private_name(accessor.name)) && class_member(accessor.body.is_some())
            }
            Node::ParameterDeclaration(_) => {
                if !legacy {
                    return false;
                }
                let Some(parent) = parent else { return false };
                let (body, parameters) = match self.node_map.get(parent) {
                    Some(Node::ConstructorDeclaration(n)) => (n.body.is_some(), n.parameters),
                    Some(Node::MethodDeclaration(n)) => (n.body.is_some(), n.parameters),
                    Some(Node::SetAccessorDeclaration(n)) => (n.body.is_some(), n.parameters),
                    _ => return false,
                };
                // `GetThisParameter(parent) != node`.
                let this_parameter = parameters.first().filter(|first| {
                    matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
                });
                body && this_parameter.and_then(|first| first.node_id) != Some(node)
                    && self.nodes.parent(parent).is_some_and(|grandparent| {
                        self.nodes.kind(grandparent) == SyntaxKind::ClassDeclaration
                    })
            }
            _ => false,
        }
    }

    /// The parser lane's statement checks that upstream reports with
    /// `c.error` rather than `grammarErrorOnNode`, so they stand whether or not
    /// the file has parse diagnostics.
    pub(crate) fn check_parser_lane_statement(&mut self, typed: Node<'_>) {
        // `Checker.checkPropertyDeclaration` (`checker.go:2709`): TS1267, an
        // abstract property with an initializer, on the property's name.
        if let Node::PropertyDeclaration(property) = typed
            && property.initializer.is_some()
            && tsr_ast::has_syntactic_modifier(property.modifiers, SyntaxKind::AbstractKeyword)
            && let Some(id) = property.node_id
            && let Some(file) = self.source_file_of_for_diagnostics(id)
        {
            let name = declaration_name_text(property.name);
            let span = self.error_span(id);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::PROPERTY_0_CANNOT_HAVE_AN_INITIALIZER_BECAUSE_IT_IS_MARKED_ABSTRACT,
                    span,
                    [name],
                ),
            );
        }
        // `Checker.checkExternalImportOrExportDeclaration` (`checker.go:5333`):
        // a module name that is present but not a string literal is TS1141.
        let module_name = match typed {
            Node::ImportDeclaration(declaration) => {
                declaration.module_specifier.and_then(|s| s.node_id())
            }
            Node::ExportDeclaration(declaration) => {
                declaration.module_specifier.and_then(|s| s.node_id())
            }
            Node::ImportEqualsDeclaration(declaration) => match declaration.module_reference {
                Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => {
                    reference.expression.and_then(|e| e.node_id())
                }
                _ => None,
            },
            _ => None,
        };
        if let Some(module_name) = module_name {
            self.check_external_module_name_is_string_literal(module_name);
        }
        if let Node::ClassDeclaration(_) = typed {
            self.check_class_declaration_has_name(typed);
        }
        // `Checker.checkIfStatement` (`checker.go:3808`): TS1313 on an empty
        // `then` statement.
        if let Node::IfStatement(statement) = typed
            && let Some(Statement::EmptyStatement(empty)) = statement.then_statement
            && let Some(id) = empty.node_id
            && let Some(file) = self.source_file_of_for_diagnostics(id)
        {
            let span = self.error_span(id);
            self.report(
                file,
                Diagnostic::new(
                    &messages::THE_BODY_OF_AN_IF_STATEMENT_CANNOT_BE_THE_EMPTY_STATEMENT,
                    span,
                ),
            );
        }
        if let Node::CatchClause(clause) = typed
            && let Some(declaration) = clause.variable_declaration.and_then(|d| d.node_id)
        {
            self.check_catch_clause_declaration(declaration);
        }
        // `Checker.checkTypeAliasDeclaration` (`checker.go:6888`): the parser
        // makes `intrinsic` a keyword type only as a whole alias body, and
        // only `BuiltinIteratorReturn` (no type parameters) and the one-
        // parameter `intrinsicTypeKinds` names may use it (a `c.error`).
        if let Node::TypeAliasDeclaration(alias) = typed
            && let Some(TypeNode::KeywordTypeNode(keyword)) = alias.r#type
            && keyword.kind == SyntaxKind::IntrinsicKeyword
            && let Some(id) = keyword.node_id
        {
            let name = alias.name.map_or("", |name| name.text);
            let allowed = match alias.type_parameters.len() {
                0 => name == "BuiltinIteratorReturn",
                1 => matches!(
                    name,
                    "Uppercase" | "Lowercase" | "Capitalize" | "Uncapitalize" | "NoInfer"
                ),
                _ => false,
            };
            if !allowed && let Some(file) = self.source_file_of_for_diagnostics(id) {
                let span = self.error_span(id);
                self.report(
                    file,
                    Diagnostic::new(
                        &messages::THE_INTRINSIC_KEYWORD_CAN_ONLY_BE_USED_TO_DECLARE_COMPILER_PROVIDED_INTRINSIC_TYPES,
                        span,
                    ),
                );
            }
        }
        // The first arm of `Checker.checkTypePredicate` (`checker.go:3055`):
        // the parser builds a predicate in any type position, and one outside
        // a signature's return type is TS1228 (a `c.error`). The remaining
        // arms (parameter lookup, rest and binding-pattern references) are
        // not ported here.
        if let Node::TypePredicateNode(predicate) = typed
            && let Some(id) = predicate.node_id
            && self.type_predicate_parent(id).is_none()
            && let Some(file) = self.source_file_of_for_diagnostics(id)
        {
            let span = self.error_span(id);
            self.report(
                file,
                Diagnostic::new(
                    &messages::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHODS,
                    span,
                ),
            );
        }
    }

    /// `Checker.getTypePredicateParent` (`checker.go:3099`): the signature
    /// whose return type is exactly this predicate.
    fn type_predicate_parent(&self, node: NodeId) -> Option<NodeId> {
        let parent = self.nodes.parent(node)?;
        let return_type = match self.node_map.get(parent)? {
            Node::ArrowFunction(signature) => signature.r#type,
            Node::CallSignatureDeclaration(signature) => signature.r#type,
            Node::FunctionDeclaration(signature) => signature.r#type,
            Node::FunctionExpression(signature) => signature.r#type,
            Node::FunctionTypeNode(signature) => signature.r#type,
            Node::MethodDeclaration(signature) => signature.r#type,
            Node::MethodSignatureDeclaration(signature) => signature.r#type,
            _ => None,
        };
        (return_type.and_then(|t| t.node_id()) == Some(node))
            .then_some(parent)
            // A reparsed `@returns` type is its function's `Type`
            // (`reparseHosted`, `parser/reparser.go:514`).
            .or_else(|| self.jsdoc_reparsed_return_owner(node))
    }

    /// The first arms of `Checker.checkExternalImportOrExportDeclaration`
    /// (`checker.go:5333`): a missing name is the parser's error, and any
    /// other non-string-literal name is TS1141 — a `c.error`, so it stands in
    /// a file with parse errors, which is where the corpus has it
    /// (`import * from Zero from "./0"` reads `Zero` as the specifier).
    ///
    /// Upstream reaches it after `checkGrammarModuleElementContext`
    /// (`grammarchecks.go:206`), whose caller returns on any parent other than
    /// a source file, module block or module declaration, **whether or not**
    /// its first-token report was silenced by the file's parse diagnostics:
    /// the function returns `!isInAppropriateContext`, not the report's
    /// result. `docs/parity/notes/r6-parsegate.md` §3.
    fn check_external_module_name_is_string_literal(&mut self, module_name: NodeId) {
        if self.nodes.kind(module_name) == SyntaxKind::StringLiteral {
            return;
        }
        // `ast.NodeIsMissing`: an empty span.
        let span = self.nodes.span(module_name);
        if span.start == span.end {
            return;
        }
        let Some(declaration) = self.nodes.ancestors(module_name).find(|&ancestor| {
            matches!(
                self.nodes.kind(ancestor),
                SyntaxKind::ImportDeclaration
                    | SyntaxKind::ExportDeclaration
                    | SyntaxKind::ImportEqualsDeclaration
            )
        }) else {
            return;
        };
        let at_module_level = self.nodes.parent(declaration).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::SourceFile | SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
            )
        });
        if !at_module_level {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(module_name) else { return };
        let span = self.error_span(module_name);
        self.report(file, Diagnostic::new(&messages::STRING_LITERAL_EXPECTED, span));
    }

    /// The parser lane's grammar checks that upstream runs only when
    /// `checkGrammarModifiers(node)` reported nothing — the caller,
    /// `check_grammar_modifier_shapes`, is already behind that guard and the
    /// file's parse diagnostics.
    pub(crate) fn check_grammar_behind_modifiers(&mut self, node: NodeId, typed: Node<'_>) {
        let list = match typed {
            // `checkVariableStatement` (`checker.go:5767`).
            Node::VariableStatement(statement) => {
                statement.declaration_list.and_then(|list| list.node_id)
            }
            // `checkForStatement` (`checker.go:3960`) and
            // `checkGrammarForInOrForOfStatement` (`grammarchecks.go:1256`).
            Node::ForStatement(statement) => statement.initializer.and_then(|i| i.node_id()),
            Node::ForInOrOfStatement(statement) => {
                self.check_grammar_for_of_async(node, statement);
                statement.initializer.and_then(|i| i.node_id())
            }
            Node::PropertyDeclaration(_) | Node::PropertySignatureDeclaration(_) => {
                self.check_grammar_property(node, typed);
                None
            }
            Node::VariableDeclaration(declaration) => {
                self.check_grammar_variable_declaration_exclamation(node, declaration);
                None
            }
            Node::ThrowStatement(statement) => {
                self.check_grammar_throw_expression(node, statement);
                None
            }
            Node::TypeOperatorNode(operator) => {
                self.check_grammar_type_operator_node(node, operator);
                None
            }
            Node::JSDocNullableType(_) | Node::JSDocNonNullableType(_) => {
                self.check_jsdoc_type_is_in_js_file(node);
                None
            }
            Node::IndexSignatureDeclaration(_) => {
                if let Some((at, message)) = self.index_signature_parameter_shape_error(node) {
                    self.grammar_error_on_node(at, message);
                }
                None
            }
            _ => None,
        };
        if let Some(list) = list
            && self.nodes.kind(list) == SyntaxKind::VariableDeclarationList
        {
            self.check_grammar_variable_declaration_list(list);
        }
    }

    /// `Checker.checkGrammarProperty` (`grammarchecks.go:1882`), the arms not
    /// ported elsewhere:
    ///
    /// | parent | arm | code |
    /// |---|---|---|
    /// | class-like | `checkGrammarForInvalidDynamicName` | TS1166 |
    /// | class-like | `accessor` property with `?` | TS1276 |
    /// | interface | initializer | TS1246 |
    /// | type literal | initializer | TS1247 |
    /// | ambient (`NodeFlagsAmbient`) | `checkAmbientInitializer` | TS1039 / TS1254 |
    /// | any (property declaration) | `!` with an initializer / without a type / where not permitted | TS1263 / TS1264 / TS1255 |
    ///
    /// Ported elsewhere, and consulted here only for the short-circuit:
    /// `check_field_named_constructor` (TS18006) and
    /// `check_interface_computed_name` (TS1169/TS1170). Not ported: the mapped
    /// type arm (TS7061, a computed `in` expression) — such a name returns
    /// before anything here. The caller is behind `!checkGrammarModifiers`.
    fn check_grammar_property(&mut self, node: NodeId, typed: Node<'_>) {
        let (name, postfix, annotation, initializer, modifiers) = match typed {
            Node::PropertyDeclaration(n) => {
                (n.name, n.postfix_token, n.r#type, n.initializer, n.modifiers)
            }
            Node::PropertySignatureDeclaration(n) => {
                (n.name, n.postfix_token, n.r#type, n.initializer, n.modifiers)
            }
            _ => return,
        };
        if let tsr_ast::PropertyName::ComputedPropertyName(computed) = name
            && let Some(Expression::BinaryExpression(binary)) = computed.expression
            && binary.operator_token.is_some_and(|op| op.kind == SyntaxKind::InKeyword)
        {
            return;
        }
        let Some(parent) = self.nodes.parent(node) else { return };
        let invalid_dynamic_name = self.invalid_dynamic_name(name);
        match self.nodes.kind(parent) {
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression => {
                if matches!(name, tsr_ast::PropertyName::StringLiteral(literal) if literal.text == "constructor")
                {
                    return;
                }
                if let Some(at) = invalid_dynamic_name {
                    self.grammar_error_on_node(
                        at,
                        &messages::A_COMPUTED_PROPERTY_NAME_IN_A_CLASS_PROPERTY_DECLARATION_MUST_HAVE_A_SIMPLE_LITERAL_TYPE_OR_A_UNIQUE_SYMBOL_TYPE,
                    );
                    return;
                }
                // `ast.IsAutoAccessorPropertyDeclaration(node) &&
                // checkGrammarForInvalidQuestionMark(node.PostfixToken(), …)`.
                if matches!(typed, Node::PropertyDeclaration(_))
                    && has_modifier(modifiers, SyntaxKind::AccessorKeyword)
                    && let Some(question) = postfix.filter(|t| t.kind == SyntaxKind::QuestionToken)
                    && let Some(at) = question.node_id
                {
                    self.grammar_error_on_node(
                        at,
                        &messages::AN_ACCESSOR_PROPERTY_CANNOT_BE_DECLARED_OPTIONAL,
                    );
                    return;
                }
            }
            kind @ (SyntaxKind::InterfaceDeclaration | SyntaxKind::TypeLiteral) => {
                if invalid_dynamic_name.is_some() {
                    return;
                }
                if let Some(at) = initializer.and_then(|i| i.node_id()) {
                    let message = if kind == SyntaxKind::InterfaceDeclaration {
                        &messages::AN_INTERFACE_PROPERTY_CANNOT_HAVE_AN_INITIALIZER
                    } else {
                        &messages::A_TYPE_LITERAL_PROPERTY_CANNOT_HAVE_AN_INITIALIZER
                    };
                    self.grammar_error_on_node(at, message);
                    return;
                }
            }
            _ => {}
        }
        // `if node.Flags&ast.NodeFlagsAmbient != 0 { c.checkAmbientInitializer(node) }`.
        if self.has_ambient_flag(node) {
            self.check_ambient_initializer(node, initializer, annotation, true);
        }
        if !matches!(typed, Node::PropertyDeclaration(_)) {
            return;
        }
        let Some(exclamation) = postfix.filter(|t| t.kind == SyntaxKind::ExclamationToken) else {
            return;
        };
        let Some(at) = exclamation.node_id else { return };
        let message = if initializer.is_some() {
            &messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS
        } else if annotation.is_none() {
            &messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS
        } else if !matches!(
            self.nodes.kind(parent),
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
        ) || self.file_is_ambient
            || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::DeclareKeyword)
            || self.declaration_is_in_an_ambient_context(node)
            || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::StaticKeyword)
            || tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AbstractKeyword)
        {
            &messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT
        } else {
            return;
        };
        self.grammar_error_on_node(at, message);
    }

    /// The definite-assignment arm of `Checker.checkGrammarVariableDeclaration`
    /// (`grammarchecks.go:1588`): TS1263 / TS1264 / TS1255 on the `!`.
    ///
    /// The arms above it return first, and are ported elsewhere
    /// (`check_using_is_initialized`, `check_const_is_initialized`), so they are
    /// replayed here as bounds: a `using`/`await using` pattern, and — outside
    /// a `for-in`/`for-of` head and an ambient context — a missing initializer
    /// on a pattern or a `const`/`using`/`await using`. A catch clause's
    /// declaration never reaches the function.
    fn check_grammar_variable_declaration_exclamation(
        &mut self,
        node: NodeId,
        declaration: &tsr_ast::VariableDeclaration<'_>,
    ) {
        let Some(exclamation) = declaration.exclamation_token else { return };
        let Some(list) = self.nodes.parent(node) else { return };
        if self.nodes.kind(list) != SyntaxKind::VariableDeclarationList {
            return;
        }
        let block_scope = self.nodes.flags(list) & NodeFlags::BLOCK_SCOPED;
        let using = block_scope == NodeFlags::USING || block_scope == NodeFlags::CONSTANT;
        let pattern = matches!(declaration.name, Some(tsr_ast::BindingName::BindingPattern(_)));
        if pattern && using {
            return;
        }
        let owner = self.nodes.parent(list);
        let owner_kind = owner.map(|owner| self.nodes.kind(owner));
        let for_in_or_of =
            matches!(owner_kind, Some(SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement));
        let ambient = self.file_is_ambient || self.declaration_is_in_an_ambient_context(node);
        if !for_in_or_of
            && !ambient
            && declaration.initializer.is_none()
            && (pattern || using || block_scope == NodeFlags::CONST)
        {
            return;
        }
        if owner_kind == Some(SyntaxKind::VariableStatement)
            && declaration.r#type.is_some()
            && declaration.initializer.is_none()
            && !ambient
        {
            return;
        }
        let message = if declaration.initializer.is_some() {
            &messages::DECLARATIONS_WITH_INITIALIZERS_CANNOT_ALSO_HAVE_DEFINITE_ASSIGNMENT_ASSERTIONS
        } else if declaration.r#type.is_none() {
            &messages::DECLARATIONS_WITH_DEFINITE_ASSIGNMENT_ASSERTIONS_MUST_ALSO_HAVE_TYPE_ANNOTATIONS
        } else {
            &messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT
        };
        let Some(at) = exclamation.node_id else { return };
        self.grammar_error_on_node(at, message);
    }

    /// `checkGrammarForInvalidDynamicName`'s test (`grammarchecks.go:1409`):
    /// the computed name to report, when the name is dynamic
    /// (`ast.IsDynamicName`: not a string, numeric or signed numeric literal)
    /// and its expression is not an entity name. A non-bindable entity name
    /// returns false there too, so whether it is late-bindable never decides
    /// the report and no type is needed.
    pub(crate) fn invalid_dynamic_name(&self, name: tsr_ast::PropertyName<'_>) -> Option<NodeId> {
        let tsr_ast::PropertyName::ComputedPropertyName(computed) = name else { return None };
        let expression = computed.expression?;
        let literal = match expression {
            Expression::StringLiteral(_)
            | Expression::NumericLiteral(_)
            | Expression::NoSubstitutionTemplateLiteral(_) => true,
            Expression::PrefixUnaryExpression(unary) => {
                matches!(unary.operator.kind, SyntaxKind::PlusToken | SyntaxKind::MinusToken)
                    && matches!(unary.operand, Some(Expression::NumericLiteral(_)))
            }
            _ => false,
        };
        if literal || self.is_entity_name_expression(expression.node_id()?) {
            return None;
        }
        computed.node_id
    }

    /// `Checker.checkThrowStatement`'s grammar arm (`checker.go:4228`): the
    /// parser mints a missing identifier when a line break follows `throw`,
    /// and the checker reports TS1142 at that identifier's `Pos()`.
    ///
    /// Upstream's missing node sits at the full start of the next token, the
    /// end of the `throw` keyword; this parser places it at the next token's
    /// trimmed start, so the position is taken from the keyword instead.
    ///
    /// Not ported: the `checkGrammarStatementInAmbientContext` guard in front
    /// of it (the caller has no ambient context to hand).
    fn check_grammar_throw_expression(
        &mut self,
        node: NodeId,
        statement: &tsr_ast::ThrowStatement<'_>,
    ) {
        let Some(Expression::Identifier(identifier)) = statement.expression else { return };
        if !identifier.text.is_empty() {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let at = self.nodes.span(node).start + u32::try_from("throw".len()).unwrap_or(0);
        self.report(
            file,
            Diagnostic::new(&messages::LINE_BREAK_NOT_PERMITTED_HERE, tsr_core::Span::new(at, at)),
        );
    }

    /// The shape arms of `Checker.checkGrammarIndexSignatureParameters`
    /// (`grammarchecks.go:796`): everything it tests before it resolves the
    /// parameter's type, as the node and message to report, or `None`.
    ///
    /// The type-reading arms that follow (TS1337, TS1268, TS1021) are ported
    /// separately in `check.rs` (`check_index_signature_key_type`,
    /// `check_index_signature_parameter_type`); each asks this first, because
    /// upstream `return`s on the first arm that fires.
    ///
    /// Not ported: the TS1025 trailing-comma report between the count and the
    /// rest arms. It does not return, so it stands alone, and its span is the
    /// comma, which this AST does not keep for a parameter list.
    pub(crate) fn index_signature_parameter_shape_error(
        &self,
        node: NodeId,
    ) -> Option<(NodeId, &'static tsr_diagnostics::Message)> {
        let Some(Node::IndexSignatureDeclaration(signature)) = self.node_map.get(node) else {
            return None;
        };
        let Some(parameter) = signature.parameters.first() else {
            return Some((node, &messages::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER));
        };
        let name = parameter.name.and_then(|name| name.node_id());
        if signature.parameters.len() != 1 {
            return Some((name?, &messages::AN_INDEX_SIGNATURE_MUST_HAVE_EXACTLY_ONE_PARAMETER));
        }
        if let Some(dots) = parameter.dot_dot_dot_token {
            return Some((
                dots.node_id?,
                &messages::AN_INDEX_SIGNATURE_CANNOT_HAVE_A_REST_PARAMETER,
            ));
        }
        if !parameter.modifiers.is_empty() {
            return Some((
                name?,
                &messages::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_ACCESSIBILITY_MODIFIER,
            ));
        }
        if let Some(question) = parameter.question_token {
            return Some((
                question.node_id?,
                &messages::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_A_QUESTION_MARK,
            ));
        }
        if parameter.initializer.is_some() {
            return Some((
                name?,
                &messages::AN_INDEX_SIGNATURE_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
            ));
        }
        if parameter.r#type.is_none() {
            return Some((
                name?,
                &messages::AN_INDEX_SIGNATURE_PARAMETER_MUST_HAVE_A_TYPE_ANNOTATION,
            ));
        }
        None
    }

    /// `checkGrammarForInOrForOfStatement`'s `async` arm
    /// (`grammarchecks.go:1251`): `for (async of xs)` outside an await context
    /// is TS1106 on the identifier. The arm does not return true, so the list
    /// checks after it still run (they cannot apply to an identifier).
    ///
    /// `NodeFlagsAwaitContext` is not set by this parser; the enclosing
    /// function's `async` stands in for it, as in
    /// `check_await_as_binding_name`. Not ported: the `for await` arms above
    /// it, and a module's top-level await context.
    fn check_grammar_for_of_async(
        &mut self,
        node: NodeId,
        statement: &tsr_ast::ForInOrOfStatement<'_>,
    ) {
        if self.nodes.kind(node) != SyntaxKind::ForOfStatement {
            return;
        }
        let Some(tsr_ast::ForInitializer::Identifier(identifier)) = statement.initializer else {
            return;
        };
        if identifier.text != "async" {
            return;
        }
        let in_async_function = self
            .nodes
            .ancestors(node)
            .find(|&ancestor| self.is_function_like_or_static_block(ancestor))
            .and_then(|function| self.node_map.get(function))
            .and_then(crate::check::modifiers_of)
            .is_some_and(|modifiers| {
                tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::AsyncKeyword)
            });
        if in_async_function {
            return;
        }
        let Some(at) = identifier.node_id else { return };
        self.grammar_error_on_node(
            at,
            &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_OF_STATEMENT_MAY_NOT_BE_ASYNC,
        );
    }

    /// `Checker.checkGrammarVariableDeclarationList` (`grammarchecks.go:1646`)
    /// without its ambient arm (the parser does not set `NodeFlags::AMBIENT`).
    /// Its last step, `checkGrammarAwaitOrAwaitUsing` for an `await using`
    /// list, runs when no earlier arm reported (`module_format.rs`). Returns
    /// whether it reported.
    pub(crate) fn check_grammar_variable_declaration_list(&mut self, list: NodeId) -> bool {
        self.check_grammar_variable_declaration_list_arms(list)
            || self.check_await_using_declaration_list(list)
    }

    /// The arms of [`Checker::check_grammar_variable_declaration_list`]
    /// before its `await using` tail.
    fn check_grammar_variable_declaration_list_arms(&mut self, list: NodeId) -> bool {
        if self.report_disallowed_trailing_comma(list, &messages::TRAILING_COMMA_NOT_ALLOWED) {
            return true;
        }
        let Some(Node::VariableDeclarationList(declarations)) = self.node_map.get(list) else {
            return false;
        };
        if declarations.declarations.is_empty() {
            // `grammarErrorAtPos(list, declarations.Pos(), declarations.End() -
            // declarations.Pos(), …)`: an empty `NodeList` sits at the end of
            // the keyword, which is where the list node itself ends.
            let Some(file) = self.source_file_of_for_diagnostics(list) else { return false };
            let end = self.nodes.span(list).end;
            self.report(
                file,
                Diagnostic::new(
                    &messages::VARIABLE_DECLARATION_LIST_CANNOT_BE_EMPTY,
                    tsr_core::Span::new(end, end),
                ),
            );
            return true;
        }
        // `NodeFlagsAwaitUsing` is `Const | Using` upstream. This parser
        // flags an `await using` list plain `USING` and drops the `await`
        // (the printer could not write it back otherwise), so the two
        // spellings are told apart only where the tree still shows the
        // `await`; a `CONST | USING` list is accepted for when it does.
        let block_scope = self.nodes.flags(list) & NodeFlags::BLOCK_SCOPED;
        if block_scope != NodeFlags::USING && block_scope != NodeFlags::CONSTANT {
            return false;
        }
        let Some(parent) = self.nodes.parent(list) else { return false };
        if self.nodes.kind(parent) == SyntaxKind::ForInStatement {
            // `for (await using x in …)` and `for (using x in …)` parse to the
            // same tree here, and the two messages differ; decline the
            // ambiguous one rather than guess.
            if block_scope != NodeFlags::CONSTANT {
                return false;
            }
            self.grammar_error_on_node(
                list,
                &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_AN_AWAIT_USING_DECLARATION,
            );
            return true;
        }
        let Some(Node::VariableStatement(statement)) = self.node_map.get(parent) else {
            return false;
        };
        if !self.nodes.parent(parent).is_some_and(|clause| {
            matches!(self.nodes.kind(clause), SyntaxKind::CaseClause | SyntaxKind::DefaultClause)
        }) {
            return false;
        }
        // A statement with no modifiers starts where its list does, unless the
        // parser consumed an `await` in front of the list: that gap is the
        // `await`, and upstream's list (and so the report) starts there.
        let statement_start = self.nodes.span(parent).start;
        let list_span = self.nodes.span(list);
        let awaited = block_scope == NodeFlags::CONSTANT
            || (statement.modifiers.is_empty() && statement_start != list_span.start);
        let (message, span) = if awaited {
            (
                &messages::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_WITHIN_A_BLOCK,
                tsr_core::Span::new(statement_start.min(list_span.start), list_span.end),
            )
        } else {
            (
                &messages::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_WITHIN_A_BLOCK,
                list_span,
            )
        };
        let Some(file) = self.source_file_of_for_diagnostics(list) else { return true };
        self.report(file, Diagnostic::new(message, span));
        true
    }

    /// `Checker.checkGrammarForDisallowedTrailingComma` (`grammarchecks.go:671`)
    /// for a list whose owner records `NodeFlags::HAS_TRAILING_COMMA`.
    ///
    /// Upstream reports at `list.End() - len(",")`. The owner's parser ends
    /// the node at the trailing comma for every list this is used on (a
    /// variable declaration list, a heritage clause's types), so the comma is
    /// the owner's last byte.
    pub(crate) fn report_disallowed_trailing_comma(
        &mut self,
        owner: NodeId,
        message: &'static tsr_diagnostics::Message,
    ) -> bool {
        if !self.nodes.flags(owner).contains(NodeFlags::HAS_TRAILING_COMMA) {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(owner) else { return false };
        let end = self.nodes.span(owner).end;
        self.report(file, Diagnostic::new(message, tsr_core::Span::new(end - 1, end)));
        true
    }

    /// `Checker.grammarErrorOnNode` with no arguments: the node's error span.
    /// Callers have already checked the file's parse diagnostics.
    pub(crate) fn grammar_error_on_node(
        &mut self,
        node: NodeId,
        message: &'static tsr_diagnostics::Message,
    ) {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::new(message, span));
    }

    /// The postfix-token arms of `Checker.checkGrammarObjectLiteralExpression`
    /// (`grammarchecks.go:1098`) and of `Checker.checkGrammarMethod` for a
    /// method in an object literal (`:1438`): `?` is TS1162 and `!` is TS1255
    /// on a property, shorthand or method, and a method's modifiers other than
    /// a lone `async` are TS1184 on its first token.
    ///
    /// The property arms do not return, so each member stands alone. The
    /// method arm sits behind `checkGrammarMethod`'s modifier test (anything
    /// but a lone `async` is TS1184 and returns first), which is mirrored as a
    /// bound rather than reported here.
    pub(crate) fn check_grammar_object_literal_postfix_tokens(&mut self, typed: Node<'_>) {
        let Node::ObjectLiteralExpression(literal) = typed else { return };
        for property in literal.properties {
            // `checkGrammarComputedPropertyName` on each member's name; its
            // result is ignored, so the member's other arms still run.
            let name = match property {
                ObjectLiteralElementLike::PropertyAssignment(n) => Some(n.name),
                ObjectLiteralElementLike::MethodDeclaration(n) => Some(n.name),
                ObjectLiteralElementLike::GetAccessorDeclaration(n) => Some(n.name),
                ObjectLiteralElementLike::SetAccessorDeclaration(n) => Some(n.name),
                _ => None,
            };
            if let Some(name) = name {
                self.check_grammar_computed_property_name(name);
            }
            let postfix = match property {
                ObjectLiteralElementLike::PropertyAssignment(assignment) => {
                    assignment.postfix_token
                }
                ObjectLiteralElementLike::ShorthandPropertyAssignment(shorthand) => {
                    shorthand.postfix_token
                }
                ObjectLiteralElementLike::MethodDeclaration(method) => {
                    let lone_async = matches!(
                        method.modifiers,
                        [ModifierLike::Token(token)] if token.kind == SyntaxKind::AsyncKeyword
                    );
                    if !method.modifiers.is_empty() && !lone_async {
                        // TS1184 on the method's first token, and return.
                        if let Some(id) = method.node_id
                            && let Some(file) = self.source_file_of_for_diagnostics(id)
                        {
                            let start = self.nodes.span(id).start;
                            self.report(
                                file,
                                Diagnostic::new(
                                    &messages::MODIFIERS_CANNOT_APPEAR_HERE,
                                    tsr_core::Span::new(start, start + 1),
                                ),
                            );
                        }
                        continue;
                    }
                    method.postfix_token
                }
                _ => continue,
            };
            let Some(token) = postfix else { continue };
            let Some(id) = token.node_id else { continue };
            let message = match token.kind {
                SyntaxKind::QuestionToken => {
                    &messages::AN_OBJECT_MEMBER_CANNOT_BE_DECLARED_OPTIONAL
                }
                SyntaxKind::ExclamationToken => {
                    &messages::A_DEFINITE_ASSIGNMENT_ASSERTION_IS_NOT_PERMITTED_IN_THIS_CONTEXT
                }
                _ => continue,
            };
            self.grammar_error_on_node(id, message);
        }
    }

    /// `Checker.checkGrammarComputedPropertyName` (`grammarchecks.go:979`):
    /// a comma expression as a computed name is TS1171, on the expression.
    /// The parser keeps the whole expression (`parseComputedPropertyName`
    /// parses a comma expression for exactly this report).
    fn check_grammar_computed_property_name(&mut self, name: tsr_ast::PropertyName<'_>) -> bool {
        if let tsr_ast::PropertyName::ComputedPropertyName(computed) = name
            && let Some(Expression::BinaryExpression(binary)) = computed.expression
            && binary.operator_token.is_some_and(|op| op.kind == SyntaxKind::CommaToken)
            && let Some(id) = binary.node_id
        {
            self.grammar_error_on_node(
                id,
                &messages::A_COMMA_EXPRESSION_IS_NOT_ALLOWED_IN_A_COMPUTED_PROPERTY_NAME,
            );
            return true;
        }
        false
    }

    /// `Checker.checkGrammarTypeOperatorNode` (`grammarchecks.go:1369`),
    /// reached from `checkTypeOperator` for every type operator.
    ///
    /// `unique` must apply to `symbol` ("'symbol' expected" on the operand)
    /// and may only type a `const` variable of a variable statement with an
    /// identifier name, a `static readonly` class property or a `readonly`
    /// property signature, the owner found through any parenthesized types;
    /// `readonly` may only modify an array or tuple type (TS1354, on the
    /// operator's first token).
    fn check_grammar_type_operator_node(
        &mut self,
        node: NodeId,
        operator: &tsr_ast::TypeOperatorNode<'_>,
    ) {
        let Some(inner) = operator.r#type.and_then(|t| t.node_id()) else { return };
        match operator.operator.kind {
            SyntaxKind::UniqueKeyword => {
                if self.nodes.kind(inner) != SyntaxKind::SymbolKeyword {
                    let Some(file) = self.source_file_of_for_diagnostics(inner) else { return };
                    let span = self.error_span(inner);
                    self.report(
                        file,
                        Diagnostic::with_args(&messages::_0_EXPECTED, span, ["symbol".to_string()]),
                    );
                    return;
                }
                // `ast.WalkUpParenthesizedTypes(node.Parent)`.
                let mut parent = self.nodes.parent(node);
                while let Some(at) = parent
                    && self.nodes.kind(at) == SyntaxKind::ParenthesizedType
                {
                    parent = self.nodes.parent(at);
                }
                let Some(parent) = parent else { return };
                match self.node_map.get(parent) {
                    Some(Node::VariableDeclaration(declaration)) => {
                        if !matches!(declaration.name, Some(tsr_ast::BindingName::Identifier(_))) {
                            self.grammar_error_on_node(
                                node,
                                &messages::UNIQUE_SYMBOL_TYPES_MAY_NOT_BE_USED_ON_A_VARIABLE_DECLARATION_WITH_A_BINDING_NAME,
                            );
                            return;
                        }
                        let list = self.nodes.parent(parent);
                        let in_statement = list.is_some_and(|list| {
                            self.nodes.kind(list) == SyntaxKind::VariableDeclarationList
                                && self.nodes.parent(list).is_some_and(|statement| {
                                    self.nodes.kind(statement) == SyntaxKind::VariableStatement
                                })
                        });
                        if !in_statement {
                            self.grammar_error_on_node(
                                node,
                                &messages::UNIQUE_SYMBOL_TYPES_ARE_ONLY_ALLOWED_ON_VARIABLES_IN_A_VARIABLE_STATEMENT,
                            );
                            return;
                        }
                        if list
                            .is_some_and(|list| !self.nodes.flags(list).contains(NodeFlags::CONST))
                            && let Some(name) = declaration.name.and_then(|n| n.node_id())
                        {
                            self.grammar_error_on_node(
                                name,
                                &messages::A_VARIABLE_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_CONST,
                            );
                        }
                    }
                    Some(Node::PropertyDeclaration(property)) => {
                        if (!has_modifier(property.modifiers, SyntaxKind::StaticKeyword)
                            || !has_modifier(property.modifiers, SyntaxKind::ReadonlyKeyword))
                            && let Some(name) = property.name.node_id()
                        {
                            self.grammar_error_on_node(
                                name,
                                &messages::A_PROPERTY_OF_A_CLASS_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_BOTH_STATIC_AND_READONLY,
                            );
                        }
                    }
                    Some(Node::PropertySignatureDeclaration(property)) => {
                        if !has_modifier(property.modifiers, SyntaxKind::ReadonlyKeyword)
                            && let Some(name) = property.name.node_id()
                        {
                            self.grammar_error_on_node(
                                name,
                                &messages::A_PROPERTY_OF_AN_INTERFACE_OR_TYPE_LITERAL_WHOSE_TYPE_IS_A_UNIQUE_SYMBOL_TYPE_MUST_BE_READONLY,
                            );
                        }
                    }
                    _ => self.grammar_error_on_node(
                        node,
                        &messages::UNIQUE_SYMBOL_TYPES_ARE_NOT_ALLOWED_HERE,
                    ),
                }
            }
            SyntaxKind::ReadonlyKeyword => {
                if !matches!(self.nodes.kind(inner), SyntaxKind::ArrayType | SyntaxKind::TupleType)
                    && let Some(file) = self.source_file_of_for_diagnostics(node)
                {
                    // `grammarErrorOnFirstToken`: the `readonly` keyword,
                    // which opens the node.
                    let start = self.nodes.span(node).start;
                    let span = tsr_core::Span::new(start, start + 8);
                    self.report(
                        file,
                        Diagnostic::with_args(
                            &messages::READONLY_TYPE_MODIFIER_IS_ONLY_PERMITTED_ON_ARRAY_AND_TUPLE_LITERAL_TYPES,
                            span,
                            ["symbol".to_string()],
                        ),
                    );
                }
            }
            _ => {}
        }
    }

    /// `grammarErrorOnNode(mod, X_0_modifier_cannot_be_used_here, …)`.
    fn report_modifier_cannot_be_used_here(&mut self, modifier: NodeId, kind: SyntaxKind) {
        let Some(file) = self.source_file_of_for_diagnostics(modifier) else { return };
        let span = self.error_span(modifier);
        self.report(
            file,
            Diagnostic::with_args(
                &messages::_0_MODIFIER_CANNOT_BE_USED_HERE,
                span,
                [modifier_text(kind).to_string()],
            ),
        );
    }
}

/// The source spelling of a modifier keyword (`scanner.GetTextOfNode` on a
/// modifier token, which is always exactly its keyword).
fn modifier_text(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::AbstractKeyword => "abstract",
        SyntaxKind::AccessorKeyword => "accessor",
        SyntaxKind::AsyncKeyword => "async",
        SyntaxKind::ConstKeyword => "const",
        SyntaxKind::DeclareKeyword => "declare",
        SyntaxKind::DefaultKeyword => "default",
        SyntaxKind::ExportKeyword => "export",
        SyntaxKind::InKeyword => "in",
        SyntaxKind::OutKeyword => "out",
        SyntaxKind::OverrideKeyword => "override",
        SyntaxKind::PrivateKeyword => "private",
        SyntaxKind::ProtectedKeyword => "protected",
        SyntaxKind::PublicKeyword => "public",
        SyntaxKind::ReadonlyKeyword => "readonly",
        SyntaxKind::StaticKeyword => "static",
        _ => "",
    }
}

/// `scanner.DeclarationNameToString` for a property name, as far as a message
/// argument needs it: the name's written text (a string literal keeps its
/// quotes), or empty for a computed name.
fn declaration_name_text(name: tsr_ast::PropertyName<'_>) -> String {
    match name {
        tsr_ast::PropertyName::Identifier(identifier) => identifier.text.to_string(),
        tsr_ast::PropertyName::PrivateIdentifier(identifier) => identifier.text.to_string(),
        tsr_ast::PropertyName::StringLiteral(literal) => format!("\"{}\"", literal.text),
        tsr_ast::PropertyName::NumericLiteral(literal) => literal.text.to_string(),
        _ => String::new(),
    }
}

impl Checker<'_, '_> {
    /// `Checker.checkJSDocTypeIsInJsFile` (`checker.go:2584`), its nullable
    /// and non-nullable arm: outside a JS file, `T?` / `?T` / `T!` / `!T` is
    /// TS17019 (postfix) or TS17020 (prefix) through `grammarErrorOnNode`,
    /// suggesting the type written out — the operand's type, with
    /// `undefined` (postfix `?`) or `undefined | null` (prefix `?`) added
    /// unless it is `never` or `void` (`getNullableType`).
    ///
    /// The other arm (TS8020 for every other JSDoc type) is not ported: those
    /// kinds reach this checker through paths whose shape is not yet
    /// upstream's, and no lane case waits on it.
    fn check_jsdoc_type_is_in_js_file(&mut self, node: NodeId) {
        if self.in_js_file(node) {
            return;
        }
        let (inner, nullable) = match self.node_map.get(node) {
            Some(Node::JSDocNullableType(n)) => (n.r#type, true),
            Some(Node::JSDocNonNullableType(n)) => (n.r#type, false),
            _ => return,
        };
        let Some(inner) = inner else { return };
        let Some(inner_id) = inner.node_id() else { return };
        let postfix = self.nodes.span(node).start == self.nodes.span(inner_id).start;
        let message = if postfix {
            &messages::_0_AT_THE_END_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE_1
        } else {
            &messages::_0_AT_THE_START_OF_A_TYPE_IS_NOT_VALID_TYPESCRIPT_SYNTAX_DID_YOU_MEAN_TO_WRITE_1
        };
        let mut ty = self.get_type_from_type_node(inner);
        if nullable && ty != self.intrinsics.never && ty != self.intrinsics.void {
            // `getNullableType(t, postfix ? Undefined : Nullable)`.
            ty = if postfix {
                self.get_union_type(&[ty, self.intrinsics.undefined])
            } else {
                self.get_union_type(&[ty, self.intrinsics.undefined, self.intrinsics.null])
            };
        }
        let printed = self.type_to_string(ty);
        let token = if nullable { "?" } else { "!" };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let span = self.error_span(node);
        self.report(file, Diagnostic::with_args(message, span, [token.to_string(), printed]));
    }

    /// `Checker.grammarErrorOnFirstToken` (`grammarchecks.go:19`): silent in a
    /// file with parse diagnostics; otherwise the range of the token at the
    /// node's start (`scanner.GetRangeOfTokenAtPosition`). Scans one token on
    /// this error path only; a host without source text cannot supply the
    /// range and reports nothing. Returns whether it reported, as upstream's
    /// callers record that result (`hasReportedStatementInAmbientContext`).
    pub(crate) fn grammar_error_on_first_token(
        &mut self,
        node: NodeId,
        message: &'static tsr_diagnostics::Message,
    ) -> bool {
        if self.file_has_parse_errors {
            return false;
        }
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return false };
        let start = self.nodes.span(node).start;
        let Some(rest) = self
            .module_host
            .and_then(|host| host.source_text(file, self.nodes))
            .and_then(|text| text.get(start as usize..))
        else {
            return false;
        };
        let token = tsr_scanner::Scanner::new(rest).scan().span;
        let span = tsr_core::Span::new(start + token.start, start + token.end);
        self.report(file, Diagnostic::new(message, span));
        true
    }

    /// The grammar arms of `Checker.checkCatchClause` (`checker.go:4247`): a
    /// type annotation whose type is not `any`/`unknown` is TS1196, else an
    /// initializer is TS1197, both on the offending node's first token.
    ///
    /// Not ported: the third arm, TS2492 for a block-scoped redeclaration of
    /// the caught name, which reads the binder's catch-clause and block
    /// locals.
    fn check_catch_clause_declaration(&mut self, declaration: NodeId) {
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
            return;
        };
        // `declaration.Type()`: in JS the reparsed `@type`.
        if let Some(type_node) =
            variable.r#type.or_else(|| self.jsdoc_self_hosted_type(declaration))
        {
            let ty = self.get_type_from_type_node(type_node);
            // `is_error` is upstream's `errorType`, which carries `TypeFlagsAny`.
            if !self.is_type_any(ty)
                && !self.type_of(ty).flags.intersects(crate::flags::TypeFlags::ANY_OR_UNKNOWN)
                && let Some(id) = type_node.node_id()
            {
                self.grammar_error_on_first_token(
                    id,
                    &messages::CATCH_CLAUSE_VARIABLE_TYPE_ANNOTATION_MUST_BE_ANY_OR_UNKNOWN_IF_SPECIFIED,
                );
            }
        } else if let Some(id) = variable.initializer.and_then(|i| i.node_id()) {
            self.grammar_error_on_first_token(
                id,
                &messages::CATCH_CLAUSE_VARIABLE_CANNOT_HAVE_AN_INITIALIZER,
            );
        } else {
            self.check_catch_clause_block_redeclarations(declaration);
        }
    }

    /// TS2492 — `checkCatchClause`'s last arm (`checker.go:4259`): an
    /// unannotated catch variable without an initializer, each of whose
    /// names (`node.Locals()`) the catch block redeclares as a block-scoped
    /// variable, is reported at that variable's value declaration. A `var`
    /// redeclaration is not block-scoped and is allowed.
    fn check_catch_clause_block_redeclarations(&mut self, declaration: NodeId) {
        let Some(clause) = self.nodes.parent(declaration) else { return };
        let Some(Node::CatchClause(catch)) = self.node_map.get(clause) else { return };
        let Some(block) = catch.block.and_then(|block| block.node_id) else { return };
        let (Some(caught), Some(block_locals)) =
            (self.binder.locals(clause), self.binder.locals(block))
        else {
            return;
        };
        let mut redeclared: Vec<(NodeId, String)> = Vec::new();
        for (&name, _) in caught {
            let Some(&local) = block_locals.get(name) else { continue };
            let entry = self.binder.symbols().get(local);
            if entry.flags.intersects(tsr_binder::SymbolFlags::BLOCK_SCOPED_VARIABLE)
                && let Some(value_declaration) = entry.value_declaration
            {
                redeclared.push((value_declaration, name.to_string()));
            }
        }
        for (value_declaration, name) in redeclared {
            let Some(file) = self.source_file_of_for_diagnostics(value_declaration) else {
                return;
            };
            let span = self.error_span(value_declaration);
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::CANNOT_REDECLARE_IDENTIFIER_0_IN_CATCH_CLAUSE,
                    span,
                    [name],
                ),
            );
        }
    }
}

impl Checker<'_, '_> {
    /// TS1355 — the `isConstTypeReference` arm of `checkAssertion`
    /// (`checker.go:12303`): `x as const` and `<const>x` require an operand
    /// `isValidConstAssertionArgument` accepts, and report on the operand
    /// otherwise.
    ///
    /// Not a grammar check upstream — `c.error`, so no parse-diagnostics gate —
    /// but a purely local predicate of the assertion node and one entity-name
    /// resolution, which is why it is dispatched from `check_node`'s assertion
    /// arm beside `check_assertion_overlap` (`assertion_overlap.rs`, the
    /// deferred half of the same `checkAssertion`) rather than from the
    /// expression checker's `check_const_assertion`, which computes the type
    /// and is not this lane's.
    ///
    /// `docs/parity/notes/r4-unused-grammar.md` §3.
    pub(crate) fn check_const_assertion_argument(&mut self, node: NodeId) {
        let (expression, annotation) = match self.node_map.get(node) {
            Some(Node::AsExpression(assertion)) => (assertion.expression, assertion.r#type),
            Some(Node::TypeAssertion(assertion)) => (assertion.expression, assertion.r#type),
            _ => return,
        };
        let (Some(expression), Some(annotation)) = (expression, annotation) else { return };
        // `isConstTypeReference` (`ast/utilities.go`): a type reference with
        // no type arguments whose name is the identifier `const`.
        if !crate::assertions::is_const_type_reference(annotation) {
            return;
        }
        let Some(operand) = expression.node_id() else { return };
        if self.is_valid_const_assertion_argument(operand) {
            return;
        }
        let Some(file) = self.source_file_of_for_diagnostics(operand) else { return };
        let span = self.error_span(operand);
        self.report(
            file,
            Diagnostic::new(
                &messages::A_CONST_ASSERTION_CAN_ONLY_BE_APPLIED_TO_REFERENCES_TO_ENUM_MEMBERS_OR_STRING_NUMBER_BOOLEAN_ARRAY_OR_OBJECT_LITERALS,
                span,
            ),
        );
    }

    /// `isValidConstAssertionArgument` (`checker.go:13623`), arm for arm.
    fn is_valid_const_assertion_argument(&mut self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(
                Node::StringLiteral(_)
                | Node::NoSubstitutionTemplateLiteral(_)
                | Node::NumericLiteral(_)
                | Node::BigIntLiteral(_)
                | Node::ArrayLiteralExpression(_)
                | Node::ObjectLiteralExpression(_)
                | Node::TemplateExpression(_),
            ) => true,
            _ if matches!(
                self.nodes.kind(node),
                SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword
            ) =>
            {
                true
            }
            Some(Node::ParenthesizedExpression(parenthesized)) => parenthesized
                .expression
                .and_then(|e| e.node_id())
                .is_some_and(|inner| self.is_valid_const_assertion_argument(inner)),
            Some(Node::PrefixUnaryExpression(unary)) => {
                let operand = unary.operand.and_then(|e| e.node_id()).map(|o| self.nodes.kind(o));
                match unary.operator.kind {
                    SyntaxKind::MinusToken => matches!(
                        operand,
                        Some(SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral)
                    ),
                    SyntaxKind::PlusToken => operand == Some(SyntaxKind::NumericLiteral),
                    _ => false,
                }
            }
            Some(Node::PropertyAccessExpression(access)) => {
                let receiver = access.expression.and_then(|e| e.node_id());
                receiver.is_some_and(|receiver| self.is_enum_entity_name_expression(receiver))
            }
            Some(Node::ElementAccessExpression(access)) => {
                let receiver = access.expression.and_then(|e| e.node_id());
                receiver.is_some_and(|receiver| self.is_enum_entity_name_expression(receiver))
            }
            _ => false,
        }
    }

    /// The access arm's tail: `SkipParentheses(expr)`, then, for an entity
    /// name expression, `resolveEntityName(expr, Value, ignoreErrors=true)`
    /// and `symbol.Flags & SymbolFlagsEnum`.
    fn is_enum_entity_name_expression(&mut self, expression: NodeId) -> bool {
        let mut expression = expression;
        while let Some(Node::ParenthesizedExpression(parenthesized)) = self.node_map.get(expression)
        {
            let Some(inner) = parenthesized.expression.and_then(|e| e.node_id()) else {
                return false;
            };
            expression = inner;
        }
        self.resolve_value_entity_name_expression(expression).is_some_and(|symbol| {
            self.binder.symbols().get(symbol).flags.intersects(tsr_binder::SymbolFlags::ENUM)
        })
    }

    /// `resolveEntityName(name, meaning, ignoreErrors=true,
    /// dontResolveAlias=false)` (`checker.go:15772`) for an entity name
    /// *expression* — an identifier, or a property access chain of them
    /// (`IsEntityNameExpression`). The left of an access resolves at
    /// `Namespace` meaning (`resolveQualifiedName`, `checker.go:15828`) with
    /// its alias followed, the right in that namespace's exports; the result
    /// is followed through its alias chain while it lacks `Value`
    /// (`checker.go:15821`).
    fn resolve_value_entity_name_expression(
        &mut self,
        node: NodeId,
    ) -> Option<tsr_binder::SymbolId> {
        use tsr_binder::SymbolFlags;
        let mut symbol = self.resolve_entity_name_expression_at(node, SymbolFlags::VALUE)?;
        let mut seen = 0;
        while !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::VALUE)
            && self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ALIAS)
        {
            seen += 1;
            if seen > 64 {
                return None;
            }
            symbol = self.binder.merged_symbol(self.resolve_alias(symbol)?);
        }
        Some(symbol)
    }

    fn resolve_entity_name_expression_at(
        &mut self,
        node: NodeId,
        meaning: tsr_binder::SymbolFlags,
    ) -> Option<tsr_binder::SymbolId> {
        use tsr_binder::SymbolFlags;
        match self.node_map.get(node)? {
            Node::Identifier(identifier) => {
                let found = self.resolve_name_with_export_alias(node, identifier.text, meaning)?;
                Some(self.binder.merged_symbol(found))
            }
            Node::PropertyAccessExpression(access) => {
                let Some(tsr_ast::MemberName::Identifier(name)) = access.name else { return None };
                let left = access.expression?.node_id()?;
                let mut namespace =
                    self.resolve_entity_name_expression_at(left, SymbolFlags::NAMESPACE)?;
                if self.binder.symbols().get(namespace).flags.intersects(SymbolFlags::ALIAS) {
                    namespace = self.resolve_alias(namespace)?;
                }
                let namespace = self.binder.merged_symbol(namespace);
                let found = *self.binder.symbols().get(namespace).exports.get(name.text)?;
                let found = self.binder.merged_symbol(found);
                (self.binder.symbols().get(found).flags.intersects(meaning)
                    || self.get_symbol_flags(found).intersects(meaning))
                .then_some(found)
            }
            _ => None,
        }
    }
}

impl Checker<'_, '_> {
    /// TS1211 — `checkClassDeclaration` (`checker.go:4285`): a class
    /// declaration with no name and no `default` modifier,
    /// `grammarErrorOnFirstToken` — the declaration's first token, which is its
    /// first modifier or decorator when it has one (`export class {}` reports
    /// on `export`), behind `!hasParseDiagnostics`.
    ///
    /// `docs/parity/notes/r4-unused-grammar.md` §4.
    fn check_class_declaration_has_name(&mut self, typed: Node<'_>) {
        let Node::ClassDeclaration(class) = typed else { return };
        if class.name.is_some()
            || tsr_ast::has_syntactic_modifier(class.modifiers, SyntaxKind::DefaultKeyword)
            || self.file_has_parse_errors
        {
            return;
        }
        let Some(node) = class.node_id else { return };
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let start = self.nodes.span(node).start;
        // `scanner.GetRangeOfTokenAtPosition(file, node.Pos())`. Without
        // source text (unit hosts) the node's own start stands for the token,
        // one character wide; line and column are the same.
        let span = self
            .module_host
            .and_then(|host| host.source_text(file, self.nodes))
            .and_then(|text| text.get(start as usize..))
            .map(|rest| tsr_scanner::Scanner::new(rest).scan().span)
            .map_or(tsr_core::Span::new(start, start + 1), |token| {
                tsr_core::Span::new(start + token.start, start + token.end)
            });
        self.report(
            file,
            Diagnostic::new(
                &messages::A_CLASS_DECLARATION_WITHOUT_THE_DEFAULT_MODIFIER_MUST_HAVE_A_NAME,
                span,
            ),
        );
    }
}

impl Checker<'_, '_> {
    /// `!isInvalidInitializer` of `checkGrammarVariableLikeDeclaration`'s
    /// ambient tail (`grammarchecks.go:1963`): the initializers a `const` or
    /// `readonly` declaration without an annotation may carry in an ambient
    /// context —
    /// `isInitializerStringOrNumberLiteralExpression` (`:1978`),
    /// `isInitializerSimpleLiteralEnumReference` (`:1996`), `true`/`false`,
    /// and `isInitializerBigIntLiteralExpression` (`:1983`), in that order.
    ///
    /// `docs/parity/notes/r4-unused-grammar.md` §6.
    pub(crate) fn is_valid_ambient_const_initializer(
        &mut self,
        initializer: Expression<'_>,
    ) -> bool {
        let Some(node) = initializer.node_id() else { return false };
        is_string_or_number_literal_initializer(self, node)
            || self.is_initializer_simple_literal_enum_reference(initializer)
            || matches!(self.nodes.kind(node), SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
            || is_bigint_literal_initializer(self, node)
    }

    /// `isInitializerSimpleLiteralEnumReference` (`grammarchecks.go:1996`):
    /// a property access, or an element access with a string/number literal
    /// argument on an entity name expression, whose checked type
    /// (`checkExpressionCached`) is enum-like.
    fn is_initializer_simple_literal_enum_reference(&mut self, expression: Expression<'_>) -> bool {
        match expression {
            Expression::PropertyAccessExpression(_) => {}
            Expression::ElementAccessExpression(access) => {
                let argument = access.argument_expression.and_then(|a| a.node_id());
                let receiver = access.expression.and_then(|e| e.node_id());
                if !argument.is_some_and(|a| is_string_or_number_literal_initializer(self, a))
                    || !receiver.is_some_and(|r| self.is_entity_name_expression(r))
                {
                    return false;
                }
            }
            _ => return false,
        }
        let checked = self.check_expression(expression);
        self.store.get(checked).flags.intersects(crate::flags::TypeFlags::ENUM_LIKE)
    }
}

/// `isInitializerStringOrNumberLiteralExpression` (`grammarchecks.go:1978`):
/// `IsStringOrNumericLiteralLike` — a string, no-substitution template or
/// numeric literal — or `-` over a numeric literal.
fn is_string_or_number_literal_initializer(checker: &Checker<'_, '_>, node: NodeId) -> bool {
    match checker.nodes.kind(node) {
        SyntaxKind::StringLiteral
        | SyntaxKind::NoSubstitutionTemplateLiteral
        | SyntaxKind::NumericLiteral => true,
        SyntaxKind::PrefixUnaryExpression => matches!(
            checker.node_map.get(node),
            Some(Node::PrefixUnaryExpression(unary))
                if unary.operator.kind == SyntaxKind::MinusToken
                    && matches!(unary.operand, Some(Expression::NumericLiteral(_)))
        ),
        _ => false,
    }
}

/// `isInitializerBigIntLiteralExpression` (`grammarchecks.go:1983`).
fn is_bigint_literal_initializer(checker: &Checker<'_, '_>, node: NodeId) -> bool {
    match checker.node_map.get(node) {
        Some(Node::BigIntLiteral(_)) => true,
        Some(Node::PrefixUnaryExpression(unary)) => {
            unary.operator.kind == SyntaxKind::MinusToken
                && matches!(unary.operand, Some(Expression::BigIntLiteral(_)))
        }
        _ => false,
    }
}

/// One written entry of `node.ModifierNodes()` as `checkGrammarModifiers`
/// walks it: a decorator or a modifier keyword.
#[derive(Clone, Copy)]
enum ModifierEntry {
    Decorator(NodeId),
    Modifier { kind: SyntaxKind, at: NodeId },
}

/// `ast.ModifierFlagsModifier`: every modifier flag but `Decorator`.
const MODIFIER_FLAGS_MODIFIER: ModifierFlags = ModifierFlags::all()
    .difference(ModifierFlags::DECORATOR)
    .difference(ModifierFlags::IMMEDIATE)
    .difference(ModifierFlags::DEFERRED);

/// `ast.ModifierFlagsParameterPropertyModifier`.
const PARAMETER_PROPERTY_MODIFIER: ModifierFlags = ModifierFlags::ACCESSIBILITY_MODIFIER
    .union(ModifierFlags::READONLY)
    .union(ModifierFlags::OVERRIDE);

/// `ast.ModifierToFlag` for the keywords `checkGrammarModifiers` sets.
fn modifier_to_flag(kind: SyntaxKind) -> ModifierFlags {
    match kind {
        SyntaxKind::PublicKeyword => ModifierFlags::PUBLIC,
        SyntaxKind::PrivateKeyword => ModifierFlags::PRIVATE,
        SyntaxKind::ProtectedKeyword => ModifierFlags::PROTECTED,
        _ => ModifierFlags::empty(),
    }
}

/// `node.ModifierNodes()` for the kinds whose checker calls
/// `checkGrammarModifiers`: `checkTypeParameter`, `checkParameter`,
/// `checkPropertyDeclaration` (and property signatures),
/// `checkClassStaticBlockDeclaration`, `checkInterfaceDeclaration`,
/// `checkEnumDeclaration`, `checkModuleDeclaration`, `checkImportDeclaration`,
/// `checkImportEqualsDeclaration`, `checkExportDeclaration`,
/// `checkExportAssignment`, `checkVariableStatement`,
/// `checkTypeAliasDeclaration` (`checker.go:2605`-`:6878`),
/// `checkGrammarFunctionLikeDeclaration` (function-likes, methods and
/// accessors, `grammarchecks.go:762`), `checkGrammarIndexSignature` (`:842`)
/// and `checkGrammarClassDeclarationHeritageClauses` (`:898`, both class
/// kinds). `None` for every other kind: an object literal member's modifiers
/// are `checkGrammarObjectLiteralExpression`'s
/// ([`Checker::check_grammar_object_literal_modifiers`]).
fn grammar_modifier_nodes(typed: Node<'_>) -> Option<&[ModifierLike<'_>]> {
    Some(match typed {
        Node::ClassStaticBlockDeclaration(n) => n.modifiers,
        Node::ClassDeclaration(_)
        | Node::ClassExpression(_)
        | Node::InterfaceDeclaration(_)
        | Node::TypeAliasDeclaration(_)
        | Node::EnumDeclaration(_)
        | Node::ModuleDeclaration(_)
        | Node::FunctionDeclaration(_)
        | Node::VariableStatement(_)
        | Node::ImportDeclaration(_)
        | Node::ImportEqualsDeclaration(_)
        | Node::ExportDeclaration(_)
        | Node::ExportAssignment(_)
        | Node::PropertyDeclaration(_)
        | Node::MethodDeclaration(_)
        | Node::ParameterDeclaration(_)
        | Node::TypeParameterDeclaration(_)
        | Node::FunctionExpression(_)
        | Node::ArrowFunction(_)
        | Node::ConstructorDeclaration(_)
        | Node::GetAccessorDeclaration(_)
        | Node::SetAccessorDeclaration(_)
        | Node::PropertySignatureDeclaration(_)
        | Node::MethodSignatureDeclaration(_)
        | Node::IndexSignatureDeclaration(_)
        | Node::FunctionTypeNode(_)
        | Node::ConstructorTypeNode(_) => crate::check::modifiers_of(typed)?,
        _ => return None,
    })
}

impl Checker<'_, '_> {
    /// `Checker.checkGrammarModifiers` (`grammarchecks.go:214`): the
    /// decorator and modifier grammar of one declaration. Every arm is
    /// `return grammarErrorOn…`, so a node gets at most one report, and the
    /// answer gates the grammar checks its callers run after it
    /// (`!c.checkGrammarModifiers(node)`), which read
    /// `modifier_chain_reported`.
    ///
    /// Runs once per node (`modifier_chain_checked`): upstream calls it from
    /// each checker that gates on it and its program-level
    /// `SortAndDeduplicateDiagnostics` folds the repeats, where this port's
    /// collection keeps every report.
    ///
    /// `grammarErrorOnNode` is silent in a file with parse diagnostics and
    /// answers `false`, which makes the whole function `false` there; the
    /// one `c.error` arm (TS8038) has the same guard.
    pub(crate) fn check_grammar_modifiers(&mut self, node: NodeId) -> bool {
        if !self.modifier_chain_checked.insert(node) {
            return self.modifier_chain_reported.contains(&node);
        }
        let Some(typed) = self.node_map.get(node) else { return false };
        let Some(written) = grammar_modifier_nodes(typed) else { return false };
        let entries: Vec<ModifierEntry> = written
            .iter()
            .filter_map(|modifier| match modifier {
                ModifierLike::Decorator(decorator) => {
                    decorator.node_id.map(ModifierEntry::Decorator)
                }
                ModifierLike::Token(token) => {
                    token.node_id.map(|at| ModifierEntry::Modifier { kind: token.kind, at })
                }
            })
            .collect();
        // The JS reparser appends the JSDoc modifiers (`@public`, `@private`,
        // `@protected`, `@readonly`, `@override`) to the written list of
        // these hosts, so the list is not empty when only they are present.
        let reparsed_host = self.file_is_js
            && matches!(
                typed,
                Node::MethodDeclaration(_)
                    | Node::GetAccessorDeclaration(_)
                    | Node::SetAccessorDeclaration(_)
                    | Node::PropertyDeclaration(_)
                    | Node::ConstructorDeclaration(_)
            );
        // `if node.Modifiers() == nil { return false }`.
        if (entries.is_empty() && !reparsed_host) || self.file_has_parse_errors {
            return false;
        }
        let reported = self.check_grammar_modifiers_worker(node, typed, &entries, reparsed_host);
        if reported {
            self.modifier_chain_reported.insert(node);
        }
        reported
    }

    /// The body of `checkGrammarModifiers` past its `nil` test, in upstream's
    /// order. Answers whether it reported.
    #[expect(clippy::too_many_lines, reason = "one native function, ported arm for arm")]
    fn check_grammar_modifiers_worker(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        entries: &[ModifierEntry],
        reparsed_host: bool,
    ) -> bool {
        if self.report_obvious_decorator_errors(node, typed, entries)
            || self.report_obvious_modifier_errors(node, typed, entries)
        {
            return true;
        }
        let kind = self.nodes.kind(node);
        // `ast.IsThisParameter(node)`.
        if let Node::ParameterDeclaration(parameter) = typed
            && matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        {
            return self.grammar_error_on_first_token(
                node,
                &messages::NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS,
            );
        }
        let block_scope_kind = match typed {
            Node::VariableStatement(statement) => statement
                .declaration_list
                .and_then(|list| list.node_id)
                .map_or(NodeFlags::empty(), |list| {
                    self.nodes.flags(list) & NodeFlags::BLOCK_SCOPED
                }),
            _ => NodeFlags::empty(),
        };
        let is_await_using = block_scope_kind == NodeFlags::CONST | NodeFlags::USING
            || self.is_await_using_statement(typed);
        let is_using = block_scope_kind == NodeFlags::USING && !is_await_using;
        let parent = self.nodes.parent(node);
        let parent_kind = parent.map(|parent| self.nodes.kind(parent));
        let parent_is_module_or_file =
            matches!(parent_kind, Some(SyntaxKind::ModuleBlock | SyntaxKind::SourceFile));
        let parent_is_class_like =
            matches!(parent_kind, Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression));
        let mut last_static = None;
        let mut last_declare = None;
        let mut last_async = None;
        let mut last_override = None;
        let mut first_decorator: Option<NodeId> = None;
        let mut flags = ModifierFlags::empty();
        let mut saw_export_before_decorators = false;
        let mut has_leading_decorators = false;
        for &entry in entries {
            let (modifier_kind, at) = match entry {
                ModifierEntry::Decorator(decorator) => {
                    if !self.node_can_be_decorated(node, typed) {
                        let message = if matches!(typed, Node::MethodDeclaration(method) if method.body.is_none())
                        {
                            &messages::A_DECORATOR_CAN_ONLY_DECORATE_A_METHOD_IMPLEMENTATION_NOT_AN_OVERLOAD
                        } else {
                            &messages::DECORATORS_ARE_NOT_VALID_HERE
                        };
                        return self.grammar_error_on_first_token(node, message);
                    } else if self.legacy_decorators
                        && matches!(kind, SyntaxKind::GetAccessor | SyntaxKind::SetAccessor)
                        && self.is_second_accessor_after_decorated_first(node, kind)
                    {
                        return self.grammar_error_on_first_token(
                            node,
                            &messages::DECORATORS_CANNOT_BE_APPLIED_TO_MULTIPLE_GET_SLASHSET_ACCESSORS_OF_THE_SAME_NAME,
                        );
                    }
                    // Any modifier but `export`/`default` before a decorator.
                    if flags.intersects(
                        !(ModifierFlags::EXPORT
                            | ModifierFlags::DEFAULT
                            | ModifierFlags::DECORATOR),
                    ) {
                        return self.grammar_modifier_error(
                            node,
                            decorator,
                            &messages::DECORATORS_ARE_NOT_VALID_HERE,
                            &[],
                        );
                    }
                    // Leading decorators, then modifiers, then trailing ones.
                    if has_leading_decorators && flags.intersects(MODIFIER_FLAGS_MODIFIER) {
                        let Some(first) = first_decorator else { return false };
                        let Some(file) = self.source_file_of_for_diagnostics(node) else {
                            return true;
                        };
                        let mut diagnostic = Diagnostic::new(
                            &messages::DECORATORS_MAY_NOT_APPEAR_AFTER_EXPORT_OR_EXPORT_DEFAULT_IF_THEY_ALSO_APPEAR_BEFORE_EXPORT,
                            self.error_span(decorator),
                        );
                        diagnostic.add_related_information(Some(Diagnostic::new(
                            &messages::DECORATOR_USED_BEFORE_EXPORT_HERE,
                            self.error_span(first),
                        )));
                        self.report(file, diagnostic);
                        return true;
                    }
                    flags |= ModifierFlags::DECORATOR;
                    if !flags.intersects(MODIFIER_FLAGS_MODIFIER) {
                        has_leading_decorators = true;
                    } else if flags.contains(ModifierFlags::EXPORT) {
                        saw_export_before_decorators = true;
                    }
                    if first_decorator.is_none() {
                        first_decorator = Some(decorator);
                    }
                    continue;
                }
                ModifierEntry::Modifier { kind, at } => (kind, at),
            };
            let text = modifier_text(modifier_kind);
            if modifier_kind != SyntaxKind::ReadonlyKeyword {
                if matches!(kind, SyntaxKind::PropertySignature | SyntaxKind::MethodSignature) {
                    return self.grammar_modifier_error(
                        node,
                        at,
                        &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_TYPE_MEMBER,
                        &[text],
                    );
                }
                if kind == SyntaxKind::IndexSignature
                    && (modifier_kind != SyntaxKind::StaticKeyword || !parent_is_class_like)
                {
                    return self.grammar_modifier_error(
                        node,
                        at,
                        &messages::_0_MODIFIER_CANNOT_APPEAR_ON_AN_INDEX_SIGNATURE,
                        &[text],
                    );
                }
            }
            if !matches!(
                modifier_kind,
                SyntaxKind::InKeyword | SyntaxKind::OutKeyword | SyntaxKind::ConstKeyword
            ) && kind == SyntaxKind::TypeParameter
            {
                return self.grammar_modifier_error(
                    node,
                    at,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_TYPE_PARAMETER,
                    &[text],
                );
            }
            // Every must-precede arm also tests `modifier.Flags&
            // ast.NodeFlagsReparsed == 0`, which a written modifier passes;
            // the reparsed ones are walked after the loop.
            match modifier_kind {
                SyntaxKind::ConstKeyword => {
                    if !matches!(kind, SyntaxKind::EnumDeclaration | SyntaxKind::TypeParameter) {
                        return self.grammar_modifier_error(
                            node,
                            node,
                            &messages::A_CLASS_MEMBER_CANNOT_HAVE_THE_0_KEYWORD,
                            &["const"],
                        );
                    }
                    if kind == SyntaxKind::TypeParameter
                        && !self.type_parameter_parent_kind(node).is_some_and(|parent| {
                            matches!(
                                parent,
                                SyntaxKind::FunctionDeclaration
                                    | SyntaxKind::MethodDeclaration
                                    | SyntaxKind::Constructor
                                    | SyntaxKind::GetAccessor
                                    | SyntaxKind::SetAccessor
                                    | SyntaxKind::FunctionExpression
                                    | SyntaxKind::ArrowFunction
                                    | SyntaxKind::ClassDeclaration
                                    | SyntaxKind::ClassExpression
                                    | SyntaxKind::FunctionType
                                    | SyntaxKind::ConstructorType
                                    | SyntaxKind::CallSignature
                                    | SyntaxKind::ConstructSignature
                                    | SyntaxKind::MethodSignature
                            )
                        })
                    {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_FUNCTION_METHOD_OR_CLASS,
                            &[text],
                        );
                    }
                }
                SyntaxKind::OverrideKeyword => {
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> =
                        if flags.contains(ModifierFlags::OVERRIDE) {
                            Some((&messages::_0_MODIFIER_ALREADY_SEEN, &["override"]))
                        } else if flags.contains(ModifierFlags::AMBIENT) {
                            Some((
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["override", "declare"],
                            ))
                        } else if flags.contains(ModifierFlags::READONLY) {
                            Some((
                                &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                                &["override", "readonly"],
                            ))
                        } else if flags.contains(ModifierFlags::ACCESSOR) {
                            Some((
                                &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                                &["override", "accessor"],
                            ))
                        } else if flags.contains(ModifierFlags::ASYNC) {
                            Some((
                                &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                                &["override", "async"],
                            ))
                        } else {
                            None
                        };
                    if let Some((message, args)) = report {
                        return self.grammar_modifier_error(node, at, message, args);
                    }
                    flags |= ModifierFlags::OVERRIDE;
                    last_override = Some(at);
                }
                SyntaxKind::PublicKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PrivateKeyword => {
                    // `visibilityToString(ast.ModifierToFlag(modifier.Kind))`.
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> = if flags
                        .intersects(ModifierFlags::ACCESSIBILITY_MODIFIER)
                    {
                        Some((&messages::ACCESSIBILITY_MODIFIER_ALREADY_SEEN, &[]))
                    } else if flags.contains(ModifierFlags::OVERRIDE) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["override"]))
                    } else if flags.contains(ModifierFlags::STATIC) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["static"]))
                    } else if flags.contains(ModifierFlags::ACCESSOR) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["accessor"]))
                    } else if flags.contains(ModifierFlags::READONLY) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["readonly"]))
                    } else if flags.contains(ModifierFlags::ASYNC) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["async"]))
                    } else if parent_is_module_or_file {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                            &[],
                        ))
                    } else if flags.contains(ModifierFlags::ABSTRACT) {
                        if modifier_kind == SyntaxKind::PrivateKeyword {
                            Some((
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["abstract"],
                            ))
                        } else {
                            Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["abstract"]))
                        }
                    } else if self.is_private_identifier_class_element_declaration(node) {
                        Some((
                                &messages::AN_ACCESSIBILITY_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                                &[],
                            ))
                    } else {
                        None
                    };
                    if let Some((message, rest)) = report {
                        // Every message here but the two argument-free ones
                        // leads with the keyword's own text.
                        let args: Vec<&str> = if message.code() == 1028 || message.code() == 18010 {
                            Vec::new()
                        } else {
                            std::iter::once(text).chain(rest.iter().copied()).collect()
                        };
                        return self.grammar_modifier_error(node, at, message, &args);
                    }
                    flags |= modifier_to_flag(modifier_kind);
                }
                SyntaxKind::StaticKeyword => {
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> = if flags
                        .contains(ModifierFlags::STATIC)
                    {
                        Some((&messages::_0_MODIFIER_ALREADY_SEEN, &["static"]))
                    } else if flags.contains(ModifierFlags::READONLY) {
                        Some((
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["static", "readonly"],
                        ))
                    } else if flags.contains(ModifierFlags::ASYNC) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["static", "async"]))
                    } else if flags.contains(ModifierFlags::ACCESSOR) {
                        Some((
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["static", "accessor"],
                        ))
                    } else if parent_is_module_or_file {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_MODULE_OR_NAMESPACE_ELEMENT,
                            &["static"],
                        ))
                    } else if kind == SyntaxKind::Parameter {
                        Some((&messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER, &["static"]))
                    } else if flags.contains(ModifierFlags::ABSTRACT) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                            &["static", "abstract"],
                        ))
                    } else if flags.contains(ModifierFlags::OVERRIDE) {
                        Some((
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["static", "override"],
                        ))
                    } else {
                        None
                    };
                    if let Some((message, args)) = report {
                        return self.grammar_modifier_error(node, at, message, args);
                    }
                    flags |= ModifierFlags::STATIC;
                    last_static = Some(at);
                }
                SyntaxKind::AccessorKeyword => {
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> =
                        if flags.contains(ModifierFlags::ACCESSOR) {
                            Some((&messages::_0_MODIFIER_ALREADY_SEEN, &["accessor"]))
                        } else if flags.contains(ModifierFlags::READONLY) {
                            Some((
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["accessor", "readonly"],
                            ))
                        } else if flags.contains(ModifierFlags::AMBIENT) {
                            Some((
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["accessor", "declare"],
                            ))
                        } else if kind != SyntaxKind::PropertyDeclaration {
                            Some((
                            &messages::ACCESSOR_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION,
                            &[],
                        ))
                        } else {
                            None
                        };
                    if let Some((message, args)) = report {
                        return self.grammar_modifier_error(node, at, message, args);
                    }
                    flags |= ModifierFlags::ACCESSOR;
                }
                SyntaxKind::ReadonlyKeyword => {
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> = if flags
                        .contains(ModifierFlags::READONLY)
                    {
                        Some((&messages::_0_MODIFIER_ALREADY_SEEN, &["readonly"]))
                    } else if !matches!(
                        kind,
                        SyntaxKind::PropertyDeclaration
                            | SyntaxKind::PropertySignature
                            | SyntaxKind::IndexSignature
                            | SyntaxKind::Parameter
                    ) {
                        Some((
                                &messages::READONLY_MODIFIER_CAN_ONLY_APPEAR_ON_A_PROPERTY_DECLARATION_OR_INDEX_SIGNATURE,
                                &[],
                            ))
                    } else if flags.contains(ModifierFlags::ACCESSOR) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                            &["readonly", "accessor"],
                        ))
                    } else {
                        None
                    };
                    if let Some((message, args)) = report {
                        return self.grammar_modifier_error(node, at, message, args);
                    }
                    flags |= ModifierFlags::READONLY;
                }
                SyntaxKind::ExportKeyword => {
                    if self.verbatim_module_syntax
                        && !self.has_ambient_flag(node)
                        && !matches!(
                            kind,
                            SyntaxKind::TypeAliasDeclaration
                                | SyntaxKind::InterfaceDeclaration
                                | SyntaxKind::ModuleDeclaration
                        )
                        && parent_kind == Some(SyntaxKind::SourceFile)
                        && parent.is_some_and(|file| {
                            self.emit_module_format_of(file) == tsr_core::ModuleKind::CommonJS
                        })
                    {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::A_TOP_LEVEL_EXPORT_MODIFIER_CANNOT_BE_USED_ON_VALUE_DECLARATIONS_IN_A_COMMONJS_MODULE_WHEN_VERBATIMMODULESYNTAX_IS_ENABLED,
                            &[],
                        );
                    }
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> = if flags
                        .contains(ModifierFlags::EXPORT)
                    {
                        Some((&messages::_0_MODIFIER_ALREADY_SEEN, &["export"]))
                    } else if flags.contains(ModifierFlags::AMBIENT) {
                        Some((
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["export", "declare"],
                        ))
                    } else if flags.contains(ModifierFlags::ABSTRACT) {
                        Some((
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["export", "abstract"],
                        ))
                    } else if flags.contains(ModifierFlags::ASYNC) {
                        Some((&messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER, &["export", "async"]))
                    } else if parent_is_class_like {
                        // `!ast.IsJSTypeAliasDeclaration(node)`: a JSDoc
                        // typedef is not a node of this tree.
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                            &["export"],
                        ))
                    } else if kind == SyntaxKind::Parameter {
                        Some((&messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER, &["export"]))
                    } else if is_using {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_USING_DECLARATION,
                            &["export"],
                        ))
                    } else if is_await_using {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_AN_AWAIT_USING_DECLARATION,
                            &["export"],
                        ))
                    } else {
                        None
                    };
                    if let Some((message, args)) = report {
                        return self.grammar_modifier_error(node, at, message, args);
                    }
                    flags |= ModifierFlags::EXPORT;
                }
                SyntaxKind::DefaultKeyword => {
                    // `container = node.Parent` at file scope, else
                    // `node.Parent.Parent`.
                    let container = if parent_kind == Some(SyntaxKind::SourceFile) {
                        parent
                    } else {
                        parent.and_then(|parent| self.nodes.parent(parent))
                    };
                    if container.is_some_and(|container| {
                        self.nodes.kind(container) == SyntaxKind::ModuleDeclaration
                            && !self.is_ambient_module_declaration_node(container)
                    }) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::A_DEFAULT_EXPORT_CAN_ONLY_BE_USED_IN_AN_ECMASCRIPT_STYLE_MODULE,
                            &[],
                        );
                    } else if is_using {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_USING_DECLARATION,
                            &["default"],
                        );
                    } else if is_await_using {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_AN_AWAIT_USING_DECLARATION,
                            &["default"],
                        );
                    } else if !flags.contains(ModifierFlags::EXPORT) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["export", "default"],
                        );
                    } else if saw_export_before_decorators && let Some(first) = first_decorator {
                        return self.grammar_modifier_error(
                            node,
                            first,
                            &messages::DECORATORS_ARE_NOT_VALID_HERE,
                            &[],
                        );
                    }
                    flags |= ModifierFlags::DEFAULT;
                }
                SyntaxKind::DeclareKeyword => {
                    let report: Option<(&'static tsr_diagnostics::Message, &[&str])> = if flags
                        .contains(ModifierFlags::AMBIENT)
                    {
                        Some((&messages::_0_MODIFIER_ALREADY_SEEN, &["declare"]))
                    } else if flags.contains(ModifierFlags::ASYNC) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                            &["async"],
                        ))
                    } else if flags.contains(ModifierFlags::OVERRIDE) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                            &["override"],
                        ))
                    } else if parent_is_class_like && kind != SyntaxKind::PropertyDeclaration {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_CLASS_ELEMENTS_OF_THIS_KIND,
                            &["declare"],
                        ))
                    } else if kind == SyntaxKind::Parameter {
                        Some((&messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER, &["declare"]))
                    } else if is_using {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_USING_DECLARATION,
                            &["declare"],
                        ))
                    } else if is_await_using {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_AN_AWAIT_USING_DECLARATION,
                            &["declare"],
                        ))
                    } else if parent_kind == Some(SyntaxKind::ModuleBlock)
                        && parent.is_some_and(|parent| self.has_ambient_flag(parent))
                    {
                        Some((&messages::A_DECLARE_MODIFIER_CANNOT_BE_USED_IN_AN_ALREADY_AMBIENT_CONTEXT, &[]))
                    } else if self.is_private_identifier_class_element_declaration(node) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            &["declare"],
                        ))
                    } else if flags.contains(ModifierFlags::ACCESSOR) {
                        Some((
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                            &["declare", "accessor"],
                        ))
                    } else {
                        None
                    };
                    if let Some((message, args)) = report {
                        return self.grammar_modifier_error(node, at, message, args);
                    }
                    flags |= ModifierFlags::AMBIENT;
                    last_declare = Some(at);
                }
                SyntaxKind::AbstractKeyword => {
                    if flags.contains(ModifierFlags::ABSTRACT) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_ALREADY_SEEN,
                            &["abstract"],
                        );
                    }
                    if !matches!(kind, SyntaxKind::ClassDeclaration | SyntaxKind::ConstructorType) {
                        if !matches!(
                            kind,
                            SyntaxKind::MethodDeclaration
                                | SyntaxKind::PropertyDeclaration
                                | SyntaxKind::GetAccessor
                                | SyntaxKind::SetAccessor
                        ) {
                            return self.grammar_modifier_error(
                                node,
                                at,
                                &messages::ABSTRACT_MODIFIER_CAN_ONLY_APPEAR_ON_A_CLASS_METHOD_OR_PROPERTY_DECLARATION,
                                &[],
                            );
                        }
                        let parent_is_abstract_class = parent_kind
                            == Some(SyntaxKind::ClassDeclaration)
                            && parent.is_some_and(|parent| {
                                matches!(
                                    self.node_map.get(parent),
                                    Some(Node::ClassDeclaration(class))
                                        if has_modifier(class.modifiers, SyntaxKind::AbstractKeyword)
                                )
                            });
                        if !parent_is_abstract_class {
                            let message = if kind == SyntaxKind::PropertyDeclaration {
                                &messages::ABSTRACT_PROPERTIES_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS
                            } else {
                                &messages::ABSTRACT_METHODS_CAN_ONLY_APPEAR_WITHIN_AN_ABSTRACT_CLASS
                            };
                            return self.grammar_modifier_error(node, at, message, &[]);
                        }
                        if flags.contains(ModifierFlags::STATIC) {
                            return self.grammar_modifier_error(
                                node,
                                at,
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["static", "abstract"],
                            );
                        }
                        if flags.contains(ModifierFlags::PRIVATE) {
                            return self.grammar_modifier_error(
                                node,
                                at,
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["private", "abstract"],
                            );
                        }
                        if flags.contains(ModifierFlags::ASYNC)
                            && let Some(async_modifier) = last_async
                        {
                            return self.grammar_modifier_error(
                                node,
                                async_modifier,
                                &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                                &["async", "abstract"],
                            );
                        }
                        if flags.contains(ModifierFlags::OVERRIDE) {
                            return self.grammar_modifier_error(
                                node,
                                at,
                                &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                                &["abstract", "override"],
                            );
                        }
                        if flags.contains(ModifierFlags::ACCESSOR) {
                            return self.grammar_modifier_error(
                                node,
                                at,
                                &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                                &["abstract", "accessor"],
                            );
                        }
                    }
                    if self
                        .declaration_name_of(node)
                        .is_some_and(|name| self.nodes.kind(name) == SyntaxKind::PrivateIdentifier)
                    {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_A_PRIVATE_IDENTIFIER,
                            &["abstract"],
                        );
                    }
                    flags |= ModifierFlags::ABSTRACT;
                }
                SyntaxKind::AsyncKeyword => {
                    if flags.contains(ModifierFlags::ASYNC) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_ALREADY_SEEN,
                            &["async"],
                        );
                    } else if flags.contains(ModifierFlags::AMBIENT)
                        || parent.is_some_and(|parent| self.has_ambient_flag(parent))
                    {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CANNOT_BE_USED_IN_AN_AMBIENT_CONTEXT,
                            &["async"],
                        );
                    } else if kind == SyntaxKind::Parameter {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_PARAMETER,
                            &["async"],
                        );
                    }
                    if flags.contains(ModifierFlags::ABSTRACT) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CANNOT_BE_USED_WITH_1_MODIFIER,
                            &["async", "abstract"],
                        );
                    }
                    flags |= ModifierFlags::ASYNC;
                    last_async = Some(at);
                }
                SyntaxKind::InKeyword | SyntaxKind::OutKeyword => {
                    let in_out_flag = if modifier_kind == SyntaxKind::InKeyword {
                        ModifierFlags::IN
                    } else {
                        ModifierFlags::OUT
                    };
                    let parent = self.type_parameter_parent_kind(node);
                    if kind != SyntaxKind::TypeParameter
                        || parent.is_some_and(|parent| {
                            !matches!(
                                parent,
                                SyntaxKind::InterfaceDeclaration
                                    | SyntaxKind::ClassDeclaration
                                    | SyntaxKind::ClassExpression
                                    | SyntaxKind::TypeAliasDeclaration
                            )
                        })
                    {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_CAN_ONLY_APPEAR_ON_A_TYPE_PARAMETER_OF_A_CLASS_INTERFACE_OR_TYPE_ALIAS,
                            &[text],
                        );
                    }
                    if flags.contains(in_out_flag) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_ALREADY_SEEN,
                            &[text],
                        );
                    }
                    if in_out_flag == ModifierFlags::IN && flags.contains(ModifierFlags::OUT) {
                        return self.grammar_modifier_error(
                            node,
                            at,
                            &messages::_0_MODIFIER_MUST_PRECEDE_1_MODIFIER,
                            &["in", "out"],
                        );
                    }
                    flags |= in_out_flag;
                }
                _ => {}
            }
        }
        // The reparsed JSDoc modifiers follow the written ones in upstream's
        // list; `check_jsdoc_reparsed_modifier_grammar` walks them with the
        // arms a reparsed modifier can reach, continuing from this walk's
        // state.
        if reparsed_host {
            let mut seen: Vec<SyntaxKind> = [
                (ModifierFlags::PUBLIC, SyntaxKind::PublicKeyword),
                (ModifierFlags::PRIVATE, SyntaxKind::PrivateKeyword),
                (ModifierFlags::PROTECTED, SyntaxKind::ProtectedKeyword),
                (ModifierFlags::READONLY, SyntaxKind::ReadonlyKeyword),
                (ModifierFlags::OVERRIDE, SyntaxKind::OverrideKeyword),
                (ModifierFlags::AMBIENT, SyntaxKind::DeclareKeyword),
                (ModifierFlags::ABSTRACT, SyntaxKind::AbstractKeyword),
                (ModifierFlags::ACCESSOR, SyntaxKind::AccessorKeyword),
            ]
            .into_iter()
            .filter(|(flag, _)| flags.contains(*flag))
            .map(|(_, keyword)| keyword)
            .collect();
            let before = self.diagnostics.len();
            self.check_jsdoc_reparsed_modifier_grammar(node, &mut seen);
            if self.diagnostics.len() != before {
                return true;
            }
        }
        if kind == SyntaxKind::Constructor {
            if let Some(at) = last_static {
                return self.grammar_modifier_error(
                    node,
                    at,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                    &["static"],
                );
            }
            if let Some(at) = last_override {
                return self.grammar_modifier_error(
                    node,
                    at,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                    &["override"],
                );
            }
            if let Some(at) = last_async {
                return self.grammar_modifier_error(
                    node,
                    at,
                    &messages::_0_MODIFIER_CANNOT_APPEAR_ON_A_CONSTRUCTOR_DECLARATION,
                    &["async"],
                );
            }
            return false;
        } else if matches!(
            kind,
            SyntaxKind::ImportDeclaration | SyntaxKind::ImportEqualsDeclaration
        ) && let Some(at) = last_declare
        {
            return self.grammar_modifier_error(
                node,
                at,
                &messages::A_0_MODIFIER_CANNOT_BE_USED_WITH_AN_IMPORT_DECLARATION,
                &["declare"],
            );
        } else if let Node::ParameterDeclaration(parameter) = typed
            && flags.intersects(PARAMETER_PROPERTY_MODIFIER)
        {
            if matches!(parameter.name, Some(tsr_ast::BindingName::BindingPattern(_))) {
                return self.grammar_modifier_error(
                    node,
                    node,
                    &messages::A_PARAMETER_PROPERTY_MAY_NOT_BE_DECLARED_USING_A_BINDING_PATTERN,
                    &[],
                );
            }
            if parameter.dot_dot_dot_token.is_some() {
                return self.grammar_modifier_error(
                    node,
                    node,
                    &messages::A_PARAMETER_PROPERTY_CANNOT_BE_DECLARED_USING_A_REST_PARAMETER,
                    &[],
                );
            }
        }
        if flags.contains(ModifierFlags::ASYNC)
            && let Some(at) = last_async
        {
            // `checkGrammarAsyncModifier` (`grammarchecks.go:659`).
            if matches!(
                kind,
                SyntaxKind::MethodDeclaration
                    | SyntaxKind::FunctionDeclaration
                    | SyntaxKind::FunctionExpression
                    | SyntaxKind::ArrowFunction
            ) {
                return false;
            }
            return self.grammar_modifier_error(
                node,
                at,
                &messages::_0_MODIFIER_CANNOT_BE_USED_HERE,
                &["async"],
            );
        }
        false
    }

    /// `grammarErrorOnNode(at, message, args…)` from inside
    /// `checkGrammarModifiers`: the file has no parse diagnostics (the caller
    /// returned before the walk otherwise), so it reports and answers `true`.
    fn grammar_modifier_error(
        &mut self,
        node: NodeId,
        at: NodeId,
        message: &'static tsr_diagnostics::Message,
        args: &[&str],
    ) -> bool {
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return true };
        let span = self.error_span(at);
        let diagnostic = if args.is_empty() {
            Diagnostic::new(message, span)
        } else {
            Diagnostic::with_args(message, span, args.iter().map(|arg| (*arg).to_string()))
        };
        self.report(file, diagnostic);
        true
    }

    /// `reportObviousDecoratorErrors` (`grammarchecks.go:642`): a decorator
    /// on a kind `ast.CanHaveIllegalDecorators` lists, on its first token.
    fn report_obvious_decorator_errors(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        entries: &[ModifierEntry],
    ) -> bool {
        if !matches!(
            typed,
            Node::FunctionDeclaration(_)
                | Node::ConstructorDeclaration(_)
                | Node::IndexSignatureDeclaration(_)
                | Node::ClassStaticBlockDeclaration(_)
                | Node::VariableStatement(_)
                | Node::InterfaceDeclaration(_)
                | Node::TypeAliasDeclaration(_)
                | Node::EnumDeclaration(_)
                | Node::ModuleDeclaration(_)
                | Node::ImportEqualsDeclaration(_)
                | Node::ImportDeclaration(_)
                | Node::ExportDeclaration(_)
                | Node::ExportAssignment(_)
        ) {
            return false;
        }
        let _ = node;
        let Some(decorator) = entries.iter().find_map(|entry| match entry {
            ModifierEntry::Decorator(decorator) => Some(*decorator),
            ModifierEntry::Modifier { .. } => None,
        }) else {
            return false;
        };
        self.grammar_error_on_first_token(decorator, &messages::DECORATORS_ARE_NOT_VALID_HERE)
    }

    /// `reportObviousModifierErrors` (`grammarchecks.go:569`) through
    /// `findFirstIllegalModifier` (`:584`): TS1184 on the first modifier a
    /// kind never takes, or takes only at the top level of a file or module
    /// block.
    fn report_obvious_modifier_errors(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
        entries: &[ModifierEntry],
    ) -> bool {
        let written = |except: Option<SyntaxKind>| {
            // `core.Find(node.ModifierNodes(), ast.IsModifier)`, then
            // `findFirstModifierExcept`'s test on that first modifier.
            entries
                .iter()
                .find_map(|entry| match entry {
                    ModifierEntry::Modifier { kind, at, .. } => Some((*kind, *at)),
                    ModifierEntry::Decorator(_) => None,
                })
                .filter(|(kind, _)| Some(*kind) != except)
                .map(|(_, at)| at)
        };
        let modifier = match typed {
            Node::GetAccessorDeclaration(_)
            | Node::SetAccessorDeclaration(_)
            | Node::ConstructorDeclaration(_)
            | Node::PropertyDeclaration(_)
            | Node::PropertySignatureDeclaration(_)
            | Node::MethodDeclaration(_)
            | Node::MethodSignatureDeclaration(_)
            | Node::IndexSignatureDeclaration(_)
            | Node::ModuleDeclaration(_)
            | Node::ImportDeclaration(_)
            | Node::ImportEqualsDeclaration(_)
            | Node::ExportDeclaration(_)
            | Node::ExportAssignment(_)
            | Node::FunctionExpression(_)
            | Node::ArrowFunction(_)
            | Node::ParameterDeclaration(_)
            | Node::TypeParameterDeclaration(_) => None,
            Node::ClassStaticBlockDeclaration(_) => written(None),
            _ => {
                let at_top_level = self.nodes.parent(node).is_some_and(|parent| {
                    matches!(
                        self.nodes.kind(parent),
                        SyntaxKind::ModuleBlock | SyntaxKind::SourceFile
                    )
                });
                if at_top_level {
                    None
                } else {
                    match typed {
                        Node::FunctionDeclaration(_) => written(Some(SyntaxKind::AsyncKeyword)),
                        Node::ClassDeclaration(_) | Node::ConstructorTypeNode(_) => {
                            written(Some(SyntaxKind::AbstractKeyword))
                        }
                        Node::ClassExpression(_)
                        | Node::InterfaceDeclaration(_)
                        | Node::TypeAliasDeclaration(_) => written(None),
                        Node::VariableStatement(statement) => {
                            let using = statement
                                .declaration_list
                                .and_then(|list| list.node_id)
                                .is_some_and(|list| {
                                    self.nodes.flags(list).contains(NodeFlags::USING)
                                });
                            written(using.then_some(SyntaxKind::AwaitKeyword))
                        }
                        Node::EnumDeclaration(_) => written(Some(SyntaxKind::ConstKeyword)),
                        // upstream panics on any other kind; none reaches here.
                        _ => None,
                    }
                }
            }
        };
        let Some(modifier) = modifier else { return false };
        self.grammar_error_on_first_token(modifier, &messages::MODIFIERS_CANNOT_APPEAR_HERE)
    }

    /// `ast.GetAllAccessorDeclarationsForDeclaration(node,
    /// symbol.Declarations)`'s `SecondAccessor == node`, behind
    /// `HasDecorators(FirstAccessor)`: the other accessor of the pair is the
    /// symbol's first declaration of the other kind, and whichever starts
    /// first is the first accessor.
    fn is_second_accessor_after_decorated_first(&self, node: NodeId, kind: SyntaxKind) -> bool {
        let other_kind = if kind == SyntaxKind::SetAccessor {
            SyntaxKind::GetAccessor
        } else {
            SyntaxKind::SetAccessor
        };
        let Some(symbol) = self.binder.symbol_of(node) else { return false };
        let Some(other) = self
            .binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .copied()
            .find(|&declaration| self.nodes.kind(declaration) == other_kind)
        else {
            return false;
        };
        if self.nodes.span(other).start >= self.nodes.span(node).start {
            return false;
        }
        self.node_map.get(other).and_then(crate::check::modifiers_of).is_some_and(|modifiers| {
            modifiers.iter().any(|modifier| matches!(modifier, ModifierLike::Decorator(_)))
        })
    }

    /// Whether a variable statement is `await using`
    /// (`NodeFlagsAwaitUsing`): this parser eats the `await` and flags the
    /// list `Using` alone, so [`Checker::is_await_using_list`] recovers it.
    fn is_await_using_statement(&self, typed: Node<'_>) -> bool {
        let Node::VariableStatement(statement) = typed else { return false };
        statement.declaration_list.and_then(|list| list.node_id).is_some_and(|list| {
            self.nodes.flags(list).contains(NodeFlags::USING) && self.is_await_using_list(list)
        })
    }

    /// A type parameter's `node.Parent` kind as `checkGrammarModifiers` reads
    /// it: for a JSDoc `@template` parameter, the declaration its list is
    /// reparsed into ([`Checker::jsdoc_template_owner_kind`]).
    fn type_parameter_parent_kind(&self, node: NodeId) -> Option<SyntaxKind> {
        self.jsdoc_template_owner_kind(node)
            .or_else(|| self.nodes.parent(node).map(|parent| self.nodes.kind(parent)))
    }

    /// `node.Flags & ast.NodeFlagsAmbient`. The parser sets the flag in a
    /// declaration file and on everything parsed inside a declaration whose
    /// modifiers include `declare` — a statement-level declaration
    /// (`parseDeclaration`, `parser.go:1128`) or a class property or method
    /// (`parseClassElement`, `:1878`). This parser never sets
    /// `NodeFlags::AMBIENT`, so the ancestors are walked.
    pub(crate) fn has_ambient_flag(&self, node: NodeId) -> bool {
        std::iter::once(node).chain(self.nodes.ancestors(node)).any(|at| {
            match self.node_map.get(at) {
                Some(Node::SourceFile(_)) => {
                    self.module_host.is_some_and(|host| host.is_declaration_file(at))
                }
                Some(
                    typed @ (Node::VariableStatement(_)
                    | Node::FunctionDeclaration(_)
                    | Node::ClassDeclaration(_)
                    | Node::InterfaceDeclaration(_)
                    | Node::TypeAliasDeclaration(_)
                    | Node::EnumDeclaration(_)
                    | Node::ModuleDeclaration(_)
                    | Node::ImportEqualsDeclaration(_)
                    | Node::ImportDeclaration(_)
                    | Node::ExportDeclaration(_)
                    | Node::ExportAssignment(_)
                    | Node::PropertyDeclaration(_)
                    | Node::MethodDeclaration(_)),
                ) => crate::check::modifiers_of(typed)
                    .is_some_and(|modifiers| has_modifier(modifiers, SyntaxKind::DeclareKeyword)),
                _ => false,
            }
        })
    }

    /// `ast.IsAmbientModule`: a module declaration with a string-literal
    /// name, or a `declare global` augmentation.
    fn is_ambient_module_declaration_node(&self, node: NodeId) -> bool {
        let Some(Node::ModuleDeclaration(module)) = self.node_map.get(node) else { return false };
        matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            || module.keyword.kind == SyntaxKind::GlobalKeyword
    }
}

impl Checker<'_, '_> {
    /// `Checker.checkGrammarAccessor` (`grammarchecks.go:1307`), arm for arm.
    /// Each arm returns, so an accessor gets at most one report. The caller
    /// (`checkAccessorDeclaration`, `checker.go:2932`) runs it behind
    /// `!checkGrammarFunctionLikeDeclaration(node)`, whose first conjunct is
    /// `checkGrammarModifiers`; the file has no parse diagnostics.
    pub(crate) fn check_grammar_accessor_declaration(
        &mut self,
        node: NodeId,
        typed: Node<'_>,
    ) -> bool {
        let is_get = matches!(typed, Node::GetAccessorDeclaration(_));
        let (modifiers, name, type_parameters, parameters, return_type, body) = match typed {
            Node::GetAccessorDeclaration(n) => {
                (n.modifiers, n.name, n.type_parameters, n.parameters, n.r#type, n.body)
            }
            Node::SetAccessorDeclaration(n) => {
                (n.modifiers, n.name, n.type_parameters, n.parameters, n.r#type, n.body)
            }
            _ => return false,
        };
        let is_abstract = has_modifier(modifiers, SyntaxKind::AbstractKeyword);
        let in_type = self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.nodes.kind(parent),
                SyntaxKind::TypeLiteral | SyntaxKind::InterfaceDeclaration
            )
        });
        if !in_type && body.is_none() && !is_abstract && !self.has_ambient_flag(node) {
            // `grammarErrorAtPos(accessor, accessor.End()-1, len(";"), …)`.
            let Some(file) = self.source_file_of_for_diagnostics(node) else { return true };
            let end = self.nodes.span(node).end;
            self.report(
                file,
                Diagnostic::with_args(
                    &messages::_0_EXPECTED,
                    tsr_core::Span::new(end - 1, end),
                    ["{".to_string()],
                ),
            );
            return true;
        }
        if let Some(body) = body {
            if is_abstract {
                self.grammar_error_on_node(
                    node,
                    &messages::AN_ABSTRACT_ACCESSOR_CANNOT_HAVE_AN_IMPLEMENTATION,
                );
                return true;
            }
            if in_type && let Some(body) = body.node_id() {
                self.grammar_error_on_node(
                    body,
                    &messages::AN_IMPLEMENTATION_CANNOT_BE_DECLARED_IN_AMBIENT_CONTEXTS,
                );
                return true;
            }
        }
        let Some(name) = name.node_id() else { return false };
        // `funcData.TypeParameters != nil`: this tree keeps no empty `<>`
        // list, so an empty one reads as absent.
        if !type_parameters.is_empty() {
            self.grammar_error_on_node(name, &messages::AN_ACCESSOR_CANNOT_HAVE_TYPE_PARAMETERS);
            return true;
        }
        // `doesAccessorHaveCorrectParameterCount`: `getAccessorThisParameter`
        // (`checker.go:19931`) is non-nil at one more parameter than the
        // accessor takes when the first is `this`.
        let first_is_this = parameters.first().is_some_and(|first| {
            matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        });
        let wanted = usize::from(!is_get);
        let has_this_parameter = parameters.len() == wanted + 1 && first_is_this;
        if !has_this_parameter && parameters.len() != wanted {
            let message = if is_get {
                &messages::A_GET_ACCESSOR_CANNOT_HAVE_PARAMETERS
            } else {
                &messages::A_SET_ACCESSOR_MUST_HAVE_EXACTLY_ONE_PARAMETER
            };
            self.grammar_error_on_node(name, message);
            return true;
        }
        if is_get {
            return false;
        }
        if return_type.is_some() {
            self.grammar_error_on_node(
                name,
                &messages::A_SET_ACCESSOR_CANNOT_HAVE_A_RETURN_TYPE_ANNOTATION,
            );
            return true;
        }
        // `GetSetAccessorValueParameter`: the parameter after a `this` one.
        let Some(parameter) = parameters.get(usize::from(has_this_parameter)) else {
            return false;
        };
        if let Some(rest) = parameter.dot_dot_dot_token.and_then(|token| token.node_id) {
            self.grammar_error_on_node(rest, &messages::A_SET_ACCESSOR_CANNOT_HAVE_REST_PARAMETER);
            return true;
        }
        if let Some(question) = parameter.question_token.and_then(|token| token.node_id) {
            self.grammar_error_on_node(
                question,
                &messages::A_SET_ACCESSOR_CANNOT_HAVE_AN_OPTIONAL_PARAMETER,
            );
            return true;
        }
        if parameter.initializer.is_some() {
            self.grammar_error_on_node(
                name,
                &messages::A_SET_ACCESSOR_PARAMETER_CANNOT_HAVE_AN_INITIALIZER,
            );
            return true;
        }
        false
    }
}

impl Checker<'_, '_> {
    /// `Checker.checkGrammarImportClause` (`grammarchecks.go:2118`), from
    /// `checkImportDeclaration` (`checker.go:5285`). Answers whether it
    /// reported.
    ///
    /// - `import type`: TS1363 when the clause has both a default and named
    ///   bindings (outside JSDoc), else the named imports'
    ///   `checkGrammarTypeOnlyNamedImportsOrExports`;
    /// - `import defer`: TS18058 / TS18059 / TS18060, exclusive, in order.
    ///
    /// Upstream reaches it only when `checkExternalImportOrExportDeclaration`
    /// passed; this port's arms of that function report from their own
    /// dispatch, so the clause is checked unconditionally, as the deferred arm
    /// already was.
    pub(crate) fn check_grammar_import_clause(&mut self, declaration: NodeId) -> bool {
        if self.file_has_parse_errors {
            return false;
        }
        let Some(Node::ImportDeclaration(import)) = self.node_map.get(declaration) else {
            return false;
        };
        let Some(clause) = import.import_clause else { return false };
        let Some(phase) = clause.phase_modifier else { return false };
        let Some(at) = clause.node_id else { return false };
        let message = match phase.kind {
            SyntaxKind::TypeKeyword => {
                if clause.name.is_some() && clause.named_bindings.is_some() {
                    &messages::A_TYPE_ONLY_IMPORT_CAN_SPECIFY_A_DEFAULT_IMPORT_OR_NAMED_BINDINGS_BUT_NOT_BOTH
                } else if let Some(tsr_ast::NamedImportBindings::NamedImports(named)) =
                    clause.named_bindings
                {
                    let specifiers: Vec<(bool, Option<NodeId>)> =
                        named.elements.iter().map(|s| (s.is_type_only, s.node_id)).collect();
                    return self.check_grammar_type_only_named_imports_or_exports(
                        &specifiers,
                        &messages::THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_IMPORT_WHEN_IMPORT_TYPE_IS_USED_ON_ITS_IMPORT_STATEMENT,
                    );
                } else {
                    return false;
                }
            }
            SyntaxKind::DeferKeyword => {
                if clause.name.is_some() {
                    &messages::DEFAULT_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT
                } else if matches!(
                    clause.named_bindings,
                    Some(tsr_ast::NamedImportBindings::NamedImports(_))
                ) {
                    &messages::NAMED_IMPORTS_ARE_NOT_ALLOWED_IN_A_DEFERRED_IMPORT
                } else if !matches!(
                    self.module_kind,
                    tsr_core::ModuleKind::ESNext | tsr_core::ModuleKind::Preserve
                ) {
                    &messages::DEFERRED_IMPORTS_ARE_ONLY_SUPPORTED_WHEN_THE_MODULE_FLAG_IS_SET_TO_ESNEXT_OR_PRESERVE
                } else {
                    return false;
                }
            }
            _ => return false,
        };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return true };
        // `grammarErrorOnNode(&node.Node)`: the clause's range, which upstream
        // starts at the phase modifier. This parser leaves the modifier
        // outside the clause's span, so the range is rebuilt from it.
        let end = self.nodes.span(at).end;
        let start =
            phase.node_id.map_or_else(|| self.nodes.span(at).start, |m| self.nodes.span(m).start);
        self.report(file, Diagnostic::new(message, tsr_core::Span::new(start, end.max(start))));
        true
    }

    /// `Checker.checkGrammarExportDeclaration` (`grammarchecks.go:196`):
    /// `export type { … }` takes no `type` on a specifier (TS2207).
    pub(crate) fn check_grammar_export_declaration(&mut self, declaration: NodeId) -> bool {
        if self.file_has_parse_errors {
            return false;
        }
        let Some(Node::ExportDeclaration(export)) = self.node_map.get(declaration) else {
            return false;
        };
        if !export.is_type_only {
            return false;
        }
        let Some(tsr_ast::NamedExportBindings::NamedExports(named)) = export.export_clause else {
            return false;
        };
        let specifiers: Vec<(bool, Option<NodeId>)> =
            named.elements.iter().map(|s| (s.is_type_only, s.node_id)).collect();
        self.check_grammar_type_only_named_imports_or_exports(
            &specifiers,
            &messages::THE_TYPE_MODIFIER_CANNOT_BE_USED_ON_A_NAMED_EXPORT_WHEN_EXPORT_TYPE_IS_USED_ON_ITS_EXPORT_STATEMENT,
        )
    }

    /// `Checker.checkGrammarTypeOnlyNamedImportsOrExports`
    /// (`grammarchecks.go:2141`): the first specifier with its own `type`, on
    /// its first token.
    fn check_grammar_type_only_named_imports_or_exports(
        &mut self,
        specifiers: &[(bool, Option<NodeId>)],
        message: &'static tsr_diagnostics::Message,
    ) -> bool {
        let Some(&(_, Some(specifier))) = specifiers.iter().find(|(type_only, _)| *type_only)
        else {
            return false;
        };
        self.grammar_error_on_first_token(specifier, message)
    }

    /// `checkImportEqualsDeclaration`'s grammar arm (`checker.go:5491`): an
    /// entity-name alias (`IsInternalModuleImportEqualsDeclaration`) written
    /// `import type` is TS1392, on the declaration.
    pub(crate) fn check_grammar_import_equals_type_only(&mut self, declaration: NodeId) {
        if self.file_has_parse_errors {
            return;
        }
        let Some(Node::ImportEqualsDeclaration(import)) = self.node_map.get(declaration) else {
            return;
        };
        if !import.is_type_only
            || matches!(
                import.module_reference,
                Some(tsr_ast::ModuleReference::ExternalModuleReference(_)) | None
            )
        {
            return;
        }
        self.grammar_error_on_node(declaration, &messages::AN_IMPORT_ALIAS_CANNOT_USE_IMPORT_TYPE);
    }
}

impl Checker<'_, '_> {
    /// `checkDecorators`' walk over a decorated declaration's decorators,
    /// as far as `checkDecorator`'s first step, `checkGrammarDecorator`
    /// (`checker.go:6062`): the same entry test (`ast.CanHaveDecorators`,
    /// `HasDecorators`, `NodeCanBeDecorated`), then each decorator in order.
    /// The decorator call resolution that follows is the calls lane's.
    pub(crate) fn check_decorators_grammar(&mut self, node: NodeId, typed: Node<'_>) {
        if self.file_has_parse_errors {
            return;
        }
        let modifiers = match typed {
            Node::ClassDeclaration(n) => n.modifiers,
            Node::ClassExpression(n) => n.modifiers,
            Node::PropertyDeclaration(n) => n.modifiers,
            Node::MethodDeclaration(n) => n.modifiers,
            Node::GetAccessorDeclaration(n) => n.modifiers,
            Node::SetAccessorDeclaration(n) => n.modifiers,
            Node::ParameterDeclaration(n) => n.modifiers,
            _ => return,
        };
        if !modifiers.iter().any(|m| matches!(m, ModifierLike::Decorator(_)))
            || !self.node_can_be_decorated(node, typed)
        {
            return;
        }
        for modifier in modifiers {
            if let ModifierLike::Decorator(decorator) = modifier {
                self.check_grammar_decorator(decorator);
            }
        }
    }

    /// `Checker.checkGrammarDecorator` (`grammarchecks.go:127`): a decorator
    /// expression outside `DecoratorParenthesizedExpression` /
    /// `DecoratorCallExpression` / `DecoratorMemberExpression` is TS1497 on
    /// the expression, with TS1498 related at the first offending node — the
    /// earliest of a `?.` token, a call that is not the outermost, or a
    /// non-identifier root. Non-null assertions and instantiation
    /// expressions are skipped. The caller has checked parse diagnostics.
    fn check_grammar_decorator(&mut self, decorator: &tsr_ast::Decorator<'_>) -> bool {
        let Some(at) = decorator.expression.and_then(|e| Node::from(e).node_id()) else {
            return false;
        };
        if self.nodes.kind(at) == SyntaxKind::ParenthesizedExpression {
            return false;
        }
        let mut node = at;
        let mut can_have_call_expression = true;
        let mut error_node: Option<NodeId> = None;
        loop {
            let next = match self.node_map.get(node) {
                Some(Node::ExpressionWithTypeArguments(inner)) => {
                    inner.expression.and_then(|e| e.node_id())
                }
                Some(Node::NonNullExpression(inner)) => inner.expression.and_then(|e| e.node_id()),
                Some(Node::CallExpression(call)) => {
                    if !can_have_call_expression {
                        error_node = Some(node);
                    }
                    if let Some(token) = call.question_dot_token {
                        error_node = token.node_id;
                    }
                    can_have_call_expression = false;
                    call.expression.and_then(|e| e.node_id())
                }
                Some(Node::PropertyAccessExpression(access)) => {
                    if let Some(token) = access.question_dot_token {
                        error_node = token.node_id;
                    }
                    can_have_call_expression = false;
                    access.expression.and_then(|e| e.node_id())
                }
                Some(Node::Identifier(_)) => break,
                _ => {
                    error_node = Some(node);
                    break;
                }
            };
            let Some(next) = next else { break };
            node = next;
        }
        let Some(error_node) = error_node else { return false };
        let Some(file) = self.source_file_of_for_diagnostics(at) else { return true };
        let mut diagnostic = Diagnostic::new(
            &messages::EXPRESSION_MUST_BE_ENCLOSED_IN_PARENTHESES_TO_BE_USED_AS_A_DECORATOR,
            self.error_span(at),
        );
        diagnostic.add_related_information(Some(Diagnostic::new(
            &messages::INVALID_SYNTAX_IN_DECORATOR,
            self.error_span(error_node),
        )));
        self.report(file, diagnostic);
        true
    }
}

impl Checker<'_, '_> {
    /// `Checker.checkGrammarModuleElementContext` (`grammarchecks.go:206`)
    /// with each caller's message: `checkModuleDeclaration` (`checker.go:5146`,
    /// an ambient module or a namespace), `checkImportDeclaration` (`:5274`),
    /// `checkImportEqualsDeclaration` (`:5465`) and `checkExportDeclaration`
    /// (`:5507`), the last three with a JavaScript twin. Outside a source
    /// file, module block or module declaration the statement is reported on
    /// its first token and the answer is `true` — whether or not parse
    /// diagnostics silence the report — so each caller "bails out to avoid
    /// cascading errors". `checkExportAssignment`'s use is
    /// `check_export_assignment`'s own (`check.rs`).
    pub(crate) fn check_grammar_module_element_context(&mut self, node: NodeId) -> bool {
        if !self.module_element_context_is_illegal(node) {
            return false;
        }
        let js = self.in_js_file(node);
        let message = match self.node_map.get(node) {
            Some(Node::ModuleDeclaration(module)) => {
                if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
                    || module.keyword.kind == SyntaxKind::GlobalKeyword
                {
                    &messages::AN_AMBIENT_MODULE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_IN_A_FILE
                } else {
                    &messages::A_NAMESPACE_DECLARATION_IS_ONLY_ALLOWED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE
                }
            }
            Some(Node::ImportDeclaration(_) | Node::ImportEqualsDeclaration(_)) => {
                if js {
                    &messages::AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE
                } else {
                    &messages::AN_IMPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE
                }
            }
            Some(Node::ExportDeclaration(_)) => {
                if js {
                    &messages::AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_MODULE
                } else {
                    &messages::AN_EXPORT_DECLARATION_CAN_ONLY_BE_USED_AT_THE_TOP_LEVEL_OF_A_NAMESPACE_OR_MODULE
                }
            }
            _ => return false,
        };
        self.grammar_error_on_first_token(node, message);
        true
    }

    /// `isInAppropriateContext`'s negation: the statement's parent is not a
    /// source file, module block or module declaration.
    pub(crate) fn module_element_context_is_illegal(&self, node: NodeId) -> bool {
        self.nodes.parent(node).is_some_and(|parent| {
            !matches!(
                self.nodes.kind(parent),
                SyntaxKind::SourceFile | SyntaxKind::ModuleBlock | SyntaxKind::ModuleDeclaration
            )
        })
    }
}
