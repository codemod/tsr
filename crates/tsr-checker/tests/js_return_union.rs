//! A JS function's return inference over two or more return types, pinned to
//! 5b1047d1: `getReturnTypeFromBody` (`checker.go:20191`) unions them under
//! `UnionReductionSubtype` in a JS file as in a TS one. The JS decline this
//! replaces stood in for unported `@overload` declarations, which
//! `docs/parity/notes/r6-jsdoc-overload.diff` ports. `docs/parity/notes/r6-jsdoc.md`
//! §5.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

fn function_type(source: &str, name: &str) -> String {
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
        if parsed.nodes.kind(id) != SyntaxKind::FunctionDeclaration {
            continue;
        }
        if let Some(Node::FunctionDeclaration(node)) = parsed.node_map.get(id)
            && node.name.is_some_and(|n| n.text == name)
        {
            let symbol = bound.symbol_of(id).expect("bound function");
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("no function {name}");
}

/// `unreachableJavascriptChecked`, `assertionsAndNonReturningFunctions`' `f2`.
#[test]
fn two_return_types_union_in_a_js_function() {
    let source = "/** @param {boolean} b */\nfunction f(b) { if (b) return 1; return \"a\"; }\n";
    assert_eq!(function_type(source, "f"), "(b: boolean) => \"a\" | 1");
}
