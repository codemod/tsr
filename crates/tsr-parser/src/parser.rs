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
    /// What the parser objected to inside the comments — upstream's
    /// `SourceFile.JSDocDiagnostics()` (`parser.go:468`).
    diagnostics: Vec<Diagnostic>,
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

    /// The parse errors inside this file's JSDoc comments, in source order and
    /// once each — upstream's `SourceFile.JSDocDiagnostics()`.
    ///
    /// Collected for every file because this parser does not know whether a
    /// file is JavaScript (`ScriptKind` has no JS arm); only a checked
    /// JavaScript file reports them (`getBindAndCheckDiagnosticsWithChecker`,
    /// `program.go:1366`), and that gate is the consumer's.
    #[must_use]
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
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
    /// Every node, by id — see [`tsr_ast::NodeMap`]. Filled here because the
    /// parser is the only stage where it costs a `push` rather than a walk.
    pub(crate) node_map: tsr_ast::NodeMap<'a>,
    /// Non-zero while `in` must not be treated as a binary operator.
    ///
    /// `for (a in b)` would otherwise consume `a in b` as a comparison and leave
    /// the loop header malformed. A counter rather than a bool because the
    /// restriction nests: `for ((a in b);;)` re-enables it inside the parens.
    pub(crate) no_in: u32,
    /// Whether `await` is a keyword here rather than an identifier.
    ///
    /// Upstream's `NodeFlagsAwaitContext` bit of `Parser.contextFlags`
    /// (`parser.go:6380`). A bool with save-and-restore rather than a counter,
    /// because the context is *set to a value* at each boundary and not merely
    /// pushed: a non-async function nested inside an async one turns it back
    /// **off**, which a counter cannot express. Use
    /// [`Parser::with_await_context`].
    ///
    /// Only `await` is tracked. Upstream carries `YieldContext` and
    /// `DisallowInContext` in the same word; `no_in` below is this port's
    /// counter for the third, and the yield context has no reader here yet —
    /// `is_binding_identifier` is upstream's own context-free test, and
    /// `isYieldExpression`'s context half is unported. §193.
    pub(crate) in_await_context: bool,
    /// Non-zero while a nested type may not consume a conditional `extends`.
    ///
    /// The extends-side of a conditional type uses this to resolve
    /// `infer U extends T ? X : Y`: there `extends T` constrains `U`, while in
    /// `(infer U extends T ? X : Y)` it belongs to the parenthesized
    /// conditional type.
    pub(crate) disallow_conditional_types: u32,
    /// One bit per list being parsed — upstream's `Parser.parsingContexts`,
    /// read by `isInSomeParsingContext`. See `list.rs`.
    pub(crate) parsing_contexts: u32,
    /// JSDoc comments, keyed by the node they document.
    ///
    /// A side table rather than a field on each node, per ADR-0003: JSDoc is
    /// absent from the overwhelming majority of nodes, and a field would cost
    /// every node a pointer to carry information a handful of them use.
    pub(crate) jsdoc: Vec<(tsr_ast::NodeId, &'a [&'a tsr_ast::JSDoc<'a>])>,
    /// Parse errors inside JSDoc comments — upstream's `Parser.jsdocDiagnostics`.
    /// Like upstream's, not rewound by speculation; `finish` drops repeats.
    pub(crate) jsdoc_diagnostics: Vec<Diagnostic>,
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
        // One node per ~10 bytes is what the corpus shows: checker.ts is 3.15 MB
        // for 304,884 nodes, dom.generated.d.ts 2.35 MB for 124,103. Estimating
        // low would reintroduce the growth this avoids, so this takes the denser
        // of the two.
        let estimate = source.len() / 10;
        Self::with_tables(
            arena,
            source,
            options,
            NodeTable::with_capacity(estimate),
            tsr_ast::NodeMap::with_capacity(estimate),
        )
    }

    /// Create a parser that **appends** to tables another file has already
    /// written to.
    ///
    /// This is what makes a [`NodeId`](tsr_ast::NodeId) unique across a whole
    /// program rather than only within one file: `NodeTable::push` hands out
    /// `self.len()`, so a table that arrives non-empty continues the numbering
    /// instead of restarting it. Nothing else has to change for that to hold —
    /// in particular speculative rollback still works, because
    /// `restore_state` truncates to an *absolute* length it recorded on the way
    /// in, not to zero.
    ///
    /// The two tables are taken and returned by value rather than borrowed for
    /// the parser's lifetime, because [`Parser`] owns them while it runs; see
    /// [`crate::parse_into`], which threads them for the caller.
    ///
    /// Every file's nodes therefore occupy one **contiguous** id range, which is
    /// the property that lets the file behind an id be recovered without a
    /// per-node column — see [`crate::ParsedInto::node_range`].
    #[must_use]
    pub fn with_tables(
        arena: &'a Arena,
        source: &'a str,
        options: ParseOptions,
        mut nodes: NodeTable,
        mut node_map: tsr_ast::NodeMap<'a>,
    ) -> Self {
        let estimate = source.len() / 10;
        nodes.reserve(estimate);
        node_map.reserve(estimate);
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
            nodes,
            node_map,
            no_in: 0,
            // False at the top level. Upstream turns it on for a file it has
            // decided is an external module (`parser.go:554`), which is what
            // makes top-level `await` legal there; this port does not make that
            // decision in the parser, so a top-level `await` outside a function
            // still goes through `isAwaitExpression`'s lookahead half. §193.
            in_await_context: false,
            disallow_conditional_types: 0,
            parsing_contexts: 0,
            jsdoc: Vec::new(),
            jsdoc_diagnostics: Vec::new(),
            parse_jsdoc: options.jsdoc,
            assign_parents: options.parents,
            depth: 0,
        }
    }

    /// Consume the parser, returning its diagnostics and node table.
    #[must_use]
    pub fn finish(mut self) -> (Vec<Diagnostic>, NodeTable, JSDocTable<'a>, tsr_ast::NodeMap<'a>) {
        // Scanner diagnostics are interleaved by position so a caller sees one
        // ordered list rather than two.
        //
        // **And `parseErrorAtRange`'s same-position guard is applied across
        // both** (`parser.go:327`). Upstream has a single list — the scanner's
        // error callback routes through the same function — so its guard
        // compares across the two sources by construction. Here the scanner
        // owns a `Vec` and the guard in `error_at` can only see the parser's,
        // which left a parser error surviving at a position the scanner had
        // already reported: `parserErrorRecovery_Block2` wants `TS1127` alone
        // and got `TS1012` beside it. §195.
        //
        // The scanner's entries go first at an equal start because upstream
        // keeps whichever was reported **first**, and the scanner reports while
        // scanning the token — before the parser can say anything about it.
        // Tagged before sorting rather than counted during it: `sort_by_key`
        // calls its key function an unpredictable number of times, so a
        // positional counter inside one is not a source ordinal.
        let mut tagged: Vec<(u8, Diagnostic)> =
            self.scanner.take_diagnostics().into_iter().map(|d| (0, d)).collect();
        tagged.extend(self.diagnostics.into_iter().map(|d| (1, d)));
        tagged.sort_by_key(|(source, d)| (d.span.start, *source, d.span.end));
        let mut diagnostics: Vec<Diagnostic> = tagged.into_iter().map(|(_, d)| d).collect();
        diagnostics.dedup_by_key(|d| d.span.start);
        let mut jsdoc_diagnostics = self.jsdoc_diagnostics;
        // A comment re-read after a speculative parse rewinds reports again;
        // upstream's `SortAndDeduplicateDiagnostics` folds the repeats.
        jsdoc_diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        jsdoc_diagnostics.dedup_by(|a, b| a.span == b.span && a.message.code() == b.message.code());
        let jsdoc = JSDocTable { entries: self.jsdoc, diagnostics: jsdoc_diagnostics };
        (diagnostics, self.nodes, jsdoc, self.node_map)
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

    /// Whether an `async` modifier is among these.
    ///
    /// The `ParseFlagsAwait` half of upstream's `signatureFlags`
    /// (`parser.go:2500`), which every signature computes from its own
    /// modifiers before parsing its parameters and body.
    pub(crate) fn is_async(modifiers: &[tsr_ast::ModifierLike<'_>]) -> bool {
        modifiers.iter().any(|modifier| {
            matches!(modifier, tsr_ast::ModifierLike::Token(token) if token.kind == tsr_ast::SyntaxKind::AsyncKeyword)
        })
    }

    /// Run `f` with [`Self::in_await_context`] set to `value`, restoring it after.
    ///
    /// Upstream's `saveContextFlags := p.contextFlags` /
    /// `p.setContextFlags(ast.NodeFlagsAwaitContext, …)` /
    /// `p.contextFlags = saveContextFlags` triple, which appears at every
    /// signature, function body, arrow body, class static block and enum body
    /// in `parser.go`.
    pub(crate) fn with_await_context<T>(
        &mut self,
        value: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let saved = std::mem::replace(&mut self.in_await_context, value);
        let result = f(self);
        self.in_await_context = saved;
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
        // In lockstep, or every id after the abandoned attempt names a different
        // node in each table.
        self.node_map.truncate(saved.nodes);
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
        if self.would_repeat_last_error(span) {
            return;
        }
        self.diagnostics.push(Diagnostic::with_args(
            message,
            span,
            args.iter().map(|s| (*s).to_string()),
        ));
    }

    /// Report at an explicit span.
    pub(crate) fn error_at(&mut self, message: &'static Message, span: Span) {
        if self.would_repeat_last_error(span) {
            return;
        }
        self.diagnostics.push(Diagnostic::new(message, span));
    }

    /// `parseErrorAtRange`'s guard (`parser.go:327`):
    ///
    /// > *Don't report another error if it would just be at the same location
    /// > as the last error.*
    ///
    /// Every parser diagnostic upstream goes through `parseErrorAtRange`, so
    /// this is a property of the **sink** and not of any reporting site. Two
    /// recoveries firing in sequence at one token is normal and correct — a
    /// statement's missing `;` and then the statement list's refusal of the
    /// same token — and upstream keeps the first only.
    ///
    /// It compares the **start** and not the whole span, which is upstream's
    /// `Pos()`: a longer or shorter range at the same start is still the same
    /// location. See `checker-notes-diag2.md` §192.
    pub(crate) fn would_repeat_last_error(&self, span: Span) -> bool {
        self.diagnostics.last().is_some_and(|last| last.span.start == span.start)
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
        let node: tsr_ast::Node<'a> = allocated.into();
        // Immediately after `NodeTable::push`, so the map's entry `n` is the node
        // whose id is `n`. That ordering is the whole reason this is a `push`
        // rather than an indexed store into a zeroed vector.
        self.node_map.push(node);
        if self.assign_parents {
            self.record_parent_of_children(node, id);
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
        let allocated: &'a Identifier<'a> = allocated;
        self.node_map.push(allocated.into());
        allocated
    }

    /// Allocate a token node.
    pub(crate) fn alloc_token(&mut self, kind: SyntaxKind, span: Span) -> &'a AstToken<'a> {
        let id = self.nodes.push(kind, span, tsr_ast::NodeFlags::empty());
        let allocated = self.arena.alloc(AstToken::new(kind));
        allocated.node_id = Some(id);
        let allocated: &'a AstToken<'a> = allocated;
        self.node_map.push(allocated.into());
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
        let statements = self.parse_statement_list(crate::list::ParsingContext::SourceElements);
        // Trailing comments document the end-of-file token
        // (`parseSourceFileWorker`'s `withJSDoc(eof, endJSDoc)`, `parser.go:438`).
        // Parsed for their diagnostics; not yet attached, because a bound
        // end-of-file `@typedef` meets the checker's unfinished typedef alias
        // bodies (docs/parity/notes/js.md).
        let _end_docs = self.parse_leading_jsdoc();
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
