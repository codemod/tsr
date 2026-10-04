//! Opt-in whole-project worker ownership and cost probe; the CLI stays serial.
//!
//! Run `checker_workers /absolute/tsconfig.json [positive-count]` from the project directory.
//! Stdout contains plain diagnostics. Stderr contains tab-separated scope,
//! phase and worker counters for an external fresh-process/RSS harness.

use std::time::Instant;

use tsr_checker::{Checker, check::FileContext};
use tsr_compiler::{LoadOptions, Program};
use tsr_core::{Arena, CompilerOptions, Tristate};
use tsr_diagnostics::{Diagnostic, format::DiagnosticFile};
use tsr_execute::{OsSystem, system::System};
use tsr_vfs::{CachedFileSystem, FileSystem};

#[path = "checker_workers/diagnostics.rs"]
mod worker_diagnostics;

#[path = "checker_workers/selection.rs"]
mod selection;

struct Host<'a> {
    fs: &'a dyn FileSystem,
    directory: &'a str,
}

impl tsr_module::types::ResolutionHost for Host<'_> {
    fn fs(&self) -> &dyn FileSystem {
        self.fs
    }

    fn current_directory(&self) -> &str {
        self.directory
    }
}

struct WorkerResult {
    diagnostics: Vec<(u32, Diagnostic)>,
    checked: usize,
    initial_types: usize,
    final_types: usize,
    computations: usize,
    initialization_seconds: f64,
    checking_seconds: f64,
}

