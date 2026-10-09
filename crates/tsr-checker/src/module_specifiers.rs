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
//! the one side table here is [`KnownSymlinks`], the program's, built
//! before the checker and read-only to it (its convention record is on the
//! type); every other function is a
//! pure function of the paths and those reads.
//!
//! `GetEachFileNameOfModule` (`:260`) and `getAllModulePathsWorker`
//! (`:198`) are ported over [`KnownSymlinks`], the program's symlink cache
//! (`internal/symlinks`), so `computeModuleSpecifiers`' loop runs over every
//! path that reaches a module (r6-specifiers §2). Not ported, each named
//! where it would sit: project-reference
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
    Ts,
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

/// `tspath.HasJSFileExtension`.
fn has_js_file_extension(path: &str) -> bool {
    [".js", ".jsx", ".mjs", ".cjs"].iter().any(|extension| path.ends_with(extension))
}

/// `tspath.ExtensionsNotSupportingExtensionlessResolution`.
const NOT_EXTENSIONLESS: [&str; 6] = [".mts", ".d.mts", ".mjs", ".cts", ".d.cts", ".cjs"];

/// `PathIsBareSpecifier` (`modulespecifiers/util.go:42`) folded into
/// `ensurePathIsNonModuleName` (`:136`): a bare path gets `./`.
fn ensure_path_is_non_module_name(path: String) -> String {
    if tsr_path::path_is_relative(&path) || tsr_path::get_root_length(&path) > 0 {
        path
    } else {
        format!("./{path}")
    }
}

/// `GetJSExtensionForDeclarationFileExtension` (`modulespecifiers/util.go:143`).
fn js_extension_for_declaration_file_extension(extension: &str) -> &str {
    match extension {
        ".d.ts" => ".js",
        ".d.mts" => ".mjs",
        ".d.cts" => ".cjs",
        // `.d.json.ts` and the like.
        other => other.get(".d".len()..other.len().saturating_sub(".ts".len())).unwrap_or(""),
    }
}

/// `TryGetRealFileNameForNonJSDeclarationFileName` (`modulespecifiers/util.go:159`):
/// `foo.d.json.ts` / `foo.module.d.css.ts` back to `foo.json` /
/// `foo.module.css`.
fn real_file_name_for_non_js_declaration_file_name(file_name: &str) -> Option<String> {
    let base_name = file_name.rsplit('/').next().unwrap_or(file_name);
    if !tsr_path::file_extension_is(file_name, ".ts")
        || !base_name.contains(".d.")
        || tsr_path::file_extension_is(base_name, ".d.ts")
    {
        return None;
    }
    let no_extension = tsr_path::remove_extension(file_name, ".ts");
    let extension = &no_extension[no_extension.rfind('.')?..];
    let before = no_extension.split_once(".d.").map_or(no_extension, |(before, _)| before);
    Some(format!("{before}{extension}"))
}

/// The node builder's half of `ModuleSpecifierPreferences`
/// (`getSpecifierForModuleSymbol`, `nodebuilderimpl.go:1296`): the
/// `ImportModuleSpecifierEnding` it passes — `Js` when its resolution mode is
/// ESM, else none — under `ImportModuleSpecifierPreferenceProjectRelative`.
#[derive(Debug, Clone, Copy)]
pub(crate) struct SpecifierPreferences {
    /// `ImportModuleSpecifierEndingPreferenceJs` (else `None`).
    pub ending_js: bool,
}

impl Checker<'_, '_> {
    /// `symbolToTypeNode`'s import-type arm (`nodebuilderimpl.go:659`-`:707`):
    /// the argument of the `import(…)` type that names `module` at
    /// `reference` — the quoted specifier, followed by
    /// `, { with: { "resolution-mode": "…" } }` when native writes the
    /// attribute.
    ///
    /// Under `node16`/`nodenext` resolution, a target emitted as ESM from a
    /// context file of another emit format is named in ESM mode with the
    /// `import` attribute. `GetEmitModuleFormatOfFile` is
    /// `ModuleHost::implied_node_format_for_emit`, as in r5-modules §6.
    ///
    /// The arm's second half — regenerating a specifier that dives into
    /// `/node_modules/` in the swapped mode — is gated on
    /// `FlagsAllowNodeModulesRelativePaths` being *unset*. `typeToString`
    /// always sets it (`FlagsIgnoreErrors`, `printer.go:202`,
    /// `nodebuilder/types.go:61`), so a printed type never swaps; only
    /// declaration emit does, and its tracker
    /// (`symbol_access.rs` `inferred_type_reports`) asks the swapped mode
    /// itself (r5-modules2 §3.2).
    pub(crate) fn import_type_argument(
        &self,
        module: tsr_binder::SymbolId,
        reference: NodeId,
    ) -> Option<String> {
        let node_next = self.module_host.is_some_and(|host| {
            host.specifier_options(ModuleKind::None).module_resolution_is_node_next
        });
        if !node_next {
            return self.module_specifier_for_symbol_in_mode(module, reference, ModuleKind::None);
        }
        let host = self.module_host?;
        let format = |file: NodeId| host.implied_node_format_for_emit(file);
        // `ast.GetSourceFileOfModule(chain[0])` and the enclosing file.
        let target_file = self
            .binder
            .symbols()
            .get(module)
            .declarations
            .first()
            .and_then(|&declaration| self.source_file_of(declaration));
        let context_file = self.source_file_of(reference);
        let mut attribute = None;
        let mut specifier = None;
        if let (Some(target), Some(context)) = (target_file, context_file)
            && format(target) == ModuleKind::ESNext
            && format(target) != format(context)
        {
            specifier =
                self.module_specifier_for_symbol_in_mode(module, reference, ModuleKind::ESNext);
            attribute = Some("import");
        }
        let specifier = match specifier {
            Some(specifier) => specifier,
            None => {
                self.module_specifier_for_symbol_in_mode(module, reference, ModuleKind::None)?
            }
        };
        Some(match attribute {
            Some(mode) => format!("{specifier}, {{ with: {{ \"resolution-mode\": \"{mode}\" }} }}"),
            None => specifier,
        })
    }

