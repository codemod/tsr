//! Circularity detection, and the seam through which the checker reaches
//! another file.
//!
//! Ported from `Checker.pushTypeResolution`, `popTypeResolution` and
//! `findResolutionCycleStartIndex` (`internal/checker/checker.go:18758`–`18794`).
//!
//! It also holds [`ModuleHost`], the slice of upstream's `Program` interface
//! (`checker.go:547`) that the checker needs in order to resolve a module
//! specifier — the one thing a cross-file alias cannot compute for itself.
//!
//! # Why a stack rather than a flag
//!
//! The obvious implementation is a "currently resolving" bit per symbol, and it
//! is wrong in a way that only shows on a cycle of length greater than one.
//! Upstream, on finding a cycle, marks **every frame from the cycle start
//! onward** as failed:
//!
//! ```go
//! for i := resolutionCycleStartIndex; i < len(c.typeResolutions); i++ {
//!     c.typeResolutions[i].result = false
//! }
//! ```
//!
//! So in `const a = b, b = c, c = a`, all three participants resolve to an error
//! — not only `a`, the one that closed the loop. A per-symbol flag gives the
//! symbol that closed the cycle an error and lets the others quietly succeed with
//! whatever half-built answer was on the stack, which is both wrong and
//! order-dependent.
//!
//! `findResolutionCycleStartIndex` stops at a published property identity. This
//! is not merely a scan optimization once object identity and member/return
//! completion are separate: later preparation can re-enter an older frame
//! through an identity already available to native lazy consumers. Return-slot
//! pushes supply the checker's existing publication facts through `push_with`;
//! callers without independently published identities retain ordinary `push`.

use tsr_ast::{NodeId, NodeTable};
use tsr_binder::SymbolId;
use tsr_core::ResolutionMode;

/// Native `TypeSystemEntity`: a symbol and its declaration-owned return slot
/// are different entities even when the declaration belongs to that symbol.
/// A base constraint is resolved per *type*; this port keys it by the type and
/// the alias-evaluation mapper it was computed under, the same identity as
/// `base_constraint_cache` (`constraints.rs`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ResolutionTarget {
    Symbol(SymbolId),
    Signature(crate::declared::TypeLiteralKey),
    BaseConstraint(crate::constraints::BaseConstraintKey),
}

impl From<SymbolId> for ResolutionTarget {
    fn from(symbol: SymbolId) -> Self {
        Self::Symbol(symbol)
    }
}

/// How the checker reaches another file.
///
/// Upstream's checker does not take a concrete `Program`. It takes an
/// **interface**, declared inside the checker package itself
/// (`internal/checker/checker.go:547`), held as the field `program Program`
/// (`checker.go:581`), whose one relevant method here is
/// `GetResolvedModule` (`checker.go:558`, implemented at
/// `internal/compiler/program.go:521` as a lookup keyed on
/// `(file.Path(), {Name, Mode})`). This trait is that interface reduced to the
/// single question `resolveExternalModule` (`checker.go:15149`) asks of it, and
/// it lives here for the same reason upstream's lives in `checker.go`: the
/// checker owns the shape of what it needs, and `tsr-compiler` depends on
/// `tsr-checker` rather than the other way round.
///
/// # Why this is one method and not sixteen
///
/// Upstream's interface carries eighteen methods because upstream's
/// `resolveExternalModule` reports fourteen distinct diagnostics — about `.ts`
/// extensions, about rewritten relative imports, about a `CommonJS` file reaching
/// an ES module. **None of that is ported**, and the arm that is ported needs
/// exactly one fact: which file a specifier names. Adding the rest now would be
/// documenting an intention as though it were built.
///
/// # Why the ids are node ids and not paths
///
/// [ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md) gave
/// a program one `NodeTable`, one `NodeMap` and one `BindResult`, so a
/// [`tsr_binder::SymbolId`] already names one symbol across every file
/// (`crates/tsr-compiler/src/lib.rs`, `Program::bind_source_files`). That is
/// what makes this seam small: the checker does not need cross-file *symbol*
/// access, because it already has it. It needs only the specifier-to-file map,
/// and a `SourceFile` node id is how a file is named inside that identity space.
///
/// The consequence is that `tsr-checker` names no path type, no
/// `ResolvedModule` and no `ResolutionMode` — all three are the host's
/// vocabulary. If a mode is ever needed to disambiguate two resolutions of one
/// specifier in one file, this method grows a parameter and the single call site
/// in [`crate::symbols`] follows.
pub trait ModuleHost {
    /// The file a module specifier names, or `None` if it names none.
    ///
    /// `Program.GetResolvedModule` (`internal/compiler/program.go:521`)
    /// composed with `GetSourceFileForResolvedModule`, which is how
    /// `resolveExternalModule` uses it (`checker.go:15149`).
    ///
    /// Both ids are **`SourceFile` node ids**. `None` covers upstream's
    /// unresolved module *and* its resolved-to-a-file-not-in-the-program case;
    /// the checker treats them alike because both answer `errorType`, and
    /// telling them apart is a diagnostic distinction this port has no consumer
    /// for.
    ///
    /// **Not the same as "the file is not a module."** That is
    /// `sourceFile.Symbol == nil` (`checker.go:15321`), which the checker tests
    /// for itself with `BindResult::symbol_of` on the returned id — so a host
    /// that answers `Some` for a plain script is answering correctly, and the
    /// checker still gaps.
    fn resolved_module(&self, importing_file: NodeId, specifier: &str) -> Option<NodeId>;

