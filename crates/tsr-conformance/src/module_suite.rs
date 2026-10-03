//! `module_resolution`: the resolver against upstream's `.trace.json` baselines.
//!
//! # The oracle
//!
//! 146 `.trace.json` files under
//! `vendor/typescript-go/testdata/baselines/reference/` record, line by line,
//! every step typescript-go's resolver took for a case that sets
//! `// @traceResolution: true`. This is a far stricter oracle than "did the
//! specifier resolve": it pins every file probe, every directory probe, every
//! `package.json` read, and every `exports` condition considered, in order. A
//! resolver that reaches the right file by a different route fails.
//!
//! # What this suite judges, and what it does not
//!
//! A trace has two halves, and they belong to different components:
//!
//! 1. **Which resolutions were requested** — a specifier, a containing file, and
//!    a module format — which is the *file loader*'s job: it walks the program,
//!    collects each file's imports and reference directives, and asks for them in
//!    a particular order.
//! 2. **What each resolution did** — the walk, which is the *resolver*'s job.
//!
//! This suite judges (2) and takes (1) from the baseline itself: each
//! `======== Resolving module 'X' from 'Y'. ========` header names a request, and
//! the suite replays exactly those requests through `tsr-module`. Everything
//! between the headers is compared.
//!
//! That is a real limitation and it is deliberate. It means the suite **cannot**
//! catch a loader that would have asked for the wrong things, or asked in the
//! wrong order — building one number out of both would have entangled the two
//! failure modes so that neither could be localised.
//!
//! The loader now exists, and it has that second gate:
//! [`crate::loader_suite`] runs it for real and compares the *headers*, their
//! order, and the resolution mode, from these same baselines. Between them the
//! two suites cover a trace end to end. See
//! [ADR-0018](../../../docs/adr/0018-splitting-the-resolution-oracle.md) for the
//! split and
//! [ADR-0019](../../../docs/adr/0019-the-loader-gate-discharges-the-mode-circularity.md)
//! for what the second half established.
//!
//! # The skip rules, and why they are not silent
//!
//! Which cases are judged, and why the rest are not, is
//! [`crate::trace_case`]'s — shared with `file_loader`, so the two suites'
//! denominators can only differ where a suite says so. `upstream_skip_reason`
//! there reimplements `harnessutil.SkipUnsupportedCompilerOptions` rather than
//! inferring a reason from an absent baseline, and
//! [`the_skip_rule_explains_every_missing_baseline`] asserts it accounts for
//! the gap exactly.

use std::collections::{HashMap, hash_map::Entry};

use tsr_core::{ModuleResolutionKind, ResolutionMode};
use tsr_module::{messages::Trace, resolver::Resolver, types::ResolvedModule};
use tsr_path::to_path;

use crate::{
    CaseEntry,
    suite::{Outcome, Suite},
    trace_case::{Setup, prepare},
};

/// The `module_resolution` suite.
pub struct ModuleResolution;

impl Suite for ModuleResolution {
    fn name(&self) -> &'static str {
        "module_resolution"
    }

    fn describes(&self) -> &'static str {
        "resolver trace matches upstream's .trace.json, step for step"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        let prepared = match prepare(case) {
            Setup::Ready(prepared) => prepared,
            Setup::Skip(reason) => return Outcome::Skipped { reason },
        };
        if prepared.expected.is_empty() {
            // Upstream traced nothing, so there is no request list to replay.
            // Whether *we* would also have requested nothing is the loader's
            // claim, and `crate::loader_suite` asserts it.
            return Outcome::Skipped {
                reason: "upstream recorded an empty trace: the case performs no resolutions"
                    .to_string(),
            };
        }

        let requests =
            parse_requests(&prepared.expected, prepared.options.module_resolution_kind());
        if requests.is_empty() {
            return Outcome::Failed {
                reason: "baseline contains no resolution request headers".to_string(),
            };
        }

        let host = prepared.host.cached();
        let resolver = Resolver::new(&host, prepared.options.clone());
        let mut traces: Vec<Trace> = Vec::new();
        for request in &requests {
            let (_, request_traces) = match request.kind {
                RequestKind::Module => resolver.resolve_module_name(
                    &request.name,
                    &request.containing_file,
                    request.mode,
                ),
                RequestKind::TypeReferenceDirective => {
                    let (_, traces) = resolver.resolve_type_reference_directive(
                        &request.name,
                        &request.containing_file,
                        request.mode,
                    );
                    (ResolvedModule::default(), traces)
                }
            };
            traces.extend(request_traces);
        }

        let actual =
            sanitize(&traces, &prepared.current_directory, prepared.use_case_sensitive_file_names);
        if actual == prepared.expected {
            Outcome::Passed
        } else {
            Outcome::Failed { reason: first_difference(&prepared.expected, &actual) }
        }
    }
}

