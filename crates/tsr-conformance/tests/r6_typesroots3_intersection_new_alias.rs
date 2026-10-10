//! `type PM = SetOptional<M, "a">` over an intersection-bodied generic alias
//! attaches the new alias to the instantiated intersection
//! (`getTypeFromTypeAliasReference`'s newAliasSymbol → `getIntersectionTypeEx`
//! with that alias, checker.go:23609, :26056). `docs/parity/notes/r6-typesroots3.md`
//! §2. Expectations from the pinned `tsgo` (5b1047d).
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
fn an_intersection_alias_reference_takes_the_declaring_alias() {
    let source = r#"// @strict: true
// @target: es2020
type M = { a: 1; b: 2; c: 3 };
type SetOptional<T, K extends keyof T> = Omit<T, K> & Partial<Pick<T, K>>;
type PM = SetOptional<M, "a">;
export declare const pm: PM;
"#;
    assert_lines("probe/intersection-new-alias", source, &["PM : PM", "pm : PM"]);
}
