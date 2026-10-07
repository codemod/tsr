//! The actual loader and Program retain serial identity under worker publication.

use std::fmt::Write as _;
use tsr_ast::NodeId;
use tsr_compiler::{FileLoader, LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions, JsxEmit, Tristate};
use tsr_vfs::FileSystem as _;

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
    assert_loader_identity(&host, &roots, false);
}

fn assert_loader_identity(host: &Host, roots: &[String], dependencies: bool) {
    let serial_arena = Arena::new();
    let arena = Arena::new();
    let serial = FileLoader::load(&serial_arena, host, options(roots, true));
    let parallel = FileLoader::load(&arena, host, options(roots, false));
    if dependencies && std::thread::available_parallelism().unwrap().get() >= 2 {
        assert!(parallel.statistics.dependency_parse_workers >= 2);
        assert!(parallel.statistics.dependency_parses_published >= 8);
        assert!(
            parallel.statistics.dependency_pending_peak
                <= 2 * parallel.statistics.dependency_parse_workers
        );
        assert_eq!(serial.statistics.dependency_parse_jobs, 0);
    }
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
fn dynamically_discovered_dependencies_keep_complete_serial_identity() {
    let (original, _) = fixture();
    let mut files = Vec::new();
    // A single root exposes a broad frontier: these files must be reached as
    // dependencies and cannot accidentally pass via the root preparation path.
    let mut root = String::new();
    for name in [
        "f0.ts",
        "f1.ts",
        "f2.ts",
        "f3.ts",
        "f4.ts",
        "f5.ts",
        "f6.ts",
        "f7.ts",
        "doc.js",
        "view.tsx",
        "data.json",
        "broken.ts",
        "empty.ts",
        "aug.ts",
        "script.ts",
        "other.ts",
    ] {
        writeln!(root, "/// <reference path=\"./{name}\" />").unwrap();
        files.push((format!("/{name}"), original.0.read_file(&format!("/{name}")).unwrap()));
    }
    // Fill the bounded queue before descending into a new, unqueued child.
    // That child must make progress serially rather than wait for free slots.
    files[0].1.push_str("\nimport './deep';\n");
    files.push(("/deep.ts".to_owned(), "export const deep = 1;".to_owned()));
    files.push((
        "/huge.ts".to_owned(),
        format!("{}\nexport const huge = 1;", "/*oversized*/".repeat(100_000)),
    ));
    root.push_str("/// <reference path=\"./huge.ts\" />\n/// <reference path=\"./missing.ts\" />\nexport {};\n");
    files.push(("/root.ts".to_owned(), root));
    let host = Host(tsr_vfs::InMemoryFileSystem::new(files, [], true));
    assert_loader_identity(&host, &["/root.ts".to_owned(), "/root.ts".to_owned()], true);
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