/// What kind of thing a baseline block resolves.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RequestKind {
    Module,
    TypeReferenceDirective,
}

/// One resolution the baseline says upstream performed.
#[derive(Debug, Clone)]
struct Request {
    kind: RequestKind,
    name: String,
    containing_file: String,
    mode: ResolutionMode,
    /// Whether this block's `Resolving in ... mode` line has been read.
    ///
    /// Needed because the resolved mode may legitimately *be* `None`, so the
    /// value cannot double as "not yet seen".
    mode_seen: bool,
}

/// Recover the request list from a baseline's block headers.
///
/// For `node16`/`nodenext` the mode is read from the block's
/// `Resolving in {CJS,ESM} mode` line, which is itself baselined output — a
/// circularity limited to that one line, and only because a containing file's
/// format is program-level knowledge the resolver is not given.
///
/// The mode is read from the block's `Resolving in {0} mode with conditions {1}.`
/// line — specifically from the **conditions**, not from the `CJS`/`ESM` word.
/// Two measured facts forced that:
///
/// - `bundler` has no ESM mode, so it prints `CJS` whatever mode it was given,
///   while its conditions still differ (`'import'` for an unspecified mode,
///   `'require'` for `CommonJS`). Reading the `CJS` word back cost 14 cases.
/// - Reading nothing at all for bundler cost 21 more, because a bundler
///   compilation genuinely does resolve some files in `CommonJS` mode.
///
/// So one token of one line per resolution is taken from the oracle, standing in
/// for the file-format detection that belongs to the file loader. The other
/// ~2,900 lines of the corpus's traces are judged.
fn parse_requests(baseline: &str, kind: ModuleResolutionKind) -> Vec<Request> {
    let is_bundler = kind == ModuleResolutionKind::Bundler;
    let mut requests: Vec<Request> = Vec::new();
    for line in baseline.lines() {
        if let Some(rest) = line.strip_prefix("======== Resolving module '") {
            if let Some((name, rest)) = rest.split_once("' from '") {
                let containing_file = rest.trim_end_matches("'. ========").to_string();
                requests.push(Request {
                    kind: RequestKind::Module,
                    name: name.to_string(),
                    containing_file,
                    mode: ResolutionMode::None,
                    mode_seen: false,
                });
            }
        } else if let Some(rest) =
            line.strip_prefix("======== Resolving type reference directive '")
        {
            if let Some((name, rest)) = rest.split_once("', containing file '") {
                let containing_file =
                    rest.split_once("', root directory").map_or("", |(file, _)| file).to_string();
                requests.push(Request {
                    kind: RequestKind::TypeReferenceDirective,
                    name: name.to_string(),
                    containing_file,
                    mode: ResolutionMode::None,
                    mode_seen: false,
                });
            }
        } else if let Some(request) = requests.last_mut()
            && !request.mode_seen
            && line.starts_with("Resolving in ")
        {
            request.mode_seen = true;
            request.mode = if line.contains("with conditions 'require'") {
                ResolutionMode::CommonJS
            } else if is_bundler {
                // Bundler's `import` conditions come from an *unspecified* mode,
                // not from ESNext — `GetConditions` promotes it internally.
                ResolutionMode::None
            } else {
                ResolutionMode::ESNext
            };
        }
    }
    requests
}

