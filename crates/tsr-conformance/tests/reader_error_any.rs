//! Location and writer contracts measured against native tsgo 5b1047d1.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const JS_SOURCE: &str = r#"// @strict: true
// @noImplicitAny: false
// @allowJs: true
// @checkJs: false
function implicit(dynamic) {
    dynamic["flag"]; dynamic["count"] = 7;
}
var concrete = { flag: true, count: 2 };
concrete["flag"]; concrete["count"] = 8;
missing["read"]; missing["write"] = 9;
var holes = [, 1];
"#;

fn records(
    source: &str,
    name: &str,
    had_errors: bool,
) -> Vec<(types_producer::Assertion, &'static str)> {
    let mut case = TestCase::parse("probe/reader-error-any", name, source);
    case.had_error_baseline = had_errors;
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|file| FileTypes { file: file.name.clone(), assertions: Vec::new() })
        .collect();
    let arena = tsr_core::Arena::new();
    let (program, rendered, ids) =
        types_producer::assertions_for_case_with_ids(&arena, &case, &case.files.as_slice());
    let mut checker = types_producer::configured_checker(&program);
    rendered
        .into_iter()
        .flatten()
        .zip(ids.into_iter().flatten())
        .map(|(assertion, id)| {
            let raw = types_producer::type_id_at_location_tracking(
                &mut checker,
                program.binder(),
                program.nodes(),
                program.node_map(),
                id,
                &mut false,
            );
            let identity = if raw == checker.intrinsics().error {
                "error"
            } else if raw == checker.intrinsics().any {
                "any"
            } else {
                "concrete"
            };
            (assertion, identity)
        })
        .collect()
}

#[test]
fn unresolved_js_read_and_write_keep_computed_error_any_not_genuine_any() {
    let actual = records(JS_SOURCE, "fixture.js", false);
    for (text, spelling, identity) in [
        (r#"missing["read"]"#, "error", "error"),
        (r#"missing["write"]"#, "error", "error"),
        (r#"missing["write"] = 9"#, "9", "concrete"),
        (r#"dynamic["flag"]"#, "any", "any"),
        (r#"dynamic["count"]"#, "any", "any"),
        (r#"concrete["flag"]"#, "boolean", "concrete"),
        (r#"concrete["count"]"#, "number", "concrete"),
    ] {
        assert!(
            actual.iter().any(|(assertion, raw)| {
                assertion.text == text && assertion.type_string == spelling && *raw == identity
            }),
            "missing native {text}: {spelling}, raw {identity}: {actual:?}"
        );
    }
}

#[test]
fn error_baseline_changes_spelling_without_changing_computed_identity() {
    for had_errors in [false, true] {
        let actual = records(JS_SOURCE, "fixture.js", had_errors);
        for text in [r#"missing["read"]"#, r#"missing["write"]"#] {
            let (assertion, raw) = actual.iter().find(|(a, _)| a.text == text).expect("access row");
            assert_eq!(*raw, "error", "the writer flag cannot turn error-any into genuine any");
            assert_eq!(assertion.type_string, if had_errors { "any" } else { "error" });
        }
        let (assertion, raw) = actual
            .iter()
            .find(|(a, _)| a.text == r#"dynamic["flag"]"#)
            .expect("computed-any control");
        assert_eq!((*raw, assertion.type_string.as_str()), ("any", "any"));
    }
}

#[test]
fn native_writer_nil_positions_are_absent_not_error_or_any_placeholders() {
    let actual = records(
        r"// @strict: true
type Alias = string;
let annotation: Alias;
const holes = [, 1];
",
        "fixture.ts",
        false,
    );
    // Native keeps the alias declaration name but drops its written use as
    // a type operand. Its omitted-expression writer also returns nil, even
    // though GetTypeAtLocation returns a non-nil type for that node.
    assert_eq!(actual.iter().filter(|(a, _)| a.text == "Alias").count(), 1);
    assert!(actual.iter().any(|(a, _)| a.line() == "annotation : string"));
    assert!(actual.iter().all(|(a, _)| a.kind != tsr_ast::SyntaxKind::OmittedExpression));
    assert!(actual.iter().all(|(a, _)| !a.text.is_empty()));
}