    /// Original source bytes owned by this `SourceFile` and node table.
    ///
    /// Native source is carried by the `SourceFile`; this port keeps it in the
    /// `ProgramFile` instead. A host must witness the table's identity before
    /// interpreting `file`, because raw `NodeId`s can collide across programs.
    /// Unsupported/legacy hosts decline; absence does not certify clean syntax.
    fn source_text(&self, _file: NodeId, _nodes: &NodeTable) -> Option<&str> {
        None
    }

    /// The parse diagnostics of `file`: native's `sourceFile.Diagnostics()`,
    /// which the checker reads in one place, `checkBinaryLikeExpressionWorker`'s
    /// comma arm (`checker.go:12537`). Witnessed against `nodes` as
    /// [`ModuleHost::source_text`] is. Hosts without parse results answer
    /// none, which costs the comma arm's one exemption and nothing else.
    /// `docs/parity/notes/r5-smallcodes2.md` §2.2.
    fn parse_diagnostics(
        &self,
        _file: NodeId,
        _nodes: &NodeTable,
    ) -> &[tsr_diagnostics::Diagnostic] {
        &[]
    }

    /// §110: the `TypeParameterDeclaration` node ids a declaration's JSDoc
    /// `@template` tags carry, in tag order. Id-vocabulary per ADR-0034;
    /// default empty so hosts without JSDoc (unit checkers) change nothing.
    fn jsdoc_template_parameters(&self, _declaration: NodeId) -> Vec<NodeId> {
        Vec::new()
    }

    /// Did resolution name a file **at all**, whether or not the program holds
    /// it?
    ///
    /// `Program.GetResolvedModule(...).IsResolved()` (`checker.go:15208`), the
    /// half of the composition above that
    /// [`ModuleHost::resolved_module`] throws away.
    ///
    /// # Why the trait grew a second method
    ///
    /// The doc on [`ModuleHost::resolved_module`] said, correctly at the time,
    /// that its `None` covers upstream's unresolved module *and* its
    /// resolved-to-a-file-not-in-the-program case, and that *"telling them apart
    /// is a diagnostic distinction this port has no consumer for."* The check
    /// traversal ([`crate::check`]) is that consumer. Upstream reports a
    /// **different code** for each — TS2307 for the first, TS7016 / TS6142 /
    /// TS2306 for the second, all at the same position — so a rule that cannot
    /// distinguish them manufactures a wrong diagnostic on every untyped
    /// `node_modules` package and every `.tsx` reached without `--jsx`.
    /// Measured, before the gate existed: 15 of 60 wrong lines.
    ///
    /// ADR-0041's "one method rather than eighteen" is a rule about not porting
    /// the `Program` interface speculatively, not a cap. This is the second
    /// question a real caller asks.
    fn module_resolution_found(&self, importing_file: NodeId, specifier: &str) -> bool;

    /// The **file name** the resolver answered for this specifier.
    ///
    /// The third question a real caller asks, and the one that picks the code
    /// among the resolved-but-not-in-the-program family: a `.js` is TS7016, a
    /// `.tsx` reached without `--jsx` is TS6142, a resolved `.ts` that is not a
    /// module is TS2306. [`ModuleHost::module_resolution_found`] separates that
    /// family from TS2307's; only the extension separates its members.
    ///
    /// Keyed by `{Name, Mode}` like the other `_in_mode` questions. Defaulted
    /// to `None`, which reports nothing — a host that is not a real program
    /// has no resolutions to describe. §357.
    fn resolved_module_path_in_mode(
        &self,
        _importing_file: NodeId,
        _specifier: &str,
        _mode: ResolutionMode,
    ) -> Option<String> {
        None
    }