/// Render traces the way upstream's baseline writer does
/// (`harnessutil.TracerForBaselining.sanitizeTrace`).
///
/// Two rewrites, both stateful across the whole run:
///
/// - The compiler version is replaced with `FakeTSVersion`, so a version bump
///   does not rewrite 35 baselines.
/// - The `package.json` existence cache messages are normalised against a
///   *separate* cache kept by the tracer, so the first mention of a file always
///   reads as a real probe and every later one as a cache hit — regardless of
///   which the resolver actually did. Without this the output depends on the
///   order resolutions happened to run in.
pub(crate) fn sanitize(
    traces: &[Trace],
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> String {
    let mut package_json_cache: HashMap<tsr_path::Path, bool> = HashMap::new();
    let mut output = String::new();
    for trace in traces {
        let line = sanitize_line(
            &trace.text,
            &mut package_json_cache,
            current_directory,
            use_case_sensitive_file_names,
        );
        output.push_str(&line);
        output.push('\n');
    }
    output
}

fn sanitize_line(
    message: &str,
    cache: &mut HashMap<tsr_path::Path, bool>,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> String {
    let version_quoted = format!("'{}'", tsr_module::package_json::TYPESCRIPT_VERSION);
    if message.contains(&version_quoted) {
        return message.replacen(&version_quoted, "'FakeTSVersion'", 1);
    }

    let key = |file: &str| to_path(file, current_directory, use_case_sensitive_file_names);

    if let Some(rest) =
        message.strip_suffix("' does not exist according to earlier cached lookups.")
        && let Some(file) = rest.strip_prefix("File '")
    {
        return match cache.entry(key(file)) {
            Entry::Occupied(_) => message.to_string(),
            Entry::Vacant(slot) => {
                slot.insert(false);
                format!("File '{file}' does not exist.")
            }
        };
    }
    if let Some(rest) = message.strip_suffix("' exists according to earlier cached lookups.")
        && let Some(file) = rest.strip_prefix("File '")
    {
        return match cache.entry(key(file)) {
            Entry::Occupied(_) => message.to_string(),
            Entry::Vacant(slot) => {
                slot.insert(true);
                format!("Found 'package.json' at '{file}'.")
            }
        };
    }
    if let Some(rest) = message.strip_suffix("' does not exist.")
        && let Some(file) = rest.strip_prefix("File '")
    {
        return match cache.entry(key(file)) {
            Entry::Occupied(_) => {
                format!("File '{file}' does not exist according to earlier cached lookups.")
            }
            Entry::Vacant(slot) => {
                slot.insert(false);
                message.to_string()
            }
        };
    }
    if let Some(rest) = message.strip_prefix("Found 'package.json' at '") {
        let file = rest.trim_end_matches("'.");
        return match cache.entry(key(file)) {
            Entry::Occupied(_) => {
                format!("File '{file}' exists according to earlier cached lookups.")
            }
            Entry::Vacant(slot) => {
                slot.insert(true);
                message.to_string()
            }
        };
    }
    message.to_string()
}

/// The first line where the two traces diverge, with a little context.
///
/// A whole-trace diff would be unreadable in a committed snapshot; the first
/// divergence is where the bug is.
pub(crate) fn first_difference(expected: &str, actual: &str) -> String {
    let expected_lines: Vec<&str> = expected.lines().collect();
    let actual_lines: Vec<&str> = actual.lines().collect();
    for (index, (expected_line, actual_line)) in
        expected_lines.iter().zip(&actual_lines).enumerate()
    {
        if expected_line != actual_line {
            return format!("line {}: expected {expected_line:?}, got {actual_line:?}", index + 1);
        }
    }
    format!(
        "traces agree for {} lines; expected {} lines, got {}",
        expected_lines.len().min(actual_lines.len()),
        expected_lines.len(),
        actual_lines.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Corpus, repo_root};

    #[test]
    fn requests_are_recovered_from_block_headers() {
        let baseline = "\
======== Resolving module './a' from '/src/b.ts'. ========
Resolving in CJS mode with conditions 'require', 'types', 'node'.
File '/src/a.ts' exists - use it as a name resolution result.
======== Module name './a' was successfully resolved to '/src/a.ts'. ========
======== Resolving type reference directive 'node', containing file '/src/b.ts', root directory '/src/node_modules/@types'. ========
======== Type reference directive 'node' was not resolved. ========
";
        let requests = parse_requests(baseline, ModuleResolutionKind::Node16);
        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].name, "./a");
        assert_eq!(requests[0].containing_file, "/src/b.ts");
        assert_eq!(requests[0].mode, ResolutionMode::CommonJS);
        assert_eq!(requests[1].kind, RequestKind::TypeReferenceDirective);
        assert_eq!(requests[1].name, "node");
        assert_eq!(requests[1].containing_file, "/src/b.ts");

        // Under bundler the `CJS` word carries no information; the conditions
        // do. `'require'` still means CommonJS...
        let bundler = parse_requests(baseline, ModuleResolutionKind::Bundler);
        assert_eq!(bundler[0].mode, ResolutionMode::CommonJS);
        // ...but `'import'` means *unspecified*, which is what `GetConditions`
        // promotes to ESM for bundler. Passing ESNext would be indistinguishable
        // here and wrong for `node16`.
        let import_conditions = baseline.replace(
            "with conditions 'require', 'types', 'node'",
            "with conditions 'import', 'types'",
        );
        assert_eq!(
            parse_requests(&import_conditions, ModuleResolutionKind::Bundler)[0].mode,
            ResolutionMode::None
        );
    }

    #[test]
    fn the_sanitiser_rewrites_the_first_mention_and_every_later_one() {
        // This is the whole point of upstream's tracer: whether a given probe was
        // a real lookup or a cache hit depends on evaluation order, so the
        // baseline is normalised so that the *first* mention always reads as a
        // probe.
        let traces = |texts: &[&str]| {
            texts
                .iter()
                .map(|text| Trace { code: 0, text: (*text).to_string() })
                .collect::<Vec<_>>()
        };
        let out = sanitize(
            &traces(&[
                "File '/a/package.json' does not exist.",
                "File '/a/package.json' does not exist.",
                "File '/b/package.json' does not exist according to earlier cached lookups.",
            ]),
            "/",
            true,
        );
        assert_eq!(
            out,
            "File '/a/package.json' does not exist.\n\
             File '/a/package.json' does not exist according to earlier cached lookups.\n\
             File '/b/package.json' does not exist.\n"
        );
    }

    #[test]
    fn the_compiler_version_is_masked() {
        let traces = vec![Trace {
            code: 6208,
            text: "'package.json' has a 'typesVersions' entry '>=3.1.0-0' that matches compiler \
                   version '7.1.0-dev', looking for a pattern to match module name 'a'."
                .to_string(),
        }];
        assert!(sanitize(&traces, "/", true).contains("'FakeTSVersion'"));
    }

    #[test]
    fn the_skip_rule_explains_every_missing_baseline() {
        // Trap this project has been caught by before: a bucket of cases with no
        // baseline read as evidence about the compiler when it was evidence
        // about the harness. The claim is precise — upstream's own skip
        // predicate, run on the *merged* options, accounts for every
        // `@traceResolution` case with no `.trace.json` — and it is asserted
        // rather than assumed.
        let corpus = Corpus::from_repo_root(&repo_root());
        if !corpus.is_available() {
            return; // Submodules not initialised; nothing to check.
        }
        let mut skipped_by_option: Vec<String> = Vec::new();
        let mut configuration_varied: Vec<String> = Vec::new();
        let mut empty_trace: Vec<String> = Vec::new();
        for case in corpus.discover().expect("discovering cases") {
            let Ok(parsed) = case.load() else { continue };
            if !parsed.options.contains_key("traceresolution") {
                continue;
            }
            if case.baselines.has_exact(case.stem(), "trace.json") {
                continue;
            }
            match crate::trace_case::prepare(&case) {
                crate::trace_case::Setup::Skip(reason) if reason.contains("upstream skips") => {
                    skipped_by_option.push(case.name.clone());
                }
                crate::trace_case::Setup::Skip(reason)
                    if reason.contains("configuration-varied") =>
                {
                    configuration_varied.push(case.name.clone());
                }
                // What is left ran and traced *nothing*: `baseline.Run` removes
                // the reference file when the content is `NoContent`, so absence
                // here means no resolution happened.
                _ => empty_trace.push(case.name.clone()),
            }
        }
        // Every `@traceResolution` case with no plain baseline falls into
        // exactly one of three named buckets, and all three are pinned. A case
        // moving between them, appearing, or disappearing fails this test rather
        // than silently changing a denominator.
        //
        // The `tsconfig-configured` bucket that used to sit here is gone:
        // `tsr-tsoptions` reads those configs, so their options are visible to
        // the predicate. That is why `skipped_by_option` grew from 23 to 41 —
        // a `baseUrl`, an `outFile` or a `moduleResolution: node` written in a
        // config is now seen — and why the empty-trace bucket grew from 3 to 5.
        assert_eq!(
            (skipped_by_option.len(), configuration_varied.len(), empty_trace.len()),
            (41, 14, 5),
            "the no-baseline buckets changed\n\nskipped by option: \
             {skipped_by_option:#?}\n\nconfiguration-varied: \
             {configuration_varied:#?}\n\nempty trace: {empty_trace:#?}"
        );
        // The empty-trace bucket is the one that would silently absorb a real
        // regression, so it is also pinned by name. `file_loader` asserts these
        // three request nothing, rather than skipping them.
        assert_eq!(
            empty_trace,
            [
                "compiler/jsdocInTypeScript",
                "compiler/moduleResolutionWithRequire",
                // A `paths` config over a single file that imports nothing.
                "compiler/pathMappingBasedModuleResolution1_node",
                "conformance/globalAugmentationModuleResolution",
                // An `@types` package whose `package.json` says
                // `"typings": null`, which auto-discovery must not include.
                "conformance/typingsLookup2",
            ],
            "cases upstream ran with an empty trace"
        );
    }
}

