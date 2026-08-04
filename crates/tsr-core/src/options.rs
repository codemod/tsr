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

    /// Whether the option was explicitly turned off.
    #[must_use]
    pub fn is_false(self) -> bool {
        self == Self::False
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
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
                ModuleKind::Preserve => ModuleResolutionKind::Bundler,
                _ if self.module_resolution == ModuleResolutionKind::Unknown => {
                    ModuleResolutionKind::Node10
                }
                _ => self.module_resolution,
            },
            explicit => explicit,
        }
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

        assert_eq!(
            CompilerOptions::default().module_resolution_kind(),
            ModuleResolutionKind::Node10
        );
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
