//! Reading a `tsconfig.json` into compiler options and a root file list.
//!
//! Ported from `internal/tsoptions` at the pinned commit — the part of it that
//! answers the two questions a `Program` asks: *what options*, and *which
//! files*. Upstream's package is 8,706 lines; most of that is the command-line
//! parser, `tsc --init`, `--showConfig`, `--build` mode, watch options, and the
//! declarations for the ~130 options this port does not yet have fields for.
//!
//! # Where this sits
//!
//! ```text
//! tsconfig.json ──▶ tsr-parser (ScriptKind::Json) ──▶ AST
//!                                                      │
//!                       tsr-tsoptions ◀────────────────┘
//!                            │
//!         CompilerOptions ───┴─── root file names ──▶ tsr-compiler's FileLoader
//! ```
//!
//! [ADR-0017](../../../docs/adr/0017-program-before-tsconfig.md) built the
//! `Program` before this deliberately, so `CompilerOptions` could be a plain
//! struct that a config parser *produces* rather than something the parser
//! defines. That ordering is why this crate is additive: nothing below it
//! changed to accommodate it.
//!
//! # What is here and what is not
//!
//! Here: the config file's own options, `files`/`include`/`exclude` expansion
//! against the file system, and the extension-priority rules that keep a
//! project from compiling its own output.
//!
//! Not here, and each is a named gap rather than an oversight:
//!
//! - **`extends` circularity.** `extends` itself is here (relative, package
//!   and array forms; inherited specs rebased as `parseConfig` does, see
//!   `docs/architecture/tsconfig.md`), but a cycle stops at a depth cap instead
//!   of upstream's `Circularity detected while resolving configuration` error.
//! - **Project references.** They belong with the rest of the project-reference
//!   machinery the file loader also does not have.
//! - **The command line.** `tsc` does not exist yet (Phase 6).
//! - **Config diagnostics.** Errors are collected and returned, but no suite
//!   judges them, and the spans are the value's rather than upstream's exact
//!   `CreateDiagnosticForNodeInSourceFile` range.

pub mod command_line;
mod generated;

pub use generated::libs::{LIB_MAP, LIB_NAMES};

pub mod declarations;
pub mod file_names;
pub mod libs;
pub mod syntax;
pub mod value;

use tsr_core::{CompilerOptions, OrderedMap, Tristate};
use tsr_diagnostics::{Diagnostic, messages};
use tsr_parser::{ParseOptions, ScriptKind, parse_with_options};
use tsr_path::{
    get_directory_path, get_normalized_absolute_path, normalize_path, normalize_slashes,
};
use tsr_vfs::FileSystem;

use crate::{
    declarations::OptionDeclaration,
    file_names::ConfigFileSpecs,
    value::{ConfigProperty, ConfigValue},
};

/// The default `include` when a config gives neither `files` nor `include`
/// (`tsoptions.defaultIncludeSpec`).
const DEFAULT_INCLUDE_SPEC: &str = "**/*";

/// The root keys whose specs an extending config inherits, rebased, from its
/// base (`setPropertyValue` in `parseConfig`, `tsconfigparsing.go:1106`).
const INHERITED_SPEC_PROPERTIES: [&str; 3] = ["include", "exclude", "files"];

/// `configDirTemplate` (`tsconfigparsing.go`).
const CONFIG_DIR_TEMPLATE: &str = "${configDir}";

/// A parsed config file (`tsoptions.ParsedCommandLine`).
#[derive(Debug, Default)]
pub struct ParsedCommandLine {
    /// The options the config declares.
    pub compiler_options: CompilerOptions,
    /// The program's root files, absolute, `files` entries first.
    pub file_names: Vec<String>,
    /// How many of [`Self::file_names`] came from `files` rather than a
    /// wildcard.
    pub literal_file_count: usize,
    /// The config as written, for the keys that are not compiler options.
    pub raw: OrderedMap<ConfigValue>,
    /// What was wrong with it.
    pub errors: Vec<Diagnostic>,
    /// The config file each of [`Self::errors`] is positioned in, by index:
    /// an `extends` base's errors are in the base. `None` for an error this
    /// port reports without a position (`ast.NewCompilerDiagnostic`
    /// upstream, or a location it does not record).
    pub error_files: Vec<Option<String>>,
}

/// Parse `text` as the config file at `config_file_name`
/// (`ParseJsonSourceFileConfigFileContent`).
///
/// `base_path` is what a relative path in the config resolves against, which is
/// the config's own directory.
#[must_use]
pub fn parse_config_file(
    config_file_name: &str,
    text: &str,
    base_path: &str,
    fs: &dyn FileSystem,
) -> ParsedCommandLine {
    let config_dir = if config_file_name.is_empty() {
        normalize_slashes(base_path)
    } else {
        get_directory_path(config_file_name).to_string()
    };
    parse_config_file_at_depth(config_file_name, text, base_path, fs, 0, &config_dir)
}

/// How deep `extends` may nest.
///
/// **No upstream counterpart**: upstream tracks a set of already-visited config
/// paths and reports a circularity diagnostic. A depth cap is the cheaper guard
/// with the same safety property — a config that extends itself terminates —
/// and it is set far above any real configuration. The cost is that a genuine
/// cycle reports "file not found" nothing rather than upstream's circularity
/// error; recorded in STATUS-cli.md rather than hidden.
const MAX_EXTENDS_DEPTH: u32 = 32;