    /// `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1249`) for a module
    /// that has a source file, over `GetModuleSpecifiers`
    /// (`modulespecifiers/specifiers.go:19`) and `computeModuleSpecifiers`
    /// (`:359`): the first specifier, unquoted. The ambient arms are the
    /// caller's (`Checker::module_specifier_for_symbol_in_mode`).
    ///
    /// `module_file` is the module's source file, `importing` the
    /// reference's. The module paths are [`all_module_paths`]: the symlinked
    /// paths the program's [`KnownSymlinks`] knows, then the file's own;
    /// redirects are not ported (r6-specifiers §2).
    ///
    /// No cache: native memoizes the answer per `(symbol, file, mode)`
    /// (`links.specifierCache`); every input here is a pure function of the
    /// program, so recomputing gives the same answer (r5-modules2 §2).
    pub(crate) fn module_specifier_for_file(
        &self,
        module_file: NodeId,
        importing: NodeId,
        override_mode: ModuleKind,
    ) -> Option<String> {
        let host = self.module_host?;
        // The node builder's own resolution mode (`:1278`), which picks the
        // ending preference it passes. The `originalModuleSpecifier` arm
        // (printing inside an import declaration) is not ported.
        let builder_mode = if override_mode == ModuleKind::None {
            host.default_resolution_mode_for_file(importing)
        } else {
            override_mode
        };
        let preferences = SpecifierPreferences { ending_js: builder_mode == ModuleKind::ESNext };
        // `computeModuleSpecifiers`' existing-import arm (r5-modules §4).
        if let Some(existing) =
            self.existing_import_specifier(importing, module_file, override_mode)
        {
            return Some(existing);
        }
        let from = host.file_path(importing)?;
        let to = host.file_path(module_file)?;
        let source_directory = tsr_path::get_directory_path(&from);
        // `getAllModulePathsWorker`: the symlinked paths that reach the
        // file, then its own, nearest the importing file first.
        let module_paths = all_module_paths(&from, &to, self.known_symlinks(), "", true);
        let imported_file_is_in_node_modules =
            module_paths.iter().any(|path| path.is_in_node_modules);
        let import_mode = if override_mode == ModuleKind::None {
            host.default_resolution_mode_for_file(importing)
        } else {
            override_mode
        };
        // `computeModuleSpecifiers`' priority (`:415`): a package-name
        // specifier for any path beats every relative one. With no
        // `paths`, `getLocalModuleSpecifier`'s `pathsOnly` answer is
        // empty, and a relative specifier is never bare, so the paths and
        // redirect buckets stay empty.
        let mut node_modules_specifier = None;
        let mut relative_specifier = None;
        for module_path in &module_paths {
            if module_path.is_in_node_modules
                && let Some(name) = self.node_module_specifier(
                    importing,
                    source_directory,
                    &module_path.file_name,
                    override_mode,
                    preferences,
                )
            {
                node_modules_specifier.get_or_insert(name);
                continue;
            }
            // "If some path to the file was in node_modules but another
            // was not, … the module specifier we actually go with will be
            // the relative path through node_modules" (`:451`).
            if relative_specifier.is_none()
                && (!imported_file_is_in_node_modules || module_path.is_in_node_modules)
            {
                relative_specifier = self.local_module_specifier(
                    importing,
                    &from,
                    &module_path.file_name,
                    import_mode,
                    preferences,
                );
            }
        }
        node_modules_specifier.or(relative_specifier)
    }

    /// The program's symlink cache (`host.GetSymlinkCache()`), which
    /// `GetEachFileNameOfModule` reads (r6-specifiers §2).
    fn known_symlinks(&self) -> Option<&KnownSymlinks> {
        self.module_host?.known_symlinks()
    }

