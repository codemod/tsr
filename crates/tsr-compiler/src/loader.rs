//! The file loader: the walk that decides what to resolve, and in what order.
//!
//! Ported from `internal/compiler/fileloader.go` and
//! `internal/compiler/filesparser.go` at the pinned commit — the part of them
//! that discovers files. Together those are 1,358 lines; most of that is project
//! references, emit, redirect deduplication, include-reason bookkeeping for
//! `--explainFiles`, and lib sorting, none of which exists here yet. What is here
//! is the DFS over root files and the per-task trace buffering.
//!
//! # What this is for
//!
//! [`tsr_module`] answers "given this specifier, from this file, in this mode,
//! where does it point, and by what route". Nothing asked it. This asks: it walks
//! the program from its root files, collects each file's imports and reference
//! directives, resolves them, and recurses into what came back.
//!
//! [ADR-0018](../../../docs/adr/0018-splitting-the-resolution-oracle.md) split
//! the `.trace.json` oracle along exactly this seam — the resolver owns
//! everything between the `======== Resolving … ========` headers, and this owns
//! the headers and their order. [`crate::loader`]'s gate is that header sequence.
//!
//! # Why traces are buffered rather than written as they happen
//!
//! Upstream loads files in parallel: `filesParser.start` queues every task on a
//! work group, and a file's imports are resolved on whichever worker picked it
//! up. If each resolution wrote its trace to the host as it ran, the order of
//! 6,700 baselined lines would depend on thread scheduling.
//!
//! So it does not. Each task accumulates its own `typeResolutionsTrace` and
//! `resolutionsTrace`, and `filesParser.getProcessedFiles` replays them during a
//! **single-threaded depth-first walk** of the task tree, type resolutions before
//! module resolutions, per file. That replay order is the observable one, and it
//! is what the baselines record.
//!
//! This port keeps the buffering and the replay, and loads single-threaded. The
//! buffering is not redundant even so: it is what lets the walk that *discovers*
//! files and the walk that *reports* them be different walks, which is the
//! property the parallel version needs and the reason the two-phase shape is
//! worth keeping rather than collapsing.
//!
//! # Deliberate omissions, and how you would notice
//!
//! Each of these is a place the header sequence could be wrong, so each is named
//! rather than left to be discovered:
//!
//! - **Lib files are not loaded.** A default-lib or `/// <reference lib="…" />`
//!   task resolves nothing (`parseTask.load` gives lib files a fixed
//!   `CommonJS` metadata precisely to avoid a `package.json` lookup), so it
//!   contributes no trace. The exception is `libReplacement`, which resolves
//!   `@typescript/lib-*` through the module resolver and *does* trace; that
//!   needs the bundled lib files on the host and is not done (bd tsr-9or.5).
//! - **`importHelpers` does not synthesise a `tslib` import.** No `.trace.json`
//!   baseline contains one — checked, `Resolving module 'tslib'` appears zero
//!   times across all 146 — so implementing it would be untested code.
//! - **Project references, redirects, and package deduplication** are absent, as
//!   they are from [`crate::Program`].
//! - **`moduleDetection` is assumed `auto`**, upstream's default. It is not a
//!   ported option; a case setting `moduleDetection: legacy` or `force` would
//!   get the wrong answer for whether a file is an external module, which
//!   changes how `declare module "x"` inside it is classified.

use rustc_hash::{FxHashMap, FxHashSet};
use tsr_core::{CompilerOptions, JsxEmit, ModuleKind, ModuleResolutionKind, ResolutionMode};
use tsr_module::{
    messages::Trace,
    resolver::Resolver,
    types::{ResolutionHost, ResolvedModule},
};
use tsr_parser::{CollectOptions, SpecifierContext};
use tsr_path::{
    Path, combine_paths,
    extension::{
        ALL_SUPPORTED_EXTENSIONS, ALL_SUPPORTED_EXTENSIONS_WITH_JSON, EXTENSION_CJS, EXTENSION_CTS,
        EXTENSION_DCTS, EXTENSION_DMTS, EXTENSION_DTS, EXTENSION_JS, EXTENSION_JSON, EXTENSION_JSX,
        EXTENSION_MJS, EXTENSION_MTS, EXTENSION_TS, EXTENSION_TSX, SUPPORTED_TS_EXTENSIONS,
        SUPPORTED_TS_EXTENSIONS_WITH_JSON, SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT,
        file_extension_is, file_extension_is_one_of,
    },
    get_base_file_name, get_canonical_file_name, get_directory_path, get_normalized_absolute_path,
    is_declaration_file_name, is_rooted_disk_path, normalize_path, to_path,
};

use crate::ProgramFile;

/// The containing file the automatic `types` resolutions are made from
/// (`module.InferredTypesContainingFile`).
const INFERRED_TYPES_CONTAINING_FILE: &str = "__inferred type names__.ts";

/// What kind of thing a resolution request asks for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RequestKind {
    /// An `import`, `require`, or module augmentation.
    Module,
    /// A `/// <reference types="…" />` or a `types` entry.
    TypeReferenceDirective,
}

/// One resolution the loader asked for.
///
/// This is the unit the loader is judged on: it is exactly what a
/// `======== Resolving … ========` header records.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionRequest {
    /// Module or type reference directive.
    pub kind: RequestKind,
    /// The specifier, as written.
    pub name: String,
    /// The file it was written in.
    pub containing_file: String,
    /// The format it is resolved as.
    pub mode: ResolutionMode,
}