fn parse_config_file_at_depth(
    config_file_name: &str,
    text: &str,
    base_path: &str,
    fs: &dyn FileSystem,
    depth: u32,
    config_dir: &str,
) -> ParsedCommandLine {
    let arena = tsr_core::Arena::new();
    // The *parser* reads it, not a JSON library: a `tsconfig.json` may carry
    // comments and trailing commas, and every option error points at a span.
    // See `tsr_parser::json`.
    let parsed = parse_with_options(
        &arena,
        text,
        ParseOptions { script_kind: ScriptKind::Json, ..ParseOptions::default() },
    );

    let base_path_for_file_names = if config_file_name.is_empty() {
        normalize_path(base_path)
    } else {
        normalize_path(&directory_of_combined_path(config_file_name, base_path))
    };

    let mut config = Config::default();
    let properties = value::root_properties(parsed.source_file, &parsed.nodes);
    let root_is_object = match &properties {
        Some(properties) => {
            config.read(properties, &base_path_for_file_names, &parsed.nodes);
            true
        }
        None => false,
    };
    let Config { mut compiler_options, raw, mut errors } = config;

    // `[]`, `"x"`, `42` — anything whose root is not an object. Upstream reports
    // it and carries on with an empty configuration, so the compilation still
    // runs and still reports whatever *else* is wrong; the alternative, bailing
    // here, would hide the no-inputs error that follows and that upstream's
    // baseline expects alongside this one.
    //
    // The empty file is deliberately **not** an error: `tsconfig.json` holding
    // nothing at all is a valid empty configuration, and it reaches this same
    // `None` because there is no root expression to read.
    if !root_is_object && let Some(statement) = parsed.source_file.statements.first() {
        // Positioned over the root value itself, so the frame squiggles `[]`
        // rather than a single column. A zero-length span would squiggle one
        // character, which is what an "expected" diagnostic wants and this is
        // not.
        errors.push(Diagnostic::with_args(
            &messages::THE_ROOT_VALUE_OF_A_0_FILE_MUST_BE_AN_OBJECT,
            {
                let tsr_ast::Statement::ExpressionStatement(statement) = statement else {
                    unreachable!("a JSON root is parsed as an expression statement")
                };
                statement
                    .expression
                    .and_then(|expression| {
                        value::span_of(tsr_ast::Node::from(expression), &parsed.nodes)
                    })
                    .unwrap_or_default()
            },
            ["tsconfig.json".to_string()],
        ));
    }
    // Everything so far is positioned in this file's own text.
    let own_file = (!config_file_name.is_empty()).then(|| normalize_slashes(config_file_name));
    let mut error_files = vec![own_file.clone(); errors.len()];
    // `extends` — resolve, parse the base, and layer this config over it.
    //
    // Ported from `getExtendsConfigPathOrArray` (`tsconfigparsing.go:509`) and
    // `getExtendsConfigPath` (`:558`). The merge is base-first: the extended
    // config supplies defaults and anything written here overrides them, and
    // `files`/`include`/`exclude` are inherited **only** when this config
    // declares none of its own, because those three select the program and a
    // partial merge would silently compile the wrong tree.
    // `${configDir}` names the directory of the config the user *invoked*, not
    // of the file the value was written in — that is the point of it: a shared
    // base can say `"outDir": "${configDir}/build"` and each extending project
    // gets its own. So `config_dir` is threaded down unchanged through every
    // `extends` hop rather than recomputed per file.
    expand_config_dir(&mut compiler_options, config_dir);
    // `paths` substitutions resolve against the directory of the config that
    // *wrote* them (`parseConfig`, `tsconfigparsing.go:1092`): set on the own
    // options before any base is merged, so inherited `paths` keep the base
    // config's directory.
    if !compiler_options.paths.is_empty() {
        compiler_options.paths_base_path.clone_from(&base_path_for_file_names);
    }

    let (extended, extend_errors) = extended_configs(
        properties.as_deref().unwrap_or_default(),
        &parsed.nodes,
        config_file_name,
        &base_path_for_file_names,
        fs,
        depth,
        config_dir,
    );
    for (diagnostic, positioned) in extend_errors {
        errors.push(diagnostic);
        error_files.push(if positioned { own_file.clone() } else { None });
    }
    // Native parseConfig merges bases left-to-right, then the real own fields.
    // Do not treat a previously inherited worker value as an own override.
    let own_workers = (compiler_options.checkers, compiler_options.single_threaded);
    let mut inherited_workers = (None, Tristate::Unknown);
    let mut raw = raw;
    // `parseConfig`'s `extendsResult` (`tsconfigparsing.go:1101-1170`): only
    // `include`, `exclude`, `files` and `compileOnSave` pass from a base's raw
    // config to this one's, and a later `extends` entry overwrites an earlier
    // one's. Every other root key (`references`, a base's own `extends`, ...)
    // stays with the file that wrote it.
    let mut inherited_specs: [Option<ConfigValue>; 3] = [None, None, None];
    let mut inherited_compile_on_save = false;
    let use_case_sensitive_file_names = fs.use_case_sensitive_file_names();
    for (extended_config_path, base) in extended {
        inherited_workers = merge_worker_options(
            inherited_workers,
            (base.compiler_options.checkers, base.compiler_options.single_threaded),
            &base.raw,
        );
        let mut relative_difference = None;
        for (slot, name) in inherited_specs.iter_mut().zip(INHERITED_SPEC_PROPERTIES) {
            if raw.contains_key(name) {
                continue;
            }
            if let Some(ConfigValue::List(specs)) = base.raw.get(name) {
                *slot = Some(ConfigValue::List(rebase_inherited_specs(
                    specs,
                    &extended_config_path,
                    base_path,
                    use_case_sensitive_file_names,
                    &mut relative_difference,
                )));
            }
        }
        if let Some(ConfigValue::Bool(compile_on_save)) = base.raw.get("compileOnSave") {
            inherited_compile_on_save = *compile_on_save;
        }
        compiler_options = merge_options(base.compiler_options, compiler_options);
        errors.splice(0..0, base.errors);
        error_files.splice(0..0, base.error_files);
    }
    for (slot, name) in inherited_specs.into_iter().zip(INHERITED_SPEC_PROPERTIES) {
        if let Some(specs) = slot {
            raw.set(name.to_string(), specs);
        }
    }
    if inherited_compile_on_save && !raw.contains_key("compileOnSave") {
        raw.set("compileOnSave".to_string(), ConfigValue::Bool(true));
    }
    (compiler_options.checkers, compiler_options.single_threaded) =
        merge_worker_options(inherited_workers, own_workers, &raw);

    if !config_file_name.is_empty() {
        compiler_options.config_file_path = normalize_slashes(config_file_name);
    }
    // An extended config is read by `ParseExtendedConfig` -> `parseConfig`
    // alone (`tsconfigparsing.go:1033`): its options and raw specs are merged
    // into the extending config, but its files are never expanded and the
    // spec checks of `parseJsonConfigFileContentWorker` never run on it. Only
    // the invoked config's merged specs select the program.
    if depth > 0 {
        error_files.resize(errors.len(), own_file);
        return ParsedCommandLine {
            compiler_options,
            file_names: Vec::new(),
            literal_file_count: 0,
            raw,
            errors,
            error_files,
        };
    }
    let files_span = properties.as_ref().and_then(|properties| {
        properties.iter().find(|property| property.name == "files").map(|property| property.span)
    });
    let specs = file_specs(
        &raw,
        &compiler_options,
        config_file_name,
        &base_path_for_file_names,
        files_span,
        &mut errors,
    );
    error_files.resize(errors.len(), own_file);
    let (file_names, literal_file_count) =
        file_names::expand(&specs, &base_path_for_file_names, &compiler_options, fs);

    if file_names.is_empty() && can_report_no_input_files(&raw) {
        errors.push(Diagnostic::with_args(
            &messages::NO_INPUTS_WERE_FOUND_IN_CONFIG_FILE_0_SPECIFIED_INCLUDE_PATHS_WERE_1_AND_EXCLUDE_PATHS_WERE_2,
            tsr_core::Span::default(),
            [config_file_name.to_string(), quoted_list(&specs.include), quoted_list(&specs.exclude)],
        ));
    }

    // The specs and no-inputs errors carry no position.
    error_files.resize(errors.len(), None);

    ParsedCommandLine { compiler_options, file_names, literal_file_count, raw, errors, error_files }
}

