//! Building a program, checking it, and saying what happened.
//!
//! Ported from `internal/execute/tsc/compile.go` and `emit.go`'s
//! diagnostic-reporting half at the pinned commit.

use tsr_core::CompilerOptions;
use tsr_diagnostics::format::{DiagnosticFile, FormattingOptions, LocatedDiagnostic};
use tsr_diagnostics::{Diagnostic, messages};
use tsr_path::get_normalized_absolute_path;
use tsr_tsoptions::command_line::ParsedCommandLine;

use crate::system::System;

/// How a `tsc` invocation ended (`tsc.ExitStatus`).
///
/// The numbers are the process's exit code and are upstream's, including the
/// gap in meaning between 1 and 2 that a shell script has to care about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum ExitStatus {
    /// Nothing was wrong.
    Success = 0,
    /// Errors were reported and nothing was written.
    DiagnosticsPresentOutputsSkipped = 1,
    /// Diagnostics with an unskipped native emit result. An empty eligible
    /// source set has EmitSkipped=false even when no files were written.
    /// Nonempty emission remains unsupported by this checker-only driver.
    DiagnosticsPresentOutputsGenerated = 2,
    /// The project itself could not be understood.
    InvalidProjectOutputsSkipped = 3,
    /// Project references form a cycle.
    ProjectReferenceCycleOutputsSkipped = 4,
    /// The command asked for something this compiler does not do.
    NotImplemented = 5,
}

impl ExitStatus {
    /// The process exit code.
    #[must_use]
    pub const fn code(self) -> i32 {
        self as i32
    }
}

/// How diagnostics should be rendered for this run.
///
/// `--pretty` is a `Tristate` rather than a `bool` precisely so that unset can
/// mean "ask the terminal": `tsc` frames and colours its output when stdout is a
/// TTY and prints the plain form when it is redirected, which is what makes
/// `tsc > errors.txt` produce something a tool can parse.
pub fn formatting_options(sys: &dyn System) -> FormattingOptions {
    FormattingOptions::new(
        sys.current_directory().to_string(),
        sys.fs().use_case_sensitive_file_names(),
    )
}

/// Whether this run should use the pretty formatter (`shouldBePretty`,
/// `internal/execute/tsc/diagnostics.go:56`).
///
/// The default is `defaultIsPretty` (`:46`): `NO_COLOR` disables it,
/// `FORCE_COLOR` enables it, and otherwise the terminal answers, which is what
/// makes `tsc` colourful interactively and machine-readable when redirected.
/// An explicit `--pretty` or `--pretty false` overrides it.
#[must_use]
pub fn should_use_pretty(sys: &dyn System, options: &CompilerOptions) -> bool {
    if !options.pretty.is_unknown() {
        return options.pretty.is_true();
    }
    if !sys.environment_variable("NO_COLOR").is_empty() {
        return false;
    }
    if !sys.environment_variable("FORCE_COLOR").is_empty() {
        return true;
    }
    sys.write_output_is_tty()
}

