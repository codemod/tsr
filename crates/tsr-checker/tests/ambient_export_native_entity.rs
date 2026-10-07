//! Native `checkExportAssignment` (`checker.go:5666`, pinned 5b1047d).
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, Span};

fn diagnostics(source: &str, ambient: bool) -> Vec<(String, Span, String)> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "control must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: if ambient { "test.d.ts" } else { "test.ts" }, text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        parsed.source_file.node_id.expect("source file identity"),
        FileContext { ambient, has_parse_errors: false },
    );
    checker.diagnostics().iter().map(|(_, d)| (d.code(), d.span, d.text())).collect()
}

#[test]
fn ambient_export_expressions_report_native_code_message_and_expression_span() {
    const MESSAGE: &str = "The expression of an export assignment must be an identifier or qualified name in an ambient context.";
    for (source, expression) in [
        ("export default 2 + 2;", "2 + 2"),
        ("export = { value: 1 };", "{ value: 1 }"),
        ("declare module \"m\" { export default 2 + 2; }", "2 + 2"),
    ] {
        let start = u32::try_from(source.find(expression).expect("expression present"))
            .expect("control offset fits u32");
        assert_eq!(
            diagnostics(source, !source.starts_with("declare module")),
            [(
                "TS2714".to_owned(),
                Span::new(
                    start,
                    start + u32::try_from(expression.len()).expect("control length fits u32")
                ),
                MESSAGE.to_owned()
            )],
            "{source}",
        );
    }
}

#[test]
fn ambient_entity_names_and_nonambient_expressions_remain_legal() {
    for (source, ambient) in [
        ("declare const value: number; export default value;", true),
        ("declare namespace N { const value: number; } export = N.value;", true),
        ("export default { value: 1 };", false),
        ("export = 2 + 2;", false),
    ] {
        assert_eq!(diagnostics(source, ambient), [], "{source}");
    }
}

#[test]
fn namespace_export_error_precedes_ambient_entity_name_check() {
    let source = "declare namespace N { export = { value: 1 }; }";
    let result = diagnostics(source, false);
    assert_eq!(result.iter().map(|(code, _, _)| code.as_str()).collect::<Vec<_>>(), ["TS1063"]);
}
