//! Native bindExportAssignment uses the lexical container symbol, not member owner.
use tsr_binder::FileInfo;
use tsr_core::Arena;
#[test]
fn invalid_function_export_belongs_to_function_not_enclosing_namespace() {
    let arena = Arena::new();
    let source = "namespace N { export function f() { export = 0; } }";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "main.ts", text: source },
    );
    let n = bound.lookup_local(parsed.source_file.node_id.unwrap(), "N").unwrap();
    let f = bound.symbols().get(n).exports["f"];
    assert!(bound.symbols().get(f).exports.contains_key("export="));
    assert!(!bound.symbols().get(n).exports.contains_key("export="));
}