    /// `getLocalModuleSpecifier` (`modulespecifiers/specifiers.go:481`) with
    /// no `paths`, `rootDirs` or `package.json` `imports` (none is visible
    /// to the checker's host; r5-modules2 §2): the relative path from the
    /// importing file's directory, through `processEnding`.
    fn local_module_specifier(
        &self,
        importing: NodeId,
        from: &str,
        to: &str,
        import_mode: ModuleKind,
        preferences: SpecifierPreferences,
    ) -> Option<String> {
        let source_directory = tsr_path::get_directory_path(from);
        // A path on another root has no relative spelling; upstream's
        // `GetRelativePathFromDirectory` would answer the absolute path.
        if (tsr_path::get_root_length(source_directory) > 0) != (tsr_path::get_root_length(to) > 0)
        {
            return None;
        }
        let allowed = self.allowed_endings(importing, preferences, import_mode)?;
        let options = tsr_path::ComparePathsOptions {
            use_case_sensitive_file_names: true,
            current_directory: String::new(),
        };
        let relative = ensure_path_is_non_module_name(tsr_path::get_relative_path_from_directory(
            source_directory,
            to,
            &options,
        ));
        self.process_ending(&relative, &allowed)
    }

    /// `processEnding` (`modulespecifiers/specifiers.go:636`). `None` where
    /// `getJSExtensionForFile` would panic (an extension with no JS form).
    ///
    /// `tryGetAnyFileFromPath`'s probe (keep `/index` when a file shares the
    /// directory's name) needs a file-system question the checker's host
    /// does not answer; the minimal ending always strips `/index`.
    pub(crate) fn process_ending(&self, file_name: &str, allowed: &[Ending]) -> Option<String> {
        if tsr_path::file_extension_is_one_of(file_name, &[".json", ".mjs", ".cjs"]) {
            return Some(file_name.to_string());
        }
        let no_extension = tsr_path::remove_file_extension(file_name);
        if no_extension == file_name {
            return Some(file_name.to_string());
        }
        let priority = |ending: Ending| allowed.iter().position(|&e| e == ending);
        let js_priority = priority(Ending::Js);
        let ts_priority = priority(Ending::Ts);
        // Go's `tsPriority < jsPriority` with `-1` for absent.
        let before_js = |index: Option<usize>| match (index, js_priority) {
            (Some(index), Some(js)) => index < js,
            _ => false,
        };
        if tsr_path::file_extension_is_one_of(file_name, &[".mts", ".cts"])
            && before_js(ts_priority)
        {
            return Some(file_name.to_string());
        }
        if tsr_path::file_extension_is_one_of(file_name, &[".d.mts", ".d.cts"]) {
            let input = tsr_path::get_declaration_file_extension(file_name);
            let extension = js_extension_for_declaration_file_extension(input);
            return Some(format!("{}{extension}", tsr_path::remove_extension(file_name, input)));
        }
        if tsr_path::file_extension_is_one_of(file_name, &[".mts", ".cts"]) {
            let (_, js) = self.js_extension_for_file(file_name)?;
            return Some(format!("{no_extension}{js}"));
        }
        if !tsr_path::file_extension_is(file_name, ".d.ts")
            && tsr_path::file_extension_is(file_name, ".ts")
            && file_name.contains(".d.")
            && let Some(real) = real_file_name_for_non_js_declaration_file_name(file_name)
        {
            return Some(real);
        }
        Some(match allowed.first().copied().unwrap_or(Ending::Minimal) {
            Ending::Minimal => {
                no_extension.strip_suffix("/index").unwrap_or(no_extension).to_string()
            }
            Ending::Index => no_extension.to_string(),
            Ending::Js => {
                let (_, js) = self.js_extension_for_file(file_name)?;
                format!("{no_extension}{js}")
            }
            Ending::Ts => {
                if !tsr_path::is_declaration_file_name(file_name) {
                    return Some(file_name.to_string());
                }
                let extensionless = allowed
                    .iter()
                    .position(|&ending| matches!(ending, Ending::Minimal | Ending::Index));
                if before_js(extensionless) {
                    no_extension.to_string()
                } else {
                    let (_, js) = self.js_extension_for_file(file_name)?;
                    format!("{no_extension}{js}")
                }
            }
        })
    }

    /// `GetAllowedEndingsInPreferredOrder` (`modulespecifiers/preferences.go:147`)
    /// for the node builder's preferences, under `syntax_mode`
    /// (`syntaxImpliedNodeFormat`, `ModuleKind::None` for none).
    pub(crate) fn allowed_endings(
        &self,
        importing: NodeId,
        preferences: SpecifierPreferences,
        syntax_mode: ModuleKind,
    ) -> Option<Vec<Ending>> {
        use Ending::{Index, Js, Minimal, Ts};
        let host = self.module_host?;
        let resolution_mode = host.default_resolution_mode_for_file(importing);
        let preferred = self.preferred_ending(
            importing,
            preferences,
            if resolution_mode == syntax_mode { ModuleKind::None } else { syntax_mode },
        );
        let node_next = host.specifier_options(resolution_mode).module_resolution_is_node_next;
        // `shouldAllowImportingTsExtension(options, importingSourceFile.FileName())`:
        // a declaration file may name `.ts` files.
        let allow_ts = self.allow_importing_ts_extensions
            || host
                .file_path(importing)
                .is_some_and(|path| tsr_path::is_declaration_file_name(&path));
        let effective = if syntax_mode == ModuleKind::None { resolution_mode } else { syntax_mode };
        if effective == ModuleKind::ESNext && node_next {
            return Some(if allow_ts { vec![Ts, Js] } else { vec![Js] });
        }
        Some(match preferred {
            Js if allow_ts => vec![Js, Ts, Minimal, Index],
            Js => vec![Js, Minimal, Index],
            Ts => vec![Ts, Minimal, Js, Index],
            Index if allow_ts => vec![Index, Minimal, Ts, Js],
            Index => vec![Index, Minimal, Js],
            Minimal if allow_ts => vec![Minimal, Index, Ts, Js],
            Minimal => vec![Minimal, Index, Js],
        })
    }

