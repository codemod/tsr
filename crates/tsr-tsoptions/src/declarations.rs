//! What each compiler option is called, what type it takes, and where it lands.
//!
//! Ported from `internal/tsoptions/declscompiler.go` and
//! `internal/tsoptions/parsinghelpers.go` (`ParseCompilerOptions`) at the pinned
//! commit.
//!
//! # One table, not two
//!
//! Upstream keeps the declaration (`optionsForCompiler`, which says `"target"`
//! is an enum over `ScriptTarget`) apart from the assignment
//! (`ParseCompilerOptions`, a 400-case switch on the option name that writes the
//! converted value into the struct). Two lists ordered by the same key, which
//! have to be kept in step by hand — and upstream has a test whose only job is
//! to check that they are.
//!
//! Here each [`OptionDeclaration`] carries its own setter, so the name, the
//! type, and the field are one entry. There is nothing to keep in step.
//!
//! # Why the table is short
//!
//! Upstream declares around 130 options because it has to accept every one a
//! user might write. This declares the ones [`CompilerOptions`] has a field for,
//! which is the same rule that governs the struct itself: an option here with no
//! field would parse into nothing. Everything else is reported as unknown, which
//! is what upstream does for a genuinely unknown option — the difference is
//! visible only in the diagnostic, and config diagnostics are not judged by any
//! suite yet.

use tsr_core::{
    CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, OrderedMap, ScriptTarget, Tristate,
};

use crate::value::ConfigValue;

/// What kind of value an option takes (`tsoptions.CommandLineOptionKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionKind {
    /// `true`/`false`.
    Boolean,
    /// A string.
    String,
    /// A number.
    Number,
    /// One of a fixed set of names, matched case-insensitively.
    Enum,
    /// A list of some element kind.
    List(&'static OptionKind),
    /// An object whose values are lists of strings — `paths`, and only `paths`.
    PathMap,
}

/// One compiler option (`tsoptions.CommandLineOption` plus its
/// `ParseCompilerOptions` arm).
pub struct OptionDeclaration {
    /// The name as written in `compilerOptions`.
    pub name: &'static str,
    /// The value shape it accepts.
    pub kind: OptionKind,
    /// Whether the value is a path, and so is made absolute against the config's
    /// directory before it is stored (`CommandLineOption.IsFilePath`).
    pub is_file_path: bool,
    /// Write the converted value into the options.
    ///
    /// Returns `false` when the value had the wrong shape, which is upstream's
    /// `Compiler option '{0}' requires a value of type '{1}'`.
    pub apply: fn(&mut CompilerOptions, &ConfigValue) -> bool,
}

/// The option this name declares, if any (`optionsForCompiler`, by name).
///
/// Matched case-sensitively, as upstream does for a config file: `"Target"` is
/// an unknown option, not `target`.
#[must_use]
pub fn find(name: &str) -> Option<&'static OptionDeclaration> {
    COMPILER_OPTIONS.iter().find(|option| option.name == name)
}

/// Assign a boolean-valued option.
fn tristate(
    options: &mut CompilerOptions,
    value: &ConfigValue,
    field: fn(&mut CompilerOptions) -> &mut Tristate,
) -> bool {
    let ConfigValue::Bool(value) = value else { return false };
    *field(options) = Tristate::from_bool(*value);
    true
}

/// Assign a string-valued option.
fn string(
    options: &mut CompilerOptions,
    value: &ConfigValue,
    field: fn(&mut CompilerOptions) -> &mut String,
) -> bool {
    let Some(value) = value.as_str() else { return false };
    *field(options) = value.to_string();
    true
}

/// Assign a list-of-strings option, dropping the falsy entries upstream drops.
fn string_list(value: &ConfigValue) -> Option<Vec<String>> {
    Some(
        value
            .as_list()?
            .iter()
            .filter(|entry| !entry.is_falsy())
            .filter_map(|entry| entry.as_str().map(str::to_string))
            .collect(),
    )
}

