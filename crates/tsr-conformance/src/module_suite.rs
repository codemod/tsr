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
//! wrong order — there is no loader yet (bd tsr-9or), and building one to get a
//! number would have entangled the two failure modes so that neither could be
//! localised. When the loader lands it gets its own gate: the *headers*, in
//! order, from the same baselines.
//!
//! # The skip rules, and why they are not silent
//!
//! 155 corpus cases set `@traceResolution`; only 109 have a baseline. The gap is
//! not missing data — **typescript-go skips those cases itself**, through
//! `harnessutil.SkipUnsupportedCompilerOptions`, because it has not ported
//! `node10`/`classic` resolution, AMD/UMD/System modules, `baseUrl`, `outFile`,
//! or an ES5 target. [`upstream_skip_reason`] reimplements that predicate, and
//! [`the_skip_rule_explains_every_missing_baseline`] asserts it accounts for the
//! gap exactly. If upstream ever unskips a case, that test fails rather than the
//! case silently vanishing from the denominator.

use std::collections::{HashMap, hash_map::Entry};

use tsr_core::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, ResolutionMode, ScriptTarget,
    Tristate,
};
use tsr_module::{
    messages::Trace,
    resolver::Resolver,
    types::{ResolutionHost, ResolvedModule},
};
use tsr_path::{get_normalized_absolute_path, to_path};
use tsr_vfs::{FileSystem, InMemoryFileSystem};

use crate::{
    CaseEntry, TestCase,
    suite::{Outcome, Suite},
};

/// Where a case's files live when it does not say otherwise
/// (`testrunner.srcFolder`).
const SRC_FOLDER: &str = "/.src";

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
        let Ok(parsed) = case.load() else {
            return Outcome::Skipped { reason: "case could not be read".to_string() };
        };
        if !parsed.options.contains_key("traceresolution") {
            return Outcome::Skipped {
                reason: "case does not set @traceResolution, so upstream records no trace"
                    .to_string(),
            };
        }

        // Checked before anything that reasons about options: a case that
        // configures itself with a tsconfig.json has options this harness cannot
        // see at all, so neither its expected behaviour nor upstream's skip
        // predicate can be evaluated for it.
        if parsed.files.iter().any(|file| file.name.ends_with("tsconfig.json")) {
            return Outcome::Skipped {
                reason: "case configures itself with a tsconfig.json, which needs tsr-tsoptions \
                         (bd tsr-9or slice 3)"
                    .to_string(),
            };
        }

        let baseline_path = case.baseline_path("trace.json");
        let Ok(expected) = std::fs::read_to_string(&baseline_path) else {
            // No baseline. Either upstream skipped the case — in which case say
            // exactly why — or it is configuration-varied, or something has
            // changed upstream and we want to hear about it.
            if let Some(reason) = upstream_skip_reason(&parsed) {
                return Outcome::Skipped { reason: format!("upstream skips this case: {reason}") };
            }
            if case.baselines.has_variant(case.stem(), "trace.json") {
                return Outcome::Skipped {
                    reason: "configuration-varied trace baselines (bd tsr-bb4.1)".to_string(),
                };
            }
            // An unskipped case with no baseline ran and traced *nothing*:
            // `baseline.Run` deletes the reference file when the content is
            // `NoContent`, so absence here means "no resolution happened", not
            // "no data". Three cases are in this bucket, all of which have no
            // import at all. There is nothing for this suite to judge — the
            // claim "we would also have requested nothing" belongs to the file
            // loader, which does not exist yet.
            return Outcome::Skipped {
                reason: "upstream recorded an empty trace: the case performs no resolutions"
                    .to_string(),
            };
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

        let requests = parse_requests(&expected, options.module_resolution_kind());
        if requests.is_empty() {
            return Outcome::Failed {
                reason: "baseline contains no resolution request headers".to_string(),
            };
        }

        let host = TestHost {
            fs: build_file_system(&parsed, &current_directory, use_case_sensitive_file_names),
            current_directory: current_directory.clone(),
        };
        let resolver = Resolver::new(&host, options);

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

        let actual = sanitize(&traces, &current_directory, use_case_sensitive_file_names);
        if actual == expected {
            Outcome::Passed
        } else {
            Outcome::Failed { reason: first_difference(&expected, &actual) }
        }
    }
}

/// A `ResolutionHost` over an in-memory file system.
struct TestHost {
    fs: InMemoryFileSystem,
    current_directory: String,
}

impl ResolutionHost for TestHost {
    fn fs(&self) -> &dyn FileSystem {
        &self.fs
    }

