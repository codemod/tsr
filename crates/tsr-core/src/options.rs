//! Compiler options.
//!
//! Ported from `internal/core/compileroptions.go` and `internal/core/tristate.go`
//! at the pinned commit.
//!
//! # Why this is in `tsr-core` and not `tsr-tsoptions`
//!
//! Upstream splits the same way, and the split is load-bearing:
//! `internal/core.CompilerOptions` is a plain struct, while `internal/tsoptions`
//! (8,706 lines) is the tsconfig.json parser and validator that *produces* one.
//! `internal/compiler/program.go` imports the first and not the second. So the
//! `Program` needs the struct, and the parser can arrive later — which is what
//! [ADR-0017](../../../docs/adr/0017-program-before-tsconfig.md) decides.
//!
//! # Why the struct is incomplete
//!
//! Upstream's has around 130 fields. This has the ones something reads, and it
//! grows when something needs a field to read. The corpus sets options with
//! `// @target:`-style directives, and their frequency across the 12,444 cases is
//! the guide: `target` 12,423, `strict` 3,319, `module` 2,306, `declaration`
//! 1,447, `allowJs` 880, `checkJs` 727, `lib` 590, `jsx` 377. A field added
//! speculatively is a field with no test.

use std::fmt;

/// A three-valued option: unset, off, or on.
///
/// Upstream's `core.Tristate`. The third state is not pedantry — `strict`
/// turns on a family of flags, and "the user did not say" has to be
/// distinguishable from "the user said no" for that to work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Tristate {
    /// Not specified.
    #[default]
    Unknown,
    /// Specified as false.
    False,
    /// Specified as true.
    True,
}

impl Tristate {
    /// Whether the option is on (`Tristate.IsTrue`).
    #[must_use]
    pub fn is_true(self) -> bool {
        self == Self::True
    }

    /// Whether the option is on or unset (`Tristate.IsTrueOrUnknown`).
    #[must_use]
    pub fn is_true_or_unknown(self) -> bool {
        matches!(self, Self::True | Self::Unknown)
    }

    /// Whether the option was left unset (`Tristate.IsUnknown`).
    #[must_use]
    pub fn is_unknown(self) -> bool {
        self == Self::Unknown
    }

    /// Whether the option was explicitly turned off.
    #[must_use]
    pub fn is_false(self) -> bool {
        self == Self::False
    }

    /// This option if it was set, else `fallback`
    /// (`Tristate.DefaultIfUnknown`).
    ///
    /// How the strict family works: `noImplicitAny` unset means "whatever
    /// `strict` says", which is why [`Tristate`] needs a third state at all.
    #[must_use]
    pub fn default_if_unknown(self, fallback: Self) -> Self {
        if self == Self::Unknown { fallback } else { self }
    }

    /// From a `true`/`false` directive value.
    #[must_use]
    pub fn from_bool(value: bool) -> Self {
        if value { Self::True } else { Self::False }
    }
}

/// The ECMAScript version emitted for (`core.ScriptTarget`).
///
/// The discriminants are upstream's, and they are ordered: comparisons like
/// `target >= ES2022` decide whether class fields use `[[Define]]` semantics, so
/// the order is behaviour rather than presentation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum ScriptTarget {
    /// Unset.
    #[default]
    None = 0,
    /// ES5, and the target `es3` now maps onto.
    ES5 = 1,
    /// ES2015, also spelled `es6`.
    ES2015 = 2,
    /// ES2016.
    ES2016 = 3,
    /// ES2017.
    ES2017 = 4,
    /// ES2018.
    ES2018 = 5,
    /// ES2019.
    ES2019 = 6,
    /// ES2020.
    ES2020 = 7,
    /// ES2021.
    ES2021 = 8,
    /// ES2022, where class fields gain `[[Define]]` semantics.
    ES2022 = 9,
    /// ES2023.
    ES2023 = 10,
    /// ES2024.
    ES2024 = 11,
    /// ES2025.
    ES2025 = 12,
    /// The moving target.
    /// The moving target.
    ESNext = 99,
}

impl ScriptTarget {
    /// Parse a `// @target:` directive value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "es3" | "es5" => Self::ES5,
            "es6" | "es2015" => Self::ES2015,
            "es2016" => Self::ES2016,
            "es2017" => Self::ES2017,
            "es2018" => Self::ES2018,
            "es2019" => Self::ES2019,
            "es2020" => Self::ES2020,
            "es2021" => Self::ES2021,
            "es2022" => Self::ES2022,
            "es2023" => Self::ES2023,
            "es2024" => Self::ES2024,
            "es2025" => Self::ES2025,
            "esnext" | "latest" => Self::ESNext,
            _ => return None,
        })
    }
}

