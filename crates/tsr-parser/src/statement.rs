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
            match self.parse_statement() {
                Some(statement) => statements.push(statement),
                None => {
                    // Nothing consumed: report once and skip a token, or the loop
                    // spins forever on unexpected input.
                    if self.pos() == before {
                        self.error_at_current(&messages::UNEXPECTED_TOKEN);
                        self.next_token();
                    }
                }
            }
            debug_assert!(
                self.pos() > before || self.at(terminator) || self.at(SyntaxKind::EndOfFile),
                "statement loop made no progress"
            );
        }
        statements
    }

    /// Parse one statement, or `None` if the cursor is not on one.
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse_statement(&mut self) -> Option<Statement<'a>> {
        let start = self.pos();

        match self.token.kind {
            SyntaxKind::SemicolonToken => {
                self.next_token();
                let node = self.finish_node(
                    EmptyStatement::new(),
                    SyntaxKind::EmptyStatement,
                    start,
                );
                Some(Statement::EmptyStatement(node))
            }
            SyntaxKind::OpenBraceToken => Some(Statement::Block(self.parse_block())),
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                // `let` and `const` are contextual: `let` alone is an identifier.
                if self.at(SyntaxKind::LetKeyword) && !self.next_starts_binding() {
                    return self.parse_expression_statement();
                }
                Some(self.parse_variable_statement(start, &[]))
            }
            SyntaxKind::FunctionKeyword => Some(self.parse_function_declaration(start, &[])),
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
            _ if self.at_modifier_starting_declaration() => {
                let modifiers = self.parse_modifiers();
                Some(self.parse_declaration_after_modifiers(start, modifiers))
            }
            _ => self.parse_expression_statement(),
        }
    }

    /// Whether the token after `let` can begin a binding.
    fn next_starts_binding(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::Identifier
                    | SyntaxKind::OpenBracketToken
                    | SyntaxKind::OpenBraceToken
            ) || is_contextual_keyword(kind)
        })
    }

    /// Look at the next token without committing.
    fn peek_kind(&mut self, predicate: impl Fn(SyntaxKind) -> bool) -> bool {
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

    /// Parse a run of modifiers.
    pub(crate) fn parse_modifiers(&mut self) -> Vec<ModifierLike<'a>> {
        let mut modifiers = Vec::new();
        while is_modifier(self.token.kind) {
            // A modifier keyword used as a name is not a modifier: `export
            // default` versus `const declare = 1`.
            let start = self.pos();
            let kind = self.token.kind;
            self.next_token();
            let token = self.alloc_token(kind, tsr_core::Span::new(start, self.pos()));
            modifiers.push(ModifierLike::Token(token));
        }
        modifiers
    }

    /// Dispatch to the declaration a modifier list precedes.
    fn parse_declaration_after_modifiers(
        &mut self,
        start: u32,
        modifiers: Vec<ModifierLike<'a>>,
    ) -> Statement<'a> {
        match self.token.kind {
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => {
                self.parse_variable_statement(start, &modifiers)
            }
            SyntaxKind::FunctionKeyword => self.parse_function_declaration(start, &modifiers),
            _ => {
                // The modifiers were a false start; treat what follows as an
                // expression so the tree still covers the text.
                self.parse_expression_statement().unwrap_or_else(|| {
                    let expression = self.missing_identifier();
                    let node = self.finish_node(
                        ExpressionStatement::new(Expression::Identifier(expression)),
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
            VariableStatement::new(modifiers, list),
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
        let exclamation = if self.at(SyntaxKind::ExclamationToken) {
            Some(self.take_token())
        } else {
            None
        };
        let type_node = self.parse_type_annotation();
        let initializer = if self.eat(SyntaxKind::EqualsToken) {
            Some(self.parse_assignment_expression())
        } else {
            None
        };
        self.finish_node(
            VariableDeclaration::new(name, exclamation, type_node, initializer),
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
        let else_branch =
            if self.eat(SyntaxKind::ElseKeyword) { Some(self.parse_statement_or_missing()) } else { None };
        let node = self.finish_node(
            IfStatement::new(condition, then_branch, else_branch),
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
        let node =
            self.finish_node(DoStatement::new(body, condition), SyntaxKind::DoStatement, start);
        Statement::DoStatement(node)
    }

    fn parse_while_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        self.expect(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);
        let body = self.parse_statement_or_missing();
        let node = self
            .finish_node(WhileStatement::new(condition, body), SyntaxKind::WhileStatement, start);
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
            let kind =
                if is_of { SyntaxKind::ForOfStatement } else { SyntaxKind::ForInStatement };
            let await_token = if is_await {
                Some(self.alloc_token(SyntaxKind::AwaitKeyword, tsr_core::Span::at(start)))
            } else {
                None
            };
            let initializer = match initializer {
                Some(initializer) => initializer,
                None => {
                    let missing = self.missing_identifier();
                    ForInitializer::from(Expression::Identifier(missing))
                }
            };
            let kind_token = self.alloc_token(kind, tsr_core::Span::at(start));
            let node = self.finish_node(
                ForInOrOfStatement::new(
                    Some(kind_token),
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
        let incrementor = if self.at(SyntaxKind::CloseParenToken) {
            None
        } else {
            Some(self.parse_expression())
        };
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
            let node = self
                .finish_node(ContinueStatement::new(label), SyntaxKind::ContinueStatement, start);
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
        let node = self
            .finish_node(WithStatement::new(expression, statement), SyntaxKind::WithStatement, start);
        Statement::WithStatement(node)
    }

    fn parse_throw_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        let expression = self.parse_expression();
        self.parse_semicolon();
        let node =
            self.finish_node(ThrowStatement::new(expression), SyntaxKind::ThrowStatement, start);
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
            if self.eat(SyntaxKind::CaseKeyword) {
                let test = self.parse_expression();
                self.expect(SyntaxKind::ColonToken);
                let statements = self.parse_clause_statements();
                let statements = self.arena.alloc_slice(&statements);
                let node = self.finish_node(
                    CaseClause::new(test, statements),
                    SyntaxKind::CaseClause,
                    clause_start,
                );
                clauses.push(CaseOrDefaultClause::CaseClause(node));
            } else if self.eat(SyntaxKind::DefaultKeyword) {
                self.expect(SyntaxKind::ColonToken);
                let statements = self.parse_clause_statements();
                let statements = self.arena.alloc_slice(&statements);
                let node = self.finish_node(
                    DefaultClause::new(statements),
                    SyntaxKind::DefaultClause,
                    clause_start,
                );
                clauses.push(CaseOrDefaultClause::DefaultClause(node));
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
            SwitchStatement::new(expression, case_block),
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
                    VariableDeclaration::new(name, None, type_node, None),
                    SyntaxKind::VariableDeclaration,
                    decl_start,
                ))
            } else {
                None
            };
            let body = self.parse_block();
            Some(self.finish_node(
                CatchClause::new(variable, body),
                SyntaxKind::CatchClause,
                catch_start,
            ))
        } else {
            None
        };

        let finally = if self.eat(SyntaxKind::FinallyKeyword) { Some(self.parse_block()) } else { None };

        if catch.is_none() && finally.is_none() {
            self.error_at_current(&messages::CATCH_OR_FINALLY_EXPECTED);
        }

        let node = self.finish_node(
            TryStatement::new(block, catch, finally),
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
                    LabeledStatement::new(label, statement),
                    SyntaxKind::LabeledStatement,
                    start,
                );
                return Some(Statement::LabeledStatement(node));
            }
        }

        self.parse_semicolon();
        let node = self.finish_node(
            ExpressionStatement::new(expression),
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
            let node =
                self.finish_node(EmptyStatement::new(), SyntaxKind::EmptyStatement, start);
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
        let asterisk = if self.at(SyntaxKind::AsteriskToken) { Some(self.take_token()) } else { None };
        let name = if self.at(SyntaxKind::OpenParenToken) || self.at(SyntaxKind::LessThanToken) {
            None
        } else {
            Some(self.parse_identifier())
        };
        let type_parameters = self.parse_type_parameters();
        let parameters = self.parse_parameter_list();
        let return_type = self.parse_type_annotation();
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