    /// The resolved module's `Extension` and `ResolvedUsingTsExtension`, and
    /// whether `GetSourceFileForResolvedModule` holds its file; `None` when
    /// it did not resolve. Read by `GetResolutionDiagnostic`
    /// (`checker.go:15209`, `crate::isolated_alias`). `mode: None` asks for
    /// the resolution every mode the file asked in agrees on (`None` when
    /// they disagree), so the caller computes the usage's mode only then.
    fn resolved_module_extension(
        &self,
        _importing_file: NodeId,
        _specifier: &str,
        _mode: Option<ResolutionMode>,
    ) -> Option<(&str, bool, bool)> {
        None
    }

    /// `program.SourceFileMayBeEmitted(file, false)` (`compiler/emitter.go:452`),
    /// read by `resolveExternalModule`'s `rewriteRelativeImportExtensions`
    /// arm (`checker.go:15282`, `crate::isolated_alias`). `false` for a host
    /// with no emit.
    fn source_file_may_be_emitted(&self, _file: NodeId) -> bool {
        false
    }

    /// The program's `ComparePathsOptions` (`UseCaseSensitiveFileNames`,
    /// `GetCurrentDirectory`), read by that arm's `GetRelativePathFromFile`.
    fn compare_paths_options(&self) -> tsr_path::ComparePathsOptions {
        tsr_path::ComparePathsOptions {
            use_case_sensitive_file_names: true,
            current_directory: String::new(),
        }
    }

    /// `Program.GetResolvedModule(file, moduleReference, mode)`
    /// (`checker.go:558`) composed with `GetSourceFileForResolvedModule`: the
    /// file the specifier resolved to **under the usage location's mode**.
    ///
    /// A file can ask for one specifier in two modes — `import("foo", { with:
    /// { "resolution-mode": "require" } })` beside a plain `import("foo")` —
    /// and get two answers. Defaulted to [`ModuleHost::resolved_module`] for
    /// hosts whose resolutions are not mode-aware.
    fn resolved_module_in_mode(
        &self,
        importing_file: NodeId,
        specifier: &str,
        _mode: ResolutionMode,
    ) -> Option<NodeId> {
        self.resolved_module(importing_file, specifier)
    }

    /// [`ModuleHost::module_resolution_found`] under the usage location's
    /// mode; defaulted like [`ModuleHost::resolved_module_in_mode`].
    fn module_resolution_found_in_mode(
        &self,
        importing_file: NodeId,
        specifier: &str,
        _mode: ResolutionMode,
    ) -> bool {
        self.module_resolution_found(importing_file, specifier)
    }

    /// `Program.GetModeForUsageLocation` (`checker.go:15202`): the resolution
    /// mode of the string-literal module specifier `usage`. `None` for a host
    /// without per-file module formats.
    fn mode_for_usage_location(&self, _importing_file: NodeId, _usage: NodeId) -> ResolutionMode {
        ResolutionMode::None
    }

    /// `Program.GetDefaultResolutionModeForFile` (`checker.go:15204`), for a
    /// location with no specifier to read a mode from.
    fn default_resolution_mode_for_file(&self, _file: NodeId) -> ResolutionMode {
        ResolutionMode::None
    }

    /// `Program.GetEmitSyntaxForUsageLocation` (`program.go:1550`): the
    /// module syntax the string-literal specifier `usage` will be emitted as
    /// (`getEmitSyntaxForUsageLocationWorker`, `fileloader.go:764`), without
    /// the `resolution-mode` overrides and option gate that
    /// [`ModuleHost::mode_for_usage_location`] applies. `None` for a host
    /// without per-file module formats.
    fn emit_syntax_for_usage_location(
        &self,
        _importing_file: NodeId,
        _usage: NodeId,
    ) -> ResolutionMode {
        ResolutionMode::None
    }

    /// `Program.GetImpliedNodeFormatForEmit` (`program.go:1554`): the format a
    /// file is emitted in (`ast.GetImpliedNodeFormatForEmitWorker`). `None`
    /// for a host without per-file module formats.
    fn implied_node_format_for_emit(&self, _file: NodeId) -> ResolutionMode {
        ResolutionMode::None
    }

    /// For an unresolved specifier that `resolveExternalModule`'s TS2834/TS2835
    /// arm (`checker.go:15420`) can describe — extensionless, relative, under
    /// `moduleResolution: node16`/`nodenext` — the
    /// `getSuggestedImportExtension` (`checker.go:15461`) answer. `None` when
    /// the arm does not apply; the checker adds the ESM-mode condition.
    fn extensionless_relative_import(
        &self,
        _importing_file: NodeId,
        _specifier: &str,
        _mode: ResolutionMode,
    ) -> Option<ExtensionlessImport> {
        None
    }

