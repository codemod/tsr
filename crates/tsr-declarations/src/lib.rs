//! Declaration emit: the transform between the `isolatedDeclarations` analysis
//! and the printer.
//!
//! # What this crate is
//!
//! [`tsr_dts`] answers *which declarations would need inference*. [`tsr_printer`]
//! turns a tree into text. Neither produces a `.d.ts`, because a `.d.ts` is a
//! different tree: bodies gone, initializers gone, `declare` added, invisible
//! declarations dropped. This crate is that transform.
//!
//! ```text
//!   source tree ──► tsr-declarations ──► declaration tree ──► tsr-printer ──► .d.ts
//!                          │
//!                          └── EmitResolver ── SyntacticResolver (here)
//!                                           └─ CheckerResolver   (Phase 4)
//! ```
//!
//! # It is a port, unlike its neighbour, and that is deliberate
//!
//! [ADR-0021](../../../docs/adr/0021-isolated-declarations-is-not-a-port.md)
//! decided that `tsr-dts` — the *rules* — is written against TypeScript's
//! `isolatedDeclarations` specification rather than ported, because upstream's
//! error path runs through the checker and there is no syntactic predicate to
//! port. That argument is about the rules, and it does not extend to the
//! transform, for a reason worth being precise about:
//!
//! **Upstream's declaration transform is two layers, and only one of them touches
//! the checker.** Layer 2 — the driver in `internal/compiler/emitter.go:55/70/213`
//! — prints through the *same* `internal/printer/printer.go` that
//! [`tsr_printer`] already ports; there is no separate `.d.ts` printer to write.
//! Layer 1 — `internal/transformers/declarations/` — is 4,160 lines, of which the
//! dispatch, the elision and the modifier arithmetic are pure syntax. The checker
//! enters through exactly one interface, `printer.EmitResolver`, and every node
//! where it would be consulted is a node [`tsr_dts::analyze`] already reports a
//! `TS9xxx` for.
//!
//! So this crate ports layer 1 under [ADR-0001](../../../docs/adr/0001-idiomatic-rewrite.md)'s
//! default, with the checker's half behind the [`EmitResolver`] trait, and
//! supplies [`SyntacticResolver`] as the checker-free implementation. Items name
//! their upstream counterpart, as `docs/conventions.md` requires — and, unlike in
//! `tsr-dts`, the correspondence is real.
//!
//! # What "shipping without a checker" costs, stated up front
//!
//! `dts_reachable_target` measures 575 of 1,289 corpus cases as needing no
//! inference. That is the *population*, not a promise: a case needing no inference
//! is not the same as a case whose text we reproduce byte for byte. Three known
//! gaps, each of which shows up as a conformance failure rather than as silence:
//!
//! - **Comments are dropped.** The printer does not emit them
//!   (`docs/architecture/printer.md`), and upstream's `.d.ts` output preserves
//!   JSDoc. Every reachable case whose declarations carry doc comments fails on
//!   that alone.
//! - **Visibility is approximated.** Upstream decides it with
//!   `EmitResolver.IsDeclarationVisible`; here it is reachability from the exports
//!   (`docs/architecture/isolated-declarations.md`).
//! - **A refused inference becomes `any`.** Upstream's `ensureType` does the same
//!   when its node builder returns nothing, so the *shape* is right and the type
//!   is wrong. [`DeclarationEmit::inference_required`] reports where, so this is
//!   counted rather than hidden.
//!
//! # Entry point
//!
//! ```
//! # use tsr_core::Arena;
//! let arena = Arena::new();
//! let source = "export const answer: number = 42;\nfunction hidden() {}\n";
//! let parsed = tsr_parser::parse(&arena, source);
//! let mut nodes = parsed.nodes;
//!
//! let emitted = tsr_declarations::emit(&arena, &mut nodes, parsed.source_file);
//! assert_eq!(emitted.text.trim(), "export declare const answer: number;");
//! ```

pub mod enum_value;
pub mod factory;
mod modifiers;
pub mod resolver;
mod transform;
mod type_builder;

pub use enum_value::EnumValue;
pub use factory::Factory;
pub use resolver::{EmitResolver, Freshness, LiteralConstHost, SyntacticResolver};

use tsr_ast::{NodeTable, SourceFile, SyntaxKind};
use tsr_core::{Arena, Span};
use tsr_diagnostics::Diagnostic;

