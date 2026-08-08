//! The TypeScript parser.
//!
//! Ported from typescript-go's `internal/parser/parser.go`.
//!
//! **Status:** 4,999/5,031 on `parser_typescript` (99.36%). This header
//! previously read "WORK IN PROGRESS — DOES NOT COMPILE ... the conformance
//! harness still reports the parser suite at 0%, which is accurate", and had
//! been wrong for some time. Correcting it rather than deleting it, because a
//! doc comment that overstates *failure* misleads in the same way as one that
//! overstates success: it is why the pragma work in [`pragma`] nearly went
//! looking for a different crate to live in.
//!
//! # Shape
//!
//! A hand-written recursive-descent parser over [`tsr_scanner::Scanner`],
//! allocating into a [`tsr_core::Arena`]. Recursive descent rather than a
//! generated parser because TypeScript's grammar is not context-free in the
//! places that matter — arrow functions versus parenthesised expressions, `<` as
//! a type-argument list versus a comparison — and every real implementation
//! resolves those by backtracking, which a table-driven parser cannot express.
//!
//! # Error recovery
//!
//! The parser never fails. Every entry point returns a tree plus a diagnostic
//! list, because the language service must work on incomplete source: a user
//! halfway through typing a declaration still expects completions. Missing nodes
//! are synthesised so the tree stays walkable.
//!
//! # What is not here yet
//!
//! Decorators, `JSX`, and `JSDoc` parsing are not implemented; see
//! `docs/architecture/parser.md` for the full list and the conformance number.

mod declaration;
mod expression;
mod jsdoc;
mod json;
mod jsx;
mod module;
mod parsed_file;
mod parser;
pub mod pragma;
pub mod references;
mod statement;
mod types;

pub use parsed_file::ParsedFile;
pub use parser::{JSDocTable, ParseOptions, ParseResult, Parser, ScriptKind};
pub use pragma::{
    CheckJsDirective, FileReference, FileReferences, ResolutionMode, parse_file_references,
};
pub use references::{
    CollectOptions, ExternalModuleReferences, ModuleSpecifier, SpecifierContext,
    collect_external_module_references, contains_jsx_tag, is_file_probably_external_module,
};

use tsr_ast::SourceFile;
use tsr_core::Arena;
use tsr_diagnostics::Diagnostic;

/// Parse a source file.
///
/// Never fails: a syntactically invalid file still yields a tree, alongside the
/// diagnostics explaining what was wrong.
///
/// ```
/// # use tsr_core::Arena;
/// let arena = Arena::new();
/// let result = tsr_parser::parse(&arena, "export const x: number = 1;");
/// assert!(result.diagnostics.is_empty());
/// assert_eq!(result.source_file.statements.len(), 1);
/// ```
#[must_use]
pub fn parse<'a>(arena: &'a Arena, source: &'a str) -> ParsedSourceFile<'a> {
    parse_with_script_kind(arena, source, ScriptKind::TypeScript)
}

/// Parse a source file in a specific dialect.
///
/// `.tsx` reads a leading `<` as JSX; `.ts` reads it as a type assertion. See
/// [`ScriptKind`].
#[must_use]
pub fn parse_with_script_kind<'a>(
    arena: &'a Arena,
    source: &'a str,
    script_kind: ScriptKind,
) -> ParsedSourceFile<'a> {
    parse_with_options(arena, source, ParseOptions { script_kind, ..Default::default() })
}

/// Parse a source file with explicit options.
///
/// The one that matters is [`ParseOptions::jsdoc`]: turning it off skips building
/// the JSDoc side table, which on a documentation-dense file is most of the work.
#[must_use]
pub fn parse_with_options<'a>(
    arena: &'a Arena,
    source: &'a str,
    options: ParseOptions,
) -> ParsedSourceFile<'a> {
    // Defined as [`parse_into`] over a fresh pair of tables, rather than as a
    // second copy of the same six lines. The two must not drift: they are the
    // single-file and multi-file spellings of one operation, and a difference
    // between them would be a difference in what a *program* parses versus what
    // a suite measures. Costs nothing — `Vec::new()` then `reserve(n)` reaches
    // the same capacity as `with_capacity(n)`.
    let mut nodes = tsr_ast::NodeTable::new();
    let mut node_map = tsr_ast::NodeMap::new();
    let parsed = parse_into(arena, source, options, &mut nodes, &mut node_map);
    ParsedSourceFile {
        source_file: parsed.source_file,
        diagnostics: parsed.diagnostics,
        nodes,
        node_map,
        jsdoc: parsed.jsdoc,
        file_references: parsed.file_references,
    }
}