    /// §143: the file path of a `SourceFile` node, for the relative-specifier
    /// spelling. `None` (the default) declines the file half.
    fn file_path(&self, _file: NodeId) -> Option<String> {
        None
    }

    /// The namespace an `@jsx` pragma names for this file, if it has one.
    ///
    /// `getJsxNamespaceAt` (`checker/jsx.go:1306`) resolves the factory's first
    /// identifier and looks for a `JSX` namespace among **that** symbol's
    /// exports, so `@jsx dom.createElement` takes its `IntrinsicElements` from
    /// `dom.JSX` rather than from the global `JSX`.
    ///
    /// Defaulted to `None`: a host that is not a real program has no pragmas to
    /// report, and answering `None` is answering correctly — the checker then
    /// falls back to the global lookup, which is upstream's own order. §211.
    fn jsx_factory_namespace(&self, _file: NodeId) -> Option<String> {
        None
    }

    /// The namespace an `@jsxFrag` pragma names for this file, if it has one
    /// (`getJsxNamespace`'s fragment arm, `checker/jsx.go:1350`).
    ///
    /// Defaulted to `None` for the reason [`ModuleHost::jsx_factory_namespace`]
    /// gives.
    fn jsx_fragment_factory_namespace(&self, _file: NodeId) -> Option<String> {
        None
    }

    /// `ast.GetJSXImplicitImportBase(options, file)` (`utilities.go:2771`):
    /// the package the automatic JSX runtime imports from, or `None` under
    /// the classic runtime.
    ///
    /// Defaulted to `None`: a host with no options and no pragmas is the
    /// classic runtime.
    fn jsx_implicit_import_base(&self, _file: NodeId) -> Option<String> {
        None
    }

    /// Whether the file has an `@jsx` pragma and an `@jsxFrag` pragma, in that
    /// order — present at all, whether or not their factories parse
    /// (`checkJsxFragment`, `checker/jsx.go:114`).
    fn jsx_pragmas_present(&self, _file: NodeId) -> (bool, bool) {
        (false, false)
    }

    /// Is this file a `.d.ts`?
    ///
    /// `SourceFile.IsDeclarationFile`, which the checker reads in
    /// `canHaveSyntheticDefault` (`checker.go:14850`) among other places. It is
    /// a *host* question here for the reason
    /// [ADR-0016](../../../docs/adr/0016-file-info-not-a-file-name.md) gives:
    /// nothing on the AST carries a file name, and the checker learns its
    /// **own** file's ambience from `FileContext` — which says nothing about
    /// the module on the other end of an import.
    ///
    /// Defaulted to `false`, which is the conservative answer: it sends
    /// `canHaveSyntheticDefault` down the source-file arm, where only an
    /// `export =` grants one. A host that cannot tell therefore reports TS1192
    /// where upstream might not, rather than the reverse. §531.
    fn is_declaration_file(&self, _file: NodeId) -> bool {
        false
    }

    /// `ast.IsPlainJSFile(file, compilerOptions.CheckJs)`
    /// (`ast/utilities.go:2889`): a JavaScript file with no `@ts-check` /
    /// `@ts-nocheck` directive under a `checkJs` that is unset. `None` from a
    /// host that cannot tell; the checker then treats every JavaScript file
    /// as plain, the suppressing direction.
    fn is_plain_js_file(&self, _file: NodeId) -> Option<bool> {
        None
    }

    /// `resolveHelpersModule`'s program half (`checker.go:28676`): what the
    /// synthetic `tslib` import the loader adds under `importHelpers`
    /// (`fileloader.go:543`) resolved to, for this file. Defaulted to
    /// [`ImportHelpersModule::NotRequested`]: a host without that import
    /// makes the checker answer `unknownSymbol` and report nothing.
    fn import_helpers_module(&self, _file: NodeId) -> ImportHelpersModule {
        ImportHelpersModule::NotRequested
    }

    /// `ModuleSpecifierGenerationHost.GetPackageJsonInfo(dir/package.json)`
    /// (`modulespecifiers/types.go`), for `tryDirectoryWithPackageJson`
    /// (`modulespecifiers/specifiers.go:832`): the `package.json` in
    /// `package_directory`, reduced to the fields specifier generation
    /// reads. `None` when there is none. Defaulted to `None`, which makes
    /// every `node_modules` directory answer "no package.json" — the
    /// `index.*`-only arm. r5-modules §5.
    fn package_json_for_specifiers(&self, _package_directory: &str) -> Option<&PackageJsonView> {
        None
    }

    /// The compiler options `tryGetModuleNameAsNodeModule` reads, and
    /// `module.GetConditions(options, mode)` for its `exports` matching.
    /// Defaulted to a host that does not resolve `exports`.
    fn specifier_options(&self, _mode: ResolutionMode) -> SpecifierOptions {
        SpecifierOptions::default()
    }

