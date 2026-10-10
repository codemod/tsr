//! Parser state: the token cursor, error recovery, and node registration.

use tsr_ast::{
    HasNodeId, Identifier, NodeTable, SourceFile, Statement, SyntaxKind, Token as AstToken,
};
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
    /// `.tsx` — `<` opens JSX; type assertions must use `as`.
    Tsx,
    /// JavaScript: `.js`, `.cjs`, `.mjs` and `.jsx` — native's `ScriptKindJS`
    /// and `ScriptKindJSX`.
    ///
    /// Both are JSX-variant script kinds (`getLanguageVariant`,
    /// `parser/utilities.go:11`), so `<` opens JSX exactly as in `.tsx`:
    /// `+ <foo> bar` in a `.js` file is a JSX element, not a cast. Both also
    /// set `NodeFlagsJavaScriptFile` (`parser.go:304`), under which the parser
    /// reads no type arguments in an expression or on a JSX tag
    /// (`tryParseTypeArgumentsInExpression`, `:5247`; `:4938`).
    JavaScript,
    /// `.json`. A file is a single value, not a statement list.
    ///
    /// Not merely a dialect flag: it selects a different entry point
    /// ([`Parser::parse_json_text`]), because a JSON document has no statements.
    Json,
}

impl ScriptKind {
    /// Infer the dialect from a file name (`GetScriptKindFromFileName`,
    /// `core/core.go:527`).
    #[must_use]
    pub fn from_file_name(name: &str) -> Self {
        let extension = std::path::Path::new(name)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or_default()
            .to_ascii_lowercase();
        match extension.as_str() {
            "tsx" => Self::Tsx,
            "js" | "cjs" | "mjs" | "jsx" => Self::JavaScript,
            "json" => Self::Json,
            _ => Self::TypeScript,
        }
    }

    /// Whether `<` in expression position opens JSX: native's
    /// `LanguageVariantJSX`.
    #[must_use]
    pub const fn allows_jsx(self) -> bool {
        matches!(self, Self::Tsx | Self::JavaScript)
    }

    /// Whether the file is JavaScript (`Parser.isJavaScript`,
    /// `parser.go:153`): the parser's `NodeFlagsJavaScriptFile` context.
    #[must_use]
    pub const fn is_javascript(self) -> bool {
        matches!(self, Self::JavaScript)
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
    pub(crate) fn publish<'b>(
        &self,
        publication: &mut tsr_ast::publication::Publication<'b>,
    ) -> JSDocTable<'b> {
        use tsr_ast::publication::Publish;
        JSDocTable {
            entries: self
                .entries
                .iter()
                .map(|(host, docs)| (publication.id(*host), docs.publish(publication)))
                .collect(),
            diagnostics: self.diagnostics.clone(),
        }
    }
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
    /// With [`Self::jsdoc`], parse only the comments typescript-go parses
    /// while parsing a non-JavaScript file: `withJSDoc`
    /// (`internal/parser/jsdoc.go:56`) flags every documented node of a
    /// `.ts`/`.tsx`/`.d.ts` file and parses its comments eagerly only when
    /// one carries `@see`/`@link`/`@linkcode`/`@linkplain`; the rest wait for
    /// a lazy `Node.JSDoc` read, which only suggestion and editor paths make.
    /// The compiler driver sets it for every non-JavaScript file
    /// ([ADR-0053](../../../docs/adr/0053-jsdoc-deferred-in-checked-ts-files.md)).
    pub defer_ts_jsdoc: bool,
    /// What decides whether the file is an external module, which gates
    /// `reparseTopLevelAwait` (`parser.go:449`). The default decides on
    /// syntax alone; the program's loader sets it from the compiler options.
    /// See [`ModuleIndicatorOptions`].
    pub module_indicator: ModuleIndicatorOptions,
}

