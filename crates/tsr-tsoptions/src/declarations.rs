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
    CompilerOptions, JsxEmit, ModuleDetectionKind, ModuleKind, ModuleResolutionKind, NewLineKind,
    OrderedMap, ScriptTarget, Tristate,
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

    // ---- Below here is what the *command line* needs and a config file does
    // ---- not. Added with the CLI; see STATUS-cli.md phase 1.
    /// The one-letter spelling, if the option has one
    /// (`CommandLineOption.ShortName`) — `-p` for `project`.
    pub short_name: Option<&'static str>,
    /// Whether the option may only be set in a `tsconfig.json`
    /// (`CommandLineOption.IsTSConfigOnly`).
    ///
    /// Not "ignored on the command line": writing it there is a *diagnostic*,
    /// and which diagnostic depends on the option's kind and on whether the
    /// following argument was `null`, `true` or `false`.
    pub is_tsconfig_only: bool,
    /// The accepted spellings of an [`OptionKind::Enum`], in upstream's
    /// declaration order.
    ///
    /// Only needed to *render the error*: `Argument for '--target' option must
    /// be: 'es5', 'es2015', …` lists them in this order. Matching is done by
    /// [`OptionDeclaration::apply`], which already knows the mapping, so this is
    /// not a second source of truth for what a value means — only for how the
    /// failure reads.
    pub enum_names: &'static [&'static str],
}

