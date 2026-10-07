//! Source-qualified `CommonJS` locals do not declare unresolved assignment names.
use tsr_ast::{NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, SymbolFlags};
use tsr_checker::Checker;
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

#[test]
fn commonjs_exports_local_retains_source_owner_and_assignment_is_not_global() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let source = "exports.a = 10; c = 10;";
    let parsed = parse_into(
        &arena,
        source,
        ParseOptions::for_file("node_modules/c.js"),
        &mut nodes,
        &mut map,
    );
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind_into(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &nodes,
        FileInfo { name: "node_modules/c.js", text: source },
    );
    let exports = bound.lookup_local(root, "exports").expect("native CommonJS local");
    let record = bound.symbols().get(exports);
    assert!(record.flags.contains(SymbolFlags::MODULE_EXPORTS));
    assert_eq!(record.value_declaration, Some(root));
    assert!(bound.global("c").is_none());
    assert!(bound.lookup_local(root, "c").is_none());
    let module = bound.symbol_of(root).expect("source module symbol");
    let mut checker = Checker::new(&bound, &nodes, &map);
    assert_eq!(checker.get_type_of_symbol(exports), checker.get_type_of_symbol(module));
}
