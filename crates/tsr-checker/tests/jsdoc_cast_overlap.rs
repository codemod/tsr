//! TS2352 on a JS `@type` cast: `reparseHosted` makes `/** @type {T} */ (e)`
//! an `AsExpression` with a `Reparsed` type node, and
//! `checkAssertionDeferred` reports at that type node (`checker.go:12323`).
//! `docs/parity/notes/r5-js.md` §3.8.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// Every TS2352 of a checked JS file, as the text at its span.
fn reports_2352(source: &str) -> Vec<String> {
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
    checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| d.message.code() == 2352)
        .map(|(_, d)| source[d.span.start as usize..d.span.end as usize].to_string())
        .collect()
}

#[test]
fn a_non_overlapping_jsdoc_cast_is_ts2352_at_its_type_node() {
    assert_eq!(reports_2352("const x = /** @type {string} */ (1);\n"), ["string"]);
}

#[test]
fn an_overlapping_jsdoc_cast_is_silent() {
    assert!(reports_2352("const x = /** @type {number} */ (1);\n").is_empty());
}
