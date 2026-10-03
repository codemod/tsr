//! Immutable Program inputs and private checker state, exercised against the CLI.

use std::collections::HashMap;
use std::fmt::Write;

use tsr_compiler::{LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions};
use tsr_diagnostics::{Diagnostic, format::DiagnosticFile};
use tsr_execute::baseline::{Baseline, BaselineSystem, TSC_LIB_PATH};
use tsr_module::types::ResolutionHost;
use tsr_vfs::{FileSystem, InMemoryFileSystem};

#[path = "../examples/checker_workers/diagnostics.rs"]
mod worker_diagnostics;

struct Host(InMemoryFileSystem);

impl ResolutionHost for Host {
    fn fs(&self) -> &dyn FileSystem {
        &self.0
    }
    fn current_directory(&self) -> &'static str {
        "/project"
    }
}

struct WorkerOutput {
    diagnostics: Vec<(u32, Diagnostic)>,
    queries: Vec<(String, String)>,
    checked: Vec<usize>,
}

fn eligible_files(program: &Program<'_>, options: &CompilerOptions) -> Vec<usize> {
    program
        .source_files()
        .iter()
        .enumerate()
        .filter(|(index, file)| {
            *index >= program.lib_files().len()
                && !options.no_check.is_true()
                && !(options.skip_lib_check.is_true()
                    && tsr_path::is_declaration_file_name(file.file_name()))
        })
        .map(|(index, _)| index)
        .collect()
}

fn check_group(
    program: &Program<'_>,
    options: &CompilerOptions,
    group: Vec<usize>,
) -> WorkerOutput {
    // Construct, check and query in one worker: neither Checker nor TypeId is
    // assumed Send. All workers can force declarations from any Program file.
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
    for &index in &group {
        let file = &program.source_files()[index];
        checker.check_source_file(
            file.source_file().node_id.unwrap(),
            tsr_checker::check::FileContext {
                ambient: tsr_path::is_declaration_file_name(file.file_name()),
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
    }
    let mut queries = Vec::new();
    for &index in &group {
        let file = &program.source_files()[index];
        if let Some(symbol) =
            program.binder().lookup_local(file.source_file().node_id.unwrap(), "probe")
        {
            let ty = checker.get_type_of_symbol(symbol);
            queries.push((file.file_name().to_owned(), checker.type_to_string(ty)));
        }
    }
    WorkerOutput {
        diagnostics: checker
            .diagnostics()
            .iter()
            .map(|(id, diagnostic)| (id.as_u32(), diagnostic.clone()))
            .collect(),
        queries,
        checked: group,
    }
}

fn run_workers(
    program: &Program<'_>,
    options: &CompilerOptions,
    workers: usize,
) -> Vec<WorkerOutput> {
    let eligible = eligible_files(program, options);
    std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                // Affinity uses the full Program array, including the lib prefix
                // and skipped declaration files, as native checkerpool.go does.
                let group =
                    eligible.iter().copied().filter(|index| index % workers == worker).collect();
                scope.spawn(move || check_group(program, options, group))
            })
            .collect();
        handles.into_iter().map(|handle| handle.join().unwrap()).collect()
    })
}

fn complete_messages(output: &str) -> Vec<String> {
    let mut messages: Vec<String> = Vec::new();
    for line in output.lines() {
        if line.starts_with("error TS") || line.contains("): error TS") {
            messages.push(line.to_owned());
        } else if let Some(message) = messages.last_mut() {
            message.push('\n');
            message.push_str(line);
        } else {
            assert!(line.is_empty(), "unexpected CLI output: {line}");
        }
    }
    messages.sort();
    messages
}

