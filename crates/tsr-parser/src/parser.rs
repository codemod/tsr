//! Parser state: the token cursor, error recovery, and node registration.

use tsr_ast::{HasNodeId, Identifier, NodeTable, SourceFile, SyntaxKind, Token as AstToken};
use tsr_core::{Arena, Span};
use tsr_diagnostics::{Diagnostic, Message, messages};
use tsr_scanner::{Scanner, Token};

/// Which dialect a file is parsed as.
///
/// The only thing this changes is what `<` means in expression position, and the
/// two readings are mutually exclusive: in `.ts` a leading `<` is a type
/// assertion (`<Foo>x`), and in `.tsx` it opens a JSX element. TypeScript made
/// them exclusive for exactly this reason — no lookahead can separate
/// `<Foo>x` from `<Foo>x</Foo>` cheaply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ScriptKind {
    /// `.ts`, `.mts`, `.cts`, `.d.ts` — `<T>expr` is a type assertion.
    #[default]
    TypeScript,
    /// `.tsx`, `.jsx` — `<` opens JSX; type assertions must use `as`.
    Tsx,
    /// `.json`. A file is a single value, not a statement list.
    ///
    /// Not merely a dialect flag: it selects a different entry point
    /// ([`Parser::parse_json_text`]), because a JSON document has no statements.
    Json,
}

impl ScriptKind {
    /// Infer the dialect from a file name.
    #[must_use]
    pub fn from_file_name(name: &str) -> Self {
        let extension = std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "tsx" | "jsx" => Self::Tsx,
            "json" => Self::Json,
            _ => Self::TypeScript,
        }
    }

    /// Whether `<` in expression position opens JSX.
    #[must_use]
    pub const fn allows_jsx(self) -> bool {
        matches!(self, Self::Tsx)
    }
}

/// What [`Parser::parse_source_file`] produced.
pub struct ParseResult<'a> {
    /// The root node.
    pub source_file: &'a SourceFile<'a>,
    /// Diagnostics in source order.
    pub diagnostics: Vec<Diagnostic>,
}

/// JSDoc comments, keyed by the node they document.
///
/// Sparse by construction: only nodes actually preceded by a `/** … */` appear,
/// which in a typical TypeScript file is a small minority. A sorted `Vec` rather
/// than a map because it is built in source order and read by lookup far less
/// often than it is built — see ADR-0003 on side tables.
#[derive(Debug, Default)]
pub struct JSDocTable<'a> {
    entries: Vec<(tsr_ast::NodeId, &'a [&'a tsr_ast::JSDoc<'a>])>,
}

impl<'a> JSDocTable<'a> {
    /// The JSDoc attached to `node`, or an empty slice.
    #[must_use]
    pub fn get(&self, node: tsr_ast::NodeId) -> &'a [&'a tsr_ast::JSDoc<'a>] {
        self.entries.iter().find(|(id, _)| *id == node).map_or(&[], |(_, docs)| *docs)
    }

    /// How many nodes carry JSDoc.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether no node carries JSDoc.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Every documented node, in source order.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (tsr_ast::NodeId, &'a [&'a tsr_ast::JSDoc<'a>])> + '_ {
        self.entries.iter().copied()
    }
}

/// What to build while parsing.
///
/// JSDoc is optional because it is the one part of the tree that only some
/// consumers want, and it is expensive: on `dom.generated.d.ts` it is 46% of
/// parse time and 57% of allocations. typescript-go makes the same split by
/// deferring JSDoc for `.ts` files — see
/// [ADR-0010](../../../docs/adr/0010-jsdoc-is-a-parse-option.md).
#[derive(Debug, Clone, Copy)]
pub struct ParseOptions {
    /// Which dialect to parse: `.tsx` reads a leading `<` as JSX.
    pub script_kind: ScriptKind,
    /// Whether to fill in the parent column of the node table.
    ///
    /// Defaults to `true`, matching typescript-go, which assigns parents inside
    /// `finishNode` and has no way to opt out. Ours is a separate pass, so a
    /// consumer that never walks upward can skip it — see
    /// [`tsr_ast::assign_parents`].
    pub parents: bool,
    /// Whether to parse `/** … */` comments into the JSDoc side table.
    ///
    /// Defaults to `true`. A consumer that turns this off and then reads
    /// [`crate::JSDocTable`] gets an empty table, not an error, so the default
    /// is the safe one: paying for JSDoc you did not need is a performance bug,
    /// while silently losing documentation is a correctness one.
    pub jsdoc: bool,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self { script_kind: ScriptKind::TypeScript, parents: true, jsdoc: true }
    }
}