    /// Whether any program file lives under `node_modules`. Every specifier
    /// `ReportLikelyUnsafeImportRequiredError` can see is generated for such
    /// a file (`symbol_chain`'s `node_modules` arm), so without one the
    /// declaration-emit tracker has nothing to find. Defaulted to `true`,
    /// which never skips the search.
    fn has_node_modules_files(&self) -> bool {
        true
    }

    /// `module.IsApplicableVersionedTypesKey`: a `types@<range>` condition
    /// whose range admits this compiler's version. Defaulted to `false`.
    fn is_applicable_versioned_types_key(&self, _key: &str) -> bool {
        false
    }

    /// `Program.GetSymlinkCache` (`compiler/program.go:2059`), which
    /// `GetEachFileNameOfModule` reads to name a realpath'd module by the
    /// symlinked paths that reach it. Defaulted to `None`: every module has
    /// its own path only. r6-specifiers §2.
    fn known_symlinks(&self) -> Option<&crate::module_specifiers::KnownSymlinks> {
        None
    }
}

/// A `package.json` as module-specifier generation reads it
/// ([`ModuleHost::package_json_for_specifiers`]).
///
/// Plain data so the checker does not depend on `tsr-module`; the program
/// converts its parsed `tsr_module::package_json::PackageJson`.
#[derive(Debug, Clone, Default)]
pub struct PackageJsonView {
    /// `type`, when it is a string.
    pub package_type: Option<String>,
    /// `typings`, `types`, `main`, when each is a string.
    pub typings: Option<String>,
    /// See [`PackageJsonView::typings`].
    pub types: Option<String>,
    /// See [`PackageJsonView::typings`].
    pub main: Option<String>,
    /// `exports`; `None` when the field is absent.
    pub exports: Option<SpecifierJson>,
    /// The `paths` of the `typesVersions` entry that matches this compiler
    /// (`GetVersionPaths`), when `typesVersions` is an object and one matched.
    pub types_versions_paths: Option<tsr_core::OrderedMap<Vec<String>>>,
}

/// A JSON value, for `exports` matching.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SpecifierJson {
    /// `null`.
    Null,
    /// A string.
    String(String),
    /// An array.
    Array(Vec<SpecifierJson>),
    /// An object, in declaration order.
    Object(Vec<(String, SpecifierJson)>),
    /// A boolean or number: matches nothing.
    Other,
}

/// [`ModuleHost::specifier_options`]'s answer.
#[derive(Debug, Clone, Default)]
pub struct SpecifierOptions {
    /// `GetResolvePackageJsonExports()`.
    pub resolve_package_json_exports: bool,
    /// `moduleResolution` is `node16`..`nodenext`.
    pub module_resolution_is_node_next: bool,
    /// `module.GetConditions(options, mode)`.
    pub conditions: Vec<String>,
}

/// [`ModuleHost::import_helpers_module`]'s answer.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImportHelpersModule {
    /// The file got no synthetic `tslib` import.
    NotRequested,
    /// The import resolved to nothing: TS2354 territory.
    NotFound,
    /// The import resolved to a file the program does not hold (an untyped
    /// package; upstream's TS7016 family).
    OutsideProgram,
    /// The import resolved to this source file.
    File(NodeId),
}

/// `getSuggestedImportExtension`'s answer (`checker.go:15461`) for an
/// extensionless relative import: the output extension of the first sibling
/// file that exists, or none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionlessImport {
    /// TS2835 — `Did you mean '{specifier}{extension}'?`
    Suggested(&'static str),
    /// TS2834 — `Consider adding an extension to the import path.`
    Unsuggested,
}

/// Which lazily-computed property of an entity is being resolved.
///
/// Upstream's `TypeSystemPropertyName`. Only the variants this port reaches are
/// listed; adding one means porting the code that resolves it, not inventing a
/// name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PropertyName {
    /// `TypeSystemPropertyNameType` — the type of a symbol.
    Type,
    /// `TypeSystemPropertyNameDeclaredType` — the type a *type* symbol declares.
    ///
    /// A separate property from [`PropertyName::Type`] because upstream keys the
    /// stack on the pair: a class symbol has both a declared type (its instance
    /// type) and a type (`typeof C`), and one may legitimately be computed while
    /// the other is in progress. Sharing one key would report a cycle that is
    /// not there.
    DeclaredType,
    /// `TypeSystemPropertyNameResolvedReturnType`, not the callable's type.
    ResolvedReturnType,
    /// `TypeSystemPropertyNameResolvedBaseConstraint` — the base constraint of
    /// a type (`getResolvedBaseConstraint`, `checker.go:27447`).
    ResolvedBaseConstraint,
    /// `TypeSystemPropertyNameResolvedBaseConstructorType` — a class's base
    /// constructor type (`getBaseConstructorTypeOfClass`, `checker.go:16957`).
    ResolvedBaseConstructorType,
    /// `TypeSystemPropertyNameResolvedBaseTypes` — a class or interface's base
    /// types (`getBaseTypes`, `checker.go:19167`).
    ResolvedBaseTypes,
}