    fn current_directory(&self) -> &str {
        &self.current_directory
    }
}

/// Assemble the case's declared units and symlinks into a file system.
fn build_file_system(
    case: &TestCase,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> InMemoryFileSystem {
    let files = case
        .files
        .iter()
        .map(|file| {
            (get_normalized_absolute_path(&file.name, current_directory), file.content.clone())
        })
        .collect::<Vec<_>>();
    let symlinks = case
        .symlinks
        .iter()
        .map(|(link, target)| {
            (
                get_normalized_absolute_path(link, current_directory),
                get_normalized_absolute_path(target, current_directory),
            )
        })
        .collect::<Vec<_>>();
    InMemoryFileSystem::new(files, symlinks, use_case_sensitive_file_names)
}

/// Whether typescript-go's own harness would skip this case, and why
/// (`harnessutil.SkipUnsupportedCompilerOptions`).
///
/// Reimplemented rather than inferred from the absent baseline: an absent file
/// proves nothing on its own, and this project has been caught by that before.
#[must_use]
pub fn upstream_skip_reason(case: &TestCase) -> Option<String> {
    let options = compiler_options(case, SRC_FOLDER);

    match options.module {
        ModuleKind::AMD | ModuleKind::UMD | ModuleKind::System => {
            return Some(format!("unsupported module kind {:?}", options.module));
        }
        _ => {}
    }
    // The big one: `node10` and `classic` resolution are not ported upstream at
    // all, so every case that *asks* for them is skipped. Note this reads the
    // raw field, as upstream does — the derived kind can never be either, since
    // `GetModuleResolutionKind` falls through to bundler.
    match options.module_resolution {
        ModuleResolutionKind::Node10 | ModuleResolutionKind::Classic => {
            return Some(format!(
                "unsupported module resolution kind {}",
                options.module_resolution
            ));
        }
        _ => {}
    }
    if options.es_module_interop.is_false() {
        return Some("esModuleInterop=false is unsupported".to_string());
    }
    if case.options.get("allowsyntheticdefaultimports").is_some_and(|v| v == "false") {
        return Some("allowSyntheticDefaultImports=false is unsupported".to_string());
    }
    if !options.base_url.is_empty() {
        return Some(format!("unsupported baseUrl {}", options.base_url));
    }
    if case.options.contains_key("outfile") {
        return Some("unsupported outFile".to_string());
    }
    if options.target == ScriptTarget::ES5 {
        return Some("unsupported target ES5".to_string());
    }
    if case.options.get("alwaysstrict").is_some_and(|v| v == "false") {
        return Some("alwaysStrict=false is unsupported".to_string());
    }
    None
}

/// Build options from the case's directives
/// (`harnessutil.SetOptionsFromTestConfig`, for the options resolution reads).
fn compiler_options(case: &TestCase, current_directory: &str) -> CompilerOptions {
    let get = |name: &str| case.options.get(name).map(String::as_str);
    let tristate = |name: &str| match get(name) {
        Some(value) if value.eq_ignore_ascii_case("true") => Tristate::True,
        Some(value) if value.eq_ignore_ascii_case("false") => Tristate::False,
        _ => Tristate::Unknown,
    };
    let list = |name: &str| {
        get(name).map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|part| !part.is_empty())
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
    };
    // `rootDirs` and `typeRoots` are made absolute against the current directory
    // before the resolver ever sees them.
    let absolute_list = |name: &str| {
        list(name).map(|values| {
            values
                .iter()
                .map(|value| get_normalized_absolute_path(value, current_directory))
                .collect::<Vec<_>>()
        })
    };

    CompilerOptions {
        target: get("target").and_then(ScriptTarget::parse).unwrap_or_default(),
        module: get("module").and_then(ModuleKind::parse).unwrap_or_default(),
        module_resolution: get("moduleresolution")
            .and_then(parse_module_resolution)
            .unwrap_or_default(),
        jsx: get("jsx").and_then(JsxEmit::parse).unwrap_or_default(),
        allow_js: tristate("allowjs"),
        check_js: tristate("checkjs"),
        strict: tristate("strict"),
        declaration: tristate("declaration"),
        es_module_interop: tristate("esmoduleinterop"),
        allow_arbitrary_extensions: tristate("allowarbitraryextensions"),
        trace_resolution: tristate("traceresolution"),
        no_resolve: tristate("noresolve"),
        base_url: get("baseurl").unwrap_or_default().to_string(),
        type_roots: absolute_list("typeroots"),
        types: list("types"),
        root_dirs: absolute_list("rootdirs").unwrap_or_default(),
        root_dir: get("rootdir")
            .map(|dir| get_normalized_absolute_path(dir, current_directory))
            .unwrap_or_default(),
        module_suffixes: list("modulesuffixes").unwrap_or_default(),
        custom_conditions: list("customconditions").unwrap_or_default(),
        resolve_json_module: tristate("resolvejsonmodule"),
        no_dts_resolution: tristate("nodtsresolution"),
        resolve_package_json_exports: tristate("resolvepackagejsonexports"),
        resolve_package_json_imports: tristate("resolvepackagejsonimports"),
        preserve_symlinks: tristate("preservesymlinks"),
        out_dir: get("outdir")
            .map(|dir| get_normalized_absolute_path(dir, current_directory))
            .unwrap_or_default(),
        declaration_dir: get("declarationdir")
            .map(|dir| get_normalized_absolute_path(dir, current_directory))
            .unwrap_or_default(),
        ..CompilerOptions::default()
    }
}