impl ParseOptions {
    /// Options for a file, with the dialect inferred from its name.
    #[must_use]
    pub fn for_file(name: &str) -> Self {
        Self { script_kind: ScriptKind::from_file_name(name), ..Self::default() }
    }

    /// The same options with JSDoc parsing turned off.
    #[must_use]
    pub const fn without_jsdoc(mut self) -> Self {
        self.jsdoc = false;
        self
    }

    /// The same options with parent assignment turned off.
    #[must_use]
    pub const fn without_parents(mut self) -> Self {
        self.parents = false;
        self
    }
}

/// A recursive-descent parser over one source file.
pub struct Parser<'a> {
    pub(crate) arena: &'a Arena,
    pub(crate) source: &'a str,
    pub(crate) script_kind: ScriptKind,
    pub(crate) scanner: Scanner<'a>,
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
    /// JSDoc comments, keyed by the node they document.
    ///
    /// A side table rather than a field on each node, per ADR-0003: JSDoc is
    /// absent from the overwhelming majority of nodes, and a field would cost
    /// every node a pointer to carry information a handful of them use.
    pub(crate) jsdoc: Vec<(tsr_ast::NodeId, &'a [&'a tsr_ast::JSDoc<'a>])>,
    /// Whether to parse JSDoc; see [`ParseOptions::jsdoc`].
    pub(crate) parse_jsdoc: bool,
    /// Whether to record parents as nodes are finished.
    assign_parents: bool,

    /// Guards against runaway recursion on pathological input.
    ///
    /// TypeScript permits arbitrarily nested expressions, and a deeply nested
    /// literal in a real file would otherwise overflow the stack — a crash, not a
    /// diagnostic. Upstream has the same guard.
    depth: u32,
}

/// Maximum expression/type nesting before the parser gives up on a subtree.
///
/// The binding constraint is a **debug** build: frames are several times larger
/// than in release, and test threads get a smaller stack than the main thread. An
/// earlier value of 512 was fine until the expression parser grew, at which point
/// a 2,000-paren input overflowed — the guard was doing its job and the constant
/// had silently stopped being conservative.
///
/// 192 is still far beyond anything hand-written, and the corpus's deepest real
/// nesting is 69.
const MAX_DEPTH: u32 = 192;

impl<'a> Parser<'a> {
    /// Create a parser positioned on the first token, in TypeScript dialect.
    #[must_use]
    pub fn new(arena: &'a Arena, source: &'a str) -> Self {
        Self::with_script_kind(arena, source, ScriptKind::TypeScript)
    }

    /// Create a parser for a specific dialect.
    #[must_use]
    pub fn with_script_kind(arena: &'a Arena, source: &'a str, script_kind: ScriptKind) -> Self {
        Self::with_options(arena, source, ParseOptions { script_kind, ..Default::default() })
    }

    /// Create a parser with explicit options.
    #[must_use]
    pub fn with_options(arena: &'a Arena, source: &'a str, options: ParseOptions) -> Self {
        let script_kind = options.script_kind;
        let mut scanner = Scanner::new(source);
        let token = scanner.scan();
        let token_value = capture_value(&scanner);
        Self {
            arena,
            source,
            script_kind,
            scanner,
            token,
            token_value,
            diagnostics: Vec::new(),
            // One node per ~10 bytes is what the corpus shows: checker.ts is
            // 3.15 MB for 304,884 nodes, dom.generated.d.ts 2.35 MB for 124,103.
            // Estimating low would reintroduce the growth this avoids, so this
            // takes the denser of the two.
            nodes: NodeTable::with_capacity(source.len() / 10),
            no_in: 0,
            jsdoc: Vec::new(),
            parse_jsdoc: options.jsdoc,
            assign_parents: options.parents,
            depth: 0,
        }
    }

