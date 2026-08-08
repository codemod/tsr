//! `--showConfig` — the configuration, resolved, as JSON.
//!
//! Ported from `internal/tsoptions/showconfig.go` at the pinned commit.
//!
//! # Why this is worth having early
//!
//! It is the highest-value phase on `STATUS-cli.md`'s board for its size: all 17
//! of the `showConfig` baselines are emit-free, and the output is a pure
//! function of the option table — no program, no checker, nothing that can be
//! wrong for a reason outside this file.
//!
//! # What it prints, and what upstream prints
//!
//! Upstream serialises the options that were **set**, in declaration order,
//! rendering each through its own declaration so an enum comes out as the name
//! the user would write rather than as an integer. This does the same for the
//! options this port declares. It does **not** yet print `files`, `include`,
//! `exclude` or `references`, which upstream includes when the config had them —
//! those live on `ParsedCommandLine`'s raw map, which the config parser does not
//! keep past the options it understands.

use tsr_core::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, ScriptTarget, Tristate,
};
use tsr_tsoptions::value::ConfigValue;

/// The order `--showConfig` prints options in (`serializeCompilerOptions`).
///
/// **This is Go struct field declaration order**, and nothing else. Upstream
/// reflects over `core.CompilerOptions` (`showconfig.go:172`) and emits every
/// non-zero exported field whose declaration is neither a command-line nor an
/// output-formatting option. So the printed order is an artefact of how the
/// upstream struct happens to be written, and it is neither the order the config
/// file wrote its keys in nor anything alphabetical.
///
/// Transcribed from the struct rather than derived, because this port's
/// `CompilerOptions` orders its fields by when the port needed them. Names not
/// declared here are simply not ported yet; a name here that this port has no
/// field for costs nothing.
///
/// **How this was got wrong twice before it was got right.** The first version
/// walked the resolved options in *our* declaration-table order. The second
/// walked the raw config in *written* order, which matched two baselines by
/// luck. Both were guesses at a rule that `showconfig.go` states outright.
const STRUCT_ORDER: &[&str] = &[
    "allowJs",
    "allowArbitraryExtensions",
    "allowUnreachableCode",
    "checkJs",
    "customConditions",
    "composite",
    "declaration",
    "declarationDir",
    "isolatedModules",
    "incremental",
    "jsx",
    "jsxImportSource",
    "lib",
    "libReplacement",
    "module",
    "moduleResolution",
    "moduleSuffixes",
    "moduleDetection",
    "noEmit",
    "noCheck",
    "noErrorTruncation",
    "noImplicitAny",
    "noLib",
    "noUncheckedIndexedAccess",
    "noEmitOnError",
    "noUnusedLocals",
    "noUnusedParameters",
    "noResolve",
    "noUncheckedSideEffectImports",
    "outDir",
    "paths",
    "preserveConstEnums",
    "preserveSymlinks",
    "resolveJsonModule",
    "resolvePackageJsonExports",
    "resolvePackageJsonImports",
    "rootDir",
    "rootDirs",
    "skipLibCheck",
    "strict",
    "strictNullChecks",
    "strictPropertyInitialization",
    "skipDefaultLibCheck",
    "target",
    "traceResolution",
    "typeRoots",
    "types",
    "useUnknownInCatchVariables",
    "verbatimModuleSyntax",
    "maxNodeModuleJsDepth",
    "allowSyntheticDefaultImports",
    "alwaysStrict",
    "baseUrl",
    "esModuleInterop",
    "outFile",
    "noDtsResolution",
];

