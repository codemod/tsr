//! `anyBaseTypeIndexInfo` (`checker.go:1048`): a class whose base is `anyType`
//! has a `string` index of `anyType` on its instance side
//! (`resolveObjectTypeMembers`, `:19147`) and, without a static index
//! signature of its own, on its static side (`resolveAnonymousTypeMembers`,
//! `:20693`) (`docs/parity/notes/r6-errorsplit3.md` §3).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/any_base_type_index_info", "probe.ts", source);
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
fn a_class_extending_any_reads_unknown_members_as_any() {
    let source = "// @target: es2015
// @Filename: a.ts
declare var Base: any;
class C extends Base {
    known = 1;
    static sknown = 2;
}
let c = new C();
c.unknown;
C.sunknown;
c.known;
";
    assert_lines(source, "a.ts", &["c.unknown : any", "C.sunknown : any", "c.known : number"]);
}

/// The control: a base that is `errorType` (an unresolved name) is not
/// `anyType`, so no index is inherited and the miss is not `any`.
#[test]
fn an_unresolved_base_inherits_no_index() {
    let source = "// @target: es2015
// @Filename: a.ts
class C extends Missing {}
let c = new C();
c.unknown;
";
    let lines = identities(source, "a.ts");
    let line = lines.iter().find(|(line, _)| line.starts_with("c.unknown : ")).expect("the read");
    assert_ne!(line.1, types_producer::TopError::Other, "{lines:?}");
}