/// Parse a file **into tables that other files of the same program share**.
///
/// The difference from [`parse_with_options`] is identity, not parsing: the node
/// table and node map are borrowed rather than created, so
/// [`NodeId`](tsr_ast::NodeId)s continue the numbering instead of restarting at
/// zero and are unique across every file parsed into the same pair. That is what
/// [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md)
/// calls widening the identity space, and it is the precondition for one file
/// resolving a name declared in another.
///
/// **Every file must be parsed into the same arena**, since the map holds
/// `Node<'a>` borrowed from it — which is why `'a` ties the arena, the source
/// and the map together in the signature. The type system enforces it; there is
/// no runtime check and none is needed.
///
/// [`parse_with_options`] is unchanged and still the right entry point for
/// anything reading one file on its own — the printer, the parse-only
/// conformance suites, a language service opening a single document. It is
/// exactly this function with a fresh pair of tables.
pub fn parse_into<'a>(
    arena: &'a Arena,
    source: &'a str,
    options: ParseOptions,
    nodes: &mut tsr_ast::NodeTable,
    node_map: &mut tsr_ast::NodeMap<'a>,
) -> ParsedInto<'a> {
    let first_node = u32::try_from(nodes.len()).expect("node count exceeds u32");
    // Taken out and put back rather than borrowed for the parser's lifetime:
    // `Parser` owns its tables while it runs, and `finish` already hands them
    // back. Both types are `Default`, so the swap costs nothing.
    let mut parser = Parser::with_tables(
        arena,
        source,
        options,
        std::mem::take(nodes),
        std::mem::take(node_map),
    );
    let source_file = if options.script_kind == ScriptKind::Json {
        parser.parse_json_text()
    } else {
        parser.parse_source_file()
    };
    let (diagnostics, node_table, jsdoc, map) = parser.finish();
    *nodes = node_table;
    *node_map = map;
    let end_node = u32::try_from(nodes.len()).expect("node count exceeds u32");

    ParsedInto {
        source_file,
        diagnostics,
        jsdoc,
        file_references: pragma::parse_file_references(source),
        node_range: first_node..end_node,
    }
}

/// Parse a standalone type, appending its nodes to an existing table.
///
/// Made for JSDoc `{type}` texts: declaration emit parses the braced text and
/// grafts the subtree into an already-parsed file's tree, which is sound
/// because the new ids continue the table exactly as [`parse_into`]'s
/// multi-file ranges do, and the arena is the file's own. `source` must be
/// *positioned*: the type text sits at its original file offset with the
/// preceding bytes present (whitespace padding works), so every span lands
/// inside the file. Returns `None` when the text does not parse cleanly or
/// does not consume the whole input.
///
/// The node map produced while parsing is discarded, so program-level id→node
/// recovery does not cover the grafted subtree; declaration emit never asks
/// it to.
#[must_use]
pub fn parse_standalone_type<'a>(
    arena: &'a Arena,
    source: &'a str,
    nodes: &mut tsr_ast::NodeTable,
) -> Option<tsr_ast::TypeNode<'a>> {
    let options =
        ParseOptions { script_kind: ScriptKind::TypeScript, jsdoc: false, ..Default::default() };
    let mut parser =
        Parser::with_tables(arena, source, options, std::mem::take(nodes), tsr_ast::NodeMap::new());
    let r#type = parser.parse_type();
    let consumed = parser.at(tsr_ast::SyntaxKind::EndOfFile);
    let (diagnostics, node_table, _, _) = parser.finish();
    *nodes = node_table;
    (diagnostics.is_empty() && consumed).then_some(r#type)
}

