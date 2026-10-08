//! `checkObjectLiteral`'s `contextualTypeHasPattern` arm (`checker.go:13250`):
//! a literal typed by the implied type of a binding pattern, or by the left
//! of a destructuring assignment, reports TS2353 at each member the pattern
//! does not name. Expected texts are native tsgo's (vendor `5b1047d`).

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, d)| (d.message.code(), d.text())).collect()
}

fn excess(name: &str, ty: &str) -> (u32, String) {
    (
        2353,
        format!(
            "Object literal may only specify known properties, and '{name}' does not exist in type '{ty}'."
        ),
    )
}

#[test]
fn a_binding_pattern_initializer_reports_members_the_pattern_does_not_name() {
    let actual = diagnostics(
        "var { } = { x: 5, y: \"hello\" };\n\
         var { x4 } = { x4: 5, y4: \"hello\" };\n\
         var { x7: a1 } = { x7: 5, y7: \"hello\" };\n\
         var { q: { r } } = { q: { r: 1, s: 2 } };\n\
         var { ...rest } = { r1: 1 };\n",
    );
    assert_eq!(
        actual,
        [excess("y4", "{ x4: any; }"), excess("y7", "{ x7: any; }"), excess("s", "{ r: any; }"),]
    );
}

// Computed members print their written expression (`[k]`, `["x"]`), read
// from the module host's source text, which this harness does not provide;
// `checkDestructuringShorthandAssigment2` and
// `destructuredLateBoundNameHasCorrectTypes` pin them in the corpus.

#[test]
fn a_destructuring_assignment_types_its_right_by_the_left_literal() {
    let actual = diagnostics(
        "var x: number, y: number;\n\
         ({ } = { x: 0, y: 0 });\n\
         ({ x } = { x: 0, y: 0 });\n\
         let r: any;\n\
         ({ x, ...r } = { x: 0, w: 1 });\n",
    );
    assert_eq!(actual, [excess("x", "{}"), excess("y", "{}"), excess("y", "{ x: number; }"),]);
}

/// `isKnownProperty` against a union with a function-type constituent: the
/// function type literal has no properties (`excessPropertyErrorForFunctionTypes`).
#[test]
fn a_function_type_constituent_knows_no_property() {
    let actual = diagnostics(
        "type FunctionType = () => any;\n\
         type DoesntWork = { a: number, c: number } | FunctionType;\n\
         let doesntWork: DoesntWork = { a: 1, c: 2, d: 3 };\n",
    );
    assert_eq!(actual, [excess("d", "DoesntWork")]);
}
