//! `tsc`, as a command.
//!
//! Ported from `internal/execute` at the pinned commit — `CommandLine`
//! (`tsc.go:52`), `tscCompilation` (`:119`), `findConfigFile` (`:265`), and the
//! exit statuses in `internal/execute/tsc/compile.go:30-39`.
//!
//! # What runs today
//!
//! Type checking, and the invocations that never reach a compiler: `--version`,
//! `--help`, `--showConfig`, and every error path that decides a command line
//! is meaningless. See `STATUS-cli.md` for the phase board.
//!
//! # What does not
//!
//! **Emit.** There is no `tsr-transformers`, so nothing writes JavaScript, and
//! 144 of the 194 upstream `tsc` baselines are behind that. Every compilation
//! here behaves as `--noEmit`, and it says so rather than pretending: see
//! [`ExitStatus`]'s note on why the outputs-generated status is unreachable.
//!
//! **`--build`, `--watch`, incremental.** Parsed and reported as unimplemented
//! rather than silently ignored, because silently compiling once when the user
//! asked to watch is worse than refusing.

pub mod baseline;
pub mod compile;
pub mod help;
pub mod help_all;
pub mod os_system;
pub mod show_config;
pub mod system;

use tsr_diagnostics::{Diagnostic, format::LocatedDiagnostic, messages};
use tsr_path::{combine_paths, for_each_ancestor_directory, normalize_path};

pub use compile::{ExitStatus, run_compilation};
pub use os_system::OsSystem;
pub use system::{System, VERSION};

/// Run a `tsc` command line to completion (`execute.CommandLine`).
///
/// The process's exit code is this value.
pub fn command_line(sys: &mut dyn System, args: &[String]) -> ExitStatus {
    // `-b` must be the *first* argument to mean build mode; anywhere else it is
    // an ordinary unknown option, which the parser's did-you-mean handles.
    if let Some(first) = args.first() {
        let lowered = first.to_ascii_lowercase();
        if matches!(lowered.as_str(), "-b" | "--b" | "-build" | "--build") {
            return build_not_implemented(sys);
        }
    }
    tsc_compilation(sys, args)
}

/// `--build` is parsed and refused (`tscBuildCompilation`).
///
/// Upstream runs a whole orchestrator here — `execute/build` is 2,039 lines over
/// `execute/incremental`'s 3,404 — and all of it is behind emit. Reporting
/// `NotImplemented` is upstream's own status 5 for exactly this situation, so
/// refusing costs no invented surface.
fn build_not_implemented(sys: &mut dyn System) -> ExitStatus {
    sys.write("error TS0: '--build' is not implemented in this port.\n");
    ExitStatus::NotImplemented
}

