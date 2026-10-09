//! A property named by a `unique symbol` type (`crate::unique_symbol_keys`;
//! pinned tsgo 5b1047d `getPropertyNameFromType`, `utilities.go:886`, and
//! `getPropertyNameNodeForSymbolFromNameType`, `nodebuilderimpl.go:2455`).
//!
//! Native control: `declarationEmitMappedTypeTemplateTypeofSymbol`'s `.types`
//! baseline prints the key `[timestampSymbol]` in its own file and in a file
//! that imports only `now`, and `[x.timestampSymbol]` in a file holding
//! `import * as x from "./a"`.
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, push_children};
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

const A: &str = "export declare const timestampSymbol: unique symbol;";

struct Host {
    roots: Vec<NodeId>,
}

impl ModuleHost for Host {
    fn resolved_module(&self, _: NodeId, specifier: &str) -> Option<NodeId> {
        (specifier == "./a").then(|| self.roots[0])
    }
    fn module_resolution_found(&self, _: NodeId, specifier: &str) -> bool {
        specifier == "./a"
    }
    fn file_path(&self, file: NodeId) -> Option<String> {
        Some(if file == self.roots[0] { "/a.ts" } else { "/b.ts" }.into())
    }
}

fn find(map: &NodeMap<'_>, root: NodeId, name: &str) -> Option<NodeId> {
    let node = map.get(root)?;
    if matches!(node, Node::Identifier(identifier) if identifier.text == name) {
        return Some(root);
    }
    let mut children = Vec::new();
    push_children(node, &mut children);
    children
        .into_iter()
        .filter_map(|child| child.node_id())
        .find_map(|child| find(map, child, name))
}

/// `(escaped name, symbol read back from it, spelling at b.ts's `site`)`.
fn spell(b: &str) -> (String, bool, String) {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in [("/a.ts", A), ("/b.ts", b)] {
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
    let declaration = nodes.parent(find(&map, roots[0], "timestampSymbol").unwrap()).unwrap();
    let symbol = bound.merged_symbol(bound.symbol_of(declaration).unwrap());
    let site = find(&map, roots[1], "site").unwrap();
    let host = Host { roots: roots.clone() };
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    let ty = checker.get_type_of_symbol(symbol);
    let name = checker.unique_symbol_property_name(ty).unwrap();
    let round_trip = checker.symbol_of_unique_property_name(&name) == Some(symbol);
    let spelled = checker.unique_symbol_property_name_at(symbol, Some(site));
    (name, round_trip, spelled)
}

#[test]
fn a_unique_symbol_names_its_property_by_its_escaped_name() {
    let (name, round_trip, _) = spell("export const site = 1;");
    assert!(name.starts_with("__@timestampSymbol@"), "{name}");
    assert!(round_trip);
}

#[test]
fn a_namespace_import_qualifies_the_key() {
    let (_, _, spelled) = spell("import * as x from \"./a\"; export const site = x;");
    assert_eq!(spelled, "[x.timestampSymbol]");
}

#[test]
fn an_unnamed_module_container_is_not_written() {
    // `getSymbolChain(module, …, yieldModuleSymbol = false)` answers nil, so
    // `symbolToExpression` writes the symbol alone, not `import("./a").`.
    let (_, _, spelled) = spell("export const site = 1;");
    assert_eq!(spelled, "[timestampSymbol]");
}

#[test]
fn a_named_import_is_the_bare_name() {
    let (_, _, spelled) = spell("import { timestampSymbol } from \"./a\"; export const site = 1;");
    assert_eq!(spelled, "[timestampSymbol]");
}