/// Options whose value can be *derived* from others (`impliedOptions`).
///
/// Appended after the struct-ordered options, in this order, when the
/// dependency that implies them is set. This is what puts `useDefineForClassFields`
/// in a config that never wrote it, and `declaration` + `incremental` in one that
/// only wrote `composite`.
///
/// Only the implications this port can compute are listed; upstream has fourteen.
/// Each entry is `(option, dependencies)`. An implied option is printed only
/// when **all three** of upstream's conditions hold (`addImpliedOptions`,
/// `showconfig.go:300`):
///
/// 1. it was not written explicitly;
/// 2. at least one of its dependencies *was* written; and
/// 3. its computed value differs from what it would compute under wholly
///    default options — otherwise the line carries no information.
///
/// Condition 3 is the one that is easy to miss and is why `--showConfig` on a
/// config that writes only `target: es5` prints no `useDefineForClassFields`:
/// the default target is also ES5, so the computed value is unchanged.
const IMPLIED_OPTIONS: &[(&str, &[&str])] = &[
    ("module", &["target"]),
    ("moduleResolution", &["module", "target"]),
    ("moduleDetection", &["module", "target"]),
    ("isolatedModules", &["verbatimModuleSyntax"]),
    ("preserveConstEnums", &["isolatedModules", "verbatimModuleSyntax"]),
    ("declaration", &["composite"]),
    ("incremental", &["composite"]),
    ("useDefineForClassFields", &["target", "module"]),
    ("resolveJsonModule", &["moduleResolution", "module", "target"]),
    ("allowJs", &["checkJs"]),
];

/// Render the resolved configuration (`showConfig`).
///
/// The shape, verified against `showConfig/Show-TSConfig-with-references.js`:
///
/// ```text
/// {
///     "compilerOptions": {
///         "composite": true,
///         "strict": true,
///         "declaration": true,
///         "incremental": true
///     },
///     "references": [
///         {
///             "path": "./packages/a"
///         }
///     ],
///     "files": [
///         "./src/index.ts"
///     ]
/// }
/// ```
///
/// `composite` and `strict` are written; `declaration` and `incremental` are
/// **implied by `composite`** and appended. See [`STRUCT_ORDER`] for the ordering
/// rule and how it was got wrong twice.
///
/// Four spaces per level; an empty options object collapses to `{}`; `files` is
/// relative and `./`-prefixed. Section order is fixed and is not the config's:
/// `compilerOptions`, `references`, `files`, `include`, `exclude`.
#[must_use]
pub fn show_config(
    options: &CompilerOptions,
    root_files: &[String],
    raw: &tsr_core::OrderedMap<ConfigValue>,
    current_directory: &str,
    use_case_sensitive_file_names: bool,
) -> String {
    let compare = tsr_path::ComparePathsOptions {
        use_case_sensitive_file_names,
        current_directory: current_directory.to_string(),
    };

    let mut rendered: Vec<(String, String)> = Vec::new();
    for name in STRUCT_ORDER {
        if let Some(value) = option_value(options, name) {
            // A file-path option prints **relative to the config**, not as the
            // absolute path the parser resolved it to: `"outDir": "./build"`.
            // Upstream passes `configFilePath` into `serializeCompilerOptions`
            // for exactly this.
            let value = if is_file_path_option(name) {
                let unquoted = value.trim_matches('"');
                format!("\"{}\"", escape(&relative_for_display(unquoted, &compare)))
            } else {
                value
            };
            rendered.push(((*name).to_string(), value));
        }
    }
    let written: Vec<String> = rendered.iter().map(|(name, _)| name.clone()).collect();
    let defaults = CompilerOptions::default();
    for (name, dependencies) in IMPLIED_OPTIONS {
        if written.iter().any(|already| already == name) {
            continue;
        }
        if !dependencies.iter().any(|dependency| written.iter().any(|w| w == dependency)) {
            continue;
        }
        let implied = implied_value(options, name);
        if implied == implied_value(&defaults, name) {
            continue;
        }
        if let Some(value) = implied {
            rendered.push(((*name).to_string(), value));
        }
    }

    let mut sections: Vec<String> = Vec::new();
    if rendered.is_empty() {
        sections.push("    \"compilerOptions\": {}".to_string());
    } else {
        let body: Vec<String> =
            rendered.iter().map(|(name, value)| format!("        \"{name}\": {value}")).collect();
        sections.push(format!("    \"compilerOptions\": {{\n{}\n    }}", body.join(",\n")));
    }

    if let Some(ConfigValue::List(references)) = raw.get("references") {
        let items: Vec<String> = references
            .iter()
            .filter_map(|reference| render_value(reference, 2))
            .map(|text| format!("        {text}"))
            .collect();
        if !items.is_empty() {
            sections.push(format!("    \"references\": [\n{}\n    ]", items.join(",\n")));
        }
    }

    if !root_files.is_empty() {
        let relative: Vec<String> =
            root_files.iter().map(|file| relative_for_display(file, &compare)).collect();
        sections.push(format!("    \"files\": {}", string_array(&relative, 1)));
    }

    for key in ["include", "exclude"] {
        if let Some(ConfigValue::List(values)) = raw.get(key) {
            let strings: Vec<String> =
                values.iter().filter_map(|value| value.as_str().map(str::to_string)).collect();
            if !strings.is_empty() {
                sections.push(format!("    \"{key}\": {}", string_array(&strings, 1)));
            }
        }
    }

    format!("{{\n{}\n}}\n", sections.join(",\n"))
}

