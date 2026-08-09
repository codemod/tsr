//! Setting up a `@traceResolution` case, for both suites that judge one.
//!
//! `module_resolution` and `file_loader` judge two halves of the same 146
//! `.trace.json` baselines ([ADR-0018](../../../docs/adr/0018-splitting-the-resolution-oracle.md),
//! [ADR-0019](../../../docs/adr/0019-the-loader-gate-discharges-the-mode-circularity.md)),
//! and everything *before* the judging is common: which cases upstream runs,
//! what options they run under, what the file system looks like, and which units
//! are root files.
//!
//! Keeping that in one place is not tidiness. The two suites' denominators are
//! meant to differ by exactly one documented step, and two copies of the setup
//! is how they would quietly stop differing by only that.
//!
//! Ported from `internal/testrunner/compiler_runner.go` (`newCompilerTest`),
//! `internal/testrunner/test_case_parser.go` (the `tsconfig.json` branch), and
//! `internal/testutil/harnessutil` (`CompileFilesEx`,
//! `SetOptionsFromTestConfig`, `SkipUnsupportedCompilerOptions`).

use tsr_core::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, ScriptTarget, Tristate,
};
use tsr_module::types::ResolutionHost;
use tsr_path::{extension::file_extension_is, get_normalized_absolute_path};
use tsr_vfs::{FileSystem, InMemoryFileSystem};

use crate::{CaseEntry, TestCase, case::TestFile};

/// Where a case's files live when it does not say otherwise
/// (`testrunner.srcFolder`).
pub const SRC_FOLDER: &str = "/.src";

/// A `@traceResolution` case, ready to run.
pub struct TraceCase {
    /// The options the compilation runs under.
    pub options: CompilerOptions,
    /// The directory relative names resolve against.
    pub current_directory: String,
    /// Whether the host distinguishes `A.ts` from `a.ts`.
    pub use_case_sensitive_file_names: bool,
    /// The file system the *compiler* sees.
    pub host: TestHost,
    /// The program's root files, in the order upstream passes them.
    pub root_file_names: Vec<String>,
    /// The committed trace, or empty when upstream ran the case and traced
    /// nothing.
    pub expected: String,
}

/// A compiler case after virtual config files, test directives, and root-file
/// selection have been applied.
///
/// This is shared by every suite that needs to judge the program TypeScript
/// actually builds, rather than every fixture unit written in the source case.
pub struct PreparedCompilation {
    /// The merged compiler options.
    pub options: CompilerOptions,
    /// The directory relative names resolve against.
    pub current_directory: String,
    /// Whether the host distinguishes `A.ts` from `a.ts`.
    pub use_case_sensitive_file_names: bool,
    /// The file system the compiler sees.
    pub host: TestHost,
    /// The program's root files, in upstream order.
    pub root_file_names: Vec<String>,
}

/// Either a compilation to run or a reason upstream does not run it.
pub enum CompilationSetup {
    /// Run it.
    Ready(Box<PreparedCompilation>),
    /// Leave it out of the denominator.
    Skip(String),
}

/// Either a case to judge or a reason not to.
pub enum Setup {
    /// Run it.
    Ready(Box<TraceCase>),
    /// Leave it out of the denominator, with a reason the snapshot prints.
    Skip(String),
}

/// A `ResolutionHost` over an in-memory file system.
pub struct TestHost {
    /// The files.
    pub fs: InMemoryFileSystem,
    /// The directory relative names resolve against.
    pub current_directory: String,
}

impl ResolutionHost for TestHost {
    fn fs(&self) -> &dyn FileSystem {
        &self.fs
    }

    fn current_directory(&self) -> &str {
        &self.current_directory
    }
}