/// The module system emitted for (`core.ModuleKind`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
#[repr(u8)]
pub enum ModuleKind {
    /// Unset.
    #[default]
    None = 0,
    /// `require`/`module.exports`.
    CommonJS = 1,
    /// AMD `define`.
    AMD = 2,
    /// UMD.
    UMD = 3,
    /// `SystemJS`.
    System = 4,
    /// ES modules, also spelled `es6`.
    ES2015 = 5,
    /// ES modules with dynamic `import`.
    ES2020 = 6,
    /// ES modules with top-level `await`.
    ES2022 = 7,
    /// The moving target.
    ESNext = 99,
    /// Node's own resolution and format detection, by Node version.
    /// Node 16's dual-format resolution.
    Node16 = 100,
    /// Node 18's.
    Node18 = 101,
    /// Node 20's.
    Node20 = 102,
    /// Whatever Node does next.
    NodeNext = 199,
    /// Emit-agnostic: the bundler decides.
    Preserve = 200,
}

impl ModuleKind {
    /// Parse a `// @module:` directive value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "commonjs" => Self::CommonJS,
            "amd" => Self::AMD,
            "umd" => Self::UMD,
            "system" => Self::System,
            "es6" | "es2015" => Self::ES2015,
            "es2020" => Self::ES2020,
            "es2022" => Self::ES2022,
            "esnext" => Self::ESNext,
            "node16" => Self::Node16,
            "node18" => Self::Node18,
            "node20" => Self::Node20,
            "nodenext" => Self::NodeNext,
            "preserve" => Self::Preserve,
            _ => return None,
        })
    }
}

/// How `import` specifiers are resolved (`core.ModuleResolutionKind`).
///
/// Ordered, and the order is upstream's numbering rather than presentation:
/// `Node16 <= kind && kind <= NodeNext` is how the compiler asks "does this
/// resolution mode split `CommonJS` from ESM", and it must exclude `Bundler`
/// (upstream `Node16 = 3`, `NodeNext = 99`, `Bundler = 100`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Default)]
pub enum ModuleResolutionKind {
    /// Unset; derived from [`CompilerOptions::module`].
    #[default]
    Unknown,
    /// Pre-Node resolution, still reachable for `module: none`.
    Classic,
    /// The original Node algorithm (`--moduleResolution node`).
    Node10,
    /// Node 16's dual-format resolution.
    Node16,
    /// Whatever Node does next.
    NodeNext,
    /// A bundler's resolution: `exports` without the format split.
    Bundler,
}

/// What JSX compiles to (`core.JsxEmit`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum JsxEmit {
    /// Unset.
    #[default]
    None,
    /// JSX is left in the output for another tool.
    Preserve,
    /// `React.createElement` calls.
    React,
    /// Preserved, but emitted to `.js`.
    ReactNative,
    /// The automatic runtime.
    ReactJsx,
    /// The automatic runtime, development build.
    ReactJsxDev,
}

impl JsxEmit {
    /// Parse a `// @jsx:` directive value.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value.to_ascii_lowercase().as_str() {
            "none" => Self::None,
            "preserve" => Self::Preserve,
            "react" => Self::React,
            "react-native" => Self::ReactNative,
            "react-jsx" => Self::ReactJsx,
            "react-jsxdev" => Self::ReactJsxDev,
            _ => return None,
        })
    }
}

/// Which format a file is being resolved *as* (`core.ResolutionMode`).
///
/// Upstream reuses `ModuleKind` for this and only ever stores three of its
/// values, so this is an alias rather than a new enum: `None` (unknown, which
/// `bundler` reads as ESM), `CommonJS`, and `ESNext`.
pub type ResolutionMode = ModuleKind;

/// An insertion-ordered string map (`collections.OrderedMap`).
///
/// `paths` and `typesVersions` are iterated in *declaration order* during
/// resolution, and the trace records each substitution as it is tried — so a
/// `HashMap` here would make the oracle unmatchable, not merely untidy. Small by
/// construction (a handful of patterns), so a `Vec` scan is the right shape.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderedMap<V> {
    entries: Vec<(String, V)>,
}

