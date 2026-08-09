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
//! - **Lib files resolve nothing.** A default-lib or `/// <reference lib="…" />`
//!   task is now loaded (see [`FileLoader::add_lib_file_tasks`]), but it
//!   contributes no trace: `parseTask.load` gives a lib file a fixed `CommonJS`
//!   metadata precisely to avoid the `package.json` lookup, and that skip is
//!   reproduced here. The exception is `libReplacement`, which resolves
//!   `@typescript/lib-*` through the module resolver and *does* trace; that is
//!   still not done (bd tsr-9or.5).
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
        EXTENSION_CJS, EXTENSION_CTS, EXTENSION_DCTS, EXTENSION_DMTS, EXTENSION_DTS, EXTENSION_JS,
        EXTENSION_JSON, EXTENSION_JSX, EXTENSION_MJS, EXTENSION_MTS, EXTENSION_TS, EXTENSION_TSX,
        SUPPORTED_TS_EXTENSIONS_WITH_JSON_FLAT, file_extension_is, file_extension_is_one_of,
    },
    get_canonical_file_name, get_directory_path, get_normalized_absolute_path, has_extension,
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

/// One resolution the loader asked for, **and what it answered**.
///
/// This is the unit the loader is judged on: `kind`, `name`, `containing_file`
/// and `mode` are exactly what a `======== Resolving … ========` header records,
/// and `(containing_file, name, mode)` is also exactly upstream's cache key —
/// `module.ModeAwareCacheKey{Name, Mode}` under `p.resolvedModules[file.Path()]`
/// (`internal/compiler/program.go:521`-`:527`).
///
/// # Why the answer is kept
///
/// It was computed and dropped on the line that computed it. The loader resolves
/// a specifier, uses the resolved file name to open a subtask, and lets it go —
/// so the program that comes out of the load knows *which files* it holds and
/// not *which import reached which file*. That edge is the whole of what a
/// checker needs to type a cross-file `import`, and rebuilding it later means
/// running the resolver a second time against a file system the program no
/// longer holds. See
/// [ADR-0041](../../../docs/adr/0041-the-checker-asks-its-program-for-a-module.md).
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
    /// The file name the resolver answered, when it resolved one.
    ///
    /// `None` is upstream's `!resolvedModule.IsResolved()`. Upstream stores an
    /// unresolved `*module.ResolvedModule` rather than nothing, because it
    /// reports `GetResolutionDiagnostic` off it; nothing here reads a
    /// diagnostic yet, so the unresolved case is the absence and a consumer
    /// cannot tell "never asked" from "asked and failed". Both must answer
    /// `errorType`, which is why the collapse is safe today and would stop
    /// being safe the moment TS2307 is ported.
    ///
    /// **Not filtered by `should_add_file`.** A resolution that the program
    /// declines to *add* as a file — a `.js` under `allowJs: false`, say — is
    /// still a resolution, and upstream's `resolvedModules` records it for the
    /// same reason. Whether the target is in the program is the *reader's*
    /// question, answered by looking the path up.
    pub resolved: Option<String>,
}

/// What a load is asked for.
///
/// A struct rather than three positional arguments because
/// `default_library_path` is the third thing that is neither an option nor a
/// root file, and because upstream keeps it somewhere this port cannot: it is
/// `CompilerHost.DefaultLibraryPath()` (`internal/compiler/host.go:16`), a
/// method on the *compiler* host, which is a different interface from the
/// *resolution* host [`FileLoader`] already takes. Adding it to
/// [`tsr_module::types::ResolutionHost`] would merge two identities upstream
/// keeps apart, and introducing a `CompilerHost: ResolutionHost` supertrait
/// would need trait upcasting to reach the resolver — stable since Rust 1.86,
/// and this workspace pins `rust-version = "1.85"`. So the path travels beside
/// the host instead. If the MSRV moves, the supertrait is the better shape.
#[derive(Debug, Clone, Default)]
pub struct LoadOptions {
    /// The options every file is loaded under.
    pub compiler_options: CompilerOptions,
    /// The files the walk starts from (`ParsedCommandLine.FileNames`).
    pub root_file_names: Vec<String>,
    /// The directory holding the bundled `lib.*.d.ts`
    /// (`CompilerHost.DefaultLibraryPath`).
    ///
    /// Empty means "there is none": lib tasks are still created, and simply find
    /// no file. That is deliberately not the same as `noLib`, which creates no
    /// task at all — a host with no libs and a program that asked for none are
    /// different situations and only one of them is a missing-file diagnostic
    /// upstream.
    pub default_library_path: String,
}

/// Everything one load produced (`compiler.processedFiles`, reduced to what
/// exists).
///
/// Carries the program's **shared** node table and node map as well as the
/// files, because under program-wide identity
/// ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md))
/// the loader is what fills them: it parses every file it reaches to find that
/// file's imports, so parsing again into fresh tables would be both a second
/// pass over 3.9 MB of lib text and a second, incompatible set of ids.
/// One diagnostic the **loader** produced, positioned in the file that caused it.
///
/// Upstream's loader reports several (`fileloader.go:395`-`:425`): a reference
/// to a file that does not exist, one with an unsupported extension, one to a
/// JavaScript file without `allowJs`, and a file referencing itself. This port
/// dropped all of them for want of anywhere to put them — see §240, which built
/// the channel and sent the first through it.
#[derive(Debug, Clone)]
pub struct LoaderDiagnostic {
    /// The file containing the reference, as the loader names it.
    pub file_name: String,
    /// The quoted value's span, from [`tsr_parser::pragma::FileReference`].
    pub span: tsr_core::Span,
    /// The message, already anchored upstream.
    pub message: &'static tsr_diagnostics::Message,
    /// Substitution arguments, in order.
    pub args: Vec<String>,
}