/// Prepare a case, or say why it is not judged.
///
/// The skip reasons are the same for both suites except where a suite is
/// genuinely less capable than the other; those belong to the suite, not here.
#[must_use]
pub fn prepare(case: &CaseEntry) -> Setup {
    let Ok(parsed) = case.load() else {
        return Setup::Skip("case could not be read".to_string());
    };
    if !parsed.options.contains_key("traceresolution") {
        return Setup::Skip(
            "case does not set @traceResolution, so upstream records no trace".to_string(),
        );
    }

    let prepared = match prepare_loaded_compilation(&parsed) {
        CompilationSetup::Ready(prepared) => prepared,
        CompilationSetup::Skip(reason) => return Setup::Skip(reason),
    };

    let expected = if let Ok(expected) = std::fs::read_to_string(case.baseline_path("trace.json")) {
        expected
    } else {
        if case.baselines.has_variant(case.stem(), "trace.json") {
            return Setup::Skip("configuration-varied trace baselines (bd tsr-bb4.1)".to_string());
        }
        // Not skipped and no baseline means upstream ran the case and traced
        // *nothing*: `baseline.Run` deletes the reference file when the content
        // is `NoContent`.
        String::new()
    };

    Setup::Ready(Box::new(TraceCase {
        host: prepared.host,
        options: prepared.options,
        current_directory: prepared.current_directory,
        use_case_sensitive_file_names: prepared.use_case_sensitive_file_names,
        root_file_names: prepared.root_file_names,
        expected,
    }))
}

/// Prepare the program roots, host, and merged options for any compiler case.
#[must_use]
pub fn prepare_compilation(case: &CaseEntry) -> CompilationSetup {
    let Ok(parsed) = case.load() else {
        return CompilationSetup::Skip("case could not be read".to_string());
    };
    prepare_loaded_compilation(&parsed)
}

fn prepare_loaded_compilation(parsed: &TestCase) -> CompilationSetup {
    let current_directory = parsed.current_directory.as_deref().map_or_else(
        || SRC_FOLDER.to_string(),
        |dir| get_normalized_absolute_path(dir, SRC_FOLDER),
    );
    let use_case_sensitive_file_names = parsed
        .options
        .get("usecasesensitivefilenames")
        .is_none_or(|value| !value.eq_ignore_ascii_case("false"));
    let Compilation { options, units, root_file_names } =
        compilation(parsed, &current_directory, use_case_sensitive_file_names);

    // Upstream's own skip predicate must see config options and directives
    // after they have been merged.
    if let Some(reason) = upstream_skip_reason(&options) {
        return CompilationSetup::Skip(format!("upstream skips this case: {reason}"));
    }

    CompilationSetup::Ready(Box::new(PreparedCompilation {
        host: TestHost {
            fs: file_system(&units, parsed, &current_directory, use_case_sensitive_file_names),
            current_directory: current_directory.clone(),
        },
        options,
        current_directory,
        use_case_sensitive_file_names,
        root_file_names,
    }))
}

/// What the harness decided to compile.
struct Compilation {
    options: CompilerOptions,
    /// The case's units, minus the `tsconfig.json` one — which upstream deletes
    /// from the list and never puts on the compiler's file system.
    units: Vec<TestFile>,
    root_file_names: Vec<String>,
}