/// Which `/** … */` comments the parser parses: [`ParseOptions::jsdoc`] and
/// [`ParseOptions::defer_ts_jsdoc`] folded into one state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum JSDocMode {
    /// None.
    Off,
    /// Every comment.
    All,
    /// Only a construct's comments when one carries `@see`/`@link`
    /// (typescript-go's eager set in a non-JavaScript file).
    SeeOrLink,
}

impl Default for ParseOptions {
    fn default() -> Self {
        Self {
            script_kind: ScriptKind::TypeScript,
            parents: true,
            jsdoc: true,
            defer_ts_jsdoc: false,
            module_indicator: ModuleIndicatorOptions::default(),
        }
    }
}

impl ParseOptions {
    /// Options for a file, with the dialect inferred from its name.
    #[must_use]
    pub fn for_file(name: &str) -> Self {
        Self { script_kind: ScriptKind::from_file_name(name), ..Self::default() }
    }

    /// The same options with [`Self::defer_ts_jsdoc`] set for a program file
    /// named `name`: on unless the file is JavaScript (`ast.IsSourceFileJS`,
    /// by extension), whose JSDoc typescript-go always parses eagerly.
    #[must_use]
    pub fn deferring_ts_jsdoc(mut self, name: &str) -> Self {
        self.defer_ts_jsdoc = !matches!(
            name.rsplit_once('.').map(|(_, extension)| extension.to_ascii_lowercase()),
            Some(extension) if matches!(extension.as_str(), "js" | "jsx" | "mjs" | "cjs")
        );
        self
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

/// What decides whether a file is an external module, for the parser's own
/// use of that decision: `reparseTopLevelAwait` runs only in a module that is
/// not a declaration file (`parseSourceFileWorker`, `parser.go:449`).
///
/// Native's `ExternalModuleIndicatorOptions` (`ast/parseoptions.go:14`) plus
/// `IsDeclarationFile`. The default — no `jsx` or `force` arm, not a
/// declaration file — makes the decision on syntax alone
/// (`isFileProbablyExternalModule`), which is native's answer under
/// `moduleDetection: legacy` and for any file whose format the options do
/// not force. A caller that knows the compiler options sets the rest in
/// [`ParseOptions::module_indicator`]: `GetExternalModuleIndicatorOptions`
/// (`ast/parseoptions.go:19`) and `tspath.IsDeclarationFileName`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ModuleIndicatorOptions {
    /// `jsx: react-jsx`/`react-jsxdev` under `moduleDetection: auto`: a JSX
    /// tag makes the file a module.
    pub jsx: bool,
    /// `moduleDetection: force`, or a format-forced module
    /// (`isFileForcedToBeModuleByFormat`).
    pub force: bool,
    /// `tspath.IsDeclarationFileName(fileName)`.
    pub declaration_file: bool,
}

/// A recursive-descent parser over one source file.
// Upstream's `Parser` (`parser.go:62`) carries its state the same way: two
// parse options and several independent flags (`hasParseError`,
// `statementHasAwaitIdentifier`, the context bits) that no state machine
// would describe.
#[allow(clippy::struct_excessive_bools)]
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
    /// [`Parser::with_await_context`] or [`Parser::with_function_context`].
    ///
    /// Upstream carries `YieldContext`, `DecoratorContext` and
    /// `DisallowInContext` in the same word: [`Self::in_yield_context`] and
    /// [`Self::in_decorator_context`] are the first two, and `no_in` above is
    /// this port's counter for the third.
    pub(crate) in_await_context: bool,
    /// Upstream's `NodeFlagsDecoratorContext` bit of `Parser.contextFlags`:
    /// set while parsing the expression after `@`, cleared by
    /// `parseExpression`, argument lists, function expressions and function
    /// bodies. Its one reader is the member-expression loop, which leaves a
    /// `[` to the decorated member's computed name.
    pub(crate) in_decorator_context: bool,
    /// Upstream's `NodeFlagsYieldContext` bit of `Parser.contextFlags`
    /// (`parser.go:6365`): on in a generator's parameters and body, off in
    /// every other signature, an arrow function, a class static block, a
    /// property initializer and an enum. Set with
    /// [`Parser::with_function_context`]. `yield` is an identifier outside
    /// it unless `isYieldExpression`'s same-line lookahead says otherwise.
    pub(crate) in_yield_context: bool,
    /// The await context [`Self::with_await_context`] replaced: the one
    /// around a signature, in which `parseParametersWorker` parses each
    /// parameter's decorators (`inOuterAwaitContext`, `parser.go:3296`).
    pub(crate) outer_await_context: bool,
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
    /// Which JSDoc to parse; see [`ParseOptions::jsdoc`] and
    /// [`ParseOptions::defer_ts_jsdoc`].
    pub(crate) jsdoc_mode: JSDocMode,
    /// Whether to record parents as nodes are finished.
    assign_parents: bool,
    /// `p.sourceFlags`, stamped on the `SourceFile`. Only
    /// `NodeFlagsPossiblyContainsImportMeta` is set so far, by the
    /// `import.meta` arm (`parser.go:5195`).
    pub(crate) source_flags: tsr_ast::NodeFlags,