/// One option's rendered value, or `None` when it is unset.
///
/// "Unset" is upstream's `fieldValue.IsZero()` — the Go zero value — which for a
/// `Tristate` is `Unknown`, for a string is empty, and for an enum is the
/// `None` variant. So `--strict false` *does* print (`False` is not the zero
/// value) and an unwritten `strict` does not.
fn option_value(options: &CompilerOptions, name: &str) -> Option<String> {
    let tristate = |value: Tristate| {
        if value.is_unknown() { None } else { Some(value.is_true().to_string()) }
    };
    let string = |value: &str| {
        if value.is_empty() { None } else { Some(format!("\"{}\"", escape(value))) }
    };

    match name {
        "allowJs" => tristate(options.allow_js),
        "allowArbitraryExtensions" => tristate(options.allow_arbitrary_extensions),
        "allowUnreachableCode" => tristate(options.allow_unreachable_code),
        "checkJs" => tristate(options.check_js),
        "composite" => tristate(options.composite),
        "declaration" => tristate(options.declaration),
        "declarationDir" => string(&options.declaration_dir),
        "isolatedModules" => tristate(options.isolated_modules),
        "incremental" => tristate(options.incremental),
        "jsxImportSource" => string(&options.jsx_import_source),
        "libReplacement" => tristate(options.lib_replacement),
        "noEmit" => tristate(options.no_emit),
        "noCheck" => tristate(options.no_check),
        "noErrorTruncation" => tristate(options.no_error_truncation),
        "noImplicitAny" => tristate(options.no_implicit_any),
        "noLib" => tristate(options.no_lib),
        "noUncheckedIndexedAccess" => tristate(options.no_unchecked_indexed_access),
        "noEmitOnError" => tristate(options.no_emit_on_error),
        "noUnusedLocals" => tristate(options.no_unused_locals),
        "noUnusedParameters" => tristate(options.no_unused_parameters),
        "noResolve" => tristate(options.no_resolve),
        "noUncheckedSideEffectImports" => tristate(options.no_unchecked_side_effect_imports),
        "outDir" => string(&options.out_dir),
        "preserveConstEnums" => tristate(options.preserve_const_enums),
        "preserveSymlinks" => tristate(options.preserve_symlinks),
        "resolveJsonModule" => tristate(options.resolve_json_module),
        "resolvePackageJsonExports" => tristate(options.resolve_package_json_exports),
        "resolvePackageJsonImports" => tristate(options.resolve_package_json_imports),
        "rootDir" => string(&options.root_dir),
        "skipLibCheck" => tristate(options.skip_lib_check),
        "strict" => tristate(options.strict),
        "strictNullChecks" => tristate(options.strict_null_checks),
        "strictPropertyInitialization" => tristate(options.strict_property_initialization),
        "skipDefaultLibCheck" => tristate(options.skip_default_lib_check),
        "traceResolution" => tristate(options.trace_resolution),
        "useUnknownInCatchVariables" => tristate(options.use_unknown_in_catch_variables),
        "verbatimModuleSyntax" => tristate(options.verbatim_module_syntax),
        "allowSyntheticDefaultImports" => tristate(options.allow_synthetic_default_imports),
        "alwaysStrict" => tristate(options.always_strict),
        "baseUrl" => string(&options.base_url),
        "esModuleInterop" => tristate(options.es_module_interop),
        "outFile" => string(&options.out_file),
        "noDtsResolution" => tristate(options.no_dts_resolution),
        "target" if options.target != ScriptTarget::None => {
            Some(format!("\"{}\"", target_name(options.target)))
        }
        "module" if options.module != ModuleKind::None => {
            Some(format!("\"{}\"", module_name(options.module)))
        }
        "moduleResolution" if options.module_resolution != ModuleResolutionKind::Unknown => {
            Some(format!("\"{}\"", module_resolution_name(options.module_resolution)))
        }
        "jsx" if options.jsx != JsxEmit::None => Some(format!("\"{}\"", jsx_name(options.jsx))),
        "moduleDetection" if options.module_detection != tsr_core::ModuleDetectionKind::None => {
            Some(format!("\"{}\"", module_detection_name(options.module_detection)))
        }
        "lib" if !options.lib.is_empty() => Some(string_array(&options.lib, 2)),
        "rootDirs" if !options.root_dirs.is_empty() => Some(string_array(&options.root_dirs, 2)),
        "moduleSuffixes" if !options.module_suffixes.is_empty() => {
            Some(string_array(&options.module_suffixes, 2))
        }
        "customConditions" if !options.custom_conditions.is_empty() => {
            Some(string_array(&options.custom_conditions, 2))
        }
        "types" => options.types.as_ref().map(|types| string_array(types, 2)),
        "typeRoots" => options.type_roots.as_ref().map(|roots| string_array(roots, 2)),
        "maxNodeModuleJsDepth" => options.max_node_module_js_depth.map(|depth| depth.to_string()),
        _ => None,
    }
}

