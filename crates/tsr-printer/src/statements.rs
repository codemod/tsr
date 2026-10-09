//! Statements, declarations, and class members.

use tsr_ast::{ClassElement, Statement, SyntaxKind};

use crate::{ListFormat, Printer, quote_string};

impl Printer<'_> {
    pub(crate) fn emit_statement(&mut self, statement: &Statement<'_>) {
        self.write_line();
        self.emit_leading_jsdoc(statement.node_id());
        match statement {
            // Ported from `Printer.emitVariableStatement` (`internal/printer/printer.go`).
            Statement::VariableStatement(node) => {
                self.emit_modifier_list(node.modifiers);
                if let Some(list) = node.declaration_list {
                    self.variable_declaration_list(list);
                }
                self.write(";");
            }
            // Ported from `Printer.emitExpressionStatement` (`internal/printer/printer.go`).
            Statement::ExpressionStatement(node) => {
                if let Some(expression) = &node.expression {
                    // A statement starting with `{`, `function`, `class` or `let[`
                    // would be parsed as a declaration; parenthesising restores the
                    // expression reading and changes the tree, so those need care.
                    // The corpus exercises this through `({}).x` and similar, which
                    // already carry a `ParenthesizedExpression` node.
                    self.emit_expression(expression);
                }
                self.write(";");
            }
            // Ported from `Printer.emitFunctionDeclaration` (`internal/printer/printer.go`).
            Statement::FunctionDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("function");
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                match &node.body {
                    Some(tsr_ast::FunctionBody::Block(block)) => {
                        self.write(" ");
                        self.emit_block(block);
                    }
                    None => self.write(";"),
                }
            }
            // Ported from `Printer.emitClassDeclaration` (`internal/printer/printer.go`).
            Statement::ClassDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("class");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.emit_type_parameters(node.type_parameters);
                self.heritage_clauses(node.heritage_clauses);
                self.write(" ");
                self.class_body(node.members);
            }
            // Ported from `Printer.emitInterfaceDeclaration` (`internal/printer/printer.go`).
            Statement::InterfaceDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("interface");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.emit_type_parameters(node.type_parameters);
                self.heritage_clauses(node.heritage_clauses);
                self.write(" ");
                self.type_members(node.members);
            }
            // Ported from `Printer.emitTypeAliasDeclaration` (`internal/printer/printer.go`).
            Statement::TypeAliasDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("type");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.emit_type_parameters(node.type_parameters);
                self.write(" = ");
                if let Some(r#type) = &node.r#type {
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            // Ported from `Printer.emitEnumDeclaration` (`internal/printer/printer.go`).
            Statement::EnumDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("enum");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.write_space();
                self.write_punctuation("{");
                self.emit_list(node.members, ListFormat::ENUM_MEMBERS, |printer, member| {
                    printer.emit_enum_member(member);
                });
                self.write_punctuation("}");
            }
            // Ported from `Printer.emitBlock` (`internal/printer/printer.go`).
            Statement::Block(node) => self.emit_block(node),
            // Ported from `Printer.emitEmptyStatement` (`internal/printer/printer.go`).
            Statement::EmptyStatement(_) => self.write(";"),
            // Ported from `Printer.emitIfStatement` (`internal/printer/printer.go`).
            Statement::IfStatement(node) => {
                self.write("if (");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(")");
                self.nested_statement(node.then_statement.as_ref());
                if let Some(otherwise) = &node.else_statement {
                    self.write_line();
                    self.write("else");
                    self.nested_statement(Some(otherwise));
                }
            }
            // Ported from `Printer.emitReturnStatement` (`internal/printer/printer.go`).
            Statement::ReturnStatement(node) => {
                self.write("return");
                if let Some(expression) = &node.expression {
                    self.write(" ");
                    self.emit_expression(expression);
                }
                self.write(";");
            }
            // Ported from `Printer.emitThrowStatement` (`internal/printer/printer.go`).
            Statement::ThrowStatement(node) => {
                self.write("throw ");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(";");
            }
            // Ported from `Printer.emitBreakStatement` (`internal/printer/printer.go`).
            Statement::BreakStatement(node) => {
                self.write("break");
                if let Some(label) = node.label {
                    self.write(" ");
                    self.write(label.text);
                }
                self.write(";");
            }
            // Ported from `Printer.emitContinueStatement` (`internal/printer/printer.go`).
            Statement::ContinueStatement(node) => {
                self.write("continue");
                if let Some(label) = node.label {
                    self.write(" ");
                    self.write(label.text);
                }
                self.write(";");
            }
            // Ported from `Printer.emitDebuggerStatement` (`internal/printer/printer.go`).
            Statement::DebuggerStatement(_) => {
                self.write("debugger");
                self.write(";");
            }
            // Ported from `Printer.emitWhileStatement` (`internal/printer/printer.go`).
            Statement::WhileStatement(node) => {
                self.write("while (");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(")");
                self.nested_statement(Some(&node.statement));
            }
            // Ported from `Printer.emitDoStatement` (`internal/printer/printer.go`).
            Statement::DoStatement(node) => {
                self.write("do");
                self.nested_statement(Some(&node.statement));
                self.write_line();
                self.write("while (");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(");");
            }
            // Ported from `Printer.emitForStatement` (`internal/printer/printer.go`).
            Statement::ForStatement(node) => {
                self.write("for (");
                if let Some(initializer) = &node.initializer {
                    self.for_initializer(initializer);
                }
                self.write(";");
                if let Some(condition) = &node.condition {
                    self.write(" ");
                    self.emit_expression(condition);
                }
                self.write(";");
                if let Some(incrementor) = &node.incrementor {
                    self.write(" ");
                    self.emit_expression(incrementor);
                }
                self.write(")");
                self.nested_statement(Some(&node.statement));
            }
            Statement::ForInOrOfStatement(node) => {
                self.write("for ");
                if node.await_modifier.is_some() {
                    self.write("await ");
                }
                self.write("(");
                if let Some(initializer) = &node.initializer {
                    self.for_initializer(initializer);
                }
                let in_statement = self.kind_of(node.node_id) == SyntaxKind::ForInStatement;
                self.write(if in_statement { " in " } else { " of " });
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(")");
                self.nested_statement(node.statement.as_ref());
            }
            // Ported from `Printer.emitTryStatement` (`internal/printer/printer.go`).
            Statement::TryStatement(node) => {
                self.write("try ");
                if let Some(block) = node.try_block {
                    self.emit_block(block);
                }
                if let Some(clause) = node.catch_clause {
                    self.write(" catch");
                    // `Printer.emitCatchClause`: the whole declaration,
                    // initializer included (TS1197 is the checker's).
                    if let Some(declaration) = clause.variable_declaration {
                        self.write(" (");
                        self.emit_variable_declaration(declaration);
                        self.write(")");
                    }
                    self.write(" ");
                    if let Some(block) = clause.block {
                        self.emit_block(block);
                    }
                }
                if let Some(block) = node.finally_block {
                    self.write(" finally ");
                    self.emit_block(block);
                }
            }
            // Ported from `Printer.emitSwitchStatement` (`internal/printer/printer.go`).
            Statement::SwitchStatement(node) => {
                self.write("switch (");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write_punctuation(")");
                self.write_space();
                self.write_punctuation("{");
                if let Some(case_block) = node.case_block {
                    self.emit_list(
                        case_block.clauses,
                        ListFormat::CASE_BLOCK_CLAUSES,
                        |printer, clause| printer.emit_case_or_default_clause(clause),
                    );
                }
                self.write_punctuation("}");
            }
            // Ported from `Printer.emitLabeledStatement` (`internal/printer/printer.go`).
            Statement::LabeledStatement(node) => {
                if let Some(label) = node.label {
                    self.write(label.text);
                }
                self.write(":");
                self.nested_statement(node.statement.as_ref());
            }
            // Ported from `Printer.emitWithStatement` (`internal/printer/printer.go`).
            Statement::WithStatement(node) => {
                self.write("with (");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(")");
                self.nested_statement(node.statement.as_ref());
            }
            // Ported from `Printer.emitModuleDeclaration` (`internal/printer/printer.go`).
            Statement::ModuleDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.emit_token_node(node.keyword);
                // `declare global { … }` is a module whose keyword *is* its name:
                // the tree carries both `GlobalKeyword` and an identifier `global`,
                // and writing each gives `declare global global`.
                if node.keyword.kind == SyntaxKind::GlobalKeyword {
                    self.module_body(node.body.as_ref());
                    return;
                }
                self.write(" ");
                match &node.name {
                    Some(tsr_ast::ModuleName::Identifier(name)) => self.write(name.text),
                    Some(tsr_ast::ModuleName::StringLiteral(literal)) => {
                        let quoted = quote_string(literal.text, literal.token_flags);
                        self.write(&quoted);
                    }
                    None => {}
                }
                self.module_body(node.body.as_ref());
            }
            // Ported from `Printer.emitImportDeclaration` (`internal/printer/printer.go`).
            Statement::ImportDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("import ");
                if let Some(clause) = node.import_clause {
                    // `import type` / `import defer` are recorded as a phase
                    // modifier token rather than a boolean.
                    if let Some(phase) = clause.phase_modifier {
                        self.emit_token_node(phase);
                        self.write(" ");
                    }
                    let mut wrote = false;
                    if let Some(name) = clause.name {
                        self.write(name.text);
                        wrote = true;
                    }
                    match &clause.named_bindings {
                        Some(tsr_ast::NamedImportBindings::NamespaceImport(namespace)) => {
                            if wrote {
                                self.write(", ");
                            }
                            self.write("* as ");
                            if let Some(name) = namespace.name {
                                self.write(name.text);
                            }
                            wrote = true;
                        }
                        Some(tsr_ast::NamedImportBindings::NamedImports(named)) => {
                            if wrote {
                                self.write(", ");
                            }
                            // Ported from `Printer.emitNamedImports`
                            // (`printer.go:3939`): the braces are written *here*,
                            // not by the format — `LFNamedImportsOrExportsElements`
                            // deliberately carries no `LFBraces`.
                            self.write_punctuation("{");
                            self.emit_list(
                                named.elements,
                                ListFormat::NAMED_IMPORTS_OR_EXPORTS_ELEMENTS,
                                |printer, specifier| {
                                    if specifier.is_type_only {
                                        printer.write("type ");
                                    }
                                    if let Some(property) = &specifier.property_name {
                                        printer.module_export_name(property);
                                        printer.write(" as ");
                                    }
                                    if let Some(name) = specifier.name {
                                        printer.write(name.text);
                                    }
                                },
                            );
                            self.write_punctuation("}");
                            wrote = true;
                        }
                        None => {}
                    }
                    if wrote {
                        self.write(" from ");
                    }
                }
                if let Some(specifier) = &node.module_specifier {
                    self.emit_expression(specifier);
                }
                self.import_attributes(node.attributes);
                self.write(";");
            }
            // Ported from `Printer.emitExportDeclaration` (`internal/printer/printer.go`).
            Statement::ExportDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("export ");
                if node.is_type_only {
                    self.write("type ");
                }
                match &node.export_clause {
                    Some(tsr_ast::NamedExportBindings::NamedExports(named)) => {
                        // Through `emit_list`, not by hand. `LFNamedImportsOrExports\
                        // Elements` carries `NO_SPACE_IF_EMPTY`, which is the whole
                        // reason `export {}` is not `export { }` — and an empty
                        // `export {}` is the scope marker the declaration transform
                        // appends to most files, so hand-writing the braces here put
                        // two spaces in the most common line in a `.d.ts`.
                        self.write_punctuation("{");
                        self.emit_list(
                            named.elements,
                            ListFormat::NAMED_IMPORTS_OR_EXPORTS_ELEMENTS,
                            |printer, specifier| {
                                if specifier.is_type_only {
                                    printer.write("type ");
                                }
                                if let Some(property) = &specifier.property_name {
                                    printer.module_export_name(property);
                                    printer.write(" as ");
                                }
                                if let Some(name) = &specifier.name {
                                    printer.module_export_name(name);
                                }
                            },
                        );
                        self.write_punctuation("}");
                    }
                    Some(tsr_ast::NamedExportBindings::NamespaceExport(namespace)) => {
                        self.write("*");
                        if let Some(name) = &namespace.name {
                            self.write(" as ");
                            self.module_export_name(name);
                        }
                    }
                    None => self.write("*"),
                }
                if let Some(specifier) = &node.module_specifier {
                    self.write(" from ");
                    self.emit_expression(specifier);
                }
                self.import_attributes(node.attributes);
                self.write(";");
            }
            // Ported from `Printer.emitExportAssignment` (`internal/printer/printer.go`).
            Statement::ExportAssignment(node) => {
                self.emit_modifier_list(node.modifiers);
                if node.is_export_equals {
                    self.write("export = ");
                } else {
                    self.write("export default ");
                }
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
                self.write(";");
            }
            // Ported from `Printer.emitImportEqualsDeclaration` (`internal/printer/printer.go`).
            Statement::ImportEqualsDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("import ");
                if node.is_type_only {
                    self.write("type ");
                }
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.write(" = ");
                match &node.module_reference {
                    Some(tsr_ast::ModuleReference::Identifier(name)) => self.write(name.text),
                    Some(tsr_ast::ModuleReference::QualifiedName(name)) => {
                        self.emit_entity_name(&tsr_ast::EntityName::QualifiedName(name));
                    }
                    Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => {
                        self.write("require(");
                        if let Some(expression) = &reference.expression {
                            self.emit_expression(expression);
                        }
                        self.write(")");
                    }
                    None => {}
                }
                self.write(";");
            }
            // Ported from `Printer.emitNamespaceExportDeclaration` (`internal/printer/printer.go`).
            Statement::NamespaceExportDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("export as namespace ");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.write(";");
            }
            other => self.unsupported_statement(other),
        }
    }

    fn unsupported_statement(&mut self, statement: &Statement<'_>) {
        let kind = self.kind_of(statement.node_id());
        self.unsupported(kind);
    }

    /// A statement in a position where a block is conventional.
    fn nested_statement(&mut self, statement: Option<&Statement<'_>>) {
        let Some(statement) = statement else {
            self.write(";");
            return;
        };
        if matches!(statement, Statement::Block(_)) {
            self.write(" ");
            let Statement::Block(block) = statement else { unreachable!() };
            self.emit_block(block);
        } else {
            self.increase_indent();
            self.emit_statement(statement);
            self.decrease_indent();
        }
    }

    /// `with { type: "json" }` — the trailing clause of an import or export.
    ///
    /// Easy to forget because it is optional and rare, and dropping it does not
    /// produce invalid syntax: the printed text parses fine, just to a smaller
    /// tree. That makes it invisible to anything except a round trip.
    fn import_attributes(&mut self, attributes: Option<&tsr_ast::ImportAttributes<'_>>) {
        let Some(attributes) = attributes else { return };
        self.write(" ");
        self.emit_token_node(attributes.token);
        self.write(" ");
        self.import_attributes_body(attributes);
    }

    /// Emit the `{ key: value }` shared by declaration and import-type
    /// attribute syntax.
    pub(crate) fn import_attributes_body(&mut self, attributes: &tsr_ast::ImportAttributes<'_>) {
        self.write("{");
        for (index, attribute) in attributes.attributes.iter().enumerate() {
            if index > 0 {
                self.write(",");
            }
            self.write(" ");
            match &attribute.name {
                Some(tsr_ast::ImportAttributeName::Identifier(name)) => self.write(name.text),
                Some(tsr_ast::ImportAttributeName::StringLiteral(literal)) => {
                    let quoted = quote_string(literal.text, literal.token_flags);
                    self.write(&quoted);
                }
                None => {}
            }
            self.write(": ");
            if let Some(value) = &attribute.value {
                self.emit_expression(value);
            }
        }
        self.write(" }");
    }

    /// The `{ … }`, `.Inner…`, or `;` that follows a namespace's name.
    ///
    /// A dotted namespace nests: `namespace A.B {}` is a `ModuleDeclaration` whose
    /// body is another `ModuleDeclaration`. Only the *name* of the inner one is
    /// written — printing it as a statement would restate `export namespace` after
    /// the dot and produce `namespace A. export namespace B {}`.
    fn module_body(&mut self, body: Option<&tsr_ast::ModuleBody<'_>>) {
        match body {
            Some(tsr_ast::ModuleBody::ModuleBlock(block)) => {
                self.write_space();
                // An empty namespace body prints `{ }` only when the original
                // source block sat on one line — even if its statements were
                // filtered away. Bodies whose source braces span lines stay
                // multiline; empty interface and type-literal bodies always do.
                if block.statements.is_empty() && self.original_span_is_single_line(block.node_id) {
                    self.write_punctuation("{");
                    self.write(" ");
                    self.write_punctuation("}");
                    return;
                }
                self.write_punctuation("{");
                self.emit_list(
                    block.statements,
                    ListFormat::MULTI_LINE_BLOCK_STATEMENTS,
                    |printer, statement| printer.emit_statement(statement),
                );
                self.write_punctuation("}");
            }
            Some(tsr_ast::ModuleBody::ModuleDeclaration(inner)) => {
                self.write(".");
                match &inner.name {
                    Some(tsr_ast::ModuleName::Identifier(name)) => self.write(name.text),
                    Some(tsr_ast::ModuleName::StringLiteral(literal)) => {
                        let quoted = crate::quote_string(literal.text, literal.token_flags);
                        self.write(&quoted);
                    }
                    None => {}
                }
                self.module_body(inner.body.as_ref());
            }
            None => self.write(";"),
        }
    }

    /// Ported from `Printer.emitBlock` (`internal/printer/printer.go`).
    pub(crate) fn emit_block(&mut self, block: &tsr_ast::Block<'_>) {
        self.write_punctuation("{");
        self.emit_list(
            block.statements,
            ListFormat::MULTI_LINE_BLOCK_STATEMENTS,
            |printer, statement| printer.emit_statement(statement),
        );
        self.write_punctuation("}");
    }

    /// Ported from `Printer.emitEnumMember` (`internal/printer/printer.go`).
    fn emit_enum_member(&mut self, member: &tsr_ast::EnumMember<'_>) {
        self.emit_property_name(&member.name);
        if let Some(initializer) = &member.initializer {
            self.write_space();
            self.write_operator("=");
            self.write_space();
            self.emit_expression(initializer);
        }
    }

    /// Ported from `Printer.emitCaseClause`/`emitDefaultClause`
    /// (`internal/printer/printer.go`), which share one node here.
    fn emit_case_or_default_clause(&mut self, clause: &tsr_ast::CaseOrDefaultClause<'_>) {
        if self.kind_of(clause.node_id) == SyntaxKind::CaseClause {
            self.write_keyword("case");
            self.write_space();
            if let Some(expression) = &clause.expression {
                self.emit_expression(expression);
            }
        } else {
            self.write_keyword("default");
        }
        self.write_punctuation(":");
        self.emit_list(
            clause.statements,
            ListFormat::CASE_OR_DEFAULT_CLAUSE_STATEMENTS,
            |printer, statement| printer.emit_statement(statement),
        );
    }

    fn for_initializer(&mut self, initializer: &tsr_ast::ForInitializer<'_>) {
        if let tsr_ast::ForInitializer::VariableDeclarationList(list) = initializer {
            self.variable_declaration_list(list);
        } else {
            self.any_expression(tsr_ast::Node::from(*initializer));
        }
    }

    fn variable_declaration_list(&mut self, list: &tsr_ast::VariableDeclarationList<'_>) {
        // `const`/`let`/`var` live in NodeFlags, not in the tree, so the keyword is
        // recovered from the list's own kind rather than from a token.
        let keyword = list_keyword(list, self.nodes);
        self.write(keyword);
        self.write(" ");
        for (index, declaration) in list.declarations.iter().enumerate() {
            if index > 0 {
                self.write(", ");
            }
            self.emit_variable_declaration(declaration);
        }
    }

    /// Ported from `Printer.emitVariableDeclaration` (`internal/printer/printer.go`).
    fn emit_variable_declaration(&mut self, declaration: &tsr_ast::VariableDeclaration<'_>) {
        if let Some(name) = &declaration.name {
            self.emit_binding_name(name);
        }
        if declaration.exclamation_token.is_some() {
            self.write("!");
        }
        if let Some(r#type) = &declaration.r#type {
            self.write(": ");
            self.emit_type_node(r#type);
        }
        if let Some(initializer) = &declaration.initializer {
            self.write(" = ");
            self.emit_expression(initializer);
        }
    }

    fn heritage_clauses(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) {
        for clause in clauses {
            self.write(" ");
            self.emit_token_node(clause.token);
            self.write(" ");
            for (index, base) in clause.types.iter().enumerate() {
                if index > 0 {
                    self.write(", ");
                }
                if let Some(expression) = &base.expression {
                    self.emit_expression(expression);
                }
                self.emit_type_arguments(base.type_arguments);
            }
        }
    }

    fn module_export_name(&mut self, name: &tsr_ast::ModuleExportName<'_>) {
        match name {
            tsr_ast::ModuleExportName::Identifier(identifier) => self.write(identifier.text),
            tsr_ast::ModuleExportName::StringLiteral(literal) => {
                let quoted = quote_string(literal.text, literal.token_flags);
                self.write(&quoted);
            }
        }
    }

    pub(crate) fn emit_class_element(&mut self, member: &ClassElement<'_>) {
        self.emit_leading_jsdoc(member.node_id());
        match member {
            // Ported from `Printer.emitPropertyDeclaration` (`internal/printer/printer.go`).
            ClassElement::PropertyDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.emit_property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.emit_token_node(token);
                }
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                if let Some(initializer) = &node.initializer {
                    self.write(" = ");
                    self.emit_expression(initializer);
                }
                self.write(";");
            }
            // Ported from `Printer.emitMethodDeclaration` (`internal/printer/printer.go`).
            ClassElement::MethodDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                self.emit_property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.emit_token_node(token);
                }
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.function_body(node.body.as_ref());
            }
            // Ported from `Printer.emitConstructor` (`internal/printer/printer.go`).
            ClassElement::ConstructorDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                self.write("constructor");
                self.emit_type_parameters(node.type_parameters);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.function_body(node.body.as_ref());
            }
            // Ported from `Printer.emitGetAccessorDeclaration` (`internal/printer/printer.go`).
            ClassElement::GetAccessorDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("get ");
                self.emit_property_name(&node.name);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.function_body(node.body.as_ref());
            }
            // Ported from `Printer.emitSetAccessorDeclaration` (`internal/printer/printer.go`).
            // Upstream reaches `emitSignature`, which writes a return type even on a
            // setter — a grammar error the checker reports, not the printer's to drop.
            ClassElement::SetAccessorDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("set ");
                self.emit_property_name(&node.name);
                self.emit_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.function_body(node.body.as_ref());
            }
            // Ported from `Printer.emitIndexSignature` (`internal/printer/printer.go`).
            ClassElement::IndexSignatureDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                // **`LFIndexSignatureParameters`, not a hand-rolled bracket.**
                // Upstream's `emitIndexSignature` emits a parameter LIST, so it
                // gets the comma delimiter and each parameter's own modifiers,
                // `...` and `?` for free. The hand-rolled loop here emitted
                // only a name and a type, which was survivable while the parser
                // recognised exactly `[id: T]` and became ten
                // `printer_round_trip` failures the moment §209 admitted
                // upstream's other eight shapes: `[public x: string]` lost its
                // modifier and `[x?]` lost its question mark, so the re-parse
                // read a computed property name instead. §209.
                self.emit_index_signature_parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.emit_type_node(r#type);
                }
                self.write(";");
            }
            // Ported from `Printer.emitClassStaticBlockDeclaration` (`internal/printer/printer.go`).
            ClassElement::ClassStaticBlockDeclaration(node) => {
                self.emit_modifier_list(node.modifiers);
                self.write("static ");
                if let Some(block) = node.body {
                    self.emit_block(block);
                }
            }
            ClassElement::SemicolonClassElement(_) => self.write(";"),
        }
    }

    fn function_body(&mut self, body: Option<&tsr_ast::FunctionBody<'_>>) {
        match body {
            Some(tsr_ast::FunctionBody::Block(block)) => {
                self.write(" ");
                self.emit_block(block);
            }
            None => self.write(";"),
        }
    }

    pub(crate) fn emit_object_member(&mut self, member: &tsr_ast::ObjectLiteralElementLike<'_>) {
        use tsr_ast::ObjectLiteralElementLike as Member;
        match member {
            // Ported from `Printer.emitPropertyAssignment` (`internal/printer/printer.go`).
            Member::PropertyAssignment(node) => {
                self.emit_property_name(&node.name);
                // §405: the recovery `?` (`{ a?: 1 }`) round-trips.
                if node.postfix_token.is_some() {
                    self.write("?");
                }
                self.write(": ");
                if let Some(initializer) = &node.initializer {
                    self.emit_expression(initializer);
                }
            }
            // Ported from `Printer.emitShorthandPropertyAssignment` (`internal/printer/printer.go`).
            Member::ShorthandPropertyAssignment(node) => {
                self.emit_property_name(&node.name);
                // §405: the recovery `?` the parser now keeps (`{ name?, id? }`)
                // round-trips — upstream emits the postfix token too.
                if node.postfix_token.is_some() {
                    self.write("?");
                }
                if let Some(initializer) = &node.object_assignment_initializer {
                    self.write(" = ");
                    self.emit_expression(initializer);
                }
            }
            // Ported from `Printer.emitSpreadAssignment` (`internal/printer/printer.go`).
            Member::SpreadAssignment(node) => {
                self.write("...");
                if let Some(expression) = &node.expression {
                    self.emit_expression(expression);
                }
            }
            Member::MethodDeclaration(_)
            | Member::GetAccessorDeclaration(_)
            | Member::SetAccessorDeclaration(_) => {
                // Object-literal methods and accessors share their class shapes.
                let element = match member {
                    Member::MethodDeclaration(node) => ClassElement::MethodDeclaration(node),
                    Member::GetAccessorDeclaration(node) => {
                        ClassElement::GetAccessorDeclaration(node)
                    }
                    Member::SetAccessorDeclaration(node) => {
                        ClassElement::SetAccessorDeclaration(node)
                    }
                    _ => unreachable!("guarded by the outer match"),
                };
                self.emit_class_element(&element);
            }
        }
    }
}

/// The declaration keyword a list was written with.
///
/// `let`/`const`/`using` are `NodeFlags` on the list rather than a token in the
/// tree, so the keyword is recovered from the side table. Getting this wrong is
/// not a formatting difference: `var` and `const` are different declarations.
fn list_keyword(
    list: &tsr_ast::VariableDeclarationList<'_>,
    nodes: &tsr_ast::NodeTable,
) -> &'static str {
    let Some(id) = list.node_id else { return "var" };
    let flags = nodes.flags(id);
    // `NodeFlagsAwaitUsing` is `CONST | USING`.
    if flags.contains(tsr_ast::NodeFlags::CONST | tsr_ast::NodeFlags::USING) {
        "await using"
    } else if flags.contains(tsr_ast::NodeFlags::USING) {
        "using"
    } else if flags.contains(tsr_ast::NodeFlags::CONST) {
        "const"
    } else if flags.contains(tsr_ast::NodeFlags::LET) {
        "let"
    } else {
        "var"
    }
}
