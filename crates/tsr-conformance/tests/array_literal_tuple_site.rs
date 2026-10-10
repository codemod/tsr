//! An array literal's tuple image prints its elements at the site, as its
//! base tuple does (`docs/parity/notes/r7-printer.md` §8). Expected lines from
//! pinned native tsgo 5b1047d's baseline for `declFileTypeAnnotationTupleType`.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn an_array_literal_tuple_qualifies_its_elements_at_the_site() {
    let source = r"// @target: es2015
class c {
}
namespace m {
    export class c {
    }
}
var k: [c, m.c] = [new c(), new m.c()];
";
    let case = TestCase::parse("probe/array_literal_tuple_site", "a.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<String> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in ["k : [c, m.c]", "[new c(), new m.c()] : [c, m.c]"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
