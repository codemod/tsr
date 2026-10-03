//! What a resolution asks for, and what it produces.
//!
//! Ported from `internal/module/types.go` at the pinned commit.

use std::fmt;

use tsr_core::{CompilerOptions, ResolutionMode};
use tsr_path::extension::{
    SUPPORTED_DECLARATION_EXTENSIONS, SUPPORTED_JS_EXTENSIONS_FLAT,
    SUPPORTED_TS_IMPLEMENTATION_EXTENSIONS,
};

use crate::messages::Trace;

/// What the resolver is allowed to reach through (`module.ResolutionHost`).
pub trait ResolutionHost {
    /// The file system to probe.
    fn fs(&self) -> &dyn tsr_vfs::FileSystem;
    /// What a relative path is resolved against.
    fn current_directory(&self) -> &str;
}

/// Which kinds of file a lookup will accept (`module.extensions`).
///
/// A bit set rather than a list because the two `node_modules` passes are defined
/// by *subtracting* one group from another — TypeScript and declarations first,
/// everything else second — and because [`Extensions::to_string`] renders the set
/// into a trace line whose exact wording is baselined.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Extensions(u8);

impl Extensions {
    /// `.ts`, `.tsx`, `.mts`, `.cts`.
    pub const TYPESCRIPT: Self = Self(1 << 0);
    /// `.js`, `.jsx`, `.mjs`, `.cjs`.
    pub const JAVASCRIPT: Self = Self(1 << 1);
    /// `.d.ts`, `.d.mts`, `.d.cts`.
    pub const DECLARATION: Self = Self(1 << 2);
    /// `.json`.
    pub const JSON: Self = Self(1 << 3);
    /// Nothing.
    pub const NONE: Self = Self(0);
    /// TypeScript and JavaScript, but not declarations
    /// (`extensionsImplementationFiles`).
    pub const IMPLEMENTATION_FILES: Self = Self(Self::TYPESCRIPT.0 | Self::JAVASCRIPT.0);

    /// Whether every bit of `other` is set.
    #[must_use]
    pub const fn contains(self, other: Self) -> bool {
        self.0 & other.0 == other.0
    }

    /// Whether any bit of `other` is set.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// Whether no bit is set.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// The bits in both.
    #[must_use]
    pub const fn intersection(self, other: Self) -> Self {
        Self(self.0 & other.0)
    }

    /// The bits in `self` but not `other`.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }

    /// The bits in either.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// The concrete extensions, in the order a lookup tries them
    /// (`extensions.Array`).
    #[must_use]
    pub fn to_array(self) -> Vec<&'static str> {
        let mut result = Vec::new();
        if self.intersects(Self::TYPESCRIPT) {
            result.extend_from_slice(SUPPORTED_TS_IMPLEMENTATION_EXTENSIONS);
        }
        if self.intersects(Self::JAVASCRIPT) {
            result.extend_from_slice(SUPPORTED_JS_EXTENSIONS_FLAT);
        }
        if self.intersects(Self::DECLARATION) {
            result.extend_from_slice(SUPPORTED_DECLARATION_EXTENSIONS);
        }
        if self.intersects(Self::JSON) {
            result.push(tsr_path::extension::EXTENSION_JSON);
        }
        result
    }
}

impl fmt::Display for Extensions {
    /// `TypeScript, JavaScript, Declaration, JSON` — the exact spelling and order
    /// the `target file types:` trace lines are baselined with.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::with_capacity(4);
        if self.intersects(Self::TYPESCRIPT) {
            parts.push("TypeScript");
        }
        if self.intersects(Self::JAVASCRIPT) {
            parts.push("JavaScript");
        }
        if self.intersects(Self::DECLARATION) {
            parts.push("Declaration");
        }
        if self.intersects(Self::JSON) {
            parts.push("JSON");
        }
        f.write_str(&parts.join(", "))
    }
}

/// Which parts of modern Node resolution are switched on
/// (`module.NodeResolutionFeatures`).
///
/// A feature set rather than a mode enum because the `node10Result` fallback
/// *removes* one bit (`EXPORTS`) from an otherwise-unchanged configuration and
/// resolves again, and because `resolvePackageJsonExports`/`Imports` toggle
/// individual bits on top of the mode's default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeResolutionFeatures(u8);

