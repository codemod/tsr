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
    /// The per-invocation memo for flow nodes with more than one antecedent.
    ///
    /// Upstream's `c.sharedFlows` (`checker.go:799`), a stack that each
    /// `getFlowTypeOfReference` truncates back to its own start. Without it the
    /// backwards walk is exponential on branchy code — a hang rather than a
    /// wrong answer, which is why it is part of the port and not an
    /// optimisation.
    pub(crate) shared_flows: Vec<(tsr_binder::FlowId, crate::flow::FlowType)>,
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
    /// `compilerOptions.allowUnreachableCode`, read as `IsTrue()`
    /// (`checker.go:12534`) — so unset is `false` and the comma-operator
    /// diagnostic is on by default.
    pub(crate) allow_unreachable_code: bool,
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
    /// Symbols `checkFunctionOrConstructorSymbol` has already visited.
    ///
    /// Upstream's `links.functionOrConstructorChecked` (`checker.go:3463`,
    /// commented *"Only check the symbol once"*). Without it a three-overload
    /// function reports three times and the `diagnostics` suite compares
    /// multisets.
    pub(crate) function_symbol_checked: rustc_hash::FxHashSet<tsr_binder::SymbolId>,
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
            declared_types: FxHashMap::default(),
            this_types: FxHashMap::default(),
            instantiations: FxHashMap::default(),
            type_reference_targets: FxHashMap::default(),
            unresolved_types: rustc_hash::FxHashSet::default(),
            resolutions: Resolutions::new(),
            flow_analysis_disabled: false,
            shared_flows: Vec::new(),
            instantiation_depth: 0,
            instantiation_count: 0,
            strict_null_checks: true,
            no_unchecked_side_effect_imports: true,
            strict_property_initialization: true,
            file_has_parse_errors: false,
            allow_unreachable_code: false,
            ambient_statement_reported: rustc_hash::FxHashSet::default(),
            function_symbol_checked: rustc_hash::FxHashSet::default(),
            tuple_types: FxHashMap::default(),
            tuple_element_lists: FxHashMap::default(),
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
            {
                let signature = signatures[0].clone();
                self.rendering_composites.insert(id);
                let out = self.signature_to_string_at(&signature, reference);
                self.rendering_composites.remove(&id);
                return Some(out);
            }
            let printed = self.type_to_string(id);
            return self.qualified_name_at(id, printed, reference);
        };
        self.module_name_at(module, reference).map(|name| format!("typeof {name}"))
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
                if printed.len() > owner_name.len()
                    && printed.starts_with(owner_name)
                    && printed.as_bytes()[owner_name.len()] == b'.'
                    && let Some(better) = self.best_name(owner, reference, false)
                    && better != owner_name
                {
                    let mut out = String::with_capacity(printed.len() + better.len());
                    out.push_str(better);
                    out.push_str(&printed[owner_name.len()..]);
                    return Some(out);
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
        let Some(qualifier) =
            self.symbol_chain(symbol, reference, SymbolFlags::TYPE | SymbolFlags::VALUE, 0)
        else {
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
        let parent = self.binder.merged_symbol(self.binder.symbols().get(symbol).parent?);
        if self.is_module_symbol(parent) || self.is_ambient_module(parent) {
            // `trySymbolTable`'s direct arm: the bare name is accessible
            // through an alias, so no qualifier may fire.
            if self.alias_in_scope_for(symbol, reference) {
                return None;
            }
            // `getAccessibleSymbolChain`'s alias arm for the *container*,
            // including its ambiguity refusal (`checker-notes-nameres.md` §14):
            // an ambiguous container declines outright — upstream picked some
            // alias there, so the `import("…")` form would be a shape upstream
            // did not print.
            match self.module_alias_at(parent, reference) {
                Ok(alias) => return Some(format!("{alias}.")),
                Err(true) => return None,
                Err(false) => {}
            }
            // The ambient branch of `getSpecifierForModuleSymbol`
            // (`nodebuilderimpl.go:1260`): the specifier IS the module's name.
            if self.is_ambient_module(parent) && !self.is_module_symbol(parent) {
                let module_name = self.binder.symbols().get(parent).name;
                return Some(format!("import(\"{module_name}\")."));
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

    /// `ast.IsInJSFile`: whether the node's source file was a `.js`-family
    /// file. Reads [`tsr_ast::NodeFlags::JAVASCRIPT_FILE`] off the **root**,
    /// because that is the one node the program stamps
    /// (`tsr-compiler/src/lib.rs`) — upstream stamps every node from the
    /// parser, which this parser cannot (it never sees the file name,
    /// ADR-0016). Climbing is O(depth); depth is bounded by the source.
    ///
    /// `false` for a checker built without a program: the unit-test
    /// constructors have no file names, and every fixture is TypeScript.
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
    fn module_name_at(&mut self, module: SymbolId, reference: NodeId) -> Option<&'a str> {
        self.module_alias_at(module, reference).ok()
    }

    /// [`Checker::module_name_at`]'s tri-state worker: `Ok(name)` for the
    /// unique in-scope alias, `Err(true)` for **ambiguity** (≥2 distinct
    /// names), `Err(false)` for none at all. [`Checker::symbol_chain`] needs
    /// the distinction — an ambiguous container means upstream picked *some*
    /// alias, so falling through to the `import("…")` form would print a shape
    /// upstream did not; only a container no alias reaches may take it.
    fn module_alias_at(&mut self, module: SymbolId, reference: NodeId) -> Result<&'a str, bool> {
        let mut candidates: Vec<SymbolId> = Vec::new();
        let mut current = Some(reference);
        while let Some(node) = current {
            if let Some(locals) = self.binder.locals(node) {
                candidates.extend(locals.values().copied());
            }
            current = self.nodes.parent(node);
        }
        candidates.extend(self.binder.globals().values().copied());

        let mut found: Option<&'a str> = None;
        for candidate in candidates {
            if !self
                .binder
                .symbols()
                .get(candidate)
                .flags
                .intersects(tsr_binder::SymbolFlags::ALIAS)
            {
                continue;
            }
            if self.resolve_alias(candidate) != Some(module) {
                continue;
            }
            let name = self.binder.symbols().get(candidate).name;
            match found {
                // The same alias reached twice through two scopes is one alias,
                // and a name colliding with itself is not ambiguity.
                Some(existing) if existing == name => {}
                Some(_) => return Err(true),
                None => found = Some(name),
            }
        }
        found.ok_or(false)
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
                if self.resolve_alias(candidate).map(|t| self.binder.merged_symbol(t))
                    != Some(target)
                {
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
