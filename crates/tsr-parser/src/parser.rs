//! Parser state: the token cursor, error recovery, and node registration.

use tsr_ast::{HasNodeId, Identifier, NodeTable, SourceFile, SyntaxKind, Token as AstToken};
use tsr_core::{Arena, Span};
use tsr_diagnostics::{Diagnostic, Message, messages};
use tsr_scanner::{Scanner, Token};

/// What [`Parser::parse_source_file`] produced.
pub struct ParseResult<'a> {
    /// The root node.
    pub source_file: &'a SourceFile<'a>,
    /// Diagnostics in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// A recursive-descent parser over one source file.
pub struct Parser<'a> {
    pub(crate) arena: &'a Arena,
    pub(crate) source: &'a str,
    scanner: Scanner<'a>,
    /// The token under the cursor.
    pub(crate) token: Token,
    /// Decoded value of [`Self::token`], when it needed decoding.
    token_value: Option<String>,
    pub(crate) diagnostics: Vec<Diagnostic>,
    pub(crate) nodes: NodeTable,
    /// Non-zero while `in` must not be treated as a binary operator.
    ///
    /// `for (a in b)` would otherwise consume `a in b` as a comparison and leave
    /// the loop header malformed. A counter rather than a bool because the
    /// restriction nests: `for ((a in b);;)` re-enables it inside the parens.
    pub(crate) no_in: u32,
    /// Guards against runaway recursion on pathological input.
    ///
    /// TypeScript permits arbitrarily nested expressions, and a deeply nested
    /// literal in a real file would otherwise overflow the stack — a crash, not a
    /// diagnostic. Upstream has the same guard.
    depth: u32,
}

/// Maximum expression/type nesting before the parser gives up on a subtree.
///
/// Chosen well below the point a release build overflows, and far above anything
/// hand-written.
const MAX_DEPTH: u32 = 512;

impl<'a> Parser<'a> {
    /// Create a parser positioned on the first token.
    #[must_use]
    pub fn new(arena: &'a Arena, source: &'a str) -> Self {
        let mut scanner = Scanner::new(source);
        let token = scanner.scan();
        let token_value = capture_value(&scanner);
        Self {
            arena,
            source,
            scanner,
            token,
            token_value,
            diagnostics: Vec::new(),
            nodes: NodeTable::new(),
            no_in: 0,
            depth: 0,
        }
    }

    /// Consume the parser, returning its diagnostics and node table.
    #[must_use]
    pub fn finish(mut self) -> (Vec<Diagnostic>, NodeTable) {
        // Scanner diagnostics are interleaved by position so a caller sees one
        // ordered list rather than two.
        let mut diagnostics = self.diagnostics;
        diagnostics.extend(self.scanner.take_diagnostics());
        diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        (diagnostics, self.nodes)
    }

    // ---- token cursor ---------------------------------------------------

    /// Advance to the next token, returning the one just consumed.
    pub(crate) fn next_token(&mut self) -> Token {
        let previous = self.token;
        self.token = self.scanner.scan();
        self.token_value = capture_value(&self.scanner);
        previous
    }