/// An option's *derived* value, when something else implies it
/// (`impliedOptions`).
///
/// Returns `None` unless the dependency is actually set, because an implication
/// with no cause is just a default and upstream does not print defaults.
fn implied_value(options: &CompilerOptions, name: &str) -> Option<String> {
    // Every arm is an existing `CompilerOptions` accessor, which is upstream's
    // shape too — `impliedOptions` stores a method reference per entry rather
    // than reimplementing the rule.
    match name {
        "module" => Some(format!("\"{}\"", module_name(options.emit_module_kind()))),
        "moduleResolution" => {
            Some(format!("\"{}\"", module_resolution_name(options.module_resolution_kind())))
        }
        "moduleDetection" => {
            Some(format!("\"{}\"", module_detection_name(options.emit_module_detection_kind())))
        }
        "isolatedModules" => Some(options.get_isolated_modules().to_string()),
        "preserveConstEnums" => Some(options.should_preserve_const_enums().to_string()),
        // `GetEmitDeclarations`.
        "declaration" => {
            Some((options.declaration.is_true() || options.composite.is_true()).to_string())
        }
        // `IsIncremental`.
        "incremental" => {
            Some((options.incremental.is_true() || options.composite.is_true()).to_string())
        }
        // `GetEmitStandardClassFields`: not false, and ES2022 or later.
        "useDefineForClassFields" => {
            Some((options.emit_script_target() >= ScriptTarget::ES2022).to_string())
        }
        "resolveJsonModule" => Some(options.get_resolve_json_module().to_string()),
        "allowJs" => Some(options.get_allow_js().to_string()),
        _ => None,
    }
}

