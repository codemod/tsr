//! Native NameResolver.Resolve admits the actual default-local name in ambient context.
use tsr_ast::{NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{FileInfo, NodeFacts, SymbolFlags};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

#[test]
fn ambient_default_local_resolves_for_its_meaning_and_respects_shadowing() {
    let arena = Arena::new();
    let text = "declare module 'pkg' { export default class Foo { value: number; } export { Foo as Named }; function test(Foo: string) { return Foo; } }";
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let parsed = parse_into(&arena, text, ParseOptions::for_file("pkg.d.ts"), &mut nodes, &mut map);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        parsed.source_file,
        &nodes,
        FileInfo { name: "pkg.d.ts", text },
    );
    let module = bound.ambient_module("pkg").unwrap();
    let default = bound.symbols().get(module).exports["default"];
    let mut export = None;
    let mut shadow = None;
    let mut module_node = None;
    for raw in parsed.node_range {
        let id = tsr_ast::NodeId::new(raw);
        match nodes.kind(id) {
            SyntaxKind::ExportSpecifier => export = Some(id),
            SyntaxKind::ReturnStatement => shadow = Some(id),
            SyntaxKind::ModuleDeclaration => module_node = Some(id),
            _ => {}
        }
    }
    assert!(bound.facts(module_node.unwrap()).contains(NodeFacts::AMBIENT_MODULE_CONTEXT));
    for meaning in [SymbolFlags::VALUE, SymbolFlags::TYPE, SymbolFlags::NAMESPACE] {
        let found = bound.resolve_name(&nodes, &map, export.unwrap(), "Foo", meaning);
        if meaning == SymbolFlags::NAMESPACE {
            assert_eq!(found, None);
        } else {
            assert_eq!(found, Some(default));
        }
    }
    let local =
        bound.resolve_name(&nodes, &map, shadow.unwrap(), "Foo", SymbolFlags::VALUE).unwrap();
    assert_ne!(local, default);
    assert!(bound.symbols().get(local).flags.intersects(SymbolFlags::FUNCTION_SCOPED_VARIABLE));
    assert_eq!(
        bound.resolve_name(&nodes, &map, export.unwrap(), "Missing", SymbolFlags::VALUE),
        None
    );
}
