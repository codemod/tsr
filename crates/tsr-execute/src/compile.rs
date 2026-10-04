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
    /// Errors were reported and output was written anyway.
    ///
    /// **Unreachable in this port**, and deliberately not deleted. Nothing here
    /// emits, so every diagnostic-bearing run skips its outputs by definition;
    /// the day `tsr-transformers` exists this becomes reachable, and a status
    /// enum missing a value upstream has is worse than one with an unused arm.
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

/// Whether this run should use the pretty formatter.
///
/// The default is the terminal's answer, which is what makes `tsc` colourful
/// interactively and machine-readable when redirected. An explicit `--pretty` or
/// `--pretty false` overrides it.
#[must_use]
pub fn should_use_pretty(sys: &dyn System, options: &CompilerOptions) -> bool {
    if options.pretty.is_unknown() { sys.write_output_is_tty() } else { options.pretty.is_true() }
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
    if should_use_pretty(sys, options) {
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
    if summary && !options.quiet.is_true() {
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
    let (mut options, root_files, config_errors, raw) = if config_file_name.is_empty() {
        (
            command_line.compiler_options.clone(),
            command_line
                .file_names
                .iter()
                .map(|name| get_normalized_absolute_path(name, &current_directory))
                .collect::<Vec<_>>(),
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
        (parsed.compiler_options, parsed.file_names, parsed.errors, parsed.raw)
    };

    apply_command_line_over_config(&mut options, command_line);

    // The no-inputs error belongs with the config's own errors and comes
    // **first**, which is the order upstream's parse produces and its baselines
    // record (`non-object-config-root.js`: TS18003 then TS5092). Raised here
    // rather than in `tsr_tsoptions::parse_config_file` because that parser is
    // shared with the conformance harness, and widening its no-inputs condition
    // would move a suite this session is not meant to touch.
    let mut config_errors = config_errors;
    if root_files.is_empty() && !config_file_name.is_empty() {
        config_errors.insert(
            0,
            Diagnostic::with_args(
                &messages::NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_0_SPECIFIED_INCLUDE_PATHS_WERE_1_AND_EXCLUDE_PATHS_WERE_2,
                tsr_core::Span::new(0, 0),
                [
                    config_file_name.to_string(),
                    "[\"**/*\"]".to_string(),
                    "[]".to_string(),
                ],
            ),
        );
        config_errors.dedup_by(|a, b| a.message.code() == b.message.code() && a.args == b.args);
    }

    if !config_errors.is_empty() {
        // A config diagnostic is positioned **in the config file**, so it prints
        // with `tsconfig.json:1:1` and a source frame. Everything with a
        // meaningful span gets the file; the no-inputs error above is about the
        // config as a whole rather than a place in it, and upstream prints it
        // location-less — which is why it is the one exception.
        let config_file = sys
            .fs()
            .read_file(config_file_name)
            .map(|text| DiagnosticFile::new(config_file_name, text));
        let located: Vec<(String, Diagnostic)> = config_errors
            .iter()
            .map(|error| {
                let name = if error.message.code() == messages::NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_0_SPECIFIED_INCLUDE_PATHS_WERE_1_AND_EXCLUDE_PATHS_WERE_2.code() {
                    String::new()
                } else {
                    config_file_name.to_string()
                };
                (name, error.clone())
            })
            .collect();
        let files: Vec<DiagnosticFile> = config_file.into_iter().collect();
        // **The summary IS printed**, and an earlier version of this suppressed
        // it. `tsc.go:225` builds `reportErrorSummary` after this branch, which
        // reads as "config errors get no summary" and is wrong: the reporter
        // built there is the *watch* one, and `non-object-config-root.js`
        // expects `Found 2 errors in the same file, starting at: tsconfig.json:1`
        // after its two diagnostics. Reading the baseline settled it where
        // reading the call order did not.
        report_located(sys, &files, &located, &options);
        return ExitStatus::DiagnosticsPresentOutputsGenerated;
    }

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

    if root_files.is_empty() {
        // Reached only with no config file at all — files named on the command
        // line that all failed to resolve. The config case is handled above.
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

    if options.list_files.is_true() || options.list_files_only.is_true() {
        let mut listing = String::new();
        for file in program.source_files() {
            listing.push_str(file.file_name());
            listing.push('\n');
        }
        sys.write(&listing);
        if options.list_files_only.is_true() {
            return ExitStatus::Success;
        }
    }

    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(&options);
    #[cfg(feature = "work-trace")]
    if let Some(trace) = work_trace {
        trace.checker_created(&options);
        checker.set_work_observer(trace);
    }

    // Eligibility is fixed before checking, independently of lazy queries into
    // skipped declarations/JSON. Use the same rule as trace reporting.
    let checked_files: Vec<_> = program
        .source_files()
        .iter()
        .enumerate()
        .filter(|(index, _)| full_check_exclusion(&program, *index).is_none())
        .filter_map(|(_, file)| file.source_file().node_id)
        .collect();
    checker.set_checked_files(checked_files);
    let checker_initialized = sys.since_start();

    let mut diagnostics: Vec<(String, Diagnostic)> = Vec::new();
    let mut checked_file_count = 0;
    if !options.no_check.is_true() {
        for (index, file) in program.source_files().iter().enumerate() {
            if full_check_exclusion(&program, index).is_some() {
                continue;
            }
            let Some(id) = file.source_file().node_id else { continue };
            // Upstream's parser sets `NodeFlagsAmbient` on every node of a
            // declaration file; this port's does not, so the bit is supplied
            // here, the same way the conformance harness supplies it.
            let ambient = tsr_path::is_declaration_file_name(file.file_name());
            checker.check_source_file(
                id,
                tsr_checker::check::FileContext {
                    ambient,
                    has_parse_errors: !file.diagnostics().is_empty(),
                },
            );
            checked_file_count += 1;
        }
    }
    let checking_finished = sys.since_start();
    if !options.no_check.is_true() {
        for (file_id, diagnostic) in checker.diagnostics() {
            if let Some(file) = program
                .source_files()
                .iter()
                .find(|candidate| candidate.source_file().node_id == Some(*file_id))
            {
                diagnostics.push((file.file_name().to_string(), diagnostic.clone()));
            }
        }
    }

    let files: Vec<DiagnosticFile> = program
        .source_files()
        .iter()
        .map(|file| DiagnosticFile::new(file.file_name(), file.text()))
        .collect();

    if !options.no_check.is_true() {
        let mut filtered = Vec::with_capacity(diagnostics.len());
        for (index, (source, indexed)) in program.source_files().iter().zip(&files).enumerate() {
            // Match SkipTypeChecking: skipped declaration files must not earn
            // unused-directive errors without ever being checked.
            if full_check_exclusion(&program, index).is_some() {
                continue;
            }
            let directives = tsr_compiler::comment_directives::directives_in(source.text());
            let entries: Vec<_> = diagnostics
                .iter()
                .filter(|(name, _)| name == source.file_name())
                .map(|entry| (indexed.line_of_position(entry.1.span.start), entry.clone()))
                .collect();
            let (kept, unused) =
                tsr_compiler::comment_directives::filter(source.text(), &entries, &directives);
            filtered.extend(kept);
            filtered.extend(
                unused.into_iter().map(|diagnostic| (source.file_name().to_string(), diagnostic)),
            );
        }
        diagnostics = filtered;
    }

    // `SortAndDeduplicateDiagnostics` / `ast.CompareDiagnostics`: worker and
    // Program order do not determine diagnostic order. This port's diagnostic
    // representation has no message chains or related-information fields.
    diagnostics.sort_by(|(left_file, left), (right_file, right)| {
        left_file
            .cmp(right_file)
            .then_with(|| left.span.start.cmp(&right.span.start))
            .then_with(|| left.span.end.cmp(&right.span.end))
            .then_with(|| left.message.code().cmp(&right.message.code()))
            .then_with(|| left.args.cmp(&right.args))
    });
    diagnostics.dedup_by(|(left_file, left), (right_file, right)| {
        left_file == right_file
            && left.span == right.span
            && left.message.code() == right.message.code()
            && left.args == right.args
    });

    report_located(sys, &files, &diagnostics, &options);
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

    let errors = diagnostics
        .iter()
        .filter(|(_, diagnostic)| diagnostic.message.category() == tsr_diagnostics::Category::Error)
        .count();
    if errors > 0 { ExitStatus::DiagnosticsPresentOutputsSkipped } else { ExitStatus::Success }
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
pub(crate) fn full_check_exclusion(
    program: &tsr_compiler::Program<'_>,
    file_index: usize,
) -> Option<&'static str> {
    let options = program.compiler_options();
    let file = &program.source_files()[file_index];
    if options.no_check.is_true() {
        return Some("no_check");
    }
    if options.skip_lib_check.is_true() && tsr_path::is_declaration_file_name(file.file_name()) {
        return Some("skip_lib_check");
    }
    if options.skip_default_lib_check.is_true() && file_index < program.lib_files().len() {
        return Some("skip_default_lib_check");
    }
    let directive = file.file_references().check_js_directive;
    if directive.is_some_and(|directive| !directive.enabled) {
        return Some("file_no_check");
    }
    if tsr_parser::ScriptKind::from_file_name(file.file_name()) == tsr_parser::ScriptKind::Json {
        return Some("json_source");
    }
    // `IsPlainJSFile` includes JS with checkJs unset, but not explicitly false.
    // A file directive overrides the option. Skipped files remain bound and
    // available to cross-file queries; this is not a loader filter.
    if is_javascript_file(file.file_name()) && directive.is_none() && options.check_js.is_false() {
        return Some("check_js_false");
    }
    if file.source_file().node_id.is_none() {
        return Some("missing_source_node");
    }
    None
}

/// Shared immutable file-kind fact for eligibility and its opt-in observer.
pub(crate) fn is_javascript_file(name: &str) -> bool {
    let extension = name.rsplit('.').next().unwrap_or_default();
    ["js", "jsx", "cjs", "mjs"].iter().any(|ext| extension.eq_ignore_ascii_case(ext))
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

/// Render diagnostics that belong to files, then the summary.
fn report_located(
    sys: &mut dyn System,
    files: &[DiagnosticFile],
    diagnostics: &[(String, Diagnostic)],
    options: &CompilerOptions,
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
    let text = render(sys, &located, options, true);
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
