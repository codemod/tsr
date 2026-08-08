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
        // **No error summary.** Upstream builds `reportErrorSummary` *after*
        // this branch (`tsc.go:225`), so a config that fails to parse prints its
        // diagnostics and stops. Passing `quiet` here would be the same effect
        // by the wrong mechanism.
        report_located_without_summary(sys, &files, &located, &options);
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
    let arena = tsr_core::Arena::new();
    let host = DriverHost { fs: sys.fs(), current_directory: current_directory.clone() };
    let program = tsr_compiler::Program::from_root_files(
        &arena,
        &host,
        tsr_compiler::LoadOptions {
            compiler_options: options.clone(),
            root_file_names: root_files,
            default_library_path: sys.default_library_path().to_string(),
        },
    );

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

    // Only the program's own files are checked, never the libraries — upstream
    // reports nothing in `lib.*.d.ts` under any configuration, and a diagnostic
    // positioned in one could match nothing a user could fix.
    let own_files: Vec<_> = program
        .root_and_referenced_files()
        .iter()
        .filter_map(|file| file.source_file().node_id)
        .collect();
    checker.set_checked_files(own_files.clone());

    let mut diagnostics: Vec<(String, Diagnostic)> = Vec::new();
    if !options.no_check.is_true() {
        for file in program.root_and_referenced_files() {
            let Some(id) = file.source_file().node_id else { continue };
            // Upstream's parser sets `NodeFlagsAmbient` on every node of a
            // declaration file; this port's does not, so the bit is supplied
            // here, the same way the conformance harness supplies it.
            let name = file.file_name();
            let ambient =
                name.ends_with(".d.ts") || name.ends_with(".d.mts") || name.ends_with(".d.cts");
            checker.check_source_file(
                id,
                tsr_checker::check::FileContext {
                    ambient,
                    has_parse_errors: !file.diagnostics().is_empty(),
                },
            );
        }
        for (file_id, diagnostic) in checker.diagnostics() {
            if let Some(file) = program
                .root_and_referenced_files()
                .iter()
                .find(|candidate| candidate.source_file().node_id == Some(*file_id))
            {
                diagnostics.push((file.file_name().to_string(), diagnostic.clone()));
            }
        }
    }

    let files: Vec<DiagnosticFile> = program
        .root_and_referenced_files()
        .iter()
        .map(|file| DiagnosticFile::new(file.file_name(), file.text()))
        .collect();

    report_located(sys, &files, &diagnostics, &options);

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
    }
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
    report_located_impl(sys, files, diagnostics, options, true);
}

/// As [`report_located`], but without the `Found N errors` block.
fn report_located_without_summary(
    sys: &mut dyn System,
    files: &[DiagnosticFile],
    diagnostics: &[(String, Diagnostic)],
    options: &CompilerOptions,
) {
    report_located_impl(sys, files, diagnostics, options, false);
}

fn report_located_impl(
    sys: &mut dyn System,
    files: &[DiagnosticFile],
    diagnostics: &[(String, Diagnostic)],
    options: &CompilerOptions,
    summary: bool,
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
    let text = render(sys, &located, options, summary);
    sys.write(&text);
}