/// Render diagnostics through whichever formatter this run selected.
fn render(
    sys: &dyn System,
    located: &[LocatedDiagnostic<'_>],
    options: &CompilerOptions,
    summary: bool,
) -> String {
    let formatting = formatting_options(sys);
    let mut text = String::new();
    let pretty = should_use_pretty(sys, options);
    if pretty {
        tsr_diagnostics::format::write_format_diagnostics_with_color_and_context(
            &mut text,
            located,
            &formatting,
        );
        // One more newline after the last frame. `CreateDiagnosticReporter`'s
        // pretty arm writes `NewLine` after each diagnostic, where the plain arm
        // relies on the diagnostic's own trailing one.
        // `non-object-config-root.js` pins the count: two blank lines between
        // the last squiggle and `Found 2 errors`, one from here and one from the
        // summary's own leading newline.
        if !located.is_empty() {
            text.push_str(&formatting.newline);
        }
    } else {
        tsr_diagnostics::format::write_format_diagnostics(&mut text, located, &formatting);
    }
    // `CreateReportErrorSummary` (`diagnostics.go:134`) reports a summary only
    // in pretty mode; plain output is the diagnostics alone.
    if summary && pretty && !options.quiet.is_true() {
        tsr_diagnostics::format::write_error_summary_text(&mut text, located, &formatting);
    }
    text
}

/// Build the program, check it, report (`performCompilation`).
pub fn run_compilation(
    sys: &mut dyn System,
    command_line: &ParsedCommandLine,
    config_file_name: &str,
) -> ExitStatus {
    let compilation_started = sys.since_start();
    let current_directory = sys.current_directory().to_string();

    // The options a compilation runs under are the config's, with the command
    // line layered over the top. Upstream does this inside
    // `GetParsedCommandLineOfConfigFile`; here the two parsers meet at this one
    // point, which is also the only place the layering rule is written down.
    let (mut options, root_files, config_errors, error_files, raw) = if config_file_name.is_empty()
    {
        (
            command_line.compiler_options.clone(),
            command_line
                .file_names
                .iter()
                .map(|name| get_normalized_absolute_path(name, &current_directory))
                .collect::<Vec<_>>(),
            Vec::new(),
            Vec::new(),
            tsr_core::OrderedMap::default(),
        )
    } else {
        let Some(text) = sys.fs().read_file(config_file_name) else {
            let error = Diagnostic::with_args(
                &messages::CANNOT_READ_FILE_0,
                tsr_core::Span::new(0, 0),
                [config_file_name.to_string()],
            );
            report(sys, &[error], &command_line.compiler_options);
            return ExitStatus::InvalidProjectOutputsSkipped;
        };
        let base_path = tsr_path::get_directory_path(config_file_name).to_string();
        let parsed =
            tsr_tsoptions::parse_config_file(config_file_name, &text, &base_path, sys.fs());
        (parsed.compiler_options, parsed.file_names, parsed.errors, parsed.error_files, parsed.raw)
    };

    apply_command_line_over_config(&mut options, command_line);

    // Native GetDiagnosticsOfAnyProgram (program.go:1782, pinned 5b1047d)
    // retains recoverable config errors while still checking syntax and,
    // absent syntax errors, semantics. Each extended file owns its errors.
    let config_diagnostics: Vec<(String, Diagnostic)> = config_errors
        .into_iter()
        .zip(error_files)
        .map(|(error, file)| (file.unwrap_or_default(), error))
        .collect();

    if options.show_config.is_true() {
        let text = crate::show_config::show_config(
            &options,
            &root_files,
            &raw,
            &current_directory,
            sys.fs().use_case_sensitive_file_names(),
        );
        sys.write(&text);
        return ExitStatus::Success;
    }

    if options.watch.is_true() {
        sys.write("error TS0: '--watch' is not implemented in this port.\n");
        return ExitStatus::NotImplemented;
    }

    if root_files.is_empty() && config_file_name.is_empty() {
        // No config exists to carry the missing-input diagnostic.
        let error = Diagnostic::with_args(
            &messages::NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_0_SPECIFIED_INCLUDE_PATHS_WERE_1_AND_EXCLUDE_PATHS_WERE_2,
            tsr_core::Span::new(0, 0),
            ["tsconfig.json".to_string(), "[]".to_string(), "[]".to_string()],
        );
        report(sys, &[error], &options);
        return ExitStatus::DiagnosticsPresentOutputsSkipped;
    }

    // Everything above is host-independent. From here the compiler runs, and it
    // borrows an arena that must outlive the program.
    let program_started = sys.since_start();
    let arena = tsr_core::Arena::new();
    // Native tsc creates a cached filesystem compiler host for each compilation.
    // Config discovery above and text reads retain their independent semantics.
    let cached_fs = tsr_vfs::CachedFileSystem::new(sys.fs());
    let host = DriverHost { fs: &cached_fs, current_directory: current_directory.clone() };
    let program = tsr_compiler::Program::from_root_files(
        &arena,
        &host,
        tsr_compiler::LoadOptions {
            compiler_options: options.clone(),
            root_file_names: root_files.clone(),
            default_library_path: sys.default_library_path().to_string(),
        },
    );
    let program_finished = sys.since_start();
    #[cfg(feature = "work-trace")]
    let work_trace = sys.work_trace();
    #[cfg(feature = "work-trace")]
    if let Some(trace) = &work_trace {
        trace.program(
            &program,
            &options,
            &root_files,
            &raw,
            &current_directory,
            sys.fs().use_case_sensitive_file_names(),
        );
    }

    // A root file the loader could not reach is a diagnostic, not silence.
    // Upstream reports it from `processAllProgramFiles`
    // (`fileloader.go:407`); this port's loader drops it, so the driver asks
    // afterwards which roots made it into the program.
    //
    // The name is reported **as written** — `tsc`, not the absolute path it was
    // resolved to — which is why the command line's own file names are kept
    // alongside the absolutised roots.
    let mut missing_roots = Vec::new();
    if config_file_name.is_empty() {
        for (written, absolute) in command_line.file_names.iter().zip(&root_files) {
            if program.source_file(absolute).is_none() {
                missing_roots.push(Diagnostic::with_args(
                    &messages::FILE_0_NOT_FOUND,
                    tsr_core::Span::new(0, 0),
                    [tsr_path::normalize_slashes(written)],
                ));
            }
        }
    }
    if !missing_roots.is_empty() {
        report(sys, &missing_roots, &options);
        return ExitStatus::DiagnosticsPresentOutputsSkipped;
    }

    // `checkerpool.go`: file `i` belongs to checker `i % count`, and each
    // checker runs on its own worker. The opt-in work trace observes one
    // private checker, so a traced run keeps a pool of one.
    #[cfg(feature = "work-trace")]
    let pool_size = if work_trace.is_some() {
        1
    } else {
        crate::checker_pool::checker_count(&options, program.source_files().len())
    };
    #[cfg(not(feature = "work-trace"))]
    let pool_size = crate::checker_pool::checker_count(&options, program.source_files().len());
    let list_only_needs_checker = options.list_files_only.is_true()
        && program.source_files().iter().any(|file| {
            file.source_file().node_id.is_some_and(|id| {
                program.nodes().flags(id).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE)
            })
        });
    #[cfg(feature = "work-trace")]
    if let Some(trace) = &work_trace
        && (!options.list_files_only.is_true() || list_only_needs_checker)
    {
        trace.checker_construction_started();
    }
    #[cfg(feature = "work-trace")]
    let configure = |checker: &mut tsr_checker::Checker<'_, '_>| {
        if let Some(trace) = &work_trace {
            trace.checker_created(&options);
            checker.set_work_observer(std::sync::Arc::clone(trace) as _);
        }
    };
    #[cfg(not(feature = "work-trace"))]
    let configure = |_: &mut tsr_checker::Checker<'_, '_>| {};
    // GetDiagnosticsOfAnyProgram still reports config and syntax in list-only
    // mode; only global/semantic/declaration work is skipped (5b1047d:1813).
    let pool = if options.list_files_only.is_true() && !list_only_needs_checker {
        crate::checker_pool::PoolOutcome {
            diagnostics: Vec::new(),
            js_syntax: Vec::new(),
            checked_files: 0,
            construction: std::time::Duration::ZERO,
        }
    } else if options.list_files_only.is_true() {
        let mut checker = tsr_checker::Checker::with_module_host(
            program.binder(),
            program.nodes(),
            program.node_map(),
            Some(&program),
        );
        checker.apply_compiler_options(&options);
        configure(&mut checker);
        let mut js_syntax = Vec::new();
        for (index, file) in program.source_files().iter().enumerate() {
            if let Some(id) = file.source_file().node_id {
                js_syntax
                    .extend(checker.js_syntax_diagnostics(id).into_iter().map(|(_, d)| (index, d)));
            }
        }
        crate::checker_pool::PoolOutcome {
            diagnostics: Vec::new(),
            js_syntax,
            checked_files: 0,
            construction: sys.since_start().saturating_sub(program_finished),
        }
    } else {
        crate::checker_pool::check_program_files(&program, &options, pool_size, &configure)
    };
    let checker_initialized = program_finished + pool.construction;
    let checked_file_count = pool.checked_files;
    let checking_finished = sys.since_start();
    // `GetDiagnosticsOfAnyProgram`: syntactic first, the semantic set only
    // when there is none (`tsr_compiler::program_diagnostics`, shared with the
    // conformance harness).
    let mut diagnostics = config_diagnostics;
    let program_diagnostics = tsr_compiler::program_diagnostics::diagnostics_of_any_program(
        &program,
        pool.js_syntax,
        pool.diagnostics,
    );
    diagnostics.extend(program_diagnostics.into_iter().map(|(index, diagnostic)| {
        (program.source_files()[index].file_name().to_string(), diagnostic)
    }));

    // Line maps are built only for files that have diagnostics, as native
    // `SourceFile.ECMALineMap` is computed on first use: indexing every
    // library file would scan megabytes nobody prints.
    let mut indexed_files: Vec<Option<DiagnosticFile>> =
        std::iter::repeat_with(|| None).take(program.source_files().len()).collect();
    let index_file = |slot: &mut Option<DiagnosticFile>, index: usize| {
        if slot.is_none() {
            let file = &program.source_files()[index];
            *slot = Some(DiagnosticFile::new(file.file_name(), file.text()));
        }
    };

    // Native SortAndDeduplicateDiagnostics (program.go:1454, pinned 5b1047d):
    // compare complete chains and merge related information. Primary paths
    // remain owned tuple keys without allocating file detail for plain heads.
    diagnostics = tsr_diagnostics::sort_and_deduplicate_located_diagnostics(diagnostics);

    let mut printed: Vec<usize> = diagnostics
        .iter()
        .filter_map(|(name, _)| {
            program.source_files().iter().position(|file| file.file_name() == name)
        })
        .collect();
    printed.sort_unstable();
    printed.dedup();
    for &index in &printed {
        index_file(&mut indexed_files[index], index);
    }
    let mut files: Vec<DiagnosticFile> = indexed_files.into_iter().flatten().collect();
    for (name, _) in &diagnostics {
        if !name.is_empty()
            && !files.iter().any(|file| file.file_name() == name)
            && let Some(text) = sys.fs().read_file(name)
        {
            files.push(DiagnosticFile::new(name.clone(), text));
        }
    }
    report_located(sys, &files, &diagnostics, &options, &program);
    let reporting_finished = sys.since_start();

    if options.extended_diagnostics.is_true() {
        // Native `reportStatistics` uses the host clock too. These serial
        // loader subphases are disjoint; discovery includes source copies and
        // reference/import collection as well as resolver calls.
        let statistics = program.statistics();
        sys.write(&format!(
            "Files:                 {}\nChecked files:         {}\n\
             Parsed files:          {}\nLoader time:           {:.3}s\n\
             File read time:        {:.3}s\nMetadata time:         {:.3}s\n\
             Parse time:            {:.3}s\nFile discovery time:   {:.3}s\n\
             Dependency parse work: {:.3}s\nDependency parse jobs: {}\n\
             Dependency published:  {}\nDependency workers:    {}\nDependency pending:    {}\n\
             Resolver time:         {:.3}s\nResolver requests:     {}\n\
             Reusable modules:      {}\nReusable type refs:    {}\n\
             Indexing time:         {:.3}s\nBind time:             {:.3}s\n\
             Config time:           {:.3}s\nProgram time:          {:.3}s\n\
             Checker init time:     {:.3}s\nCheck time:            {:.3}s\n\
             Reporting time:        {:.3}s\nCompilation time:      {:.3}s\n",
            program.source_files().len(),
            checked_file_count,
            statistics.load.parsed_files,
            statistics.load.total_time.as_secs_f64(),
            statistics.load.read_time.as_secs_f64(),
            statistics.load.metadata_time.as_secs_f64(),
            statistics.load.parse_time.as_secs_f64(),
            statistics.load.discovery_time().as_secs_f64(),
            statistics.load.dependency_parse_work.as_secs_f64(),
            statistics.load.dependency_parse_jobs,
            statistics.load.dependency_parses_published,
            statistics.load.dependency_parse_workers,
            statistics.load.dependency_pending_peak,
            statistics.load.resolution_time.as_secs_f64(),
            statistics.load.resolution_requests,
            statistics.load.reusable_module_requests,
            statistics.load.reusable_type_requests,
            statistics.indexing_time.as_secs_f64(),
            statistics.bind_time.as_secs_f64(),
            program_started.saturating_sub(compilation_started).as_secs_f64(),
            program_finished.saturating_sub(program_started).as_secs_f64(),
            checker_initialized.saturating_sub(program_finished).as_secs_f64(),
            checking_finished.saturating_sub(checker_initialized).as_secs_f64(),
            reporting_finished.saturating_sub(checking_finished).as_secs_f64(),
            reporting_finished.saturating_sub(compilation_started).as_secs_f64(),
        ));
    }

    if diagnostics.is_empty() {
        return ExitStatus::Success;
    }
    // EmitFilesAndReportErrors / CombineEmitResults (5b1047d): an empty
    // emitter list is an unskipped result. List-only and noEmitOnError instead
    // return a skipped result before source eligibility is considered.
    let empty_emit = !options.list_files_only.is_true()
        && !options.no_emit_on_error.is_true()
        && !(0..program.source_files().len())
            .any(|index| program.source_file_may_be_emitted(index));
    if empty_emit {
        ExitStatus::DiagnosticsPresentOutputsGenerated
    } else {
        ExitStatus::DiagnosticsPresentOutputsSkipped
    }
}

