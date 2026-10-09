//! `bindThisPropertyAssignment`'s dynamic-name arm (`binder.go:1129-1131`):
//! a JS class's `this[k] = v` is filed under the class symbol's
//! `__assignment` export, which only the class's static resolution reads
//! (`getResolvedMembersOrExportsOfSymbol`, `checker.go:15962`). So the
//! class's static side carries the late member and an instance does not.
//! Expectations from the pinned tsgo on the same source
//! (`docs/parity/notes/r6-errorsplit2.md` §13).

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Each rendered line of `file`.
fn lines(source: &str, file: &str) -> Vec<String> {
    let case = TestCase::parse("probe/late_bound_this_assignment", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let arena = tsr_core::Arena::new();
    let (rendered, _) =
        types_producer::assertions_for_case_with_error_kinds(&arena, &case, &expected);
    let index = case.files.iter().position(|unit| unit.name == file).expect("the file");
    rendered[index].iter().map(tsr_conformance::types_producer::Assertion::line).collect()
}

#[test]
fn the_static_side_carries_a_this_element_assignment() {
    let source = "// @allowJs: true
// @checkJs: true
// @noEmit: true
// @strict: true
// @target: es2015
// @filename: a.js
const _sym = Symbol(\"_sym\");
const k = \"dyn\";
export class MyClass {
    constructor() {
        this[_sym] = \"ok\";
        this[k] = 1;
    }
    static m() {
        this[k] = true;
    }
}
const s1 = MyClass[_sym];
const s2 = MyClass[k];
";
    let lines = lines(source, "a.js");
    for expected in ["MyClass[_sym] : string", "MyClass[k] : number"] {
        assert!(lines.iter().any(|line| line == expected), "no `{expected}` in {lines:?}");
    }
}
