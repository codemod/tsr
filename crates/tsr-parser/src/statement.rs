//! Statement and declaration parsing.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse statements until `terminator` or end of file.
    ///
    /// The loop is the parser's main recovery point: if a statement makes no
    /// progress, the offending token is discarded so the file cannot hang.
    pub(crate) fn parse_statement_list(&mut self, terminator: SyntaxKind) -> Vec<Statement<'a>> {
        let mut statements = Vec::new();
        while !self.at(terminator) && !self.at(SyntaxKind::EndOfFile) {
            let before = self.pos();
            let parsed = self.parse_statement();

            // Termination is enforced here rather than assumed. A statement parser
            // can legitimately produce a node while consuming nothing — an
            // expression statement whose expression was synthesised from a token
            // it could not use — and that node covers no text, so keeping it would
            // add a zero-width statement *and* spin the loop forever.
            if self.pos() == before {
                if parsed.is_none() {
                    self.error_at_current(&messages::UNEXPECTED_TOKEN);
                }
                self.next_token();
                continue;
            }

            if let Some(statement) = parsed {
                statements.push(statement);
            }
        }
        statements
    }

    /// Parse one statement, or `None` if the cursor is not on one.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse_statement(&mut self) -> Option<Statement<'a>> {
        let start = self.pos();

        // Bound before matching: the guards below need `&mut self` for lookahead,
        // which a match on `self.token.kind` directly would forbid.
        let kind = self.token.kind;
        match kind {
            SyntaxKind::SemicolonToken => {
                self.next_token();
                let node =
                    self.finish_node(EmptyStatement::new(), SyntaxKind::EmptyStatement, start);
                Some(Statement::EmptyStatement(node))
            }
            SyntaxKind::OpenBraceToken => Some(Statement::Block(self.parse_block())),
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                // `const enum` is an enum declaration, not a variable named `enum`.
                if self.at(SyntaxKind::ConstKeyword) && self.next_is_enum() {
                    let modifiers = self.parse_modifiers();
                    return Some(self.parse_enum_declaration(start, &modifiers));
                }
                // `let` is contextual: `let` alone is an identifier.
                if self.at(SyntaxKind::LetKeyword) && !self.next_starts_binding() {
                    return self.parse_expression_statement();
                }
                Some(self.parse_variable_statement(start, &[]))
            }
            SyntaxKind::FunctionKeyword => Some(self.parse_function_declaration(start, &[])),
            SyntaxKind::ClassKeyword => Some(self.parse_class_declaration(start, &[])),
            SyntaxKind::ImportKeyword if self.import_starts_declaration() => {
                Some(self.parse_import_declaration(start, &[]))
            }
            SyntaxKind::ExportKeyword => {
                self.next_token();
                Some(self.parse_export(start, &[]))
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword
                if self.next_starts_module_name() =>
            {
                Some(self.parse_module_declaration(start, &[]))
            }
            SyntaxKind::EnumKeyword => Some(self.parse_enum_declaration(start, &[])),
            SyntaxKind::InterfaceKeyword if self.next_is_identifier() => {
                Some(self.parse_interface_declaration(start, &[]))
            }
            SyntaxKind::TypeKeyword if self.next_is_identifier() => {
                Some(self.parse_type_alias_declaration(start, &[]))
            }
            SyntaxKind::IfKeyword => Some(self.parse_if_statement()),
            SyntaxKind::DoKeyword => Some(self.parse_do_statement()),
            SyntaxKind::WhileKeyword => Some(self.parse_while_statement()),
            SyntaxKind::ForKeyword => Some(self.parse_for_statement()),
            SyntaxKind::ContinueKeyword | SyntaxKind::BreakKeyword => {
                Some(self.parse_break_or_continue())
            }
            SyntaxKind::ReturnKeyword => Some(self.parse_return_statement()),
            SyntaxKind::WithKeyword => Some(self.parse_with_statement()),
            SyntaxKind::SwitchKeyword => Some(self.parse_switch_statement()),
            SyntaxKind::ThrowKeyword => Some(self.parse_throw_statement()),
            SyntaxKind::TryKeyword => Some(self.parse_try_statement()),
            SyntaxKind::DebuggerKeyword => {
                self.next_token();
                self.parse_semicolon();
                let node = self.finish_node(
                    DebuggerStatement::new(),
                    SyntaxKind::DebuggerStatement,
                    start,
                );
                Some(Statement::DebuggerStatement(node))
            }
            SyntaxKind::AtToken => {
                let modifiers = self.parse_modifiers();
                Some(self.parse_declaration_after_modifiers(start, &modifiers))
            }
            _ if self.at_modifier_starting_declaration() => {
                let modifiers = self.parse_modifiers();
                Some(self.parse_declaration_after_modifiers(start, &modifiers))
            }
            _ => self.parse_expression_statement(),
        }
    }

    /// Whether the token after `let` can begin a binding.
    fn next_starts_binding(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::Identifier | SyntaxKind::OpenBracketToken | SyntaxKind::OpenBraceToken
            ) || is_contextual_keyword(kind)
        })
    }

    /// Look at the next token without committing.
    pub(crate) fn peek_kind(&mut self, predicate: impl Fn(SyntaxKind) -> bool) -> bool {
        let mut matched = false;
        // `try_parse` always rewinds when the closure returns `None`, which makes
        // it a lookahead rather than a parse.
        self.try_parse(|p| {
            p.next_token();
            matched = predicate(p.token.kind);
            None::<()>
        });
        matched
    }

    /// Whether `namespace`/`module` is followed by a name rather than used as one.
    ///
    /// The name may itself be a contextual keyword: `namespace require { … }` is
    /// legal, and rejecting it leaves the whole namespace body unparsed.
    fn next_starts_module_name(&mut self) -> bool {
        self.peek_kind(|kind| {
            kind == SyntaxKind::Identifier
                || kind == SyntaxKind::StringLiteral
                || is_contextual_keyword(kind)
        })
    }

    /// Whether the next token is an identifier, for contextual keywords.
    fn next_is_identifier(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::Identifier || is_contextual_keyword(kind))
    }

    /// Whether the cursor is on a modifier that introduces a declaration.
    fn at_modifier_starting_declaration(&self) -> bool {
        matches!(
            self.token.kind,
            SyntaxKind::DeclareKeyword
                | SyntaxKind::AbstractKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::ReadonlyKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::StaticKeyword
        )
    }

    /// Parse a run of modifiers, including decorators.
    ///
    /// Decorators are `ModifierLike` in the AST, and may interleave with keyword
    /// modifiers: `@dec public readonly x` and `public @dec x` are both accepted
    /// by the grammar.
    pub(crate) fn parse_modifiers(&mut self) -> Vec<ModifierLike<'a>> {
        let mut modifiers = Vec::new();
        loop {
            if self.at(SyntaxKind::AtToken) {
                modifiers.push(ModifierLike::Decorator(self.parse_decorator()));
                continue;
            }
            let kind = self.token.kind;
            if !is_modifier(kind) {
                break;
            }
            // `const` is a modifier only in `const enum`. Everywhere else it opens
            // a variable declaration, and consuming it here would leave
            // `export const a = 1` looking like a bare expression.
            if kind == SyntaxKind::ConstKeyword && !self.next_is_enum() {
                break;
            }
            // A modifier keyword can also be a member *name*:
            // `interface I { abstract(): void }` declares a method called
            // `abstract`. What follows decides.
            if self.modifier_is_actually_a_name() {
                break;
            }
            let start = self.pos();
            self.next_token();
            let token = self.alloc_token(kind, tsr_core::Span::new(start, self.pos()));
            modifiers.push(ModifierLike::Token(token));
        }
        modifiers
    }

    /// `@expr`, where `expr` is a call or member chain.
    fn parse_decorator(&mut self) -> &'a Decorator<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::AtToken);
        let expression = self.parse_decorator_expression();
        self.finish_node(Decorator::new(Some(expression)), SyntaxKind::Decorator, start)
    }

    /// Whether the modifier at the cursor is really a member name.
    ///
    /// A name is followed by something that continues a member — `(`, `:`, `?`,
    /// `=`, `;`, `,`, `}`, or a line break ending the member — whereas a genuine
    /// modifier is followed by another modifier or the thing it modifies.
    fn modifier_is_actually_a_name(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::OpenParenToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::EqualsToken
                    | SyntaxKind::SemicolonToken
                    | SyntaxKind::CommaToken
                    | SyntaxKind::CloseBraceToken
                    | SyntaxKind::CloseParenToken
                    | SyntaxKind::LessThanToken
            )
        })
    }

    /// Whether the token after `const` is `enum`.
    fn next_is_enum(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::EnumKeyword)
    }

    /// Dispatch to the declaration a modifier list precedes.
    pub(crate) fn parse_declaration_after_modifiers(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        match self.token.kind {
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                self.parse_variable_statement(start, modifiers)
            }
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(start, modifiers),
            SyntaxKind::ClassKeyword => self.parse_class_declaration(start, modifiers),
            SyntaxKind::InterfaceKeyword => self.parse_interface_declaration(start, modifiers),
            SyntaxKind::TypeKeyword => self.parse_type_alias_declaration(start, modifiers),
            SyntaxKind::EnumKeyword => self.parse_enum_declaration(start, modifiers),
            SyntaxKind::ImportKeyword => self.parse_import_declaration(start, modifiers),
            // `declare global { … }` augments the global scope; `global` is a
            // contextual keyword standing in for the module name.
            SyntaxKind::NamespaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::GlobalKeyword => self.parse_module_declaration(start, modifiers),
            _ => {
                // The modifiers were a false start; treat what follows as an
                // expression so the tree still covers the text.
                self.parse_expression_statement().unwrap_or_else(|| {
                    let expression = self.missing_identifier();
                    let node = self.finish_node(
                        ExpressionStatement::new(Some(Expression::Identifier(expression))),
                        SyntaxKind::ExpressionStatement,
                        start,
                    );
                    Statement::ExpressionStatement(node)
                })
            }
        }
    }

    // ---- individual statements ------------------------------------------

    pub(crate) fn parse_block(&mut self) -> &'a Block<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let statements = self.parse_statement_list(SyntaxKind::CloseBraceToken);
        self.expect(SyntaxKind::CloseBraceToken);
        let statements = self.arena.alloc_slice(&statements);
        self.finish_node(Block::new(statements, true), SyntaxKind::Block, start)
    }

    fn parse_variable_statement(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        let list = self.parse_variable_declaration_list();
        self.parse_semicolon();
        let modifiers = self.arena.alloc_slice(modifiers);
        let node = self.finish_node(
            VariableStatement::new(modifiers, Some(list)),
            SyntaxKind::VariableStatement,
            start,
        );
        Statement::VariableStatement(node)
    }

    pub(crate) fn parse_variable_declaration_list(&mut self) -> &'a VariableDeclarationList<'a> {
        let start = self.pos();
        // `var`, `let`, or `const`; the flag distinguishing them lives on the node
        // flags upstream, which the binder sets.
        self.next_token();

        let mut declarations = Vec::new();
        loop {
            declarations.push(self.parse_variable_declaration());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
        }
        let declarations = self.arena.alloc_slice(&declarations);
        self.finish_node(
            VariableDeclarationList::new(declarations),
            SyntaxKind::VariableDeclarationList,
            start,
        )
    }

    fn parse_variable_declaration(&mut self) -> &'a VariableDeclaration<'a> {
        let start = self.pos();
        let name = self.parse_binding_name();
        let exclamation =
            if self.at(SyntaxKind::ExclamationToken) { Some(self.take_token()) } else { None };
        let type_node = self.parse_type_annotation();
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        self.finish_node(
            VariableDeclaration::new(Some(name), exclamation, type_node, initializer),
            SyntaxKind::VariableDeclaration,
            start,
        )
    }

    fn parse_if_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        self.expect(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);
        let then_branch = self.parse_statement_or_missing();
        let else_branch = if self.eat(SyntaxKind::ElseKeyword) {
            Some(self.parse_statement_or_missing())
        } else {
            None
        };
        let node = self.finish_node(
            IfStatement::new(Some(condition), Some(then_branch), else_branch),
            SyntaxKind::IfStatement,
            start,
        );
        Statement::IfStatement(node)
    }

    fn parse_do_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        let body = self.parse_statement_or_missing();
        self.expect(SyntaxKind::WhileKeyword);
        self.expect(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);
        // A `;` after `do…while(…)` is optional even without ASI.
        self.eat(SyntaxKind::SemicolonToken);
        let node = self.finish_node(
            DoStatement::new(body, Some(condition)),
            SyntaxKind::DoStatement,
            start,
        );
        Statement::DoStatement(node)
    }

    fn parse_while_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        self.expect(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);
        let body = self.parse_statement_or_missing();
        let node = self.finish_node(
            WhileStatement::new(Some(condition), body),
            SyntaxKind::WhileStatement,
            start,
        );
        Statement::WhileStatement(node)
    }

    /// Parse `for`, `for…in`, and `for…of`, which share a prefix.
    fn parse_for_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        let is_await = self.eat(SyntaxKind::AwaitKeyword);
        self.expect(SyntaxKind::OpenParenToken);

        let initializer: Option<ForInitializer<'a>> = if self.at(SyntaxKind::SemicolonToken) {
            None
        } else if matches!(
            self.token.kind,
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword
        ) {
            Some(ForInitializer::VariableDeclarationList(self.parse_variable_declaration_list()))
        } else {
            // `in` is banned here so `for (x in y)` is not read as a comparison.
            Some(ForInitializer::from(self.parse_expression_no_in()))
        };

        if self.at(SyntaxKind::InKeyword) || self.at(SyntaxKind::OfKeyword) {
            let is_of = self.at(SyntaxKind::OfKeyword);
            self.next_token();
            let expression =
                if is_of { self.parse_assignment_expression() } else { self.parse_expression() };
            self.expect(SyntaxKind::CloseParenToken);
            let body = self.parse_statement_or_missing();
            let kind = if is_of { SyntaxKind::ForOfStatement } else { SyntaxKind::ForInStatement };
            let await_token = if is_await {
                Some(self.alloc_token(SyntaxKind::AwaitKeyword, tsr_core::Span::at(start)))
            } else {
                None
            };
            let initializer = initializer.unwrap_or_else(|| {
                let missing = self.missing_identifier();
                ForInitializer::from(Expression::Identifier(missing))
            });
            let kind_token = self.alloc_token(kind, tsr_core::Span::at(start));
            let node = self.finish_node(
                ForInOrOfStatement::new(
                    kind_token,
                    await_token,
                    Some(initializer),
                    Some(expression),
                    Some(body),
                ),
                kind,
                start,
            );
            return Statement::ForInOrOfStatement(node);
        }

        self.expect(SyntaxKind::SemicolonToken);
        let condition =
            if self.at(SyntaxKind::SemicolonToken) { None } else { Some(self.parse_expression()) };
        self.expect(SyntaxKind::SemicolonToken);
        let incrementor =
            if self.at(SyntaxKind::CloseParenToken) { None } else { Some(self.parse_expression()) };
        self.expect(SyntaxKind::CloseParenToken);
        let body = self.parse_statement_or_missing();
        let node = self.finish_node(
            ForStatement::new(initializer, condition, incrementor, body),
            SyntaxKind::ForStatement,
            start,
        );
        Statement::ForStatement(node)
    }

    fn parse_break_or_continue(&mut self) -> Statement<'a> {
        let start = self.pos();
        let is_break = self.at(SyntaxKind::BreakKeyword);
        self.next_token();
        // A label must be on the same line; a newline ends the statement.
        let label = if self.can_parse_semicolon() || self.token.has_preceding_line_break() {
            None
        } else {
            Some(self.parse_identifier())
        };
        self.parse_semicolon();
        if is_break {
            let node =
                self.finish_node(BreakStatement::new(label), SyntaxKind::BreakStatement, start);
            Statement::BreakStatement(node)
        } else {
            let node = self.finish_node(
                ContinueStatement::new(label),
                SyntaxKind::ContinueStatement,
                start,
            );
            Statement::ContinueStatement(node)
        }
    }

    fn parse_return_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        let expression = if self.can_parse_semicolon() || self.token.has_preceding_line_break() {
            None
        } else {
            Some(self.parse_expression())
        };
        self.parse_semicolon();
        let node =
            self.finish_node(ReturnStatement::new(expression), SyntaxKind::ReturnStatement, start);
        Statement::ReturnStatement(node)
    }

    fn parse_with_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        self.expect(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);
        let statement = self.parse_statement_or_missing();
        let node = self.finish_node(
            WithStatement::new(Some(expression), Some(statement)),
            SyntaxKind::WithStatement,
            start,
        );
        Statement::WithStatement(node)
    }

    fn parse_throw_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        let expression = self.parse_expression();
        self.parse_semicolon();
        let node = self.finish_node(
            ThrowStatement::new(Some(expression)),
            SyntaxKind::ThrowStatement,
            start,
        );
        Statement::ThrowStatement(node)
    }

    fn parse_switch_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        self.expect(SyntaxKind::OpenParenToken);
        let expression = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);

        let block_start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let mut clauses = Vec::new();
        while !self.at(SyntaxKind::CloseBraceToken) && !self.at(SyntaxKind::EndOfFile) {
            let clause_start = self.pos();
            if self.at(SyntaxKind::CaseKeyword) {
                let kind_token = self.take_token();
                let test = self.parse_expression();
                self.expect(SyntaxKind::ColonToken);
                let statements = self.parse_clause_statements();
                let statements = self.arena.alloc_slice(&statements);
                clauses.push(self.finish_node(
                    CaseOrDefaultClause::new(kind_token, Some(test), statements),
                    SyntaxKind::CaseClause,
                    clause_start,
                ));
            } else if self.at(SyntaxKind::DefaultKeyword) {
                let kind_token = self.take_token();
                self.expect(SyntaxKind::ColonToken);
                let statements = self.parse_clause_statements();
                let statements = self.arena.alloc_slice(&statements);
                clauses.push(self.finish_node(
                    CaseOrDefaultClause::new(kind_token, None, statements),
                    SyntaxKind::DefaultClause,
                    clause_start,
                ));
            } else {
                self.error_at_current(&messages::UNEXPECTED_TOKEN);
                self.next_token();
            }
        }
        self.expect(SyntaxKind::CloseBraceToken);
        let clauses = self.arena.alloc_slice(&clauses);
        let case_block =
            self.finish_node(CaseBlock::new(clauses), SyntaxKind::CaseBlock, block_start);
        let node = self.finish_node(
            SwitchStatement::new(Some(expression), Some(case_block)),
            SyntaxKind::SwitchStatement,
            start,
        );
        Statement::SwitchStatement(node)
    }

    /// Statements inside a `case`/`default`, up to the next clause.
    fn parse_clause_statements(&mut self) -> Vec<Statement<'a>> {
        let mut statements = Vec::new();
        while !matches!(
            self.token.kind,
            SyntaxKind::CaseKeyword
                | SyntaxKind::DefaultKeyword
                | SyntaxKind::CloseBraceToken
                | SyntaxKind::EndOfFile
        ) {
            let before = self.pos();
            match self.parse_statement() {
                Some(statement) => statements.push(statement),
                None if self.pos() == before => {
                    self.error_at_current(&messages::UNEXPECTED_TOKEN);
                    self.next_token();
                }
                None => {}
            }
        }
        statements
    }

    fn parse_try_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        let block = self.parse_block();

        let catch = if self.at(SyntaxKind::CatchKeyword) {
            let catch_start = self.pos();
            self.next_token();
            // `catch {}` without a binding is legal since ES2019.
            let variable = if self.eat(SyntaxKind::OpenParenToken) {
                let decl_start = self.pos();
                let name = self.parse_binding_name();
                let type_node = self.parse_type_annotation();
                self.expect(SyntaxKind::CloseParenToken);
                Some(self.finish_node(
                    VariableDeclaration::new(Some(name), None, type_node, None),
                    SyntaxKind::VariableDeclaration,
                    decl_start,
                ))
            } else {
                None
            };
            let body = self.parse_block();
            Some(self.finish_node(
                CatchClause::new(variable, Some(body)),
                SyntaxKind::CatchClause,
                catch_start,
            ))
        } else {
            None
        };

        let finally =
            if self.eat(SyntaxKind::FinallyKeyword) { Some(self.parse_block()) } else { None };

        if catch.is_none() && finally.is_none() {
            self.error_at_current(&messages::CATCH_OR_FINALLY_EXPECTED);
        }

        let node = self.finish_node(
            TryStatement::new(Some(block), catch, finally),
            SyntaxKind::TryStatement,
            start,
        );
        Statement::TryStatement(node)
    }

    fn parse_expression_statement(&mut self) -> Option<Statement<'a>> {
        let start = self.pos();
        if !self.at_expression_start() {
            return None;
        }
        let expression = self.parse_expression();

        // `label:` looks like an expression statement until the colon.
        if let Expression::Identifier(label) = expression {
            if self.eat(SyntaxKind::ColonToken) {
                let statement = self.parse_statement_or_missing();
                let node = self.finish_node(
                    LabeledStatement::new(Some(label), Some(statement)),
                    SyntaxKind::LabeledStatement,
                    start,
                );
                return Some(Statement::LabeledStatement(node));
            }
        }

        self.parse_semicolon();
        let node = self.finish_node(
            ExpressionStatement::new(Some(expression)),
            SyntaxKind::ExpressionStatement,
            start,
        );
        Some(Statement::ExpressionStatement(node))
    }

    /// A statement, or a synthesised empty one so the tree stays complete.
    fn parse_statement_or_missing(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.parse_statement().unwrap_or_else(|| {
            self.error_at_current(&messages::STATEMENT_EXPECTED);
            let node = self.finish_node(EmptyStatement::new(), SyntaxKind::EmptyStatement, start);
            Statement::EmptyStatement(node)
        })
    }

    /// Parse `function f<T>(a: T): R { … }`.
    fn parse_function_declaration(
        &mut self,
        start: u32,
        modifiers: &[ModifierLike<'a>],
    ) -> Statement<'a> {
        self.expect(SyntaxKind::FunctionKeyword);
        let asterisk =
            if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        let name = if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            None
        } else {
            Some(self.parse_identifier())
        };
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameter_list();
        let return_type = self.parse_return_type_annotation();
        // An overload signature has no body, just a semicolon.
        let body = if self.at(SyntaxKind::OpenBraceToken) {
            Some(FunctionBody::Block(self.parse_block()))
        } else {
            self.parse_semicolon();
            None
        };

        let modifiers = self.arena.alloc_slice(modifiers);
        let type_parameters = self.arena.alloc_slice(&type_parameters);
        let parameters = self.arena.alloc_slice(&parameters);
        let node = self.finish_node(
            FunctionDeclaration::new(
                modifiers,
                asterisk,
                name,
                type_parameters,
                parameters,
                return_type,
                None,
                body,
            ),
            SyntaxKind::FunctionDeclaration,
            start,
        );
        Statement::FunctionDeclaration(node)
    }

    // ---- semicolons ------------------------------------------------------

    /// Whether a semicolon may be inserted here.
    ///
    /// Automatic semicolon insertion: a `;` is implied before `}`, at end of file,
    /// or across a line break.
    pub(crate) fn can_parse_semicolon(&self) -> bool {
        self.at(SyntaxKind::SemicolonToken)
            || self.at(SyntaxKind::CloseBraceToken)
            || self.at(SyntaxKind::EndOfFile)
            || self.token.has_preceding_line_break()
    }

    /// Consume a semicolon, real or inserted.
    pub(crate) fn parse_semicolon(&mut self) {
        if self.eat(SyntaxKind::SemicolonToken) {
            return;
        }
        if !self.can_parse_semicolon() {
            self.error_at_current_with(&messages::_0_EXPECTED, &[";"]);
        }
    }
}

/// Whether `kind` is a modifier keyword.
fn is_modifier(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::AbstractKeyword
            | SyntaxKind::AccessorKeyword
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::DefaultKeyword
            | SyntaxKind::ExportKeyword
            | SyntaxKind::InKeyword
            | SyntaxKind::OutKeyword
            | SyntaxKind::OverrideKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::StaticKeyword
    )
}

/// Whether `kind` is a keyword that may also be an ordinary identifier.
pub(crate) fn is_contextual_keyword(kind: SyntaxKind) -> bool {
    kind.is_keyword() && !is_reserved_word(kind)
}

/// Whether `kind` is a reserved word that can never name a binding.
pub(crate) fn is_reserved_word(kind: SyntaxKind) -> bool {
    (SyntaxKind::FIRST_RESERVED_WORD as u16..=SyntaxKind::LAST_RESERVED_WORD as u16)
        .contains(&(kind as u16))
}
