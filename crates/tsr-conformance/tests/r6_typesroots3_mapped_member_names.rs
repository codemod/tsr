//! A mapped symbol's printed name comes from its key type and the
//! declarations it links (`getPropertyNameNodeForSymbolFromNameType`,
//! nodebuilderimpl.go:2455), and the members table is sorted by
//! `compareSymbols` (`getNamedMembers`, checker.go:22049).
//! `docs/parity/notes/r6-typesroots3.md` §2. Expectations from the pinned
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
fn mapped_member_names_follow_the_name_type_and_declaration_order() {
    let source = r#"// @strict: true
// @target: es2020
type W = { "12": 1, 'x-y': 2, 3: 3 };
export declare const qw: { [K in keyof W]: 0 };
export declare const qk: { [K in "b" | "a" | "1x" | 2]: 0 };
"#;
    assert_lines(
        "probe/mapped-member-names",
        source,
        &[r#"qw : { "12": 0; 'x-y': 0; 3: 0; }"#, r#"qk : { "1x": 0; 2: 0; a: 0; b: 0; }"#],
    );
}
