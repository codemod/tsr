//! `tryGetModuleNameAsNodeModule` (`modulespecifiers/specifiers.go:743`) and
//! the pieces it calls: the package-name specifier of a file under
//! `node_modules` — `inner/other.js`, `@scope/pkg`, `foo/node_modules/nested`
//! — read from the package's `package.json` `exports`, `typesVersions` and
//! `typings`/`types`/`main`.
//!
//! Called from `Checker::module_specifier_for_symbol` (`checker.rs`) for a
//! module path containing `/node_modules/`, before the relative specifier
//! `computeModuleSpecifiers` (`:359`) falls back to. The program answers the
//! `package.json` reads through `ModuleHost::package_json_for_specifiers`;
//! no cache, side table or traversal lives here, and every function is a
//! pure function of the paths and those reads.
//!
//! Not ported, each named where it would sit: symlinked module paths
//! (`GetEachFileNameOfModule`'s symlink cache, `:260`), project-reference
//! and duplicate-package redirects (`IsRedirect`), the global typings cache
//! (empty in every corpus program), the `.d.json.ts` remap
//! (`TryGetRealFileNameForNonJSDeclarationFileName`), and the `.ts` ending
//! that `allowImportingTsExtensions` can choose. r5-modules §5.

use tsr_ast::NodeId;
use tsr_core::ModuleKind;

use crate::checker::Checker;
use crate::resolution::{PackageJsonView, SpecifierJson};

/// `NodeModulePathParts` (`modulespecifiers/util.go:274`): byte indices into
/// a path under `node_modules`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct NodeModulePathParts {
    /// The `/` that starts the outermost `/node_modules/`.
    pub top_level_node_modules_index: usize,
    /// The `/` that ends it (before the outermost package name).
    pub top_level_package_name_index: usize,
    /// The `/` after the innermost package's name (its root directory's
    /// end); `None` is upstream's `-1`, a file directly in `node_modules`
    /// (`node_modules/foo.d.ts`).
    pub package_root_index: Option<usize>,
    /// The `/` before the file name.
    pub file_name_index: usize,
}

/// `core.IndexAfter(s, "/", start)`.
fn slash_after(path: &str, start: usize) -> Option<usize> {
    path.get(start..)?.find('/').map(|offset| start + offset)
}

/// `GetNodeModulePathParts` (`modulespecifiers/util.go:290`).
#[must_use]
pub fn node_module_path_parts(full_path: &str) -> Option<NodeModulePathParts> {
    #[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
    enum State {
        BeforeNodeModules,
        NodeModules,
        Scope,
        PackageContent,
    }
    let bytes = full_path.as_bytes();
    let (mut top_level_node_modules_index, mut top_level_package_name_index) = (0, 0);
    let mut package_root_index = Some(0);
    let mut part_end = Some(0usize);
    let mut part_start = 0usize;
    let mut state = State::BeforeNodeModules;
    while let Some(end) = part_end {
        part_start = end;
        part_end = slash_after(full_path, part_start + 1);
        let at_node_modules = full_path[part_start..].starts_with("/node_modules/");
        match state {
            State::BeforeNodeModules => {
                if at_node_modules {
                    top_level_node_modules_index = part_start;
                    top_level_package_name_index = part_end?;
                    state = State::NodeModules;
                }
            }
            State::NodeModules | State::Scope => {
                if state == State::NodeModules && bytes.get(part_start + 1) == Some(&b'@') {
                    state = State::Scope;
                } else {
                    package_root_index = part_end;
                    state = State::PackageContent;
                }
            }
            State::PackageContent => {
                if at_node_modules {
                    state = State::NodeModules;
                }
            }
        }
    }
    (state > State::NodeModules).then_some(NodeModulePathParts {
        top_level_node_modules_index,
        top_level_package_name_index,
        package_root_index,
        file_name_index: part_start,
    })
}

/// `tryDirectoryWithPackageJson`'s `rootIdx`: the package root's end, or the
/// whole path for upstream's `-1`.
#[must_use]
pub fn package_root_end(parts: NodeModulePathParts, module_path: &str) -> usize {
    parts.package_root_index.unwrap_or(module_path.len())
}

