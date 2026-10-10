//! `checkExpressionForMutableLocation` reads its contextual type through
//! `instantiateContextualType` (`docs/parity/notes/r6-printer.md` §3).
//! Expectations from pinned native tsgo 5b1047d.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r"// @strict: true
// @target: es2015
enum E { A = 'A', B = 'B', C = 'C' }
const m: Map<E, number> = new Map([[E.A, 1], [E.B, 2]]);
declare function pair<K, V>(entries: [K, V][]): Map<K, V>;
const p: Map<E, number> = pair([[E.C, 3]]);
";

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/mutable_location_return_mapper", "a.ts", source);
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
fn a_return_mapper_inference_keeps_an_enum_member_literal() {
    let lines = assertions(SOURCE);
    // The annotation's `Map<E, …>` infers `K = E` through the return
    // mapper, so each member literal is a literal of its contextual type.
    for wanted in [
        "new Map([[E.A, 1], [E.B, 2]]) : Map<E.A | E.B, number>",
        "pair([[E.C, 3]]) : Map<E.C, number>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
