//! Inference into an intersection-bearing target: couldContainTypeVariables
//! reads an intersection's constituents, and inferFromTypes infers to the
//! simplified form of an indexed access (`(T & U)[K] -> T[K] & U[K]`, a generic
//! mapped object's `E[P := K]`; `inference.go:217`).
//! `docs/parity/notes/r6-typesroots3.md` §4. Expectations from the pinned
//! `tsgo` (5b1047d).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Every assertion line the corpus pipeline produces for `source`.
fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

fn assert_lines(name: &str, source: &str, wanted: &[&str]) {
    let lines = lines(name, source);
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn inference_reaches_through_intersections_and_indexed_accesses() {
    let source = r#"// @strict: true
// @target: es2020
interface FC<V> { initialValues: V; validate?: (props: V) => void; }
interface E { extra?: number }
declare function G1<V = object>(x: Readonly<FC<V> & E>): V;
export const g1 = G1({ initialValues: { foo: "" }, validate: q1 => { q1; } });
declare function F14<V>(x: (FC<V> & {})["initialValues"]): V;
export const a14 = F14({ foo: "" });
"#;
    assert_lines(
        "probe/inference-intersection-indexed-access",
        source,
        &["g1 : { foo: string; }", "q1 : { foo: string; }", "a14 : { foo: string; }"],
    );
}
