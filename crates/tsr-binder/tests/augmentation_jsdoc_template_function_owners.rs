//! Native gathered template declarations retain their function owner and identity.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{BindResult, FileInfo};
use tsr_core::Arena;

#[test]
fn same_named_function_templates_do_not_merge_defaults_or_typedef_symbols() {
    let arena = Arena::new();
    let source = "/** @template [T=string] @typedef {T} A */\nlet a;\n/** @template T @template [U=T] @param {T} a @param {U} b */\nfunction first(a,b) {}\n/** @template [T=number] @param {T} a */\nfunction second(a) {}";
    let parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("main.js"),
    );
    let docs: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "main.js", text: source },
        &docs,
    );
    let root = parsed.source_file.node_id.unwrap();
    let mut functions = Vec::new();
    for raw in 0..parsed.nodes.len() {
        let id = NodeId::new(u32::try_from(raw).unwrap());
        if let Some(Node::FunctionDeclaration(f)) = parsed.node_map.get(id) {
            functions.push((f.name.unwrap().text, id));
        }
    }
    let first = functions.iter().find(|(name, _)| *name == "first").unwrap().1;
    let second = functions.iter().find(|(name, _)| *name == "second").unwrap().1;
    let t1 = bound.lookup_local(first, "T").expect("first owns T");
    let u1 = bound.lookup_local(first, "U").expect("first owns U");
    let t2 = bound.lookup_local(second, "T").expect("second owns T");
    assert_ne!(t1, t2);
    assert_eq!(bound.symbols().get(t1).declarations.len(), 1);
    assert_eq!(bound.symbols().get(t2).declarations.len(), 1);
    assert!(bound.lookup_local(root, "T").is_none());
    let u = bound.symbols().get(u1).declarations[0];
    let Node::TypeParameterDeclaration(u) = parsed.node_map.get(u).unwrap() else {
        panic!("type parameter")
    };
    let default = u.default_type.unwrap().node_id().unwrap();
    assert_eq!(parsed.nodes.kind(default), SyntaxKind::TypeReference);
    assert_eq!(
        bound.resolve_name(
            &parsed.nodes,
            &parsed.node_map,
            default,
            "T",
            tsr_binder::SymbolFlags::TYPE
        ),
        Some(t1)
    );
}
