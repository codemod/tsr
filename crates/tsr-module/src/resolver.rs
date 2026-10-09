//! The resolution state machine.
//!
//! Ported from `internal/module/resolver.go` and `internal/module/cache.go` at
//! the pinned commit.
//!
//! # What the shape of this file is for
//!
//! Resolution is not a function from specifier to file. It is a *walk*, and its
//! oracle is the walk rather than the destination: 146 committed `.trace.json`
//! baselines record every file probe, every directory probe, every `package.json`
//! read, and every `exports` condition considered, in order. A resolver that
//! reaches the right file by probing in a different order fails every one of
//! them.
//!
//! That is why this is a near-transliteration of upstream rather than an
//! idiomatic rewrite — an exception to this project's usual rule, and a
//! deliberate one. Two consequences worth naming:
//!
//! - **Search results are three-valued.** [`Search`] distinguishes "keep
//!   looking" from "stop, with nothing" from "stop, with this file". Upstream
//!   spells these `nil`, `&resolved{}`, and a populated pointer. Collapsing the
//!   first two — the obvious simplification — changes which fallbacks run.
//! - **Loaders are an enum, not closures.** Upstream passes
//!   `resolutionKindSpecificLoader` function values that capture mutable
//!   resolution state and are then called with that same state; Rust will not
//!   allow it, so [`Loader`] names the four call sites instead.
//!
//! # Which resolution kinds exist here
//!
//! Only `node16`, `nodenext`, and `bundler`. `node10` and `classic` are **not
//! ported upstream** — `resolver.go` contains no mention of either, and
//! typescript-go's test harness skips every case that would need them, which is
//! why 46 of the corpus's 155 `@traceResolution` cases have no baseline at all.
//! See `docs/architecture/module-resolution.md`.

use std::{cell::RefCell, rc::Rc};

use rustc_hash::FxHashMap;
use tsr_core::{CompilerOptions, ModuleResolutionKind, OrderedMap, ResolutionMode, Tristate};
use tsr_path::{
    ComparePathsOptions, combine_paths, contains_path, ensure_trailing_directory_separator,
    extension::*, get_base_file_name, get_directory_path, get_normalized_absolute_path,
    get_path_components, get_relative_path_from_directory, has_trailing_directory_separator,
    is_external_module_name_relative, is_rooted_disk_path, normalize_path, path_is_relative,
    remove_trailing_directory_separator, to_path,
};

use crate::{
    json::Json,
    messages::{self, Message, Trace},
    package_json::{PackageJson, TYPESCRIPT_VERSION, VersionPaths},
    types::{
        Extensions, NodeResolutionFeatures, PackageId, ResolutionHost, ResolvedModule,
        ResolvedTypeReferenceDirective, extension_is_ok, get_conditions,
        get_node_resolution_features,
    },
    util::{
        self, INFERRED_TYPES_CONTAINING_FILE, ParsedPatterns, compare_pattern_keys,
        mangle_scoped_package_name, match_pattern_or_exact, matches_pattern_with_trailer,
        normalize_path_for_cjs_resolution, parse_node_module_from_path, parse_package_name,
        try_parse_patterns,
    },
};

/// A file a search settled on (`module.resolved`).
///
/// The field names are upstream's, including the one clippy objects to:
/// `resolved_using_ts_extension` is what the checker calls it, and renaming it
/// here would break the anchor to `module.resolved`.
#[derive(Debug, Clone, Default)]
#[allow(clippy::struct_field_names)]
pub struct Resolved {
    path: String,
    extension: String,
    package_id: PackageId,
    original_path: String,
    resolved_using_ts_extension: bool,
}

impl Resolved {
    fn is_resolved(&self) -> bool {
        !self.path.is_empty()
    }
}

/// The three-valued result of one step of the search.
///
/// `None` means *keep looking*; `Some` means *stop*, whether or not it found
/// anything. See the module docs for why the distinction cannot be collapsed.
type Search = Option<Resolved>;

/// The output extensions an `exports` target may name, which
/// [`ResolutionState::try_load_input_file_for_path`] maps back to inputs.
const JS_AND_DTS: &[&str] = &[
    EXTENSION_MJS,
    EXTENSION_CJS,
    EXTENSION_JS,
    EXTENSION_JSON,
    EXTENSION_DMTS,
    EXTENSION_DCTS,
    EXTENSION_DTS,
];

/// Keep looking (`module.continueSearching`).
const fn continue_searching() -> Search {
    None
}

/// Stop, having found nothing (`module.unresolved`).
///
/// The `Option` is not redundant: this is the *middle* of the three states, and
/// its whole job is to be `Some` so callers stop searching. See [`Search`].
#[allow(clippy::unnecessary_wraps)]
fn unresolved() -> Search {
    Some(Resolved::default())
}

/// A cached `package.json` lookup (`packagejson.InfoCacheEntry`).
#[derive(Debug)]
pub struct PackageJsonInfo {
    /// The directory the `package.json` was looked for in.
    pub package_directory: String,
    /// Whether that directory existed.
    pub directory_exists: bool,
    /// The parsed file, if there was one.
    pub contents: Option<Rc<PackageJson>>,
}

impl PackageJsonInfo {
    /// Whether a `package.json` was actually found (`InfoCacheEntry.Exists`).
    fn exists(&self) -> bool {
        self.contents.is_some()
    }
}

/// Whether an optional entry holds a real `package.json`.
fn info_exists(info: Option<&Rc<PackageJsonInfo>>) -> bool {
    info.is_some_and(|info| info.exists())
}

/// Which of the four call sites a nested lookup came from.
///
/// Upstream's `resolutionKindSpecificLoader` closures, reified. See the module
/// docs.
#[derive(Debug, Clone)]
enum Loader {
    /// `nodeLoadModuleByRelativeName(extensions, candidate, considerPackageJson: true)`.
    NodeRelative,
    /// The loader inside `loadModuleFromSpecificNodeModulesDirectory`.
    SpecificNodeModules { rest: String, package_info: Option<Rc<PackageJsonInfo>> },
    /// The loader inside `loadNodeModuleFromDirectoryWorker`.
    NodeModuleDirectory { package_file: String, package_info: Option<Rc<PackageJsonInfo>> },
}

/// Native `moduleResolutionCacheKey` (`internal/module/cache.go`). Options are
/// fixed for this resolver; project-reference redirects are not supported yet.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ModuleResolutionCacheKey {
    containing_directory: String,
    name: String,
    resolution_mode: u8,
}

/// Native `typeRefDirectiveResolutionCacheKey` (`internal/module/cache.go`).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct TypeReferenceCacheKey {
    request: ModuleResolutionCacheKey,
    from_inferred_types_containing_file: bool,
}

/// A configured resolver, with its caches (`module.Resolver`).
///
/// One per program. The `package.json` cache is not an optimisation here: the
/// `File '{0}' exists according to earlier cached lookups.` trace lines are
/// *emitted from the cache hit*, so a resolver without one produces different
/// output, not merely slower output.
pub struct Resolver<'host> {
    host: &'host dyn ResolutionHost,
    compiler_options: CompilerOptions,
    package_json_cache: RefCell<FxHashMap<tsr_path::Path, Rc<PackageJsonInfo>>>,
    module_resolution_cache: RefCell<FxHashMap<ModuleResolutionCacheKey, ResolvedModule>>,
    type_reference_cache: RefCell<FxHashMap<TypeReferenceCacheKey, ResolvedTypeReferenceDirective>>,
    parsed_patterns_for_paths: ParsedPatterns,
}

impl<'host> Resolver<'host> {
    /// Build a resolver over a host and its options (`module.NewResolver`).
    #[must_use]
    pub fn new(host: &'host dyn ResolutionHost, compiler_options: CompilerOptions) -> Self {
        let parsed_patterns_for_paths = try_parse_patterns(&compiler_options.paths);
        Self {
            host,
            compiler_options,
            package_json_cache: RefCell::new(FxHashMap::default()),
            module_resolution_cache: RefCell::new(FxHashMap::default()),
            type_reference_cache: RefCell::new(FxHashMap::default()),
            parsed_patterns_for_paths,
        }
    }

    /// The options this resolver was built with.
    #[must_use]
    pub fn compiler_options(&self) -> &CompilerOptions {
        &self.compiler_options
    }

    /// The nearest enclosing `package.json`, if any
    /// (`Resolver.GetPackageScopeForPath`).
    ///
    /// The file loader asks this of every file it parses, to derive the file's
    /// module format from the `type` field. It is *not* a resolution: upstream
    /// runs it through a `resolutionState` with tracing off, so the lookups it
    /// performs produce no trace lines even under `traceResolution`. They do
    /// warm the `package.json` cache, which later resolutions observe — the
    /// baseline sanitiser exists precisely to make that unobservable.
    #[must_use]
    pub fn get_package_scope_for_path(&self, directory: &str) -> Option<Rc<PackageJsonInfo>> {
        let mut state = ResolutionState::new(
            self,
            String::new(),
            directory.to_string(),
            /* is_type_reference_directive */ false,
            ResolutionMode::None,
            /* tracing */ false,
        );
        state.get_package_scope_for_path(directory)
    }

    /// Resolve an `import` specifier (`Resolver.ResolveModuleName`).
    ///
    /// Returns the resolution and the trace lines it produced. The traces are
    /// *returned* rather than written to the host as they happen, because the
    /// file loader replays them in a deterministic order that parallel resolution
    /// would otherwise scramble — see `tsr-compiler`'s loader.
    ///
    /// # Panics
    ///
    /// If the effective resolution kind is `node10` or `classic`, neither of
    /// which is ported upstream. The conformance harness filters those cases out
    /// before they get here, exactly as typescript-go's own harness does.
    pub fn resolve_module_name(
        &self,
        module_name: &str,
        containing_file: &str,
        resolution_mode: ResolutionMode,
    ) -> (ResolvedModule, Vec<Trace>) {
        let containing_directory = get_directory_path(containing_file).to_string();
        let tracing = self.compiler_options.trace_resolution == Tristate::True;
        let cache_key = ModuleResolutionCacheKey {
            containing_directory: containing_directory.clone(),
            name: module_name.to_string(),
            resolution_mode: resolution_mode as u8,
        };
        // Native bypasses query-cache reads when tracing, so each request still
        // emits its full walk. Its package-JSON cache remains observable.
        if !tracing && let Some(cached) = self.module_resolution_cache.borrow().get(&cache_key) {
            return (cached.clone(), Vec::new());
        }

        let mut state = ResolutionState::new(
            self,
            module_name.to_string(),
            containing_directory,
            /* is_type_reference_directive */ false,
            resolution_mode,
            tracing,
        );

        state.trace(&messages::RESOLVING_MODULE_0_FROM_1, &[module_name, containing_file]);

        let module_resolution = self.compiler_options.module_resolution_kind();
        if self.compiler_options.module_resolution == module_resolution {
            state.trace(
                &messages::EXPLICITLY_SPECIFIED_MODULE_RESOLUTION_KIND_COLON_0,
                &[&module_resolution.to_string()],
            );
        } else {
            state.trace(
                &messages::MODULE_RESOLUTION_KIND_IS_NOT_SPECIFIED_USING_0,
                &[&module_resolution.to_string()],
            );
        }

        assert!(
            matches!(
                module_resolution,
                ModuleResolutionKind::Node16
                    | ModuleResolutionKind::NodeNext
                    | ModuleResolutionKind::Bundler
            ),
            "unexpected moduleResolution: {module_resolution}"
        );
        let result = state.resolve_node_like();

        if result.is_resolved() {
            if result.package_id.is_set() {
                state.trace(
                    &messages::MODULE_NAME_0_WAS_SUCCESSFULLY_RESOLVED_TO_1_WITH_PACKAGE_ID_2,
                    &[module_name, &result.resolved_file_name, &result.package_id.to_string()],
                );
            } else {
                state.trace(
                    &messages::MODULE_NAME_0_WAS_SUCCESSFULLY_RESOLVED_TO_1,
                    &[module_name, &result.resolved_file_name],
                );
            }
        } else {
            state.trace(&messages::MODULE_NAME_0_WAS_NOT_RESOLVED, &[module_name]);
        }

        // `moduleResolutionCache.Set` keeps the first result, including failure.
        self.module_resolution_cache
            .borrow_mut()
            .entry(cache_key)
            .or_insert_with(|| result.clone());
        (result, state.traces)
    }

    /// `Resolver.ResolvePackageDirectory` (`module/resolver.go:331`): the
    /// directory of package `module_name` in the nearest `node_modules`
    /// above `containing_file`, with its realpath. `None` when no
    /// `node_modules` holds it. The program's symlink cache asks it for each
    /// runtime dependency of an emitted file's package
    /// (`Program.GetSymlinkCache`, `compiler/program.go:2095`). Untraced and
    /// uncached, as upstream's.
    #[must_use]
    pub fn resolve_package_directory(
        &self,
        module_name: &str,
        containing_file: &str,
        resolution_mode: ResolutionMode,
    ) -> Option<ResolvedModule> {
        let containing_directory = get_directory_path(containing_file).to_string();
        let mut state = ResolutionState::new(
            self,
            module_name.to_string(),
            containing_directory,
            /* is_type_reference_directive */ false,
            resolution_mode,
            /* tracing */ false,
        );
        state.resolve_package_directory_only = true;
        let result = state
            .load_module_from_nearest_node_modules_directory(/* types_scope_only */ false)
            .filter(|result| !result.path.is_empty())?;
        Some(state.create_resolved_module_handling_symlink(Some(result)))
    }