/// `module.GetPackageNameFromTypesPackageName` (`module/util.go:81`).
fn package_name_from_types_package_name(mangled: &str) -> String {
    match mangled.strip_prefix("@types/") {
        Some(rest) => match rest.split_once("__") {
            Some((scope, name)) => format!("@{scope}/{name}"),
            None => rest.to_string(),
        },
        None => mangled.to_string(),
    }
}

/// `ModuleSpecifierEnding` (`modulespecifiers/types.go`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Ending {
    Minimal,
    Index,
    Js,
}

/// `tspath.RemoveFileExtension`'s extension, for the extensions a program
/// file can carry. Longest first, so `.d.ts` wins over `.ts`.
fn file_extension(path: &str) -> Option<&'static str> {
    [
        ".d.mts", ".d.cts", ".d.ts", ".tsx", ".mts", ".cts", ".ts", ".jsx", ".mjs", ".cjs", ".js",
        ".json",
    ]
    .into_iter()
    .find(|extension| path.ends_with(extension))
}

/// `tspath.HasTSFileExtension`.
fn has_ts_file_extension(path: &str) -> bool {
    [".ts", ".tsx", ".mts", ".cts"].iter().any(|extension| path.ends_with(extension))
}

/// `tspath.HasPrefixAndSuffixWithoutOverlap`.
fn has_prefix_and_suffix_without_overlap(text: &str, prefix: &str, suffix: &str) -> bool {
    text.len() >= prefix.len() + suffix.len() && text.starts_with(prefix) && text.ends_with(suffix)
}

/// `tspath.GetNormalizedAbsolutePath(path, "")` for the joined
/// package-relative strings `exports` matching builds: `.` and `..`
/// segments and doubled separators resolved.
fn normalize(path: &str) -> String {
    let rooted = path.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                if parts.last().is_some_and(|last| *last != "..") {
                    parts.pop();
                } else if !rooted {
                    parts.push("..");
                }
            }
            other => parts.push(other),
        }
    }
    let joined = parts.join("/");
    if rooted { format!("/{joined}") } else { joined }
}

/// `tspath.CombinePaths(a, b)` for a relative `b`.
fn combine(directory: &str, relative: &str) -> String {
    if relative.starts_with('/') {
        return relative.to_string();
    }
    if directory.is_empty() || directory.ends_with('/') {
        format!("{directory}{relative}")
    } else {
        format!("{directory}/{relative}")
    }
}

impl Checker<'_, '_> {
    /// `processEnding` (`modulespecifiers/specifiers.go:636`) under
    /// `allowed` endings (never `.ts`, which `allowImportingTsExtensions`
    /// alone admits and the caller declines). `None` for the
    /// `foo.d.json.ts` remap, which is not ported.
    pub(crate) fn process_ending(&self, file_name: &str, allowed: &[Ending]) -> Option<String> {
        let Some(extension) = file_extension(file_name) else {
            return Some(file_name.to_string());
        };
        if matches!(extension, ".json" | ".mjs" | ".cjs") {
            return Some(file_name.to_string());
        }
        let base = &file_name[..file_name.len() - extension.len()];
        let (_, js) = self.js_extension_for_file(file_name)?;
        if matches!(extension, ".d.mts" | ".d.cts" | ".mts" | ".cts") {
            return Some(format!("{base}{js}"));
        }
        if extension == ".ts" && file_name.contains(".d.") {
            return None;
        }
        Some(match allowed.first().copied().unwrap_or(Ending::Minimal) {
            Ending::Minimal => base.strip_suffix("/index").unwrap_or(base).to_string(),
            Ending::Index => base.to_string(),
            Ending::Js => format!("{base}{js}"),
        })
    }

    /// `getAllowedEndingsInPreferredOrder` (`modulespecifiers/preferences.go`)
    /// without `allowImportingTsExtensions` (`None` there, as the caller
    /// declines it): `[Js]` for an ESM file under `node16`..`nodenext`
    /// resolution, else the preferred ending first.
    pub(crate) fn allowed_endings(
        &self,
        importing: NodeId,
        mode: ModuleKind,
    ) -> Option<Vec<Ending>> {
        let host = self.module_host?;
        if mode == ModuleKind::ESNext && host.specifier_options(mode).module_resolution_is_node_next
        {
            return Some(vec![Ending::Js]);
        }
        Some(if self.module_specifier_uses_js_ending(importing, mode)? {
            vec![Ending::Js, Ending::Minimal, Ending::Index]
        } else {
            vec![Ending::Minimal, Ending::Index, Ending::Js]
        })
    }

