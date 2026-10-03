//! The worker seam: immutable program inputs, checker-local types and caches.
//! These are ownership/determinism controls, not evidence of CLI parallel speed.

use tsr_compiler::{LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions, Idx, Tristate};
use tsr_module::types::ResolutionHost;
use tsr_vfs::{FileSystem, InMemoryFileSystem};

struct Host(InMemoryFileSystem);

impl ResolutionHost for Host {
    fn fs(&self) -> &dyn FileSystem {
        &self.0
    }

    fn current_directory(&self) -> &'static str {
        "/project"
    }
}

type DiagnosticImage = (usize, u32, u32, String);

fn check_group(
    program: &Program<'_>,
    options: &CompilerOptions,
    group: &[usize],
) -> Vec<DiagnosticImage> {
    // Neither Checker nor TypeId crosses a worker boundary. Every worker can
    // force types in any file, using the same read-only NodeId/SymbolId space.
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(program),
    );
    checker.apply_compiler_options(options);
    checker.set_checked_files(
        program.root_and_referenced_files().iter().filter_map(|file| file.source_file().node_id),
    );
    for &index in group {
        let file = &program.root_and_referenced_files()[index];
        checker.check_source_file(
            file.source_file().node_id.unwrap(),
            tsr_checker::check::FileContext {
                ambient: tsr_path::is_declaration_file_name(file.file_name()),
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
    }
    checker
        .diagnostics()
        .iter()
        .map(|(id, diagnostic)| {
            (id.index(), diagnostic.span.start, diagnostic.message.code(), diagnostic.text())
        })
        .collect()
}

#[test]
fn fully_bound_program_is_shareable_without_sharing_its_allocator() {
    fn assert_shareable<T: Send + Sync>() {}
    assert_shareable::<Program<'static>>();
}

#[test]
fn independent_checkers_preserve_cross_file_errors_and_merged_globals() {
    let sources = [
        ("model.ts", "export interface Box<T> { value: T } export class Service { value = 1; }"),
        (
            "a.ts",
            "import { Service } from './model'; import type { Box } from './model'; const a: Box<number> = { value: 'bad' }; const service = new Service(); const wrong: string = service.value;",
        ),
        (
            "b.ts",
            "import type { Box } from './model'; const b: Box<string> = { value: 1 }; const global: Shared = { first: 1, second: 'ok' };",
        ),
        ("globals-a.d.ts", "interface Shared { first: number }"),
        ("globals-b.d.ts", "interface Shared { second: string }"),
    ];
    let host = Host(InMemoryFileSystem::new(
        sources.iter().map(|(name, text)| (format!("/project/{name}"), (*text).to_string())),
        [],
        true,
    ));
    let arena = Arena::new();
    let options = CompilerOptions {
        no_lib: Tristate::True,
        strict: Tristate::True,
        ..CompilerOptions::default()
    };
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: options.clone(),
            root_file_names: sources.iter().map(|(name, _)| format!("/project/{name}")).collect(),
            default_library_path: "/libs".to_string(),
        },
    );
    let indexes: Vec<_> = (0..program.root_and_referenced_files().len()).collect();
    let mut serial = check_group(&program, &options, &indexes);
    serial.sort();
    assert!(serial.iter().any(|image| image.2 == 2322), "the negative control was not checked");
    for workers in [2, 3] {
        let mut parallel = std::thread::scope(|scope| {
            let handles: Vec<_> = (0..workers)
                .map(|worker| {
                    let group: Vec<_> =
                        indexes.iter().copied().skip(worker).step_by(workers).collect();
                    let program = &program;
                    let options = &options;
                    scope.spawn(move || check_group(program, options, &group))
                })
                .collect();
            handles.into_iter().flat_map(|handle| handle.join().unwrap()).collect::<Vec<_>>()
        });
        parallel.sort();
        assert_eq!(parallel, serial, "diagnostics changed at {workers} workers");
    }
}