/// A config value as JSON, indented `level` steps of four.
///
/// `None` for a value upstream omits: an explicit `null`, which means "unset
/// this" rather than "print null".
fn render_value(value: &ConfigValue, level: usize) -> Option<String> {
    let inner = "    ".repeat(level + 1);
    let outer = "    ".repeat(level);
    match value {
        ConfigValue::Null => None,
        ConfigValue::Bool(flag) => Some(flag.to_string()),
        ConfigValue::Number(number) => Some(render_number(*number)),
        ConfigValue::String(text) => Some(format!("\"{}\"", escape(text))),
        ConfigValue::List(values) => {
            let rendered: Vec<String> = values
                .iter()
                .filter_map(|entry| render_value(entry, level + 1))
                .map(|text| format!("{inner}{text}"))
                .collect();
            if rendered.is_empty() {
                Some("[]".to_string())
            } else {
                Some(format!("[\n{}\n{outer}]", rendered.join(",\n")))
            }
        }
        ConfigValue::Map(map) => {
            let rendered: Vec<String> = map
                .entries()
                .filter_map(|(name, entry)| {
                    render_value(entry, level + 1)
                        .map(|text| format!("{inner}\"{}\": {text}", escape(name)))
                })
                .collect();
            if rendered.is_empty() {
                Some("{}".to_string())
            } else {
                Some(format!("{{\n{}\n{outer}}}", rendered.join(",\n")))
            }
        }
    }
}

/// A JSON number, printed as an integer when it is one.
fn render_number(number: f64) -> String {
    if number.fract() == 0.0 && number.abs() < 1e15 {
        format!("{number:.0}")
    } else {
        number.to_string()
    }
}

/// Whether an option's value is a path, and so prints relative to the config.
///
/// Read off the declaration table rather than listed here, so it cannot drift
/// from what the parser treats as a path.
fn is_file_path_option(name: &str) -> bool {
    tsr_tsoptions::declarations::COMPILER_OPTIONS
        .iter()
        .any(|declaration| declaration.name == name && declaration.is_file_path)
}

/// A path as `--showConfig` prints it: relative, and explicitly so.
fn relative_for_display(path: &str, compare: &tsr_path::ComparePathsOptions) -> String {
    let relative = tsr_path::convert_to_relative_path(path, compare);
    if relative.starts_with('.') || relative.starts_with('/') {
        relative
    } else {
        format!("./{relative}")
    }
}

/// A JSON array of strings, one per line, indented `level` steps of four.
fn string_array(values: &[String], level: usize) -> String {
    let inner = "    ".repeat(level + 1);
    let outer = "    ".repeat(level);
    let rendered: Vec<String> =
        values.iter().map(|value| format!("{inner}\"{}\"", escape(value))).collect();
    format!("[\n{}\n{outer}]", rendered.join(",\n"))
}

/// The two escapes a path or an option value can need.
fn escape(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

fn target_name(target: ScriptTarget) -> &'static str {
    match target {
        ScriptTarget::None | ScriptTarget::ES5 => "es5",
        ScriptTarget::ES2015 => "es2015",
        ScriptTarget::ES2016 => "es2016",
        ScriptTarget::ES2017 => "es2017",
        ScriptTarget::ES2018 => "es2018",
        ScriptTarget::ES2019 => "es2019",
        ScriptTarget::ES2020 => "es2020",
        ScriptTarget::ES2021 => "es2021",
        ScriptTarget::ES2022 => "es2022",
        ScriptTarget::ES2023 => "es2023",
        ScriptTarget::ES2024 => "es2024",
        ScriptTarget::ES2025 => "es2025",
        ScriptTarget::ESNext => "esnext",
    }
}

