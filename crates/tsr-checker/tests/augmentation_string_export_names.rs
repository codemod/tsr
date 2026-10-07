//! Native getExternalModuleMember uses decoded string export names, including empty.
use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_into};

struct Host(NodeId);
impl ModuleHost for Host {
    fn resolved_module(&self, _: NodeId, name: &str) -> Option<NodeId> {
        (name == "./a").then_some(self.0)
    }
    fn module_resolution_found(&self, _: NodeId, name: &str) -> bool {
        name == "./a"
    }
}

#[test]
fn empty_and_dashed_names_resolve_to_the_original_value_symbol() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = BindResult::empty();
    let mut roots = Vec::new();
    for (name, source) in [
        ("a.ts", "export const value = 1; export { value as \"\", value as \"a-b\" };"),
        ("main.ts", "import { \"\" as empty, \"a-b\" as dashed } from './a';"),
    ] {
        let parsed = parse_into(&arena, source, ParseOptions::for_file(name), &mut nodes, &mut map);
        assert!(parsed.diagnostics.is_empty());
        roots.push(parsed.source_file.node_id.unwrap());
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            parsed.source_file,
            &nodes,
            FileInfo { name, text: source },
        );
    }
    let host = Host(roots[0]);
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    for name in ["empty", "dashed"] {
        let alias = bound.lookup_local(roots[1], name).unwrap();
        let ty = checker.get_type_of_symbol(alias);
        assert_eq!(checker.type_to_string(ty), "1");
    }
}