/// One inherited `include`/`exclude`/`files` list, rewritten to be relative
/// to the extending config (the `core.Map` in `parseConfig`'s
/// `setPropertyValue`, `tsconfigparsing.go:1110-1130`).
///
/// A base's specs were written relative to the base's directory, but they are
/// expanded against the *invoked* config's directory, so each relative spec is
/// prefixed with the path from the extending config's `base_path` to the
/// base's directory. A rooted spec, a `${configDir}` spec (which already names
/// the invoked config's directory), and a non-string element pass through.
///
/// `relative_difference` is computed once per extended config, on its first
/// relative spec, and shared by its three lists, as upstream's closure
/// variable is. Upstream tests it against `""` rather than a separate flag, so
/// a base in the same directory recomputes the (empty) difference each time;
/// the `Option` gives the same answers.
fn rebase_inherited_specs(
    specs: &[ConfigValue],
    extended_config_path: &str,
    base_path: &str,
    use_case_sensitive_file_names: bool,
    relative_difference: &mut Option<String>,
) -> Vec<ConfigValue> {
    specs
        .iter()
        .map(|spec| {
            let ConfigValue::String(path) = spec else {
                return spec.clone();
            };
            if starts_with_config_dir_template(path) || tsr_path::is_rooted_disk_path(path) {
                return spec.clone();
            }
            let difference = relative_difference.get_or_insert_with(|| {
                tsr_path::convert_to_relative_path(
                    get_directory_path(extended_config_path),
                    &tsr_path::ComparePathsOptions {
                        use_case_sensitive_file_names,
                        current_directory: normalize_slashes(base_path),
                    },
                )
            });
            ConfigValue::String(tsr_path::combine_paths(difference, &[path]))
        })
        .collect()
}

/// `startsWithConfigDirTemplate` (`tsconfigparsing.go:441`): a
/// case-insensitive prefix test.
fn starts_with_config_dir_template(value: &str) -> bool {
    value
        .get(..CONFIG_DIR_TEMPLATE.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(CONFIG_DIR_TEMPLATE))
}

/// `getSubstitutedStringArrayWithConfigDirTemplate` (`tsconfigparsing.go:1590`)
/// with `getSubstitutedPathWithConfigDirTemplate` (`:1586`): a spec that starts
/// with `${configDir}` has its first exact occurrence replaced by `./` and is
/// made absolute against the invoked config's directory.
fn substitute_config_dir_in_specs(specs: &mut [String], base_path: &str) {
    for spec in specs {
        if starts_with_config_dir_template(spec) {
            *spec = get_normalized_absolute_path(
                &spec.replacen(CONFIG_DIR_TEMPLATE, "./", 1),
                base_path,
            );
        }
    }
}

/// Substitute `${configDir}` in every path-valued option.
///
/// A `tsconfig.json` template variable rather than a shell one: it expands to
/// the directory of the config that was invoked, which is what lets a base
/// config shared through `extends` name per-project output directories.
///
/// Applied to the options this port stores as paths. `paths` itself is not
/// substituted — its values are module specifiers resolved against
/// `pathsBasePath`, not filesystem paths.
fn expand_config_dir(options: &mut CompilerOptions, config_dir: &str) {
    const TEMPLATE: &str = "${configDir}";
    let expand = |value: &mut String| {
        if value.contains(TEMPLATE) {
            *value = normalize_slashes(&value.replace(TEMPLATE, config_dir));
        }
    };
    expand(&mut options.out_dir);
    expand(&mut options.declaration_dir);
    expand(&mut options.root_dir);
    expand(&mut options.base_url);
    expand(&mut options.out_file);
    for root in &mut options.root_dirs {
        expand(root);
    }
    if let Some(roots) = options.type_roots.as_mut() {
        for root in roots {
            expand(root);
        }
    }
}

/// A parsed base config and the path it was read from.
type ExtendedConfig = (String, ParsedCommandLine);

