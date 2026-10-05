//! List parsing and its error recovery: upstream's `ParsingContext` machinery.
//!
//! Ported from typescript-go's `Parser.parseList`/`parseListIndex`
//! (`internal/parser/parser.go`), `isListElement`, `isListTerminator`,
//! `abortParsingListOrMoveToNextToken`, `isInSomeParsingContext` and
//! `parsingContextErrors`.
//!
//! Every list the parser is inside is recorded as one bit of
//! [`Parser::parsing_contexts`]. A token that is neither an element nor the
//! terminator of the current list is reported with the list's own message and
//! skipped — **unless** it is an element or terminator of *any* enclosing
//! list, in which case the current list ends there and the enclosing one
//! resumes. That decision is what keeps one stray token from cascading into
//! a run of follow-on errors, and it is only right when every enclosing list
//! records its context; a list loop that does not is invisible to it.
//!
//! The enum lists only the contexts whose list loops are ported onto this
//! machinery. Upstream's numbering is kept for those so a bit means the same
//! context on both sides.

use tsr_ast::SyntaxKind;
use tsr_diagnostics::messages;

use crate::parser::Parser;

/// typescript-go's `ParsingContext` (`parser.go`), restricted to the contexts
/// whose lists use this module.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub(crate) enum ParsingContext {
    /// `PCSourceElements`: statements of a source file.
    SourceElements = 0,
    /// `PCBlockStatements`: statements of a block or module block.
    BlockStatements = 1,
    /// `PCSwitchClauses`: the clauses of a `switch` case block.
    SwitchClauses = 2,
    /// `PCSwitchClauseStatements`: statements of a `case`/`default` clause.
    SwitchClauseStatements = 3,
    /// `PCTypeMembers`: members of an interface or type literal.
    TypeMembers = 4,
    /// `PCClassMembers`: members of a class body.
    ClassMembers = 5,
    /// `PCEnumMembers`: members of an enum body.
    EnumMembers = 6,
    /// `PCVariableDeclarations`: declarations of a variable statement.
    VariableDeclarations = 8,
    /// `PCArgumentExpressions`: arguments of a call or `new`.
    ArgumentExpressions = 11,
}

impl ParsingContext {
    /// Every context, in upstream's order — `isInSomeParsingContext` walks
    /// them lowest bit first.
    const ALL: [Self; 9] = [
        Self::SourceElements,
        Self::BlockStatements,
        Self::SwitchClauses,
        Self::SwitchClauseStatements,
        Self::TypeMembers,
        Self::ClassMembers,
        Self::EnumMembers,
        Self::VariableDeclarations,
        Self::ArgumentExpressions,
    ];

    const fn bit(self) -> u32 {
        1 << self as u8
    }
}

