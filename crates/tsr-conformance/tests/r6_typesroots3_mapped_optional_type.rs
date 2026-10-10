//! An optional mapped property's own type includes `undefined` under
//! strictNullChecks (`getTypeOfMappedSymbol`, checker.go:20993), so the
//! resolved mapped type prints it. `docs/parity/notes/r6-typesroots3.md` §2.
//! Expectations from the pinned `tsgo` (5b1047d).
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
fn an_optional_mapped_property_prints_its_undefined() {
    let source = r#"// @strict: true
// @target: es2020
type M = { a: 1; b: 2; c: 3 };
export declare const qo: { [K in keyof M]?: K };
export declare const inter: { [K in keyof (Omit<M, "c"> & Partial<Pick<M, "c">>)]: K };
"#;
    assert_lines(
        "probe/mapped-optional-type",
        source,
        &[
            r#"qo : { a?: "a" | undefined; b?: "b" | undefined; c?: "c" | undefined; }"#,
            r#"inter : { a: "a"; b: "b"; c?: "c" | undefined; }"#,
        ],
    );
}
