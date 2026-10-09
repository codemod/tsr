//! TS18043 — `checkAliasSymbol`'s JS arm (`checker.go:6751`): an export
//! specifier whose target has no value meaning, in a JavaScript file.
//! Expectations were checked against a native `tsgo` built from the pinned
//! submodule.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// The codes reported for a one-file JS module, with their spans' text.
fn reports(source: &str) -> Vec<(u32, String)> {
    let name = "t.js";
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let mut reports: Vec<(u32, String)> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| matches!(d.message.code(), 18042 | 18043 | 2484))
        .map(|(_, d)| {
            (d.message.code(), source[d.span.start as usize..d.span.end as usize].to_string())
        })
        .collect();
    reports.sort();
    reports
}

#[test]
fn exporting_a_typedef_is_ts18043() {
    let source = "/** @typedef {{ x: any }} JSDocType */\nexport { JSDocType };\nexport { JSDocType as ThisIsFine };\nexport const v = 1;\n";
    assert_eq!(reports(source), [(18043, "JSDocType".into()), (18043, "JSDocType".into())]);
}

#[test]
fn exporting_a_value_does_not_report() {
    assert!(reports("const a = 1;\nexport { a };\n").is_empty());
}
