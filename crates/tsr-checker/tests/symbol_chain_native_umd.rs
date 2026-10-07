//! Native 5b1047d `trySymbolTable`: UMD aliases are available to scripts,
//! never to external-module local-name lookup (even with allowUmdGlobalAccess).
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, push_children};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

struct Host;
impl ModuleHost for Host {
    fn resolved_module(&self, _: NodeId, _: &str) -> Option<NodeId> {
        None
    }
    fn module_resolution_found(&self, _: NodeId, _: &str) -> bool {
        false
    }
    fn file_path(&self, _: NodeId) -> Option<String> {
        Some("/lib.d.ts".into())
    }
}

fn site(map: &NodeMap<'_>, root: NodeId) -> Option<NodeId> {
    let node = map.get(root)?;
    if matches!(node, Node::Identifier(identifier) if identifier.text == "copy") {
        return Some(root);
    }
    let mut children = Vec::new();
    push_children(node, &mut children);
    children.into_iter().filter_map(|child| child.node_id()).find_map(|child| site(map, child))
}

fn print_module(source: &str) -> String {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in
        [("/lib.d.ts", "export const X = 1; export as namespace N;"), ("/use.ts", source)]
    {
        let parsed = tsr_parser::parse_into(
            &arena,
            text,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut map,
        );
        assert!(parsed.diagnostics.is_empty());
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            tsr_binder::FileInfo { name, text },
        );
    }
    let module = bound.symbol_of(roots[0]).unwrap();
    let host = Host;
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    let ty = checker.get_type_of_symbol(module);
    checker.type_to_string_at(ty, site(&map, roots[1]).unwrap()).unwrap()
}

#[test]
fn external_module_cannot_name_a_module_through_its_umd_global() {
    assert_eq!(print_module("const copy = N; export { copy };"), "typeof import(\"./lib\")");
}

#[test]
fn script_can_name_the_same_module_through_its_umd_global() {
    assert_eq!(print_module("const copy = N;"), "typeof N");
}
