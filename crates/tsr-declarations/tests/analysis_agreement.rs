//! The analysis and the emitter must agree about what needs inference.
//!
//! # The property, and why it is not checkable inside either crate
//!
//! `tsr_dts::analyze` decides *which declarations need inference*. The emitter's
//! type builder decides *which expressions it can produce a type for*. These are
//! the same question asked by two crates that share no code, and they can drift
//! apart in either direction — silently, because both directions produce output
//! that parses:
//!
//! - **The analysis accepts, the emitter refuses.** The `.d.ts` gets `any` where a
//!   real type belonged. Nothing reports it; the file is valid TypeScript that
//!   lies about its types.
//! - **The analysis reports, the emitter produces something.** A guess presented
//!   as a fact, in a file whose whole purpose is to be believed.
//!
//! The conformance corpus cannot see either, because a case with a `TS9xxx` is
//! excluded from `dts_emit`'s denominator by construction. So the agreement is
//! asserted here, over a table written by hand.
//!
//! `DeclarationEmit::inference_required` is what makes the first direction
//! observable at all: it records every point where the resolver was asked for a
//! type and had none.

use tsr_core::Arena;

/// Emit one source and report `(diagnostic count, inference-required count)`.
fn emit(source: &str) -> (usize, usize, String) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "test source must parse cleanly: {source:?}");
    let mut nodes = parsed.nodes;
    let result = tsr_declarations::emit(&arena, &mut nodes, parsed.source_file);
    assert!(
        result.unsupported.is_empty(),
        "printer does not support {:?} in {source:?}",
        result.unsupported
    );
    (result.diagnostics.len(), result.inference_required.len(), result.text)
}

/// Constructs `tsr_dts::rules::infer` returns `Ok` for.
///
/// Every one of these must emit without a single refusal. A new entry that fails
/// is a real gap: the analysis is telling users the file is `isolatedDeclarations`
/// clean while the emitter writes `any` into it.
const APPARENT: &[&str] = &[
    "export const a = 1;",
    "export const b = \"s\";",
    "export const c = true;",
    "export const d = 1n;",
    "export const e = -1;",
    "export const f = +1;",
    "export let g = 1;",
    "export let h = \"s\";",
    "export let i = false;",
    "export let j = `a${1}b`;",
    "export const k = { a: 1, b: \"s\" };",
    "export let l = { a: 1 };",
    "export const m = [1, 2] as const;",
    "export const n = { a: 1 } as const;",
    "export const o: number = 1;",
    "export const p = 1 as number;",
    "export const q = (x: number): string => \"s\";",
    "export const r = null;",
    "export const s = /a/;",
    "export class T { u: number = 1; v(): void {} }",
    "export interface W { x: number; }",
    "export type X = { y: number };",
    "export enum Y { A, B = 2 }",
    "export function z(a: number): void {}",
];

/// Constructs the analysis reports.
///
/// The emitter is allowed to write `any` for these — that is upstream's own
/// `ensureType` fallback — but it must *record* that it did. A construct that
/// reports a diagnostic and needs no inference means the analysis is reporting
/// something the emitter could have handled, which costs a reachable case.
const NEEDS_INFERENCE: &[&str] = &[
    "export const a = f();",
    "export let b = [1, 2];",
    "export let c;",
    "export const d = { ...e };",
    "export function f() { return 1; }",
    "export class G { h() { return 1; } }",
];

#[test]
fn every_apparent_type_is_emitted_without_a_guess() {
    for source in APPARENT {
        let (diagnostics, refusals, text) = emit(source);
        assert_eq!(
            diagnostics, 0,
            "the analysis reported on {source:?}, which this table calls clean\n{text}"
        );
        assert_eq!(
            refusals, 0,
            "the analysis calls {source:?} clean and the emitter still wrote `any`:\n{text}"
        );
    }
}

#[test]
fn every_reported_construct_is_reported_by_both() {
    for source in NEEDS_INFERENCE {
        let (diagnostics, _, text) = emit(source);
        assert!(diagnostics > 0, "the analysis said nothing about {source:?}:\n{text}");
    }
}

/// The one construct the two crates are known to disagree about.
///
/// `tsr_dts::rules` passes "the declaration list is `const`" in where the const
/// *context* belongs, so it accepts `export const b = [1, 2]` — for which
/// TypeScript raises `TS9017`, because only `as const` makes an array inferable.
/// This crate has the corrected reading, so the emitter refuses where the analysis
/// does not, and the case reaches `dts_emit`'s denominator with no type to emit.
///
/// Asserted rather than removed: the day `bd tsr-49v.2.6` is fixed, this test
/// fails and says so, which is the only way a known divergence stops being a
/// permanent one.
#[test]
fn the_known_const_context_disagreement_still_stands() {
    let (diagnostics, refusals, _) = emit("export const b = [1, 2];");
    assert_eq!(
        diagnostics, 0,
        "tsr-dts now reports this — bd tsr-49v.2.6 is fixed, delete this test"
    );
    assert!(refusals > 0, "the emitter now types this without the analysis agreeing");
}

/// The emitted `.d.ts` must itself parse.
///
/// A weaker property than byte-matching and a completely different one: the
/// corpus gate compares against upstream's text, so a construct upstream's
/// baselines happen not to contain is unmeasured there. This says the output is at
/// least *TypeScript*, over both tables — including the ones where the emitter
/// guessed.
#[test]
fn emitted_declarations_parse() {
    for source in APPARENT.iter().chain(NEEDS_INFERENCE) {
        let (_, _, text) = emit(source);
        let arena = Arena::new();
        let reparsed = tsr_parser::parse(&arena, &text);
        assert!(
            reparsed.diagnostics.is_empty(),
            "emitting {source:?} produced text that does not parse:\n{text}\n{:?}",
            reparsed.diagnostics.first().map(tsr_diagnostics::Diagnostic::text)
        );
    }
}

/// A `.d.ts` never carries a function body, an initializer it should not, or a
/// statement with a runtime effect.
///
/// Stated as a property over the *text* rather than the tree, because that is what
/// ships. Each of these has a specific way of getting through: a body survives if a
/// `transformX` forgets to drop it, an `=` survives if `ensureNoInitializer` is
/// skipped, and a runtime statement survives if `visit_statement`'s elision list is
/// incomplete.
#[test]
fn declarations_carry_no_runtime() {
    let source = "\
export function f(a: number): void { console.log(a); }
export class C { m(): void { return; } p: number = 1; }
console.log(1);
for (const x of []) {}
export const g = 1;
export let h: number = 2;
";
    let (_, _, text) = emit(source);
    assert!(!text.contains("console.log"), "a body survived:\n{text}");
    assert!(!text.contains("for ("), "a runtime statement survived:\n{text}");
    // `export declare const g = 1;` is the one legal `=`: a literal const emits its
    // value instead of a type.
    assert!(text.contains("const g = 1"), "a literal const lost its value:\n{text}");
    assert!(text.contains("h: number"), "an annotated let lost its annotation:\n{text}");
    assert!(!text.contains("p: number = 1"), "a property kept its initializer:\n{text}");
}