fn module_name(module: ModuleKind) -> &'static str {
    match module {
        ModuleKind::None => "none",
        ModuleKind::CommonJS => "commonjs",
        ModuleKind::AMD => "amd",
        ModuleKind::UMD => "umd",
        ModuleKind::System => "system",
        ModuleKind::ES2015 => "es2015",
        ModuleKind::ES2020 => "es2020",
        ModuleKind::ES2022 => "es2022",
        ModuleKind::ESNext => "esnext",
        ModuleKind::Node16 => "node16",
        ModuleKind::Node18 => "node18",
        ModuleKind::Node20 => "node20",
        ModuleKind::NodeNext => "nodenext",
        ModuleKind::Preserve => "preserve",
    }
}

fn module_resolution_name(kind: ModuleResolutionKind) -> &'static str {
    match kind {
        ModuleResolutionKind::Unknown | ModuleResolutionKind::Node10 => "node10",
        ModuleResolutionKind::Classic => "classic",
        ModuleResolutionKind::Node16 => "node16",
        ModuleResolutionKind::NodeNext => "nodenext",
        ModuleResolutionKind::Bundler => "bundler",
    }
}

fn module_detection_name(kind: tsr_core::ModuleDetectionKind) -> &'static str {
    match kind {
        tsr_core::ModuleDetectionKind::None | tsr_core::ModuleDetectionKind::Auto => "auto",
        tsr_core::ModuleDetectionKind::Legacy => "legacy",
        tsr_core::ModuleDetectionKind::Force => "force",
    }
}