/// The result of emitting one file's declarations.
#[derive(Debug, Clone)]
pub struct DeclarationEmit {
    /// The `.d.ts` text.
    pub text: String,
    /// The `TS9xxx` diagnostics [`tsr_dts::analyze`] reported for this file.
    ///
    /// Non-empty means the text is *approximate*: upstream would have inferred
    /// where we could not. `tsc --isolatedDeclarations` behaves the same way — it
    /// reports and still emits — but a caller ratcheting against a baseline wants
    /// to know before comparing.
    pub diagnostics: Vec<Diagnostic>,
    /// Node kinds the printer does not implement, in the order first met.
    ///
    /// Non-empty means the text is *incomplete*, which is a different and worse
    /// failure than being approximate, and is kept separate for that reason.
    pub unsupported: Vec<SyntaxKind>,
    /// Where the resolver was asked for a type and had none, so `any` was emitted.
    ///
    /// This is the measurable form of "the emitter guessed". It should be empty
    /// for every case `diagnostics` is empty for; the two disagreeing means the
    /// analysis and the type builder have drifted, which
    /// `tests/analysis_agreement.rs` exists to catch.
    pub inference_required: Vec<Span>,
}

/// Syntax-only declaration emit options.
///
/// These are deliberately limited to facts that do not require a checker. The
/// source text is optional so existing AST-only callers retain their behavior;
/// options whose semantics depend on comments are inert without it.
#[derive(Debug, Clone, Copy, Default)]
// Each bool mirrors an independent upstream boolean compiler option; folding
// them into enums would invent states upstream does not have.
#[allow(clippy::struct_excessive_bools)]
pub struct DeclarationEmitOptions<'a> {
    /// Original source text used to inspect comment trivia around declarations.
    pub source_text: Option<&'a str>,
    /// Remove declarations whose nearest leading comment contains `@internal`.
    pub strip_internal: bool,
    /// Suppress comments in declaration output.
    pub remove_comments: bool,
    /// Preserve `null` as a literal type instead of widening it to `any`.
    pub strict_null_checks: bool,
    /// Treat the file as a module even without import/export syntax.
    ///
    /// Module detection is not purely syntactic: a `.mts`/`.cts` extension (and
    /// `moduleDetection: force`) makes upstream set the external-module
    /// indicator, so an extensionless-module file's unexported declarations
    /// drop and the `.d.ts` keeps an `export {}` marker.
    pub force_module: bool,
    /// Append a `//# sourceMappingURL=` directive naming this file's
    /// declaration map. The map itself is not produced — only the reference
    /// `declarationMap` makes upstream write at the end of the `.d.ts`.
    pub source_map_url: Option<&'a str>,
}

/// The kind of a preserved triple-slash declaration reference.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationReferenceKind {
    /// `/// <reference path="…" />`.
    Path,
    /// `/// <reference types="…" />`.
    Types,
    /// `/// <reference lib="…" />`.
    Lib,
}

/// A `types` reference's optional module-resolution phase.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationResolutionMode {
    /// No `resolution-mode` attribute.
    None,
    /// `resolution-mode="require"`.
    Require,
    /// `resolution-mode="import"`.
    Import,
}

/// A triple-slash reference to preserve in declaration output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeclarationReference {
    /// Reference kind and attribute name.
    pub kind: DeclarationReferenceKind,
    /// Referenced path, package, or library.
    pub file_name: String,
    /// Optional resolution phase for `types` references.
    pub resolution_mode: DeclarationResolutionMode,
    /// Byte position in the source preamble, used to restore mixed-kind order.
    pub position: u32,
}

/// Emit the `.d.ts` text for one parsed file.
///
/// `nodes` is taken by `&mut` because the transform synthesizes nodes — a
/// `declare` modifier, an `export {}` marker, a widened type — and every node
/// needs a row in the side table before the printer can read its kind or flags.
/// See [`factory`] for why registration is not optional.
#[must_use]
pub fn emit<'a>(
    arena: &'a Arena,
    nodes: &mut NodeTable,
    file: &'a SourceFile<'a>,
) -> DeclarationEmit {
    emit_with_references_and_options(arena, nodes, file, &[], DeclarationEmitOptions::default())
}

/// Emit declarations with syntax-only compiler options.
#[must_use]
pub fn emit_with_options<'a>(
    arena: &'a Arena,
    nodes: &mut NodeTable,
    file: &'a SourceFile<'a>,
    options: DeclarationEmitOptions<'a>,
) -> DeclarationEmit {
    emit_with_references_and_options(arena, nodes, file, &[], options)
}

