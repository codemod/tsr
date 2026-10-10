//! Trailing comments document the end-of-file token, pinned to 5b1047d1:
//! `parseSourceFileWorker`'s `withJSDoc(eof, endJSDoc)`
//! (`parser/parser.go:438`), so a `@typedef` that ends the file is reparsed
//! into its statements like any other. `docs/parity/notes/r6-jsdoc.md` §8.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

fn variable_type(source: &str, name: &str) -> String {
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
    for index in 0..parsed.nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        if parsed.nodes.kind(id) != SyntaxKind::VariableDeclaration {
            continue;
        }
        if let Some(Node::VariableDeclaration(node)) = parsed.node_map.get(id)
            && matches!(node.name, Some(tsr_ast::BindingName::Identifier(n)) if n.text == name)
        {
            let symbol = bound.symbol_of(id).expect("bound variable");
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("no variable {name}");
}

/// `jsdocTypeDefAtStartOfFile`: a typedef after the last statement.
#[test]
fn a_typedef_ending_the_file_is_declared() {
    let source = "/** @type {Third} */\nvar c;\n\n/** @typedef {number} Third */\n";
    assert_eq!(variable_type(source, "c"), "number");
}
