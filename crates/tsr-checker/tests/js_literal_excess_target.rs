//! `hasExcessProperties`' JS-literal exemption reads the TARGET, pinned to
//! 5b1047d1 (`relater.go:2715`): only a JS object literal's own type, made
//! without a contextual type (`ObjectFlagsJSLiteral`, `checker.go:13206`),
//! takes any property. A JS literal checked against a `@type` target is
//! elaborated like a TypeScript one. `docs/parity/notes/r6-jsdoc.md` §12.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

#[test]
fn a_js_literal_against_a_discriminated_type_reports_its_member() {
    let source =
        "/** @type {{ type: \"foo\" } | { type: \"bar\" }} */\nconst o = { type: \"other\" };\n";
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.js"));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    for (_, docs) in parsed.jsdoc.iter() {
        for doc in docs {
            if let Some(id) = doc.node_id {
                parsed.nodes.add_flags(id, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
            }
        }
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.js", text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let reported: Vec<_> = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| (d.message.code(), &source[d.span.start as usize..d.span.end as usize]))
        .collect();
    assert!(reported.contains(&(2322, "type")), "{reported:?}");
}