    /// `tryGetModuleNameAsNodeModule` (`modulespecifiers/specifiers.go:743`)
    /// with `packageNameOnly = false` and no override mode, for a module at
    /// `module_path` imported from `importing` (whose directory is
    /// `source_directory`). `None` is upstream's `""`: the caller falls back
    /// to the relative specifier.
    pub(crate) fn node_module_specifier(
        &self,
        importing: NodeId,
        source_directory: &str,
        module_path: &str,
        override_mode: ModuleKind,
    ) -> Option<String> {
        let parts = node_module_path_parts(module_path)?;
        let host = self.module_host?;
        let mode = if override_mode == ModuleKind::None {
            host.default_resolution_mode_for_file(importing)
        } else {
            override_mode
        };
        let allowed = self.allowed_endings(importing, mode)?;
        // Upstream's loop advances a local `packageRootIndex` but hands
        // `tryDirectoryWithPackageJson` the unchanged `*parts`, so every
        // iteration re-tries the package root and the loop ends in
        // `processEnding` of the first attempt's file. Ported as the one
        // attempt it is at the pinned commit (strada passed the advancing
        // index). r5-modules §5.2.
        let module_specifier =
            match self.try_directory_with_package_json(mode, parts, module_path, &allowed) {
                DirectoryAttempt::BlockedByExports => return None,
                DirectoryAttempt::VerbatimFromExports(specifier) => return Some(specifier),
                DirectoryAttempt::PackageRoot(path) => path,
                DirectoryAttempt::ModuleFile(file) => self.process_ending(&file, &allowed)?,
            };
        // "if node_modules folder is in this folder or any of its parent
        // folders, no need to keep it" — and if it is not, there is no
        // package name to give.
        let path_to_top_level_node_modules =
            module_specifier.get(..parts.top_level_node_modules_index)?;
        if !source_directory.starts_with(path_to_top_level_node_modules) {
            return None;
        }
        let directory_name = module_specifier.get(parts.top_level_package_name_index + 1..)?;
        Some(package_name_from_types_package_name(directory_name))
    }

    /// `tryDirectoryWithPackageJson` (`modulespecifiers/specifiers.go:832`)
    /// for the directory `module_path[..root]`.
    fn try_directory_with_package_json(
        &self,
        mode: ModuleKind,
        parts: NodeModulePathParts,
        module_path: &str,
        allowed: &[Ending],
    ) -> DirectoryAttempt {
        let package_root_path = &module_path[..package_root_end(parts, module_path)];
        let module_file = || DirectoryAttempt::ModuleFile(module_path.to_string());
        let Some(host) = self.module_host else { return module_file() };
        let Some(package_json) = host.package_json_for_specifiers(package_root_path) else {
            // No package.json: an index file still resolves as the directory.
            let file_name = parts
                .package_root_index
                .and_then(|root| module_path.get(root + 1..))
                .unwrap_or(module_path);
            return if matches!(file_name, "index.d.ts" | "index.js" | "index.ts" | "index.tsx") {
                DirectoryAttempt::PackageRoot(package_root_path.to_string())
            } else {
                module_file()
            };
        };
        let mut import_mode = mode;
        if [".cjs", ".cts", ".d.cts"].iter().any(|extension| module_path.ends_with(extension)) {
            import_mode = ModuleKind::CommonJS;
        } else if [".mjs", ".mts", ".d.mts"]
            .iter()
            .any(|extension| module_path.ends_with(extension))
        {
            import_mode = ModuleKind::ESNext;
        }
        let options = host.specifier_options(import_mode);
        if options.resolve_package_json_exports
            && let Some(exports) = &package_json.exports
        {
            let directory_name =
                package_root_path.get(parts.top_level_package_name_index + 1..).unwrap_or("");
            let package_name = package_name_from_types_package_name(directory_name);
            if let Some(specifier) = self.module_name_from_exports(
                module_path,
                package_root_path,
                &package_name,
                exports,
                &options.conditions,
            ) {
                return DirectoryAttempt::VerbatimFromExports(specifier);
            }
            return DirectoryAttempt::BlockedByExports;
        }
        self.try_main_or_types_versions(package_json, package_root_path, module_path, allowed)
    }