/// Resolve and parse every config this one extends.
///
/// Returns them in declaration order, each with the path it was read from, so
/// a later `extends` entry overrides an earlier one — which is what upstream's
/// left-to-right merge does — and inherited specs can be rebased from the
/// base's directory.
fn extended_configs(
    properties: &[ConfigProperty<'_>],
    nodes: &tsr_ast::NodeTable,
    config_file_name: &str,
    base_path: &str,
    fs: &dyn FileSystem,
    depth: u32,
    config_dir: &str,
) -> (Vec<ExtendedConfig>, Vec<(Diagnostic, bool)>) {
    let mut errors = Vec::new();
    if depth >= MAX_EXTENDS_DEPTH {
        return (Vec::new(), errors);
    }
    let new_base = if config_file_name.is_empty() {
        base_path.to_string()
    } else {
        get_directory_path(config_file_name).to_string()
    };
    let mut paths = Vec::new();
    // onPropertySet resolves every written initializer, retaining its errors,
    // but overwrites extendedConfigPath: only the final paths load base files.
    for property in properties.iter().filter(|property| property.name == "extends") {
        paths.clear();
        let mut names = Vec::new();
        match &property.value {
            ConfigValue::String(name) => names.push((name.as_str(), Some(property.span))),
            ConfigValue::List(entries) => {
                let elements = property.value_expression.and_then(|expression| {
                    if let tsr_ast::Expression::ArrayLiteralExpression(array) = expression {
                        Some(array.elements)
                    } else {
                        None
                    }
                });
                for (index, entry) in entries.iter().enumerate() {
                    if let Some(name) = entry.as_str() {
                        let span = elements.and_then(|elements| elements.get(index)).and_then(
                            |expression| value::span_of(tsr_ast::Node::from(*expression), nodes),
                        );
                        names.push((name, span));
                    }
                }
            }
            _ => continue,
        }
        for (name, span) in names {
            if let Some(path) = resolve_extends_path(name, &new_base, fs) {
                paths.push(path);
            } else {
                errors.push((
                    Diagnostic::with_args(
                        if name.is_empty() {
                            &messages::COMPILER_OPTION_0_CANNOT_BE_GIVEN_AN_EMPTY_STRING
                        } else {
                            &messages::FILE_0_NOT_FOUND
                        },
                        span.unwrap_or_default(),
                        [if name.is_empty() { "extends".to_string() } else { name.to_string() }],
                    ),
                    span.is_some(),
                ));
            }
        }
    }

    let mut parsed = Vec::new();
    for path in paths {
        if let Some(text) = fs.read_file(&path) {
            let directory = get_directory_path(&path).to_string();
            let base =
                parse_config_file_at_depth(&path, &text, &directory, fs, depth + 1, config_dir);
            parsed.push((path, base));
        } else {
            // A resolved explicit .json read failure is global TS5083.
            errors.push((
                Diagnostic::with_args(
                    &messages::CANNOT_READ_FILE_0,
                    tsr_core::Span::default(),
                    [path],
                ),
                false,
            ));
        }
    }
    (parsed, errors)
}

/// `getExtendsConfigPath` — the file an `extends` value names.
///
/// A rooted or explicitly-relative name resolves against the config's directory;
/// **anything else is a module**, resolved through `node_modules` exactly as an
/// `import` would be (`module.ResolveConfig`, `resolver.go:2077`).
///
/// The module form is not exotic. `"extends":
/// "@scope/tsconfig/base.json"` is how every monorepo shares a configuration,
/// and it was the *first* thing a real repository hit when this port declined
/// it — which is why the earlier decision to decline was wrong in practice even
/// though it was defensible in the abstract.
fn resolve_extends_path(name: &str, base_path: &str, fs: &dyn FileSystem) -> Option<String> {
    let name = normalize_slashes(name);
    if !(tsr_path::is_rooted_disk_path(&name) || name.starts_with("./") || name.starts_with("../"))
    {
        return resolve_extends_module(&name, base_path, fs);
    }
    let path = get_normalized_absolute_path(&name, base_path);
    if fs.file_exists(&path)
        || std::path::Path::new(name.as_str())
            .extension()
            .is_some_and(|extension| extension == "json")
    {
        return Some(path);
    }
    // A name without `.json` is retried with it, so `"extends": "./base"` works.
    //
    // Case-sensitive, deliberately: upstream compares against
    // `tspath.ExtensionJson` with `strings.HasSuffix`, so `./base.JSON` is
    // retried as `./base.JSON.json`. Matching a compiler's path handling to its
    // upstream matters more here than being lenient.
    if std::path::Path::new(path.as_str()).extension().is_none_or(|extension| extension != "json") {
        let with_extension = format!("{path}.json");
        if fs.file_exists(&with_extension) {
            return Some(with_extension);
        }
    }
    None
}

/// Resolve a bare `extends` name through `node_modules` (`module.ResolveConfig`).
///
/// The resolver needs a host, and the only thing it uses one for here is the
/// file system and a current directory — so the config's own directory serves
/// as both.
fn resolve_extends_module(name: &str, base_path: &str, fs: &dyn FileSystem) -> Option<String> {
    struct ConfigHost<'a> {
        fs: &'a dyn FileSystem,
        current_directory: String,
    }
    impl tsr_module::types::ResolutionHost for ConfigHost<'_> {
        fn fs(&self) -> &dyn FileSystem {
            self.fs
        }
        fn current_directory(&self) -> &str {
            &self.current_directory
        }
    }

    let host = ConfigHost { fs, current_directory: base_path.to_string() };
    // `nodenext`, not the project's own setting: the config being extended has
    // not been read yet, so its `moduleResolution` is what this is resolving
    // *toward*.
    let options = CompilerOptions {
        module_resolution: tsr_core::ModuleResolutionKind::NodeNext,
        ..CompilerOptions::default()
    };
    let resolver = tsr_module::resolver::Resolver::new(&host, options);
    let from_config = tsr_path::combine_paths(base_path, &["tsconfig.json"]);
    let found = resolver.resolve_config(name, &from_config);
    found.is_resolved().then(|| found.resolved_file_name.clone())
}

