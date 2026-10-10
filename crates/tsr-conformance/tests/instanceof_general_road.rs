//! `narrowTypeByInstanceof` over `getInstanceType` (`flow.go:964-976`): a callee
//! that is no class (a type literal with a construct signature, an
//! intersection) narrows by its construct signatures' return
//! (`docs/parity/notes/r6-errorsplit3.md` §2).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/instanceof_general_road", "probe.ts", source);
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
fn a_construct_signature_callee_narrows() {
    let source = "// @target: es2015
// @Filename: a.ts
interface D { foo: string; }
declare var D: { new (): D; };
declare var obj7: D | string;
if (obj7 instanceof D) {
    obj7.foo;
}
interface One { one(): void }
interface Two { two(): void }
declare const instance: One | Two;
declare const ClassOne: { new(): One } & { foo: true };
if (instance instanceof ClassOne) {
    instance.one();
}
";
    assert_lines(source, "a.ts", &["obj7.foo : string", "instance.one() : void"]);
}
