//! Native block-scoped declaration export placement uses blockScopeContainer.
use tsr_binder::FileInfo;
use tsr_core::Arena;
#[test]
fn block_export_modifier_does_not_publish_namespace_member() {
    let arena = Arena::new();
    let source = "namespace N { { export function hidden() {} } export function visible() {} }";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        FileInfo { name: "main.ts", text: source },
    );
    let n = bound.lookup_local(parsed.source_file.node_id.unwrap(), "N").unwrap();
    assert!(!bound.symbols().get(n).exports.contains_key("hidden"));
    assert!(bound.symbols().get(n).exports.contains_key("visible"));
}