    /// Resolve a `tsconfig.json`'s `extends` target (`module.ResolveConfig`).
    ///
    /// `resolver.go:2077` plus `resolveConfig` (`:371`). Three things make it
    /// different from an ordinary module resolution, and all three already exist
    /// in [`ResolutionState`] — this is the entry point that was missing, not
    /// the machinery:
    ///
    /// - **`is_config_lookup`**, which makes a bare directory resolve to
    ///   `tsconfig.json` rather than `index.js` and changes which
    ///   `package.json` field is consulted.
    /// - **JSON-only extensions**: an `extends` target is a config file, so a
    ///   sibling `.ts` must not satisfy it.
    /// - **`nodenext` regardless of the project's own `moduleResolution`**,
    ///   because the config has not been read yet — its `moduleResolution` is
    ///   the thing being resolved *toward*.
    ///
    /// `containing_file` is the config doing the extending; resolution starts
    /// from its directory.
    #[must_use]
    pub fn resolve_config(&self, module_name: &str, containing_file: &str) -> ResolvedModule {
        let containing_directory = get_directory_path(containing_file).to_string();
        let mut state = ResolutionState::new(
            self,
            module_name.to_string(),
            containing_directory,
            /* is_type_reference_directive */ false,
            ResolutionMode::CommonJS,
            /* tracing */ false,
        );
        state.is_config_lookup = true;
        state.extensions = Extensions::JSON;
        state.resolve_node_like()
    }

    /// Resolve a `/// <reference types="..." />`
    /// (`Resolver.ResolveTypeReferenceDirective`).
    pub fn resolve_type_reference_directive(
        &self,
        type_reference_directive_name: &str,
        containing_file: &str,
        resolution_mode: ResolutionMode,
    ) -> (ResolvedTypeReferenceDirective, Vec<Trace>) {
        let containing_directory = get_directory_path(containing_file).to_string();
        let tracing = self.compiler_options.trace_resolution == Tristate::True;
        let from_inferred_types_containing_file =
            containing_file.ends_with(INFERRED_TYPES_CONTAINING_FILE);
        let cache_key = TypeReferenceCacheKey {
            request: ModuleResolutionCacheKey {
                containing_directory: containing_directory.clone(),
                name: type_reference_directive_name.to_string(),
                resolution_mode: resolution_mode as u8,
            },
            from_inferred_types_containing_file,
        };
        if !tracing && let Some(cached) = self.type_reference_cache.borrow().get(&cache_key) {
            return (cached.clone(), Vec::new());
        }

        let (type_roots, from_config) =
            self.compiler_options.get_effective_type_roots(self.host.current_directory());

        let mut state = ResolutionState::new(
            self,
            type_reference_directive_name.to_string(),
            containing_directory,
            /* is_type_reference_directive */ true,
            resolution_mode,
            tracing,
        );
        state.trace(
            &messages::RESOLVING_TYPE_REFERENCE_DIRECTIVE_0_CONTAINING_FILE_1_ROOT_DIRECTORY_2,
            &[type_reference_directive_name, containing_file, &type_roots.join(",")],
        );

        let result = state.resolve_type_reference_directive(
            &type_roots,
            from_config,
            from_inferred_types_containing_file,
        );

        if result.is_resolved() {
            let primary = if result.primary { "true" } else { "false" };
            if result.package_id.is_set() {
                state.trace(
                    &messages::TYPE_REFERENCE_DIRECTIVE_0_WAS_SUCCESSFULLY_RESOLVED_TO_1_WITH_PACKAGE_ID_2_PRIMARY_COLON_3,
                    &[
                        type_reference_directive_name,
                        &result.resolved_file_name,
                        &result.package_id.to_string(),
                        primary,
                    ],
                );
            } else {
                state.trace(
                    &messages::TYPE_REFERENCE_DIRECTIVE_0_WAS_SUCCESSFULLY_RESOLVED_TO_1_PRIMARY_COLON_2,
                    &[type_reference_directive_name, &result.resolved_file_name, primary],
                );
            }
        } else {
            state.trace(
                &messages::TYPE_REFERENCE_DIRECTIVE_0_WAS_NOT_RESOLVED,
                &[type_reference_directive_name],
            );
        }

        self.type_reference_cache.borrow_mut().insert(cache_key, result.clone());
        (result, state.traces)
    }
}

/// One in-flight resolution (`module.resolutionState`).
///
/// The flags are upstream's fields, each read at a different point in the walk;
/// packing them into an enum would obscure which combinations actually occur.
#[allow(clippy::struct_excessive_bools)]
struct ResolutionState<'a, 'host> {
    resolver: &'a Resolver<'host>,
    tracing: bool,
    traces: Vec<Trace>,

    // The request.
    name: String,
    containing_directory: String,
    is_config_lookup: bool,
    features: NodeResolutionFeatures,
    esm_mode: bool,
    conditions: Vec<String>,
    extensions: Extensions,

    // Mutable search state.
    candidate_ending_is_from_config: bool,
    resolved_package_directory: bool,
    /// `resolvePackageDirectoryOnly`: [`Resolver::resolve_package_directory`].
    resolve_package_directory_only: bool,
    diagnostics: Vec<Trace>,
}

impl<'a, 'host> ResolutionState<'a, 'host> {
    fn new(
        resolver: &'a Resolver<'host>,
        name: String,
        containing_directory: String,
        is_type_reference_directive: bool,
        resolution_mode: ResolutionMode,
        tracing: bool,
    ) -> Self {
        let options = &resolver.compiler_options;

        let mut extensions = if is_type_reference_directive {
            Extensions::DECLARATION
        } else if options.no_dts_resolution == Tristate::True {
            Extensions::IMPLEMENTATION_FILES
        } else {
            Extensions::TYPESCRIPT.union(Extensions::JAVASCRIPT).union(Extensions::DECLARATION)
        };
        if !is_type_reference_directive && options.get_resolve_json_module() {
            extensions = extensions.union(Extensions::JSON);
        }

        let (features, esm_mode, conditions) = match options.module_resolution_kind() {
            ModuleResolutionKind::Node16 => (
                NodeResolutionFeatures::NODE16_DEFAULT,
                resolution_mode == ResolutionMode::ESNext,
                get_conditions(options, resolution_mode),
            ),
            ModuleResolutionKind::NodeNext => (
                NodeResolutionFeatures::NODENEXT_DEFAULT,
                resolution_mode == ResolutionMode::ESNext,
                get_conditions(options, resolution_mode),
            ),
            // Bundler never sets `esmMode`: it has no format split to detect.
            ModuleResolutionKind::Bundler => (
                get_node_resolution_features(options),
                false,
                get_conditions(options, resolution_mode),
            ),
            _ => (NodeResolutionFeatures::NONE, false, Vec::new()),
        };

        Self {
            resolver,
            tracing,
            traces: Vec::new(),
            name,
            containing_directory,
            is_config_lookup: false,
            features,
            esm_mode,
            conditions,
            extensions,
            candidate_ending_is_from_config: false,
            resolved_package_directory: false,
            resolve_package_directory_only: false,
            diagnostics: Vec::new(),
        }
    }

