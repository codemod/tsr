use std::collections::BTreeMap;
use std::sync::{Condvar, Mutex};

use super::*;
use tsr_compiler::{FileLoader, LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions, ModuleKind, ModuleResolutionKind, Tristate};
use tsr_vfs::{CachedFileSystem, DirectoryEntries, InMemoryFileSystem};

struct Host<'a>(&'a dyn FileSystem);

impl tsr_module::types::ResolutionHost for Host<'_> {
    fn fs(&self) -> &dyn FileSystem {
        self.0
    }
    fn current_directory(&self) -> &str {
        "/"
    }
}

// Test-only frozen read results. There is no fallback: an incomplete plan must
// fail rather than silently turn an unknown path into a missing file.
struct PreparedFs<'a> {
    backing: &'a dyn FileSystem,
    texts: BTreeMap<String, Option<String>>,
}

impl FileSystem for PreparedFs<'_> {
    fn use_case_sensitive_file_names(&self) -> bool {
        self.backing.use_case_sensitive_file_names()
    }
    fn file_exists(&self, path: &str) -> bool {
        self.backing.file_exists(path)
    }
    fn read_file(&self, path: &str) -> Option<String> {
        self.texts.get(path).unwrap_or_else(|| panic!("read not in frozen plan: {path}")).clone()
    }
    fn directory_exists(&self, path: &str) -> bool {
        self.backing.directory_exists(path)
    }
    fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
        self.backing.get_accessible_entries(path)
    }
    fn realpath(&self, path: &str) -> String {
        self.backing.realpath(path)
    }
}

fn loaded_image(fs: &dyn FileSystem, options: LoadOptions) -> String {
    let arena = Arena::new();
    let cache = CachedFileSystem::new(fs);
    let loaded = FileLoader::load(&arena, &Host(&cache), options);
    assert!(loaded.file_names.iter().any(|f| f == "/src/b.ts"), "cycle is loaded");
    assert!(loaded.files.iter().any(|f| !f.jsdoc().is_empty()), "JSDoc is exercised");
    assert!(!loaded.requests.is_empty(), "resolutions are exercised");
    assert!(!loaded.traces.is_empty(), "trace replay is exercised");
    assert!(!loaded.package_redirects.is_empty(), "package identity redirects are exercised");
    // Includes complete parsed ASTs, node ranges, parents/flags/spans, JSDoc,
    // parse/loader diagnostics, requests/results, traces and package redirects.
    // Timings and capacities are deliberately excluded.
    format!(
        "{:?}\n{}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        loaded.file_names,
        loaded.lib_file_count,
        loaded.files,
        loaded.nodes,
        loaded.loader_diagnostics,
        loaded.requests,
        loaded.traces,
        loaded.package_redirects,
    )
}

