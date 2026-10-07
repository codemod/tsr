//! Source-qualified `CommonJS` locals do not declare unresolved assignment names.
use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo, SymbolFlags};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

struct Host(NodeId);
impl ModuleHost for Host {
    fn resolved_module(&self, _: NodeId, name: &str) -> Option<NodeId> {
        (name == "c").then_some(self.0)
    }
    fn module_resolution_found(&self, _: NodeId, name: &str) -> bool {
        name == "c"
    }
}

#[test]
fn loaded_commonjs_source_does_not_publish_assignment_as_importable_export() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = BindResult::empty();
    let mut roots = Vec::new();
    for (name, source) in [
        ("node_modules/c.js", "exports.a = 10; c = 10;"),
        ("moduleA/a.js", "import { c } from 'c'; c++;"),
    ] {
        let parsed = parse_into(&arena, source, ParseOptions::for_file(name), &mut nodes, &mut map);
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            FileInfo { name, text: source },
        );
    }
    let module = bound.symbol_of(roots[0]).unwrap();
    assert!(bound.symbols().get(module).exports.contains_key("a"));
    assert!(!bound.symbols().get(module).exports.contains_key("c"));
    assert!(bound.global("c").is_none());
    let alias = bound.lookup_local(roots[1], "c").unwrap();
    let host = Host(roots[0]);
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    let ty = checker.get_type_of_symbol(alias);
    assert_eq!(checker.type_to_string(ty), "error");
}

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
