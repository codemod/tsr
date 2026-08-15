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
        let docs = self.parse_leading_jsdoc();
        let statement = self.parse_statement_worker();
        if let Some(statement) = statement {
            self.attach_jsdoc(statement.into(), docs);
        }
        statement
    }

    fn parse_statement_worker(&mut self) -> Option<Statement<'a>> {
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
            // `using x = r;` and `await using x = r;` — explicit resource
            // management. `using` is contextual, so a binding name must follow.
            SyntaxKind::UsingKeyword if self.next_starts_binding() => {
                Some(self.parse_variable_statement(start, &[]))
            }
            SyntaxKind::AwaitKeyword if self.next_starts_using_declaration() => {
                self.next_token();
                Some(self.parse_variable_statement(start, &[]))
            }
            SyntaxKind::FunctionKeyword => Some(self.parse_function_declaration(start, &[])),
            SyntaxKind::ClassKeyword => Some(self.parse_class_declaration(start, &[])),
            SyntaxKind::ImportKeyword if self.import_starts_declaration() => {
                Some(self.parse_import_declaration(start, &[]))
            }
            SyntaxKind::ExportKeyword => {
                // Keep the `export` token rather than discarding it. It is a
                // modifier on whatever declaration follows, and the binder reads
                // it to route a namespace member into the namespace's exports
                // instead of its locals — without it, `namespace M { export const
                // X = 1 }` declares `X` rather than `M.X`.
                let modifier_start = self.pos();
                self.next_token();
                let token = self.alloc_token(
                    SyntaxKind::ExportKeyword,
                    tsr_core::Span::new(modifier_start, self.pos()),
                );
                Some(self.parse_export(start, token, &[]))
            }
            SyntaxKind::NamespaceKeyword | SyntaxKind::ModuleKeyword
                if self.next_starts_module_name() =>
            {
                Some(self.parse_module_declaration(start, &[]))
            }
            // `global { … }` augments the global scope from inside a module body.
            SyntaxKind::GlobalKeyword if self.next_is_open_brace_token() => {
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
                let modifiers = self.arena.alloc_slice(&modifiers);
                Some(self.parse_declaration_after_modifiers(start, modifiers))
            }
            _ if self.at_modifier_starting_declaration() && self.next_starts_declaration() => {
                let modifiers = self.parse_modifiers();
                let modifiers = self.arena.alloc_slice(&modifiers);
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
                SyntaxKind::Identifier | SyntaxKind::OpenBracketToken | SyntaxKind::OpenBraceToken
            ) || is_contextual_keyword(kind)
        })
    }

    /// Look at the next token without committing.
    /// Look at the next token through a predicate that may read more than its
    /// kind — the preceding-line-break flag, in particular.
    pub(crate) fn peek_token(&mut self, predicate: impl Fn(&Self) -> bool) -> bool {
        let mut matched = false;
        self.try_parse(|p| {
            p.next_token();
            matched = predicate(p);
            None::<()>
        });
        matched
    }

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
    ///
    /// §361: **on the same line** —
    /// `nextTokenIsIdentifierOrStringLiteralOnSameLine` (parser.go:6103).
    /// These keywords are legal identifiers, and an identifier cannot be
    /// followed by another identifier across a line break: ASI makes
    /// `module⏎"my external module"⏎x = 1;` an expression statement naming
    /// `module` followed by a string statement
    /// (`asiPreventsParsingAsAmbientExternalModule01`).
    fn next_starts_module_name(&mut self) -> bool {
        self.peek_token(|p| {
            !p.token.has_preceding_line_break()
                && (p.token.kind == SyntaxKind::Identifier
                    || p.token.kind == SyntaxKind::StringLiteral
                    || is_contextual_keyword(p.token.kind))
        })
    }

    /// Whether the next token is an identifier **on the same line**, for the
    /// contextual keywords `interface` and `type`.
    ///
    /// §361: upstream's `nextTokenIsIdentifierOnSameLine` (parser.go:6101),
    /// the arm scanStartOfDeclaration documents with `namespace⏎n`: a line
    /// break after the keyword makes it a plain identifier expression
    /// (`asiPreventsParsingAsInterface01`).
    fn next_is_identifier(&mut self) -> bool {
        self.peek_token(|p| {
            !p.token.has_preceding_line_break()
                && (p.token.kind == SyntaxKind::Identifier || is_contextual_keyword(p.token.kind))
        })
    }

    /// Whether `await` here begins an `await using` declaration.
    fn next_starts_using_declaration(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::UsingKeyword)
    }

    /// Whether the cursor opens a variable declaration list.
    fn at_variable_declaration_list(&mut self) -> bool {
        match self.token.kind {
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => true,
            SyntaxKind::UsingKeyword => self.next_starts_binding(),
            SyntaxKind::AwaitKeyword => self.next_starts_using_declaration(),
            _ => false,
        }
    }

    /// Whether `global` here opens an augmentation block.
    fn next_is_open_brace_token(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::OpenBraceToken)
    }

    /// Whether what follows a modifier keyword actually begins a declaration.
    ///
    /// `declare` and friends are contextual: `var declare: any; declare
    /// instanceof C;` uses one as a plain identifier, and treating it as a
    /// modifier there swallows the expression statement.
    fn next_starts_declaration(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::VarKeyword
                    | SyntaxKind::LetKeyword
                    | SyntaxKind::ConstKeyword
                    | SyntaxKind::FunctionKeyword
                    | SyntaxKind::ClassKeyword
                    | SyntaxKind::InterfaceKeyword
                    | SyntaxKind::TypeKeyword
                    | SyntaxKind::EnumKeyword
                    | SyntaxKind::ImportKeyword
                    | SyntaxKind::ExportKeyword
                    | SyntaxKind::NamespaceKeyword
                    | SyntaxKind::ModuleKeyword
                    | SyntaxKind::GlobalKeyword
                    | SyntaxKind::AtToken
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::DeclareKeyword
                    | SyntaxKind::ReadonlyKeyword
                    | SyntaxKind::StaticKeyword
                    | SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::AccessorKeyword
                    | SyntaxKind::OverrideKeyword
                    | SyntaxKind::DefaultKeyword
            )
        })
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
        self.parse_modifiers_ex(false, false)
    }

    /// `parseModifiersEx`'s `permitConstAsModifier` (`parser.go:3859`).
    ///
    /// `const` is a modifier in two places upstream and a declaration keyword
    /// everywhere else: on a **class member** (`parser.go:1854`) and on a **type
    /// parameter** (`:3230`). Type parameters already handle it in
    /// `types.rs`; this is the class-member half.
    ///
    /// It matters for error recovery rather than for valid code. `static const H
    /// = 1` is not legal TypeScript, and what upstream does with it is parse
    /// `const` as an erroneous modifier and `H` as the member's name. Without
    /// this, our parser ends the modifier run at `static`, takes `const` as the
    /// member *name*, and then reads `H = 1` as a second member — two
    /// declarations where upstream has one, with different names.
    /// `stop_on_start_of_class_static_block` is upstream's third parameter
    /// (`parseModifiersEx`, `parser.go:4023`), passed `true` only by
    /// `parseClassElement`. Without it `async static { }` eats `static` as a
    /// modifier and leaves the `{` to be read as an object literal, so
    /// `conformance/classStaticBlock20` — three static blocks, each written
    /// with an illegal modifier — records one assertion upstream and three
    /// here. §211.
    pub(crate) fn parse_modifiers_ex(
        &mut self,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Vec<ModifierLike<'a>> {
        let mut modifiers = Vec::new();
        let mut seen_static = false;
        loop {
            if self.at(SyntaxKind::AtToken) {
                modifiers.push(ModifierLike::Decorator(self.parse_decorator()));
                continue;
            }
            let kind = self.token.kind;
            if !is_modifier(kind) {
                break;
            }
            // `static {` is the head of a class static BLOCK, never a modifier
            // run — upstream stops here so `parseClassElement` can see the
            // `static` itself (`parser.go:4023`, `:2500`).
            if stop_on_start_of_class_static_block
                && kind == SyntaxKind::StaticKeyword
                && self.next_is_open_brace()
            {
                break;
            }
            // `const` opens a variable declaration in most positions, and
            // consuming it here would leave `export const a = 1` looking like a
            // bare expression. It is a modifier in exactly two cases: before
            // `enum`, and — where the caller permits it — on a class member,
            // which is `nextTokenIsOnSameLineAndCanFollowModifier`
            // (`parser.go:4040`). Upstream's comment on the line-break test:
            // *"so that when 'const' is a standalone declaration, we don't issue
            // an error"*.
            if kind == SyntaxKind::ConstKeyword {
                let is_modifier_here = if permit_const_as_modifier {
                    self.look_ahead(|p| {
                        p.next_token();
                        !p.token.has_preceding_line_break() && can_follow_modifier(p.token.kind)
                    })
                } else {
                    self.next_is_enum()
                };
                if !is_modifier_here {
                    break;
                }
            }
            // A modifier keyword can also be a member *name*:
            // `interface I { abstract(): void }` declares a method called
            // `abstract`. What follows decides.
            if self.modifier_is_actually_a_name(seen_static) {
                break;
            }
            seen_static |= kind == SyntaxKind::StaticKeyword;
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
    /// Upstream's `tryParseModifier`/`nextTokenCanFollowModifier`, and the shape
    /// of the test matters: it is a **whitelist** of what may follow a modifier,
    /// not a blacklist of what may not. `class C { static static }` declares a
    /// static member called `static`, and no list of "things a name is followed
    /// by" reaches that — the second `static` is followed by `}` in one case and
    /// by `[x: string]: string` in another.
    ///
    /// Two rules, both upstream's:
    ///
    /// - **A second `static` is never a modifier** (`hasSeenStaticModifier`),
    ///   which is what makes `static static` a name.
    /// - Everything else must be followed, **on the same line**, by something
    ///   that can follow a modifier. `static` itself is exempt from the same-line
    ///   part, because `static` on its own line still modifies what comes next.
    fn modifier_is_actually_a_name(&mut self, seen_static: bool) -> bool {
        let kind = self.token.kind;
        if seen_static && kind == SyntaxKind::StaticKeyword {
            return true;
        }
        !self.peek_token(|p| match kind {
            // `export` may be followed by a decorator — `@dec export @dec class`
            // — and not by the tokens that make it an export *declaration*
            // rather than a modifier (`canFollowExportModifier`).
            SyntaxKind::ExportKeyword => {
                p.token.kind == SyntaxKind::AtToken
                    || !matches!(
                        p.token.kind,
                        SyntaxKind::AsteriskToken
                            | SyntaxKind::AsKeyword
                            | SyntaxKind::OpenBraceToken
                    ) && can_follow_modifier(p.token.kind)
            }
            // `export default` is followed by the declaration it exports
            // (`nextTokenCanFollowDefaultKeyword`).
            SyntaxKind::DefaultKeyword => matches!(
                p.token.kind,
                SyntaxKind::ClassKeyword
                    | SyntaxKind::FunctionKeyword
                    | SyntaxKind::InterfaceKeyword
                    | SyntaxKind::AbstractKeyword
                    | SyntaxKind::AsyncKeyword
                    | SyntaxKind::AtToken
            ),
            // `static` alone is exempt from the same-line rule: `static` on its
            // own line still modifies what comes next.
            SyntaxKind::StaticKeyword => can_follow_modifier(p.token.kind),
            _ => {
                !p.token.flags.contains(tsr_scanner::TokenFlags::PRECEDING_LINE_BREAK)
                    && can_follow_modifier(p.token.kind)
            }
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
        modifiers: &'a [ModifierLike<'a>],
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
            // Recovery for misplaced modifiers before a second `export`, such
            // as `declare export = value` and `export declare export = value`.
            // Upstream still builds the export-assignment node after reporting
            // the modifier error; keeping it as an expression statement makes
            // the recovery tree unstable under printing.
            SyntaxKind::ExportKeyword => {
                let export_token = self.take_token();
                self.parse_export(start, export_token, modifiers)
            }
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
        // `var`, `let`, `const`, or `using`. All four produce the same node kind,
        // so the keyword is recorded in the node flags — the binder reads it to
        // decide function scope versus block scope, and it is not recoverable from
        // the tree otherwise.
        let keyword = self.token.kind;
        self.next_token();
        let flags = match keyword {
            SyntaxKind::LetKeyword => tsr_ast::NodeFlags::LET,
            SyntaxKind::ConstKeyword => tsr_ast::NodeFlags::CONST,
            SyntaxKind::UsingKeyword => tsr_ast::NodeFlags::USING,
            _ => tsr_ast::NodeFlags::empty(),
        };

        // **`for (var of X)` declares nothing.** `parseVariableDeclarationList`
        // (`parser.go:1583`) checks, before parsing any declaration at all,
        // whether the token is `of` followed by an identifier and then `)` —
        // and if so builds an EMPTY declaration list, letting the `of` be read
        // as the for-of keyword. Upstream's own comment: *"the reason this is
        // not automatic is that 'of' is a valid identifier"*.
        //
        // Without it, `for (var of of) { }` declares a variable NAMED `of`,
        // consumes the second `of` as the for-of keyword, and leaves `)` for
        // the iterable — a manufactured missing identifier, and an
        // empty-source-text `.types` line upstream does not write
        // (`parserForOfStatement21` records exactly one, `>of : any`, which is
        // the *expression*, not a declaration). §214.
        //
        // The lookahead is `nextIsIdentifierAndCloseParen` (`:1595`) and it is
        // deliberately narrow: `for (var of x)` with anything else after `x`
        // really is a variable named `of`.
        if self.at(SyntaxKind::OfKeyword)
            && self.look_ahead(|parser| {
                parser.next_token();
                if !parser.is_binding_identifier() {
                    return false;
                }
                parser.next_token();
                parser.at(SyntaxKind::CloseParenToken)
            })
        {
            let end = self.node_end();
            return self.finish_node_with_flags(
                VariableDeclarationList::new(&[]),
                SyntaxKind::VariableDeclarationList,
                start,
                end,
                flags,
            );
        }

        let mut declarations = Vec::new();
        let mut trailing_comma = false;
        loop {
            declarations.push(self.parse_variable_declaration());
            if !self.eat(SyntaxKind::CommaToken) {
                break;
            }
            // **Re-test after the separator.** Upstream's `parseDelimitedList`
            // (`parser.go:664-667`) does not fall into `parseElement` after a
            // comma — it `continue`s to the top of the loop and asks
            // `isListElement` again, which for `PCVariableDeclarations` is
            // `isBindingIdentifierOrPrivateIdentifierOrPattern` (`:871`). So
            // `var a,` at end of file produces ONE declaration upstream, and
            // this loop used to produce two: the second with a missing
            // identifier that nothing in the source spells.
            //
            // The consequence was not a wrong type but an extra `.types` line
            // with empty source text, which fails the case on its assertion
            // COUNT while every line it does render is right — see
            // `docs/architecture/checker-notes-nearmiss.md` §191.
            if !self.is_binding_identifier_or_private_identifier_or_pattern() {
                trailing_comma = true;
                break;
            }
        }
        let declarations = self.arena.alloc_slice(&declarations);
        let end = self.node_end();
        // Upstream derives this from the list's span outrunning its last node
        // (`ast.go:137`); this AST keeps child slices plain, so the fact has to
        // be recorded on the parent (`NodeFlags::HAS_TRAILING_COMMA`) or it is
        // unrecoverable. `checkGrammarVariableDeclarationList`
        // (`grammarchecks.go:1648`) is what reads it, for TS1009.
        let flags =
            if trailing_comma { flags | tsr_ast::NodeFlags::HAS_TRAILING_COMMA } else { flags };
        self.finish_node_with_flags(
            VariableDeclarationList::new(declarations),
            SyntaxKind::VariableDeclarationList,
            start,
            end,
            flags,
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
        // Keep the `await`'s **own** span. It was allocated at `Span::at(start)`
        // — the `for`'s position — which made every diagnostic reported on the
        // modifier land on the `for` instead, four columns early (§830).
        let await_span = self.token.span;
        let is_await = self.eat(SyntaxKind::AwaitKeyword);
        self.expect(SyntaxKind::OpenParenToken);

        let initializer: Option<ForInitializer<'a>> = if self.at(SyntaxKind::SemicolonToken) {
            None
        } else if self.at_variable_declaration_list() {
            // `for (await using x of …)` — the `await` belongs to the declaration,
            // not to the loop.
            self.eat(SyntaxKind::AwaitKeyword);
            // §411: the DECLARATION half of the `in` ban — a var initializer
            // in a for head must not read `1 in X` as a comparison, or
            // `for (var a = 1 in X)` loses its ForIn shape
            // (`parserForInStatement4/6`; the bare-expression half has had
            // this since the arm below was written).
            self.no_in += 1;
            let list = self.parse_variable_declaration_list();
            self.no_in -= 1;
            Some(ForInitializer::VariableDeclarationList(list))
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
                Some(self.alloc_token(SyntaxKind::AwaitKeyword, await_span))
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
        // §236: **both halves**. Upstream's guard is
        // `p.token != KindSemicolonToken && p.token != KindCloseParenToken`
        // (`parser.go:1319`); this had only the first, so `for () { }` — where
        // the token after the failed `;` is `)` — parsed an expression and
        // manufactured a zero-width identifier. `compiler/for` rendered one
        // assertion more than upstream and failed on the count alone with every
        // line otherwise right.
        //
        // The incrementor below already tests `)` because that is the only
        // thing that can follow it. The condition can be closed by EITHER
        // token, and a guard written from the common case sees only the
        // common one — `docs/conventions.md` corollary 30.
        let condition =
            if self.at(SyntaxKind::SemicolonToken) || self.at(SyntaxKind::CloseParenToken) {
                None
            } else {
                Some(self.parse_expression())
            };
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
            // §698: upstream's `parseStatement` has no "no statement" answer —
            // a token that cannot start one falls through to
            // `parseExpressionOrLabeledStatement`, whose `parseExpression`
            // mints a MISSING IDENTIFIER. So `if (a` (with no body) yields an
            // `ExpressionStatement` over that identifier, and the `.types`
            // baseline records an empty-text `> : any` line for it
            // (`parserErrorRecoveryIfStatement1`–`4`,
            // `parserErrorRecovery_ObjectLiteral2`/`4`/`5`, and 8 more files
            // short by exactly that line).
            //
            // An `EmptyStatement` carries no expression, so it emitted nothing
            // and every one of those files came up one assertion short.
            self.error_at_current(&messages::STATEMENT_EXPECTED);
            let identifier = self.missing_identifier();
            let node = self.finish_node(
                ExpressionStatement::new(Some(Expression::Identifier(identifier))),
                SyntaxKind::ExpressionStatement,
                start,
            );
            Statement::ExpressionStatement(node)
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
        // Parameters and body are inside this function's own await context, not
        // the enclosing one — `parseParameters(signatureFlags)` /
        // `parseFunctionBlockOrSemicolon(signatureFlags)` at `parser.go:2506`.
        // A non-async function nested in an async one turns the context OFF,
        // which is why this is set to a value rather than pushed. §193.
        let is_async = Self::is_async(modifiers);
        let (parameters, return_type, body) = self.with_await_context(is_async, |parser| {
            let parameters = parser.parse_parameter_list();
            let return_type = parser.parse_return_type_annotation();
            // An overload signature has no body, just a semicolon.
            let body = if parser.at(SyntaxKind::OpenBraceToken) {
                Some(FunctionBody::Block(parser.parse_block()))
            } else {
                parser.parse_semicolon();
                None
            };
            (parameters, return_type, body)
        });

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

/// What may follow a modifier keyword for it to be one (`canFollowModifier`).
///
/// A member name, or the punctuation that opens one: `[computed]`, `{`, `*gen()`,
/// `...rest`.
fn can_follow_modifier(kind: SyntaxKind) -> bool {
    matches!(
        kind,
        SyntaxKind::OpenBracketToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::AsteriskToken
            | SyntaxKind::DotDotDotToken
    ) || is_literal_property_name(kind)
}

/// Upstream's `isLiteralPropertyName`: anything that can be written as a member
/// name without brackets. A keyword counts — `class C { default() {} }`.
fn is_literal_property_name(kind: SyntaxKind) -> bool {
    kind == SyntaxKind::Identifier
        || kind == SyntaxKind::PrivateIdentifier
        || kind == SyntaxKind::StringLiteral
        || kind == SyntaxKind::NumericLiteral
        || kind == SyntaxKind::BigIntLiteral
        || kind.is_keyword()
}

/// Whether `kind` is a reserved word that can never name a binding.
pub(crate) fn is_reserved_word(kind: SyntaxKind) -> bool {
    (SyntaxKind::FIRST_RESERVED_WORD as u16..=SyntaxKind::LAST_RESERVED_WORD as u16)
        .contains(&(kind as u16))
}