fn jsx_name(jsx: JsxEmit) -> &'static str {
    match jsx {
        JsxEmit::None | JsxEmit::Preserve => "preserve",
        JsxEmit::React => "react",
        JsxEmit::ReactNative => "react-native",
        JsxEmit::ReactJsx => "react-jsx",
        JsxEmit::ReactJsxDev => "react-jsxdev",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::OrderedMap;

    fn config(entries: &[(&str, ConfigValue)]) -> OrderedMap<ConfigValue> {
        let mut map = OrderedMap::default();
        for (name, value) in entries {
            map.set((*name).to_string(), value.clone());
        }
        map
    }

    fn render(options: &CompilerOptions) -> String {
        show_config(options, &[], &OrderedMap::default(), "/home/project", true)
    }

    #[test]
    fn an_empty_configuration_collapses_to_an_empty_object() {
        assert_eq!(render(&CompilerOptions::default()), "{\n    \"compilerOptions\": {}\n}\n");
    }

    #[test]
    fn options_print_in_upstreams_struct_order_not_the_configs() {
        // `Show-TSConfig-with-compileOnSave-and-more.js` writes
        // `esModuleInterop, target, module, strict` and upstream prints
        // `module, strict, target, esModuleInterop`. That single case is what
        // rules out "written order", which two earlier versions of this file
        // assumed.
        let options = CompilerOptions {
            es_module_interop: Tristate::True,
            target: ScriptTarget::ES5,
            module: ModuleKind::CommonJS,
            strict: Tristate::True,
            ..CompilerOptions::default()
        };
        let text = render(&options);
        let at = |name: &str| text.find(name).unwrap_or_else(|| panic!("{name} in {text}"));
        assert!(
            at("\"module\"") < at("\"strict\"")
                && at("\"strict\"") < at("\"target\"")
                && at("\"target\"") < at("\"esModuleInterop\""),
            "{text}"
        );
    }

    #[test]
    fn an_unset_option_is_absent_and_an_explicit_false_is_not() {
        // Upstream's rule is Go's `IsZero`, so `False` prints and `Unknown` does
        // not — the distinction `Tristate` exists for.
        assert!(!render(&CompilerOptions::default()).contains("strict"));
        let off = CompilerOptions { strict: Tristate::False, ..CompilerOptions::default() };
        assert!(render(&off).contains("\"strict\": false"));
    }

    #[test]
    fn composite_implies_declaration_and_incremental() {
        // `Show-TSConfig-with-references.js`: the config writes `composite` and
        // `strict`; upstream prints four options, the last two derived.
        let options = CompilerOptions {
            composite: Tristate::True,
            strict: Tristate::True,
            ..CompilerOptions::default()
        };
        let text = render(&options);
        assert!(text.contains("\"declaration\": true"), "{text}");
        assert!(text.contains("\"incremental\": true"), "{text}");
        // Implied options are appended, so they follow the written ones.
        assert!(text.find("\"strict\"") < text.find("\"declaration\""), "{text}");
    }

    #[test]
    fn verbatim_module_syntax_implies_isolated_modules() {
        let options = CompilerOptions {
            verbatim_module_syntax: Tristate::True,
            ..CompilerOptions::default()
        };
        let text = render(&options);
        assert!(text.contains("\"isolatedModules\": true"), "{text}");
        // And that in turn implies `preserveConstEnums`.
        assert!(text.contains("\"preserveConstEnums\": true"), "{text}");
    }

    #[test]
    fn an_implication_with_no_cause_prints_nothing() {
        // A default is not an implication; upstream prints neither.
        assert!(!render(&CompilerOptions::default()).contains("declaration"));
    }

    #[test]
    fn an_enum_renders_as_the_name_a_user_would_write() {
        let options =
            CompilerOptions { target: ScriptTarget::ES2015, ..CompilerOptions::default() };
        assert!(render(&options).contains("\"target\": \"es2015\""));
    }

    #[test]
    fn a_root_file_is_relative_and_dot_prefixed() {
        let text = show_config(
            &CompilerOptions::default(),
            &["/home/project/src/index.ts".to_string()],
            &OrderedMap::default(),
            "/home/project",
            true,
        );
        assert!(text.contains("\"./src/index.ts\""), "{text}");
    }

    #[test]
    fn references_are_objects_and_come_before_files() {
        let mut reference = OrderedMap::default();
        reference.set("path".to_string(), ConfigValue::String("./packages/a".to_string()));
        let raw = config(&[("references", ConfigValue::List(vec![ConfigValue::Map(reference)]))]);
        let text = show_config(
            &CompilerOptions::default(),
            &["/home/project/a.ts".to_string()],
            &raw,
            "/home/project",
            true,
        );
        assert!(text.contains("\"path\": \"./packages/a\""), "{text}");
        assert!(
            text.find("\"references\"") < text.find("\"files\""),
            "sections have a fixed order: {text}"
        );
    }

    #[test]
    fn include_and_exclude_are_echoed_from_the_config() {
        let raw = config(&[(
            "exclude",
            ConfigValue::List(vec![ConfigValue::String("test".to_string())]),
        )]);
        let text = show_config(&CompilerOptions::default(), &[], &raw, "/home/project", true);
        assert!(text.contains("\"exclude\""), "{text}");
        assert!(text.contains("\"test\""), "{text}");
    }

    #[test]
    fn a_whole_number_does_not_render_a_decimal_point() {
        assert_eq!(render_number(2.0), "2");
        assert_eq!(render_number(2.5), "2.5");
    }

    #[test]
    fn a_windows_path_normalises_to_forward_slashes() {
        // **Not an escaping test any more, and the change is the point.** A path
        // option is made relative before printing, and relativising runs it
        // through `normalize_slashes`, so no backslash survives to be escaped.
        // The earlier version of this test asserted `C:\\out` and was testing a
        // path that can no longer occur.
        let options =
            CompilerOptions { out_dir: r"C:\out".to_string(), ..CompilerOptions::default() };
        let text = render(&options);
        assert!(text.contains("\"outDir\": \"./C:/out\""), "{text}");
        assert!(!text.contains('\\'), "no backslash should survive: {text}");
    }

    #[test]
    fn a_quote_in_a_value_is_escaped() {
        // `escape` still matters for anything that is not a path — an `include`
        // pattern, a reference path — so it keeps a test of its own.
        assert_eq!(escape(r#"a"b"#), r#"a\"b"#);
        assert_eq!(escape(r"a\b"), r"a\\b");
    }
}