/// Layer `own` over `base`, field by field.
///
/// Only the options a config can set are merged; the driver's own
/// (`--help`, `--showConfig`) cannot appear in a `tsconfig.json` and are left
/// alone. An option `own` did not set keeps `base`'s value, which for a
/// `Tristate` is exactly "unset means inherit".
fn merge_options(base: CompilerOptions, own: CompilerOptions) -> CompilerOptions {
    let mut merged = base;
    macro_rules! tristate {
        ($($field:ident),* $(,)?) => {
            $(if !own.$field.is_unknown() { merged.$field = own.$field; })*
        };
    }
    macro_rules! text {
        ($($field:ident),* $(,)?) => {
            $(if !own.$field.is_empty() { merged.$field.clone_from(&own.$field); })*
        };
    }
    macro_rules! list {
        ($($field:ident),* $(,)?) => {
            $(if !own.$field.is_empty() { merged.$field.clone_from(&own.$field); })*
        };
    }

    tristate!(
        no_lib,
        allow_js,
        check_js,
        strict,
        declaration,
        es_module_interop,
        isolated_modules,
        allow_arbitrary_extensions,
        trace_resolution,
        no_resolve,
        resolve_json_module,
        no_dts_resolution,
        resolve_package_json_exports,
        resolve_package_json_imports,
        preserve_symlinks,
        no_implicit_any,
        allow_non_ts_extensions,
        lib_replacement,
        allow_synthetic_default_imports,
        always_strict,
        strict_null_checks,
        strict_property_initialization,
        use_unknown_in_catch_variables,
        no_unchecked_indexed_access,
        no_unused_locals,
        no_unused_parameters,
        allow_unreachable_code,
        preserve_const_enums,
        verbatim_module_syntax,
        no_unchecked_side_effect_imports,
        no_emit,
        no_emit_on_error,
        no_check,
        composite,
        incremental,
        skip_lib_check,
        skip_default_lib_check,
        source_map,
        declaration_map,
        emit_declaration_only,
        remove_comments,
    );
    text!(base_url, out_dir, declaration_dir, root_dir, out_file, jsx_import_source);
    list!(lib, root_dirs, module_suffixes, custom_conditions);

    if own.target != tsr_core::ScriptTarget::None {
        merged.target = own.target;
    }
    if own.module != tsr_core::ModuleKind::None {
        merged.module = own.module;
    }
    if own.module_resolution != tsr_core::ModuleResolutionKind::Unknown {
        merged.module_resolution = own.module_resolution;
    }
    if own.jsx != tsr_core::JsxEmit::None {
        merged.jsx = own.jsx;
    }
    if own.types.is_some() {
        merged.types = own.types;
    }
    if own.type_roots.is_some() {
        merged.type_roots = own.type_roots;
    }
    if !own.paths.is_empty() {
        merged.paths = own.paths;
        merged.paths_base_path = own.paths_base_path;
    }
    if own.max_node_module_js_depth.is_some() {
        merged.max_node_module_js_depth = own.max_node_module_js_depth;
    }
    merged
}

/// Native worker-field merge, including nulls belonging to this source config.
fn merge_worker_options(
    (base_checkers, base_single): (Option<isize>, Tristate),
    (own_checkers, own_single): (Option<isize>, Tristate),
    raw: &OrderedMap<ConfigValue>,
) -> (Option<isize>, Tristate) {
    let is_null = |name| matches!(raw.get("compilerOptions"), Some(ConfigValue::Map(options)) if matches!(options.get(name), Some(ConfigValue::Null)));
    (
        if own_checkers.is_some() || is_null("checkers") { own_checkers } else { base_checkers },
        if !own_single.is_unknown() || is_null("singleThreaded") {
            own_single
        } else {
            base_single
        },
    )
}

/// What reading the config's own properties produced
/// (`tsoptions.parsedTsconfig`).
#[derive(Default)]
struct Config {
    compiler_options: CompilerOptions,
    raw: OrderedMap<ConfigValue>,
    errors: Vec<Diagnostic>,
}

