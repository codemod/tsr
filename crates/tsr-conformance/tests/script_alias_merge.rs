//! A script's `import q = Q` merging into an earlier script's `var q`
//! (`mergeSymbol`, `checker.go:14153-14159`): the non-alias target is its own
//! `resolveSymbol`, so upstream merges, and `q` is the variable's type rather
//! than upstream's `errorType` for an alias of a non-value
//! (`docs/parity/notes/r5-errorsplit6.md` §5).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/script_alias_merge", "probe.ts", source);
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
fn an_import_merged_into_an_earlier_var_is_the_variable() {
    let source = "// @target: es2015
// @Filename: a.ts
namespace P { }
var q;
// @Filename: b.ts
namespace Q { }
import q = Q;
";
    let lines = identities(source, "b.ts");
    let q = lines
        .iter()
        .find(|(line, _)| line == "q : any")
        .unwrap_or_else(|| panic!("a `q` line: {lines:?}"));
    assert_eq!(q.1, types_producer::TopError::Other, "{lines:?}");
    // The control: `Q` is a non-value namespace, upstream's `errorType`
    // (printed `error`: the probe case has no error baseline).
    let namespace = lines
        .iter()
        .find(|(line, _)| line == "Q : error")
        .unwrap_or_else(|| panic!("a `Q` line: {lines:?}"));
    assert_eq!(namespace.1, types_producer::TopError::Native, "{lines:?}");
}
