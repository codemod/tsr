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
    /// Per-file memo: does the file contain import/export machinery? The
    /// §31 gate (`checker-notes-narrow.md`).
    pub(crate) file_import_machinery: FxHashMap<NodeId, bool>,
    /// The memoized `typeof globalThis` type (`checker-notes-narrow.md` §33).
    pub(crate) global_this_type: Option<TypeId>,
    /// One `unique symbol` per WRITTEN `unique symbol` type node
    /// (`checker-notes-callres.md` §27).
    pub(crate) unique_symbol_nodes: FxHashMap<NodeId, TypeId>,
    /// One `this` type per class/interface declaration — upstream's
    /// `d.thisType` for TYPE-POSITION `this` (`checker-notes-callres.md`
    /// §28).
    pub(crate) this_type_nodes: FxHashMap<NodeId, TypeId>,
    /// One members-carrying qualified reference type per (namespace-site
    /// spelling, target symbol) — `checker-notes-narrow.md` §41.
    pub(crate) qualified_reference_types: FxHashMap<(String, SymbolId), TypeId>,
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
    pub(crate) flow_loop_cache: FxHashMap<(usize, u64), (TypeId, Vec<TypeId>)>,
    /// `isReachableFlowNode`'s cache over SHARED flow nodes — upstream's
    /// `flowNodeReachable` (`checker.go`, keyed by `*ast.FlowNode`). §743.
    pub(crate) flow_node_reachable: FxHashMap<usize, bool>,
    /// In-process loop-label computations with their so-far unions —
    /// upstream's `flowLoopKeys`/`flowLoopTypes` stacks. Non-empty means the
    /// checker is in a transient fixpoint pass, and `check_expression` must
    /// not persist results (`checker-notes-narrow.md` §12.6).
    pub(crate) flow_loop_stack: Vec<((usize, u64), Vec<TypeId>)>,
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
    /// A class symbol to its `this` type, upstream's `d.thisType`
    /// (`checker.go:17334`). One per class, so `this` has a stable identity
    /// inside one.
    pub(crate) this_types: FxHashMap<SymbolId, TypeId>,
    /// `symbol -> the type it *declares*`, upstream's
    /// `declaredTypeLinks[symbol].declaredType`.
    ///
    /// Separate from [`Checker::symbol_types`] because they are different
    /// questions about the same symbol: a class `C` declares the instance type
    /// `C` and *has* the type `typeof C`. Merging them would answer one with the
    /// other.
    pub(crate) declared_types: FxHashMap<SymbolId, TypeId>,
    /// In-progress resolutions, for circularity detection.
    pub(crate) resolutions: Resolutions<SymbolId>,
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
    pub(crate) shared_flows: Vec<(tsr_binder::FlowId, crate::flow::FlowType)>,
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
    /// `noImplicitThis`, the fifth member of the strict family — the four
    /// beside it were ported when their consumers arrived, and TS2683 is this
    /// one's. §416.
    pub(crate) no_implicit_this: bool,
    /// `getEmitModuleKind` — `options.module`, or ES2015 for an ES2015-or-later
    /// target and `CommonJS` otherwise. §478.
    pub(crate) module_kind: tsr_core::ModuleKind,
    /// `c.legacyDecorators` — `experimentalDecorators` is on. §644.
    pub(crate) legacy_decorators: bool,
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
    /// never clears freshness (upstream's `getRegularTypeOfObjectLiteral` at
    /// widening sites), so a variable's type can wrongly read as fresh where
    /// upstream's has been cloned regular — measured, and the corpus holds no
    /// case where that direction shows.
    pub(crate) fresh_object_literal_types: rustc_hash::FxHashSet<crate::types::TypeId>,
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
    /// Only the names are stored: every member of a pattern-implied object is
    /// `any` by construction (`getTypeFromObjectBindingPattern`,
    /// `checker.go:17938`, with no initializer to infer from), so the value
    /// needs no table.
    pub(crate) pattern_implied_members: rustc_hash::FxHashMap<crate::types::TypeId, Vec<String>>,
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
    /// (`getLocalJsxNamespace`), is not ported — see
    /// [`Checker::jsx_namespace_symbol`].
    pub(crate) jsx_namespace: String,
    /// What JSX compiles to. TS2874 is reported **only** under
    /// [`tsr_core::JsxEmit::React`] (`checker.go:28508`). §261.
    pub(crate) jsx_emit: tsr_core::JsxEmit,
    /// `exactOptionalPropertyTypes` (`checker.go:987`): a `?:` property's
    /// optionality is `missingType`, removed at write positions.
    pub(crate) exact_optional_property_types: bool,
    /// §82: depth cap for aliased-condition inlining — upstream's
    /// `inlineLevel` (`flow.go`), capped at 5.
    pub(crate) alias_inline_level: u8,
    /// §85: `T & {}`-family mints, keyed (type variable, spelling).
    pub(crate) non_null_type_variables: FxHashMap<(TypeId, String), TypeId>,
    /// §85's reverse map: mint → (base type variable, refinement kind).
    pub(crate) non_null_mint_bases: FxHashMap<TypeId, (TypeId, crate::flow::NonNullKind)>,
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
    /// §92: types PRODUCED by alias evaluation — the only intersections the
    /// shape property road may search (a WRITTEN intersection answering
    /// confidently was 134 G→W in the discriminated-union family).
    pub(crate) alias_evaluated_types: rustc_hash::FxHashSet<TypeId>,
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
    /// Once per symbol for TS2385. §1021.
    pub(crate) overload_accessibility_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
    /// Symbols `checkFunctionOrConstructorSymbol` has already visited.
    ///
    /// Upstream's `links.functionOrConstructorChecked` (`checker.go:3463`,
    /// commented *"Only check the symbol once"*). Without it a three-overload
    /// function reports three times and the `diagnostics` suite compares
    /// multisets.
    pub(crate) function_symbol_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
    /// Symbols whose overload ambient agreement has been checked. §673.
    pub(crate) overload_agreement_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
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
    pub(crate) tuple_element_lists: FxHashMap<TypeId, (Vec<TypeId>, bool)>,
    /// §79: interning for optional-element tuples, keyed on (member,
    /// optional) pairs so `[number, string?]` and `[number, string]` stay
    /// distinct types.
    pub(crate) optional_tuple_types: FxHashMap<OptionalTupleKey, TypeId>,
    /// §79: which positions of an optional-element tuple carry `?` — read at
    /// the index roads, where an optional element answers `| undefined`.
    pub(crate) tuple_optional_masks: FxHashMap<TypeId, Vec<bool>>,
    /// §87: a trailing-rest variadic's tuple NODE — resolved lazily by
    /// positional consumers; the print stays §40's.
    pub(crate) tuple_rest_tails: FxHashMap<TypeId, tsr_ast::NodeId>,
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
    /// The values of [`Checker::instantiated_signatures`], for the O(1)
    /// membership test the call resolver makes.
    pub(crate) minted_signature_types: rustc_hash::FxHashSet<TypeId>,
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
    /// How many instantiations this checker has performed in total.
    ///
    /// Upstream's `c.instantiationCount` (`checker.go:591`), the other half of
    /// the `checker.go:22111` guard. **A recorded divergence:** upstream resets
    /// it to zero per checked statement (`checker.go:2246`, `:2509`, `:7563`);
    /// this port has no check traversal to reset from (ADR-0040), so the budget
    /// of 5,000,000 is per checker — per file, as the conformance harness
    /// constructs one checker per case. Stricter than upstream on a file whose
    /// statements would legitimately instantiate more than 5M types in
    /// aggregate, which no corpus case does.
    pub(crate) instantiation_count: u32,
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
        // **A KNOWN DIVERGENCE LIVES ON THIS LINE.** Upstream seeds
        // `undefinedWideningType`, not `undefinedType` — two types that PRINT
        // ALIKE and widen differently, so `const x = undefined` is `undefined`
        // and `let x = undefined` is `any` (`checker.go:955`). This port has
        // exactly one `undefined` (`crate::intrinsics`) and no widening
        // variant, so `let` answers `undefined` where upstream answers `any`.
        //
        // Accepted deliberately rather than hidden. Gap and wrong both score as
        // not-right, so the 22 affected sites cost no gradient; what they cost
        // is diagnostic separability on those 22, against ~1,675 lines the
        // symbol makes right. The sites are enumerated in
        // `docs/architecture/checker-notes-enums.md` so the follow-up can
        // verify it fixed exactly those, and
        // `tests/globals.rs::a_let_initialised_with_undefined_records_a_known_divergence`
        // reddens the moment a widening intrinsic lands.
        //
        // **This is a missing intrinsic, not a merged identity.** The widening
        // type does not exist here at all, which is visible to anyone who greps
        // `intrinsics.rs` and finds one `undefined` where upstream has two —
        // unlike `038def4`, where both identities existed and one was used for
        // the other.
        //
        // See `Binder::declare_synthesised_globals` for the other half.
        let mut symbol_types = FxHashMap::default();
        if let Some(undefined) = binder.undefined_symbol() {
            symbol_types.insert(undefined, intrinsics.undefined);
        }
        Self {
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
            symbol_assignment_scan: FxHashMap::default(),
            symbol_empty_array_assignment_scan: FxHashMap::default(),
            last_assignment_pos: FxHashMap::default(),
            enum_member_regular: FxHashMap::default(),
            alias_placeholders: FxHashMap::default(),
            file_import_machinery: FxHashMap::default(),
            global_this_type: None,
            unique_symbol_nodes: FxHashMap::default(),
            this_type_nodes: FxHashMap::default(),
            qualified_reference_types: FxHashMap::default(),
            assignments_marked: rustc_hash::FxHashSet::default(),
            definitely_assigned: rustc_hash::FxHashSet::default(),
            flow_loop_cache: FxHashMap::default(),
            flow_node_reachable: FxHashMap::default(),
            flow_loop_stack: Vec::new(),
            declared_types: FxHashMap::default(),
            this_types: FxHashMap::default(),
            instantiations: FxHashMap::default(),
            type_reference_targets: FxHashMap::default(),
            reference_display_arity: FxHashMap::default(),
            literal_this_types: FxHashMap::default(),
            unresolved_types: rustc_hash::FxHashSet::default(),
            resolutions: Resolutions::new(),
            flow_analysis_disabled: false,
            flow_disabled_containers: rustc_hash::FxHashSet::default(),
            shared_flows: Vec::new(),
            narrow_value_stack: std::collections::HashSet::new(),
            call_inference_signatures: rustc_hash::FxHashMap::default(),
            resolving_signature_calls: rustc_hash::FxHashSet::default(),
            contextual_return_in_flight: rustc_hash::FxHashSet::default(),
            contextual_return_depth: 0,
            intra_expression_member_maps: rustc_hash::FxHashMap::default(),
            narrow_value_types: rustc_hash::FxHashMap::default(),
            union_origin: rustc_hash::FxHashMap::default(),
            enum_value_types: rustc_hash::FxHashMap::default(),
            enum_access_spelling: rustc_hash::FxHashMap::default(),
            named_union_by_members: rustc_hash::FxHashMap::default(),

            instantiation_depth: 0,
            instantiation_count: 0,
            strict_null_checks: true,
            no_implicit_this: false,
            module_kind: tsr_core::ModuleKind::None,
            legacy_decorators: false,
            standard_class_fields: false,
            allow_synthetic_defaults: false,
            allow_importing_ts_extensions: false,
            no_unchecked_side_effect_imports: true,
            no_unchecked_indexed_access: false,
            use_unknown_in_catch_variables: false,
            strict_property_initialization: true,
            file_has_parse_errors: false,
            merge_conflicts_reported: false,
            assignability_probe: Vec::new(),
            allow_unreachable_code: false,
            unreachable_code_is_error: false,
            preserve_const_enums: false,
            exhaustive_switches: rustc_hash::FxHashSet::default(),
            no_implicit_any: false,
            js_literal_types: rustc_hash::FxHashSet::default(),
            fresh_object_literal_types: rustc_hash::FxHashSet::default(),
            object_literal_index_infos: rustc_hash::FxHashMap::default(),
            pattern_implied_members: rustc_hash::FxHashMap::default(),
            jsx_namespace: "React".to_string(),
            jsx_emit: tsr_core::JsxEmit::None,
            exact_optional_property_types: false,
            alias_inline_level: 0,
            non_null_type_variables: FxHashMap::default(),
            non_null_mint_bases: FxHashMap::default(),
            pre_optional_marker: FxHashMap::default(),
            jsdoc_entries: FxHashMap::default(),
            jsdoc_hosts: FxHashMap::default(),
            identity_unmapped_type_parameters: false,
            render_type_parameter_scope: Vec::new(),
            alias_body_evaluations: FxHashMap::default(),
            alias_evaluated_types: rustc_hash::FxHashSet::default(),
            alias_evaluation_bindings: Vec::new(),
            alias_named_signature_types: rustc_hash::FxHashSet::default(),
            no_unused_locals: false,
            no_unused_parameters: false,
            symbol_reference_kinds: FxHashMap::default(),
            unused_check_nodes: Vec::new(),
            referenced_member_names: crate::unused::MemberNames::default(),
            file_is_ambient: false,
            checked_files: rustc_hash::FxHashSet::default(),
            ambient_statement_reported: rustc_hash::FxHashSet::default(),
            enum_checked: rustc_hash::FxHashSet::default(),
            merged_spaces_checked: rustc_hash::FxHashSet::default(),
            overload_accessibility_checked: rustc_hash::FxHashSet::default(),
            function_symbol_checked: rustc_hash::FxHashSet::default(),
            overload_agreement_checked: rustc_hash::FxHashSet::default(),
            modifier_chain_reported: rustc_hash::FxHashSet::default(),
            decorator_error_reported: rustc_hash::FxHashSet::default(),
            tuple_types: FxHashMap::default(),
            tuple_element_lists: FxHashMap::default(),
            optional_tuple_types: FxHashMap::default(),
            tuple_optional_masks: FxHashMap::default(),
            tuple_rest_tails: FxHashMap::default(),
            type_parameter_symbols: FxHashMap::default(),
            signature_types: FxHashMap::default(),
            instantiated_signatures: FxHashMap::default(),
            minted_signature_types: rustc_hash::FxHashSet::default(),
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
        // The strict family (`checker.go:919-926`).
        self.strict_null_checks = options.strict_option_value(options.strict_null_checks);
        self.no_implicit_this = options.strict_option_value(options.no_implicit_this);
        self.legacy_decorators = options.experimental_decorators.is_true();
        // `GetEmitStandardClassFields` — the flag, defaulting to
        // `target >= ES2022`. §751.
        self.standard_class_fields = match options.use_define_for_class_fields {
            tsr_core::Tristate::True => true,
            tsr_core::Tristate::False => false,
            tsr_core::Tristate::Unknown => options.target >= tsr_core::ScriptTarget::ES2022,
        };
        self.module_kind = if options.module == tsr_core::ModuleKind::None {
            if options.target >= tsr_core::ScriptTarget::ES2015 {
                tsr_core::ModuleKind::ES2015
            } else {
                tsr_core::ModuleKind::CommonJS
            }
        } else {
            options.module
        };
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
        self.use_unknown_in_catch_variables =
            options.strict_option_value(options.use_unknown_in_catch_variables);
        self.no_implicit_any = options.strict_option_value(options.no_implicit_any);

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
            // dotted name and only its root is the namespace.
            options.jsx_factory.split('.').next().unwrap_or("React").to_string()
        };

        // `== TSTrue` (`checker.go:6115`) — `strict` does not reach it.
        self.no_unchecked_indexed_access = options.no_unchecked_indexed_access.is_true();
        self.exact_optional_property_types = options.exact_optional_property_types.is_true();

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
            // The composite-print twin (`checker-notes-modobj.md` §10.13): a
            // single-signature type re-renders from its structure at the site,
            // so embedded named types take their qualifiers and renames. The
            // visiting set is the cycle guard a self-referential function type
            // needs (`type F = () => F`): a re-entered id falls back to its
            // baked text, exactly what the site-less renderer would produce.
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
                && !self.alias_named_signature_types.contains(&id)
            {
                let signatures = signatures.clone();
                self.rendering_composites.insert(id);
                // §102 (decoded): shadow renames against the SITE, byText
                // claims across the signatures of ONE render.
                let mut claimed = rustc_hash::FxHashSet::default();
                let mut out = String::from("{ ");
                for signature in &signatures {
                    let signature = self.rename_type_parameters_for_site(
                        signature.clone(),
                        reference,
                        &mut claimed,
                    );
                    out.push_str(&self.signature_member_text_at(&signature, reference));
                    out.push_str("; ");
                }
                out.push('}');
                self.rendering_composites.remove(&id);
                return Some(out);
            }
            // §97 (`checker-notes-narrow.md`): a union carrying ORIGIN
            // entries re-renders each entry at the site — the alias-named
            // entries qualify through the same stack §95 uses; any decline
            // keeps the baked spelling.
            if let Some(entries) = self.union_origin.get(&id).cloned()
                && !self.rendering_composites.contains(&id)
            {
                self.rendering_composites.insert(id);
                let mut parts = Vec::with_capacity(entries.len());
                let mut complete = true;
                for entry in entries {
                    if let Some(part) = self.type_to_string_at(entry, reference) {
                        parts.push(part);
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
        if let Some(name) = self.module_name_at(module, reference) {
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
        if self.module_alias_at(module, reference).is_none() {
            let declarations = &self.binder.symbols().get(module).declarations;
            if let [declaration] = declarations.as_slice()
                && let Some(tsr_ast::Node::ModuleDeclaration(node)) =
                    self.node_map.get(*declaration)
                && let Some(tsr_ast::ModuleName::StringLiteral(literal)) = node.name
            {
                // SS196: escaped, via the shared `quote` — see calls.rs.
                return Some(format!("typeof import({})", crate::printing::quote(literal.text)));
            }
            // §521 — the FILE-module half §143 left waiting: a module whose
            // one declaration is a SOURCE FILE prints the relative form,
            // `typeof import("./foo")`. The module symbol's name is the
            // resolved path with its extension stripped
            // (`bind_source_file_as_external_module`), so the harness's
            // rooted `/foo` spells `./foo` by prefixing the dot — the same
            // shape every baseline in the pool records
            // (`getSpecifierForModuleSymbol`'s relative half, reduced to the
            // one directory layout the corpus mounts).
            if let [declaration] = declarations.as_slice()
                && self.nodes.kind(*declaration) == SyntaxKind::SourceFile
            {
                let name = self.binder.symbols().get(module).name;
                if let Some(relative) = name.strip_prefix('/')
                    && !relative.contains('/')
                    && !relative.is_empty()
                {
                    return Some(format!("typeof import(\"./{relative}\")"));
                }
            }
        }
        None
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
                    if let Some(written) = &parameter.written_constraint {
                        out.push_str(written);
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
            if let Some(written) = &parameter.written_text {
                out.push_str(written);
            } else {
                let rendered = self
                    .type_to_string_at(parameter.r#type, reference)
                    .unwrap_or_else(|| self.type_to_string(parameter.r#type));
                out.push_str(&rendered);
            }
        }
        out.push_str("): ");
        match (&signature.predicate, &signature.written_return) {
            (Some(predicate), _) => out.push_str(&self.type_predicate_to_string(predicate)),
            (None, Some(written)) => out.push_str(written),
            (None, None) => {
                let rendered = self
                    .type_to_string_at(signature.r#type, reference)
                    .unwrap_or_else(|| self.type_to_string(signature.r#type));
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
    fn reference_text_at(
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
        let named = if let Some(better) = self.best_name(target, reference, false)
            && better != target_name
        {
            better.to_string()
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
                // §673: the enum's OWN type is registered in
                // `enum_member_owners` too — §55.1's divergent twins put the
                // regular spelling there — and it prints as the bare enum name
                // with no `{enum}.` prefix to rename. The guard below needs a
                // prefix (`printed.len() > owner_name.len()`), so `Mode` fell
                // straight through unqualified while `Mode.Open` right beside it
                // was qualified. `import f = require('./m')` wants `f.Mode` at
                // every one of those positions (`enumFromExternalModule`).
                if printed == owner_name
                    && let Some(qualifier) = self.symbol_chain(
                        owner,
                        reference,
                        SymbolFlags::TYPE | SymbolFlags::VALUE,
                        0,
                    )
                {
                    let mut out = String::with_capacity(printed.len() + qualifier.len());
                    out.push_str(&qualifier);
                    out.push_str(&printed);
                    return Some(out);
                }
                if printed.len() > owner_name.len()
                    && printed.starts_with(owner_name)
                    && printed.as_bytes()[owner_name.len()] == b'.'
                {
                    if let Some(better) = self.best_name(owner, reference, false)
                        && better != owner_name
                    {
                        let mut out = String::with_capacity(printed.len() + better.len());
                        out.push_str(better);
                        out.push_str(&printed[owner_name.len()..]);
                        return Some(out);
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
        if let Some(better) = self.best_name(symbol, reference, false)
            && better != name
        {
            let mut out = String::with_capacity(printed.len() + better.len());
            out.push_str(&printed[..suffix_at - name.len()]);
            out.push_str(better);
            out.push_str(&printed[suffix_at..]);
            return Some(out);
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
                && let Some(better) = self.best_name(parent, reference, false)
                && better != parent_name
            {
                let mut out = String::with_capacity(printed.len() + better.len());
                out.push_str(better);
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
    fn symbol_chain(
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
            if let Some(alias) = self.module_alias_at(parent, reference) {
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
                // The port's module names are ROOT-relative virtual paths
                // (`/file`); the same-directory slice is a single leading
                // slash. Deeper structure keeps the decline — a wrong
                // specifier is worse than the bare name.
                let stem = module_name.strip_prefix('/').unwrap_or(module_name);
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
                if !same_file && !imported_here && !stem.contains('/') && !stem.is_empty() {
                    return Some(format!("import(\"./{stem}\")."));
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
            .best_name(parent, reference, true)
            .unwrap_or(self.binder.symbols().get(parent).name);
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
    fn needs_qualification(
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
    pub(crate) fn is_ambient_module(&self, symbol: SymbolId) -> bool {
        self.binder.symbols().get(symbol).declarations.iter().any(|&declaration| {
            matches!(
                self.node_map.get(declaration),
                Some(Node::ModuleDeclaration(module))
                    if matches!(module.name, Some(tsr_ast::ModuleName::StringLiteral(_)))
            )
        })
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

    fn module_name_at(&mut self, module: SymbolId, reference: NodeId) -> Option<&'a str> {
        self.module_alias_at(module, reference)
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
    fn module_alias_at(&mut self, module: SymbolId, reference: NodeId) -> Option<&'a str> {
        let mut tables: Vec<Vec<SymbolId>> = Vec::new();
        let mut current = Some(reference);
        while let Some(node) = current {
            if let Some(locals) = self.binder.locals(node) {
                tables.push(locals.values().copied().collect());
            }
            current = self.nodes.parent(node);
        }
        tables.push(self.binder.globals().values().copied().collect());

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
                let resolved = self.resolve_alias(candidate);
                if resolved != Some(module)
                    && resolved.map(|r| self.resolve_alias_fully(r)) != Some(module_target)
                {
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
    fn compare_symbols(&self, a: SymbolId, b: SymbolId) -> std::cmp::Ordering {
        if a == b {
            return std::cmp::Ordering::Equal;
        }
        let key = |symbol: SymbolId| {
            self.binder.symbols().get(symbol).declarations.first().map(|&declaration| {
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
            })
        };
        // `len(s1.Declarations) != 0` before the comparison (`:376-383`): a
        // symbol WITH declarations sorts before one without, which is what
        // `Option`'s own ordering gives once `None` is mapped to the greater
        // side.
        match (key(a), key(b)) {
            (Some(a_key), Some(b_key)) if a_key != b_key => return a_key.cmp(&b_key),
            (Some(_), None) => return std::cmp::Ordering::Less,
            (None, Some(_)) => return std::cmp::Ordering::Greater,
            _ => {}
        }
        let names = self.binder.symbols().get(a).name.cmp(self.binder.symbols().get(b).name);
        if names != std::cmp::Ordering::Equal {
            return names;
        }
        a.cmp(&b)
    }

    /// The name upstream's `getAccessibleSymbolChain` scope walk prints for
    /// `symbol` at `reference` — **the innermost table wins**, and within a
    /// table the direct hit is checked before the aliases (`trySymbolTable`,
    /// `symbolaccessibility.go:543` then `:562`).
    ///
    /// `Some(name)` — the symbol's own name on a direct hit, or the unique
    /// alias name in the first table that reaches the symbol. `None` when no
    /// table reaches it, or when one table holds ≥2 distinct alias names for
    /// it — the chain-choice rule this port declines to guess
    /// (`checker-notes-nameres.md` §14 measured both tie-breaks at ~96%
    /// coincidence).
    /// `admit_local_import_equals` — whether a **same-file** `import a = b`
    /// may supply the name. The corpus splits on print position
    /// (`checker-notes-modobj.md` §10.16): a chain **segment** takes it
    /// (`typeof m1_im1_private.c1`, the §10.9 residue's own baselines), while
    /// the **whole printed name** does not (`m1_im1_private :` itself records
    /// `typeof m1_M1_public`, and admitting the local alias there lost 130
    /// lines in exactly the four `privacy*` cases — the §10.16 bar's named
    /// falsifier, fired and honoured).
    fn best_name(
        &mut self,
        symbol: SymbolId,
        reference: NodeId,
        admit_local_import_equals: bool,
    ) -> Option<&'a str> {
        let own = self.binder.symbols().get(symbol).name;
        let target = self.binder.merged_symbol(symbol);
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
                return Some(own);
            }
            let mut found: Option<&'a str> = None;
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
                let excluded = declarations.iter().any(|&declaration| {
                    matches!(
                        self.node_map.get(declaration),
                        Some(Node::ExportSpecifier(_) | Node::NamespaceExport(_))
                    ) || (!admit_local_import_equals
                        && matches!(
                            self.node_map.get(declaration),
                            Some(Node::ImportEqualsDeclaration(node))
                                if !matches!(
                                    node.module_reference,
                                    Some(tsr_ast::ModuleReference::ExternalModuleReference(_))
                                )
                        ))
                });
                if excluded {
                    continue;
                }
                // §501: the candidate may resolve to an `export=` link whose
                // own target is the symbol — compare through the full chain
                // too (`import * as React from "react"` over
                // `export = __React`).
                let resolved = self.resolve_alias(candidate);
                let reaches = resolved.map(|t| self.binder.merged_symbol(t)) == Some(target)
                    || resolved
                        .map(|t| {
                            let full = self.resolve_alias_fully(t);
                            self.binder.merged_symbol(full)
                        })
                        .map(Some)
                        == Some(Some(target));
                if !reaches {
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
        candidates.extend(self.binder.globals().values().copied());
        candidates.into_iter().any(|candidate| {
            self.binder.symbols().get(candidate).flags.intersects(SymbolFlags::ALIAS)
                && self.resolve_alias(candidate).map(|t| self.binder.merged_symbol(t))
                    == Some(target)
        })
    }

    /// Whether a type is `errorType` itself, by identity.
    ///
    /// Not a flag test: `errorType` and `anyType` share `TypeFlagsAny` and are
    /// distinguished only by identity, which is the whole point of them being
    /// separate types (`checker.go:979`).
    pub(crate) fn is_error(&self, id: TypeId) -> bool {
        id == self.intrinsics.error || self.unresolved_types.contains(&id)
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
        let mut flags = self.nodes.flags(declaration);
        let mut node = declaration;
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
}
