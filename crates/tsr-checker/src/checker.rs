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
use tsr_ast::{NodeFlags, NodeId, NodeMap, NodeTable, SyntaxKind};
use tsr_binder::{BindResult, SymbolId};

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
    /// `symbol -> its type`, upstream's `valueSymbolLinks[symbol].resolvedType`.
    pub(crate) symbol_types: FxHashMap<SymbolId, TypeId>,
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
            symbol_types,
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
            tuple_types: FxHashMap::default(),
            tuple_element_lists: FxHashMap::default(),
            type_parameter_symbols: FxHashMap::default(),
            signature_types: FxHashMap::default(),
            instantiated_signatures: FxHashMap::default(),
            minted_signature_types: rustc_hash::FxHashSet::default(),
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
                self.is_module_symbol(symbol).then_some(symbol)
            }
            _ => None,
        };
        let Some(module) = module else { return Some(self.type_to_string(id)) };
        self.module_name_at(module, reference).map(|name| format!("typeof {name}"))
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
                Some(_) => return None,
                None => found = Some(name),
            }
        }
        found
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
