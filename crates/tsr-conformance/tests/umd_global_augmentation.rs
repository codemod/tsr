//! `mergeSymbol` resolves a non-transient alias target (`checker.go:14153-14164`):
//! a `declare global` namespace merging into a UMD global (`export as
//! namespace a`) merges into the module the alias names, and the merged module
//! replaces the alias in the globals (`docs/parity/notes/r6-errorsplit3.md` §2).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/umd_global_augmentation", "probe.ts", source);
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
fn a_global_augmentation_of_a_umd_alias_reaches_the_module() {
    let source = "// @module: commonjs
// @target: es2015
// @Filename: /a.d.ts
export as namespace a;
export const x = 0;

// @Filename: /b.ts
import * as a2 from \"./a\";
declare global {
    namespace a {
        export const y = 0;
    }
}
a.y;
a2.y;
";
    assert_lines(source, "/b.ts", &["a.y : 0", "a2.y : 0"]);
}