impl Config {
    /// `parseOwnConfigOfJsonSourceFile`, for the root object's properties.
    fn read(
        &mut self,
        properties: &[ConfigProperty<'_>],
        base_path: &str,
        nodes: &tsr_ast::NodeTable,
    ) {
        for property in properties {
            // Every root key is kept raw — `files`, `include`, `exclude` and
            // `references` are not compiler options and are read from here.
            self.raw.set(property.name.clone(), property.value.clone());

            if property.name != "compilerOptions" {
                continue;
            }
            let Some(tsr_ast::Expression::ObjectLiteralExpression(object)) =
                property.value_expression
            else {
                // `"compilerOptions": 1`. Upstream reports the type error and
                // carries on with no options, which is what an empty read does.
                continue;
            };
            for option in value::properties_of(object, nodes) {
                self.set_compiler_option(&option, base_path);
            }
        }
    }

    /// One `compilerOptions` entry (`onPropertySet` → `ParseCompilerOptions`).
    fn set_compiler_option(&mut self, property: &ConfigProperty<'_>, base_path: &str) {
        let Some(declaration) = declarations::find(&property.name) else {
            // `onPropertySet` (`tsconfigparsing.go:215`): at the key, naming
            // `ElementOptions.GetSpellingSuggestion` (`:611`) when it has one.
            let suggestion = tsr_core::get_spelling_suggestion(
                &property.name,
                declarations::COMPILER_OPTIONS,
                |option| option.name,
                |a, b| a.name.cmp(b.name),
            );
            self.errors.push(match suggestion {
                Some(option) => Diagnostic::with_args(
                    &messages::UNKNOWN_COMPILER_OPTION_0_DID_YOU_MEAN_1,
                    property.name_span,
                    [property.name.clone(), option.name.to_string()],
                ),
                None => Diagnostic::with_args(
                    &messages::UNKNOWN_COMPILER_OPTION_0,
                    property.name_span,
                    [property.name.clone()],
                ),
            });
            return;
        };
        // A path option is made absolute against the config's directory
        // *before* it is stored, so nothing downstream needs to know where the
        // config was (`normalizeNonListOptionValue`).
        let value = normalize_option_value(declaration, &property.value, base_path);
        if !(declaration.apply)(&mut self.compiler_options, &value) {
            // `convertJsonOptionOfEnumType`: a string outside the map is
            // `createDiagnosticForInvalidEnumType` (`errors.go:14`).
            if declaration.kind == declarations::OptionKind::Enum && value.as_str().is_some() {
                self.errors.push(Diagnostic::with_args(
                    &messages::ARGUMENT_FOR_0_OPTION_MUST_BE_COLON_1,
                    property.span,
                    [
                        format!("--{}", declaration.name),
                        format!("'{}'", declaration.enum_names.join("', '")),
                    ],
                ));
                return;
            }
            self.errors.push(Diagnostic::with_args(
                &messages::COMPILER_OPTION_0_REQUIRES_A_VALUE_OF_TYPE_1,
                property.span,
                [property.name.clone(), option_type_name(declaration)],
            ));
        }
    }
}

/// Make a file-path option absolute (`normalizeNonListOptionValue`).
fn normalize_option_value(
    declaration: &OptionDeclaration,
    value: &ConfigValue,
    base_path: &str,
) -> ConfigValue {
    if !declaration.is_file_path {
        return value.clone();
    }
    match value {
        ConfigValue::String(path) => ConfigValue::String(absolute(path, base_path)),
        ConfigValue::List(entries) => ConfigValue::List(
            entries
                .iter()
                .map(|entry| match entry {
                    ConfigValue::String(path) => ConfigValue::String(absolute(path, base_path)),
                    other => other.clone(),
                })
                .collect(),
        ),
        other => other.clone(),
    }
}

/// An empty path means the config's own directory, which upstream spells `"."`
/// before making it absolute.
fn absolute(path: &str, base_path: &str) -> String {
    // A `${configDir}` value is left verbatim. It is already anchored — to the
    // invoked config's directory — so joining it to *this* file's directory
    // would produce `../configs/second/${configDir}/decls`, which is what this
    // function did before the template existed. `expand_config_dir` substitutes
    // it afterwards, and the result is absolute by construction.
    if path.contains("${configDir}") {
        return normalize_slashes(path);
    }
    let normalized = normalize_slashes(path);
    let normalized = if normalized.is_empty() { "." } else { &normalized };
    get_normalized_absolute_path(normalized, base_path)
}

/// The `files`/`include`/`exclude` a config resolved to, with upstream's
/// defaults applied (the middle of `parseJsonConfigFileContentWorker`).
fn file_specs(
    raw: &OrderedMap<ConfigValue>,
    options: &CompilerOptions,
    config_file_name: &str,
    base_path_for_file_names: &str,
    files_span: Option<tsr_core::Span>,
    errors: &mut Vec<Diagnostic>,
) -> ConfigFileSpecs {
    let files = string_list_property(raw, "files");
    let include = string_list_property(raw, "include");
    let mut exclude = string_list_property(raw, "exclude");

    if files.as_ref().is_some_and(Vec::is_empty)
        && raw.get("references").and_then(ConfigValue::as_list).is_none_or(<[_]>::is_empty)
        && raw.get("extends").is_none()
    {
        errors.push(Diagnostic::with_args(
            &messages::THE_FILES_LIST_IN_CONFIG_FILE_0_IS_EMPTY,
            files_span.unwrap_or_default(),
            [if config_file_name.is_empty() { "tsconfig.json" } else { config_file_name }
                .to_string()],
        ));
    }

    // With no `exclude`, the output directories exclude themselves — otherwise
    // a second build would compile what the first one emitted.
    if exclude.is_none() {
        let outputs: Vec<String> = [&options.out_dir, &options.declaration_dir]
            .into_iter()
            .filter(|directory| !directory.is_empty())
            .cloned()
            .collect();
        if !outputs.is_empty() {
            exclude = Some(outputs);
        }
    }

    // Neither `files` nor `include` means "everything under here".
    let include = match (&files, include) {
        (None, None) => vec![DEFAULT_INCLUDE_SPEC.to_string()],
        (_, include) => include.unwrap_or_default(),
    };

    // `${configDir}` in a spec names the invoked config's directory, after the
    // output-directory default (whose values are already absolute).
    let mut files = files.unwrap_or_default();
    let mut include = include;
    let mut exclude = exclude.unwrap_or_default();
    for specs in [&mut files, &mut include, &mut exclude] {
        substitute_config_dir_in_specs(specs, base_path_for_file_names);
    }

    ConfigFileSpecs { files, include, exclude }
}

/// A root property read as a list of strings, or `None` if absent.
///
/// A non-list value is a type error upstream reports; here it reads as absent,
/// which is the same behaviour it produces — `"files": "a.ts"` appears in the
/// corpus and upstream falls back to the default include for it.
fn string_list_property(raw: &OrderedMap<ConfigValue>, name: &str) -> Option<Vec<String>> {
    Some(
        raw.get(name)?
            .as_list()?
            .iter()
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
    )
}

/// Whether "no inputs were found" is worth reporting
/// (`canJsonReportNoInputFiles`).
///
/// Only an invocation without explicit files or references reports TS18003;
/// extended bases are excluded by the caller's resolution depth (5b1047d:1425).
fn can_report_no_input_files(raw: &OrderedMap<ConfigValue>) -> bool {
    !raw.contains_key("files") && !raw.contains_key("references")
}

/// The directory a config's relative paths resolve against
/// (`tsoptions.directoryOfCombinedPath`).
fn directory_of_combined_path(file_name: &str, base_path: &str) -> String {
    get_directory_path(&get_normalized_absolute_path(file_name, base_path)).to_string()
}

/// `["a","b"]`, the way upstream's `StringifyJson` renders a spec list into the
/// "no inputs were found" message.
fn quoted_list(values: &[String]) -> String {
    let inner: Vec<String> = values.iter().map(|value| format!("\"{value}\"")).collect();
    format!("[{}]", inner.join(","))
}

/// The type name an option's error message names
/// (`getCompilerOptionValueTypeString`).
fn option_type_name(declaration: &OptionDeclaration) -> String {
    use declarations::OptionKind;
    match declaration.kind {
        OptionKind::Boolean => "boolean",
        OptionKind::String | OptionKind::Enum => "string",
        OptionKind::Number => "number",
        OptionKind::List(_) => "Array",
        OptionKind::PathMap => "object",
    }
    .to_string()
}

#[cfg(test)]
mod tests {
    use tsr_core::{ModuleKind, ModuleResolutionKind, ScriptTarget, Tristate};
    use tsr_vfs::InMemoryFileSystem;

    use super::*;

    fn fs(paths: &[&str]) -> InMemoryFileSystem {
        InMemoryFileSystem::new(
            paths.iter().map(|path| ((*path).to_string(), String::new())),
            [],
            true,
        )
    }

    fn parse(config: &str, paths: &[&str]) -> ParsedCommandLine {
        parse_config_file("/tsconfig.json", config, "/", &fs(paths))
    }

    #[test]
    fn an_empty_config_takes_every_file_under_its_directory() {
        let parsed = parse("{}", &["/a.ts", "/src/b.ts", "/node_modules/pkg/c.d.ts"]);
        assert_eq!(parsed.file_names, ["/a.ts", "/src/b.ts"]);
        assert_eq!(parsed.literal_file_count, 0);
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    }

