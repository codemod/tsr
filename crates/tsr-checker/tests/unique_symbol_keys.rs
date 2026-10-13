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
        match specifier {
            "./a" => Some(self.roots[0]),
            "./b" => self.roots.get(1).copied(),
            _ => None,
        }
    }
    fn module_resolution_found(&self, _: NodeId, specifier: &str) -> bool {
        matches!(specifier, "./a" | "./b")
    }
    fn file_path(&self, file: NodeId) -> Option<String> {
        let position = self.roots.iter().position(|&root| root == file)?;
        ["/a.ts", "/b.ts", "/c.ts"].get(position).map(|name| (*name).to_owned())
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

/// Distinct same-spelled exports are separate keys; namespace and renamed
/// imports of one export are the same key on inheritance and composites.
#[test]
fn computed_member_identity_survives_aliases_and_rejects_real_conflicts() {
    let files = [
        (
            "/a.ts",
            "export declare const symbol: unique symbol;\nexport interface Hash { [symbol](): number }",
        ),
        (
            "/b.ts",
            "export declare const symbol: unique symbol;\nexport interface Stream<T> { [symbol]: T }",
        ),
        (
            "/c.ts",
            "import * as A from './a';\n\
                   import * as B from './b';\n\
                   import { symbol as key } from './b';\n\
                   export declare const symbol: unique symbol;\n\
                   interface Equal extends A.Hash { [symbol](that: Equal): boolean }\n\
                   class Good implements B.Stream<number> { [key] = 1 }\n\
                   declare const good: Good; const read: number = good[B.symbol];\n\
                   declare const composite: { [key]: number } | { [B.symbol]: number };\n\
                   const combined: number = composite[key];\n\
                   interface Left { [key]?: { a?: true } }\n\
                   interface Right { [B.symbol]?: { b?: true } }\n\
                   interface Own extends Left, Right { [key]?: { a?: true, b?: true } }\n\
                   interface Wrong extends A.Hash { [A.symbol](that: Wrong): boolean }\n\
                   class Missing implements B.Stream<number> { [A.symbol] = 1 }\n\
                   class Bad implements B.Stream<number> { [key] = 'bad' }\n\
                   interface Unresolved extends Left, Right {}",
        ),
    ];
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let mut bound = tsr_binder::BindResult::empty();
    let mut roots = Vec::new();
    for (name, text) in files {
        let parsed = tsr_parser::parse_into(
            &arena,
            text,
            tsr_parser::ParseOptions::for_file(name),
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
    let host = Host { roots: roots.clone() };
    let mut checker = Checker::with_module_host(&bound, &nodes, &map, Some(&host));
    for &root in &roots {
        checker.check_source_file(
            root,
            tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
        );
    }
    let mut errors: Vec<_> = checker
        .diagnostics()
        .iter()
        .map(|(file, diagnostic)| {
            assert_eq!(*file, roots[2]);
            let line = files[2].1[..diagnostic.span.start as usize].matches('\n').count() + 1;
            (line, diagnostic.message.code())
        })
        .collect();
    errors.sort_unstable();
    assert_eq!(errors, [(13, 2430), (14, 2420), (15, 2416), (16, 2320)]);
}
