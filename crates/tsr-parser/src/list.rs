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
}

impl ParsingContext {
    /// Every context, in upstream's order — `isInSomeParsingContext` walks
    /// them lowest bit first.
    const ALL: [Self; 4] = [
        Self::SourceElements,
        Self::BlockStatements,
        Self::SwitchClauses,
        Self::SwitchClauseStatements,
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
            ParsingContext::BlockStatements | ParsingContext::SwitchClauses => {
                self.at(SyntaxKind::CloseBraceToken)
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
        }
    }
}