    /// The rest of `tryDirectoryWithPackageJson`: `typesVersions` mapping,
    /// then the main file (`typings` / `types` / `main`, else `index.js`).
    fn try_main_or_types_versions(
        &self,
        package_json: &PackageJsonView,
        package_root_path: &str,
        module_path: &str,
        allowed: &[Ending],
    ) -> DirectoryAttempt {
        let mut module_file_to_try = module_path.to_string();
        let mut maybe_blocked_by_types_versions = false;
        if let Some(paths) = &package_json.types_versions_paths {
            let sub_module_name = module_path.get(package_root_path.len() + 1..).unwrap_or("");
            match self.module_name_from_paths(sub_module_name, paths, allowed, package_root_path) {
                Some(from_paths) => module_file_to_try = combine(package_root_path, &from_paths),
                None => maybe_blocked_by_types_versions = true,
            }
        }
        let main_file_relative = package_json
            .typings
            .as_deref()
            .or(package_json.types.as_deref())
            .or(package_json.main.as_deref())
            .unwrap_or("index.js");
        if main_file_relative.is_empty() {
            return DirectoryAttempt::ModuleFile(module_file_to_try);
        }
        if maybe_blocked_by_types_versions
            && package_json.types_versions_paths.as_ref().is_some_and(|paths| {
                paths.keys().any(|key| pattern_matches(key, main_file_relative))
            })
        {
            return DirectoryAttempt::ModuleFile(module_file_to_try);
        }
        let main_export_file = normalize(&combine(package_root_path, main_file_relative));
        let without_extension = |path: &str| {
            file_extension(path).map_or(path.to_string(), |extension| {
                path[..path.len() - extension.len()].to_string()
            })
        };
        if without_extension(&main_export_file) == without_extension(&module_file_to_try) {
            return DirectoryAttempt::PackageRoot(package_root_path.to_string());
        }
        let not_extensionless = [".mts", ".d.mts", ".mjs", ".cts", ".d.cts", ".cjs"]
            .iter()
            .any(|extension| module_file_to_try.ends_with(extension));
        let directory = module_file_to_try.rsplit_once('/').map_or("", |(directory, _)| directory);
        let base_name = module_file_to_try.rsplit('/').next().unwrap_or("");
        if package_json.package_type.as_deref() != Some("module")
            && !not_extensionless
            && module_file_to_try.starts_with(&main_export_file)
            && directory == main_export_file.trim_end_matches('/')
            && without_extension(base_name) == "index"
        {
            return DirectoryAttempt::PackageRoot(package_root_path.to_string());
        }
        DirectoryAttempt::ModuleFile(module_file_to_try)
    }

    /// `tryGetModuleNameFromExports` (`modulespecifiers/specifiers.go:976`).
    fn module_name_from_exports(
        &self,
        target: &str,
        package_directory: &str,
        package_name: &str,
        exports: &SpecifierJson,
        conditions: &[String],
    ) -> Option<String> {
        if let SpecifierJson::Object(entries) = exports
            && is_subpaths(entries)
        {
            for (key, value) in entries {
                let sub_package_name = normalize(&combine(package_name, key));
                let mode = if key.ends_with('/') {
                    Matching::Directory
                } else if key.contains('*') {
                    Matching::Pattern
                } else {
                    Matching::Exact
                };
                if let Some(found) = self.module_name_from_exports_or_imports(
                    target,
                    package_directory,
                    &sub_package_name,
                    value,
                    conditions,
                    mode,
                ) {
                    return Some(found);
                }
            }
        }
        self.module_name_from_exports_or_imports(
            target,
            package_directory,
            package_name,
            exports,
            conditions,
            Matching::Exact,
        )
    }

