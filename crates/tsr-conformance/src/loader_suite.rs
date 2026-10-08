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

use crate::{
    CaseEntry,
    module_suite::{first_difference, sanitize},
    suite::{Outcome, Suite},
    trace_case::{Setup, prepare},
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
        let prepared = match prepare(case) {
            Setup::Ready(prepared) => prepared,
            Setup::Skip(reason) => return Outcome::Skipped { reason },
        };
        // `libReplacement` resolves `@typescript/lib-*` for every lib file the
        // target implies — the one lib task that *does* trace. The loader now
        // loads lib files, but `pathForLibFile` takes only the plain path
        // (bd tsr-9or.5), so the resolutions this case expects are absent.
        if prepared.options.lib_replacement.is_true() {
            return Outcome::Skipped {
                reason: "libReplacement resolves the bundled lib files through the module \
                         resolver, which the loader does not do (bd tsr-9or.5)"
                    .to_string(),
            };
        }

        // No `default_library_path`: the corpus host is an in-memory file system
        // holding the case's own units, and the bundled `lib.*.d.ts` are not on
        // it. The lib tasks are still created and simply find nothing, which is
        // what keeps this suite measuring resolution rather than lib loading —
        // a lib file resolves nothing, so a program that has one and a program
        // that does not produce the same trace.
        // The arena the walk parses into. Scoped to this case and dropped with
        // it, which is ADR-0034's caller-owns-the-arena shape at its smallest:
        // this suite reads only the traces, so nothing borrowing it escapes.
        let arena = tsr_core::Arena::new();
        let host = prepared.host.cached();
        let loaded = FileLoader::load(
            &arena,
            &host,
            tsr_compiler::LoadOptions {
                compiler_options: prepared.options.clone(),
                root_file_names: prepared.root_file_names.clone(),
                default_library_path: String::new(),
            },
        );
        let produced = sanitize(
            &loaded.traces,
            &prepared.current_directory,
            prepared.use_case_sensitive_file_names,
        );
        let actual = requests_only(&produced);
        let expected = requests_only(&prepared.expected);
        if actual == expected {
            Outcome::Passed
        } else {
            Outcome::Failed { reason: describe(&expected, &actual, &loaded, &prepared.options) }
        }
    }
}

/// `file_loader`, over each named configuration of a case whose directives
/// vary, against that configuration's own `case(<configuration>).trace.json`.
///
/// Kept beside [`crate::module_suite::ModuleResolutionConfigured`] so the two
/// halves of a trace keep sharing one denominator per population
/// ([`crate::trace_case`]).
pub struct FileLoaderRequestsConfigured;

impl Suite for FileLoaderRequestsConfigured {
    fn name(&self) -> &'static str {
        "file_loader_configured"
    }

    fn describes(&self) -> &'static str {
        "the file_loader suite's judgement for each configuration upstream's runner \
         compiles a configuration-varied case under, against that configuration's \
         suffixed .trace.json"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if case.configuration.is_none() {
            return Outcome::Skipped { reason: "not a named configuration".into() };
        }
        FileLoaderRequests.run(case)
    }

    fn per_configuration(&self) -> bool {
        true
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

/// The first divergence, plus enough context to classify it without re-running.
fn describe(
    expected: &[&str],
    actual: &[&str],
    loaded: &tsr_compiler::LoadedFiles<'_>,
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