impl<V> Default for OrderedMap<V> {
    /// An empty map.
    ///
    /// Hand-written rather than derived: `derive(Default)` would demand
    /// `V: Default`, which the value type of a config map has no reason to be.
    fn default() -> Self {
        Self::new()
    }
}

impl<V> OrderedMap<V> {
    /// An empty map.
    #[must_use]
    pub fn new() -> Self {
        Self { entries: Vec::new() }
    }

    /// Append or replace `key`. Replacing keeps the original position.
    pub fn set(&mut self, key: impl Into<String>, value: V) {
        let key = key.into();
        if let Some(existing) = self.entries.iter_mut().find(|(k, _)| *k == key) {
            existing.1 = value;
        } else {
            self.entries.push((key, value));
        }
    }

    /// The value for `key`.
    #[must_use]
    pub fn get(&self, key: &str) -> Option<&V> {
        self.entries.iter().find(|(k, _)| k == key).map(|(_, v)| v)
    }

    /// Whether `key` is present.
    #[must_use]
    pub fn contains_key(&self, key: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == key)
    }

    /// Remove `key`, keeping the order of the rest.
    ///
    /// Wildcard file expansion needs it: a file found later can displace one
    /// found earlier when it has a higher-priority extension.
    pub fn remove(&mut self, key: &str) {
        self.entries.retain(|(existing, _)| existing != key);
    }

    /// The values, in insertion order, consuming the map.
    pub fn into_values(self) -> impl Iterator<Item = V> {
        self.entries.into_iter().map(|(_, value)| value)
    }

    /// How many entries.
    #[must_use]
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether there are no entries.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// The keys, in insertion order.
    pub fn keys(&self) -> impl Iterator<Item = &str> {
        self.entries.iter().map(|(k, _)| k.as_str())
    }

    /// The entries, in insertion order.
    pub fn entries(&self) -> impl Iterator<Item = (&str, &V)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v))
    }
}

impl<V> FromIterator<(String, V)> for OrderedMap<V> {
    fn from_iter<T: IntoIterator<Item = (String, V)>>(iter: T) -> Self {
        let mut map = Self::new();
        for (key, value) in iter {
            map.set(key, value);
        }
        map
    }
}