impl NodeResolutionFeatures {
    /// `package.json` `imports`, for `#`-prefixed specifiers.
    pub const IMPORTS: Self = Self(1 << 0);
    /// A package referring to itself by its own name.
    pub const SELF_NAME: Self = Self(1 << 1);
    /// `package.json` `exports`.
    pub const EXPORTS: Self = Self(1 << 2);
    /// `exports` patterns with text after the `*`.
    pub const EXPORTS_PATTERN_TRAILERS: Self = Self(1 << 3);
    /// `#/`-rooted `imports` patterns. Not in Node yet — see nodejs/node#60864 —
    /// so `node16` withholds it and `nodenext`/`bundler` do not.
    pub const IMPORTS_PATTERN_ROOT: Self = Self(1 << 4);
    /// Nothing.
    pub const NONE: Self = Self(0);
    /// Everything.
    pub const ALL: Self = Self(
        Self::IMPORTS.0
            | Self::SELF_NAME.0
            | Self::EXPORTS.0
            | Self::EXPORTS_PATTERN_TRAILERS.0
            | Self::IMPORTS_PATTERN_ROOT.0,
    );
    /// `node16`'s default: everything but `#/`-rooted patterns.
    pub const NODE16_DEFAULT: Self = Self(
        Self::IMPORTS.0 | Self::SELF_NAME.0 | Self::EXPORTS.0 | Self::EXPORTS_PATTERN_TRAILERS.0,
    );
    /// `nodenext`'s default.
    pub const NODENEXT_DEFAULT: Self = Self::ALL;
    /// `bundler`'s default.
    pub const BUNDLER_DEFAULT: Self = Self::ALL;

    /// Whether any bit of `other` is set.
    #[must_use]
    pub const fn intersects(self, other: Self) -> bool {
        self.0 & other.0 != 0
    }

    /// `self` with `other`'s bits set.
    #[must_use]
    pub const fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    /// `self` with `other`'s bits cleared.
    #[must_use]
    pub const fn difference(self, other: Self) -> Self {
        Self(self.0 & !other.0)
    }
}

/// Which npm package a resolved file came from (`module.PackageId`).
///
/// Its [`fmt::Display`] form appears in the `with Package ID '{2}'` trace line,
/// so the exact shape `name/sub@version+peer@version` is baselined.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash)]
pub struct PackageId {
    /// The package's `name`.
    pub name: String,
    /// The path within the package.
    pub sub_module_name: String,
    /// The package's `version`.
    pub version: String,
    /// A rendered `+name@version` list of resolved peer dependencies.
    pub peer_dependencies: String,
}

impl PackageId {
    /// The name including any sub-path (`PackageId.PackageName`).
    #[must_use]
    pub fn package_name(&self) -> String {
        if self.sub_module_name.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.name, self.sub_module_name)
        }
    }

    /// Whether a package was identified at all.
    #[must_use]
    pub fn is_set(&self) -> bool {
        !self.name.is_empty()
    }
}

impl fmt::Display for PackageId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}@{}{}", self.package_name(), self.version, self.peer_dependencies)
    }
}

/// What resolving an `import` produced (`module.ResolvedModule`).
#[derive(Debug, Clone, Default)]
pub struct ResolvedModule {
    /// The file, or empty if unresolved.
    pub resolved_file_name: String,
    /// The symlink the file was reached through, if it was.
    pub original_path: String,
    /// The extension that matched.
    pub extension: String,
    /// Whether the specifier itself named a TypeScript extension.
    pub resolved_using_ts_extension: bool,
    /// Which package it came from.
    pub package_id: PackageId,
    /// Whether it lives under `node_modules`.
    pub is_external_library_import: bool,
    /// What `node10`-style resolution would have found. Populated only to power
    /// the "your library needs a configuration update" diagnostic.
    pub alternate_result: String,
    /// Diagnostics the resolution itself produced, as rendered trace lines.
    pub resolution_diagnostics: Vec<Trace>,
}

impl ResolvedModule {
    /// Whether anything was found.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        !self.resolved_file_name.is_empty()
    }
}

