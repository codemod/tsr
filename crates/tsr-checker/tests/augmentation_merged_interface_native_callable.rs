//! A merged interface contributes instance members, not callable-value exports.
use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, SymbolFlags};
use tsr_checker::Checker;
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

#[test]
fn merged_interface_does_not_erase_or_add_callable_value_properties() {
    let arena = Arena::new();
    let source =
        "function Merged(): number { return 1; } interface Merged { instanceOnly: string; }";
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
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
    let symbol = bound.resolve_name(&nodes, &map, root, "Merged", SymbolFlags::VALUE).unwrap();
    let mut checker = Checker::new(&bound, &nodes, &map);
    let value = checker.get_type_of_symbol(symbol);
    assert_eq!(checker.type_to_string(value), "() => number");
    assert_eq!(checker.get_property_of_type(value, "instanceOnly"), None);
    let instance = checker.get_declared_type_of_symbol(symbol);
    assert!(checker.get_property_of_type(instance, "instanceOnly").is_some());
}