    /// `getPreferredEnding` (`modulespecifiers/preferences.go:121`) with no
    /// old specifier, then `getModuleSpecifierEndingPreference` (`:68`).
    fn preferred_ending(
        &self,
        importing: NodeId,
        preferences: SpecifierPreferences,
        resolution_mode: ModuleKind,
    ) -> Ending {
        let Some(host) = self.module_host else { return Ending::Minimal };
        let resolution_mode = if resolution_mode == ModuleKind::None {
            host.default_resolution_mode_for_file(importing)
        } else {
            resolution_mode
        };
        let node_next = host.specifier_options(resolution_mode).module_resolution_is_node_next;
        // `shouldAllowImportingTsExtension(compilerOptions, "")`: the option alone.
        let allow_ts = self.allow_importing_ts_extensions;
        if preferences.ending_js || resolution_mode == ModuleKind::ESNext && node_next {
            if !allow_ts {
                return Ending::Js;
            }
            return if self.infer_preference(importing, resolution_mode, node_next) == Ending::Js {
                Ending::Js
            } else {
                Ending::Ts
            };
        }
        if !allow_ts {
            return if self.uses_extensions_on_imports(importing) {
                Ending::Js
            } else {
                Ending::Minimal
            };
        }
        self.infer_preference(importing, resolution_mode, node_next)
    }

    /// `usesExtensionsOnImports` (`modulespecifiers/preferences.go:18`): the
    /// first relative import whose extension is optional decides.
    fn uses_extensions_on_imports(&self, importing: NodeId) -> bool {
        self.first_import_text(importing, |text| {
            (tsr_path::path_is_relative(text)
                && !tsr_path::file_extension_is_one_of(text, &NOT_EXTENSIONLESS))
            .then(|| has_ts_file_extension(text) || has_js_file_extension(text))
        })
        .unwrap_or(false)
    }

    /// `inferPreference` (`modulespecifiers/preferences.go:28`). JS
    /// `require` calls at the top of a file without imports are upstream's
    /// own TODO, as here.
    fn infer_preference(&self, importing: NodeId, mode: ModuleKind, node_next: bool) -> Ending {
        let mut uses_js = false;
        let decided = self.first_import_text(importing, |text| {
            if !tsr_path::path_is_relative(text)
                || node_next && mode == ModuleKind::CommonJS
                || tsr_path::file_extension_is_one_of(text, &NOT_EXTENSIONLESS)
            {
                return None;
            }
            if has_ts_file_extension(text) {
                return Some(Ending::Ts);
            }
            uses_js |= has_js_file_extension(text);
            None
        });
        decided.unwrap_or(if uses_js { Ending::Js } else { Ending::Minimal })
    }