    fn options(&self) -> &CompilerOptions {
        &self.resolver.compiler_options
    }

    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        self.resolver.host.fs()
    }

    fn current_directory(&self) -> &str {
        self.resolver.host.current_directory()
    }

    fn compare_paths_options(&self) -> ComparePathsOptions {
        ComparePathsOptions {
            use_case_sensitive_file_names: self.fs().use_case_sensitive_file_names(),
            current_directory: self.current_directory().to_string(),
        }
    }

    fn trace(&mut self, message: &Message, args: &[&str]) {
        if self.tracing {
            self.traces.push(Trace { code: message.code, text: message.format(args) });
        }
    }

    // ---- Type reference directives -----------------------------------------

    /// `resolutionState.resolveTypeReferenceDirective`.
    fn resolve_type_reference_directive(
        &mut self,
        type_roots: &[String],
        from_config: bool,
        from_inferred_types_containing_file: bool,
    ) -> ResolvedTypeReferenceDirective {
        if type_roots.is_empty() {
            self.trace(
                &messages::ROOT_DIRECTORY_CANNOT_BE_DETERMINED_SKIPPING_PRIMARY_SEARCH_PATHS,
                &[],
            );
        } else {
            self.trace(&messages::RESOLVING_WITH_PRIMARY_SEARCH_PATH_0, &[&type_roots.join(", ")]);
            for type_root in type_roots {
                let candidate = self.candidate_from_type_root(type_root);
                if !self.fs().directory_exists(type_root) {
                    self.trace(
                        &messages::DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT,
                        &[type_root],
                    );
                    continue;
                }
                if from_config
                    && let Some(mut resolved) =
                        self.load_module_from_file(Extensions::DECLARATION, &candidate)
                {
                    let package_directory = parse_node_module_from_path(&resolved.path, false);
                    if !package_directory.is_empty() {
                        let info = self.get_package_json_info(&package_directory);
                        resolved.package_id = self.get_package_id(&resolved.path, info.as_ref());
                    }
                    return self.create_resolved_type_reference_directive(Some(resolved), true);
                }
                if let Some(resolved) =
                    self.load_node_module_from_directory(Extensions::DECLARATION, &candidate, true)
                {
                    return self.create_resolved_type_reference_directive(Some(resolved), true);
                }
            }
        }

        // Secondary lookup, in `node_modules`.
        let resolved = if from_config && from_inferred_types_containing_file {
            self.trace(&messages::RESOLVING_TYPE_REFERENCE_DIRECTIVE_FOR_PROGRAM_THAT_SPECIFIES_CUSTOM_TYPEROOTS_SKIPPING_LOOKUP_IN_NODE_MODULES_FOLDER, &[]);
            continue_searching()
        } else {
            let containing_directory = self.containing_directory.clone();
            self.trace(
                &messages::LOOKING_UP_IN_NODE_MODULES_FOLDER_INITIAL_LOCATION_0,
                &[&containing_directory],
            );
            if is_external_module_name_relative(&self.name) {
                let candidate =
                    normalize_path_for_cjs_resolution(&containing_directory, &self.name.clone());
                self.node_load_module_by_relative_name(Extensions::DECLARATION, &candidate, true)
            } else {
                self.load_module_from_nearest_node_modules_directory(false)
            }
        };
        self.create_resolved_type_reference_directive(resolved, false)
    }

    /// `resolutionState.getCandidateFromTypeRoot`.
    fn candidate_from_type_root(&mut self, type_root: &str) -> String {
        let name_for_lookup = if type_root.ends_with("/node_modules/@types")
            || type_root.ends_with("/node_modules/@types/")
        {
            self.mangle_scoped_package_name(&self.name.clone())
        } else {
            self.name.clone()
        };
        combine_paths(type_root, &[&name_for_lookup])
    }

    /// `resolutionState.mangleScopedPackageName`, which traces when it changes
    /// the name.
    fn mangle_scoped_package_name(&mut self, name: &str) -> String {
        let mangled = mangle_scoped_package_name(name);
        if mangled != name {
            self.trace(&messages::SCOPED_PACKAGE_DETECTED_LOOKING_IN_0, &[&mangled]);
        }
        mangled
    }

    /// `resolutionState.resolveFromTypeRoot`.
    fn resolve_from_type_root(&mut self) -> Search {
        let Some(type_roots) = self.options().type_roots.clone() else {
            return continue_searching();
        };
        for type_root in &type_roots {
            let candidate = self.candidate_from_type_root(type_root);
            if !self.fs().directory_exists(type_root) {
                self.trace(
                    &messages::DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT,
                    &[type_root],
                );
                continue;
            }
            if let Some(mut resolved) =
                self.load_module_from_file(Extensions::DECLARATION, &candidate)
            {
                let package_directory = parse_node_module_from_path(&resolved.path, false);
                if !package_directory.is_empty() {
                    let info = self.get_package_json_info(&package_directory);
                    resolved.package_id = self.get_package_id(&resolved.path, info.as_ref());
                }
                return Some(resolved);
            }
            if let Some(resolved) =
                self.load_node_module_from_directory(Extensions::DECLARATION, &candidate, true)
            {
                return Some(resolved);
            }
        }
        continue_searching()
    }

    // ---- Node-like module resolution ---------------------------------------

    /// `resolutionState.resolveNodeLike`.
    fn resolve_node_like(&mut self) -> ResolvedModule {
        let conditions = self
            .conditions
            .iter()
            .map(|condition| format!("'{condition}'"))
            .collect::<Vec<_>>()
            .join(", ");
        let mode = if self.esm_mode { "ESM" } else { "CJS" };
        self.trace(&messages::RESOLVING_IN_0_MODE_WITH_CONDITIONS_1, &[mode, &conditions]);

        let mut result = self.resolve_node_like_worker();

        // The `node10Result` pass: a package that resolved to JavaScript under
        // `exports` may have types that only `node10`-style resolution can see,
        // and the checker turns that into "your library needs a configuration
        // update" rather than a plain missing-types error.
        if self.resolved_package_directory
            && !self.is_config_lookup
            && self.features.intersects(NodeResolutionFeatures::EXPORTS)
            && self.extensions.intersects(Extensions::TYPESCRIPT.union(Extensions::DECLARATION))
            && !is_external_module_name_relative(&self.name)
            && result.is_resolved()
            && result.is_external_library_import
            && !extension_is_ok(
                Extensions::TYPESCRIPT.union(Extensions::DECLARATION),
                &result.extension,
            )
            && self.conditions.iter().any(|condition| condition == "import")
        {
            self.trace(&messages::RESOLUTION_OF_NON_RELATIVE_NAME_FAILED_TRYING_WITH_MODERN_NODE_RESOLUTION_FEATURES_DISABLED_TO_SEE_IF_NPM_LIBRARY_NEEDS_CONFIGURATION_UPDATE, &[]);
            self.features = self.features.difference(NodeResolutionFeatures::EXPORTS);
            self.extensions =
                self.extensions.intersection(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));
            let diagnostics_count = self.diagnostics.len();
            let diagnostic_result = self.resolve_node_like_worker();
            if diagnostic_result.is_resolved() && diagnostic_result.is_external_library_import {
                result.alternate_result = diagnostic_result.resolved_file_name;
            }
            // The second pass's diagnostics are discarded: it is a probe, not a
            // resolution, and its complaints are about a configuration the user
            // did not ask for.
            self.diagnostics.truncate(diagnostics_count);
        }
        result
    }

    /// `resolutionState.resolveNodeLikeWorker`.
    fn resolve_node_like_worker(&mut self) -> ResolvedModule {
        if let Some(resolved) = self.try_load_module_using_optional_resolution_settings() {
            return self.create_resolved_module_handling_symlink(Some(resolved));
        }

        if is_external_module_name_relative(&self.name) {
            let containing_directory = self.containing_directory.clone();
            let candidate =
                normalize_path_for_cjs_resolution(&containing_directory, &self.name.clone());
            let extensions = self.extensions;
            let resolved = self.node_load_module_by_relative_name(extensions, &candidate, true);
            let is_external =
                resolved.as_ref().is_some_and(|resolved| resolved.path.contains("/node_modules/"));
            return self.create_resolved_module(resolved, is_external);
        }

        if self.features.intersects(NodeResolutionFeatures::IMPORTS)
            && self.name.starts_with('#')
            && let Some(resolved) = self.load_module_from_imports()
        {
            return self.create_resolved_module_handling_symlink(Some(resolved));
        }
        if self.features.intersects(NodeResolutionFeatures::SELF_NAME)
            && let Some(resolved) = self.load_module_from_self_name_reference()
        {
            return self.create_resolved_module_handling_symlink(Some(resolved));
        }
        if self.name.contains(':') {
            let (name, extensions) = (self.name.clone(), self.extensions.to_string());
            self.trace(
                &messages::SKIPPING_MODULE_0_THAT_LOOKS_LIKE_AN_ABSOLUTE_URI_TARGET_FILE_TYPES_COLON_1,
                &[&name, &extensions],
            );
            return self.create_resolved_module(None, false);
        }
        let (name, extensions) = (self.name.clone(), self.extensions.to_string());
        self.trace(
            &messages::LOADING_MODULE_0_FROM_NODE_MODULES_FOLDER_TARGET_FILE_TYPES_COLON_1,
            &[&name, &extensions],
        );
        if let Some(resolved) = self.load_module_from_nearest_node_modules_directory(false) {
            return self.create_resolved_module_handling_symlink(Some(resolved));
        }
        if self.extensions.intersects(Extensions::DECLARATION)
            && let Some(resolved) = self.resolve_from_type_root()
        {
            return self.create_resolved_module_handling_symlink(Some(resolved));
        }
        self.create_resolved_module(None, false)
    }

    /// `resolutionState.loadModuleFromSelfNameReference`.
    fn load_module_from_self_name_reference(&mut self) -> Search {
        let directory_path =
            get_normalized_absolute_path(&self.containing_directory, self.current_directory());
        let scope = self.get_package_scope_for_path(&directory_path);
        let Some(scope) = scope.filter(|scope| scope.exists()) else {
            return continue_searching();
        };
        let contents = scope.contents.clone().expect("checked by exists");
        if contents.exports.is_falsy() {
            return continue_searching();
        }
        let Some(name) = contents.name.value.clone() else { return continue_searching() };

        let parts = get_path_components(&self.name, "");
        let name_parts = get_path_components(&name, "");
        if parts.len() < name_parts.len() || parts[..name_parts.len()] != name_parts[..] {
            return continue_searching();
        }
        let trailing = &parts[name_parts.len()..];
        let subpath = if trailing.is_empty() {
            ".".to_string()
        } else {
            let refs: Vec<&str> = trailing.iter().map(String::as_str).collect();
            combine_paths(".", &refs)
        };

        // Two passes, prioritising TypeScript and declarations, to match
        // `loadModuleFromNearestNodeModulesDirectoryWorker` — except when
        // `allowJs` is on outside `node_modules`, where a single pass lets a
        // package's own input `.js` files win over its emitted declarations.
        if self.options().get_allow_js() && !self.containing_directory.contains("/node_modules/") {
            let extensions = self.extensions;
            return self.load_module_from_exports(&scope, extensions, &subpath);
        }
        let priority =
            self.extensions.intersection(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));
        let secondary =
            self.extensions.difference(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));
        if let Some(resolved) = self.load_module_from_exports(&scope, priority, &subpath) {
            return Some(resolved);
        }
        self.load_module_from_exports(&scope, secondary, &subpath)
    }

    /// `resolutionState.loadModuleFromImports`.
    fn load_module_from_imports(&mut self) -> Search {
        if self.name == "#"
            || (self.name.starts_with("#/")
                && !self.features.intersects(NodeResolutionFeatures::IMPORTS_PATTERN_ROOT))
        {
            let name = self.name.clone();
            self.trace(&messages::INVALID_IMPORT_SPECIFIER_0_HAS_NO_POSSIBLE_RESOLUTIONS, &[&name]);
            return continue_searching();
        }
        let directory_path =
            get_normalized_absolute_path(&self.containing_directory, self.current_directory());
        let scope = self.get_package_scope_for_path(&directory_path);
        let Some(scope) = scope.filter(|scope| scope.exists()) else {
            self.trace(
                &messages::DIRECTORY_0_HAS_NO_CONTAINING_PACKAGE_JSON_SCOPE_IMPORTS_WILL_NOT_RESOLVE,
                &[&directory_path],
            );
            return continue_searching();
        };
        let contents = scope.contents.clone().expect("checked by exists");
        let Some(table) = contents.imports.as_object().map(<[(String, Json)]>::to_vec) else {
            let directory = scope.package_directory.clone();
            self.trace(&messages::X_PACKAGE_JSON_SCOPE_0_HAS_NO_IMPORTS_DEFINED, &[&directory]);
            return continue_searching();
        };

        let (name, extensions) = (self.name.clone(), self.extensions);
        if let Some(result) =
            self.load_module_from_exports_or_imports(extensions, &name, &table, &scope, true)
        {
            return Some(result);
        }
        let directory = scope.package_directory.clone();
        self.trace(
            &messages::IMPORT_SPECIFIER_0_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH_1,
            &[&name, &directory],
        );
        continue_searching()
    }

    /// `resolutionState.loadModuleFromExports`.
    fn load_module_from_exports(
        &mut self,
        package_info: &Rc<PackageJsonInfo>,
        extensions: Extensions,
        subpath: &str,
    ) -> Search {
        let Some(contents) = package_info.contents.clone() else { return continue_searching() };
        if contents.exports.is_falsy() {
            return continue_searching();
        }

        if subpath == "." {
            let main_export = match &contents.exports.value {
                Some(Json::String(_) | Json::Array(_)) => contents.exports.value.clone(),
                Some(Json::Object(entries)) => {
                    if contents.exports.is_conditions() {
                        contents.exports.value.clone()
                    } else {
                        entries.iter().rev().find(|(key, _)| key == ".").map(|(_, v)| v.clone())
                    }
                }
                _ => None,
            };
            if let Some(main_export) = main_export {
                return self.load_module_from_target_export_or_import(
                    extensions,
                    subpath,
                    package_info,
                    false,
                    &main_export,
                    "",
                    false,
                    ".",
                );
            }
        } else if contents.exports.is_subpaths()
            && let Some(table) = contents.exports.as_object().map(<[(String, Json)]>::to_vec)
            && let Some(result) = self.load_module_from_exports_or_imports(
                extensions,
                subpath,
                &table,
                package_info,
                false,
            )
        {
            return Some(result);
        }

        let directory = package_info.package_directory.clone();
        self.trace(
            &messages::EXPORT_SPECIFIER_0_DOES_NOT_EXIST_IN_PACKAGE_JSON_SCOPE_AT_PATH_1,
            &[subpath, &directory],
        );
        continue_searching()
    }

    /// `resolutionState.loadModuleFromExportsOrImports`.
    fn load_module_from_exports_or_imports(
        &mut self,
        extensions: Extensions,
        module_name: &str,
        lookup_table: &[(String, Json)],
        scope: &Rc<PackageJsonInfo>,
        is_imports: bool,
    ) -> Search {
        if !module_name.ends_with('/')
            && !module_name.contains('*')
            && let Some((_, target)) = lookup_table.iter().rev().find(|(key, _)| key == module_name)
        {
            let target = target.clone();
            return self.load_module_from_target_export_or_import(
                extensions,
                module_name,
                scope,
                is_imports,
                &target,
                "",
                false,
                module_name,
            );
        }

        let mut expanding_keys: Vec<&String> = lookup_table
            .iter()
            .map(|(key, _)| key)
            .filter(|key| key.matches('*').count() == 1 || key.ends_with('/'))
            .collect();
        expanding_keys.sort_by(|a, b| compare_pattern_keys(a, b));

        for potential_target in expanding_keys {
            let target = lookup_table
                .iter()
                .rev()
                .find(|(key, _)| key == potential_target)
                .map_or(Json::Null, |(_, value)| value.clone());

            // The three shapes, in upstream's order. Each *returns* rather than
            // continuing: the first key that matches is the answer, even if it
            // resolves to nothing.
            if self.features.intersects(NodeResolutionFeatures::EXPORTS_PATTERN_TRAILERS)
                && matches_pattern_with_trailer(potential_target, module_name)
            {
                let star =
                    potential_target.find('*').expect("checked by matches_pattern_with_trailer");
                let subpath = module_name
                    [star..module_name.len() - (potential_target.len() - 1 - star)]
                    .to_string();
                return self.load_module_from_target_export_or_import(
                    extensions,
                    module_name,
                    scope,
                    is_imports,
                    &target,
                    &subpath,
                    true,
                    &potential_target.clone(),
                );
            } else if potential_target.ends_with('*')
                && module_name.starts_with(&potential_target[..potential_target.len() - 1])
            {
                let subpath = module_name[potential_target.len() - 1..].to_string();
                return self.load_module_from_target_export_or_import(
                    extensions,
                    module_name,
                    scope,
                    is_imports,
                    &target,
                    &subpath,
                    true,
                    &potential_target.clone(),
                );
            } else if module_name.starts_with(potential_target.as_str()) {
                let subpath = module_name[potential_target.len()..].to_string();
                return self.load_module_from_target_export_or_import(
                    extensions,
                    module_name,
                    scope,
                    is_imports,
                    &target,
                    &subpath,
                    false,
                    &potential_target.clone(),
                );
            }
        }
        continue_searching()
    }

    /// `resolutionState.loadModuleFromTargetExportOrImport`.
    #[allow(clippy::too_many_arguments)]
    fn load_module_from_target_export_or_import(
        &mut self,
        extensions: Extensions,
        module_name: &str,
        scope: &Rc<PackageJsonInfo>,
        is_imports: bool,
        target: &Json,
        subpath: &str,
        is_pattern: bool,
        key: &str,
    ) -> Search {
        let field = if is_imports { "imports" } else { "exports" };
        match target {
            Json::String(target_string) => self.load_module_from_string_target(
                extensions,
                module_name,
                scope,
                is_imports,
                target_string,
                subpath,
                is_pattern,
                key,
            ),
            Json::Object(entries) => {
                self.trace(&messages::ENTERING_CONDITIONAL_EXPORTS, &[]);
                for (condition, sub_target) in entries {
                    if !self.condition_matches(condition) {
                        self.trace(&messages::SAW_NON_MATCHING_CONDITION_0, &[condition]);
                        continue;
                    }
                    self.trace(&messages::MATCHED_0_CONDITION_1, &[field, condition]);
                    let sub_target = sub_target.clone();
                    let condition = condition.clone();
                    if let Some(result) = self.load_module_from_target_export_or_import(
                        extensions,
                        module_name,
                        scope,
                        is_imports,
                        &sub_target,
                        subpath,
                        is_pattern,
                        key,
                    ) {
                        if result.is_resolved() {
                            self.trace(&messages::RESOLVED_UNDER_CONDITION_0, &[&condition]);
                        }
                        self.trace(&messages::EXITING_CONDITIONAL_EXPORTS, &[]);
                        return Some(result);
                    }
                    self.trace(&messages::FAILED_TO_RESOLVE_UNDER_CONDITION_0, &[&condition]);
                }
                self.trace(&messages::EXITING_CONDITIONAL_EXPORTS, &[]);
                continue_searching()
            }
            Json::Array(elements) => {
                if elements.is_empty() {
                    return self.trace_invalid_target(scope, module_name);
                }
                for element in elements.clone() {
                    if let Some(result) = self.load_module_from_target_export_or_import(
                        extensions,
                        module_name,
                        scope,
                        is_imports,
                        &element,
                        subpath,
                        is_pattern,
                        key,
                    ) {
                        return Some(result);
                    }
                }
                // Falls through to "invalid type", which is upstream's control
                // flow: an array whose every element failed is reported the same
                // way as a target of the wrong shape.
                self.trace_invalid_target(scope, module_name)
            }
            Json::Null => {
                let directory = scope.package_directory.clone();
                self.trace(
                    &messages::X_PACKAGE_JSON_SCOPE_0_EXPLICITLY_MAPS_SPECIFIER_1_TO_NULL,
                    &[&directory, module_name],
                );
                unresolved()
            }
            _ => self.trace_invalid_target(scope, module_name),
        }
    }

    fn trace_invalid_target(&mut self, scope: &Rc<PackageJsonInfo>, module_name: &str) -> Search {
        let directory = scope.package_directory.clone();
        self.trace(
            &messages::X_PACKAGE_JSON_SCOPE_0_HAS_INVALID_TYPE_FOR_TARGET_OF_SPECIFIER_1,
            &[&directory, module_name],
        );
        continue_searching()
    }

    /// The `Json::String` arm of `loadModuleFromTargetExportOrImport`.
    #[allow(clippy::too_many_arguments)]
    fn load_module_from_string_target(
        &mut self,
        extensions: Extensions,
        module_name: &str,
        scope: &Rc<PackageJsonInfo>,
        is_imports: bool,
        target_string: &str,
        subpath: &str,
        is_pattern: bool,
        key: &str,
    ) -> Search {
        if !is_pattern && !subpath.is_empty() && !target_string.ends_with('/') {
            return self.trace_invalid_target(scope, module_name);
        }

        if !target_string.starts_with("./") {
            // An `imports` target may name another package, which is resolved
            // from the package root as if it had been written there.
            if is_imports
                && !target_string.starts_with("../")
                && !target_string.starts_with('/')
                && !is_rooted_disk_path(target_string)
            {
                let combined_lookup = if is_pattern {
                    target_string.replace('*', subpath)
                } else {
                    format!("{target_string}{subpath}")
                };
                let scope_directory = ensure_trailing_directory_separator(&scope.package_directory);
                self.trace(
                    &messages::USING_0_SUBPATH_1_WITH_TARGET_2,
                    &["imports", key, &combined_lookup],
                );
                self.trace(
                    &messages::RESOLVING_MODULE_0_FROM_1,
                    &[&combined_lookup, &scope_directory],
                );

                let saved = (self.name.clone(), self.containing_directory.clone());
                self.name = combined_lookup;
                self.containing_directory = scope_directory;
                let result = self.resolve_node_like();
                self.name = saved.0;
                self.containing_directory = saved.1;

                return if result.is_resolved() {
                    Some(Resolved {
                        path: result.resolved_file_name,
                        extension: result.extension,
                        package_id: result.package_id,
                        original_path: result.original_path,
                        resolved_using_ts_extension: result.resolved_using_ts_extension,
                    })
                } else {
                    continue_searching()
                };
            }
            return self.trace_invalid_target(scope, module_name);
        }

        let parts = if path_is_relative(target_string) {
            get_path_components(target_string, "")[1..].to_vec()
        } else {
            get_path_components(target_string, "")
        };
        // A target may not escape its package, nor route through `node_modules`.
        if parts[1..].iter().any(|part| part == ".." || part == "." || part == "node_modules") {
            return self.trace_invalid_target(scope, module_name);
        }
        let resolved_target = combine_paths(&scope.package_directory, &[target_string]);
        let subpath_parts = get_path_components(subpath, "");
        if subpath_parts.iter().any(|part| part == ".." || part == "." || part == "node_modules") {
            return self.trace_invalid_target(scope, module_name);
        }

        let message_target = if is_pattern {
            target_string.replace('*', subpath)
        } else {
            format!("{target_string}{subpath}")
        };
        let field = if is_imports { "imports" } else { "exports" };
        self.trace(&messages::USING_0_SUBPATH_1_WITH_TARGET_2, &[field, key, &message_target]);

        let final_path = if is_pattern {
            get_normalized_absolute_path(
                &resolved_target.replace('*', subpath),
                self.current_directory(),
            )
        } else {
            get_normalized_absolute_path(
                &format!("{resolved_target}{subpath}"),
                self.current_directory(),
            )
        };

        let package_path = combine_paths(&scope.package_directory, &["package.json"]);
        if let Some(mut input_link) =
            self.try_load_input_file_for_path(&final_path, subpath, &package_path, is_imports)
        {
            input_link.package_id = self.get_package_id(&input_link.path, Some(scope));
            return Some(input_link);
        }
        if let Some(mut result) =
            self.load_file_name_from_package_json_field(extensions, &final_path, target_string)
        {
            result.package_id = self.get_package_id(&result.path, Some(scope));
            return Some(result);
        }
        continue_searching()
    }

    /// `resolutionState.tryLoadInputFileForPath`.
    ///
    /// Maps an `exports` target that points at *output* (`./dist/index.js`) back
    /// to the input the program actually holds, so a package that self-imports
    /// through its own export map still typechecks against its sources.
    fn try_load_input_file_for_path(
        &mut self,
        final_path: &str,
        entry: &str,
        package_path: &str,
        is_imports: bool,
    ) -> Search {
        let options = self.options().clone();
        if self.is_config_lookup
            || (options.declaration_dir.is_empty() && options.out_dir.is_empty())
            || final_path.contains("/node_modules/")
        {
            return continue_searching();
        }
        if !options.config_file_path.is_empty()
            && !contains_path(
                get_directory_path(package_path),
                &options.config_file_path,
                &self.compare_paths_options(),
            )
        {
            return continue_searching();
        }

        // Unlike TypeScript's own implementation, this makes no guesses: without
        // an explicit root there is nothing to map back to, and saying so is a
        // diagnostic rather than a silent miss.
        let root_dir = if options.root_dir.is_empty() {
            if options.config_file_path.is_empty() {
                let message = if is_imports {
                    &messages::THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_IMPORT_MAP_ENTRY_0_IN_FILE_1
                } else {
                    &messages::THE_PROJECT_ROOT_IS_AMBIGUOUS_BUT_IS_REQUIRED_TO_RESOLVE_EXPORT_MAP_ENTRY_0_IN_FILE_1
                };
                let entry = if entry.is_empty() { "." } else { entry };
                self.diagnostics.push(Trace {
                    code: message.code,
                    text: message.format(&[entry, package_path]),
                });
                return unresolved();
            }
            get_directory_path(&options.config_file_path).to_string()
        } else {
            options.root_dir.clone()
        };

        for candidate_dir in self.output_directories_for_base_directory(&root_dir) {
            if !contains_path(&candidate_dir, final_path, &self.compare_paths_options()) {
                continue;
            }
            let fragment = if final_path.len() > candidate_dir.len() {
                &final_path[candidate_dir.len() + 1..]
            } else {
                ""
            };
            let possible_input_base = combine_paths(&root_dir, &[fragment]);
            for extension in JS_AND_DTS {
                if !file_extension_is(&possible_input_base, extension) {
                    continue;
                }
                for possible_extension in
                    get_possible_original_input_extension_for_extension(&possible_input_base)
                {
                    if !extension_is_ok(self.extensions, &possible_extension) {
                        continue;
                    }
                    let candidate = change_extension(&possible_input_base, &possible_extension);
                    if self.fs().file_exists(&candidate) {
                        let extensions = self.extensions;
                        if let Some(resolved) =
                            self.load_file_name_from_package_json_field(extensions, &candidate, "")
                        {
                            return Some(resolved);
                        }
                    }
                }
            }
        }
        continue_searching()
    }

    /// `resolutionState.getOutputDirectoriesForBaseDirectory`.
    fn output_directories_for_base_directory(&self, common_source_dir_guess: &str) -> Vec<String> {
        let options = self.options();
        let current_dir = if options.config_file_path.is_empty() {
            common_source_dir_guess
        } else {
            self.current_directory()
        };
        let mut candidates = Vec::new();
        if !options.declaration_dir.is_empty() {
            candidates.push(get_normalized_absolute_path(
                &combine_paths(current_dir, &[&options.declaration_dir]),
                self.current_directory(),
            ));
        }
        if !options.out_dir.is_empty() && options.out_dir != options.declaration_dir {
            candidates.push(get_normalized_absolute_path(
                &combine_paths(current_dir, &[&options.out_dir]),
                self.current_directory(),
            ));
        }
        candidates
    }

    // ---- node_modules ------------------------------------------------------

    /// `resolutionState.loadModuleFromNearestNodeModulesDirectory`.
    ///
    /// Two passes up the directory tree, not one: pass one looks for TypeScript
    /// and declarations (including `@types`), pass two for JavaScript and JSON.
    /// The split is what lets an `@types` package high in the tree beat an
    /// untyped implementation lower down.
    fn load_module_from_nearest_node_modules_directory(
        &mut self,
        types_scope_only: bool,
    ) -> Search {
        let priority =
            self.extensions.intersection(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));
        let secondary =
            self.extensions.difference(Extensions::TYPESCRIPT.union(Extensions::DECLARATION));

        if !priority.is_empty() {
            self.trace(
                &messages::SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_PREFERRED_EXTENSIONS_COLON_0,
                &[&priority.to_string()],
            );
            if let Some(result) = self
                .load_module_from_nearest_node_modules_directory_worker(priority, types_scope_only)
            {
                return Some(result);
            }
        }
        if !secondary.is_empty() && !types_scope_only {
            self.trace(
                &messages::SEARCHING_ALL_ANCESTOR_NODE_MODULES_DIRECTORIES_FOR_FALLBACK_EXTENSIONS_COLON_0,
                &[&secondary.to_string()],
            );
            return self.load_module_from_nearest_node_modules_directory_worker(
                secondary,
                types_scope_only,
            );
        }
        continue_searching()
    }

    /// `resolutionState.loadModuleFromNearestNodeModulesDirectoryWorker`.
    fn load_module_from_nearest_node_modules_directory_worker(
        &mut self,
        extensions: Extensions,
        types_scope_only: bool,
    ) -> Search {
        // Walked by hand rather than through `for_each_ancestor_directory`
        // because the body needs `&mut self`.
        let mut directory = self.containing_directory.clone();
        loop {
            // A `node_modules` directory is skipped as a *search origin*: its
            // own `node_modules/node_modules` is not a thing.
            if get_base_file_name(&directory) != "node_modules"
                && let Some(result) = self.load_module_from_immediate_node_modules_directory(
                    extensions,
                    &directory,
                    types_scope_only,
                )
            {
                return Some(result);
            }
            let parent = get_directory_path(&directory).to_string();
            if parent == directory {
                return continue_searching();
            }
            directory = parent;
        }
    }

    /// `resolutionState.loadModuleFromImmediateNodeModulesDirectory`.
    fn load_module_from_immediate_node_modules_directory(
        &mut self,
        extensions: Extensions,
        directory: &str,
        types_scope_only: bool,
    ) -> Search {
        let node_modules_folder = combine_paths(directory, &["node_modules"]);
        if !self.fs().directory_exists(&node_modules_folder) {
            self.trace(
                &messages::DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT,
                &[&node_modules_folder],
            );
            return continue_searching();
        }

        if !types_scope_only {
            let name = self.name.clone();
            if let Some(result) = self.load_module_from_specific_node_modules_directory(
                extensions,
                &name,
                &node_modules_folder,
            ) {
                return Some(result);
            }
        }

        if extensions.intersects(Extensions::DECLARATION) {
            let at_types = combine_paths(&node_modules_folder, &["@types"]);
            if !self.fs().directory_exists(&at_types) {
                self.trace(
                    &messages::DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT,
                    &[&at_types],
                );
                return continue_searching();
            }
            let mangled = self.mangle_scoped_package_name(&self.name.clone());
            return self.load_module_from_specific_node_modules_directory(
                Extensions::DECLARATION,
                &mangled,
                &at_types,
            );
        }
        continue_searching()
    }

    /// `resolutionState.loadModuleFromSpecificNodeModulesDirectory`.
    fn load_module_from_specific_node_modules_directory(
        &mut self,
        extensions: Extensions,
        module_name: &str,
        node_modules_directory: &str,
    ) -> Search {
        // The trailing separator is stripped so `pkg/` and `pkg` produce the same
        // candidate: the `package.json` cache stores the *caller's* directory
        // spelling, and a mismatch makes the `main`/`types` lookup silently skip.
        // See microsoft/typescript-go#3526.
        let candidate = remove_trailing_directory_separator(&normalize_path(&combine_paths(
            node_modules_directory,
            &[module_name],
        )));
        let (package_name, rest) = parse_package_name(module_name);
        let (package_name, rest) = (package_name.to_string(), rest.to_string());
        let package_directory = if package_name.is_empty() {
            candidate.clone()
        } else {
            combine_paths(node_modules_directory, &[&package_name])
        };

        if self.resolve_package_directory_only {
            if self.fs().directory_exists(&package_directory) {
                return Some(Resolved { path: package_directory, ..Resolved::default() });
            }
            return continue_searching();
        }

        let mut root_package_info = None;
        // A nested `package.json` — `node_modules/foo/bar/package.json` — is
        // consulted first, but only when export maps are not in play, since an
        // export map could redirect around this location entirely.
        let mut package_info = self.get_package_json_info(&candidate);
        if !rest.is_empty() && info_exists(package_info.as_ref()) {
            if self.features.intersects(NodeResolutionFeatures::EXPORTS) {
                root_package_info = self.get_package_json_info(&package_directory);
            }
            let root_has_exports = root_package_info
                .as_ref()
                .and_then(|info| info.contents.as_ref())
                .is_some_and(|contents| contents.exports.is_present());
            if !root_has_exports {
                if let Some(from_file) = self.load_module_from_file(extensions, &candidate) {
                    return Some(from_file);
                }
                if let Some(mut from_directory) = self.load_node_module_from_directory_worker(
                    extensions,
                    &candidate,
                    package_info.as_ref(),
                ) {
                    from_directory.package_id =
                        self.get_package_id(&from_directory.path, package_info.as_ref());
                    return Some(from_directory);
                }
            }
        }

        if !rest.is_empty() {
            package_info = root_package_info
                .clone()
                .or_else(|| self.get_package_json_info(&package_directory));
        }

        let loader =
            Loader::SpecificNodeModules { rest: rest.clone(), package_info: package_info.clone() };

        // `package_info` is `Some` whenever the directory was *looked at*, even
        // if it held no `package.json` — which is what marks the package as
        // resolved for the `node10Result` fallback.
        if let Some(info) = package_info.clone() {
            self.resolved_package_directory = true;
            let has_usable_exports =
                info.contents.as_ref().is_some_and(|contents| !contents.exports.is_falsy());
            if self.features.intersects(NodeResolutionFeatures::EXPORTS) && has_usable_exports {
                // Export maps outrank file, directory, and `typesVersions`
                // lookups — and block them. A top-level `"exports": null` is the
                // documented exception, and `is_falsy` is what implements it.
                return self.load_module_from_exports(
                    &info,
                    extensions,
                    &combine_paths(".", &[&rest]),
                );
            }
            if !rest.is_empty()
                && let Some(contents) = info.contents.clone()
            {
                let version_paths = self.replay_version_paths(&contents);
                if version_paths.exists() {
                    self.trace(
                        &messages::X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_0_THAT_MATCHES_COMPILER_VERSION_1_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME_2,
                        &[&version_paths.version, TYPESCRIPT_VERSION, &rest],
                    );
                    let patterns = try_parse_patterns(&version_paths.paths);
                    if let Some(from_paths) = self.try_load_module_using_paths(
                        extensions,
                        &rest,
                        &package_directory,
                        &version_paths.paths,
                        &patterns,
                        &loader,
                    ) {
                        return Some(from_paths);
                    }
                }
            }
        }

        self.run_loader(&loader, extensions, &candidate)
    }

    /// Dispatch to whichever nested lookup this call site wanted.
    fn run_loader(&mut self, loader: &Loader, extensions: Extensions, candidate: &str) -> Search {
        match loader {
            Loader::NodeRelative => {
                self.node_load_module_by_relative_name(extensions, candidate, true)
            }
            Loader::SpecificNodeModules { rest, package_info } => {
                // `esmMode` suppresses the bare-file lookup for a package's own
                // entry point, but not for a subpath within it.
                if !rest.is_empty() || !self.esm_mode {
                    if let Some(mut from_file) = self.load_module_from_file(extensions, candidate) {
                        from_file.package_id =
                            self.get_package_id(&from_file.path, package_info.as_ref());
                        return Some(from_file);
                    }
                }
                if let Some(mut from_directory) = self.load_node_module_from_directory_worker(
                    extensions,
                    candidate,
                    package_info.as_ref(),
                ) {
                    from_directory.package_id =
                        self.get_package_id(&from_directory.path, package_info.as_ref());
                    return Some(from_directory);
                }
                // ESM disables directory lookups generally, but a bare package
                // name still falls back to `index.js` when there is no `main`
                // and no `exports`.
                let exports_absent = package_info
                    .as_ref()
                    .and_then(|info| info.contents.as_ref())
                    .is_some_and(|contents| {
                        !contents.exports.is_present()
                            || matches!(contents.exports.value, Some(Json::Null))
                    });
                if rest.is_empty() && exports_absent && self.esm_mode {
                    let index = combine_paths(candidate, &["index.js"]);
                    if let Some(mut result) = self.load_module_from_file(extensions, &index) {
                        result.package_id =
                            self.get_package_id(&result.path, package_info.as_ref());
                        return Some(result);
                    }
                }
                continue_searching()
            }
            Loader::NodeModuleDirectory { package_file, package_info } => {
                if let Some(from_file) =
                    self.load_file_name_from_package_json_field(extensions, candidate, package_file)
                {
                    return Some(from_file);
                }
                // Even a declarations-only lookup may reach a `.ts` file through
                // `package.json` `types`.
                let expanded = if extensions == Extensions::DECLARATION {
                    Extensions::TYPESCRIPT.union(Extensions::DECLARATION)
                } else {
                    extensions
                };
                let saved_esm = self.esm_mode;
                let saved_from_config = self.candidate_ending_is_from_config;
                self.candidate_ending_is_from_config = true;
                let is_module_package =
                    package_info.as_ref().and_then(|info| info.contents.as_ref()).is_some_and(
                        |contents| contents.package_type.value.as_deref() == Some("module"),
                    );
                if info_exists(package_info.as_ref()) && !is_module_package {
                    // A CommonJS package's `main` may omit its extension.
                    self.esm_mode = false;
                }
                let result = self.node_load_module_by_relative_name(expanded, candidate, false);
                self.esm_mode = saved_esm;
                self.candidate_ending_is_from_config = saved_from_config;
                result
            }
        }
    }

    // ---- Optional resolution settings --------------------------------------

    /// `resolutionState.tryLoadModuleUsingOptionalResolutionSettings`.
    fn try_load_module_using_optional_resolution_settings(&mut self) -> Search {
        if let Some(resolved) = self.try_load_module_using_paths_if_eligible() {
            return Some(resolved);
        }
        if is_external_module_name_relative(&self.name) {
            self.try_load_module_using_root_dirs()
        } else {
            // There is no `baseUrl` fallback: upstream removed it.
            continue_searching()
        }
    }

    /// `resolutionState.tryLoadModuleUsingPathsIfEligible`.
    fn try_load_module_using_paths_if_eligible(&mut self) -> Search {
        if self.options().paths.is_empty() || path_is_relative(&self.name) {
            return continue_searching();
        }
        let name = self.name.clone();
        self.trace(
            &messages::X_PATHS_OPTION_IS_SPECIFIED_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME_0,
            &[&name],
        );
        let base_directory = self.options().get_paths_base_path(self.current_directory());
        let paths = self.options().paths.clone();
        let patterns = self.resolver.parsed_patterns_for_paths.clone();
        let extensions = self.extensions;
        self.try_load_module_using_paths(
            extensions,
            &name,
            &base_directory,
            &paths,
            &patterns,
            &Loader::NodeRelative,
        )
    }

    /// `resolutionState.tryLoadModuleUsingPaths`.
    fn try_load_module_using_paths(
        &mut self,
        extensions: Extensions,
        module_name: &str,
        containing_directory: &str,
        paths: &OrderedMap<Vec<String>>,
        path_patterns: &ParsedPatterns,
        loader: &Loader,
    ) -> Search {
        let matched = match_pattern_or_exact(path_patterns, module_name);
        if !matched.is_valid() {
            return continue_searching();
        }
        let matched_star = matched.matched_text(module_name).to_string();
        self.trace(&messages::MODULE_NAME_0_MATCHED_PATTERN_1, &[module_name, &matched.text]);

        let substitutions = paths.get(&matched.text).cloned().unwrap_or_default();
        for subst in substitutions {
            let path = subst.replacen('*', &matched_star, 1);
            let candidate = normalize_path(&combine_paths(containing_directory, &[&path]));
            self.trace(
                &messages::TRYING_SUBSTITUTION_0_CANDIDATE_MODULE_LOCATION_COLON_1,
                &[&subst, &path],
            );

            // A substitution may carry its own extension, in which case the file
            // is tried verbatim first.
            let extension_from_subst = try_get_extension_from_path(&subst);
            if !extension_from_subst.is_empty()
                && let (path, true) = self.try_file(&candidate)
            {
                return Some(Resolved {
                    path,
                    extension: extension_from_subst.to_string(),
                    ..Resolved::default()
                });
            }

            // An extension that came from configuration is not an extension the
            // *specifier* wrote, so it must not set `resolvedUsingTsExtension`.
            let saved = self.candidate_ending_is_from_config;
            if !extension_from_subst.is_empty() {
                self.candidate_ending_is_from_config = true;
            }
            let resolved = self.run_loader(loader, extensions, &candidate);
            self.candidate_ending_is_from_config = saved;
            if let Some(resolved) = resolved {
                return Some(resolved);
            }
        }
        continue_searching()
    }

    /// `resolutionState.tryLoadModuleUsingRootDirs`.
    fn try_load_module_using_root_dirs(&mut self) -> Search {
        let root_dirs = self.options().root_dirs.clone();
        if root_dirs.is_empty() {
            return continue_searching();
        }
        let name = self.name.clone();
        self.trace(
            &messages::X_ROOTDIRS_OPTION_IS_SET_USING_IT_TO_RESOLVE_RELATIVE_MODULE_NAME_0,
            &[&name],
        );
        let candidate =
            normalize_path(&combine_paths(&self.containing_directory.clone(), &[&name]));

        let mut matched_root_dir = String::new();
        let mut matched_prefix = String::new();
        for root_dir in &root_dirs {
            let mut normalized_root = normalize_path(root_dir);
            if !normalized_root.ends_with('/') {
                normalized_root.push('/');
            }
            let is_longest = candidate.starts_with(&normalized_root)
                && (matched_prefix.is_empty() || matched_prefix.len() < normalized_root.len());
            self.trace(
                &messages::CHECKING_IF_0_IS_THE_LONGEST_MATCHING_PREFIX_FOR_1_2,
                &[&normalized_root, &candidate, if is_longest { "true" } else { "false" }],
            );
            if is_longest {
                matched_prefix = normalized_root;
                matched_root_dir.clone_from(root_dir);
            }
        }

        if matched_prefix.is_empty() {
            return continue_searching();
        }
        self.trace(&messages::LONGEST_MATCHING_PREFIX_FOR_0_IS_1, &[&candidate, &matched_prefix]);
        let suffix = candidate[matched_prefix.len()..].to_string();

        self.trace(
            &messages::LOADING_0_FROM_THE_ROOT_DIR_1_CANDIDATE_LOCATION_2,
            &[&suffix, &matched_prefix, &candidate],
        );
        let extensions = self.extensions;
        if let Some(resolved) = self.node_load_module_by_relative_name(extensions, &candidate, true)
        {
            return Some(resolved);
        }

        self.trace(&messages::TRYING_OTHER_ENTRIES_IN_ROOTDIRS, &[]);
        for root_dir in &root_dirs {
            if *root_dir == matched_root_dir {
                continue;
            }
            let candidate = combine_paths(&normalize_path(root_dir), &[&suffix]);
            self.trace(
                &messages::LOADING_0_FROM_THE_ROOT_DIR_1_CANDIDATE_LOCATION_2,
                &[&suffix, root_dir, &candidate],
            );
            if let Some(resolved) =
                self.node_load_module_by_relative_name(extensions, &candidate, true)
            {
                return Some(resolved);
            }
        }
        self.trace(&messages::MODULE_RESOLUTION_USING_ROOTDIRS_HAS_FAILED, &[]);
        continue_searching()
    }

    // ---- File and directory lookups ----------------------------------------

    /// `resolutionState.nodeLoadModuleByRelativeName`.
    fn node_load_module_by_relative_name(
        &mut self,
        extensions: Extensions,
        candidate: &str,
        consider_package_json: bool,
    ) -> Search {
        self.trace(
            &messages::LOADING_MODULE_AS_FILE_SLASH_FOLDER_CANDIDATE_MODULE_LOCATION_0_TARGET_FILE_TYPES_COLON_1,
            &[candidate, &extensions.to_string()],
        );
        if !has_trailing_directory_separator(candidate) {
            let parent = get_directory_path(candidate).to_string();
            if !self.fs().directory_exists(&parent) {
                self.trace(
                    &messages::DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT,
                    &[&parent],
                );
                return continue_searching();
            }
            if let Some(mut resolved) = self.load_module_from_file(extensions, candidate) {
                if consider_package_json {
                    let package_directory = parse_node_module_from_path(&resolved.path, false);
                    if !package_directory.is_empty() {
                        let info = self.get_package_json_info(&package_directory);
                        resolved.package_id = self.get_package_id(&resolved.path, info.as_ref());
                    }
                }
                return Some(resolved);
            }
        }
        if !self.fs().directory_exists(candidate) {
            self.trace(
                &messages::DIRECTORY_0_DOES_NOT_EXIST_SKIPPING_ALL_LOOKUPS_IN_IT,
                &[candidate],
            );
            return continue_searching();
        }
        // ESM relative imports do no directory lookups at all — neither
        // `package.json` redirection nor an implicit `index`.
        if self.esm_mode {
            return continue_searching();
        }
        self.load_node_module_from_directory(extensions, candidate, consider_package_json)
    }

    /// `resolutionState.loadModuleFromFile`.
    fn load_module_from_file(&mut self, extensions: Extensions, candidate: &str) -> Search {
        // `./foo.js` → `./foo.ts`.
        if let Some(resolved) =
            self.load_module_from_file_no_implicit_extensions(extensions, candidate)
        {
            return Some(resolved);
        }
        // `./foo` → `./foo.ts`, which ESM does not do.
        if self.esm_mode {
            return continue_searching();
        }
        self.try_adding_extensions(candidate, extensions, "")
    }

    /// `resolutionState.loadModuleFromFileNoImplicitExtensions`.
    fn load_module_from_file_no_implicit_extensions(
        &mut self,
        extensions: Extensions,
        candidate: &str,
    ) -> Search {
        let base = get_base_file_name(candidate);
        if !base.contains('.') {
            // Extensionless: there is nothing to substitute, and extensionless
            // files are not supported.
            return continue_searching();
        }
        let known = remove_file_extension(candidate);
        let extensionless = if known == candidate {
            // Not a known extension, so fall back to the last dot — this is what
            // makes `./foo.css` reachable as `./foo.d.css.ts`.
            &candidate[..candidate.rfind('.').expect("base contains a dot")]
        } else {
            known
        };
        let original_extension = candidate[extensionless.len()..].to_string();
        let extensionless = extensionless.to_string();
        self.trace(
            &messages::FILE_NAME_0_HAS_A_1_EXTENSION_STRIPPING_IT,
            &[candidate, &original_extension],
        );
        self.try_adding_extensions(&extensionless, extensions, &original_extension)
    }

    /// `resolutionState.tryAddingExtensions`.
    ///
    /// The substitution table. Each arm's order is the priority order and is
    /// baselined step by step.
    fn try_adding_extensions(
        &mut self,
        extensionless: &str,
        extensions: Extensions,
        original_extension: &str,
    ) -> Search {
        let directory = get_directory_path(extensionless).to_string();
        if !directory.is_empty() && !self.fs().directory_exists(&directory) {
            return continue_searching();
        }

        // Whether the specifier itself named a TypeScript extension, which is
        // what `resolvedUsingTsExtension` reports to the checker.
        let ts = |from: &[&str]| from.contains(&original_extension);

        macro_rules! try_extension {
            ($extension:expr, $used_ts:expr) => {
                if let Some(resolved) = self.try_extension($extension, extensionless, $used_ts) {
                    return Some(resolved);
                }
            };
        }

        match original_extension {
            EXTENSION_MJS | EXTENSION_MTS | EXTENSION_DMTS => {
                let used_ts = ts(&[EXTENSION_MTS, EXTENSION_DMTS]);
                if extensions.intersects(Extensions::TYPESCRIPT) {
                    try_extension!(EXTENSION_MTS, used_ts);
                }
                if extensions.intersects(Extensions::DECLARATION) {
                    try_extension!(EXTENSION_DMTS, used_ts);
                }
                if extensions.intersects(Extensions::JAVASCRIPT) {
                    try_extension!(EXTENSION_MJS, false);
                }
                continue_searching()
            }
            EXTENSION_CJS | EXTENSION_CTS | EXTENSION_DCTS => {
                let used_ts = ts(&[EXTENSION_CTS, EXTENSION_DCTS]);
                if extensions.intersects(Extensions::TYPESCRIPT) {
                    try_extension!(EXTENSION_CTS, used_ts);
                }
                if extensions.intersects(Extensions::DECLARATION) {
                    try_extension!(EXTENSION_DCTS, used_ts);
                }
                if extensions.intersects(Extensions::JAVASCRIPT) {
                    try_extension!(EXTENSION_CJS, false);
                }
                continue_searching()
            }
            EXTENSION_JSON => {
                if extensions.intersects(Extensions::DECLARATION) {
                    try_extension!(".d.json.ts", false);
                }
                if extensions.intersects(Extensions::JSON) {
                    try_extension!(EXTENSION_JSON, false);
                }
                continue_searching()
            }
            EXTENSION_TSX | EXTENSION_JSX => {
                // Nearly the same as the `.ts`/`.js` arm below, but prefers an
                // exact `.tsx`/`.jsx` match first: a compilation may not contain
                // both `a.ts` and `a.tsx`, so their outputs would clash.
                let used_ts = ts(&[EXTENSION_TSX]);
                if extensions.intersects(Extensions::TYPESCRIPT) {
                    try_extension!(EXTENSION_TSX, used_ts);
                    try_extension!(EXTENSION_TS, used_ts);
                }
                if extensions.intersects(Extensions::DECLARATION) {
                    try_extension!(EXTENSION_DTS, used_ts);
                }
                if extensions.intersects(Extensions::JAVASCRIPT) {
                    try_extension!(EXTENSION_JSX, false);
                    try_extension!(EXTENSION_JS, false);
                }
                continue_searching()
            }
            EXTENSION_TS | EXTENSION_DTS | EXTENSION_JS | "" => {
                let used_ts = ts(&[EXTENSION_TS, EXTENSION_DTS]);
                if extensions.intersects(Extensions::TYPESCRIPT) {
                    try_extension!(EXTENSION_TS, used_ts);
                    try_extension!(EXTENSION_TSX, used_ts);
                }
                if extensions.intersects(Extensions::DECLARATION) {
                    try_extension!(EXTENSION_DTS, used_ts);
                }
                if extensions.intersects(Extensions::JAVASCRIPT) {
                    try_extension!(EXTENSION_JS, false);
                    try_extension!(EXTENSION_JSX, false);
                }
                if self.is_config_lookup {
                    try_extension!(EXTENSION_JSON, false);
                }
                continue_searching()
            }
            other => {
                // An arbitrary extension: only its declaration form is reachable.
                if extensions.intersects(Extensions::DECLARATION)
                    && !is_declaration_file_name(&format!("{extensionless}{other}"))
                {
                    let declaration = format!(".d{other}.ts");
                    if let Some(resolved) = self.try_extension(&declaration, extensionless, false) {
                        return Some(resolved);
                    }
                }
                continue_searching()
            }
        }
    }

    /// `resolutionState.tryExtension`.
    fn try_extension(
        &mut self,
        extension: &str,
        extensionless: &str,
        resolved_using_ts_extension: bool,
    ) -> Search {
        let file_name = format!("{extensionless}{extension}");
        match self.try_file(&file_name) {
            (path, true) => Some(Resolved {
                path,
                extension: extension.to_string(),
                resolved_using_ts_extension: !self.candidate_ending_is_from_config
                    && resolved_using_ts_extension,
                ..Resolved::default()
            }),
            _ => continue_searching(),
        }
    }

    /// `resolutionState.tryFile`, applying `moduleSuffixes`.
    fn try_file(&mut self, file_name: &str) -> (String, bool) {
        let suffixes = self.options().module_suffixes.clone();
        if suffixes.is_empty() {
            let exists = self.try_file_lookup(file_name);
            return (file_name.to_string(), exists);
        }
        let extension = try_get_extension_from_path(file_name);
        let stem = remove_extension(file_name, extension).to_string();
        for suffix in suffixes {
            let path = format!("{stem}{suffix}{extension}");
            if self.try_file_lookup(&path) {
                return (path, true);
            }
        }
        (file_name.to_string(), false)
    }

    /// `resolutionState.tryFileLookup` — the probe every baseline is made of.
    fn try_file_lookup(&mut self, file_name: &str) -> bool {
        if self.fs().file_exists(file_name) {
            self.trace(&messages::FILE_0_EXISTS_USE_IT_AS_A_NAME_RESOLUTION_RESULT, &[file_name]);
            true
        } else {
            self.trace(&messages::FILE_0_DOES_NOT_EXIST, &[file_name]);
            false
        }
    }

    /// `resolutionState.loadNodeModuleFromDirectory`.
    fn load_node_module_from_directory(
        &mut self,
        extensions: Extensions,
        candidate: &str,
        consider_package_json: bool,
    ) -> Search {
        let package_info =
            if consider_package_json { self.get_package_json_info(candidate) } else { None };
        self.load_node_module_from_directory_worker(extensions, candidate, package_info.as_ref())
    }

    /// `resolutionState.loadNodeModuleFromDirectoryWorker`.
    fn load_node_module_from_directory_worker(
        &mut self,
        extensions: Extensions,
        candidate: &str,
        package_info: Option<&Rc<PackageJsonInfo>>,
    ) -> Search {
        let mut package_file = String::new();
        let mut version_paths = VersionPaths::default();

        if let Some(info) = package_info
            && let Some(contents) = info.contents.clone()
        {
            version_paths = self.replay_version_paths(&contents);
            let same_directory = tsr_path::compare_paths(
                candidate,
                &info.package_directory,
                &ComparePathsOptions {
                    use_case_sensitive_file_names: self.fs().use_case_sensitive_file_names(),
                    current_directory: String::new(),
                },
            )
            .is_eq();
            if same_directory && let Some(file) = self.get_package_file(extensions, info, &contents)
            {
                package_file = file;
            }
        }

        let loader = Loader::NodeModuleDirectory {
            package_file: package_file.clone(),
            package_info: package_info.cloned(),
        };

        let index_path = if self.is_config_lookup {
            combine_paths(candidate, &["tsconfig"])
        } else {
            combine_paths(candidate, &["index"])
        };

        if version_paths.exists()
            && (package_file.is_empty()
                || contains_path(candidate, &package_file, &ComparePathsOptions::default()))
        {
            let module_name = if package_file.is_empty() {
                get_relative_path_from_directory(
                    candidate,
                    &index_path,
                    &ComparePathsOptions::default(),
                )
            } else {
                get_relative_path_from_directory(
                    candidate,
                    &package_file,
                    &ComparePathsOptions::default(),
                )
            };
            self.trace(
                &messages::X_PACKAGE_JSON_HAS_A_TYPESVERSIONS_ENTRY_0_THAT_MATCHES_COMPILER_VERSION_1_LOOKING_FOR_A_PATTERN_TO_MATCH_MODULE_NAME_2,
                &[&version_paths.version, TYPESCRIPT_VERSION, &module_name],
            );
            let patterns = try_parse_patterns(&version_paths.paths);
            if let Some(result) = self.try_load_module_using_paths(
                extensions,
                &module_name,
                candidate,
                &version_paths.paths,
                &patterns,
                &loader,
            ) {
                return Some(result);
            }
        }

        if !package_file.is_empty()
            && let Some(result) = self.run_loader(&loader, extensions, &package_file.clone())
        {
            return Some(result);
        }

        // ESM does no `index` lookup.
        if self.esm_mode {
            return continue_searching();
        }
        if !self.fs().directory_exists(candidate) {
            return continue_searching();
        }
        self.load_module_from_file(extensions, &index_path)
    }

    /// `resolutionState.loadFileNameFromPackageJSONField`.
    ///
    /// Only ever called with paths written *in a `package.json`*, never with a
    /// specifier from source — which is why it always permits a TypeScript
    /// extension on the candidate.
    fn load_file_name_from_package_json_field(
        &mut self,
        extensions: Extensions,
        candidate: &str,
        package_json_value: &str,
    ) -> Search {
        if (extensions.intersects(Extensions::TYPESCRIPT)
            && has_implementation_ts_file_extension(candidate))
            || (extensions.intersects(Extensions::DECLARATION)
                && is_declaration_file_name(candidate))
        {
            if let (path, true) = self.try_file(candidate) {
                let extension = try_extract_ts_extension(&path);
                // True only when the pattern ended in `*` *and* the candidate has
                // a TypeScript extension — meaning the star matched it, rather
                // than the extension coming from the pattern.
                let resolved_using_ts_extension =
                    package_json_value.ends_with('*') && !extension.is_empty();
                return Some(Resolved {
                    path,
                    extension: extension.to_string(),
                    resolved_using_ts_extension,
                    ..Resolved::default()
                });
            }
            return continue_searching();
        }

        if self.is_config_lookup
            && extensions.intersects(Extensions::JSON)
            && file_extension_is(candidate, EXTENSION_JSON)
            && let (path, true) = self.try_file(candidate)
        {
            return Some(Resolved {
                path,
                extension: EXTENSION_JSON.to_string(),
                ..Resolved::default()
            });
        }

        self.load_module_from_file_no_implicit_extensions(extensions, candidate)
    }

    /// `resolutionState.getPackageFile`: `tsconfig`, then `typings`/`types`,
    /// then `main`.
    fn get_package_file(
        &mut self,
        extensions: Extensions,
        info: &Rc<PackageJsonInfo>,
        contents: &PackageJson,
    ) -> Option<String> {
        let directory = info.package_directory.clone();
        if self.is_config_lookup {
            return self.package_json_path_field("tsconfig", &contents.tsconfig, &directory);
        }
        if extensions.intersects(Extensions::DECLARATION) {
            if let Some(file) =
                self.package_json_path_field("typings", &contents.typings, &directory)
            {
                return Some(file);
            }
            if let Some(file) = self.package_json_path_field("types", &contents.types, &directory) {
                return Some(file);
            }
        }
        if extensions.intersects(Extensions::IMPLEMENTATION_FILES.union(Extensions::DECLARATION)) {
            return self.package_json_path_field("main", &contents.main, &directory);
        }
        None
    }

    // ---- package.json ------------------------------------------------------

    /// `resolutionState.getPackageScopeForPath`.
    fn get_package_scope_for_path(&mut self, directory: &str) -> Option<Rc<PackageJsonInfo>> {
        let mut directory = directory.to_string();
        loop {
            if let Some(result) = self.get_package_json_info(&directory) {
                return Some(result);
            }
            let parent = get_directory_path(&directory).to_string();
            if parent == directory {
                return None;
            }
            directory = parent;
        }
    }

    /// `resolutionState.getPackageJsonInfo`.
    ///
    /// The cache is observable: a second lookup of the same `package.json`
    /// reports `according to earlier cached lookups` rather than probing again,
    /// and the baselines record which. Returns `None` for "there is no
    /// `package.json` here" *and* `Some` with no contents never happens — an
    /// entry without contents is cached but reported as absent.
    fn get_package_json_info(&mut self, package_directory: &str) -> Option<Rc<PackageJsonInfo>> {
        let package_json_path = combine_paths(package_directory, &["package.json"]);
        let key = to_path(
            &package_json_path,
            self.current_directory(),
            self.fs().use_case_sensitive_file_names(),
        );

        if let Some(existing) = self.resolver.package_json_cache.borrow().get(&key).cloned() {
            if existing.contents.is_some() {
                self.trace(
                    &messages::FILE_0_EXISTS_ACCORDING_TO_EARLIER_CACHED_LOOKUPS,
                    &[&package_json_path],
                );
                // The cache stores the *first* caller's directory spelling; a
                // later caller comparing against its own candidate needs its own.
                return Some(if existing.package_directory == package_directory {
                    existing
                } else {
                    Rc::new(PackageJsonInfo {
                        package_directory: package_directory.to_string(),
                        directory_exists: existing.directory_exists,
                        contents: existing.contents.clone(),
                    })
                });
            }
            if existing.directory_exists {
                self.trace(
                    &messages::FILE_0_DOES_NOT_EXIST_ACCORDING_TO_EARLIER_CACHED_LOOKUPS,
                    &[&package_json_path],
                );
            }
            return None;
        }

        let directory_exists = self.fs().directory_exists(package_directory);
        if directory_exists && self.fs().file_exists(&package_json_path) {
            let contents = self.fs().read_file(&package_json_path).unwrap_or_default();
            let parsed = PackageJson::parse(&contents);
            self.trace(&messages::FOUND_PACKAGE_JSON_AT_0, &[&package_json_path]);
            let entry = Rc::new(PackageJsonInfo {
                package_directory: package_directory.to_string(),
                directory_exists: true,
                contents: Some(Rc::new(parsed)),
            });
            // First writer wins, as upstream's `LoadOrStore` does.
            let stored = self
                .resolver
                .package_json_cache
                .borrow_mut()
                .entry(key)
                .or_insert_with(|| Rc::clone(&entry))
                .clone();
            return Some(if stored.package_directory == package_directory {
                stored
            } else {
                Rc::new(PackageJsonInfo {
                    package_directory: package_directory.to_string(),
                    directory_exists: stored.directory_exists,
                    contents: stored.contents.clone(),
                })
            });
        }

        if directory_exists {
            self.trace(&messages::FILE_0_DOES_NOT_EXIST, &[&package_json_path]);
        }
        self.resolver.package_json_cache.borrow_mut().entry(key).or_insert_with(|| {
            Rc::new(PackageJsonInfo {
                package_directory: package_directory.to_string(),
                directory_exists,
                contents: None,
            })
        });
        None
    }

    /// Read `typesVersions`, replaying the traces it produced.
    ///
    /// Upstream threads a trace callback into `GetVersionPaths`; here the traces
    /// are computed with the value and replayed on each read, which is the same
    /// observable behaviour.
    fn replay_version_paths(&mut self, contents: &PackageJson) -> VersionPaths {
        let (paths, traces) = contents.get_version_paths();
        if self.tracing {
            self.traces.extend(traces.iter().cloned());
        }
        paths.clone()
    }

    /// `resolutionState.getPackageId`.
    fn get_package_id(
        &mut self,
        resolved_file_name: &str,
        package_info: Option<&Rc<PackageJsonInfo>>,
    ) -> PackageId {
        let Some(info) = package_info.filter(|info| info.exists()) else {
            return PackageId::default();
        };
        let contents = info.contents.clone().expect("checked by exists");
        let (Some(name), Some(version)) =
            (contents.name.value.clone(), contents.version.value.clone())
        else {
            return PackageId::default();
        };
        let sub_module_name = if resolved_file_name.len() > info.package_directory.len() {
            resolved_file_name[info.package_directory.len() + 1..].to_string()
        } else {
            String::new()
        };
        let peer_dependencies = self.read_package_json_peer_dependencies(info, &contents);
        PackageId { name, sub_module_name, version, peer_dependencies }
    }

    /// `resolutionState.readPackageJsonPeerDependencies`.
    ///
    /// Two copies of one package with different peers are different packages, so
    /// the resolved peer versions become part of the package id.
    fn read_package_json_peer_dependencies(
        &mut self,
        info: &Rc<PackageJsonInfo>,
        contents: &PackageJson,
    ) -> String {
        if !self.validate_package_json_field(
            "peerDependencies",
            contents.peer_dependencies.is_present(),
            contents.peer_dependencies.is_valid(),
            "object",
            contents.peer_dependencies.actual_json_type(),
        ) {
            return String::new();
        }
        let Some(peers) = contents.peer_dependencies.value.clone().filter(|p| !p.is_empty()) else {
            return String::new();
        };
        self.trace(&messages::X_PACKAGE_JSON_HAS_A_PEERDEPENDENCIES_FIELD, &[]);

        let package_directory = self.real_path(&info.package_directory);
        let Some(index) = package_directory.rfind("/node_modules") else { return String::new() };
        let node_modules = format!("{}/", &package_directory[..index + "/node_modules".len()]);

        let mut names: Vec<String> = peers.into_iter().map(|(name, _)| name).collect();
        names.sort_unstable();
        let mut result = String::new();
        for name in names {
            let peer = self.get_package_json_info(&format!("{node_modules}{name}"));
            if let Some(peer) = peer.filter(|peer| peer.exists()) {
                let version = peer
                    .contents
                    .as_ref()
                    .and_then(|contents| contents.version.value.clone())
                    .unwrap_or_default();
                result.push('+');
                result.push_str(&name);
                result.push('@');
                result.push_str(&version);
                self.trace(&messages::FOUND_PEERDEPENDENCY_0_WITH_1_VERSION, &[&name, &version]);
            } else {
                self.trace(&messages::FAILED_TO_FIND_PEERDEPENDENCY_0, &[&name]);
            }
        }
        result
    }

    /// `resolutionState.validatePackageJSONField`.
    fn validate_package_json_field(
        &mut self,
        field_name: &str,
        is_present: bool,
        is_valid: bool,
        expected_type: &str,
        actual_type: &str,
    ) -> bool {
        if is_present {
            if is_valid {
                return true;
            }
            self.trace(
                &messages::EXPECTED_TYPE_OF_0_FIELD_IN_PACKAGE_JSON_TO_BE_1_GOT_2,
                &[field_name, expected_type, actual_type],
            );
        }
        self.trace(&messages::X_PACKAGE_JSON_DOES_NOT_HAVE_A_0_FIELD, &[field_name]);
        false
    }

    /// `resolutionState.getPackageJSONPathField`.
    fn package_json_path_field(
        &mut self,
        field_name: &str,
        field: &crate::package_json::Expected<String>,
        directory: &str,
    ) -> Option<String> {
        if !self.validate_package_json_field(
            field_name,
            field.is_present(),
            field.is_valid(),
            "string",
            field.actual_json_type(),
        ) {
            return None;
        }
        let value = field.value.clone().unwrap_or_default();
        if value.is_empty() {
            self.trace(&messages::X_PACKAGE_JSON_HAD_A_FALSY_0_FIELD, &[field_name]);
            return None;
        }
        let path = normalize_path(&combine_paths(directory, &[&value]));
        self.trace(
            &messages::X_PACKAGE_JSON_HAS_0_FIELD_1_THAT_REFERENCES_2,
            &[field_name, &value, &path],
        );
        Some(path)
    }

    /// `resolutionState.conditionMatches`.
    fn condition_matches(&self, condition: &str) -> bool {
        if condition == "default" || self.conditions.iter().any(|c| c == condition) {
            return true;
        }
        // Versioned `types@` conditions only apply when `types` itself does.
        if !self.conditions.iter().any(|c| c == "types") {
            return false;
        }
        util::is_applicable_versioned_types_key(condition)
    }

    // ---- Results -----------------------------------------------------------

    /// `resolutionState.realPath`.
    fn real_path(&mut self, path: &str) -> String {
        let resolved = normalize_path(&self.fs().realpath(path));
        self.trace(&messages::RESOLVING_REAL_PATH_FOR_0_RESULT_1, &[path, &resolved]);
        resolved
    }

    /// `resolutionState.getOriginalAndResolvedFileName`.
    fn original_and_resolved_file_name(&mut self, file_name: &str) -> (String, String) {
        let resolved = self.real_path(file_name);
        if tsr_path::compare_paths(file_name, &resolved, &self.compare_paths_options()).is_eq() {
            // Differing only in case: prefer what the caller wrote, so
            // `forceConsistentCasingInFileNames` can still report it.
            return (String::new(), file_name.to_string());
        }
        (file_name.to_string(), resolved)
    }

    /// `resolutionState.createResolvedModuleHandlingSymlink`.
    fn create_resolved_module_handling_symlink(
        &mut self,
        resolved: Option<Resolved>,
    ) -> ResolvedModule {
        let is_external =
            resolved.as_ref().is_some_and(|resolved| resolved.path.contains("/node_modules/"));
        let mut resolved = resolved;
        if self.options().preserve_symlinks != Tristate::True
            && is_external
            && resolved.as_ref().is_some_and(|r| r.original_path.is_empty())
            && !is_external_module_name_relative(&self.name)
        {
            let path = resolved.as_ref().expect("is_external implies Some").path.clone();
            let (original_path, resolved_file_name) = self.original_and_resolved_file_name(&path);
            if !original_path.is_empty()
                && let Some(resolved) = resolved.as_mut()
            {
                resolved.path = resolved_file_name;
                resolved.original_path = original_path;
            }
        }
        self.create_resolved_module(resolved, is_external)
    }

    /// `resolutionState.createResolvedModule`.
    fn create_resolved_module(
        &mut self,
        resolved: Option<Resolved>,
        is_external_library_import: bool,
    ) -> ResolvedModule {
        let mut result = ResolvedModule {
            resolution_diagnostics: self.diagnostics.clone(),
            ..Default::default()
        };
        if let Some(resolved) = resolved {
            result.resolved_file_name = resolved.path;
            result.original_path = resolved.original_path;
            result.is_external_library_import = is_external_library_import;
            result.resolved_using_ts_extension = resolved.resolved_using_ts_extension;
            result.extension = resolved.extension;
            result.package_id = resolved.package_id;
        }
        result
    }

    /// `resolutionState.createResolvedTypeReferenceDirective`.
    ///
    /// # Panics
    ///
    /// If the resolved file does not have a TypeScript extension. A type
    /// reference directive that resolved to `.js` is a resolver bug, not a user
    /// error, and upstream asserts the same thing.
    fn create_resolved_type_reference_directive(
        &mut self,
        resolved: Option<Resolved>,
        primary: bool,
    ) -> ResolvedTypeReferenceDirective {
        let mut result = ResolvedTypeReferenceDirective {
            resolution_diagnostics: self.diagnostics.clone(),
            ..Default::default()
        };
        let Some(resolved) = resolved.filter(Resolved::is_resolved) else { return result };

        assert!(
            extension_is_ts(&resolved.extension),
            "expected a TypeScript file extension, got {:?}",
            resolved.extension
        );
        result.resolved_file_name.clone_from(&resolved.path);
        result.primary = primary;
        result.package_id = resolved.package_id;
        result.is_external_library_import = resolved.path.contains("/node_modules/");

        if self.options().preserve_symlinks != Tristate::True {
            let (original_path, resolved_file_name) =
                self.original_and_resolved_file_name(&resolved.path);
            if !original_path.is_empty() {
                result.resolved_file_name = resolved_file_name;
                result.original_path = original_path;
            }
        }
        result
    }
}