    /// Consume the parser, returning its diagnostics and node table.
    #[must_use]
    pub fn finish(mut self) -> (Vec<Diagnostic>, NodeTable, JSDocTable<'a>) {
        // Scanner diagnostics are interleaved by position so a caller sees one
        // ordered list rather than two.
        let mut diagnostics = self.diagnostics;
        diagnostics.extend(self.scanner.take_diagnostics());
        diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        (diagnostics, self.nodes, JSDocTable { entries: self.jsdoc })
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

    /// Re-scan a compound `<` token as a single `<`.
    pub(crate) fn rescan_less_than(&mut self) {
        self.token = self.scanner.rescan_less_than();
    }

    /// Re-scan a `/` as a regular expression literal.
    pub(crate) fn rescan_regular_expression(&mut self) {
        self.token = self.scanner.rescan_as_regular_expression();
        self.token_value = capture_value(&self.scanner);
    }

    /// Re-scan the current token as JSX child content.
    pub(crate) fn rescan_jsx_token(&mut self) {
        self.token = self.scanner.rescan_jsx_token();
        self.token_value = capture_value(&self.scanner);
    }

    /// Scan the next token as JSX child content.
    pub(crate) fn scan_jsx_token(&mut self) {
        self.token = self.scanner.scan_jsx_token();
        self.token_value = capture_value(&self.scanner);
    }

    /// Extend the current identifier with JSX's `-`.
    pub(crate) fn scan_jsx_identifier(&mut self) {
        self.token = self.scanner.scan_jsx_identifier();
        self.token_value = capture_value(&self.scanner);
    }

    /// Re-scan the current token as a JSX attribute value.
    pub(crate) fn rescan_jsx_attribute_value(&mut self) {
        self.token = self.scanner.rescan_jsx_attribute_value();
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
        let saved = self.save_state();
        let result = f(self);
        if result.is_none() {
            self.restore_state(saved);
        }
        result
    }

    /// Run `f` on a saved position and rewind unconditionally
    /// (upstream's `Parser.lookAhead`).
    ///
    /// [`Self::try_parse`] rewinds only on failure, which is what a speculative
    /// *parse* wants. A lookahead asks a question and never keeps the answer's
    /// side effects — JSON parsing needs one to tell `{"a": 1}` written without
    /// its braces from a bare string.
    pub(crate) fn look_ahead<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let saved = self.save_state();
        let result = f(self);
        self.restore_state(saved);
        result
    }

    fn save_state(&self) -> ParserState {
        ParserState {
            scanner: self.scanner.save(),
            token: self.token,
            token_value: self.token_value.clone(),
            diagnostics: self.diagnostics.len(),
            nodes: self.nodes.len(),
        }
    }

