//! `IsTypeSymbolAccessible` / `IsSymbolAccessible` as the type printer asks
//! them (`crate::alias_accessibility`; pinned tsgo 5b1047d
//! `symbolaccessibility.go:11`, `:839`, read by `nodebuilderimpl.go:3362`
//! and `:451`).
//!
//! Native controls (`.types` baselines at the pinned commit):
//! `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks1` prints the
//! non-exported `Id<…>` of `createApi.ts` structurally in `index.ts`;
//! `declarationEmitInlinedDistributiveConditional` names the exported
//! `PublicKeys1` from another module through an import type, so an exported
//! alias of another module is accessible (`allowModules`) while its
//! non-exported sibling `PublicKeys2` is not.
use tsr_ast::{Node, NodeId, NodeMap, NodeTable, push_children};
use tsr_binder::SymbolFlags;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

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

/// The first identifier spelled `name` under `root`.
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

/// Whether the type alias declared as `alias` in `/a.ts` is accessible at the
/// identifier `site` of `/b.ts`, with `meaning`.
fn accessible(a: &str, b: &str, alias: &str, site: &str, meaning: SymbolFlags) -> bool {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in [("/a.ts", a), ("/b.ts", b)] {
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
    let declaration = nodes.parent(find(&map, roots[0], alias).unwrap()).unwrap();
    let symbol = bound.symbol_of(declaration).unwrap();
    let site = find(&map, roots[1], site).unwrap();
    let host = Host { roots: roots.clone() };
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    checker.is_symbol_accessible_at(symbol, site, meaning)
}

const USE: &str = "import { f } from \"./a\"; export const site = f;";

#[test]
fn a_non_exported_alias_of_another_module_cannot_be_named() {
    // `IsAnySymbolAccessible` finds no chain from `/b.ts` and `Id` has no
    // container; `isSymbolAccessibleWorker` answers CannotBeNamed.
    let a = "type Id<T> = { [K in keyof T]: T[K] }; export declare function f(): Id<{}>;";
    assert!(!accessible(a, USE, "Id", "site", SymbolFlags::TYPE));
}

#[test]
fn an_exported_alias_of_another_module_is_named_through_its_module() {
    // `allowModules`: the container `/a.ts` is an external module, so the
    // alias is reachable as `import("./a").Id`.
    let a = "export type Id<T> = { [K in keyof T]: T[K] }; export declare function f(): Id<{}>;";
    assert!(accessible(a, USE, "Id", "site", SymbolFlags::TYPE));
}

#[test]
fn an_imported_alias_is_named_at_the_importing_site() {
    let a = "export type Id<T> = { [K in keyof T]: T[K] }; export declare function f(): Id<{}>;";
    let b = "import { f, Id } from \"./a\"; export const site = f;";
    assert!(accessible(a, b, "Id", "site", SymbolFlags::TYPE));
}

#[test]
fn a_non_exported_alias_is_accessible_in_its_own_file() {
    // The control for the first test: the same declaration printed from its
    // own module's scope is found by the chain walk in the file's locals.
    let a = "type Id<T> = { [K in keyof T]: T[K] }; export declare function f(): Id<{}>;";
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let parsed = tsr_parser::parse_into(
        &arena,
        a,
        tsr_parser::ParseOptions::default(),
        &mut nodes,
        &mut map,
    );
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "/a.ts", text: a },
    );
    let declaration = nodes.parent(find(&map, root, "Id").unwrap()).unwrap();
    let symbol = bound.symbol_of(declaration).unwrap();
    let site = find(&map, root, "f").unwrap();
    let mut checker = Checker::new(&bound, &nodes, &map);
    assert!(checker.is_type_symbol_accessible_at(symbol, site));
}
