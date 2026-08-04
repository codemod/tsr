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
    let diagnostics = tsr_dts::analyze(file, nodes);
    let resolver = SyntacticResolver::new(file);

    let (declaration_file, inference_required) = {
        let factory = Factory::new(arena, nodes);
        let mut transformer = transform::Transformer::new(factory, resolver);
        let result = transformer.transform_source_file(file);
        (result, std::mem::take(&mut transformer.inference_required))
    };

    let printed = tsr_printer::print(declaration_file, nodes);
    DeclarationEmit {
        text: printed.text,
        diagnostics,
        unsupported: printed.unsupported,
        inference_required,
    }
}
