//! String-literal module export names: `export { x as "<X>" }` and
//! `import { "<X>" as y }` (tsr-2zk.991).
//!
//! `getExternalModuleMember` (`checker.go:14667`) reads
//! `specifier.PropertyNameOrName().Text()`, and the binder keys a module's
//! `exports` by the same text, so a string-literal name is looked up exactly as
//! an identifier is. `docs/parity/notes/r5-modexports.md` §1.
//!
//! | mutation | reddens |
//! |---|---|
//! | `get_external_module_member` answers `None` for a string-literal name again | all three positive tests |
//! | `module_export_name_text` returns `""` for a string literal | all three positive tests |

use tsr_ast::{NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

/// `"./m"` names the fixture called `m` (as in `tests/cross_file_aliases.rs`).
struct Fixtures {
    files: Vec<(&'static str, NodeId)>,
}

impl ModuleHost for Fixtures {
    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }

    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let name = specifier.strip_prefix("./").unwrap_or(specifier);
        self.files.iter().find(|(fixture, _)| *fixture == name).map(|&(_, id)| id)
    }
}

struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
    host: Fixtures,
}

/// Parse and bind several files into one identity space (ADR-0034).
fn program<'a>(arena: &'a Arena, files: &[(&'static str, &str)]) -> Fixture<'a> {
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &'a str = arena.alloc_str(source);
        let file = tsr_parser::parse_into(
            arena,
            source,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut node_map,
        );
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
        parsed.push((*name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let file_name: &'a str = arena.alloc_str(&format!("/{name}.ts"));
        host.files.push((name, source_file.node_id.expect("a parsed file has an id")));
        bound = tsr_binder::bind_into(
            bound,
            arena,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    Fixture { nodes, node_map, bound, host }
}

/// The type of the alias an import or export specifier binds under `name`.
fn type_of_alias(fixture: &Fixture<'_>, name: &str) -> String {
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let symbol = (0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32"))
        .map(NodeId::new)
        .filter(|&id| {
            matches!(
                fixture.nodes.kind(id),
                SyntaxKind::ImportSpecifier | SyntaxKind::ExportSpecifier
            )
        })
        .filter_map(|id| fixture.bound.symbol_of(id))
        .find(|&symbol| fixture.bound.symbols().get(symbol).name == name)
        .unwrap_or_else(|| panic!("no import or export specifier named `{name}`"));
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn a_named_import_of_a_string_literal_export_resolves() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "const v = 17; export { v as \"<X>\" };"),
            ("main", "import { \"<X>\" as y } from './m'; y;"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "y"), "17");
}

#[test]
fn a_string_literal_re_export_of_a_string_literal_export_resolves() {
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("m", "const v = 17; export { v as \"<X>\" };"),
            ("r", "export { \"<X>\" as \"<Y>\" } from './m';"),
            ("main", "import { \"<Y>\" as y } from './r'; y;"),
        ],
    );
    assert_eq!(type_of_alias(&fixture, "<Y>"), "17");
    assert_eq!(type_of_alias(&fixture, "y"), "17");
}

#[test]
fn a_string_literal_name_that_spells_an_identifier_export_resolves() {
    // The binder keys `exports` by text, so `"v"` and `v` are one key.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[("m", "export const v = 17;"), ("main", "import { \"v\" as y } from './m'; y;")],
    );
    assert_eq!(type_of_alias(&fixture, "y"), "17");
}