/// One frame of the resolution stack.
#[derive(Debug, Clone, Copy)]
struct Resolution<K> {
    target: K,
    property: PropertyName,
    /// `false` once this frame is known to be part of a cycle.
    succeeded: bool,
}

/// The resolution stack, `Checker.typeResolutions`.
///
/// Generic in the key because upstream's is: `pushTypeResolution` takes a
/// `TypeSystemEntity`, which is a symbol for `TypeSystemPropertyNameType` and a
/// **type** for `TypeSystemPropertyNameResolvedTypeArguments`. The checker uses
/// `Resolutions<SymbolId>` today; the other keys arrive with the code that needs
/// them.
pub struct Resolutions<K> {
    stack: Vec<Resolution<K>>,
    /// Stack depths at which this port entered a type construct that native
    /// resolves *lazily* (an anonymous type literal's members, a deferred
    /// type reference's arguments; `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode`,
    /// `isDeferredTypeReferenceNode`, `checker.go:22933`, `:23236`). This port
    /// builds those eagerly, so it re-reaches frames native never re-reaches
    /// while they are active. Such a re-entry is not a native cycle: it must
    /// not fail any frame. See [`Resolutions::deferred_since`].
    deferrals: Vec<usize>,
    /// `Checker.resolutionStart` (`checker.go:788`): the lowest frame
    /// `findResolutionCycleStartIndex` (`checker.go:18783`) searches. Raised
    /// to the stack's depth by [`Resolutions::reset_start`] while variances
    /// are measured (`getVariancesWorker`, `relater.go:1358`), so a frame
    /// pushed outside that scope is not a cycle participant inside it. Native
    /// also raises it while a call is first resolved (`getResolvedSignature`,
    /// `checker.go:8417`); that scope is not wired here: this port's call
    /// re-entry guards (`resolving_signature_calls`) answer before the
    /// re-entered symbol's cycle closes, so the reset measured 15 RIGHT lines
    /// lost (`circularReferenceInReturnType`,
    /// `propertyAccessOnTypeParameterWithConstraints4/5`).
    start: usize,
    /// Monotonic count of answers this stack gave that depend on its active
    /// frames: every cycle [`Resolutions::push_with`] found (it marks frames
    /// failed), and every read-only probe that answered `true`
    /// ([`Resolutions::on_stack`], [`Resolutions::deferred_since`],
    /// [`Resolutions::has_property_frame`], [`Resolutions::has_active_return`],
    /// [`Resolutions::active_signature_keys`] yielding a frame). A probe that
    /// answers `false` answers what an empty stack would. A computation during
    /// which this did not move read nothing from the frames, so its answer
    /// does not depend on which frames were open: the memo tables'
    /// publication rule (`docs/parity/notes/r4-perf3.md` §2). A `Cell`
    /// because the probes are `&self`; the stack is one checker's.
    observations: std::cell::Cell<u64>,
}

/// `observations` is left out: it counts reads, so a read-only probe moves
/// it, and the state snapshots tests take before and after a read-only
/// probe compare this rendering.
impl<K: std::fmt::Debug> std::fmt::Debug for Resolutions<K> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Resolutions")
            .field("stack", &self.stack)
            .field("deferrals", &self.deferrals)
            .field("start", &self.start)
            .finish_non_exhaustive()
    }
}

impl<K> Default for Resolutions<K> {
    fn default() -> Self {
        Self {
            stack: Vec::new(),
            deferrals: Vec::new(),
            start: 0,
            observations: std::cell::Cell::new(0),
        }
    }
}