    /// `Parser.statementHasAwaitIdentifier` (`parser.go:83`): whether the
    /// top-level statement being parsed created an identifier spelled
    /// `await` outside the constructs that reset it (binding names, property
    /// names, function bodies, enum, namespace, import and export
    /// declarations). Read by [`Self::parse_source_file`] to collect the spans
    /// `reparseTopLevelAwait` re-reads; see
    /// [`docs/parity/notes/r7-parser.md`](../../../docs/parity/notes/r7-parser.md) §2.
    pub(crate) statement_has_await_identifier: bool,
    /// `Parser.possibleAwaitSpans` (`parser.go:92`): half-open statement
    /// index ranges of the source file, flattened into pairs.
    possible_await_spans: Vec<usize>,
    /// Each top-level statement's full start and end, in statement order:
    /// native's `Node.Pos()`/`Node.End()`, which `reparseTopLevelAwait` reads
    /// off the original statements. This port's spans start at the token, not
    /// at the leading trivia, so the full start is recorded as each statement
    /// is parsed.
    top_level_extents: Vec<(u32, u32)>,
    /// Scanner diagnostics carried over from the first parse by
    /// `reparseTopLevelAwait`; merged by [`Self::finish`] as the scanner's own.
    carried_scanner_diagnostics: Vec<Diagnostic>,
    /// What decides whether the file is an external module, which gates the
    /// top-level await reparse. See [`ModuleIndicatorOptions`].
    module_indicator: ModuleIndicatorOptions,
    /// The node table's length when this parser started: the first id of this
    /// file, to which the reparse truncates.
    first_node: usize,

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
        let first_node = nodes.len();
        let script_kind = options.script_kind;
        let mut scanner = Scanner::new(source);
        scanner.set_jsx_language_variant(script_kind.allows_jsx());
        let token = scanner.scan();
        let token_value = capture_value(&scanner);
        let mut parser = Self {
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
            in_decorator_context: false,
            in_yield_context: false,
            outer_await_context: false,
            disallow_conditional_types: 0,
            parsing_contexts: 0,
            jsdoc: Vec::new(),
            jsdoc_diagnostics: Vec::new(),
            jsdoc_mode: match (options.jsdoc, options.defer_ts_jsdoc) {
                (false, _) => JSDocMode::Off,
                (true, false) => JSDocMode::All,
                (true, true) => JSDocMode::SeeOrLink,
            },
            assign_parents: options.parents,
            depth: 0,
            source_flags: tsr_ast::NodeFlags::empty(),
            statement_has_await_identifier: false,
            possible_await_spans: Vec::new(),
            top_level_extents: Vec::new(),
            carried_scanner_diagnostics: Vec::new(),
            module_indicator: options.module_indicator,
            first_node,
        };
        parser.sync_scanner_diagnostics();
        parser
    }

    /// Consume the parser, returning its diagnostics and node table.
    #[must_use]
    pub fn finish(mut self) -> (Vec<Diagnostic>, NodeTable, JSDocTable<'a>, tsr_ast::NodeMap<'a>) {
        // The scanner's reports were moved into `self.diagnostics` as each
        // token was scanned ([`Self::sync_scanner_diagnostics`]), through the
        // same-position guard native's single sink applies (`parser.go:327`);
        // only the reparse's carried scanner reports remain separate. The
        // list is then sorted by position, and an exact repeat (same span and
        // code: a token re-scanned after a speculative parse rewound past it)
        // is folded as `SortAndDeduplicateDiagnostics` folds it. An earlier
        // version deduplicated by start alone across both lists, which also
        // dropped a parser error the scanner had not reported just before it
        // (§195's TS1127-alone case is the consecutive guard's, and still
        // holds). docs/parity/notes/r7-parser.md §7.
        self.sync_scanner_diagnostics();
        let mut diagnostics = self.diagnostics;
        diagnostics.extend(self.carried_scanner_diagnostics);
        diagnostics.sort_by_key(|d| (d.span.start, d.span.end));
        diagnostics.dedup_by(|a, b| a.span == b.span && a.message.code() == b.message.code());
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
    #[inline]
    pub(crate) fn next_token(&mut self) -> Token {
        // `Parser.nextToken` (`parser.go:381`): a keyword spelled with an
        // escape is reported as it is consumed. Identifier creation consumes
        // with `nextTokenWithoutCheck` instead ([`Self::next_token_without_check`]),
        // so `var \u0061wait` and `{ def\u0061ult: 1 }` stay silent.
        // The flag first: it is almost never set, so the common token pays
        // one test and the report stays out of line.
        if self.token.flags.contains(tsr_scanner::TokenFlags::UNICODE_ESCAPE) {
            self.report_escaped_keyword();
        }
        self.next_token_without_check()
    }

    #[cold]
    #[inline(never)]
    fn report_escaped_keyword(&mut self) {
        if self.token.kind.is_keyword() {
            self.error_at_current(&messages::KEYWORDS_CANNOT_CONTAIN_ESCAPE_CHARACTERS);
        }
    }

    /// `Parser.nextTokenWithoutCheck` (`parser.go:391`).
    pub(crate) fn next_token_without_check(&mut self) -> Token {
        let previous = self.token;
        self.token = self.scanner.scan();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
        previous
    }

    /// Route what the scanner reported while scanning the token just read
    /// through [`Self::error_at`]'s same-position guard, in report order.
    ///
    /// Native's scanner reports through the parser's own sink (`scanError`
    /// → `parseErrorAtRange`, `parser.go:318`), so one list holds both, and
    /// the guard compares a parser error with a scanner error reported just
    /// before it — and only with that one. This port's scanner keeps its own
    /// list; moving each report over as the token is scanned keeps native's
    /// order and its consecutive-only rule (`<test1 32data={32} />` keeps
    /// TS1351 at `data` beside the TS1005 at `data`, because TS1003 at `32`
    /// was reported between them). docs/parity/notes/r7-parser.md §7.
    #[inline]
    pub(crate) fn sync_scanner_diagnostics(&mut self) {
        if !self.scanner.diagnostics().is_empty() {
            self.drain_scanner_diagnostics();
        }
    }

    #[cold]
    #[inline(never)]
    fn drain_scanner_diagnostics(&mut self) {
        for diagnostic in self.scanner.take_diagnostics() {
            if !self.would_repeat_last_error(diagnostic.span) {
                self.diagnostics.push(diagnostic);
            }
        }
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
        self.sync_scanner_diagnostics();
    }

    /// Re-scan a compound `<` token as a single `<`.
    pub(crate) fn rescan_less_than(&mut self) {
        self.token = self.scanner.rescan_less_than();
        self.sync_scanner_diagnostics();
    }

    /// Re-scan a `/` as a regular expression literal.
    pub(crate) fn rescan_regular_expression(&mut self) {
        self.token = self.scanner.rescan_as_regular_expression();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
    }

    /// Re-scan the current token as JSX child content.
    pub(crate) fn rescan_jsx_token(&mut self) {
        self.token = self.scanner.rescan_jsx_token();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
    }

    /// Scan the next token as JSX child content.
    pub(crate) fn scan_jsx_token(&mut self) {
        self.token = self.scanner.scan_jsx_token();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
    }

    /// Extend the current identifier with JSX's `-`.
    pub(crate) fn scan_jsx_identifier(&mut self) {
        self.token = self.scanner.scan_jsx_identifier();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
    }

    /// Re-scan the current token as a JSX attribute value.
    pub(crate) fn rescan_jsx_attribute_value(&mut self) {
        // The scanner discards what the expression-rules scan of this token
        // reported (an unterminated string across lines); those reports
        // already moved to this list, so they are discarded here too. Native
        // never scans the value with expression rules (`scanJsxAttributeValue`
        // replaces the `nextToken` after `=`).
        let from = self.scanner.full_start();
        while self.diagnostics.last().is_some_and(|d| d.span.start >= from) {
            self.diagnostics.pop();
        }
        self.token = self.scanner.rescan_jsx_attribute_value();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
    }

    /// `reScanTemplateToken(isTaggedTemplate)` (`parser.go:3692`) for a
    /// template's head, refreshing the cached value: an untagged template is
    /// re-scanned with invalid escapes reported, which cooks a legacy octal
    /// escape to its character (`scanEscapeSequence`, `scanner.go:1700`).
    pub(crate) fn rescan_template(&mut self, is_tagged: bool) {
        self.token = self.scanner.rescan_template(is_tagged);
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
    }

    /// Re-scan a `}` as the continuation of a template literal.
    pub(crate) fn rescan_template_continuation(&mut self) {
        self.token = self.scanner.rescan_template_continuation();
        self.token_value = capture_value(&self.scanner);
        self.sync_scanner_diagnostics();
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
        let saved_outer = std::mem::replace(&mut self.outer_await_context, saved);
        let result = f(self);
        self.outer_await_context = saved_outer;
        self.in_await_context = saved;
        result
    }

    /// Run `f` in a signature's own context: `parseParametersWorker`'s and
    /// `parseFunctionBlock`'s `setContextFlags(YieldContext, flags&Yield)` /
    /// `setContextFlags(AwaitContext, flags&Await)` (`parser.go:3298`,
    /// `:3498`), restored after. [`Self::with_await_context`] is the await
    /// half alone, for the places native sets only that bit.
    pub(crate) fn with_function_context<T>(
        &mut self,
        is_await: bool,
        is_yield: bool,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let saved_yield = std::mem::replace(&mut self.in_yield_context, is_yield);
        let result = self.with_await_context(is_await, f);
        self.in_yield_context = saved_yield;
        result
    }

    fn save_state(&self) -> ParserState {
        ParserState {
            scanner: self.scanner.save(),
            token: self.token,
            token_value: self.token_value.clone(),
            diagnostics: self.diagnostics.len(),
            nodes: self.nodes.len(),
            statement_has_await_identifier: self.statement_has_await_identifier,
        }
    }

    fn restore_state(&mut self, saved: ParserState) {
        self.scanner.restore(saved.scanner);
        self.token = saved.token;
        self.token_value = saved.token_value;
        self.statement_has_await_identifier = saved.statement_has_await_identifier;
        // Guarded like `Scanner::restore`'s truncate (r5-binperf.md §5).
        if saved.diagnostics < self.diagnostics.len() {
            self.diagnostics.truncate(saved.diagnostics);
        }
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

    /// The context flags `finishNode` ORs into every node (`parser.go`'s
    /// `node.Flags |= p.contextFlags`). `NodeFlagsAwaitContext` and
    /// `NodeFlagsYieldContext` are tracked: the checker reads the first on a
    /// top-level `await` (`checkGrammarAwaitOrAwaitUsing`,
    /// `grammarchecks.go:1690`) and on an `await` identifier
    /// (`checkContextualIdentifier`, `binder.go:1311`).
    #[inline]
    fn context_node_flags(&self) -> tsr_ast::NodeFlags {
        let mut flags = tsr_ast::NodeFlags::empty();
        if self.in_await_context {
            flags |= tsr_ast::NodeFlags::AWAIT_CONTEXT;
        }
        if self.in_yield_context {
            flags |= tsr_ast::NodeFlags::YIELD_CONTEXT;
        }
        flags
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
        let id = self.nodes.push(kind, Span::new(start, end), flags | self.context_node_flags());
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
            self.nodes.push(SyntaxKind::Identifier, Span::at(start), self.context_node_flags());
        let allocated = self.arena.alloc(node);
        allocated.node_id = Some(id);
        let allocated: &'a Identifier<'a> = allocated;
        self.node_map.push(allocated.into());
        allocated
    }

    /// Allocate a token node.
    pub(crate) fn alloc_token(&mut self, kind: SyntaxKind, span: Span) -> &'a AstToken<'a> {
        let id = self.nodes.push(kind, span, self.context_node_flags());
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

    /// Parse a whole file — `parseSourceFileWorker` (`parser.go:428`),
    /// including its `reparseTopLevelAwait` step (`parser.go:449`).
    pub fn parse_source_file(&mut self) -> &'a SourceFile<'a> {
        let start = self.pos();
        let statements = self.parse_top_level_statements();
        let file = self.finish_source_file(start, &statements);
        if self.possible_await_spans.is_empty()
            || self.module_indicator.declaration_file
            || !self.is_external_module(file)
        {
            return file;
        }
        self.reparse_top_level_await(start)
    }

    /// `parseListIndex(PCSourceElements, parseToplevelStatement)`
    /// (`parser.go:434`, `:497`): the statement list, recording which
    /// statements created an `await` identifier.
    fn parse_top_level_statements(&mut self) -> Vec<Statement<'a>> {
        let mut index = 0;
        self.parse_list(crate::list::ParsingContext::SourceElements, |parser| {
            let i = index;
            index += 1;
            parser.parse_toplevel_statement(i)
        })
    }

    /// `parseToplevelStatement` (`parser.go:497`). This port has no reparse
    /// list (JSDoc `@typedef` is bound off the comment, not spliced into the
    /// statements), so `i` needs no adjustment. Native's second condition,
    /// `statement.Flags&NodeFlagsAwaitContext == 0`, always holds here: the
    /// first parse of a top-level statement is never in an await context.
    fn parse_toplevel_statement(&mut self, i: usize) -> Statement<'a> {
        self.statement_has_await_identifier = false;
        let full_start = self.node_end();
        let statement = self.parse_statement();
        self.top_level_extents.push((full_start, self.statement_end(statement)));
        if self.statement_has_await_identifier {
            match self.possible_await_spans.last_mut() {
                Some(last) if *last == i => *last = i + 1,
                _ => self.possible_await_spans.extend([i, i + 1]),
            }
        }
        statement
    }

    /// `Node.End()` of a statement just parsed.
    fn statement_end(&self, statement: Statement<'a>) -> u32 {
        statement.node_id().map_or_else(|| self.node_end(), |id| self.nodes.span(id).end)
    }

    /// The end-of-file token and the `SourceFile` node over `statements`.
    fn finish_source_file(
        &mut self,
        start: u32,
        statements: &[Statement<'a>],
    ) -> &'a SourceFile<'a> {
        // Trailing comments document the end-of-file token
        // (`parseSourceFileWorker`'s `withJSDoc(eof, endJSDoc)`, `parser.go:438`):
        // its `@typedef`/`@callback`/`@import` declarations are reparsed into
        // the file's statements like any other host's.
        let end_docs = self.parse_leading_jsdoc();
        let eof = self.alloc_token(SyntaxKind::EndOfFile, self.token.span);
        self.attach_jsdoc(tsr_ast::Node::from(eof), end_docs);
        let file = self.finish_node(
            SourceFile::new(self.arena.alloc_slice(statements), eof),
            SyntaxKind::SourceFile,
            start,
        );
        if !self.source_flags.is_empty()
            && let Some(id) = file.node_id
        {
            self.nodes.add_flags(id, self.source_flags);
        }
        file
    }

    /// `result.ExternalModuleIndicator != nil` after `finishSourceFile`
    /// (`getExternalModuleIndicator`, `ast/parseoptions.go:60`).
    fn is_external_module(&self, file: &SourceFile<'a>) -> bool {
        if crate::references::is_file_probably_external_module(file, &self.nodes) {
            return true;
        }
        (self.module_indicator.jsx && crate::references::contains_jsx_tag(file))
            || self.module_indicator.force
    }

    /// `reparseTopLevelAwait` (`parser.go:514`).
    ///
    /// Native re-reads each span of statements that created an `await`
    /// identifier in an await context, splices the new statements between the
    /// untouched originals, keeps the original diagnostics outside the spans
    /// and the new ones inside them. The new nodes are created after the
    /// whole first tree, and the originals they replace stay allocated.
    ///
    /// This port cannot leave replaced nodes behind: a file's nodes are one
    /// contiguous id range that consumers scan (`file.node_range()`), and an
    /// orphaned statement there would be found by position. So the reparse
    /// **re-parses the whole file** into the same id range, reproducing the
    /// copied statements by parsing them again (the parse is deterministic
    /// and they are parsed in the same context), and runs native's span loop
    /// at the same positions. Diagnostics follow native exactly: outside the
    /// spans they are the first parse's, filtered by position as native's
    /// `FindIndex` slices are; inside, the reparse's. See
    /// [`docs/parity/notes/r7-parser.md`](../../../docs/parity/notes/r7-parser.md) §2.
    fn reparse_top_level_await(&mut self, start: u32) -> &'a SourceFile<'a> {
        let spans = std::mem::take(&mut self.possible_await_spans);
        let original = std::mem::take(&mut self.top_level_extents);
        debug_assert!(spans.len() % 2 == 0, "possibleAwaitSpans malformed");
        let saved_scanner = self.scanner.take_diagnostics();
        let saved_parser = std::mem::take(&mut self.diagnostics);
        let source_flags = self.source_flags;
        self.restart();
        self.source_flags = source_flags;

        // The diagnostics of the first parse whose position lies in
        // `[from, to)` — native's two `FindIndex` slices over its one list.
        let mut scanner_out = Vec::new();
        let mut parser_out = Vec::new();
        let keep = |from: u32,
                    to: u32,
                    scanner_out: &mut Vec<Diagnostic>,
                    parser_out: &mut Vec<Diagnostic>| {
            let inside = |d: &&Diagnostic| d.span.start >= from && d.span.start < to;
            scanner_out.extend(saved_scanner.iter().filter(inside).cloned());
            parser_out.extend(saved_parser.iter().filter(inside).cloned());
        };
        // Index ranges of the reparse's own diagnostics, kept wholesale.
        let mut windows: Vec<(usize, usize, usize, usize)> = Vec::new();

        let n = original.len();
        let mut statements: Vec<Statement<'a>> = Vec::with_capacity(n);
        let mut after_await_statement = 0;
        let mut i = 0;
        while i < spans.len() {
            let next_await_statement = spans[i];
            let from = original[after_await_statement].0;
            let to = original[next_await_statement].0;
            self.parse_top_level_statements_until(to, &mut statements);
            keep(from, to, &mut scanner_out, &mut parser_out);

            let scanner_mark = self.scanner.diagnostics().len();
            let parser_mark = self.diagnostics.len();
            after_await_statement = spans[i + 1];
            let saved_await = std::mem::replace(&mut self.in_await_context, true);
            while !self.at(SyntaxKind::EndOfFile) {
                let start_pos = self.node_end();
                let statement = self.parse_statement();
                statements.push(statement);
                if start_pos == self.node_end() {
                    self.next_token();
                }
                if after_await_statement < n {
                    let last_end = original[after_await_statement - 1].1;
                    let end = self.statement_end(statement);
                    if end == last_end {
                        // Done reparsing this section.
                        break;
                    }
                    if end > last_end {
                        // Ate into the next statement: continue with the next span.
                        i += 2;
                        after_await_statement = if i < spans.len() { spans[i + 1] } else { n };
                    }
                }
            }
            self.in_await_context = saved_await;
            windows.push((
                scanner_mark,
                self.scanner.diagnostics().len(),
                parser_mark,
                self.diagnostics.len(),
            ));
            i += 2;
        }
        if after_await_statement < n {
            let from = original[after_await_statement].0;
            self.parse_top_level_statements_until(u32::MAX, &mut statements);
            keep(from, u32::MAX, &mut scanner_out, &mut parser_out);
        }

        let reparse_scanner = self.scanner.take_diagnostics();
        let reparse_parser = std::mem::take(&mut self.diagnostics);
        for &(scanner_from, scanner_to, parser_from, parser_to) in &windows {
            scanner_out.extend_from_slice(&reparse_scanner[scanner_from..scanner_to]);
            parser_out.extend_from_slice(&reparse_parser[parser_from..parser_to]);
        }
        self.carried_scanner_diagnostics = scanner_out;
        self.diagnostics = parser_out;
        self.finish_source_file(start, &statements)
    }

    /// The source-element list from the cursor up to the statement whose full
    /// start is `stop` (`u32::MAX`: end of file), appended to `statements`.
    /// Used by the reparse to reproduce native's copied statements: the list
    /// loop reaches the token at `stop` exactly where the first parse began
    /// that statement.
    fn parse_top_level_statements_until(&mut self, stop: u32, statements: &mut Vec<Statement<'a>>) {
        let kind = crate::list::ParsingContext::SourceElements;
        let saved = self.parsing_contexts;
        self.parsing_contexts |= kind.bit();
        while self.node_end() < stop && !self.is_list_terminator(kind) {
            if self.is_list_element(kind, false) {
                let before = self.pos();
                statements.push(self.parse_statement());
                if self.pos() == before && !self.at(SyntaxKind::EndOfFile) {
                    self.next_token();
                }
                continue;
            }
            if self.abort_parsing_list_or_move_to_next_token(kind) {
                break;
            }
        }
        self.parsing_contexts = saved;
    }

    /// Rewind to the start of the file with empty tables, as a fresh parser
    /// over the same tables would be.
    fn restart(&mut self) {
        self.nodes.truncate(self.first_node);
        self.node_map.truncate(self.first_node);
        self.jsdoc.clear();
        self.jsdoc_diagnostics.clear();
        let mut scanner = Scanner::new(self.source);
        scanner.set_jsx_language_variant(self.script_kind.allows_jsx());
        self.token = scanner.scan();
        self.token_value = capture_value(&scanner);
        self.scanner = scanner;
        self.sync_scanner_diagnostics();
        self.no_in = 0;
        self.in_await_context = false;
        self.in_decorator_context = false;
        self.in_yield_context = false;
        self.outer_await_context = false;
        self.disallow_conditional_types = 0;
        self.parsing_contexts = 0;
        self.depth = 0;
        self.statement_has_await_identifier = false;
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
    /// Part of native's `ParserState` (`parser.go:345`).
    statement_has_await_identifier: bool,
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