    /// The first `Some` of `decide` over `importing`'s `Imports()` texts
    /// (`collectExternalModuleReferences`), in order. The statement-level
    /// imports are read off the tree first; the full collection (dynamic
    /// `import()`, `require`, import types) is walked only when they did not
    /// decide.
    fn first_import_text<T>(
        &self,
        importing: NodeId,
        mut decide: impl FnMut(&str) -> Option<T>,
    ) -> Option<T> {
        use tsr_ast::{Expression, ModuleReference, Statement};
        let Some(tsr_ast::Node::SourceFile(source)) = self.node_map.get(importing) else {
            return None;
        };
        for statement in source.statements {
            let specifier = match statement {
                Statement::ImportDeclaration(node) => node.module_specifier,
                Statement::ExportDeclaration(node) => node.module_specifier,
                Statement::ImportEqualsDeclaration(node) => match node.module_reference {
                    Some(ModuleReference::ExternalModuleReference(external)) => external.expression,
                    _ => None,
                },
                _ => None,
            };
            let Some(Expression::StringLiteral(literal)) = specifier else { continue };
            if let Some(found) = decide(literal.text) {
                return Some(found);
            }
        }
        let is_js_file = self.in_js_file(importing);
        if !is_js_file
            && !self
                .nodes
                .flags(importing)
                .contains(tsr_ast::NodeFlags::POSSIBLY_CONTAINS_DYNAMIC_IMPORT)
        {
            return None;
        }
        // `collectDynamicImports` (`parser/references.go`, mirrored by
        // `tsr_parser::collect_external_module_references`, which the checker
        // does not depend on): `import()` calls, `require()` in JavaScript,
        // and literal import types, by position.
        let mut dynamic: Vec<(u32, &str)> = Vec::new();
        let mut stack = vec![tsr_ast::Node::from(source)];
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            let literal = match node {
                tsr_ast::Node::CallExpression(call) => {
                    let callee_ok = match call.expression {
                        Some(Expression::KeywordExpression(keyword)) => {
                            keyword.kind == tsr_ast::SyntaxKind::ImportKeyword
                        }
                        Some(Expression::Identifier(name)) => {
                            is_js_file && name.text == "require" && call.arguments.len() == 1
                        }
                        _ => false,
                    };
                    match call.arguments.first().map(|argument| tsr_ast::Node::from(*argument)) {
                        Some(tsr_ast::Node::StringLiteral(literal)) if callee_ok => {
                            literal.node_id.map(|id| (id, literal.text))
                        }
                        Some(tsr_ast::Node::NoSubstitutionTemplateLiteral(literal))
                            if callee_ok =>
                        {
                            literal.node_id.map(|id| (id, literal.text))
                        }
                        _ => None,
                    }
                }
                tsr_ast::Node::ImportTypeNode(import) => match import.argument {
                    Some(tsr_ast::TypeNode::LiteralTypeNode(literal_type)) => {
                        match literal_type.literal {
                            Some(tsr_ast::Node::StringLiteral(literal)) => {
                                literal.node_id.map(|id| (id, literal.text))
                            }
                            _ => None,
                        }
                    }
                    _ => None,
                },
                _ => None,
            };
            if let Some((id, text)) = literal {
                dynamic.push((self.nodes.span(id).start, text));
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().copied());
        }
        dynamic.sort_by_key(|&(position, _)| position);
        dynamic.into_iter().find_map(|(_, text)| decide(text))
    }

    /// `tryGetModuleNameAsNodeModule` (`modulespecifiers/specifiers.go:743`)
    /// with `packageNameOnly = false`, for a module at `module_path`
    /// imported from `importing` (whose directory is `source_directory`).
    /// `None` is upstream's `""`: the caller falls back to the relative
    /// specifier.
    pub(crate) fn node_module_specifier(
        &self,
        importing: NodeId,
        source_directory: &str,
        module_path: &str,
        override_mode: ModuleKind,
        preferences: SpecifierPreferences,
    ) -> Option<String> {
        let parts = node_module_path_parts(module_path)?;
        let host = self.module_host?;
        let mode = if override_mode == ModuleKind::None {
            host.default_resolution_mode_for_file(importing)
        } else {
            override_mode
        };
        // `getAllowedEndingsInPreferredOrder(core.ResolutionModeNone)`.
        let allowed = self.allowed_endings(importing, preferences, ModuleKind::None)?;
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

/// `symlinks.KnownDirectoryLink` (`internal/symlinks/knownsymlinks.go:13`):
/// the real directory a directory symlink points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnownDirectoryLink {
    /// `Real`: the realpath's spelling, with a trailing separator.
    pub real: String,
    /// `RealPath`: `toPath(real)`, with a trailing separator.
    pub real_path: String,
}

/// `symlinks.KnownSymlinks` (`internal/symlinks/knownsymlinks.go:22`): the
/// program's symlink cache, which `GetEachFileNameOfModule` reads to name a
/// realpath'd module file by the symlinked paths that reach it.
///
/// Checker port convention record (`docs/conventions.md`). Native operation:
/// `Program.GetSymlinkCache` (`compiler/program.go:2059`), built once per
/// program from every resolution's `(OriginalPath, ResolvedFileName)` and
/// from each emitted package's runtime dependencies. Key identity: the
/// symlink directory's path (`directories`) and the real directory's path
/// (`directories_by_realpath`), both `toPath` spellings with a trailing
/// separator. Owner: the program (`tsr-compiler`), which builds it after
/// loading and hands it to the checker read-only through `ModuleHost`.
/// Publication: built whole before any checker exists, never mutated after,
/// so there is no partial state to observe; native's lazy `getValue` builds
/// it on first ask, which answers the same. Receiver/alias context: none (a
/// program-wide path table). Expensive work: the build (one
/// `guessDirectorySymlink` per resolution whose original path differs);
/// reads are hash lookups.
///
/// Native's sets are `SyncSet`s, iterated in no fixed order; the symlink
/// directories here keep insertion order, and every consumer sorts
/// (`getAllModulePathsWorker`), so the order never reaches an answer.
#[derive(Debug, Clone, Default)]
pub struct KnownSymlinks {
    directories: rustc_hash::FxHashMap<String, Option<KnownDirectoryLink>>,
    directories_by_realpath: rustc_hash::FxHashMap<String, Vec<String>>,
    files: rustc_hash::FxHashMap<String, String>,
    files_by_realpath: rustc_hash::FxHashMap<String, Vec<String>>,
    current_directory: String,
    use_case_sensitive_file_names: bool,
}

impl KnownSymlinks {
    /// `symlinks.NewKnownSymlink`.
    #[must_use]
    pub fn new(current_directory: &str, use_case_sensitive_file_names: bool) -> Self {
        Self {
            current_directory: current_directory.to_string(),
            use_case_sensitive_file_names,
            ..Self::default()
        }
    }

    /// `tspath.ToPath` under this cache's directory and case sensitivity.
    fn to_path(&self, file_name: &str) -> String {
        tsr_path::to_path(file_name, &self.current_directory, self.use_case_sensitive_file_names)
            .as_str()
            .to_string()
    }

