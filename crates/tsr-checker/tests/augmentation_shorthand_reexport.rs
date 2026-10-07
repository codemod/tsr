//! Native shorthand ambient members retain the module's canonical any identity.
use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo};
use tsr_checker::Checker;
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

#[test]
fn named_reexport_of_shorthand_module_has_computed_any_type() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = BindResult::empty();
    let mut root = None;
    for (name, source) in [
        ("declarations.d.ts", "declare module 'opaque';"),
        ("barrel.ts", "export { value } from 'opaque';"),
    ] {
        let parsed = parse_into(&arena, source, ParseOptions::for_file(name), &mut nodes, &mut map);
        assert!(parsed.diagnostics.is_empty());
        root = parsed.source_file.node_id;
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            FileInfo { name, text: source },
        );
    }
    let module = bound.symbol_of(root.unwrap()).unwrap();
    let alias = bound.symbols().get(module).exports["value"];
    let mut checker = Checker::new(&bound, &nodes, &map);
    let ty = checker.get_type_of_symbol(alias);
    assert_eq!(checker.type_to_string(ty), "any");
}