/// What resolving a `/// <reference types="..." />` produced
/// (`module.ResolvedTypeReferenceDirective`).
#[derive(Debug, Clone, Default)]
pub struct ResolvedTypeReferenceDirective {
    /// Whether it was found in a `typeRoots` directory rather than
    /// `node_modules`.
    pub primary: bool,
    /// The file, or empty if unresolved.
    pub resolved_file_name: String,
    /// The symlink it was reached through, if it was.
    pub original_path: String,
    /// Which package it came from.
    pub package_id: PackageId,
    /// Whether it lives under `node_modules`.
    pub is_external_library_import: bool,
    /// Diagnostics the resolution itself produced.
    pub resolution_diagnostics: Vec<Trace>,
}

impl ResolvedTypeReferenceDirective {
    /// Whether anything was found.
    #[must_use]
    pub fn is_resolved(&self) -> bool {
        !self.resolved_file_name.is_empty()
    }
}

/// The `exports`/`imports` conditions in effect (`module.GetConditions`).
///
/// Order is baselined: it appears verbatim in
/// `Resolving in {0} mode with conditions {1}.`
#[must_use]
pub fn get_conditions(options: &CompilerOptions, resolution_mode: ResolutionMode) -> Vec<String> {
    let module_resolution = options.module_resolution_kind();
    // An unknown mode under `bundler` is ESM: a bundler has no CommonJS/ESM
    // split to detect, so it always takes the `import` branch.
    let resolution_mode = if resolution_mode == ResolutionMode::None
        && module_resolution == tsr_core::ModuleResolutionKind::Bundler
    {
        ResolutionMode::ESNext
    } else {
        resolution_mode
    };

    let mut conditions = Vec::with_capacity(3 + options.custom_conditions.len());
    conditions.push(
        if resolution_mode == ResolutionMode::ESNext { "import" } else { "require" }.to_string(),
    );
    if options.no_dts_resolution != tsr_core::Tristate::True {
        conditions.push("types".to_string());
    }
    if module_resolution != tsr_core::ModuleResolutionKind::Bundler {
        conditions.push("node".to_string());
    }
    conditions.extend(options.custom_conditions.iter().cloned());
    conditions
}

/// The features a resolution kind switches on
/// (`module.getNodeResolutionFeatures`).
#[must_use]
pub fn get_node_resolution_features(options: &CompilerOptions) -> NodeResolutionFeatures {
    use tsr_core::{ModuleResolutionKind, Tristate};

    let mut features = match options.module_resolution_kind() {
        ModuleResolutionKind::Node16 => NodeResolutionFeatures::NODE16_DEFAULT,
        ModuleResolutionKind::NodeNext => NodeResolutionFeatures::NODENEXT_DEFAULT,
        ModuleResolutionKind::Bundler => NodeResolutionFeatures::BUNDLER_DEFAULT,
        _ => NodeResolutionFeatures::NONE,
    };
    match options.resolve_package_json_exports {
        Tristate::True => features = features.union(NodeResolutionFeatures::EXPORTS),
        Tristate::False => features = features.difference(NodeResolutionFeatures::EXPORTS),
        Tristate::Unknown => {}
    }
    match options.resolve_package_json_imports {
        Tristate::True => features = features.union(NodeResolutionFeatures::IMPORTS),
        Tristate::False => features = features.difference(NodeResolutionFeatures::IMPORTS),
        Tristate::Unknown => {}
    }
    features
}

