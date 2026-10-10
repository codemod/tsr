//! `getTypeOfNode`'s fall-through (`checker.go:32035`): a node no arm claims,
//! and that is no expression node, is upstream's `errorType`, not the port's
//! gap. Both halves of a `JsxNamespacedName` take it, even when the prefix
//! names a variable in scope (`isInExpressionContext` has no
//! `JsxNamespacedName` parent arm), as the native identity probe recorded on
//! `jsxNamespacePrefixInName` (`docs/parity/notes/r6-errorsplit2.md` §2).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each line of `file` with its top-level error identity.
fn identities(source: &str, file: &str) -> Vec<(String, types_producer::TopError)> {
    let case = TestCase::parse("probe/type_of_node_fallthrough", "probe.tsx", source);
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
fn both_halves_of_a_namespaced_jsx_name_are_error_type() {
    let source = "// @target: es2015
// @jsx: preserve
// @Filename: a.tsx
var a = 1;
var x = <a:element />;
";
    let lines = identities(source, "a.tsx");
    for name in ["a : error", "element : error"] {
        let line = lines
            .iter()
            .find(|(line, _)| line == name)
            .unwrap_or_else(|| panic!("a `{name}` line: {lines:?}"));
        assert_eq!(line.1, types_producer::TopError::Native, "{lines:?}");
    }
    // The control: the declaration's own `a` is the variable.
    let declared = lines
        .iter()
        .find(|(line, _)| line == "a : number")
        .unwrap_or_else(|| panic!("a declared `a` line: {lines:?}"));
    assert_eq!(declared.1, types_producer::TopError::Other, "{lines:?}");
}
