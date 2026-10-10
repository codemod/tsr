//! `resolveAnonymousTypeMembers`' `globalThisSymbol` arm (`checker.go:20674-20682`):
//! `typeof globalThis` has the globals as members, so a composite receiver
//! such as `Window & typeof globalThis` finds them
//! (`docs/parity/notes/r6-errorsplit3.md` §2).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/global_this_composite_members", "probe.ts", source);
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
fn window_reads_globals_through_typeof_global_this() {
    let source = "// @target: es2015
// @lib: esnext,dom
// @Filename: a.ts
declare var myv: number;
window.console;
window.myv;
";
    assert_lines(source, "a.ts", &["window.console : Console", "window.myv : number"]);
}
