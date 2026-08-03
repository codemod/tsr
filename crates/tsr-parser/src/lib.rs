//! The TypeScript parser.
//!
//! # STATUS: WORK IN PROGRESS — DOES NOT COMPILE
//!
//! This crate is excluded from the workspace (`Cargo.toml`) until it builds. The
//! structure, precedence table, error recovery, and node construction are written;
//! what remains is reconciling roughly 40 call sites against the generated
//! constructor signatures, which changed under it when node nullability was
//! corrected. Tracked as `bd tsr-pum.2`.
//!
//! Nothing depends on it, and the conformance harness still reports the parser
//! suite at 0%, which is accurate.
//!
//! Ported from typescript-go's `internal/parser/parser.go`.
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
mod jsx;
mod module;
mod parsed_file;
mod parser;
mod statement;
mod types;

pub use parsed_file::ParsedFile;
pub use parser::{JSDocTable, ParseOptions, ParseResult, Parser, ScriptKind};

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
    let mut parser = Parser::with_options(arena, source, options);
    let source_file = parser.parse_source_file();
    let (diagnostics, mut node_table, jsdoc) = parser.finish();
    if options.parents {
        tsr_ast::assign_parents(tsr_ast::Node::SourceFile(source_file), &mut node_table);
    }
    ParsedSourceFile { source_file, diagnostics, nodes: node_table, jsdoc }
}

/// The result of parsing one file.
pub struct ParsedSourceFile<'a> {
    /// The root of the tree.
    pub source_file: &'a SourceFile<'a>,
    /// Everything the scanner and parser objected to, in source order.
    pub diagnostics: Vec<Diagnostic>,
    /// Kind, span, and parent for every node the parser registered.
    pub nodes: tsr_ast::NodeTable,
    /// JSDoc comments, keyed by the node they document.
    pub jsdoc: JSDocTable<'a>,
}