impl OptionDeclaration {
    /// The shape every entry starts from, so a declaration lists only what is
    /// unusual about it.
    ///
    /// `apply` refusing everything is deliberate: an entry that forgets it
    /// writes nothing and reports a type error, which is loud. The alternative
    /// default — accept and discard — would silently drop the option.
    pub const DEFAULT: Self = Self {
        name: "",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |_, _| false,
        short_name: None,
        is_tsconfig_only: false,
        enum_names: &[],
    };
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
        short_name: Some("t"),
        enum_names: &[
            "es5", "es2015", "es2016", "es2017", "es2018", "es2019", "es2020", "es2021", "es2022",
            "es2023", "es2024", "es2025", "esnext",
        ],
        ..OptionDeclaration::DEFAULT
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
        short_name: Some("m"),
        enum_names: &[
            "none", "commonjs", "amd", "system", "umd", "es6", "es2015", "es2020", "es2022",
            "esnext", "node16", "node18", "node20", "nodenext", "preserve",
        ],
        ..OptionDeclaration::DEFAULT
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
        enum_names: &["node10", "classic", "node16", "nodenext", "bundler"],
        ..OptionDeclaration::DEFAULT
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
        enum_names: &["preserve", "react-native", "react", "react-jsx", "react-jsxdev"],
        ..OptionDeclaration::DEFAULT
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
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noLib",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_lib),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowJs",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_js),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "checkJs",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.check_js),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "strict",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.strict),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noImplicitAny",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_implicit_any),
        ..OptionDeclaration::DEFAULT
    },
    // The strict family and the rest of what the checker reads. Every name here
    // is upstream's spelling from `declscompiler.go`; the lookup is
    // case-insensitive, so a `.errors.txt` case writing `@strictNullChecks` and a
    // tsconfig writing `"strictNullChecks"` reach the same entry.
    OptionDeclaration {
        name: "strictNullChecks",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.strict_null_checks),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "strictPropertyInitialization",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.strict_property_initialization),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "useUnknownInCatchVariables",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.use_unknown_in_catch_variables),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noUncheckedIndexedAccess",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_unchecked_indexed_access),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noUnusedLocals",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_unused_locals),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noUnusedParameters",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_unused_parameters),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowUnreachableCode",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_unreachable_code),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "preserveConstEnums",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.preserve_const_enums),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "verbatimModuleSyntax",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.verbatim_module_syntax),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noUncheckedSideEffectImports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.no_unchecked_side_effect_imports)
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "declaration",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.declaration),
        short_name: Some("d"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "esModuleInterop",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.es_module_interop),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "isolatedModules",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.isolated_modules),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowArbitraryExtensions",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_arbitrary_extensions),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowNonTsExtensions",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_non_ts_extensions),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "traceResolution",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.trace_resolution),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noResolve",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_resolve),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "resolveJsonModule",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.resolve_json_module),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noDtsResolution",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.no_dts_resolution),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "resolvePackageJsonExports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.resolve_package_json_exports),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "resolvePackageJsonImports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.resolve_package_json_imports),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "preserveSymlinks",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.preserve_symlinks),
        ..OptionDeclaration::DEFAULT
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
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "libReplacement",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.lib_replacement),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowSyntheticDefaultImports",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.allow_synthetic_default_imports)
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "alwaysStrict",
        kind: OptionKind::Boolean,
        is_file_path: false,
        apply: |options, value| tristate(options, value, |o| &mut o.always_strict),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "outFile",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.out_file),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "baseUrl",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.base_url),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "rootDir",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.root_dir),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "outDir",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.out_dir),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "declarationDir",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.declaration_dir),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "jsxImportSource",
        kind: OptionKind::String,
        is_file_path: false,
        apply: |options, value| string(options, value, |o| &mut o.jsx_import_source),
        ..OptionDeclaration::DEFAULT
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
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
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
        ..OptionDeclaration::DEFAULT
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
        ..OptionDeclaration::DEFAULT
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
        ..OptionDeclaration::DEFAULT
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
        ..OptionDeclaration::DEFAULT
    },
    // `declscompiler.go:1179-1186`. A language-service option: upstream declares
    // it so that `tsc` accepts it, and `core.CompilerOptions` has **no field for
    // it** — the compiler never reads the value. Declared here for the same
    // reason and stored nowhere, so a config that configures an editor plugin
    // does not draw `TS5023: Unknown compiler option 'plugins'` from a
    // type-checking run. Three hits in one 22-package monorepo.
    //
    // Upstream's kind is a bare `CommandLineOptionTypeList` with no element
    // type, so the elements are unvalidated; the real shape is
    // `[{ "name": "…", … }]`. `apply` therefore checks only that the value is a
    // list, which is what `option_type_name` will name in the diagnostic if it
    // is not.
    OptionDeclaration {
        name: "plugins",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: false,
        apply: |_, value| matches!(value, ConfigValue::List(_)),
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
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
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
    },
    // ---- The command-line surface. Upstream declares these in the same table as
    // ---- everything else (`declscompiler.go`), because `tsc -p .` and a
    // ---- `"project"` key in a config go through one parser. Most are read by
    // ---- the driver rather than by the compiler; see `tsr-execute`.
    OptionDeclaration {
        name: "help",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.help),
        short_name: Some("h"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "help",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.help),
        short_name: Some("?"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "all",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.all),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "version",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.version),
        short_name: Some("v"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "init",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.init),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "watch",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.watch),
        short_name: Some("w"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "showConfig",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.show_config),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "listFiles",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.list_files),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "listFilesOnly",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.list_files_only),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "listEmittedFiles",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.list_emitted_files),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "ignoreConfig",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.ignore_config),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noEmit",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_emit),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noEmitOnError",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_emit_on_error),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "pretty",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.pretty),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "quiet",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.quiet),
        short_name: Some("q"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noErrorTruncation",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_error_truncation),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "skipLibCheck",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.skip_lib_check),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "skipDefaultLibCheck",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.skip_default_lib_check),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "incremental",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.incremental),
        short_name: Some("i"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "composite",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.composite),
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noCheck",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_check),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "project",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.project),
        short_name: Some("p"),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "locale",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.locale),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "sourceMap",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.source_map),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "declarationMap",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.declaration_map),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "emitDeclarationOnly",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.emit_declaration_only),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "removeComments",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.remove_comments),
        ..OptionDeclaration::DEFAULT
    },
    // The remainder of upstream's table. Each is parsed into a field nothing
    // reads yet — see the note on `CompilerOptions`.
    OptionDeclaration {
        name: "preserveWatchOutput",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.preserve_watch_output),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "explainFiles",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.explain_files),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "diagnostics",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.diagnostics),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "extendedDiagnostics",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.extended_diagnostics),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "inlineSourceMap",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.inline_source_map),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "deduplicatePackages",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.deduplicate_packages),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "assumeChangesOnlyAffectDirectDependencies",
        kind: OptionKind::Boolean,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.assume_changes_only_affect_direct_dependencies)
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "singleThreaded",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.single_threaded),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "importHelpers",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.import_helpers),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "downlevelIteration",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.downlevel_iteration),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "isolatedDeclarations",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.isolated_declarations),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "erasableSyntaxOnly",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.erasable_syntax_only),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "strictFunctionTypes",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.strict_function_types),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "strictBindCallApply",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.strict_bind_call_apply),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "strictBuiltinIteratorReturn",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.strict_builtin_iterator_return),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noImplicitThis",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_implicit_this),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "stableTypeOrdering",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.stable_type_ordering),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "exactOptionalPropertyTypes",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.exact_optional_property_types),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noImplicitReturns",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_implicit_returns),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noFallthroughCasesInSwitch",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_fallthrough_cases_in_switch),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noImplicitOverride",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_implicit_override),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noPropertyAccessFromIndexSignature",
        kind: OptionKind::Boolean,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.no_property_access_from_index_signature)
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowUmdGlobalAccess",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_umd_global_access),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowImportingTsExtensions",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_importing_ts_extensions),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "rewriteRelativeImportExtensions",
        kind: OptionKind::Boolean,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.rewrite_relative_import_extensions)
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "inlineSources",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.inline_sources),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "experimentalDecorators",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.experimental_decorators),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "emitDecoratorMetadata",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.emit_decorator_metadata),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "emitBOM",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.emit_b_o_m),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "stripInternal",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.strip_internal),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "disableSizeLimit",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.disable_size_limit),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "disableSourceOfProjectReferenceRedirect",
        kind: OptionKind::Boolean,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.disable_source_of_project_reference_redirect)
        },
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "disableSolutionSearching",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.disable_solution_searching),
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "disableReferencedProjectLoad",
        kind: OptionKind::Boolean,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.disable_referenced_project_load)
        },
        is_tsconfig_only: true,
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "noEmitHelpers",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.no_emit_helpers),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "allowUnusedLabels",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.allow_unused_labels),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "forceConsistentCasingInFileNames",
        kind: OptionKind::Boolean,
        apply: |options, value| {
            tristate(options, value, |o| &mut o.force_consistent_casing_in_file_names)
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "useDefineForClassFields",
        kind: OptionKind::Boolean,
        apply: |options, value| tristate(options, value, |o| &mut o.use_define_for_class_fields),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "generateCpuProfile",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.generate_cpu_profile),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "generateTrace",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.generate_trace),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "pprofDir",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.pprof_dir),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "tsBuildInfoFile",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.ts_build_info_file),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "sourceRoot",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.source_root),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "mapRoot",
        kind: OptionKind::String,
        is_file_path: true,
        apply: |options, value| string(options, value, |o| &mut o.map_root),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "jsxFactory",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.jsx_factory),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "jsxFragmentFactory",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.jsx_fragment_factory),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "reactNamespace",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.react_namespace),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "ignoreDeprecations",
        kind: OptionKind::String,
        apply: |options, value| string(options, value, |o| &mut o.ignore_deprecations),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "newLine",
        kind: OptionKind::Enum,
        enum_names: &["crlf", "lf"],
        apply: |options, value| {
            let Some(kind) = enum_value(
                value,
                &[("crlf", NewLineKind::CarriageReturnLineFeed), ("lf", NewLineKind::LineFeed)],
            ) else {
                return false;
            };
            options.new_line = kind;
            true
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "moduleDetection",
        kind: OptionKind::Enum,
        enum_names: &["auto", "legacy", "force"],
        apply: |options, value| {
            let Some(kind) = enum_value(
                value,
                &[
                    ("auto", ModuleDetectionKind::Auto),
                    ("legacy", ModuleDetectionKind::Legacy),
                    ("force", ModuleDetectionKind::Force),
                ],
            ) else {
                return false;
            };
            options.module_detection = kind;
            true
        },
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "checkers",
        kind: OptionKind::Number,
        apply: |options, value| {
            let ConfigValue::Number(count) = value else { return false };
            #[allow(clippy::cast_possible_truncation)]
            {
                options.checkers = Some(*count as i32);
            }
            true
        },
        ..OptionDeclaration::DEFAULT
    },
];