impl<K: Clone + PartialEq> Resolutions<K> {
    /// An empty stack.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// How deep the stack is. For tests that assert it is left balanced.
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.len()
    }

    /// [`Resolutions::observations`]' current value: compare two reads to
    /// learn whether anything between them depended on the open frames.
    #[must_use]
    pub(crate) fn observations(&self) -> u64 {
        self.observations.get()
    }

    /// Count one frame-dependent answer and pass it through.
    fn observed(&self, answer: bool) -> bool {
        if answer {
            self.observations.set(self.observations.get() + 1);
        }
        answer
    }

    /// Return work is active even before a nested symbol/frame closes a cycle.
    pub(crate) fn has_active_return(&self) -> bool {
        self.observed(
            self.stack.iter().any(|frame| frame.property == PropertyName::ResolvedReturnType),
        )
    }

    /// Whether `(target, property)` is currently resolving, WITHOUT marking
    /// the cycle as failed — the §29 alias placeholder's read-only probe
    /// (`checker-notes-narrow.md`).
    #[must_use]
    pub fn on_stack(&self, target: impl Into<K>, property: PropertyName) -> bool {
        let target = target.into();
        self.observed(self.stack.iter().any(|r| r.target == target && r.property == property))
    }

    /// Enter a construct native resolves lazily; balance with
    /// [`Resolutions::exit_deferred`].
    pub(crate) fn enter_deferred(&mut self) {
        self.deferrals.push(self.stack.len());
    }

    /// Leave the innermost lazily resolved construct.
    pub(crate) fn exit_deferred(&mut self) {
        self.deferrals.pop().expect("exited a deferral that was never entered");
    }

    /// Whether the innermost active `(target, property)` frame was pushed
    /// *before* the current lazily resolved construct was entered — native
    /// would only reach it again after that frame completed, so the re-entry
    /// is this port's eagerness, not a cycle. Read-only: marks nothing.
    #[must_use]
    pub(crate) fn deferred_since(&self, target: impl Into<K>, property: PropertyName) -> bool {
        let target = target.into();
        let Some(index) =
            self.stack.iter().rposition(|r| r.target == target && r.property == property)
        else {
            return false;
        };
        self.observed(self.deferrals.last().is_some_and(|&depth| depth > index))
    }

    /// Whether any frame resolves `property`.
    #[must_use]
    pub(crate) fn has_property_frame(&self, property: PropertyName) -> bool {
        self.observed(self.stack.iter().any(|frame| frame.property == property))
    }

    /// Begin resolving `property` of `symbol`.
    ///
    /// Returns `false` if that is already in progress — a cycle — having first
    /// marked every frame from the cycle's start as failed, so that all of its
    /// participants report the circularity rather than only the one that closed
    /// it. The caller must **not** pop when this returns `false`; nothing was
    /// pushed.
    pub fn push(&mut self, target: impl Into<K>, property: PropertyName) -> bool {
        self.push_with(target, property, |_, _| false)
    }

    /// Native findResolutionCycleStartIndex / typeResolutionHasProperty
    /// (5b1047d1, checker.go:18786). Check publication before matching a frame;
    /// a resolved property stops the search, even if it is the requested pair.
    /// Frames below `resolutionStart` ([`Resolutions::reset_start`]) are not
    /// searched.
    pub(crate) fn push_with(
        &mut self,
        target: impl Into<K>,
        property: PropertyName,
        mut has_property: impl FnMut(&K, PropertyName) -> bool,
    ) -> bool {
        let target = target.into();
        let mut cycle = None;
        let start = self.start.min(self.stack.len());
        for (offset, frame) in self.stack[start..].iter().enumerate().rev() {
            if has_property(&frame.target, frame.property) {
                break;
            }
            if frame.target == target && frame.property == property {
                cycle = Some(start + offset);
                break;
            }
        }
        if let Some(start) = cycle {
            self.observed(true);
            for frame in &mut self.stack[start..] {
                frame.succeeded = false;
            }
            return false;
        }
        self.stack.push(Resolution { target, property, succeeded: true });
        true
    }

    /// Begin a scope whose cycle search starts at the current depth
    /// (`c.resolutionStart = len(c.typeResolutions)`, `checker.go:8425`,
    /// `relater.go:1361`); answers the saved start for
    /// [`Resolutions::restore_start`].
    pub(crate) fn reset_start(&mut self) -> usize {
        std::mem::replace(&mut self.start, self.stack.len())
    }

    /// End a [`Resolutions::reset_start`] scope (`c.resolutionStart =
    /// saveResolutionStart`, `checker.go:8429`, `relater.go:1406`).
    pub(crate) fn restore_start(&mut self, saved: usize) {
        self.start = saved;
    }

    /// Finish the innermost resolution, reporting whether it was cycle-free.
    ///
    /// # Panics
    ///
    /// If the stack is empty, which means a [`Resolutions::push`] that returned
    /// `false` was popped anyway.
    pub fn pop(&mut self) -> bool {
        self.stack.pop().expect("popped a resolution that was never pushed").succeeded
    }
}