/// Emit declarations and prepend explicitly preserved triple-slash references.
#[must_use]
pub fn emit_with_references<'a>(
    arena: &'a Arena,
    nodes: &mut NodeTable,
    file: &'a SourceFile<'a>,
    references: &[DeclarationReference],
) -> DeclarationEmit {
    emit_with_references_and_options(
        arena,
        nodes,
        file,
        references,
        DeclarationEmitOptions::default(),
    )
}

/// Emit declarations with preserved references and syntax-only compiler options.
#[must_use]
pub fn emit_with_references_and_options<'a>(
    arena: &'a Arena,
    nodes: &mut NodeTable,
    file: &'a SourceFile<'a>,
    references: &[DeclarationReference],
    options: DeclarationEmitOptions<'a>,
) -> DeclarationEmit {
    let diagnostics = tsr_dts::analyze_with_options(
        file,
        nodes,
        tsr_dts::AnalysisOptions { strict_null_checks: options.strict_null_checks },
    );
    let javascript = file
        .node_id
        .is_some_and(|id| nodes.flags(id).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE));
    let commonjs = tsr_dts::visibility::is_commonjs_module(file, javascript);
    let resolver = SyntacticResolver::new(file, options.strict_null_checks, commonjs);

    let (declaration_file, inference_required) = {
        let factory = Factory::new(arena, nodes);
        let mut transformer = transform::Transformer::new(factory, resolver, options);
        let result = transformer.transform_source_file(file);
        (result, std::mem::take(&mut transformer.inference_required))
    };

    let printed = match options.source_text.filter(|_| !options.remove_comments) {
        Some(source_text) => tsr_printer::print_with_source(declaration_file, nodes, source_text),
        None => tsr_printer::print(declaration_file, nodes),
    };
    let mut text = render_references(references);
    text.push_str(&printed.text);
    if let Some(url) = options.source_map_url {
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("//# sourceMappingURL=");
        text.push_str(url);
    }
    DeclarationEmit { text, diagnostics, unsupported: printed.unsupported, inference_required }
}

fn render_references(references: &[DeclarationReference]) -> String {
    let mut references = references.to_vec();
    references.sort_by_key(|reference| reference.position);
    let mut output = String::new();
    for reference in references {
        let attribute = match reference.kind {
            DeclarationReferenceKind::Path => "path",
            DeclarationReferenceKind::Types => "types",
            DeclarationReferenceKind::Lib => "lib",
        };
        let file_name = if reference.kind == DeclarationReferenceKind::Path {
            declaration_reference_name(&reference.file_name)
        } else {
            reference.file_name
        };
        output.push_str("/// <reference ");
        output.push_str(attribute);
        output.push_str("=\"");
        output.push_str(&escape_reference_attribute(&file_name));
        output.push('"');
        match reference.resolution_mode {
            DeclarationResolutionMode::None => {}
            DeclarationResolutionMode::Require => {
                output.push_str(" resolution-mode=\"require\"");
            }
            DeclarationResolutionMode::Import => {
                output.push_str(" resolution-mode=\"import\"");
            }
        }
        output.push_str(" preserve=\"true\" />\n");
    }
    output
}

fn declaration_reference_name(name: &str) -> String {
    // Declaration references are written relative to the emitted file's
    // directory. typescript-go normalizes an explicit same-directory `./`
    // prefix away before replacing the source extension.
    let name = name.strip_prefix("./").unwrap_or(name);
    // A reference to a declaration file keeps its name: rewriting `.ts` off
    // `bar.d.ts` would produce `bar.d.d.ts` (`commonSourceDirectory`).
    let lower = name.to_ascii_lowercase();
    if lower.ends_with(".d.ts") || lower.ends_with(".d.mts") || lower.ends_with(".d.cts") {
        return name.to_string();
    }
    for (source, declaration) in [
        (".mts", ".d.mts"),
        (".cts", ".d.cts"),
        (".mjs", ".d.mts"),
        (".cjs", ".d.cts"),
        (".tsx", ".d.ts"),
        (".jsx", ".d.ts"),
        (".ts", ".d.ts"),
        (".js", ".d.ts"),
    ] {
        if let Some(stem) = name.strip_suffix(source) {
            return format!("{stem}{declaration}");
        }
    }
    name.to_string()
}

fn escape_reference_attribute(value: &str) -> String {
    value.replace('&', "&amp;").replace('"', "&quot;").replace('<', "&lt;").replace('>', "&gt;")
}