/// Everything one walk of the file graph produced.
#[derive(Debug, Default)]
pub struct LoadedFiles<'a> {
    /// Every file the walk reached, **lib files first**.
    ///
    /// Upstream's `allFiles := append(libFiles, files...)`
    /// (`filesparser.go:518`), with `libFiles` sorted by
    /// `getDefaultLibFilePriority` first. Everything after the libs is in the
    /// order the replay walk reached it.
    pub file_names: Vec<String>,
    /// How many of the leading entries of [`LoadedFiles::file_names`] are lib
    /// files. The boundary upstream keeps as two separate slices.
    pub lib_file_count: usize,
    /// The parsed files, positionally matching [`LoadedFiles::file_names`].
    ///
    /// The loader parses every file it reaches anyway (`parseTask.load` sets
    /// `t.file`), so handing them out costs nothing and is what stops a
    /// [`crate::Program`] built from a load re-reading and re-parsing 3.9 MB of
    /// lib files.
    ///
    /// A file the host could not read appears in neither this nor
    /// [`LoadedFiles::file_names`]: it is upstream's `missingFiles`, which is a
    /// diagnostic rather than a member of the program.
    pub files: Vec<ProgramFile<'a>>,
    /// What the walk could not resolve. See [`LoaderDiagnostic`].
    pub loader_diagnostics: Vec<LoaderDiagnostic>,
    /// Kind, span and parent for every node of **every** file, in load order.
    pub nodes: tsr_ast::NodeTable,
    /// The typed node behind each id, over the same shared numbering.
    pub node_map: tsr_ast::NodeMap<'a>,
    /// Every resolution requested **and what it answered**, in replay order.
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
struct ParseTask<'a> {
    file_name: String,
    path: Path,
    /// Whether this is the synthetic task that resolves `types`/`@types`.
    is_for_automatic_type_directive: bool,
    /// The bundled lib file this task loads, if it loads one
    /// (`parseTask.libFile`).
    ///
    /// Its *base name*, not its path: it is what
    /// `tsoptions::libs::lib_option_index` sorts on, and upstream recovers the
    /// same string from the path by stripping the default library directory
    /// (`fileloader.go:326`). Keeping it is the same information without the
    /// prefix arithmetic.
    lib_file: Option<&'static str>,
    depth: Depth,
    /// Whether [`FileLoader::load_task`] ran for this task. A task for a path
    /// some earlier task already claimed stays unloaded and is skipped by the
    /// replay walk, along with its subtree.
    loaded: bool,
    /// The parsed file (`parseTask.file`). `None` when the host could not read
    /// it, which is upstream's `missingFiles`.
    file: Option<ProgramFile<'a>>,
    sub_tasks: Vec<usize>,
    metadata: SourceFileMetaData,
    type_resolutions_trace: Vec<Trace>,
    type_resolution_requests: Vec<ResolutionRequest>,
    resolutions_trace: Vec<Trace>,
    resolution_requests: Vec<ResolutionRequest>,
}