/// Whether `extension` is one the lookup would accept (`module.extensionIsOk`).
#[must_use]
pub fn extension_is_ok(extensions: Extensions, extension: &str) -> bool {
    use tsr_path::extension::*;
    (extensions.intersects(Extensions::JAVASCRIPT)
        && matches!(extension, EXTENSION_JS | EXTENSION_JSX | EXTENSION_MJS | EXTENSION_CJS))
        || (extensions.intersects(Extensions::TYPESCRIPT)
            && matches!(extension, EXTENSION_TS | EXTENSION_TSX | EXTENSION_MTS | EXTENSION_CTS))
        || (extensions.intersects(Extensions::DECLARATION)
            && matches!(extension, EXTENSION_DTS | EXTENSION_DMTS | EXTENSION_DCTS))
        || (extensions.intersects(Extensions::JSON) && extension == EXTENSION_JSON)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tsr_core::{ModuleKind, ModuleResolutionKind, Tristate};

    #[test]
    fn the_extension_set_renders_the_way_the_baselines_spell_it() {
        // 300 baseline lines say exactly this. The order is not alphabetical and
        // is not negotiable.
        let all = Extensions::TYPESCRIPT
            .union(Extensions::JAVASCRIPT)
            .union(Extensions::DECLARATION)
            .union(Extensions::JSON);
        assert_eq!(all.to_string(), "TypeScript, JavaScript, Declaration, JSON");
        assert_eq!(
            Extensions::TYPESCRIPT.union(Extensions::DECLARATION).to_string(),
            "TypeScript, Declaration"
        );
    }

    #[test]
    fn the_two_node_modules_passes_partition_the_extension_set() {
        // Pass one is TypeScript+Declaration, pass two is everything else; the
        // partition is what gives `@types` packages priority over untyped
        // implementations higher up the tree.
        let all = Extensions::TYPESCRIPT
            .union(Extensions::JAVASCRIPT)
            .union(Extensions::DECLARATION)
            .union(Extensions::JSON);
        let priority = all.intersection(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));
        let secondary = all.difference(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));
        assert_eq!(priority.to_string(), "TypeScript, Declaration");
        assert_eq!(secondary.to_string(), "JavaScript, JSON");
        assert_eq!(priority.union(secondary), all);
    }

    #[test]
    fn conditions_follow_the_mode_and_the_resolution_kind() {
        let bundler =
            CompilerOptions { module: ModuleKind::Preserve, ..CompilerOptions::default() };
        // Bundler: no `node`, and an unspecified mode reads as ESM.
        assert_eq!(get_conditions(&bundler, ResolutionMode::None), ["import", "types"]);

        let node16 = CompilerOptions { module: ModuleKind::Node16, ..CompilerOptions::default() };
        assert_eq!(get_conditions(&node16, ResolutionMode::CommonJS), ["require", "types", "node"]);
        assert_eq!(get_conditions(&node16, ResolutionMode::ESNext), ["import", "types", "node"]);

        // `noDtsResolution` drops `types`; custom conditions append.
        let custom = CompilerOptions {
            module: ModuleKind::Node16,
            no_dts_resolution: Tristate::True,
            custom_conditions: vec!["bun".to_string()],
            ..CompilerOptions::default()
        };
        assert_eq!(get_conditions(&custom, ResolutionMode::ESNext), ["import", "node", "bun"]);
    }

    #[test]
    fn node16_withholds_the_rooted_imports_pattern_and_nodenext_does_not() {
        let node16 = CompilerOptions { module: ModuleKind::Node16, ..CompilerOptions::default() };
        assert!(
            !get_node_resolution_features(&node16)
                .intersects(NodeResolutionFeatures::IMPORTS_PATTERN_ROOT)
        );
        let nodenext =
            CompilerOptions { module: ModuleKind::NodeNext, ..CompilerOptions::default() };
        assert!(
            get_node_resolution_features(&nodenext)
                .intersects(NodeResolutionFeatures::IMPORTS_PATTERN_ROOT)
        );
    }

    #[test]
    fn resolve_package_json_exports_overrides_the_kinds_default() {
        let options = CompilerOptions {
            module_resolution: ModuleResolutionKind::Bundler,
            resolve_package_json_exports: Tristate::False,
            ..CompilerOptions::default()
        };
        assert!(
            !get_node_resolution_features(&options).intersects(NodeResolutionFeatures::EXPORTS)
        );
    }

    #[test]
    fn a_package_id_renders_with_its_submodule_and_peers() {
        let id = PackageId {
            name: "foo".to_string(),
            sub_module_name: "lib/index.d.ts".to_string(),
            version: "1.0.0".to_string(),
            peer_dependencies: "+bar@2.0.0".to_string(),
        };
        assert_eq!(id.to_string(), "foo/lib/index.d.ts@1.0.0+bar@2.0.0");
    }
}