/// Which units are root files, and under what options
/// (`newCompilerTest` plus `CompileFilesEx`'s `programFileNames`).
fn compilation(
    case: &TestCase,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> Compilation {
    let config_index =
        case.files.iter().position(|file| config_name_from_file_name(&file.name).is_some());

    let Some(config_index) = config_index else {
        let options = apply_test_directives(CompilerOptions::default(), case, current_directory);
        let units = case.files.clone();
        let root_file_names = root_files_without_a_config(case, &units, current_directory);
        return Compilation { options, units, root_file_names };
    };

    // A tsconfig unit is parsed against a file system holding *every* unit,
    // including itself, and is then removed from the compilation.
    let config = &case.files[config_index];
    let config_file_name = get_normalized_absolute_path(&config.name, current_directory);
    let config_fs =
        build_file_system(&case.files, case, current_directory, use_case_sensitive_file_names);
    let parsed_config = tsr_tsoptions::parse_config_file(
        &config_file_name,
        &config.content,
        tsr_path::get_directory_path(&config_file_name),
        &config_fs,
    );

    let mut units = case.files.clone();
    units.remove(config_index);

    // Root files are the *intersection* of the config's file list with the
    // case's units, in unit order — a `files` entry naming something the case
    // does not declare is simply absent.
    let root_file_names = units
        .iter()
        .map(|unit| get_normalized_absolute_path(&unit.name, current_directory))
        .filter(|name| parsed_config.file_names.contains(name))
        .filter(|name| !is_excluded_root_extension(name))
        .collect();

    // Directives are applied *over* the config, as `SetOptionsFromTestConfig`
    // does to the cloned config options.
    let options = apply_test_directives(parsed_config.compiler_options, case, current_directory);
    Compilation { options, units, root_file_names }
}

/// Root files for a case with no `tsconfig.json`.
///
/// Upstream's heuristic on the *last* unit: if it uses `require(` or a
/// `/// <reference path`, the case is assumed to pull the rest in by reference,
/// so only that unit is a root and the others merely exist on disk.
fn root_files_without_a_config(
    case: &TestCase,
    units: &[TestFile],
    current_directory: &str,
) -> Vec<String> {
    let Some(last) = units.last() else { return Vec::new() };
    let only_last = case.options.contains_key("noimplicitreferences")
        || last.content.contains("require(")
        || contains_path_reference(&last.content);

    let selected: Vec<&TestFile> = if only_last { vec![last] } else { units.iter().collect() };
    selected
        .into_iter()
        .map(|unit| get_normalized_absolute_path(&unit.name, current_directory))
        .filter(|name| !is_excluded_root_extension(name))
        .collect()
}

/// `.json` and `.tsbuildinfo` units are never root files
/// (`harnessutil.CompileFilesEx`).
fn is_excluded_root_extension(name: &str) -> bool {
    file_extension_is(name, ".json") || file_extension_is(name, ".tsbuildinfo")
}

/// Upstream's `referencesRegex`, which is the literal pattern `reference\spath`.
fn contains_path_reference(content: &str) -> bool {
    content.match_indices("reference").any(|(index, _)| {
        let rest = &content[index + "reference".len()..];
        rest.starts_with(char::is_whitespace) && rest[1..].starts_with("path")
    })
}

/// Which unit is the config (`harnessutil.GetConfigNameFromFileName`).
fn config_name_from_file_name(name: &str) -> Option<&'static str> {
    let base = tsr_path::get_base_file_name(name).to_ascii_lowercase();
    match base.as_str() {
        "tsconfig.json" => Some("tsconfig.json"),
        "jsconfig.json" => Some("jsconfig.json"),
        _ => None,
    }
}