    /// Whether nothing was recorded (no resolution went through a symlink).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.directories.is_empty() && self.files.is_empty()
    }

    /// `HasDirectory`.
    #[must_use]
    pub fn has_directory(&self, symlink_path: &str) -> bool {
        self.directories.contains_key(&tsr_path::ensure_trailing_directory_separator(symlink_path))
    }

    /// `DirectoriesByRealpath().Load(real_directory_path)`: the symlink
    /// directories (as spelled when recorded) that point at the real
    /// directory `real_directory_path` (a path with a trailing separator).
    #[must_use]
    pub fn symlinks_of_real_directory(&self, real_directory_path: &str) -> Option<&[String]> {
        self.directories_by_realpath.get(real_directory_path).map(Vec::as_slice)
    }

    /// `FilesByRealpath().Load(realpath)`.
    #[must_use]
    pub fn symlinks_of_real_file(&self, real_path: &str) -> Option<&[String]> {
        self.files_by_realpath.get(real_path).map(Vec::as_slice)
    }

    /// `SetDirectory`.
    pub fn set_directory(
        &mut self,
        symlink: &str,
        symlink_path: String,
        real_directory: Option<KnownDirectoryLink>,
    ) {
        if let Some(real) = &real_directory
            && !self.directories.contains_key(&symlink_path)
        {
            let set = self.directories_by_realpath.entry(real.real_path.clone()).or_default();
            if !set.iter().any(|known| known == symlink) {
                set.push(symlink.to_string());
            }
        }
        self.directories.insert(symlink_path, real_directory);
    }

    /// `SetFile`.
    pub fn set_file(&mut self, symlink: &str, symlink_path: String, realpath: &str) {
        if !self.files.contains_key(&symlink_path) {
            let realpath_path = self.to_path(realpath);
            let set = self.files_by_realpath.entry(realpath_path).or_default();
            if !set.iter().any(|known| known == symlink) {
                set.push(symlink.to_string());
            }
        }
        self.files.insert(symlink_path, realpath.to_string());
    }

    /// `ProcessResolution` (`knownsymlinks.go:94`): record that
    /// `original_path` was resolved to `resolved_file_name`, and the
    /// directory symlink `guessDirectorySymlink` infers from the two.
    pub fn process_resolution(&mut self, original_path: &str, resolved_file_name: &str) {
        if original_path.is_empty() || resolved_file_name.is_empty() {
            return;
        }
        self.set_file(original_path, self.to_path(original_path), resolved_file_name);
        if let Some((common_resolved, common_original)) =
            self.guess_directory_symlink(resolved_file_name, original_path)
        {
            let symlink_path = self.to_path(&common_original);
            if !contains_ignored_path(&symlink_path) {
                let real_path =
                    tsr_path::ensure_trailing_directory_separator(&self.to_path(&common_resolved));
                self.set_directory(
                    &common_original,
                    tsr_path::ensure_trailing_directory_separator(&symlink_path),
                    Some(KnownDirectoryLink {
                        real: tsr_path::ensure_trailing_directory_separator(&common_resolved),
                        real_path,
                    }),
                );
            }
        }
    }

    /// `guessDirectorySymlink` (`knownsymlinks.go:113`): strip the common
    /// trailing components of the two paths, stopping at a `node_modules`
    /// or `@scope` directory; the remaining prefixes are the real directory
    /// and its symlink.
    fn guess_directory_symlink(&self, a: &str, b: &str) -> Option<(String, String)> {
        let mut a_parts = tsr_path::get_path_components(
            &tsr_path::get_normalized_absolute_path(a, &self.current_directory),
            "",
        );
        let mut b_parts = tsr_path::get_path_components(
            &tsr_path::get_normalized_absolute_path(b, &self.current_directory),
            "",
        );
        let canonical = |name: &str| {
            tsr_path::get_canonical_file_name(name, self.use_case_sensitive_file_names)
        };
        let is_node_modules_or_scope = |name: &str| {
            !name.is_empty() && (canonical(name) == "node_modules" || name.starts_with('@'))
        };
        let mut is_directory = false;
        while a_parts.len() >= 2
            && b_parts.len() >= 2
            && !is_node_modules_or_scope(&a_parts[a_parts.len() - 2])
            && !is_node_modules_or_scope(&b_parts[b_parts.len() - 2])
            && canonical(&a_parts[a_parts.len() - 1]) == canonical(&b_parts[b_parts.len() - 1])
        {
            a_parts.pop();
            b_parts.pop();
            is_directory = true;
        }
        is_directory.then(|| {
            (
                tsr_path::get_path_from_path_components(&a_parts),
                tsr_path::get_path_from_path_components(&b_parts),
            )
        })
    }
}

/// `tspath.ContainsIgnoredPath` (`tspath/ignoredpaths.go:11`), which
/// `modulespecifiers.containsIgnoredPath` duplicates.
fn contains_ignored_path(path: &str) -> bool {
    path.contains("/node_modules/.") || path.contains("/.git") || path.contains(".#")
}

/// `tspath.StartsWithDirectory` (`tspath/path.go:1203`).
fn starts_with_directory(file_name: &str, directory_name: &str, case_sensitive: bool) -> bool {
    if directory_name.is_empty() {
        return false;
    }
    let file = tsr_path::get_canonical_file_name(file_name, case_sensitive);
    let directory = tsr_path::get_canonical_file_name(directory_name, case_sensitive);
    let directory = directory.trim_end_matches('/').trim_end_matches('\\');
    file.starts_with(&format!("{directory}/")) || file.starts_with(&format!("{directory}\\"))
}