fn checked_image(fs: &dyn FileSystem, options: LoadOptions) -> String {
    let arena = Arena::new();
    let cache = CachedFileSystem::new(fs);
    let program = Program::from_root_files(&arena, &Host(&cache), options.clone());
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(&options.compiler_options);
    checker.set_checked_files(
        program.root_and_referenced_files().iter().filter_map(|f| f.source_file().node_id),
    );
    for file in program.root_and_referenced_files() {
        checker.check_source_file(
            file.source_file().node_id.expect("parsed file root"),
            tsr_checker::check::FileContext {
                ambient: tsr_path::is_declaration_file_name(file.file_name()),
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
    }
    // Read every ID through the completed program's table and typed map. No
    // parser result or node ID crosses the read-worker boundary.
    let mut image = format!("{:?}\n{:?}", program.loader_diagnostics(), checker.diagnostics());
    for file in program.source_files() {
        for raw in file.node_range() {
            let id = tsr_ast::NodeId::new(raw);
            image.push_str(&format!(
                "\n{raw}:{:?}:{:?}:{:?}:{:?}:{:?}",
                program.nodes().kind(id),
                program.nodes().span(id),
                program.nodes().flags(id),
                program.nodes().parent(id),
                program.node_map().get(id),
            ));
        }
    }
    image
}

#[test]
fn prepared_text_preserves_complete_serial_loader_and_checker_images() {
    let files = [
        ("/src/a.ts", "import { b } from './b.js'; export const a = 1; const wrong: string = b;"),
        ("/src/b.ts", "import { a } from './a.js'; export const b: number = a;"),
        ("/src/global1.ts", "interface Shared { left: number }"),
        ("/src/global2.ts", "interface Shared { right: string }\nlet shared: Shared;"),
        ("/src/view.tsx", "export const view = <span />;"),
        ("/src/doc.js", "/** @param {number} x */\nexport function twice(x) { return x * 2; }"),
        ("/src/data.json", "{\"name\":\"control\",\"count\":2}"),
        ("/src/use.ts", "import {Token} from 'pkg'; import './nested/use.js'; let t: Token;"),
        ("/src/nested/use.ts", "import {Token} from 'pkg'; export let n: Token;"),
        (
            "/node_modules/pkg/package.json",
            "{\"name\":\"pkg\",\"version\":\"1.0.0\",\"types\":\"index.d.ts\"}",
        ),
        ("/node_modules/pkg/index.d.ts", "export class Token { value: number }"),
        (
            "/src/nested/node_modules/pkg/package.json",
            "{\"name\":\"pkg\",\"version\":\"1.0.0\",\"types\":\"index.d.ts\"}",
        ),
        ("/src/nested/node_modules/pkg/index.d.ts", "export class Token { value: number }"),
        ("/package.json", "{\"type\":\"module\"}"),
    ];
    let links = [("/link", "/src")];
    for case_sensitive in [true, false] {
        let fs = InMemoryFileSystem::new(
            files.iter().map(|(p, t)| ((*p).into(), (*t).into())),
            links.iter().map(|(p, t)| ((*p).into(), (*t).into())),
            case_sensitive,
        );
        let mut paths: Vec<String> = files.iter().map(|(p, _)| (*p).into()).collect();
        paths.extend(
            files
                .iter()
                .filter_map(|(p, _)| p.strip_prefix("/src/"))
                .map(|relative| format!("/link/{relative}")),
        );
        paths.extend(["/link/a.ts".into(), "/src/missing.ts".into(), "/src/a.ts".into()]);
        let mut options = LoadOptions {
            compiler_options: CompilerOptions {
                no_lib: Tristate::True,
                types: Some(vec![]),
                allow_js: Tristate::True,
                check_js: Tristate::True,
                resolve_json_module: Tristate::True,
                trace_resolution: Tristate::True,
                module: ModuleKind::NodeNext,
                module_resolution: ModuleResolutionKind::NodeNext,
                ..Default::default()
            },
            root_file_names: vec![
                "/src/a.ts".into(),
                "/src/global1.ts".into(),
                "/src/global2.ts".into(),
                "/src/view.tsx".into(),
                "/src/doc.js".into(),
                "/src/data.json".into(),
                "/src/use.ts".into(),
                "/link/a.ts".into(),
                "/src/missing.ts".into(),
                "/src/a.ts".into(),
            ],
            default_library_path: String::new(),
        };
        if !case_sensitive {
            paths.push("/SRC/A.ts".into());
            options.root_file_names.push("/SRC/A.ts".into());
        }
        let reference_load = loaded_image(&fs, options.clone());
        let reference_check = checked_image(&fs, options.clone());
        for workers in [1, 2, 4] {
            let mut texts = BTreeMap::new();
            let stats = prepare(&fs, &paths, workers, |index, path, text| {
                assert_eq!(path, paths[index]);
                assert_eq!(text, fs.read_file(path));
                texts.insert(path.to_owned(), text);
            })
            .unwrap();
            assert_eq!(stats.workers.iter().map(|w| w.reads).sum::<usize>(), paths.len());
            assert_eq!(stats.workers.iter().map(|w| w.failed).sum::<usize>(), 1);
            let prepared = PreparedFs { backing: &fs, texts };
            assert_eq!(loaded_image(&prepared, options.clone()), reference_load);
            assert_eq!(checked_image(&prepared, options.clone()), reference_check);
        }
    }
}

#[test]
fn empty_plan_and_invalid_counts_do_not_read() {
    let fs = InMemoryFileSystem::default();
    let stats = prepare(&fs, &[], 4, |_, _, _| panic!("empty plan")).unwrap();
    assert!(stats.workers.is_empty());
    assert_eq!(stats.max_batch_bytes, 0);
    for count in [0, 5, usize::MAX] {
        assert!(prepare(&fs, &[], count, |_, _, _| panic!("invalid count")).is_err());
    }
}

#[test]
fn backing_filesystems_and_owned_read_results_are_shareable() {
    fn require_sync<T: Sync>() {}
    fn require_send<T: Send>() {}
    require_sync::<OsFileSystem>();
    require_sync::<InMemoryFileSystem>();
    require_send::<Option<String>>();
}

#[test]
fn parsing_in_the_consumer_preserves_one_program_node_domain() {
    let files = [
        ("/a.ts", "export interface A<T> { value: T }"),
        ("/b.js", "/** @param {number} x */\nfunction twice(x) { return x * 2; }"),
        ("/c.json", "{\"items\":[1,2]}"),
        ("/bad.ts", "let = ;"),
    ];
    let fs =
        InMemoryFileSystem::new(files.iter().map(|(p, t)| ((*p).into(), (*t).into())), [], true);
    let paths: Vec<String> = ["/a.ts", "/missing.ts", "/b.js", "/c.json", "/bad.ts", "/a.ts"]
        .iter()
        .map(|p| (*p).into())
        .collect();
    let image = |workers: Option<usize>| {
        let arena = Arena::new();
        let mut nodes = tsr_ast::NodeTable::new();
        let mut map = tsr_ast::NodeMap::new();
        let mut records = Vec::new();
        // This closure holds the !Sync arena and exclusive tables. It executes
        // on the coordinator and need not implement Send or Sync.
        let consume = |index, path: &str, text: Option<String>| {
            let Some(text) = text else {
                records.push(format!("{index}:{path}:missing"));
                return;
            };
            let source = arena.alloc_str(&text);
            let parsed = tsr_parser::parse_into(
                &arena,
                source,
                tsr_parser::ParseOptions {
                    script_kind: tsr_parser::ScriptKind::from_file_name(path),
                    ..Default::default()
                },
                &mut nodes,
                &mut map,
            );
            if path == "/bad.ts" {
                assert!(!parsed.diagnostics.is_empty(), "parse failure is exercised");
            }
            records.push(format!(
                "{index}:{path}:{:?}:{:?}:{:?}:{:?}:{:?}",
                parsed.node_range,
                parsed.source_file,
                parsed.diagnostics,
                parsed.jsdoc,
                parsed.file_references
            ));
        };
        if let Some(workers) = workers {
            prepare(&fs, &paths, workers, consume).unwrap();
        } else {
            prepare_serial(&fs, &paths, consume);
        }
        let recovered: Vec<_> = (0..nodes.len())
            .map(|raw| {
                let id = tsr_ast::NodeId::new(u32::try_from(raw).unwrap());
                format!("{:?}", map.get(id))
            })
            .collect();
        format!("{records:?}\n{nodes:?}\n{recovered:?}")
    };
    let serial = image(None);
    for workers in [1, 2, 4] {
        assert_eq!(image(Some(workers)), serial);
    }
}

#[test]
fn unwinding_consumer_closes_workers_without_waiting_for_more_tasks() {
    let fs = InMemoryFileSystem::default();
    let paths = vec!["/missing1.ts".into(), "/missing2.ts".into(), "/missing3.ts".into()];
    let result = std::panic::catch_unwind(|| {
        let _ = prepare(&fs, &paths, 2, |_, _, _| panic!("consumer failed"));
    });
    assert!(result.is_err());
}

// The first read cannot finish until the other three workers have completed
// their reads. This proves overlap and out-of-order completion without sleeps.
struct ReversedFs {
    remaining: Mutex<usize>,
    ready: Condvar,
}

impl FileSystem for ReversedFs {
    fn read_file(&self, path: &str) -> Option<String> {
        let mut remaining = self.remaining.lock().unwrap();
        if path == "/0" {
            while *remaining != 0 {
                remaining = self.ready.wait(remaining).unwrap();
            }
        } else {
            *remaining -= 1;
            self.ready.notify_all();
        }
        Some(path.repeat(7))
    }
    fn use_case_sensitive_file_names(&self) -> bool {
        panic!("metadata on worker")
    }
    fn file_exists(&self, _: &str) -> bool {
        panic!("metadata on worker")
    }
    fn directory_exists(&self, _: &str) -> bool {
        panic!("metadata on worker")
    }
    fn get_accessible_entries(&self, _: &str) -> DirectoryEntries {
        panic!("metadata on worker")
    }
    fn realpath(&self, _: &str) -> String {
        panic!("metadata on worker")
    }
}

#[test]
fn overlapping_reads_replay_in_plan_order_with_a_bounded_batch() {
    let fs = ReversedFs { remaining: Mutex::new(3), ready: Condvar::new() };
    let paths: Vec<_> = (0..4).map(|i| format!("/{i}")).collect();
    let mut received = Vec::new();
    let stats = prepare(&fs, &paths, 4, |index, path, text| {
        received.push((index, path.to_owned(), text.unwrap()));
    })
    .unwrap();
    assert_eq!(
        received,
        paths.iter().enumerate().map(|(i, p)| (i, p.clone(), p.repeat(7))).collect::<Vec<_>>()
    );
    assert_eq!(stats.max_batch_bytes, 56);
    assert!(stats.workers.iter().all(|w| w.reads == 1 && w.failed == 0));
}
