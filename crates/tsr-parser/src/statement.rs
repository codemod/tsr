//! Statement and declaration parsing.

use tsr_ast::*;
use tsr_diagnostics::messages;

use crate::list::ParsingContext;
use crate::parser::Parser;

impl<'a> Parser<'a> {
    /// Parse a statement list — typescript-go's
    /// `parseList(PCSourceElements | PCBlockStatements, parseStatement)`
    /// (`parser.go`). The list machinery in `list.rs` owns recovery: a token
    /// that cannot start a statement is reported (`TS1128`) and skipped,
    /// unless an enclosing list can use it.
    pub(crate) fn parse_statement_list(&mut self, context: ParsingContext) -> Vec<Statement<'a>> {
        self.parse_list(context, Self::parse_statement)
    }

    /// Parse one statement — typescript-go's `Parser.parseStatement`
    /// (`parser.go`). Like upstream it always produces a statement: a token
    /// that starts nothing falls through to an expression statement over a
    /// missing identifier.
    pub(crate) fn parse_statement(&mut self) -> Statement<'a> {
        let docs = self.parse_leading_jsdoc();
        let statement = self.parse_statement_worker();
        self.attach_jsdoc(statement.into(), docs);
        statement
    }

    fn parse_statement_worker(&mut self) -> Statement<'a> {
        let start = self.pos();

        // Bound before matching: the guards below need `&mut self` for lookahead,
        // which a match on `self.token.kind` directly would forbid.
        let kind = self.token.kind;
        match kind {
            SyntaxKind::SemicolonToken => {
                self.next_token();
                let node =
                    self.finish_node(EmptyStatement::new(), SyntaxKind::EmptyStatement, start);
                return Statement::EmptyStatement(node);
            }
            SyntaxKind::OpenBraceToken => return Statement::Block(self.parse_block()),
            SyntaxKind::VarKeyword => return self.parse_variable_statement(start, &[]),
            SyntaxKind::LetKeyword if self.is_let_declaration() => {
                return self.parse_variable_statement(start, &[]);
            }
            SyntaxKind::AwaitKeyword if self.is_await_using_declaration() => {
                self.next_token();
                return self.parse_variable_statement(start, &[]);
            }
            SyntaxKind::UsingKeyword if self.is_using_declaration() => {
                return self.parse_variable_statement(start, &[]);
            }
            SyntaxKind::FunctionKeyword => return self.parse_function_declaration(start, &[]),
            SyntaxKind::ClassKeyword => return self.parse_class_declaration(start, &[]),
            SyntaxKind::IfKeyword => return self.parse_if_statement(),
            SyntaxKind::DoKeyword => return self.parse_do_statement(),
            SyntaxKind::WhileKeyword => return self.parse_while_statement(),
            SyntaxKind::ForKeyword => return self.parse_for_statement(),
            SyntaxKind::ContinueKeyword | SyntaxKind::BreakKeyword => {
                return self.parse_break_or_continue();
            }
            SyntaxKind::ReturnKeyword => return self.parse_return_statement(),
            SyntaxKind::WithKeyword => return self.parse_with_statement(),
            SyntaxKind::SwitchKeyword => return self.parse_switch_statement(),
            SyntaxKind::ThrowKeyword => return self.parse_throw_statement(),
            SyntaxKind::TryKeyword | SyntaxKind::CatchKeyword | SyntaxKind::FinallyKeyword => {
                return self.parse_try_statement();
            }
            SyntaxKind::DebuggerKeyword => {
                self.next_token();
                self.parse_semicolon();
                let node = self.finish_node(
                    DebuggerStatement::new(),
                    SyntaxKind::DebuggerStatement,
                    start,
                );
                return Statement::DebuggerStatement(node);
            }
            SyntaxKind::AtToken => return self.parse_declaration(),
            SyntaxKind::AsyncKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::EnumKeyword
            | SyntaxKind::ExportKeyword
            | SyntaxKind::ImportKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::AbstractKeyword
            | SyntaxKind::AccessorKeyword
            | SyntaxKind::StaticKeyword
            | SyntaxKind::ReadonlyKeyword
            | SyntaxKind::GlobalKeyword
                if self.is_start_of_declaration() =>
            {
                return self.parse_declaration();
            }
            _ => {}
        }
        self.parse_expression_or_labeled_statement()
    }

    /// typescript-go's `Parser.parseDeclaration` (`parser.go`): modifiers,
    /// then the declaration they precede.
    fn parse_declaration(&mut self) -> Statement<'a> {
        let start = self.pos();
        let modifiers = self.parse_modifiers();
        let modifiers = self.arena.alloc_slice(&modifiers);
        self.parse_declaration_after_modifiers(start, modifiers)
    }

    /// Whether the cursor starts a statement — typescript-go's
    /// `Parser.isStartOfStatement` (`parser.go`), the element test of every
    /// statement list.
    pub(crate) fn is_start_of_statement(&mut self) -> bool {
        match self.token.kind {
            // 'catch' and 'finally' do not actually indicate that the code is
            // part of a statement; they are accepted so they can be parsed
            // gracefully and reported later.
            SyntaxKind::AtToken
            | SyntaxKind::SemicolonToken
            | SyntaxKind::OpenBraceToken
            | SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::UsingKeyword
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::ClassKeyword
            | SyntaxKind::EnumKeyword
            | SyntaxKind::IfKeyword
            | SyntaxKind::DoKeyword
            | SyntaxKind::WhileKeyword
            | SyntaxKind::ForKeyword
            | SyntaxKind::ContinueKeyword
            | SyntaxKind::BreakKeyword
            | SyntaxKind::ReturnKeyword
            | SyntaxKind::WithKeyword
            | SyntaxKind::SwitchKeyword
            | SyntaxKind::ThrowKeyword
            | SyntaxKind::TryKeyword
            | SyntaxKind::DebuggerKeyword
            | SyntaxKind::CatchKeyword
            | SyntaxKind::FinallyKeyword
            // When these don't start a declaration, they're an identifier in
            // an expression statement.
            | SyntaxKind::AsyncKeyword
            | SyntaxKind::DeclareKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword
            | SyntaxKind::TypeKeyword
            | SyntaxKind::GlobalKeyword
            | SyntaxKind::DeferKeyword => true,
            SyntaxKind::ImportKeyword => {
                self.is_start_of_declaration()
                    || self.is_next_token_open_paren_or_less_than_or_dot()
            }
            SyntaxKind::ConstKeyword | SyntaxKind::ExportKeyword => self.is_start_of_declaration(),
            // When these don't start a declaration, they may be the start of a
            // class member if an identifier immediately follows. Otherwise
            // they're an identifier in an expression statement.
            SyntaxKind::AccessorKeyword
            | SyntaxKind::PublicKeyword
            | SyntaxKind::PrivateKeyword
            | SyntaxKind::ProtectedKeyword
            | SyntaxKind::StaticKeyword
            | SyntaxKind::ReadonlyKeyword => {
                self.is_start_of_declaration()
                    || !self.look_ahead(Self::next_token_is_identifier_or_keyword_on_same_line)
            }
            _ => self.is_start_of_expression(),
        }
    }

    /// typescript-go's `Parser.isNextTokenOpenParenOrLessThanOrDot`
    /// (`parser.go`).
    pub(crate) fn is_next_token_open_paren_or_less_than_or_dot(&mut self) -> bool {
        self.peek_kind(|kind| {
            matches!(
                kind,
                SyntaxKind::OpenParenToken | SyntaxKind::LessThanToken | SyntaxKind::DotToken
            )
        })
    }

    /// typescript-go's `Parser.isStartOfDeclaration` (`parser.go`).
    pub(crate) fn is_start_of_declaration(&mut self) -> bool {
        self.look_ahead(Self::scan_start_of_declaration)
    }

    /// typescript-go's `Parser.scanStartOfDeclaration` (`parser.go`): skip
    /// modifiers and decide whether a declaration keyword follows.
    fn scan_start_of_declaration(&mut self) -> bool {
        loop {
            match self.token.kind {
                SyntaxKind::VarKeyword
                | SyntaxKind::LetKeyword
                | SyntaxKind::ConstKeyword
                | SyntaxKind::FunctionKeyword
                | SyntaxKind::ClassKeyword
                | SyntaxKind::EnumKeyword => return true,
                SyntaxKind::UsingKeyword => return self.is_using_declaration(),
                SyntaxKind::AwaitKeyword => return self.is_await_using_declaration(),
                // 'declare', 'module', 'namespace', 'interface' and 'type' are
                // legal identifiers, but an identifier cannot be followed by
                // another identifier on the same line.
                SyntaxKind::InterfaceKeyword
                | SyntaxKind::TypeKeyword
                | SyntaxKind::DeferKeyword => {
                    return self.next_token_is_identifier_on_same_line();
                }
                SyntaxKind::ModuleKeyword | SyntaxKind::NamespaceKeyword => {
                    self.next_token();
                    return (self.is_identifier() || self.at(SyntaxKind::StringLiteral))
                        && !self.token.has_preceding_line_break();
                }
                SyntaxKind::AbstractKeyword
                | SyntaxKind::AccessorKeyword
                | SyntaxKind::AsyncKeyword
                | SyntaxKind::DeclareKeyword
                | SyntaxKind::PrivateKeyword
                | SyntaxKind::ProtectedKeyword
                | SyntaxKind::PublicKeyword
                | SyntaxKind::ReadonlyKeyword => {
                    let previous = self.token.kind;
                    self.next_token();
                    // ASI takes effect for this modifier.
                    if self.token.has_preceding_line_break() {
                        return false;
                    }
                    if previous == SyntaxKind::DeclareKeyword && self.at(SyntaxKind::TypeKeyword) {
                        // `declare type` commits to a type alias;
                        // `parseTypeAliasDeclaration` reports a line break.
                        return true;
                    }
                }
                SyntaxKind::GlobalKeyword => {
                    self.next_token();
                    return matches!(
                        self.token.kind,
                        SyntaxKind::OpenBraceToken
                            | SyntaxKind::Identifier
                            | SyntaxKind::ExportKeyword
                    );
                }
                SyntaxKind::ImportKeyword => {
                    self.next_token();
                    return matches!(
                        self.token.kind,
                        SyntaxKind::DeferKeyword
                            | SyntaxKind::StringLiteral
                            | SyntaxKind::AsteriskToken
                            | SyntaxKind::OpenBraceToken
                            | SyntaxKind::Identifier
                    ) || self.token.kind.is_keyword();
                }
                SyntaxKind::ExportKeyword => {
                    self.next_token();
                    if matches!(
                        self.token.kind,
                        SyntaxKind::EqualsToken
                            | SyntaxKind::AsteriskToken
                            | SyntaxKind::OpenBraceToken
                            | SyntaxKind::DefaultKeyword
                            | SyntaxKind::AsKeyword
                            | SyntaxKind::AtToken
                    ) {
                        return true;
                    }
                    if self.at(SyntaxKind::TypeKeyword) {
                        self.next_token();
                        return self.at(SyntaxKind::AsteriskToken)
                            || self.at(SyntaxKind::OpenBraceToken)
                            || (self.is_identifier() && !self.token.has_preceding_line_break());
                    }
                }
                SyntaxKind::StaticKeyword => {
                    self.next_token();
                }
                _ => return false,
            }
        }
    }

    /// typescript-go's `Parser.nextTokenIsIdentifierOnSameLine` (`parser.go`).
    fn next_token_is_identifier_on_same_line(&mut self) -> bool {
        self.next_token();
        self.is_identifier() && !self.token.has_preceding_line_break()
    }

    /// typescript-go's `Parser.isLetDeclaration` (`parser.go`): `let`
    /// followed by a binding identifier, `{` or `[`.
    fn is_let_declaration(&mut self) -> bool {
        self.look_ahead(|p| {
            p.next_token();
            p.is_binding_identifier()
                || p.at(SyntaxKind::OpenBraceToken)
                || p.at(SyntaxKind::OpenBracketToken)
        })
    }

    /// typescript-go's `Parser.isUsingDeclaration` (`parser.go`).
    pub(crate) fn is_using_declaration(&mut self) -> bool {
        self.look_ahead(|p| {
            p.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line()
        })
    }

    /// typescript-go's `Parser.isAwaitUsingDeclaration` (`parser.go`).
    pub(crate) fn is_await_using_declaration(&mut self) -> bool {
        self.look_ahead(|p| {
            p.next_token();
            p.at(SyntaxKind::UsingKeyword)
                && p.next_token_is_binding_identifier_or_start_of_destructuring_on_same_line()
        })
    }

    /// typescript-go's
    /// `Parser.nextTokenIsBindingIdentifierOrStartOfDestructuringOnSameLine`
    /// (`parser.go`) with `disallowOf` false.
    fn next_token_is_binding_identifier_or_start_of_destructuring_on_same_line(&mut self) -> bool {
        self.next_token();
        (self.is_binding_identifier() || self.at(SyntaxKind::OpenBraceToken))
            && !self.token.has_preceding_line_break()
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

    /// Whether a `for` header opens with a variable declaration list.
    ///
    /// Not yet upstream's test (`parseForOrForInOrForOfStatement` requires
    /// `using` to be followed on the same line by a binding, with `of`
    /// disallowed): `await using` lists are not flagged `AwaitUsing` here, so
    /// the printer cannot write the `await` back, and the stricter test turns
    /// that into a round-trip difference.
    fn at_variable_declaration_list(&mut self) -> bool {
        match self.token.kind {
            SyntaxKind::VarKeyword | SyntaxKind::LetKeyword | SyntaxKind::ConstKeyword => true,
            SyntaxKind::UsingKeyword => self.peek_kind(|kind| {
                matches!(
                    kind,
                    SyntaxKind::Identifier
                        | SyntaxKind::OpenBracketToken
                        | SyntaxKind::OpenBraceToken
                ) || is_contextual_keyword(kind)
            }),
            SyntaxKind::AwaitKeyword => self.peek_kind(|kind| kind == SyntaxKind::UsingKeyword),
            _ => false,
        }
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
        self.parse_modifiers_worker(
            true,
            permit_const_as_modifier,
            stop_on_start_of_class_static_block,
        )
    }

    /// A type parameter's modifiers — `parseTypeParameter`'s
    /// `parseModifiersEx(allowDecorators: false, permitConstAsModifier: true,
    /// stopOnStartOfClassStaticBlock: false)` (`parser.go:3230`). Any modifier
    /// is taken (`<public T>` is the checker's TS1273), and `in`/`out`/`const`
    /// are a modifier only when a name can follow on the same line, so
    /// `<in in>` names its parameter `in`.
    pub(crate) fn parse_type_parameter_modifiers(&mut self) -> Vec<ModifierLike<'a>> {
        self.parse_modifiers_worker(false, true, false)
    }

    fn parse_modifiers_worker(
        &mut self,
        allow_decorators: bool,
        permit_const_as_modifier: bool,
        stop_on_start_of_class_static_block: bool,
    ) -> Vec<ModifierLike<'a>> {
        let mut modifiers = Vec::new();
        let mut seen_static = false;
        loop {
            if allow_decorators && self.at(SyntaxKind::AtToken) {
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
            // `parseModifier`'s `finishNode(factory.NewModifier(kind), pos)`:
            // the node ends where the keyword ends, not at the next token.
            modifiers.push(ModifierLike::Token(self.take_token()));
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
        // `nextTokenCanFollowModifier` (`parser.go`).
        !self.look_ahead(|p| match kind {
            SyntaxKind::ExportKeyword => {
                p.next_token();
                if p.at(SyntaxKind::DefaultKeyword) {
                    return p.look_ahead(Self::next_token_can_follow_default_keyword);
                }
                if p.at(SyntaxKind::TypeKeyword) {
                    return p.look_ahead(|p| {
                        p.next_token();
                        p.can_follow_export_modifier()
                    });
                }
                p.can_follow_export_modifier()
            }
            SyntaxKind::DefaultKeyword => p.next_token_can_follow_default_keyword(),
            // `static` alone is exempt from the same-line rule: `static` on its
            // own line still modifies what comes next.
            SyntaxKind::StaticKeyword => {
                p.next_token();
                can_follow_modifier(p.token.kind)
            }
            _ => {
                p.next_token();
                !p.token.has_preceding_line_break() && can_follow_modifier(p.token.kind)
            }
        })
    }

    /// typescript-go's `Parser.canFollowExportModifier` (`parser.go`).
    fn can_follow_export_modifier(&self) -> bool {
        self.at(SyntaxKind::AtToken)
            || !matches!(
                self.token.kind,
                SyntaxKind::AsteriskToken | SyntaxKind::AsKeyword | SyntaxKind::OpenBraceToken
            ) && can_follow_modifier(self.token.kind)
    }

    /// typescript-go's `Parser.nextTokenCanFollowDefaultKeyword` (`parser.go`).
    fn next_token_can_follow_default_keyword(&mut self) -> bool {
        self.next_token();
        match self.token.kind {
            SyntaxKind::ClassKeyword
            | SyntaxKind::FunctionKeyword
            | SyntaxKind::InterfaceKeyword
            | SyntaxKind::AtToken => true,
            SyntaxKind::AbstractKeyword => self.look_ahead(|p| {
                p.next_token();
                p.at(SyntaxKind::ClassKeyword) && !p.token.has_preceding_line_break()
            }),
            SyntaxKind::AsyncKeyword => self.look_ahead(|p| {
                p.next_token();
                p.at(SyntaxKind::FunctionKeyword) && !p.token.has_preceding_line_break()
            }),
            _ => false,
        }
    }

    /// Whether the token after `const` is `enum`.
    fn next_is_enum(&mut self) -> bool {
        self.peek_kind(|kind| kind == SyntaxKind::EnumKeyword)
    }

    /// Dispatch to the declaration a modifier list precedes — typescript-go's
    /// `Parser.parseDeclarationWorker` (`parser.go`).
    pub(crate) fn parse_declaration_after_modifiers(
        &mut self,
        start: u32,
        modifiers: &'a [ModifierLike<'a>],
    ) -> Statement<'a> {
        let kind = self.token.kind;
        match kind {
            SyntaxKind::VarKeyword
            | SyntaxKind::LetKeyword
            | SyntaxKind::ConstKeyword
            | SyntaxKind::UsingKeyword => {
                return self.parse_variable_statement(start, modifiers);
            }
            SyntaxKind::AwaitKeyword if self.is_await_using_declaration() => {
                self.next_token();
                return self.parse_variable_statement(start, modifiers);
            }
            SyntaxKind::FunctionKeyword => {
                return self.parse_function_declaration(start, modifiers);
            }
            SyntaxKind::ClassKeyword => return self.parse_class_declaration(start, modifiers),
            SyntaxKind::InterfaceKeyword => {
                return self.parse_interface_declaration(start, modifiers);
            }
            SyntaxKind::TypeKeyword => return self.parse_type_alias_declaration(start, modifiers),
            SyntaxKind::EnumKeyword => return self.parse_enum_declaration(start, modifiers),
            SyntaxKind::GlobalKeyword
            | SyntaxKind::ModuleKeyword
            | SyntaxKind::NamespaceKeyword => {
                return self.parse_module_declaration(start, modifiers);
            }
            SyntaxKind::ImportKeyword => return self.parse_import_declaration(start, modifiers),
            SyntaxKind::ExportKeyword => {
                let export_token = self.take_token();
                return self.parse_export(start, export_token, modifiers);
            }
            _ => {}
        }
        // Decorators and/or modifiers promised a declaration that did not
        // follow. For recovery, an incomplete declaration.
        let at = self.node_end();
        self.error_at(&messages::DECLARATION_EXPECTED, tsr_core::Span::at(at));
        let node = self.finish_node(
            MissingDeclaration::new(modifiers),
            SyntaxKind::MissingDeclaration,
            start,
        );
        Statement::MissingDeclaration(node)
    }

    // ---- individual statements ------------------------------------------

    /// typescript-go's `Parser.parseBlock` (`parser.go`) without a custom
    /// missing-`{` message.
    pub(crate) fn parse_block(&mut self) -> &'a Block<'a> {
        self.parse_block_with(None)
    }

    /// typescript-go's `Parser.parseBlock` (`parser.go`). A block whose `{`
    /// is missing has no statements: upstream parses none rather than reading
    /// what follows as its body.
    pub(crate) fn parse_block_with(
        &mut self,
        missing_open_brace: Option<&'static tsr_diagnostics::Message>,
    ) -> &'a Block<'a> {
        self.parse_block_ex(false, missing_open_brace)
    }

    /// `parseBlock(ignoreMissingOpenBrace, diagnosticMessage)`: with
    /// `ignore_missing_open_brace` the statements are parsed even though the
    /// `{` was reported missing.
    pub(crate) fn parse_block_ex(
        &mut self,
        ignore_missing_open_brace: bool,
        missing_open_brace: Option<&'static tsr_diagnostics::Message>,
    ) -> &'a Block<'a> {
        let start = self.pos();
        let open_brace_parsed = match missing_open_brace {
            Some(message) if !self.at(SyntaxKind::OpenBraceToken) => {
                self.error_at_current(message);
                false
            }
            _ => self.expect(SyntaxKind::OpenBraceToken),
        };
        if !open_brace_parsed && !ignore_missing_open_brace {
            return self.finish_node(Block::new(&[], true), SyntaxKind::Block, start);
        }
        let statements = self.parse_statement_list(ParsingContext::BlockStatements);
        // `parseExpectedMatchingBrackets`.
        self.expect(SyntaxKind::CloseBraceToken);
        let statements = self.arena.alloc_slice(&statements);
        let block = self.finish_node(Block::new(statements, true), SyntaxKind::Block, start);
        if self.at(SyntaxKind::EqualsToken) {
            self.error_at_current(&messages::DECLARATION_OR_STATEMENT_EXPECTED_THIS_FOLLOWS_A_BLOCK_OF_STATEMENTS_SO_IF_YOU_INTENDED_TO_WRITE_A_DESTRUCTURING_ASSIGNMENT_YOU_MIGHT_NEED_TO_WRAP_THE_WHOLE_ASSIGNMENT_IN_PARENTHESES);
            self.next_token();
        }
        block
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

        // `parseDelimitedList(PCVariableDeclarations, …)`. An invalid token
        // is reported and skipped unless an enclosing list wants it
        // (`var arg\u003` is `var arg, u003` upstream, §1045), and a list
        // that ends at once — `const;` — is empty, for the checker's TS1123.
        let allow_exclamation = self.no_in == 0;
        let (declarations, trailing_comma) = self
            .parse_delimited_list(ParsingContext::VariableDeclarations, |parser| {
                parser.parse_variable_declaration(allow_exclamation)
            });
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

    /// typescript-go's `Parser.parseVariableDeclarationWorker` (`parser.go`).
    /// `allow_exclamation` is false in a `for` initializer.
    fn parse_variable_declaration(
        &mut self,
        allow_exclamation: bool,
    ) -> &'a VariableDeclaration<'a> {
        let docs = self.parse_leading_jsdoc();
        let start = self.pos();
        let name = if self.at(SyntaxKind::PrivateIdentifier) {
            // Native parseVariableDeclarationWorker supplies its private-name
            // diagnostic, then consumes the token as an identifier.
            self.error_at_current(
                &messages::PRIVATE_IDENTIFIERS_ARE_NOT_ALLOWED_IN_VARIABLE_DECLARATIONS,
            );
            let text = self.token_value();
            self.next_token();
            BindingName::Identifier(self.finish_node(
                Identifier::new(text),
                SyntaxKind::Identifier,
                start,
            ))
        } else {
            self.parse_binding_name()
        };
        let exclamation = if allow_exclamation
            && matches!(name, BindingName::Identifier(_))
            && self.at(SyntaxKind::ExclamationToken)
            && !self.token.has_preceding_line_break()
        {
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
        let node = self.finish_node(
            VariableDeclaration::new(Some(name), exclamation, type_node, initializer),
            SyntaxKind::VariableDeclaration,
            start,
        );
        self.attach_jsdoc(Node::VariableDeclaration(node), docs);
        node
    }

    fn parse_if_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.next_token();
        self.expect(SyntaxKind::OpenParenToken);
        let condition = self.parse_expression();
        self.expect(SyntaxKind::CloseParenToken);
        let then_branch = self.parse_statement();
        let else_branch =
            if self.eat(SyntaxKind::ElseKeyword) { Some(self.parse_statement()) } else { None };
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
        let body = self.parse_statement();
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
        let body = self.parse_statement();
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

        // `parseForOrForInOrForOfStatement`'s switch (`parser.go:1307`): after
        // `for await` the `of` is *expected* (a missing one is `'of'
        // expected`, then `in` may still make a for-in), otherwise optional.
        // A for-in carries no `await` token.
        let is_of = if is_await {
            self.expect(SyntaxKind::OfKeyword)
        } else {
            self.eat(SyntaxKind::OfKeyword)
        };
        if is_of || self.eat(SyntaxKind::InKeyword) {
            let expression =
                if is_of { self.parse_assignment_expression() } else { self.parse_expression() };
            self.expect(SyntaxKind::CloseParenToken);
            let body = self.parse_statement();
            let kind = if is_of { SyntaxKind::ForOfStatement } else { SyntaxKind::ForInStatement };
            let await_token = if is_await && is_of {
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
        let body = self.parse_statement();
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
        let statement = self.parse_statement();
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
        // §721: ASI. `throw` followed by a LINE BREAK cannot take the next
        // line's expression — the semicolon is inserted at the newline — so
        // upstream mints a MISSING identifier and lets the following line parse
        // as its own statement (`parser.go:1458`):
        //
        // ```text
        // throw
        // a;          upstream:  > : any      then  >a : any
        // ```
        //
        // This port called `parse_expression` unconditionally, swallowing `a`
        // into the throw and emitting one assertion fewer
        // (`throwWithoutNewLine2`).
        let expression = if self.token.flags.contains(tsr_scanner::TokenFlags::PRECEDING_LINE_BREAK)
        {
            Expression::Identifier(self.missing_identifier())
        } else {
            self.parse_expression()
        };
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

        // `parseCaseBlock`.
        let block_start = self.pos();
        self.expect(SyntaxKind::OpenBraceToken);
        let clauses =
            self.parse_list(ParsingContext::SwitchClauses, Self::parse_case_or_default_clause);
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

    /// typescript-go's `Parser.parseCaseOrDefaultClause` (`parser.go`).
    fn parse_case_or_default_clause(&mut self) -> &'a CaseOrDefaultClause<'a> {
        let clause_start = self.pos();
        let is_case = self.at(SyntaxKind::CaseKeyword);
        let kind_token = self.take_token();
        let test = if is_case { Some(self.parse_expression()) } else { None };
        self.expect(SyntaxKind::ColonToken);
        let statements =
            self.parse_list(ParsingContext::SwitchClauseStatements, Self::parse_statement);
        let statements = self.arena.alloc_slice(&statements);
        let kind = if is_case { SyntaxKind::CaseClause } else { SyntaxKind::DefaultClause };
        self.finish_node(CaseOrDefaultClause::new(kind_token, test, statements), kind, clause_start)
    }

    /// typescript-go's `Parser.parseTryStatement` (`parser.go`). Also the
    /// statement a stray `catch` or `finally` starts, which then reports
    /// `'try' expected`.
    fn parse_try_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        self.expect(SyntaxKind::TryKeyword);
        let block = self.parse_block();

        // `parseCatchClause`.
        let catch = if self.at(SyntaxKind::CatchKeyword) {
            let catch_start = self.pos();
            self.next_token();
            // `catch {}` without a binding is legal since ES2019. Otherwise
            // `parseCatchClause` parses a full `parseVariableDeclaration()`,
            // initializer included; the checker rejects it (TS1197).
            let variable = if self.eat(SyntaxKind::OpenParenToken) {
                let variable = self.parse_variable_declaration(false);
                self.expect(SyntaxKind::CloseParenToken);
                Some(variable)
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

        // If we don't have a catch clause, then we must have a finally clause.
        // Try to parse one out no matter what.
        let finally = if catch.is_none() || self.at(SyntaxKind::FinallyKeyword) {
            if !self.eat(SyntaxKind::FinallyKeyword) {
                self.error_at_current(&messages::CATCH_OR_FINALLY_EXPECTED);
            }
            Some(self.parse_block())
        } else {
            None
        };

        let node = self.finish_node(
            TryStatement::new(Some(block), catch, finally),
            SyntaxKind::TryStatement,
            start,
        );
        Statement::TryStatement(node)
    }

    /// typescript-go's `Parser.parseExpressionOrLabeledStatement`
    /// (`parser.go`).
    fn parse_expression_or_labeled_statement(&mut self) -> Statement<'a> {
        let start = self.pos();
        let expression = self.parse_expression();

        // `label:` looks like an expression statement until the colon.
        if let Expression::Identifier(label) = expression {
            if self.eat(SyntaxKind::ColonToken) {
                let statement = self.parse_statement();
                let node = self.finish_node(
                    LabeledStatement::new(Some(label), Some(statement)),
                    SyntaxKind::LabeledStatement,
                    start,
                );
                return Statement::LabeledStatement(node);
            }
        }

        if !self.try_parse_semicolon() {
            self.parse_error_for_missing_semicolon_after(expression);
        }
        let node = self.finish_node(
            ExpressionStatement::new(Some(expression)),
            SyntaxKind::ExpressionStatement,
            start,
        );
        Statement::ExpressionStatement(node)
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
        // `parseFunctionDeclaration` (`parser.go:1717`): only a `default`
        // function may omit its name; any other reports TS1003 and gets a
        // missing identifier.
        let name = if !tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::DefaultKeyword)
            || self.is_binding_identifier()
        {
            Some(self.parse_identifier())
        } else {
            None
        };
        let type_parameters = self.parse_type_parameters();
        // Parameters and body are inside this function's own await context, not
        // the enclosing one — `parseParameters(signatureFlags)` /
        // `parseFunctionBlockOrSemicolon(signatureFlags)` at `parser.go:2506`.
        // A non-async function nested in an async one turns the context OFF,
        // which is why this is set to a value rather than pushed. §193.
        let is_async = Self::is_async(modifiers);
        let is_generator = asterisk.is_some();
        let (parameters, return_type, body) =
            self.with_function_context(is_generator, is_async, |parser| {
                let parameters = parser.parse_parameter_list();
                let return_type = parser.parse_return_type_annotation();
                // An overload signature has no body, just a semicolon
                // (`parseFunctionBlockOrSemicolon`, `'{' or ';' expected`).
                let body =
                    parser.parse_function_block_or_semicolon(false, Some(&messages::OR_EXPECTED));
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

    /// typescript-go's `Parser.tryParseSemicolon` (`parser.go`).
    pub(crate) fn try_parse_semicolon(&mut self) -> bool {
        if !self.can_parse_semicolon() {
            return false;
        }
        self.eat(SyntaxKind::SemicolonToken);
        true
    }

    /// typescript-go's `Parser.parseErrorForMissingSemicolonAfter`
    /// (`parser.go`): a missing `;` after an expression statement is often a
    /// misused or misspelled keyword, and the message says so.
    pub(crate) fn parse_error_for_missing_semicolon_after(&mut self, node: Expression<'a>) {
        // Tagged template literals are sometimes used in places where only
        // simple strings are allowed: `module `M1` {` parses as module`M1`.
        if let Expression::TaggedTemplateExpression(tagged) = node {
            if let Some(template) = tagged.template.and_then(|t| t.node_id()) {
                let span = self.nodes.span(template);
                self.error_at(
                    &messages::MODULE_DECLARATION_NAMES_MAY_ONLY_USE_OR_QUOTED_STRINGS,
                    span,
                );
                return;
            }
        }
        // Otherwise, if this isn't a well-known keyword-like identifier, give
        // the generic fallback message.
        let expression_text = match node {
            Expression::Identifier(identifier) => identifier.text,
            _ => "",
        };
        let span = node.node_id().map_or(self.token.span, |id| self.nodes.span(id));
        self.parse_error_for_missing_semicolon_after_name(expression_text, span);
    }

    /// The identifier half of `parseErrorForMissingSemicolonAfter`: `text` is
    /// the node's identifier text (empty for any other node) and `span` its
    /// trivia-free extent.
    pub(crate) fn parse_error_for_missing_semicolon_after_name(
        &mut self,
        expression_text: &str,
        span: tsr_core::Span,
    ) {
        if expression_text.is_empty() {
            self.error_at_current_with(&messages::_0_EXPECTED, &[";"]);
            return;
        }
        match expression_text {
            "const" | "let" | "var" => {
                self.error_at(&messages::VARIABLE_DECLARATION_NOT_ALLOWED_AT_THIS_LOCATION, span);
                return;
            }
            // If a declared node failed to parse, it would have emitted a
            // diagnostic already.
            "declare" => return,
            "interface" => {
                self.parse_error_for_invalid_name(
                    &messages::INTERFACE_NAME_CANNOT_BE_0,
                    &messages::INTERFACE_MUST_BE_GIVEN_A_NAME,
                    SyntaxKind::OpenBraceToken,
                );
                return;
            }
            "is" => {
                let to = self.pos();
                self.error_at(
                    &messages::A_TYPE_PREDICATE_IS_ONLY_ALLOWED_IN_RETURN_TYPE_POSITION_FOR_FUNCTIONS_AND_METHODS,
                    tsr_core::Span::new(span.start, to),
                );
                return;
            }
            "module" | "namespace" => {
                self.parse_error_for_invalid_name(
                    &messages::NAMESPACE_NAME_CANNOT_BE_0,
                    &messages::NAMESPACE_MUST_BE_GIVEN_A_NAME,
                    SyntaxKind::OpenBraceToken,
                );
                return;
            }
            "type" => {
                self.parse_error_for_invalid_name(
                    &messages::TYPE_ALIAS_NAME_CANNOT_BE_0,
                    &messages::TYPE_ALIAS_MUST_BE_GIVEN_A_NAME,
                    SyntaxKind::EqualsToken,
                );
                return;
            }
            _ => {}
        }
        // The user alternatively might have misspelled or forgotten to add a
        // space after a common keyword.
        let suggestion = tsr_core::spelling::get_spelling_suggestion(
            expression_text,
            VIABLE_KEYWORD_SUGGESTIONS.iter(),
            |candidate| candidate,
            Ord::cmp,
        )
        .map(|keyword| (*keyword).to_string())
        .or_else(|| space_suggestion(expression_text));
        if let Some(suggestion) = suggestion {
            if self.would_repeat_last_error(span) {
                return;
            }
            self.diagnostics.push(tsr_diagnostics::Diagnostic::with_args(
                &messages::UNKNOWN_KEYWORD_OR_IDENTIFIER_DID_YOU_MEAN_0,
                span,
                [suggestion],
            ));
            return;
        }
        // Unknown tokens are handled with their own errors in the scanner.
        if self.at(SyntaxKind::Unknown) {
            return;
        }
        // Otherwise, we know this some kind of unknown word, not just a
        // missing expected semicolon.
        self.error_at(&messages::UNEXPECTED_KEYWORD_OR_IDENTIFIER, span);
    }

    /// typescript-go's `Parser.parseErrorForInvalidName` (`parser.go`).
    fn parse_error_for_invalid_name(
        &mut self,
        name_diagnostic: &'static tsr_diagnostics::Message,
        blank_diagnostic: &'static tsr_diagnostics::Message,
        token_if_blank_name: SyntaxKind,
    ) {
        if self.at(token_if_blank_name) {
            self.error_at_current(blank_diagnostic);
        } else {
            let value = self.token_value();
            self.error_at_current_with(name_diagnostic, &[value]);
        }
    }
}

/// The source text of a keyword kind (`scanner.TokenToString`): the
/// two-letter keywords, else the `textToKeyword` entry among
/// [`VIABLE_KEYWORD_SUGGESTIONS`]. Error paths only.
pub(crate) fn keyword_text(kind: SyntaxKind) -> Option<&'static str> {
    ["as", "do", "if", "in", "is", "of"]
        .into_iter()
        .chain(VIABLE_KEYWORD_SUGGESTIONS.iter().copied())
        .find(|text| tsr_scanner::keyword_kind(text) == Some(kind))
}

/// typescript-go's `viableKeywordSuggestions` (`parser.go`), which is
/// `scanner.GetViableKeywordSuggestions`: every keyword of `textToKeyword`
/// longer than two characters.
const VIABLE_KEYWORD_SUGGESTIONS: &[&str] = &[
    "abstract",
    "accessor",
    "any",
    "asserts",
    "assert",
    "bigint",
    "boolean",
    "break",
    "case",
    "catch",
    "class",
    "continue",
    "const",
    "constructor",
    "debugger",
    "declare",
    "default",
    "defer",
    "delete",
    "else",
    "enum",
    "export",
    "extends",
    "false",
    "finally",
    "for",
    "from",
    "function",
    "get",
    "immediate",
    "implements",
    "import",
    "infer",
    "instanceof",
    "interface",
    "intrinsic",
    "keyof",
    "let",
    "module",
    "namespace",
    "never",
    "new",
    "null",
    "number",
    "object",
    "package",
    "private",
    "protected",
    "public",
    "override",
    "out",
    "readonly",
    "require",
    "global",
    "return",
    "satisfies",
    "set",
    "static",
    "string",
    "super",
    "switch",
    "symbol",
    "this",
    "throw",
    "true",
    "try",
    "type",
    "typeof",
    "undefined",
    "unique",
    "unknown",
    "using",
    "var",
    "void",
    "while",
    "with",
    "yield",
    "async",
    "await",
];

/// typescript-go's `getSpaceSuggestion` (`parser.go`): `functionfoo` reads
/// as `function foo`.
///
/// Upstream walks a slice built from a Go map, so when two keywords are
/// prefixes of one word its pick is unordered; this walks the list above.
fn space_suggestion(expression_text: &str) -> Option<String> {
    VIABLE_KEYWORD_SUGGESTIONS.iter().find_map(|keyword| {
        (expression_text.len() > keyword.len() + 2 && expression_text.starts_with(keyword))
            .then(|| format!("{keyword} {}", &expression_text[keyword.len()..]))
    })
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