/// One file parsed into a program's shared tables, by [`parse_into`].
///
/// Everything [`ParsedSourceFile`] carries except the two tables, which the
/// caller now owns, plus the range of ids this file claimed in them.
pub struct ParsedInto<'a> {
    /// The root of this file's tree.
    pub source_file: &'a SourceFile<'a>,
    /// Everything the scanner and parser objected to, in source order.
    pub diagnostics: Vec<Diagnostic>,
    /// JSDoc comments, keyed by the node they document.
    pub jsdoc: JSDocTable<'a>,
    /// What the file's `///`-directive preamble declared.
    pub file_references: FileReferences,
    /// The half-open range of node ids this file's parse produced.
    ///
    /// **Contiguous, and that is load-bearing.** Files are parsed one at a time,
    /// so a file's nodes are one unbroken run of ids — which means the file
    /// behind an id is recoverable by binary search over a handful of
    /// boundaries, rather than by a per-node file column costing 4 bytes on
    /// every node in the program. A `Span` stays an offset into its *own*
    /// file's text, so something has to answer "which text", and this is what
    /// answers it.
    ///
    /// Empty when the file produced no nodes at all.
    pub node_range: std::ops::Range<u32>,
}

/// The result of parsing one file.
pub struct ParsedSourceFile<'a> {
    /// The root of the tree.
    pub source_file: &'a SourceFile<'a>,
    /// Everything the scanner and parser objected to, in source order.
    pub diagnostics: Vec<Diagnostic>,
    /// Kind, span, and parent for every node the parser registered.
    pub nodes: tsr_ast::NodeTable,
    /// The typed node behind each id — the way back into the tree.
    ///
    /// See [`tsr_ast::NodeMap`] and
    /// [ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md).
    pub node_map: tsr_ast::NodeMap<'a>,
    /// JSDoc comments, keyed by the node they document.
    pub jsdoc: JSDocTable<'a>,
    /// What the file's `///`-directive preamble declared.
    ///
    /// These add files to the *program*, so the file loader reads them the same
    /// way it reads an `import`. See [`pragma`].
    pub file_references: FileReferences,
}

#[cfg(test)]
mod tests {
    use tsr_ast::{NodeMap, NodeTable};

    use super::*;

