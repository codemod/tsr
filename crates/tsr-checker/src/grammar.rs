//! Grammar checks ported from typescript-go's `internal/checker/grammarchecks.go`
//! that the parser lane owns.
//!
//! Each one is a `grammarError*` report: it stands only in a file without
//! parse diagnostics, which the caller (`check_grammar_modifier_shapes`)
//! already guarantees.

use tsr_ast::{
    Expression, ModifierLike, Node, NodeFlags, NodeId, ObjectLiteralElementLike, Statement,
    SyntaxKind, TypeNode,
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

    /// The two `checkGrammarModifiers` tests (`grammarchecks.go:221`, `:245`)
    /// that run before its per-keyword switch and return from it:
    ///
    /// - TS1433 — a `this` parameter takes neither decorators nor modifiers;
    /// - TS1206 / TS1249 — a decorator on a node `ast.NodeCanBeDecorated`
    ///   rejects, reported on the node's first token (its first decorator).
    ///
    /// Returns whether it reported; the caller then skips the rest of the
    /// modifier chain, and `modifier_chain_reported` keeps the rules split out
    /// of the chain quiet, as upstream's `!checkGrammarModifiers(node)` does.
    ///
    /// The kinds `reportObviousDecoratorErrors` rejects outright are
    /// `check_illegal_decorator`'s; the legacy private-name arm of
    /// `NodeCanBeDecorated` is `check_decorated_private_name`'s, so it answers
    /// "can be decorated" here rather than reporting twice.
    pub(crate) fn check_grammar_decorator_target(&mut self, node: NodeId, typed: Node<'_>) -> bool {
        let Some(modifiers) = crate::check::modifiers_of(typed) else { return false };
        if modifiers.is_empty() {
            return false;
        }
        let message = if let Node::ParameterDeclaration(parameter) = typed
            && matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        {
            &messages::NEITHER_DECORATORS_NOR_MODIFIERS_MAY_BE_APPLIED_TO_THIS_PARAMETERS
        } else if !modifiers.iter().any(|m| matches!(m, ModifierLike::Decorator(_)))
            || self.node_can_be_decorated(node, typed)
        {
            return false;
        } else if matches!(typed, Node::MethodDeclaration(method) if method.body.is_none()) {
            &messages::A_DECORATOR_CAN_ONLY_DECORATE_A_METHOD_IMPLEMENTATION_NOT_AN_OVERLOAD
        } else {
            &messages::DECORATORS_ARE_NOT_VALID_HERE
        };
        self.modifier_chain_reported.insert(node);
        self.decorator_error_reported.insert(node);
        // `grammarErrorOnFirstToken(node, …)`: silent in a file with parse
        // diagnostics (the parser already reported TS1433 on a `this`
        // parameter, `parser.go:3334`); the node starts at its first decorator
        // or modifier.
        self.grammar_error_on_first_token(node, message);
        true
    }

    /// `ast.NodeCanBeDecorated` (`ast/utilities.go:4254`) for the kinds that
    /// reach `checkGrammarModifiers`' decorator arm, under
    /// `experimentalDecorators` (`legacy_decorators`) or standard decorators.
    fn node_can_be_decorated(&self, node: NodeId, typed: Node<'_>) -> bool {
        let legacy = self.legacy_decorators;
        let parent = self.nodes.parent(node);
        let parent_kind = parent.map(|parent| self.nodes.kind(parent));
        let parent_is_class_declaration = parent_kind == Some(SyntaxKind::ClassDeclaration);
        let parent_is_class_like =
            matches!(parent_kind, Some(SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression));
        let private_name = |name: tsr_ast::PropertyName<'_>| {
            matches!(name, tsr_ast::PropertyName::PrivateIdentifier(_))
        };
        match typed {
            Node::ClassDeclaration(_) => true,
            Node::ClassExpression(_) => !legacy,
            Node::PropertyDeclaration(property) => {
                (legacy && (private_name(property.name) || parent_is_class_declaration))
                    || (!legacy
                        && parent_is_class_like
                        && !tsr_ast::has_syntactic_modifier(
                            property.modifiers,
                            SyntaxKind::AbstractKeyword,
                        )
                        && !tsr_ast::has_syntactic_modifier(
                            property.modifiers,
                            SyntaxKind::DeclareKeyword,
                        ))
            }
            Node::MethodDeclaration(method) => {
                (legacy && private_name(method.name))
                    || (method.body.is_some()
                        && (if legacy {
                            parent_is_class_declaration
                        } else {
                            parent_is_class_like
                        }))
            }
            Node::GetAccessorDeclaration(accessor) => {
                (legacy && private_name(accessor.name))
                    || (accessor.body.is_some()
                        && (if legacy {
                            parent_is_class_declaration
                        } else {
                            parent_is_class_like
                        }))
            }
            Node::SetAccessorDeclaration(accessor) => {
                (legacy && private_name(accessor.name))
                    || (accessor.body.is_some()
                        && (if legacy {
                            parent_is_class_declaration
                        } else {
                            parent_is_class_like
                        }))
            }
            Node::ParameterDeclaration(_) => {
                // Standard decorators do not decorate parameters yet.
                if !legacy {
                    return false;
                }
                let Some(parent) = parent else { return false };
                let has_body = match self.node_map.get(parent) {
                    Some(Node::ConstructorDeclaration(n)) => n.body.is_some(),
                    Some(Node::MethodDeclaration(n)) => n.body.is_some(),
                    Some(Node::SetAccessorDeclaration(n)) => n.body.is_some(),
                    _ => false,
                };
                // `GetThisParameter(parent) != node` is the TS1433 arm's,
                // which the caller has already taken.
                has_body
                    && self
                        .nodes
                        .parent(parent)
                        .is_some_and(|grand| self.nodes.kind(grand) == SyntaxKind::ClassDeclaration)
            }
            _ => false,
        }
    }

    /// The parser lane's statement checks that upstream reports with
    /// `c.error` rather than `grammarErrorOnNode`, so they stand whether or not
    /// the file has parse diagnostics.
    pub(crate) fn check_parser_lane_statement(&mut self, typed: Node<'_>) {
        // `checkGrammarTaggedTemplateChain` (`grammarchecks.go:859`), from
        // `checkTaggedTemplateExpression`: TS1358 on the template of a tagged
        // template in an optional chain.
        if let Node::TaggedTemplateExpression(tagged) = typed
            && !self.file_has_parse_errors
            && let Some(id) = tagged.node_id
            && (tagged.question_dot_token.is_some()
                || self.nodes.flags(id).contains(tsr_ast::NodeFlags::OPTIONAL_CHAIN))
            && let Some(template) = tagged.template.and_then(|template| template.node_id())
            && let Some(file) = self.source_file_of_for_diagnostics(template)
        {
            let span = self.error_span(template);
            self.report(
                file,
                Diagnostic::new(
                    &messages::TAGGED_TEMPLATE_EXPRESSIONS_ARE_NOT_PERMITTED_IN_AN_OPTIONAL_CHAIN,
                    span,
                ),
            );
        }
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
    /// Upstream reaches it after `checkGrammarModuleElementContext`, whose
    /// report returns first; that grammar error needs a declaration outside a
    /// source file or module block, so the bound is the parent's kind unless
    /// the file's parse errors silence the grammar report.
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
            matches!(self.nodes.kind(parent), SyntaxKind::SourceFile | SyntaxKind::ModuleBlock)
        });
        if !at_module_level && !self.file_has_parse_errors {
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
            // `checkClassExpression` → `checkGrammarModifiers`; a class
            // expression never reaches `check_modifier_order`.
            Node::ClassExpression(_) => {
                self.check_grammar_decorator_target(node, typed);
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
    /// | interface | initializer | TS1246 |
    /// | type literal | initializer | TS1247 |
    /// | any (property declaration) | `!` with an initializer / without a type / where not permitted | TS1263 / TS1264 / TS1255 |
    ///
    /// Ported elsewhere, and consulted here only for the short-circuit:
    /// `check_field_named_constructor` (TS18006) and
    /// `check_interface_computed_name` (TS1169/TS1170). Not ported: the mapped
    /// type arm (TS7061, a computed `in` expression) — such a name returns
    /// before anything here — and `checkAmbientInitializer`, which is
    /// `check_ambient_initializer`'s.
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
    /// without its trailing `checkGrammarAwaitOrAwaitUsing`, which is reported
    /// elsewhere. Returns whether it reported.
    pub(crate) fn check_grammar_variable_declaration_list(&mut self, list: NodeId) -> bool {
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
        // `NodeFlagsAwaitUsing` is `Const | Using` (`nodeflags.go:51`), as the
        // parser now flags an `await using` list, which starts at the `await`.
        let block_scope = self.nodes.flags(list) & NodeFlags::BLOCK_SCOPED;
        if block_scope != NodeFlags::USING && block_scope != NodeFlags::CONSTANT {
            return false;
        }
        let using = block_scope == NodeFlags::USING;
        let Some(parent) = self.nodes.parent(list) else { return false };
        if self.nodes.kind(parent) == SyntaxKind::ForInStatement {
            self.grammar_error_on_node(
                list,
                if using {
                    &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_A_USING_DECLARATION
                } else {
                    &messages::THE_LEFT_HAND_SIDE_OF_A_FOR_IN_STATEMENT_CANNOT_BE_AN_AWAIT_USING_DECLARATION
                },
            );
            return true;
        }
        // `declarationList.Flags&NodeFlagsAmbient`: the parser's ambient
        // context, which this tree answers through the declaration's context.
        if self.file_is_ambient || self.declaration_is_in_an_ambient_context(list) {
            self.grammar_error_on_node(
                list,
                if using {
                    &messages::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS
                } else {
                    &messages::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_AMBIENT_CONTEXTS
                },
            );
            return true;
        }
        if self.nodes.kind(parent) == SyntaxKind::VariableStatement
            && self.nodes.parent(parent).is_some_and(|clause| {
                matches!(
                    self.nodes.kind(clause),
                    SyntaxKind::CaseClause | SyntaxKind::DefaultClause
                )
            })
        {
            self.grammar_error_on_node(
                list,
                if using {
                    &messages::USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_WITHIN_A_BLOCK
                } else {
                    &messages::AWAIT_USING_DECLARATIONS_ARE_NOT_ALLOWED_IN_CASE_OR_DEFAULT_CLAUSES_UNLESS_CONTAINED_WITHIN_A_BLOCK
                },
            );
            return true;
        }
        false
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

    /// The line-terminator arm of `checkGrammarArrowFunction`
    /// (`grammarchecks.go:791`): TS1200 on the `=>` when its *full* text
    /// (`file.Text()[token.Pos():token.End()]`, leading trivia included)
    /// holds a line break.
    ///
    /// The `=>` token node's span starts at the token itself, so the token's
    /// full start, the end of the token before it, is recovered by scanning
    /// from the end of the last child that precedes the arrow (the return
    /// type, else the last parameter, else the last type parameter, else the
    /// arrow's start): only `,`, `)` and `>` can sit between that child and
    /// the `=>`. Error path only: the text before the `=>` is scanned only
    /// when it holds a line break at all.
    ///
    /// Not ported: the `.mts`/`.cts` single-type-parameter arm (TS7060).
    pub(crate) fn check_grammar_arrow_line_terminator(&mut self, node: NodeId) {
        let Some(Node::ArrowFunction(arrow)) = self.node_map.get(node) else { return };
        if self.file_has_parse_errors {
            return;
        }
        let Some(token) = arrow.equals_greater_than_token.and_then(|token| token.node_id) else {
            return;
        };
        let arrow_span = self.nodes.span(token);
        let anchor = arrow
            .r#type
            .and_then(|ty| ty.node_id())
            .or_else(|| arrow.parameters.last().and_then(|parameter| parameter.node_id))
            .or_else(|| arrow.type_parameters.last().and_then(|parameter| parameter.node_id))
            .map_or(self.nodes.span(node).start, |child| self.nodes.span(child).end);
        let Some(file) = self.source_file_of_for_diagnostics(node) else { return };
        let Some(text) = self.module_host.and_then(|host| host.source_text(file, self.nodes))
        else {
            return;
        };
        let Some(between) = text.get(anchor as usize..arrow_span.start as usize) else { return };
        if !between.chars().any(tsr_scanner::is_line_break) {
            return;
        }
        // The end of the last token before the `=>`.
        let mut scanner = tsr_scanner::Scanner::new(between);
        let mut full_start = 0;
        loop {
            let scanned = scanner.scan();
            if scanned.kind == SyntaxKind::EndOfFile {
                break;
            }
            full_start = scanned.span.end;
        }
        if !between[full_start as usize..].chars().any(tsr_scanner::is_line_break) {
            return;
        }
        self.report(
            file,
            Diagnostic::new(&messages::LINE_TERMINATOR_NOT_PERMITTED_BEFORE_ARROW, arrow_span),
        );
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