/// The options a compilation runs under (`core.CompilerOptions`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CompilerOptions {
    /// The ECMAScript version emitted for.
    pub target: ScriptTarget,
    /// The module system emitted for.
    pub module: ModuleKind,
    /// How specifiers are resolved.
    pub module_resolution: ModuleResolutionKind,
    /// What JSX compiles to.
    pub jsx: JsxEmit,
    /// Explicit `lib` entries. Empty means "the default for the target".
    pub lib: Vec<String>,
    /// Load no default library at all.
    pub no_lib: Tristate,
    /// Include `.js` files in the program.
    pub allow_js: Tristate,
    /// Report type errors in `.js` files.
    pub check_js: Tristate,
    /// The umbrella flag turning on the strict family.
    pub strict: Tristate,
    /// Emit `.d.ts` alongside the output.
    pub declaration: Tristate,
    /// Emit helpers for `CommonJS` default imports.
    pub es_module_interop: Tristate,
    /// Require every file to be transpilable alone.
    pub isolated_modules: Tristate,
    /// Permit importing files with unknown extensions.
    pub allow_arbitrary_extensions: Tristate,

    // ---- Module resolution. Added by Phase 3 slice 2; every field below is one
    // ---- some `.trace.json` baseline exercises.
    /// Log every resolution step. The `.trace.json` oracle exists because of it.
    pub trace_resolution: Tristate,
    /// Do not follow imports into new files at all.
    pub no_resolve: Tristate,
    /// Path mapping patterns, in declaration order.
    pub paths: OrderedMap<Vec<String>>,
    /// What `paths` substitutions are resolved against; the config's directory.
    pub paths_base_path: String,
    /// Legacy non-relative resolution root. Present only so the harness can see
    /// it: upstream *skips* every test that sets it, so no baseline exercises it.
    pub base_url: String,
    /// Explicit `@types` roots. `None` means "walk to every ancestor
    /// `node_modules/@types`", which is a different thing from an empty list.
    pub type_roots: Option<Vec<String>>,
    /// Type packages to include automatically. `None` means "all of them".
    pub types: Option<Vec<String>>,
    /// Virtual source roots a relative specifier may be resolved through.
    pub root_dirs: Vec<String>,
    /// The declared source root.
    pub root_dir: String,
    /// Suffixes tried before the bare file name, e.g. `.ios`.
    pub module_suffixes: Vec<String>,
    /// Extra `exports`/`imports` conditions to match.
    pub custom_conditions: Vec<String>,
    /// Allow importing `.json`.
    pub resolve_json_module: Tristate,
    /// Resolve to implementation files rather than declarations.
    pub no_dts_resolution: Tristate,
    /// Honour `package.json` `exports`.
    pub resolve_package_json_exports: Tristate,
    /// Honour `package.json` `imports`.
    pub resolve_package_json_imports: Tristate,
    /// Report the symlink rather than its target.
    pub preserve_symlinks: Tristate,
    /// Where output goes; consulted to map an output path back to its input.
    pub out_dir: String,
    /// Where declarations go; likewise.
    pub declaration_dir: String,
    /// The tsconfig this came from, if any.
    pub config_file_path: String,

    // ---- The file loader. Added by Phase 3 slice 3; each is read while
    // ---- deciding which files enter the program and how they are resolved.
    /// Report an implicit `any`. Read by the loader only to decide whether a
    /// `.js` resolution without types is an error and so excluded from the
    /// program (`module.GetResolutionDiagnostic`).
    pub no_implicit_any: Tristate,
    /// The package the automatic JSX runtime is imported from; `react` when
    /// unset and `jsx` names an automatic runtime.
    pub jsx_import_source: String,
    /// Load files whose extension TypeScript does not recognise.
    pub allow_non_ts_extensions: Tristate,
    /// Resolve each lib file's name as an `@typescript/lib-*` module instead of
    /// loading the bundled one.
    pub lib_replacement: Tristate,
    /// Synthesise a default import for a `CommonJS` module without one.
    pub allow_synthetic_default_imports: Tristate,
    /// Emit `"use strict"` in every file.
    pub always_strict: Tristate,
    /// Concatenate the output into one file. Present because upstream's test
    /// harness skips every case that sets it; nothing here emits.
    pub out_file: String,
    /// How far into `node_modules` a JavaScript file's imports are followed.
    /// `None` is upstream's zero, which is what makes a `.js` dependency's own
    /// imports invisible by default.
    pub max_node_module_js_depth: Option<i32>,

    // ---- The checker. Every field below is read by `Checker`, and each one was
    // ---- previously derived by each caller from its own raw string map; see
    // ---- `CompilerOptions::configure` on the checker side and
    // ---- docs/adr/0042-checker-options-come-from-compiler-options.md.
    /// Treat `null` and `undefined` as distinct from every other type.
    /// A **strict-family** option: see [`Self::strict_option_value`].
    pub strict_null_checks: Tristate,
    /// Require every declared class property to be definitely assigned.
    /// Strict-family.
    pub strict_property_initialization: Tristate,
    /// Give an un-annotated `catch (e)` the type `unknown` rather than `any`.
    /// Strict-family.
    pub use_unknown_in_catch_variables: Tristate,
    /// Add `undefined` to the result of an index signature access.
    ///
    /// Read as `== TSTrue` upstream (`checker.go:6115`), **not** as a
    /// strict-family option — `strict` does not turn it on.
    pub no_unchecked_indexed_access: Tristate,
    /// Report a local that is never read.
    pub no_unused_locals: Tristate,
    /// Report a parameter that is never read.
    pub no_unused_parameters: Tristate,
    /// Permit code the control-flow graph proves unreachable.
    ///
    /// Three-valued in a way that matters and is easy to get wrong: unset makes
    /// unreachable code a *suggestion*, which never reaches a `.errors.txt`,
    /// while an explicit `false` makes it an error. So "is it an error" is
    /// `is_false()` and **not** the negation of "is it allowed".
    pub allow_unreachable_code: Tristate,
    /// Emit `const enum` declarations rather than erasing them.
    pub preserve_const_enums: Tristate,
    /// Require every import to be resolvable as written, with no elision.
    /// Turns on `isolatedModules` behaviour (`GetIsolatedModules`).
    pub verbatim_module_syntax: Tristate,
    /// Check a side-effect-only `import "x"` resolves.
    ///
    /// Reads as **on when unset** (`IsTrueOrUnknown`), which is why only an
    /// explicit `false` silences it.
    pub no_unchecked_side_effect_imports: Tristate,
}