    /// Parse several sources into one pair of tables, as a program does.
    fn parse_program<'a>(
        arena: &'a Arena,
        sources: &[&'a str],
        nodes: &mut NodeTable,
        node_map: &mut NodeMap<'a>,
    ) -> Vec<ParsedInto<'a>> {
        sources
            .iter()
            .map(|source| parse_into(arena, source, ParseOptions::default(), nodes, node_map))
            .collect()
    }

    #[test]
    fn a_second_file_continues_the_numbering_instead_of_restarting_it() {
        // The whole point. Parsed separately, both files would call their first
        // node `NodeId(0)` and a symbol from one would silently name a node in
        // the other — the unsoundness ADR-0034 is about.
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut node_map = NodeMap::new();
        let parsed =
            parse_program(&arena, &["const a = 1;", "const b = 2;"], &mut nodes, &mut node_map);

        assert_eq!(parsed[0].node_range.start, 0);
        assert_eq!(
            parsed[0].node_range.end, parsed[1].node_range.start,
            "the second file starts where the first stopped"
        );
        assert!(parsed[1].node_range.end > parsed[1].node_range.start, "and it produced nodes");
        assert_eq!(nodes.len(), parsed[1].node_range.end as usize, "the table holds both files");
    }

    #[test]
    fn both_files_nodes_are_readable_from_the_one_map() {
        // Identity is only useful if the id still reaches the *right* node.
        //
        // **This test was rewritten after a mutation survived it.** It first
        // checked `node_map.get(id).node_id() == id` over each file's range,
        // which a map holding only the last file passes: that map is internally
        // consistent, it is simply the wrong map. So the assertion now names
        // content that only one of the two files contains, which no single-file
        // map can satisfy for both.
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut node_map = NodeMap::new();
        let parsed = parse_program(
            &arena,
            &["const alpha = 1;", "const beta = 2; const gamma = 3;"],
            &mut nodes,
            &mut node_map,
        );

        let names_in = |range: &std::ops::Range<u32>| -> Vec<String> {
            range
                .clone()
                .filter_map(|id| node_map.get(tsr_ast::NodeId::new(id)))
                .filter_map(|node| match node {
                    tsr_ast::Node::Identifier(identifier) => Some(identifier.text.to_string()),
                    _ => None,
                })
                .collect()
        };
        assert_eq!(names_in(&parsed[0].node_range), ["alpha"]);
        assert_eq!(names_in(&parsed[1].node_range), ["beta", "gamma"]);
        assert_eq!(node_map.len(), nodes.len(), "one map, both files, no gaps");
    }

    #[test]
    fn each_files_nodes_are_one_contiguous_run() {
        // What lets the file behind an id be found by binary search over
        // boundaries rather than by a 4-byte column on every node. Three files,
        // because two cannot show interleaving.
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut node_map = NodeMap::new();
        let parsed = parse_program(
            &arena,
            &["const a = 1;", "function f() { return 2; }", "class C { m() {} }"],
            &mut nodes,
            &mut node_map,
        );
        for pair in parsed.windows(2) {
            assert_eq!(pair[0].node_range.end, pair[1].node_range.start, "no gap, no overlap");
        }
    }

    #[test]
    fn speculative_rollback_inside_a_later_file_does_not_eat_an_earlier_one() {
        // `restore_state` truncates the node table to a length it recorded on
        // the way in. That is an *absolute* length, so it is only safe when the
        // table may start non-empty — truncating to a file-relative length would
        // discard every node of every file parsed before this one.
        //
        // The source is an arrow-function/parenthesised-expression ambiguity,
        // which is the construct that actually makes the parser speculate.
        let arena = Arena::new();
        let mut nodes = NodeTable::new();
        let mut node_map = NodeMap::new();
        let first = "const first = 1;";
        let parsed = parse_program(
            &arena,
            &[first, "const g = (a, b) => a + b; const h = (a + b);"],
            &mut nodes,
            &mut node_map,
        );

        assert!(parsed[1].diagnostics.is_empty(), "the fixture must parse");
        // Every id the first file claimed still reaches its own node.
        for id in parsed[0].node_range.clone() {
            let id = tsr_ast::NodeId::new(id);
            assert!(
                node_map.get(id).is_some(),
                "the first file survived the second's backtracking"
            );
        }
        assert_eq!(
            nodes.len(),
            node_map.len(),
            "the two tables stay in lockstep across a rollback in a non-empty table",
        );
    }

    #[test]
    fn parsing_one_file_alone_is_the_same_parse() {
        // `parse_with_options` is defined as `parse_into` over fresh tables, so
        // this pins that the definition did not change what a single file
        // parses. It is the guard on the refactor rather than on the feature.
        let source = "export class C { m(x: number): string { return `${x}`; } }";
        let arena = Arena::new();
        let alone = parse_with_options(&arena, source, ParseOptions::default());

        let arena2 = Arena::new();
        let mut nodes = NodeTable::new();
        let mut node_map = NodeMap::new();
        let into = parse_into(&arena2, source, ParseOptions::default(), &mut nodes, &mut node_map);

        assert_eq!(alone.nodes.len(), nodes.len());
        assert_eq!(alone.diagnostics.len(), into.diagnostics.len());
        assert_eq!(into.node_range.start, 0);
        assert_eq!(into.node_range.end as usize, alone.nodes.len());
        for id in 0..into.node_range.end {
            let id = tsr_ast::NodeId::new(id);
            assert_eq!(alone.nodes.kind(id), nodes.kind(id), "node {id:?} differs in kind");
            assert_eq!(alone.nodes.span(id), nodes.span(id), "node {id:?} differs in span");
        }
    }
}
