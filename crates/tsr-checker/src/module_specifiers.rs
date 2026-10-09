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
    /// `import` attribute. A specifier still diving into `/node_modules/` is
    /// generated again in the swapped mode and, if that one does not, written
    /// with the swapped mode's attribute. The tracker report for a specifier
    /// that stays unportable is `track_unsafe_import`'s (r5-modules §6).
    /// `GetEmitModuleFormatOfFile` is `ModuleHost::implied_node_format_for_emit`,
    /// as there.
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
        let mut specifier = match specifier {
            Some(specifier) => specifier,
            None => {
                self.module_specifier_for_symbol_in_mode(module, reference, ModuleKind::None)?
            }
        };
        if specifier.contains("/node_modules/") {
            let swapped =
                if context_file.is_some_and(|context| format(context) == ModuleKind::ESNext) {
                    ModuleKind::CommonJS
                } else {
                    ModuleKind::ESNext
                };
            if let Some(retry) =
                self.module_specifier_for_symbol_in_mode(module, reference, swapped)
                && !retry.contains("/node_modules/")
            {
                specifier = retry;
                attribute = Some(if swapped == ModuleKind::ESNext { "import" } else { "require" });
            }
        }
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
    /// reference's. The module paths are the file's own path only:
    /// `GetEachFileNameOfModule`'s symlink and redirect alternatives are not
    /// ported (r5-modules2 §2), so the per-path loops run once.
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
        // The `IsInNodeModules` arm: a package-name specifier, after which
        // `getLocalModuleSpecifier` runs `pathsOnly` and (with no `paths`)
        // answers nothing, so the package name wins.
        if to.contains("/node_modules/")
            && let Some(name) = self.node_module_specifier(
                importing,
                source_directory,
                &to,
                override_mode,
                preferences,
            )
        {
            return Some(name);
        }
        let import_mode = if override_mode == ModuleKind::None {
            host.default_resolution_mode_for_file(importing)
        } else {
            override_mode
        };
        self.local_module_specifier(importing, &from, &to, import_mode, preferences)
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
        ensure_path_is_non_module_name, js_extension_for_declaration_file_extension,
        node_module_path_parts, package_name_from_types_package_name,
        real_file_name_for_non_js_declaration_file_name,
    };

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