    /// Source text of the current token.
    pub(crate) fn token_text(&self) -> &'a str {
        &self.source[self.token.span.start as usize..self.token.span.end as usize]
    }

    /// Decoded value of the current token, arena-allocated.
    pub(crate) fn token_value(&self) -> &'a str {
        match self.token_value.as_deref() {
            Some(decoded) => self.arena.alloc_str(decoded),
            None => self.token_text(),
        }
    }

    /// Whether the cursor is on `kind`.
    pub(crate) fn at(&self, kind: SyntaxKind) -> bool {
        self.token.kind == kind
    }

    /// Consume the current token if it is `kind`.
    pub(crate) fn eat(&mut self, kind: SyntaxKind) -> bool {
        if self.at(kind) {
            self.next_token();
            true
        } else {
            false
        }
    }

    /// Consume `kind`, or report that it was expected.
    ///
    /// Does **not** skip the offending token: the caller's recovery loop decides
    /// what to discard, and consuming here would swallow a token that closes an
    /// enclosing construct.
    pub(crate) fn expect(&mut self, kind: SyntaxKind) -> bool {
        if self.eat(kind) {
            return true;
        }
        self.error_at_current_with(&messages::_0_EXPECTED, &[token_to_text(kind)]);
        false
    }

    /// The position at which the current token starts.
    pub(crate) fn pos(&self) -> u32 {
        self.token.span.start
    }

    /// Re-scan the current `>`-family token as a single `>`.
    pub(crate) fn rescan_greater_than(&mut self) {
        self.token = self.scanner.rescan_greater_than();
    }

    /// Re-scan a `/` as a regular expression literal.
    pub(crate) fn rescan_regular_expression(&mut self) {
        self.token = self.scanner.rescan_as_regular_expression();
        self.token_value = capture_value(&self.scanner);
    }

    /// Re-scan a `}` as the continuation of a template literal.
    pub(crate) fn rescan_template_continuation(&mut self) {
        self.token = self.scanner.rescan_template_continuation();
        self.token_value = capture_value(&self.scanner);
    }

    /// Run `f` on a saved position, rewinding if it returns `None`.
    ///
    /// TypeScript's grammar needs this in several places — most visibly deciding
    /// whether `(` opens an arrow function's parameter list or a parenthesised
    /// expression. Diagnostics emitted during a rejected attempt are discarded,
    /// so a speculative path cannot leave phantom errors behind.
    pub(crate) fn try_parse<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        let saved_scanner = self.scanner.save();
        let saved_token = self.token;
        let saved_value = self.token_value.clone();
        let saved_diagnostics = self.diagnostics.len();
        let saved_nodes = self.nodes.len();

        let result = f(self);
        if result.is_none() {
            self.scanner.restore(saved_scanner);
            self.token = saved_token;
            self.token_value = saved_value;
            self.diagnostics.truncate(saved_diagnostics);
            // Nodes registered during the abandoned attempt stay in the table but
            // are unreachable from the tree. Truncating is safe only because ids
            // are handed out sequentially and nothing else holds one yet.
            self.nodes.truncate(saved_nodes);
        }
        result
    }

    /// Enter a nested construct, refusing past [`MAX_DEPTH`].
    ///
    /// Returns `None` when the limit is hit, which callers turn into a missing
    /// node — a diagnostic beats a stack overflow.
    pub(crate) fn descend<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> Option<T> {
        if self.depth >= MAX_DEPTH {
            return None;
        }
        self.depth += 1;
        let result = f(self);
        self.depth -= 1;
        Some(result)
    }

    // ---- diagnostics ----------------------------------------------------

    /// Report at the current token.
    pub(crate) fn error_at_current(&mut self, message: &'static Message) {
        let span = self.token.span;
        self.error_at(message, span);
    }

    /// Report at the current token, with substitution arguments.
    pub(crate) fn error_at_current_with(&mut self, message: &'static Message, args: &[&str]) {
        let span = self.token.span;
        self.diagnostics.push(Diagnostic::with_args(
            message,
            span,
            args.iter().map(|s| (*s).to_string()),
        ));
    }

    /// Report at an explicit span.
    pub(crate) fn error_at(&mut self, message: &'static Message, span: Span) {
        self.diagnostics.push(Diagnostic::new(message, span));
    }

    // ---- node construction ----------------------------------------------

    /// Allocate a node and record its kind and span.
    ///
    /// `start` is where the construct began; the node ends where the previous
    /// token ended, which is the current token's start minus its leading trivia.
    pub(crate) fn finish_node<T: HasNodeId>(
        &mut self,
        node: T,
        kind: SyntaxKind,
        start: u32,
    ) -> &'a T {
        let end = self.node_end();
        let id = self.nodes.push(kind, Span::new(start, end), tsr_ast::NodeFlags::empty());
        let allocated: &'a T = self.arena.alloc(node);
        allocated.set_node_id(id);
        allocated
    }

    /// Where the most recently consumed token ended.
    fn node_end(&self) -> u32 {
        self.scanner.full_start()
    }

    /// A synthesised identifier standing in for one the source omitted.
    ///
    /// Error recovery needs a node to attach children to; a zero-width identifier
    /// keeps the tree walkable without pretending text exists.
    pub(crate) fn missing_identifier(&mut self) -> &'a Identifier<'a> {
        let start = self.pos();
        let node = Identifier::new("");
        let id =
            self.nodes.push(SyntaxKind::Identifier, Span::at(start), tsr_ast::NodeFlags::empty());
        let allocated: &'a Identifier<'a> = self.arena.alloc(node);
        allocated.node_id.set(Some(id));
        allocated
    }

    /// Allocate a token node.
    pub(crate) fn alloc_token(&mut self, kind: SyntaxKind, span: Span) -> &'a AstToken<'a> {
        self.nodes.push(kind, span, tsr_ast::NodeFlags::empty());
        self.arena.alloc(AstToken::new(kind))
    }

    /// Consume the current token and allocate it as a node.
    pub(crate) fn take_token(&mut self) -> &'a AstToken<'a> {
        let token = self.token;
        self.next_token();
        self.alloc_token(token.kind, token.span)
    }

    // ---- entry point ----------------------------------------------------

    /// Parse a whole file.
    pub fn parse_source_file(&mut self) -> &'a SourceFile<'a> {
        let start = self.pos();
        let statements = self.parse_statement_list(SyntaxKind::EndOfFile);
        let eof = self.alloc_token(SyntaxKind::EndOfFile, self.token.span);
        self.finish_node(
            SourceFile::new(self.arena.alloc_slice(&statements), eof),
            SyntaxKind::SourceFile,
            start,
        )
    }
}

/// Capture the scanner's decoded value, if it differs from the raw text.
fn capture_value(scanner: &Scanner<'_>) -> Option<String> {
    let value = scanner.token_value();
    if std::ptr::eq(value.as_ptr(), scanner.token_text().as_ptr())
        && value.len() == scanner.token_text().len()
    {
        None
    } else {
        Some(value.to_string())
    }
}

/// Human-readable text for a token kind, for `'{0}' expected.`
pub(crate) fn token_to_text(kind: SyntaxKind) -> &'static str {
    match kind {
        SyntaxKind::OpenBraceToken => "{",
        SyntaxKind::CloseBraceToken => "}",
        SyntaxKind::OpenParenToken => "(",
        SyntaxKind::CloseParenToken => ")",
        SyntaxKind::OpenBracketToken => "[",
        SyntaxKind::CloseBracketToken => "]",
        SyntaxKind::SemicolonToken => ";",
        SyntaxKind::CommaToken => ",",
        SyntaxKind::ColonToken => ":",
        SyntaxKind::DotToken => ".",
        SyntaxKind::EqualsToken => "=",
        SyntaxKind::EqualsGreaterThanToken => "=>",
        SyntaxKind::LessThanToken => "<",
        SyntaxKind::GreaterThanToken => ">",
        SyntaxKind::QuestionToken => "?",
        SyntaxKind::Identifier => "identifier",
        SyntaxKind::WhileKeyword => "while",
        SyntaxKind::FromKeyword => "from",
        other => other.name(),
    }
}
