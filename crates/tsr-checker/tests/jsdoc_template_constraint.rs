//! A JSDoc `@template {C} T` constrains the reparsed type parameter of a JS
//! function's signature, pinned to 5b1047d1: `gatherTypeParameters`
//! (`parser/reparser.go:293`) gives the tag's first parameter the
//! constraint. `docs/parity/notes/r5-jsdoc5.md` §4.4.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of every declaration of `kind` named `name` in a JS file, in
/// source order, skipping parameters of a comment's own signature types.
fn declaration_types(source: &str, kind: SyntaxKind, name: &str) -> Vec<String> {
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.js"));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    // As the loader stamps a JS file: the root and every comment's root.
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
    let mut out = Vec::new();
    for index in 0..parsed.nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        if parsed.nodes.kind(id) != kind
            || parsed
                .nodes
                .parent(id)
                .is_some_and(|parent| matches!(parsed.nodes.kind(parent), SyntaxKind::FunctionType))
        {
            continue;
        }
        let named = match parsed.node_map.get(id) {
            Some(Node::ParameterDeclaration(node)) => {
                matches!(node.name, Some(tsr_ast::BindingName::Identifier(n)) if n.text == name)
            }
            Some(Node::FunctionDeclaration(node)) => node.name.is_some_and(|n| n.text == name),
            Some(Node::BindingElement(node)) => {
                matches!(node.name, Some(tsr_ast::BindingName::Identifier(n)) if n.text == name)
            }
            _ => false,
        };
        if !named {
            continue;
        }
        let symbol = bound.symbol_of(id).expect("bound declaration");
        let ty = checker.get_type_of_symbol(symbol);
        out.push(checker.type_to_string(ty));
    }
    out
}
/// `jsdocTemplateTag3`.
#[test]
fn a_template_constraint_is_the_signature_parameter_constraint() {
    let source = "/**\n * @template {{ a: number }} T\n * @param {T} t\n * @return {T}\n */\nfunction f(t) { return t; }\n";
    assert_eq!(
        declaration_types(source, SyntaxKind::FunctionDeclaration, "f"),
        ["<T extends { a: number; }>(t: T) => T"]
    );
}