    /// `tryGetModuleNameFromExportsOrImports` (`:1199`) for `exports`
    /// (`isImports = false`, `preferTsExtension = false`).
    fn module_name_from_exports_or_imports(
        &self,
        target: &str,
        package_directory: &str,
        package_name: &str,
        exports: &SpecifierJson,
        conditions: &[String],
        mode: Matching,
    ) -> Option<String> {
        match exports {
            SpecifierJson::String(value) => {
                let path_or_pattern = normalize(&combine(package_directory, value));
                let extension_swapped = if has_ts_file_extension(target) {
                    let (input, js) = self.js_extension_for_file(target)?;
                    Some(format!("{}{js}", &target[..target.len() - input.len()]))
                } else {
                    None
                };
                match mode {
                    Matching::Exact => (extension_swapped.as_deref()
                        == Some(path_or_pattern.as_str())
                        || target == path_or_pattern)
                        .then(|| package_name.to_string()),
                    Matching::Directory => {
                        let within = |file: &str| {
                            let directory = path_or_pattern.trim_end_matches('/');
                            file.strip_prefix(directory)
                                .and_then(|rest| rest.strip_prefix('/'))
                                .map(str::to_string)
                        };
                        let fragment = extension_swapped
                            .as_deref()
                            .and_then(within)
                            .or_else(|| within(target))?;
                        Some(normalize(&combine(&combine(package_name, value), &fragment)))
                    }
                    Matching::Pattern => {
                        let (leading, trailing) = path_or_pattern.split_once('*')?;
                        let replace = |file: &str| {
                            has_prefix_and_suffix_without_overlap(file, leading, trailing).then(
                                || {
                                    package_name.replacen(
                                        '*',
                                        &file[leading.len()..file.len() - trailing.len()],
                                        1,
                                    )
                                },
                            )
                        };
                        extension_swapped.as_deref().and_then(replace).or_else(|| replace(target))
                    }
                }
            }
            SpecifierJson::Array(values) => values.iter().find_map(|value| {
                self.module_name_from_exports_or_imports(
                    target,
                    package_directory,
                    package_name,
                    value,
                    conditions,
                    mode,
                )
            }),
            SpecifierJson::Object(entries) => entries.iter().find_map(|(key, value)| {
                let applies = key == "default"
                    || conditions.iter().any(|condition| condition == key)
                    || conditions.iter().any(|condition| condition == "types")
                        && self
                            .module_host
                            .is_some_and(|host| host.is_applicable_versioned_types_key(key));
                if !applies {
                    return None;
                }
                self.module_name_from_exports_or_imports(
                    target,
                    package_directory,
                    package_name,
                    value,
                    conditions,
                    mode,
                )
            }),
            SpecifierJson::Null | SpecifierJson::Other => None,
        }
    }

    /// `tryGetModuleNameFromPaths` (`:1089`) for a package's `typesVersions`
    /// paths, relative to `base_directory`.
    fn module_name_from_paths(
        &self,
        relative_to_base_url: &str,
        paths: &tsr_core::OrderedMap<Vec<String>>,
        allowed: &[Ending],
        base_directory: &str,
    ) -> Option<String> {
        for (key, values) in paths.entries() {
            for pattern_text in values {
                let normalized = normalize(pattern_text);
                let absolute = normalize(&combine(base_directory, pattern_text));
                let pattern = absolute
                    .strip_prefix(base_directory)
                    .and_then(|rest| rest.strip_prefix('/'))
                    .map_or(normalized, str::to_string);
                let mut candidates: Vec<(Ending, String)> = allowed
                    .iter()
                    .filter_map(|&ending| {
                        Some((ending, self.process_ending(relative_to_base_url, &[ending])?))
                    })
                    .collect();
                if file_extension(&pattern).is_some() {
                    candidates.push((Ending::Js, relative_to_base_url.to_string()));
                }
                if let Some((prefix, suffix)) = pattern.split_once('*') {
                    for (_, value) in &candidates {
                        if has_prefix_and_suffix_without_overlap(value, prefix, suffix) {
                            let matched = &value[prefix.len()..value.len() - suffix.len()];
                            if !tsr_path::path_is_relative(matched) {
                                return Some(key.replacen('*', matched, 1));
                            }
                        }
                    }
                } else if candidates.iter().any(|(_, value)| *value == pattern) {
                    return Some(key.to_string());
                }
            }
        }
        None
    }
}

