//! A JS variable's reparsed `@type` is the contextual root of its object
//! literal's members, pinned to 5b1047d1: `reparseHosted`'s
//! `KindJSDocTypeTag` arm makes it the declaration's `Type`, so a literal
//! member keeps its literal type under a unit-wanting member
//! (`isLiteralOfContextualType`). `docs/parity/notes/r6-jsdoc.md` §12.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn a_jsdoc_typed_literal_keeps_a_wanted_literal_member() {
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
    let literal = (0..parsed.nodes.len())
        .map(|index| NodeId::new(u32::try_from(index).unwrap()))
        .find(|&id| parsed.nodes.kind(id) == SyntaxKind::ObjectLiteralExpression)
        .expect("object literal");
    let Some(Node::ObjectLiteralExpression(node)) = parsed.node_map.get(literal) else {
        unreachable!()
    };
    let ty = checker.check_expression(tsr_ast::Expression::ObjectLiteralExpression(node));
    assert_eq!(checker.type_to_string(ty), "{ type: \"other\"; }");
}