fn rendered(
    program: &Program<'_>,
    options: &CompilerOptions,
    workers: &[WorkerOutput],
    sys: &BaselineSystem,
) -> Vec<String> {
    let mut raw: Vec<_> = workers.iter().flat_map(|worker| worker.diagnostics.clone()).collect();
    let names: HashMap<_, _> = program
        .source_files()
        .iter()
        .map(|file| (file.source_file().node_id.unwrap().as_u32(), file.file_name()))
        .collect();
    worker_diagnostics::normalize(&mut raw, |id| names[&id]);
    let mut output = String::new();
    for index in eligible_files(program, options) {
        let source = &program.source_files()[index];
        let id = source.source_file().node_id.unwrap().as_u32();
        let file = DiagnosticFile::new(source.file_name(), source.text());
        let entries: Vec<_> = raw
            .iter()
            .filter(|(file_id, _)| *file_id == id)
            .map(|(_, diagnostic)| {
                (file.line_of_position(diagnostic.span.start), diagnostic.clone())
            })
            .collect();
        let directives = tsr_compiler::comment_directives::directives_in(source.text());
        let (mut kept, unused) =
            tsr_compiler::comment_directives::filter(source.text(), &entries, &directives);
        kept.extend(unused);
        let located: Vec<_> = kept
            .iter()
            .map(|diagnostic| {
                tsr_diagnostics::format::LocatedDiagnostic::in_file(&file, diagnostic)
            })
            .collect();
        tsr_diagnostics::format::write_format_diagnostics(
            &mut output,
            &located,
            &tsr_execute::compile::formatting_options(sys),
        );
    }
    complete_messages(&output)
}

fn queries(workers: &[WorkerOutput]) -> Vec<(String, String)> {
    let mut queries: Vec<_> = workers.iter().flat_map(|worker| worker.queries.clone()).collect();
    queries.sort();
    queries
}

fn compare_fixture(
    sources: &[(&str, &str)],
    flags: &[&str],
    verify: impl FnOnce(&Program<'_>, &CompilerOptions, &[WorkerOutput], &[String]),
) {
    let files: Vec<_> = sources
        .iter()
        .map(|(name, text)| {
            (
                if name.starts_with('/') { (*name).to_owned() } else { format!("/project/{name}") },
                (*text).to_owned(),
            )
        })
        .collect();
    let mut args: Vec<_> = ["--noEmit", "--strict", "--pretty", "false", "--quiet"]
        .into_iter()
        .map(str::to_owned)
        .collect();
    args.extend(flags.iter().map(|flag| (*flag).to_owned()));
    args.extend(
        files
            .iter()
            .filter(|(name, _)| !name.starts_with(TSC_LIB_PATH))
            .map(|(name, _)| name.clone()),
    );
    let baseline = Baseline {
        current_directory: "/project".into(),
        use_case_sensitive_file_names: true,
        files: files.clone(),
        args,
        ..Baseline::default()
    };
    let host = Host(InMemoryFileSystem::new(files, [], true));
    let parsed =
        tsr_tsoptions::command_line::parse_command_line(&baseline.args, &host.0, "/project");
    assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    let arena = Arena::new();
    let options = parsed.compiler_options;
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: options.clone(),
            root_file_names: parsed.file_names,
            default_library_path: TSC_LIB_PATH.into(),
        },
    );
    assert!(program.source_files().iter().all(|file| file.diagnostics().is_empty()));
    let mut sys = BaselineSystem::new(&baseline);
    tsr_execute::command_line(&mut sys, &baseline.args);
    let cli = complete_messages(sys.output());
    let serial = run_workers(&program, &options, 1);
    assert_eq!(rendered(&program, &options, &serial, &sys), cli, "serial seam differs from CLI");
    for count in [2, 3, 4] {
        let workers = run_workers(&program, &options, count);
        assert_eq!(
            rendered(&program, &options, &workers, &sys),
            cli,
            "diagnostics at {count} workers"
        );
        assert_eq!(queries(&workers), queries(&serial), "queries at {count} workers");
        let mut checked: Vec<_> =
            workers.iter().flat_map(|worker| worker.checked.clone()).collect();
        checked.sort_unstable();
        assert_eq!(checked, eligible_files(&program, &options), "checked scope at {count} workers");
    }
    verify(&program, &options, &serial, &cli);
}

#[test]
fn fully_bound_program_is_shareable_without_sharing_its_allocator() {
    fn assert_shareable<T: Send + Sync>() {}
    assert_shareable::<Program<'static>>();
}

#[test]
fn cross_file_errors_and_merged_globals_match_serial_cli() {
    compare_fixture(
        &[
            (
                "model.ts",
                "export interface Box<T> { value: T } export class Service { value = 1; }",
            ),
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
        ],
        &["--noLib"],
        |_, _, _, diagnostics| {
            assert_eq!(diagnostics.len(), 3);
            assert!(diagnostics.iter().all(|message| message.contains("error TS2322")));
        },
    );
}

