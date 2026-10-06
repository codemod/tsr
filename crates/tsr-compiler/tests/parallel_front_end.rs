//! The actual loader and Program retain serial identity under worker publication.

use std::fmt::Write as _;
use tsr_ast::NodeId;
use tsr_compiler::{FileLoader, LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions, JsxEmit, Tristate};

struct Host(tsr_vfs::InMemoryFileSystem);
impl tsr_module::types::ResolutionHost for Host {
    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        &self.0
    }
    fn current_directory(&self) -> &'static str {
        "/"
    }
}

fn fixture() -> (Host, Vec<String>) {
    let mut files = Vec::new();
    for i in 0..8 {
        let mut text = format!(
            "import type {{ I{}_0 }} from './f{}'; export {{ I{}_0 }};\n",
            (i + 1) % 8,
            (i + 1) % 8,
            (i + 1) % 8
        );
        for j in 0..500 {
            writeln!(text, "export interface I{i}_{j} {{ value: string; n: number; method(x: string): number; }}").unwrap();
        }
        files.push((format!("/f{i}.ts"), text));
    }
    files.extend([
        ("/script.ts".to_owned(), "interface Shared { a: string; } const duplicate = 1;".to_owned()),
        ("/other.ts".to_owned(), "interface Shared { b: number; } const duplicate = 2;".to_owned()),
        ("/aug.ts".to_owned(), "export {}; declare global { interface Shared { c: boolean; } }".to_owned()),
        ("/doc.js".to_owned(), "/** @typedef {{value: string}} Value */\n/** @param {Value} v */ function get(v) { return v.value; } module.exports = get;".to_owned()),
        ("/view.tsx".to_owned(), "export const view = <div data-name='a'>{1}</div>;".to_owned()),
        ("/data.json".to_owned(), "{\"value\": \"a\\nb\", \"list\": [true, 1, null]}".to_owned()),
        ("/broken.ts".to_owned(), "function broken( { return ;".to_owned()),
        ("/empty.ts".to_owned(), String::new()),
    ]);
    let mut roots: Vec<_> = files.iter().map(|(name, _)| name.clone()).collect();
    roots.push("/f0.ts".to_owned()); // First claim and spelling still win.
    (Host(tsr_vfs::InMemoryFileSystem::new(files, [], true)), roots)
}

fn options(roots: &[String], serial: bool) -> LoadOptions {
    LoadOptions {
        root_file_names: roots.to_vec(),
        compiler_options: CompilerOptions {
            no_lib: Tristate::True,
            allow_js: Tristate::True,
            allow_non_ts_extensions: Tristate::True,
            resolve_json_module: Tristate::True,
            trace_resolution: Tristate::True,
            extended_diagnostics: Tristate::True,
            single_threaded: Tristate::from_bool(serial),
            jsx: JsxEmit::Preserve,
            ..Default::default()
        },
        ..Default::default()
    }
}

#[test]
fn loader_replay_and_every_published_node_match_serial() {
    let (host, roots) = fixture();
    let serial_arena = Arena::new();
    let arena = Arena::new();
    let serial = FileLoader::load(&serial_arena, &host, options(&roots, true));
    let parallel = FileLoader::load(&arena, &host, options(&roots, false));
    assert_eq!(parallel.file_names, serial.file_names);
    assert_eq!(parallel.requests, serial.requests);
    assert_eq!(format!("{:?}", parallel.traces), format!("{:?}", serial.traces));
    assert_eq!(
        format!("{:?}", parallel.loader_diagnostics),
        format!("{:?}", serial.loader_diagnostics)
    );
    assert_eq!(parallel.package_redirects, serial.package_redirects);
    assert_eq!(parallel.statistics.parsed_files, serial.statistics.parsed_files);
    assert_eq!(parallel.nodes.len(), serial.nodes.len());
    for index in 0..parallel.nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        assert_eq!(parallel.nodes.parent(id), serial.nodes.parent(id));
        assert_eq!(parallel.nodes.kind(id), serial.nodes.kind(id));
        assert_eq!(parallel.nodes.flags(id), serial.nodes.flags(id));
        assert_eq!(parallel.nodes.span(id), serial.nodes.span(id));
        assert_eq!(
            format!("{:?}", parallel.node_map.get(id)),
            format!("{:?}", serial.node_map.get(id))
        );
    }
    for (actual, expected) in parallel.files.iter().zip(&serial.files) {
        assert_eq!(actual.node_range(), expected.node_range());
        assert_eq!(format!("{:?}", actual.jsdoc()), format!("{:?}", expected.jsdoc()));
        assert_eq!(format!("{:?}", actual.diagnostics()), format!("{:?}", expected.diagnostics()));
    }
}

#[test]
fn program_bind_identity_and_idempotence_match_serial() {
    let (host, roots) = fixture();
    let serial_arena = Arena::new();
    let arena = Arena::new();
    let serial = Program::from_root_files(&serial_arena, &host, options(&roots, true));
    let mut parallel = Program::from_root_files(&arena, &host, options(&roots, false));
    let a = parallel.binder();
    let e = serial.binder();
    assert_eq!(a.symbols().len(), e.symbols().len());
    for ((id, actual), (expected_id, expected)) in a.symbols().iter().zip(e.symbols().iter()) {
        assert_eq!(id, expected_id);
        assert_eq!(
            (
                actual.name,
                actual.flags,
                &actual.declarations,
                actual.value_declaration,
                actual.parent,
                actual.export_symbol
            ),
            (
                expected.name,
                expected.flags,
                &expected.declarations,
                expected.value_declaration,
                expected.parent,
                expected.export_symbol
            )
        );
        for (actual, expected) in
            [(&actual.members, &expected.members), (&actual.exports, &expected.exports)]
        {
            assert_eq!(actual.is_present(), expected.is_present());
            assert_eq!(
                actual.iter().collect::<std::collections::BTreeMap<_, _>>(),
                expected.iter().collect::<std::collections::BTreeMap<_, _>>()
            );
        }
    }
    for index in 0..parallel.nodes().len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        assert_eq!(a.symbol_of(id), e.symbol_of(id));
        assert_eq!(a.flow_of(id), e.flow_of(id));
        assert_eq!(a.locals(id), e.locals(id));
    }
    let identity = a.symbols().identity().clone();
    let count = a.symbols().len();
    let flow_count = a.flow().len();
    parallel.bind_source_files(&arena);
    assert_eq!(parallel.binder().symbols().identity(), &identity);
    assert_eq!(parallel.binder().symbols().len(), count);
    assert_eq!(parallel.binder().flow().len(), flow_count);
}