impl CompilerOptions {
    /// The target actually emitted for (`GetEmitScriptTarget`).
    ///
    /// Unset does not mean "none": it means ES5, unless the module kind implies
    /// something newer. That default is why so much of the corpus sets `target`
    /// explicitly.
    #[must_use]
    pub fn emit_script_target(&self) -> ScriptTarget {
        if self.target != ScriptTarget::None {
            return self.target;
        }
        match self.module {
            ModuleKind::Node16 | ModuleKind::Node18 => ScriptTarget::ES2022,
            ModuleKind::Node20 => ScriptTarget::ES2023,
            ModuleKind::NodeNext => ScriptTarget::ESNext,
            _ => ScriptTarget::ES5,
        }
    }

    /// The module kind actually emitted (`GetEmitModuleKind`).
    #[must_use]
    pub fn emit_module_kind(&self) -> ModuleKind {
        if self.module != ModuleKind::None {
            return self.module;
        }
        if self.emit_script_target() >= ScriptTarget::ES2015 {
            ModuleKind::ES2015
        } else {
            ModuleKind::CommonJS
        }
    }

    /// How specifiers resolve (`GetModuleResolutionKind`).
    ///
    /// **`node10` and `classic` are not outcomes of this function.** Upstream
    /// has not ported either algorithm, so both — and "unspecified" — fall
    /// through to `bundler` unless the module kind names a Node mode. An earlier
    /// version of this port had an extra arm returning `Node10` for the
    /// unspecified case; it does not exist in
    /// `core.CompilerOptions.GetModuleResolutionKind`, and it made 54 baselined
    /// corpus cases derive an unimplemented kind. Found by the
    /// `module_resolution` conformance suite, which is what a strict oracle is
    /// for.
    ///
    /// A `node10`/`classic` *request* survives in the raw
    /// [`CompilerOptions::module_resolution`] field, which is what upstream's
    /// test harness reads to decide whether to skip a case.
    #[must_use]
    pub fn module_resolution_kind(&self) -> ModuleResolutionKind {
        match self.module_resolution {
            ModuleResolutionKind::Unknown
            | ModuleResolutionKind::Classic
            | ModuleResolutionKind::Node10 => match self.emit_module_kind() {
                ModuleKind::Node16 | ModuleKind::Node18 | ModuleKind::Node20 => {
                    ModuleResolutionKind::Node16
                }
                ModuleKind::NodeNext => ModuleResolutionKind::NodeNext,
                _ => ModuleResolutionKind::Bundler,
            },
            explicit => explicit,
        }
    }

    /// Whether `.json` imports resolve (`GetResolveJsonModule`).
    ///
    /// Unset does not mean off: `nodenext`/`node20` and every bundler resolution
    /// turn it on, which is why so many traces list JSON among the target file
    /// types without the case mentioning `resolveJsonModule`.
    #[must_use]
    pub fn get_resolve_json_module(&self) -> bool {
        if self.resolve_json_module != Tristate::Unknown {
            return self.resolve_json_module == Tristate::True;
        }
        if matches!(self.emit_module_kind(), ModuleKind::Node20 | ModuleKind::NodeNext) {
            return true;
        }
        self.module_resolution_kind() == ModuleResolutionKind::Bundler
    }

    /// Whether `package.json` `exports` are honoured
    /// (`GetResolvePackageJsonExports`).
    ///
    /// Unset means *on*, which is worth stating because it makes
    /// `importSyntaxAffectsModuleResolution` true under every default — so a
    /// specifier's syntax decides its resolution mode even under `bundler`.
    #[must_use]
    pub fn get_resolve_package_json_exports(&self) -> bool {
        self.resolve_package_json_exports.is_true_or_unknown()
    }

    /// Whether `package.json` `imports` are honoured
    /// (`GetResolvePackageJsonImports`).
    #[must_use]
    pub fn get_resolve_package_json_imports(&self) -> bool {
        self.resolve_package_json_imports.is_true_or_unknown()
    }

    /// Whether `.js` files enter the program (`GetAllowJS`).
    #[must_use]
    pub fn get_allow_js(&self) -> bool {
        if self.allow_js != Tristate::Unknown {
            return self.allow_js == Tristate::True;
        }
        self.check_js == Tristate::True
    }