/// Layer command-line options over a config file's.
///
/// **The rule is "written wins", not "non-default wins".** A `Tristate` can say
/// whether it was written, but a `String` cannot — `--outDir ""` and an unwritten
/// `outDir` are the same value — which is why the parser records
/// [`ParsedCommandLine::set_options`] and this reads it rather than comparing
/// against defaults. Getting that wrong makes a config's `outDir` silently
/// survive an explicit command-line override, or vanish without one.
fn apply_command_line_over_config(options: &mut CompilerOptions, command_line: &ParsedCommandLine) {
    for declaration in tsr_tsoptions::declarations::COMPILER_OPTIONS {
        if !command_line.was_set(declaration.name) {
            continue;
        }
        // Re-run the setter against the command line's own resolved options, so
        // the value written here is the same one the parser computed. Copying
        // field-by-field would be a third place that knows what each option
        // means, which is exactly what ADR-0042 removed.
        copy_option(options, &command_line.compiler_options, declaration.name);
    }
}

/// Copy one named option from `from` into `into`.
///
/// A `match` on the name is what this port has instead of upstream's reflection
/// over struct tags. It is the one place the CLI needs a per-option arm, and it
/// is confined to the options the driver actually layers.
fn copy_option(into: &mut CompilerOptions, from: &CompilerOptions, name: &str) {
    macro_rules! copy {
        ($($option:literal => $field:ident),* $(,)?) => {
            match name {
                $($option => into.$field.clone_from(&from.$field),)*
                _ => {}
            }
        };
    }
    copy! {
        "target" => target,
        "module" => module,
        "moduleResolution" => module_resolution,
        "jsx" => jsx,
        "lib" => lib,
        "noLib" => no_lib,
        "allowJs" => allow_js,
        "checkJs" => check_js,
        "strict" => strict,
        "strictNullChecks" => strict_null_checks,
        "strictPropertyInitialization" => strict_property_initialization,
        "useUnknownInCatchVariables" => use_unknown_in_catch_variables,
        "noUncheckedIndexedAccess" => no_unchecked_indexed_access,
        "noUnusedLocals" => no_unused_locals,
        "noUnusedParameters" => no_unused_parameters,
        "allowUnreachableCode" => allow_unreachable_code,
        "preserveConstEnums" => preserve_const_enums,
        "verbatimModuleSyntax" => verbatim_module_syntax,
        "noUncheckedSideEffectImports" => no_unchecked_side_effect_imports,
        "noImplicitAny" => no_implicit_any,
        "declaration" => declaration,
        "esModuleInterop" => es_module_interop,
        "isolatedModules" => isolated_modules,
        "allowSyntheticDefaultImports" => allow_synthetic_default_imports,
        "alwaysStrict" => always_strict,
        "resolveJsonModule" => resolve_json_module,
        "outDir" => out_dir,
        "rootDir" => root_dir,
        "declarationDir" => declaration_dir,
        "baseUrl" => base_url,
        "types" => types,
        "typeRoots" => type_roots,
        "noEmit" => no_emit,
        "noEmitOnError" => no_emit_on_error,
        "noCheck" => no_check,
        "pretty" => pretty,
        "listFiles" => list_files,
        "listFilesOnly" => list_files_only,
        "showConfig" => show_config,
        "skipLibCheck" => skip_lib_check,
        "skipDefaultLibCheck" => skip_default_lib_check,
        "noErrorTruncation" => no_error_truncation,
        "watch" => watch,
        "incremental" => incremental,
        "composite" => composite,
        "quiet" => quiet,
        "traceResolution" => trace_resolution,
        "extendedDiagnostics" => extended_diagnostics,
        "singleThreaded" => single_threaded,
        "checkers" => checkers,
        "deduplicatePackages" => deduplicate_packages,
    }
}