/// `tscCompilation` — decide what this invocation means, then do it.
///
/// Almost all of the function is error paths, and their *order* is the
/// behaviour: `--version` beats `--help`, both beat a missing config, and a
/// command line with both `--project` and file names is rejected before either
/// is looked at.
fn tsc_compilation(sys: &mut dyn System, args: &[String]) -> ExitStatus {
    let current_directory = sys.current_directory().to_string();
    let command_line =
        tsr_tsoptions::command_line::parse_command_line(args, sys.fs(), &current_directory);

    if !command_line.errors.is_empty() {
        report_diagnostics(sys, &command_line.errors);
        return ExitStatus::DiagnosticsPresentOutputsSkipped;
    }

    let options = &command_line.compiler_options;

    if options.init.is_true() {
        // `--init` writes a `tsconfig.json`. Not ported: it needs the help
        // system's per-option descriptions to write the commented template, and
        // writing a *different* template would be an invented artifact that
        // users' repositories would then contain.
        sys.write("error TS0: '--init' is not implemented in this port.\n");
        return ExitStatus::NotImplemented;
    }

    if options.version.is_true() {
        print_version(sys);
        return ExitStatus::Success;
    }

    if options.help.is_true() || options.all.is_true() {
        crate::help::print_help(sys, options.all.is_true());
        return ExitStatus::Success;
    }

    if options.watch.is_true() && options.list_files_only.is_true() {
        report_diagnostics(
            sys,
            &[Diagnostic::with_args(
                &messages::OPTIONS_0_AND_1_CANNOT_BE_COMBINED,
                tsr_core::Span::new(0, 0),
                ["watch".to_string(), "listFilesOnly".to_string()],
            )],
        );
        return ExitStatus::DiagnosticsPresentOutputsSkipped;
    }

    // Which `tsconfig.json`, if any, this invocation is about.
    let mut config_file_name = String::new();
    if !options.project.is_empty() {
        if !command_line.file_names.is_empty() {
            report_diagnostics(
                sys,
                &[Diagnostic::new(
                    &messages::OPTION_PROJECT_CANNOT_BE_MIXED_WITH_SOURCE_FILES_ON_A_COMMAND_LINE,
                    tsr_core::Span::new(0, 0),
                )],
            );
            return ExitStatus::DiagnosticsPresentOutputsSkipped;
        }

        let file_or_directory = normalize_path(&options.project);
        if sys.fs().directory_exists(&file_or_directory) {
            config_file_name = combine_paths(&file_or_directory, &["tsconfig.json"]);
            if !sys.fs().file_exists(&config_file_name) {
                report_diagnostics(
                    sys,
                    &[Diagnostic::with_args(
                        &messages::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_CURRENT_DIRECTORY_COLON_0,
                        tsr_core::Span::new(0, 0),
                        [config_file_name.clone()],
                    )],
        );
                return ExitStatus::DiagnosticsPresentOutputsSkipped;
            }
        } else {
            config_file_name.clone_from(&file_or_directory);
            if !sys.fs().file_exists(&config_file_name) {
                report_diagnostics(
                    sys,
                    &[Diagnostic::with_args(
                        &messages::THE_SPECIFIED_PATH_DOES_NOT_EXIST_COLON_0,
                        tsr_core::Span::new(0, 0),
                        [file_or_directory],
                    )],
                );
                return ExitStatus::DiagnosticsPresentOutputsSkipped;
            }
        }
    } else if !options.ignore_config.is_true() || command_line.file_names.is_empty() {
        let search_path = normalize_path(&current_directory);
        config_file_name = find_config_file(sys, &search_path);

        if !command_line.file_names.is_empty() {
            if !config_file_name.is_empty() {
                // Naming files *and* having a config is an error rather than a
                // precedence rule, which is the part users are surprised by.
                report_diagnostics(
                    sys,
                    &[Diagnostic::new(
                        &messages::TSCONFIG_JSON_IS_PRESENT_BUT_WILL_NOT_BE_LOADED_IF_FILES_ARE_SPECIFIED_ON_COMMANDLINE_USE_IGNORECONFIG_TO_SKIP_THIS_ERROR,
                        tsr_core::Span::new(0, 0),
                    )],
        );
                return ExitStatus::DiagnosticsPresentOutputsSkipped;
            }
        } else if config_file_name.is_empty() {
            if options.show_config.is_true() {
                report_diagnostics(
                    sys,
                    &[Diagnostic::with_args(
                        &messages::CANNOT_FIND_A_TSCONFIG_JSON_FILE_AT_THE_CURRENT_DIRECTORY_COLON_0,
                        tsr_core::Span::new(0, 0),
                        [normalize_path(&current_directory)],
                    )],
        );
            } else {
                // `tsc` with nothing to do prints its banner and help — and
                // exits **1**, not 0. A build script that treats "no config" as
                // success would otherwise pass while compiling nothing.
                print_version(sys);
                crate::help::print_help(sys, false);
            }
            return ExitStatus::DiagnosticsPresentOutputsSkipped;
        }
    }

    compile::run_compilation(sys, &command_line, &config_file_name)
}

/// `findConfigFile` — the nearest `tsconfig.json` at or above `search_path`.
fn find_config_file(sys: &dyn System, search_path: &str) -> String {
    for_each_ancestor_directory(search_path, |ancestor| {
        let candidate = combine_paths(ancestor, &["tsconfig.json"]);
        if sys.fs().file_exists(&candidate) { Some(candidate) } else { None }
    })
    .unwrap_or_default()
}

/// `PrintVersion`.
fn print_version(sys: &mut dyn System) {
    let text = format!("Version {}\n", sys.version());
    sys.write(&text);
}

/// Render diagnostics that are not attached to any file.
fn report_diagnostics(sys: &mut dyn System, diagnostics: &[Diagnostic]) {
    // No options are available at every call site here — some of these errors
    // are *about* the options — so the terminal decides, which is the same
    // default `should_use_pretty` applies when `--pretty` is unset.
    let formatting = compile::formatting_options(sys);
    let located: Vec<LocatedDiagnostic<'_>> =
        diagnostics.iter().map(LocatedDiagnostic::global).collect();
    let mut text = String::new();
    if sys.write_output_is_tty() {
        tsr_diagnostics::format::write_format_diagnostics_with_color_and_context(
            &mut text,
            &located,
            &formatting,
        );
        text.push_str(&formatting.newline);
    } else {
        tsr_diagnostics::format::write_format_diagnostics(&mut text, &located, &formatting);
    }
    sys.write(&text);
}