    /// Where `paths` substitutions are resolved from (`GetPathsBasePath`).
    #[must_use]
    pub fn get_paths_base_path(&self, current_directory: &str) -> String {
        if self.paths.is_empty() {
            return String::new();
        }
        if !self.paths_base_path.is_empty() {
            return self.paths_base_path.clone();
        }
        current_directory.to_string()
    }

    /// The `@types` roots, and whether they were configured
    /// (`GetEffectiveTypeRoots`).
    ///
    /// The second value is not cosmetic: a *configured* `typeRoots` makes
    /// resolution try file-or-directory in each root and skip the `node_modules`
    /// fallback, while the derived list does neither.
    ///
    /// # Panics
    ///
    /// If there is neither a config file path nor a current directory to walk
    /// from — upstream panics on the same condition, because the answer would
    /// otherwise be silently empty.
    #[must_use]
    pub fn get_effective_type_roots(&self, current_directory: &str) -> (Vec<String>, bool) {
        if let Some(roots) = &self.type_roots {
            return (roots.clone(), true);
        }
        let base_dir = if self.config_file_path.is_empty() {
            assert!(
                !current_directory.is_empty(),
                "cannot get effective type roots without a config file path or current directory"
            );
            current_directory.to_string()
        } else {
            tsr_path::get_directory_path(&self.config_file_path).to_string()
        };

        let mut roots = Vec::new();
        tsr_path::for_each_ancestor_directory::<()>(&base_dir, |dir| {
            roots.push(tsr_path::combine_paths(dir, &["node_modules", "@types"]));
            None
        });
        (roots, false)
    }

    /// Resolve one member of the strict family (`GetStrictOptionValue`).
    ///
    /// Ported from `internal/core/compileroptions.go:294`. The rule is three
    /// steps and the third is the surprising one: an explicit value wins;
    /// otherwise `strict` decides; and an **unset `strict` counts as on**,
    /// because upstream asks `options.Strict != TSFalse` rather than
    /// `options.Strict == TSTrue`.
    ///
    /// That last step is why a file compiled with no options at all is checked
    /// under `strictNullChecks`, `noImplicitAny`, `strictPropertyInitialization`
    /// and `useUnknownInCatchVariables`. Two callers in this repository derived
    /// the rule independently and disagreed about it for eleven sessions
    /// (`docs/architecture/checker-notes-diag2.md` §80); having one function is
    /// the point of it existing.
    #[must_use]
    pub fn strict_option_value(&self, value: Tristate) -> bool {
        if value.is_unknown() { !self.strict.is_false() } else { value.is_true() }
    }

    /// Whether every file must stand alone (`GetIsolatedModules`).
    ///
    /// `internal/core/compileroptions.go:330`. `verbatimModuleSyntax` implies it,
    /// which is the leg a hand-written derivation reliably forgets.
    #[must_use]
    pub fn get_isolated_modules(&self) -> bool {
        self.isolated_modules.is_true() || self.verbatim_module_syntax.is_true()
    }

    /// Whether `const enum` members survive emit (`ShouldPreserveConstEnums`).
    ///
    /// `internal/core/compileroptions.go:278`. An `IsTrue()` option rather than a
    /// strict-family one, so unset is **off** — the opposite default from
    /// [`Self::strict_option_value`], and the two are easy to conflate.
    #[must_use]
    pub fn should_preserve_const_enums(&self) -> bool {
        self.preserve_const_enums.is_true() || self.get_isolated_modules()
    }

    /// Whether `types` contains `*` (`UsesWildcardTypes`).
    #[must_use]
    pub fn uses_wildcard_types(&self) -> bool {
        self.types.as_ref().is_some_and(|types| types.iter().any(|t| t == "*"))
    }

    /// The default library for this target (`tsoptions.GetDefaultLibFileName`).
    ///
    /// A lookup table rather than the tsconfig layer it lives in upstream; see
    /// the module docs.
    #[must_use]
    pub fn default_lib_file_name(&self) -> &'static str {
        match self.emit_script_target() {
            ScriptTarget::ESNext => "lib.esnext.full.d.ts",
            ScriptTarget::ES2025 => "lib.es2025.full.d.ts",
            ScriptTarget::ES2024 => "lib.es2024.full.d.ts",
            ScriptTarget::ES2023 => "lib.es2023.full.d.ts",
            ScriptTarget::ES2022 => "lib.es2022.full.d.ts",
            ScriptTarget::ES2021 => "lib.es2021.full.d.ts",
            ScriptTarget::ES2020 => "lib.es2020.full.d.ts",
            ScriptTarget::ES2019 => "lib.es2019.full.d.ts",
            ScriptTarget::ES2018 => "lib.es2018.full.d.ts",
            ScriptTarget::ES2017 => "lib.es2017.full.d.ts",
            ScriptTarget::ES2016 => "lib.es2016.full.d.ts",
            ScriptTarget::ES2015 => "lib.es6.d.ts",
            ScriptTarget::ES5 | ScriptTarget::None => "lib.d.ts",
        }
    }
}