/// Why one loaded file is excluded from the CLI's full semantic worker.
///
/// Ported from `Program.SkipTypeChecking` and `canIncludeBindAndCheckDiagnostics`
/// (`internal/compiler/program.go`) at the pinned commit. Indexing the Program's
/// ordered files preserves default-library membership without a name heuristic
/// or a per-file library scan. Project-reference redirects are not implemented
/// in this driver, so their native exclusion is not claimed here.
///
/// # Panics
///
/// Panics if `file_index` is outside `program.source_files()`.
#[must_use]
pub fn full_check_exclusion(
    program: &tsr_compiler::Program<'_>,
    file_index: usize,
) -> Option<&'static str> {
    // Skipped files remain bound and available to cross-file queries; this is
    // not a loader filter.
    if let Some(reason) =
        tsr_compiler::program_diagnostics::skip_type_checking(program, file_index, false)
    {
        return Some(reason);
    }
    if program.source_files()[file_index].source_file().node_id.is_none() {
        return Some("missing_source_node");
    }
    None
}

/// The `ResolutionHost` the driver hands to the loader.
///
/// The last piece of the seam STATUS-cli.md §1 listed as missing: every other
/// implementation in the workspace lives in a test, an example, or the
/// conformance harness.
struct DriverHost<'a> {
    fs: &'a dyn tsr_vfs::FileSystem,
    current_directory: String,
}

