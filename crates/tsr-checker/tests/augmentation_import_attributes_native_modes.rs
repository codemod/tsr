//! Import-clause targets preserve explicit resolution-mode attributes.
use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_binder::{BindResult, FileInfo};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::{Arena, ResolutionMode};
use tsr_parser::{ParseOptions, parse_into};

struct Host {
    import: NodeId,
    require: NodeId,
    modes: Vec<(NodeId, ResolutionMode)>,
}
impl ModuleHost for Host {
    fn module_resolution_found(&self, _: NodeId, name: &str) -> bool {
        name == "pkg"
    }
    fn resolved_module(&self, _: NodeId, _: &str) -> Option<NodeId> {
        None
    }
    fn resolved_module_in_mode(
        &self,
        _: NodeId,
        name: &str,
        mode: ResolutionMode,
    ) -> Option<NodeId> {
        if name != "pkg" {
            return None;
        }
        match mode {
            ResolutionMode::ESNext => Some(self.import),
            ResolutionMode::CommonJS => Some(self.require),
            _ => None,
        }
    }
    fn mode_for_usage_location(&self, _: NodeId, usage: NodeId) -> ResolutionMode {
        self.modes.iter().find(|(id, _)| *id == usage).unwrap().1
    }
}

#[test]
fn same_specifier_default_aliases_reach_distinct_native_mode_targets() {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = BindResult::empty();
    let mut roots = Vec::new();
    for (name, source) in [
        ("esm.d.mts", "declare const value: 'esm'; export default value;"),
        ("cjs.d.cts", "declare const value: 'cjs'; export = value;"),
        (
            "main.mts",
            "import type ESM from 'pkg' with { 'resolution-mode': 'import' }; import type CJS from 'pkg' with { 'resolution-mode': 'require' };",
        ),
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
    let mut modes = Vec::new();
    let mut aliases = Vec::new();
    for raw in 0..u32::try_from(nodes.len()).unwrap() {
        let id = NodeId::new(raw);
        if let Some(Node::ImportDeclaration(import)) = map.get(id) {
            let clause = import.import_clause.unwrap();
            let name = clause.name.unwrap().text;
            let mode =
                if name == "ESM" { ResolutionMode::ESNext } else { ResolutionMode::CommonJS };
            modes.push((import.module_specifier.unwrap().node_id().unwrap(), mode));
            aliases.push((name, bound.symbol_of(clause.node_id.unwrap()).unwrap()));
        }
    }
    let host = Host { import: roots[0], require: roots[1], modes };
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    for (name, alias) in aliases {
        let target = checker.resolve_alias(alias).unwrap();
        let ty = checker.get_type_of_symbol(target);
        assert_eq!(checker.type_to_string(ty), if name == "ESM" { "\"esm\"" } else { "\"cjs\"" });
    }
}