#[test]
fn recursive_cross_file_generics_can_be_queried_after_checking() {
    compare_fixture(
        &[
            (
                "a.ts",
                "import type { B } from './b'; export interface A<T> { value: T; peer?: B<T> }",
            ),
            (
                "b.ts",
                "import type { A } from './a'; export interface B<T> { value: T; peer?: A<T> }",
            ),
            (
                "use.ts",
                "import type { A } from './a'; export const probe: A<number> = { value: 1 }; const bad: A<number> = { value: 'bad' };",
            ),
        ],
        &["--noLib"],
        |_, _, workers, diagnostics| {
            assert_eq!(diagnostics.len(), 1, "{diagnostics:#?}");
            assert!(diagnostics[0].contains("error TS2322"));
            assert!(
                diagnostics[0]
                    .ends_with("error TS2322: Type 'string' is not assignable to type 'number'.")
            );
            assert_eq!(queries(workers), [("/project/use.ts".into(), "A<number>".into())]);
        },
    );
}

#[test]
fn generic_object_literal_members_keep_concrete_types_and_literal_context() {
    compare_fixture(
        &[
            (
                "model.ts",
                "export interface Base<T> { value: T } export interface Box<T = number> extends Base<T> {} export type Mapped<T> = { [K in keyof T]: T[K] };",
            ),
            (
                "use.ts",
                "import type { Base, Box, Mapped } from './model';\nconst goodDefault: Box = { value: 1 };\nconst badDefault: Box = { value: 'bad' };\nconst goodMapped: Mapped<Base<string>> = { value: 'ok' };\nconst badMapped: Mapped<Base<string>> = { value: 1 };\nconst goodLiteral: Base<'yes'> = { value: 'yes' };\nconst badLiteral: Base<'yes'> = { value: 'no' };\nconst badAsserted: Base<number> = { value: 'bad' as const };",
            ),
        ],
        &["--noLib"],
        |_, _, _, diagnostics| {
            let endings: Vec<_> = diagnostics
                .iter()
                .map(|message| message.split("error TS2322: ").nth(1).unwrap())
                .collect();
            assert_eq!(
                endings,
                [
                    "Type 'string' is not assignable to type 'number'.",
                    "Type 'number' is not assignable to type 'string'.",
                    "Type '\"no\"' is not assignable to type '\"yes\"'.",
                    "Type 'string' is not assignable to type 'number'.",
                ],
            );
        },
    );
}

#[test]
fn literal_error_display_preserves_never_and_singleton_targets() {
    compare_fixture(
        &[(
            "use.ts",
            "const impossible: { value: never } = { value: 1 as const };\nconst booleanTarget: { value: boolean } = { value: 'bad' as const };\nconst singletonTarget: { value: 'yes' | 2 } = { value: 'bad' as const };\nconst templateTarget: { value: `yes${string}` } = { value: 'bad' as const };",
        )],
        &["--noLib"],
        |_, _, _, diagnostics| {
            let endings: Vec<_> = diagnostics
                .iter()
                .map(|message| message.split("error TS2322: ").nth(1).unwrap())
                .collect();
            assert_eq!(
                endings,
                [
                    "Type '1' is not assignable to type 'never'.",
                    "Type 'string' is not assignable to type 'boolean'.",
                    "Type '\"bad\"' is not assignable to type '\"yes\" | 2'.",
                    "Type '\"bad\"' is not assignable to type '`yes${string}`'.",
                ],
            );
        },
    );
}

#[test]
#[ignore = "tsr-6.49: imported module augmentation members are not visible"]
fn augmentation_and_comment_directives_match_serial_cli() {
    compare_fixture(
        &[
            ("model.ts", "export interface Service { base: number }"),
            (
                "augment.ts",
                "import './model'; declare module './model' { interface Service { extra: string } }",
            ),
            (
                "use.ts",
                "import type { Service } from './model'; export const probe: Service = { base: 1, extra: 'ok' };\n// @ts-expect-error\nconst suppressed: Service = { base: 1, extra: 2 };\nconst bad: Service = { base: 1, extra: 3 };\n// @ts-expect-error\nconst unused: number = 1;\n// @ts-ignore\nconst ignored: number = 'bad';",
            ),
        ],
        &["--noLib"],
        |_, _, workers, diagnostics| {
            assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");
            assert!(diagnostics.iter().any(|message| message.contains("error TS2322")));
            assert!(diagnostics.iter().any(|message| message.contains("error TS2578")));
            assert_eq!(queries(workers), [("/project/use.ts".into(), "Service".into())]);
        },
    );
}