impl tsr_module::types::ResolutionHost for DriverHost<'_> {
    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        self.fs
    }

    fn current_directory(&self) -> &str {
        &self.current_directory
    }
}

/// Render file-less diagnostics and the summary.
fn report(sys: &mut dyn System, diagnostics: &[Diagnostic], options: &CompilerOptions) {
    let located: Vec<LocatedDiagnostic<'_>> =
        diagnostics.iter().map(LocatedDiagnostic::global).collect();
    let text = render(sys, &located, options, true);
    sys.write(&text);
}

/// Render diagnostics, file listing, then summary (native emit.go:120-128).
fn report_located(
    sys: &mut dyn System,
    files: &[DiagnosticFile],
    diagnostics: &[(String, Diagnostic)],
    options: &CompilerOptions,
    program: &tsr_compiler::Program<'_>,
) {
    let located: Vec<LocatedDiagnostic<'_>> = diagnostics
        .iter()
        .map(|(file_name, diagnostic)| {
            match files.iter().find(|file| file.file_name() == file_name) {
                Some(file) => LocatedDiagnostic::in_file(file, diagnostic),
                None => LocatedDiagnostic::global(diagnostic),
            }
        })
        .collect();
    let mut text = render(sys, &located, options, false);
    if options.list_files.is_true() || options.list_files_only.is_true() {
        for file in program.source_files() {
            text.push_str(file.file_name());
            text.push_str(&formatting_options(sys).newline);
        }
    }
    if should_use_pretty(sys, options) && !options.quiet.is_true() {
        tsr_diagnostics::format::write_error_summary_text(
            &mut text,
            &located,
            &formatting_options(sys),
        );
    }
    sys.write(&text);
}