/// The `watchOptions` table (`tsoptions.OptionsForWatch`).
///
/// # Why these exist when watch mode does not
///
/// `parseStrings` consults this table when a name misses the compiler one
/// (`commandlineparser.go:150`), so upstream *accepts* `--watchFile` on any
/// command line, watch mode or not. Without these entries this port reported
/// `Unknown compiler option '--watchFile'` for a flag `tsc` takes happily —
/// a difference a user would hit before they hit anything watch-related.
///
/// Every entry parses its value and **stores nothing**: there is no
/// `WatchOptions` struct here because nothing would read it. That is the
/// deliberate half of this — an accepted-and-discarded option is honest about a
/// feature that does not exist, where an unknown-option error is wrong about the
/// command line. When watch mode lands, these grow setters and a struct to
/// write into.
pub static WATCH_OPTIONS: &[OptionDeclaration] = &[
    OptionDeclaration {
        name: "watchInterval",
        kind: OptionKind::Number,
        apply: |_, value| matches!(value, ConfigValue::Number(_)),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "watchFile",
        kind: OptionKind::Enum,
        enum_names: &[
            "fixedpollinginterval",
            "prioritypollinginterval",
            "dynamicprioritypolling",
            "fixedchunksizepolling",
            "usefsevents",
            "usefseventsonparentdirectory",
        ],
        apply: |_, value| value.as_str().is_some(),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "watchDirectory",
        kind: OptionKind::Enum,
        enum_names: &[
            "usefsevents",
            "fixedpollinginterval",
            "dynamicprioritypolling",
            "fixedchunksizepolling",
        ],
        apply: |_, value| value.as_str().is_some(),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "fallbackPolling",
        kind: OptionKind::Enum,
        enum_names: &["fixedinterval", "priorityinterval", "dynamicpriority", "fixedchunksize"],
        apply: |_, value| value.as_str().is_some(),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "synchronousWatchDirectory",
        kind: OptionKind::Boolean,
        apply: |_, value| matches!(value, ConfigValue::Bool(_)),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "excludeDirectories",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: true,
        apply: |_, value| matches!(value, ConfigValue::List(_)),
        ..OptionDeclaration::DEFAULT
    },
    OptionDeclaration {
        name: "excludeFiles",
        kind: OptionKind::List(&OptionKind::String),
        is_file_path: true,
        apply: |_, value| matches!(value, ConfigValue::List(_)),
        ..OptionDeclaration::DEFAULT
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declaration_has_a_distinct_name_except_the_one_upstream_repeats() {
        // **`help` is declared twice, and that is upstream's shape rather than a
        // slip here.** `CommandLineOption.ShortName` is a single field, so `-h`
        // and `-?` cannot both hang off one entry; upstream writes two
        // declarations (`declscompiler.go:17` and `:27`) and the help printer
        // walks the table, printing `--help, -h` and `--help, -?` as separate
        // lines. Collapsing them would change baselined output.
        //
        // Any *other* duplicate shadows an option silently, which is what this
        // still guards.
        let mut names: Vec<&str> = COMPILER_OPTIONS
            .iter()
            .map(|option| option.name)
            .filter(|name| *name != "help")
            .collect();
        let count = names.len();
        names.sort_unstable();
        names.dedup();
        assert_eq!(names.len(), count, "a duplicate name would shadow an option silently");
        assert_eq!(
            COMPILER_OPTIONS.iter().filter(|option| option.name == "help").count(),
            2,
            "help is declared once per short name"
        );
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