impl Parser<'_> {
    /// Parse elements until the list's terminator — typescript-go's
    /// `Parser.parseList` (`parser.go`), via `parseListIndex`.
    pub(crate) fn parse_list<T>(
        &mut self,
        kind: ParsingContext,
        mut parse_element: impl FnMut(&mut Self) -> T,
    ) -> Vec<T> {
        let saved = self.parsing_contexts;
        self.parsing_contexts |= kind.bit();
        let mut list = Vec::new();
        while !self.is_list_terminator(kind) {
            if self.is_list_element(kind, false) {
                let before = self.pos();
                list.push(parse_element(self));
                // Upstream relies on `isListElement` admitting only tokens
                // its element parser consumes. Should a port gap ever break
                // that, skip the token rather than spin.
                if self.pos() == before && !self.at(SyntaxKind::EndOfFile) {
                    debug_assert!(false, "{kind:?} element consumed nothing");
                    self.next_token();
                }
                continue;
            }
            if self.abort_parsing_list_or_move_to_next_token(kind) {
                break;
            }
        }
        self.parsing_contexts = saved;
        list
    }

    /// Parse comma-separated elements until the list's terminator —
    /// typescript-go's `Parser.parseDelimitedList` (`parser.go`). Also
    /// answers whether the list ended in a comma, which upstream reads off
    /// the list's span.
    pub(crate) fn parse_delimited_list<T>(
        &mut self,
        kind: ParsingContext,
        mut parse_element: impl FnMut(&mut Self) -> T,
    ) -> (Vec<T>, bool) {
        let saved = self.parsing_contexts;
        self.parsing_contexts |= kind.bit();
        let mut list = Vec::new();
        let mut trailing_comma = false;
        loop {
            if self.is_list_element(kind, false) {
                let start = self.node_end();
                list.push(parse_element(self));
                trailing_comma = false;
                if self.eat(SyntaxKind::CommaToken) {
                    // No need to check for a zero length node since we know
                    // we parsed a comma.
                    trailing_comma = true;
                    continue;
                }
                if self.is_list_terminator(kind) {
                    break;
                }
                // We didn't get a comma, and the list wasn't terminated:
                // explicitly parse out a comma for a good error message.
                if kind == ParsingContext::EnumMembers {
                    self.error_at_current(&messages::AN_ENUM_MEMBER_NAME_MUST_BE_FOLLOWED_BY_A_OR);
                } else {
                    self.expect(SyntaxKind::CommaToken);
                }
                if start == self.node_end() {
                    // Not remotely recognizable as an element and nothing was
                    // consumed: advance to avoid an infinite loop.
                    self.next_token();
                }
                continue;
            }
            if self.is_list_terminator(kind) {
                break;
            }
            if self.abort_parsing_list_or_move_to_next_token(kind) {
                break;
            }
        }
        self.parsing_contexts = saved;
        (list, trailing_comma)
    }

    /// Whether the cursor starts an element of `context` — typescript-go's
    /// `Parser.isListElement` (`parser.go`).
    pub(crate) fn is_list_element(
        &mut self,
        context: ParsingContext,
        in_error_recovery: bool,
    ) -> bool {
        match context {
            ParsingContext::SourceElements
            | ParsingContext::BlockStatements
            | ParsingContext::SwitchClauseStatements => {
                // If we're in error recovery, then we don't want to treat ';'
                // as an empty statement: it shows up in far too many contexts.
                !(self.at(SyntaxKind::SemicolonToken) && in_error_recovery)
                    && self.is_start_of_statement()
            }
            ParsingContext::SwitchClauses => {
                self.at(SyntaxKind::CaseKeyword) || self.at(SyntaxKind::DefaultKeyword)
            }
            ParsingContext::TypeMembers => self.look_ahead(Self::scan_type_member_start),
            // Semicolons are class elements (as specified by ES6) except in
            // error recovery, where they almost always belong to statements.
            ParsingContext::ClassMembers => {
                self.look_ahead(Self::scan_class_member_start)
                    || (self.at(SyntaxKind::SemicolonToken) && !in_error_recovery)
            }
            // Include open bracket computed properties. This technically also
            // lets in indexers, which would be a candidate for improved error
            // reporting.
            ParsingContext::EnumMembers => {
                self.at(SyntaxKind::OpenBracketToken) || self.is_literal_property_name()
            }
            ParsingContext::VariableDeclarations => {
                self.is_binding_identifier_or_private_identifier_or_pattern()
            }
            ParsingContext::ArgumentExpressions => {
                self.at(SyntaxKind::DotDotDotToken) || self.is_start_of_expression()
            }
        }
    }

    /// typescript-go's `Parser.isLiteralPropertyName` (`parser.go`).
    pub(crate) fn is_literal_property_name(&self) -> bool {
        token_is_identifier_or_keyword(self.token.kind)
            || matches!(
                self.token.kind,
                SyntaxKind::StringLiteral | SyntaxKind::NumericLiteral | SyntaxKind::BigIntLiteral
            )
    }

    /// typescript-go's `Parser.scanTypeMemberStart` (`parser.go`).
    fn scan_type_member_start(&mut self) -> bool {
        // Return true if we have the start of a signature member.
        if matches!(
            self.token.kind,
            SyntaxKind::OpenParenToken
                | SyntaxKind::LessThanToken
                | SyntaxKind::GetKeyword
                | SyntaxKind::SetKeyword
        ) {
            return true;
        }
        let mut id_token = false;
        // Eat up all modifiers, but hold on to the last one in case it is
        // actually an identifier.
        while self.token.kind.is_modifier() {
            id_token = true;
            self.next_token();
        }
        // Index signatures and computed property names are type members.
        if self.at(SyntaxKind::OpenBracketToken) {
            return true;
        }
        // Try to get the first property-like token following all modifiers.
        if self.is_literal_property_name() {
            id_token = true;
            self.next_token();
        }
        // If we were able to get any potential identifier, check that it is
        // the start of a member declaration.
        id_token
            && (matches!(
                self.token.kind,
                SyntaxKind::OpenParenToken
                    | SyntaxKind::LessThanToken
                    | SyntaxKind::QuestionToken
                    | SyntaxKind::ColonToken
                    | SyntaxKind::CommaToken
            ) || self.can_parse_semicolon())
    }

    /// typescript-go's `Parser.scanClassMemberStart` (`parser.go`).
    fn scan_class_member_start(&mut self) -> bool {
        let mut id_token = None;
        if self.at(SyntaxKind::AtToken) {
            return true;
        }
        // Eat up all modifiers, but hold on to the last one in case it is
        // actually an identifier.
        while self.token.kind.is_modifier() {
            id_token = Some(self.token.kind);
            // A class member modifier (`ast.IsClassMemberModifier`) certainly
            // starts a class member, which allows better error recovery.
            if matches!(
                self.token.kind,
                SyntaxKind::PublicKeyword
                    | SyntaxKind::PrivateKeyword
                    | SyntaxKind::ProtectedKeyword
                    | SyntaxKind::ReadonlyKeyword
                    | SyntaxKind::OverrideKeyword
                    | SyntaxKind::StaticKeyword
                    | SyntaxKind::AccessorKeyword
            ) {
                return true;
            }
            self.next_token();
        }
        if self.at(SyntaxKind::AsteriskToken) {
            return true;
        }
        // Try to get the first property-like token following all modifiers.
        // This can either be an identifier or the 'get' or 'set' keywords.
        if self.is_literal_property_name() {
            id_token = Some(self.token.kind);
            self.next_token();
        }
        // Index signatures and computed properties are class members.
        if self.at(SyntaxKind::OpenBracketToken) {
            return true;
        }
        let Some(id_token) = id_token else { return false };
        // A non-keyword identifier, or an accessor, is safe to parse.
        if !id_token.is_keyword()
            || matches!(id_token, SyntaxKind::SetKeyword | SyntaxKind::GetKeyword)
        {
            return true;
        }
        // A keyword other than an accessor: look a little farther along.
        match self.token.kind {
            SyntaxKind::OpenParenToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::ExclamationToken
            | SyntaxKind::ColonToken
            | SyntaxKind::EqualsToken
            | SyntaxKind::QuestionToken => true,
            // Semicolons, closing braces, end of file and line breaks.
            _ => self.can_parse_semicolon(),
        }
    }

    /// Whether the cursor ends a list of `kind` — typescript-go's
    /// `Parser.isListTerminator` (`parser.go`).
    pub(crate) fn is_list_terminator(&mut self, kind: ParsingContext) -> bool {
        if self.at(SyntaxKind::EndOfFile) {
            return true;
        }
        match kind {
            ParsingContext::SourceElements => false,
            ParsingContext::BlockStatements
            | ParsingContext::SwitchClauses
            | ParsingContext::TypeMembers
            | ParsingContext::ClassMembers
            | ParsingContext::EnumMembers => self.at(SyntaxKind::CloseBraceToken),
            // If we can consume a semicolon (either explicitly, or with ASI),
            // then consider us done. A for-in/of declaration ends at its
            // keyword, and for error recovery a `=>` stops the list at once.
            ParsingContext::VariableDeclarations => {
                self.can_parse_semicolon()
                    || matches!(
                        self.token.kind,
                        SyntaxKind::InKeyword
                            | SyntaxKind::OfKeyword
                            | SyntaxKind::EqualsGreaterThanToken
                    )
            }
            // Tokens other than ')' are here for better error recovery.
            ParsingContext::ArgumentExpressions => {
                matches!(self.token.kind, SyntaxKind::CloseParenToken | SyntaxKind::SemicolonToken)
            }
            ParsingContext::SwitchClauseStatements => matches!(
                self.token.kind,
                SyntaxKind::CloseBraceToken | SyntaxKind::CaseKeyword | SyntaxKind::DefaultKeyword
            ),
        }
    }

    /// Report the token, then either end the list (an enclosing list wants
    /// the token) or skip it. Returns whether to abort — typescript-go's
    /// `Parser.abortParsingListOrMoveToNextToken` (`parser.go`).
    pub(crate) fn abort_parsing_list_or_move_to_next_token(
        &mut self,
        kind: ParsingContext,
    ) -> bool {
        self.parsing_context_errors(kind);
        if self.is_in_some_parsing_context() {
            return true;
        }
        self.next_token();
        false
    }

    /// Whether the cursor is at an element or terminator of the current list
    /// or any enclosing one — typescript-go's `Parser.isInSomeParsingContext`
    /// (`parser.go`).
    fn is_in_some_parsing_context(&mut self) -> bool {
        for kind in ParsingContext::ALL {
            if self.parsing_contexts & kind.bit() != 0
                && (self.is_list_element(kind, true) || self.is_list_terminator(kind))
            {
                return true;
            }
        }
        false
    }

    /// The error for a token a list of `context` cannot use — typescript-go's
    /// `Parser.parsingContextErrors` (`parser.go`).
    fn parsing_context_errors(&mut self, context: ParsingContext) {
        match context {
            ParsingContext::SourceElements if self.at(SyntaxKind::DefaultKeyword) => {
                self.error_at_current_with(&messages::_0_EXPECTED, &["export"]);
            }
            ParsingContext::SourceElements | ParsingContext::BlockStatements => {
                self.error_at_current(&messages::DECLARATION_OR_STATEMENT_EXPECTED);
            }
            ParsingContext::SwitchClauses => {
                self.error_at_current(&messages::CASE_OR_DEFAULT_EXPECTED);
            }
            ParsingContext::SwitchClauseStatements => {
                self.error_at_current(&messages::STATEMENT_EXPECTED);
            }
            ParsingContext::TypeMembers => {
                self.error_at_current(&messages::PROPERTY_OR_SIGNATURE_EXPECTED);
            }
            ParsingContext::ClassMembers => {
                self.error_at_current(
                    &messages::UNEXPECTED_TOKEN_A_CONSTRUCTOR_METHOD_ACCESSOR_OR_PROPERTY_WAS_EXPECTED,
                );
            }
            ParsingContext::EnumMembers => self.error_at_current(&messages::ENUM_MEMBER_EXPECTED),
            ParsingContext::VariableDeclarations if self.token.kind.is_keyword() => {
                let text = self.token_text();
                self.error_at_current_with(
                    &messages::_0_IS_NOT_ALLOWED_AS_A_VARIABLE_DECLARATION_NAME,
                    &[text],
                );
            }
            ParsingContext::VariableDeclarations => {
                self.error_at_current(&messages::VARIABLE_DECLARATION_EXPECTED);
            }
            ParsingContext::ArgumentExpressions => {
                self.error_at_current(&messages::ARGUMENT_EXPRESSION_EXPECTED);
            }
        }
    }
}

/// typescript-go's `tokenIsIdentifierOrKeyword` (`internal/parser/utilities.go`):
/// every kind from `Identifier` on — identifiers, private identifiers and
/// keywords.
pub(crate) fn token_is_identifier_or_keyword(kind: SyntaxKind) -> bool {
    kind >= SyntaxKind::Identifier && kind <= SyntaxKind::LAST_TOKEN
}
