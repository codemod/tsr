//! TS2308 from `getExportsOfModuleWorker`'s collision table
//! (`checker.go:16148`, `extendExportSymbols` at `:16235`).
//!
//! Two `export *` that bring the same name with different resolved symbols
//! report on every star after the first, naming the first star's specifier
//! as written; a name the module exports itself, the same symbol reached
//! twice, and `default` do not report.

use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::BindResult;
use tsr_checker::check::FileContext;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::Arena;

struct Fixtures {
    files: Vec<(&'static str, NodeId, &'static str)>,
}

impl ModuleHost for Fixtures {
    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool {
        self.resolved_module(importing_file, specifier).is_some()
    }

    fn resolved_module(&self, _importing_file: NodeId, specifier: &str) -> Option<NodeId> {
        let name = specifier.strip_prefix("./").unwrap_or(specifier);
        self.files.iter().find(|(fixture, ..)| *fixture == name).map(|&(_, id, _)| id)
    }

    fn source_text(&self, file: NodeId, _nodes: &NodeTable) -> Option<&str> {
        self.files.iter().find(|(_, id, _)| *id == file).map(|&(.., text)| text)
    }
}

/// The TS2308 messages reported for the last fixture file.
fn ts2308(files: &[(&'static str, &'static str)]) -> Vec<String> {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for &(name, source) in files {
        let file = tsr_parser::parse_into(
            &arena,
            source,
            tsr_parser::ParseOptions::default(),
            &mut nodes,
            &mut node_map,
        );
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
        parsed.push((name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let file_name: &str = arena.alloc_str(&format!("/{name}.ts"));
        host.files.push((name, source_file.node_id.expect("a parsed file has an id"), source));
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name: file_name, text: source },
        );
    }
    let root = host.files.last().expect("a fixture").1;
    let mut checker = Checker::with_module_host(&bound, &nodes, &node_map, Some(&host));
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let mut messages: Vec<String> = checker
        .diagnostics()
        .iter()
        .filter(|(_, d)| d.code() == "TS2308")
        .map(|(_, d)| d.text())
        .collect();
    messages.sort();
    messages
}

#[test]
fn a_name_two_stars_bring_reports_on_the_later_star() {
    assert_eq!(
        ts2308(&[
            ("t1", "export var x = 1; export var y = 2;"),
            ("t2", "export default 1; export function foo() {}"),
            ("t3", "var x = 1; var y = 2; export { x, y };"),
            ("t4", "export * from './t1';\nexport * from \"./t2\";\nexport * from \"./t3\";"),
        ]),
        vec![
            "Module './t1' has already exported a member named 'x'. Consider explicitly re-exporting to resolve the ambiguity.",
            "Module './t1' has already exported a member named 'y'. Consider explicitly re-exporting to resolve the ambiguity.",
        ]
    );
}

#[test]
fn a_name_the_module_exports_itself_does_not_report() {
    assert!(
        ts2308(&[
            ("a", "export var x = 1;"),
            ("b", "export var x = 2;"),
            ("c", "export * from './a'; export * from './b'; export var x = 3;"),
        ])
        .is_empty()
    );
}

#[test]
fn the_same_symbol_reached_twice_does_not_report() {
    assert!(
        ts2308(&[
            ("a", "export var x = 1;"),
            ("b", "export * from './a';"),
            ("c", "export * from './a'; export * from './b';"),
        ])
        .is_empty()
    );
}

#[test]
fn default_is_never_re_exported_by_a_star() {
    assert!(
        ts2308(&[
            ("a", "export default 1;"),
            ("b", "export default 2;"),
            ("c", "export * from './a'; export * from './b';"),
        ])
        .is_empty()
    );
}