/// `ModulePath` (`modulespecifiers/types.go:36`). `IsRedirect` (a project
/// reference's output) is never set: this port has no project-reference
/// redirects (r6-specifiers §2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePath {
    /// `FileName`.
    pub file_name: String,
    /// `IsInNodeModules`: the path contains `/node_modules/`.
    pub is_in_node_modules: bool,
}

impl ModulePath {
    fn new(file_name: String) -> Self {
        let is_in_node_modules = file_name.contains("/node_modules/");
        Self { file_name, is_in_node_modules }
    }
}

/// `GetEachFileNameOfModule` (`modulespecifiers/specifiers.go:260`) with
/// `preferSymlinks = true`, the only value `getAllModulePathsWorker` passes:
/// every path through a known directory symlink that reaches
/// `imported_file_name`, then the file's own (real) path.
///
/// `GetProjectReferenceFromSource` and `GetRedirectTargets` answer nothing
/// here (no project references; duplicate-package redirects are not
/// exposed to the checker), so the targets are the one imported file. The
/// global typings cache is empty in every program this port builds, so the
/// ancestor walk stops only at the root.
#[must_use]
pub fn each_file_name_of_module(
    importing_file_name: &str,
    imported_file_name: &str,
    symlinks: Option<&KnownSymlinks>,
    current_directory: &str,
    case_sensitive: bool,
) -> Vec<ModulePath> {
    let target = tsr_path::get_normalized_absolute_path(imported_file_name, current_directory);
    let targets = [target];
    let mut should_filter_ignored_paths = !targets.iter().all(|path| contains_ignored_path(path));
    let mut results = Vec::with_capacity(2);
    if let Some(symlinks) = symlinks.filter(|symlinks| !symlinks.is_empty()) {
        let mut real_path_directory = tsr_path::get_directory_path(&targets[0]).to_string();
        loop {
            let key = tsr_path::ensure_trailing_directory_separator(
                tsr_path::to_path(&real_path_directory, current_directory, case_sensitive).as_str(),
            );
            if let Some(symlink_set) = symlinks.symlinks_of_real_directory(&key) {
                // "Don't want to a package to globally import from itself":
                // every ancestor hits the same test, so the walk stops.
                if starts_with_directory(importing_file_name, &real_path_directory, case_sensitive)
                {
                    break;
                }
                for target in &targets {
                    if !starts_with_directory(target, &real_path_directory, case_sensitive) {
                        continue;
                    }
                    let relative = tsr_path::get_relative_path_from_directory(
                        &real_path_directory,
                        target,
                        &tsr_path::ComparePathsOptions {
                            use_case_sensitive_file_names: case_sensitive,
                            current_directory: current_directory.to_string(),
                        },
                    );
                    for symlink_directory in symlink_set {
                        let option = tsr_path::normalize_path(&tsr_path::combine_paths(
                            symlink_directory,
                            &[&relative],
                        ));
                        results.push(ModulePath::new(option));
                        should_filter_ignored_paths = true;
                    }
                }
            }
            let parent = tsr_path::get_directory_path(&real_path_directory).to_string();
            if parent == real_path_directory {
                break;
            }
            real_path_directory = parent;
        }
    }
    for path in targets {
        if !(should_filter_ignored_paths && contains_ignored_path(&path)) {
            results.push(ModulePath::new(path));
        }
    }
    results
}

