//! A written annotation naming a module's unexported alias is not reused
//! where the alias cannot be named (`docs/parity/notes/r6-printer.md` §2).
//! Expectations from pinned native tsgo 5b1047d
//! (`declarationEmitPartialNodeReuseTypeReferences`).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @filename: a.ts
export type SpecialString = string;
type PrivateSpecialString = string;
export const o = (p1: SpecialString, p2: PrivateSpecialString) => null! as { foo: SpecialString, bar: PrivateSpecialString };
// @filename: b.ts
import * as a from "./a";
export const g = a.o
"#;

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/foreign_site_alias_reuse", "a.ts", source);
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
fn an_unexported_alias_is_serialized_at_a_foreign_site() {
    let lines = assertions(SOURCE);
    for wanted in [
        "o : (p1: SpecialString, p2: PrivateSpecialString) => { foo: SpecialString; bar: PrivateSpecialString; }",
        "g : (p1: a.SpecialString, p2: string) => { foo: a.SpecialString; bar: string; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
