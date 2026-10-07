//! Native exported import-equals is an external-module indicator for either RHS.
use tsr_binder::{FileInfo, SymbolFlags};
use tsr_core::Arena;

#[test]
fn exported_entity_alias_creates_module_not_global_alias() {
    let arena = Arena::new();
    let source = "export import Alias = Missing;";
    let parsed = tsr_parser::parse(&arena, source);
    assert!(tsr_binder::is_external_module(parsed.source_file));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "barrel.ts", text: source },
    );
    let root = parsed.source_file.node_id.unwrap();
    let module = bound.symbol_of(root).expect("external source module");
    let alias = bound.symbols().get(module).exports["Alias"];
    assert!(bound.symbols().get(alias).flags.contains(SymbolFlags::ALIAS));
    assert!(bound.global("Alias").is_none());
}

#[test]
fn unexported_entity_alias_does_not_create_external_module() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "import Alias = Missing;");
    assert!(!tsr_binder::is_external_module(parsed.source_file));
}
