//! The checker's state: what it remembers, and the handles it hands out.
//!
//! Ported from `internal/checker/checker.go`. This module holds the [`Checker`]
//! struct itself and the few helpers that belong to no single concern; the
//! functions that compute types live in one module per concern and add `impl`
//! blocks to this same type:
//!
//! | module | upstream | answers |
//! |---|---|---|
//! | [`crate::expressions`] | `checkExpression` | the type of an expression |
//! | [`crate::binary`] | `checkBinaryLikeExpression` | `a + b`, `a === b`, `a = b` |
//! | [`crate::members`] | `checkPropertyAccessExpression`, `getPropertyOfType` | `a.b` |
//! | [`crate::symbols`] | `getTypeOfSymbol` | the type a *value* symbol has |
//! | [`crate::declared`] | `getTypeFromTypeNode`, `getDeclaredTypeOfSymbol` | what a type node and a *type* symbol denote |
//! | [`crate::literals`] | `getWidenedLiteralType` and its pair | fresh versus regular |
//!
//! The split is by upstream concern rather than by size, so that a reader who
//! knows `checker.go` can find the arm they want; upstream keeps all of this in
//! one 60,269-line file and that is not a shape worth reproducing. The fields
//! below are `pub(crate)` because every one of those modules writes to a memo.

use rustc_hash::FxHashMap;

use tsr_ast::{Node, NodeFlags, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, SymbolFlags, SymbolId};

use crate::{
    intrinsics::Intrinsics,
    printing,
    resolution::{ModuleHost, Resolutions},
    types::{TypeId, TypeStore},
};

/// §79/§80's interning key: (member, optional) pairs, labels, readonly.
pub(crate) type OptionalTupleKey = (Vec<(TypeId, bool)>, Vec<Option<String>>, bool);