/// The `@types` packages to include when `types` is unset or contains `*`
/// (`module.GetAutomaticTypeDirectiveNames`).
///
/// A package whose `package.json` says `"typings": null` is skipped:
/// `types-publisher` writes that for packages that ship no types of their own.
#[must_use]
pub fn get_automatic_type_directive_names(
    options: &CompilerOptions,
    host: &dyn ResolutionHost,
) -> Vec<String> {
    if !options.uses_wildcard_types() {
        return options.types.clone().unwrap_or_default();
    }

    let mut wildcard_matches = Vec::new();
    let (type_roots, _) = options.get_effective_type_roots(host.current_directory());
    for root in &type_roots {
        if !host.fs().directory_exists(root) {
            continue;
        }
        for directory in host.fs().get_accessible_entries(root).directories {
            let normalized = normalize_path(&directory);
            let package_json_path = combine_paths(root, &[&normalized, "package.json"]);
            let is_not_needed_package = host
                .fs()
                .read_file(&package_json_path)
                .map(|contents| PackageJson::parse(&contents))
                .is_some_and(|package| package.typings.null);
            if is_not_needed_package {
                continue;
            }
            let base = get_base_file_name(&normalized);
            if !base.starts_with('.') {
                wildcard_matches.push(base.to_string());
            }
        }
    }

    // The wildcard expands *in place*, because order matters to program
    // construction.
    let mut result: Vec<String> = Vec::new();
    for entry in options.types.iter().flatten() {
        if entry == "*" {
            result.extend(wildcard_matches.iter().cloned());
        } else {
            result.push(entry.clone());
        }
    }
    result.dedup();
    result
}

