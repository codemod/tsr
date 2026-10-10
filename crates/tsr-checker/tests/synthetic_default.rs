//! Synthetic `default` on a namespace import under `module: node16`+
//! (tsr-2zk.992): `getTypeWithSyntheticDefaultOnly` (`checker.go:15632`) and
//! `getTypeWithSyntheticDefaultImportType` (`checker.go:15646`), reached from
//! `resolveESModuleSymbol` (`checker.go:15568`).
//! `docs/parity/notes/r5-modexports.md` §2–§3.
//!
//! | mutation | reddens |
//! |---|---|
//! | `get_type_with_synthetic_default_only` answers `None` | [`an_es_namespace_import_of_json_has_only_a_default`] **only** |
//! | `namespace_import_default_member_type` answers `None` | [`a_commonjs_namespace_import_of_json_with_a_default_member_is_spread`] **only** |
//! | `namespace_import_default_member_type` skips the `default`-member test | [`a_commonjs_namespace_import_of_json_without_a_default_member_is_the_json`] **only** |
#![allow(
    clippy::case_sensitive_file_extension_comparisons,
    reason = "test file names are lowercase literals"
)]

use tsr_ast::{NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::{Checker, resolution::ModuleHost};
use tsr_core::{Arena, ModuleKind};

/// `"./x"` names the fixture `x`; usages emit as ESM from `.mts` files and as
/// `CommonJS` from `.cts` files (`GetEmitSyntaxForUsageLocation`).
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

    fn emit_syntax_for_usage_location(&self, importing_file: NodeId, _usage: NodeId) -> ModuleKind {
        match self.files.iter().find(|&&(_, id)| id == importing_file) {
            Some((name, _)) if name.ends_with(".mts") => ModuleKind::ESNext,
            Some((name, _)) if name.ends_with(".cts") => ModuleKind::CommonJS,
            _ => ModuleKind::None,
        }
    }
}

struct Fixture<'a> {
    nodes: NodeTable,
    node_map: NodeMap<'a>,
    bound: BindResult<'a>,
    host: Fixtures,
}

/// Parse and bind several files into one identity space (ADR-0034); a
/// `.json` fixture parses as JSON.
fn program<'a>(arena: &'a Arena, files: &[(&'static str, &str)]) -> Fixture<'a> {
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &'a str = arena.alloc_str(source);
        let options = tsr_parser::ParseOptions {
            script_kind: if name.ends_with(".json") {
                tsr_parser::ScriptKind::Json
            } else {
                tsr_parser::ScriptKind::TypeScript
            },
            ..tsr_parser::ParseOptions::default()
        };
        let file = tsr_parser::parse_into(arena, source, options, &mut nodes, &mut node_map);
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
        parsed.push((*name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    let mut host = Fixtures { files: Vec::new() };
    for (name, source, source_file) in parsed {
        let file_name: &'a str = arena.alloc_str(&format!("/{name}"));
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

/// The type of the namespace import `name`, checked under `module: node16`.
fn type_of_namespace_import(fixture: &Fixture<'_>, name: &str) -> String {
    let mut checker = Checker::with_module_host(
        &fixture.bound,
        &fixture.nodes,
        &fixture.node_map,
        Some(&fixture.host),
    );
    let options = tsr_core::CompilerOptions {
        module: ModuleKind::Node16,
        ..tsr_core::CompilerOptions::default()
    };
    checker.apply_compiler_options(&options);
    let symbol = (0..u32::try_from(fixture.nodes.len()).expect("node count fits in u32"))
        .map(NodeId::new)
        .filter(|&id| fixture.nodes.kind(id) == SyntaxKind::NamespaceImport)
        .filter_map(|id| fixture.bound.symbol_of(id))
        .find(|&symbol| fixture.bound.symbols().get(symbol).name == name)
        .unwrap_or_else(|| panic!("no namespace import named `{name}`"));
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn an_es_namespace_import_of_json_has_only_a_default() {
    // `isOnlyImportableAsDefault`: JSON modules get no named exports in ESM.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("config.json", "{ \"version\": 1 }"),
            ("main.mts", "import * as ns from './config.json'; ns;"),
        ],
    );
    assert_eq!(type_of_namespace_import(&fixture, "ns"), "{ default: { version: number; }; }");
}

#[test]
fn a_commonjs_namespace_import_of_json_with_a_default_member_is_spread() {
    // A JSON module binds its value as `export =`, so it can have a synthetic
    // default (`canHaveSyntheticDefault`), which overrides its own `default`.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("package.json", "{ \"name\": \"pkg\", \"default\": \"misdirection\" }"),
            ("main.cts", "import * as ns from './package.json'; ns;"),
        ],
    );
    assert_eq!(
        type_of_namespace_import(&fixture, "ns"),
        "{ name: string; default: { name: string; default: string; }; }"
    );
}

#[test]
fn a_commonjs_namespace_import_of_json_without_a_default_member_is_the_json() {
    // `resolveESModuleSymbol`'s third arm needs signatures, a `default`
    // member, or an ESM-to-CommonJS reference; none holds.
    let arena = Arena::new();
    let fixture = program(
        &arena,
        &[
            ("config.json", "{ \"version\": 1 }"),
            ("main.cts", "import * as ns from './config.json'; ns;"),
        ],
    );
    assert_eq!(type_of_namespace_import(&fixture, "ns"), "{ version: number; }");
}