#[cfg(test)]
mod pragma_tests {
    use tsr_parser::parse_file_references;

    use crate::{Corpus, repo_root};

    /// Run the preamble scanner over every unit of every corpus case.
    ///
    /// Not a pass rate — the gate for `///`-directives is the file loader, which
    /// asks whether the *right* references were found. This is the cheaper
    /// question underneath it: does the scanner survive 12,444 real files, none
    /// of which it was written against, and does it keep finding the same number
    /// of directives.
    ///
    /// The counts are pinned because they are the only thing standing between a
    /// silently-broken scanner and a loader that quietly loads fewer files.
    ///
    /// They were checked against a raw grep of the corpus rather than merely
    /// recorded: grep finds 411 `path`, 74 `types`, 11 `lib`; the parser finds
    /// 405, 71, 11. Every one of the six differences was inspected and is the
    /// parser being right —
    ///
    /// - `compiler/doNotemitTripleSlashComments` and
    ///   `compiler/emitTopOfFileTripleSlashCommentOnNotEmittedNodeIfRemoveCommentsIsFalse`
    ///   write references *after* real code, where they are ordinary comments
    ///   and add no files to the program.
    /// - `compiler/tripleSlashInCommentNotParsed` puts one inside a `/* */`
    ///   block comment, where `<reference` is not a pragma at all. That case
    ///   exists to assert exactly this.
    #[test]
    fn the_preamble_scanner_survives_the_corpus_and_finds_a_stable_count() {
        let corpus = Corpus::from_repo_root(&repo_root());
        if !corpus.is_available() {
            return; // Submodules not initialised.
        }

        let (mut paths, mut types, mut libs, mut invalid) = (0_usize, 0, 0, 0);
        for case in corpus.discover().expect("discovering cases") {
            let Ok(parsed) = case.load() else { continue };
            for file in &parsed.files {
                let references = parse_file_references(&file.content);
                paths += references.referenced_files.len();
                types += references.type_reference_directives.len();
                libs += references.lib_reference_directives.len();
                invalid += references.invalid_reference_directives.len();
            }
        }

        assert_eq!(
            (paths, types, libs, invalid),
            (405, 71, 11, 1),
            "preamble directive counts moved: (path, types, lib, invalid)"
        );
    }
}