/// Everything one load produced (`compiler.processedFiles`, reduced to what
/// exists).
#[derive(Debug, Default)]
pub struct LoadedFiles {
    /// Every file the walk reached, in the order the walk reached it.
    pub file_names: Vec<String>,
    /// Every resolution requested, in replay order.
    pub requests: Vec<ResolutionRequest>,
    /// Every trace line the resolver produced, in replay order.
    pub traces: Vec<Trace>,
}

/// A file's module format and where that format came from
/// (`ast.SourceFileMetaData`).
#[derive(Debug, Clone, Default)]
struct SourceFileMetaData {
    /// The `type` field of the nearest enclosing `package.json`, if it applies.
    package_json_type: String,
    /// The format the file is treated as.
    implied_node_format: ResolutionMode,
}

/// How far into `node_modules` reaching a file went, and whether that matters.
///
/// The two travel together everywhere — upstream sets them on the same
/// `resolvedRef` and reads them in the same line of `filesParser.start` — and
/// keeping them together is what stops [`ParseTask`] being a bag of flags.
#[derive(Debug, Clone, Copy, Default)]
struct Depth {
    /// Whether reaching this file crossed into `node_modules`.
    increase: bool,
    /// Whether this file is dropped once the depth budget is spent, which is
    /// true only of a JavaScript file found inside `node_modules`.
    elide: bool,
}

/// One file the walk wants (`compiler.parseTask`).
struct ParseTask {
    file_name: String,
    path: Path,
    /// Whether this is the synthetic task that resolves `types`/`@types`.
    is_for_automatic_type_directive: bool,
    depth: Depth,
    /// Whether [`FileLoader::load_task`] ran for this task. A task for a path
    /// some earlier task already claimed stays unloaded and is skipped by the
    /// replay walk, along with its subtree.
    loaded: bool,
    sub_tasks: Vec<usize>,
    metadata: SourceFileMetaData,
    type_resolutions_trace: Vec<Trace>,
    type_resolution_requests: Vec<ResolutionRequest>,
    resolutions_trace: Vec<Trace>,
    resolution_requests: Vec<ResolutionRequest>,
}

impl ParseTask {
    fn new(file_name: String, path: Path) -> Self {
        Self {
            file_name,
            path,
            is_for_automatic_type_directive: false,
            depth: Depth::default(),
            loaded: false,
            sub_tasks: Vec::new(),
            metadata: SourceFileMetaData::default(),
            type_resolutions_trace: Vec::new(),
            type_resolution_requests: Vec::new(),
            resolutions_trace: Vec::new(),
            resolution_requests: Vec::new(),
        }
    }
}

/// A resolved reference that becomes a subtask (`compiler.resolvedRef`).
struct ResolvedRef {
    file_name: String,
    depth: Depth,
}

