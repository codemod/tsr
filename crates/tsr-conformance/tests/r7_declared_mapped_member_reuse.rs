//! r7-declared §57: a homomorphic mapped member linked to its source
//! property prints the source's written annotation when the types agree,
//! checked against pinned native tsgo 5b1047d.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r"// @strict: true
interface M { a?: number; b: string; c?: { x: 1 } }
type DR<T> = T extends object ? { readonly [K in keyof T]: DR<T[K]> } : T;
declare const d: DR<M>;
export const z = d;
";

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r7_declared_reuse", "r7_declared_reuse.ts", source);
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

#[test]
fn a_linked_mapped_member_reuses_its_source_annotation() {
    let wanted = "z : { readonly a?: number; readonly b: string; readonly c?: { readonly x: 1; } | undefined; }";
    let lines = assertions(SOURCE);
    assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
}
