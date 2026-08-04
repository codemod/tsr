//! Statements, declarations, and class members.

use tsr_ast::{ClassElement, Statement, SyntaxKind};

use crate::{Printer, quote_string};

impl Printer<'_> {
    pub(crate) fn statement(&mut self, statement: &Statement<'_>) {
        self.newline();
        match statement {
            Statement::VariableStatement(node) => {
                self.modifiers(node.modifiers);
                if let Some(list) = node.declaration_list {
                    self.variable_declaration_list(list);
                }
                self.write_raw(";");
            }
            Statement::ExpressionStatement(node) => {
                if let Some(expression) = &node.expression {
                    // A statement starting with `{`, `function`, `class` or `let[`
                    // would be parsed as a declaration; parenthesising restores the
                    // expression reading and changes the tree, so those need care.
                    // The corpus exercises this through `({}).x` and similar, which
                    // already carry a `ParenthesizedExpression` node.
                    self.expression(expression);
                }
                self.write_raw(";");
            }
            Statement::FunctionDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("function");
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                match &node.body {
                    Some(tsr_ast::FunctionBody::Block(block)) => {
                        self.write(" ");
                        self.block(block);
                    }
                    None => self.write_raw(";"),
                }
            }
            Statement::ClassDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("class");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.type_parameters(node.type_parameters);
                self.heritage_clauses(node.heritage_clauses);
                self.write(" ");
                self.class_body(node.members);
            }
            Statement::InterfaceDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("interface");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.type_parameters(node.type_parameters);
                self.heritage_clauses(node.heritage_clauses);
                self.write(" ");
                self.type_members(node.members);
            }
            Statement::TypeAliasDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("type");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.type_parameters(node.type_parameters);
                self.write(" = ");
                if let Some(r#type) = &node.r#type {
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            Statement::EnumDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("enum");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.write(" {");
                self.indented(|printer| {
                    for (index, member) in node.members.iter().enumerate() {
                        if index > 0 {
                            printer.write_raw(",");
                        }
                        printer.newline();
                        printer.property_name(&member.name);
                        if let Some(initializer) = &member.initializer {
                            printer.write(" = ");
                            printer.expression(initializer);
                        }
                    }
                });
                self.newline();
                self.write_raw("}");
            }
            Statement::Block(node) => self.block(node),
            Statement::EmptyStatement(_) => self.write_raw(";"),
            Statement::IfStatement(node) => {
                self.write("if (");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(")");
                self.nested_statement(node.then_statement.as_ref());
                if let Some(otherwise) = &node.else_statement {
                    self.newline();
                    self.write("else");
                    self.nested_statement(Some(otherwise));
                }
            }
            Statement::ReturnStatement(node) => {
                self.write("return");
                if let Some(expression) = &node.expression {
                    self.write(" ");
                    self.expression(expression);
                }
                self.write_raw(";");
            }
            Statement::ThrowStatement(node) => {
                self.write("throw ");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(";");
            }
            Statement::BreakStatement(node) => {
                self.write("break");
                if let Some(label) = node.label {
                    self.write(" ");
                    self.write(label.text);
                }
                self.write_raw(";");
            }
            Statement::ContinueStatement(node) => {
                self.write("continue");
                if let Some(label) = node.label {
                    self.write(" ");
                    self.write(label.text);
                }
                self.write_raw(";");
            }
            Statement::DebuggerStatement(_) => {
                self.write("debugger");
                self.write_raw(";");
            }
            Statement::WhileStatement(node) => {
                self.write("while (");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(")");
                self.nested_statement(Some(&node.statement));
            }
            Statement::DoStatement(node) => {
                self.write("do");
                self.nested_statement(Some(&node.statement));
                self.newline();
                self.write("while (");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(");");
            }
            Statement::ForStatement(node) => {
                self.write("for (");
                if let Some(initializer) = &node.initializer {
                    self.for_initializer(initializer);
                }
                self.write_raw(";");
                if let Some(condition) = &node.condition {
                    self.write(" ");
                    self.expression(condition);
                }
                self.write_raw(";");
                if let Some(incrementor) = &node.incrementor {
                    self.write(" ");
                    self.expression(incrementor);
                }
                self.write_raw(")");
                self.nested_statement(Some(&node.statement));
            }
            Statement::ForInOrOfStatement(node) => {
                self.write("for ");
                if node.await_modifier.is_some() {
                    self.write("await ");
                }
                self.write_raw("(");
                if let Some(initializer) = &node.initializer {
                    self.for_initializer(initializer);
                }
                let in_statement = self.kind_of(node.node_id) == SyntaxKind::ForInStatement;
                self.write(if in_statement { " in " } else { " of " });
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(")");
                self.nested_statement(node.statement.as_ref());
            }
            Statement::TryStatement(node) => {
                self.write("try ");
                if let Some(block) = node.try_block {
                    self.block(block);
                }
                if let Some(clause) = node.catch_clause {
                    self.write(" catch");
                    if let Some(declaration) = clause.variable_declaration {
                        self.write(" (");
                        if let Some(name) = &declaration.name {
                            self.binding_name(name);
                        }
                        if let Some(r#type) = &declaration.r#type {
                            self.write(": ");
                            self.type_node(r#type);
                        }
                        self.write_raw(")");
                    }
                    self.write(" ");
                    if let Some(block) = clause.block {
                        self.block(block);
                    }
                }
                if let Some(block) = node.finally_block {
                    self.write(" finally ");
                    self.block(block);
                }
            }
            Statement::SwitchStatement(node) => {
                self.write("switch (");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(") {");
                if let Some(case_block) = node.case_block {
                    self.indented(|printer| {
                        for clause in case_block.clauses {
                            printer.newline();
                            if printer.kind_of(clause.node_id) == SyntaxKind::CaseClause {
                                printer.write("case ");
                                if let Some(expression) = &clause.expression {
                                    printer.expression(expression);
                                }
                                printer.write_raw(":");
                            } else {
                                printer.write("default:");
                            }
                            printer.indented(|printer| {
                                for statement in clause.statements {
                                    printer.statement(statement);
                                }
                            });
                        }
                    });
                }
                self.newline();
                self.write_raw("}");
            }
            Statement::LabeledStatement(node) => {
                if let Some(label) = node.label {
                    self.write(label.text);
                }
                self.write_raw(":");
                self.nested_statement(node.statement.as_ref());
            }
            Statement::WithStatement(node) => {
                self.write("with (");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(")");
                self.nested_statement(node.statement.as_ref());
            }
            Statement::ModuleDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.token(node.keyword);
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
                        let quoted = quote_string(literal.text);
                        self.write(&quoted);
                    }
                    None => {}
                }
                self.module_body(node.body.as_ref());
            }
            Statement::ImportDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("import ");
                if let Some(clause) = node.import_clause {
                    // `import type` / `import defer` are recorded as a phase
                    // modifier token rather than a boolean.
                    if let Some(phase) = clause.phase_modifier {
                        self.token(phase);
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
                                self.write_raw(", ");
                            }
                            self.write("* as ");
                            if let Some(name) = namespace.name {
                                self.write(name.text);
                            }
                            wrote = true;
                        }
                        Some(tsr_ast::NamedImportBindings::NamedImports(named)) => {
                            if wrote {
                                self.write_raw(", ");
                            }
                            self.write("{ ");
                            for (index, specifier) in named.elements.iter().enumerate() {
                                if index > 0 {
                                    self.write_raw(", ");
                                }
                                if specifier.is_type_only {
                                    self.write("type ");
                                }
                                if let Some(property) = &specifier.property_name {
                                    self.module_export_name(property);
                                    self.write(" as ");
                                }
                                if let Some(name) = specifier.name {
                                    self.write(name.text);
                                }
                            }
                            self.write(" }");
                            wrote = true;
                        }
                        None => {}
                    }
                    if wrote {
                        self.write(" from ");
                    }
                }
                if let Some(specifier) = &node.module_specifier {
                    self.expression(specifier);
                }
                self.import_attributes(node.attributes);
                self.write_raw(";");
            }
            Statement::ExportDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("export ");
                if node.is_type_only {
                    self.write("type ");
                }
                match &node.export_clause {
                    Some(tsr_ast::NamedExportBindings::NamedExports(named)) => {
                        self.write("{ ");
                        for (index, specifier) in named.elements.iter().enumerate() {
                            if index > 0 {
                                self.write_raw(", ");
                            }
                            if specifier.is_type_only {
                                self.write("type ");
                            }
                            if let Some(property) = &specifier.property_name {
                                self.module_export_name(property);
                                self.write(" as ");
                            }
                            if let Some(name) = &specifier.name {
                                self.module_export_name(name);
                            }
                        }
                        self.write(" }");
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
                    self.expression(specifier);
                }
                self.import_attributes(node.attributes);
                self.write_raw(";");
            }
            Statement::ExportAssignment(node) => {
                self.modifiers(node.modifiers);
                if node.is_export_equals {
                    self.write("export = ");
                } else {
                    self.write("export default ");
                }
                if let Some(expression) = &node.expression {
                    self.expression(expression);
                }
                self.write_raw(";");
            }
            Statement::ImportEqualsDeclaration(node) => {
                self.modifiers(node.modifiers);
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
                        self.entity_name(&tsr_ast::EntityName::QualifiedName(name));
                    }
                    Some(tsr_ast::ModuleReference::ExternalModuleReference(reference)) => {
                        self.write("require(");
                        if let Some(expression) = &reference.expression {
                            self.expression(expression);
                        }
                        self.write_raw(")");
                    }
                    None => {}
                }
                self.write_raw(";");
            }
            Statement::NamespaceExportDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("export as namespace ");
                if let Some(name) = node.name {
                    self.write(name.text);
                }
                self.write_raw(";");
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
            self.write_raw(";");
            return;
        };
        if matches!(statement, Statement::Block(_)) {
            self.write(" ");
            let Statement::Block(block) = statement else { unreachable!() };
            self.block(block);
        } else {
            self.indented(|printer| printer.statement(statement));
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
        self.token(attributes.token);
        self.write(" {");
        for (index, attribute) in attributes.attributes.iter().enumerate() {
            if index > 0 {
                self.write_raw(",");
            }
            self.write(" ");
            match &attribute.name {
                Some(tsr_ast::ImportAttributeName::Identifier(name)) => self.write(name.text),
                Some(tsr_ast::ImportAttributeName::StringLiteral(literal)) => {
                    let quoted = quote_string(literal.text);
                    self.write(&quoted);
                }
                None => {}
            }
            self.write_raw(": ");
            if let Some(value) = &attribute.value {
                self.expression(value);
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
                self.write(" {");
                self.indented(|printer| {
                    for inner in block.statements {
                        printer.statement(inner);
                    }
                });
                self.newline();
                self.write_raw("}");
            }
            Some(tsr_ast::ModuleBody::ModuleDeclaration(inner)) => {
                self.write_raw(".");
                match &inner.name {
                    Some(tsr_ast::ModuleName::Identifier(name)) => self.write_raw(name.text),
                    Some(tsr_ast::ModuleName::StringLiteral(literal)) => {
                        let quoted = crate::quote_string(literal.text);
                        self.write_raw(&quoted);
                    }
                    None => {}
                }
                self.module_body(inner.body.as_ref());
            }
            None => self.write_raw(";"),
        }
    }

    pub(crate) fn block(&mut self, block: &tsr_ast::Block<'_>) {
        self.write("{");
        self.indented(|printer| {
            for statement in block.statements {
                printer.statement(statement);
            }
        });
        self.newline();
        self.write_raw("}");
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
                self.write_raw(", ");
            }
            if let Some(name) = &declaration.name {
                self.binding_name(name);
            }
            if declaration.exclamation_token.is_some() {
                self.write("!");
            }
            if let Some(r#type) = &declaration.r#type {
                self.write(": ");
                self.type_node(r#type);
            }
            if let Some(initializer) = &declaration.initializer {
                self.write(" = ");
                self.expression(initializer);
            }
        }
    }

    fn heritage_clauses(&mut self, clauses: &[&tsr_ast::HeritageClause<'_>]) {
        for clause in clauses {
            self.write(" ");
            self.token(clause.token);
            self.write(" ");
            for (index, base) in clause.types.iter().enumerate() {
                if index > 0 {
                    self.write_raw(", ");
                }
                if let Some(expression) = &base.expression {
                    self.expression(expression);
                }
                self.type_arguments(base.type_arguments);
            }
        }
    }

    fn module_export_name(&mut self, name: &tsr_ast::ModuleExportName<'_>) {
        match name {
            tsr_ast::ModuleExportName::Identifier(identifier) => self.write(identifier.text),
            tsr_ast::ModuleExportName::StringLiteral(literal) => {
                let quoted = quote_string(literal.text);
                self.write(&quoted);
            }
        }
    }

    pub(crate) fn class_element(&mut self, member: &ClassElement<'_>) {
        match member {
            ClassElement::PropertyDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.token(token);
                }
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                if let Some(initializer) = &node.initializer {
                    self.write(" = ");
                    self.expression(initializer);
                }
                self.write_raw(";");
            }
            ClassElement::MethodDeclaration(node) => {
                self.modifiers(node.modifiers);
                if node.asterisk_token.is_some() {
                    self.write("*");
                }
                self.property_name(&node.name);
                if let Some(token) = node.postfix_token {
                    self.token(token);
                }
                self.type_parameters(node.type_parameters);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.function_body(node.body.as_ref());
            }
            ClassElement::ConstructorDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("constructor");
                self.parameters(node.parameters);
                self.function_body(node.body.as_ref());
            }
            ClassElement::GetAccessorDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("get ");
                self.property_name(&node.name);
                self.parameters(node.parameters);
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.function_body(node.body.as_ref());
            }
            ClassElement::SetAccessorDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("set ");
                self.property_name(&node.name);
                self.parameters(node.parameters);
                self.function_body(node.body.as_ref());
            }
            ClassElement::IndexSignatureDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("[");
                for parameter in node.parameters {
                    if let Some(name) = &parameter.name {
                        self.binding_name(name);
                    }
                    if let Some(r#type) = &parameter.r#type {
                        self.write(": ");
                        self.type_node(r#type);
                    }
                }
                self.write("]");
                if let Some(r#type) = &node.r#type {
                    self.write(": ");
                    self.type_node(r#type);
                }
                self.write_raw(";");
            }
            ClassElement::ClassStaticBlockDeclaration(node) => {
                self.modifiers(node.modifiers);
                self.write("static ");
                if let Some(block) = node.body {
                    self.block(block);
                }
            }
            ClassElement::SemicolonClassElement(_) => self.write_raw(";"),
        }
    }

    fn function_body(&mut self, body: Option<&tsr_ast::FunctionBody<'_>>) {
        match body {
            Some(tsr_ast::FunctionBody::Block(block)) => {
                self.write(" ");
                self.block(block);
            }
            None => self.write_raw(";"),
        }
    }

    pub(crate) fn object_member(&mut self, member: &tsr_ast::ObjectLiteralElementLike<'_>) {
        use tsr_ast::ObjectLiteralElementLike as Member;
        match member {
            Member::PropertyAssignment(node) => {
                self.property_name(&node.name);
                self.write(": ");
                if let Some(initializer) = &node.initializer {
                    self.expression(initializer);
                }
            }
            Member::ShorthandPropertyAssignment(node) => {
                self.property_name(&node.name);
                if let Some(initializer) = &node.object_assignment_initializer {
                    self.write(" = ");
                    self.expression(initializer);
                }
            }
            Member::SpreadAssignment(node) => {
                self.write("...");
                if let Some(expression) = &node.expression {
                    self.expression(expression);
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
                self.class_element(&element);
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
    if flags.contains(tsr_ast::NodeFlags::USING) {
        "using"
    } else if flags.contains(tsr_ast::NodeFlags::CONST) {
        "const"
    } else if flags.contains(tsr_ast::NodeFlags::LET) {
        "let"
    } else {
        "var"
    }
}