impl Resolutions<ResolutionTarget> {
    /// Read native signature resolution ownership without publishing anything
    /// or marking a cycle. Parameter-only projection requires exactly one
    /// successful original key; canonical return demand can decline an active
    /// original even when the caller carries an unrelated alias frame.
    pub(crate) fn active_signature_keys(
        &self,
        declaration: NodeId,
    ) -> impl Iterator<Item = (&crate::declared::TypeLiteralKey, bool)> {
        self.observed(self.stack.iter().any(|frame| {
            frame.property == PropertyName::ResolvedReturnType
                && matches!(&frame.target, ResolutionTarget::Signature(key) if key.node == declaration)
        }));
        self.stack.iter().filter_map(move |frame| match &frame.target {
            ResolutionTarget::Signature(key)
                if frame.property == PropertyName::ResolvedReturnType
                    && key.node == declaration =>
            {
                Some((key, frame.succeeded))
            }
            _ => None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_resolution_that_does_not_recurse_succeeds() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.pop());
        assert_eq!(r.depth(), 0, "the stack must be left balanced");
    }

    #[test]
    fn frames_below_the_resolution_start_are_not_cycle_participants() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        let saved = r.reset_start();
        assert!(r.push(0u32, PropertyName::Type), "the outer frame is below the start");
        assert!(!r.push(0u32, PropertyName::Type), "a frame inside the scope still cycles");
        assert!(!r.pop());
        r.restore_start(saved);
        assert!(r.pop(), "the inner cycle did not fail the outer frame");
    }

    #[test]
    fn nested_resolutions_of_different_symbols_both_succeed() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.pop());
        assert!(r.pop());
    }

    #[test]
    fn a_direct_self_reference_is_a_cycle() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(!r.push(0u32, PropertyName::Type), "`const a = a` must not recurse forever");
        assert!(!r.pop(), "the frame it re-entered is now known to have failed");
    }

    #[test]
    fn only_a_reentry_past_a_lazy_construct_is_deferred() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::DeclaredType));
        // `type T = T | string`: the union resolves eagerly natively too.
        assert!(!r.deferred_since(0u32, PropertyName::DeclaredType));
        // `type T = { x: T }`: the literal's members are native's lazy work.
        r.enter_deferred();
        assert!(r.deferred_since(0u32, PropertyName::DeclaredType));
        // A frame pushed inside the literal is re-entered eagerly again.
        assert!(r.push(1u32, PropertyName::DeclaredType));
        assert!(!r.deferred_since(1u32, PropertyName::DeclaredType));
        assert!(r.pop());
        r.exit_deferred();
        assert!(!r.deferred_since(0u32, PropertyName::DeclaredType));
        assert!(r.pop(), "a deferred re-entry fails no frame");
    }

    #[test]
    fn every_participant_in_a_longer_cycle_fails_not_only_the_one_that_closed_it() {
        // `a -> b -> c -> a`. This is the case a per-symbol "in progress" flag
        // gets wrong: it would fail `a` and let `b` and `c` succeed with a
        // half-built answer, which is both incorrect and dependent on which
        // symbol was asked for first.
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::Type));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.push(2u32, PropertyName::Type));
        assert!(!r.push(0u32, PropertyName::Type), "closes the cycle");

        assert!(!r.pop(), "c is part of the cycle");
        assert!(!r.pop(), "b is part of the cycle");
        assert!(!r.pop(), "a is part of the cycle");
    }

    #[test]
    fn a_frame_outside_the_cycle_still_succeeds() {
        // `outer -> a -> b -> a`. `outer` is not a participant: the cycle starts
        // at `a`, so only frames from there on are marked. Marking the whole
        // stack would turn an unrelated enclosing resolution into an error.
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(9u32, PropertyName::Type)); // outer
        assert!(r.push(0u32, PropertyName::Type)); // a
        assert!(r.push(1u32, PropertyName::Type)); // b
        assert!(!r.push(0u32, PropertyName::Type));

        assert!(!r.pop(), "b is in the cycle");
        assert!(!r.pop(), "a is in the cycle");
        assert!(r.pop(), "outer merely contained it");
    }

    #[test]
    fn published_identity_stops_the_scan_but_not_a_newer_actual_cycle() {
        let mut r: Resolutions<u32> = Resolutions::new();
        assert!(r.push(0u32, PropertyName::ResolvedReturnType));
        assert!(r.push(1u32, PropertyName::Type));
        assert!(r.push_with(0u32, PropertyName::ResolvedReturnType, |key, property| {
            *key == 1 && property == PropertyName::Type
        }));
        assert!(r.pop());
        assert!(r.push(2u32, PropertyName::ResolvedReturnType));
        assert!(!r.push_with(2u32, PropertyName::ResolvedReturnType, |key, _| *key == 1));
        assert!(!r.pop(), "the newer return cycle must still fail");
        assert!(r.pop(), "the published identity was outside that cycle");
        assert!(r.pop());
    }
}
