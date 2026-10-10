//! `getTypeOfNode`'s `NodeFlagsInWithStatement` exit (`checker.go:31932`):
//! every node of a `with` statement's body is upstream's `errorType`, not the
//! port's gap (`docs/parity/notes/r6-errorsplit2.md` §7).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/with_statement_body", "probe.ts", source);
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

#[test]
fn a_literal_in_a_with_body_is_error_type() {
    let source = "// @target: es2015
// @Filename: a.ts
declare var o: any;
with (o) {
    12;
}
";
    let lines = identities(source, "a.ts");
    let literal = lines
        .iter()
        .find(|(line, _)| line.starts_with("12 : "))
        .unwrap_or_else(|| panic!("a `12` line: {lines:?}"));
    assert_eq!(literal.1, types_producer::TopError::Native, "{lines:?}");
    // The control: the `with` expression itself is outside the body.
    let object = lines
        .iter()
        .find(|(line, _)| line == "o : any")
        .unwrap_or_else(|| panic!("an `o` line: {lines:?}"));
    assert_eq!(object.1, types_producer::TopError::Other, "{lines:?}");
}