fn check_group(program: &Program<'_>, options: &CompilerOptions, group: &[usize]) -> WorkerResult {
    let started = Instant::now();
    let mut checker = Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(program),
    );
    checker.apply_compiler_options(options);
    checker.set_checked_files(
        program.root_and_referenced_files().iter().filter_map(|file| file.source_file().node_id),
    );
    let initial_types = checker.type_count();
    let initialized = Instant::now();
    for &index in group {
        let file = &program.source_files()[index];
        checker.check_source_file(
            file.source_file().node_id.expect("a parsed root is registered"),
            FileContext {
                ambient: tsr_path::is_declaration_file_name(file.file_name()),
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
    }
    let finished = Instant::now();
    WorkerResult {
        diagnostics: checker
            .diagnostics()
            .iter()
            .map(|(id, diagnostic)| (id.as_u32(), diagnostic.clone()))
            .collect(),
        checked: group.len(),
        initial_types,
        final_types: checker.type_count(),
        computations: checker.computations(),
        initialization_seconds: initialized.duration_since(started).as_secs_f64(),
        checking_seconds: finished.duration_since(initialized).as_secs_f64(),
    }
}

fn run() -> Result<bool, Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let (project, requested) = match args.as_slice() {
        [project] => (project, None),
        [project, requested] => (project, Some(requested.as_str())),
        _ => return Err("usage: checker_workers /absolute/tsconfig.json [positive-count]".into()),
    };
    let override_count = selection::parse_override(requested)?;
    let started = Instant::now();
    let sys = OsSystem::new();
    let text = sys.fs().read_file(project).ok_or("cannot read project")?;
    let mut parsed = tsr_tsoptions::parse_config_file(
        project,
        &text,
        tsr_path::get_directory_path(project),
        sys.fs(),
    );
    if !parsed.errors.is_empty() {
        return Err(format!("config errors: {:?}", parsed.errors).into());
    }
    let options = &mut parsed.compiler_options;
    options.checkers = override_count.or(options.checkers);
    options.no_emit = Tristate::True;
    options.incremental = Tristate::False;
    options.composite = Tristate::False;
    options.pretty = Tristate::False;
    options.extended_diagnostics = Tristate::False;
    let arena = Arena::new();
    let fs = CachedFileSystem::new(sys.fs());
    let host = Host { fs: &fs, directory: sys.current_directory() };
    let program = Program::from_root_files(
        &arena,
        &host,
        LoadOptions {
            compiler_options: options.clone(),
            root_file_names: parsed.file_names,
            default_library_path: sys.default_library_path().into(),
        },
    );
    let loaded = Instant::now();
    let cpus = std::thread::available_parallelism().map_or(1, usize::from);
    let requested = options.checkers;
    let workers = selection::checker_count(
        requested,
        program.source_files().len(),
        options.single_threaded.is_true(),
    );
    eprintln!(
        "POLICY\t{}\t{workers}\t{}",
        requested.map_or_else(|| "default".into(), |n| n.to_string()),
        options.single_threaded.is_true()
    );
    let lib_count = program.lib_files().len();
    let eligible: Vec<_> = program
        .source_files()
        .iter()
        .enumerate()
        .filter(|(index, file)| {
            *index >= lib_count
                && file.source_file().node_id.is_some()
                && !options.no_check.is_true()
                && !(options.skip_lib_check.is_true()
                    && tsr_path::is_declaration_file_name(file.file_name()))
                && !(options.skip_default_lib_check.is_true()
                    && program.lib_files().iter().any(|lib| lib.file_name() == file.file_name()))
        })
        .map(|(index, _)| index)
        .collect();
    // Native checkerpool.go associates every file before selecting checked
    // files; filtering first would give different cache affinity and costs.
    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                let group: Vec<_> =
                    eligible.iter().copied().filter(|index| index % workers == worker).collect();
                let program = &program;
                let options = &*options;
                scope.spawn(move || check_group(program, options, &group))
            })
            .collect();
        handles
            .into_iter()
            .map(|handle| handle.join().expect("worker panicked"))
            .collect::<Vec<_>>()
    });
    let checked = Instant::now();
    let mut raw: Vec<_> = results.iter().flat_map(|worker| worker.diagnostics.clone()).collect();
    let file_names: std::collections::HashMap<_, _> = program
        .source_files()
        .iter()
        .filter_map(|file| file.source_file().node_id.map(|id| (id.as_u32(), file.file_name())))
        .collect();
    worker_diagnostics::normalize(&mut raw, |id| file_names[&id]);
    let mut diagnostics = Vec::new();
    let files: Vec<_> = program
        .root_and_referenced_files()
        .iter()
        .map(|file| DiagnosticFile::new(file.file_name(), file.text()))
        .collect();
    for &index in &eligible {
        let source = &program.source_files()[index];
        let file = &files[index - lib_count];
        let id = source.source_file().node_id.unwrap().as_u32();
        let entries: Vec<_> = raw
            .iter()
            .filter(|(file_id, _)| *file_id == id)
            .map(|(_, diagnostic)| {
                (
                    file.line_of_position(diagnostic.span.start),
                    (source.file_name(), diagnostic.clone()),
                )
            })
            .collect();
        let directives = tsr_compiler::comment_directives::directives_in(source.text());
        let (kept, unused) =
            tsr_compiler::comment_directives::filter(source.text(), &entries, &directives);
        diagnostics.extend(kept);
        diagnostics.extend(unused.into_iter().map(|diagnostic| (source.file_name(), diagnostic)));
    }
    let located: Vec<_> = diagnostics
        .iter()
        .map(|(name, diagnostic)| {
            let file = files.iter().find(|file| file.file_name() == *name).unwrap();
            tsr_diagnostics::format::LocatedDiagnostic::in_file(file, diagnostic)
        })
        .collect();
    let mut output = String::new();
    tsr_diagnostics::format::write_format_diagnostics(
        &mut output,
        &located,
        &tsr_execute::compile::formatting_options(&sys),
    );
    print!("{output}");
    eprintln!(
        "PHASE\t{workers}\t{cpus}\t{}\t{}\t{}",
        loaded.duration_since(started).as_secs_f64(),
        checked.duration_since(loaded).as_secs_f64(),
        started.elapsed().as_secs_f64()
    );
    for (worker, result) in results.iter().enumerate() {
        eprintln!(
            "WORKER\t{worker}\t{}\t{}\t{}\t{}\t{}\t{}",
            result.checked,
            result.initial_types,
            result.final_types,
            result.computations,
            result.initialization_seconds,
            result.checking_seconds
        );
    }
    for file in program.source_files() {
        eprintln!("LOADED\t{}", file.file_name());
    }
    for &index in &eligible {
        eprintln!("CHECKED\t{}", program.source_files()[index].file_name());
    }
    Ok(diagnostics
        .iter()
        .any(|(_, diagnostic)| diagnostic.message.category() == tsr_diagnostics::Category::Error))
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(errors) => std::process::ExitCode::from(u8::from(errors)),
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::from(2)
        }
    }
}
