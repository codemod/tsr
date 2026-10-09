//! An unannotated set-accessor parameter reads the getter's type
//! (`getTypeForVariableLikeDeclaration`, `checker.go:16719`), after
//! `tryGetTypeFromEffectiveTypeNode`. In a JavaScript file the reparsed
//! `@param` is that effective type node, so a documented parameter keeps its
//! tag and an undocumented one reads the getter. `docs/parity/notes/r5-js.md`
//! §3.1.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first set accessor's parameter in a JS file.
fn setter_parameter_type(source: &str) -> String {
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
    let parameter = (0..u32::try_from(parsed.nodes.len()).expect("fits"))
        .map(NodeId::new)
        .find(|&id| {
            parsed.nodes.kind(id) == SyntaxKind::Parameter
                && parsed
                    .nodes
                    .parent(id)
                    .is_some_and(|p| parsed.nodes.kind(p) == SyntaxKind::SetAccessor)
        })
        .expect("a setter parameter");
    let symbol = bound.symbol_of(parameter).expect("bound");
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn an_undocumented_js_setter_parameter_reads_the_getter() {
    let source = "export const t = {\n  get value() { return 'v'; },\n  set value(v) {},\n};\n";
    assert_eq!(setter_parameter_type(source), "string");
}

#[test]
fn a_param_tag_on_a_js_setter_wins_over_the_getter() {
    let source = "class C {\n  /** @returns {string} */\n  get p() { return ''; }\n  /** @param {number | string} p */\n  set p(p) {}\n}\n";
    assert_eq!(setter_parameter_type(source), "string | number");
}