fn parse_module_resolution(value: &str) -> Option<ModuleResolutionKind> {
    Some(match value.to_ascii_lowercase().as_str() {
        "classic" => ModuleResolutionKind::Classic,
        "node" | "node10" => ModuleResolutionKind::Node10,
        "node16" => ModuleResolutionKind::Node16,
        "nodenext" => ModuleResolutionKind::NodeNext,
        "bundler" => ModuleResolutionKind::Bundler,
        _ => return None,
    })
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
fn sanitize(
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
fn first_difference(expected: &str, actual: &str) -> String {
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
        // baseline read as evidence about the compiler when it was evidence about
        // the harness. Here the claim is precise — upstream's own skip predicate
        // accounts for every `@traceResolution` case with no `.trace.json` — and
        // it is asserted rather than assumed.
        let corpus = Corpus::from_repo_root(&repo_root());
        if !corpus.is_available() {
            return; // Submodules not initialised; nothing to check.
        }
        let mut skipped_by_option: Vec<String> = Vec::new();
        let mut tsconfig_configured: Vec<String> = Vec::new();
        let mut empty_trace: Vec<String> = Vec::new();
        for case in corpus.discover().expect("discovering cases") {
            let Ok(parsed) = case.load() else { continue };
            if !parsed.options.contains_key("traceresolution") {
                continue;
            }
            if case.baselines.has_any(case.stem(), "trace.json") {
                continue;
            }
            if upstream_skip_reason(&parsed).is_some() {
                skipped_by_option.push(case.name.clone());
                continue;
            }
            // Options this harness cannot see: the case configures itself
            // through a `tsconfig.json` unit.
            if parsed.files.iter().any(|file| file.name.ends_with("tsconfig.json")) {
                tsconfig_configured.push(case.name.clone());
                continue;
            }
            // What is left ran and traced *nothing*: `baseline.Run` removes the
            // reference file when the content is `NoContent`, so absence here
            // means no resolution happened. Listed by name rather than detected
            // by a content heuristic — `import M = N` looks like an import and
            // resolves nothing, so any heuristic would be guessing.
            empty_trace.push(case.name.clone());
        }
        // Every `@traceResolution` case with no baseline falls into exactly one
        // of three named buckets, and all three are pinned. A case moving
        // between them, appearing, or disappearing fails this test rather than
        // silently changing the denominator.
        //
        // 155 cases set `@traceResolution`; 109 have a baseline; 46 do not:
        //   23  upstream skips them by compiler option (mostly `node10`/
        //       `classic` resolution, which is not ported upstream at all)
        //   20  configure themselves through a `tsconfig.json` unit, so their
        //       options are not in their directives and the predicate above
        //       cannot be evaluated for them (bd tsr-9or slice 3)
        //    3  ran and traced nothing: `baseline.Run` removes the reference
        //       file when the content is `NoContent`
        assert_eq!(
            (skipped_by_option.len(), tsconfig_configured.len(), empty_trace.len()),
            (23, 20, 3),
            "the no-baseline buckets changed\n\nskipped by option: \
             {skipped_by_option:#?}\n\ntsconfig-configured: \
             {tsconfig_configured:#?}\n\nempty trace: {empty_trace:#?}"
        );
        assert_eq!(
            skipped_by_option.len() + tsconfig_configured.len() + empty_trace.len(),
            46,
            "every @traceResolution case without a baseline must be accounted for"
        );
        // The empty-trace bucket is the one that would silently absorb a real
        // regression, so it is also pinned by name.
        assert_eq!(
            empty_trace,
            [
                "compiler/jsdocInTypeScript",
                "compiler/moduleResolutionWithRequire",
                "conformance/globalAugmentationModuleResolution",
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