#[cfg(test)]
mod cache_tests {
    use std::cell::Cell;

    use super::*;
    use tsr_vfs::{DirectoryEntries, FileSystem, InMemoryFileSystem};

    struct CountingFileSystem {
        snapshot: RefCell<InMemoryFileSystem>,
        probes: Cell<usize>,
    }

    impl CountingFileSystem {
        fn probe(&self) {
            self.probes.set(self.probes.get() + 1);
        }
    }

    impl FileSystem for CountingFileSystem {
        fn use_case_sensitive_file_names(&self) -> bool {
            true
        }
        fn file_exists(&self, path: &str) -> bool {
            self.probe();
            self.snapshot.borrow().file_exists(path)
        }
        fn read_file(&self, path: &str) -> Option<String> {
            self.probe();
            self.snapshot.borrow().read_file(path)
        }
        fn directory_exists(&self, path: &str) -> bool {
            self.probe();
            self.snapshot.borrow().directory_exists(path)
        }
        fn get_accessible_entries(&self, path: &str) -> DirectoryEntries {
            self.probe();
            self.snapshot.borrow().get_accessible_entries(path)
        }
        fn realpath(&self, path: &str) -> String {
            self.probe();
            self.snapshot.borrow().realpath(path)
        }
    }

    struct Host {
        fs: CountingFileSystem,
    }

