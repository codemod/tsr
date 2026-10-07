//! Native `CommonJS` exports do not introduce value names into source lexical scope.
use tsr_binder::{FileInfo, SymbolFlags};
use tsr_core::Arena;
#[test]
fn exported_property_is_not_an_unqualified_local_or_global() {
    let arena = Arena::new();
    let source = "exports.value = 10; value++;";
    let parsed = tsr_parser::parse_with_options(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("main.js"),
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "main.js", text: source },
    );
    let root = parsed.source_file.node_id.unwrap();
    let module = bound.symbol_of(root).unwrap();
    assert!(bound.symbols().get(module).exports.contains_key("value"));
    assert_eq!(
        bound.resolve_name(&parsed.nodes, &parsed.node_map, root, "value", SymbolFlags::VALUE),
        None
    );
    assert!(bound.lookup_local(root, "exports").is_some());
}