#[test]
fn directives_and_checker_local_queries_match_serial_cli() {
    compare_fixture(
        &[
            (
                "a.ts",
                "export const probe: number = 1;\n// @ts-expect-error\nconst suppressed: number = 'bad';\nconst bad: number = 'bad';\n// @ts-expect-error\nconst unused: number = 1;\n// @ts-ignore\nconst ignored: number = 'bad';",
            ),
            ("b.ts", "export const other: string = 'ok';"),
        ],
        &["--noLib"],
        |_, _, workers, diagnostics| {
            assert_eq!(diagnostics.len(), 2);
            assert!(diagnostics.iter().any(|message| message.contains("error TS2322")));
            assert!(diagnostics.iter().any(|message| message.contains("error TS2578")));
            assert_eq!(queries(workers), [("/project/a.ts".into(), "number".into())]);
        },
    );
}

#[test]
fn program_merge_conflicts_are_deduplicated_across_private_checkers() {
    compare_fixture(
        &[
            ("a.d.ts", "declare const clash: number;"),
            ("b.d.ts", "declare const clash: number;"),
            ("a.ts", "export const probe: number = 1;"),
            ("b.ts", "export const other: string = 'ok';"),
        ],
        &["--noLib"],
        |program, options, serial, diagnostics| {
            assert_eq!(diagnostics.len(), 2, "{diagnostics:#?}");
            assert!(diagnostics.iter().all(|message| message.contains("error TS2451")));
            let workers = run_workers(program, options, 4);
            let raw_count: usize = workers.iter().map(|worker| worker.diagnostics.len()).sum();
            assert!(
                raw_count > serial[0].diagnostics.len(),
                "control did not produce worker duplicates"
            );
        },
    );
}

#[test]
fn skewed_groups_preserve_every_distinct_error_span() {
    let mut heavy = String::new();
    for index in 0..96 {
        writeln!(heavy, "const wrong_{index}: number = 'bad';").unwrap();
    }
    compare_fixture(
        &[
            ("heavy.ts", &heavy),
            ("a.ts", "const a: number = 'bad';"),
            ("b.ts", "const b: string = 1;"),
            ("c.ts", "const c: boolean = 1;"),
        ],
        &["--noLib"],
        |_, _, _, diagnostics| {
            assert_eq!(diagnostics.len(), 99);
            assert!(diagnostics.iter().all(|message| message.contains("error TS2322")));
        },
    );
}

#[test]
fn affinity_keeps_the_lib_prefix_and_skipped_declarations() {
    compare_fixture(
        &[
            ("/home/src/tslibs/TS/Lib/lib.d.ts", "interface Object {}"),
            ("a.ts", "const a: number = 'bad';"),
            ("skipped.d.ts", "// @ts-expect-error\ninterface Skipped {}"),
            ("b.ts", "const b: string = 1;"),
        ],
        &["--skipLibCheck", "--target", "es5"],
        |program, options, _, diagnostics| {
            assert_eq!(program.lib_files().len(), 1);
            assert_eq!(diagnostics.len(), 2);
            let workers = run_workers(program, options, 4);
            assert_eq!(workers[1].checked, [1]);
            assert_eq!(workers[3].checked, [3]);
            assert!(workers[0].checked.is_empty() && workers[2].checked.is_empty());
        },
    );
}

#[test]
fn no_check_does_not_create_unused_directive_errors() {
    compare_fixture(
        &[("a.ts", "// @ts-expect-error\nconst a: number = 1;\nconst bad: number = 'bad';")],
        &["--noLib", "--noCheck"],
        |_, _, workers, diagnostics| {
            assert!(diagnostics.is_empty());
            assert!(workers[0].checked.is_empty());
            assert!(queries(workers).is_empty());
        },
    );
}