    impl ResolutionHost for Host {
        fn fs(&self) -> &dyn FileSystem {
            &self.fs
        }
        fn current_directory(&self) -> &'static str {
            "/"
        }
    }

    fn snapshot(files: &[(&str, &str)]) -> InMemoryFileSystem {
        InMemoryFileSystem::new(
            files.iter().map(|(name, text)| (name.to_string(), text.to_string())),
            [],
            true,
        )
    }

    fn host(files: &[(&str, &str)]) -> Host {
        Host {
            fs: CountingFileSystem {
                snapshot: RefCell::new(snapshot(files)),
                probes: Cell::new(0),
            },
        }
    }

    fn options() -> CompilerOptions {
        CompilerOptions {
            module: tsr_core::ModuleKind::NodeNext,
            module_resolution: ModuleResolutionKind::NodeNext,
            ..Default::default()
        }
    }

    #[test]
    fn sibling_module_queries_reuse_success_and_failure_without_filesystem_probes() {
        let host = host(&[("/src/local.ts", "export const value = 1;")]);
        let resolver = Resolver::new(&host, options());
        for name in ["./local", "./absent"] {
            let (first, traces) =
                resolver.resolve_module_name(name, "/src/a.ts", ResolutionMode::CommonJS);
            assert!(traces.is_empty());
            let probes = host.fs.probes.get();
            assert!(probes > 0);
            let (second, traces) =
                resolver.resolve_module_name(name, "/src/b.ts", ResolutionMode::CommonJS);
            assert_eq!(first.is_resolved(), name == "./local");
            assert_eq!(second.resolved_file_name, first.resolved_file_name);
            assert_eq!(host.fs.probes.get(), probes, "cached {name} touched the filesystem");
            assert!(traces.is_empty());
        }
    }

    #[test]
    fn directory_and_resolution_mode_are_distinct_cache_keys() {
        let host = host(&[
            (
                "/node_modules/pkg/package.json",
                r#"{"exports":{"import":"./esm.d.ts","require":"./cjs.d.ts"}}"#,
            ),
            ("/node_modules/pkg/esm.d.ts", "export const value: number;"),
            ("/node_modules/pkg/cjs.d.ts", "export const value: string;"),
            ("/other/node_modules/pkg/index.d.ts", "export const value: boolean;"),
        ]);
        let resolver = Resolver::new(&host, options());
        for (file, mode, expected) in [
            ("/src/a.ts", ResolutionMode::CommonJS, "/node_modules/pkg/cjs.d.ts"),
            ("/src/b.ts", ResolutionMode::ESNext, "/node_modules/pkg/esm.d.ts"),
            ("/other/c.ts", ResolutionMode::CommonJS, "/other/node_modules/pkg/index.d.ts"),
        ] {
            let probes = host.fs.probes.get();
            let (result, _) = resolver.resolve_module_name("pkg", file, mode);
            assert_eq!(result.resolved_file_name, expected);
            assert!(host.fs.probes.get() > probes);
        }
    }

    #[test]
    fn inferred_type_requests_do_not_reuse_ordinary_type_reference_results() {
        let host = host(&[
            ("/custom/placeholder", ""),
            (
                "/src/node_modules/@types/pkg/package.json",
                r#"{"exports":{"require":"./cjs.d.ts","import":"./esm.d.ts"}}"#,
            ),
            ("/src/node_modules/@types/pkg/cjs.d.ts", "export const value: number;"),
            ("/src/node_modules/@types/pkg/esm.d.ts", "export const value: string;"),
        ]);
        let mut options = options();
        options.type_roots = Some(vec!["/custom".into()]);
        let resolver = Resolver::new(&host, options);
        let (ordinary, _) =
            resolver.resolve_type_reference_directive("pkg", "/src/a.ts", ResolutionMode::CommonJS);
        assert!(ordinary.is_resolved());
        let probes = host.fs.probes.get();
        let (sibling, _) =
            resolver.resolve_type_reference_directive("pkg", "/src/b.ts", ResolutionMode::CommonJS);
        assert_eq!(sibling.resolved_file_name, ordinary.resolved_file_name);
        assert_eq!(host.fs.probes.get(), probes);
        let (esm, _) =
            resolver.resolve_type_reference_directive("pkg", "/src/a.ts", ResolutionMode::ESNext);
        assert_ne!(esm.resolved_file_name, ordinary.resolved_file_name);
        assert!(esm.resolved_file_name.ends_with("/esm.d.ts"));
        let inferred = format!("/src/{INFERRED_TYPES_CONTAINING_FILE}");
        let (missing, _) =
            resolver.resolve_type_reference_directive("pkg", &inferred, ResolutionMode::CommonJS);
        assert!(!missing.is_resolved(), "custom type roots suppress inferred secondary lookup");
        let probes = host.fs.probes.get();
        let (again, _) =
            resolver.resolve_type_reference_directive("pkg", &inferred, ResolutionMode::CommonJS);
        assert!(!again.is_resolved());
        assert_eq!(host.fs.probes.get(), probes);
    }

    #[test]
    fn trace_resolution_bypasses_query_cache_and_keeps_package_cache_traces() {
        let host = host(&[
            (
                "/node_modules/pkg/package.json",
                r#"{"name":"pkg","version":"1.0.0","types":"index.d.ts"}"#,
            ),
            ("/node_modules/pkg/index.d.ts", "export const value: number;"),
        ]);
        let mut options = options();
        options.trace_resolution = Tristate::True;
        let resolver = Resolver::new(&host, options);
        let (_, first) = resolver.resolve_module_name("pkg", "/src/a.ts", ResolutionMode::CommonJS);
        let probes = host.fs.probes.get();
        let (_, second) =
            resolver.resolve_module_name("pkg", "/src/b.ts", ResolutionMode::CommonJS);
        assert!(host.fs.probes.get() > probes);
        assert!(first[0].text.contains("/src/a.ts"));
        assert!(second[0].text.contains("/src/b.ts"));
        assert!(
            second.iter().any(|trace| trace.text.contains("according to earlier cached lookups"))
        );
    }

    #[test]
    fn new_resolver_observes_new_snapshot_and_config_queries_have_separate_semantics() {
        let host = host(&[]);
        let resolver = Resolver::new(&host, options());
        assert!(
            !resolver
                .resolve_module_name("./late", "/src/a.ts", ResolutionMode::CommonJS)
                .0
                .is_resolved()
        );
        host.fs
            .snapshot
            .replace(snapshot(&[("/src/late.ts", "export {};"), ("/src/late.json", "{}")]));
        assert!(
            !resolver
                .resolve_module_name("./late", "/src/b.ts", ResolutionMode::CommonJS)
                .0
                .is_resolved()
        );
        let fresh = Resolver::new(&host, options());
        assert_eq!(
            fresh
                .resolve_module_name("./late", "/src/a.ts", ResolutionMode::CommonJS)
                .0
                .resolved_file_name,
            "/src/late.ts"
        );
        assert_eq!(
            fresh.resolve_config("./late", "/src/tsconfig.json").resolved_file_name,
            "/src/late.json"
        );
    }

    #[test]
    fn cached_results_preserve_symlink_and_package_identity() {
        let host = host(&[]);
        host.fs.snapshot.replace(InMemoryFileSystem::new(
            [
                (
                    "/store/pkg/package.json".into(),
                    r#"{"name":"pkg","version":"1.0.0","types":"index.d.ts"}"#.into(),
                ),
                ("/store/pkg/index.d.ts".into(), "export const value: number;".into()),
            ],
            [("/src/node_modules/pkg".into(), "/store/pkg".into())],
            true,
        ));
        let resolver = Resolver::new(&host, options());
        let (first, _) = resolver.resolve_module_name("pkg", "/src/a.ts", ResolutionMode::CommonJS);
        assert_eq!(first.resolved_file_name, "/store/pkg/index.d.ts");
        assert_eq!(first.original_path, "/src/node_modules/pkg/index.d.ts");
        assert!(first.package_id.is_set());
        let probes = host.fs.probes.get();
        let (cached, _) =
            resolver.resolve_module_name("pkg", "/src/b.ts", ResolutionMode::CommonJS);
        assert_eq!(cached.resolved_file_name, first.resolved_file_name);
        assert_eq!(cached.original_path, first.original_path);
        assert_eq!(cached.package_id, first.package_id);
        assert_eq!(host.fs.probes.get(), probes);
        let mut options = options();
        options.preserve_symlinks = Tristate::True;
        let preserving = Resolver::new(&host, options);
        let (logical, _) =
            preserving.resolve_module_name("pkg", "/src/a.ts", ResolutionMode::CommonJS);
        assert_eq!(logical.resolved_file_name, "/src/node_modules/pkg/index.d.ts");
    }
}