#[cfg(test)]
mod directive_tests {
    use super::ExitStatus;
    use crate::baseline::{Baseline, BaselineSystem};

    fn compile(source: &str) -> (ExitStatus, String) {
        compile_with_options(source, &[])
    }

    fn compile_with_options(source: &str, extra_options: &[&str]) -> (ExitStatus, String) {
        let mut baseline = Baseline {
            name: "directives".to_string(),
            current_directory: "/project".to_string(),
            use_case_sensitive_file_names: true,
            files: vec![
                ("/project/a.ts".to_string(), source.to_string()),
                ("/project/empty.js".to_string(), String::new()),
            ],
            args: vec!["--noEmit", "--noLib", "--strict", "--pretty", "false", "a.ts"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            expected_status: String::new(),
            expected_output: String::new(),
            expects_emit: false,
            environment: Vec::new(),
        };
        baseline.args.extend(extra_options.iter().map(|option| (*option).to_string()));
        let mut system = BaselineSystem::new(&baseline);
        let status = crate::command_line(&mut system, &baseline.args);
        (status, system.output().to_string())
    }

    #[test]
    fn extended_statistics_count_checks_without_hiding_errors() {
        let (status, output) =
            compile_with_options("const bad: number = 'bad';", &["--extendedDiagnostics"]);
        assert_eq!(status, ExitStatus::DiagnosticsPresentOutputsSkipped, "{output}");
        assert!(output.contains("TS2322"), "{output}");
        assert!(output.contains("Checked files:         1\n"), "{output}");
        assert!(output.contains("Check time:            0.000s\n"), "{output}");
    }

    #[test]
    fn no_check_reports_zero_actual_checks() {
        let (status, output) = compile_with_options(
            "const bad: number = 'bad';",
            &["--extendedDiagnostics", "--noCheck"],
        );
        assert_eq!(status, ExitStatus::Success, "{output}");
        assert!(!output.contains("TS2322"), "{output}");
        assert!(output.contains("Checked files:         0\n"), "{output}");
    }

    #[test]
    fn nocheck_preamble_skips_full_check_and_unused_expect_error() {
        let (status, output) = compile_with_options(
            "// @ts-nocheck\n// @ts-expect-error\nconst bad: number = 'bad';\n",
            &["--extendedDiagnostics"],
        );
        assert_eq!(status, ExitStatus::Success, "{output}");
        assert!(!output.contains("error TS"), "{output}");
        assert!(output.contains("Checked files:         0\n"), "{output}");
    }

    #[test]
    fn a_syntax_error_is_reported_and_suppresses_the_semantic_pass() {
        // `GetDiagnosticsOfAnyProgram`: semantic diagnostics are asked for
        // only when no file has a syntactic one.
        let (status, output) = compile("const bad: number = 'bad';\nlet y = ;\n");
        assert_eq!(status, ExitStatus::DiagnosticsPresentOutputsSkipped, "{output}");
        assert!(output.contains("a.ts(2,9): error TS1109"), "{output}");
        assert!(!output.contains("TS2322"), "{output}");
    }

    #[test]
    fn binder_diagnostics_are_reported() {
        let (_, output) = compile("let a = 1;\nlet a = 2;\n");
        assert!(output.contains("a.ts(1,5): error TS2451"), "{output}");
        assert!(output.contains("a.ts(2,5): error TS2451"), "{output}");
    }

    #[test]
    fn never_rest_is_assignable_while_scalar_rest_still_reports() {
        let (_, output) = compile(
            "declare function okay(...args: never): void;\ndeclare function bad(...args: string): void;",
        );
        let errors: Vec<_> = output.lines().filter(|line| line.contains("error TS2370:")).collect();
        assert_eq!(errors.len(), 1, "{output}");
        assert!(errors[0].starts_with("a.ts(2,"), "{output}");
    }

    fn compile_files(files: Vec<(String, String)>, roots: &[&str], flags: &[&str]) -> String {
        let baseline = Baseline {
            current_directory: "/project".into(),
            use_case_sensitive_file_names: true,
            files,
            ..Baseline::default()
        };
        let mut system = BaselineSystem::new(&baseline);
        let args = ["--noEmit", "--pretty", "false", "--extendedDiagnostics"]
            .into_iter()
            .chain(flags.iter().copied())
            .chain(roots.iter().copied())
            .map(str::to_string)
            .collect::<Vec<_>>();
        crate::command_line(&mut system, &args);
        system.output().into()
    }

    #[test]
    fn library_checks_follow_skip_options_without_dropping_imported_types() {
        for (flags, checks, lib_error, declaration_error) in [
            (vec![], 3, true, true),
            (vec!["--skipDefaultLibCheck"], 2, false, true),
            (vec!["--skipLibCheck"], 1, false, false),
            (vec!["--noCheck"], 0, false, false),
        ] {
            let mut flags = flags;
            flags.extend(["--target", "es5"]);
            let output = compile_files(
                vec![
                    (format!("{}/lib.d.ts", crate::baseline::TSC_LIB_PATH),
                        "interface LibraryBad { value: MissingLibType; }".into()),
                    ("/project/dep.d.ts".into(),
                        "export interface Decl { value: number; other: MissingDeclType; }".into()),
                    ("/project/a.ts".into(),
                        "import type { Decl } from './dep'; declare const x: Decl; const bad: string = x.value;".into()),
                ],
                &["a.ts"],
                &flags,
            );
            assert!(
                output.contains(&format!("Checked files:         {checks}\n")),
                "{flags:?}: {output}"
            );
            assert_eq!(
                output.contains("Cannot find name 'MissingLibType'"),
                lib_error,
                "{flags:?}: {output}"
            );
            assert_eq!(
                output.contains("Cannot find name 'MissingDeclType'"),
                declaration_error,
                "{flags:?}: {output}"
            );
            assert_eq!(output.contains("error TS2322:"), checks != 0, "{flags:?}: {output}");
        }
    }

    #[test]
    fn js_full_check_distinguishes_unset_false_true_and_file_directives() {
        for (preamble, flags, checks) in [
            ("", vec![], 1),
            ("", vec!["--checkJs", "false"], 0),
            ("", vec!["--checkJs"], 1),
            ("// @ts-check\n", vec!["--checkJs", "false"], 1),
            ("// @ts-nocheck\n", vec!["--checkJs"], 0),
            ("// @ts-nocheck\n// @ts-check\n", vec![], 1),
            ("// @ts-check\n// @ts-nocheck\n", vec![], 0),
        ] {
            let mut flags = flags;
            flags.extend(["--noLib", "--allowJs"]);
            let output = compile_files(
                vec![("/project/a.js".into(), format!("{preamble}export const value = 1;"))],
                &["a.js"],
                &flags,
            );
            assert!(!output.contains("error TS"), "{flags:?}: {output}");
            assert!(
                output.contains(&format!("Checked files:         {checks}\n")),
                "{preamble:?} {flags:?}: {output}"
            );
        }
    }

    #[test]
    fn semantic_diagnostic_order_is_independent_of_worker_and_root_order() {
        let output = compile_files(
            vec![
                ("/project/z.ts".into(), "const z: number = 'bad';".into()),
                ("/project/a.ts".into(), "const a: number = 'bad';".into()),
            ],
            &["z.ts", "a.ts"],
            &["--noLib"],
        );
        let errors: Vec<_> = output.lines().filter(|line| line.contains("error TS2322:")).collect();
        assert_eq!(errors.len(), 2, "{output}");
        assert!(errors[0].starts_with("a.ts("), "{output}");
        assert!(errors[1].starts_with("z.ts("), "{output}");
    }

    #[test]
    fn plain_output_has_no_summary_and_color_variables_choose_the_default() {
        let run = |args: &[&str], environment: &[(&str, &str)]| {
            let mut baseline = Baseline {
                name: "pretty".to_string(),
                current_directory: "/project".to_string(),
                use_case_sensitive_file_names: true,
                files: vec![("/project/a.ts".to_string(), "const a: number = '';".to_string())],
                args: args.iter().map(|arg| (*arg).to_string()).collect(),
                expected_status: String::new(),
                expected_output: String::new(),
                expects_emit: false,
                environment: environment
                    .iter()
                    .map(|(name, value)| ((*name).to_string(), (*value).to_string()))
                    .collect(),
            };
            baseline.args.extend(["--noLib", "a.ts"].map(str::to_string));
            let mut system = BaselineSystem::new(&baseline);
            crate::command_line(&mut system, &baseline.args);
            system.output().to_string()
        };
        let plain = "a.ts(1,7): error TS2322: Type 'string' is not assignable to type 'number'.\n";
        // The replay host is a terminal, so only `NO_COLOR` or `--pretty false`
        // selects plain output; `--pretty` wins over the environment.
        assert_eq!(run(&["--pretty", "false"], &[("FORCE_COLOR", "1")]), plain);
        assert_eq!(run(&[], &[("NO_COLOR", "1"), ("FORCE_COLOR", "1")]), plain);
        let pretty = run(&["--pretty"], &[("NO_COLOR", "1")]);
        assert!(pretty.contains("Found 1 error"), "{pretty}");
        assert_eq!(run(&[], &[]), pretty);
    }

    #[test]
    fn every_pool_size_reports_each_owned_diagnostic_exactly_once() {
        // Each file imports its predecessor, so checkers resolve declarations
        // in files they do not own; only the owner may publish their errors.
        let files: Vec<(String, String)> = (0..5)
            .map(|index| {
                let import = if index == 0 {
                    String::new()
                } else {
                    format!("import {{ v{} }} from './f{}';\n", index - 1, index - 1)
                };
                (
                    format!("/project/f{index}.ts"),
                    format!("{import}export const v{index}: number = 'bad{index}';\n"),
                )
            })
            .collect();
        let roots = ["f0.ts", "f1.ts", "f2.ts", "f3.ts", "f4.ts"];
        // Statistics carry timings; compare diagnostics and the checked count.
        let report = |flags: &[&str]| {
            let output = compile_files(files.clone(), &roots, flags);
            let checked =
                output.lines().find(|line| line.starts_with("Checked files:")).map(str::to_owned);
            (output.split("\nFiles:").next().unwrap_or_default().to_owned(), checked)
        };
        let serial = report(&["--noLib", "--singleThreaded"]);
        assert_eq!(serial.0.matches("error TS2322:").count(), 5, "{}", serial.0);
        for checkers in ["1", "2", "3", "4", "9"] {
            let pooled = report(&["--noLib", "--checkers", checkers]);
            assert_eq!(pooled, serial, "--checkers {checkers}");
        }
    }

    #[test]
    fn directives_suppress_errors_in_the_cli() {
        let (status, output) = compile(
            "// @ts-expect-error\nconst a: string = 1;\n// @ts-ignore\nconst b: string = 2;\n",
        );
        assert_eq!(status, ExitStatus::Success, "{output}");
        assert!(output.is_empty(), "{output}");
    }

    #[test]
    fn unused_expect_error_and_unsuppressed_errors_still_report() {
        let (status, output) =
            compile("// @ts-expect-error\nconst a: number = 1;\nconst b: string = 2;\n");
        assert_eq!(status, ExitStatus::DiagnosticsPresentOutputsSkipped);
        assert!(output.contains("TS2578"), "{output}");
        assert!(output.contains("TS2322"), "{output}");
    }

    #[test]
    fn side_effect_js_imports_do_not_require_declarations() {
        let (status, output) = compile("import './empty.js';");
        assert_eq!(status, ExitStatus::Success, "{output}");
    }

    #[test]
    fn js_value_imports_still_require_declarations_under_strict() {
        let (status, output) = compile("import value from './empty.js'; value;");
        assert_eq!(status, ExitStatus::DiagnosticsPresentOutputsSkipped);
        assert!(output.contains("TS7016"), "{output}");
    }
    #[test]
    fn command_line_worker_overrides_replace_or_clear_project_values() {
        let fs = tsr_vfs::InMemoryFileSystem::new([], [], true);
        for (args, count, single) in [
            (
                vec!["--checkers", "2", "--singleThreaded", "false"],
                Some(2),
                tsr_core::Tristate::False,
            ),
            (vec!["--checkers", "null"], None, tsr_core::Tristate::True),
            (vec!["--singleThreaded", "false"], Some(8), tsr_core::Tristate::False),
            (vec![], Some(8), tsr_core::Tristate::True),
            (
                vec![
                    "--checkers",
                    "2",
                    "--checkers",
                    "null",
                    "--singleThreaded",
                    "true",
                    "--singleThreaded",
                    "null",
                ],
                None,
                tsr_core::Tristate::Unknown,
            ),
        ] {
            let args: Vec<_> = args.into_iter().map(str::to_string).collect();
            let cli = tsr_tsoptions::command_line::parse_command_line(&args, &fs, "/");
            assert!(cli.errors.is_empty());
            let mut options = tsr_core::CompilerOptions {
                checkers: Some(8),
                single_threaded: tsr_core::Tristate::True,
                ..Default::default()
            };
            super::apply_command_line_over_config(&mut options, &cli);
            assert_eq!(options.checkers, count);
            assert_eq!(options.single_threaded, single);
        }
    }
}
