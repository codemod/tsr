//! `file_loader`: the *requests* against upstream's `.trace.json` baselines.
//!
//! # The other half of the oracle
//!
//! [`crate::module_suite`] judges what a resolution *did*, and takes the list of
//! resolutions from the baseline's own headers. That was a deliberate split
//! ([ADR-0018](../../../docs/adr/0018-splitting-the-resolution-oracle.md)), and
//! it left a named blind spot: nothing checked that a real compiler would have
//! asked for those specifiers, from those files, in that order.
//!
//! This suite is the other half. It runs [`tsr_compiler::FileLoader`] over the
//! case — the same walk `tsc` does — and compares the resolutions it *requests*
//! against the baseline, ignoring everything the resolver does in between.
//!
//! Concretely, both sides are reduced to the lines that describe a request:
//!
//! ```text
//! ======== Resolving module 'foo' from '/a.ts'. ========
//! Resolving in CJS mode with conditions 'require', 'types', 'node'.
//! ```
//!
//! and nothing else. The first line is the loader's specifier and containing
//! file; the second is the mode it chose, rendered by the resolver. The closing
//! `======== Module name … was resolved ========` header is *not* compared —
//! that is a resolution result, and `module_resolution` owns it.
//!
//! # Why the mode line is in scope, and why that matters
//!
//! ADR-0018 recorded a circularity it could not avoid: `module_resolution` reads
//! each resolution's mode *out of the baseline*, because a file's module format
//! is program-level knowledge the resolver is not given. Its stated falsifier was
//! "if the resolver ever emitted the wrong mode, the suite would not catch it —
//! it would have supplied it."
//!
//! Here nothing is supplied. `getImpliedNodeFormatForFile`,
//! `getModeForUsageLocation`, and the `package.json` `type` lookup all run for
//! real, and the mode line is compared. This suite is the falsifier ADR-0018
//! asked for.
//!
//! # The three empty-trace cases
//!
//! `module_resolution` skips three cases whose expected trace is *empty*, with
//! the reason "the claim 'we would also have requested nothing' belongs to the
//! file loader". Here it is a real assertion, and it is the sharpest one in the
//! suite — each of the three is a way to resolve something you should not:
//!
//! - `compiler/moduleResolutionWithRequire` — `require()` in a `.ts` file.
//! - `compiler/jsdocInTypeScript` — `import("…")` inside JSDoc in a `.ts` file,
//!   and `import(String())`, whose argument is not a literal.
//! - `conformance/globalAugmentationModuleResolution` — `declare global`, whose
//!   name is an identifier and not a module.

use tsr_compiler::{FileLoader, RequestKind};
use tsr_core::CompilerOptions;
use tsr_path::get_normalized_absolute_path;

use crate::{
    CaseEntry, TestCase,
    module_suite::{
        SRC_FOLDER, TestHost, build_file_system, compiler_options, first_difference, sanitize,
        upstream_skip_reason,
    },
    suite::{Outcome, Suite},
};

/// The `file_loader` suite.
pub struct FileLoaderRequests;