/// Assemble the case's units and symlinks into a file system.
fn file_system(
    units: &[TestFile],
    case: &TestCase,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> InMemoryFileSystem {
    build_file_system(units, case, current_directory, use_case_sensitive_file_names)
}

fn build_file_system(
    units: &[TestFile],
    case: &TestCase,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> InMemoryFileSystem {
    let files = units
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
/// Reimplemented rather than inferred from an absent baseline: an absent file
/// proves nothing on its own, and this project has been caught by that before.
#[must_use]
pub fn upstream_skip_reason(options: &CompilerOptions) -> Option<String> {
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
    if options.allow_synthetic_default_imports.is_false() {
        return Some("allowSyntheticDefaultImports=false is unsupported".to_string());
    }
    if !options.base_url.is_empty() {
        return Some(format!("unsupported baseUrl {}", options.base_url));
    }
    if !options.out_file.is_empty() {
        return Some(format!("unsupported outFile {}", options.out_file));
    }
    if options.target == ScriptTarget::ES5 {
        return Some("unsupported target ES5".to_string());
    }
    if options.always_strict.is_false() {
        return Some("alwaysStrict=false is unsupported".to_string());
    }
    None
}

/// Apply the case's `// @name: value` directives over `base`
/// (`harnessutil.SetOptionsFromTestConfig`).
///
/// Over, not instead of: a tsconfig-configured case may still carry directives,
/// and upstream applies them to the options the config produced.
#[must_use]
pub fn apply_test_directives(
    base: CompilerOptions,
    case: &TestCase,
    current_directory: &str,
) -> CompilerOptions {
    let get = |name: &str| case.options.get(name).map(String::as_str);
    let tristate = |name: &str, base: Tristate| match get(name) {
        Some(value) if value.eq_ignore_ascii_case("true") => Tristate::True,
        Some(value) if value.eq_ignore_ascii_case("false") => Tristate::False,
        _ => base,
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
    let absolute = |name: &str, base: String| {
        get(name).map_or(base, |dir| get_normalized_absolute_path(dir, current_directory))
    };

    CompilerOptions {
        target: get("target").and_then(ScriptTarget::parse).unwrap_or(base.target),
        module: get("module").and_then(ModuleKind::parse).unwrap_or(base.module),
        module_resolution: get("moduleresolution")
            .and_then(parse_module_resolution)
            .unwrap_or(base.module_resolution),
        jsx: get("jsx").and_then(JsxEmit::parse).unwrap_or(base.jsx),
        // §263 — the three names `getJsxNamespace` reads. Their absence here is
        // what §262 could not identify: `jsxFactoryAndJsxFragmentFactory` sets
        // `@jsxFactory: h`, this map dropped it, `Checker::jsx_namespace`
        // defaulted to `React`, and TS2874 reported a name the file never
        // mentions. The same value is read by `jsx_namespace_name`, which
        // §255's TS7026 and §221's `getJsxNamespaceAt` also consult.
        jsx_factory: get("jsxfactory").map_or(base.jsx_factory, str::to_string),
        jsx_fragment_factory: get("jsxfragmentfactory")
            .map_or(base.jsx_fragment_factory, str::to_string),
        react_namespace: get("reactnamespace").map_or(base.react_namespace, str::to_string),
        allow_js: tristate("allowjs", base.allow_js),
        check_js: tristate("checkjs", base.check_js),
        strict: tristate("strict", base.strict),
        no_implicit_any: tristate("noimplicitany", base.no_implicit_any),
        // The rest of what the checker reads. These land in `CompilerOptions`
        // **unresolved** — as the `Tristate` the directive wrote, not as the
        // `bool` the checker wants — because resolving them is
        // `Checker::apply_compiler_options`'s job and there is now exactly one
        // copy of that logic (ADR-0042). Before it, `diagnostics_suite` derived
        // all eleven here from the raw string map, and `types_producer` derived
        // two of them differently.
        strict_null_checks: tristate("strictnullchecks", base.strict_null_checks),
        strict_property_initialization: tristate(
            "strictpropertyinitialization",
            base.strict_property_initialization,
        ),
        use_unknown_in_catch_variables: tristate(
            "useunknownincatchvariables",
            base.use_unknown_in_catch_variables,
        ),
        no_unchecked_indexed_access: tristate(
            "nouncheckedindexedaccess",
            base.no_unchecked_indexed_access,
        ),
        exact_optional_property_types: tristate(
            "exactoptionalpropertytypes",
            base.exact_optional_property_types,
        ),
        no_unused_locals: tristate("nounusedlocals", base.no_unused_locals),
        no_unused_parameters: tristate("nounusedparameters", base.no_unused_parameters),
        allow_unreachable_code: tristate("allowunreachablecode", base.allow_unreachable_code),
        preserve_const_enums: tristate("preserveconstenums", base.preserve_const_enums),
        isolated_modules: tristate("isolatedmodules", base.isolated_modules),
        verbatim_module_syntax: tristate("verbatimmodulesyntax", base.verbatim_module_syntax),
        no_unchecked_side_effect_imports: tristate(
            "nouncheckedsideeffectimports",
            base.no_unchecked_side_effect_imports,
        ),
        declaration: tristate("declaration", base.declaration),
        es_module_interop: tristate("esmoduleinterop", base.es_module_interop),
        allow_synthetic_default_imports: tristate(
            "allowsyntheticdefaultimports",
            base.allow_synthetic_default_imports,
        ),
        always_strict: tristate("alwaysstrict", base.always_strict),
        lib_replacement: tristate("libreplacement", base.lib_replacement),
        allow_arbitrary_extensions: tristate(
            "allowarbitraryextensions",
            base.allow_arbitrary_extensions,
        ),
        allow_non_ts_extensions: tristate("allownontsextensions", base.allow_non_ts_extensions),
        trace_resolution: tristate("traceresolution", base.trace_resolution),
        no_resolve: tristate("noresolve", base.no_resolve),
        base_url: absolute("baseurl", base.base_url),
        out_file: absolute("outfile", base.out_file),
        type_roots: absolute_list("typeroots").or(base.type_roots),
        types: list("types").or(base.types),
        root_dirs: absolute_list("rootdirs").unwrap_or(base.root_dirs),
        root_dir: absolute("rootdir", base.root_dir),
        module_suffixes: list("modulesuffixes").unwrap_or(base.module_suffixes),
        custom_conditions: list("customconditions").unwrap_or(base.custom_conditions),
        resolve_json_module: tristate("resolvejsonmodule", base.resolve_json_module),
        no_dts_resolution: tristate("nodtsresolution", base.no_dts_resolution),
        resolve_package_json_exports: tristate(
            "resolvepackagejsonexports",
            base.resolve_package_json_exports,
        ),
        resolve_package_json_imports: tristate(
            "resolvepackagejsonimports",
            base.resolve_package_json_imports,
        ),
        preserve_symlinks: tristate("preservesymlinks", base.preserve_symlinks),
        out_dir: absolute("outdir", base.out_dir),
        declaration_dir: absolute("declarationdir", base.declaration_dir),
        jsx_import_source: get("jsximportsource").map_or(base.jsx_import_source, str::to_string),
        max_node_module_js_depth: get("maxnodemodulejsdepth")
            .and_then(|value| value.parse().ok())
            .or(base.max_node_module_js_depth),
        // **`lib` and `noLib` are settable by a directive, and this file used to
        // say they were not.** Upstream's harness applies *every* directive whose
        // name matches a compiler-option declaration — `SetOptionsFromTestConfig`
        // (`internal/testutil/harnessutil/harnessutil.go:266`) looks the name up
        // with `getCommandLineOption` (`:1151`) against
        // `tsoptions.OptionsDeclarations` and calls `ParseCompilerOptions`. `lib`
        // and `noLib` are both in that table, so both are honoured upstream.
        //
        // 925 corpus cases carry `@lib` and 92 carry `@noLib`, so this is not a
        // long-tail fidelity point. The machinery to honour them already existed:
        // `tsr_tsoptions::libs::lib_file_names` reads exactly these two fields.
        //
        // **Measured on the corpus, `26efa2a` against this commit, per case with
        // `examples/casedelta.rs`: +431 lines and +2 cases, over 24 cases that
        // moved at all — 22 gained 433, two lost one line each.** Both halves of
        // that are worth keeping, because both contradict something that was
        // asserted before it was measured:
        //
        // - **`bd tsr-cug` sized `compiler/temporal` at 1,784 lines. It gained
        //   362** — the row was a ceiling and the real conversion is 4.9× smaller,
        //   because a name resolving is necessary and not sufficient for the line
        //   to match. This is the rule `docs/conventions.md` states as *a row
        //   population is a ceiling on the row, never a conversion*, and it is
        //   the fifth row here to collapse on contact with a measurement.
        // - **The both-directions risk is real in principle and negligible in
        //   fact.** `@lib: es5` and `@noLib` ask for *fewer* libs than the target
        //   default, so this port had been resolving names upstream cannot and
        //   answering a confident type where upstream answers `any` — **wrong**
        //   lines, not gaps, and invisible in any histogram of what we failed to
        //   compute. That was the argument for expecting a large negative
        //   component. It costs **2 lines, in 2 cases**. The reasoning was sound
        //   and the magnitude was guesswork; only the split run settled it.
        //
        // Splitting the two directives across separate corpus runs: **`lib`
        // carries the whole +431 and `noLib` moves one line** (a `-1` in
        // `compiler/decoratorMetadataNoLibIsolatedModulesTypes`). `noLib` is kept
        // anyway — it is upstream's behaviour and it is the direction that
        // prevents *wrong* answers rather than the one that converts gaps — but
        // it must not be cited as having converted anything.
        //
        // An unknown lib entry is skipped rather than substituted, which is
        // `get_lib_file_name`'s documented behaviour and upstream's.
        lib: list("lib").unwrap_or(base.lib),
        no_lib: tristate("nolib", base.no_lib),
        // Not settable by a directive: these come from the config or nowhere.
        paths: base.paths,
        paths_base_path: base.paths_base_path,
        config_file_path: base.config_file_path,
        // **`isolatedModules` is now honoured** — it is a directive upstream (80
        // corpus cases) and was dropped here, filed as `bd tsr-e7a`, on the
        // grounds that it belonged to the declaration-emit suites rather than to
        // `checker_types`. That was only ever half true: `diagnostics_suite` read
        // it out of the raw directive map itself, because
        // `ShouldPreserveConstEnums` folds it in. Routing every checker option
        // through `CompilerOptions` (ADR-0042) left nowhere for that private read
        // to live, so the directive is applied above with the rest.
        //
        // Everything else keeps `base`'s value. That is a change of shape: this
        // literal used to name every field, so adding one to `CompilerOptions`
        // broke the build here and forced a decision about whether the corpus
        // sets it. It stopped being a useful forcing function once the
        // command-line surface arrived — the driver's twenty-odd options
        // (`--help`, `--pretty`, `--showConfig`) are not test directives and
        // never will be, so the build break was pure noise. `bd`-less note: the
        // directives this harness *does* honour are the ones listed above, and
        // `casedelta.rs` is what would show a missing one.
        ..base
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

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use tsr_core::{CompilerOptions, ScriptTarget, Tristate};

    use super::*;
    use crate::TestCase;

    /// A case carrying nothing but the directives under test.
    ///
    /// `options` is what the case parser produces: names already lowercased and
    /// values already trimmed (`case.rs`), which is why these keys are lowercase
    /// and the assertions do not re-test that normalisation.
    fn case_with(options: &[(&str, &str)]) -> TestCase {
        TestCase {
            name: "test/directives".to_string(),
            files: Vec::new(),
            options: options
                .iter()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect::<BTreeMap<_, _>>(),
            symlinks: BTreeMap::new(),
            current_directory: None,
            error: None,
        }
    }

    fn applied(options: &[(&str, &str)]) -> CompilerOptions {
        apply_test_directives(CompilerOptions::default(), &case_with(options), "/")
    }

    /// `@lib` reaches the loader, and reaches it as *files*.
    ///
    /// Asserting on `lib_file_names` rather than on the `lib` field is the point:
    /// carrying the strings through and having them select nothing would satisfy
    /// a field-equality test and still load no `lib.esnext.temporal.d.ts`, which
    /// is the failure that was actually happening.
    ///
    /// Red under **M1** — restore `lib: base.lib`. Then `lib` is empty, the
    /// default-for-target file is loaded instead, and neither `contains` holds.
    #[test]
    fn a_lib_directive_selects_the_files_it_names() {
        let options = applied(&[("lib", "esnext,esnext.temporal,dom")]);
        assert_eq!(options.lib, ["esnext", "esnext.temporal", "dom"], "the list reaches options");

        let files = tsr_tsoptions::libs::lib_file_names(&options);
        assert!(
            files.contains(&"lib.esnext.temporal.d.ts"),
            "the lib compiler/temporal asks for must be selected, got {files:?}"
        );
        assert!(files.contains(&"lib.dom.d.ts"), "got {files:?}");
        assert!(
            !files.contains(&CompilerOptions::default().default_lib_file_name()),
            "an explicit lib list replaces the target default rather than adding to it: {files:?}"
        );
    }

    /// The direction that produces **wrong** lines rather than gaps.
    ///
    /// `@noLib` asks for fewer libs than the default, so dropping it makes this
    /// port resolve names upstream cannot and answer a confident type where
    /// upstream answers `any`.
    ///
    /// Red under **M2** — restore `no_lib: base.no_lib`. `no_lib` stays
    /// `Unknown`, `lib_file_names` returns the target default, and the vector is
    /// not empty.
    #[test]
    fn a_nolib_directive_loads_no_lib_at_all() {
        let options = applied(&[("nolib", "true")]);
        assert_eq!(options.no_lib, Tristate::True);
        assert!(
            tsr_tsoptions::libs::lib_file_names(&options).is_empty(),
            "noLib must load nothing"
        );

        // Pinned by construction rather than by arithmetic: with no directive at
        // all there is nothing for either arm to read, so this case cannot depend
        // on the change under test and must keep loading the target default.
        let untouched = applied(&[("target", "es5")]);
        assert_eq!(untouched.target, ScriptTarget::ES5);
        assert!(untouched.lib.is_empty());
        assert_eq!(untouched.no_lib, Tristate::Unknown);
        assert_eq!(
            tsr_tsoptions::libs::lib_file_names(&untouched),
            vec![untouched.default_lib_file_name()],
            "a case with no lib directive is unaffected"
        );
    }

    #[test]
    fn virtual_config_controls_javascript_roots_and_options() {
        let case = TestCase {
            name: "compiler/configured-js".to_string(),
            files: vec![
                TestFile {
                    name: "tsconfig.json".to_string(),
                    content: r#"{"compilerOptions":{"allowJs":true,"jsx":"preserve"},"files":["index.js"]}"#
                        .to_string(),
                },
                TestFile {
                    name: "index.js".to_string(),
                    content: "export const view = <div />;".to_string(),
                },
                TestFile {
                    name: "not-a-root.js".to_string(),
                    content: "not read".to_string(),
                },
            ],
            options: BTreeMap::new(),
            symlinks: BTreeMap::new(),
            current_directory: None,
            error: None,
        };

        let compilation = compilation(&case, SRC_FOLDER, true);
        assert_eq!(compilation.options.allow_js, Tristate::True);
        assert_eq!(compilation.options.jsx, JsxEmit::Preserve);
        assert_eq!(compilation.root_file_names, ["/.src/index.js"]);
    }

    /// The three JSX factory directives reach `CompilerOptions`.
    ///
    /// **This is a regression test for a silent-default defect, not for a
    /// parser.** `jsxFactoryAndJsxFragmentFactory` sets `@jsxFactory: h`; this
    /// function dropped it, `Checker::apply_compiler_options` took its `React`
    /// default, and TS2874 reported a name the file never mentions — 36 wrong
    /// lines and 4 lost cases, with the *rule* correct throughout. See
    /// `docs/architecture/checker-notes-diag2.md` §262 and §263.
    ///
    /// The failure mode is what makes this worth pinning: a dropped directive
    /// does not error, it substitutes a **plausible** default, and every
    /// diagnostic downstream is confidently wrong about a value nobody typed.
    ///
    /// Red under M1 — restore any of the three to `base.…`.
    #[test]
    fn the_jsx_factory_directives_reach_the_options() {
        let options = applied(&[
            ("jsx", "react"),
            ("jsxfactory", "h"),
            ("jsxfragmentfactory", "Frag"),
            ("reactnamespace", "Preact"),
        ]);
        assert_eq!(options.jsx_factory, "h");
        assert_eq!(options.jsx_fragment_factory, "Frag");
        assert_eq!(options.react_namespace, "Preact");
    }

    /// A case that sets none of them keeps the defaults, so the test above is
    /// asserting the *directive* rather than a constant.
    #[test]
    fn no_jsx_directives_leaves_the_factories_empty() {
        let options = applied(&[("jsx", "react")]);
        assert!(options.jsx_factory.is_empty());
        assert!(options.jsx_fragment_factory.is_empty());
        assert!(options.react_namespace.is_empty());
    }
}
