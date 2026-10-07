//! Pinned native declaration emit gives `any` for every bound rename below.
//! These consumers exercise the binding value through typeof and a real call,
//! not just the presentation of a callable alias.
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn callable_type_binding_renames_flow_any_through_typeof_to_call_results() {
    let names = [
        "ordinary",
        "string",
        "number",
        "boolean",
        "any",
        "unknown",
        "never",
        "object",
        "symbol",
        "bigint",
        "undefined",
    ];
    for name in names {
        let source = format!(
            "type F = ({{a: {name}}}) => typeof {name}; declare const f: F; const result = f({{a: 1}}); type C = new ({{a: {name}}}) => typeof {name}; declare const c: C; const constructed = new c({{a: 1}});"
        );
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, &source);
        assert!(parsed.diagnostics.is_empty(), "{name}");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: &source },
        );
        let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        for consumer in ["result", "constructed"] {
            let result = bound.lookup_local(root, consumer).unwrap();
            let result_type = checker.get_type_of_symbol(result);
            assert_eq!(checker.type_to_string(result_type), "any", "{name}/{consumer}");
        }
    }
}
