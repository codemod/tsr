//! `getPropertyOfType` reads the reduced apparent type, so an intersection whose
//! discriminants conflict is skipped by the discriminant's union property
//! (`docs/parity/notes/r6-errorsplit3.md` §2).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/never_discriminant_constituent", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let arena = tsr_core::Arena::new();
    let (rendered, kinds) =
        types_producer::assertions_for_case_with_error_kinds(&arena, &case, &expected);
    let index = case.files.iter().position(|unit| unit.name == file).expect("the file");
    rendered[index].iter().zip(&kinds[index]).map(|(line, kind)| (line.line(), kind.top)).collect()
}

/// Asserts every `wanted` line is printed, with no error identity.
fn assert_lines(source: &str, file: &str, wanted: &[&str]) {
    let lines = identities(source, file);
    for want in wanted {
        let line = lines
            .iter()
            .find(|(line, _)| line == want)
            .unwrap_or_else(|| panic!("a `{want}` line: {lines:?}"));
        assert_eq!(line.1, types_producer::TopError::Other, "{want}: {lines:?}");
    }
}

#[test]
fn a_distributed_intersection_narrows_by_its_discriminant() {
    let source = "// @target: es2015
// @strict: true
// @Filename: a.ts
type a = { type: \"a\", data: string }
type b = { type: \"b\", name: string }
type c = { type: \"c\", other: string }
function f(problem: (a | b | c) & (b | c)) {
    if (problem.type === \"b\") {
        problem.name;
    }
}
";
    assert_lines(source, "a.ts", &["problem.name : string"]);
}
