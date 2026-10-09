//! TS2344 on an import type's type arguments: `checkImportType`'s
//! `checkTypeReferenceOrImport` (`checker.go:2998`, `:3324`) over the symbol
//! `getTypeFromImportTypeNode` (`checker.go:24575`) resolves. Expectations
//! were checked against a native `tsgo` built from the pinned submodule.

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

/// The TS2344 messages reported for the last fixture file.
fn ts2344(files: &[(&'static str, &'static str)]) -> Vec<String> {
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
        .filter(|(_, d)| d.code() == "TS2344")
        .map(|(_, d)| d.text())
        .collect();
    messages.sort();
    messages
}

#[test]
fn an_unconstrained_type_parameter_argument_reports() {
    assert_eq!(
        ts2344(&[
            ("file1", "export type Foo<T extends string> = { foo: T }"),
            ("file2", "type Bar<T> = import('./file1').Foo<T>;"),
        ]),
        ["Type 'T' does not satisfy the constraint 'string'."]
    );
}

#[test]
fn a_satisfying_argument_does_not_report() {
    assert!(
        ts2344(&[
            ("file1", "export type Foo<T extends string> = { foo: T }"),
            ("file2", "type Ok = import('./file1').Foo<\"a\">;"),
        ])
        .is_empty()
    );
}

#[test]
fn a_namespace_qualifier_is_walked() {
    assert_eq!(
        ts2344(&[
            ("file1", "export namespace N { export interface Box<T extends string> { v: T } }"),
            ("file2", "type B = import('./file1').N.Box<number>;"),
        ]),
        ["Type 'number' does not satisfy the constraint 'string'."]
    );
}