/// `getAllModulePathsWorker` (`modulespecifiers/specifiers.go:198`): the
/// module's paths ordered by closeness to the importing file's directory
/// (paths under it first, then under each ancestor), ties broken by
/// `comparePathsByRedirect` (fewer directory separators, then the path).
#[must_use]
pub fn all_module_paths(
    importing_file_name: &str,
    imported_file_name: &str,
    symlinks: Option<&KnownSymlinks>,
    current_directory: &str,
    case_sensitive: bool,
) -> Vec<ModulePath> {
    let paths = each_file_name_of_module(
        importing_file_name,
        imported_file_name,
        symlinks,
        current_directory,
        case_sensitive,
    );
    // `allFileNames`, a map keyed by file name: a later duplicate replaces
    // the earlier entry, which is the same path.
    let mut remaining: Vec<ModulePath> = Vec::with_capacity(paths.len());
    for path in paths {
        if let Some(existing) = remaining.iter_mut().find(|p| p.file_name == path.file_name) {
            *existing = path;
        } else {
            remaining.push(path);
        }
    }
    let compare = |a: &ModulePath, b: &ModulePath| {
        let separators = |path: &str| path.bytes().filter(|&byte| byte == b'/').count();
        separators(&a.file_name).cmp(&separators(&b.file_name)).then_with(|| {
            tsr_path::compare_paths(
                &a.file_name,
                &b.file_name,
                &tsr_path::ComparePathsOptions {
                    use_case_sensitive_file_names: case_sensitive,
                    current_directory: String::new(),
                },
            )
        })
    };
    let mut sorted = Vec::with_capacity(remaining.len());
    let mut directory = tsr_path::get_directory_path(importing_file_name).to_string();
    while !remaining.is_empty() {
        let directory_start = tsr_path::ensure_trailing_directory_separator(&directory);
        let (mut in_directory, rest): (Vec<_>, Vec<_>) =
            remaining.into_iter().partition(|path| path.file_name.starts_with(&directory_start));
        remaining = rest;
        in_directory.sort_by(compare);
        sorted.extend(in_directory);
        let parent = tsr_path::get_directory_path(&directory).to_string();
        if parent == directory {
            break;
        }
        directory = parent;
    }
    remaining.sort_by(compare);
    sorted.extend(remaining);
    sorted
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
    use super::{
        KnownSymlinks, all_module_paths, each_file_name_of_module, ensure_path_is_non_module_name,
        js_extension_for_declaration_file_extension, node_module_path_parts,
        package_name_from_types_package_name, real_file_name_for_non_js_declaration_file_name,
    };

    /// `symlinkedWorkspaceDependenciesNoDirectLinkGeneratesNonrelativeName`'s
    /// layout: `packageA` linked into two packages' `node_modules`.
    fn workspace_symlinks() -> KnownSymlinks {
        let mut symlinks = KnownSymlinks::new("/", true);
        symlinks.process_resolution(
            "/w/packageB/node_modules/package-a/index.d.ts",
            "/w/packageA/index.d.ts",
        );
        symlinks.process_resolution(
            "/w/packageC/node_modules/package-a/package.json",
            "/w/packageA/package.json",
        );
        symlinks
    }

    #[test]
    fn a_resolution_through_a_link_records_the_directory_symlink() {
        // `guessDirectorySymlink` strips the shared `index.d.ts` and stops
        // at the `node_modules` parent.
        let symlinks = workspace_symlinks();
        assert!(symlinks.has_directory("/w/packageB/node_modules/package-a"));
        assert!(symlinks.has_directory("/w/packageC/node_modules/package-a/"));
        assert_eq!(
            symlinks.symlinks_of_real_directory("/w/packageA/"),
            Some(
                &[
                    "/w/packageB/node_modules/package-a".to_string(),
                    "/w/packageC/node_modules/package-a".to_string(),
                ][..]
            )
        );
        // The same path on both sides records nothing.
        let mut none = KnownSymlinks::new("/", true);
        none.process_resolution("", "/a/b.ts");
        assert!(none.is_empty());
    }

    #[test]
    fn each_file_name_puts_the_symlinked_paths_before_the_realpath() {
        let symlinks = workspace_symlinks();
        let paths = each_file_name_of_module(
            "/w/packageC/index.ts",
            "/w/packageA/index.d.ts",
            Some(&symlinks),
            "/",
            true,
        );
        let names: Vec<_> = paths.iter().map(|path| path.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "/w/packageB/node_modules/package-a/index.d.ts",
                "/w/packageC/node_modules/package-a/index.d.ts",
                "/w/packageA/index.d.ts",
            ]
        );
        assert!(paths[0].is_in_node_modules && !paths[2].is_in_node_modules);
        // A package never names itself through its own links.
        let own = each_file_name_of_module(
            "/w/packageA/src/x.ts",
            "/w/packageA/index.d.ts",
            Some(&symlinks),
            "/",
            true,
        );
        assert_eq!(own.len(), 1);
    }

    #[test]
    fn module_paths_sort_nearest_the_importing_directory_first() {
        let symlinks = workspace_symlinks();
        let paths = all_module_paths(
            "/w/packageC/index.ts",
            "/w/packageA/index.d.ts",
            Some(&symlinks),
            "/",
            true,
        );
        let names: Vec<_> = paths.iter().map(|path| path.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "/w/packageC/node_modules/package-a/index.d.ts",
                "/w/packageA/index.d.ts",
                "/w/packageB/node_modules/package-a/index.d.ts",
            ]
        );
    }

    #[test]
    fn non_js_declaration_files_remap_to_their_real_names() {
        // `TryGetRealFileNameForNonJSDeclarationFileName` (`util.go:159`).
        assert_eq!(
            real_file_name_for_non_js_declaration_file_name("./foo.d.html.ts").as_deref(),
            Some("./foo.html")
        );
        assert_eq!(
            real_file_name_for_non_js_declaration_file_name("./a/foo.module.d.css.ts").as_deref(),
            Some("./a/foo.module.css")
        );
        assert_eq!(real_file_name_for_non_js_declaration_file_name("./foo.d.ts"), None);
        assert_eq!(real_file_name_for_non_js_declaration_file_name("./a.d/foo.ts"), None);
        assert_eq!(js_extension_for_declaration_file_extension(".d.json.ts"), ".json");
        assert_eq!(js_extension_for_declaration_file_extension(".d.mts"), ".mjs");
    }

    #[test]
    fn bare_paths_become_relative() {
        assert_eq!(ensure_path_is_non_module_name("foo".into()), "./foo");
        assert_eq!(ensure_path_is_non_module_name("../foo".into()), "../foo");
        assert_eq!(ensure_path_is_non_module_name("/foo".into()), "/foo");
    }

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