    fn restore_state(&mut self, saved: ParserState) {
        self.scanner.restore(saved.scanner);
        self.token = saved.token;
        self.token_value = saved.token_value;
        self.diagnostics.truncate(saved.diagnostics);
        // Nodes registered during the abandoned attempt stay in the table but
        // are unreachable from the tree. Truncating is safe only because ids are
        // handed out sequentially and nothing else holds one yet.
        self.nodes.truncate(saved.nodes);
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
    pub(crate) fn finish_node<T>(&mut self, node: T, kind: SyntaxKind, start: u32) -> &'a T
    where
        T: HasNodeId,
        &'a T: Into<tsr_ast::Node<'a>>,
    {
        let end = self.node_end();
        self.finish_node_with_end(node, kind, start, end)
    }

    /// Record `id` as the parent of every immediate child of `node`.
    ///
    /// Done here rather than in a pass over the finished tree. The children were
    /// created moments ago and are still in cache, whereas a second traversal
    /// re-walks a tree that has fallen out of it — measured at 11.2% of a
    /// `checker.ts` parse. typescript-go does the same thing in `finishNode` for
    /// what is presumably the same reason.
    fn record_parent_of_children(&mut self, node: tsr_ast::Node<'a>, id: tsr_ast::NodeId) {
        // Ids directly, without materialising the children. Collecting them into a
        // `Vec<Node>` first cost a push per child and then a 192-arm
        // `Node::node_id` match to read each id back out; `for_each_child_id`
        // dispatches at the field's static type, which is usually a direct `Cell`
        // read.
        let nodes = &mut self.nodes;
        tsr_ast::for_each_child_id(node, |child_id| nodes.set_parent(child_id, id));
    }

    /// Allocate a node with an explicit end offset.
    ///
    /// JSDoc needs this: its nodes end at token boundaries the ordinary
    /// "previous token's end" rule does not describe, because inside a comment
    /// layout is tokenised rather than skipped as trivia.
    pub(crate) fn finish_node_with_end<T>(
        &mut self,
        node: T,
        kind: SyntaxKind,
        start: u32,
        end: u32,
    ) -> &'a T
    where
        T: HasNodeId,
        &'a T: Into<tsr_ast::Node<'a>>,
    {
        self.finish_node_with_flags(node, kind, start, end, tsr_ast::NodeFlags::empty())
    }

    /// Allocate a node with explicit end and node flags.
    ///
    /// Node flags carry facts the tree's shape does not: whether a variable list
    /// was written `var`, `let`, or `const` is not recoverable from the nodes,
    /// because upstream models all three with one node kind and distinguishes them
    /// here. The binder needs it to decide function versus block scope.
    pub(crate) fn finish_node_with_flags<T>(
        &mut self,
        node: T,
        kind: SyntaxKind,
        start: u32,
        end: u32,
        flags: tsr_ast::NodeFlags,
    ) -> &'a T
    where
        T: HasNodeId,
        &'a T: Into<tsr_ast::Node<'a>>,
    {
        let id = self.nodes.push(kind, Span::new(start, end), flags);
        // `alloc` hands back `&mut` for a block nothing else can reference yet, so
        // the id is written before any shared reference exists. That is why nodes
        // need no interior mutability and the finished tree is `Sync`.
        let allocated: &'a mut T = self.arena.alloc(node);
        allocated.set_node_id(id);
        let allocated: &'a T = allocated;
        if self.assign_parents {
            self.record_parent_of_children(allocated.into(), id);
        }
        allocated
    }

    /// Where the most recently consumed token ended.
    pub(crate) fn node_end(&self) -> u32 {
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
        let allocated = self.arena.alloc(node);
        allocated.node_id = Some(id);
        allocated
    }

    /// Allocate a token node.
    pub(crate) fn alloc_token(&mut self, kind: SyntaxKind, span: Span) -> &'a AstToken<'a> {
        let id = self.nodes.push(kind, span, tsr_ast::NodeFlags::empty());
        let allocated = self.arena.alloc(AstToken::new(kind));
        allocated.node_id = Some(id);
        allocated
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

/// Everything [`Parser::look_ahead`] and [`Parser::try_parse`] must put back.
///
/// The diagnostic and node counts are lengths rather than saved contents: both
/// vectors only grow during a speculative attempt, so truncating restores them.
struct ParserState {
    scanner: tsr_scanner::ScannerState,
    token: Token,
    token_value: Option<String>,
    diagnostics: usize,
    nodes: usize,
}

/// Capture the scanner's decoded value, if it differs from the raw text.
fn capture_value(scanner: &Scanner<'_>) -> Option<String> {
    // Asking the scanner directly, rather than comparing `token_value` against
    // `token_text`: that comparison built both slices and compared pointers on
    // every token, to answer a question the scanner already knows the answer to.
    scanner.decoded_value().map(str::to_string)
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