/// The walk (`compiler.fileLoader` + `compiler.filesParser`).
pub struct FileLoader<'host> {
    host: &'host dyn ResolutionHost,
    options: CompilerOptions,
    resolver: Resolver<'host>,
    tasks: Vec<ParseTask>,
    root_tasks: Vec<usize>,
    /// The task that claimed each path. Upstream's `taskDataByPath`, minus the
    /// per-casing map: see [`FileLoader::process_task`].
    claimed: FxHashMap<Path, usize>,
    supported_extensions: &'static [&'static [&'static str]],
    supported_extensions_with_json: &'static [&'static [&'static str]],
    max_node_module_js_depth: i32,
}

impl<'host> FileLoader<'host> {
    /// Walk the program from `root_file_names` (`processAllProgramFiles`).
    ///
    /// Root files are taken as given, which is what upstream does too — they
    /// come from `ParsedCommandLine.FileNames`, and turning a `tsconfig.json`
    /// into that list is `tsr-tsoptions`' job (bd tsr-9or slice 3).
    #[must_use]
    pub fn load(
        host: &'host dyn ResolutionHost,
        options: CompilerOptions,
        root_file_names: &[String],
    ) -> LoadedFiles {
        let supported_extensions =
            if options.get_allow_js() { ALL_SUPPORTED_EXTENSIONS } else { SUPPORTED_TS_EXTENSIONS };
        let supported_extensions_with_json = if !options.get_resolve_json_module() {
            supported_extensions
        } else if options.get_allow_js() {
            ALL_SUPPORTED_EXTENSIONS_WITH_JSON
        } else {
            SUPPORTED_TS_EXTENSIONS_WITH_JSON
        };

        let mut loader = Self {
            resolver: Resolver::new(host, options.clone()),
            host,
            max_node_module_js_depth: options.max_node_module_js_depth.unwrap_or(0),
            options,
            tasks: Vec::new(),
            root_tasks: Vec::new(),
            claimed: FxHashMap::default(),
            supported_extensions,
            supported_extensions_with_json,
        };

        for root in root_file_names {
            loader.add_root_file_task(root);
        }
        // Lib files would be added here (`compilerOptions.Lib`, or the default
        // for the target). They resolve nothing; see the module docs.
        if !root_file_names.is_empty() {
            loader.add_automatic_type_directive_task();
        }

        let roots = loader.root_tasks.clone();
        for root in roots {
            loader.process_task(root, 0);
        }
        loader.collect_files()
    }

    // ---- Root tasks --------------------------------------------------------

    /// `fileLoader.addRootFileTask`.
    fn add_root_file_task(&mut self, file_name: &str) {
        let current_directory = self.host.current_directory().to_string();
        let absolute = get_normalized_absolute_path(file_name, &current_directory);
        // A root file with no extension is resolved against the first extension
        // group, so `// @filename: a` with an `a.ts` on disk still loads. A root
        // file that does not resolve stays in the task list under its original
        // name and becomes a missing-file diagnostic upstream.
        let resolved =
            self.source_file_from_reference(&absolute, &current_directory).unwrap_or(absolute);
        self.push_root(resolved);
    }

    /// `fileLoader.addAutomaticTypeDirectiveTasks`.
    ///
    /// Appended after the root files, which is why every `types`/`@types`
    /// resolution appears at the *end* of a baseline rather than the start.
    fn add_automatic_type_directive_task(&mut self) {
        let containing_directory = if self.options.config_file_path.is_empty() {
            self.host.current_directory().to_string()
        } else {
            get_directory_path(&self.options.config_file_path).to_string()
        };
        let file_name = combine_paths(&containing_directory, &[INFERRED_TYPES_CONTAINING_FILE]);
        let index = self.push_root(file_name);
        self.tasks[index].is_for_automatic_type_directive = true;
    }

    fn push_root(&mut self, file_name: String) -> usize {
        let index = self.new_task(file_name);
        self.root_tasks.push(index);
        index
    }

    fn new_task(&mut self, file_name: String) -> usize {
        let path = self.to_path(&file_name);
        self.tasks.push(ParseTask::new(file_name, path));
        self.tasks.len() - 1
    }

    fn to_path(&self, file_name: &str) -> Path {
        to_path(
            file_name,
            self.host.current_directory(),
            self.host.fs().use_case_sensitive_file_names(),
        )
    }

    // ---- The walk ----------------------------------------------------------

    /// Claim a path, load it, and recurse (`filesParser.start`).
    ///
    /// Upstream keys its task data by path *and* by file-name casing, so one
    /// path reached under two spellings loads twice, and re-reaches a task at a
    /// lower depth to reprocess subtasks it had elided. Neither is reproduced:
    /// this claims a path once, at the depth it is first reached in depth-first
    /// order.
    ///
    /// Both simplifications are visible in the oracle if they are wrong — a
    /// second casing would produce a second set of resolutions, and a
    /// too-shallow claim would drop them entirely — which is why they are worth
    /// taking rather than porting `parseTaskData`'s mutex and depth bookkeeping
    /// before anything needs them.
    fn process_task(&mut self, index: usize, depth: i32) {
        let path = self.tasks[index].path.clone();
        if self.claimed.contains_key(&path) {
            return;
        }
        self.claimed.insert(path, index);

        let current_depth = depth + i32::from(self.tasks[index].depth.increase);
        if self.tasks[index].depth.elide && current_depth > self.max_node_module_js_depth {
            return;
        }

        self.load_task(index);
        let sub_tasks = self.tasks[index].sub_tasks.clone();
        for sub_task in sub_tasks {
            self.process_task(sub_task, current_depth);
        }
    }

    /// `parseTask.load`.
    fn load_task(&mut self, index: usize) {
        self.tasks[index].loaded = true;
        if self.tasks[index].is_for_automatic_type_directive {
            self.load_automatic_type_directives(index);
            return;
        }

        let file_name = self.tasks[index].file_name.clone();
        if has_extension(&file_name) && !self.options.allow_non_ts_extensions.is_true() {
            let canonical =
                get_canonical_file_name(&file_name, self.host.fs().use_case_sensitive_file_names());
            if !self.is_supported_extension(&canonical) {
                // An unsupported extension is a diagnostic upstream, not a file.
                return;
            }
        }

        self.tasks[index].metadata = self.load_source_file_meta_data(&file_name);

        let Some(text) = self.host.fs().read_file(&file_name) else { return };
        let file = ProgramFile::parse(self.tasks[index].path.clone(), file_name.clone(), text);

        // `/// <reference path="…" />` — a file, not a module: no resolver, no
        // trace, but it is a subtask and so it is walked, and what *it* imports
        // is resolved.
        if !self.options.no_resolve.is_true() {
            let references = file.file_references().referenced_files.clone();
            for reference in &references {
                if let Some(resolved) =
                    self.resolve_tripleslash_path_reference(&reference.file_name, &file_name)
                {
                    self.add_sub_task(index, &resolved);
                }
            }
            self.resolve_type_reference_directives(index, &file);
        }

        // `/// <reference lib="…" />` would be handled here. See the module docs.

        self.resolve_imports_and_module_augmentations(index, &file);
    }

    /// `parseTask.loadAutomaticTypeDirectives` +
    /// `fileLoader.resolveAutomaticTypeDirectives`.
    fn load_automatic_type_directives(&mut self, index: usize) {
        let containing_file = self.tasks[index].file_name.clone();
        let names =
            tsr_module::resolver::get_automatic_type_directive_names(&self.options, self.host);
        for name in names {
            // Upstream passes an unspecified mode deliberately: under
            // node16/nodenext a `types` entry loads as CommonJS, and under
            // bundler the unspecified mode is what triggers the `import`
            // condition.
            let mode = ResolutionMode::None;
            let (resolved, traces) =
                self.resolver.resolve_type_reference_directive(&name, &containing_file, mode);
            self.tasks[index].type_resolution_requests.push(ResolutionRequest {
                kind: RequestKind::TypeReferenceDirective,
                name: name.clone(),
                containing_file: containing_file.clone(),
                mode,
            });
            self.tasks[index].type_resolutions_trace.extend(traces);
            if resolved.is_resolved() {
                self.add_sub_task(
                    index,
                    &ResolvedRef {
                        file_name: resolved.resolved_file_name,
                        depth: Depth {
                            increase: resolved.is_external_library_import,
                            elide: false,
                        },
                    },
                );
            }
        }
    }

    /// `fileLoader.resolveTypeReferenceDirectives`.
    fn resolve_type_reference_directives(&mut self, index: usize, file: &ProgramFile) {
        let directives = file.file_references().type_reference_directives.clone();
        if directives.is_empty() {
            return;
        }
        let file_name = self.tasks[index].file_name.clone();
        let metadata = self.tasks[index].metadata.clone();

        for directive in &directives {
            // `getModeForTypeReferenceDirectiveInFile`: an explicit
            // `resolution-mode` on the directive wins over the file's format.
            let mode = match directive.resolution_mode {
                tsr_parser::ResolutionMode::CommonJS => ResolutionMode::CommonJS,
                tsr_parser::ResolutionMode::ESNext => ResolutionMode::ESNext,
                tsr_parser::ResolutionMode::None => {
                    self.default_resolution_mode_for_file(&file_name, &metadata)
                }
            };
            let (resolved, traces) = self.resolver.resolve_type_reference_directive(
                &directive.file_name,
                &file_name,
                mode,
            );
            self.tasks[index].type_resolution_requests.push(ResolutionRequest {
                kind: RequestKind::TypeReferenceDirective,
                name: directive.file_name.clone(),
                containing_file: file_name.clone(),
                mode,
            });
            self.tasks[index].type_resolutions_trace.extend(traces);
            if resolved.is_resolved() {
                self.add_sub_task(
                    index,
                    &ResolvedRef {
                        file_name: resolved.resolved_file_name,
                        depth: Depth {
                            increase: resolved.is_external_library_import,
                            elide: false,
                        },
                    },
                );
            }
        }
    }

    /// `fileLoader.resolveImportsAndModuleAugmentations`.
    fn resolve_imports_and_module_augmentations(&mut self, index: usize, file: &ProgramFile) {
        let file_name = self.tasks[index].file_name.clone();
        let metadata = self.tasks[index].metadata.clone();
        let is_js_file = is_javascript_file(&file_name);
        let is_declaration_file = is_declaration_file_name(&file_name);

        let is_external_module = file
            .with_ast(|source_file| self.is_external_module(source_file, &file_name, &metadata));
        let references = file.with_ast(|source_file| {
            tsr_parser::collect_external_module_references(
                source_file,
                file.nodes(),
                CollectOptions { is_declaration_file, is_js_file, is_external_module },
            )
        });

        let mut specifiers: Vec<tsr_parser::ModuleSpecifier> = Vec::new();
        // `importHelpers`' `tslib` would come first. See the module docs.
        if is_js_file || file_extension_is(&file_name, EXTENSION_TSX) {
            if let Some(jsx_import) = self.jsx_runtime_import() {
                specifiers.push(tsr_parser::ModuleSpecifier {
                    text: jsx_import,
                    pos: 0,
                    context: SpecifierContext::Synthetic,
                    resolution_mode_override: tsr_parser::ResolutionMode::None,
                });
            }
        }
        let imports_start = specifiers.len();
        let import_count = references.imports.len();
        specifiers.extend(references.imports);
        specifiers.extend(references.module_augmentations);

        for (position, specifier) in specifiers.iter().enumerate() {
            if specifier.text.is_empty() {
                continue;
            }
            let mode = self.mode_for_usage_location(&file_name, &metadata, specifier);
            let (resolved, traces) =
                self.resolver.resolve_module_name(&specifier.text, &file_name, mode);
            self.tasks[index].resolution_requests.push(ResolutionRequest {
                kind: RequestKind::Module,
                name: specifier.text.clone(),
                containing_file: file_name.clone(),
                mode,
            });
            self.tasks[index].resolutions_trace.extend(traces);

            if !resolved.is_resolved() {
                continue;
            }
            // A synthetic specifier has no index among the file's own imports,
            // which is what upstream's `importIndex < 0` means.
            let is_synthetic = position < imports_start;
            let is_module_augmentation = position >= imports_start + import_count;
            // A module augmentation is resolved but never adds a file: upstream
            // reaches the same place through index arithmetic, where an
            // augmentation's index is past the end of `file.Imports()` and so
            // fails `shouldAddFile`'s `importIndex < len(file.Imports())`.
            if !is_module_augmentation
                && self.should_add_file(&resolved, is_declaration_file, is_synthetic)
            {
                let resolved_file_name = resolved.resolved_file_name.clone();
                let is_js = !file_extension_is_one_of(
                    &resolved_file_name,
                    SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT,
                );
                self.add_sub_task(
                    index,
                    &ResolvedRef {
                        depth: Depth {
                            increase: resolved.is_external_library_import,
                            elide: resolved.is_external_library_import
                                && is_js
                                && resolved_file_name.contains("/node_modules/"),
                        },
                        file_name: resolved_file_name,
                    },
                );
            }
        }
    }

    /// `module.GetResolutionDiagnostic`, collapsed to the question the loader
    /// asks of it.
    ///
    /// Upstream returns the diagnostic *message* so `--explainFiles` and the
    /// checker can report it; here nothing consumes the message, so returning
    /// which one it was would be storage for a value no caller reads. The
    /// predicate — and every branch that decides it — is the same.
    fn should_add_file(
        &self,
        resolved: &ResolvedModule,
        containing_is_declaration_file: bool,
        _is_synthetic: bool,
    ) -> bool {
        let need_jsx = || self.options.jsx != JsxEmit::None;
        let need_allow_js = || {
            self.options.get_allow_js()
                || !self.options.no_implicit_any.default_if_unknown(self.options.strict).is_true()
        };
        let extension = resolved.extension.as_str();
        let diagnostic_free = if matches!(
            extension,
            EXTENSION_TS
                | EXTENSION_DTS
                | EXTENSION_MTS
                | EXTENSION_DMTS
                | EXTENSION_CTS
                | EXTENSION_DCTS
        ) {
            true
        } else if extension == EXTENSION_TSX {
            need_jsx()
        } else if extension == EXTENSION_JSX {
            need_jsx() && need_allow_js()
        } else if matches!(extension, EXTENSION_JS | EXTENSION_MJS | EXTENSION_CJS) {
            need_allow_js()
        } else if extension == EXTENSION_JSON {
            self.options.get_resolve_json_module()
        } else {
            containing_is_declaration_file || self.options.allow_arbitrary_extensions.is_true()
        };
        if !diagnostic_free || self.options.no_resolve.is_true() {
            return false;
        }
        let is_js = !file_extension_is_one_of(
            &resolved.resolved_file_name,
            SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT,
        );
        !is_js || self.options.get_allow_js()
    }

    /// `fileLoader.resolveTripleslashPathReference`.
    ///
    /// The only reference kind that is *not* a resolution: a `path` reference
    /// names a file directly, so it is checked against the file system and
    /// produces no trace.
    fn resolve_tripleslash_path_reference(
        &self,
        module_name: &str,
        containing_file: &str,
    ) -> Option<ResolvedRef> {
        let base = get_directory_path(containing_file);
        let referenced = if is_rooted_disk_path(module_name) {
            module_name.to_string()
        } else {
            combine_paths(base, &[module_name])
        };
        let normalized = normalize_path(&referenced);
        let resolved = self.source_file_from_reference(&normalized, containing_file)?;
        Some(ResolvedRef { file_name: resolved, depth: Depth::default() })
    }

    /// `fileLoader.getSourceFileFromReference`, reduced to its success value.
    ///
    /// Every `None` here is a diagnostic upstream — unsupported extension, file
    /// not found, a file referencing itself. None of them is a resolution, so
    /// none of them is visible in the oracle this slice is judged on.
    fn source_file_from_reference(&self, file_name: &str, containing_file: &str) -> Option<String> {
        let allow_non_ts_extensions = self.options.allow_non_ts_extensions.is_true();
        let case_sensitive = self.host.fs().use_case_sensitive_file_names();

        if has_extension(file_name) {
            let canonical = get_canonical_file_name(file_name, case_sensitive);
            if !allow_non_ts_extensions && !self.is_supported_extension(&canonical) {
                return None;
            }
            if !self.host.fs().file_exists(file_name) {
                return None;
            }
            if get_canonical_file_name(containing_file, case_sensitive) == canonical {
                return None;
            }
            return Some(file_name.to_string());
        }

        if allow_non_ts_extensions {
            return self.host.fs().file_exists(file_name).then(|| file_name.to_string());
        }
        // Only the *first* extension group, so `./foo` never finds `foo.mts`.
        self.supported_extensions[0].iter().find_map(|extension| {
            let candidate = format!("{file_name}{extension}");
            self.host.fs().file_exists(&candidate).then_some(candidate)
        })
    }

    fn is_supported_extension(&self, canonical_file_name: &str) -> bool {
        self.supported_extensions_with_json
            .iter()
            .any(|group| file_extension_is_one_of(canonical_file_name, group))
    }

    fn add_sub_task(&mut self, parent: usize, reference: &ResolvedRef) {
        let index = self.new_task(normalize_path(&reference.file_name));
        self.tasks[index].depth = reference.depth;
        self.tasks[parent].sub_tasks.push(index);
    }

    // ---- Module format -----------------------------------------------------

    /// `fileLoader.loadSourceFileMetaData`.
    fn load_source_file_meta_data(&self, file_name: &str) -> SourceFileMetaData {
        let scope = self.resolver.get_package_scope_for_path(get_directory_path(file_name));
        let module_resolution = self.options.module_resolution_kind();

        let mut package_json_type = String::new();
        if let Some(scope) = &scope
            && let Some(contents) = &scope.contents
            && let Some(value) = &contents.package_type.value
        {
            // Upstream's precedence, preserved literally: `(!A && B) || C`. A
            // `.mts` under a `"type": "module"` package takes its format from
            // the extension, *unless* it is inside `node_modules`, where the
            // package's declared type wins for compatibility.
            let extension_decides = file_extension_is_one_of(
                file_name,
                &[EXTENSION_MTS, EXTENSION_CTS, EXTENSION_MJS, EXTENSION_CJS],
            );
            let node_semantics = ModuleResolutionKind::Node16 <= module_resolution
                && module_resolution <= ModuleResolutionKind::NodeNext;
            if (!extension_decides && node_semantics) || file_name.contains("/node_modules/") {
                package_json_type.clone_from(value);
            }
        }

        SourceFileMetaData {
            implied_node_format: implied_node_format_for_file(file_name, &package_json_type),
            package_json_type,
        }
    }

    /// `getDefaultResolutionModeForFile`.
    fn default_resolution_mode_for_file(
        &self,
        file_name: &str,
        metadata: &SourceFileMetaData,
    ) -> ResolutionMode {
        if self.import_syntax_affects_module_resolution() {
            self.implied_node_format_for_emit(file_name, metadata)
        } else {
            ResolutionMode::None
        }
    }

    /// `getModeForUsageLocation`.
    fn mode_for_usage_location(
        &self,
        file_name: &str,
        metadata: &SourceFileMetaData,
        specifier: &tsr_parser::ModuleSpecifier,
    ) -> ResolutionMode {
        // An explicit `with { "resolution-mode": … }` wins outright. Upstream
        // reads it only for a type-only import or an `import` type, which is
        // the same set the parser records an override for.
        match specifier.resolution_mode_override {
            tsr_parser::ResolutionMode::CommonJS => return ResolutionMode::CommonJS,
            tsr_parser::ResolutionMode::ESNext => return ResolutionMode::ESNext,
            tsr_parser::ResolutionMode::None => {}
        }
        if !self.import_syntax_affects_module_resolution() {
            return ResolutionMode::None;
        }

        // `getEmitSyntaxForUsageLocationWorker`.
        if matches!(
            specifier.context,
            SpecifierContext::RequireCall | SpecifierContext::ImportEquals
        ) {
            return ModuleKind::CommonJS;
        }
        let file_emit_mode = self.emit_module_format_of_file(file_name, metadata);
        if specifier.context == SpecifierContext::ImportCall {
            return if should_transform_import_call(&self.options, file_emit_mode) {
                ModuleKind::CommonJS
            } else {
                ModuleKind::ESNext
            };
        }
        if file_emit_mode == ModuleKind::CommonJS {
            return ModuleKind::CommonJS;
        }
        if is_non_node_esm(file_emit_mode) || file_emit_mode == ModuleKind::Preserve {
            return ModuleKind::ESNext;
        }
        ModuleKind::None
    }

    /// `importSyntaxAffectsModuleResolution`.
    fn import_syntax_affects_module_resolution(&self) -> bool {
        let kind = self.options.module_resolution_kind();
        (ModuleResolutionKind::Node16 <= kind && kind <= ModuleResolutionKind::NodeNext)
            || self.options.get_resolve_package_json_exports()
            || self.options.get_resolve_package_json_imports()
    }

    /// `ast.GetImpliedNodeFormatForEmitWorker`.
    fn implied_node_format_for_emit(
        &self,
        file_name: &str,
        metadata: &SourceFileMetaData,
    ) -> ResolutionMode {
        let emit_module_kind = self.options.emit_module_kind();
        if ModuleKind::Node16 <= emit_module_kind && emit_module_kind <= ModuleKind::NodeNext {
            return metadata.implied_node_format;
        }
        if metadata.implied_node_format == ModuleKind::CommonJS
            && (metadata.package_json_type == "commonjs"
                || file_extension_is_one_of(file_name, &[EXTENSION_CJS, EXTENSION_CTS]))
        {
            return ModuleKind::CommonJS;
        }
        if metadata.implied_node_format == ModuleKind::ESNext
            && (metadata.package_json_type == "module"
                || file_extension_is_one_of(file_name, &[EXTENSION_MJS, EXTENSION_MTS]))
        {
            return ModuleKind::ESNext;
        }
        ModuleKind::None
    }

    /// `ast.GetEmitModuleFormatOfFileWorker`.
    fn emit_module_format_of_file(
        &self,
        file_name: &str,
        metadata: &SourceFileMetaData,
    ) -> ModuleKind {
        let result = self.implied_node_format_for_emit(file_name, metadata);
        if result == ModuleKind::None { self.options.emit_module_kind() } else { result }
    }

    /// `ast.IsExternalModule`, for a file this loader parsed.
    ///
    /// `moduleDetection` is assumed to be `auto`; see the module docs.
    fn is_external_module(
        &self,
        source_file: &tsr_ast::SourceFile<'_>,
        file_name: &str,
        metadata: &SourceFileMetaData,
    ) -> bool {
        if tsr_parser::is_file_probably_external_module(source_file) {
            return true;
        }
        if is_declaration_file_name(file_name) {
            return false;
        }
        if matches!(self.options.jsx, JsxEmit::ReactJsx | JsxEmit::ReactJsxDev)
            && tsr_parser::contains_jsx_tag(source_file)
        {
            return true;
        }
        // `isFileForcedToBeModuleByFormat`.
        self.implied_node_format_for_emit(file_name, metadata) == ModuleKind::ESNext
            || file_extension_is_one_of(
                file_name,
                &[EXTENSION_CJS, EXTENSION_CTS, EXTENSION_MJS, EXTENSION_MTS],
            )
    }

    /// `ast.GetJSXRuntimeImport(ast.GetJSXImplicitImportBase(…))`.
    ///
    /// The `@jsxImportSource` and `@jsxRuntime` *pragmas* are not read: the
    /// preamble scanner does not extract them yet, so a file overriding the
    /// runtime per-file resolves the option's value instead.
    fn jsx_runtime_import(&self) -> Option<String> {
        let automatic = matches!(self.options.jsx, JsxEmit::ReactJsx | JsxEmit::ReactJsxDev)
            || !self.options.jsx_import_source.is_empty();
        if !automatic {
            return None;
        }
        let base = if self.options.jsx_import_source.is_empty() {
            "react"
        } else {
            &self.options.jsx_import_source
        };
        let runtime = if self.options.jsx == JsxEmit::ReactJsxDev {
            "jsx-dev-runtime"
        } else {
            "jsx-runtime"
        };
        Some(format!("{base}/{runtime}"))
    }

    // ---- The replay --------------------------------------------------------

    /// `filesParser.getProcessedFiles`' `collectFiles`, reduced to the trace
    /// replay and the file list.
    ///
    /// The order here is the whole point: depth-first over the task tree, each
    /// path once, and within a file the *type* resolutions before the module
    /// resolutions — because that is the order `collectFiles` writes them to the
    /// host, and therefore the order the baselines are in.
    fn collect_files(&self) -> LoadedFiles {
        let mut result = LoadedFiles::default();
        let mut seen: FxHashSet<Path> = FxHashSet::default();
        for root in &self.root_tasks {
            self.collect_task(*root, &mut seen, &mut result);
        }
        result
    }

    fn collect_task(&self, index: usize, seen: &mut FxHashSet<Path>, result: &mut LoadedFiles) {
        let task = &self.tasks[index];
        if !task.loaded || !seen.insert(task.path.clone()) {
            return;
        }
        result.requests.extend(task.type_resolution_requests.iter().cloned());
        result.traces.extend(task.type_resolutions_trace.iter().cloned());
        result.requests.extend(task.resolution_requests.iter().cloned());
        result.traces.extend(task.resolutions_trace.iter().cloned());

        for sub_task in &task.sub_tasks {
            self.collect_task(*sub_task, seen, result);
        }

        if !task.is_for_automatic_type_directive {
            result.file_names.push(task.file_name.clone());
        }
    }
}

/// `ast.GetImpliedNodeFormatForFile`.
fn implied_node_format_for_file(path: &str, package_json_type: &str) -> ResolutionMode {
    if file_extension_is_one_of(path, &[EXTENSION_DMTS, EXTENSION_MTS, EXTENSION_MJS]) {
        ResolutionMode::ESNext
    } else if file_extension_is_one_of(path, &[EXTENSION_DCTS, EXTENSION_CTS, EXTENSION_CJS]) {
        ResolutionMode::CommonJS
    } else if file_extension_is_one_of(
        path,
        &[EXTENSION_DTS, EXTENSION_TS, EXTENSION_TSX, EXTENSION_JS, EXTENSION_JSX],
    ) {
        if package_json_type == "module" {
            ResolutionMode::ESNext
        } else {
            ResolutionMode::CommonJS
        }
    } else {
        ResolutionMode::None
    }
}

/// `ast.ShouldTransformImportCall`.
fn should_transform_import_call(
    options: &CompilerOptions,
    implied_node_format_for_emit: ModuleKind,
) -> bool {
    let module_kind = options.emit_module_kind();
    if (ModuleKind::Node16 <= module_kind && module_kind <= ModuleKind::NodeNext)
        || module_kind == ModuleKind::Preserve
    {
        return false;
    }
    implied_node_format_for_emit < ModuleKind::ES2015
}

/// `core.ModuleKind.IsNonNodeESM`.
fn is_non_node_esm(kind: ModuleKind) -> bool {
    ModuleKind::ES2015 <= kind && kind <= ModuleKind::ESNext
}

/// Whether the file name has an extension at all (`tspath.HasExtension`).
fn has_extension(file_name: &str) -> bool {
    get_base_file_name(file_name).contains('.')
}

/// Whether the file is JavaScript (`ast.IsSourceFileJS`, by extension).
fn is_javascript_file(file_name: &str) -> bool {
    file_extension_is_one_of(
        file_name,
        &[EXTENSION_JS, EXTENSION_JSX, EXTENSION_MJS, EXTENSION_CJS],
    )
}

#[cfg(test)]
mod tests {
    use tsr_core::{ModuleKind, ModuleResolutionKind, Tristate};
    use tsr_vfs::{FileSystem, InMemoryFileSystem};

    use super::*;

    struct TestHost {
        fs: InMemoryFileSystem,
    }

    impl ResolutionHost for TestHost {
        fn fs(&self) -> &dyn FileSystem {
            &self.fs
        }

        fn current_directory(&self) -> &'static str {
            "/"
        }
    }

    fn load(files: &[(&str, &str)], roots: &[&str], options: CompilerOptions) -> LoadedFiles {
        let host = TestHost {
            fs: InMemoryFileSystem::new(
                files.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
                [],
                true,
            ),
        };
        let roots: Vec<String> = roots.iter().map(|r| (*r).to_string()).collect();
        FileLoader::load(&host, options, &roots)
    }

    fn traced() -> CompilerOptions {
        CompilerOptions { trace_resolution: Tristate::True, ..CompilerOptions::default() }
    }

    fn names(loaded: &LoadedFiles) -> Vec<(&str, &str)> {
        loaded.requests.iter().map(|r| (r.name.as_str(), r.containing_file.as_str())).collect()
    }

    #[test]
    fn the_walk_is_depth_first_and_reaches_each_file_once() {
        // `a` imports `b` and `c`; `b` imports `c` too. Depth-first means `c`
        // is requested from `b` before it is requested from `a`, and the file
        // is loaded once.
        let loaded = load(
            &[
                ("/a.ts", "import \"./b\";\nimport \"./c\";\n"),
                ("/b.ts", "import \"./c\";\n"),
                ("/c.ts", "export {};\n"),
            ],
            &["/a.ts"],
            traced(),
        );
        assert_eq!(
            names(&loaded),
            [("./b", "/a.ts"), ("./c", "/a.ts"), ("./c", "/b.ts")],
            "both of a.ts's imports are resolved before b.ts is walked into"
        );
        assert_eq!(loaded.file_names, ["/c.ts", "/b.ts", "/a.ts"]);
    }

    #[test]
    fn type_resolutions_are_replayed_before_module_resolutions_of_the_same_file() {
        // The one ordering rule that is not simply "source order":
        // `collectFiles` writes `typeResolutionsTrace` then `resolutionsTrace`,
        // so every `/// <reference types>` in a file precedes every `import` in
        // it — which is also, incidentally, the source order here, since a
        // directive is only a pragma while it is still in the preamble.
        let loaded = load(
            &[
                ("/a.ts", "/// <reference types=\"pkg\" />\nimport \"./b\";\n"),
                ("/b.ts", "export {};\n"),
            ],
            &["/a.ts"],
            traced(),
        );
        assert_eq!(
            loaded.requests.iter().map(|r| (r.kind, r.name.as_str())).collect::<Vec<_>>(),
            [(RequestKind::TypeReferenceDirective, "pkg"), (RequestKind::Module, "./b"),]
        );
    }

    #[test]
    fn the_automatic_type_directive_task_runs_last() {
        // It is appended after every root file, which is why `types` entries
        // appear at the *end* of a baseline however early the option is
        // declared.
        let loaded = load(
            &[("/a.ts", "import \"./b\";\n"), ("/b.ts", "export {};\n")],
            &["/a.ts"],
            CompilerOptions { types: Some(vec!["pkg".to_string()]), ..traced() },
        );
        assert_eq!(names(&loaded), [("./b", "/a.ts"), ("pkg", "/__inferred type names__.ts")]);
    }

    #[test]
    fn a_module_augmentation_is_resolved_but_adds_no_file() {
        let loaded = load(
            &[("/a.ts", "export {};\ndeclare module \"./b\" { }\n"), ("/b.ts", "export {};\n")],
            &["/a.ts"],
            traced(),
        );
        assert_eq!(names(&loaded), [("./b", "/a.ts")]);
        assert_eq!(loaded.file_names, ["/a.ts"], "the augmented module is not pulled in");
    }

    #[test]
    fn a_triple_slash_path_reference_adds_a_file_without_a_resolution() {
        // The one reference kind that never reaches the resolver: it names a
        // file, so it is a file-system probe and produces no trace.
        let loaded = load(
            &[
                ("/a.ts", "/// <reference path=\"./b.ts\" />\n"),
                ("/b.ts", "import \"./c\";\n"),
                ("/c.ts", "export {};\n"),
            ],
            &["/a.ts"],
            traced(),
        );
        assert_eq!(names(&loaded), [("./c", "/b.ts")]);
        assert_eq!(loaded.file_names, ["/c.ts", "/b.ts", "/a.ts"]);
    }

    #[test]
    fn no_resolve_still_resolves_but_stops_adding_files() {
        // `noResolve` is checked in `shouldAddFile`, *after* the resolution has
        // run and traced. A loader that skipped the resolution entirely would
        // produce a shorter trace than upstream.
        let loaded = load(
            &[
                ("/a.ts", "import \"./b\";\n"),
                ("/b.ts", "import \"./c\";\n"),
                ("/c.ts", "export {};\n"),
            ],
            &["/a.ts"],
            CompilerOptions { no_resolve: Tristate::True, ..traced() },
        );
        assert_eq!(names(&loaded), [("./b", "/a.ts")]);
        assert_eq!(loaded.file_names, ["/a.ts"]);
    }

    #[test]
    fn the_module_format_comes_from_the_extension_and_the_package_json_type() {
        // Under node16 the mode is the file's format, and the format of a plain
        // `.ts` is decided by the nearest `package.json`.
        let options = CompilerOptions {
            module: ModuleKind::Node16,
            module_resolution: ModuleResolutionKind::Node16,
            ..traced()
        };
        let esm = load(
            &[
                ("/package.json", "{ \"type\": \"module\" }"),
                ("/a.ts", "import \"./b.js\";\n"),
                ("/b.ts", "export {};\n"),
            ],
            &["/a.ts"],
            options.clone(),
        );
        assert_eq!(esm.requests[0].mode, ModuleKind::ESNext);

        let cjs = load(
            &[
                ("/package.json", "{ \"type\": \"commonjs\" }"),
                ("/a.ts", "import \"./b.js\";\n"),
                ("/b.ts", "export {};\n"),
            ],
            &["/a.ts"],
            options,
        );
        assert_eq!(cjs.requests[0].mode, ModuleKind::CommonJS);
    }

    #[test]
    fn an_import_equals_resolves_as_commonjs_whatever_the_file_format_is() {
        // `getEmitSyntaxForUsageLocationWorker`: `require`-shaped syntax is
        // CommonJS even in an ESM file.
        let loaded = load(
            &[
                ("/package.json", "{ \"type\": \"module\" }"),
                ("/a.ts", "import b = require(\"./b\");\n"),
                ("/b.ts", "export {};\n"),
            ],
            &["/a.ts"],
            CompilerOptions {
                module: ModuleKind::Node16,
                module_resolution: ModuleResolutionKind::Node16,
                ..traced()
            },
        );
        assert_eq!(loaded.requests[0].mode, ModuleKind::CommonJS);
    }

    #[test]
    fn a_root_file_without_an_extension_is_resolved_against_the_first_extension_group() {
        let loaded = load(&[("/a.ts", "export {};\n")], &["/a"], traced());
        assert_eq!(loaded.file_names, ["/a.ts"]);
    }
}