impl ParseTask<'_> {
    fn new(file_name: String, path: Path) -> Self {
        Self {
            file_name,
            path,
            is_for_automatic_type_directive: false,
            lib_file: None,
            depth: Depth::default(),
            loaded: false,
            file: None,
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
///
/// Two lifetimes, deliberately: `'host` is the host and its file system, `'a`
/// is the **arena the program is being built in**, which every parsed node,
/// every file name and every file text borrows from. They are not the same
/// extent — the conformance harness holds one host for a whole corpus run and
/// one arena per case — and unifying them would make the shorter one infect the
/// longer.
pub struct FileLoader<'host, 'a> {
    host: &'host dyn ResolutionHost,
    /// Where every file's tree, name and text is allocated. See ADR-0034,
    /// "Who owns the arena": the caller owns it, and both this and
    /// [`crate::Program`] borrow it.
    arena: &'a tsr_core::Arena,
    /// The program's shared node table, filled in load order.
    nodes: tsr_ast::NodeTable,
    /// The program's shared node map, over the same numbering.
    node_map: tsr_ast::NodeMap<'a>,
    options: CompilerOptions,
    resolver: Resolver<'host>,
    tasks: Vec<ParseTask<'a>>,
    root_tasks: Vec<usize>,
    /// See [`LoaderDiagnostic`].
    loader_diagnostics: Vec<LoaderDiagnostic>,
    /// The task that claimed each path. Upstream's `taskDataByPath`, minus the
    /// per-casing map: see [`FileLoader::process_task`].
    claimed: FxHashMap<Path, usize>,
    supported_extensions: &'static [&'static [&'static str]],
    supported_extensions_with_json: &'static [&'static [&'static str]],
    max_node_module_js_depth: i32,
    /// Where the bundled `lib.*.d.ts` live, normalised and absolute
    /// (`fileLoader.defaultLibraryPath`).
    default_library_path: String,
}

impl<'host, 'a> FileLoader<'host, 'a> {
    /// Walk the program from `root_file_names` (`processAllProgramFiles`),
    /// parsing every file it reaches into `arena`.
    ///
    /// Root files are taken as given, which is what upstream does too — they
    /// come from `ParsedCommandLine.FileNames`, and turning a `tsconfig.json`
    /// into that list is `tsr-tsoptions`' job (bd tsr-9or slice 3).
    #[must_use]
    pub fn load(
        arena: &'a tsr_core::Arena,
        host: &'host dyn ResolutionHost,
        load_options: LoadOptions,
    ) -> LoadedFiles<'a> {
        let LoadOptions { compiler_options: options, root_file_names, default_library_path } =
            load_options;
        // From `tsr-tsoptions`, as upstream's `fileloader.go` takes them from
        // `internal/tsoptions`: the same two lists decide which files a wildcard
        // `include` expands to and which extensions a reference may name.
        let supported_extensions = tsr_tsoptions::file_names::supported_extensions(&options);
        let supported_extensions_with_json =
            tsr_tsoptions::file_names::supported_extensions_with_json(&options);

        let mut loader = Self {
            loader_diagnostics: Vec::new(),
            resolver: Resolver::new(host, options.clone()),
            host,
            arena,
            nodes: tsr_ast::NodeTable::new(),
            node_map: tsr_ast::NodeMap::new(),
            max_node_module_js_depth: options.max_node_module_js_depth.unwrap_or(0),
            options,
            // As upstream normalises it once on construction (`fileloader.go:136`),
            // so every `pathForLibFile` is a plain join.
            default_library_path: get_normalized_absolute_path(
                &default_library_path,
                host.current_directory(),
            ),
            tasks: Vec::new(),
            root_tasks: Vec::new(),
            claimed: FxHashMap::default(),
            supported_extensions,
            supported_extensions_with_json,
        };

        for root in &root_file_names {
            loader.add_root_file_task(root);
        }
        // `fileloader.go:157`. Both this and the automatic type directives are
        // guarded on there being root files at all: a program with no roots gets
        // no libs, which is why an empty `files`/`include` is an empty program
        // rather than a program of nothing but `lib.d.ts`.
        if !root_file_names.is_empty() {
            loader.add_lib_file_tasks();
            loader.add_automatic_type_directive_task();
        }

        let roots = loader.root_tasks.clone();
        for root in roots {
            loader.process_task(root, 0);
        }
        let (mut result, order) = loader.collect_files();
        result.files = order
            .into_iter()
            .map(|index| loader.tasks[index].file.take().expect("only read files are collected"))
            .collect();
        // The shared tables travel with the files, because a `NodeId` is only
        // meaningful beside them. Moved rather than rebuilt: they are the ones
        // the files were parsed into.
        result.nodes = loader.nodes;
        result.node_map = loader.node_map;
        result
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

    /// The default libs, or the ones `--lib` named (`fileloader.go:157-171`).
    ///
    /// Added **after** every root file and **before** the automatic type
    /// directive task, which is upstream's order and therefore the order they
    /// appear in the replay walk. It does not decide the order they appear in
    /// the program: [`FileLoader::collect_files`] sorts the libs afterwards, as
    /// `sortLibs` does.
    fn add_lib_file_tasks(&mut self) {
        for name in tsr_tsoptions::libs::lib_file_names(&self.options) {
            let index = self.push_root(self.path_for_lib_file(name));
            self.tasks[index].lib_file = Some(name);
        }
    }

    /// Where a bundled lib file lives (`fileLoader.pathForLibFile`).
    ///
    /// The `libReplacement` half — resolving `@typescript/lib-dom` through the
    /// module resolver, which *does* trace — is not ported (bd tsr-9or.5). It is
    /// off by default and no `.trace.json` baseline exercises it.
    /// `pathForLibFile` (`fileloader.go:645`).
    ///
    /// Under `libReplacement` a lib file's name is resolved as an
    /// `@typescript/lib-*` **module** and the package's file is loaded instead
    /// of the bundled one — which is how `libTypeScriptOverrideSimple` expects
    /// `window` to be undefined after `/// <reference lib="dom" />`.
    ///
    /// An unresolved package falls back to the bundled path, so a program
    /// without the `@typescript/*` packages is unaffected.
    ///
    /// The **trace replay** is not built: upstream stores each resolution and
    /// emits its traces at the end of the run, sorted by path key
    /// (`bd tsr-9or.5`). `docs/architecture/checker-notes-diag2.md` §531.
    fn path_for_lib_file(&self, name: &str) -> String {
        let bundled = combine_paths(&self.default_library_path, &[name]);
        if !self.options.lib_replacement.is_true() || name == "lib.d.ts" {
            return bundled;
        }
        let library_name = library_name_from_lib_file_name(name);
        let containing_directory = if self.options.config_file_path.is_empty() {
            self.host.current_directory().to_string()
        } else {
            get_directory_path(&self.options.config_file_path).to_string()
        };
        let resolve_from = combine_paths(
            &containing_directory,
            &[&format!("__lib_node_modules_lookup_{name}__.ts")],
        );
        let (resolution, _traces) = self.resolver.resolve_module_name(
            &library_name,
            &resolve_from,
            tsr_core::ResolutionMode::CommonJS,
        );
        if resolution.resolved_file_name.is_empty() {
            bundled
        } else {
            resolution.resolved_file_name
        }
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

        // A lib file's format is fixed rather than looked up. Upstream's reason
        // is watcher noise — "we can safely skip looking up their package.json
        // to avoid adding spurious lookups" — and the consequence here is
        // sharper: `load_source_file_meta_data` warms the resolver's
        // `package.json` cache, which later resolutions observe, so doing it for
        // a lib file would change the trace of a program that has one.
        self.tasks[index].metadata = if self.tasks[index].lib_file.is_some() {
            SourceFileMetaData {
                package_json_type: String::new(),
                implied_node_format: ResolutionMode::CommonJS,
            }
        } else {
            self.load_source_file_meta_data(&file_name)
        };

        let Some(text) = self.host.fs().read_file(&file_name) else { return };

        // The host's `String` is copied into the arena and then dropped. That
        // copy is what lets `ProgramFile` own nothing: a `Symbol`'s name and the
        // binder's `FileInfo` borrow the source text, so under one program-wide
        // `SymbolStore` the text has to outlive every file — which the arena
        // does and a per-file `String` does not. See ADR-0034, "Who owns the
        // arena". The cost is one extra copy of text already in memory.
        let arena = self.arena;
        let text: &'a str = arena.alloc_str(&text);
        let name: &'a str = arena.alloc_str(&file_name);

        let inferred_script_kind = tsr_parser::ScriptKind::from_file_name(name);
        let lowered = name.to_ascii_lowercase();
        let script_kind = if inferred_script_kind == tsr_parser::ScriptKind::TypeScript
            && self.options.jsx != tsr_core::JsxEmit::None
            && [".js", ".cjs", ".mjs"].iter().any(|ext| lowered.ends_with(ext))
        {
            tsr_parser::ScriptKind::Tsx
        } else {
            inferred_script_kind
        };
        let options = tsr_parser::ParseOptions { script_kind, ..Default::default() };
        // Into the program's shared tables, not fresh ones: this is the parser
        // half of the identity widening, and parsing into fresh tables here
        // would give two files the same `NodeId`s.
        let parsed =
            tsr_parser::parse_into(arena, text, options, &mut self.nodes, &mut self.node_map);
        // Upstream's parser stamps `NodeFlagsJavaScriptFile` from its
        // `ScriptKind`; this parser never sees the file name (ADR-0016), so
        // the loader — which holds both the name and the table — stamps the
        // root. The second of the two parse sites; `Program::new` is the
        // other, and `checker-notes-assign.md` §11.1 is the consumer that
        // found the flag declared and set by nothing.
        if [".js", ".jsx", ".cjs", ".mjs"].iter().any(|ext| lowered.ends_with(ext))
            && let Some(root) = tsr_ast::Node::SourceFile(parsed.source_file).node_id()
        {
            self.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
        }
        let file = ProgramFile::new(self.tasks[index].path.clone(), name, text, parsed);

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
                } else {
                    // `File_0_not_found` (`fileloader.go:407`). The argument is
                    // the reference text with slashes normalised
                    // (`diagnosticFileName`), **not** the resolved candidate —
                    // upstream reports what the author wrote.
                    self.loader_diagnostics.push(LoaderDiagnostic {
                        file_name: file_name.clone(),
                        span: reference.span,
                        message: &tsr_diagnostics::messages::FILE_0_NOT_FOUND,
                        args: vec![tsr_path::normalize_slashes(&reference.file_name)],
                    });
                }
            }
            self.resolve_type_reference_directives(index, &file);
        }

        // `/// <reference lib="…" />` (`filesparser.go:133-155`). Between the
        // path/type references above and the imports below, which is upstream's
        // position and so the order these subtasks are walked in.
        //
        // It resolves nothing — the name is a table lookup, not a module
        // resolution — so it adds no request and no trace. A name the table does
        // not know is a diagnostic upstream and is dropped here, as every other
        // loader diagnostic is.
        if !self.options.no_lib.is_true() {
            let libs = file.file_references().lib_reference_directives.clone();
            for lib in &libs {
                if let Some(name) = tsr_tsoptions::libs::get_lib_file_name(&lib.file_name) {
                    let path = self.path_for_lib_file(name);
                    let sub = self.add_sub_task(
                        index,
                        &ResolvedRef { file_name: path, depth: Depth::default() },
                    );
                    self.tasks[sub].lib_file = Some(name);
                }
            }
        }

        // Collected here rather than inside the resolving walk below, because the
        // node table is now the *loader's* and reading it needs a borrow that
        // cannot coexist with the `&mut self` the walk takes. The order of the
        // two is unchanged.
        let metadata = self.tasks[index].metadata.clone();
        let is_external_module = self.is_external_module(file.source_file(), &file_name, &metadata);
        let references = tsr_parser::collect_external_module_references(
            file.source_file(),
            &self.nodes,
            CollectOptions {
                is_declaration_file: is_declaration_file_name(&file_name),
                is_js_file: is_javascript_file(&file_name),
                is_external_module,
            },
        );
        self.resolve_imports_and_module_augmentations(index, references);
        self.tasks[index].file = Some(file);
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
                resolved: resolved.is_resolved().then(|| resolved.resolved_file_name.clone()),
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
    fn resolve_type_reference_directives(&mut self, index: usize, file: &ProgramFile<'a>) {
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
                resolved: resolved.is_resolved().then(|| resolved.resolved_file_name.clone()),
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
    fn resolve_imports_and_module_augmentations(
        &mut self,
        index: usize,
        references: tsr_parser::ExternalModuleReferences,
    ) {
        let file_name = self.tasks[index].file_name.clone();
        let metadata = self.tasks[index].metadata.clone();
        let is_js_file = is_javascript_file(&file_name);
        let is_declaration_file = is_declaration_file_name(&file_name);

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
                resolved: resolved.is_resolved().then(|| resolved.resolved_file_name.clone()),
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

    fn add_sub_task(&mut self, parent: usize, reference: &ResolvedRef) -> usize {
        let index = self.new_task(normalize_path(&reference.file_name));
        self.tasks[index].depth = reference.depth;
        self.tasks[parent].sub_tasks.push(index);
        index
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
    /// The replay walk, and the file list it produces.
    ///
    /// Returns the task index behind each entry of
    /// [`LoadedFiles::file_names`], so the caller can move each task's parsed
    /// file out in the same order. Two values rather than one because the walk
    /// takes `&self` — the recursion visits a task's subtasks while borrowing
    /// it — and moving a `ProgramFile` out needs `&mut`.
    fn collect_files(&self) -> (LoadedFiles<'a>, Vec<usize>) {
        let mut result = LoadedFiles::default();
        let mut seen: FxHashSet<Path> = FxHashSet::default();
        // Upstream collects lib files into their own slice and concatenates
        // afterwards (`filesparser.go:486`, `:518`). Same here, because the two
        // are interleaved in the walk and only the libs are sorted.
        let mut libs: Vec<usize> = Vec::new();
        let mut rest: Vec<usize> = Vec::new();
        for root in &self.root_tasks {
            self.collect_task(*root, &mut seen, &mut result, &mut libs, &mut rest);
        }

        // `fileLoader.sortLibs` / `getDefaultLibFilePriority` (`fileloader.go:315`).
        // A stable sort, as `slices.SortFunc` is not — but upstream's comparator
        // is over a total order that only ties for two files of equal priority,
        // and equal priority means the same file, which the walk claims once. So
        // the tie never arises and stability is free insurance rather than a
        // divergence.
        libs.sort_by_key(|index| self.lib_file_priority(*index));

        result.lib_file_count = libs.len();
        result.loader_diagnostics.clone_from(&self.loader_diagnostics);
        let order: Vec<usize> = libs.into_iter().chain(rest).collect();
        result.file_names = order.iter().map(|i| self.tasks[*i].file_name.clone()).collect();
        (result, order)
    }

    /// Where a lib file sorts (`fileLoader.getDefaultLibFilePriority`).
    ///
    /// Upstream recovers the lib's name from its path and looks it up in
    /// `tsoptions.Libs`; the base name is recorded on the task here instead, so
    /// this is the lookup without the string surgery. `lib.d.ts` and
    /// `lib.es6.d.ts` sort first (priority 0); a name in the table sorts at its
    /// index plus one; anything else sorts last.
    fn lib_file_priority(&self, index: usize) -> usize {
        let Some(name) = self.tasks[index].lib_file else { return usize::MAX };
        if name == "lib.d.ts" || name == "lib.es6.d.ts" {
            return 0;
        }
        tsr_tsoptions::libs::lib_option_index(name)
            .map_or(tsr_tsoptions::LIB_MAP.len() + 2, |index| index + 1)
    }

    fn collect_task(
        &self,
        index: usize,
        seen: &mut FxHashSet<Path>,
        result: &mut LoadedFiles<'a>,
        libs: &mut Vec<usize>,
        rest: &mut Vec<usize>,
    ) {
        let task = &self.tasks[index];
        if !task.loaded || !seen.insert(task.path.clone()) {
            return;
        }
        result.requests.extend(task.type_resolution_requests.iter().cloned());
        result.traces.extend(task.type_resolutions_trace.iter().cloned());
        result.requests.extend(task.resolution_requests.iter().cloned());
        result.traces.extend(task.resolutions_trace.iter().cloned());

        for sub_task in &task.sub_tasks {
            self.collect_task(*sub_task, seen, result, libs, rest);
        }

        // A task whose file could not be read is upstream's `missingFiles`
        // (`filesparser.go:483`) — a diagnostic, and *not* a member of the
        // program. Before lib files this distinction was invisible, because
        // every path the walk reached had come from a file-system probe that
        // had already found it; a lib file is the first task built from a name
        // rather than from a probe, so it is the first that can be absent.
        if !task.is_for_automatic_type_directive && task.file.is_some() {
            if task.lib_file.is_some() { libs } else { rest }.push(index);
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

/// Whether the file is JavaScript (`ast.IsSourceFileJS`, by extension).
fn is_javascript_file(file_name: &str) -> bool {
    file_extension_is_one_of(
        file_name,
        &[EXTENSION_JS, EXTENSION_JSX, EXTENSION_MJS, EXTENSION_CJS],
    )
}

/// `getLibraryNameFromLibFileName` (`fileloader.go:676`).
///
/// ```text
/// lib.dom.d.ts                    → @typescript/lib-dom
/// lib.dom.iterable.d.ts           → @typescript/lib-dom/iterable
/// lib.es2015.symbol.wellknown.d.ts → @typescript/lib-es2015/symbol-wellknown
/// ```
///
/// The first component after `lib` names the package; the rest become one path
/// segment joined by `-`, stopping at the `d` of `.d.ts`. §531.
fn library_name_from_lib_file_name(lib_file_name: &str) -> String {
    let components: Vec<&str> = lib_file_name.split('.').collect();
    let mut path = String::from("@typescript/lib-");
    if let Some(second) = components.get(1) {
        path.push_str(second);
    }
    let mut index = 2;
    while let Some(&component) = components.get(index) {
        if component.is_empty() || component == "d" {
            break;
        }
        path.push(if index == 2 { '/' } else { '-' });
        path.push_str(component);
        index += 1;
    }
    path
}

#[cfg(test)]
mod tests {
    use tsr_core::{ModuleKind, ModuleResolutionKind, ScriptTarget, Tristate};
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

    /// What a load produced, with everything that borrows the arena left
    /// behind.
    ///
    /// The arena is created and dropped inside [`load_from`], so a
    /// `LoadedFiles<'a>` cannot escape it. This suite judges the *walk* — which
    /// files, in what order, and which resolutions — and none of that borrows;
    /// keeping the trees alive would make every test name a lifetime for
    /// nothing.
    #[derive(Debug, Default)]
    struct Loaded {
        file_names: Vec<String>,
        lib_file_count: usize,
        /// How many files came back parsed, which is the claim that the loader
        /// hands over what it already parsed rather than only naming it.
        file_count: usize,
        diagnostic_count: usize,
        requests: Vec<ResolutionRequest>,
        traces: Vec<Trace>,
    }

    fn load(files: &[(&str, &str)], roots: &[&str], options: CompilerOptions) -> Loaded {
        load_from(files, roots, options, "/libs")
    }

    fn load_from(
        files: &[(&str, &str)],
        roots: &[&str],
        options: CompilerOptions,
        default_library_path: &str,
    ) -> Loaded {
        let host = TestHost {
            fs: InMemoryFileSystem::new(
                files.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
                [],
                true,
            ),
        };
        let arena = tsr_core::Arena::new();
        let loaded = FileLoader::load(
            &arena,
            &host,
            LoadOptions {
                compiler_options: options,
                root_file_names: roots.iter().map(|r| (*r).to_string()).collect(),
                default_library_path: default_library_path.to_string(),
            },
        );
        Loaded {
            diagnostic_count: loaded.files.iter().map(|file| file.diagnostics().len()).sum(),
            file_names: loaded.file_names,
            lib_file_count: loaded.lib_file_count,
            file_count: loaded.files.len(),
            requests: loaded.requests,
            traces: loaded.traces,
        }
    }

    /// The base names of the lib files a load produced, in program order.
    fn libs(loaded: &Loaded) -> Vec<&str> {
        loaded.file_names[..loaded.lib_file_count]
            .iter()
            .map(|name| name.rsplit('/').next().expect("a name has at least one segment"))
            .collect()
    }

    fn traced() -> CompilerOptions {
        CompilerOptions { trace_resolution: Tristate::True, ..CompilerOptions::default() }
    }

    #[test]
    fn jsx_option_selects_tsx_parsing_for_javascript_roots() {
        let options = CompilerOptions {
            allow_js: Tristate::True,
            jsx: tsr_core::JsxEmit::Preserve,
            no_lib: Tristate::True,
            ..CompilerOptions::default()
        };
        let loaded =
            load(&[("/index.js", "export const view = <div />;")], &["/index.js"], options);
        assert_eq!(loaded.file_count, 1);
        assert_eq!(loaded.diagnostic_count, 0);
    }

    fn names(loaded: &Loaded) -> Vec<(&str, &str)> {
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
    fn a_request_carries_the_file_the_resolver_answered() {
        // The loader computed this and threw it away on the line that computed
        // it: `resolved.resolved_file_name` went into `add_sub_task` and was
        // dropped. Without it a program knows *which* files it holds and not
        // *which import reached which file*, which is the edge a cross-file
        // alias needs. ADR-0041.
        let loaded = load(
            &[("/a.ts", "import \"./b\";\nimport \"./nope\";\n"), ("/b.ts", "export {};\n")],
            &["/a.ts"],
            traced(),
        );
        let answered: Vec<(&str, Option<&str>)> =
            loaded.requests.iter().map(|r| (r.name.as_str(), r.resolved.as_deref())).collect();
        assert_eq!(
            answered,
            [("./b", Some("/b.ts")), ("./nope", None)],
            "the resolved name survives the load, and an unresolved import is `None` \
             rather than a plausible-looking path"
        );
    }

    #[test]
    fn a_request_records_a_resolution_the_program_declines_to_add() {
        // `should_add_file` is what decides membership, and it runs *after* the
        // resolution. A `.js` under `allowJs: false` resolves and is then not
        // added — upstream's `resolvedModules` records it just the same, so a
        // reader asking "what did this import resolve to" gets the resolver's
        // answer and not the program's membership test.
        //
        // The control is pinned by construction rather than by arithmetic: the
        // file list here is fixed at exactly `/a.ts` **before any code runs**,
        // because `allow_js` is off and `/b.js` is the only other file. So a
        // change that accidentally recorded only added files would leave this
        // request's `resolved` at `None` and cannot pass by coincidence.
        let loaded = load(
            &[("/a.ts", "import \"./b\";\n"), ("/b.js", "export {};\n")],
            &["/a.ts"],
            traced(),
        );
        assert_eq!(loaded.file_names, ["/a.ts"], "`/b.js` is resolved and not added");
        assert_eq!(
            loaded.requests.iter().map(|r| r.resolved.as_deref()).collect::<Vec<_>>(),
            [Some("/b.js")],
            "…and the resolution is recorded anyway"
        );
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

    // ---- Lib files ---------------------------------------------------------

    /// A file system with a lib directory holding every name asked for.
    fn with_libs<'a>(
        files: &[(&'a str, &'a str)],
        lib_names: &[&str],
    ) -> Vec<(String, &'a str, String)> {
        let mut all: Vec<(String, &str, String)> =
            files.iter().map(|(n, t)| ((*n).to_string(), *t, (*t).to_string())).collect();
        for name in lib_names {
            all.push((format!("/libs/{name}"), "", "declare const marker: number;\n".to_string()));
        }
        all
    }

    fn load_with_libs(
        files: &[(&str, &str)],
        lib_names: &[&str],
        roots: &[&str],
        options: CompilerOptions,
    ) -> Loaded {
        let all = with_libs(files, lib_names);
        let borrowed: Vec<(&str, &str)> =
            all.iter().map(|(name, _, text)| (name.as_str(), text.as_str())).collect();
        load_from(&borrowed, roots, options, "/libs")
    }

    #[test]
    fn the_default_lib_is_loaded_and_comes_first() {
        // The whole point of the slice: without this the program is the case's
        // own files and nothing else, so `Array` has no declaration anywhere.
        let loaded = load_with_libs(
            &[("/a.ts", "const x = 1;\n")],
            &["lib.d.ts"],
            &["/a.ts"],
            // `lib.d.ts` IS the ES5 lib, so the target is named rather
            // than defaulted: `GetEmitScriptTarget` returns the latest standard
            // for an unset target, which would pick `lib.es2025.full.d.ts`.
            CompilerOptions { target: ScriptTarget::ES5, ..CompilerOptions::default() },
        );
        assert_eq!(loaded.file_names, ["/libs/lib.d.ts", "/a.ts"]);
        assert_eq!(loaded.lib_file_count, 1);
        assert_eq!(loaded.file_count, 2, "the loader hands out what it parsed");
    }

    #[test]
    fn the_target_chooses_the_default_lib() {
        let loaded = load_with_libs(
            &[("/a.ts", "const x = 1;\n")],
            &["lib.es2020.full.d.ts"],
            &["/a.ts"],
            CompilerOptions { target: tsr_core::ScriptTarget::ES2020, ..Default::default() },
        );
        assert_eq!(libs(&loaded), ["lib.es2020.full.d.ts"]);
    }

    #[test]
    fn no_lib_loads_none_and_an_absent_lib_directory_is_not_the_same_thing() {
        let none = load_with_libs(
            &[("/a.ts", "const x = 1;\n")],
            &["lib.d.ts"],
            &["/a.ts"],
            CompilerOptions { no_lib: Tristate::True, ..Default::default() },
        );
        assert_eq!(none.file_names, ["/a.ts"]);
        assert_eq!(none.lib_file_count, 0);

        // A host with no lib directory produces the same *file list* by a
        // different route — the task exists and finds nothing. The distinction
        // is upstream's missing-file diagnostic, which this port does not have,
        // so it is asserted where it is visible: `noLib` never creates the task,
        // so it never probes for the file.
        let missing = load(&[("/a.ts", "const x = 1;\n")], &["/a.ts"], CompilerOptions::default());
        assert_eq!(missing.file_names, ["/a.ts"]);
        assert_eq!(missing.lib_file_count, 0);
    }

    #[test]
    fn a_program_with_no_root_files_gets_no_libs() {
        // Upstream guards the whole lib block on `len(rootFiles) > 0`. Without
        // it, an empty `include` would compile a program consisting of nothing
        // but `lib.d.ts`.
        let loaded = load_with_libs(
            &[("/a.ts", "const x = 1;\n")],
            &["lib.d.ts"],
            &[],
            // `lib.d.ts` IS the ES5 lib, so the target is named rather
            // than defaulted: `GetEmitScriptTarget` returns the latest standard
            // for an unset target, which would pick `lib.es2025.full.d.ts`.
            CompilerOptions { target: ScriptTarget::ES5, ..CompilerOptions::default() },
        );
        assert!(loaded.file_names.is_empty());
    }

    #[test]
    fn an_explicit_lib_list_is_sorted_by_load_order_and_not_by_the_order_given() {
        // `sortLibs`. `es2015.symbol` sits after `dom` in upstream's `Libs`, so
        // asking for them the other way round must still load `dom` first —
        // load order decides which declaration of a merged global wins, and a
        // list that merely echoed the command line would silently reverse it.
        let loaded = load_with_libs(
            &[("/a.ts", "const x = 1;\n")],
            &["lib.es2015.symbol.d.ts", "lib.dom.d.ts", "lib.es5.d.ts"],
            &["/a.ts"],
            CompilerOptions {
                lib: vec!["es2015.symbol".into(), "dom".into(), "es5".into()],
                ..Default::default()
            },
        );
        assert_eq!(libs(&loaded), ["lib.es5.d.ts", "lib.dom.d.ts", "lib.es2015.symbol.d.ts"]);
    }

    #[test]
    fn the_unnumbered_lib_sorts_ahead_of_every_numbered_one() {
        // `getDefaultLibFilePriority` gives `lib.d.ts` and `lib.es6.d.ts`
        // priority 0, ahead of `es5` at index 0 + 1. They are not in `Libs` at
        // all, so a lookup without the special case would sort them *last*.
        let loaded = load_with_libs(
            &[("/a.ts", "/// <reference lib=\"es5\" />\nconst x = 1;\n")],
            &["lib.d.ts", "lib.es5.d.ts"],
            &["/a.ts"],
            // `lib.d.ts` IS the ES5 lib, so the target is named rather
            // than defaulted: `GetEmitScriptTarget` returns the latest standard
            // for an unset target, which would pick `lib.es2025.full.d.ts`.
            CompilerOptions { target: ScriptTarget::ES5, ..CompilerOptions::default() },
        );
        assert_eq!(libs(&loaded), ["lib.d.ts", "lib.es5.d.ts"]);
    }

    #[test]
    fn a_reference_lib_directive_adds_a_lib_file_and_resolves_nothing() {
        let loaded = load_with_libs(
            &[("/a.ts", "/// <reference lib=\"es2015.symbol\" />\nconst x = 1;\n")],
            &["lib.d.ts", "lib.es2015.symbol.d.ts"],
            &["/a.ts"],
            // ES5 named rather than defaulted; `lib.d.ts` is the ES5 lib and an
            // unset target resolves to the latest standard.
            CompilerOptions { target: ScriptTarget::ES5, ..traced() },
        );
        assert_eq!(libs(&loaded), ["lib.d.ts", "lib.es2015.symbol.d.ts"]);
        // It is a table lookup, not a module resolution: no request, no trace.
        assert!(names(&loaded).is_empty());
        assert!(loaded.traces.is_empty());
    }

    #[test]
    fn an_unknown_reference_lib_adds_nothing() {
        // Upstream reports `processingDiagnosticKindUnknownReference`. This port
        // has no loader diagnostics, so the file is simply absent — never
        // substituted with a default, which would be a plausible answer to a
        // question upstream answers with an error.
        //
        // The program asks for `es5` explicitly and the host *also* ships
        // `lib.d.ts`, so a fallback to the default would be visible. Written
        // against the default lib instead, this test could not see one: the
        // fallback would name a file the program already had.
        let loaded = load_with_libs(
            &[("/a.ts", "/// <reference lib=\"nonsense\" />\nconst x = 1;\n")],
            &["lib.d.ts", "lib.es5.d.ts"],
            &["/a.ts"],
            CompilerOptions { lib: vec!["es5".into()], ..Default::default() },
        );
        assert_eq!(libs(&loaded), ["lib.es5.d.ts"]);
    }

    #[test]
    fn no_lib_also_silences_a_reference_lib_directive() {
        let loaded = load_with_libs(
            &[("/a.ts", "/// <reference lib=\"es5\" />\nconst x = 1;\n")],
            &["lib.es5.d.ts"],
            &["/a.ts"],
            CompilerOptions { no_lib: Tristate::True, ..Default::default() },
        );
        assert_eq!(loaded.lib_file_count, 0);
    }

    #[test]
    fn a_lib_files_format_is_fixed_and_its_package_json_is_never_read() {
        // The skip upstream takes for watcher noise and this port must take for
        // correctness: `load_source_file_meta_data` warms the resolver's
        // `package.json` cache, which later resolutions observe.
        //
        // Made observable by giving the lib directory a `"type": "module"`
        // package and the lib file an import. Under `node16` a `.d.ts` in a
        // module package is ESM, so the import would resolve in `ESNext` mode;
        // the fixed `CommonJS` metadata is what keeps it CommonJS.
        let options = CompilerOptions {
            trace_resolution: Tristate::True,
            module_resolution: ModuleResolutionKind::Node16,
            // The mounted default library is spelled `lib.d.ts`, which is the
            // ES5 one; an unset target now resolves to the latest standard and
            // would look for `lib.es2025.full.d.ts` instead, mount nothing, and
            // make this test assert about an empty program.
            target: ScriptTarget::ES5,
            ..CompilerOptions::default()
        };
        let loaded = load_from(
            &[
                ("/a.ts", "const x = 1;\n"),
                ("/libs/lib.d.ts", "import \"./dep.js\";\n"),
                ("/libs/dep.d.ts", "export {};\n"),
                ("/libs/package.json", "{ \"type\": \"module\" }"),
            ],
            &["/a.ts"],
            options,
            "/libs",
        );
        let modes: Vec<ResolutionMode> = loaded.requests.iter().map(|r| r.mode).collect();
        assert_eq!(
            modes,
            [ResolutionMode::CommonJS],
            "a lib file resolves as CommonJS however its package.json reads"
        );
    }
}