impl fmt::Display for ModuleResolutionKind {
    /// The spelling the resolution trace uses
    /// (`core.ModuleResolutionKind.String()`).
    ///
    /// This text is compared against committed baselines, so it is behaviour.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Unknown => "Unknown",
            Self::Classic => "Classic",
            Self::Node10 => "Node10",
            Self::Node16 => "Node16",
            Self::NodeNext => "NodeNext",
            Self::Bundler => "Bundler",
        })
    }
}

impl fmt::Display for ScriptTarget {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unset_target_is_es5_unless_the_module_kind_says_otherwise() {
        // The default that makes `// @target:` the corpus's most common directive.
        assert_eq!(CompilerOptions::default().emit_script_target(), ScriptTarget::ES5);

        let node16 = CompilerOptions { module: ModuleKind::Node16, ..Default::default() };
        assert_eq!(node16.emit_script_target(), ScriptTarget::ES2022);

        let explicit = CompilerOptions { target: ScriptTarget::ES2020, ..Default::default() };
        assert_eq!(explicit.emit_script_target(), ScriptTarget::ES2020);
    }

    #[test]
    fn an_unset_module_kind_follows_the_target() {
        assert_eq!(CompilerOptions::default().emit_module_kind(), ModuleKind::CommonJS);
        let modern = CompilerOptions { target: ScriptTarget::ES2015, ..Default::default() };
        assert_eq!(modern.emit_module_kind(), ModuleKind::ES2015);
    }

    #[test]
    fn module_resolution_follows_the_module_kind_when_unset() {
        let node16 = CompilerOptions { module: ModuleKind::Node16, ..Default::default() };
        assert_eq!(node16.module_resolution_kind(), ModuleResolutionKind::Node16);

        let preserve = CompilerOptions { module: ModuleKind::Preserve, ..Default::default() };
        assert_eq!(preserve.module_resolution_kind(), ModuleResolutionKind::Bundler);

        // Unspecified is *bundler*, not node10: upstream never returns node10
        // from this function because it has not ported that algorithm.
        assert_eq!(
            CompilerOptions::default().module_resolution_kind(),
            ModuleResolutionKind::Bundler
        );
        // Even an explicit `node10` request derives to bundler; the request
        // survives only in the raw field, which is what decides whether a
        // conformance case is judgeable at all.
        let node10 = CompilerOptions {
            module_resolution: ModuleResolutionKind::Node10,
            ..Default::default()
        };
        assert_eq!(node10.module_resolution_kind(), ModuleResolutionKind::Bundler);
        assert_eq!(node10.module_resolution, ModuleResolutionKind::Node10);
    }

    #[test]
    fn a_tristate_distinguishes_unset_from_off() {
        assert!(Tristate::Unknown.is_true_or_unknown());
        assert!(!Tristate::Unknown.is_true());
        assert!(!Tristate::False.is_true_or_unknown());
        assert!(Tristate::from_bool(true).is_true());
    }

    #[test]
    fn directive_values_parse_the_way_the_corpus_writes_them() {
        assert_eq!(ScriptTarget::parse("ES2015"), Some(ScriptTarget::ES2015));
        assert_eq!(ScriptTarget::parse("es6"), Some(ScriptTarget::ES2015));
        assert_eq!(ScriptTarget::parse("esnext"), Some(ScriptTarget::ESNext));
        // ES3 was removed as a target and maps onto ES5 rather than failing.
        assert_eq!(ScriptTarget::parse("es3"), Some(ScriptTarget::ES5));
        assert_eq!(ScriptTarget::parse("nonsense"), None);
        assert_eq!(ModuleKind::parse("commonjs"), Some(ModuleKind::CommonJS));
        assert_eq!(JsxEmit::parse("react-jsx"), Some(JsxEmit::ReactJsx));
    }
}
