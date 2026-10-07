//! Native module export lookup excludes the internal default key from lexical scope.
use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, SymbolFlags};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

#[test]
fn default_export_key_is_not_a_local_name_but_named_default_is() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let source = "export default function Real() { return true; }";
    let parsed =
        parse_into(&arena, source, ParseOptions::for_file("main.ts"), &mut nodes, &mut map);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind_into(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &nodes,
        FileInfo { name: "main.ts", text: source },
    );
    let module = bound.symbol_of(root).unwrap();
    let default = bound.symbols().get(module).exports["default"];
    assert_eq!(bound.resolve_name(&nodes, &map, root, "Real", SymbolFlags::VALUE), Some(default));
    assert_eq!(bound.resolve_name(&nodes, &map, root, "default", SymbolFlags::VALUE), None);
    assert_eq!(bound.resolve_name(&nodes, &map, root, "default", SymbolFlags::TYPE), None);
    assert_eq!(bound.resolve_name(&nodes, &map, root, "default", SymbolFlags::NAMESPACE), None);
}