/// A specifier the node builder generated that dives into `node_modules`
/// — the arguments of `ReportLikelyUnsafeImportRequiredError`
/// (`nodebuilderimpl.go:709`) before its mode-swap retry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct UnsafeImport {
    /// The specifier, unquoted (`oldSpecifier`).
    pub specifier: String,
    /// `symbol.Name` of the symbol being named.
    pub symbol_name: String,
    /// The module the chain is rooted at (`chain[0]`).
    pub module: tsr_binder::SymbolId,
}

impl Checker<'_, '_> {
    /// Record a generated `import("…")` specifier for the active
    /// declaration-emit tracker when it contains `/node_modules/`
    /// (`nodebuilderimpl.go:681`'s test; `FlagsAllowNodeModulesRelativePaths`
    /// is never set by declaration emit). A no-op outside one.
    pub(crate) fn track_unsafe_import(
        &mut self,
        quoted_specifier: &str,
        symbol: tsr_binder::SymbolId,
        module: tsr_binder::SymbolId,
    ) {
        let Some(tracker) = self.unsafe_import_tracker.as_mut() else { return };
        let specifier = quoted_specifier
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
            .unwrap_or(quoted_specifier);
        if !specifier.contains("/node_modules/") {
            return;
        }
        let symbol_name = self.binder.symbols().get(symbol).name.to_string();
        let entry = UnsafeImport { specifier: specifier.to_string(), symbol_name, module };
        if !tracker.contains(&entry) {
            tracker.push(entry);
        }
    }
}

/// What one `tryDirectoryWithPackageJson` attempt answered.
enum DirectoryAttempt {
    /// `blockedByExports`.
    BlockedByExports,
    /// `verbatimFromExports`, with the specifier.
    VerbatimFromExports(String),
    /// `packageRootPath`: the directory can be imported by name.
    PackageRoot(String),
    /// Only `moduleFileToTry`.
    ModuleFile(String),
}

/// `MatchingMode` (`modulespecifiers/types.go`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Matching {
    Exact,
    Directory,
    Pattern,
}

/// `ExportsOrImports.IsSubpaths`: an object whose keys start with `.`
/// (`packagejson.objectKind`; a mixed object is neither).
fn is_subpaths(entries: &[(String, SpecifierJson)]) -> bool {
    let dot = entries.iter().any(|(key, _)| key.starts_with('.'));
    let other = entries
        .iter()
        .any(|(key, _)| !key.is_empty() && !key.starts_with('.') && !key.starts_with('#'));
    dot && !other
}

/// `MatchPatternOrExact(TryParsePatterns(paths), candidate)` is non-empty:
/// `key` matches `candidate` exactly, or as a single-`*` pattern.
fn pattern_matches(key: &str, candidate: &str) -> bool {
    match key.split_once('*') {
        Some((prefix, suffix)) => has_prefix_and_suffix_without_overlap(candidate, prefix, suffix),
        None => key == candidate,
    }
}

#[cfg(test)]
mod tests {
    use super::{node_module_path_parts, package_name_from_types_package_name};

    #[test]
    fn path_parts_index_the_outermost_node_modules_and_the_innermost_package() {
        let path = "/a/node_modules/foo/node_modules/@s/nested/lib/x.d.ts";
        let parts = node_module_path_parts(path).expect("a node_modules path");
        assert_eq!(&path[..parts.top_level_node_modules_index], "/a");
        assert_eq!(
            &path[parts.top_level_package_name_index + 1..parts.package_root_index.unwrap()],
            "foo/node_modules/@s/nested"
        );
        assert_eq!(&path[parts.file_name_index + 1..], "x.d.ts");
        assert!(node_module_path_parts("/a/src/x.ts").is_none());
        // A file directly in `node_modules` has upstream's `-1` root.
        let flat = node_module_path_parts("/a/node_modules/foo.d.ts").expect("a node_modules path");
        assert_eq!(flat.package_root_index, None);
    }

    #[test]
    fn types_packages_unmangle_to_their_package_name() {
        assert_eq!(package_name_from_types_package_name("@types/foo__bar"), "@foo/bar");
        assert_eq!(package_name_from_types_package_name("@types/node"), "node");
        assert_eq!(package_name_from_types_package_name("inner/other.js"), "inner/other.js");
    }
}