/// Match an enum value by name, case-insensitively as upstream does.
fn enum_value<T: Copy>(value: &ConfigValue, names: &[(&str, T)]) -> Option<T> {
    let text = value.as_str()?;
    names.iter().find(|(name, _)| name.eq_ignore_ascii_case(text)).map(|(_, variant)| *variant)
}

/// `paths`: an object of `pattern -> [substitutions]`, in declaration order.
fn path_map(value: &ConfigValue) -> Option<OrderedMap<Vec<String>>> {
    let entries = value.as_map()?;
    Some(
        entries
            .entries()
            .map(|(pattern, substitutions)| {
                (pattern.to_string(), string_list(substitutions).unwrap_or_default())
            })
            .collect(),
    )
}

/// Every option this port understands.
///
/// The order is upstream's declaration order, which is the order `tsc
/// --showConfig` and `--help` print them in.
pub static COMPILER_OPTIONS: &[OptionDeclaration] = &[
    OptionDeclaration {
        name: "target",
        kind: OptionKind::Enum,
        is_file_path: false,
        apply: |options, value| {
            // `es3` is upstream's deprecated alias for `es5`, and `latest` for
            // `esnext`; both still appear in the corpus.
            let Some(target) = enum_value(
                value,
                &[
                    ("es3", ScriptTarget::ES5),
                    ("es5", ScriptTarget::ES5),
                    ("es6", ScriptTarget::ES2015),
                    ("es2015", ScriptTarget::ES2015),
                    ("es2016", ScriptTarget::ES2016),
                    ("es2017", ScriptTarget::ES2017),
                    ("es2018", ScriptTarget::ES2018),
                    ("es2019", ScriptTarget::ES2019),
                    ("es2020", ScriptTarget::ES2020),
                    ("es2021", ScriptTarget::ES2021),
                    ("es2022", ScriptTarget::ES2022),
                    ("es2023", ScriptTarget::ES2023),
                    ("es2024", ScriptTarget::ES2024),
                    ("es2025", ScriptTarget::ES2025),
                    ("esnext", ScriptTarget::ESNext),
                    ("latest", ScriptTarget::ESNext),
                ],
            ) else {
                return false;
            };
            options.target = target;
            true
        },
    },
    OptionDeclaration {
        name: "module",
        kind: OptionKind::Enum,
        is_file_path: false,
        apply: |options, value| {
            let Some(module) = enum_value(
                value,
                &[
                    ("none", ModuleKind::None),
                    ("commonjs", ModuleKind::CommonJS),
                    ("amd", ModuleKind::AMD),
                    ("umd", ModuleKind::UMD),
                    ("system", ModuleKind::System),
                    ("es6", ModuleKind::ES2015),
                    ("es2015", ModuleKind::ES2015),
                    ("es2020", ModuleKind::ES2020),
                    ("es2022", ModuleKind::ES2022),
                    ("esnext", ModuleKind::ESNext),
                    ("node16", ModuleKind::Node16),
                    ("node18", ModuleKind::Node18),
                    ("node20", ModuleKind::Node20),
                    ("nodenext", ModuleKind::NodeNext),
                    ("preserve", ModuleKind::Preserve),
                ],
            ) else {
                return false;
            };
            options.module = module;
            true
        },
    },
    OptionDeclaration {
        name: "moduleResolution",
        kind: OptionKind::Enum,
        is_file_path: false,
        apply: |options, value| {
            // `node` is the old spelling of `node10`. Neither algorithm is
            // ported — here or upstream — but the *request* is recorded, because
            // upstream's own test harness reads it to decide whether to skip a
            // case. See `CompilerOptions::module_resolution_kind`.
            let Some(resolution) = enum_value(
                value,
                &[
                    ("classic", ModuleResolutionKind::Classic),
                    ("node", ModuleResolutionKind::Node10),
                    ("node10", ModuleResolutionKind::Node10),
                    ("node16", ModuleResolutionKind::Node16),
                    ("nodenext", ModuleResolutionKind::NodeNext),
                    ("bundler", ModuleResolutionKind::Bundler),
                ],
            ) else {
                return false;
            };
            options.module_resolution = resolution;
            true
        },
    },
    OptionDeclaration {
        name: "jsx",
        kind: OptionKind::Enum,
        is_file_path: false,
        apply: |options, value| {
            let Some(jsx) = enum_value(
                value,
                &[
                    ("none", JsxEmit::None),
                    ("preserve", JsxEmit::Preserve),
                    ("react", JsxEmit::React),
                    ("react-native", JsxEmit::ReactNative),
                    ("react-jsx", JsxEmit::ReactJsx),
                    ("react-jsxdev", JsxEmit::ReactJsxDev),
                ],
            ) else {
                return false;
            };
            options.jsx = jsx;
            true
        },
    },
    OptionDeclaration {
        name: "lib",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: false,
        apply: |options, value| {
            let Some(lib) = string_list(value) else { return false };
            options.lib = lib;
            true
        },
    },
    OptionDeclaration {
        name: "noLib",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_lib),
    },
    OptionDeclaration {
        name: "allowJs",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_js),
    },
    OptionDeclaration {
        name: "checkJs",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.check_js),
    },
    OptionDeclaration {
        name: "strict",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.strict),
    },
    OptionDeclaration {
        name: "noImplicitAny",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_implicit_any),
    },
    OptionDeclaration {
        name: "declaration",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.declaration),
    },
    OptionDeclaration {
        name: "esModuleInterop",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.es_module_interop),
    },
    OptionDeclaration {
        name: "isolatedModules",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.isolated_modules),
    },
    OptionDeclaration {
        name: "allowArbitraryExtensions",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_arbitrary_extensions),
    },
    OptionDeclaration {
        name: "allowNonTsExtensions",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_non_ts_extensions),
    },
    OptionDeclaration {
        name: "traceResolution",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.trace_resolution),
    },
    OptionDeclaration {
        name: "noResolve",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_resolve),
    },
    OptionDeclaration {
        name: "resolveJsonModule",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.resolve_json_module),
    },
    OptionDeclaration {
        name: "noDtsResolution",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_dts_resolution),
    },
    OptionDeclaration {
        name: "resolvePackageJsonExports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.resolve_package_json_exports),
    },
    OptionDeclaration {
        name: "resolvePackageJsonImports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.resolve_package_json_imports),
    },
    OptionDeclaration {
        name: "preserveSymlinks",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.preserve_symlinks),
    },
    OptionDeclaration {
        name: "maxNodeModuleJsDepth",
        kind: OptionKind::Number,
        is_file_path: false,
        apply: |options, value| {
            let ConfigValue::Number(depth) = value else { return false };
            #[expect(
                clippy::cast_possible_truncation,
                reason = "upstream stores this as an int; a fractional depth is a config error \
                          it also truncates"
            )]
            {
                options.max_node_module_js_depth = Some(*depth as i32);
            }
            true
        },
    },
    OptionDeclaration {
        name: "libReplacement",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.lib_replacement),
    },
    OptionDeclaration {
        name: "allowSyntheticDefaultImports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.allow_synthetic_default_imports)
        },
    },
    OptionDeclaration {
        name: "alwaysStrict",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.always_strict),
    },
    OptionDeclaration {
        name: "outFile",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.out_file),
    },
    OptionDeclaration {
        name: "baseUrl",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.base_url),
    },
    OptionDeclaration {
        name: "rootDir",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.root_dir),
    },
    OptionDeclaration {
        name: "outDir",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.out_dir),
    },
    OptionDeclaration {
        name: "declarationDir",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.declaration_dir),
    },
    OptionDeclaration {
        name: "jsxImportSource",
        kind: OptionKind::String,
        is_file_path: false,
        apply: |options, value| string(options, value, |o| &mut o.jsx_import_source),
    },
    OptionDeclaration {
        name: "rootDirs",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: true,
        apply: |options, value| {
            let Some(root_dirs) = string_list(value) else { return false };
            options.root_dirs = root_dirs;
            true
        },
    },
    OptionDeclaration {
        name: "typeRoots",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: true,
        apply: |options, value| {
            // `typeRoots: []` is meaningful and different from absent: it means
            // "look nowhere", which is why the field is an `Option`.
            let Some(type_roots) = string_list(value) else { return false };
            options.type_roots = Some(type_roots);
            true
        },
    },
    OptionDeclaration {
        name: "types",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: false,
        apply: |options, value| {
            // Likewise: `types: []` means "include no @types packages", where
            // absent means "include all of them".
            let Some(types) = string_list(value) else { return false };
            options.types = Some(types);
            true
        },
    },
    OptionDeclaration {
        name: "moduleSuffixes",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: false,
        apply: |options, value| {
            let Some(suffixes) = string_list(value) else { return false };
            options.module_suffixes = suffixes;
            true
        },
    },
    OptionDeclaration {
        name: "customConditions",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: false,
        apply: |options, value| {
            let Some(conditions) = string_list(value) else { return false };
            options.custom_conditions = conditions;
            true
        },
    },
    OptionDeclaration {
        name: "paths",
        kind: OptionKind::PathMap,
        is_file_path: false,
        apply: |options, value| {
            let Some(paths) = path_map(value) else { return false };
            options.paths = paths;
            true
        },
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declaration_has_a_distinct_name() {
        let mut names: Vec<&str> = COMPILER_OPTIONS.iter().map(|option| option.name).collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a duplicate name would shadow an option silently");
    }

    #[test]
    fn a_file_path_option_is_a_string_or_a_list_of_strings() {
        // `is_file_path` drives path normalisation, which only makes sense for
        // a string. A boolean marked as a path would be silently mangled.
        for option in COMPILER_OPTIONS.iter().filter(|option| option.is_file_path) {
            assert!(
                matches!(option.kind, OptionKind::String | OptionKind::List(&OptionKind::String)),
                "{} is a file path but takes {:?}",
                option.name,
                option.kind
            );
        }
    }

    #[test]
    fn an_option_of_the_wrong_shape_is_rejected_rather_than_coerced() {
        let mut options = CompilerOptions::default();
        let target = find("target").expect("target is declared");
        assert!(!(target.apply)(&mut options, &ConfigValue::Bool(true)));
        assert_eq!(options.target, ScriptTarget::None);
        assert!((target.apply)(&mut options, &ConfigValue::String("es2020".into())));
        assert_eq!(options.target, ScriptTarget::ES2020);
    }

    #[test]
    fn an_enum_value_is_matched_case_insensitively() {
        let mut options = CompilerOptions::default();
        assert!((find("module").unwrap().apply)(
            &mut options,
            &ConfigValue::String("CommonJS".into())
        ));
        assert_eq!(options.module, ModuleKind::CommonJS);
    }

    #[test]
    fn a_list_drops_its_falsy_entries() {
        let mut options = CompilerOptions::default();
        let value = ConfigValue::List(vec![
            ConfigValue::String(String::new()),
            ConfigValue::String("node".into()),
            ConfigValue::Null,
        ]);
        assert!((find("types").unwrap().apply)(&mut options, &value));
        assert_eq!(options.types, Some(vec!["node".to_string()]));
    }

    #[test]
    fn an_empty_type_roots_list_is_not_the_same_as_an_absent_one() {
        // `typeRoots: []` means "look nowhere"; absent means "walk to every
        // ancestor node_modules/@types". One corpus case turns on exactly this.
        let mut options = CompilerOptions::default();
        assert_eq!(options.type_roots, None);
        assert!((find("typeRoots").unwrap().apply)(&mut options, &ConfigValue::List(Vec::new())));
        assert_eq!(options.type_roots, Some(Vec::new()));
    }
}