/// Computes types.
///
/// **Every method takes `&mut self` and returns [`TypeId`].** No method hands out
/// a reference into checker state, which is what makes lazy memoisation ordinary
/// safe Rust here — see
/// [ADR-0013](../../../docs/adr/0013-checker-memoisation.md). Read a type's
/// contents with [`Checker::type_of`].
#[allow(
    clippy::struct_excessive_bools,
    reason = "upstream's `Checker` holds each strict-option value as its own \
              bool field — sixteen of them run from `checker.go:601` to \
              `checker.go:614`. Grouping \
              them here would make every read spell a path upstream's does not, \
              for a lint whose stated fix — a state machine — does not describe \
              independent compiler options."
)]
pub struct Checker<'a, 'n> {
    pub(crate) symbols: crate::symbol_access::CheckerSymbols<'a>,
    pub(crate) store: TypeStore,
    pub(crate) intrinsics: Intrinsics,
    pub(crate) nodes: &'n NodeTable,
    /// The way back from a `NodeId` to the typed node
    /// ([ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)).
    /// A symbol names its declaration by id; the annotation and initialiser live
    /// in the node.
    pub(crate) node_map: &'n NodeMap<'a>,
    pub(crate) binder: &'a BindResult<'a>,
    /// The program, reduced to the one question the checker cannot answer for
    /// itself: which file a module specifier names.
    ///
    /// Upstream's `c.program` (`internal/checker/checker.go:581`), whose type is
    /// the `Program` interface declared at `checker.go:547` and assigned at
    /// `checker.go:908`. `Option` because upstream's is never absent and this
    /// port's frequently is — see [`Checker::new`] for why that is a
    /// constructor decision rather than a defect.
    ///
    /// **Lifetime `'n`, not `'a`.** `'a` is the arena every node and symbol
    /// borrows from; `'n` is the borrow of the tables. The host is a
    /// `tsr_compiler::Program`, which *borrows* the arena rather than owning it
    /// ([ADR-0034](../../../docs/adr/0034-a-program-needs-one-identity-space.md),
    /// "Who owns the arena"), so its own borrow is the shorter of the two and
    /// tying it to `'a` would demand a program that outlives the arena it
    /// borrows — which no caller can supply.
    pub(crate) module_host: Option<&'n dyn ModuleHost>,
    /// `expression node -> its type`, the memo upstream keeps in `nodeLinks`.
    pub(crate) node_types: FxHashMap<NodeId, TypeId>,
    /// How many times the *worker* has run, as opposed to the memo answering.
    ///
    /// Instrumentation, not state: it exists because the memo is otherwise
    /// unobservable. Interning already makes a repeated literal produce no new
    /// type, so a test that counts types cannot tell a working memo from a
    /// missing one — a no-op test, and this counter is what makes the memo test
    /// actually bite.
    pub(crate) computations: usize,
    /// Fresh literal type -> its widened (regular) form.
    ///
    /// Upstream keeps this as `regularType` on the literal type itself
    /// (`types.go`, `LiteralType.regularType`). A side table here, for the same
    /// reason the binder uses one: the stored type is immutable once created.
    pub(crate) regular_types: FxHashMap<TypeId, TypeId>,
    /// An enum *member* type -> the enum symbol that declares it.
    ///
    /// Upstream needs no such table: `getBaseTypeOfEnumLikeType`
    /// (`checker.go:25470`) reads `t.symbol` off the type and walks to its
    /// parent. [`crate::types::TypeData::Named`] carries no symbol — the whole
    /// point of ADR-0003 is that a back-edge like type -> symbol lives in an
    /// id-keyed side table rather than as a field — so the edge lives here,
    /// keyed by [`TypeId`].
    ///
    /// **The parent hop is collapsed.** Upstream stores the *member's* symbol
    /// and calls `getParentOfSymbol` to reach the enum; this stores the enum
    /// symbol directly, because that is the only thing any reader wants and the
    /// member symbol is already the key's origin. The consequence to accept: a
    /// future caller wanting the member symbol itself cannot get it from here.
    pub(crate) enum_member_owners: FxHashMap<TypeId, SymbolId>,
    /// Every diagnostic the check traversal has reported, paired with the
    /// `SourceFile` node it belongs to.
    ///
    /// Upstream's `c.diagnostics`, an `ast.DiagnosticsCollection` field on the
    /// `Checker` (`internal/checker/checker.go:661`), drained rather than
    /// computed by `GetDiagnostics` (`checker.go:13951`) — ADR-0040 decision
    /// (1). The file is carried alongside because
    /// [`tsr_diagnostics::Diagnostic`] holds only a span, and under ADR-0034 one
    /// `NodeTable` spans every file of a program, so a span alone cannot say
    /// which unit it is an offset into.
    ///
    /// Empty for every checker nobody calls [`Checker::check_source_file`] on,
    /// which is every call site the `checker_types` gradient runs through: the
    /// traversal is a second entry point, never a side effect of a query.
    pub(crate) diagnostics: Vec<(NodeId, tsr_diagnostics::Diagnostic)>,
    /// `symbol -> its type`, upstream's `valueSymbolLinks[symbol].resolvedType`.
    pub(crate) symbol_types: FxHashMap<SymbolId, TypeId>,
    /// Native `InitializerIsUndefinedComputed` / `InitializerIsUndefined`:
    /// parameter default facts, with `true` installed during resolution.
    pub(crate) parameter_initializer_contains_undefined: FxHashMap<NodeId, bool>,
    /// `thisExpandoKinds` / `thisExpandoLocations` (`checker.go`): the
    /// classification `isConstructorDeclaredThisProperty` caches per symbol.
    pub(crate) this_expando_kinds:
        FxHashMap<SymbolId, crate::assignment_declarations::ThisAssignmentDeclaration<'a>>,
    /// `symbol -> whether any assignment in its container targets it` — the
    /// ever-assigned memo (`checker-notes-narrow.md` §9.7), read by the flow
    /// START arm's outer-reference split.
    pub(crate) symbol_assignment_scan: FxHashMap<SymbolId, bool>,
    /// §736: `symbol -> whether any assignment to it writes an empty array` — the
    /// declaration-side stand-in for the second disjunct of upstream's
    /// `isEmptyArrayAssignment` (`flow.go:283`), which this port must answer
    /// before the flow walk starts rather than per flow node. See
    /// [`Checker::is_auto_array_declaration`].
    pub(crate) symbol_empty_array_assignment_scan: FxHashMap<SymbolId, bool>,
    /// Last assignment position per parameter/mutable-local, `i64::MAX` for
    /// nested-function assignments — upstream's `markedAssignmentSymbolLinks`
    /// (`flow.go:2668` family). 0 means "never assigned".
    pub(crate) last_assignment_pos: FxHashMap<SymbolId, i64>,
    /// The regular form of a FRESH enum member type — upstream's
    /// `freshType`/`regularType` back-link (`types.go`), which interning
    /// cannot reproduce for `new_named` ids (`checker-notes-narrow.md` §18).
    pub(crate) enum_member_regular: FxHashMap<TypeId, TypeId>,
    /// One NAMED placeholder per alias symbol, served to mentions of the
    /// alias inside its own resolution (`checker-notes-narrow.md` §29).
    pub(crate) alias_placeholders: FxHashMap<SymbolId, TypeId>,
    /// `aliasSymbolLinks.aliasTarget`, owned by [`Checker::resolve_alias`].
    pub(crate) alias_targets: FxHashMap<SymbolId, crate::symbols::AliasTarget>,
    /// How many `resolve_alias` workers are active (`Resolving` entries).
    pub(crate) alias_resolving: u32,
    /// Completed `extends` base symbols per (owner, refuse type arguments),
    /// owned by `Checker::base_symbols_of_ex`.
    pub(crate) base_symbols: FxHashMap<(SymbolId, bool), Vec<SymbolId>>,
    /// Completed declared call/construct signatures of an interface or type
    /// literal symbol, owned by `Checker::signature_candidates_of_interface_symbol`.
    pub(crate) interface_signatures: FxHashMap<
        (crate::symbol_access::SymbolRef, crate::signatures::SignatureKind),
        Vec<crate::signatures::Signature>,
    >,
    /// Per-file memo: does the file contain import/export machinery? The
    /// §31 gate (`checker-notes-narrow.md`).
    pub(crate) file_import_machinery: FxHashMap<NodeId, bool>,
    /// The memoized `typeof globalThis` type (`checker-notes-narrow.md` §33).
    pub(crate) global_this_type: Option<TypeId>,
    /// One `unique symbol` per WRITTEN `unique symbol` type node
    /// (`checker-notes-callres.md` §27).
    pub(crate) unique_symbol_nodes: FxHashMap<NodeId, TypeId>,
    /// `uniqueESSymbolTypes` (checker.go:22982): one `unique symbol` per
    /// declaration symbol (`crate::unique_symbols`).
    pub(crate) unique_es_symbol_types: FxHashMap<SymbolId, TypeId>,
    /// One `this` type per class/interface declaration — upstream's
    /// `d.thisType` for TYPE-POSITION `this` (`checker-notes-callres.md`
    /// §28).
    pub(crate) this_type_nodes: FxHashMap<NodeId, TypeId>,
    /// One members-carrying qualified reference type per (namespace-site
    /// spelling, target symbol) — `checker-notes-narrow.md` §41.
    pub(crate) qualified_reference_types: FxHashMap<(String, SymbolId), TypeId>,
    /// The GENERIC half of [`Self::qualified_reference_types`], keyed by the
    /// type-argument identities as well as the printed text. Upstream's
    /// `createTypeReferenceEx` (`checker.go:25107`) caches instantiations by
    /// `getTypeListKey(typeArguments)`; two same-spelled but distinct type
    /// parameters (`Promise.Thenable<R>` under a class `R` and under a method's
    /// shadowing `R`) are two references. `checker-99-shadowed-names.md`.
    pub(crate) qualified_generic_reference_types:
        FxHashMap<(String, SymbolId, Vec<TypeId>), TypeId>,
    /// §926: the WRITTEN spelling of a qualified type reference whose printed
    /// name [`Checker::qualification_free_name`] shortened, keyed by the
    /// reference node.
    ///
    /// Upstream prints the same reference two ways and the corpus shows both on
    /// adjacent rows: `param : publicClass` for the parameter's own type, and
    /// `myMethod : (param: privateModule.publicClass) => void` for the signature
    /// containing it. The standalone print goes through `symbolToString`, which
    /// emits the shortest accessible name; the signature print goes through
    /// `serializeTypeForDeclaration`, which **reuses the written annotation
    /// node**. This map is the reuse half — `written_annotation_text` consults
    /// it so a shortened name keeps its written spelling inside a signature.
    pub(crate) qualified_written_text: FxHashMap<tsr_ast::NodeId, String>,
    /// Function/source-file roots whose assignments have been marked —
    /// `NodeCheckFlagsAssignmentsMarked`.
    pub(crate) assignments_marked: rustc_hash::FxHashSet<NodeId>,
    /// Symbols the assignment walk saw in a **definite** assignment position —
    /// upstream's `markedAssignmentSymbolLinks[symbol].hasDefiniteAssignment`
    /// (`flow.go:2657`), the second field on the links
    /// [`Checker::last_assignment_pos`] is the first of.
    ///
    /// Written by the same walk and read only by `isSymbolAssignedDefinitely`,
    /// whose sole consumer is TS2454's `isNeverInitialized`
    /// (`checker-notes-diag2.md` §42). Kept as its own set rather than folded
    /// into the position map because the two are written under **different**
    /// conditions: the position is skipped once it reads `i64::MAX`, the flag
    /// never is.
    pub(crate) definitely_assigned: rustc_hash::FxHashSet<SymbolId>,
    /// Converged loop-label types, keyed by (flow node, reference key).
    /// Upstream's `flowLoopCache` (`internal/checker/flow.go:1325` family).
    ///
    /// §739: the second field is the label's evolving-array ELEMENT
    /// contribution — the `array_elements` slice accumulated while computing
    /// the label. Upstream needs no such field because its cached value on the
    /// array track IS an evolving array type carrying its element union; this
    /// port's unfinalized stand-in (`state.declared_type`) carries nothing, so
    /// a bare-`TypeId` cache hit skipped the antecedent walk AND its
    /// accumulation, and every query after the first finalised over an empty
    /// list (`controlFlowArrays` f10, `want=(string | number)[] got=any[]`).
    /// A hit replays the slice into the querying state; empty off the array
    /// track.
    pub(crate) flow_loop_cache: FxHashMap<(usize, u64, TypeId), (TypeId, Vec<TypeId>)>,
    /// `isReachableFlowNode`'s cache over SHARED flow nodes — upstream's
    /// `flowNodeReachable` (`checker.go`, keyed by `*ast.FlowNode`). §743.
    pub(crate) flow_node_reachable: FxHashMap<usize, bool>,
    /// In-process loop-label computations with their so-far unions —
    /// upstream's `flowLoopKeys`/`flowLoopTypes` stacks. Non-empty means the
    /// checker is in a transient fixpoint pass, and `check_expression` must
    /// not persist results (`checker-notes-narrow.md` §12.6).
    pub(crate) flow_loop_stack: Vec<((usize, u64, TypeId), Vec<TypeId>)>,
    /// `(generic symbol, type arguments) -> the instantiated reference`,
    /// upstream's `d.instantiations` keyed by `getTypeListKey`
    /// (`checker.go:17342`).
    ///
    /// Identity matters even though nothing yet looks inside one of these types:
    /// `C<number>` written twice must be one type, or the first relation check
    /// written will compare two handles that should have been equal.
    pub(crate) instantiations: FxHashMap<(SymbolId, Vec<TypeId>), TypeId>,
    /// The reverse of [`Checker::instantiations`]: `the instantiated reference
    /// -> (generic symbol, type arguments)`.
    ///
    /// Upstream needs no such table because a `TypeReference` *carries* its
    /// target and its `resolvedTypeArguments`
    /// (`internal/checker/types.go`), so `instantiateType` reads the pair
    /// straight off the type. Here the pair is the intern map's **key**, which
    /// makes it unreachable from a [`TypeId`] — and substitution is exactly the
    /// operation that has a `TypeId` and needs the pair. Writing it into a side
    /// table keyed by id is the ADR-0003 move, and it costs one insert per
    /// *distinct* reference rather than per lookup, because
    /// [`crate::declared`]'s `create_type_reference` returns early on a hit.
    ///
    /// The entry is the whole of what substitution can rebuild. A tuple or a
    /// function type is not interned on a `(symbol, arguments)` pair at all, so
    /// it has no entry and no rebuild — see
    /// [`Checker::instantiate_type`](crate::Checker::instantiate_type).
    pub(crate) type_reference_targets: FxHashMap<TypeId, (SymbolId, Vec<TypeId>)>,
    /// ADR-0045's alias attribute, first writer: upstream's `Type.alias`
    /// (`symbol` + `typeArguments`) for a type built by an alias-accepting
    /// constructor. Written once, at creation, by
    /// [`Checker::deferred_alias_reference`](crate::Checker) — the
    /// `createDeferredTypeReference` arm of `isDeferredTypeReferenceNode`
    /// (`checker.go:23236`) — and read only by the printer
    /// ([`Checker::type_to_string_at`]), mirroring the node builder's alias
    /// arm (`nodebuilderimpl.go:3362`). Relations, members and inference never
    /// read it. An absent entry means "no alias", a completed answer.
    pub(crate) alias_of: FxHashMap<TypeId, (SymbolId, Vec<TypeId>)>,
    /// Intern table for those alias-carrying references, keyed by
    /// `(alias symbol, the reference the alias is put on)`: upstream caches the
    /// deferred reference per alias-body node, and the body is resolved once
    /// per alias, so one alias over one reference is one type.
    pub(crate) deferred_alias_references: FxHashMap<(SymbolId, TypeId), TypeId>,
    /// §136 (printseam §6): the WRITTEN arity of a default-filled reference —
    /// prints show this many leading arguments, matching upstream's
    /// written-annotation reuse (`Iterable<number>` written short prints
    /// short; `Generator<Y, any, any>` written full prints full). Absent for
    /// fully-written references. Propagated through instantiation rebuilds.
    pub(crate) reference_display_arity: FxHashMap<TypeId, usize>,
    pub(crate) literal_this_types: FxHashMap<NodeId, TypeId>,
    /// Types minted for a type reference whose **name does not resolve**.
    ///
    /// Upstream mints one `errorType` per unresolved alias key, carrying an
    /// alias symbol named after the entity, so the printer writes `NodeType`
    /// rather than `any` (`checker.go:23580`, `bd tsr-eep`). This port's
    /// equivalent is a [`crate::types::TypeData::Named`] carrying the written
    /// text — but it must still answer [`Checker::is_error`], or the
    /// arithmetic and `+` arms would stop propagating and start claiming `any`
    /// for an operand nobody could resolve.
    ///
    /// A set rather than a flag on the type because `is_error` is *identity*
    /// throughout this crate, deliberately: `errorType` and `anyType` both
    /// carry `TypeFlags::ANY` and every guard here tests the identity so the
    /// two stay apart.
    pub(crate) unresolved_types: rustc_hash::FxHashSet<TypeId>,
    /// §786: the deferred `keyof T` mints of §35, kept apart from the rest of
    /// [`Checker::unresolved_types`] because they are the only members of that
    /// set that are GENERIC. `isGenericIndexType` needs that distinction and
    /// the printed text is not a sound way to recover it.
    pub(crate) deferred_keyof_types: rustc_hash::FxHashSet<TypeId>,
    /// The operand of a deferred `IndexType`, for substitution (types.go).
    pub(crate) deferred_keyof_operands: FxHashMap<TypeId, TypeId>,
    /// Active semantic index queries. Unresolved circular alias bodies must
    /// remain a gap rather than recursively expanding their own key sets.
    pub(crate) index_types_in_progress: rustc_hash::FxHashSet<TypeId>,
    /// §813: the DEFERRED `keyof X` / `X[Y]` mints, as a set of their own.
    ///
    /// A subset of [`Self::unresolved_types`] rather than a new kind of type.
    /// It exists because **one consumer must treat these like a type variable
    /// and the rest must not**: `getAdjustedTypeWithFacts`
    /// (`checker.go:31159`) narrows a possibly-nullable operand by
    /// INTERSECTION, and `removeNullableByIntersection` (`:31179`) gates on
    /// nothing but the operand's *facts* — a deferred `T[K]` is intersected
    /// there exactly as `T` is, so `T[K] | undefined` narrows to
    /// `T[K] & ({} | undefined)` and not to a bare `T[K]`.
    ///
    /// Distinct from [`Self::deferred_keyof_types`], which answers a different
    /// question (*is this mint usable as a generic index?*, §786) and holds only
    /// the `keyof` half. Testing `unresolved_types` instead would be too wide:
    /// the §31 type-reference mint is in it too, and upstream has a real,
    /// resolved type for `Foo<T>` whose facts it can actually read.
    pub(crate) deferred_index_mints: rustc_hash::FxHashSet<TypeId>,
    /// Structured deferred indexed accesses, matching `IndexedAccessType`
    /// (`internal/checker/types.go`), with the persistent `IncludeUndefined` bit.
    pub(crate) deferred_indexed_access_types: FxHashMap<TypeId, (TypeId, TypeId, bool)>,
    /// `getIndexedAccessTypeOrUndefined`'s interning key (`checker.go`).
    pub(crate) deferred_indexed_access_cache: FxHashMap<(TypeId, TypeId, bool), TypeId>,
    /// A class symbol to its `this` type, upstream's `d.thisType`
    /// (`checker.go:17334`). One per class, so `this` has a stable identity
    /// inside one.
    pub(crate) this_types: FxHashMap<SymbolId, TypeId>,
    /// `ObjectFlagsIsConstrainedTypeVariable`: intersection -> (variable, primitive).
    pub(crate) constrained_type_variables: FxHashMap<TypeId, (TypeId, TypeId)>,
    /// `symbol -> the type it *declares*`, upstream's
    /// `declaredTypeLinks[symbol].declaredType`.
    ///
    /// Separate from [`Checker::symbol_types`] because they are different
    /// questions about the same symbol: a class `C` declares the instance type
    /// `C` and *has* the type `typeof C`. Merging them would answer one with the
    /// other.
    pub(crate) declared_types: FxHashMap<SymbolId, TypeId>,
    /// In-progress resolutions, for circularity detection.
    pub(crate) resolutions: Resolutions<crate::resolution::ResolutionTarget>,
    /// Pinned `NodeCheckFlagsInCheckIdentifier` (checker.go:13766): immediate
    /// binding holder `NodeId`s whose parent/constraint lookup is active in
    /// this private Checker. No result or completeness is published; re-entry
    /// falls back to ordinary symbol/flow checking. Cleared before narrowing,
    /// including unsupported parent lookup, so flow can re-enter independently.
    pub(crate) dependent_binding_parents_in_flight: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// Whether control-flow analysis has given up in the current container.
    ///
    /// Upstream's `c.flowAnalysisDisabled` (`checker.go:801`). Set when
    /// `getTypeAtFlowNode` reaches 2,000 recursive invocations, after which
    /// every `getFlowTypeOfReference` in the rest of the containing function or
    /// module body answers `errorType`. It is **state, not a guard**: the limit
    /// changes what a deep function answers rather than only protecting the
    /// stack. See [`crate::flow`].
    pub(crate) flow_analysis_disabled: bool,
    /// Containers whose flow analysis tripped the §14 too-large bail —
    /// upstream's `flowAnalysisDisabled`, whose `checkBlock` save/restore
    /// (`checker.go:3791`) scopes the poison to the containing function or
    /// module body. Here the scope is lexical containment of the reference.
    pub(crate) flow_disabled_containers: rustc_hash::FxHashSet<NodeId>,
    /// The per-invocation memo for flow nodes with more than one antecedent.
    ///
    /// Upstream's `c.sharedFlows` (`checker.go:799`), a stack that each
    /// `getFlowTypeOfReference` truncates back to its own start. Without it the
    /// backwards walk is exponential on branchy code — a hang rather than a
    /// wrong answer, which is why it is part of the port and not an
    /// optimisation.
    pub(crate) shared_flows: crate::perf_links::SharedFlows,
    /// The global table's `ALIAS`-flagged entries in its iteration order
    /// ([`Checker::global_alias_entries`], `r5-checkperf.md` §7).
    global_alias_entries: Option<std::rc::Rc<[(&'a str, SymbolId)]>>,
    /// §52's reentrancy guard: operand nodes currently being typed FROM
    /// INSIDE an equality-narrowing walk. Typing `value` can re-enter the
    /// same reference's flow walk through the operand's own narrowing and
    /// loop; a node already on this stack answers `t` unchanged instead.
    pub(crate) narrow_value_stack: std::collections::HashSet<tsr_ast::NodeId>,
    /// callres2 reunion: the pass-1 partially-instantiated candidate, keyed
    /// by the CALL node — upstream's `getResolvedSignature` cache reduced to
    /// the inference window (`checker-notes-callres2.md`, the reunion
    /// design). `contextual_type_for_argument` consults it FIRST, which is
    /// what lets a context-sensitive callback see instantiated parameter
    /// types instead of erroring on the circularity.
    pub(crate) call_inference_signatures:
        rustc_hash::FxHashMap<tsr_ast::NodeId, crate::signatures::Signature>,
    /// Completed `signatureLinks.resolvedSignature` (`getResolvedSignature`,
    /// internal/checker/checker.go), currently retained for calls with context-
    /// sensitive arguments. Later contextual reads use the selected signature.
    pub(crate) resolved_call_signatures:
        rustc_hash::FxHashMap<tsr_ast::NodeId, crate::signatures::Signature>,
    /// Native 5b1047d `signatureLinks.effectsSignature == unknownSignature`.
    /// Private Checker / call `NodeId` completion, currently admitted only after
    /// materializing explicitly annotated, noncontextual signatures with no
    /// predicate or never effect. Absence includes deferred and inferred work;
    /// no active/provisional `None` is published here. See
    /// docs/architecture/checker-effects-completion.md.
    pub(crate) completed_no_effects_calls: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// `CallState.candidatesForArgumentError` (`checker.go:8838`) as the
    /// final (assignable) pass of the §487 overload walk left it when both
    /// passes rejected every candidate, keyed by the CALL node. Owner:
    /// `calls.rs` (`transcribed_generic_set_walk`, the only writer, which
    /// removes the entry when it picks or declines);
    /// `reportCallResolutionErrors` (`checker.go:9649`) on the diagnostics
    /// road is the reader. See [`crate::calls::OverloadArgumentFailure`].
    pub(crate) overload_argument_failures:
        rustc_hash::FxHashMap<tsr_ast::NodeId, crate::calls::OverloadArgumentFailure>,
    /// Calls currently serving contextual signatures containing type parameters
    /// propagated from a generic argument (`instantiateTypeWithSingleGenericCallSignature`,
    /// internal/checker/checker.go).
    pub(crate) higher_order_context_calls: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// §469: the signature-links table, reduced to the ONE bit upstream reads
    /// on the contextual road — whether a call's signature resolution is in
    /// flight. Upstream parks `resolvingSignature` in `signatureLinks` before
    /// resolving (`checker.go:8427`) and `getContextualTypeForArgumentAtIndex`
    /// answers that sentinel instead of re-resolving (`checker.go:29785`);
    /// `getTypeAtPosition` on the parameterless sentinel is `anyType` at every
    /// index (`relater.go:1757`). This is the cycle-breaker §445's refusal
    /// named as its reopening condition: a call argument's contextual
    /// signature resolves the callee, and the callee's own type computation
    /// can re-enter the same call's contextual road — which overflowed the
    /// stack when return inference joined that road. A call node present here
    /// answers `any` on re-entry rather than resolving again.
    pub(crate) resolving_signature_calls: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// `NodeCheckFlagsContextChecked` (`checker.go:10155`,
    /// `contextuallyCheckFunctionExpressionOrObjectLiteralMethod`) for the
    /// context-sensitive arguments of the overload walk in progress: an
    /// argument here was already checked under an earlier candidate of its
    /// call, so its parameter and return types are fixed and inference reads
    /// its published type instead of evicting and re-checking it. Keyed by
    /// the argument node; owner `calls.rs` `transcribed_generic_set_walk`,
    /// which inserts after a generic candidate's inference checked the
    /// argument and removes its arguments when the walk ends. No work happens
    /// on publication.
    pub(crate) context_checked_arguments: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// Active synchronous iterable resolution, guarding recursive protocols.
    pub(crate) resolving_iteration_types: rustc_hash::FxHashSet<TypeId>,
    /// `getResolvedMembersOrExportsOfSymbol` / `lateBindMember`: semantic names,
    /// with an empty entry installed while resolving to break recursive keys.
    pub(crate) late_bound_member_names:
        rustc_hash::FxHashMap<(SymbolId, bool), Vec<(String, tsr_ast::NodeId)>>,
    /// §469's other half of the signature-links table: DECLARATIONS whose
    /// inferred return type is currently consulting the contextual road.
    /// Upstream's `signatureLinks` is keyed per NODE and serves both the
    /// call side (`resolvedSignature`, the set above) and the declaration
    /// side (`getSignatureFromDeclaration`'s cache), so a function
    /// expression's signature materializes once and a cyclic re-query hits
    /// the links. This port rebuilds signatures per query, so the
    /// intra-expression road — whose pass-1 memo hands back a parameter type
    /// containing the argument literal's OWN members — could re-enter the
    /// same declaration's return inference unboundedly (measured: one
    /// declaration at every depth to the cap, `intraExpressionInferences`).
    /// A declaration in flight declines the contextual consult and keeps the
    /// pre-§469 answer, widening.
    pub(crate) contextual_return_in_flight: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// The resolved contextual `this` slot of a completed object method,
    /// corresponding to assignParameterType's symbol links (checker.go).
    /// An absent slot is resolved absence, distinct from an unchecked method.
    pub(crate) contextual_this_parameters: FxHashMap<NodeId, Option<TypeId>>,
    /// §469's stack budget: how many contextual-return consults are nested
    /// RIGHT NOW, cycles aside. The two parks above break true cycles; this
    /// bounds genuine nesting, because each consult's subtree re-runs call
    /// resolution unmemoized and ~20 distinct arrows nesting through it sat
    /// AT the 8 MiB worker budget — `intraExpressionInferences` passed or
    /// overflowed on environment jitter alone. Same policy as the binder's
    /// and printer's `MAX_DEPTH`: the walk carries its own bound rather than
    /// the harness growing a stack nobody else has. Productive depth measured
    /// ≤ 9 corpus-wide (the §469 histogram); the bound is 16.
    pub(crate) contextual_return_depth: u32,
    /// SS135: while a deferred OBJECT-LITERAL argument re-checks under the
    /// serve memo, its node id maps to the pass-1 substitution so the
    /// property road can serve MEMBER types instantiated - the object
    /// parameter type itself is symbol-backed and cannot be instantiated
    /// structurally, so the substitution applies at the property read
    /// instead (`inferFromIntraExpressionSites`, `inference.go:1285`).
    #[allow(clippy::type_complexity)]
    // the other workstream's map; factoring it is theirs to do. §589
    pub(crate) intra_expression_member_maps: rustc_hash::FxHashMap<
        tsr_ast::NodeId,
        (Vec<(crate::types::TypeId, crate::types::TypeId)>, Vec<crate::types::TypeId>, Vec<String>),
    >,
    /// assignContextualParameterTypes reads the call's mapper at the signature,
    /// after contextual object templates have supplied their member shape.
    #[allow(clippy::type_complexity)]
    pub(crate) contextual_signature_mappers:
        FxHashMap<NodeId, (Vec<(TypeId, TypeId)>, Vec<TypeId>, Vec<String>)>,
    /// §52's operand memo — upstream's `getTypeOfExpression` is CACHED, and
    /// without the cache every equality re-types its operand, each typing
    /// re-entering other references' walks: exponential on condition
    /// chains (`compiler/con*` hung the corpus run, caught by `sample`
    /// showing the `narrow_type_by_equality` ↔ `get_type_at_flow_node`
    /// spin).
    pub(crate) narrow_value_types: rustc_hash::FxHashMap<tsr_ast::NodeId, crate::types::TypeId>,
    /// §53 (`checker-notes-narrow.md`): a union's ORIGIN entry list — the
    /// unexpanded inputs it was built from (named unions kept whole). The
    /// union's text renders from these; its constituent list stays the
    /// flattened members; narrowing filters PROJECT them.
    pub(crate) union_origin: rustc_hash::FxHashMap<crate::types::TypeId, Vec<crate::types::TypeId>>,
    /// §55: value-keyed enum literal interning — `getEnumLiteralType`
    /// (`checker.go:25362`). Key: (enum symbol, folded value's canonical
    /// string). `enum E { A, B = A }` gives both members ONE type.
    pub(crate) enum_value_types:
        rustc_hash::FxHashMap<(tsr_binder::SymbolId, String), crate::types::TypeId>,
    /// §55.1's spelling split, single-member enums only: the member's FRESH
    /// form prints per-name (`E.A`, declaration lines) while ACCESS
    /// expressions print the enum (`Enum.A : Enum`). Fresh id → the
    /// enum-spelled regular the access road answers instead.
    pub(crate) enum_access_spelling:
        rustc_hash::FxHashMap<crate::types::TypeId, crate::types::TypeId>,
    /// §58: sorted member list of every NAMED union → the named type, for
    /// the flow-JOIN rebuild only (annotation positions keep expansions —
    /// the §52.1 study's split).
    pub(crate) named_union_by_members:
        rustc_hash::FxHashMap<Vec<crate::types::TypeId>, crate::types::TypeId>,

    /// Upstream's `c.strictNullChecks` (`checker.go:604`, set from the
    /// compiler options at `:919` via `GetStrictOptionValue`).
    ///
    /// **Defaults to `true`**, which preserves every existing construction
    /// site bit-for-bit — the whole crate was written assuming it on. The
    /// conformance harness overrides it per case from the `@strict` /
    /// `@strictNullChecks` directives ([`Checker::set_strict_null_checks`]),
    /// because upstream's test runner leaves strict mode off unless a case
    /// asks for it, and a union built under the wrong mode prints the wrong
    /// constituents (`checker.go:25783`; `crate::unions`).
    ///
    /// Only the union constructor consults it so far. Every other
    /// `strictNullChecks` branch upstream has (the `&&` arm's widening,
    /// optionality's added `undefined`, `unknown` narrowing) still assumes
    /// **on**; each is a separately measurable change and `bd tsr-e10` names
    /// the optionality one.
    pub(crate) strict_null_checks: bool,

    /// `strictFunctionTypes`, used by `compareSignaturesRelated`
    /// (`internal/checker/relater.go`) for parameter variance.
    pub(crate) strict_function_types: bool,
    /// Strict-family selection for the `BuiltinIteratorReturn` intrinsic.
    pub(crate) strict_builtin_iterator_return: bool,
    /// Current structural inference direction (`InferenceState`,
    /// internal/checker/inference.go), restored around each entry walk.
    pub(crate) inference_contravariant: bool,
    pub(crate) inference_bivariant: bool,
    /// Current `InferenceState.priority` (`internal/checker/inference.go`).
    pub(crate) inference_priority: crate::inference::InferencePriority,
    /// Lowest priority reached by this inference walk (`InferenceState`'s
    /// inferencePriority); -1 records an incomplete recursive walk.
    pub(crate) inference_observed_priority: i32,
    /// invokeOnce's pair status for the current inference entry (inference.go).
    pub(crate) inference_visited_pairs: FxHashMap<(TypeId, TypeId), i32>,
    pub(crate) inference_source_stack: Vec<TypeId>,
    pub(crate) inference_target_stack: Vec<TypeId>,
    pub(crate) inference_expanding: (bool, bool),
    /// Active `InferenceContext` snapshots for nested call return inference.
    pub(crate) active_inference_contexts:
        FxHashMap<NodeId, crate::inference::InferenceContextSnapshot>,
    /// `silentNeverType`, the `NoDefault` mapper's non-inferrable wildcard.
    pub(crate) silent_never_type: Option<TypeId>,
    /// `varianceLinks`, the recursion sentinel, and synthetic marker types
    /// used by `getVariancesWorker` (internal/checker/relater.go).
    pub(crate) variance_cache: FxHashMap<SymbolId, Option<Vec<crate::variances::Variance>>>,
    pub(crate) variance_in_progress: rustc_hash::FxHashSet<SymbolId>,
    /// Native `Relation.results` for every relation kind, for the checker's
    /// lifetime (`tsr-2zk.902`); see [`crate::relation_cache`].
    pub(crate) relation_results: crate::relation_cache::RelationResults,
    /// `isDiscriminantProperty`'s answer per `(union, property name)`, the
    /// port's stand-in for native's `CheckFlagsIsDiscriminant` on the
    /// synthetic union property; see `Checker::is_discriminant_property`.
    pub(crate) discriminant_properties: FxHashMap<TypeId, FxHashMap<Box<str>, bool>>,
    pub(crate) variance_markers: Option<[TypeId; 3]>,
    pub(crate) variance_marker_types: rustc_hash::FxHashSet<TypeId>,
    /// Contextual signature instantiations and their recursion sentinel,
    /// corresponding to cached signatures in checker.go.
    pub(crate) signature_context_cache:
        FxHashMap<crate::inference::SignatureContextKey, crate::signatures::Signature>,
    pub(crate) signature_context_in_progress:
        rustc_hash::FxHashSet<crate::inference::SignatureContextKey>,

    /// `strictBindCallApply` (`checker.go:919-926`), read by the property
    /// fallback chain in [`crate::members`] and nothing else.
    ///
    /// It decides what `getGlobalStrictFunctionType` answers:
    ///
    /// ```go
    /// func (c *Checker) getGlobalStrictFunctionType(name string) *Type {
    ///     if c.strictBindCallApply {
    ///         return c.getGlobalType(name, 0 /*arity*/, true /*reportErrors*/)
    ///     }
    ///     return c.globalFunctionType
    /// }
    /// ```
    ///
    /// With the flag off, `globalCallableFunctionType` **is**
    /// `globalFunctionType`, so `f.bind` resolves to `Function.bind` and
    /// relating a function type to `Function` is identity. Resolving it to
    /// `CallableFunction`'s generic `this`-parameter overloads instead asks the
    /// relater a structurally hard question it answers `false`, which is what
    /// produced the corpus's `ElementRef & Function` rows
    /// (`docs/architecture/checker-notes-deferred.md` §846).
    pub(crate) strict_bind_call_apply: bool,
    /// `noImplicitThis`, the fifth member of the strict family — the four
    /// beside it were ported when their consumers arrived, and TS2683 is this
    /// one's. §416.
    pub(crate) no_implicit_this: bool,
    /// `getEmitModuleKind` — `options.module`, or ES2015 for an ES2015-or-later
    /// target and `CommonJS` otherwise. §478.
    pub(crate) module_kind: tsr_core::ModuleKind,
    /// The `ReportLikelyUnsafeImportRequiredError` sink
    /// (`crate::module_specifiers::UnsafeImport`): `Some` only while
    /// declaration emit serializes an inferred type. r5-modules §6.
    pub(crate) unsafe_import_tracker: Option<Vec<crate::module_specifiers::UnsafeImport>>,
    /// `c.legacyDecorators` — `experimentalDecorators` is on. §644.
    pub(crate) legacy_decorators: bool,
    /// `compilerOptions.ImportHelpers.IsTrue()`, read by
    /// `checkExternalEmitHelpers` (`crate::emit_helpers`).
    pub(crate) import_helpers: bool,
    /// `sourceFileLinks`' `externalHelpersModule` and
    /// `requestedExternalEmitHelpers`, keyed by source file
    /// (`crate::emit_helpers` documents ownership).
    pub(crate) external_helpers: FxHashMap<NodeId, crate::emit_helpers::ExternalHelpersLinks>,
    /// `emitStandardClassFields` — `useDefineForClassFields` with upstream's
    /// `target >= ES2022` default. A derived field shadows its base at
    /// construction time only under `[[Define]]` semantics, which is what
    /// TS2729's ancestor guard turns on. §751.
    pub(crate) standard_class_fields: bool,
    /// `getAllowSyntheticDefaultImports` (§132): the explicit option, else
    /// `esModuleInterop`, else `module == System`. Consulted by the §131/§132
    /// deliberate-error arms only; a `true` keeps those gaps honest.
    pub(crate) allow_synthetic_defaults: bool,
    /// §143 slice 2: `allowImportingTsExtensions` — extension-keeping
    /// import-specifier spellings; the relative-spelling arm declines when
    /// set.
    pub(crate) allow_importing_ts_extensions: bool,
    /// `compilerOptions.noUncheckedSideEffectImports`, read through upstream's
    /// `IsTrueOrUnknown` (`checker.go:5321`) — so the default here is `true`,
    /// matching an *unset* option rather than a `false` one.
    ///
    /// Consulted by exactly one rule ([`crate::check`]'s side-effect import
    /// arm). It is a field rather than a `CompilerOptions` read for the same
    /// reason `strict_null_checks` is: this port has one call site that knows a
    /// case's directives, and widening `CompilerOptions` for a single consumer
    /// is what the `strict_null_checks` comment declines to do.
    pub(crate) no_unchecked_side_effect_imports: bool,
    /// `@noUncheckedIndexedAccess`: an index-signature access adds
    /// `| undefined` (`checker-notes-narrow.md` §17's fired leg).
    pub(crate) no_unchecked_indexed_access: bool,
    /// `@useUnknownInCatchVariables` (defaults to `strict`): an
    /// annotation-less catch variable is `unknown`
    /// (`checker-notes-narrow.md` §21).
    pub(crate) use_unknown_in_catch_variables: bool,

    /// `compilerOptions.strictPropertyInitialization` through
    /// `GetStrictOptionValue` (`checker.go:922`) — so it follows `strict` when
    /// unset, exactly as [`Checker::strict_null_checks`] does.
    ///
    /// Held apart from `strict_null_checks` even though the two default
    /// together, because a case can write one `false` and leave the other on and
    /// the corpus contains such cases. Collapsing them would over-report every
    /// one of them.
    pub(crate) strict_property_initialization: bool,
    /// `compilerOptions.NoImplicitOverride.IsTrue()`, read by
    /// `checkMemberForOverrideModifier` (`checker.go:4729`).
    pub(crate) no_implicit_override: bool,
    /// `compilerOptions.NoImplicitReturns == TSTrue`, read by
    /// `checkAllCodePathsInNonVoidFunctionReturnOrThrow` (`checker.go:3728`).
    pub(crate) no_implicit_returns: bool,
    /// Did the parser report a diagnostic in the file currently being walked?
    ///
    /// Set by [`Checker::check_source_file`] and read by the rules that cannot
    /// trust a recovered tree. Upstream needs no equivalent: its recovery is the
    /// recovery the baselines were produced from, so a node it built in a broken
    /// file is still the node the diagnostic is about. Here the two parsers
    /// disagree about *what tree a broken file has*, and a rule that reports on
    /// a node one parser invented is reporting about a program the other never
    /// saw. `crate::check` measures what this refusal costs.
    pub(crate) file_has_parse_errors: bool,
    /// Whether [`Checker::report_merge_conflicts`] has already run.
    ///
    /// The binder's cross-file merge conflicts belong to the *program*, not to
    /// any one file, but the only hook the consumer calls is per-file
    /// `check_source_file`. Reporting on the first call and never again is what
    /// makes the set program-scoped without adding a second entry point.
    /// See `docs/architecture/checker-notes-diag2.md` §159.
    pub(crate) merge_conflicts_reported: bool,
    /// Every position `report_assignability_failure` was *asked about*, with
    /// the gate that decided it. Filled only under `TSR_ASSIGN_PROBE`.
    ///
    /// The question it exists for: of the TS2322 lines the corpus wants and
    /// this port does not emit, how many are positions the walk **never
    /// visits** (a missing reporting anchor — this workstream's) versus
    /// positions it visits where the **relation declines** (`checker_types`'
    /// members and relation completeness)? `bd tsr-bxp` blocks on that split.
    /// See `docs/architecture/checker-notes-diag2.md` §172.
    pub assignability_probe: Vec<(NodeId, tsr_core::Span, u8)>,
    /// `compilerOptions.allowUnreachableCode`, read as `IsTrue()`
    /// (`checker.go:12534`) — so unset is `false` and the comma-operator
    /// diagnostic is on by default.
    pub(crate) allow_unreachable_code: bool,
    /// `unreachableCodeIsError` — the option written **explicitly** `false`.
    pub(crate) unreachable_code_is_error: bool,
    /// `CompilerOptions.ShouldPreserveConstEnums()` — see
    /// [`Checker::set_preserve_const_enums`].
    pub(crate) preserve_const_enums: bool,
    /// `CompilerOptions.GetIsolatedModules()` (`isolatedModules` or
    /// `verbatimModuleSyntax`): the enum member reports TS18055/TS18056.
    pub(crate) isolated_modules: bool,
    /// `compilerOptions.IsolatedModules.IsTrue()` itself, which
    /// `checkAliasSymbol`'s TS2865 reads instead of `GetIsolatedModules()`.
    pub(crate) isolated_modules_option: bool,
    /// `compilerOptions.VerbatimModuleSyntax.IsTrue()`.
    pub(crate) verbatim_module_syntax: bool,
    /// The options `module_format.rs` reads (`tsr-2zk.985`).
    pub(crate) module_format_options: crate::module_format::ModuleFormatOptions,
    /// `compilerOptions.noUnusedLocals`, read as `IsTrue()`
    /// (`checker.go:7107`) — unset is `false`, which is what keeps the whole
    /// unused-identifier family off for every case that does not ask for it.
    /// `compilerOptions.noImplicitAny` through `GetStrictOptionValue`
    /// (`checker.go:922`) — it follows `strict` when unset, like
    /// `strictNullChecks`.
    /// `switch` statements whose exhaustiveness is being computed right now.
    ///
    /// The `Computing` half of `switchStatementLinks.exhaustiveState`
    /// (`internal/checker/flow.go:1934`) and **only** that half — see
    /// `Checker::is_exhaustive_switch_statement` for why the result is not
    /// cached and what would have to land first.
    pub(crate) exhaustive_switches: rustc_hash::FxHashSet<NodeId>,
    pub(crate) no_implicit_any: bool,
    /// `slices.Contains(c.compilerOptions.Lib, "lib.dom.d.ts")`, read by
    /// `containerSeemsToBeEmptyDomElement` (`checker.go:11654`): the explicit
    /// `lib` list names the DOM lib (`"dom"` maps to `lib.dom.d.ts` in
    /// `tsoptions.LibMap`; both spellings, case-insensitively).
    pub(crate) lib_includes_dom: bool,
    /// `c.compilerOptions.UsesWildcardTypes()` (`core/compileroptions.go:326`):
    /// `types` contains `"*"`. Upstream then reports TS2580 rather than
    /// TS2591 for an unresolved Node core module
    /// (`getCannotResolveModuleNameErrorForSpecificModule`); this port
    /// declines instead (`module_specifier_unfindable_for_diagnostics`).
    pub(crate) uses_wildcard_types: bool,
    /// Object-literal types created in a JS file — upstream's
    /// `ObjectFlagsJSLiteral` (`utilities.go:1753`), carried in a side table
    /// per ADR-0003 rather than widening `TypeData`. Read by the element
    /// access lookup's failure path, which answers `any` for them.
    pub(crate) js_literal_types: rustc_hash::FxHashSet<crate::types::TypeId>,
    /// Object-literal types as minted by `check_object_literal` — upstream's
    /// `ObjectFlagsFreshLiteral`, carried in a side table per ADR-0003. Read
    /// by the subtype reduction's excess-property gate (`hasExcessProperties`,
    /// `relater.go:2667`): a fresh literal with a property its removal target
    /// lacks fails every relation, which is what keeps
    /// `[{id:1}, {id:2, name:"foo"}]` a two-constituent union. This port
    /// clones regular object types at variable widening sites, preserving the
    /// fresh expression identity for excess-property checks.
    pub(crate) fresh_object_literal_types: rustc_hash::FxHashSet<crate::types::TypeId>,
    /// getRegularTypeOfObjectLiteral (checker.go:28159), memoized per fresh type.
    pub(crate) regular_object_literal_types: FxHashMap<TypeId, TypeId>,
    /// `ObjectFlagsObjectLiteral` and `ContainsSpread`, retained by regularization.
    pub(crate) object_literal_spread_flags: FxHashMap<TypeId, bool>,
    /// createArrayLiteralType clones references without marking their shared target.
    pub(crate) array_literal_images: FxHashMap<TypeId, TypeId>,
    pub(crate) array_literal_bases: FxHashMap<TypeId, TypeId>,
    /// getWidenedType's root cache and getUndefinedProperty's name cache.
    pub(crate) widened_object_types: FxHashMap<TypeId, TypeId>,
    /// Reverse of `regular_object_literal_types` and `widened_object_types`
    /// for entries whose original precedes its target: target -> originals.
    /// Maintained by `Checker::record_object_type_transfer`; read only by the
    /// printer's walk back to a fresh literal (`printing.rs`).
    pub(crate) object_type_transfer_origins: FxHashMap<TypeId, Vec<(TypeId, bool)>>,
    pub(crate) widening_undefined_properties: FxHashMap<String, crate::widening::WideningProperty>,
    /// The index signature an **object literal** minted, keyed by the type id.
    ///
    /// §539. `check_object_literal` builds a computed-name literal's index
    /// signature (`getObjectLiteralIndexInfo`, `checker.go:19721`) and pushes it
    /// into the members list it *prints* from — so `{ [this.bar()]: 1 }` reads
    /// `{ [x: number]: number; }` correctly. But the minted type is a
    /// `TypeData::Named` over the binder's `__object` symbol, and
    /// [`Checker::get_index_infos_of_type`] recovers index infos from a
    /// symbol's **declarations**. An object literal has no index-signature
    /// declaration, so the lookup found nothing and every
    /// `{ [computed]: v }[k]` answered `errorType` while printing the signature
    /// it could not consult.
    ///
    /// A side table rather than a `TypeData` field, per
    /// [ADR-0003](../../../docs/adr/0003-tree-plus-side-tables.md) and matching
    /// the two tables declared beside it — the information exists only where
    /// the literal was checked, which is exactly the shape a side table is for.
    /// The member NAMES of a pattern-implied object type, keyed by type id.
    ///
    /// §565, and the same shape §539 fixed for object literals. The implied
    /// type of `{m}` is minted as `new_named(OBJECT, "{ m: any; }", None)` —
    /// **no symbol**, because a binding pattern declares none — so
    /// `get_type_of_property_of_type` has no members table to read and every
    /// destructured element gapped: `function t({m}) { return m; }` answered
    /// `error` while `function r([x]) { return x; }` answered `any`, the array
    /// road reading its tuple positionally and the object road having nothing
    /// to read.
    ///
    /// **Name and type.** §565 stored only names, on the reasoning that every
    /// member of a pattern-implied object is `any` by construction
    /// (`getTypeFromObjectBindingPattern`, `checker.go:17938`, *"with no
    /// initializer to infer from"*). §893 admits elements that DO have an
    /// initializer, so that reasoning expired with it and the type has to travel
    /// with the name — otherwise the printed form says `a?: string` while
    /// reading `a` answers `any`, which is §56's "the print road moves WITH the
    /// symbol road" broken in one table.
    pub(crate) pattern_implied_members:
        rustc_hash::FxHashMap<crate::types::TypeId, Vec<(String, crate::types::TypeId)>>,
    /// §800: the MEMBERS an object literal printed, keyed by its minted type.
    ///
    /// The (since removed, tsr-6.25) `spread_members_of` re-derived a member list
    /// from the `__object` symbol, but each member's type came back from
    /// `get_type_of_symbol` —
    /// the DECLARED, widened type. A literal in a const context retained its
    /// literal member types at print time and nothing else remembers them, so
    /// a re-mint through the symbol road silently widens
    /// (`{ readonly a: 1; }` became `{ readonly a: number; }`). ADR-0003.
    pub(crate) object_literal_members: rustc_hash::FxHashMap<TypeId, Vec<crate::objects::Member>>,
    /// Captured semantic members of anonymous objects.
    /// Instantiation maps these `TypeId`s; property reads use the same images.
    pub(crate) anonymous_properties:
        rustc_hash::FxHashMap<TypeId, (Vec<crate::objects::AnonymousProperty>, bool)>,
    /// Module copies have distinct identity, but share their source's property
    /// symbols. Native cloneTypeAsModuleType's originating import/target links.
    pub(crate) module_value_clones: FxHashMap<TypeId, (SymbolId, TypeId)>,
    /// Native `cachedTypes` under `CachedTypeKindDefaultOnlyType` and
    /// `CachedTypeKindSyntheticType` (`checker.go:15632`, `:15646`), keyed by
    /// the module value type. `crate::module_exports`; its convention record
    /// is `docs/parity/notes/r5-modexports.md` §3.
    pub(crate) synthetic_default_types:
        FxHashMap<(crate::module_exports::SyntheticDefaultKind, TypeId), TypeId>,
    pub(crate) instantiated_objects: rustc_hash::FxHashMap<(TypeId, Vec<(TypeId, TypeId)>), TypeId>,
    /// The inverse of [`Self::instantiated_objects`] for a freshly minted
    /// object: its source and mapper. Native keeps `ObjectType.mapper` on the
    /// instantiation, and `CompareTypes` orders two instantiations of one
    /// anonymous symbol by it (`compareTypeMappers`, utilities.go:683).
    /// Written once, by the producer that mints the result.
    pub(crate) instantiated_object_mappers:
        rustc_hash::FxHashMap<TypeId, (TypeId, Vec<(TypeId, TypeId)>)>,
    /// `instantiationExpressionTypes` (checker.go:10667) and its parked reports.
    pub(crate) instantiation_expressions:
        crate::instantiation_expressions::InstantiationExpressionLinks,
    pub(crate) any_function_type: Option<TypeId>,
    /// `ObjectFlagsNonInferrableType` on `SkipContextSensitive` object images.
    pub(crate) non_inferrable_types: rustc_hash::FxHashSet<TypeId>,
    /// IndexInfo.components declaration lists; copies retain this identity.
    pub(crate) index_components: Vec<Vec<tsr_ast::NodeId>>,
    pub(crate) object_literal_index_infos:
        rustc_hash::FxHashMap<crate::types::TypeId, Vec<crate::index_signatures::IndexInfo>>,
    /// The identifier the JSX namespace hangs off, `getJsxNamespace`'s
    /// `c._jsxNamespace` (`internal/checker/jsx.go:1372-1382`).
    ///
    /// **Defaults to `React`**, and that default is load-bearing rather than a
    /// convenience: with a modern `@types/react` the `JSX` namespace lives at
    /// `React.JSX` and there is no global one, so this name is the whole road
    /// to `JSX.IntrinsicElements`. Overridden by the first identifier of
    /// `jsxFactory` (`h` for `h.createElement`), else by `reactNamespace`.
    ///
    /// The per-file `@jsx` pragma, which upstream consults first
    /// (`getLocalJsxNamespace`), comes from the host — see
    /// [`Checker::jsx_namespace_at`].
    pub(crate) jsx_namespace: String,
    /// The first identifier of `jsxFragmentFactory`, when that option parses
    /// as an entity name (`getJsxFragmentFactoryEntity`, `jsx.go:1431`).
    pub(crate) jsx_fragment_namespace: Option<String>,
    /// `checkJsxFragment`'s option half (`jsx.go:114`): `Some` when the JSX
    /// transform is enabled and `jsxFragmentFactory` is unset, carrying
    /// whether `jsxFactory` is set.
    pub(crate) jsx_fragment_factory_missing: Option<bool>,
    /// What JSX compiles to. TS2874 is reported **only** under
    /// [`tsr_core::JsxEmit::React`] (`checker.go:28508`). §261.
    pub(crate) jsx_emit: tsr_core::JsxEmit,
    /// `exactOptionalPropertyTypes` (`checker.go:987`): a `?:` property's
    /// optionality is `missingType`, removed at write positions.
    pub(crate) exact_optional_property_types: bool,
    /// `c.languageVersion` (`checker.go:948`, `GetEmitScriptTarget`), read by
    /// the operators lane's TS2791 (`docs/parity/notes/operators.md` §8).
    pub(crate) language_version: tsr_core::ScriptTarget,
    /// §82: depth cap for aliased-condition inlining — upstream's
    /// `inlineLevel` (`flow.go`), capped at 5.
    pub(crate) alias_inline_level: u8,
    /// Semantic non-null refinements and their base variables for flow joins.
    pub(crate) non_null_refinement_bases: FxHashMap<TypeId, TypeId>,
    /// §5 of `checker-notes-nnaccess.md`: an optional-chain link's type **before**
    /// `propagateOptionalTypeMarker` unioned the marker in, keyed by the link's
    /// node.
    ///
    /// This stands in for upstream's `optionalType` — an `undefined` distinct
    /// from the real one, which is how `removeOptionalTypeMarker`
    /// (`checker.go:29073`) takes back exactly what the chain added and leaves a
    /// genuine `undefined` alone. This port has one `undefined`
    /// (`checker-notes-nnaccess.md` §2), so subtracting the marker by *flag* is
    /// wrong in both directions: `this?.a.#b` with `a?: A` must still report
    /// possibly-`undefined` and did not. Remembering the pre-union type is the
    /// same subtraction done by identity instead.
    pub(crate) pre_optional_marker: FxHashMap<tsr_ast::NodeId, TypeId>,
    /// §92: alias-body evaluations, keyed (alias symbol, arguments); error
    /// marks a remembered refusal.
    pub(crate) alias_body_evaluations: FxHashMap<(tsr_binder::SymbolId, Vec<TypeId>), TypeId>,
    pub(crate) type_literal_types: FxHashMap<crate::declared::TypeLiteralKey, TypeId>,
    /// Source declaration syntax retained by instantiateAnonymousType.
    pub(crate) type_literal_origins: FxHashMap<TypeId, tsr_ast::NodeId>,
    pub(crate) key_names_in_progress: rustc_hash::FxHashSet<TypeId>,
    /// §92: types PRODUCED by alias evaluation — the only intersections the
    /// shape property road may search (a WRITTEN intersection answering
    /// confidently was 134 G→W in the discriminated-union family).
    pub(crate) alias_evaluated_types: rustc_hash::FxHashSet<TypeId>,
    /// getConditionalTypeInstantiation's `root.instantiations`
    /// (checker.go:22485) for conditional alias roots: (alias symbol, ordered
    /// type arguments, result alias, inside a mapped template) → the
    /// evaluated type. Completed evaluations only; see `declared.rs`'
    /// `evaluate_conditional_alias`.
    pub(crate) conditional_alias_instantiations:
        FxHashMap<(tsr_binder::SymbolId, Vec<TypeId>, Option<tsr_binder::SymbolId>, bool), TypeId>,
    /// §107: while true, `instantiate_type` answers an UNMAPPED type
    /// parameter with ITSELF instead of refusing — the print-clone's
    /// substitution runs over signatures that legitimately mention ENCLOSING
    /// (foreign) parameters; real instantiation keeps the refusal.
    pub(crate) identity_unmapped_type_parameters: bool,
    /// §110: JSDoc per host node, handed by whoever built the tree (the
    /// conformance producer or a driver); consulted before the module host.
    pub(crate) jsdoc_entries: FxHashMap<NodeId, &'a [&'a tsr_ast::JSDoc<'a>]>,
    /// §269: JSDoc root → host node, the one parent hop the tree deliberately
    /// does not record (see `attach_jsdoc`). Consulted only when a parent walk
    /// dead-ends inside a comment — an `@import` tag's specifier resolving
    /// against its FILE is the sole client.
    pub(crate) jsdoc_hosts: FxHashMap<NodeId, NodeId>,
    /// §107: the RENDER scope — (name, symbol) of each signature's own type
    /// parameters, pushed for the duration of its slot rendering.
    pub(crate) render_type_parameter_scope: Vec<(String, tsr_binder::SymbolId)>,
    /// Private node-builder allocations; never part of a semantic mapper.
    pub(crate) render_type_parameter_names: crate::printing::TypeParameterNames,
    /// §91: the conditional-alias evaluator's binding frames — type-parameter
    /// symbol → the argument it is bound to during one body evaluation. Empty
    /// outside evaluation, which is the gate on the evaluator's node arms.
    pub(crate) alias_evaluation_bindings: Vec<FxHashMap<tsr_binder::SymbolId, TypeId>>,
    /// §89: signature types whose baked text is an ALIAS NAME — the site
    /// renderer's composite re-render must not rebuild them structurally.
    pub(crate) alias_named_signature_types: rustc_hash::FxHashSet<TypeId>,
    pub(crate) no_unused_locals: bool,
    /// `compilerOptions.noUnusedParameters` (`checker.go:7109`).
    pub(crate) no_unused_parameters: bool,
    /// Upstream's `symbolReferenceLinks[symbol].referenceKinds`
    /// (`types.go:164`), rebuilt by [`crate::unused`]'s marking pass because
    /// this port has no type check to hang the resolver callback on.
    pub(crate) symbol_reference_kinds: FxHashMap<tsr_binder::SymbolId, tsr_binder::SymbolFlags>,
    /// Upstream's `links.identifierCheckNodes` (`checker.go:7043`).
    pub(crate) unused_check_nodes: Vec<NodeId>,
    /// Every member name the file mentions, for the by-name stand-in
    /// `markPropertyAsReferenced` needs — see [`crate::unused`].
    pub(crate) referenced_member_names: crate::unused::MemberNames,
    /// Is the file currently being walked a declaration file?
    ///
    /// The file half of `NodeFlagsAmbient`, kept on the checker because
    /// `reportUnused` (`checker.go:7092`) asks for it at a node the walk has
    /// already left.
    pub(crate) file_is_ambient: bool,
    /// Whether the file being checked is a JS file: the reparsed JSDoc type
    /// nodes [`Checker::jsdoc_reparsed_type_nodes`] answers exist only
    /// there, so `check_node` asks it only then.
    pub(crate) file_is_js: bool,
    /// The source files this program is *checking*, as opposed to the ones it
    /// merely holds.
    ///
    /// Upstream needs no equivalent: it has the `Program`, and
    /// `SkipTypeChecking` answers from the file's path and options. Here the
    /// checker is handed one file at a time and cannot see the list, so the
    /// caller supplies it — and the distinction is load-bearing for any rule
    /// that must not report about a **library** type. See
    /// [`Checker::set_checked_files`].
    pub(crate) checked_files: rustc_hash::FxHashSet<NodeId>,
    #[cfg(feature = "work-trace")]
    pub(crate) work_observer: Option<std::sync::Arc<dyn crate::work_trace::WorkObserver>>,
    /// Blocks that have already reported
    /// `Statements are not allowed in ambient contexts`.
    ///
    /// Upstream's `hasReportedStatementInAmbientContext`, a bit on the *block's*
    /// node links (`grammarchecks.go:2062`). It exists so
    /// `declare module "m" { a; b; c; }` reports once rather than three times,
    /// and under the `diagnostics` suite's exact-multiset comparison reporting
    /// three fails the case as surely as reporting none — so it is part of the
    /// rule, not an optimisation.
    pub(crate) ambient_statement_reported: rustc_hash::FxHashSet<NodeId>,
    /// `declaredTypeLinks.Get(enumSymbol).enumChecked` (`checker.go:5087`) —
    /// upstream's *"only perform this check once per symbol"*. Keyed on the
    /// symbol, because the loop it guards runs over every declaration. §953.
    pub(crate) enum_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
    /// Once per symbol for `checkExportsOnMergedDeclarations`. §960.
    pub(crate) merged_spaces_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
    /// Symbols `checkFunctionOrConstructorSymbol` has already visited.
    ///
    /// Upstream's `links.functionOrConstructorChecked` (`checker.go:3463`,
    /// commented *"Only check the symbol once"*). Without it a three-overload
    /// function reports three times and the `diagnostics` suite compares
    /// multisets.
    pub(crate) function_symbol_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
    /// Nodes for which `check_modifier_order` — this port's slice of upstream's
    /// `checkGrammarModifiers` — has already reported. Upstream's callers are
    /// gated on `!c.checkGrammarModifiers(node)`, and this port dropped that
    /// return value when it split the chain into several functions. §876.
    pub(crate) modifier_chain_reported: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// Nodes whose decorators were already rejected. `reportObviousDecoratorErrors`
    /// is the first test in `checkGrammarModifiers` and returns from the whole
    /// function, so the per-keyword switch never runs for them. §878.
    pub(crate) decorator_error_reported: rustc_hash::FxHashSet<tsr_ast::NodeId>,
    /// `(element types, readonly) -> the tuple type`.
    ///
    /// Upstream interns a tuple through `createTypeReference` on a target
    /// synthesised by `getTupleTargetType` (`checker.go:24148`), so identity
    /// falls out of the reference machinery. There is no target symbol here —
    /// the plain tuple arm builds a `TypeData::Named` directly — so the intern
    /// key is the element list itself. Identity matters for the same reason it
    /// did for `C<number>`: a union of two spellings of `[number, string]`
    /// must collapse to one constituent. `readonly` is part of the key because
    /// `readonly [A]` and `[A]` are distinct types upstream, built from
    /// different targets.
    pub(crate) tuple_types: FxHashMap<(Vec<TypeId>, bool), TypeId>,
    /// The reverse of [`Checker::tuple_types`]: `tuple type -> (elements,
    /// readonly)`, written at the same mint site.
    ///
    /// What `compare_types` reads for its tuple arm (`bd tsr-5ll`): upstream's
    /// tuple is a reference to a synthesised target and sorts by
    /// `compareTupleTypes` (`utilities.go`) over the target's element
    /// information, while this port's tuple is a [`TypeData::Named`] whose
    /// text would otherwise be mistaken for a *name* and sorted by ASCII —
    /// `[` before letters — which put an unnamed tuple ahead of named types.
    /// The `type_reference_targets` precedent: record the data where it is
    /// already in hand rather than reshape the type.
    /// §952: a homomorphic IDENTITY mapped type's optionality modifier, keyed
    /// by the minted reference type.
    ///
    /// `Partial<O>` and `Readonly<O>` both answer O's own member symbols — the
    /// mapping is `{ [P in keyof T]: T[P] }` and only the MODIFIERS differ — so
    /// the mint reuses O's members owner rather than synthesising symbols this
    /// binder has no way to make. What the owner cannot carry is the modifier,
    /// which is what this table holds: `Some(true)` adds `?` (and therefore
    /// `| undefined` at every read), `Some(false)` removes it, `None` leaves it
    /// alone.
    ///
    /// Upstream builds fresh property symbols in `resolveMappedTypeMembers`
    /// (`checker.go`) and sets `CheckFlagsReadonly`/optionality on each. The
    /// observable difference between that and reusing the source's symbol is the
    /// property TYPE, which this table supplies at the one seam that reads it —
    /// [`Checker::get_type_of_property_of_type`].
    /// `.0` is the optionality modifier, `.1` the `readonly` modifier — each
    /// `Some(true)` to add, `Some(false)` to remove (`-?` / `-readonly`), `None`
    /// to leave alone. The readonly half is read by
    /// [`Checker::is_readonly_assignment_target`]: `Readonly<Bar>`'s `x4.a = 1`
    /// is upstream's error and its target prints `any` (`mappedTypes6`), which a
    /// reused member owner cannot know on its own.
    pub(crate) mapped_identity_optionality: FxHashMap<TypeId, (Option<bool>, Option<bool>)>,
    /// Deferred homomorphic identity maps retain their type-variable source
    /// (instantiateMappedType, internal/checker/checker.go). A tuple spread
    /// can follow its array constraint without erasing the generic operand.
    pub(crate) mapped_identity_sources: FxHashMap<TypeId, TypeId>,
    /// getConstraintTypeFromMappedType/getTemplateTypeFromMappedType metadata.
    pub(crate) mapped_types: FxHashMap<TypeId, crate::mapped::MappedTypeInfo>,
    pub(crate) base_constraint_cache:
        FxHashMap<crate::constraints::BaseConstraintKey, Option<TypeId>>,
    pub(crate) base_constraint_depth: usize,
    pub(crate) mapped_template_depth: usize,
    /// True/false types of deferred mapped templates (inferToConditionalType).
    pub(crate) mapped_conditional_branches: FxHashMap<TypeId, (TypeId, TypeId)>,
    pub(crate) mapped_conditionals: FxHashMap<TypeId, crate::mapped::MappedConditionalInfo>,
    /// Ordinary deferred conditional nodes retain only their root and mapper;
    /// inference reads branches lazily without resolving the extends operand.
    pub(crate) conditional_inference_nodes:
        FxHashMap<TypeId, crate::declared::ConditionalInferenceNode>,
    /// Deferred alias constraints captured lazily under their reference mapper.
    /// Kept separate from inference branches because a distributive constraint
    /// may replace the check parameter with its base constraint.
    pub(crate) conditional_constraint_branches: FxHashMap<TypeId, (TypeId, TypeId)>,
    pub(crate) mapped_alias_in_progress: rustc_hash::FxHashSet<SymbolId>,
    /// Recursive mapped alias references met while their alias was being
    /// captured. Upstream resolves a mapped type's parts lazily
    /// (getConstraintTypeFromMappedType/getTemplateTypeFromMappedType), so a
    /// self reference such as `Spec<T[P]>` inside `Spec<T>` still has them.
    pub(crate) deferred_mapped_aliases: FxHashMap<TypeId, (SymbolId, Vec<TypeId>)>,
    pub(crate) mapped_members_in_progress: rustc_hash::FxHashSet<TypeId>,
    pub(crate) template_alias_in_progress: rustc_hash::FxHashSet<crate::symbol_access::SymbolRef>,
    pub(crate) template_literal_parts: FxHashMap<TypeId, crate::templates::TemplateLiteralParts>,
    /// Native 5b1047d getStringMappingTypeForGenericType owns completed images
    /// by the actual mapping symbol and target type, even for module clones.
    /// Both existing stores are Checker-local; literal/union/template results
    /// can still share structural types. There is no active mapping reservation.
    pub(crate) string_mapping_types: FxHashMap<TypeId, (crate::symbol_access::SymbolRef, TypeId)>,
    pub(crate) string_mapping_cache: FxHashMap<(crate::symbol_access::SymbolRef, TypeId), TypeId>,
    pub(crate) template_literal_cache: FxHashMap<crate::templates::TemplateLiteralParts, TypeId>,
    pub(crate) mapped_apparent_types: FxHashMap<TypeId, TypeId>,
    /// `resolvedBaseConstructorType`/`resolvedBaseTypes` per class or
    /// interface symbol; owner and publication rules in [`crate::base_types`].
    pub(crate) base_type_links: crate::base_types::BaseTypeLinks,
    pub(crate) type_parameter_default_cache: FxHashMap<
        crate::declared::TypeParameterDefaultKey,
        crate::declared::TypeParameterDefaultState,
    >,
    pub(crate) type_parameter_constraint_cache:
        FxHashMap<crate::members::TypeParameterConstraintKey, Option<TypeId>>,
    pub(crate) reverse_mapped_cache: FxHashMap<(TypeId, TypeId, TypeId), Option<TypeId>>,
    pub(crate) reverse_mapped_member_cache: FxHashMap<(TypeId, TypeId, TypeId), TypeId>,
    /// reverseMappedSourceStack/reverseMappedTargetStack and
    /// reverseExpandingFlags (inference.go:1066).
    pub(crate) reverse_mapped_source_stack: Vec<TypeId>,
    pub(crate) reverse_mapped_target_stack: Vec<TypeId>,
    pub(crate) reverse_expanding: (bool, bool),
    /// Reverse mapped objects whose members have not been resolved yet.
    pub(crate) pending_reverse_mapped: FxHashMap<TypeId, crate::mapped::ReverseMappedInfo>,
    /// The `any`-placeholder rendering of each reverse mapped object.
    pub(crate) reverse_placeholder_texts: FxHashMap<TypeId, String>,
    /// Whether each reverse mapped property being resolved has an anonymous
    /// source type (nodebuilderimpl.go shouldUsePlaceholderForProperty).
    pub(crate) reverse_property_anonymous: Vec<bool>,
    pub(crate) tuple_element_lists: FxHashMap<TypeId, (Vec<TypeId>, bool)>,
    /// §79: interning for optional-element tuples, keyed on (member,
    /// optional) pairs so `[number, string?]` and `[number, string]` stay
    /// distinct types.
    pub(crate) optional_tuple_types: FxHashMap<OptionalTupleKey, TypeId>,
    /// §79: which positions of an optional-element tuple carry `?` — read at
    /// the index roads, where an optional element answers `| undefined`.
    pub(crate) tuple_optional_masks: FxHashMap<TypeId, Vec<bool>>,
    /// Tuple labels retained across `instantiateTypeWorker` rebuilds, matching
    /// typescript-go's `TupleElementInfo.labeledDeclaration` (`types.go`).
    pub(crate) tuple_labels: FxHashMap<TypeId, Vec<Option<String>>>,
    /// §87: a trailing-rest variadic's tuple NODE — resolved lazily by
    /// positional consumers; the print stays §40's.
    pub(crate) tuple_rest_tails: FxHashMap<TypeId, tsr_ast::NodeId>,
    /// §791: the NODE behind a §40 PRINT-ONLY variadic tuple — one whose rest
    /// element is over a type parameter, so no element list could be built.
    ///
    /// `tuple_rest_tails` records only the trailing-`...T[]` shape and only for
    /// §86's contextual expansion. This records EVERY print-only variadic, so
    /// an alias instantiation can re-resolve the node with its type parameters
    /// bound and let §40's own splice run on concrete arguments.
    pub(crate) variadic_tuple_nodes: FxHashMap<TypeId, tsr_ast::NodeId>,
    /// Resolved tuple arguments and element information before normalization,
    /// corresponding to typescript-go's `TupleElementInfo` (`types.go`).
    pub(crate) variadic_tuple_elements: FxHashMap<TypeId, (Vec<crate::tuples::TupleElement>, bool)>,
    /// Interned normalized variadic references, matching `createTypeReference`
    /// (`internal/checker/checker.go`).
    pub(crate) variadic_tuple_types: FxHashMap<(Vec<crate::tuples::TupleElement>, bool), TypeId>,
    /// §791: the variadic aliases whose normalisation is in progress. A
    /// variadic body can reference its own alias, and the re-resolve below
    /// re-enters this road; without the guard that is an unbounded recursion
    /// (measured as a stack overflow on the first corpus run).
    pub(crate) variadic_alias_in_progress: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
    /// `a type-parameter type -> the symbol it was minted from`.
    ///
    /// The third instance of the `type_reference_targets` precedent, and it
    /// exists for a reason the type itself states: `new_named_type` gives a
    /// type parameter `members: None` **deliberately**, because a type
    /// parameter owns no members table and pointing it at its own symbol would
    /// make a property lookup "silently succeed against the wrong symbol".
    /// That is right, and it also means there is no route from the `TypeId`
    /// back to the symbol — which `getApparentType`'s instantiable head needs
    /// in order to read the `extends` constraint (`bd tsr-rppd`). Recorded
    /// where it is already in hand rather than by widening `TypeData::Named`,
    /// whose `members` field would then mean two different things.
    pub(crate) type_parameter_symbols: FxHashMap<TypeId, SymbolId>,
    /// Written references do not count as generative recursion by their target.
    pub(crate) reference_types_from_nodes: rustc_hash::FxHashSet<TypeId>,
    /// Fresh signature parameters retain a target and constraint mapper.
    pub(crate) instantiated_type_parameters:
        FxHashMap<TypeId, crate::inference::InstantiatedTypeParameter>,
    /// `the baked signature type -> the signatures its text was rendered from`.
    ///
    /// The sibling of [`Checker::type_reference_targets`] for function-shaped
    /// types, and the same ADR-0003 move: upstream's `*Type` carries its
    /// resolved signatures (`checker.go`, `t.AsObjectType()`), so
    /// `instantiateSignature` reads them off the type; here the [`Signature`]s
    /// exist at exactly the two sites that bake them to text
    /// (`crate::function_types`, `crate::symbols`) and are dropped there, which
    /// made every signature-typed member of a generic uninstantiable. One entry
    /// per distinct baked type; written where the text is rendered, because
    /// that is the last point the structure exists. `bd tsr-0hc`.
    pub(crate) signature_types: FxHashMap<TypeId, Vec<crate::signatures::Signature>>,
    /// Native decorator signature links belong to the decorated declaration.
    pub(crate) decorator_types: crate::decorators::DecoratorTypes,
    /// Declaration-owned resolved return slots (native getReturnTypeOfSignature,
    /// typescript-go 5b1047d1). The key retains captured bindings/template mode;
    /// contextual functions still recheck rather than reusing a premature return.
    /// Absence is uncomputed/active (the resolution stack distinguishes them),
    /// Some(None) is unsupported, Some(Some(type)) is completed including any
    /// from a failed circular resolution. Private Checker lifetime/options.
    /// Only `return_type_of` publishes; its body worker is the expensive boundary.
    pub(crate) signature_returns: FxHashMap<crate::declared::TypeLiteralKey, Option<TypeId>>,
    /// Native uncontextualized object-member function values retain lazy return
    /// slots (5b1047d1, checker.go:10166/19836). Declaration/captured key owned
    /// by this Checker; no mapper or contextual assignment is reused. Absence
    /// differs from pending here, active in resolutions, completed/unsupported
    /// in `signature_returns`. Only the canonical return accessor removes pending
    /// and completes the original signature; no provisional any is published.
    pub(crate) pending_signature_returns:
        FxHashMap<crate::declared::TypeLiteralKey, crate::signatures::LazyReturnState>,
    /// Native return-slot errors report once per declaration, even when this
    /// port rechecks a mutable contextual signature. Diagnostic ownership only;
    /// this set never certifies completion or substitutes a semantic return.
    pub(crate) return_cycle_diagnostics: rustc_hash::FxHashSet<NodeId>,
    /// reportCircularityError / getTypeOfAccessors (checker.go:18822, :18511)
    /// diagnostics already reported, keyed by the reporting declaration node.
    /// Native's diagnostic collection drops the identical report a failed push
    /// and its failed pop both make; this set is that de-duplication only and
    /// never certifies a type.
    pub(crate) circularity_reported: rustc_hash::FxHashSet<NodeId>,
    /// Unannotated declarations whose symbol type is `reportCircularityError`'s
    /// `anyType` (checker.go:18822): the `any` upstream computes too, not a
    /// fallback for an unported path. Read by TS2403's trusted-`any` rule
    /// (`docs/parity/notes/r5-constraints2.md` §7).
    pub(crate) circular_any_declarations: rustc_hash::FxHashSet<NodeId>,
    /// resolveAnonymousTypeMembers / getDefaultConstructSignatures (checker.go).
    /// None marks an active or unsupported class constructor resolution.
    pub(crate) class_construct_signatures:
        FxHashMap<SymbolId, Option<Vec<crate::signatures::Signature>>>,
    /// Memo tables for answers native keeps in symbol/type links
    /// (`crate::perf_links`; contracts in `docs/parity/notes/r4-perf.md`).
    pub(crate) perf_links: crate::perf_links::PerfLinks,
    /// `(module, export name) -> the type-only `export *` declaration` that
    /// [`Checker::specifier_type_only_export_star`]'s walk answers: native
    /// `typeOnlyExportStarMap[name]`, built once per module by
    /// `getExportsOfModuleWorker` (`checker.go:16148`). Owned by that function;
    /// `docs/parity/notes/perf.md` §17.
    pub(crate) type_only_export_stars: FxHashMap<(SymbolId, &'a str), Option<NodeId>>,
    /// `(baked signature type, substitution map) -> the instantiated type`,
    /// upstream's per-mapper instantiation cache (`checker.go:22125`) reduced
    /// to the one key this port can build.
    ///
    /// Doubles as the guard `resolve_call_signature` needs: a minted type is
    /// `Anonymous` over the *uninstantiated* symbol (the `signature` bit must
    /// survive for union parenthesisation), so resolving a call through that
    /// symbol would answer the uninstantiated return type — a wrong line. The
    /// resolver tests [`Checker::is_instantiated_signature_type`] and gaps.
    pub(crate) instantiated_signatures: FxHashMap<(TypeId, Vec<(TypeId, TypeId)>), TypeId>,
    /// Instantiated anonymous types retain their mapper for native array-member
    /// union fallback (getArrayMemberCallSignatures, checker.go).
    pub(crate) instantiated_signature_mappers: FxHashMap<TypeId, Vec<(TypeId, TypeId)>>,
    /// Resolved union call/construct and intersection call lists; None marks active resolution.
    pub(crate) composite_signature_types:
        FxHashMap<(TypeId, bool), Option<Vec<crate::signatures::Signature>>>,
    /// getReducedType's discriminant-conflict result, computed before reading
    /// intersection signatures without changing written annotation identity.
    pub(crate) never_intersection_types: FxHashMap<TypeId, bool>,
    /// `emptyTypeLiteralType` (checker.go:22939): the one type every
    /// member-less, unaliased type literal resolves to. Minted on first use.
    pub(crate) empty_type_literal_type: Option<TypeId>,
    /// The values of [`Checker::instantiated_signatures`], for the O(1)
    /// membership test the call resolver makes.
    pub(crate) minted_signature_types: rustc_hash::FxHashSet<TypeId>,
    /// §946: while set, `contextual_type_for_argument_resolving` answers the
    /// **uninstantiated** parameter type instead of the one its fixing mapper
    /// produces — upstream's PASS ONE.
    ///
    /// Upstream checks a call's arguments twice: once against the parameter type
    /// as written (`{ fields: A }`, where `isLiteralOfContextualType` sees a type
    /// variable and keeps a literal fresh), and once against the instantiated
    /// one. This port has a single pass, so the freshness question and the
    /// answer question read the same contextual type — and the fixing mapper has
    /// already replaced `A` with `unknown` by then.
    ///
    /// Set only around the freshness query in
    /// [`Checker::check_expression_for_mutable_location`]; nothing else reads it.
    pub(crate) contextual_prefers_uninstantiated: bool,
    /// Raw contextual query for inferTypeArguments applies to this expression
    /// only; nested callback parameter checks still use their fixing mapper.
    pub(crate) uninstantiated_context_node: Option<NodeId>,
    /// §937: memo for `Checker::target_could_contain_parameter`, upstream's
    /// `couldContainTypeVariables` gate on the property arm of inference.
    ///
    /// **The memo is the difference between a 4-minute conformance run and one
    /// that did not finish in 20.** The predicate follows members, so a
    /// lib-typed target drags in `Array`, `String` and their dozens of members
    /// on every query; the same types recur across every call in a file.
    ///
    /// Only TOP-LEVEL answers are cached: a result computed while a cycle guard
    /// was active can be `false` for the cycle rather than for the type, and
    /// caching that would poison every later query.
    pub(crate) could_contain_parameter_cache: rustc_hash::FxHashMap<(TypeId, Vec<TypeId>), bool>,
    /// Composite types currently being re-rendered at a site — the cycle
    /// guard of [`Checker::type_to_string_at`]'s twin arm (§10.13).
    pub(crate) rendering_composites: rustc_hash::FxHashSet<TypeId>,
    /// How many frames of [`Checker::instantiate_type`] are on the stack.
    ///
    /// Upstream's `c.instantiationDepth` (`checker.go:592`), consumed by the
    /// guard at `checker.go:22111`. These are **not** stack-safety devices —
    /// they exist because an infinite generic type perpetually mints new type
    /// identities (`interface List<T> { next: List<List<T>> }`), and the cap of
    /// 100 is what makes ADR-0029's modest stack budget safe. `bd tsr-el3.2`.
    pub(crate) instantiation_depth: u32,
    /// How many instantiations have run since the current expression check.
    ///
    /// Upstream's `c.instantiationCount` (`checker.go:591`), the other half of
    /// the `checker.go:22111` guard. checkExpressionEx resets it before its
    /// worker (`checker.go:7563`); this port does the same in `check_expression`.
    /// The statement/deferred-node reset sites remain outside this port's
    /// on-demand expression traversal.
    pub(crate) instantiation_count: u32,
    /// Upstream's `c.currentNode` (`checker.go:596`); `current_node.rs`.
    pub(crate) current_node: Option<NodeId>,
    /// Whether the program declares any pattern ambient module
    /// (`declare module "*.css"`).
    ///
    /// Upstream's `c.patternAmbientModules` (`checker.go:765`), filled once by
    /// `initializeChecker` (`checker.go:1318`) from each file's binder list.
    /// This port's binder keeps no such list, so the answer is the same
    /// name-shape test over the binder's global table that
    /// `Checker::has_pattern_ambient_module` documents, taken once here.
    ///
    /// Port boundary: owned by this checker, keyed by nothing (one answer per
    /// program), published at construction and never changed — the binder's
    /// globals are immutable for the checker's lifetime. The expensive work
    /// (one scan of every global name) happens here instead of on every
    /// module-specifier query.
    pub(crate) has_pattern_ambient_modules: bool,
}

impl<'a, 'n> Checker<'a, 'n> {
    /// Create a checker with no way to reach another file.
    ///
    /// # Why the host is a second constructor and not a fourth parameter
    ///
    /// `Checker::new` has **84 call sites** (`grep -rn "Checker::new" crates
    /// --include='*.rs' | wc -l` at the time of writing), spread over
    /// `crates/tsr-checker/tests/`, `crates/tsr-conformance/`,
    /// `crates/tsr-checker-spike/` and a bench. All but one of them parse a
    /// single file and have no program to offer. Three options were weighed:
    ///
    /// | shape | cost |
    /// |---|---|
    /// | fourth `Option<&dyn ModuleHost>` parameter | 84 edits, ~81 of them writing `None`, across five crates and three other agents' files |
    /// | a builder | a second way to construct the central type, for one optional field |
    /// | **a second constructor** | one new function; `new` keeps its signature and delegates |
    ///
    /// The second constructor wins on the property the change is judged by:
    /// **a call site with no host must behave bit-for-bit as it does today.**
    /// Under the fourth-parameter shape that is a claim about 81 hand-edits;
    /// here it is true *by construction*, because `new` is literally
    /// [`Checker::with_module_host`] applied to `None` and there is one body.
    /// That is the same reasoning `contextual.rs` used when it returned `None`
    /// rather than an answer: returning nothing invents nothing, so the only
    /// lines that can move are ones someone moved deliberately.
    ///
    /// **What would make the fourth parameter win:** a second optional
    /// dependency. Two `Option` fields means four constructors under this
    /// shape and one signature under that one, and at that point the builder
    /// becomes the right answer rather than the parameter. Upstream has no
    /// such fork because its `Program` interface is never absent — every
    /// upstream checker comes from `NewChecker(program)` — so the fork is
    /// this port's, created by a unit-test harness that predates the program.
    #[must_use]
    pub fn new(
        binder: &'a BindResult<'a>,
        nodes: &'n NodeTable,
        node_map: &'n NodeMap<'a>,
    ) -> Self {
        Self::with_module_host(binder, nodes, node_map, None)
    }

    /// Read validated private and bound symbol ownership for this Checker.
    #[must_use]
    pub fn symbol_access(&self) -> &crate::symbol_access::CheckerSymbols<'a> {
        &self.symbols
    }

    /// Create a checker that can reach another file through `module_host`.
    ///
    /// `NewChecker(program)` (`internal/checker/checker.go:908`, where
    /// `c.program = program`), with the interface narrowed to
    /// [`ModuleHost`] — see that trait for why one method rather than
    /// eighteen.
    ///
    /// Passing `None` is [`Checker::new`], and the two share this body so they
    /// cannot drift.
    /// §110: hand the checker a file's JSDoc side table.
    ///
    /// The `#[must_use]` that arrived here belonged to `with_module_host`
    /// below; a doc comment inserted between an attribute and its item moves
    /// the attribute onto the wrong one, which is §241's shape a second time.
    pub fn set_jsdoc(
        &mut self,
        entries: impl IntoIterator<Item = (NodeId, &'a [&'a tsr_ast::JSDoc<'a>])>,
    ) {
        for (host, docs) in entries {
            for doc in docs {
                if let Some(doc_id) = doc.node_id {
                    self.jsdoc_hosts.insert(doc_id, host);
                }
            }
            self.jsdoc_entries.insert(host, docs);
        }
    }

    /// Create a checker that can reach another file through `module_host`.
    #[must_use]
    pub fn with_module_host(
        binder: &'a BindResult<'a>,
        nodes: &'n NodeTable,
        node_map: &'n NodeMap<'a>,
        module_host: Option<&'n dyn ModuleHost>,
    ) -> Self {
        let mut store = TypeStore::new();
        let intrinsics = Intrinsics::create(&mut store);
        // `valueSymbolLinks.Get(c.undefinedSymbol).resolvedType`
        // (`checker.go:1345`) — the synthesised `undefined` global has no
        // declaration, so no dispatch arm can compute its type and upstream
        // sets it directly. `symbol_types` IS that link table here, and
        // `get_type_of_variable_or_parameter_or_property` reads it before doing
        // any work, so seeding it is what makes the symbol answer at all.
        //
        // Upstream seeds `undefinedWideningType` — the same type as
        // `undefinedType` in strict mode, a distinct widening twin otherwise
        // (`createWideningType`, `checker.go:25027`), so `let x = undefined`
        // is `any` under `strictNullChecks: false`. The checker starts strict,
        // where the two coincide; `set_strict_null_checks` re-seeds the slot
        // when a case turns the flag off (`docs/parity/notes/contextual.md`
        // §7). This line used to record that divergence: the port had no
        // widening twin at all.
        //
        // See `Binder::declare_synthesised_globals` for the other half.
        let mut symbol_types = FxHashMap::default();
        if let Some(undefined) = binder.undefined_symbol() {
            symbol_types.insert(undefined, intrinsics.undefined);
        }
        Self {
            symbols: crate::symbol_access::CheckerSymbols::new(
                binder.symbols(),
                intrinsics.unresolved,
            ),
            store,
            intrinsics,
            nodes,
            node_map,
            binder,
            module_host,
            computations: 0,
            node_types: FxHashMap::default(),
            regular_types: FxHashMap::default(),
            enum_member_owners: FxHashMap::default(),
            diagnostics: Vec::new(),
            symbol_types,
            parameter_initializer_contains_undefined: FxHashMap::default(),
            this_expando_kinds: FxHashMap::default(),
            symbol_assignment_scan: FxHashMap::default(),
            symbol_empty_array_assignment_scan: FxHashMap::default(),
            last_assignment_pos: FxHashMap::default(),
            enum_member_regular: FxHashMap::default(),
            alias_placeholders: FxHashMap::default(),
            alias_targets: FxHashMap::default(),
            alias_resolving: 0,
            base_symbols: FxHashMap::default(),
            interface_signatures: FxHashMap::default(),
            file_import_machinery: FxHashMap::default(),
            global_this_type: None,
            unique_symbol_nodes: FxHashMap::default(),
            unique_es_symbol_types: FxHashMap::default(),
            this_type_nodes: FxHashMap::default(),
            qualified_reference_types: FxHashMap::default(),
            qualified_generic_reference_types: FxHashMap::default(),
            qualified_written_text: FxHashMap::default(),
            assignments_marked: rustc_hash::FxHashSet::default(),
            definitely_assigned: rustc_hash::FxHashSet::default(),
            flow_loop_cache: FxHashMap::default(),
            flow_node_reachable: FxHashMap::default(),
            flow_loop_stack: Vec::new(),
            declared_types: FxHashMap::default(),
            this_types: FxHashMap::default(),
            constrained_type_variables: FxHashMap::default(),
            instantiations: FxHashMap::default(),
            type_reference_targets: FxHashMap::default(),
            alias_of: FxHashMap::default(),
            deferred_alias_references: FxHashMap::default(),
            reference_display_arity: FxHashMap::default(),
            literal_this_types: FxHashMap::default(),
            unresolved_types: rustc_hash::FxHashSet::default(),
            deferred_keyof_types: rustc_hash::FxHashSet::default(),
            deferred_keyof_operands: FxHashMap::default(),
            index_types_in_progress: rustc_hash::FxHashSet::default(),
            deferred_index_mints: rustc_hash::FxHashSet::default(),
            deferred_indexed_access_types: FxHashMap::default(),
            deferred_indexed_access_cache: FxHashMap::default(),
            resolutions: Resolutions::new(),
            dependent_binding_parents_in_flight: rustc_hash::FxHashSet::default(),
            flow_analysis_disabled: false,
            flow_disabled_containers: rustc_hash::FxHashSet::default(),
            shared_flows: crate::perf_links::SharedFlows::default(),
            global_alias_entries: None,
            narrow_value_stack: std::collections::HashSet::new(),
            call_inference_signatures: rustc_hash::FxHashMap::default(),
            resolved_call_signatures: rustc_hash::FxHashMap::default(),
            completed_no_effects_calls: rustc_hash::FxHashSet::default(),
            overload_argument_failures: rustc_hash::FxHashMap::default(),
            higher_order_context_calls: rustc_hash::FxHashSet::default(),
            resolving_signature_calls: rustc_hash::FxHashSet::default(),
            context_checked_arguments: rustc_hash::FxHashSet::default(),
            resolving_iteration_types: rustc_hash::FxHashSet::default(),
            late_bound_member_names: rustc_hash::FxHashMap::default(),
            contextual_return_in_flight: rustc_hash::FxHashSet::default(),
            contextual_this_parameters: FxHashMap::default(),
            contextual_return_depth: 0,
            intra_expression_member_maps: rustc_hash::FxHashMap::default(),
            contextual_signature_mappers: FxHashMap::default(),
            narrow_value_types: rustc_hash::FxHashMap::default(),
            union_origin: rustc_hash::FxHashMap::default(),
            enum_value_types: rustc_hash::FxHashMap::default(),
            enum_access_spelling: rustc_hash::FxHashMap::default(),
            named_union_by_members: rustc_hash::FxHashMap::default(),

            instantiation_depth: 0,
            instantiation_count: 0,
            current_node: None,
            has_pattern_ambient_modules: binder.globals().keys().any(|name| name.contains('*')),
            strict_null_checks: true,
            strict_function_types: true,
            strict_builtin_iterator_return: true,
            inference_contravariant: false,
            inference_bivariant: false,
            inference_priority: crate::inference::InferencePriority::NONE,
            inference_visited_pairs: FxHashMap::default(),
            inference_source_stack: Vec::new(),
            inference_target_stack: Vec::new(),
            inference_expanding: (false, false),
            inference_observed_priority: i32::from(
                crate::inference::InferencePriority::MAX_VALUE.bits(),
            ),
            active_inference_contexts: FxHashMap::default(),
            silent_never_type: None,
            variance_cache: FxHashMap::default(),
            variance_in_progress: rustc_hash::FxHashSet::default(),
            relation_results: crate::relation_cache::RelationResults::default(),
            discriminant_properties: FxHashMap::default(),
            variance_markers: None,
            variance_marker_types: rustc_hash::FxHashSet::default(),
            signature_context_cache: FxHashMap::default(),
            signature_context_in_progress: rustc_hash::FxHashSet::default(),
            // Mirrors `strict_null_checks`' struct default; every corpus road
            // calls `apply_compiler_options`, which overrides both.
            strict_bind_call_apply: true,
            no_implicit_this: false,
            module_kind: tsr_core::ModuleKind::None,
            unsafe_import_tracker: None,
            legacy_decorators: false,
            import_helpers: false,
            external_helpers: FxHashMap::default(),
            standard_class_fields: false,
            allow_synthetic_defaults: false,
            allow_importing_ts_extensions: false,
            no_unchecked_side_effect_imports: true,
            no_unchecked_indexed_access: false,
            use_unknown_in_catch_variables: false,
            strict_property_initialization: true,
            no_implicit_override: false,
            no_implicit_returns: false,
            file_has_parse_errors: false,
            merge_conflicts_reported: false,
            assignability_probe: Vec::new(),
            allow_unreachable_code: false,
            unreachable_code_is_error: false,
            preserve_const_enums: false,
            isolated_modules: false,
            isolated_modules_option: false,
            verbatim_module_syntax: false,
            module_format_options: crate::module_format::ModuleFormatOptions::default(),
            exhaustive_switches: rustc_hash::FxHashSet::default(),
            no_implicit_any: false,
            lib_includes_dom: false,
            uses_wildcard_types: false,
            js_literal_types: rustc_hash::FxHashSet::default(),
            fresh_object_literal_types: rustc_hash::FxHashSet::default(),
            regular_object_literal_types: FxHashMap::default(),
            object_literal_spread_flags: FxHashMap::default(),
            array_literal_images: FxHashMap::default(),
            array_literal_bases: FxHashMap::default(),
            widened_object_types: FxHashMap::default(),
            object_type_transfer_origins: FxHashMap::default(),
            widening_undefined_properties: FxHashMap::default(),
            object_literal_members: rustc_hash::FxHashMap::default(),
            anonymous_properties: rustc_hash::FxHashMap::from_iter([
                (intrinsics.empty_object, (Vec::new(), true)),
                (intrinsics.unknown_empty_object, (Vec::new(), true)),
            ]),
            module_value_clones: FxHashMap::default(),
            synthetic_default_types: FxHashMap::default(),
            instantiated_objects: rustc_hash::FxHashMap::default(),
            instantiated_object_mappers: rustc_hash::FxHashMap::default(),
            instantiation_expressions:
                crate::instantiation_expressions::InstantiationExpressionLinks::default(),
            any_function_type: None,
            non_inferrable_types: rustc_hash::FxHashSet::default(),
            index_components: Vec::new(),
            object_literal_index_infos: rustc_hash::FxHashMap::default(),
            pattern_implied_members: rustc_hash::FxHashMap::default(),
            jsx_namespace: "React".to_string(),
            jsx_fragment_namespace: None,
            jsx_fragment_factory_missing: None,
            jsx_emit: tsr_core::JsxEmit::None,
            exact_optional_property_types: false,
            language_version: tsr_core::ScriptTarget::ESNext,
            alias_inline_level: 0,
            non_null_refinement_bases: FxHashMap::default(),
            pre_optional_marker: FxHashMap::default(),
            jsdoc_entries: FxHashMap::default(),
            jsdoc_hosts: FxHashMap::default(),
            identity_unmapped_type_parameters: false,
            render_type_parameter_scope: Vec::new(),
            render_type_parameter_names: crate::printing::TypeParameterNames::default(),
            alias_body_evaluations: FxHashMap::default(),
            type_literal_types: FxHashMap::default(),
            type_literal_origins: FxHashMap::default(),
            key_names_in_progress: rustc_hash::FxHashSet::default(),
            alias_evaluated_types: rustc_hash::FxHashSet::default(),
            conditional_alias_instantiations: FxHashMap::default(),
            alias_evaluation_bindings: Vec::new(),
            alias_named_signature_types: rustc_hash::FxHashSet::default(),
            no_unused_locals: false,
            no_unused_parameters: false,
            symbol_reference_kinds: FxHashMap::default(),
            unused_check_nodes: Vec::new(),
            referenced_member_names: crate::unused::MemberNames::default(),
            file_is_ambient: false,
            file_is_js: false,
            checked_files: rustc_hash::FxHashSet::default(),
            #[cfg(feature = "work-trace")]
            work_observer: None,
            ambient_statement_reported: rustc_hash::FxHashSet::default(),
            enum_checked: rustc_hash::FxHashSet::default(),
            merged_spaces_checked: rustc_hash::FxHashSet::default(),
            function_symbol_checked: rustc_hash::FxHashSet::default(),
            modifier_chain_reported: rustc_hash::FxHashSet::default(),
            decorator_error_reported: rustc_hash::FxHashSet::default(),
            tuple_types: FxHashMap::default(),
            mapped_identity_optionality: FxHashMap::default(),
            mapped_identity_sources: FxHashMap::default(),
            mapped_types: FxHashMap::default(),
            base_constraint_cache: FxHashMap::default(),
            base_constraint_depth: 0,
            mapped_template_depth: 0,
            mapped_conditional_branches: FxHashMap::default(),
            mapped_conditionals: FxHashMap::default(),
            conditional_inference_nodes: FxHashMap::default(),
            conditional_constraint_branches: FxHashMap::default(),
            mapped_alias_in_progress: rustc_hash::FxHashSet::default(),
            deferred_mapped_aliases: FxHashMap::default(),
            mapped_members_in_progress: rustc_hash::FxHashSet::default(),
            template_alias_in_progress: rustc_hash::FxHashSet::default(),
            template_literal_parts: FxHashMap::default(),
            string_mapping_types: FxHashMap::default(),
            string_mapping_cache: FxHashMap::default(),
            template_literal_cache: FxHashMap::default(),
            mapped_apparent_types: FxHashMap::default(),
            base_type_links: crate::base_types::BaseTypeLinks::default(),
            type_parameter_default_cache: FxHashMap::default(),
            type_parameter_constraint_cache: FxHashMap::default(),
            reverse_mapped_cache: FxHashMap::default(),
            reverse_mapped_member_cache: FxHashMap::default(),
            reverse_mapped_source_stack: Vec::new(),
            reverse_mapped_target_stack: Vec::new(),
            reverse_expanding: (false, false),
            pending_reverse_mapped: FxHashMap::default(),
            reverse_placeholder_texts: FxHashMap::default(),
            reverse_property_anonymous: Vec::new(),
            tuple_element_lists: FxHashMap::default(),
            optional_tuple_types: FxHashMap::default(),
            tuple_optional_masks: FxHashMap::default(),
            tuple_labels: FxHashMap::default(),
            tuple_rest_tails: FxHashMap::default(),
            variadic_tuple_nodes: FxHashMap::default(),
            variadic_tuple_elements: FxHashMap::default(),
            variadic_tuple_types: FxHashMap::default(),
            variadic_alias_in_progress: rustc_hash::FxHashSet::default(),
            type_parameter_symbols: FxHashMap::default(),
            instantiated_type_parameters: FxHashMap::default(),
            reference_types_from_nodes: rustc_hash::FxHashSet::default(),
            signature_types: FxHashMap::default(),
            decorator_types: crate::decorators::DecoratorTypes::default(),
            signature_returns: FxHashMap::default(),
            pending_signature_returns: FxHashMap::default(),
            return_cycle_diagnostics: rustc_hash::FxHashSet::default(),
            circularity_reported: rustc_hash::FxHashSet::default(),
            circular_any_declarations: rustc_hash::FxHashSet::default(),
            class_construct_signatures: FxHashMap::default(),
            perf_links: crate::perf_links::PerfLinks::default(),
            type_only_export_stars: FxHashMap::default(),
            instantiated_signatures: FxHashMap::default(),
            instantiated_signature_mappers: FxHashMap::default(),
            composite_signature_types: FxHashMap::default(),
            never_intersection_types: FxHashMap::default(),
            empty_type_literal_type: None,
            minted_signature_types: rustc_hash::FxHashSet::default(),
            contextual_prefers_uninstantiated: false,
            uninstantiated_context_node: None,
            could_contain_parameter_cache: rustc_hash::FxHashMap::default(),
            rendering_composites: rustc_hash::FxHashSet::default(),
        }
    }

    /// The program this checker can reach another file through, if it has one.
    ///
    /// Upstream reads `c.program` directly (`internal/checker/checker.go:581`)
    /// because everything in `checker.go` is one package; here the arms that
    /// need it live in sibling modules and read the `pub(crate)` field. This
    /// accessor exists for the *other* reader: a caller outside the crate that
    /// wants to know whether the checker it built is the cross-file kind — which
    /// `crates/tsr-compiler/tests/module_host.rs` is, and which is the only
    /// direct evidence that [`Checker::new`] passes `None`.
    #[must_use]
    pub fn module_host(&self) -> Option<&'n dyn ModuleHost> {
        self.module_host
    }

    /// Configure every option this checker reads, from one `CompilerOptions`.
    ///
    /// The port of upstream's option block in `NewChecker`
    /// (`internal/checker/checker.go:915-928`) plus the three options upstream
    /// reads lazily at their use sites — `NoUnusedLocals`/`NoUnusedParameters`
    /// (`unusedIsError`, `checker.go:7104`), `AllowUnreachableCode`
    /// (`checker.go:2264` and `:2448`), and `NoUncheckedSideEffectImports`
    /// (`checker.go:5321`). Upstream can afford to read those late because the
    /// checker holds its `compilerOptions`; this port copies each into a `bool`
    /// field at construction, so they are all resolved here instead.
    ///
    /// # Why this exists rather than each caller deriving its own
    ///
    /// Because two callers did, and disagreed. The rules are not uniform — three
    /// different defaults appear in the eleven lines below, and which family an
    /// option belongs to is not guessable from its name:
    ///
    /// | read | unset means | example |
    /// |---|---|---|
    /// | [`CompilerOptions::strict_option_value`] | **on**, unless `strict: false` | `strictNullChecks` |
    /// | `IsTrue()` | off | `noUnusedLocals` |
    /// | `IsTrueOrUnknown()` | on, unconditionally | `noUncheckedSideEffectImports` |
    ///
    /// `allowUnreachableCode` belongs to none of them and needs *two* reads: an
    /// unset value makes unreachable code a suggestion, which never reaches a
    /// `.errors.txt`, so `unreachable_code_is_error` is `is_false()` and is **not**
    /// the negation of `allow_unreachable_code`
    /// (`docs/architecture/checker-notes-diag2.md` §82).
    ///
    /// See
    /// [ADR-0042](../../../docs/adr/0042-checker-options-come-from-compiler-options.md).
    ///
    /// # When to call it
    ///
    /// Immediately after construction, before any type is created. The union
    /// constructor consults `strict_null_checks` at build time, so flipping it
    /// once types exist leaves a store built under two configurations.
    pub fn apply_compiler_options(&mut self, options: &tsr_core::CompilerOptions) {
        let previous_strict_function_types = self.strict_function_types;
        // The strict family (`checker.go:919-926`).
        self.set_strict_null_checks(options.strict_option_value(options.strict_null_checks));
        self.strict_builtin_iterator_return =
            options.strict_option_value(options.strict_builtin_iterator_return);
        self.strict_function_types = options.strict_option_value(options.strict_function_types);
        if self.strict_function_types != previous_strict_function_types {
            self.variance_cache.clear();
            self.signature_context_cache.clear();
        }
        self.strict_bind_call_apply = options.strict_option_value(options.strict_bind_call_apply);
        self.no_implicit_this = options.strict_option_value(options.no_implicit_this);
        self.legacy_decorators = options.experimental_decorators.is_true();
        self.import_helpers = options.import_helpers.is_true();
        // `GetEmitStandardClassFields` — the flag, defaulting to
        // `target >= ES2022`. §751.
        self.standard_class_fields = match options.use_define_for_class_fields {
            tsr_core::Tristate::True => true,
            tsr_core::Tristate::False => false,
            tsr_core::Tristate::Unknown => {
                options.emit_script_target() >= tsr_core::ScriptTarget::ES2022
            }
        };
        // `c.moduleKind = c.compilerOptions.GetEmitModuleKind()`
        // (`checker.go:915`): the ladder over the *emit* target, so an unset
        // target (ES2025) gives ES2022, not CommonJS. `r5-config.md` §3.
        self.module_kind = options.emit_module_kind();
        // `getAllowSyntheticDefaultImports`: explicit wins; else
        // `esModuleInterop` (explicit only — its own Node16+ default is the
        // §131 Node16/NodeNext exclusion's business); else `module == System`.
        self.allow_importing_ts_extensions = options.allow_importing_ts_extensions.is_true();
        self.allow_synthetic_defaults = match options.allow_synthetic_default_imports {
            tsr_core::Tristate::True => true,
            tsr_core::Tristate::False => false,
            tsr_core::Tristate::Unknown => {
                options.es_module_interop.is_true()
                    || options.module == tsr_core::ModuleKind::System
            }
        };
        self.strict_property_initialization =
            options.strict_option_value(options.strict_property_initialization);
        self.no_implicit_override = options.no_implicit_override.is_true();
        self.no_implicit_returns = options.no_implicit_returns.is_true();
        self.use_unknown_in_catch_variables =
            options.strict_option_value(options.use_unknown_in_catch_variables);
        self.no_implicit_any = options.strict_option_value(options.no_implicit_any);
        self.uses_wildcard_types =
            options.types.as_ref().is_some_and(|types| types.iter().any(|t| t == "*"));
        self.lib_includes_dom = options
            .lib
            .iter()
            .any(|lib| lib.eq_ignore_ascii_case("dom") || lib.eq_ignore_ascii_case("lib.dom.d.ts"));

        // `getJsxNamespace`'s three-way default (`jsx.go:1372-1382`): `React`,
        // unless `jsxFactory` names an entity — in which case its **first**
        // identifier, so `h.createElement` gives `h` — or `reactNamespace`
        // names one outright. `jsxFactory` wins over `reactNamespace`.
        self.jsx_emit = options.jsx;
        self.jsx_namespace = if options.jsx_factory.is_empty() {
            if options.react_namespace.is_empty() {
                "React".to_string()
            } else {
                options.react_namespace.clone()
            }
        } else {
            // `GetFirstIdentifier(parseIsolatedEntityName(…))`. The entity is a
            // dotted name and only its root is the namespace; a factory that
            // does not parse leaves the default, `React` (`jsx.go:1376`).
            crate::jsx_factory::isolated_entity_name_root(&options.jsx_factory)
                .unwrap_or("React")
                .to_string()
        };
        // `getJsxFragmentFactoryEntity`'s option arm (`jsx.go:1431`).
        self.jsx_fragment_namespace =
            crate::jsx_factory::isolated_entity_name_root(&options.jsx_fragment_factory)
                .map(str::to_string);
        // `GetJSXTransformEnabled` (`compileroptions.go`): the three emits that
        // call a factory.
        let jsx_transform = matches!(
            options.jsx,
            tsr_core::JsxEmit::React | tsr_core::JsxEmit::ReactJsx | tsr_core::JsxEmit::ReactJsxDev
        );
        self.jsx_fragment_factory_missing = (jsx_transform
            && options.jsx_fragment_factory.is_empty())
        .then_some(!options.jsx_factory.is_empty());

        // `== TSTrue` (`checker.go:6115`) — `strict` does not reach it.
        self.no_unchecked_indexed_access = options.no_unchecked_indexed_access.is_true();
        self.exact_optional_property_types = options.exact_optional_property_types.is_true();
        self.language_version = options.emit_script_target();

        // `unusedIsError` (`checker.go:7104`), both `IsTrue()`. An unset option
        // reports nothing at all, which is what confines the unused family to the
        // cases that ask for it.
        self.no_unused_locals = options.no_unused_locals.is_true();
        self.no_unused_parameters = options.no_unused_parameters.is_true();

        // Two reads of one option, deliberately not each other's negation.
        self.allow_unreachable_code = options.allow_unreachable_code.is_true();
        self.unreachable_code_is_error = options.allow_unreachable_code.is_false();

        // `ShouldPreserveConstEnums`, which folds in `isolatedModules` — and, via
        // `GetIsolatedModules`, `verbatimModuleSyntax` too.
        self.preserve_const_enums = options.should_preserve_const_enums();
        self.isolated_modules = options.get_isolated_modules();
        self.isolated_modules_option = options.isolated_modules.is_true();
        self.verbatim_module_syntax = options.verbatim_module_syntax.is_true();
        self.module_format_options =
            crate::module_format::ModuleFormatOptions::from_options(options);

        // `IsTrueOrUnknown` (`checker.go:5321`): on unless explicitly off.
        self.no_unchecked_side_effect_imports =
            options.no_unchecked_side_effect_imports.is_true_or_unknown();
    }

    /// Set [`Checker::strict_null_checks`] from a case's compiler options.
    ///
    /// Called by the conformance harness before any type is created — the
    /// union constructor consults the flag at build time, so flipping it after
    /// types exist would leave a mixed store.
    pub fn set_strict_null_checks(&mut self, on: bool) {
        self.strict_null_checks = on;
        self.intrinsics.select_strict_null_checks(on);
        // `checker.go:1345`: the synthesised `undefined` symbol's type is
        // `undefinedWideningType`, which the flag just selected.
        if let Some(undefined) = self.binder.undefined_symbol() {
            self.symbol_types.insert(undefined, self.intrinsics.undefined_widening);
        }
    }

    /// Set [`Checker::no_unchecked_side_effect_imports`] from a case's compiler
    /// options.
    ///
    /// Upstream reads `c.compilerOptions.NoUncheckedSideEffectImports.IsTrueOrUnknown()`
    /// (`checker.go:5321`), so **unset means on** — the field's default. Only an
    /// explicit `false` turns the side-effect-import diagnostic off, which is
    /// what `compiler/ambientExportDefaultErrors` writes and what made this
    /// setter necessary rather than optional.
    pub fn set_no_unchecked_side_effect_imports(&mut self, on: bool) {
        self.no_unchecked_side_effect_imports = on;
    }

    /// Set [`Checker::no_unchecked_indexed_access`] from a case's compiler
    /// options.
    pub fn set_no_unchecked_indexed_access(&mut self, on: bool) {
        self.no_unchecked_indexed_access = on;
    }

    /// Set [`Checker::use_unknown_in_catch_variables`] from a case's compiler
    /// options.
    pub fn set_use_unknown_in_catch_variables(&mut self, on: bool) {
        self.use_unknown_in_catch_variables = on;
    }

    /// Set [`Checker::strict_property_initialization`] from a case's compiler
    /// options.
    ///
    /// `GetStrictOptionValue` (`checker.go:922`): the explicit flag wins, then
    /// `@strict`, and the default is on — the same rule the harness applies to
    /// `strictNullChecks`.
    pub fn set_strict_property_initialization(&mut self, on: bool) {
        self.strict_property_initialization = on;
    }

    /// Set [`Checker::allow_unreachable_code`] from a case's compiler options.
    ///
    /// Read as `IsTrue()` upstream (`checker.go:12534`), so an unset option is
    /// `false` and the comma-operator diagnostic fires.
    pub fn set_allow_unreachable_code(&mut self, on: bool) {
        self.allow_unreachable_code = on;
    }

    /// Set [`Checker::unreachable_code_is_error`] from a case's compiler
    /// options.
    ///
    /// `unreachableCodeIsError` (`binder.go`) is
    /// `AllowUnreachableCode == core.TSFalse` — **explicitly** false, not
    /// merely unset. With the option unset upstream emits a *suggestion*, which
    /// never reaches a `.errors.txt`. This is the third state
    /// [`Checker::allow_unreachable_code`]'s `bool` cannot carry, and reading
    /// unset as `false` would report TS7027 on every case in the corpus with
    /// dead code — `checker-notes-diag2.md` §82.
    pub fn set_unreachable_code_is_error(&mut self, on: bool) {
        self.unreachable_code_is_error = on;
    }

    /// Set [`Checker::preserve_const_enums`] from a case's compiler options.
    ///
    /// `ShouldPreserveConstEnums` is `PreserveConstEnums.IsTrue() ||
    /// IsolatedModules.IsTrue()` (`core/compileroptions.go`), an `IsTrue()`
    /// option and therefore **false when unset** — not a strict option, whose
    /// unset state is true (`GetStrictOptionValue`, `core/compileroptions.go:294`;
    /// the three option shapes are tabulated in `checker-notes-diag2.md` §84).
    ///
    /// It decides two questions and both are `isSourceElementUnreachable`'s
    /// (`checker.go:2455`): whether a `const enum` is code, and whether a
    /// namespace containing only `const enum`s is instantiated.
    pub fn set_preserve_const_enums(&mut self, on: bool) {
        self.preserve_const_enums = on;
    }

    /// Set `noUnusedLocals` / `noUnusedParameters` from a case's compiler
    /// options.
    ///
    /// Both read as `IsTrue()` upstream (`checker.go:7104`), so unset is off and
    /// [`crate::unused`] produces nothing at all — which is what bounds that
    /// family's blast radius to the cases that opt in.
    pub fn set_no_unused(&mut self, locals: bool, parameters: bool) {
        self.no_unused_locals = locals;
        self.no_unused_parameters = parameters;
    }

    /// Declare which files this program checks, before checking any of them.
    ///
    /// The bundled `lib.*.d.ts` are in the program and are never walked
    /// (`diagnostics_suite::from_check_traversal` says so and says why), so a
    /// declaration in a file outside this set is a **library** declaration.
    /// `crate::nonexistent_property` needs that distinction: upstream reports
    /// TS2550 / TS2812 rather than TS2339 for a missing member of a lib type,
    /// and this port models neither.
    ///
    /// It must be called **before** the first `check_source_file`, because the
    /// set is a property of the program and not of the file being walked —
    /// deriving it from the files walked so far would make the answer depend on
    /// the order.
    pub fn set_checked_files(&mut self, files: impl IntoIterator<Item = NodeId>) {
        self.checked_files = files.into_iter().collect();
    }

    /// Set `noImplicitAny` from a case's compiler options.
    ///
    /// A strict-family option: the explicit flag wins, `@strict` is the
    /// fallback. Unset is **off** here rather than on, because this port reads
    /// the corpus's directives and the corpus writes `@strict` where it means
    /// it — the same three-step read the harness makes for
    /// `strictPropertyInitialization`, with a different default because
    /// `noImplicitAny` is not on by default outside `strict`.
    pub fn set_no_implicit_any(&mut self, on: bool) {
        self.no_implicit_any = on;
    }

    /// The well-known types.
    #[must_use]
    pub fn intrinsics(&self) -> &Intrinsics {
        &self.intrinsics
    }

    /// Read a type's contents.
    ///
    /// The only way in: [`TypeId`] is a handle and the store owns the type.
    #[must_use]
    pub fn type_of(&self, id: TypeId) -> &crate::types::Type {
        self.store.get(id)
    }

    /// Read-only view of [`Checker::type_reference_targets`] — the
    /// `(symbol, arguments)` pair a reference was interned on.
    ///
    /// Exposed for **measurement**: a counterfactual probe outside this crate
    /// (`crates/tsr-conformance/examples/infergen.rs`) has to decompose a
    /// reference the same way substitution does, and reconstructing the pair
    /// from the printed text would forecast a different algorithm than the one
    /// being sized. Visibility only — nothing here computes anything.
    #[must_use]
    pub fn type_reference_target(&self, id: TypeId) -> Option<&(SymbolId, Vec<TypeId>)> {
        self.type_reference_targets.get(&id)
    }

    /// Read-only view of [`Checker::signature_types`] — the signatures a baked
    /// function-shaped type was rendered from.
    ///
    /// The sibling of [`Checker::type_reference_target`], and exposed for the
    /// same measurement reason.
    #[must_use]
    pub fn signatures_of_type(&self, id: TypeId) -> Option<&Vec<crate::signatures::Signature>> {
        self.signature_types.get(&id)
    }

    /// A declared type parameter's name as the node builder allocates it at a
    /// print site — `typeParameterToName` (`nodebuilderimpl.go:1404`) under
    /// `GenerateNamesForShadowedTypeParams`, which the `.types` writer passes
    /// with the assertion's parent as the enclosing declaration
    /// (`type_symbol_baseline.go:394`).
    ///
    /// The written name is kept unless it is already allocated to a different
    /// parameter in the enclosing signature renders (the by-text set, modelled
    /// by [`Checker::render_type_parameter_scope`]) or it resolves at the
    /// enclosing declaration to a different type parameter
    /// (`typeParameterShadowsOtherTypeParameterInScope`, `:1396`); then the
    /// first free `name_n` is used. A parameter already allocated in an
    /// enclosing render answers that allocation — upstream's by-id cache.
    /// `docs/architecture/checker-99-shadowed-names.md`.
    pub(crate) fn type_parameter_name_at(
        &mut self,
        id: TypeId,
        symbol: SymbolId,
        reference: NodeId,
    ) -> String {
        let enclosing = self.nodes.parent(reference).unwrap_or(reference);
        self.allocate_type_parameter_name(id, symbol, enclosing)
    }

    /// Chain-depth cap for [`Checker::symbol_chain`]. Upstream has none.
    const MAX_SYMBOL_CHAIN: usize = 8;

    /// Render a type as a `.types` baseline would print it.
    #[must_use]
    pub fn type_to_string(&self, id: TypeId) -> String {
        printing::type_to_string(self.store.get(id))
    }

    /// Render a type **as seen from a particular reference site**.
    ///
    /// The second entry point beside [`Checker::type_to_string`], which is left
    /// exactly as it was. `type_to_string` has 110 call sites across five
    /// checker modules and a dozen test files; changing its signature would put
    /// a cross-cutting refactor in the same commit as a behaviour change, and
    /// would collide with two agents editing those files. More importantly the
    /// split makes the property **structural rather than maintained**: a caller
    /// with no reference node cannot get a context-sensitive name, so no call
    /// site can regress by omission.
    ///
    /// # Why a name can depend on the reference site at all
    ///
    /// Upstream does not read the name off the declaration. `symbolToTypeNode`
    /// goes through `NodeBuilderImpl.lookupSymbolChain`
    /// (`internal/checker/nodebuilderimpl.go:1061`) to
    /// `Checker.getAccessibleSymbolChain`
    /// (`internal/checker/symbolaccessibility.go:373`), whose `trySymbolTable`
    /// iterates **the alias symbols of every table in scope from the reference**
    /// and returns the name of one that resolves to the target. A module
    /// symbol's own name is its file path, so the printed form is always some
    /// alias's name and never the symbol's.
    ///
    /// # Returns `None` rather than guessing
    ///
    /// `None` means *this port cannot name this type here*, and the caller
    /// renders a gap. That is the whole safety property of this function, and it
    /// is why the answer is an `Option` rather than a fallback string: the baked
    /// text for a module object is the stripped file path, so falling back to it
    /// would turn every unnameable case into a confidently wrong line — the
    /// exact outcome that got `bd tsr-6ph` refused twice, at 2.1 and 2.5 wrong
    /// per right.
    ///
    /// Ambiguity is the case that forces it. When two aliases in scope name one
    /// module the corpus contradicts itself:
    /// `compiler/es6ImportNameSpaceImport` prints the *earlier* alias for a
    /// later one, while `compiler/unusedImports_entireImportDeclaration` prints
    /// each of `ns`, `ns2`, `ns3` under its *own* name. Upstream distinguishes
    /// them through `cloneTypeAsModuleType`, which this port does not have — so
    /// no tie-break reproduces both, and both are answered `None`. Measured over
    /// the corpus: 706 lines print a module object's name, **634 have exactly
    /// one alias in scope and 630 of those name correctly (99.4%)**, and the 72
    /// ambiguous ones gap. See
    /// [`docs/architecture/checker-notes-nameres.md`](../../../docs/architecture/checker-notes-nameres.md)
    /// §14.
    #[must_use]
    pub fn type_to_string_at(&mut self, id: TypeId, reference: NodeId) -> Option<String> {
        self.render_type_parameter_names.depth += 1;
        let result = self.type_to_string_at_worker(id, reference);
        self.render_type_parameter_names.depth -= 1;
        if self.render_type_parameter_names.depth == 0 {
            self.render_type_parameter_names.allocations.clear();
        }
        result
    }

    fn type_to_string_at_worker(&mut self, id: TypeId, reference: NodeId) -> Option<String> {
        // typeToTypeNodeHelper reduces before printing (nodebuilderimpl.go:3228).
        let id = self.get_reduced_type(id);
        if let Some(&symbol) = self.type_parameter_symbols.get(&id) {
            return Some(self.type_parameter_name_at(id, symbol, reference));
        }
        // Native createAnonymousTypeNodeEx checks the visited semantic identity
        // before expanding completed signatures again. Replaying a baked
        // signature here would add an extra recursive layer on every print.
        if self.rendering_composites.contains(&id)
            && self.signature_types.contains_key(&id)
            && matches!(self.store.get(id).data, crate::types::TypeData::Anonymous { .. })
            && !self.alias_named_signature_types.contains(&id)
        {
            return Some(
                self.recursive_callable_text_at(id, reference).unwrap_or_else(|| "any".to_string()),
            );
        }
        if let crate::types::TypeData::Anonymous { symbol, .. } = self.store.get(id).data
            && self.binder.symbols().get(symbol).flags.contains(SymbolFlags::MODULE_EXPORTS)
        {
            return self.module_exports_text_at(id, symbol, reference);
        }
        if let Some(&(_, source)) = self.module_value_clones.get(&id) {
            let crate::types::TypeData::Anonymous { symbol, .. } = self.store.get(source).data
            else {
                return None;
            };
            if !self
                .binder
                .symbols()
                .get(symbol)
                .flags
                .intersects(SymbolFlags::CLASS | SymbolFlags::VALUE_MODULE)
            {
                if !self.rendering_composites.insert(id) {
                    return None;
                }
                let out = self.callable_object_to_string_at(id, reference);
                self.rendering_composites.remove(&id);
                return out;
            }
            return self.module_clone_name_at(id, reference).map(|name| format!("typeof {name}"));
        }
        if let Some(text) = self.export_equals_class_text_at(id, reference) {
            return Some(text);
        }
        // A cloned module alias cannot name the original class instance or
        // constructor. The published original constructor's export= container
        // is handled above; inaccessible instance routes remain unsupported.
        let original = match self.store.get(id).data {
            crate::types::TypeData::Named { members, .. } => members,
            crate::types::TypeData::Anonymous { symbol, .. } => Some(symbol),
            _ => None,
        };
        if let Some(original) = original {
            let original = self.resolve_alias_fully(original);
            let has_clone = self.module_value_clones.values().any(|&(_, source)| {
                matches!(self.store.get(source).data,
                    crate::types::TypeData::Anonymous { symbol, .. }
                        if self.binder.merged_symbol(symbol) == original
                            && self.binder.symbols().get(symbol).flags
                                .intersects(SymbolFlags::CLASS | SymbolFlags::VALUE_MODULE))
            });
            if has_clone && self.best_name(original, reference).is_none() {
                return None;
            }
        }
        let module = match &self.store.get(id).data {
            crate::types::TypeData::Anonymous { symbol, .. } => {
                let symbol = *symbol;
                // §501 — `bd tsr-e2u`'s naming half: a NAMESPACE object
                // (`typeof __React` reached through
                // `declare module "react" { export = __React }`) renders
                // through the accessibility walk — `best_name`'s per-table
                // direct-hit-then-alias priority is `trySymbolTable`'s own
                // order, so a site importing the module prints the ALIAS
                // (`typeof React`) while a site where the namespace's own
                // name is innermost keeps it (`typeof N`, the case §219's
                // record named as the widening's regression — protected by
                // the direct hit, not by a gate). No name found falls back
                // to the baked text, exactly what every such site printed
                // before the interception existed.
                if !self.is_module_symbol(symbol)
                    && !self.is_ambient_module(symbol)
                    && self
                        .binder
                        .symbols()
                        .get(symbol)
                        .flags
                        .intersects(SymbolFlags::VALUE_MODULE | SymbolFlags::NAMESPACE_MODULE)
                    && let Some(name) = self.export_equals_alias_name_at(symbol, reference)
                {
                    return Some(format!("typeof {name}"));
                }
                // Ambient modules (`declare module "x"`) resolve since the
                // `tryFindAmbientModule` arm (`checker-notes-modobj.md` §10)
                // and must take the same interception: their baked text is the
                // module's own name, which is never what upstream prints.
                (self.is_module_symbol(symbol) || self.is_ambient_module(symbol)).then_some(symbol)
            }
            _ => None,
        };
        let Some(module) = module else {
            if let Some(text) = self.type_literal_text_at(id, reference) {
                return Some(text);
            }
            if let Some(text) = self.object_literal_text_at(id, reference, true) {
                return Some(text);
            }
            if self.signature_types.contains_key(&id)
                && self.anonymous_properties.contains_key(&id)
                && !self.rendering_composites.contains(&id)
                && !self.alias_named_signature_types.contains(&id)
            {
                self.rendering_composites.insert(id);
                let out = self.callable_object_to_string_at(id, reference);
                self.rendering_composites.remove(&id);
                return out;
            }
            // The composite-print twin (`checker-notes-modobj.md` §10.13): a
            // single-signature type re-renders from its structure at the site,
            // so embedded named types take their qualifiers and renames. The
            // visiting set is the cycle guard a self-referential function type
            // needs: re-entry uses the completed semantic callable's native
            // typeof/anonymous fallback above, without reading baked text.
            if let Some(signatures) = self.signature_types.get(&id)
                && signatures.len() == 1
                && !self.rendering_composites.contains(&id)
                && !self.alias_named_signature_types.contains(&id)
            {
                let signature = signatures[0].clone();
                self.rendering_composites.insert(id);
                let out = self.signature_to_string_at(&signature, reference);
                self.rendering_composites.remove(&id);
                return Some(out);
            }
            // §99 (`checker-notes-narrow.md`): the MULTI-signature type-literal
            // form renders at the site too — an overloaded member's embedded
            // names were staying bare (`{ (roundTo: PluralizeUnit<...>):
            // Duration; ... }` wanting `Temporal.`-qualified slots).
            if let Some(signatures) = self.signature_types.get(&id)
                && signatures.len() > 1
                && !self.rendering_composites.contains(&id)
            {
                let signatures = signatures.clone();
                self.rendering_composites.insert(id);
                // §102 (decoded): shadow renames against the SITE, byText
                // claims across the signatures of ONE render.
                let mut claimed = rustc_hash::FxHashSet::default();
                let mut site_renamed = false;
                let mut out = String::from("{ ");
                for signature in &signatures {
                    let names_depth = self.render_type_parameter_names.allocations.len();
                    let rendered = self.rename_type_parameters_for_site(
                        signature.clone(),
                        reference,
                        &mut claimed,
                    );
                    site_renamed |= signature
                        .type_parameters
                        .iter()
                        .map(|parameter| &parameter.name)
                        .ne(rendered.type_parameters.iter().map(|parameter| &parameter.name));
                    // Each overload is its own `enterNewScope`: its allocated
                    // names are visible to its slots and to no sibling.
                    let scope_depth = self.render_type_parameter_scope.len();
                    self.push_render_type_parameter_scope(&rendered);
                    out.push_str(&self.signature_member_text_at(&rendered, reference));
                    self.render_type_parameter_scope.truncate(scope_depth);
                    self.render_type_parameter_names.allocations.truncate(names_depth);
                    out.push_str("; ");
                }
                out.push('}');
                self.rendering_composites.remove(&id);
                // An instantiated signature set may carry a correct print-only
                // rename that is absent from its stored semantic signatures.
                // Keep that baked spelling at neutral sites; rebuild only when
                // this site adds a shadow allocation of its own.
                if !self.alias_named_signature_types.contains(&id) || site_renamed {
                    return Some(out);
                }
            }
            // §97 (`checker-notes-narrow.md`): a union carrying ORIGIN
            // entries re-renders each entry at the site — the alias-named
            // entries qualify through the same stack §95 uses; any decline
            // keeps the baked spelling.
            // Native nodebuilderimpl.go:3362 chooses this union's own alias
            // before rendering its origin. Its nested provenance belongs to
            // filtering, not to the spelling of an unchanged aliased union.
            if let Some(entries) = self.union_origin.get(&id).cloned()
                && matches!(
                    self.store.get(id).data,
                    crate::types::TypeData::Union { symbol: None, .. }
                )
                && !self.rendering_composites.contains(&id)
            {
                self.rendering_composites.insert(id);
                let mut parts = Vec::with_capacity(entries.len());
                let mut complete = true;
                let multiple = entries.len() > 1;
                // formatUnionTypes (printer.go:383) orders the printed
                // entries: `null` then `undefined` last, booleans collapsed.
                for part in crate::unions::union_print_parts(&self.store, &entries) {
                    let entry = match part {
                        crate::unions::UnionPrintPart::Type(entry) => entry,
                        crate::unions::UnionPrintPart::Keyword(keyword) => {
                            parts.push(keyword.to_string());
                            continue;
                        }
                    };
                    if let Some(part) = self.type_to_string_at(entry, reference) {
                        let parenthesised =
                            crate::unions::union_constituent_needs_parentheses(&self.store, entry);
                        parts.push(if multiple && parenthesised {
                            format!("({part})")
                        } else {
                            part
                        });
                    } else {
                        complete = false;
                        break;
                    }
                }
                self.rendering_composites.remove(&id);
                if complete {
                    return Some(parts.join(" | "));
                }
            }
            // Inferred unions have no written origin, but their semantic slots
            // still need the site's alias, qualifier and parameter allocation.
            if let Some(out) = self.union_text_at(id, reference) {
                return Some(out);
            }
            // §95 (`checker-notes-narrow.md`): a GENERIC reference re-renders
            // its ARGUMENT slots at the site — the baked argument text was
            // minted at creation (inside-view), and `split_around_name`'s
            // qualifier only ever touches the outer name
            // (`Temporal.PartialTemporalLike<ZonedDateTimeLikeObject>` wanting
            // the argument qualified too). Any piece declining falls back to
            // the baked road below.
            // **The zero-argument case is NOT worth widening this gate for**:
            // dropping `!arguments.is_empty()` measures *no transitions vs
            // baseline* over the whole corpus (§535). A non-generic reference
            // already reaches the identical text through `qualified_name_at`
            // below, because `split_around_name` fires exactly where the
            // printed form IS the symbol's own name — which is every
            // zero-argument reference. Reverted per §515; recorded so the next
            // reader does not re-run it.
            // ADR-0045 rule 5: the node builder's alias arm
            // (`nodebuilderimpl.go:3362`) prints `Alias<Args>` from the alias
            // symbol before any structure, named at the site like any
            // reference (`reference_text_at`'s chain / rename / qualifier).
            if let Some((alias, alias_arguments)) = self.alias_of.get(&id).cloned()
                && !self.rendering_composites.contains(&id)
            {
                self.rendering_composites.insert(id);
                let rebuilt = self.reference_text_at(alias, &alias_arguments, reference);
                self.rendering_composites.remove(&id);
                if let Some(out) = rebuilt {
                    return Some(out);
                }
            }
            if let Some((target, arguments)) = self.type_reference_targets.get(&id).cloned()
                && !arguments.is_empty()
                && !self.rendering_composites.contains(&id)
            {
                self.rendering_composites.insert(id);
                // §136: a default-filled reference prints its WRITTEN arity.
                let shown =
                    self.reference_display_arity.get(&id).copied().unwrap_or(arguments.len());
                let rebuilt = self.reference_text_at(target, &arguments[..shown], reference);
                self.rendering_composites.remove(&id);
                if let Some(out) = rebuilt {
                    return Some(out);
                }
            }
            let printed = self.type_to_string(id);
            return self.qualified_name_at(id, printed, reference);
        };
        if let Some(name) = self.module_name_at(module, reference, SymbolFlags::VALUE) {
            // SS197 (measured -66 cases, reverted): qualifying with
            // `globalThis.` when the bare name resolves to a DIFFERENT symbol
            // at the reference is far too broad — 12 cases want the
            // qualification and the same test fires on ~78 that do not.
            // Upstream's rule is `getSymbolChain`'s ACCESSIBILITY walk, which
            // asks whether the symbol is reachable by its own name from the
            // reference's scope chain, not merely whether some other symbol
            // shares the spelling. The `collisionCodeGenModuleWith*` family
            // (12 deficit-1 cases) needs that walk, not a shadowing probe.
            return Some(format!("typeof {name}"));
        }
        // §143 slice 1 (`checker-notes-narrow.md`): a container NO alias
        // reaches spells the import form — `getSpecifierForModuleSymbol`'s
        // AMBIENT half: a module declared ONCE as `declare module "name"`
        // prints `typeof import("name")` verbatim (privacyImportParseErrors'
        // wants). The NONE case only — an ambiguous container means upstream
        // picked some alias — since §533 this port picks the same one, so
        // "no alias at all" is simply `None`; AUGMENTED ambients (2+
        // declarations, moduleAugmentationExtend*'s bare-name wants) gate
        // out; FILE modules wait for the relative-specifier half.
        if self.module_alias_at(module, reference, SymbolFlags::VALUE).is_none() {
            return self
                .module_specifier_for_symbol(module, reference)
                .map(|specifier| format!("typeof import({specifier})"));
        }
        None
    }

    /// `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:1249`), quoted, for
    /// the module forms this port spells: the module's **source file**
    /// declaration if it has one (`GetDeclarationOfKind(symbol,
    /// KindSourceFile)`), else its ambient name.
    ///
    /// Any declaration, not only a sole one: a module augmented by
    /// `declare module "x"` carries the augmentation's declarations too
    /// (`mergeModuleAugmentation`, `docs/parity/notes/names-modules.md` §4),
    /// and upstream still finds the file, or reads the ambient name off the
    /// symbol.
    ///
    /// - **Ambient** (§143 slice 1, `checker-notes-narrow.md`): `declare module
    ///   "name"` prints `"name"` verbatim — escaped via the shared `quote`
    ///   (SS196). Read from the first string-named module declaration.
    /// - **File**: `GetModuleSpecifiers` over the module's file —
    ///   the existing import, the `node_modules` package name, else the
    ///   relative path through `processEnding`
    ///   ([`Checker::module_specifier_for_file`], r5-modules2 §2). With no
    ///   host path, the module symbol's name (the path with its extension
    ///   stripped, `bind_source_file_as_external_module`) is spelled
    ///   `./name` when it sits at the root.
    ///
    /// What the printers write inside `import(…)`: `symbolToTypeNode`'s
    /// import-type arm over this function, with its `resolution-mode`
    /// attribute ([`Checker::import_type_argument`], r5-modules2 §3.2).
    pub(crate) fn module_specifier_for_symbol(
        &self,
        module: SymbolId,
        reference: NodeId,
    ) -> Option<String> {
        self.import_type_argument(module, reference)
    }

    /// [`Checker::module_specifier_for_symbol`] under `getSpecifierForModuleSymbol`'s
    /// `overrideImportMode` (`ModuleKind::None` for none): the mode the
    /// existing-import, `node_modules` and ending choices read in place of the
    /// importing file's default (r5-modules §6).
    pub(crate) fn module_specifier_for_symbol_in_mode(
        &self,
        module: SymbolId,
        reference: NodeId,
        override_mode: tsr_core::ModuleKind,
    ) -> Option<String> {
        let symbol = self.binder.symbols().get(module);
        // `tryGetModuleNameFromAmbientModule` (`modulespecifiers/specifiers.go:107`),
        // which `GetModuleSpecifiersWithInfo` asks before any path: a
        // string-named module declaration names the module, unless it is an
        // external augmentation spelled with a relative name.
        if let Some(name) = symbol.declarations.iter().find_map(|&declaration| {
            let Some(tsr_ast::Node::ModuleDeclaration(node)) = self.node_map.get(declaration)
            else {
                return None;
            };
            let Some(tsr_ast::ModuleName::StringLiteral(literal)) = node.name else { return None };
            (!(tsr_path::is_external_module_name_relative(literal.text)
                && self.is_module_augmentation_external(declaration)))
            .then_some(literal.text)
        }) {
            return Some(crate::printing::quote(name));
        }
        if let Some(&file) = symbol
            .declarations
            .iter()
            .find(|&&declaration| self.nodes.kind(declaration) == SyntaxKind::SourceFile)
        {
            // The file arm: `GetModuleSpecifiers` over the module's file
            // (`crate::module_specifiers`, r5-modules2 §2).
            if let Some(from) = self.source_file_of(reference)
                && self.module_host.is_some_and(|host| host.file_path(from).is_some())
            {
                return self
                    .module_specifier_for_file(file, from, override_mode)
                    .map(|specifier| crate::printing::quote(&specifier));
            }
            // With no host path, the module symbol's name (the path with its
            // extension stripped, `bind_source_file_as_external_module`) is
            // spelled `./name` when it sits at the root, the one directory
            // layout most of the corpus mounts.
            let relative = symbol.name.strip_prefix('/')?;
            return (!relative.contains('/') && !relative.is_empty())
                .then(|| format!("\"./{relative}\""));
        }
        None
    }

    /// `ast.IsModuleAugmentationExternal` (`utilities.go:1694`): a module
    /// declaration at the top level of an external module, or inside a
    /// top-level ambient module of a script.
    fn is_module_augmentation_external(&self, declaration: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(declaration) else { return false };
        match self.nodes.kind(parent) {
            SyntaxKind::SourceFile => self.binder.symbol_of(parent).is_some(),
            SyntaxKind::ModuleBlock => {
                let Some(outer) = self.nodes.parent(parent) else { return false };
                let Some(file) = self.nodes.parent(outer) else { return false };
                matches!(self.node_map.get(outer), Some(tsr_ast::Node::ModuleDeclaration(module))
                    if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_))))
                    && self.nodes.kind(file) == SyntaxKind::SourceFile
                    && self.binder.symbol_of(file).is_none()
            }
            _ => false,
        }
    }

    /// Pinned tsgo 5b1047d shouldWriteTypeOfFunctionSymbol: typeof is admitted
    /// for a static method or nonlocal function only on recursive re-entry.
    /// Completed declaration metadata supplies identity, not spelling. The
    /// native baseline's enclosing declaration is the assertion's parent;
    /// top-level expression/arrow symbols rewrite to their variable except
    /// when that variable is the enclosing declaration being printed. Existing
    /// accessibility/alias naming owns qualification; no site result is cached.
    fn recursive_callable_text_at(&mut self, id: TypeId, reference: NodeId) -> Option<String> {
        let mut symbol = self.completed_callable_symbol(id)?;
        let record = self.binder.symbols().get(symbol);
        let static_method = record.flags.contains(SymbolFlags::METHOD)
            && record.declarations.iter().any(|&declaration| {
                matches!(self.node_map.get(declaration), Some(Node::MethodDeclaration(node))
                    if tsr_ast::has_syntactic_modifier(node.modifiers, SyntaxKind::StaticKeyword)
                        && matches!(node.name, tsr_ast::PropertyName::Identifier(_)))
            });
        let mut nonlocal = record.flags.contains(SymbolFlags::FUNCTION) && record.parent.is_some();
        let mut expression_parent = None;
        if record.flags.contains(SymbolFlags::FUNCTION) && record.parent.is_none() {
            for &declaration in &record.declarations {
                let Some(parent) = self.nodes.parent(declaration) else { continue };
                if matches!(
                    self.nodes.kind(parent),
                    SyntaxKind::SourceFile | SyntaxKind::ModuleBlock
                ) {
                    nonlocal = true;
                    break;
                }
                if matches!(
                    self.nodes.kind(declaration),
                    SyntaxKind::FunctionExpression | SyntaxKind::ArrowFunction
                ) && self.nodes.kind(parent) == SyntaxKind::VariableDeclaration
                    && let Some(list) = self.nodes.parent(parent)
                    && self.nodes.kind(list) == SyntaxKind::VariableDeclarationList
                    && let Some(statement) = self.nodes.parent(list)
                    && self.nodes.kind(statement) == SyntaxKind::VariableStatement
                    && self.nodes.parent(statement).is_some_and(|scope| {
                        matches!(
                            self.nodes.kind(scope),
                            SyntaxKind::SourceFile | SyntaxKind::ModuleBlock
                        )
                    })
                {
                    nonlocal = true;
                    let named = matches!(self.node_map.get(declaration), Some(Node::FunctionExpression(node)) if node.name.is_some());
                    expression_parent = Some((parent, named));
                    break;
                }
            }
        }
        if !static_method && !nonlocal {
            return None;
        }
        // Anonymous-expression symbols in this binder have declarations but no
        // value_declaration. Native getNameOfSymbol also takes the assigned
        // variable's name for an unnamed expression/arrow; a written expression
        // name is retained only while printing its own variable declaration.
        if let Some((variable, named)) = expression_parent
            && (!named || Some(variable) != self.nodes.parent(reference))
        {
            symbol = self.binder.merged_symbol(self.binder.symbol_of(variable)?);
        }
        let own = self.binder.symbols().get(symbol).name;
        let name = if static_method {
            let class = self.binder.symbols().get(symbol).parent?;
            let class_name = self.value_symbol_name_at(class, reference);
            format!("{class_name}.{own}")
        } else {
            self.value_symbol_name_at(symbol, reference)
        };
        Some(format!("typeof {name}"))
    }

    fn value_symbol_name_at(&mut self, symbol: SymbolId, reference: NodeId) -> String {
        let own = self.binder.symbols().get(symbol).name;
        if let Some(name) = self.best_name(symbol, reference)
            && name != own
        {
            return name;
        }
        // `getNameOfSymbolAsWritten`: the declaration's name as written.
        let own = self.symbol_name_as_written(symbol);
        match self.symbol_chain(symbol, reference, SymbolFlags::VALUE, 0) {
            Some(prefix) => format!("{prefix}{own}"),
            None => own,
        }
    }

    /// §99's member-form twin of [`crate::objects::signature_member_text`]:
    /// same slots, same written-text precedence, every RENDERED slot through
    /// [`Checker::type_to_string_at`] with the baked text as the per-slot
    /// fallback — the §10.13 contract, member spelling.
    /// §735 made this `pub(crate)`: the object-literal METHOD arm renders
    /// through it too, so a method member's slots take the same site-rendering
    /// the property members got. Slot-for-slot identical to
    /// [`crate::objects::signature_member_text`] with the `_at` fallback, which
    /// is what makes the swap a superset rather than a second spelling.
    pub(crate) fn signature_member_text_at(
        &mut self,
        signature: &crate::signatures::Signature,
        reference: NodeId,
    ) -> String {
        let mut out = match signature.kind {
            crate::signatures::SignatureKind::Call => String::new(),
            crate::signatures::SignatureKind::Construct
            | crate::signatures::SignatureKind::AbstractConstruct => "new ".to_string(),
        };
        if !signature.type_parameters.is_empty() {
            out.push('<');
            for (index, parameter) in signature.type_parameters.iter().enumerate() {
                if index > 0 {
                    out.push_str(", ");
                }
                if parameter.is_const {
                    out.push_str("const ");
                }
                out.push_str(&parameter.name);
                if let Some(constraint) = parameter.constraint {
                    out.push_str(" extends ");
                    if let Some(text) =
                        self.type_parameter_constraint_text(parameter, constraint, Some(reference))
                    {
                        out.push_str(&text);
                    } else {
                        let rendered = self
                            .type_to_string_at(constraint, reference)
                            .unwrap_or_else(|| self.type_to_string(constraint));
                        out.push_str(&rendered);
                    }
                }
                if let Some(default) = parameter.default {
                    let rendered = self
                        .type_to_string_at(default, reference)
                        .unwrap_or_else(|| self.type_to_string(default));
                    out.push_str(" = ");
                    out.push_str(&rendered);
                }
            }
            out.push('>');
        }
        out.push('(');
        for (index, parameter) in
            signature.this_parameter.iter().chain(signature.parameters.iter()).enumerate()
        {
            if index > 0 {
                out.push_str(", ");
            }
            if parameter.rest {
                out.push_str("...");
            }
            out.push_str(&parameter.name);
            out.push_str(if parameter.optional { "?: " } else { ": " });
            let parameter_type = self.parameter_type(parameter);
            if let Some(text) = parameter.written_text.and_then(|written| {
                self.written_annotation_text_at(written, parameter_type, reference)
            }) {
                out.push_str(&text);
            } else if let Some(text) =
                self.signature_parameter_alias_text_at(signature, parameter, reference)
            {
                out.push_str(&text);
            } else {
                // `serializeTypeForDeclaration`'s fallback serializes the
                // symbol's type, optionality included.
                let serialized = self.serialized_parameter_type(parameter, parameter_type);
                let rendered = self
                    .type_to_string_at(serialized, reference)
                    .unwrap_or_else(|| self.type_to_string(serialized));
                out.push_str(&rendered);
            }
        }
        out.push_str("): ");
        match (self.reused_return_text(signature, Some(reference)), &signature.predicate) {
            (Some(text), _) => out.push_str(&text),
            (None, Some(predicate)) => out.push_str(&self.type_predicate_to_string(predicate)),
            (None, None) => {
                let return_type =
                    self.get_return_type_of_signature(signature).unwrap_or(self.intrinsics.error);
                let rendered = self
                    .signature_return_alias_text_at(signature, reference)
                    .or_else(|| self.type_to_string_at(return_type, reference))
                    .unwrap_or_else(|| self.type_to_string(return_type));
                out.push_str(&rendered);
            }
        }
        out
    }

    /// §95's rebuild: the reference's print from `(target, arguments)` with
    /// every argument rendered AT THE SITE, the target named exactly as
    /// [`Checker::qualified_name_at`]'s tail names it (rename, else chain
    /// qualifier, else bare), and the `Array`/`ReadonlyArray` shorthands
    /// preserved. `None` when any argument cannot be site-rendered.
    pub(crate) fn reference_text_at(
        &mut self,
        target: tsr_binder::SymbolId,
        arguments: &[TypeId],
        reference: NodeId,
    ) -> Option<String> {
        let mut printed_arguments = Vec::with_capacity(arguments.len());
        for &argument in arguments {
            printed_arguments.push(self.type_to_string_at(argument, reference)?);
        }
        if let [element_text] = printed_arguments.as_slice() {
            if self.global_type_symbol("Array") == Some(target) {
                let element = self.wrap_array_element_text(arguments[0], element_text);
                return Some(format!("{element}[]"));
            }
            if self.global_type_symbol("ReadonlyArray") == Some(target) {
                let element = self.wrap_array_element_text(arguments[0], element_text);
                return Some(format!("readonly {element}[]"));
            }
        }
        let target_name = self.binder.symbols().get(target).name;
        let named = if let Some(chain) = self.accessible_type_name_chain_at(target, reference) {
            chain
                .iter()
                .map(|&symbol| self.binder.symbols().get(symbol).name)
                .collect::<Vec<_>>()
                .join(".")
        } else if let Some(better) = self.best_name(target, reference)
            && better != target_name
        {
            better
        } else if let Some(qualifier) =
            // **`TYPE`, not `TYPE | VALUE`** — §7.3's correction, at the second
            // site that had the same bug. This function renders a generic TYPE
            // REFERENCE (`C<T>`), and `symbolToTypeNode`
            // (`internal/checker/nodebuilderimpl.go:649`) passes ONE meaning,
            // which `needsQualification` tests as `flags & meaning`
            // (`symbolaccessibility.go:720`). The union let a Value-only shadow
            // qualify a Type reference: `conformance/noInfer` writes
            // `type Component<Props> = …` and then
            // `declare function doWork<Props>(Component: Component<Props>, …)`,
            // where the PARAMETER named `Component` shadows the type alias in
            // `Value` only — upstream prints `Component<Props>` bare and the
            // union spelled `globalThis.Component<Props>`.
            self.symbol_chain(target, reference, SymbolFlags::TYPE, 0)
        {
            format!("{qualifier}{target_name}")
        } else {
            target_name.to_string()
        };
        if printed_arguments.is_empty() {
            return Some(named);
        }
        Some(format!("{named}<{}>", printed_arguments.join(", ")))
    }

    /// **Design P** — prepend the namespace qualifier a name needs to be read
    /// correctly from `reference`.
    ///
    /// `lookupSymbolChainWorker` (`internal/checker/nodebuilderimpl.go:1066`)
    /// puts every printed name through `getSymbolChain`
    /// (`internal/checker/nodebuilderimpl.go:1087`), which walks up to the
    /// symbol's containers for as long as `needsQualification`
    /// (`internal/checker/symbolaccessibility.go:688`) says the bare name would
    /// not resolve back to this symbol at the reference site. This port bakes a
    /// bare name into the type at creation, so the qualifier is recovered here,
    /// at print time, where the site is known.
    ///
    /// Sized before it was built, in
    /// [`docs/architecture/checker-notes-qualname.md`](../../../docs/architecture/checker-notes-qualname.md)
    /// §10: **2,990 conversions against 14 lines at risk**, the mechanism
    /// `STATUS.md` refused for four cycles on a 3,202-line loss that its own
    /// named cause (`bd tsr-56r`, landed at `3b7fa44`) had already removed.
    ///
    /// # The gate is the design, and it is narrow on purpose
    ///
    /// [`Checker::split_around_name`] fires only where the printed form **is**
    /// the symbol's own name. `checker-notes-nameres.md` §49 records what the
    /// ungated form does: *"without that gate it climbs from an anonymous
    /// `__function` symbol to no parent and gaps every function and object type
    /// in the corpus."* Qualifying names *inside* signature and type-literal
    /// renderings is a different and much worse design — measured in §10 at
    /// 3,859 conversions against **717** lines lost, 5.4:1 against this
    /// mechanism's 213:1 — and it is deliberately not built here.
    fn qualified_name_at(
        &mut self,
        id: TypeId,
        printed: String,
        reference: NodeId,
    ) -> Option<String> {
        let symbol = match &self.store.get(id).data {
            crate::types::TypeData::Named { members, .. } => *members,
            crate::types::TypeData::Anonymous { symbol, .. } => Some(*symbol),
            // A union or intersection that a type alias (or an enum) names
            // prints under that symbol's name — and until the seventh session
            // this match dropped it, which is why a class-typed return slot
            // qualified (`Temporal.Duration`) while an alias-typed parameter
            // slot in the same signature did not (`DurationLike` bare, wanting
            // `Temporal.DurationLike`). `bd tsr-2ghn`'s counterfactual found
            // the asymmetry; the fields have carried the symbol all along.
            crate::types::TypeData::Union { symbol, .. }
            | crate::types::TypeData::Intersection { symbol, .. } => *symbol,
            _ => None,
        };
        let Some(symbol) = symbol else {
            // An enum **member** type carries no symbol by design — the
            // enum-union collapse keys on exactly that, and giving it one
            // regressed two cases (§10.16's mechanism (b), first form,
            // reverted on measurement). The member→enum edge this rename
            // needs already exists in `enum_member_owners`, so the baked
            // `{enum}.` prefix takes the segment rename through the side
            // table instead: `exportAssignmentEnum` wants `EnumE.A` over the
            // baked `E.A`.
            if let Some(&owner) = self.enum_member_owners.get(&id) {
                let owner = self.binder.merged_symbol(owner);
                let owner_name = self.binder.symbols().get(owner).name;
                // Enum member names can be indexed accesses too. Rename the
                // owner at the site without changing the member's literal type
                // or its fresh/regular identity (nodebuilderimpl.go:821).
                let name_start = if printed == owner_name {
                    Some(0)
                } else if printed.starts_with(&format!("(typeof {owner_name})[")) {
                    Some("(typeof ".len())
                } else {
                    None
                };
                if let Some(start) = name_start {
                    let named = self
                        .best_name(owner, reference)
                        .filter(|name| name != owner_name)
                        .unwrap_or_else(|| {
                            match self.symbol_chain(owner, reference, SymbolFlags::TYPE, 0) {
                                Some(prefix) => format!("{prefix}{owner_name}"),
                                None => owner_name.to_string(),
                            }
                        });
                    return Some(format!(
                        "{}{named}{}",
                        &printed[..start],
                        &printed[start + owner_name.len()..]
                    ));
                }
                if printed.len() > owner_name.len()
                    && printed.starts_with(owner_name)
                    && printed.as_bytes()[owner_name.len()] == b'.'
                {
                    if let Some(better) = self.best_name(owner, reference) {
                        if better != owner_name {
                            let mut out = String::with_capacity(printed.len() + better.len());
                            out.push_str(&better);
                            out.push_str(&printed[owner_name.len()..]);
                            return Some(out);
                        }
                        if self.own_name_alias_at(owner, reference) {
                            return Some(printed);
                        }
                    }
                    // §457: the same baked `{enum}.` prefix, owed a
                    // QUALIFIER rather than a rename — the owner segment
                    // takes the chain the bare-name road already builds. An
                    // enum inside a module prints its member `m1.e3.a` at a
                    // site outside `m1` (`es6ModuleConstEnumDeclaration`,
                    // whose neighbouring `x3 : m1.e3` line the enum-TYPE
                    // road qualifies while the member's baked prefix never
                    // passed through `symbol_chain`).
                    if let Some(qualifier) = self.symbol_chain(
                        owner,
                        reference,
                        SymbolFlags::TYPE | SymbolFlags::VALUE,
                        0,
                    ) {
                        let mut out = String::with_capacity(printed.len() + qualifier.len());
                        out.push_str(&qualifier);
                        out.push_str(&printed);
                        return Some(out);
                    }
                }
            }
            return Some(printed);
        };
        let name = self.binder.symbols().get(symbol).name;
        let Some(suffix_at) = Self::split_around_name(&printed, name) else {
            return Some(printed);
        };
        // The RENAME (`checker-notes-modobj.md` §10.8): the innermost
        // accessible name for the symbol may be an *alias's* — `typeof React`
        // for a type whose symbol is the global `__React`, because the file's
        // own table holds the alias and upstream's scope walk stops at the
        // first table that reaches the symbol. Measured over every printed
        // line in the corpus before building: it changes zero of them — its
        // whole population is lines that gap today.
        if let Some(better) = self.best_name(symbol, reference) {
            if better != name {
                let mut out = String::with_capacity(printed.len() + better.len());
                out.push_str(&printed[..suffix_at - name.len()]);
                out.push_str(&better);
                out.push_str(&printed[suffix_at..]);
                return Some(out);
            }
            if self.own_name_alias_at(symbol, reference) {
                return Some(printed);
            }
        }
        // The §10.9 segment rename, applied to a **baked** prefix: an enum
        // member's text is created as `{enum}.{member}` (`declared.rs`,
        // `get_declared_type_of_enum`), so the container's name never passes
        // through `symbol_chain`'s per-segment `best_name`. When the printed
        // form is exactly `{parent}.{name}` — the whole prefix, nothing before
        // it — the parent segment takes the same rename the chain builder
        // applies: `exportAssignmentEnum` wants `EnumE.A` over the baked
        // `E.A`. The exact-prefix gate is bar leg 4's protection (§10.16): a
        // dotted name embedded deeper in a composite is not this shape.
        if let Some(parent) = self.binder.symbols().get(symbol).parent {
            let parent = self.binder.merged_symbol(parent);
            let parent_name = self.binder.symbols().get(parent).name;
            let prefix = &printed[..suffix_at - name.len()];
            if prefix.len() == parent_name.len() + 1
                && prefix.starts_with(parent_name)
                && prefix.ends_with('.')
                && let Some(better) = self.best_name(parent, reference)
                && better != parent_name
            {
                let mut out = String::with_capacity(printed.len() + better.len());
                out.push_str(&better);
                out.push_str(&printed[parent_name.len()..]);
                return Some(out);
            }
        }
        // A symbol literally named `default` — a default export reached with
        // no accessible alias. Upstream NEVER prints `default` as a name
        // (`getNameOfSymbolAsWritten` substitutes the binding); with no
        // better name in scope, the honest answer is a gap, not
        // `typeof default` — 40 such lines were minted and reverted by this
        // refusal (`checker-notes-modobj.md` §10.11).
        if name == "default" {
            return None;
        }
        // §509: no qualifier where the bare name RESOLVES to this very
        // symbol at the site - upstream's chain walk starts at the reference
        // and stops at the first accessible spelling, so a member referenced
        // from INSIDE its own namespace prints bare
        // (`interMixingModulesInterfaces2-5` want `B` and `typeof B` inside
        // `A`, `A.B` outside). `resolve_name` is the shadow-exact test: a
        // shadowing `B` at the site resolves to the OTHER symbol and the
        // qualifier proceeds.
        if let Some(resolved) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            reference,
            name,
            SymbolFlags::TYPE | SymbolFlags::VALUE,
        ) {
            let resolved = self.binder.merged_symbol(resolved);
            let own = self.binder.merged_symbol(symbol);
            // Identity, or the same written name in the same CONTAINER - the
            // port carries an interface and its non-exported sibling
            // namespace as two unmerged symbols where upstream merges them,
            // and for NAMING purposes a same-name same-container hit is the
            // symbol (the chain would spell identically).
            // Identity, or a shared DECLARATION - the port can carry two
            // symbol records for one written declaration (the locals-table
            // entry and the exports/parented one), where upstream has one;
            // for NAMING purposes a hit on the same declaration IS the
            // symbol.
            let same_spelling = resolved == own || {
                let a = self.binder.symbols().get(resolved).declarations.first().copied();
                let b = self.binder.symbols().get(own).declarations.first().copied();
                // FIRST declarations must agree, not merely overlap: two
                // same-named siblings whose visibility differs (the
                // `duplicateSymbolsExportMatching` inst pair) share a merged
                // name-table record, and the innermost bare hit is the one
                // bound FIRST - the exported twin stays qualified.
                a.is_some() && a == b
            };
            // ...and the hit must be LEXICAL: some shared declaration's own
            // container block is an ancestor of the reference. Two sibling
            // blocks of one merged `module M` are NOT each other's scope -
            // upstream qualifies a first-block symbol referenced from the
            // second (`duplicateSymbolsExportMatching`, a passing case, wants
            // `typeof M.C`; the full-stop rule measured this gate in), while
            // a member referenced from within its own block prints bare.
            let lexically_contains_reference = |checker: &Self| {
                let declarations = checker.binder.symbols().get(resolved).declarations.to_vec();
                declarations.iter().any(|&declaration| {
                    let Some(container) = checker.nodes.parent(declaration) else {
                        return false;
                    };
                    let mut current = Some(reference);
                    while let Some(node) = current {
                        if node == container {
                            return true;
                        }
                        current = checker.nodes.parent(node);
                    }
                    false
                })
            };
            if same_spelling && lexically_contains_reference(self) {
                return Some(printed);
            }
        }
        // §527 (ADR-0044): the MEANING is upstream's `mask` — ONE meaning,
        // not the union. `symbolToTypeNode` (`nodebuilderimpl.go:649`) sets
        // `isTypeOf := mask == SymbolFlagsValue`, and `needsQualification`
        // (`symbolaccessibility.go:716-719`) qualifies only when the
        // shadowing symbol carries THAT meaning. `TYPE | VALUE` let a
        // Value-only shadow qualify a Type reference.
        let meaning =
            if printed.starts_with("typeof ") { SymbolFlags::VALUE } else { SymbolFlags::TYPE };
        let Some(qualifier) = self.symbol_chain(symbol, reference, meaning, 0) else {
            return Some(printed);
        };
        let mut out = String::with_capacity(printed.len() + qualifier.len());
        out.push_str(&printed[..suffix_at - name.len()]);
        out.push_str(&qualifier);
        out.push_str(&printed[suffix_at - name.len()..]);
        Some(out)
    }

    /// Where `name` sits in `printed`, if `printed` is that name possibly under
    /// `typeof` and possibly with type arguments. Returns the offset just past
    /// the name.
    ///
    /// `Some` is the gate [`Checker::qualified_name_at`] documents. `None` for
    /// anything else — a union, a signature, a type literal — because a
    /// qualifier belongs on a *name*, and a name that is only a fragment of a
    /// larger rendering is reached by machinery this port does not have.
    fn split_around_name(printed: &str, name: &str) -> Option<usize> {
        if name.is_empty() {
            return None;
        }
        // `typeof C` — the static side, which is the majority of the sized
        // population (705 of 2,990 conversions are `typeof` positions, §10.3).
        if let Some(rest) = printed.strip_prefix("typeof ") {
            return (rest == name).then_some(printed.len());
        }
        if printed == name {
            return Some(printed.len());
        }
        // `C<string>` — the qualifier goes on `C`, never inside the arguments.
        printed.strip_prefix(name).filter(|rest| rest.starts_with('<')).map(|_| name.len())
    }

    /// `getSymbolChain` (`internal/checker/nodebuilderimpl.go:1087`), reduced to
    /// the arms this port can answer. Returns the dotted prefix — `"M."`,
    /// `"A.B."` — or `None` when no qualifier is owed or none can be built.
    ///
    /// The two tests, in upstream's order:
    ///
    /// 1. [`Checker::needs_qualification`] — the stop condition at
    ///    `internal/checker/nodebuilderimpl.go:1094`. **This is the entire
    ///    difference between 14 lines at risk and 717**, and it is why the
    ///    ungated design lost 3,202 lines in cycle 20b.
    /// 2. `getContainersOfSymbol`
    ///    (`internal/checker/symbolaccessibility.go:280`), whose first and
    ///    normal answer is `getParentOfSymbol` (`internal/checker/checker.go:14365`)
    ///    — the symbol whose table this one lives in. Recursion then continues
    ///    with `getQualifiedLeftMeaning`, i.e. `SymbolFlagsNamespace`
    ///    (`internal/checker/nodebuilderimpl.go:1111`).
    ///
    /// # What it refuses, and why a refusal beats a wrong name
    ///
    /// - **An external-module container with no route to a name.** Upstream
    ///   prints one through `getSpecifierForModuleSymbol`
    ///   (`internal/checker/nodebuilderimpl.go:1104`). Since the
    ///   container-qualifier slice (`checker-notes-modobj.md` §10.6) this port
    ///   answers two of its shapes — a unique in-scope alias of the container
    ///   ([`Checker::module_name_at`]), and an **ambient** container, whose
    ///   specifier is exact (`nodebuilderimpl.go:1260`) — after
    ///   `trySymbolTable`'s direct arm (`symbolaccessibility.go:535`): an
    ///   in-scope alias naming the symbol *itself* makes the bare name
    ///   accessible and stops the chain, which is what keeps the
    ///   `moduleAugmentation` right-lines right (16 at risk without it, 3
    ///   with, for 0 conversions). A *file* container with no alias still
    ///   declines: only the `modulespecifiers` package could spell it.
    /// - **No container at all** — `Symbol.parent` unset, which is what the
    ///   binder records for a namespace *local* rather than an export. Upstream
    ///   agrees: `getParentOfSymbol` answers nil and the fallback loop
    ///   (`internal/checker/symbolaccessibility.go:288`) only recovers
    ///   containers for external-module children. Measured: **229** lines.
    /// - **`getWithAlternativeContainers`**
    ///   (`internal/checker/symbolaccessibility.go:117`) — the re-export and
    ///   `export =` routes — is **not** ported. §10.3 measures its absence at
    ///   288 lines that get a different container than upstream's; every one of
    ///   them is a line that is wrong today and stays wrong, so the omission
    ///   costs conversions rather than manufacturing losses.
    pub(crate) fn symbol_chain(
        &mut self,
        symbol: SymbolId,
        reference: NodeId,
        meaning: SymbolFlags,
        depth: usize,
    ) -> Option<String> {
        // Upstream has no cap; a cycle in `parent` would be a binder defect and
        // this guard exists so that one cannot hang the corpus.
        if depth >= Self::MAX_SYMBOL_CHAIN {
            return None;
        }
        let name = self.binder.symbols().get(symbol).name;
        if !self.needs_qualification(symbol, name, reference, meaning) {
            return None;
        }
        let Some(container) = self.binder.symbols().get(symbol).parent else {
            // No container — for a symbol that lives in `c.globals` this is not
            // the end of the road upstream, it is `trySymbolTable`'s LAST arm.
            return self.global_this_chain(symbol, name, reference, meaning);
        };
        let parent = self.binder.merged_symbol(container);
        if self.is_module_symbol(parent) || self.is_ambient_module(parent) {
            // `trySymbolTable`'s direct arm: the bare name is accessible
            // through an alias, so no qualifier may fire.
            if self.alias_in_scope_for(symbol, reference) {
                return None;
            }
            // `getAccessibleSymbolChain`'s alias arm for the *container*.
            // §14's ambiguity refusal is GONE as of §533: upstream sorts the
            // candidate chains and returns the first (`symbolaccessibility.go:
            // 582-586`), and this port now computes the same order, so an
            // ambiguous container yields a name rather than declining. Only a
            // container no alias reaches falls through to `import("…")`.
            // Native getQualifiedLeftMeaning: only exact VALUE keeps value
            // meaning; a type or combined meaning qualifies through a namespace.
            let left_meaning = if meaning == SymbolFlags::VALUE {
                SymbolFlags::VALUE
            } else {
                SymbolFlags::NAMESPACE
            };
            if let Some(alias) = self.module_alias_at(parent, reference, left_meaning) {
                return Some(format!("{alias}."));
            }
            // The ambient branch of `getSpecifierForModuleSymbol`
            // (`nodebuilderimpl.go:1260`): the specifier IS the module's name,
            // **with its quotes taken off** —
            // `stringutil.StripQuotes(symbol.Name)`, guarded by
            // `ast.IsAmbientModuleSymbolName`. The symbol is stored as `"fs"`
            // and the printed form is `import("fs")`, so stripping here is
            // upstream's own step rather than a compensation for the storage.
            if self.is_ambient_module(parent) && !self.is_module_symbol(parent) {
                let module_name = tsr_core::strip_quotes(self.binder.symbols().get(parent).name);
                return Some(format!("import(\"{module_name}\")."));
            }
            // §106 (`checker-notes-narrow.md`): the FILE-module half of
            // `getSpecifierForModuleSymbol` — a module inaccessible at the
            // site spells `import("./name").`; this port stores the stripped
            // file path as the module symbol's name. Same-directory slice:
            // a name carrying visible directory structure keeps the decline
            // (a wrong specifier is worse than the bare name).
            if self.is_module_symbol(parent) {
                let module_name = tsr_core::strip_quotes(self.binder.symbols().get(parent).name);
                // The port's module names are absolute virtual paths
                // (`/file`, `/.src/file`); the same-directory slice is the
                // name relative to the reference file's directory when the
                // module sits under it, else relative to `/` as before. The
                // harness compiles in the runner's `/.src` (r5-config §4),
                // where "root-relative" would make every unit look nested.
                // Deeper structure keeps the decline — a wrong specifier is
                // worse than the bare name.
                let reference_directory = self
                    .source_file_of(reference)
                    .and_then(|file| self.module_host.and_then(|host| host.file_path(file)))
                    .map(|path| tsr_path::get_directory_path(&path).to_string());
                let stem = reference_directory
                    .as_deref()
                    .filter(|directory| *directory != "/")
                    .and_then(|directory| module_name.strip_prefix(directory))
                    .and_then(|rest| rest.strip_prefix('/'))
                    .unwrap_or_else(|| module_name.strip_prefix('/').unwrap_or(module_name));
                // NEVER for the reference's own file: a same-file name that
                // failed to resolve is a synthetic/evaluated position, and
                // the bare name is upstream's print there (chain1's 217 R→W
                // measured without this gate).
                let same_file = {
                    let mut current = Some(reference);
                    let mut reference_file = None;
                    while let Some(id) = current {
                        if self.nodes.kind(id) == tsr_ast::SyntaxKind::SourceFile {
                            reference_file = Some(id);
                            break;
                        }
                        current = self.nodes.parent(id);
                    }
                    reference_file.is_some_and(|file| {
                        self.binder
                            .symbols()
                            .get(parent)
                            .declarations
                            .first()
                            .is_some_and(|&declaration| declaration == file)
                    })
                };
                // ...and never when the reference's file IMPORTS the module
                // under any binding: the name is reachable there and the bare
                // (or alias) print is upstream's — our resolver just cannot
                // walk every re-export form yet (exportsAndImports3's 16 R→W
                // measured without this gate). Only a module the file never
                // mentions gets the specifier spelling.
                let imported_here = {
                    let mut current = Some(reference);
                    let mut reference_file = None;
                    while let Some(id) = current {
                        if self.nodes.kind(id) == tsr_ast::SyntaxKind::SourceFile {
                            reference_file = Some(id);
                            break;
                        }
                        current = self.nodes.parent(id);
                    }
                    reference_file
                        .is_some_and(|file| self.file_mentions_module_specifier(file, stem))
                };
                // `forEachSymbolTableInScope` (`symbolaccessibility.go`) reads
                // the reference file's own `exports`; `needsQualification`
                // stops at the first table holding the name. A same-file
                // symbol that table does not hold (a conflicting declaration
                // `declareSymbol` split into its own symbol), or whose name
                // resolves to another symbol first, reaches the specifier for
                // its own file. Only an exported symbol whose name did not
                // resolve at all keeps the decline above.
                let held_by_exports =
                    self.binder.symbols().get(parent).exports.get(name).is_some_and(|&held| {
                        self.binder.merged_symbol(held) == self.binder.merged_symbol(symbol)
                    });
                let unresolved_export = same_file
                    && held_by_exports
                    && self
                        .binder
                        .resolve_name(self.nodes, self.node_map, reference, name, meaning)
                        .is_none();
                if !unresolved_export && !imported_here && !stem.contains('/') && !stem.is_empty() {
                    // `getSpecifierForModuleSymbol`'s spelling
                    // (`crate::module_specifiers`, r5-modules2 §3): the
                    // module symbol's name keeps declaration suffixes
                    // (`./foo.d`) that `processEnding` removes.
                    let specifier = self.module_specifier_for_symbol(parent, reference)?;
                    return Some(format!("import({specifier})."));
                }
                // A module under `node_modules` takes `getSpecifierForModuleSymbol`'s
                // whole answer — an existing import, the package name
                // (`tryGetModuleNameAsNodeModule`) or the relative fallback —
                // where the flat arm above declines (r5-modules §5). The
                // relative-module gates above are left as they are.
                if !unresolved_export
                    && !same_file
                    && !imported_here
                    && stem.contains("node_modules/")
                    && let Some(specifier) = self.module_specifier_for_symbol(parent, reference)
                {
                    self.track_unsafe_import(&specifier, symbol, parent);
                    return Some(format!("import({specifier})."));
                }
            }
            return None;
        }
        if !self.binder.symbols().get(parent).flags.intersects(SymbolFlags::MODULE) {
            return None;
        }
        // The RENAME applies to chain *segments* too (`§10.9`'s 67-line
        // residue): a chain rooted at `__React` prints under the innermost
        // accessible name for that segment — `React.Component`, not
        // `__React.Component`. Same walk, same `useOnlyExternalAliasing`
        // filter, falling back to the segment's own name.
        let parent_name = self
            .best_name(parent, reference)
            .unwrap_or_else(|| self.binder.symbols().get(parent).name.to_string());
        // A multi-segment accessible alias route is already rooted in scope.
        // Recursing on the declared parent would qualify it a second time.
        if parent_name.contains('.') {
            return Some(format!("{parent_name}."));
        }
        Some(match self.symbol_chain(parent, reference, SymbolFlags::NAMESPACE, depth + 1) {
            Some(prefix) => format!("{prefix}{parent_name}."),
            // The container itself resolves bare here: the chain stops, which
            // is the recursion's base case rather than a refusal.
            None => format!("{parent_name}."),
        })
    }

    /// §106's imported-here gate: whether `file`'s import/export-from
    /// statements mention a module specifier whose stem matches `stem`.
    fn file_mentions_module_specifier(&self, file: NodeId, stem: &str) -> bool {
        let Some(Node::SourceFile(source)) = self.node_map.get(file) else { return false };
        source.statements.iter().any(|statement| {
            let specifier = match statement {
                tsr_ast::Statement::ImportDeclaration(node) => node.module_specifier,
                tsr_ast::Statement::ExportDeclaration(node) => node.module_specifier,
                tsr_ast::Statement::ImportEqualsDeclaration(node) => match node.module_reference {
                    Some(tsr_ast::ModuleReference::ExternalModuleReference(external)) => {
                        external.expression
                    }
                    _ => None,
                },
                _ => None,
            };
            let Some(tsr_ast::Expression::StringLiteral(text)) = specifier else {
                return false;
            };
            let text = text.text.trim_start_matches("./").trim_start_matches('/');
            text == stem
        })
    }

    /// `ast.IsInJSFile`: whether the node's source file was a `.js`-family
    /// file. Reads [`tsr_ast::NodeFlags::JAVASCRIPT_FILE`] off the **root**,
    /// because that is the one node the program stamps
    /// (`tsr-compiler/src/lib.rs`) — upstream stamps every node from the
    /// parser, which this parser cannot (it never sees the file name,
    /// ADR-0016). Climbing is O(depth); depth is bounded by the source.
    ///
    /// `false` for a checker built without a program: the unit-test
    /// constructors have no file names, and every fixture is TypeScript.
    /// §136 (printseam §7): whether a node lives in a bundled DEFAULT
    /// LIBRARY file — the loader stamps the root, mirror of
    /// [`Checker::in_js_file`].
    pub(crate) fn in_default_library(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            current = parent;
        }
        self.nodes.flags(current).contains(NodeFlags::DEFAULT_LIBRARY)
    }

    pub(crate) fn in_js_file(&self, node: NodeId) -> bool {
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            current = parent;
        }
        self.nodes.flags(current).contains(NodeFlags::JAVASCRIPT_FILE)
    }

    /// `needsQualification` (`internal/checker/symbolaccessibility.go:688`):
    /// does the symbol's own name, resolved from the reference site, come back
    /// as this same symbol?
    ///
    /// Upstream walks every symbol table in scope and answers *no qualification
    /// needed* the moment one holds the symbol itself
    /// (`internal/checker/symbolaccessibility.go:702`). `resolve_name` walks the
    /// same tables in the same order, so it agrees on the first hit.
    ///
    /// # The `getMergedSymbol` call is load-bearing and was got wrong once
    ///
    /// Upstream reads `symbolFromSymbolTable := c.getMergedSymbol(res)`
    /// (`internal/checker/symbolaccessibility.go:696`) **before** the identity
    /// test six lines later. `examples/qualname.rs` omitted that merge and so
    /// scored 13 merged declarations — a class and a namespace sharing a name —
    /// as needing a qualifier, publishing an at-risk column of 36 where the
    /// correct figure is 23. `checker-notes-qualname.md` §10.4 carries the
    /// correction. Dropping the merge here re-introduces those 13 lines as real
    /// losses, and `symbol_chain_does_not_split_a_merged_declaration` is the
    /// test that says so.
    pub(crate) fn needs_qualification(
        &self,
        symbol: SymbolId,
        name: &str,
        reference: NodeId,
        meaning: SymbolFlags,
    ) -> bool {
        match self.binder.resolve_name(self.nodes, self.node_map, reference, name, meaning) {
            Some(found) => self.binder.merged_symbol(found) != self.binder.merged_symbol(symbol),
            None => true,
        }
    }

    /// `trySymbolTable`'s globals arm
    /// (`internal/checker/symbolaccessibility.go:588-591`): the two-element
    /// chain `[globalThis, target]`, rendered as the prefix `"globalThis."`.
    ///
    /// `globalThisSymbol` is an ordinary `Module` symbol whose `Exports` **is**
    /// the globals table (`checker.go:962-964`) and there is no special case in
    /// the printer; this arm is the only injection point in the compiler. It
    /// sits at the very bottom of `trySymbolTable`, reached when the direct arm
    /// failed, the alias loop produced no candidate chain, and the table being
    /// tried is `c.globals`.
    ///
    /// In one sentence (`checker-notes-sitename.md` §4.5): **`globalThis.` is
    /// prefixed exactly when the target is reachable through the globals table
    /// but no in-scope table can name it**, because a different symbol of the
    /// same meaning shadows the name and no alias offers a route.
    ///
    /// # The three gates, and why the middle one is the whole build
    ///
    /// 1. **Reachable through globals, as this symbol** — `globals()[name]`
    ///    merges to `symbol`. Without it the prefix would be a claim about a
    ///    table the name is not in.
    /// 2. **Shadowed at the site** — [`Checker::is_shadowed_at`], the honest
    ///    transcription, and **not** [`Checker::needs_qualification`]. This arm
    ///    was built once before against the conflated predicate and measured
    ///    **38 W→R against 25 R→W with two PASSING cases damaged**
    ///    (`collisionCodeGenEnumWithEnumMemberConflict`,
    ///    `strictModeReservedWord2`), and was reverted (§7.1). The witness is
    ///    `enum Color { Color, Thing = Color }`: the enum MEMBER shadows in
    ///    `Value`, the reference is a `Type`, upstream does not qualify — but
    ///    `resolve_name` answers `None` at that position and the conflated
    ///    predicate therefore said *qualify*, so the arm spelled
    ///    `globalThis.Color`. §8 is the diagnosis and §12 is the split that
    ///    makes gate 2 expressible at all.
    /// 3. **No alias route** — an in-scope alias naming the symbol itself is a
    ///    candidate chain upstream's alias loop would have yielded first, and
    ///    it would print the alias, not `globalThis.`. This port cannot spell
    ///    that chain here, so it declines rather than printing a form upstream
    ///    did not.
    ///
    /// # The falsifier, registered before the measurement (§6)
    ///
    /// If the arm fires on more than a handful of sites, the globals gate is
    /// not doing its job and this is SS197 under a new name — the probe that
    /// asked the shadowing question at *every* site and measured **−66 cases**,
    /// because a local, module or member scope shadow fires there while
    /// upstream's rule cannot reach the globals arm at all.
    fn global_this_chain(
        &mut self,
        symbol: SymbolId,
        name: &str,
        reference: NodeId,
        meaning: SymbolFlags,
    ) -> Option<String> {
        let &global = self.binder.globals().get(name)?;
        if self.binder.merged_symbol(global) != self.binder.merged_symbol(symbol) {
            return None;
        }
        if !self.is_shadowed_at(symbol, name, reference, meaning) {
            return None;
        }
        if self.alias_in_scope_for(symbol, reference) {
            return None;
        }
        Some("globalThis.".to_string())
    }

    /// `needsQualification` (`internal/checker/symbolaccessibility.go:688-726`),
    /// transcribed **honestly** — the table walk, not `resolve_name`.
    ///
    /// # Why this exists next to [`Checker::needs_qualification`] rather than
    /// replacing it
    ///
    /// `needs_qualification` is not upstream's predicate. It is upstream's
    /// predicate **OR'd with a second condition**, and the OR is load-bearing:
    /// its `None => true` arm answers *"the port could not resolve this name at
    /// this site"*, which is the port's proxy for upstream's **first** disjunct
    /// at `nodebuilderimpl.go:1093-1094` —
    ///
    /// > `chain == nil` **OR** `needsQualification(chain[0], …)`
    ///
    /// — *no accessible chain exists*, which upstream computes with
    /// `getAccessibleSymbolChain` (`symbolaccessibility.go:373`), an entirely
    /// different function. Transcribing `None => false` in place measures
    /// **6,850 R→W** (`compiler/temporal` 3,318,
    /// `resolvingClassDeclarationWhenInBaseTypeResolution` 1,022, the whole
    /// `privacy*CannotName*` family): those are cross-file, namespace-member and
    /// lib names the port's `resolve_name` cannot reach at the site, and every
    /// one of them genuinely needs its qualifier. `checker-notes-sitename.md`
    /// §8 carries the measurement.
    ///
    /// So the split is **additive**. This function answers the *shadowing*
    /// question alone — the one `symbolaccessibility.go:588-591`'s globals arm
    /// and `:535-593`'s direct arm actually sit behind. The conflated predicate
    /// keeps every call site it has until each is re-pointed with its own
    /// scorepair. `enum Color { Color }` is the witness for why that matters:
    /// `Color` is accessible as itself (upstream prints it bare), but
    /// `resolve_name` answers `None` at that position and the conflated
    /// predicate therefore says *qualify*.
    ///
    /// # The walk
    ///
    /// `someSymbolTableInScope` (`symbolaccessibility.go:746-803`) from the
    /// reference outward: a location's `locals`, then — by kind — a module's or
    /// file's `exports`, or a class/interface's **type** members (type
    /// parameters are bound into `members`, `symbolaccessibility.go:766`), and
    /// `c.globals` at the end. Per table, upstream's callback exactly: the name
    /// absent is a *continue*; the entry being this symbol stops the walk with
    /// **no** qualification; otherwise the alias is resolved (unless it is an
    /// export specifier), `getSymbolFlags` is taken, and a flags/meaning
    /// intersection **qualifies**. A name in no table leaves `qualify` false.
    ///
    /// # What is not walked, and why each is a miss rather than a wrong answer
    ///
    /// - **The script-source-file `locals` skip.** Upstream skips a global
    ///   source file's locals (`ast.IsGlobalSourceFile`) because those names are
    ///   merged into `c.globals`. This port's `globals` is populated only when
    ///   several files are bound into one result (`BindResult::globals`), so a
    ///   single-file program keeps its top level in `locals` and skipping it
    ///   would consult no table at all. Visiting both is safe because every hit
    ///   is put through `merged_symbol`, so the two tables cannot disagree about
    ///   *which* symbol a name denotes.
    /// - **`getClassExpressionNameTable`** (`symbolaccessibility.go:809`) — a
    ///   class expression's own name is in no table here either. Its absence can
    ///   only make this answer `false` where upstream answers `true`.
    /// - **The external-module gate on the `exports` arm.** Upstream visits a
    ///   file's exports only for an external or `CommonJS` module. Nothing is ever
    ///   routed to a *script* file's exports in this binder (`is_export_context`
    ///   requires `self.is_module`), so the table is empty and the arm cannot
    ///   fire — the same reasoning `BindResult::resolve_name` records for its
    ///   own unconditional exports arm.
    ///
    /// # Not yet called from the qualifier road
    ///
    /// `checker-notes-sitename.md` §8.4 requires each re-pointing to carry its
    /// own measurement, and a step that is not byte-identical on the untouched
    /// sites has changed the disjunction. It is `pub` rather than `pub(crate)`
    /// so that `tests/symbol_chain.rs` can pin the divergence from
    /// [`Checker::needs_qualification`] directly — which is the whole point of
    /// the split, and a thing no `.types` fixture can show while the conflated
    /// predicate is still the one wired up.
    pub fn is_shadowed_at(
        &mut self,
        symbol: SymbolId,
        name: &str,
        reference: NodeId,
        meaning: SymbolFlags,
    ) -> bool {
        let target = self.binder.merged_symbol(symbol);
        let mut current = Some(reference);
        while let Some(location) = current {
            if let Some(&found) = self.binder.locals(location).and_then(|table| table.get(name))
                && let Some(answer) = self.qualifies_for(found, target, meaning)
            {
                return answer;
            }
            match self.nodes.kind(location) {
                SyntaxKind::SourceFile | SyntaxKind::ModuleDeclaration => {
                    if let Some(owner) = self.binder.symbol_of(location)
                        && let Some(&found) = self.binder.symbols().get(owner).exports.get(name)
                        && let Some(answer) = self.qualifies_for(found, target, meaning)
                    {
                        return answer;
                    }
                }
                SyntaxKind::ClassDeclaration
                | SyntaxKind::ClassExpression
                | SyntaxKind::InterfaceDeclaration => {
                    // Upstream builds a filtered copy holding only the members
                    // carrying `SymbolFlagsType`; a single lookup with the same
                    // filter is the same question without the allocation.
                    if let Some(owner) = self.binder.symbol_of(location)
                        && let Some(&found) = self.binder.symbols().get(owner).members.get(name)
                        && self
                            .binder
                            .symbols()
                            .get(self.binder.merged_symbol(found))
                            .flags
                            .intersects(SymbolFlags::TYPE)
                        && let Some(answer) = self.qualifies_for(found, target, meaning)
                    {
                        return answer;
                    }
                }
                _ => {}
            }
            current = self.nodes.parent(location);
        }
        if let Some(&found) = self.binder.globals().get(name)
            && let Some(answer) = self.qualifies_for(found, target, meaning)
        {
            return answer;
        }
        false
    }

    /// One table hit of [`Checker::is_shadowed_at`]'s callback
    /// (`symbolaccessibility.go:694-724`): `Some(false)` — the entry *is* the
    /// symbol, stop with no qualification; `Some(true)` — a different symbol
    /// with an overlapping meaning, stop and qualify; `None` — keep walking.
    fn qualifies_for(
        &mut self,
        found: SymbolId,
        target: SymbolId,
        meaning: SymbolFlags,
    ) -> Option<bool> {
        let from_table = self.binder.merged_symbol(found);
        if from_table == target {
            return Some(false);
        }
        // `shouldResolveAlias` (`:713`): an alias qualifies on its *target's*
        // flags, except where the alias is an export specifier — that one keeps
        // its own.
        let flags = self.binder.symbols().get(from_table).flags;
        let is_export_specifier = self
            .binder
            .symbols()
            .get(from_table)
            .declarations
            .iter()
            .any(|&declaration| self.nodes.kind(declaration) == SyntaxKind::ExportSpecifier);
        let flags = if flags.intersects(SymbolFlags::ALIAS) && !is_export_specifier {
            self.get_symbol_flags(from_table)
        } else {
            flags
        };
        flags.intersects(meaning).then_some(true)
    }

    /// Whether a symbol is an ambient external module — `declare module "x"`.
    ///
    /// The sibling refusal to [`Checker::is_module_symbol`]: upstream reaches
    /// both through `getSpecifierForModuleSymbol`
    /// (`internal/checker/nodebuilderimpl.go:1104`) rather than through a dotted
    /// name, so neither may become a qualifier.
    ///
    /// `ast.IsAmbientModuleSymbolName(symbol.Name)`: the binder names a
    /// string-named module by its quoted specifier (`quoted_module_name`), and
    /// nothing else has a quoted name. Asking the *declarations* instead stopped
    /// being equivalent once module augmentations merge
    /// (`docs/parity/notes/names-modules.md` §4): an augmentation of an
    /// `export =` function-and-namespace (`moment`) adds a `declare module
    /// "moment"` declaration to that function, which is not a module.
    pub(crate) fn is_ambient_module(&self, symbol: SymbolId) -> bool {
        let name = self.binder.symbols().get(symbol).name;
        name.len() >= 2 && name.starts_with('"') && name.ends_with('"')
    }

    /// Whether a symbol is a **file's** module symbol.
    ///
    /// The positive test is that one of its declarations is a `SourceFile`,
    /// which is how the binder creates it (`bindSourceFile`) and is not true of
    /// any other symbol. A flag test would not do: `SymbolFlags::VALUE_MODULE`
    /// is carried by every `namespace N {}` as well, and those have real names
    /// that print correctly through the baked text.
    fn is_module_symbol(&self, symbol: SymbolId) -> bool {
        self.binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .any(|&declaration| self.nodes.kind(declaration) == SyntaxKind::SourceFile)
    }

    /// The single alias in scope at `reference` that resolves to `module`.
    ///
    /// `None` when there is none, or when there is more than one — see
    /// [`Checker::type_to_string_at`] for why more-than-one is not a tie-break.
    /// §501 — the naming half of `bd tsr-e2u`, cut to exactly the measured
    /// population: a namespace object reached through
    /// `declare module "m" { export = __X }` prints the IMPORTING alias
    /// (`typeof React`), and nothing else moves. The first draft routed every
    /// namespace print through the full accessibility walk and measured
    /// **274 R→W** (temporal 46 — qualified baked texts flattened to leaf
    /// names), so this walk is deliberately narrower than `best_name`:
    ///
    /// - innermost table first, and a DIRECT HIT on the symbol's own name
    ///   answers `None` — the baked text (which carries qualification this
    ///   single-name walk cannot rebuild) wins everywhere it wins today;
    /// - only an alias whose immediate target is ITSELF an alias — the
    ///   two-hop `import → export=` signature — renames; a one-hop alias to
    ///   the namespace was already handled (or refused) by the existing
    ///   roads and keeps its behavior;
    /// - two distinct such names in one table answer `None`, the §14
    ///   two-alias decline.
    fn export_equals_alias_name_at(
        &mut self,
        namespace: SymbolId,
        reference: NodeId,
    ) -> Option<&'a str> {
        let own = self.binder.symbols().get(namespace).name;
        let target = self.binder.merged_symbol(namespace);
        let mut tables: Vec<Vec<(&'a str, SymbolId)>> = Vec::new();
        let mut current = Some(reference);
        while let Some(node) = current {
            if let Some(locals) = self.binder.locals(node) {
                tables.push(locals.iter().map(|(&name, &id)| (name, id)).collect());
            }
            current = self.nodes.parent(node);
        }
        tables.push(self.binder.globals().iter().map(|(&name, &id)| (name, id)).collect());
        for table in tables {
            if let Some(&(_, hit)) = table.iter().find(|&&(name, _)| name == own)
                && self.binder.merged_symbol(hit) == target
            {
                return None;
            }
            let mut found: Option<&'a str> = None;
            for (name, candidate) in table {
                if !self.binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS) {
                    continue;
                }
                if name == "default" || name == "export=" {
                    continue;
                }
                // A DEFAULT import is excluded: under esModuleInterop its
                // chain runs through the synthetic default, and upstream
                // prints the module-qualified form there —
                // `exportAssignmentOfExportNamespaceWithDefault` (a PASSING
                // case) records `typeof import("b").a` where this walk's
                // local name would print `typeof a`; the full-stop rule
                // measured it in.
                if self.binder.symbols().get(candidate).declarations.iter().any(|&declaration| {
                    matches!(self.node_map.get(declaration), Some(Node::ImportClause(_)))
                }) {
                    continue;
                }
                if self.alias_targets_module_clone(candidate) {
                    continue;
                }
                let Some(link) = self.resolve_alias(candidate) else { continue };
                // The two-hop signature: the immediate target is the
                // `export=` alias, and ITS target is the namespace.
                if !self.binder.symbols().get(link).flags.intersects(SymbolFlags::ALIAS) {
                    continue;
                }
                if self.binder.merged_symbol(self.resolve_alias_fully(link)) != target {
                    continue;
                }
                match found {
                    Some(existing) if existing == name => {}
                    Some(_) => return None,
                    None => found = Some(name),
                }
            }
            if found.is_some() {
                return found;
            }
        }
        None
    }

    fn module_name_at(
        &mut self,
        module: SymbolId,
        reference: NodeId,
        meaning: SymbolFlags,
    ) -> Option<&'a str> {
        self.module_alias_at(module, reference, meaning)
    }

    /// The in-scope alias naming `module` at `reference`, **choosing** when
    /// more than one reaches it.
    ///
    /// # §533: this used to DECLINE on ambiguity, and upstream never does
    ///
    /// `trySymbolTable` gathers every alias in a table that reaches the symbol
    /// into `candidateChains` and then
    /// (`internal/checker/symbolaccessibility.go:582-586`):
    ///
    /// > ```go
    /// > // pick first, shortest
    /// > slices.SortStableFunc(candidateChains, c.compareSymbolChains)
    /// > return candidateChains[0]
    /// > ```
    ///
    /// `compareSymbolChainsWorker` (`:595-610`) is *shorter chain wins*, then
    /// `compareSymbols` element by element; `compareSymbolsWorker`
    /// (`utilities.go:366-391`) is *first declaration's position* — by file
    /// index, then by offset within the file (`compareNodes`, `:393-412`) —
    /// then the name, then the symbol id. **It is a total order, and upstream
    /// always gets an answer.**
    ///
    /// This port answered `Err(true)` — decline — whenever two distinct names
    /// reached the module, on the reasoning (`checker-notes-nameres.md` §14)
    /// that upstream picked *some* alias and guessing wrong prints a name
    /// upstream did not. That was the right call while the tie-break was a
    /// guess. It is not a guess: `compiler/importDecl` writes
    /// `import m4 = require("./importDecl_require")` at line 33 and
    /// `import multiImport_m4 = require("./importDecl_require")` at line 79,
    /// and the baseline records **`m4.d`** — the earlier declaration, which is
    /// exactly `compareNodes`' answer. The decline fired **238 times** across
    /// the corpus (16 distinct module/alias-pair shapes) and every one of them
    /// printed a bare name where upstream printed a qualified one.
    ///
    /// # Chain length is not a discriminator here, and that is why this is
    /// `compareSymbols` rather than `compareSymbolChains`
    ///
    /// Every candidate this function considers is a **one-element** chain — an
    /// alias in scope that names the module directly. `compareSymbolChains`'
    /// first key (`len(a) - len(b)`) is therefore always zero and the order
    /// reduces to `compareSymbols` on the alias symbols. Porting the length key
    /// would be porting a comparison over chains this function does not build.
    ///
    /// # The scope walk still dominates the sort
    ///
    /// Upstream sorts *within one table* — `trySymbolTable` runs per table,
    /// innermost first, and returns as soon as a table yields candidates. So
    /// the tie-break only ever separates aliases declared in the SAME scope; an
    /// inner scope's alias beats an outer one's regardless of position. This
    /// collects per table and stops at the first table that yields anything,
    /// which is the same shape. Flattening the tables and sorting globally
    /// would let a file-scope alias at offset 10 beat a block-scope alias at
    /// offset 500 that upstream would have returned first.
    fn module_alias_at(
        &mut self,
        module: SymbolId,
        reference: NodeId,
        meaning: SymbolFlags,
    ) -> Option<&'a str> {
        let mut tables: Vec<Vec<SymbolId>> = Vec::new();
        let mut current = Some(reference);
        while let Some(node) = current {
            if let Some(locals) = self.binder.locals(node) {
                tables.push(locals.values().copied().collect());
            }
            // Pinned 5b1047d someSymbolTableInScope (:746-775) visits raw
            // exports after locals. The original Program owns these symbols;
            // only sole external import-equals declarations extend this view.
            // Existing alias resolution, meaning/shadow checks and per-table
            // native ordering below own selection. No completion/publication
            // or new cache; re-export/default/internal aliases stay excluded.
            if matches!(
                self.nodes.kind(node),
                SyntaxKind::SourceFile | SyntaxKind::ModuleDeclaration
            ) && let Some(owner) = self.binder.symbol_of(node)
            {
                let owner = self.binder.merged_symbol(owner);
                tables.push(
                    self.binder
                        .symbols()
                        .get(owner)
                        .exports
                        .values()
                        .copied()
                        .filter(|&candidate| {
                            let [declaration] =
                                self.binder.symbols().get(candidate).declarations.as_slice()
                            else {
                                return false;
                            };
                            matches!(self.node_map.get(*declaration),
                                Some(Node::ImportEqualsDeclaration(alias))
                                    if matches!(alias.module_reference,
                                        Some(tsr_ast::ModuleReference::ExternalModuleReference(_))))
                        })
                        .collect(),
                );
            }
            current = self.nodes.parent(node);
        }
        // Only `ALIAS` candidates pass the first test below; the global
        // table's are listed once (`r5-checkperf.md` §7).
        tables.push(self.global_alias_entries().iter().map(|&(_, id)| id).collect());

        // §501: the module handed in may itself be an alias — following
        // `export =` returns the `export=` symbol under upstream's
        // `dontResolveAlias` contract — so both sides compare FULLY resolved:
        // `import * as React from "react"` resolves to the `export=` symbol,
        // whose target is the `__React` namespace, and either spelling of the
        // module object must find the alias.
        let module_target = self.resolve_alias_fully(module);
        for table in tables {
            let mut candidates: Vec<SymbolId> = Vec::new();
            for candidate in table {
                if !self
                    .binder
                    .symbols()
                    .get(candidate)
                    .flags
                    .intersects(tsr_binder::SymbolFlags::ALIAS)
                {
                    continue;
                }
                // Ported from typescript-go's `trySymbolTable`
                // (`internal/checker/symbolaccessibility.go`), pinned 5b1047d:
                // this is a local-name lookup, not a qualified exports lookup.
                let entry = self.binder.symbols().get(candidate);
                if entry.name == "default"
                    || entry.name == "export="
                    || entry.declarations.iter().any(|&declaration| {
                        self.nodes.kind(declaration) == SyntaxKind::ExportSpecifier
                            || matches!(self.node_map.get(declaration), Some(Node::NamespaceExport(_)))
                                && self.nodes.parent(declaration).and_then(|parent| self.node_map.get(parent))
                                    .is_some_and(|parent| matches!(parent, Node::ExportDeclaration(export) if export.module_specifier.is_some()))
                    })
                    || (entry.declarations.first().is_some_and(|&declaration| {
                        self.nodes.kind(declaration) == SyntaxKind::NamespaceExportDeclaration
                    }) && self.source_file_of(reference).and_then(|file| self.node_map.get(file))
                        .is_some_and(|file| matches!(file, Node::SourceFile(source)
                            if tsr_binder::is_external_module_in(source, self.nodes))))
                {
                    continue;
                }
                let resolved = self.resolve_alias(candidate);
                if resolved != Some(module)
                    && resolved.map(|r| self.resolve_alias_fully(r)) != Some(module_target)
                {
                    continue;
                }
                if self.alias_targets_module_clone(candidate) {
                    continue;
                }
                let name = self.binder.symbols().get(candidate).name;
                if self.is_shadowed_at(candidate, name, reference, meaning) {
                    continue;
                }
                candidates.push(candidate);
            }
            if let Some(&best) = candidates.iter().min_by(|&&a, &&b| self.compare_symbols(a, b)) {
                return Some(self.binder.symbols().get(best).name);
            }
        }
        None
    }

    /// `compareSymbolsWorker` (`internal/checker/utilities.go:366-391`) and the
    /// `compareNodes` (`:393-412`) it delegates to, reduced to the keys this
    /// port has: **first declaration's position**, then the name, then the
    /// symbol id as a last resort.
    ///
    /// `compareNodes` orders by the declaration's file index in the program and
    /// then by offset within the file. This port has no `fileIndexMap`; it uses
    /// the enclosing `SourceFile`'s [`NodeId`], which the parser allocates in
    /// program order, so the two agree on ordering without agreeing on the
    /// numbers. A declaration with no enclosing file sorts last rather than
    /// panicking.
    ///
    /// Upstream's nil arms (`:369-375`) have no counterpart — a [`SymbolId`] is
    /// always a symbol here.
    pub(crate) fn compare_symbols(&self, a: SymbolId, b: SymbolId) -> std::cmp::Ordering {
        if a == b {
            return std::cmp::Ordering::Equal;
        }
        self.compare_symbols_key(a).cmp(&self.compare_symbols_key(b))
    }

    /// [`Checker::compare_symbols`] as a sort key: equal keys are the same
    /// symbol, and key order is that comparison's order. A sort over many
    /// symbols computes it once per element (`sort_by_cached_key`) instead of
    /// re-walking two declarations' parent chains per comparison.
    pub(crate) fn compare_symbols_key(
        &self,
        symbol: SymbolId,
    ) -> (bool, Option<(NodeId, u32)>, &'a str, SymbolId) {
        let entry = self.binder.symbols().get(symbol);
        let position = entry.declarations.first().map(|&declaration| {
            let mut file = declaration;
            let mut current = Some(declaration);
            while let Some(node) = current {
                if self.nodes.kind(node) == SyntaxKind::SourceFile {
                    file = node;
                    break;
                }
                current = self.nodes.parent(node);
            }
            (file, self.nodes.span(declaration).start)
        });
        // `len(s1.Declarations) != 0` before the comparison (`:376-383`): a
        // symbol WITH declarations sorts before one without; then the first
        // declaration's position, the name, and the id.
        (position.is_none(), position, entry.name, symbol)
    }

    /// Completed TYPE chain for a declaration-namespace member, ported from
    /// `getAccessibleSymbolChain` / `trySymbolTable` / `getCandidateListForSymbol`
    /// (`symbolaccessibility.go:535`, pinned 5b1047d). Only `reference_text_at`
    /// consumes it; ordinary naming and VALUE alias admission are unchanged.
    ///
    /// Program `SymbolIds`, original declaration/export tables and the reference
    /// `NodeId` own the route. Its root is the original in-scope alias, even when
    /// spelled like the declaration. TYPE checks the leaf; NAMESPACE checks
    /// qualifiers. Query-local visited owners prevent cyclic export traversal;
    /// no chain is published into ordinary caches/images. Unsupported/active
    /// owners decline to the existing naming path, not a guessed empty table.
    /// Scope tables are copied lazily, only until the first completed route;
    /// native's accessibility cache is not duplicated. Traversal/copy cost is
    /// unmeasured (tsr-6.47.3.1.1); this is no optimization claim.
    fn accessible_type_name_chain_at(
        &mut self,
        target: SymbolId,
        reference: NodeId,
    ) -> Option<Vec<SymbolId>> {
        fn can_qualify(
            checker: &mut Checker<'_, '_>,
            symbol: SymbolId,
            reference: NodeId,
            meaning: SymbolFlags,
        ) -> bool {
            let name = checker.binder.symbols().get(symbol).name;
            let mut hits = Vec::new();
            let mut current = Some(reference);
            while let Some(node) = current {
                // Native skips script-file locals: the binder merged them into globals.
                let global_file = checker.nodes.kind(node) == SyntaxKind::SourceFile
                    && checker.binder.symbol_of(node).is_none();
                if !global_file
                    && let Some(&hit) =
                        checker.binder.locals(node).and_then(|table| table.get(name))
                {
                    hits.push(hit);
                }
                match checker.nodes.kind(node) {
                    SyntaxKind::SourceFile | SyntaxKind::ModuleDeclaration => {
                        if let Some(owner) = checker.binder.symbol_of(node)
                            && let Some(&hit) =
                                checker.binder.symbols().get(owner).exports.get(name)
                        {
                            hits.push(hit);
                        }
                    }
                    SyntaxKind::ClassDeclaration
                    | SyntaxKind::ClassExpression
                    | SyntaxKind::InterfaceDeclaration => {
                        if let Some(owner) = checker.binder.symbol_of(node)
                            && let Some(&hit) =
                                checker.binder.symbols().get(owner).members.get(name)
                            && checker.binder.symbols().get(hit).flags.intersects(SymbolFlags::TYPE)
                        {
                            hits.push(hit);
                        }
                        if let Some(Node::ClassExpression(class)) = checker.node_map.get(node)
                            && class.name.is_some_and(|identifier| identifier.text == name)
                            && let Some(owner) = checker.binder.symbol_of(node)
                        {
                            hits.push(owner);
                        }
                    }
                    _ => {}
                }
                current = checker.nodes.parent(node);
            }
            if let Some(&hit) = checker.binder.globals().get(name) {
                hits.push(hit);
            }
            for hit in hits {
                let hit = checker.binder.merged_symbol(hit);
                if hit == checker.binder.merged_symbol(symbol) {
                    return true;
                }
                let entry = checker.binder.symbols().get(hit);
                let flags = if entry.flags.intersects(SymbolFlags::ALIAS)
                    && !entry.declarations.iter().any(|&declaration| {
                        checker.nodes.kind(declaration) == SyntaxKind::ExportSpecifier
                    }) {
                    let Some(resolved) = checker.semantic_type_naming_alias_target(
                        hit,
                        SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
                        &mut Vec::new(),
                    ) else {
                        return false;
                    };
                    checker.binder.symbols().get(resolved).flags
                } else {
                    entry.flags
                };
                if flags.intersects(meaning) {
                    return false;
                }
            }
            true
        }

        fn try_table(
            checker: &mut Checker<'_, '_>,
            target: SymbolId,
            reference: NodeId,
            table: &[(&str, SymbolId)],
            local: bool,
            ignore_qualification: bool,
            visited: &mut Vec<SymbolId>,
        ) -> Option<Vec<SymbolId>> {
            let own = checker.binder.symbols().get(target).name;
            if let Some(&(_, hit)) = table.iter().find(|&&(name, _)| name == own)
                && (checker.binder.merged_symbol(hit) == target
                    || checker
                        .binder
                        .symbols()
                        .get(hit)
                        .export_symbol
                        .is_some_and(|export| checker.binder.merged_symbol(export) == target))
                && (ignore_qualification
                    || can_qualify(checker, target, reference, SymbolFlags::TYPE))
            {
                return Some(vec![target]);
            }
            let mut chains = Vec::new();
            for &(name, alias) in table {
                let entry = checker.binder.symbols().get(alias);
                if !entry.flags.intersects(SymbolFlags::ALIAS)
                    || name == "default"
                    || name == "export="
                {
                    continue;
                }
                if entry.declarations.iter().any(|&declaration| {
                    (local && matches!(checker.node_map.get(declaration), Some(Node::NamespaceExport(_))))
                        || (!ignore_qualification && checker.nodes.kind(declaration) == SyntaxKind::ExportSpecifier)
                        || (checker.nodes.kind(declaration) == SyntaxKind::NamespaceExportDeclaration
                            && checker.source_file_of_for_diagnostics(reference).and_then(|file| checker.node_map.get(file)).is_some_and(|file| matches!(file, Node::SourceFile(source) if tsr_binder::is_external_module_in(source, checker.nodes))))
                }) { continue; }
                let Some(resolved) = checker.semantic_type_naming_alias_target(
                    alias,
                    SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
                    &mut Vec::new(),
                ) else {
                    continue;
                };
                if resolved == target {
                    if ignore_qualification
                        || can_qualify(checker, alias, reference, SymbolFlags::TYPE)
                    {
                        chains.push(vec![alias]);
                    }
                    continue;
                }
                if visited.contains(&resolved)
                    || checker.semantic_type_naming_alias_target(
                        resolved,
                        SymbolFlags::NAMESPACE,
                        &mut Vec::new(),
                    ) != Some(resolved)
                {
                    continue;
                }
                let exports: Vec<_> = checker
                    .binder
                    .symbols()
                    .get(resolved)
                    .exports
                    .iter()
                    .map(|(&name, &symbol)| (name, symbol))
                    .collect();
                visited.push(resolved);
                let chain = try_table(checker, target, reference, &exports, false, true, visited);
                visited.pop();
                if let Some(mut chain) = chain
                    && can_qualify(checker, alias, reference, SymbolFlags::NAMESPACE)
                {
                    chain.insert(0, alias);
                    chains.push(chain);
                }
            }
            chains.sort_by(|a, b| {
                a.len().cmp(&b.len()).then_with(|| {
                    a.iter()
                        .zip(b)
                        .map(|(&a, &b)| checker.compare_symbols(a, b))
                        .find(|order| !order.is_eq())
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            });
            chains.into_iter().next()
        }

        let target = self.binder.merged_symbol(target);
        self.semantic_type_naming_alias_target(target, SymbolFlags::TYPE, &mut Vec::new())?;
        let parent = self.binder.symbols().get(target).parent?;
        self.semantic_type_naming_alias_target(parent, SymbolFlags::NAMESPACE, &mut Vec::new())?;
        let mut current = Some(reference);
        while let Some(node) = current {
            let global_file = self.nodes.kind(node) == SyntaxKind::SourceFile
                && self.binder.symbol_of(node).is_none();
            if !global_file && let Some(locals) = self.binder.locals(node) {
                let table: Vec<_> = locals.iter().map(|(&name, &symbol)| (name, symbol)).collect();
                if let Some(chain) =
                    try_table(self, target, reference, &table, true, false, &mut Vec::new())
                {
                    return Some(chain);
                }
            }
            if matches!(
                self.nodes.kind(node),
                SyntaxKind::ModuleDeclaration | SyntaxKind::SourceFile
            ) && let Some(owner) = self.binder.symbol_of(node)
            {
                let table: Vec<_> = self
                    .binder
                    .symbols()
                    .get(self.binder.merged_symbol(owner))
                    .exports
                    .iter()
                    .map(|(&name, &symbol)| (name, symbol))
                    .collect();
                if let Some(chain) =
                    try_table(self, target, reference, &table, true, false, &mut Vec::new())
                {
                    return Some(chain);
                }
            }
            // Native scope tables include immutable TYPE members and class-expression names.
            if matches!(
                self.nodes.kind(node),
                SyntaxKind::ClassDeclaration
                    | SyntaxKind::ClassExpression
                    | SyntaxKind::InterfaceDeclaration
            ) && let Some(owner) = self.binder.symbol_of(node)
            {
                let table: Vec<_> = self
                    .binder
                    .symbols()
                    .get(owner)
                    .members
                    .iter()
                    .filter(|&(_, &member)| {
                        self.binder.symbols().get(member).flags.intersects(SymbolFlags::TYPE)
                    })
                    .map(|(&name, &symbol)| (name, symbol))
                    .collect();
                if let Some(chain) =
                    try_table(self, target, reference, &table, false, false, &mut Vec::new())
                {
                    return Some(chain);
                }
                if let Some(Node::ClassExpression(class)) = self.node_map.get(node)
                    && let Some(name) = class.name
                    && let Some(chain) = try_table(
                        self,
                        target,
                        reference,
                        &[(name.text, owner)],
                        true,
                        false,
                        &mut Vec::new(),
                    )
                {
                    return Some(chain);
                }
            }
            current = self.nodes.parent(node);
        }
        let globals: Vec<_> =
            self.binder.globals().iter().map(|(&name, &symbol)| (name, symbol)).collect();
        try_table(self, target, reference, &globals, true, false, &mut Vec::new())
    }

    /// The name upstream's `getAccessibleSymbolChain` scope walk prints for
    /// `symbol` at `reference` — **the innermost table wins**, and within a
    /// table the direct hit is checked before the aliases (`trySymbolTable`,
    /// `symbolaccessibility.go:543` then `:562`).
    ///
    /// `Some(name)` — the symbol's own name on a direct hit, or the first
    /// direct alias under `compareSymbols` in the first table that reaches
    /// the symbol. All direct aliases have one-element chains, so native
    /// `trySymbolTable`'s shortest-chain ordering ties on length here.
    /// `None` when no table reaches it.
    ///
    /// A same-file `import a = b` alias competes like any other alias
    /// (`useOnlyExternalAliasing` is false on the baseline path). Until type-refs
    /// round 3 it was excluded from the whole printed name, because admitting
    /// it lost 130 `privacy*` lines (`checker-notes-modobj.md` §10.16). The
    /// exclusion stood in for the `ExportSymbol` arm of `trySymbolTable`
    /// (`symbolaccessibility.go:551`), which is ported below: an exported
    /// declaration's local makes the symbol a CANDIDATE sorted with the
    /// aliases, not an immediate answer, and the earlier declaration wins
    /// (`docs/parity/notes/type-refs.md` §3.3).
    pub(crate) fn best_name(&mut self, symbol: SymbolId, reference: NodeId) -> Option<String> {
        /// One scope table, copied only when the walk reaches it: a name found
        /// in an inner scope never copies the outer ones (notably globals).
        enum Table<'a> {
            Locals(NodeId),
            Exports(SymbolId),
            ClassName(&'a str, SymbolId),
            Globals,
        }
        let own = self.binder.symbols().get(symbol).name;
        let target = self.binder.merged_symbol(symbol);
        let mut tables: Vec<Table<'a>> = Vec::new();
        let mut current = Some(reference);
        while let Some(node) = current {
            if self.binder.locals(node).is_some() {
                tables.push(Table::Locals(node));
            }
            // `someSymbolTableInScope` visits a namespace declaration's
            // exports immediately after its locals
            // (`symbolaccessibility.go:746-775`). An `export import A = M`
            // lives in that exports table, so a locals-only approximation
            // misses `A.C` and falls back to the declaration name `M.C`.
            if matches!(
                self.nodes.kind(node),
                SyntaxKind::ModuleDeclaration | SyntaxKind::SourceFile
            ) && let Some(module) = self.binder.symbol_of(node)
            {
                tables.push(Table::Exports(self.binder.merged_symbol(module)));
            }
            // someSymbolTableInScope / getClassExpressionNameTable (native
            // symbolaccessibility.go:794): the private self-name is an AST
            // binding, not a container locals entry in either implementation.
            // Publish its one-entry display table at this exact scope, before
            // outer aliases and after the scope's existing locals.
            if let Some(Node::ClassExpression(class)) = self.node_map.get(node)
                && let Some(name) = class.name
                && let Some(symbol) = self.binder.symbol_of(node)
            {
                tables.push(Table::ClassName(name.text, symbol));
            }
            current = self.nodes.parent(node);
        }
        tables.push(Table::Globals);
        let binder = self.binder;
        for table in tables {
            // The global table is the program's largest: its direct hit is a
            // keyed lookup (keys are unique, so it is the entry the scan
            // found) and only its `ALIAS` entries — the only ones the
            // candidate loop below acts on — are listed, in table order
            // (`r5-checkperf.md` §7).
            let global_direct =
                matches!(table, Table::Globals).then(|| binder.globals().get(own).copied());
            let table: Vec<(&'a str, SymbolId)> = match table {
                Table::Locals(node) => binder
                    .locals(node)
                    .into_iter()
                    .flat_map(|locals| locals.iter())
                    .map(|(&name, &id)| (name, id))
                    .collect(),
                Table::Exports(module) => binder
                    .symbols()
                    .get(module)
                    .exports
                    .iter()
                    .map(|(&name, &id)| (name, id))
                    .collect(),
                Table::ClassName(name, symbol) => vec![(name, symbol)],
                Table::Globals => self.global_alias_entries().to_vec(),
            };
            let direct = global_direct.unwrap_or_else(|| {
                table.iter().find(|&&(name, _)| name == own).map(|&(_, hit)| hit)
            });
            if direct.is_some_and(|hit| self.binder.merged_symbol(hit) == target) {
                return Some(own.to_string());
            }
            // `trySymbolTable` (`symbolaccessibility.go:551`): a local whose
            // ExportSymbol is the target is NOT a direct hit — it becomes a
            // one-element candidate chain `[symbol]` that competes with the
            // table's aliases under `compareSymbolChains`, i.e. by
            // `compareSymbols` (first declaration's position). This is what
            // keeps `typeof m1_M1_public` in the `privacy*` baselines: the
            // exported namespace is declared before the `import x = …` alias
            // naming it, so the symbol's own name sorts first.
            let mut found: Option<(&str, SymbolId)> = direct
                .filter(|&hit| {
                    self.binder
                        .symbols()
                        .get(hit)
                        .export_symbol
                        .is_some_and(|exported| self.binder.merged_symbol(exported) == target)
                })
                .map(|_| (own, symbol));
            let mut qualified = Vec::new();
            for (name, candidate) in table {
                if !self.binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS) {
                    continue;
                }
                // Upstream's own alias exclusions (`trySymbolTable`,
                // `symbolaccessibility.go:564`–`:575`) under
                // `useOnlyExternalAliasing == false` — which is the value the
                // baseline path passes (`nodebuilderimpl.go:1088` reads it from
                // flags and only hover sets it, `nodebuilder_hover.go:431`).
                // This arm shipped narrower first — external `import a =
                // require` only — because a same-file `import a = b` alias
                // renamed `typeof m1_M1_public` to `typeof m1_im1_private`,
                // 130 right lines lost (§10.9). What protects that family
                // upstream is not the external filter: it is the **per-table
                // direct-hit priority** above, which this walk has had all
                // along. §10.16 measured the widened filter's own population
                // at 26 lines and its bar names the §10.9 recurrence as the
                // falsifier.
                if name == "default" || name == "export=" {
                    continue;
                }
                let declarations = &self.binder.symbols().get(candidate).declarations;
                // Export-specifier-declared symbols are not in scope
                // (`symbolaccessibility.go:574`, mirroring `resolveName`), and
                // a namespace re-export (`export * as ns from "m"`) is omitted
                // on a local-name lookup (`:571`), which every call here is.
                if declarations.iter().any(|&declaration| {
                    matches!(
                        self.node_map.get(declaration),
                        Some(Node::ExportSpecifier(_) | Node::NamespaceExport(_))
                    )
                        // trySymbolTable excludes UMD aliases in external
                        // module files (symbolaccessibility.go:568).
                        || (self.nodes.kind(declaration) == SyntaxKind::NamespaceExportDeclaration
                            && self.source_file_of_for_diagnostics(reference)
                                .and_then(|file| self.node_map.get(file))
                                .is_some_and(|file| matches!(file, Node::SourceFile(source)
                                    if tsr_binder::is_external_module_in(source, self.nodes))))
                }) {
                    continue;
                }
                // §501: the candidate may resolve to an `export=` link whose
                // own target is the symbol — compare through the full chain
                // too (`import * as React from "react"` over
                // `export = __React`).
                let resolved = self.resolve_alias(candidate);
                let reaches = (resolved.map(|t| self.binder.merged_symbol(t)) == Some(target)
                    || resolved.is_some_and(|t| {
                        let full = self.resolve_alias_fully(t);
                        self.binder.merged_symbol(full) == target
                    }))
                    && !self.alias_targets_module_clone(candidate);
                if reaches
                    && found.is_none_or(|(_, best)| self.compare_symbols(candidate, best).is_lt())
                {
                    found = Some((name, candidate));
                }
                // getCandidateListForSymbol looks through an alias's exports
                // before consulting the outer scope (symbolaccessibility.go:630).
                // The container's own direct name must not preempt this route:
                // a private namespace Hidden may be in scope, but C is reached
                // through Local = Hidden and therefore prints Local.C.
                if let Some(resolved) = resolved {
                    let resolved = self.resolve_alias_fully(resolved);
                    let exports: Vec<_> = self
                        .binder
                        .symbols()
                        .get(resolved)
                        .exports
                        .iter()
                        .map(|(&key, &value)| (key, value))
                        .collect();
                    // trySymbolTable checks a direct own-name hit before
                    // sorting aliases even when this table is reached through
                    // a container alias (symbolaccessibility.go:543-547).
                    // Do not resolve an export specifier for this test: an
                    // alias named C is still a competing alias, not direct C.
                    if let Some(&(export_name, exported)) =
                        exports.iter().find(|&&(key, _)| key == own)
                        && self.binder.merged_symbol(exported) == target
                        && !self.is_shadowed_at(candidate, name, reference, SymbolFlags::NAMESPACE)
                    {
                        qualified.push((candidate, exported, format!("{name}.{export_name}")));
                        continue;
                    }
                    for (export_name, exported) in exports {
                        if export_name == "export=" {
                            continue;
                        }
                        let exported_target = self.resolve_alias_fully(exported);
                        if self.binder.merged_symbol(exported_target) == target
                            && !self.is_shadowed_at(
                                candidate,
                                name,
                                reference,
                                SymbolFlags::NAMESPACE,
                            )
                        {
                            qualified.push((candidate, exported, format!("{name}.{export_name}")));
                        }
                    }
                }
            }
            if let Some((name, _)) = found {
                return Some(name.to_string());
            }
            qualified.sort_by(|a, b| {
                self.compare_symbols(a.0, b.0).then_with(|| self.compare_symbols(a.1, b.1))
            });
            if let Some((_, _, name)) = qualified.into_iter().next() {
                return Some(name);
            }
        }
        None
    }

    /// An accessible alias with the target's own name stops qualification
    /// (`symbolaccessibility.go:656-684`). Merged namespace/alias symbols have
    /// their own meaning too: following their entire alias chain would hide a
    /// real shadow, so compare only the immediate target here.
    ///
    /// The hit need only CARRY `ALIAS`: upstream's `AliasExcludes` is `Alias`
    /// alone, so `import Y = X.Y; var Y = 12` is one symbol with both flags,
    /// and `trySymbolTable`'s alias iteration (`:562`) still takes it
    /// (`shadowedInternalModule` records `Y`, not `X.Y`). Requiring exactly
    /// `ALIAS` dropped those merged aliases (`docs/parity/notes/type-refs.md`
    /// §3.4).
    fn own_name_alias_at(&mut self, symbol: SymbolId, reference: NodeId) -> bool {
        let name = self.binder.symbols().get(symbol).name;
        let Some(hit) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            reference,
            name,
            SymbolFlags::TYPE | SymbolFlags::VALUE,
        ) else {
            return false;
        };
        self.binder.symbols().get(hit).flags.contains(SymbolFlags::ALIAS)
            && self.resolve_alias(hit).map(|target| self.binder.merged_symbol(target))
                == Some(self.binder.merged_symbol(symbol))
            && !self.alias_targets_module_clone(hit)
    }

    /// Whether any in-scope alias resolves to `target` itself at `reference` —
    /// `trySymbolTable`'s direct arm (`symbolaccessibility.go:535`), reduced to
    /// existence: when it holds, the bare name is accessible and
    /// [`Checker::symbol_chain`] must not qualify. Same scope walk as
    /// [`Checker::module_name_at`], different comparison target.
    fn alias_in_scope_for(&mut self, target: SymbolId, reference: NodeId) -> bool {
        let target = self.binder.merged_symbol(target);
        let mut candidates: Vec<SymbolId> = Vec::new();
        let mut current = Some(reference);
        while let Some(node) = current {
            if let Some(locals) = self.binder.locals(node) {
                candidates.extend(locals.values().copied());
            }
            current = self.nodes.parent(node);
        }
        // Only `ALIAS` candidates reach the test below; the global table's
        // are listed once (`r5-checkperf.md` §7).
        candidates.extend(self.global_alias_entries().iter().map(|&(_, id)| id));
        candidates.into_iter().any(|candidate| {
            self.binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS)
                && self.resolve_alias(candidate).map(|t| self.binder.merged_symbol(t))
                    == Some(target)
                && !self.alias_targets_module_clone(candidate)
        })
    }

    /// The global symbol table's entries whose symbol carries `ALIAS`, in the
    /// table's iteration order: the only global entries `best_name`'s and
    /// `alias_in_scope_for`'s alias loops act on (`trySymbolTable`'s alias
    /// iteration, `symbolaccessibility.go:562`). The binder's tables are
    /// immutable for the checker's lifetime, so the list is computed once
    /// and is always complete. Private `Checker` (`r5-checkperf.md` §7).
    fn global_alias_entries(&mut self) -> std::rc::Rc<[(&'a str, SymbolId)]> {
        if let Some(entries) = &self.global_alias_entries {
            return entries.clone();
        }
        let binder = self.binder;
        let entries: std::rc::Rc<[(&'a str, SymbolId)]> = binder
            .globals()
            .iter()
            .filter(|&(_, &id)| binder.symbols().get(id).flags.intersects(SymbolFlags::ALIAS))
            .map(|(&name, &id)| (name, id))
            .collect();
        self.global_alias_entries = Some(entries.clone());
        entries
    }

    /// A clone's aliases name that module value, not its callable source.
    fn alias_targets_module_clone(&mut self, alias: SymbolId) -> bool {
        let target = self.resolve_alias_fully(alias);
        if !self
            .binder
            .symbols()
            .get(target)
            .flags
            .intersects(SymbolFlags::CLASS | SymbolFlags::FUNCTION)
        {
            return false;
        }
        let value = self.get_type_of_symbol(alias);
        self.module_value_clones.contains_key(&value)
    }

    fn module_clone_name_at(&mut self, value: TypeId, reference: NodeId) -> Option<String> {
        let mut current = Some(reference);
        while let Some(node) = current {
            let aliases: Vec<_> = self
                .binder
                .locals(node)
                .into_iter()
                .flat_map(|table| table.values().copied())
                .filter(|&symbol| self.binder.symbols().get(symbol).flags == SymbolFlags::ALIAS)
                .collect();
            let mut candidates = Vec::new();
            for alias in aliases {
                let name = self.binder.symbols().get(alias).name;
                if self.get_type_of_symbol(alias) == value
                    && !self.is_shadowed_at(alias, name, reference, SymbolFlags::VALUE)
                {
                    candidates.push(alias);
                }
            }
            if let Some(&alias) = candidates.iter().min_by(|&&a, &&b| self.compare_symbols(a, b)) {
                return Some(self.binder.symbols().get(alias).name.to_string());
            }
            current = self.nodes.parent(node);
        }
        None
    }

    /// Whether a type is `errorType` itself, by identity.
    ///
    /// Not a flag test: `errorType` and `anyType` share `TypeFlagsAny` and are
    /// distinguished only by identity, which is the whole point of them being
    /// separate types (`checker.go:979`).
    ///
    /// Both of the port's error identities answer true (ADR-0048): its gap
    /// ([`Intrinsics::error`](crate::Intrinsics)) and upstream's own
    /// `errorType` ([`Intrinsics::native_error`](crate::Intrinsics)).
    ///
    /// This is upstream's `isErrorType` (`checker.go:26638`: `errorType`, or an
    /// any-flagged type with an alias — this port's `unresolved_types`) with
    /// the gap counted in, because the gap stands where upstream may have
    /// answered `errorType`. A caller that mirrors native `isErrorType` uses
    /// this; a caller that only means "the port computed nothing" uses
    /// [`Checker::is_gap`]; one that mirrors `IsTypeAny` uses
    /// [`Checker::is_type_any`]. The audit that sorted the call sites is
    /// `docs/parity/notes/r5-errorsplit3.md` §2.
    pub(crate) fn is_error(&self, id: TypeId) -> bool {
        id == self.intrinsics.error
            || id == self.intrinsics.native_error
            || self.unresolved_types.contains(&id)
    }

    /// Whether a type is the port's gap: "this port could not compute it".
    ///
    /// No upstream counterpart, by construction: a caller asking this declines
    /// where upstream has no test, and upstream's `errorType` and unresolved
    /// references flow past it like any other any-flagged type (ADR-0048).
    ///
    /// The deferred mints count as the gap: the `keyof T` and `T[K]` types
    /// this port mints as an OBJECT-flagged `Named` in
    /// [`Checker::unresolved_types`] ([`Checker::deferred_keyof_types`], the
    /// declared-index mints) are its placeholder for an index or indexed-access
    /// type it does not represent, not anything upstream computes. Upstream's
    /// own unresolved reference is any-flagged with an alias
    /// (`checker.go:26641`), which is exactly the any-flagged members of that
    /// set. Measured: `keyofAndForIn` and the three
    /// `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks` cases
    /// report spuriously when the mints stop declining
    /// (`docs/parity/notes/r5-errorsplit3.md` §3).
    pub(crate) fn is_gap(&self, id: TypeId) -> bool {
        id == self.intrinsics.error
            || (self.unresolved_types.contains(&id)
                && !self.store.get(id).flags.intersects(crate::flags::TypeFlags::ANY))
    }

    /// The error a union or intersection answers once `constituent`, an error
    /// type, joins the error already `seen` (`IncludesError`,
    /// `checker.go:25659` / `:26092`): upstream's `errorType` while every error
    /// constituent was upstream's, the port's gap as soon as one was not — a
    /// gap in a constituent is a gap in the whole (ADR-0048).
    pub(crate) fn included_error(&self, seen: Option<TypeId>, constituent: TypeId) -> TypeId {
        let native = self.intrinsics.native_error;
        if constituent == native && seen.is_none_or(|seen| seen == native) {
            native
        } else {
            self.intrinsics.error
        }
    }

    /// Ported from `ast.GetCombinedNodeFlags` / `getCombinedFlags`
    /// (`internal/ast/utilities.go:1180`).
    ///
    /// `const` is not a flag on the declaration: it is on the enclosing
    /// `VariableDeclarationList`, so answering "is this a const?" means walking
    /// up. This is the first place the port needs `NodeTable::parent`, and the
    /// reason [ADR-0033](../../../docs/adr/0033-the-parser-fills-the-node-map.md)
    /// insisted the lookup answer parent ids rather than only declarations.
    pub(crate) fn combined_node_flags(&self, declaration: NodeId) -> NodeFlags {
        // §756: `getCombinedFlags` starts at `GetRootDeclaration(node)`
        // (`ast/utilities.go:1181`/`:1173`) — a binding element walks out
        // through its pattern to the owning declaration. Without this step a
        // destructured `const { kind: x } = obj` never sees the CONST flag,
        // because the binding element's parent is the pattern and not the
        // declaration list.
        let mut node = declaration;
        while self.nodes.kind(node) == SyntaxKind::BindingElement {
            let Some(pattern) = self.nodes.parent(node) else { break };
            let Some(owner) = self.nodes.parent(pattern) else { break };
            node = owner;
        }
        let mut flags = self.nodes.flags(node);
        if self.nodes.kind(node) == SyntaxKind::VariableDeclaration {
            let Some(parent) = self.nodes.parent(node) else { return flags };
            node = parent;
        }
        if self.nodes.kind(node) == SyntaxKind::VariableDeclarationList {
            flags |= self.nodes.flags(node);
            let Some(parent) = self.nodes.parent(node) else { return flags };
            node = parent;
        }
        if self.nodes.kind(node) == SyntaxKind::VariableStatement {
            flags |= self.nodes.flags(node);
        }
        flags
    }

    /// How many times an expression type was actually computed rather than
    /// served from the memo. See [`Checker::computations`]'s field docs.
    #[must_use]
    pub fn computations(&self) -> usize {
        self.computations
    }

    /// How many types exist. For tests that assert interning actually interns.
    #[must_use]
    pub fn type_count(&self) -> usize {
        self.store.len()
    }

    /// The node table this checker reads.
    #[must_use]
    pub fn nodes(&self) -> &NodeTable {
        self.nodes
    }

    /// `getSymbolOfPartOfRightHandSideOfImportEquals` (`checker.go:14474`):
    /// the symbol a name inside an `import a = b.c.d` module reference
    /// denotes. A right-hand identifier stands for its qualified name; a name
    /// that is the whole reference's root or the left of a further qualified
    /// name is a namespace (`import a = |b|`, `import a = |b.c|.d`), and the
    /// whole reference takes every meaning (`import a = |b.c|`). Both resolve
    /// with `dontResolveAlias`, so an alias answers itself.
    ///
    /// Over [`Checker::resolve_entity_name_ex`], which is `ignoreErrors =
    /// true`; upstream passes `false`, and the diagnostics that would report
    /// belong to the import-equals check, not to this query. No cache:
    /// upstream has none either; the callers memoise the types they build.
    pub fn get_symbol_of_part_of_right_hand_side_of_import_equals(
        &mut self,
        entity_name: NodeId,
    ) -> Option<SymbolId> {
        let mut entity_name = entity_name;
        // `ast.IsRightSideOfQualifiedNameOrPropertyAccess`.
        if self.nodes.kind(entity_name) == SyntaxKind::Identifier
            && let Some(parent) = self.nodes.parent(entity_name)
            && match self.node_map.get(parent) {
                Some(Node::QualifiedName(qualified)) => {
                    qualified.right.and_then(|right| right.node_id) == Some(entity_name)
                }
                Some(Node::PropertyAccessExpression(access)) => {
                    access.name.and_then(|name| name.node_id()) == Some(entity_name)
                }
                _ => false,
            }
        {
            entity_name = parent;
        }
        let name = match self.node_map.get(entity_name)? {
            Node::Identifier(identifier) => tsr_ast::EntityName::Identifier(identifier),
            Node::QualifiedName(qualified) => tsr_ast::EntityName::QualifiedName(qualified),
            _ => return None,
        };
        let in_qualified_name = self
            .nodes
            .parent(entity_name)
            .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::QualifiedName);
        if matches!(name, tsr_ast::EntityName::Identifier(_)) || in_qualified_name {
            return self.resolve_entity_name_ex(name, SymbolFlags::NAMESPACE, true);
        }
        self.resolve_entity_name_ex(
            name,
            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE,
            true,
        )
    }

    /// The export-assignment arm of `getSymbolOfNameOrPropertyAccessExpression`
    /// (`checker.go:31780`): the expression of `export = x` / `export default
    /// x` resolves with every meaning including `Alias`, `ignoreErrors`, and
    /// without `dontResolveAlias` — but since `Alias` is in the meaning,
    /// `resolveEntityName`'s chain walk (`checker.go:15821`) never runs and an
    /// alias answers itself. `None` is upstream's nil/`unknownSymbol` miss.
    ///
    /// Only the identifier spelling is answered: `export = a.b` is a property
    /// access, an expression node, which `getTypeOfNode` types before it ever
    /// asks for this symbol, and [`Checker::resolve_entity_name_ex`] has no
    /// property-access arm. No cache, as for the import-equals sibling above.
    pub fn get_symbol_of_export_assignment_expression(&mut self, name: NodeId) -> Option<SymbolId> {
        let parent = self.nodes.parent(name)?;
        let Some(Node::ExportAssignment(assignment)) = self.node_map.get(parent) else {
            return None;
        };
        if assignment.expression.and_then(|expression| expression.node_id()) != Some(name) {
            return None;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(name) else { return None };
        self.resolve_entity_name_ex(
            tsr_ast::EntityName::Identifier(identifier),
            SymbolFlags::VALUE | SymbolFlags::TYPE | SymbolFlags::NAMESPACE | SymbolFlags::ALIAS,
            false,
        )
    }

    /// `getDeclaredTypeOfAlias` (`checker.go:24094`): the declared type of
    /// the symbol an alias chain resolves to (`resolveAlias` resolves the
    /// whole chain), `errorType` when the chain ends unresolved or at a
    /// symbol that declares no type. `tryGetDeclaredTypeOfSymbol`
    /// (`checker.go:23678`) tests this arm last, after the type meanings, so
    /// callers ask [`Checker::get_declared_type_of_symbol`] first for a
    /// symbol carrying one. Not memoised per alias (upstream's
    /// `declaredTypeLinks`); the target's declared type is.
    pub fn get_declared_type_of_alias(&mut self, symbol: SymbolId) -> TypeId {
        let target = self.resolve_alias_fully(symbol);
        if self.binder.symbols().get(target).flags.intersects(SymbolFlags::ALIAS) {
            return self.intrinsics.error;
        }
        self.get_declared_type_of_symbol(target)
    }
}