impl Suite for FileLoaderRequests {
    fn name(&self) -> &'static str {
        "file_loader"
    }

    fn describes(&self) -> &'static str {
        "the loader requests the resolutions upstream's .trace.json records, in order"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let Ok(parsed) = case.load() else {
            return Outcome::Skipped { reason: "case could not be read".to_string() };
        };
        if !parsed.options.contains_key("traceresolution") {
            return Outcome::Skipped {
                reason: "case does not set @traceResolution, so upstream records no trace"
                    .to_string(),
            };
        }
        if parsed.files.iter().any(|file| file.name.ends_with("tsconfig.json")) {
            return Outcome::Skipped {
                reason: "case configures itself with a tsconfig.json, which needs tsr-tsoptions \
                         (bd tsr-9or slice 3)"
                    .to_string(),
            };
        }
        // `libReplacement` resolves `@typescript/lib-*` through the module
        // resolver for every lib file the target implies, which needs the
        // bundled `lib.*.d.ts` files on the host. The loader does not load lib
        // files at all (bd tsr-9or.5).
        if parsed.options.get("libreplacement").is_some_and(|value| value != "false") {
            return Outcome::Skipped {
                reason: "libReplacement resolves the bundled lib files, which the loader does not \
                         load (bd tsr-9or.5)"
                    .to_string(),
            };
        }

        let baseline_path = case.baseline_path("trace.json");
        let expected = if let Ok(expected) = std::fs::read_to_string(&baseline_path) {
            expected
        } else {
            if let Some(reason) = upstream_skip_reason(&parsed) {
                return Outcome::Skipped { reason: format!("upstream skips this case: {reason}") };
            }
            if case.baselines.has_variant(case.stem(), "trace.json") {
                return Outcome::Skipped {
                    reason: "configuration-varied trace baselines (bd tsr-bb4.1)".to_string(),
                };
            }
            // No baseline and not skipped means upstream ran the case and traced
            // *nothing*. Unlike `module_resolution`, that is a claim this suite
            // can check: the expectation is an empty request list.
            String::new()
        };

        let current_directory = parsed.current_directory.as_deref().map_or_else(
            || SRC_FOLDER.to_string(),
            |dir| get_normalized_absolute_path(dir, SRC_FOLDER),
        );
        let options = compiler_options(&parsed, &current_directory);
        let use_case_sensitive_file_names = parsed
            .options
            .get("usecasesensitivefilenames")
            .is_none_or(|value| !value.eq_ignore_ascii_case("false"));

        let host = TestHost {
            fs: build_file_system(&parsed, &current_directory, use_case_sensitive_file_names),
            current_directory: current_directory.clone(),
        };
        let root_file_names = root_file_names(&parsed, &current_directory);
        let loaded = FileLoader::load(&host, options.clone(), &root_file_names);

        let produced = sanitize(&loaded.traces, &current_directory, use_case_sensitive_file_names);
        let actual = requests_only(&produced);
        let expected = requests_only(&expected);
        if actual == expected {
            Outcome::Passed
        } else {
            Outcome::Failed { reason: describe(&expected, &actual, &loaded, &options) }
        }
    }
}

/// The lines that describe a *request*, and nothing else.
///
/// Two shapes survive: the opening block header, which is the loader's
/// specifier and containing file, and the mode line, which is the loader's
/// chosen format. Everything else in a trace is the resolver's walk, which
/// `module_resolution` judges.
fn requests_only(trace: &str) -> Vec<&str> {
    trace
        .lines()
        .filter(|line| line.starts_with("======== Resolving ") || line.starts_with("Resolving in "))
        .collect()
}

/// Which units the compiler is invoked on (`compiler_runner.newCompilerTest`).
///
/// Not a detail: it decides whether a five-file case has five root files or one.
/// Upstream's rule is a heuristic on the *last* unit — if it uses `require(` or
/// a `/// <reference path`, the case is assumed to pull the rest in by
/// reference, so only that unit is a root and the others merely exist on disk.
///
/// `.json` and `.tsbuildinfo` units are never root files
/// (`harnessutil.CompileFilesEx`).
fn root_file_names(case: &TestCase, current_directory: &str) -> Vec<String> {
    let units: Vec<&crate::case::TestFile> = case.files.iter().collect();
    let Some(last) = units.last() else { return Vec::new() };

    let only_last = case.options.contains_key("noimplicitreferences")
        || last.content.contains("require(")
        || contains_path_reference(&last.content);

    let selected: Vec<&crate::case::TestFile> =
        if only_last { vec![*last] } else { units.into_iter().collect() };

    selected
        .into_iter()
        .filter(|unit| {
            !tsr_path::extension::file_extension_is(&unit.name, ".json")
                && !tsr_path::extension::file_extension_is(&unit.name, ".tsbuildinfo")
        })
        .map(|unit| get_normalized_absolute_path(&unit.name, current_directory))
        .collect()
}

/// Upstream's `referencesRegex`, which is the literal pattern `reference\spath`.
fn contains_path_reference(content: &str) -> bool {
    content.match_indices("reference").any(|(index, _)| {
        let rest = &content[index + "reference".len()..];
        rest.starts_with(|c: char| c.is_whitespace()) && rest[1..].starts_with("path")
    })
}

/// The first divergence, plus enough context to classify it without re-running.
fn describe(
    expected: &[&str],
    actual: &[&str],
    loaded: &tsr_compiler::LoadedFiles,
    options: &CompilerOptions,
) -> String {
    let difference = first_difference(&expected.join("\n"), &actual.join("\n"));
    let requested = loaded
        .requests
        .iter()
        .map(|request| {
            let kind = match request.kind {
                RequestKind::Module => "module",
                RequestKind::TypeReferenceDirective => "types",
            };
            format!(
                "{kind} {:?} from {:?} {:?}",
                request.name, request.containing_file, request.mode
            )
        })
        .collect::<Vec<_>>()
        .join("; ");
    format!(
        "{difference} [resolution={}] [requested: {requested}]",
        options.module_resolution_kind()
    )
}