    #[test]
    fn compiler_options_are_read_and_typed() {
        let parsed = parse(
            r#"{ "compilerOptions": {
                "module": "commonjs", "moduleResolution": "node16",
                "target": "es2020", "strict": true, "types": ["node"]
            } }"#,
            &["/a.ts"],
        );
        let options = &parsed.compiler_options;
        assert_eq!(options.module, ModuleKind::CommonJS);
        assert_eq!(options.module_resolution, ModuleResolutionKind::Node16);
        assert_eq!(options.target, ScriptTarget::ES2020);
        assert_eq!(options.strict, Tristate::True);
        assert_eq!(options.types, Some(vec!["node".to_string()]));
        assert_eq!(options.config_file_path, "/tsconfig.json");
    }

    #[test]
    fn a_path_option_is_made_absolute_against_the_configs_directory() {
        let parsed = parse_config_file(
            "/project/tsconfig.json",
            r#"{ "compilerOptions": { "outDir": "bin", "typeRoots": ["./types"] } }"#,
            "/",
            &fs(&["/project/a.ts"]),
        );
        assert_eq!(parsed.compiler_options.out_dir, "/project/bin");
        assert_eq!(parsed.compiler_options.type_roots, Some(vec!["/project/types".to_string()]));
    }

    #[test]
    fn an_out_dir_excludes_itself_when_exclude_is_absent() {
        // Otherwise the second build compiles what the first one emitted.
        let parsed = parse(
            r#"{ "compilerOptions": { "outDir": "bin" } }"#,
            &["/a.ts", "/bin/a.js", "/bin/a.d.ts"],
        );
        assert_eq!(parsed.file_names, ["/a.ts"]);
    }

    #[test]
    fn files_are_taken_verbatim_and_lead_the_list() {
        let parsed = parse(r#"{ "files": ["b.ts"] }"#, &["/a.ts", "/b.ts"]);
        assert_eq!(parsed.file_names, ["/b.ts"]);
        assert_eq!(parsed.literal_file_count, 1);
    }

    #[test]
    fn a_files_entry_that_is_not_a_list_falls_back_to_the_default_include() {
        // `"files": "a.ts"` is in the corpus. Upstream reports a type error and
        // behaves as though `files` were absent, so `include` defaults in.
        let parsed = parse(r#"{ "files": "a.ts" }"#, &["/a.ts", "/b.ts"]);
        assert_eq!(parsed.file_names, ["/a.ts", "/b.ts"]);
    }

    #[test]
    fn an_unknown_option_is_reported_and_does_not_stop_the_rest() {
        let parsed =
            parse(r#"{ "compilerOptions": { "nonsense": 1, "strict": true } }"#, &["/a.ts"]);
        assert_eq!(parsed.compiler_options.strict, Tristate::True);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().contains("nonsense"), "{}", parsed.errors[0].text());
    }

    #[test]
    fn option_errors_point_at_the_key_or_the_value_and_name_their_file() {
        // An unknown option is reported at its key, with a suggestion when one
        // is close; a value outside the enum map is TS6046 at the value. `es3`
        // left `targetOptionMap` with TypeScript 6.
        let text = r#"{ "compilerOptions": { "strictt": true, "target": "ES3" } }"#;
        let parsed = parse(text, &["/a.ts"]);
        let found: Vec<_> = parsed
            .errors
            .iter()
            .map(|d| (d.message.code(), &text[d.span.start as usize..d.span.end as usize]))
            .collect();
        assert_eq!(found, [(5025, "\"strictt\""), (6046, "\"ES3\"")]);
        assert_eq!(
            parsed.error_files,
            [Some("/tsconfig.json".to_string()), Some("/tsconfig.json".to_string())]
        );
    }

    #[test]
    fn extends_read_and_resolution_errors_keep_distinct_native_locations() {
        let text = r#"{"extends":["./missing","./missing.json"],"files":["main.ts"]}"#;
        let parsed = parse(text, &["/main.ts"]);
        let diagnostic =
            |code| parsed.errors.iter().position(|d| d.message.code() == code).unwrap();
        let resolution = diagnostic(6053);
        assert_eq!(parsed.error_files[resolution].as_deref(), Some("/tsconfig.json"));
        let span = parsed.errors[resolution].span;
        assert_eq!(&text[span.start as usize..span.end as usize], "\"./missing\"");
        let read = diagnostic(5083);
        assert_eq!(parsed.error_files[read], None);
        assert_eq!(parsed.errors[read].args, ["/missing.json"]);
    }

    #[test]
    fn empty_files_diagnostic_points_at_its_array() {
        let text = r#"{"files":[]}"#;
        let parsed = parse(text, &[]);
        let index = parsed.errors.iter().position(|d| d.message.code() == 18002).unwrap();
        assert_eq!(parsed.error_files[index].as_deref(), Some("/tsconfig.json"));
        let span = parsed.errors[index].span;
        assert_eq!(&text[span.start as usize..span.end as usize], "[]");
    }

    #[test]
    fn duplicate_extends_keeps_each_initializer_location_but_only_loads_final_paths() {
        for text in [
            r#"{"extends":"./missing-a","extends":"./missing-b","files":["main.ts"]}"#,
            r#"{"extends":"./missing-a","extends":["./missing-b"],"files":["main.ts"]}"#,
        ] {
            let parsed = parse(text, &["/main.ts"]);
            let located: Vec<_> = parsed
                .errors
                .iter()
                .enumerate()
                .filter(|(_, diagnostic)| diagnostic.message.code() == 6053)
                .map(|(index, diagnostic)| {
                    assert_eq!(parsed.error_files[index].as_deref(), Some("/tsconfig.json"));
                    &text[diagnostic.span.start as usize..diagnostic.span.end as usize]
                })
                .collect();
            assert_eq!(located, ["\"./missing-a\"", "\"./missing-b\""]);
        }
        let parsed = parse(
            r#"{"extends":"./missing.json","extends":[],"files":["main.ts"]}"#,
            &["/main.ts"],
        );
        assert!(parsed.errors.is_empty(), "an overwritten resolved path must not be read");
    }

    #[test]
    fn plugins_is_accepted_and_stored_nowhere() {
        // A language-service option (`declscompiler.go:1181`). It is declared so
        // that `tsc` accepts it and has no `core.CompilerOptions` field, so the
        // only observable behaviour is the *absence* of TS5023 — which is
        // exactly what a checking run of a real editor-configured repository
        // needs.
        let parsed = parse(
            r#"{ "compilerOptions": { "plugins": [{ "name": "next" }], "strict": true } }"#,
            &["/a.ts"],
        );
        assert!(
            parsed.errors.is_empty(),
            "`plugins` is a real option: {:?}",
            parsed.errors.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
        );
        assert_eq!(parsed.compiler_options.strict, Tristate::True);
    }

    #[test]
    fn plugins_with_a_non_array_value_is_still_a_type_error() {
        // Declaring an option this port stores nothing for must not turn it into
        // one that accepts anything.
        let parsed = parse(r#"{ "compilerOptions": { "plugins": "next" } }"#, &["/a.ts"]);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().contains("Array"), "{}", parsed.errors[0].text());
    }

    #[test]
    fn an_option_of_the_wrong_type_is_reported_and_left_unset() {
        let parsed = parse(r#"{ "compilerOptions": { "strict": "yes" } }"#, &["/a.ts"]);
        assert_eq!(parsed.compiler_options.strict, Tristate::Unknown);
        assert_eq!(parsed.errors.len(), 1);
        assert!(parsed.errors[0].text().contains("boolean"), "{}", parsed.errors[0].text());
    }

    #[test]
    fn comments_and_trailing_commas_survive() {
        // The reason the parser reads this file rather than a JSON library.
        let parsed = parse(
            "{\n  // typeRoots defaults to node_modules/@types\n  \"compilerOptions\": { \"typeRoots\": [], },\n}",
            &["/a.ts"],
        );
        assert_eq!(parsed.compiler_options.type_roots, Some(Vec::new()));
        assert!(parsed.errors.is_empty(), "{:?}", parsed.errors);
    }

    #[test]
    fn paths_keep_their_declaration_order_and_learn_their_base() {
        let parsed = parse_config_file(
            "/project/tsconfig.json",
            r#"{ "compilerOptions": { "paths": { "z/*": ["./z/*"], "a/*": ["./a/*"] } } }"#,
            "/",
            &fs(&["/project/a.ts"]),
        );
        assert_eq!(parsed.compiler_options.paths.keys().collect::<Vec<_>>(), ["z/*", "a/*"]);
        assert_eq!(parsed.compiler_options.paths_base_path, "/project");
    }

    #[test]
    fn inherited_paths_resolve_from_the_config_that_wrote_them() {
        let fs = InMemoryFileSystem::new(
            [
                (
                    "/other/tsconfig.base.json".into(),
                    r#"{"compilerOptions":{"paths":{"p1":["./lib/p1"]}}}"#.into(),
                ),
                ("/project/index.ts".into(), String::new()),
            ],
            [],
            true,
        );
        let inherited = parse_config_file(
            "/project/tsconfig.json",
            r#"{"extends":"../other/tsconfig.base.json"}"#,
            "/",
            &fs,
        );
        assert_eq!(inherited.compiler_options.paths_base_path, "/other");
        let own = parse_config_file(
            "/project/tsconfig.json",
            r#"{"extends":"../other/tsconfig.base.json","compilerOptions":{"paths":{"q":["./q"]}}}"#,
            "/",
            &fs,
        );
        assert_eq!(own.compiler_options.paths_base_path, "/project");
    }
    #[test]
    fn own_worker_options_replace_base_values_and_explicit_false_survives() {
        let fs = InMemoryFileSystem::new([
            ("/base.json".into(), r#"{"compilerOptions":{"checkers":8,"singleThreaded":true},"files":["main.ts"]}"#.into()),
            ("/main.ts".into(), String::new()),
        ], [], true);
        let inherited =
            parse_config_file("/tsconfig.json", r#"{"extends":"./base.json"}"#, "/", &fs);
        assert!(inherited.errors.is_empty());
        assert_eq!(inherited.compiler_options.checkers, Some(8));
        assert_eq!(inherited.compiler_options.single_threaded, Tristate::True);
        let own = parse_config_file(
            "/tsconfig.json",
            r#"{"extends":"./base.json","compilerOptions":{"checkers":2,"singleThreaded":false}}"#,
            "/",
            &fs,
        );
        assert!(own.errors.is_empty());
        assert_eq!(own.compiler_options.checkers, Some(2));
        assert_eq!(own.compiler_options.single_threaded, Tristate::False);
        let cleared = parse_config_file(
            "/tsconfig.json",
            r#"{"extends":"./base.json","compilerOptions":{"checkers":null,"singleThreaded":null}}"#,
            "/",
            &fs,
        );
        assert!(cleared.errors.is_empty());
        assert_eq!(cleared.compiler_options.checkers, None);
        assert_eq!(cleared.compiler_options.single_threaded, Tristate::Unknown);
    }
    #[test]
    fn worker_options_keep_last_base_order_and_declaring_null_origin() {
        let fs = InMemoryFileSystem::new([
            ("/base.json".into(), r#"{"compilerOptions":{"checkers":8,"singleThreaded":true},"files":["main.ts"]}"#.into()),
            ("/second.json".into(), r#"{"compilerOptions":{"checkers":4,"singleThreaded":false}}"#.into()),
            ("/clear.json".into(), r#"{"compilerOptions":{"checkers":null,"singleThreaded":null}}"#.into()),
            ("/derived-clear.json".into(), r#"{"extends":"./clear.json"}"#.into()),
            ("/main.ts".into(), String::new()),
        ], [], true);
        for (text, count, single) in [
            (r#"{"extends":["./base.json","./second.json"]}"#, Some(4), Tristate::False),
            (
                r#"{"extends":["./base.json","./second.json"],"compilerOptions":{"checkers":2,"singleThreaded":true}}"#,
                Some(2),
                Tristate::True,
            ),
            (r#"{"extends":["./base.json","./clear.json"]}"#, None, Tristate::Unknown),
            (r#"{"extends":["./base.json","./derived-clear.json"]}"#, Some(8), Tristate::True),
        ] {
            let parsed = parse_config_file("/tsconfig.json", text, "/", &fs);
            assert!(parsed.errors.is_empty(), "{text}: {:?}", parsed.errors);
            assert_eq!(parsed.compiler_options.checkers, count, "{text}");
            assert_eq!(parsed.compiler_options.single_threaded, single, "{text}");
        }
    }
}
