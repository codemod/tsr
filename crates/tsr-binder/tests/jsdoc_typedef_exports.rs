//! A top-level `@typedef` in a JS module is an implicitly exported
//! `JSTypeAliasDeclaration` (`ast.IsImplicitlyExportedJSDocDeclaration`),
//! bound into the module's exports with `SymbolFlagsTypeAliasExcludes`
//! (`binder.go:1600`, `:375`). Expectations were checked against a native
//! `tsgo` built from the pinned submodule.

use tsr_ast::NodeFlags;
use tsr_binder::{BindResult, FileInfo};
use tsr_core::Arena;

fn ts2300_spans(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let mut parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("index.js"),
    );
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.expect("registered source file");
    parsed.nodes.add_flags(root, NodeFlags::JAVASCRIPT_FILE);
    let docs: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "index.js", text: source },
        &docs,
    );
    let mut spans: Vec<String> = bound
        .diagnostics()
        .iter()
        .filter(|d| d.code() == "TS2300")
        .map(|d| source[d.span.start as usize..d.span.end as usize].to_string())
        .collect();
    spans.sort();
    spans
}

#[test]
fn a_typedef_named_default_conflicts_with_a_default_exported_class() {
    let source =
        "export default class C {};\n/**\n * @typedef {string | number} default\n */\nvar q;\n";
    assert_eq!(ts2300_spans(source), ["C", "default"]);
}

#[test]
fn a_typedef_beside_an_export_assignment_alias_does_not_conflict() {
    let source = "class Cls {}\nexport default Cls;\n/**\n * @typedef {string | number} default\n */\nvar q;\n";
    assert!(ts2300_spans(source).is_empty());
}
