//! Control-flow narrowing: what a *reference* is worth, as opposed to what its
//! declaration says.
//!
//! Ported from `internal/checker/flow.go` (2,764 lines) at the pinned commit.
//! `bd tsr-4sc.11`. The binder has built the flow graph since `tsr-binder`
//! landed and nothing read it until this module.
//!
//! # The shape
//!
//! [`Checker::get_flow_type_of_reference`] walks the flow graph **backwards**
//! from a reference until it can say what the type is at that point:
//! `getFlowTypeOfReference` (`flow.go:77`) sets up a [`FlowState`] and
//! `getTypeAtFlowNode` (`flow.go:117`) does the walk, one flow node at a time,
//! narrowing as it passes conditions and stopping at assignments and at the
//! start of the container.
//!
//! # Why a partial port of this is safe, unlike most of the checker
//!
//! **Upstream's `narrowType` default arm returns the type unchanged.** A guard
//! form that is not ported here is therefore not a wrong answer — it is the
//! answer this checker already gives, the declared type. All of the risk in
//! this module lives in the guards that *are* ported; none in the ones that are
//! not. That is the opposite of most items in this port, where a missing arm
//! yields `errorType` and a gap.
//!
//! The one exception, and it is why [`Checker::is_matching_reference`] is
//! deliberately narrow: a reference match that is too *loose* narrows the wrong
//! reference, which produces a plausible wrong answer rather than a gap.
//!
//! # The two limits are behaviour, not protection
//!
//! Both are ported here from the first commit rather than deferred as
//! `bd tsr-el3.2` defers the instantiation limits, and the distinction is that
//! there the loop the limit guards does not exist yet, while here it does:
//!
//! - **The depth cap of 2,000** does not merely stop the walk. It sets
//!   `flowAnalysisDisabled`, after which *every* reference in the rest of the
//!   containing function answers `errorType` (`flow.go:82`). Skipping it would
//!   change what a deep function answers.
//! - **The shared-flow memo** is a correctness-shaped optimisation: a flow node
//!   that is the antecedent of more than one node is visited once per path
//!   without it, so the walk is exponential on branchy code. That is a hang, not
//!   a wrong number.

use tsr_ast::{Node, NodeFlags, NodeId, SyntaxKind};
use tsr_binder::{Antecedents, FlowFlags, FlowId, FlowStore, ReduceLabel, SymbolFlags, SymbolId};

use crate::{
    checker::Checker,
    flags::TypeFlags,
    types::{TypeData, TypeId},
};

/// Upstream's `FlowType` (`flow.go`): a type, plus whether it is still
/// provisional because a loop has not been walked to a fixed point.
///
/// `incomplete` is carried and propagated but nothing yet *acts* on it — the
/// loop-label arm that produces incomplete types is not ported (see
/// [`Checker::get_type_at_flow_node`]). Keeping the field is what makes that
/// arm a fill-in rather than a change of signature.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FlowType {
    /// The type in effect.
    pub t: TypeId,
    /// Whether the answer is provisional (upstream's `incomplete`).
    pub incomplete: bool,
}

/// One `getFlowTypeOfReference` invocation's working state
/// (upstream's `FlowState`, `flow.go`).
struct FlowState {
    /// Element types seen at `ARRAY_MUTATION` nodes while walking an auto-array
    /// reference. §710's `addEvolvingArrayElementType`.
    array_elements: Vec<TypeId>,
    /// Active `finally` path reductions (`FlowState.reduceLabels`, flow.go:48).
    /// A read past a finally block excludes paths that are still throwing or
    /// returning; a read inside the block retains those paths.
    reduce_labels: Vec<ReduceLabel>,
    /// The reference node the question is about.
    reference: NodeId,
    /// What that reference resolves to, when it is an identifier.
    ///
    /// Upstream has no such field: it compares reference *expressions* with
    /// `isMatchingReference`, because a reference can be `a.b.c` and there is
    /// no single symbol for it. This port kept a symbol because the match was
    /// identifier-only — and `bd tsr-6ka` measured that as the binding
    /// constraint on every narrowing arm, so the match is now structural and
    /// this field is `None` for an access-expression reference.
    ///
    /// It is kept rather than dropped because the identifier arm genuinely
    /// needs it: the binder records an assignment's flow node against the
    /// *declaration*, whose identity is a symbol and not an expression to
    /// compare against.
    symbol: Option<SymbolId>,
    /// The type the declaration gives it.
    declared_type: TypeId,
    /// The type at the top of the flow graph.
    initial_type: TypeId,
    /// Whether the declaration is one upstream types with `autoType`
    /// (`checker.go:976`) rather than with a real `any`.
    ///
    /// Upstream distinguishes the two by *identity* — `autoType` is a separate
    /// intrinsic that also prints `any` — and asks `declaredType == autoType` in
    /// [`Checker::get_type_at_flow_assignment`]. Here the question is asked of
    /// the declaration instead, in [`Checker::is_auto_typed_declaration`]; see
    /// that method for why the identity trick is not reproduced.
    is_auto: bool,
    /// Whether the declaration is upstream's `autoArrayType` trigger — no
    /// annotation, empty-array-literal initializer (`flow.go:1404`'s guard).
    /// Decides the §14 `ARRAY_MUTATION` depth accounting.
    is_auto_array: bool,
    /// Whether the reference's control-flow container differs from its
    /// declaration's — upstream's `isOuterVariable`, deciding the START arm's
    /// answer (`checker-notes-narrow.md` §9.7).
    outer_reference: bool,
    /// §50's pseudo-reference payload: the binding PATTERN whose sibling
    /// elements act as discriminants on the walked union
    /// (`checker-notes-narrow.md`).
    discriminant_pattern: Option<NodeId>,
    /// The container bound the walk may not leave — upstream's
    /// `flowContainer`, possibly extended outward by the §13 loop
    /// (`checker.go:11139`). `None` bounds nothing beyond the graph itself.
    flow_container: Option<NodeId>,
    /// Where this invocation's entries in `shared_flows` begin.
    shared_flow_start: usize,
    /// §739: the start of the innermost in-flight loop-label's element region
    /// in `array_elements`. The `ARRAY_MUTATION` arm dedupes against
    /// `array_elements[element_dedupe_mark..]` only, so a label's contribution
    /// slice is SELF-CONTAINED — an element already accumulated downstream of
    /// the label (between the reference and the label) must not suppress the
    /// same element's contribution inside the label, or the slice cached for
    /// that label misses it and every later query replaying the cache misses
    /// it too. `0` outside any loop-label computation, i.e. dedupe against the
    /// whole accumulator, which is §710's original behaviour.
    element_dedupe_mark: usize,
    /// Recursion depth, against the 2,000 cap.
    depth: u32,
}

/// Upstream's cap (`flow.go:118`), reproduced exactly rather than rounded.
const MAX_FLOW_DEPTH: u32 = 2_000;

/// `getBranchLabelAntecedents` (flow.go): use the innermost active reduction.
/// The iterator borrows only the graph, so recursive walks may update the
/// reduction stack without allocating a temporary antecedent list.
fn branch_label_antecedents<'flow>(
    store: &'flow FlowStore,
    flow: FlowId,
    reduce_labels: &[ReduceLabel],
) -> Antecedents<'flow> {
    for reduce in reduce_labels.iter().rev() {
        if reduce.target == flow {
            return store.reduced_antecedents(*reduce);
        }
    }
    store.antecedents(flow)
}

/// One constituent's fate under `getNarrowedTypeWorker`'s ladder
/// (`checker-notes-narrow.md` §22).
#[derive(Clone, Copy, PartialEq, Eq)]
enum NarrowedConstituent {
    /// The ladder mapped it (possibly to the candidate).
    Mapped(TypeId),
    /// No rung fired: the true branch drops it.
    Dropped,
    /// A rung the relation cannot decide.
    Undecidable,
}

/// What `getSignatureFromDeclaration(container).thisParameter` answers for
/// `getExplicitThisType` (`flow.go:2215`).
enum ThisParameterOfContainer {
    /// No `this` parameter: the class arm decides.
    Absent,
    /// A `this` parameter, with its explicit type when it has one
    /// (`getExplicitTypeOfSymbol`, `flow.go:2155`).
    Present(Option<TypeId>),
}

bitflags::bitflags! {
    /// What is knowable about a type without narrowing it
    /// (upstream's `TypeFacts`, `checker.go`).
    ///
    /// **A fragment.** Upstream's table has around thirty facts — the `typeof`
    /// families among them. Only the bits this module's ported guards ask for
    /// are here: the two truthiness bits, and the six nullable-equality bits
    /// [`Checker::narrow_type_by_equality`] filters on. The rest arrive with
    /// their consumers rather than as a table nothing reads, which is the same
    /// discipline `FlowFlags` followed in the binder.
    ///
    /// The bit *positions* are upstream's own (`checker.go:400`–`:425`) even
    /// though nothing here depends on the numeric values, so that a future arm
    /// ported from a `switch` on `TypeFacts` can be read against upstream
    /// without a translation table.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct TypeFacts: u32 {
        /// `typeof x === "string"` can hold for a value of the type.
        const TYPEOF_EQ_STRING = 1 << 0;
        /// `typeof x === "number"` can hold.
        const TYPEOF_EQ_NUMBER = 1 << 1;
        /// `typeof x === "bigint"` can hold.
        const TYPEOF_EQ_BIG_INT = 1 << 2;
        /// `typeof x === "boolean"` can hold.
        const TYPEOF_EQ_BOOLEAN = 1 << 3;
        /// `typeof x === "symbol"` can hold.
        const TYPEOF_EQ_SYMBOL = 1 << 4;
        /// `typeof x === "object"` can hold.
        const TYPEOF_EQ_OBJECT = 1 << 5;
        /// `typeof x === "function"` can hold.
        const TYPEOF_EQ_FUNCTION = 1 << 6;
        /// `typeof x` can be a host-object string outside the standard eight.
        const TYPEOF_EQ_HOST_OBJECT = 1 << 7;
        /// `typeof x !== "string"` can hold.
        const TYPEOF_NE_STRING = 1 << 8;
        /// `typeof x !== "number"` can hold.
        const TYPEOF_NE_NUMBER = 1 << 9;
        /// `typeof x !== "bigint"` can hold.
        const TYPEOF_NE_BIG_INT = 1 << 10;
        /// `typeof x !== "boolean"` can hold.
        const TYPEOF_NE_BOOLEAN = 1 << 11;
        /// `typeof x !== "symbol"` can hold.
        const TYPEOF_NE_SYMBOL = 1 << 12;
        /// `typeof x !== "object"` can hold.
        const TYPEOF_NE_OBJECT = 1 << 13;
        /// `typeof x !== "function"` can hold.
        const TYPEOF_NE_FUNCTION = 1 << 14;
        /// `typeof x` can be something other than a host-object string.
        const TYPEOF_NE_HOST_OBJECT = 1 << 15;
        /// `TypeFactsAllTypeofNE` (`checker.go`): every `typeof x !== …` bit.
        const ALL_TYPEOF_NE = Self::TYPEOF_NE_STRING.bits()
            | Self::TYPEOF_NE_NUMBER.bits()
            | Self::TYPEOF_NE_BIG_INT.bits()
            | Self::TYPEOF_NE_BOOLEAN.bits()
            | Self::TYPEOF_NE_SYMBOL.bits()
            | Self::TYPEOF_NE_OBJECT.bits()
            | Self::TYPEOF_NE_FUNCTION.bits()
            | Self::NE_UNDEFINED.bits();
        /// The type can compare equal to `undefined`.
        const EQ_UNDEFINED = 1 << 16;
        /// The type can compare equal to `null`.
        const EQ_NULL = 1 << 17;
        /// The type can compare equal to either under `==`.
        const EQ_UNDEFINED_OR_NULL = 1 << 18;
        /// The type can compare *unequal* to `undefined`.
        const NE_UNDEFINED = 1 << 19;
        /// The type can compare unequal to `null`.
        const NE_NULL = 1 << 20;
        /// The type can compare unequal to both under `!=`.
        const NE_UNDEFINED_OR_NULL = 1 << 21;
        /// The type can be truthy.
        const TRUTHY = 1 << 22;
        /// The type can be falsy.
        const FALSY = 1 << 23;
        /// The type *is* (or contains) `undefined` — upstream's
        /// `TypeFactsIsUndefined` (`checker.go:425`), carried by
        /// `TypeFactsUndefinedFacts` and **not** by `VoidFacts`, which is
        /// otherwise the same set. Deliberately absent from the
        /// undecidable-default aggregate below: upstream sets it only for
        /// `undefined`-bearing types, and a default that claimed it would
        /// stop `getNonUndefinedType` callers stripping where upstream
        /// strips.
        const IS_UNDEFINED = 1 << 24;
        /// `TypeFactsIsNull` (`internal/checker/checker.go`), including
        /// union constituents when comparing nullable callback parameters.
        const IS_NULL = 1 << 25;
    }
}

impl Checker<'_, '_> {
    /// The type of `reference`, narrowed by the control flow reaching it.
    ///
    /// `getFlowTypeOfReference` (`flow.go:77`) and its `Ex` form (`:81`).
    /// `declared_type` is what the symbol's declaration gives it, and is both the
    /// initial type at the top of the graph and the answer when there is no flow
    /// node at all.
    ///
    /// # What is not ported, and what each costs
    ///
    /// - **Evolving *array* types** — `ObjectFlagsEvolvingArray`,
    ///   `autoArrayType`, `finalizeEvolvingArrayType`. The scalar half of the
    ///   same mechanism *is* ported (see [`Checker::is_auto_typed_declaration`]
    ///   and [`Checker::get_type_at_flow_assignment`]).
    ///
    ///   **§703 corrects what stood here.** This said the array half was
    ///   "selected by an empty-array initialiser, which this port answers
    ///   `false` for". That is **stale**: [`Checker::is_auto_array_declaration`]
    ///   answers `true` for exactly `var x = []` with no annotation, so the
    ///   array half IS selected and `state.is_auto_array` is set.
    ///
    ///   The gap is one step further in: the `ARRAY_MUTATION` arm below only
    ///   guards recursion depth and then walks to the antecedent — it never
    ///   **accumulates** the pushed element types, so an evolving array never
    ///   evolves and the declared `any[]` survives. `controlFlowArrays` wants
    ///   `() => (string | number)[]` and gets `() => any[]` on **80 lines** for
    ///   that reason. What is missing is `addEvolvingArrayElementType` at each
    ///   `x.push(e)` / `x[i] = e` plus `finalizeEvolvingArrayType` at the
    ///   reference.
    /// - *(Ported since; kept as a correction of the record.)* This list named
    ///   the `unreachableNeverType` and non-null-assertion fallbacks at the end
    ///   of `getFlowTypeOfReferenceEx` as unported. Both are now
    ///   [`Checker::flow_result_or_declared`].
    /// - **`flowContainer`**, so the `Start` arm does not continue outward into
    ///   an enclosing function's flow.
    pub(crate) fn get_flow_type_of_reference(
        &mut self,
        reference: NodeId,
        symbol: Option<SymbolId>,
        declared_type: TypeId,
    ) -> TypeId {
        let declared_type = self.narrowable_type_for_reference(declared_type, reference);
        // checkIdentifier's removeOptionalityFromDeclaredType keeps the
        // parameter's write type intact, but an actual default with no
        // undefined value removes undefined at the START of its read flow.
        // A documented [p=default] has no initializer in the emitted code.
        let mut initial_type = None;
        if self.strict_null_checks
            && let Some(declaration) =
                symbol.and_then(|symbol| self.binder.symbols().get(symbol).value_declaration)
            && let Some(Node::ParameterDeclaration(parameter)) = self.node_map.get(declaration)
            && let Some(initializer) = parameter.initializer
            && self.get_type_facts(declared_type).contains(TypeFacts::IS_UNDEFINED)
            && !self.parameter_initializer_contains_undefined(declaration, initializer)
        {
            initial_type = Some(self.get_type_with_facts(declared_type, TypeFacts::NE_UNDEFINED));
        }
        self.get_flow_type_of_reference_ex(reference, symbol, declared_type, initial_type)
    }

    /// Native `parameterInitializerContainsUndefined`: memoize the initializer
    /// facts and retain undefined during recursive default resolution.
    fn parameter_initializer_contains_undefined(
        &mut self,
        declaration: NodeId,
        initializer: tsr_ast::Expression<'_>,
    ) -> bool {
        if let Some(&contains) = self.parameter_initializer_contains_undefined.get(&declaration) {
            return contains;
        }
        self.parameter_initializer_contains_undefined.insert(declaration, true);
        let typed = self.check_expression(initializer);
        let contains = typed == self.intrinsics.error
            || self.get_type_facts(typed).contains(TypeFacts::IS_UNDEFINED);
        self.parameter_initializer_contains_undefined.insert(declaration, contains);
        contains
    }

    /// §50's pseudo-reference walk: narrow `parent_union` at `reference`'s
    /// flow position, with `pattern`'s sibling elements acting as
    /// discriminants (`checker-notes-narrow.md` §50).
    pub(crate) fn narrow_destructured_parent(
        &mut self,
        reference: NodeId,
        pattern: NodeId,
        parent_union: TypeId,
    ) -> TypeId {
        if self.flow_analysis_disabled {
            return parent_union;
        }
        let Some(flow) = self.binder.flow_of(reference) else {
            return parent_union;
        };
        let mut state = FlowState {
            array_elements: Vec::new(),
            reduce_labels: Vec::new(),
            reference,
            symbol: None,
            declared_type: parent_union,
            initial_type: parent_union,
            is_auto: false,
            discriminant_pattern: Some(pattern),
            is_auto_array: false,
            outer_reference: false,
            flow_container: self.control_flow_container(reference),
            shared_flow_start: self.shared_flows.len(),
            element_dedupe_mark: 0,
            depth: 0,
        };
        let answer = self.get_type_at_flow_node(&mut state, flow);
        let result = self.finalize_evolving_array(&mut state, answer).t;
        self.shared_flows.truncate(state.shared_flow_start);
        self.flow_result_or_declared(reference, result, parent_union)
    }

    /// `getFlowTypeOfReferenceEx`'s explicit `initialType` parameter.
    ///
    /// Upstream's default *is* the declared type (`checker.go`,
    /// `getFlowTypeOfReference`), and this port additionally substitutes
    /// `undefined` for an auto-typed declaration — the arm documented at
    /// [`Checker::get_flow_type_of_reference`]. `Some(initial)` overrides both.
    /// Defaulted JSDoc parameters use it to remove undefined from entry reads,
    /// without changing the declared write type.
    ///
    /// # The diagnostic caller deliberately starts uninitialized
    ///
    /// TS2454 (`Variable_0_is_used_before_being_assigned`, `checker.go:11191`)
    /// is decided by running the graph with the top-of-graph type set to
    /// `T | undefined` and asking whether `undefined` survives to the reference
    /// (`checker.go:11170`). Any *query* would answer the declared type there —
    /// which is exactly what the reporting road wants to differ from, and why
    /// ADR-0040 calls the two roads different entry points rather than one
    /// function with a flag.
    pub(crate) fn get_flow_type_of_reference_ex(
        &mut self,
        reference: NodeId,
        symbol: Option<SymbolId>,
        declared_type: TypeId,
        initial_type: Option<TypeId>,
    ) -> TypeId {
        if self.flow_analysis_disabled {
            return self.intrinsics.error;
        }
        // §14: a reference inside a container whose analysis tripped the
        // too-large bail answers upstream's give-up value — `errorType`,
        // WHICH UPSTREAM PRINTS AS `any`. The `any` intrinsic here is that
        // observable, not a computed claim; ADR-0038's `error` printing is
        // for THIS port's failures, and this is upstream's own (TS2563).
        if !self.flow_disabled_containers.is_empty() {
            let mut ancestor = Some(reference);
            while let Some(id) = ancestor {
                if self.flow_disabled_containers.contains(&id) {
                    // §14.1: in a JS file the give-up value reaches the
                    // baseline printer VERBATIM (` : error`, the §35
                    // shape); the `any` rendering is the TS half only.
                    return if self.in_js_file(reference) {
                        self.intrinsics.error
                    } else {
                        self.intrinsics.any
                    };
                }
                ancestor = self.nodes.parent(id);
            }
        }
        let Some(flow) = self.binder.flow_of(reference) else {
            // Upstream returns the declared type when a reference has no flow
            // node — it is not an error, it is a position the binder never
            // needed to record a flow for.
            return declared_type;
        };
        // Only a declaration can be automatically typed, so an access-
        // expression reference is never auto: `a.b` has a declared property
        // type whatever the flow says.
        let is_auto = symbol.is_some_and(|symbol| self.is_auto_typed_declaration(symbol));
        let mut state = FlowState {
            array_elements: Vec::new(),
            reduce_labels: Vec::new(),
            reference,
            symbol,
            declared_type,
            // `checkIdentifier` (`checker.go:11165`): when the declared type is
            // automatic, the type at the top of the graph is `undefined`, not
            // the declared type. That is what makes `let x; x;` answer
            // `undefined` and `let x; if (c) x = 1; x;` answer
            // `number | undefined` — the second is a union built by the branch
            // label out of the assignment on one path and this on the other.
            initial_type: match initial_type {
                Some(initial) => initial,
                // `checker.go:11165`'s substituted initial — and the §9.3
                // rule's non-strict half (`checker-notes-narrow.md` §9.4):
                // with `strictNullChecks` off, a never-assigned auto
                // reference answers `any` (the corpus's non-strict
                // want-`any` population), and the initial IS that answer;
                // strict same-container keeps `undefined`, which the §9.2
                // revert's losses pinned.
                //
                // # §227: an open row this arm decides, and a diagnosis that
                // did NOT complete
                //
                // Three deficit-1 cases —
                // `compiler/exportAssignmentWith{DeclareAndExportModifiers,
                // DeclareModifier,ExportModifier}` — are `var x;` followed by
                // `declare export = x;`, and upstream records `>x : any` on
                // **both** the declaration and the reference. This port gets
                // the declaration right and answers `undefined` at the
                // reference, which is this arm's strict leg.
                //
                // What was checked and eliminated, so nobody repeats it:
                //
                // - **Not a compiler-options bug.** The obvious theory is that
                //   the harness turns `strictNullChecks` on where upstream
                //   leaves it off. It does not: `strict_option_value`
                //   (`tsr-core/src/options.rs:916`) is
                //   `if unknown { !strict.is_false() }`, and upstream's
                //   `GetStrictOptionValue` (`core/compileroptions.go:294`) is
                //   `options.Strict != TSFalse`. Character-for-character the
                //   same rule, so both run these fixtures strict.
                // - **Not a missing `autoType`.** `is_auto` exists and is set
                //   correctly here.
                //
                // What is NOT explained: how upstream reaches `any`. Reading
                // `checker.go:11149-11158`, `assumeInitialized` looks false for
                // this shape — `t` IS `autoType`, which disables the whole
                // `t != autoType && …` group, and no other disjunct applies
                // (not a parameter, not an alias, same container, no `!`, the
                // `declare` sits on the export assignment rather than on
                // `var x`). That gives `initialType = undefinedType`, a flow
                // type of `undefined`, and neither final branch at `:11182` or
                // `:11190` fires — so a static read of upstream predicts
                // `undefined`, which is what this port already answers, and the
                // baseline says otherwise.
                //
                // So one of those steps is wrong and it needs upstream
                // *executed*, not read. **Do not "fix" this arm from the
                // baseline**: making the strict leg answer `any` would satisfy
                // three cases by contradicting §9.2, whose revert losses are
                // what pinned `undefined` here in the first place.
                //
                // ## Narrowed once more, from the errors baseline
                //
                // `exportAssignmentWithDeclareModifier.errors.txt` records
                // exactly one diagnostic, `TS1120: An export assignment cannot
                // have modifiers`, and **no `TS2454` (*used before being
                // assigned*)**. That is decisive about which branch upstream
                // took: `TS2454` is emitted by the `else if` at
                // `checker.go:11190`, so its absence means that branch did not
                // run, which means `assumeInitialized` was **true**, which
                // means `initialType = t = autoType`, the flow type stayed
                // `autoType`, and the first branch answered
                // `convertAutoToAny(autoType)` = `any`.
                //
                // So the open question is now exactly one step wide: **which
                // disjunct of `assumeInitialized` (`checker.go:11150-11158`) is
                // true for this shape?** Reading them, none obviously is —
                // not a parameter, not an alias, same flow container, no
                // spread/module-exports/binding-element, no `!`, the whole
                // `t != autoType && …` group disabled because `t` IS auto, and
                // the `declare` sits on the export assignment rather than on
                // `var x`, so the declaration carries no `Ambient` flag.
                //
                // One of those readings is wrong, and separating them is a
                // single instrumented run of upstream. That instrument does not
                // exist and **two rows now want it** — this one and the
                // type-alias singleton in `checker-notes-nearmiss.md` §229 —
                // which is the argument for building it rather than routing
                // around it a third time.
                None if is_auto && !self.strict_null_checks => self.intrinsics.any,
                None if is_auto => self.intrinsics.undefined,
                None => declared_type,
            },
            is_auto,
            discriminant_pattern: None,
            is_auto_array: symbol.is_some_and(|symbol| self.is_auto_array_declaration(symbol)),
            outer_reference: self.is_outer_reference(reference, symbol),
            // checkIdentifier supplies its explicit variable bound. Receiver
            // and access callers use native getFlowTypeOfReference's nil bound;
            // the START arm decides which creation-site edges they may follow.
            flow_container: symbol
                .and_then(|symbol| self.extended_flow_container(reference, Some(symbol))),
            shared_flow_start: self.shared_flows.len(),
            element_dedupe_mark: 0,
            depth: 0,
        };
        let answer = self.get_type_at_flow_node(&mut state, flow);
        let result = self.finalize_evolving_array(&mut state, answer).t;
        self.shared_flows.truncate(state.shared_flow_start);
        self.flow_result_or_declared(reference, result, declared_type)
    }

    /// The tail of `getFlowTypeOfReferenceEx` (`flow.go:111`): the declared
    /// type replaces the flow answer when the walk ended unreachable, or when
    /// the reference is the operand of `x!` and narrowing left nothing but
    /// `null`/`undefined` — `x = undefined; x!` reads the declared type, not
    /// `never` (`typeGuardsAsAssertions`).
    fn flow_result_or_declared(
        &mut self,
        reference: NodeId,
        result: TypeId,
        declared_type: TypeId,
    ) -> TypeId {
        if result == self.intrinsics.unreachable_never {
            return declared_type;
        }
        if self
            .nodes
            .parent(reference)
            .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::NonNullExpression)
            && !self.type_of(result).flags.intersects(TypeFlags::NEVER)
        {
            let non_null = self.get_type_with_facts(result, TypeFacts::NE_UNDEFINED_OR_NULL);
            if self.type_of(non_null).flags.intersects(TypeFlags::NEVER) {
                return declared_type;
            }
        }
        result
    }

    /// `getFlowTypeOfProperty` (`checker.go:11444`): an access-expression
    /// reference whose declared type is upstream's `autoType`, starting at
    /// `start` — the reference's own flow node, or a constructor's
    /// `ReturnFlowNode` for `getFlowTypeInConstructor` (`flow.go:2466`).
    ///
    /// Upstream synthesizes a fresh `this.x` node parented to the constructor;
    /// this tree is immutable (ADR-0012), so the caller passes an existing
    /// `this.x` reference from the same constructor. Matching is structural
    /// (`isMatchingReference`), so any same-named `this` access in that
    /// container denotes the same reference. The auto declared type is
    /// spelled `any`, as for auto-typed variables here.
    pub(crate) fn get_flow_type_of_property(
        &mut self,
        reference: NodeId,
        start: Option<FlowId>,
        initial_type: TypeId,
    ) -> TypeId {
        let any = self.intrinsics.any;
        if self.flow_analysis_disabled {
            return self.intrinsics.error;
        }
        let Some(flow) = start.or_else(|| self.binder.flow_of(reference)) else {
            return any;
        };
        let mut state = FlowState {
            array_elements: Vec::new(),
            reduce_labels: Vec::new(),
            reference,
            symbol: None,
            declared_type: any,
            initial_type,
            is_auto: true,
            discriminant_pattern: None,
            is_auto_array: false,
            outer_reference: false,
            flow_container: self.extended_flow_container(reference, None),
            shared_flow_start: self.shared_flows.len(),
            element_dedupe_mark: 0,
            depth: 0,
        };
        let answer = self.get_type_at_flow_node(&mut state, flow);
        let result = self.finalize_evolving_array(&mut state, answer).t;
        self.shared_flows.truncate(state.shared_flow_start);
        self.flow_result_or_declared(reference, result, any)
    }

    /// `getUnionOrEvolvingArrayType` (`flow.go:1314`), the junction rule:
    ///
    /// ```go
    /// // At flow control branch or loop junctions, if the type along every antecedent code path
    /// // is an evolving array type, we construct a combined evolving array type. Otherwise we
    /// // finalize all evolving array types.
    /// if isEvolvingArrayTypeList(types) {
    ///     return c.getEvolvingArrayType(c.getUnionType(core.Map(types, c.getElementTypeOfEvolvingArrayType)))
    /// }
    /// result := ... core.SameMap(types, c.finalizeEvolvingArrayType) ...
    /// ```
    ///
    /// `None` means *"this junction is not on the array track"* and the caller
    /// takes its ordinary union.
    ///
    /// # Why both halves are needed, with the two cases that force them
    ///
    /// This port spells an unfinalized evolving array as `state.declared_type`,
    /// so *"is an evolving array"* reads as `t == state.declared_type`.
    ///
    /// ```ts
    /// function f4() {                         // EVERY path evolving
    ///     let x = [];
    ///     if (cond()) { x.push(5); } else { x.push("hello"); }
    ///     return x;                           // (string | number)[]
    /// }
    /// function f6() {                         // ONE path evolving, one not
    ///     let x;
    ///     if (cond()) { x = 5; } else { x = []; x.push("hello"); }
    ///     return x;                           // number | string[]
    /// }
    /// ```
    ///
    /// **`f4` needs the junction to stay evolving** so the single finalisation
    /// at the query's end sees both elements; finalising per-branch gave
    /// `number[] | (string | number)[]`. **`f6` needs the junction to finalise
    /// the evolving branch on the spot**, because the other branch is `number`
    /// and a union with the unfinalized `any` stand-in collapses to `any` —
    /// which then finalises whole and answers `string[]`, losing the `number`
    /// arm. §737 measured both failures, one from each rule applied alone.
    fn union_or_evolving_array(
        &mut self,
        state: &mut FlowState,
        types: &[TypeId],
    ) -> Option<TypeId> {
        if types.is_empty() {
            return None;
        }
        // `isEvolvingArrayTypeList`: every non-`never` constituent is evolving,
        // and at least one is. `never` constituents are skipped by the caller
        // already, so "at least one" is simply a non-empty list.
        if types.iter().all(|&t| t == state.declared_type) {
            return Some(state.declared_type);
        }
        if !types.contains(&state.declared_type) {
            return None;
        }
        // `core.SameMap(types, c.finalizeEvolvingArrayType)` — finalise the
        // evolving constituents, leave the rest, then union.
        let finalized = self.finalized_array_type(state)?;
        let mapped: Vec<TypeId> =
            types.iter().map(|&t| if t == state.declared_type { finalized } else { t }).collect();
        Some(self.get_union_type(&mapped))
    }

    /// `finalizeEvolvingArrayType`'s array half: `Array<union of the elements
    /// collected so far>`, or `None` when the port cannot build it.
    fn finalized_array_type(&mut self, state: &FlowState) -> Option<TypeId> {
        let array = self.global_type_symbol("Array")?;
        let elements = state.array_elements.clone();
        // `createFinalArrayType` answers `autoArrayType` for a `never` element
        // type (`flow.go:1582`), which is the no-elements-yet case here.
        let element =
            if elements.is_empty() { self.intrinsics.any } else { self.get_union_type(&elements) };
        Some(self.create_type_reference(array, vec![element]))
    }

    /// `finalizeEvolvingArrayType` and its operation-target twin
    /// (`flow.go:105-109`), applied **once per reference query**.
    ///
    /// # "Once" is the load-bearing word
    ///
    /// Upstream calls this in `getFlowTypeOfReference` — the OUTER function:
    ///
    /// ```go
    /// var resultType *Type
    /// if evolvedType.objectFlags&ObjectFlagsEvolvingArray != 0 && c.isEvolvingArrayOperationTarget(reference) {
    ///     resultType = c.autoArrayType
    /// } else {
    ///     resultType = c.finalizeEvolvingArrayType(evolvedType)
    /// }
    /// ```
    ///
    /// §710 put it at the tail of [`Checker::get_type_at_flow_node`], which is
    /// **recursive** — so every nested call finalised too, each against whatever
    /// `state.array_elements` happened to hold at that moment. At a branch label
    /// the two antecedent calls finalised at *different* accumulator states and
    /// the join unioned the results:
    ///
    /// ```text
    /// f4:  want (string | number)[]   got  number[] | (string | number)[]
    /// ```
    ///
    /// That is the `X[] | Y[]` shape §736 recorded as "branch-merge residue" and
    /// could not explain. It was never a merge problem: it is one finalisation
    /// per recursion level where upstream has one per query. §737.
    fn finalize_evolving_array(&mut self, state: &mut FlowState, answer: FlowType) -> FlowType {
        if !state.is_auto_array || answer.t != state.declared_type {
            return answer;
        }
        let Some(array) = self.global_type_symbol("Array") else { return answer };
        if self.is_evolving_array_operation_target(state.reference) {
            // The UNFINALIZED spelling: `any[]` outright, so `x.push(…)`
            // resolves and *"operations on empty arrays are possible without
            // implicit any errors"* (upstream's own comment, `flow.go:101`).
            //
            // §736: **§710 got this for free and that hid the rule.** In the
            // DECLARATION half `state.declared_type` already IS `any[]`
            // (`autoArrayType`, `symbols.rs:4270`), so answering the declared
            // type here was `any[]` by coincidence. In the ASSIGNMENT half it is
            // `any`, and the identical code answered `x : any` — 9 RIGHT→WRONG,
            // every one a `want=number got=any` at an `x.push(…)` whose receiver
            // had stopped being an array.
            let auto_array = self.create_type_reference(array, vec![self.intrinsics.any]);
            return FlowType { t: auto_array, incomplete: answer.incomplete };
        }
        if state.array_elements.is_empty() {
            return answer;
        }
        let elements = state.array_elements.clone();
        let element = self.get_union_type(&elements);
        let evolved = self.create_type_reference(array, vec![element]);
        FlowType { t: evolved, incomplete: answer.incomplete }
    }

    /// One step of the backwards walk (`getTypeAtFlowNode`, `flow.go:117`).
    ///
    /// A loop rather than tail recursion, as upstream is: the arms that cannot
    /// answer move to the antecedent and go round again, which is what keeps a
    /// long straight-line function from consuming 2,000 frames of depth for
    /// nodes that say nothing.
    fn get_type_at_flow_node(&mut self, state: &mut FlowState, from: FlowId) -> FlowType {
        if state.depth >= MAX_FLOW_DEPTH {
            // Upstream disables flow analysis for the rest of the containing
            // function or module body and reports TS2563 (`flow.go:118`); the
            // diagnostic is not ported (`bd tsr-4sc`), the state change and
            // the answer are. The answer is upstream's `errorType` — printed
            // `any` there, so the `any` intrinsic IS the observable
            // (`checker-notes-narrow.md` §14).
            if let Some(container) = self.function_or_source_file_ancestor(state.reference) {
                self.flow_disabled_containers.insert(container);
            }
            // §14.1: the JS half prints `error` verbatim.
            let t = if self.in_js_file(state.reference) {
                self.intrinsics.error
            } else {
                self.intrinsics.any
            };
            return FlowType { t, incomplete: false };
        }
        state.depth += 1;

        // A `&'a` field, so copying it out ends the borrow of `self` and leaves
        // `&mut self` available for the narrowing calls below.
        let binder = self.binder;
        let mut flow = from;
        let mut shared: Option<FlowId> = None;
        let answer = loop {
            let flags = binder.flow().flags(flow);

            if flags.contains(FlowFlags::SHARED) {
                if let Some(cached) = self.shared_flows[state.shared_flow_start..]
                    .iter()
                    .find(|(id, _)| *id == flow)
                    .map(|(_, t)| *t)
                {
                    state.depth -= 1;
                    return cached;
                }
                shared = Some(flow);
            }

            if flags.contains(FlowFlags::ASSIGNMENT) {
                if let Some(t) = self.get_type_at_flow_assignment(state, flow) {
                    break t;
                }
            } else if flags.intersects(FlowFlags::CONDITION) {
                break self.get_type_at_flow_condition(state, flow);
            } else if flags.contains(FlowFlags::LOOP_LABEL) {
                // Native 5b1047d getTypeAtFlowNode (flow.go:169-174) uses the same
                // loop worker in JS. Keep its existing captured initial-type
                // key, active accumulator and completed-cache publication;
                // falling through would erase an assigned error into any.
                break self.get_type_at_flow_loop_label(state, flow);
            } else if flags.contains(FlowFlags::BRANCH_LABEL) {
                let antecedents =
                    branch_label_antecedents(binder.flow(), flow, &state.reduce_labels);
                let mut probe = antecedents.clone();
                let Some(first) = probe.next() else {
                    // A label with no antecedents is unreachable.
                    break FlowType { t: self.declared_or_never(state), incomplete: false };
                };
                if probe.next().is_none() {
                    flow = first;
                    continue;
                }
                break self.get_type_at_flow_branch_label(state, antecedents);
            } else if flags.contains(FlowFlags::REDUCE_LABEL) {
                // `getTypeAtFlowNode` (flow.go:181) scopes the replacement to
                // this recursive walk, restoring it before a sibling path.
                let Some(reduce) = binder.flow().reduce_label(flow) else {
                    break FlowType { t: state.declared_type, incomplete: false };
                };
                let Some(antecedent) = binder.flow().antecedent(flow) else {
                    break FlowType { t: state.declared_type, incomplete: false };
                };
                state.reduce_labels.push(reduce);
                let result = self.get_type_at_flow_node(state, antecedent);
                state.reduce_labels.pop();
                break result;
            } else if flags.contains(FlowFlags::START) {
                // Every container has its own START (`binder.rs:903`), so the
                // walk stops at the function boundary by construction —
                // upstream's `flowContainer` bound, already in the graph. The
                // ANSWER at the stop splits (`checker-notes-narrow.md` §9.7):
                // a reference whose declaration lives in THIS container takes
                // the substituted initial; an OUTER reference takes the
                // declared type, exactly as upstream's bounded walk returns
                // `t` — and an auto declaration's declared type here already
                // IS `anyType`, which is `convertAutoToAny` with no code.
                // …and only when the variable is EVER assigned: upstream's
                // `assumeInitialized = isOuterVariable && !isNeverInitialized`
                // — a never-assigned outer `let x;` keeps `undefined`
                // (`nestedBlockScopedBindings*`, the §9.7 bar's first
                // measurement: 24 such losses), while an assigned-elsewhere
                // capture takes the declared auto → `any`
                // (`capturedLetConstInLoop*`).
                // §13: a closure START whose container is not the walk's
                // bound continues from the closure's creation site in the
                // enclosing container (`flow.go:187`). The container payload
                // exists only for function expressions, arrows, and
                // object/class-expression methods (`binder.rs:903`), and the
                // bound was extended in `extended_flow_container` only for
                // constants and past-last-assignment mutables — everything
                // else stops here exactly as before.
                // flow.go's START arm: access expressions never follow a
                // creation-site edge; runtime this follows arrows only.
                // Query-this is an identifier, as in the native AST, and its
                // receiver was already selected by checkThisExpression.
                if let Some(container) = binder.flow().node(flow)
                    && state.flow_container != Some(container)
                    && !matches!(
                        self.nodes.kind(state.reference),
                        SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
                    )
                    && !(self.nodes.kind(state.reference) == SyntaxKind::ThisKeyword
                        && self.nodes.kind(container) != SyntaxKind::ArrowFunction)
                    && let Some(outer) = binder.flow_of(container)
                {
                    flow = outer;
                    continue;
                }
                if state.outer_reference
                    && (state.symbol.is_some_and(|s| {
                        // `isNeverInitialized` asks for a DEFINITE assignment
                        // (`checker.go:11147`): an outer `let i: number`
                        // touched only by `i++` keeps `undefined`.
                        self.symbol_has_any_assignment(s) && !self.is_never_initialized(s)
                    })
                        // `isNeverInitialized` requires a mutable LOCAL
                        // (`checker.go:11147`, `isMutableLocalVariableDeclaration`):
                        // a file-level declaration referenced inside a
                        // function is assumed initialized whatever its
                        // initializer says — the jsxEsprima half of the §9.7
                        // population, 48 lines the first refinement dropped.
                        || state.symbol.is_some_and(|s| self.declaration_is_file_level(s)))
                {
                    break FlowType { t: state.declared_type, incomplete: false };
                }
                break FlowType { t: state.initial_type, incomplete: false };
            } else if flags.contains(FlowFlags::SWITCH_CLAUSE) {
                break self.get_type_at_switch_clause(state, flow);
            } else if flags.contains(FlowFlags::CALL) {
                // §127 (`checker-notes-narrow.md`): `getTypeAtFlowCall` — an
                // assertion signature (`asserts x is T` / bare `asserts x`)
                // narrows the matching reference in the statement flow that
                // follows. `None` = no assertion effect; skip to the
                // antecedent exactly as before. (The old comment here said
                // this "needs call resolution this checker lacks" — stale
                // since callres landed; the condition road below has resolved
                // signatures for `is` predicates since §100.)
                if let Some(narrowed) = self.get_type_at_flow_call(state, flow) {
                    break narrowed;
                }
                match binder.flow().antecedent(flow) {
                    Some(next) => {
                        flow = next;
                        continue;
                    }
                    None => break FlowType { t: state.declared_type, incomplete: false },
                }
            } else if flags.contains(FlowFlags::UNREACHABLE) {
                // Upstream's default arm: unreachable-code errors belong to the
                // binder, and the checker returns the declared type to avoid
                // follow-on noise. `convertAutoToAny` (`checker.go:11186`) is
                // upstream's follow-up here and needs no code: it maps
                // `autoType` to `anyType`, and an automatic declaration's
                // `declared_type` in this port already *is* `anyType` — the two
                // are one intrinsic here rather than two.
                break FlowType { t: state.declared_type, incomplete: false };
            } else {
                // §14: an ARRAY_MUTATION node against an auto-array
                // reference is a RECURSION upstream (`flow.go:1404` calls
                // `getTypeAtFlowNode` per mutation), and the depth cap is
                // what makes `largeControlFlowGraph` bail deliberately. This
                // walk skips the node iteratively, so the recursion is
                // accounted for explicitly — one depth step per mutation.
                if flags.contains(FlowFlags::ARRAY_MUTATION) && state.is_auto_array {
                    // §710: `addEvolvingArrayElementType` — `x.push(a)`
                    // contributes each argument, `x[i] = e` the right-hand side.
                    if let Some(mutation) = binder.flow().node(flow) {
                        let contributed: Vec<TypeId> = match self.node_map.get(mutation) {
                            Some(Node::CallExpression(call)) => {
                                call.arguments.iter().map(|&a| self.check_expression(a)).collect()
                            }
                            Some(Node::BinaryExpression(binary)) => binary
                                .right
                                .map(|right| vec![self.check_expression(right)])
                                .unwrap_or_default(),
                            _ => Vec::new(),
                        };
                        for element in contributed {
                            let widened = self.get_base_type_of_literal_type(element);
                            // §739: dedupe against the innermost loop-label
                            // region only — see `element_dedupe_mark`. An
                            // element already seen DOWNSTREAM of the label must
                            // still enter the label's own slice, or the cached
                            // slice is incomplete for every later query.
                            if widened != self.intrinsics.error
                                && !state.array_elements[state.element_dedupe_mark..]
                                    .contains(&widened)
                            {
                                state.array_elements.push(widened);
                            }
                        }
                    }
                    state.depth += 1;
                    if state.depth >= MAX_FLOW_DEPTH {
                        if let Some(container) =
                            self.function_or_source_file_ancestor(state.reference)
                        {
                            self.flow_disabled_containers.insert(container);
                        }
                        break FlowType { t: self.intrinsics.any, incomplete: false };
                    }
                }
                // An unrelated array mutation contributes no type change;
                // continue along its antecedent after the evolving-array work.
                match binder.flow().antecedent(flow) {
                    Some(next) => {
                        flow = next;
                        continue;
                    }
                    None => break FlowType { t: state.declared_type, incomplete: false },
                }
            }

            // Only reached when the assignment arm declined to answer.
            match binder.flow().antecedent(flow) {
                Some(next) => flow = next,
                None => break FlowType { t: state.declared_type, incomplete: false },
            }
        };

        if let Some(id) = shared {
            self.shared_flows.push((id, answer));
        }
        state.depth -= 1;
        answer
    }

    /// The type after an assignment (`getTypeAtFlowAssignment`, `flow.go:216`).
    ///
    /// `None` means "this assignment says nothing about our reference", which is
    /// upstream's nil return and sends the walk to the antecedent.
    ///
    /// # The two reductions
    ///
    /// Upstream's arm has two: the **automatic** one, which replaces the
    /// declared `any` with what was assigned (this is `any` *evolution*), and
    /// the **union** one, which keeps the declared constituents the assigned
    /// type could be — `getAssignmentReducedType` (`flow.go:2399`). Both are
    /// ported here.
    ///
    /// # What is not
    ///
    /// - **Compound assignment** (`x += 1`), which takes the base type of the
    ///   antecedent's literal type. Unported, so it leaves the declared type.
    /// - **`autoArrayType` and the evolving-array machinery** —
    ///   `ObjectFlagsEvolvingArray`, `getEvolvingArrayType`,
    ///   `addEvolvingArrayElementType`, `finalizeEvolvingArrayType`. An evolving
    ///   array is a *second* mechanism layered on this one: `let x = [];` gets
    ///   `autoArrayType`, and every `x.push(e)` widens the element type through
    ///   an `ARRAY_MUTATION` flow node, which this port's walk skips. Porting the
    ///   scalar half without it is safe because the two are selected by the
    ///   declaration: [`Checker::is_auto_typed_declaration`] answers `false` for
    ///   `let x = [];` (it has an initialiser), so such a variable keeps today's
    ///   answer rather than getting a half-evolved one.
    fn get_type_at_flow_assignment(
        &mut self,
        state: &mut FlowState,
        flow: FlowId,
    ) -> Option<FlowType> {
        let node = self.binder.flow().node(flow)?;
        if !self.is_matching_reference(state, node) {
            // `flow.go:267`: `for (const _ in ref)` acts as a non-null on
            // `ref`. §747; the `optionalChainContainsReference` half §748.
            // Upstream answers `FlowType{t: …}` — the antecedent's
            // `incomplete` is NOT carried (§748 corrected §747's carrying
            // it).
            if self.nodes.kind(node) == SyntaxKind::VariableDeclaration
                && let Some(list) = self.nodes.parent(node)
                && let Some(statement) = self.nodes.parent(list)
                && self.nodes.kind(statement) == SyntaxKind::ForInStatement
                && let Some(Node::ForInOrOfStatement(for_in)) = self.node_map.get(statement)
                && let Some(expression) = for_in.expression.and_then(|e| e.node_id())
                && (self.is_matching_reference(state, expression)
                    || self.optional_chain_contains_reference(state, expression))
            {
                let antecedent = self.binder.flow().antecedent(flow)?;
                let prior = self.get_type_at_flow_node(state, antecedent);
                let t = if self
                    .get_type_facts(prior.t)
                    .intersects(TypeFacts::IS_UNDEFINED | TypeFacts::IS_NULL)
                {
                    self.get_non_nullable_type(prior.t)
                } else {
                    prior.t
                };
                return Some(FlowType { t, incomplete: false });
            }
            // `flow.go:255`: the assignment may be to a **left-hand part** of
            // the reference — for `x.y.z` we may be at an assignment to `x.y`
            // or to `x` — and any such assignment invalidates everything
            // narrowed about the whole reference, so the declared type is the
            // answer.
            //
            // **This arm was missing from the first run of `bd tsr-6ka` and the
            // corpus named it**: `conformance/destructuringControlFlow` writes
            // `if (obj.a) { obj = {}; obj.a }` and records `string | undefined`
            // for the inner `obj.a`, where this port answered the narrowed
            // `string`. Without it, property narrowing survives an assignment
            // that replaces the object it was narrowed on — the over-narrowing
            // direction, which produces a confident wrong line.
            //
            // `flow.go:256`: an UNREACHABLE assignment answers the sentinel
            // (§747) — at a read the exit converts it to the declared type,
            // which is what this arm answered before; at a JOIN it drops
            // out, which the declared type did not (§744's lesson).
            if self.contains_matching_reference(state, node) {
                if !self.is_reachable_flow_node(flow) {
                    return Some(FlowType {
                        t: self.intrinsics.unreachable_never,
                        incomplete: false,
                    });
                }
                return Some(FlowType { t: state.declared_type, incomplete: false });
            }
            return None;
        }
        // `flow.go:226` (§747), the same sentinel on the direct match.
        if !self.is_reachable_flow_node(flow) {
            return Some(FlowType { t: self.intrinsics.unreachable_never, incomplete: false });
        }
        // `flow.go:229`: a COMPOUND assignment does not narrow — the walk
        // skips its effect and answers the antecedent's type at the
        // literal's base. `a += anyExpr` leaves a narrowed `number` intact;
        // computing the `+=` result here was §15's 968 confident wrongs
        // (`binaryArithmeticControlFlowGraphNotTooLarge`).
        if self.assignment_target_kind(node) == crate::expressions::AssignmentTargetKind::Compound {
            let antecedent = self.binder.flow().antecedent(flow)?;
            let prior = self.get_type_at_flow_node(state, antecedent);
            return Some(FlowType {
                t: self.get_base_type_of_literal_type(prior.t),
                incomplete: prior.incomplete,
            });
        }
        if state.is_auto {
            // §736 — `flow.go:233`, the line §712 named as the whole remaining
            // entry point:
            //
            // ```go
            // if c.isEmptyArrayAssignment(node) {
            //     return FlowType{t: c.getEvolvingArrayType(c.neverType)}
            // }
            // ```
            //
            // An empty-array assignment to an auto variable does NOT answer
            // `never[]` — it answers the EVOLVING array, which the mutations
            // already met on the way here have been extending. This port has no
            // evolving-array type object (ADR-0003 keeps the accumulation in
            // `state.array_elements` instead), so the unfinalized evolving array
            // is spelled as the declared type, which is precisely the value
            // §710's finalisation gate at the end of the walk tests for.
            //
            // **Returning the assigned type here is what made §713 measure
            // zero.** With `never[]` coming back, `answer.t == state.declared_type`
            // never held and the accumulated elements were discarded however
            // many `x.push(…)` had contributed.
            if state.is_auto_array && self.is_empty_array_assignment(node) {
                return Some(FlowType { t: state.declared_type, incomplete: false });
            }
            // `flow.go:232`. Upstream then asks whether the assigned type is
            // assignable to the declared one and falls back to `any[]`; the
            // declared type here is `any`, to which everything is assignable, so
            // the fallback is unreachable and is not written out.
            let Some(assigned) = self.get_initial_or_assigned_type(node) else {
                // An assignment form whose right-hand side this port cannot
                // reach — a `for..of` binding, a destructuring target. Upstream
                // computes *something*; `any` is what this checker says today,
                // and it is the one answer that cannot be more wrong than the
                // status quo. Returning `None` would be worse: the walk would
                // continue past a real assignment to the `undefined` at the top
                // of the graph.
                return Some(FlowType { t: state.declared_type, incomplete: false });
            };
            return Some(FlowType {
                t: self.get_widened_literal_type(assigned),
                incomplete: false,
            });
        }
        if self.store.get(state.declared_type).flags.contains(TypeFlags::UNION) {
            let declared = state.declared_type;
            if let Some(assigned) = self.get_initial_or_assigned_type(node) {
                return Some(FlowType {
                    t: self.get_assignment_reduced_type(declared, assigned),
                    incomplete: false,
                });
            }
        }
        Some(FlowType { t: state.declared_type, incomplete: false })
    }

    /// Whether ANY assignment in the declaration's control-flow container
    /// targets this symbol — upstream's `isNeverInitialized` complement,
    /// approximated syntactically and memoized per symbol
    /// (`checker-notes-narrow.md` §9.7): assignment operators and `++`/`--`
    /// whose target identifier resolves back to the symbol.
    fn symbol_has_any_assignment(&mut self, symbol: SymbolId) -> bool {
        if let Some(&cached) = self.symbol_assignment_scan.get(&symbol) {
            return cached;
        }
        let answer = (|| {
            let declaration = self.binder.symbols().get(symbol).value_declaration?;
            let name = self.binder.symbols().get(symbol).name;
            // The scan root: the declaration's control-flow container body.
            let mut root = declaration;
            while let Some(parent) = self.nodes.parent(root) {
                root = parent;
                if matches!(
                    self.nodes.kind(root),
                    tsr_ast::SyntaxKind::FunctionDeclaration
                        | tsr_ast::SyntaxKind::FunctionExpression
                        | tsr_ast::SyntaxKind::ArrowFunction
                        | tsr_ast::SyntaxKind::MethodDeclaration
                        | tsr_ast::SyntaxKind::GetAccessor
                        | tsr_ast::SyntaxKind::SetAccessor
                        | tsr_ast::SyntaxKind::Constructor
                        | tsr_ast::SyntaxKind::SourceFile
                ) {
                    break;
                }
            }
            let root_node = self.node_map.get(root)?;
            let mut stack = vec![root_node];
            let mut children = Vec::new();
            while let Some(node) = stack.pop() {
                let target = match node {
                    Node::BinaryExpression(binary)
                        if binary.operator_token.is_some_and(|t| {
                            tsr_ast::SyntaxKind::EqualsToken == t.kind
                                || matches!(
                                    t.kind,
                                    tsr_ast::SyntaxKind::PlusEqualsToken
                                        | tsr_ast::SyntaxKind::MinusEqualsToken
                                        | tsr_ast::SyntaxKind::AsteriskEqualsToken
                                        | tsr_ast::SyntaxKind::SlashEqualsToken
                                        | tsr_ast::SyntaxKind::PercentEqualsToken
                                        | tsr_ast::SyntaxKind::BarEqualsToken
                                        | tsr_ast::SyntaxKind::AmpersandEqualsToken
                                        | tsr_ast::SyntaxKind::CaretEqualsToken
                                        | tsr_ast::SyntaxKind::BarBarEqualsToken
                                        | tsr_ast::SyntaxKind::AmpersandAmpersandEqualsToken
                                        | tsr_ast::SyntaxKind::QuestionQuestionEqualsToken
                                )
                        }) =>
                    {
                        binary.left
                    }
                    Node::PrefixUnaryExpression(unary)
                        if matches!(
                            unary.operator.kind,
                            tsr_ast::SyntaxKind::PlusPlusToken
                                | tsr_ast::SyntaxKind::MinusMinusToken
                        ) =>
                    {
                        unary.operand
                    }
                    Node::PostfixUnaryExpression(unary) => unary.operand,
                    _ => None,
                };
                if let Some(tsr_ast::Expression::Identifier(identifier)) = target
                    && identifier.text == name
                    && let Some(id) = identifier.node_id
                    && self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        id,
                        name,
                        SymbolFlags::VALUE,
                    ) == Some(symbol)
                {
                    return Some(true);
                }
                children.clear();
                tsr_ast::push_children(node, &mut children);
                stack.extend(children.iter().copied());
            }
            Some(false)
        })()
        .unwrap_or(false);
        self.symbol_assignment_scan.insert(symbol, answer);
        answer
    }
    /// The walk's container bound, extended outward for constants and
    /// past-last-assignment mutables — `checkIdentifier`'s loop,
    /// `checker.go:11139`, and the §13 build. The default bound is the
    /// reference's own control-flow container, which stops the walk exactly
    /// where the pre-§13 START arm stopped it.
    fn extended_flow_container(
        &mut self,
        reference: NodeId,
        symbol: Option<SymbolId>,
    ) -> Option<NodeId> {
        let mut container = self.control_flow_container(reference)?;
        let Some(symbol) = symbol else { return Some(container) };
        let declaration_container = self
            .binder
            .symbols()
            .get(symbol)
            .value_declaration
            .and_then(|declaration| self.control_flow_container(declaration));
        while Some(container) != declaration_container
            && matches!(
                self.nodes.kind(container),
                // SS172: upstream ascends through `IsFunctionLike`
                // (ast/utilities.go:518), which includes FUNCTION
                // DECLARATIONS; this enumeration omitted them while the
                // enumeration at the top of this module for the same family
                // lists them - two spellings of one concept disagreeing,
                // which is the SS169 shape and a self-inconsistency
                // regardless of whether the corpus fires it.
                tsr_ast::SyntaxKind::FunctionDeclaration
                    | tsr_ast::SyntaxKind::FunctionExpression
                    | tsr_ast::SyntaxKind::ArrowFunction
                    | tsr_ast::SyntaxKind::MethodDeclaration
                    | tsr_ast::SyntaxKind::Constructor
                    | tsr_ast::SyntaxKind::GetAccessor
                    | tsr_ast::SyntaxKind::SetAccessor
            )
            && (self.is_constant_variable(symbol)
                || self.is_parameter_or_mutable_local_variable(symbol)
                    && self.is_past_last_assignment(symbol, reference))
        {
            container = self.control_flow_container(container)?;
        }
        Some(container)
    }

    /// `isConstantVariable` (`utilities.go:1040`): a variable whose
    /// declaration list carries `const`.
    /// `isReadonlySymbol` (`checker.go:13849`), the four decidable arms:
    /// const variable, enum member, readonly-modifier property, get-only
    /// accessor and Object.defineProperty descriptor. `CheckFlagsReadonly`
    /// remains represented separately by mapped/synthetic property contexts.
    pub(crate) fn is_readonly_symbol(&mut self, symbol: SymbolId) -> bool {
        let record = self.binder.symbols().get(symbol);
        let flags = record.flags;
        if flags.intersects(SymbolFlags::ENUM_MEMBER) {
            return true;
        }
        if flags.intersects(SymbolFlags::GET_ACCESSOR)
            && !flags.intersects(SymbolFlags::SET_ACCESSOR)
        {
            return true;
        }
        if flags.intersects(SymbolFlags::VARIABLE) && self.is_constant_variable(symbol) {
            return true;
        }
        if flags.intersects(SymbolFlags::PROPERTY)
            && let Some(declaration) = record.value_declaration
            && let Some(Node::PropertyDeclaration(property)) = self.node_map.get(declaration)
            && property.modifiers.iter().any(|modifier| {
                tsr_ast::Node::from(*modifier)
                    .node_id()
                    .is_some_and(|id| self.nodes.kind(id) == tsr_ast::SyntaxKind::ReadonlyKeyword)
            })
        {
            return true;
        }
        self.assignment_declaration_is_readonly(symbol)
    }

    /// `isConstantReference` (`checker.go`): the reference an ALIASED
    /// condition may narrow. An identifier must be a `const` variable or a
    /// parameter/local with NO recorded assignment (`obj` reassigned in the
    /// body killed f15's narrowing upstream and must here — the §82 gate's
    /// firing falsifier); a property access needs a READONLY property on a
    /// constant receiver.
    /// §83: whether `derived`'s `extends` chain (identifier heritage
    /// expressions only, depth-capped) contains `base`.
    /// SS159 (Phase 1 slice 1 of the checkDerived worker,
    /// checker-notes-callres2.md SS158): is `t` DERIVED FROM `candidate` -
    /// identity, class extends chains, or declared interface heritage
    /// (SS146's walk). `Some(false)` is only answered where derivation is
    /// REFUTABLE: primitives, nullish, and literals cannot derive from an
    /// object candidate; two class/interface owners with followable chains
    /// that do not meet. Everything else is `None` (undecidable) and the
    /// caller declines to its old road.
    fn is_derived_from_decidable(&mut self, t: TypeId, candidate: TypeId) -> Option<bool> {
        if t == candidate {
            return Some(true);
        }
        let flags = self.store.get(t).flags;
        if flags.intersects(
            TypeFlags::NULLABLE
                | TypeFlags::UNIT
                | TypeFlags::STRING
                | TypeFlags::NUMBER
                | TypeFlags::BOOLEAN
                | TypeFlags::BIG_INT
                | TypeFlags::ES_SYMBOL
                | TypeFlags::VOID
                | TypeFlags::NEVER,
        ) {
            return Some(false);
        }
        // SS163 (isTypeDerivedFrom's structural arms, relater.go:4964-4975,
        // transcribed): source union -> EVERY; target union -> SOME; source
        // intersection -> SOME - each Kleene-lifted (decidable-true /
        // decidable-false / else None). Source constraints follow below.
        if let TypeData::Union { types, .. } = &self.store.get(t).data {
            let constituents = types.clone();
            for constituent in constituents {
                match self.is_derived_from_decidable(constituent, candidate) {
                    Some(true) => {}
                    Some(false) => return Some(false),
                    None => return None,
                }
            }
            return Some(true);
        }
        if let TypeData::Union { types, .. } = &self.store.get(candidate).data {
            let constituents = types.clone();
            let mut any_none = false;
            for constituent in constituents {
                match self.is_derived_from_decidable(t, constituent) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => any_none = true,
                }
            }
            return if any_none { None } else { Some(false) };
        }
        if let TypeData::Intersection { types, .. } = &self.store.get(t).data {
            let constituents = types.clone();
            let mut any_none = false;
            for constituent in constituents {
                match self.is_derived_from_decidable(constituent, candidate) {
                    Some(true) => return Some(true),
                    Some(false) => {}
                    None => any_none = true,
                }
            }
            return if any_none { None } else { Some(false) };
        }
        // isTypeDerivedFrom follows a source's base constraint, but a type
        // parameter target is not its constraint. In particular Derived does
        // not derive from the polymorphic this parameter constrained to Base.
        if flags.intersects(TypeFlags::INSTANTIABLE_NON_PRIMITIVE) {
            let constraint = self.base_constraint_of_type(t).unwrap_or(self.intrinsics.unknown);
            if constraint == t {
                return None;
            }
            return self.is_derived_from_decidable(constraint, candidate);
        }
        if self.store.get(candidate).flags.contains(TypeFlags::TYPE_PARAMETER)
            || flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
        {
            return Some(false);
        }
        // isTypeDerivedFrom's empty-object and global Object arms (relater.go:4982).
        if self.is_empty_anonymous_object_type(candidate) {
            return Some(flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE));
        }
        if let TypeData::Named { members: Some(owner), .. } = self.store.get(candidate).data
            && self.global_type_symbol_with_arity("Object", 0) == Some(owner)
        {
            return Some(
                flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE)
                    && !self.is_empty_anonymous_object_type(t),
            );
        }
        // hasBaseType cannot derive an object from a primitive target, nor
        // can the primitive `object` type have a declared base chain. This
        // also decides the reverse comparison when a predicate removes a
        // primitive constituent from an object union.
        if !self.store.get(candidate).flags.contains(TypeFlags::OBJECT)
            || flags.contains(TypeFlags::NON_PRIMITIVE)
        {
            return Some(false);
        }
        let owner_of = |checker: &Self, id: TypeId| -> Option<SymbolId> {
            match checker.store.get(id).data {
                TypeData::Named { members: Some(owner), .. } => Some(owner),
                // SS162v2: a REFERENCE derives through its target symbol
                // (isTypeDerivedFrom over the target's declared base chain).
                _ => checker
                    .type_reference_targets
                    .get(&id)
                    .map(|(target, _)| checker.binder.merged_symbol(*target)),
            }
        };
        let (Some(t_owner), Some(c_owner)) = (owner_of(self, t), owner_of(self, candidate)) else {
            return None;
        };
        if t_owner == c_owner {
            return Some(true);
        }
        if self.class_extends_chain_contains(t_owner, c_owner) {
            return Some(true);
        }
        let mut visiting = Vec::new();
        if self.heritage_chain_contains(t_owner, c_owner, &mut visiting) {
            return Some(true);
        }
        // Refutable only when BOTH chains are followable to their ends.
        let mut visiting_t = Vec::new();
        let mut visiting_c = Vec::new();
        if self.walk_completes_for_narrowing(t_owner, &mut visiting_t)
            && self.walk_completes_for_narrowing(c_owner, &mut visiting_c)
        {
            return Some(false);
        }
        None
    }

    /// SS159: every base link from `owner` resolves (the refutability
    /// condition for a negative derivation answer).
    fn walk_completes_for_narrowing(
        &mut self,
        owner: SymbolId,
        visiting: &mut Vec<SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return false;
        }
        visiting.push(owner);
        let has_heritage = self
            .binder
            .symbols()
            .get(owner)
            .declarations
            .iter()
            .copied()
            .collect::<Vec<_>>()
            .into_iter()
            .any(|d| match self.node_map.get(d) {
                Some(Node::ClassDeclaration(node)) => !node.heritage_clauses.is_empty(),
                Some(Node::InterfaceDeclaration(node)) => !node.heritage_clauses.is_empty(),
                _ => false,
            });
        match self.base_symbols_of(owner) {
            Some(bases) => {
                bases.into_iter().all(|base| self.walk_completes_for_narrowing(base, visiting))
            }
            None => !has_heritage,
        }
    }

    fn class_extends_chain_contains(&mut self, derived: SymbolId, base: SymbolId) -> bool {
        let mut current = derived;
        for _ in 0..16 {
            if current == base {
                return true;
            }
            let Some(declaration) =
                self.binder.symbols().get(current).declarations.first().copied()
            else {
                return false;
            };
            let heritage = match self.node_map.get(declaration) {
                Some(Node::ClassDeclaration(class)) => class.heritage_clauses,
                Some(Node::ClassExpression(class)) => class.heritage_clauses,
                _ => return false,
            };
            let mut next = None;
            for clause in heritage {
                if clause.token.kind != SyntaxKind::ExtendsKeyword {
                    continue;
                }
                for expression in clause.types {
                    if let Some(tsr_ast::Expression::Identifier(identifier)) = expression.expression
                        && let Some(id) = identifier.node_id
                    {
                        next = self.binder.resolve_name(
                            self.nodes,
                            self.node_map,
                            id,
                            identifier.text,
                            SymbolFlags::VALUE,
                        );
                    }
                }
            }
            match next {
                Some(symbol) => current = symbol,
                None => return false,
            }
        }
        false
    }

    /// §126's gate: whether an identifier reference resolves to a variable
    /// declared at a source file's top level. The instanceof FALSE branch
    /// does not narrow those in the baselines (typeGuardOfFormInstanceOf)
    /// while parameters and locals narrow
    /// (instanceofWithStructurallyIdenticalTypes); an unresolvable
    /// reference answers `true` — the arm then declines, the safe side.
    fn reference_is_top_level_var(&mut self, reference: NodeId) -> bool {
        let Some(Node::Identifier(identifier)) = self.node_map.get(reference) else {
            return true;
        };
        let Some(symbol) = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            reference,
            identifier.text,
            SymbolFlags::VALUE,
        ) else {
            return true;
        };
        let Some(&declaration) = self.binder.symbols().get(symbol).declarations.first() else {
            return true;
        };
        if self.nodes.kind(declaration) != SyntaxKind::VariableDeclaration {
            return false;
        }
        // VariableDeclaration → VariableDeclarationList → VariableStatement
        // → SourceFile, exactly — anything else (a function body, a block)
        // is a local.
        self.nodes
            .parent(declaration)
            .and_then(|list| self.nodes.parent(list))
            .and_then(|statement| self.nodes.parent(statement))
            .is_some_and(|container| self.nodes.kind(container) == SyntaxKind::SourceFile)
    }

    /// `ast.IsThisInTypeQuery`: only the leftmost identifier in the entity
    /// name is query `this`; a property named `this` is an ordinary identifier.
    pub(crate) fn is_this_in_type_query(&self, node: NodeId) -> bool {
        if !matches!(self.node_map.get(node), Some(Node::Identifier(name)) if name.text == "this") {
            return false;
        }
        let mut current = node;
        while let Some(parent) = self.nodes.parent(current) {
            if let Some(Node::QualifiedName(name)) = self.node_map.get(parent)
                && name.left.and_then(|left| left.node_id()) == Some(current)
            {
                current = parent;
            } else {
                return self.nodes.kind(parent) == SyntaxKind::TypeQuery;
            }
        }
        false
    }

    pub(crate) fn is_constant_reference(&mut self, reference: NodeId) -> bool {
        match self.node_map.get(reference) {
            Some(Node::Identifier(identifier)) => {
                if self.is_this_in_type_query(reference) {
                    return false;
                }
                let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    reference,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                if self.is_constant_variable(symbol) {
                    return true;
                }
                // A parameter or local counts when nothing assigns it —
                // `ensure_assignments_marked` walks the symbol's enclosing
                // function once, then the map answers.
                self.ensure_assignments_marked(symbol);
                if self.is_parameter_or_mutable_local_variable(symbol)
                    && !self.last_assignment_pos.contains_key(&symbol)
                {
                    return true;
                }
                // §239: upstream's **third** disjunct (`flow.go:1821`) —
                // `symbol.ValueDeclaration != nil &&
                //  ast.IsFunctionExpression(symbol.ValueDeclaration)`. A name
                // bound to a function expression is a constant reference even
                // when it is neither a const nor an unassigned local, because
                // the binding cannot be reassigned through that declaration.
                //
                // Found by a systematic sweep rather than by a failing case:
                // upstream's predicate has three disjuncts and this had two.
                self.binder.symbols().get(symbol).value_declaration.is_some_and(|declaration| {
                    self.nodes.kind(declaration) == SyntaxKind::FunctionExpression
                })
            }
            Some(Node::PropertyAccessExpression(access)) => {
                let Some(tsr_ast::MemberName::Identifier(_)) = access.name else {
                    return false;
                };
                let Some(receiver) = access.expression.and_then(|e| e.node_id()) else {
                    return false;
                };
                let receiver_type = match self.node_map.get(reference) {
                    Some(Node::PropertyAccessExpression(node)) => {
                        let Some(expression) = node.expression else { return false };
                        self.check_expression(expression)
                    }
                    _ => return false,
                };
                let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                    return false;
                };
                let readonly = self
                    .get_property_of_type(receiver_type, name.text)
                    .is_some_and(|property| self.is_readonly_symbol(property));
                readonly && self.is_constant_reference(receiver)
            }
            // §904: `case ast.KindElementAccessExpression` shares upstream's
            // property-access arm verbatim (`flow.go:1823`) — the two kinds are
            // one `case`. Reduced here to a literal key, which is the only
            // element access whose property symbol this port can resolve; a
            // computed key keeps the old `false`.
            Some(Node::ElementAccessExpression(access)) => {
                let Some(receiver) = access.expression.and_then(|e| e.node_id()) else {
                    return false;
                };
                let Some(tsr_ast::Expression::StringLiteral(key)) = access.argument_expression
                else {
                    return false;
                };
                let Some(expression) = access.expression else { return false };
                let receiver_type = self.check_expression(expression);
                let readonly = self
                    .get_property_of_type(receiver_type, key.text)
                    .is_some_and(|property| self.is_readonly_symbol(property));
                readonly && self.is_constant_reference(receiver)
            }
            _ => {
                // §904: `case ast.KindThisKeyword: return true` (`flow.go:1816`),
                // upstream's FIRST arm. `this` cannot be reassigned, so a
                // reference rooted at it is constant and narrowing survives
                // across intervening calls — `this.x` chains on it.
                //
                // Tested by KIND rather than by node shape because `this` is a
                // keyword expression here, not a node kind of its own.
                self.nodes.kind(reference) == SyntaxKind::ThisKeyword
            }
        }
    }

    pub(crate) fn is_constant_variable(&self, symbol: SymbolId) -> bool {
        // §756: `isConstantVariable` (`utilities.go:1040`) is
        // `symbol.Flags&Variable != 0 && getDeclarationNodeFlagsFromSymbol(symbol)&Constant != 0`,
        // and that flag lookup is `getCombinedNodeFlags` on the value
        // declaration (`checker.go:18681`). This port had walked exactly ONE
        // parent and demanded a `VariableDeclarationList`, which answers
        // `false` for a destructured `const { kind: x } = obj` — the binding
        // element's parent is the pattern. It also missed `using`
        // ([`NodeFlags::CONSTANT`] is `CONST | USING`, as upstream's is) and
        // did not check the symbol is a VARIABLE at all.
        let symbol_data = self.binder.symbols().get(symbol);
        if !symbol_data.flags.intersects(SymbolFlags::VARIABLE) {
            return false;
        }
        let Some(declaration) = symbol_data.value_declaration else {
            return false;
        };
        self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT)
    }

    /// `isMutableLocalVariableDeclaration` (`utilities.go:1053`), faithfully:
    /// a `let` declaration that is neither exported nor declared at the top
    /// level of a **global** source file.
    ///
    /// Deliberately *not* shared with
    /// [`Checker::is_parameter_or_mutable_local_variable`] above, which
    /// approximates `IsGlobalSourceFile` by refusing every file-level `let`.
    /// That approximation is the `.types` workstream's §13 reader and moving it
    /// moves `checker_types`; a top-level `let` in a **module** file is a
    /// mutable local upstream and this predicate says so.
    /// `checker-notes-diag2.md` §42 records the divergence.
    pub(crate) fn is_mutable_local_variable_declaration(&self, declaration: NodeId) -> bool {
        let Some(list) = self.nodes.parent(declaration) else { return false };
        if !self.nodes.flags(list).intersects(tsr_ast::NodeFlags::LET) {
            return false;
        }
        let Some(statement) = self.nodes.parent(list) else { return false };
        let Some(Node::VariableStatement(variable)) = self.node_map.get(statement) else {
            // Not a variable statement at all — a `for (let x …)` head, whose
            // parent is the loop. Upstream's global test only fires for the
            // `VariableStatement` shape, so this is a mutable local.
            return true;
        };
        // `GetCombinedModifierFlags & ModifierFlagsExport`.
        if variable.modifiers.iter().any(|modifier| {
            tsr_ast::Node::from(*modifier)
                .node_id()
                .is_some_and(|id| self.nodes.kind(id) == tsr_ast::SyntaxKind::ExportKeyword)
        }) {
            return false;
        }
        // `IsGlobalSourceFile`: a source file that is **not** an external or
        // CommonJS module. `bindSourceFileAsExternalModule` gives a module file
        // a symbol on its `SourceFile` node and gives a script none, which is
        // how `crate::unused` asks the same question.
        let Some(scope) = self.nodes.parent(statement) else { return true };
        !(self.nodes.kind(scope) == tsr_ast::SyntaxKind::SourceFile
            && self.binder.symbol_of(scope).is_none())
    }

    /// `isParameterOrMutableLocalVariable` (`utilities.go:1044`): a
    /// parameter, catch-clause variable, or `let` local. Upstream's
    /// exported/global exclusions are approximated by refusing file-level
    /// `let`s outright — conservative: the §13 extension stops earlier.
    fn is_parameter_or_mutable_local_variable(&self, symbol: SymbolId) -> bool {
        let Some(mut declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        // `GetRootDeclaration`: climb out of binding patterns.
        while let Some(parent) = self.nodes.parent(declaration) {
            if matches!(
                self.nodes.kind(parent),
                tsr_ast::SyntaxKind::BindingElement
                    | tsr_ast::SyntaxKind::ObjectBindingPattern
                    | tsr_ast::SyntaxKind::ArrayBindingPattern
                    | tsr_ast::SyntaxKind::Parameter
            ) {
                declaration = parent;
            } else {
                break;
            }
        }
        match self.nodes.kind(declaration) {
            tsr_ast::SyntaxKind::Parameter => true,
            tsr_ast::SyntaxKind::VariableDeclaration => {
                let Some(list) = self.nodes.parent(declaration) else { return false };
                if self.nodes.kind(list) == tsr_ast::SyntaxKind::CatchClause {
                    return true;
                }
                if !self.nodes.flags(list).intersects(tsr_ast::NodeFlags::LET) {
                    return false;
                }
                // Upstream's exclusions: exported (`export let x` in a
                // namespace stays wide — `narrowingPastLastAssignment`'s
                // namespace block, the §13 measurement's last line) and
                // global (file-level).
                let Some(statement) = self.nodes.parent(list) else { return false };
                if let Some(Node::VariableStatement(variable)) = self.node_map.get(statement)
                    && variable.modifiers.iter().any(|modifier| {
                        tsr_ast::Node::from(*modifier).node_id().is_some_and(|id| {
                            self.nodes.kind(id) == tsr_ast::SyntaxKind::ExportKeyword
                        })
                    })
                {
                    return false;
                }
                self.nodes
                    .parent(statement)
                    .is_some_and(|scope| self.nodes.kind(scope) != tsr_ast::SyntaxKind::SourceFile)
            }
            _ => false,
        }
    }

    /// `isParameterOrMutableLocalVariable` (`utilities.go:1044`) as written:
    /// unlike [`Checker::is_parameter_or_mutable_local_variable`], a
    /// module-level `let` is a mutable local. Gates `isSymbolAssignedDefinitely`'s
    /// record only.
    fn is_parameter_or_mutable_local_variable_faithful(&self, symbol: SymbolId) -> bool {
        let Some(mut declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        while let Some(parent) = self.nodes.parent(declaration) {
            if matches!(
                self.nodes.kind(parent),
                SyntaxKind::BindingElement
                    | SyntaxKind::ObjectBindingPattern
                    | SyntaxKind::ArrayBindingPattern
                    | SyntaxKind::Parameter
            ) {
                declaration = parent;
            } else {
                break;
            }
        }
        match self.nodes.kind(declaration) {
            SyntaxKind::Parameter => true,
            SyntaxKind::VariableDeclaration => {
                self.nodes
                    .parent(declaration)
                    .is_some_and(|list| self.nodes.kind(list) == SyntaxKind::CatchClause)
                    || self.is_mutable_local_variable_declaration(declaration)
            }
            _ => false,
        }
    }

    /// `isPastLastAssignment` (`flow.go:2668`): never assigned (0) or last
    /// assigned before the reference.
    fn is_past_last_assignment(&mut self, symbol: SymbolId, location: NodeId) -> bool {
        self.ensure_assignments_marked(symbol);
        let pos = self.last_assignment_pos.get(&symbol).copied().unwrap_or(0);
        pos == 0 || pos < i64::from(self.nodes.span(location).start)
    }

    /// `ensureAssignmentsMarked` (`flow.go:2674`): one marking walk per
    /// enclosing function/source-file root.
    /// `isSomeSymbolAssigned` (`checker.go:31471`). §764.
    ///
    /// Whether ANY symbol bound by `root_declaration`'s name is assigned
    /// anywhere — for a plain identifier that is the one symbol, for a binding
    /// pattern it recurses over the elements. Upstream uses it to refuse the
    /// pseudo-reference narrowing on a PARAMETER that the body reassigns: the
    /// destructured siblings are then no longer projections of one parent
    /// value, so discriminating them against each other is unsound.
    pub(crate) fn is_some_symbol_assigned(&mut self, root_declaration: NodeId) -> bool {
        let name = match self.node_map.get(root_declaration) {
            Some(Node::ParameterDeclaration(node)) => node.name,
            Some(Node::VariableDeclaration(node)) => node.name,
            Some(Node::BindingElement(node)) => node.name,
            _ => None,
        };
        let Some(name) = name else { return false };
        self.is_some_symbol_assigned_worker(name)
    }

    /// `isSomeSymbolAssignedWorker` (`checker.go:31475`). §764.
    fn is_some_symbol_assigned_worker(&mut self, name: tsr_ast::BindingName<'_>) -> bool {
        match name {
            tsr_ast::BindingName::Identifier(identifier) => {
                let Some(id) = identifier.node_id else { return false };
                // `getSymbolOfDeclaration(node.Parent)` — the symbol is the
                // DECLARATION's, and the name node is how it is reached.
                let Some(declaration) = self.nodes.parent(id) else { return false };
                let Some(symbol) = self.binder.symbol_of(declaration) else {
                    return false;
                };
                self.ensure_assignments_marked(symbol);
                self.last_assignment_pos.contains_key(&symbol)
            }
            tsr_ast::BindingName::BindingPattern(pattern) => pattern
                .elements
                .iter()
                .filter_map(|element| element.name)
                .collect::<Vec<_>>()
                .into_iter()
                .any(|inner| self.is_some_symbol_assigned_worker(inner)),
        }
    }

    fn ensure_assignments_marked(&mut self, symbol: SymbolId) {
        if self.last_assignment_pos.contains_key(&symbol) {
            return;
        }
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return;
        };
        let Some(root) = self.function_or_source_file_ancestor(declaration) else { return };
        if self.assignments_marked.contains(&root) {
            return;
        }
        // `hasParentWithAssignmentsMarked`: an already-marked ancestor's walk
        // covered this root's subtree.
        let mut ancestor = self.nodes.parent(root);
        while let Some(id) = ancestor {
            if self.assignments_marked.contains(&id) {
                self.assignments_marked.insert(root);
                return;
            }
            ancestor = self.nodes.parent(id);
        }
        self.assignments_marked.insert(root);
        self.mark_node_assignments(root);
    }

    /// The nearest function-like or source-file ancestor (inclusive walk from
    /// the parent) — upstream's `ast.IsFunctionOrSourceFile` `FindAncestor`.
    pub(crate) fn function_or_source_file_ancestor(&self, node: NodeId) -> Option<NodeId> {
        let mut current = Some(node);
        while let Some(id) = current {
            if matches!(
                self.nodes.kind(id),
                tsr_ast::SyntaxKind::FunctionDeclaration
                    | tsr_ast::SyntaxKind::FunctionExpression
                    | tsr_ast::SyntaxKind::ArrowFunction
                    | tsr_ast::SyntaxKind::MethodDeclaration
                    | tsr_ast::SyntaxKind::GetAccessor
                    | tsr_ast::SyntaxKind::SetAccessor
                    | tsr_ast::SyntaxKind::Constructor
                    | tsr_ast::SyntaxKind::SourceFile
            ) {
                return Some(id);
            }
            current = self.nodes.parent(id);
        }
        None
    }

    /// `markNodeAssignments` (`flow.go:2698`): record the last assignment
    /// position for every parameter/mutable-local assigned under `root` —
    /// `i64::MAX` when the assignment sits in a nested function. The export-
    /// specifier arm is unported (value re-exports of mutable locals), which
    /// under-reports `MAX` — conservative for §13's *reader*, which then
    /// extends when upstream would not; the bar's falsifier (a) watches the
    /// population where that could bite.
    fn mark_node_assignments(&mut self, root: NodeId) {
        let Some(root_node) = self.node_map.get(root) else { return };
        let mut stack = vec![root_node];
        let mut children = Vec::new();
        while let Some(node) = stack.pop() {
            if let Node::Identifier(identifier) = node
                && let Some(id) = identifier.node_id
                && let kind = self.assignment_target_kind(id)
                && kind != crate::expressions::AssignmentTargetKind::None
                && let Some(symbol) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    id,
                    identifier.text,
                    SymbolFlags::VALUE,
                )
            {
                // The definite flag is gated by upstream's predicate as written
                // (a module-level `let` is a mutable local); the position below
                // keeps the `.types` reader's file-level refusal (§42).
                if kind == crate::expressions::AssignmentTargetKind::Definite
                    && self.is_parameter_or_mutable_local_variable_faithful(symbol)
                {
                    self.definitely_assigned.insert(symbol);
                }
                // `hasDefiniteAssignment` is written **outside** the
                // `lastAssignmentPos != MAX` guard upstream (`flow.go:2718`):
                // the guard governs the position, not the flag, and a symbol
                // already at `MAX` can still gain its first definite
                // assignment. Splitting the two writes apart is what keeps this
                // addition from moving `last_assignment_pos` by a single entry
                // — `checker-notes-diag2.md` §42.
                if self.is_parameter_or_mutable_local_variable(symbol)
                    && self.last_assignment_pos.get(&symbol) != Some(&i64::MAX)
                {
                    self.record_assignment_position(id, symbol);
                }
            }
            children.clear();
            tsr_ast::push_children(node, &mut children);
            stack.extend(children.iter().copied());
        }
    }

    /// The position half of [`Checker::mark_node_assignments`]'s identifier arm
    /// (`flow.go:2711`), unchanged from the form the `.types` workstream
    /// measured — extracted only so the definite-assignment flag can be written
    /// under its own condition.
    fn record_assignment_position(&mut self, id: NodeId, symbol: SymbolId) {
        let referencing = self.function_or_source_file_ancestor(id);
        let declaring = self
            .binder
            .symbols()
            .get(symbol)
            .value_declaration
            .and_then(|declaration| self.function_or_source_file_ancestor(declaration));
        let pos = if referencing == declaring {
            self.binder
                .symbols()
                .get(symbol)
                .value_declaration
                .map_or(i64::MAX, |declaration| self.extend_assignment_position(id, declaration))
        } else {
            i64::MAX
        };
        // Upstream's source-order walk overwrites, leaving the
        // source-LAST assignment's extended position; this walk is
        // stack-ordered, so the equivalent is the MAXIMUM — larger
        // never wrongly reads as "past" (it can only stop the §13
        // extension sooner than upstream would).
        let entry = self.last_assignment_pos.entry(symbol).or_insert(0);
        *entry = (*entry).max(pos);
    }

    /// `isSymbolAssignedDefinitely` (`flow.go:2655`): is there a definite
    /// assignment to this symbol anywhere in its root's subtree?
    ///
    /// The whole point of the per-symbol record. TS2454's `isNeverInitialized`
    /// is its only consumer, and a *name*-based approximation of the same
    /// question measured 4 lost cases (`checker-notes-diag2.md` §42).
    /// `isNeverInitialized` (`checker.go:11147`): a mutable local
    /// `VariableDeclaration`, not a `for..in`/`for..of` head, with no
    /// initialiser and no `!`, that no definite assignment targets.
    pub(crate) fn is_never_initialized(&mut self, symbol: SymbolId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        let Some(Node::VariableDeclaration(variable)) = self.node_map.get(declaration) else {
            return false;
        };
        if variable.initializer.is_some() || variable.exclamation_token.is_some() {
            return false;
        }
        if self.nodes.parent(declaration).and_then(|list| self.nodes.parent(list)).is_some_and(
            |owner| {
                matches!(
                    self.nodes.kind(owner),
                    SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement
                )
            },
        ) {
            return false;
        }
        self.is_mutable_local_variable_declaration(declaration)
            && !self.is_symbol_assigned_definitely(symbol)
    }

    pub(crate) fn is_symbol_assigned_definitely(&mut self, symbol: SymbolId) -> bool {
        self.ensure_assignments_marked(symbol);
        self.definitely_assigned.contains(&symbol)
    }

    /// `extendAssignmentPosition` (`flow.go:2752`): the assignment position,
    /// stretched to the end of any compound statement between it and the
    /// declaration — conservatism in place of flow analysis.
    fn extend_assignment_position(&self, node: NodeId, declaration: NodeId) -> i64 {
        let declaration_pos = self.nodes.span(declaration).start;
        let mut pos = i64::from(self.nodes.span(node).start);
        let mut current = Some(node);
        while let Some(id) = current {
            if self.nodes.span(id).start <= declaration_pos {
                break;
            }
            if matches!(
                self.nodes.kind(id),
                tsr_ast::SyntaxKind::VariableStatement
                    | tsr_ast::SyntaxKind::ExpressionStatement
                    | tsr_ast::SyntaxKind::IfStatement
                    | tsr_ast::SyntaxKind::DoStatement
                    | tsr_ast::SyntaxKind::WhileStatement
                    | tsr_ast::SyntaxKind::ForStatement
                    | tsr_ast::SyntaxKind::ForInStatement
                    | tsr_ast::SyntaxKind::ForOfStatement
                    | tsr_ast::SyntaxKind::WithStatement
                    | tsr_ast::SyntaxKind::SwitchStatement
                    | tsr_ast::SyntaxKind::TryStatement
                    | tsr_ast::SyntaxKind::ClassDeclaration
            ) {
                pos = i64::from(self.nodes.span(id).end);
            }
            current = self.nodes.parent(id);
        }
        pos
    }

    /// Whether the symbol's value declaration sits at file level — its
    /// control-flow container is the `SourceFile`. The §9.7 outer split's
    /// second disjunct.
    fn declaration_is_file_level(&self, symbol: SymbolId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        // `var` only: a `let`/`const` is block-scoped and stays a mutable
        // LOCAL wherever its block sits (`nestedBlockScopedBindings9/11`
        // regressed on a file-level bare block before this gate — round 3 of
        // the §9.7 measurements).
        let is_var = self.nodes.parent(declaration).is_some_and(|list| {
            self.nodes.kind(list) == tsr_ast::SyntaxKind::VariableDeclarationList
                && !self
                    .nodes
                    .flags(list)
                    .intersects(tsr_ast::NodeFlags::LET | tsr_ast::NodeFlags::CONST)
        });
        if !is_var {
            return false;
        }
        let mut id = declaration;
        while let Some(parent) = self.nodes.parent(id) {
            match self.nodes.kind(parent) {
                tsr_ast::SyntaxKind::SourceFile => return true,
                tsr_ast::SyntaxKind::FunctionDeclaration
                | tsr_ast::SyntaxKind::FunctionExpression
                | tsr_ast::SyntaxKind::ArrowFunction
                | tsr_ast::SyntaxKind::MethodDeclaration
                | tsr_ast::SyntaxKind::GetAccessor
                | tsr_ast::SyntaxKind::SetAccessor
                | tsr_ast::SyntaxKind::Constructor => return false,
                _ => {}
            }
            id = parent;
        }
        false
    }

    /// Upstream's `isOuterVariable` (`checker.go:11130`): the reference's
    /// control-flow container is not the declaration's. The container walk is
    /// `closuregap.rs`'s predicate brought in-tree.
    fn is_outer_reference(&self, reference: NodeId, symbol: Option<SymbolId>) -> bool {
        let Some(symbol) = symbol else { return false };
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        let container = |mut id: NodeId| -> Option<NodeId> {
            while let Some(parent) = self.nodes.parent(id) {
                match self.nodes.kind(parent) {
                    tsr_ast::SyntaxKind::FunctionDeclaration
                    | tsr_ast::SyntaxKind::FunctionExpression
                    | tsr_ast::SyntaxKind::ArrowFunction
                    | tsr_ast::SyntaxKind::MethodDeclaration
                    | tsr_ast::SyntaxKind::GetAccessor
                    | tsr_ast::SyntaxKind::SetAccessor
                    | tsr_ast::SyntaxKind::Constructor
                    | tsr_ast::SyntaxKind::SourceFile => return Some(parent),
                    _ => {}
                }
                id = parent;
            }
            None
        };
        container(reference) != container(declaration)
    }
    /// Whether `symbol`'s declaration is one upstream gives `autoType`
    /// (`getTypeForVariableLikeDeclaration`, `checker.go:16697`).
    ///
    /// # Why this asks the declaration rather than comparing types
    ///
    /// Upstream's `autoType` is a *distinct intrinsic that prints `any`*, so
    /// `declaredType == autoType` is a pointer comparison. Reproducing that
    /// would mean a second `any` in [`crate::intrinsics`] and a new arm in
    /// `getTypeForVariableLikeDeclaration`, both outside this module. The
    /// question is only ever asked here, and the declaration carries every input
    /// to it, so it is asked here. The consequence to accept: a caller that
    /// hands this module a declared type *not* derived from the symbol's
    /// declaration would get the automatic treatment anyway. There is one
    /// caller, [`crate::expressions`], and it passes `get_type_of_symbol`.
    ///
    /// # `noImplicitAny` is assumed on, and this is the load-bearing assumption
    ///
    /// Upstream gates the whole arm on `c.noImplicitAny` (`checker.go:16697`):
    /// with it **off**, `let x; x = 1; x` is `any` at every use and nothing
    /// evolves. The flag is **assumed on** here, matching `strict`.
    ///
    /// Its original reason — *"this checker has no compiler options plumbed at
    /// all"* — expired with ADR-0042: [`Checker::no_implicit_any`] is set from
    /// `CompilerOptions` at construction and can be read here. The assumption
    /// survives only because nobody has changed the arm to read it.
    ///
    /// **How you would know this was wrong:** conformance lines *regressing*
    /// from `any` to a narrowed type in fixtures compiled without
    /// `noImplicitAny`. That population is invisible from inside this crate; it
    /// is the first thing to look at if the coverage delta from this commit is
    /// negative. `bd tsr-4sc.11` tracks plumbing the options through.
    ///
    /// # Unported conditions, each of which makes this answer `false` too often
    ///
    /// - A `null` or `undefined` **initialiser** also yields `autoType`
    ///   upstream; only a missing initialiser does here.
    /// - An **empty array literal** initialiser yields `autoArrayType`, which is
    ///   not ported — so answering `false` for it is correct, not a gap.
    /// - **`export`ed** and **ambient** declarations are excluded upstream and
    ///   are not excluded here, because modifier flags are not reachable from
    ///   this module. An exported `let x;` therefore evolves when upstream
    ///   leaves it `any`.
    fn is_auto_typed_declaration(&mut self, symbol: SymbolId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        let Some(Node::VariableDeclaration(node)) = self.node_map.get(declaration) else {
            return false;
        };
        // A catch-clause variable is `unknown`/`any` by DECLARATION KIND, not
        // an evolving auto — before this gate the §15 compound skip walked
        // through to a START whose auto-initial `undefined` was
        // typeof-narrowed to `never` (`useUnknownInCatchVariables01`, the
        // §15 bar's fired leg).
        if self
            .nodes
            .parent(declaration)
            .is_some_and(|parent| self.nodes.kind(parent) == tsr_ast::SyntaxKind::CatchClause)
        {
            return false;
        }
        // §779: an AMBIENT declaration (`declare var a;`) can never be
        // assigned, so upstream's flow type stays `autoType` and
        // `convertAutoToAny` (`checker.go:11182`) answers `any` — autoType IS
        // `any` (`checker.go:976`). This port's auto road answers its INITIAL
        // `undefined` instead.
        if self.nodes.parent(declaration).and_then(|list| self.nodes.parent(list)).is_some_and(
            |statement| match self.node_map.get(statement) {
                Some(Node::VariableStatement(node)) => {
                    crate::check::has_modifier(node.modifiers, tsr_ast::SyntaxKind::DeclareKeyword)
                }
                _ => false,
            },
        ) {
            return false;
        }
        // A binding pattern is excluded upstream: `let { a } = x` declares
        // through the pattern and the auto reduction never applies.
        if !matches!(node.name, Some(tsr_ast::BindingName::Identifier(_))) {
            return false;
        }
        // §345: a for-in/for-of BINDING is never auto — upstream's
        // `isNeverInitialized` excludes `IsForInOrOfStatement(
        // declaration.Parent.Parent)` by name (`checker.go:11147`), and its
        // declared type is computed from the head, not `autoType`, so the
        // top-of-graph substitution must not inject `undefined`:
        // `v; for (var v of [0]) { }` records `>v : number` at the USE
        // (`conformance/for-of8/22`).
        if let Some(statement) =
            self.nodes.parent(declaration).and_then(|list| self.nodes.parent(list)).filter(
                |&statement| {
                    matches!(
                        self.nodes.kind(statement),
                        tsr_ast::SyntaxKind::ForInStatement | tsr_ast::SyntaxKind::ForOfStatement
                    )
                },
            )
        {
            // ...except the SELF-REFERENTIAL for-IN recovery shape
            // `for (var of in of) { }`, where the head expression IS the
            // bound name: upstream records `any` there
            // (`parserForOfStatement19`, the both-statements draft's one
            // R->W), and the auto road is what produces it. The for-OF
            // twin (`for (var v of v)`) goes the OTHER way: non-auto, the
            // §38 arm cycles to the implicit-any road, and `any` is the
            // declared answer (`for-of32/55`).
            let self_referential = self.nodes.kind(statement)
                == tsr_ast::SyntaxKind::ForInStatement
                && matches!(
                    (self.node_map.get(statement), node.name),
                    (
                        Some(Node::ForInOrOfStatement(head)),
                        Some(tsr_ast::BindingName::Identifier(bound)),
                    ) if matches!(
                        head.expression,
                        Some(tsr_ast::Expression::Identifier(iterated))
                            if iterated.text == bound.text
                    )
                );
            if !self_referential {
                return false;
            }
            // §740: this arm must BYPASS the `no_implicit_any` gate below.
            // Upstream's `any` for `for (var of in of) { }` is not the auto
            // road at all — it is CIRCULARITY: the for-in arm
            // (`checker.go:16657`) checks the head expression, which resolves
            // the same symbol mid-computation, `popTypeResolution` fails, and
            // `reportCircularityError` answers `anyType`. The auto road is
            // this port's STAND-IN for that mechanism, and the fixture
            // (`parserForOfStatement19`) is `@strict: false` — gating the
            // stand-in on `noImplicitAny` re-broke the case the §345 arm was
            // built for (1 R→W, measured).
            return true;
        }
        if node.r#type.is_some()
            || self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT)
        {
            return false;
        }
        // §738: upstream's condition is `initializer == nil ||
        // c.isNullOrUndefined(initializer)` (`checker.go:16702`), and its own
        // comment names all three words:
        //
        // ```go
        // // use control flow tracked 'any' type for non-ambient, non-exported var or let variables
        // // with no initializer or a 'null' or 'undefined' initializer.
        // if c.getCombinedNodeFlagsCached(declaration)&ast.NodeFlagsConstant == 0 &&
        //         (initializer == nil || c.isNullOrUndefined(initializer)) {
        //     return c.autoType
        // }
        // ```
        //
        // This port had the no-initializer half only. **The DECLARED-type half
        // was already complete** — `symbols.rs:4304` mints `any` for a
        // null/undefined initialiser under the same guards — so `let x = null`
        // already had the right declared type and the flow walk simply refused
        // to treat it as auto. `controlFlowArrays`' `f7` is the shape:
        //
        // ```ts
        // let x = null;
        // if (cond()) { x = []; while (cond()) { x.push("hello"); } }
        // return x;                                    // string[] | null
        // ```
        //
        // The `const` test above is upstream's and load-bearing for the same
        // reason it is at the mint site: a `const x = null` can never be
        // reassigned, so it keeps `null` and has nothing to evolve into.
        // §740: upstream's whole block is guarded by `c.noImplicitAny`
        // (`checker.go:16697`) — with the flag off, `let x;` is an ordinary
        // implicit `any` and NOTHING evolves; every reference answers `any`.
        // §738 gated the null/undefined-initializer half (8 R→W without it)
        // and recorded the no-initializer half as a known asymmetry —
        // unconditional since §9.7, measured that way, its own build. This is
        // that build. The array half (`is_auto_array_declaration`) has carried
        // the same gate since §710, so this closes the LAST ungated entry
        // onto the auto road.
        match node.initializer {
            None => self.no_implicit_any,
            Some(initializer) => {
                self.no_implicit_any && self.is_null_or_undefined_expression(initializer)
            }
        }
    }

    /// `isEmptyArrayAssignment` (`flow.go:283`), verbatim in both disjuncts:
    /// a variable declaration whose initializer is `[]`, or a non-binding-element
    /// node whose PARENT is a binary expression with `[]` on the right.
    fn is_empty_array_assignment(&self, node: NodeId) -> bool {
        let is_empty_array = |expression: Option<tsr_ast::Expression<'_>>| {
            matches!(
                expression,
                Some(tsr_ast::Expression::ArrayLiteralExpression(array))
                    if array.elements.is_empty()
            )
        };
        match self.node_map.get(node) {
            Some(Node::VariableDeclaration(declaration)) => {
                return is_empty_array(declaration.initializer);
            }
            // Upstream's `!ast.IsBindingElement(node)` guard on the second
            // disjunct: a binding element sits under a binary expression in
            // destructuring form and must not read its `[]` as its own.
            Some(Node::BindingElement(_)) => return false,
            _ => {}
        }
        self.nodes.parent(node).is_some_and(|parent| {
            matches!(
                self.node_map.get(parent),
                Some(Node::BinaryExpression(binary)) if is_empty_array(binary.right)
            )
        })
    }

    /// Upstream's `autoArrayType` trigger (`checker.go`, the evolving-array
    /// machinery): a variable declared with no annotation and an empty
    /// array-literal initializer. Only the §14 depth accounting asks.
    fn is_auto_array_declaration(&mut self, symbol: SymbolId) -> bool {
        // §710: `autoArrayType` lives in the SAME `noImplicitAny` block as the
        // scalar auto type (`checker.go:16697` — *"in noImplicitAny mode or a
        // .js file"*, then `isEmptyArrayLiteral(initializer)` →
        // `c.autoArrayType`). Without the flag, `var x = []` is an ordinary
        // `any[]` and never evolves.
        //
        // This is why `typedArrays` (no `@noImplicitAny`) records
        // `typedArrays : any[]` on every line while `controlFlowArrays`
        // (`@noImplicitAny: true`) evolves to `(string | number | boolean)[]`
        // from the SAME `x[0] = …` element-assignment form. §708 read that
        // difference as a missing gate in `getTypeAtFlowArrayMutation`; it is
        // this flag, one level earlier.
        if !self.no_implicit_any {
            return false;
        }
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        let Some(Node::VariableDeclaration(node)) = self.node_map.get(declaration) else {
            return false;
        };
        if node.r#type.is_some() {
            return false;
        }
        if matches!(
            node.initializer,
            Some(tsr_ast::Expression::ArrayLiteralExpression(array))
                if array.elements.is_empty()
        ) {
            return true;
        }
        // §736 — the ASSIGNMENT half of the array track, §712's reopening
        // condition. Upstream does not decide the track from the declaration at
        // all: `isEmptyArrayAssignment` (`flow.go:283`) has TWO disjuncts, and
        // the second is *"the node's parent is a binary expression whose right
        // operand is `[]`"* — a per-flow-NODE test. `let x; x = [];` takes that
        // second disjunct and never the first.
        //
        // This port has to answer earlier than upstream does. `is_auto_array`
        // selects whether the ARRAY_MUTATION arm accumulates, and the mutations
        // are met on the way BACK to the assignment, so the flag must be set
        // before the walk starts. The declaration-shaped stand-in for upstream's
        // node test is therefore: an auto-typed declaration (`let x;`) somewhere
        // in whose container `x = []` is written.
        //
        // **This alone is not the fix, and §713 measured that** — it widened
        // exactly this predicate, gained ZERO and lost 6. The other half is in
        // [`Checker::get_type_at_flow_assignment`]: the assignment node must
        // return the EVOLVING array rather than the assigned `never[]`, or
        // §710's finalisation gate (`answer.t == state.declared_type`) can never
        // hold. Neither half measures without the other.
        self.is_auto_typed_declaration(symbol) && self.symbol_has_empty_array_assignment(symbol)
    }

    /// §736: does any assignment to this symbol write an empty array literal?
    ///
    /// The declaration-side stand-in for the second disjunct of upstream's
    /// `isEmptyArrayAssignment` (`flow.go:283`), which this port must answer
    /// before the flow walk starts — see [`Checker::is_auto_array_declaration`]
    /// for why. Deliberately the same scan shape as
    /// [`Checker::symbol_has_any_assignment`]: the declaration's control-flow
    /// container, walked for `=` binaries whose target identifier resolves back
    /// to this symbol, here additionally requiring the right operand to be `[]`.
    fn symbol_has_empty_array_assignment(&mut self, symbol: SymbolId) -> bool {
        if let Some(&cached) = self.symbol_empty_array_assignment_scan.get(&symbol) {
            return cached;
        }
        // Inserted BEFORE the scan: `is_matching_reference` below resolves
        // names, which can re-enter this predicate for the same symbol. A
        // pessimistic `false` is the same answer the scan gives when it finds
        // nothing, so the guard cannot invent a track.
        self.symbol_empty_array_assignment_scan.insert(symbol, false);
        let answer = (|| {
            let declaration = self.binder.symbols().get(symbol).value_declaration?;
            let name = self.binder.symbols().get(symbol).name;
            let mut root = declaration;
            while let Some(parent) = self.nodes.parent(root) {
                root = parent;
                if matches!(
                    self.nodes.kind(root),
                    tsr_ast::SyntaxKind::FunctionDeclaration
                        | tsr_ast::SyntaxKind::FunctionExpression
                        | tsr_ast::SyntaxKind::ArrowFunction
                        | tsr_ast::SyntaxKind::MethodDeclaration
                        | tsr_ast::SyntaxKind::GetAccessor
                        | tsr_ast::SyntaxKind::SetAccessor
                        | tsr_ast::SyntaxKind::Constructor
                        | tsr_ast::SyntaxKind::SourceFile
                ) {
                    break;
                }
            }
            let root_node = self.node_map.get(root)?;
            let mut stack = vec![root_node];
            let mut children = Vec::new();
            while let Some(node) = stack.pop() {
                if let Node::BinaryExpression(binary) = node
                    && binary
                        .operator_token
                        .is_some_and(|t| t.kind == tsr_ast::SyntaxKind::EqualsToken)
                    && matches!(
                        binary.right,
                        Some(tsr_ast::Expression::ArrayLiteralExpression(array))
                            if array.elements.is_empty()
                    )
                    && let Some(tsr_ast::Expression::Identifier(target)) = binary.left
                    && target.text == name
                {
                    // Name equality is not identity: an inner scope may shadow
                    // the name. Resolve the target back to this symbol, exactly
                    // as `symbol_has_any_assignment` does.
                    if target.node_id.is_some_and(|id| {
                        self.binder
                            .resolve_name(
                                self.nodes,
                                self.node_map,
                                id,
                                name,
                                tsr_binder::SymbolFlags::VALUE,
                            )
                            .is_some_and(|found| found == symbol)
                    }) {
                        return Some(true);
                    }
                }
                children.clear();
                tsr_ast::push_children(node, &mut children);
                stack.extend(children.iter().copied());
            }
            Some(false)
        })()
        .unwrap_or(false);
        self.symbol_empty_array_assignment_scan.insert(symbol, answer);
        answer
    }

    /// The type an assignment flow node puts into the variable
    /// (`getInitialOrAssignedType`, `flow.go:276`).
    ///
    /// The binder records an assignment's flow node against the variable
    /// declaration when it has an initialiser, and against the *target
    /// identifier* otherwise — so the two arms here are upstream's
    /// `getInitialType` (`flow.go:2234`) and `getAssignedType` (`flow.go:2288`)
    /// reduced to the forms that binder produces.
    ///
    /// `None` for the destructuring forms (binding elements, array/object
    /// literal targets), whose element projection is not reproduced here.
    /// (This said `for..in`, `for..of` and `delete` were `None` too; they are
    /// ported since — a correction of the record.)
    fn get_initial_or_assigned_type(&mut self, node: NodeId) -> Option<TypeId> {
        if let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) {
            // `getInitialTypeOfVariableDeclaration` (`flow.go:2244`).
            if let Some(initializer) = declaration.initializer {
                return Some(self.check_expression(initializer));
            }
            let statement = self.nodes.parent(node).and_then(|list| self.nodes.parent(list))?;
            return self.for_in_or_of_assigned_type(statement);
        }
        let parent = self.nodes.parent(node)?;
        // `getAssignedType` (`flow.go:2288`): the `for..in`, `for..of` and
        // `delete` arms. The destructuring arms (array/object literal
        // elements, spreads, property assignments) stay unported and answer
        // `None` — the declared type, as upstream's `errorType` reduces to.
        match self.nodes.kind(parent) {
            SyntaxKind::ForInStatement | SyntaxKind::ForOfStatement => {
                return self.for_in_or_of_assigned_type(parent);
            }
            SyntaxKind::DeleteExpression => return Some(self.intrinsics.undefined),
            _ => {}
        }
        // `getAssignedTypeOfBinaryExpression` (`flow.go:2314`), restricted to a
        // plain `x = e`. A destructuring default (`[x = 1] = y`) reaches the same
        // upstream function by a different route and is not handled.
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else { return None };
        if binary.operator_token?.kind != SyntaxKind::EqualsToken {
            return None;
        }
        if binary.left.and_then(|left| Node::from(left).node_id()) != Some(node) {
            return None;
        }
        Some(self.check_expression(binary.right?))
    }

    /// The `for..in` / `for..of` arms of `getInitialTypeOfVariableDeclaration`
    /// and `getAssignedType` (`flow.go:2244`, `:2288`): `string` for `for..in`,
    /// `checkRightHandSideOfForOf` for `for..of`. `None` when this port cannot
    /// decide the iterated element (upstream's `errorType`).
    fn for_in_or_of_assigned_type(&mut self, statement: NodeId) -> Option<TypeId> {
        match self.nodes.kind(statement) {
            SyntaxKind::ForInStatement => Some(self.intrinsics.string),
            SyntaxKind::ForOfStatement => {
                let Some(Node::ForInOrOfStatement(for_of)) = self.node_map.get(statement) else {
                    return None;
                };
                let expression = for_of.expression?;
                self.for_of_statement_element_type(expression, for_of.await_modifier.is_some())
            }
            _ => None,
        }
    }

    /// Keep the constituents of a union declared type that the assigned type
    /// could be (`getAssignmentReducedType`, `flow.go:2399`).
    ///
    /// The memo upstream keeps (`assignmentReducedTypes`) is not ported: it
    /// guards repeated assignability queries, and this port's assignability is
    /// not yet the cost centre that makes a cache pay.
    /// SS160: decidable assignability between REFERENCES - same target
    /// symbol with pairwise-identical arguments is assignable; same target
    /// with a decidably-unrelated argument pair (the relater's primitive
    /// domain) is not; anything else is undecidable.
    fn reference_assignable_decidable(&mut self, source: TypeId, target: TypeId) -> Option<bool> {
        let (source_target, source_arguments) = self.type_reference_targets.get(&source)?.clone();
        let (target_target, target_arguments) = self.type_reference_targets.get(&target)?.clone();
        if self.binder.merged_symbol(source_target) != self.binder.merged_symbol(target_target)
            || source_arguments.len() != target_arguments.len()
        {
            return None;
        }
        if source_arguments == target_arguments {
            return Some(true);
        }
        let mut refuted = false;
        for (&sa, &ta) in source_arguments.iter().zip(target_arguments.iter()) {
            if sa == ta {
                continue;
            }
            match self.relate_ternary(sa, ta, crate::relater::Relation::Assignable) {
                crate::relater::Ternary::Related => {}
                crate::relater::Ternary::NotRelated => refuted = true,
                crate::relater::Ternary::Unknown => return None,
            }
        }
        Some(!refuted)
    }

    fn get_assignment_reduced_type(&mut self, declared: TypeId, assigned: TypeId) -> TypeId {
        if declared == assigned {
            return declared;
        }
        if self.store.get(assigned).flags.contains(TypeFlags::NEVER) {
            return assigned;
        }
        let TypeData::Union { types, .. } = &self.store.get(declared).data else {
            return declared;
        };
        let constituents = types.clone();
        // A one-constituent union exists in this port only as the one-member
        // enum deviation (`unions.rs`, `get_named_union_type`); upstream's
        // declared type there IS the member — no union, no reduction
        // (`enumOperations`, the §18 measurement's largest fired leg).
        if constituents.len() == 1 {
            return declared;
        }
        let mut kept = Vec::with_capacity(constituents.len());
        for &constituent in &constituents {
            // SS160: the reference-identity slice answers first; its
            // undecidables keep the old road.
            match self.reference_assignable_decidable(assigned, constituent) {
                Some(true) => kept.push(constituent),
                Some(false) => {}
                None => {
                    if self.type_maybe_assignable_to(assigned, constituent) {
                        kept.push(constituent);
                    }
                }
            }
        }
        // "Ensure that we narrow to fresh types if the assignment is a fresh
        // boolean literal type" (`flow.go:2421`): `var c4 = true` narrows the
        // declared `boolean` to the **fresh** `true`, which is what lets a
        // mutable-location or initializer boundary widen it back to `boolean`
        // — `checker-notes-narrow.md` §7 (`bd tsr-xs0`) carries the sizing
        // and the bar. Mapped over the kept constituents, as upstream's
        // `mapType(filteredType, getFreshTypeOfLiteralType)` is.
        let assigned_is_fresh_boolean = {
            let ty = self.store.get(assigned);
            ty.flags.contains(TypeFlags::BOOLEAN_LITERAL) && ty.fresh
        };
        // filterType preserves the input when every constituent survives,
        // including the alias/origin carried by the declared union.
        if kept == constituents && !assigned_is_fresh_boolean {
            return declared;
        }
        let kept = if assigned_is_fresh_boolean {
            kept.into_iter().map(|t| self.get_fresh_type_of_literal_type(t)).collect()
        } else {
            kept
        };
        let reduced = self.get_union_type(&kept);
        // Upstream's own guard on its "crude heuristic" (`flow.go:2424`): when
        // the assigned type is not assignable to what the filter kept, give up
        // and narrow nothing rather than print a type the assignment refutes.
        if self.reference_assignable_decidable(assigned, reduced) == Some(true)
            || self.is_type_assignable_to(assigned, reduced)
        {
            reduced
        } else {
            declared
        }
    }

    /// `typeMaybeAssignableTo` (`flow.go:2434`): a union source needs only one
    /// constituent to be assignable, which is what makes assigning
    /// `string | number` to a `string | number | boolean` declaration keep two
    /// constituents rather than none.
    fn type_maybe_assignable_to(&mut self, source: TypeId, target: TypeId) -> bool {
        let sources: Option<Vec<TypeId>> = match &self.store.get(source).data {
            TypeData::Union { types, .. } => Some(types.clone()),
            _ => None,
        };
        match sources {
            Some(sources) => sources.into_iter().any(|s| self.is_type_assignable_to(s, target)),
            None => self.is_type_assignable_to(source, target),
        }
    }

    /// The type after a condition is known to have gone one way
    /// (`getTypeAtFlowCondition`, `flow.go:340`).
    /// `getEffectsSignature` (`flow.go:2047`): the signature whose EFFECTS
    /// (an `asserts` predicate or a `never` return) a CALL flow node
    /// carries, or `None` when the call has none.
    ///
    /// Upstream caches this per node in `signatureLinks.effectsSignature`;
    /// this port recomputes — the walk memoises per flow node already.
    /// The `[Symbol.hasInstance]` binary-expression arm is not ported (no
    /// `instanceof` flow-call nodes in this binder).
    fn get_effects_signature(
        &mut self,
        call_node: NodeId,
        call: &tsr_ast::CallExpression<'_>,
    ) -> Option<crate::signatures::Signature> {
        let callee = call.expression?;
        let callee_id = callee.node_id()?;
        // Native signatures retain lazy returns when checking effects. This
        // port still materializes vectors, so prove the negative before any
        // callee typing; no provisional return can leak to a cold helper's
        // completed slot. Unknown ownership keeps the ordinary effects path.
        if let Some(Node::Identifier(identifier)) = self.node_map.get(callee_id)
            && let Some(symbol) = self.binder.resolve_name(
                self.nodes,
                self.node_map,
                callee_id,
                identifier.text,
                SymbolFlags::VALUE,
            )
            && self.function_symbol_has_no_effects(symbol)
        {
            return None;
        }
        let parent_is_statement = self
            .nodes
            .parent(call_node)
            .is_some_and(|parent| self.nodes.kind(parent) == SyntaxKind::ExpressionStatement);
        let func_type = if parent_is_statement {
            self.get_type_of_dotted_name(callee_id)?
        } else if self.nodes.kind(callee_id) == SyntaxKind::SuperKeyword {
            return None;
        } else {
            // `checkNonNullExpression(node.Expression())`. The optional-chain
            // arm (`getOptionalExpressionType`) is not split out: this
            // port's `check_expression` of a chain already answers the
            // non-optional type at the callee position.
            self.check_expression(callee)
        };
        if func_type == self.intrinsics.error {
            return None;
        }
        let apparent = self.apparent_type(func_type);
        let signatures = self.call_signatures_of_type(apparent)?;
        let signature = if signatures.len() == 1 && signatures[0].type_parameters.is_empty() {
            signatures.into_iter().next()?
        } else if signatures.iter().any(|s| self.has_type_predicate_or_never_return(s)) {
            self.resolve_call_signature_with_type_arguments(
                func_type,
                Some(call.arguments),
                !call.type_arguments.is_empty(),
            )?
        } else {
            return None;
        };
        if !self.has_type_predicate_or_never_return(&signature) {
            return None;
        }
        if !signature.type_parameters.is_empty() {
            let mut instantiated = None;
            self.check_generic_call_with(
                &signature,
                Some(call_node),
                call.arguments,
                Some(&mut instantiated),
            );
            return instantiated;
        }
        Some(signature)
    }

    /// `hasTypePredicateOrNeverReturnType` (`flow.go:2211`): a predicate, or
    /// an ANNOTATED return that is `never`. Upstream reads the annotation
    /// (`getReturnTypeFromAnnotation`), never the inferred return — a
    /// function whose body only throws does not truncate flow at its
    /// callers upstream either, which corrects §744's residue note.
    fn has_type_predicate_or_never_return(
        &mut self,
        signature: &crate::signatures::Signature,
    ) -> bool {
        if signature.predicate.is_some() {
            return true;
        }
        let annotated = match self.node_map.get(signature.declaration) {
            Some(Node::FunctionDeclaration(f)) => f.r#type.is_some(),
            Some(Node::MethodDeclaration(m)) => m.r#type.is_some(),
            Some(Node::FunctionExpression(f)) => f.r#type.is_some(),
            Some(Node::ArrowFunction(f)) => f.r#type.is_some(),
            Some(Node::FunctionTypeNode(f)) => f.r#type.is_some(),
            Some(Node::MethodSignatureDeclaration(m)) => m.r#type.is_some(),
            Some(Node::CallSignatureDeclaration(c)) => c.r#type.is_some(),
            _ => false,
        };
        if annotated {
            return self.store.get(signature.r#type).flags.contains(TypeFlags::NEVER);
        }
        // A JSDoc return node is not copied into this AST's type field. Native
        // effects read the annotation itself, never an inferred or mapped return.
        self.jsdoc_return_annotation(signature.declaration).is_some_and(|annotation| {
            let returned = self.get_type_from_type_node(annotation);
            self.store.get(returned).flags.contains(TypeFlags::NEVER)
        })
    }

    /// `getSignaturesOfType(t, SignatureKindCall)` reads declared, instantiated
    /// and composite call lists through the shared kind-specific resolver.
    pub(crate) fn call_signatures_of_type(
        &mut self,
        t: TypeId,
    ) -> Option<Vec<crate::signatures::Signature>> {
        self.signatures_of_type_kind(t, crate::signatures::SignatureKind::Call)
    }

    /// Native parameter-only inference/comparison reads signature links without
    /// getReturnTypeOfSignature (inference.go:838, relater.go:4452). Retain the
    /// existing original vector/TypeId instead of rebuilding its declarations
    /// under an alias mapper. An unpublished FUNCTION vector requires exact
    /// active-source proof for an ephemeral primitive-parameter projection;
    /// other active/unsupported work cannot certify an empty or completed set.
    pub(crate) fn signature_shapes_of_type_kind(
        &mut self,
        ty: TypeId,
        kind: crate::signatures::SignatureKind,
    ) -> Option<Vec<crate::signatures::Signature>> {
        let ty = self.apparent_type(ty);
        if let Some(signatures) = self.signature_types.get(&ty) {
            return Some(
                signatures
                    .iter()
                    .filter(|signature| {
                        (signature.kind == crate::signatures::SignatureKind::Call)
                            == (kind == crate::signatures::SignatureKind::Call)
                    })
                    .cloned()
                    .collect(),
            );
        }
        if let TypeData::Anonymous { symbol, .. } = self.store.get(ty).data
            && self
                .binder
                .symbols()
                .get(self.binder.merged_symbol(symbol))
                .flags
                .intersects(SymbolFlags::FUNCTION | SymbolFlags::METHOD)
        {
            let signature = self.parameter_only_signature_of_active_function(ty, symbol)?;
            return Some((signature.kind == kind).then_some(signature).into_iter().collect());
        }
        self.signatures_of_type_kind(ty, kind)
    }

    /// getSignaturesOfType (internal/checker/checker.go) uses separate call
    /// and construct sets; abstract constructors belong to the construct set.
    pub(crate) fn signatures_of_type_kind(
        &mut self,
        t: TypeId,
        kind: crate::signatures::SignatureKind,
    ) -> Option<Vec<crate::signatures::Signature>> {
        let is_call = kind == crate::signatures::SignatureKind::Call;
        let t = self.apparent_type(t);
        // getSignaturesOfType resolves the intrinsic object type to an empty
        // member table; it does not make an intersected callable unresolved.
        // Native getReducedApparentType can already have replaced `object`
        // with a canonical empty object (5b1047d, checker.go:18959/21754).
        // These completed intrinsic objects have no call or construct slots;
        // symbol-less objects without this identity still decline below.
        if self.store.get(t).flags.contains(TypeFlags::NON_PRIMITIVE)
            || t == self.intrinsics.empty_object
            || t == self.intrinsics.unknown_empty_object
        {
            return Some(Vec::new());
        }
        if self.intersection_has_never_discriminant(t) {
            return Some(Vec::new());
        }
        if let Some(signatures) = self.signature_types.get(&t).cloned() {
            return signatures
                .into_iter()
                .filter(|s| (s.kind == crate::signatures::SignatureKind::Call) == is_call)
                .map(|signature| self.complete_signature_return(signature))
                .collect();
        }
        match self.store.get(t).data {
            TypeData::Anonymous { symbol, .. } => {
                let symbol = self.binder.merged_symbol(symbol);
                if self.binder.symbols().get(symbol).flags.contains(SymbolFlags::CLASS) {
                    return if is_call {
                        Some(Vec::new())
                    } else {
                        self.get_class_construct_signatures(symbol)
                    };
                }
                let signatures = self.get_signatures_of_symbol(symbol)?;
                Some(
                    signatures
                        .into_iter()
                        .filter(|s| (s.kind == crate::signatures::SignatureKind::Call) == is_call)
                        .collect(),
                )
            }
            TypeData::Named { .. } => self.signature_candidates_of_named_type(t, kind),
            TypeData::Union { .. } => self.resolved_union_signatures(t, kind),
            TypeData::Intersection { .. } => self.intersection_signatures(t, kind),
            _ => None,
        }
    }

    /// `getTypeOfDottedName` (`flow.go:2122`): the type of a dotted name
    /// WITHOUT flow analysis — identifiers and property chains through
    /// their EXPLICIT types only, so that resolving an assertion's callee
    /// inside the walk cannot re-enter the walk. `this` answers
    /// [`Checker::get_explicit_this_type`]. Private names and `with`
    /// statements decline.
    fn get_type_of_dotted_name(&mut self, node: NodeId) -> Option<TypeId> {
        match self.node_map.get(node) {
            Some(Node::Identifier(identifier)) => {
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    node,
                    identifier.text,
                    SymbolFlags::VALUE,
                )?;
                // `getExportSymbolOfValueSymbolIfExported`.
                let symbol = self.binder.merged_symbol(symbol);
                self.get_explicit_type_of_symbol(symbol)
            }
            Some(Node::KeywordExpression(keyword)) => match keyword.kind {
                SyntaxKind::ThisKeyword => self.get_explicit_this_type(node),
                SyntaxKind::SuperKeyword => Some(self.check_super_expression(node)),
                _ => None,
            },
            Some(Node::PropertyAccessExpression(access)) => {
                let base = access.expression?.node_id()?;
                let t = self.get_type_of_dotted_name(base)?;
                let tsr_ast::MemberName::Identifier(name) = access.name? else { return None };
                let property = self.get_property_of_type(t, name.text)?;
                self.get_explicit_type_of_symbol(property)
            }
            Some(Node::ParenthesizedExpression(wrapper)) => {
                let inner = wrapper.expression?.node_id()?;
                self.get_type_of_dotted_name(inner)
            }
            _ => None,
        }
    }

    /// `getExplicitThisType` (`flow.go:2215`): the explicit type of the `this`
    /// container's `this` parameter, else the class's `this` (static side for
    /// a static member or static block), else nothing — an object-literal
    /// method's `this` is contextual, not explicit, so a call through it has
    /// no effects signature and cannot re-enter the method's own return type.
    ///
    /// The instance `this` shares `Checker::this_types` (keyed by class
    /// symbol, minted once) with `check_this_expression`, so both roads see
    /// one identity.
    fn get_explicit_this_type(&mut self, node: NodeId) -> Option<TypeId> {
        let container = self.get_this_container(node, false)?;
        if let ThisParameterOfContainer::Present(explicit) =
            self.this_parameter_of_container(container)
        {
            return explicit;
        }
        let class = self.nodes.parent(container)?;
        if !matches!(
            self.nodes.kind(class),
            SyntaxKind::ClassDeclaration | SyntaxKind::ClassExpression
        ) {
            return None;
        }
        let symbol = self.binder.symbol_of(class)?;
        let is_static = self.nodes.kind(container) == SyntaxKind::ClassStaticBlockDeclaration
            || self.node_map.get(container).and_then(crate::check::modifiers_of).is_some_and(
                |modifiers| tsr_ast::has_syntactic_modifier(modifiers, SyntaxKind::StaticKeyword),
            );
        if is_static {
            return Some(self.get_type_of_symbol(symbol));
        }
        if let Some(&cached) = self.this_types.get(&symbol) {
            return Some(cached);
        }
        let this_type =
            self.store.new_named(TypeFlags::TYPE_PARAMETER, "this".to_string(), Some(symbol));
        self.this_types.insert(symbol, this_type);
        Some(this_type)
    }

    /// `getSignatureFromDeclaration(container).thisParameter` read through
    /// `getExplicitTypeOfSymbol` (`getExplicitThisType`, `flow.go:2217`),
    /// without materializing the signature: this port's signatures resolve
    /// their return type eagerly, and the container's return type may be the
    /// very inference whose reachability walk is asking (`getEffectsSignature`
    /// under `getReturnTypeFromBody`), which would close a cycle into `any`.
    ///
    /// [`ThisParameterOfContainer::Present`] when the signature has a `this`
    /// parameter, which ends the lookup even without an explicit type:
    /// - a written first parameter named `this` is explicit only when
    ///   annotated (`isDeclarationWithExplicitTypeAnnotation`, `flow.go:2197`);
    /// - a JSDoc `@this`, which `reparseHosted` inserts as an annotated
    ///   parameter;
    /// - the contextual signature's `this` parameter that
    ///   `assignContextualParameterTypes` (`checker.go:10349`) copies onto a
    ///   context-sensitive function expression or method; the copy's value
    ///   declaration is the context's annotated parameter.
    fn this_parameter_of_container(&mut self, container: NodeId) -> ThisParameterOfContainer {
        let map = self.node_map;
        let Some(node) = map.get(container) else { return ThisParameterOfContainer::Absent };
        let parameters = match node {
            Node::FunctionDeclaration(node) => node.parameters,
            Node::FunctionExpression(node) => node.parameters,
            Node::MethodDeclaration(node) => node.parameters,
            Node::MethodSignatureDeclaration(node) => node.parameters,
            Node::CallSignatureDeclaration(node) => node.parameters,
            Node::ConstructSignatureDeclaration(node) => node.parameters,
            Node::IndexSignatureDeclaration(node) => node.parameters,
            Node::GetAccessorDeclaration(node) => node.parameters,
            Node::SetAccessorDeclaration(node) => node.parameters,
            Node::ConstructorDeclaration(node) => node.parameters,
            _ => return ThisParameterOfContainer::Absent,
        };
        if let Some(first) = parameters.first().filter(|first| {
            matches!(first.name, Some(tsr_ast::BindingName::Identifier(name)) if name.text == "this")
        }) {
            if first.r#type.is_none() {
                return ThisParameterOfContainer::Present(None);
            }
            let symbol = first.node_id.and_then(|id| self.binder.symbol_of(id));
            return ThisParameterOfContainer::Present(
                symbol.map(|symbol| self.get_type_of_symbol(symbol)),
            );
        }
        match self.jsdoc_this_parameter_type(container) {
            Some(this_type) => ThisParameterOfContainer::Present(Some(this_type)),
            None => match self.contextual_this_parameter_type(container) {
                Some(this_type) => ThisParameterOfContainer::Present(Some(this_type)),
                None => ThisParameterOfContainer::Absent,
            },
        }
    }

    /// `getExplicitTypeOfSymbol` (`flow.go:2154`): functions, methods,
    /// classes and namespaces answer their type; a variable or property
    /// only when its declaration carries an annotation. Not ported: the
    /// mapped-symbol origin arm, the `for..of` iterated-type arm, and the
    /// related-info diagnostic (this caller passes `nil`).
    pub(crate) fn get_explicit_type_of_symbol(&mut self, symbol: SymbolId) -> Option<TypeId> {
        let symbol = self.resolve_alias_fully(symbol);
        let flags = self.binder.symbols().get(symbol).flags;
        if flags.intersects(
            SymbolFlags::FUNCTION
                | SymbolFlags::METHOD
                | SymbolFlags::CLASS
                | SymbolFlags::VALUE_MODULE,
        ) {
            return Some(self.get_type_of_symbol(symbol));
        }
        if flags.intersects(SymbolFlags::VARIABLE | SymbolFlags::PROPERTY) {
            let declaration = self.binder.symbols().get(symbol).value_declaration?;
            // `isDeclarationWithExplicitTypeAnnotation`.
            let explicit = match self.node_map.get(declaration) {
                Some(Node::VariableDeclaration(v)) => v.r#type.is_some(),
                Some(Node::PropertyDeclaration(p)) => p.r#type.is_some(),
                Some(Node::PropertySignatureDeclaration(p)) => p.r#type.is_some(),
                Some(Node::ParameterDeclaration(p)) => p.r#type.is_some(),
                _ => false,
            };
            if explicit {
                return Some(self.get_type_of_symbol(symbol));
            }
        }
        None
    }

    /// `getTypeAtFlowCall` (`flow.go`), the assertion half: a CALL flow
    /// node whose resolved signature carries an `asserts` predicate narrows
    /// its reference argument or receiver. A bare `asserts this` has no type
    /// effect; never-returning calls contribute the unreachable sentinel.
    fn get_type_at_flow_call(&mut self, state: &mut FlowState, flow: FlowId) -> Option<FlowType> {
        let binder = self.binder;
        let call_node = binder.flow().node(flow)?;
        let Some(Node::CallExpression(call)) = self.node_map.get(call_node) else {
            return None;
        };
        // §746: `getEffectsSignature` proper replaces §127's syntactic
        // pre-gate. The callee of a statement-level call is typed through
        // `getTypeOfDottedName` — EXPLICIT types only, no flow — which is
        // what keeps the walk a read-only probe (§741's load-bearing
        // property, and the reason §127's iteration 3 needed a gate at all:
        // it had typed the callee through `checkExpression`).
        let signature = self.get_effects_signature(call_node, call)?;
        // §128 second attempt returned the DECLARED type here, reasoning
        // that the observable at an unreachable read is the declared type
        // (upstream converts its sentinel at the walk's exit, `flow.go:111`).
        // That was right at a read and WRONG at a junction: `if (x ===
        // undefined) fail(); x.length` joins the cut-off path with the live
        // one, and the declared `string | undefined` re-entered the union
        // where upstream's `unreachableNeverType` drops out as `never`
        // (`neverReturningFunctions1`, 8 false TS18048/TS2532). §744 ports
        // the sentinel: [`Intrinsics::unreachable_never`], dropped by every
        // join through its `NEVER` flag and converted to the declared type
        // only at the two walk exits.
        if signature.predicate.is_none() {
            if self.store.get(signature.r#type).flags.contains(TypeFlags::NEVER) {
                return Some(FlowType { t: self.intrinsics.unreachable_never, incomplete: false });
            }
            return None;
        }
        let predicate = signature.predicate.as_ref()?;
        if !predicate.asserts {
            return None;
        }
        let predicate_type = predicate.r#type;
        let argument_id = self.get_type_predicate_argument(&signature, call);
        let antecedent = binder.flow().antecedent(flow)?;
        let incoming = self.get_type_at_flow_node(state, antecedent);
        if self.store.get(incoming.t).flags.contains(TypeFlags::NEVER) {
            return Some(incoming);
        }
        let narrowed = match (predicate_type, argument_id) {
            (Some(predicate_type), Some(argument)) => self.narrow_type_by_type_predicate(
                state,
                incoming.t,
                predicate_type,
                argument,
                true,
            ),
            (None, Some(argument)) if predicate.parameter_name.is_some() => {
                self.narrow_type_by_assertion(state, incoming.t, argument)
            }
            _ => incoming.t,
        };
        Some(FlowType { t: narrowed, incomplete: incoming.incomplete })
    }

    /// `narrowTypeByAssertion` (`flow.go:339`): a bare `asserts x` narrows
    /// by its argument as a TRUE condition — except that a literal `false`
    /// (`assert(false)`) answers the `unreachableNeverType` sentinel, `&&`
    /// asserts both halves in sequence and `||` joins the two. §744 sent
    /// every shape through `narrow_type`, which answers the DECLARED type
    /// for `false` and so re-entered a cut-off path at the next join; §745
    /// ports the three arms.
    ///
    /// The `||` join drops `never`-flagged halves the way upstream's
    /// `addTypeToUnion` does (the sentinel included), and returns a LONE
    /// type unchanged — the same rule §744 found load-bearing at the
    /// branch-label join.
    fn narrow_type_by_assertion(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        expression: NodeId,
    ) -> TypeId {
        let mut id = expression;
        // `ast.SkipParentheses`.
        while let Some(Node::ParenthesizedExpression(wrapper)) = self.node_map.get(id) {
            match wrapper.expression.and_then(|e| e.node_id()) {
                Some(inner) => id = inner,
                None => break,
            }
        }
        match self.node_map.get(id) {
            Some(Node::KeywordExpression(keyword)) if keyword.kind == SyntaxKind::FalseKeyword => {
                return self.intrinsics.unreachable_never;
            }
            Some(Node::BinaryExpression(binary)) => {
                if let (Some(left), Some(right), Some(operator)) =
                    (binary.left, binary.right, binary.operator_token)
                    && let (Some(left), Some(right)) = (left.node_id(), right.node_id())
                {
                    match operator.kind {
                        SyntaxKind::AmpersandAmpersandToken => {
                            let after_left = self.narrow_type_by_assertion(state, t, left);
                            return self.narrow_type_by_assertion(state, after_left, right);
                        }
                        SyntaxKind::BarBarToken => {
                            let by_left = self.narrow_type_by_assertion(state, t, left);
                            let by_right = self.narrow_type_by_assertion(state, t, right);
                            if by_left == by_right {
                                return by_left;
                            }
                            let live: Vec<TypeId> = [by_left, by_right]
                                .into_iter()
                                .filter(|&member| {
                                    !self.store.get(member).flags.contains(TypeFlags::NEVER)
                                })
                                .collect();
                            return self.get_union_type(&live);
                        }
                        _ => {}
                    }
                }
            }
            _ => {}
        }
        self.narrow_type(state, t, id, true)
    }

    fn get_type_at_flow_condition(&mut self, state: &mut FlowState, flow: FlowId) -> FlowType {
        let binder = self.binder;
        let Some(antecedent) = binder.flow().antecedent(flow) else {
            return FlowType { t: state.declared_type, incomplete: false };
        };
        let incoming = self.get_type_at_flow_node(state, antecedent);
        if self.store.get(incoming.t).flags.contains(TypeFlags::NEVER) {
            return incoming;
        }
        let Some(condition) = binder.flow().node(flow) else { return incoming };
        let assume_true = binder.flow().flags(flow).contains(FlowFlags::TRUE_CONDITION);
        let narrowed = self.narrow_type(state, incoming.t, condition, assume_true);
        if narrowed == incoming.t {
            return incoming;
        }
        // Upstream switches to `silentNeverType` when narrowing an *incomplete*
        // type to `never`, so that a loop's provisional pass reports nothing.
        // Not ported: nothing here produces an incomplete type yet.
        FlowType { t: narrowed, incomplete: incoming.incomplete }
    }
    /// The converging loop-label walk — `Checker.getTypeAtFlowLoopLabel`
    /// (`internal/checker/flow.go:1325`). On-stack re-entry answers with the
    /// so-far union marked incomplete; a converged answer is cached. The
    /// residual divergences from upstream (the self-referential `any` bail,
    /// so-far under-accumulation) are priced in
    /// `docs/architecture/checker-notes-narrow.md` §12.6.
    fn get_type_at_flow_loop_label(&mut self, state: &mut FlowState, flow: FlowId) -> FlowType {
        // §839.3: the INITIAL TYPE is part of the key.
        //
        // Upstream never needs it, because `checkIdentifier` walks once and
        // uses the one answer for both the type and the uninitialized-variable
        // diagnostic. This port asks twice — the type road from the declared
        // type, `check_used_before_assigned` from `declared | undefined` — and
        // a two-element key `(flow, symbol)` lets the second walk's loop result
        // be replayed as the first's. Measured at **10 lines in
        // `parserindenter`**, a `while (parent != null && …)` over a
        // `var parent: ParseNode;`: the walk seeded with `| undefined` cached
        // the loop, the type query replayed it, and `parent` answered `any` at
        // nine sites plus one enclosing condition.
        let key = (
            tsr_core::index::Idx::index(flow),
            match state.symbol {
                Some(symbol) => symbol.index() as u64,
                None => (1 << 63) | u64::from(state.reference.as_u32()),
            },
            state.initial_type,
        );
        if let Some((cached, elements)) = self.flow_loop_cache.get(&key) {
            let (cached, elements) = (*cached, elements.clone());
            Self::replay_loop_elements(state, &elements);
            return FlowType { t: cached, incomplete: false };
        }
        // The on-stack answer requires a NON-EMPTY so-far list — an empty one
        // means an outer loop's back edge re-entered an inner loop mid-flight,
        // and upstream restarts that inner analysis rather than answering
        // (`flow.go:1347`); the restart terminates because a loop junction's
        // first antecedent is always the non-looping path. See
        // `checker-notes-narrow.md` §12.8.
        if let Some((_, types)) = self
            .flow_loop_stack
            .iter()
            .find(|(stacked, types)| *stacked == key && !types.is_empty())
        {
            let so_far = types.clone();
            return FlowType { t: self.get_union_type(&so_far), incomplete: true };
        }
        let antecedents: Vec<FlowId> = self.binder.flow().antecedents(flow).collect();
        let stack_index = self.flow_loop_stack.len();
        self.flow_loop_stack.push((key, Vec::new()));
        // §739: everything the antecedent walks contribute to `array_elements`
        // from here on is THIS label's element region — the slice a cache hit
        // must replay for queries that never walk the antecedents at all.
        let element_mark = state.array_elements.len();
        let outer_dedupe_mark = state.element_dedupe_mark;
        state.element_dedupe_mark = element_mark;
        let mut subtype_reduction = false;
        let mut first: Option<FlowType> = None;
        for antecedent in antecedents {
            let depth_mark = state.depth;
            let flow_type = if first.is_none() {
                let entry = self.get_type_at_flow_node(state, antecedent);
                first = Some(entry);
                entry
            } else {
                let shared_mark = self.shared_flows.len();
                let back = self.get_type_at_flow_node(state, antecedent);
                self.shared_flows.truncate(shared_mark);
                if let Some((cached, elements)) = self.flow_loop_cache.get(&key) {
                    let (cached, elements) = (*cached, elements.clone());
                    state.depth = depth_mark;
                    self.flow_loop_stack.truncate(stack_index);
                    state.element_dedupe_mark = outer_dedupe_mark;
                    // The restarted analysis completed elsewhere; its slice is
                    // a superset of what this partial walk accumulated, and
                    // the replay dedupes.
                    Self::replay_loop_elements(state, &elements);
                    return FlowType { t: cached, incomplete: false };
                }
                back
            };
            state.depth = depth_mark;
            if std::env::var("TSR_TRACE_LOOP").is_ok() {
                eprintln!(
                    "  LOOPANTE key={key:?} t={:?} incomplete={}",
                    flow_type.t, flow_type.incomplete
                );
            }
            let live = &mut self.flow_loop_stack[stack_index].1;
            if !live.contains(&flow_type.t) {
                live.push(flow_type.t);
            }
            if !self.is_type_subset_of(flow_type.t, state.initial_type) {
                subtype_reduction = true;
            }
            // Upstream breaks here unconditionally (`flow.go:1387`), on the
            // stated ground that *"the only possible outcome is subtypes that
            // will be removed in the final union type anyway"*.
            //
            // §736: **that break cannot fire on the array track upstream, and
            // it could here.** Upstream's flowing value on that track is an
            // EVOLVING ARRAY — a distinct type object — so `flowType.t ==
            // f.declaredType` is false however many antecedents agree. This
            // port has no evolving-array object and spells the unfinalized
            // array AS `state.declared_type`, which made the comparison true
            // and broke out of the antecedent loop before the back edge was
            // ever walked. The element pushed inside a loop body
            // (`while (cond()) { x.push("hello") }`) was therefore never
            // collected into `state.array_elements`.
            //
            // So this gate does not depart from upstream; it restores what
            // upstream does, from which the stand-in had silently diverged.
            // Worth +2 W→R over the corpus at zero adverse.
            if flow_type.t == state.declared_type && !state.is_auto_array {
                break;
            }
        }
        let types = self.flow_loop_stack[stack_index].1.clone();
        self.flow_loop_stack.truncate(stack_index);
        state.element_dedupe_mark = outer_dedupe_mark;
        // §744: the same lone-sentinel rule as the branch label — see there.
        if let [only] = types.as_slice()
            && self.store.get(*only).flags.contains(TypeFlags::NEVER)
        {
            let result = *only;
            let incomplete = first.is_some_and(|f| f.incomplete);
            if !incomplete {
                let contributed = state.array_elements[element_mark..].to_vec();
                self.flow_loop_cache.insert(key, (result, contributed));
            }
            return FlowType { t: result, incomplete };
        }
        let types: Vec<TypeId> = types
            .into_iter()
            .filter(|&t| !self.store.get(t).flags.contains(TypeFlags::NEVER))
            .collect();
        let result = if types.is_empty() {
            self.intrinsics.never
        } else if subtype_reduction {
            match self.union_with_subtype_reduction(&types) {
                Some(reduced) => reduced,
                None => state.declared_type,
            }
        } else {
            self.get_union_type(&types)
        };
        let result = self.recombine_unknown_type(result);
        let incomplete = first.is_some_and(|f| f.incomplete);
        if std::env::var("TSR_TRACE_LOOP").is_ok() {
            eprintln!(
                "LOOP key={key:?} types={types:?} subtype_red={subtype_reduction} result={result:?} incomplete={incomplete}"
            );
        }
        if incomplete {
            return FlowType { t: result, incomplete: true };
        }
        let contributed = state.array_elements[element_mark..].to_vec();
        self.flow_loop_cache.insert(key, (result, contributed));
        FlowType { t: result, incomplete: false }
    }

    /// §739: merge a cached loop label's element slice into the querying
    /// state — the accumulation the skipped antecedent walk would have done.
    fn replay_loop_elements(state: &mut FlowState, elements: &[TypeId]) {
        if !state.is_auto_array {
            return;
        }
        for &element in elements {
            if !state.array_elements[state.element_dedupe_mark..].contains(&element) {
                state.array_elements.push(element);
            }
        }
    }

    fn is_type_subset_of(&mut self, sub: TypeId, superset: TypeId) -> bool {
        if sub == superset || self.store.get(sub).flags.contains(TypeFlags::NEVER) {
            return true;
        }
        let super_constituents: Vec<TypeId> = match &self.store.get(superset).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![superset],
        };
        let sub_constituents: Vec<TypeId> = match &self.store.get(sub).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![sub],
        };
        sub_constituents.into_iter().all(|c| {
            let regular = self.get_regular_type_of_literal_type(c);
            super_constituents
                .iter()
                .any(|&s| s == c || self.get_regular_type_of_literal_type(s) == regular)
        })
    }

    /// The union of what every path into a junction says
    /// (`getTypeAtFlowBranchLabel`, `flow.go:363`).
    ///
    /// Upstream additionally tracks incompleteness across antecedents and
    /// short-circuits on `declaredType` once every constituent is present.
    /// Neither changes the answer here; both are performance and loop
    /// bookkeeping.
    fn get_type_at_flow_branch_label(
        &mut self,
        state: &mut FlowState,
        antecedents: Antecedents<'_>,
    ) -> FlowType {
        let mut types: Vec<TypeId> = Vec::new();
        let never = self.intrinsics.never;
        let mut subtype_reduction = false;
        for antecedent in antecedents {
            // `!c.isExhaustiveSwitchStatement(bypassFlow.…SwitchStatement)`
            // (`flow.go:1292`). The **bypass** antecedent is the path where the
            // `switch` matched no clause; when every value of the discriminant
            // is covered there is no such path, and its contribution must not
            // join the union. Without this,
            //
            // ```ts
            // let g: string;
            // switch (interval) {          // "day" | "week" | "month"
            //   case "day": g = "d"; break;
            //   case "week": g = "w"; break;
            //   case "month": g = "m"; break;
            // }
            // return g;                    // TS2454, wrongly
            // ```
            //
            // the bypass path carries the pre-switch `undefined` into the join
            // and every exhaustive switch reports "used before being assigned".
            if self.bypass_of_exhaustive_switch(antecedent) {
                continue;
            }
            let t = self.get_type_at_flow_node(state, antecedent).t;
            // `never` from a path means that path cannot reach here, so it
            // contributes nothing — which is what makes `if (x) {} else { throw }`
            // narrow after the `if`.
            if !types.contains(&t) {
                types.push(t);
            }
            // `flow.go:1280`/`:1298` (§749): an antecedent type that is not a
            // SUBSET of the initial type — a "foreign" type narrowed for the
            // reference, `LeadGuard` joining a declared `GuardInterface` —
            // switches the join to `UnionReductionSubtype`, exactly as the
            // loop label already does.
            if !self.is_type_subset_of(t, state.initial_type) {
                subtype_reduction = true;
            }
        }
        // §744: upstream appends EVERY antecedent type and hands the list to
        // `getUnionType`, which returns a lone type unchanged and drops
        // `never`-flagged members from a longer list. That is what lets a
        // join whose every path is cut off (`f30`'s trailing `x` in
        // `neverReturningFunctions1`) answer the `unreachableNeverType`
        // sentinel — converted to the declared type at the exit — instead
        // of a bare `never`.
        if let [only] = types.as_slice()
            && self.store.get(*only).flags.contains(TypeFlags::NEVER)
        {
            return FlowType { t: *only, incomplete: false };
        }
        let types: Vec<TypeId> = types
            .into_iter()
            .filter(|&t| !self.store.get(t).flags.contains(TypeFlags::NEVER))
            .collect();
        // §737: `getUnionOrEvolvingArrayType` (`flow.go:1314`) runs at every
        // junction, BEFORE the ordinary union — see
        // [`Checker::union_or_evolving_array`].
        if state.is_auto_array
            && let Some(t) = self.union_or_evolving_array(state, &types)
        {
            return FlowType { t, incomplete: false };
        }
        let t = if types.is_empty() {
            never
        } else if subtype_reduction && let Some(reduced) = self.union_with_subtype_reduction(&types)
        {
            // §749: the decidability-gated `removeSubtypes` the loop label
            // runs (`checker-notes-assign.md` §9); the heritage slice below
            // is the fallback when it declines.
            match &self.store.get(reduced).data {
                TypeData::Union { types: members, symbol: None, .. } => {
                    self.named_union_by_members.get(members).copied().unwrap_or(reduced)
                }
                _ => reduced,
            }
        } else {
            // §58 (`checker-notes-narrow.md`): a join re-forming a NAMED
            // union's exact member set answers the named type.
            // SS203: a flow join is upstream's `UnionReductionSubtype` site
            // (`getUnionType(antecedentTypes, UnionReductionSubtype)`), unlike
            // a WRITTEN union which reduces only literals. The general
            // `removeSubtypes` is refused (`bd tsr-eak`), but its DECLARED
            // heritage slice is decidable and already built for SS159's
            // narrowing lattice: at `switch (true) { case base instanceof
            // Derived1: /* fallthrough */ default: }` the join is
            // `Derived1 | Base`, and upstream prints `Base`.
            let types = {
                let mut kept: Vec<TypeId> = Vec::with_capacity(types.len());
                for &candidate in &types {
                    let subsumed = types.iter().any(|&other| {
                        other != candidate
                            && self.is_derived_from_decidable(candidate, other) == Some(true)
                    });
                    if !subsumed {
                        kept.push(candidate);
                    }
                }
                if kept.is_empty() { types } else { kept }
            };
            let joined = self.get_union_type(&types);
            match &self.store.get(joined).data {
                TypeData::Union { types: members, symbol: None, .. } => {
                    self.named_union_by_members.get(members).copied().unwrap_or(joined)
                }
                _ => joined,
            }
        };
        let t = self.recombine_unknown_type(t);
        FlowType { t, incomplete: false }
    }

    /// `isReachableFlowNode` (`flow.go:2513`): can control reach this flow
    /// node at all? §743.
    ///
    /// The first consumer is `functionHasImplicitReturn` (`checker.go:20307`,
    /// [`Checker::function_has_implicit_return`]), whose §741 stand-in was the
    /// statement-shaped [`Checker::block_completes_normally`] walk — a walk
    /// that answers `None` at every switch, loop, try and call in the way and
    /// so declined the strict-mode `| undefined` on every body with a call
    /// after its last `return`. Upstream reads the flow graph the binder
    /// already built; so does this.
    ///
    /// Upstream also keeps a one-entry `lastFlowNode`/`lastFlowNodeReachable`
    /// pair in front of the map. Not ported: it is a memo over the same
    /// answer the map holds, and the map is keyed on every SHARED node.
    pub(crate) fn is_reachable_flow_node(&mut self, flow: FlowId) -> bool {
        let mut reduce_labels: Vec<ReduceLabel> = Vec::new();
        self.is_reachable_flow_node_worker(&mut reduce_labels, flow, false)
    }

    /// `isReachableFlowNodeWorker` (`flow.go:2522`), arm for arm.
    fn is_reachable_flow_node_worker(
        &mut self,
        reduce_labels: &mut Vec<ReduceLabel>,
        mut flow: FlowId,
        mut no_cache_check: bool,
    ) -> bool {
        let binder = self.binder;
        loop {
            let flags = binder.flow().flags(flow);
            if flags.contains(FlowFlags::SHARED) {
                if !no_cache_check {
                    let key = tsr_core::index::Idx::index(flow);
                    if let Some(&reachable) = self.flow_node_reachable.get(&key) {
                        return reachable;
                    }
                    let reachable = self.is_reachable_flow_node_worker(reduce_labels, flow, true);
                    self.flow_node_reachable.insert(key, reachable);
                    return reachable;
                }
                no_cache_check = false;
            }
            if flags.intersects(
                FlowFlags::ASSIGNMENT | FlowFlags::CONDITION | FlowFlags::ARRAY_MUTATION,
            ) {
                match binder.flow().antecedent(flow) {
                    Some(next) => flow = next,
                    None => return false,
                }
            } else if flags.contains(FlowFlags::CALL) {
                if self.flow_call_ends_reachability(flow) {
                    return false;
                }
                match binder.flow().antecedent(flow) {
                    Some(next) => flow = next,
                    None => return false,
                }
            } else if flags.contains(FlowFlags::BRANCH_LABEL) {
                // A branching point is reachable if any branch is reachable.
                let antecedents = branch_label_antecedents(binder.flow(), flow, reduce_labels);
                for antecedent in antecedents {
                    if self.is_reachable_flow_node_worker(reduce_labels, antecedent, false) {
                        return true;
                    }
                }
                return false;
            } else if flags.contains(FlowFlags::LOOP_LABEL) {
                // A loop is reachable if the control flow path that leads to
                // the top is reachable.
                match binder.flow().antecedents(flow).next() {
                    Some(entry) => flow = entry,
                    None => return false,
                }
            } else if flags.contains(FlowFlags::SWITCH_CLAUSE) {
                // The control flow path representing an unmatched value in a
                // switch statement with no default clause is unreachable if
                // the switch statement is exhaustive.
                if self.bypass_of_exhaustive_switch(flow) {
                    return false;
                }
                match binder.flow().antecedent(flow) {
                    Some(next) => flow = next,
                    None => return false,
                }
            } else if flags.contains(FlowFlags::REDUCE_LABEL) {
                let Some(reduce) = binder.flow().reduce_label(flow) else { return false };
                let Some(antecedent) = binder.flow().antecedent(flow) else { return false };
                reduce_labels.push(reduce);
                let result = self.is_reachable_flow_node_worker(reduce_labels, antecedent, false);
                reduce_labels.pop();
                return result;
            } else {
                return !flags.contains(FlowFlags::UNREACHABLE);
            }
        }
    }

    /// The CALL arm of `isReachableFlowNodeWorker` (`flow.go:2541`): a call
    /// whose effects signature returns `never`, or asserts an argument that
    /// is literally false, ends every path through it.
    ///
    /// §746: `getEffectsSignature` proper decides which calls enter — the
    /// callee typed through `getTypeOfDottedName` (explicit types, no flow)
    /// for a statement-level call, which is what keeps this walk a
    /// read-only probe inside signature inference (§741's first cut
    /// re-entered the signature being inferred by typing the callee through
    /// `checkExpression`).
    fn flow_call_ends_reachability(&mut self, flow: FlowId) -> bool {
        let binder = self.binder;
        let Some(call_node) = binder.flow().node(flow) else { return false };
        let Some(Node::CallExpression(call)) = self.node_map.get(call_node) else { return false };
        let Some(signature) = self.get_effects_signature(call_node, call) else { return false };
        if let Some(predicate) = signature.predicate.as_ref()
            && predicate.asserts
            && predicate.r#type.is_none()
            && let Some(name) = predicate.parameter_name.as_deref()
            && let Some(index) = signature.parameters.iter().position(|p| p.name == name)
            && let Some(argument) = call.arguments.get(index)
            && self.is_false_expression(*argument)
        {
            return true;
        }
        self.store.get(signature.r#type).flags.contains(TypeFlags::NEVER)
    }

    /// `isFalseExpression` (`flow.go:2589`): `false`, `a && false`,
    /// `false && a`, `false || false`, through parentheses.
    fn is_false_expression(&self, expression: tsr_ast::Expression<'_>) -> bool {
        let Some(mut id) = expression.node_id() else { return false };
        // `ast.SkipParentheses`.
        while let Some(Node::ParenthesizedExpression(wrapper)) = self.node_map.get(id) {
            match wrapper.expression.and_then(|e| e.node_id()) {
                Some(inner) => id = inner,
                None => return false,
            }
        }
        match self.node_map.get(id) {
            Some(Node::KeywordExpression(keyword)) => keyword.kind == SyntaxKind::FalseKeyword,
            Some(Node::BinaryExpression(binary)) => {
                let (Some(left), Some(right), Some(operator)) =
                    (binary.left, binary.right, binary.operator_token)
                else {
                    return false;
                };
                match operator.kind {
                    SyntaxKind::AmpersandAmpersandToken => {
                        self.is_false_expression(left) || self.is_false_expression(right)
                    }
                    SyntaxKind::BarBarToken => {
                        self.is_false_expression(left) && self.is_false_expression(right)
                    }
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// Is this antecedent the **unmatched** edge of a `switch` that in fact
    /// covers every case?
    ///
    /// `data.ClauseStart == data.ClauseEnd && c.isExhaustiveSwitchStatement(…)`,
    /// which upstream asks in two places for the same reason —
    /// `getTypeAtFlowBranchLabel` (`flow.go:1292`) when joining, and
    /// `isReachableFlowNodeWorker` (`:2572`) when asking reachability. Both
    /// ask through this since §743 ported the walk.
    fn bypass_of_exhaustive_switch(&mut self, antecedent: FlowId) -> bool {
        let Some(clause) = self.binder.flow().switch_clause(antecedent) else { return false };
        // `is_empty()` is `ClauseStart == ClauseEnd` — the binder's own name for
        // "fell through every clause without matching".
        if !clause.is_empty() {
            return false;
        }
        self.is_exhaustive_switch_statement(clause.switch_statement)
    }

    /// `computeExhaustiveSwitchStatement`'s **literal** arm (`flow.go:1967`):
    ///
    /// ```go
    /// t := c.getBaseConstraintOrType(c.checkExpressionCached(node.Expression()))
    /// if !isLiteralType(t) { return false }
    /// switchTypes := c.getSwitchClauseTypes(node)
    /// if len(switchTypes) == 0 || core.Some(switchTypes, isNeitherUnitTypeNorNever) { return false }
    /// return c.eachTypeContainedIn(c.mapType(t, c.getRegularTypeOfLiteralType), switchTypes)
    /// ```
    ///
    /// # The `typeof` arm (§743)
    ///
    /// `switch (typeof x)` has its own road (`flow.go:1950-1966`) through
    /// `getNotEqualFactsFromTypeofSwitch` and the type-facts table. It was
    /// declined until the reachability walk started asking this predicate
    /// on every switch bypass edge, where "non-exhaustive" turned from a
    /// silent false positive into an appended `| undefined`
    /// (`narrowingByTypeofInSwitch`'s `switchOrdering`). Ported arm for arm;
    /// `getBaseConstraintOrType` is [`Checker::base_constraint_or_type`],
    /// which reads a constrained type parameter's constraint and nothing
    /// deeper.
    ///
    /// # Re-entrancy is guarded; the ANSWER is deliberately not memoised
    ///
    /// Upstream carries `exhaustiveState` — `Unknown`/`Computing`/`True`/`False`
    /// — on the switch's links (`flow.go:1933-1947`), which both guards
    /// re-entry *and* caches the result. Only the guard is ported, and the
    /// difference is measured rather than stylistic.
    ///
    /// Upstream reaches the discriminant through `checkExpressionCached`, so
    /// its first computation is normally **not** re-entrant and the cached
    /// answer is the real one. This port has no expression-type cache, so this
    /// function calls `check_expression` and, inside a loop, re-enters the very
    /// flow walk that called it:
    ///
    /// ```ts
    /// while (true) {
    ///     let key: string;
    ///     switch (unionVal) { case "A": { key = "AA"; break; } }
    ///     functionB(key);
    /// }
    /// ```
    ///
    /// Caching the first answer therefore freezes a value computed *during* a
    /// cycle. Measured: memoising cost `compiler/exhaustiveSwitchCheckCircularity`
    /// outright (32/32 → 30/32), where guarding without memoising loses no case
    /// at all. Recomputing costs a `check_expression` per query on a path that
    /// only runs for a switch with no `default`.
    ///
    /// **The falsifier**: if an expression-type cache ever lands, this should
    /// become upstream's three-valued state, and that case is the one to re-run.
    fn is_exhaustive_switch_statement(&mut self, switch: NodeId) -> bool {
        // Re-entry resolves to `false`, which is upstream's own resolution at
        // `flow.go:1944` and is also the conservative answer: a switch that
        // cannot be shown exhaustive keeps its bypass edge.
        if !self.exhaustive_switches.insert(switch) {
            return false;
        }
        let computed = self.compute_exhaustive_switch_statement(switch);
        self.exhaustive_switches.remove(&switch);
        computed
    }

    /// `computeExhaustiveSwitchStatement` (`flow.go:1949`), the literal arm.
    fn compute_exhaustive_switch_statement(&mut self, switch: NodeId) -> bool {
        let Some(Node::SwitchStatement(statement)) = self.node_map.get(switch) else {
            return false;
        };
        let Some(expression) = statement.expression else { return false };
        // §743: the `typeof` arm (`flow.go:1950-1966`), ported now that
        // `isReachableFlowNode` asks this predicate on every switch bypass
        // edge — a `typeof` switch that answered `false` here read as
        // "end reachable" and appended `| undefined` to `switchOrdering`
        // (`narrowingByTypeofInSwitch`, 1 R→W in the first scorepair).
        if let Some(Node::TypeOfExpression(typeof_node)) =
            expression.node_id().and_then(|id| self.node_map.get(id))
        {
            let Some(witnesses) = self.switch_clause_typeof_witnesses(statement) else {
                return false;
            };
            let Some(operand) = typeof_node.expression else { return false };
            let checked = self.check_expression(operand);
            let operand_constraint = self.base_constraint_or_type(checked);
            // Get the not-equal flags for all handled cases.
            let not_equal_facts = Self::not_equal_facts_from_typeof_switch(0, 0, &witnesses);
            if self.store.get(operand_constraint).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
                // We special case the top types to be exhaustive when all
                // cases are handled.
                return (TypeFacts::ALL_TYPEOF_NE & not_equal_facts) == TypeFacts::ALL_TYPEOF_NE;
            }
            // A missing not-equal flag indicates that the type wasn't
            // handled by some case.
            let constituents: Vec<TypeId> = match &self.store.get(operand_constraint).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![operand_constraint],
            };
            return !constituents
                .into_iter()
                .any(|t| (self.get_type_facts(t) & not_equal_facts) == not_equal_facts);
        }
        let checked = self.check_expression(expression);
        let discriminant = self.base_constraint_or_type(checked);
        //
        // `isLiteralType` — every constituent is a unit type.
        //
        // §743: a STRING-enum member written in TYPE position is a named
        // `OBJECT` mint carrying the member symbol (`declared.rs`, the
        // `qualified_type_reference` string-valued arm, kept for
        // `discriminatedUnionTypes4`'s sake), so `type YesNo = Choice.Yes |
        // Choice.No` declares two mints where upstream declares two enum
        // literal types — and the case expressions `Choice.Yes` ARE the
        // literal types. This test compares identities, so the mint is
        // un-spelled to the member type it stands for before the comparison
        // (`stringEnumLiteralTypes1/2` f10, 2 R→W in the first scorepair).
        let constituents: Vec<TypeId> = match &self.store.get(discriminant).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![discriminant],
        };
        let constituents: Vec<TypeId> =
            constituents.into_iter().map(|t| self.enum_member_behind_mint(t)).collect();
        if constituents.is_empty()
            || !constituents.iter().all(|&t| self.type_of(t).flags.intersects(TypeFlags::UNIT))
        {
            return false;
        }
        let Some(clause_types) = self.switch_clause_types(statement) else { return false };
        // `len == 0 || Some(isNeitherUnitTypeNorNever)`. A `default` clause
        // contributes `never` here, which is *allowed* — `isNeitherUnitTypeNorNever`
        // admits it — and a switch with a `default` has no bypass edge anyway.
        if clause_types.is_empty()
            || clause_types.iter().any(|&t| {
                t != self.intrinsics.never && !self.type_of(t).flags.intersects(TypeFlags::UNIT)
            })
        {
            return false;
        }
        // `eachTypeContainedIn(mapType(t, getRegularTypeOfLiteralType), switchTypes)`.
        constituents.into_iter().all(|constituent| {
            let regular = self.get_regular_type_of_literal_type(constituent);
            clause_types.contains(&regular)
        })
    }

    /// getBaseConstraintOrType (checker.go), using recursive semantic constraints.
    pub(crate) fn base_constraint_or_type(&mut self, t: TypeId) -> TypeId {
        self.base_constraint_of_type(t).unwrap_or(t)
    }

    /// The enum member type behind a string-enum qualified-reference mint,
    /// or the type unchanged. See the §743 note in
    /// [`Checker::compute_exhaustive_switch_statement`].
    fn enum_member_behind_mint(&mut self, t: TypeId) -> TypeId {
        let ty = self.store.get(t);
        if !ty.flags.contains(TypeFlags::OBJECT) {
            return t;
        }
        let TypeData::Named { members: Some(symbol), .. } = ty.data else { return t };
        if !self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::ENUM_MEMBER) {
            return t;
        }
        let declared = self.get_declared_type_of_enum_member(symbol);
        if declared == self.intrinsics.error {
            return t;
        }
        self.get_regular_type_of_literal_type(declared)
    }

    /// `getSwitchClauseTypeOfWitnesses` (`flow.go:1989`): one entry per
    /// clause — the string a `case` compares `typeof x` against, `None` for a
    /// `default` or a repeated string; the whole answer is `None` when any
    /// `case` is not a string literal.
    fn switch_clause_typeof_witnesses(
        &self,
        switch: &tsr_ast::SwitchStatement<'_>,
    ) -> Option<Vec<Option<String>>> {
        let case_block = switch.case_block?;
        let mut witnesses: Vec<Option<String>> = Vec::with_capacity(case_block.clauses.len());
        for clause in case_block.clauses {
            if clause.kind.kind != SyntaxKind::CaseKeyword {
                witnesses.push(None);
                continue;
            }
            let text = match clause
                .expression
                .and_then(|e| e.node_id())
                .and_then(|id| self.node_map.get(id))
            {
                Some(Node::StringLiteral(literal)) => literal.text,
                Some(Node::NoSubstitutionTemplateLiteral(literal)) => literal.text,
                _ => return None,
            };
            if witnesses.iter().any(|w| w.as_deref() == Some(text)) {
                witnesses.push(None);
            } else {
                witnesses.push(Some(text.to_owned()));
            }
        }
        Some(witnesses)
    }

    /// `getNotEqualFactsFromTypeofSwitch` (`flow.go:2012`): the combined
    /// not-equal facts for every witness outside `start..end`.
    fn not_equal_facts_from_typeof_switch(
        start: usize,
        end: usize,
        witnesses: &[Option<String>],
    ) -> TypeFacts {
        let mut facts = TypeFacts::empty();
        for (i, witness) in witnesses.iter().enumerate() {
            if (i < start || i >= end)
                && let Some(witness) = witness
            {
                facts |= Self::typeof_ne_facts(witness);
            }
        }
        facts
    }

    /// `typeofNEFacts` (`flow.go:635`), with `TypeofNEHostObject` for any
    /// string outside the table.
    fn typeof_ne_facts(witness: &str) -> TypeFacts {
        match witness {
            "string" => TypeFacts::TYPEOF_NE_STRING,
            "number" => TypeFacts::TYPEOF_NE_NUMBER,
            "bigint" => TypeFacts::TYPEOF_NE_BIG_INT,
            "boolean" => TypeFacts::TYPEOF_NE_BOOLEAN,
            "symbol" => TypeFacts::TYPEOF_NE_SYMBOL,
            "undefined" => TypeFacts::NE_UNDEFINED,
            "object" => TypeFacts::TYPEOF_NE_OBJECT,
            "function" => TypeFacts::TYPEOF_NE_FUNCTION,
            _ => TypeFacts::TYPEOF_NE_HOST_OBJECT,
        }
    }

    /// The declared type, or `never` when a junction has no way in at all.
    fn declared_or_never(&self, state: &FlowState) -> TypeId {
        let _ = self;
        state.declared_type
    }

    /// Whether `node` is a reference to the same thing the question is about
    /// (`isMatchingReference`, `flow.go:2085`).
    ///
    /// **Identifier-only, deliberately.** Upstream also matches `this`, `super`,
    /// property-access chains (`a.b.c`), element-access chains with literal
    /// indices, and the declaration nodes of variables and parameters. Each of
    /// those is a gap here.
    ///
    /// This is the one place in the module where being *loose* would be worse
    /// than being incomplete: a match that is too generous narrows the wrong
    /// reference and prints a plausible wrong type, where a match that is too
    /// strict simply leaves the declared type. So it asks the strictest question
    /// available — does this node resolve to the very symbol the reference
    /// resolved to.
    /// `getAccessedPropertyName` (`flow.go`), keyed on the argument's TYPE
    /// rather than its syntax. §693.
    ///
    /// The free `accessed_property_name` below answers an element access only
    /// for a written `StringLiteral`, so `foo[key]` with
    /// `const key = 'key' as const` gave `None` and could not be matched
    /// against `foo.key` — the pair fell to `(Some, None)` and then to
    /// `_ => false`, leaving `foo.key` unnarrowed inside `if (foo[key])`
    /// (`typeGuardNarrowsIndexedAccessOfKnownProperty2`/`4`). Upstream keys on
    /// the argument type being a string or number literal, which matches both
    /// spellings to the same name.
    fn accessed_property_name_at(&mut self, node: Node<'_>) -> Option<String> {
        if let Some(name) = accessed_property_name(node) {
            return Some(name);
        }
        let Node::ElementAccessExpression(access) = node else { return None };
        let argument = access.argument_expression?;
        // `tryGetElementAccessExpressionName` (`flow.go:1743`): beyond a
        // literal-like argument, only an entity name resolving to a CONSTANT
        // variable or an enum member names a property
        // (`tryGetNameFromEntityNameExpression`). A parameter narrowed to a
        // literal does not: `key = "a"; obj[key]` is not `obj.a`.
        let mut key_expression = argument;
        if !matches!(
            argument,
            tsr_ast::Expression::StringLiteral(_)
                | tsr_ast::Expression::NumericLiteral(_)
                | tsr_ast::Expression::NoSubstitutionTemplateLiteral(_)
        ) {
            let symbol = self.entity_name_expression_value_symbol(argument)?;
            if self.is_constant_variable(symbol) {
                // `tryGetNameFromEntityNameExpression` (`flow.go:1753`): the
                // declared annotation's literal, else the initializer's type
                // — only for a non-binding-element declaration declared before
                // this use, so a later `const` is never resolved from here.
                let declaration = self.binder.symbols().get(symbol).value_declaration?;
                if let Some(annotation) = self.type_annotation_of(declaration) {
                    let declared = self.get_type_from_type_node(annotation);
                    if let TypeData::StringLiteral(text) | TypeData::NumberLiteral(text) =
                        &self.store.get(declared).data
                    {
                        return Some(text.clone());
                    }
                }
                if self.nodes.kind(declaration) != SyntaxKind::VariableDeclaration
                    || !self
                        .is_block_scoped_name_declared_before_use(declaration, argument.node_id()?)
                {
                    return None;
                }
                key_expression = self.initializer_of(declaration)?;
            } else if !self.binder.symbols().get(symbol).flags.contains(SymbolFlags::ENUM_MEMBER) {
                return None;
            }
        }
        let key = self.check_expression(key_expression);
        match &self.store.get(key).data {
            // A numeric literal key names the same member as its text —
            // `a[0]` and `a["0"]` are one property — so both arms answer the
            // literal's text.
            TypeData::StringLiteral(text) | TypeData::NumberLiteral(text) => Some(text.clone()),
            _ => None,
        }
    }

    /// `resolveEntityName(node, SymbolFlagsValue, ignoreErrors)` for an
    /// entity-name expression: an identifier, or a dotted chain of them whose
    /// right names resolve through the merged exports of the left. Aliases
    /// are followed; anything else answers `None`.
    fn entity_name_expression_value_symbol(
        &mut self,
        expression: tsr_ast::Expression<'_>,
    ) -> Option<SymbolId> {
        let symbol = match expression {
            tsr_ast::Expression::Identifier(identifier) => self.binder.resolve_name(
                self.nodes,
                self.node_map,
                identifier.node_id?,
                identifier.text,
                SymbolFlags::VALUE,
            )?,
            tsr_ast::Expression::PropertyAccessExpression(access) => {
                let left = self.entity_name_expression_value_symbol(access.expression?)?;
                let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                    return None;
                };
                let left = self.binder.merged_symbol(left);
                *self.binder.symbols().get(left).exports.get(name.text)?
            }
            _ => return None,
        };
        Some(self.resolve_alias_fully(symbol))
    }

    /// `isEvolvingArrayOperationTarget` (`flow.go:1542`): the receiver of
    /// `.length`, `.push`/`.unshift`, or a numeric `x[i] = …` keeps the
    /// **unfinalized** evolving array, which prints `any[]`. §710.
    fn is_evolving_array_operation_target(&mut self, node: NodeId) -> bool {
        // §736: upstream opens with `root := c.getReferenceRoot(node)` and
        // tests the ROOT's parent, not the node's. The port tested the node's
        // parent directly, which is the same thing for every shape §710 met and
        // wrong for `f16`'s `(x = [], x).push(5)` — the reference is the right
        // operand of a comma inside parentheses, so its parent is the comma and
        // the `.push` is two levels further out.
        let node = self.get_reference_root(node);
        let Some(parent) = self.nodes.parent(node) else { return false };
        match self.node_map.get(parent) {
            Some(Node::PropertyAccessExpression(access)) => {
                let Some(tsr_ast::MemberName::Identifier(name)) = access.name else {
                    return false;
                };
                name.text == "length"
                    || (matches!(name.text, "push" | "unshift")
                        && self.nodes.parent(parent).is_some_and(|call| {
                            self.nodes.kind(call) == SyntaxKind::CallExpression
                        }))
            }
            Some(Node::ElementAccessExpression(access)) => {
                if access.expression.and_then(|e| e.node_id()) != Some(node) {
                    return false;
                }
                let Some(owner) = self.nodes.parent(parent) else { return false };
                let Some(Node::BinaryExpression(binary)) = self.node_map.get(owner) else {
                    return false;
                };
                binary.operator_token.is_some_and(|t| t.kind == SyntaxKind::EqualsToken)
                    && binary.left.and_then(|l| Node::from(l).node_id()) == Some(parent)
            }
            _ => false,
        }
    }

    /// `getReferenceRoot` (`flow.go:1876`), verbatim: a reference wrapped in
    /// parentheses, used as the LEFT operand of `=`, or used as the RIGHT
    /// operand of a comma, is rooted at the wrapper — those three forms all
    /// evaluate to the reference itself, so an operation on the wrapper is an
    /// operation on the reference.
    fn get_reference_root(&self, node: NodeId) -> NodeId {
        let Some(parent) = self.nodes.parent(node) else { return node };
        let climbs = match self.node_map.get(parent) {
            Some(Node::ParenthesizedExpression(_)) => true,
            Some(Node::BinaryExpression(binary)) => {
                let operator = binary.operator_token.map(|t| t.kind);
                let left = binary.left.and_then(|l| Node::from(l).node_id());
                let right = binary.right.and_then(|r| Node::from(r).node_id());
                (operator == Some(SyntaxKind::EqualsToken) && left == Some(node))
                    || (operator == Some(SyntaxKind::CommaToken) && right == Some(node))
            }
            _ => false,
        };
        if climbs { self.get_reference_root(parent) } else { node }
    }

    fn is_matching_reference(&mut self, state: &FlowState, node: NodeId) -> bool {
        if node == state.reference {
            return true;
        }
        if let Some(inner) = self.matching_reference_target(node) {
            return self.is_matching_reference(state, inner);
        }
        match state.symbol {
            // An identifier reference. The binder records an assignment's flow
            // node against the *target* node, which for `x = 1` is the
            // identifier `x` and for a declaration with an initialiser is the
            // declaration itself. Both answer through the symbol they declare
            // or resolve to, which is why this arm is symbol-based where the
            // one below is structural.
            Some(symbol) => {
                if let Some(candidate) = self.binder.symbol_of(node) {
                    return candidate == symbol;
                }
                if self.nodes.kind(node) != SyntaxKind::Identifier {
                    return false;
                }
                let Some(Node::Identifier(identifier)) = self.node_map.get(node) else {
                    return false;
                };
                self.binder
                    .resolve_name(
                        self.nodes,
                        self.node_map,
                        node,
                        identifier.text,
                        SymbolFlags::VALUE,
                    )
                    .is_some_and(|resolved| resolved == symbol)
            }
            // An access-expression reference: `a.b`, `this.x`, `a.b.c`.
            None => self.references_match(state.reference, node),
        }
    }

    /// Whether `node` matches a **left-hand part** of the reference —
    /// `containsMatchingReference` (`flow.go:1841`).
    ///
    /// For a reference `x.y.z` the parts are `x.y` and `x`. An assignment to
    /// either invalidates what was narrowed about the whole, which is why this
    /// is asked in [`Checker::get_type_at_flow_assignment`] on the *miss* path:
    /// the assignment is not to this reference, but it is to something the
    /// reference is built from.
    ///
    /// The reference itself is deliberately not included, matching upstream's
    /// split between this and `isOrContainsMatchingReference` — the caller has
    /// already tested that case.
    fn contains_matching_reference(&mut self, state: &FlowState, node: NodeId) -> bool {
        let mut source = state.reference;
        while let Some(receiver) = match self.node_map.get(source) {
            Some(
                access @ (Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_)),
            ) => access.expression_id(),
            _ => None,
        } {
            source = receiver;
            // Compared against the *sub-reference*, so this cannot reuse
            // `is_matching_reference`, which is fixed to `state.reference`.
            if self.references_match(source, node) {
                return true;
            }
            // An identifier part is also matched by the symbol the binder
            // recorded the assignment against — `obj = {}` records its flow
            // node on the declaration, not on an expression, which
            // `references_match`'s identifier arm handles only when the target
            // resolves. Asking the symbol directly is what makes an assignment
            // to a `let` visible here.
            if let (Some(Node::Identifier(identifier)), Some(assigned)) =
                (self.node_map.get(source), self.binder.symbol_of(node))
                && self
                    .binder
                    .resolve_name(
                        self.nodes,
                        self.node_map,
                        source,
                        identifier.text,
                        SymbolFlags::VALUE,
                    )
                    .is_some_and(|resolved| resolved == assigned)
            {
                return true;
            }
        }
        false
    }

    /// Whether two reference *expressions* denote the same thing
    /// (`isMatchingReference`, `flow.go`), for the forms this port can decide.
    ///
    /// # The arms, and the ones deliberately left out
    ///
    /// Ported: identifiers (by resolved symbol, and against a declaration
    /// through the symbol it declares), `this`, a parenthesised operand on
    /// either side, and the access arm — **the same accessed property name and
    /// a recursively matching receiver**, which is upstream's rule character
    /// for character.
    ///
    /// An **element access with an identifier argument** matches when both
    /// arguments resolve to one symbol that is a constant or an unassigned
    /// parameter/mutable local (`flow.go:1629`; the unassigned half landed with
    /// `checker-99-union-key-access.md`). Assignment/comma targets are unwrapped
    /// before source matching. Not ported: `super` and `MetaProperty`; both
    /// answer `false`, which costs a narrowing and never invents one.
    ///
    /// # Why `false` is the safe default here, unlike everywhere else
    ///
    /// This module's header records the one way narrowing produces a *wrong*
    /// answer rather than a gap: a match that is too **loose** narrows the
    /// wrong reference. So every unported arm answers `false` and every ported
    /// arm is an equality rather than a heuristic — an element access whose
    /// argument this port cannot prove constant is refused rather than matched
    /// on its text.
    pub(crate) fn references_match(&mut self, source: NodeId, target: NodeId) -> bool {
        if source == target {
            // Native query identifiers match runtime ThisKeyword only; a
            // qualified query is likewise not an access-expression target.
            // Keep the shortcut for every ordinary reference.
            let mut root = source;
            while let Some(Node::QualifiedName(name)) = self.node_map.get(root) {
                let Some(left) = name.left.and_then(|left| left.node_id()) else { break };
                root = left;
            }
            if self.is_this_in_type_query(root) {
                return false;
            }
            return true;
        }
        // `flow.go`'s two switches, in full. §844: this block previously
        // listed `KindParenthesizedExpression` alone on each side, and each
        // switch names more than that:
        //
        // ```go
        // switch target.Kind {
        // case ast.KindParenthesizedExpression, ast.KindNonNullExpression:
        //     return c.isMatchingReference(source, target.Expression())
        // ...
        // switch source.Kind {
        // case ast.KindNonNullExpression, ast.KindParenthesizedExpression, ast.KindSatisfiesExpression:
        //     return c.isMatchingReference(source.Expression(), target)
        // ```
        //
        // So `(a.b)`, `a.b!` and `a.b satisfies T` are all the same reference
        // as `a.b`. `nonNullReferenceMatching` is the case: a guard written
        // `typeof this.props.thumbYProps!.elementRef === 'function'` and a use
        // written the same way did not match, because the `!` stopped the walk
        // in the middle of the dotted path.
        //
        // `SatisfiesExpression` is source-side only, exactly as upstream has
        // it — not symmetric, and not tidied.
        let strip_source = |checker: &Self, id: NodeId| -> Option<NodeId> {
            match checker.node_map.get(id)? {
                Node::ParenthesizedExpression(node) if !checker.is_jsdoc_type_assertion(id) => {
                    node.expression.and_then(|e| e.node_id())
                }
                Node::NonNullExpression(node) => node.expression.and_then(|e| e.node_id()),
                Node::SatisfiesExpression(node) => node.expression.and_then(|e| e.node_id()),
                _ => None,
            }
        };
        // Native applies the target switch before the source switch. In
        // particular `(x = rhs)` and `(effect(), x)` match x on both the
        // identifier and structural-access roads (flow.go:1597).
        if let Some(inner) = self.matching_reference_target(target) {
            return self.references_match(source, inner);
        }
        if let Some(inner) = strip_source(self, source) {
            return self.references_match(inner, target);
        }

        match (self.node_map.get(source), self.node_map.get(target)) {
            // `this` matches `this` and nothing else.
            (Some(Node::KeywordExpression(left)), Some(Node::KeywordExpression(right))) => {
                left.kind == SyntaxKind::ThisKeyword && right.kind == SyntaxKind::ThisKeyword
            }
            // §840: a QUALIFIED NAME matches the property-access spelling of
            // the same dotted path. `isMatchingReference`
            // (`vendor/typescript-go/internal/checker/flow.go:1639-1643`):
            //
            // ```go
            // case ast.KindQualifiedName:
            //     if ast.IsAccessExpression(target) {
            //         if targetPropertyName, ok := c.getAccessedPropertyName(target); ok {
            //             return source.AsQualifiedName().Right.Text() == targetPropertyName &&
            //                 c.isMatchingReference(source.AsQualifiedName().Left, target.Expression())
            //         }
            //     }
            // ```
            //
            // The two spellings are the same reference written in the two
            // grammars: `typeof properties.foo` parses its path as a
            // `QualifiedName` because it is in a type position, while the guard
            // `if (properties.foo)` above it is a `PropertyAccessExpression`.
            // The binder already records a flow node for the qualified form
            // (`binder.rs`, gated on `is_part_of_type_query` exactly as
            // `binder.go:605-608` gates it), so the walk reaches the guard and
            // was declining to apply it for want of this arm alone.
            (Some(Node::QualifiedName(qualified)), Some(target)) => {
                let (Some(left), Some(name)) = (qualified.left, qualified.right) else {
                    return false;
                };
                let Some(target_name) = self.accessed_property_name_at(target) else {
                    return false;
                };
                if name.text != target_name {
                    return false;
                }
                let left_id = match left {
                    tsr_ast::EntityName::Identifier(identifier) => identifier.node_id,
                    tsr_ast::EntityName::QualifiedName(inner) => inner.node_id,
                };
                let (Some(left_id), Some(target_left)) = (left_id, target.expression_id()) else {
                    return false;
                };
                self.references_match(left_id, target_left)
            }
            // Two accesses: same property name, matching receivers. The name
            // comparison is on the **member name text**, which is what
            // `getAccessedPropertyName` answers for a property access.
            (
                Some(left @ (Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_))),
                Some(right),
            ) => {
                let names_match = match (
                    self.accessed_property_name_at(left),
                    self.accessed_property_name_at(right),
                ) {
                    (Some(left_name), Some(right_name)) => left_name == right_name,
                    // §423: `a[i]` matches `a[i]` when `i` is a CONST — the
                    // arm the doc above refused pending `isSymbolAssigned`;
                    // a `const` needs no assignment analysis (upstream's
                    // isMatchingReference element arm:
                    // same argument symbol + isConstantVariable).
                    // `foo[index] !== undefined` narrows `foo[index]`
                    // (`typeGuardNarrowsIndexedAccessOfKnownProperty3`).
                    (None, None) => {
                        let argument = |checker: &mut Self, node: &Node<'_>| -> Option<SymbolId> {
                            let Node::ElementAccessExpression(access) = node else {
                                return None;
                            };
                            let Some(tsr_ast::Expression::Identifier(identifier)) =
                                access.argument_expression
                            else {
                                return None;
                            };
                            let id = identifier.node_id?;
                            let symbol = checker.binder.resolve_name(
                                checker.nodes,
                                checker.node_map,
                                id,
                                identifier.text,
                                SymbolFlags::VALUE,
                            )?;
                            // The full predicate (`flow.go:1629`):
                            // `isConstantVariable(symbol) ||
                            // isParameterOrMutableLocalVariable(symbol) &&
                            // !isSymbolAssigned(symbol)` — `obj[key]` with an
                            // unassigned parameter `key` is one reference.
                            if checker.is_constant_variable(symbol) {
                                return Some(symbol);
                            }
                            if !checker.is_parameter_or_mutable_local_variable(symbol) {
                                return None;
                            }
                            checker.ensure_assignments_marked(symbol);
                            (!checker.last_assignment_pos.contains_key(&symbol)).then_some(symbol)
                        };
                        let left_argument = argument(self, &left);
                        left_argument.is_some() && left_argument == argument(self, &right)
                    }
                    _ => false,
                };
                if !names_match {
                    return false;
                }
                let (Some(left_receiver), Some(right_receiver)) =
                    (left.expression_id(), right.expression_id())
                else {
                    return false;
                };
                self.references_match(left_receiver, right_receiver)
            }
            // An identifier against an identifier, or against the declaration
            // the binder recorded the flow node on.
            (Some(Node::Identifier(identifier)), _) => {
                if self.is_this_in_type_query(source) {
                    return self.nodes.kind(target) == SyntaxKind::ThisKeyword;
                }
                let Some(resolved) = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    source,
                    identifier.text,
                    SymbolFlags::VALUE,
                ) else {
                    return false;
                };
                if let Some(declared) = self.binder.symbol_of(target) {
                    return declared == resolved;
                }
                let Some(Node::Identifier(other)) = self.node_map.get(target) else {
                    return false;
                };
                self.binder
                    .resolve_name(self.nodes, self.node_map, target, other.text, SymbolFlags::VALUE)
                    .is_some_and(|candidate| candidate == resolved)
            }
            _ => false,
        }
    }

    /// Native ast.IsJSDocTypeAssertion detects a reparsed `AsExpression` inside
    /// parentheses (ast/utilities.go:759). Our parser stores its type in the
    /// side table instead. Look there first to avoid a root walk for ordinary
    /// parentheses in this flow hot path.
    fn is_jsdoc_type_assertion(&self, node: NodeId) -> bool {
        self.nodes.kind(node) == SyntaxKind::ParenthesizedExpression
            && self.jsdoc_cast_annotation(node).is_some()
            && self.in_js_file(node)
    }

    /// The target-side wrapper switch of `isMatchingReference`
    /// (flow.go:1598). Unlike getReferenceCandidate, all compound assignments
    /// match their left reference, and non-null wrappers are transparent.
    fn matching_reference_target(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::ParenthesizedExpression(inner) => {
                // Native reparses a JS @type cast as an AsExpression inside
                // the parentheses (ast/utilities.go:759). This port keeps its
                // type in the side table; preserve that assertion boundary.
                if self.is_jsdoc_type_assertion(node) { None } else { inner.expression?.node_id() }
            }
            Node::NonNullExpression(inner) => inner.expression?.node_id(),
            Node::BinaryExpression(binary) => {
                let operator = binary.operator_token?.kind;
                if operator == SyntaxKind::CommaToken {
                    binary.right?.node_id()
                } else if operator.is_assignment_operator()
                    && is_left_hand_side_expression(binary.left?)
                {
                    binary.left?.node_id()
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    /// `getReferenceCandidate` (flow.go:1861): normalize the value/reference
    /// operands used by equality, typeof, instanceof and in guards. Ordinary
    /// arithmetic compound assignments and non-null wrappers stay intact.
    fn get_reference_candidate(&self, mut node: NodeId) -> NodeId {
        loop {
            let next = match self.node_map.get(node) {
                Some(Node::ParenthesizedExpression(inner)) => {
                    if self.is_jsdoc_type_assertion(node) {
                        return node;
                    }
                    inner.expression
                }
                Some(Node::BinaryExpression(binary)) => match binary.operator_token.map(|t| t.kind)
                {
                    Some(
                        SyntaxKind::EqualsToken
                        | SyntaxKind::BarBarEqualsToken
                        | SyntaxKind::AmpersandAmpersandEqualsToken
                        | SyntaxKind::QuestionQuestionEqualsToken,
                    ) => binary.left,
                    Some(SyntaxKind::CommaToken) => binary.right,
                    _ => None,
                },
                _ => None,
            }
            .and_then(|expr| expr.node_id());
            let Some(next) = next else { return node };
            node = next;
        }
    }

    /// Narrow `t` by a condition expression known to be true or false
    /// (`narrowType`, `flow.go:2226`).
    ///
    /// The unported forms return `t` unchanged, which is upstream's own default
    /// arm — see this module's header for why that makes a partial port safe.
    /// Not ported: `typeof` guards, discriminant comparisons, `instanceof`,
    /// `in`, and user-defined type predicates. Each needs machinery this
    /// checker does not have yet (comparability, call resolution), and each
    /// currently leaves the declared type.
    ///
    /// **Equality is ported for a nullable operand only** — see
    /// [`Checker::narrow_type_by_equality`], which returns `t` unchanged for
    /// the comparability half. And every arm here is additionally limited by
    /// [`Checker::is_matching_reference`] being identifier-only: `a.b !==
    /// undefined` narrows nothing, because this port has no flow reference for
    /// a property access. That is the largest bound on what any guard here can
    /// convert, and it is measured — `docs/architecture/checker-notes-narrow.md`
    /// §4.
    /// `getTypeAtSwitchClause` (`flow.go:1059`). All four of upstream's arms
    /// are ported: the identifier discriminant and the `typeof` witness (§16),
    /// `switch (true)` (§59), and the DEFAULT arm's optional-chain containment
    /// plus discriminant-property road (§754, through the §750 pair). The one
    /// containment variant not transcribed is the `typeof`-of-a-chain form
    /// (`flow.go:1077`-`:1080`), whose clause check differs; it declines here.
    /// See `checker-notes-narrow.md` §16 and `checker-notes-callres.md` §754.
    fn get_type_at_switch_clause(&mut self, state: &mut FlowState, flow: FlowId) -> FlowType {
        let binder = self.binder;
        let Some(antecedent) = binder.flow().antecedent(flow) else {
            return FlowType { t: state.declared_type, incomplete: false };
        };
        let incoming = self.get_type_at_flow_node(state, antecedent);
        let Some(clause) = binder.flow().switch_clause(flow) else {
            return incoming;
        };
        let Some(Node::SwitchStatement(switch)) = self.node_map.get(clause.switch_statement) else {
            return incoming;
        };
        let Some(mut expr) = switch.expression else { return incoming };
        // A parenthesized expression in a JS file can be a JSDoc cast
        // (`isJSDocTypeAssertion`), which upstream deliberately does NOT
        // look through (`parenthesizedJSDocCastDoesNotNarrow`) — declining
        // the whole skip in JS files is the conservative containment.
        if !self.in_js_file(state.reference) {
            while let tsr_ast::Expression::ParenthesizedExpression(inner) = expr {
                let Some(next) = inner.expression else { return incoming };
                expr = next;
            }
        }
        // §59 (`checker-notes-narrow.md`): `switch (true)` — the clause
        // expressions are conditions (`narrowTypeBySwitchOnTrue`).
        if let tsr_ast::Expression::KeywordExpression(keyword) = expr
            && keyword.kind == SyntaxKind::TrueKeyword
        {
            let narrowed = self.narrow_type_by_switch_on_true(state, incoming.t, switch, &clause);
            return FlowType { t: narrowed, incomplete: incoming.incomplete };
        }
        let narrowed = if expr.node_id().is_some_and(|id| self.is_matching_reference(state, id)) {
            self.narrow_type_by_switch_on_discriminant(incoming.t, switch, &clause)
        } else if let tsr_ast::Expression::TypeOfExpression(type_of) = expr
            && type_of
                .expression
                .and_then(|e| e.node_id())
                .is_some_and(|id| self.is_matching_reference(state, id))
        {
            self.narrow_type_by_switch_on_typeof(incoming.t, switch, &clause)
        } else {
            // §754 (`flow.go:1071`-`:1087`), upstream's DEFAULT arm. The
            // optional-chain containment ASSIGNS and falls through to the
            // discriminant road — it does not return. This port returned,
            // which is the third arm found with that shape (§51.5 at the
            // value-equality arm, §753 at the typeof arm), and it is why
            // `switch (o?.kind)` stripped the `undefined` and then declined
            // to discriminate.
            let mut t = incoming.t;
            // SS154: the clause range excluding undefined (and any default)
            // proves the chain result defined, so the BASE strips
            // undefined/null (`narrowTypeBySwitchOptionalChainContainment`,
            // `flow.go:1223`).
            if self.strict_null_checks {
                if expr
                    .node_id()
                    .is_some_and(|id| self.optional_chain_contains_reference(state, id))
                {
                    if !self.switch_clause_range_covers_nullish(switch, &clause) {
                        t = self.get_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL);
                    }
                } else if let tsr_ast::Expression::TypeOfExpression(type_of) = expr
                    && type_of
                        .expression
                        .and_then(|e| e.node_id())
                        .is_some_and(|id| self.optional_chain_contains_reference(state, id))
                    && !self.switch_clause_range_spells_undefined(switch, &clause)
                {
                    // §761 (`flow.go:1077`-`:1080`), §754's residue: the
                    // SECOND containment variant. `switch (typeof o?.x)`
                    // proves the chain defined when no clause in the range is
                    // the STRING `"undefined"` — a different clause check from
                    // the direct form's, which looks for a nullish clause TYPE.
                    t = self.get_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL);
                }
            }
            // `flow.go:1083`-`:1086`: `switch (s.kind)` through the pair,
            // replacing §51's inline property-access test. The inner
            // narrowing is `narrowTypeBySwitchOnDiscriminant` applied to the
            // PROPERTY type. `narrowTypeBySwitchOnDiscriminantProperty`'s
            // key-property fast path (`flow.go:1232`-`:1245`) is not ported,
            // for the reason §752 records.
            if let Some(id) = expr.node_id()
                && let Some(access) = self.get_discriminant_property_access(state, id, t)
            {
                t = self.narrow_type_by_discriminant(t, access, |checker, prop| {
                    checker.narrow_type_by_switch_on_discriminant(prop, switch, &clause)
                });
            }
            t
        };
        FlowType { t: narrowed, incomplete: incoming.incomplete }
    }

    /// `narrowTypeBySwitchOnTrue` (`flow.go:1129` region): prior clauses
    /// refute, the current set asserts per clause and unions, a default in
    /// the set skips the assert half but still refutes later clauses
    /// (`checker-notes-narrow.md` §59).
    fn narrow_type_by_switch_on_true(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        switch: &tsr_ast::SwitchStatement<'_>,
        clause: &tsr_binder::SwitchClause,
    ) -> TypeId {
        let Some(block) = switch.case_block else { return t };
        let clauses = block.clauses;
        let clause_start = clause.clause_start as usize;
        let clause_end = clause.clause_end as usize;
        // The side-table node kind, NOT the token field — the §16
        // CaseKeyword trap's third application.
        let is_case = |checker: &Self, c: &tsr_ast::CaseOrDefaultClause<'_>| {
            c.node_id.is_some_and(|id| checker.nodes.kind(id) == SyntaxKind::CaseClause)
        };
        let default_index = clauses.iter().position(|c| !is_case(self, c));
        let has_default = clause_start == clause_end
            || default_index.is_some_and(|d| d >= clause_start && d < clause_end);
        let mut narrowed = t;
        for c in clauses.iter().take(clause_start.min(clauses.len())) {
            if is_case(self, c)
                && let Some(expression) = c.expression
                && let Some(id) = expression.node_id()
            {
                narrowed = self.narrow_type(state, narrowed, id, false);
            }
        }
        if has_default {
            for c in clauses.iter().skip(clause_end) {
                if is_case(self, c)
                    && let Some(expression) = c.expression
                    && let Some(id) = expression.node_id()
                {
                    narrowed = self.narrow_type(state, narrowed, id, false);
                }
            }
            return narrowed;
        }
        let range: Vec<TypeId> = clauses
            .iter()
            .skip(clause_start)
            .take(clause_end.saturating_sub(clause_start))
            .map(|c| {
                if is_case(self, c)
                    && let Some(id) = c.expression.and_then(|e| e.node_id())
                {
                    self.narrow_type(state, narrowed, id, true)
                } else {
                    narrowed
                }
            })
            .collect();
        if range.is_empty() {
            return narrowed;
        }
        // The §51 kept-all lesson at the clause union: identical branches
        // keep the original spelling.
        if range.iter().all(|&r| r == range[0]) {
            return range[0];
        }
        self.get_union_type(&range)
    }

    /// `narrowTypeBySwitchOnDiscriminant` (`flow.go:1092`), the
    /// comparable-filter path. Declines whole — answers `t` unchanged — when
    /// any clause type is missing, non-unit, or a comparability the relation
    /// cannot decide, so the failure mode is the unnarrowed status quo. The
    /// unknown-ground path is unported (stated in §16).
    /// SS154: does this clause range possibly cover a nullish discriminant -
    /// a default clause, an unreadable clause list, or any nullable-flagged
    /// clause type? `false` licenses the chain-base strip.
    fn switch_clause_range_covers_nullish(
        &mut self,
        switch: &tsr_ast::SwitchStatement<'_>,
        clause: &tsr_binder::SwitchClause,
    ) -> bool {
        let Some(clause_types) = self.switch_clause_types(switch) else {
            return true;
        };
        let (start, end) = (clause.clause_start as usize, clause.clause_end as usize);
        let slice = &clause_types[start.min(clause_types.len())..end.min(clause_types.len())];
        if start == end || slice.contains(&self.intrinsics.never) {
            return true;
        }
        slice
            .iter()
            .any(|&clause_type| self.store.get(clause_type).flags.intersects(TypeFlags::NULLABLE))
    }

    /// The clause check of `narrowTypeBySwitchOptionalChainContainment`'s
    /// TYPEOF variant (`flow.go:1078`-`:1079`). §761.
    ///
    /// Upstream's predicate is `!(never || the string literal "undefined")`
    /// applied to EVERY clause type in the range; this answers its negation —
    /// "some clause could be undefined" — so the caller reads the same way the
    /// direct variant's [`Checker::switch_clause_range_covers_nullish`] does.
    /// An empty range is upstream's implicit-fallthrough case and declines.
    fn switch_clause_range_spells_undefined(
        &mut self,
        switch: &tsr_ast::SwitchStatement<'_>,
        clause: &tsr_binder::SwitchClause,
    ) -> bool {
        let Some(clause_types) = self.switch_clause_types(switch) else {
            return true;
        };
        let (start, end) = (clause.clause_start as usize, clause.clause_end as usize);
        let slice = &clause_types[start.min(clause_types.len())..end.min(clause_types.len())];
        if start == end {
            return true;
        }
        slice.iter().any(|&clause_type| {
            let ty = self.store.get(clause_type);
            ty.flags.contains(TypeFlags::NEVER)
                || (ty.flags.intersects(TypeFlags::STRING_LITERAL)
                    && matches!(&ty.data, TypeData::StringLiteral(text) if text == "undefined"))
        })
    }

    fn narrow_type_by_switch_on_discriminant(
        &mut self,
        t: TypeId,
        switch: &tsr_ast::SwitchStatement<'_>,
        clause: &tsr_binder::SwitchClause,
    ) -> TypeId {
        let Some(clause_types) = self.switch_clause_types(switch) else { return t };
        if clause_types.is_empty() {
            return t;
        }
        let (start, end) = (clause.clause_start as usize, clause.clause_end as usize);
        let slice = &clause_types[start.min(clause_types.len())..end.min(clause_types.len())];
        let has_default = start == end || slice.contains(&self.intrinsics.never);
        if self.store.get(t).flags.intersects(TypeFlags::UNKNOWN) {
            if has_default {
                return t;
            }
            // Pinned 5b1047d, flow.go:1102-1124. The existing switch-node
            // cache supplies regular clause identities; unknown narrows only
            // when the whole range is grounded. Object values select the
            // canonical nonPrimitive type, not their specific object image.
            let mut grounded = Vec::with_capacity(slice.len());
            for &clause_type in slice {
                let flags = self.store.get(clause_type).flags;
                grounded.push(
                    if flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE) {
                        clause_type
                    } else if flags.intersects(TypeFlags::OBJECT) {
                        self.intrinsics.non_primitive
                    } else {
                        return t;
                    },
                );
            }
            return self.get_union_type(&grounded);
        }
        let discriminant = self.get_union_type(slice);
        let case_type = if self.store.get(discriminant).flags.intersects(TypeFlags::NEVER) {
            self.intrinsics.never
        } else {
            // The filter needs a decidable comparability per constituent; an
            // Unknown pair declines the whole narrowing.
            let constituents: Vec<TypeId> = match &self.store.get(t).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![t],
            };
            let mut kept = Vec::new();
            for constituent in constituents {
                match self.comparable_ternary(discriminant, constituent) {
                    Some(true) => kept.push(constituent),
                    Some(false) => {}
                    None => return t,
                }
            }
            let filtered = self.get_union_type(&kept);
            self.replace_primitives_with_literals(filtered, discriminant)
        };
        if !has_default {
            return case_type;
        }
        // The default half filters away every unit type another clause
        // handles (`flow.go:1139`).
        let all_types = clause_types.clone();
        let default_type = self.filter_switch_default(t, &all_types);
        let Some(default_type) = default_type else { return t };
        if self.store.get(case_type).flags.intersects(TypeFlags::NEVER) {
            return default_type;
        }
        self.get_union_type(&[case_type, default_type])
    }

    /// The default-clause filter of `narrowTypeBySwitchOnDiscriminant`
    /// (`flow.go:1139`): keep constituents that are not unit types some
    /// other clause already handles. `None` declines (an undecidable pair).
    fn filter_switch_default(&mut self, t: TypeId, switch_types: &[TypeId]) -> Option<TypeId> {
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let mut kept = Vec::new();
        for constituent in constituents {
            let flags = self.store.get(constituent).flags;
            if !flags.intersects(TypeFlags::UNIT) {
                kept.push(constituent);
                continue;
            }
            let unit = if flags.intersects(TypeFlags::UNDEFINED) {
                self.intrinsics.undefined
            } else {
                self.get_regular_type_of_literal_type(constituent)
            };
            let mut handled = false;
            for &switch_type in switch_types {
                if !self.store.get(switch_type).flags.intersects(TypeFlags::UNIT) {
                    continue;
                }
                match self.comparable_ternary(switch_type, unit) {
                    Some(true) => {
                        handled = true;
                        break;
                    }
                    Some(false) => {}
                    None => return None,
                }
            }
            if !handled {
                kept.push(constituent);
            }
        }
        Some(self.get_union_type(&kept))
    }

    /// `narrowTypeBySwitchOnTypeOf` (`flow.go:1157`): the per-clause string
    /// witnesses, unioned for a case range, NE-fact-filtered for a default.
    fn narrow_type_by_switch_on_typeof(
        &mut self,
        t: TypeId,
        switch: &tsr_ast::SwitchStatement<'_>,
        clause: &tsr_binder::SwitchClause,
    ) -> TypeId {
        let Some(case_block) = switch.case_block else { return t };
        let clauses = case_block.clauses;
        // `getSwitchClauseTypeOfWitnesses` (`flow.go:1989`): every case must
        // be a string literal or the whole switch yields no witnesses. A
        // repeated text leaves the later occurrence empty.
        let mut witnesses: Vec<Option<&str>> = Vec::with_capacity(clauses.len());
        for case in clauses {
            if case.kind.kind == SyntaxKind::CaseKeyword {
                let text = match case.expression {
                    Some(tsr_ast::Expression::StringLiteral(literal)) => literal.text,
                    Some(tsr_ast::Expression::NoSubstitutionTemplateLiteral(literal)) => {
                        literal.text
                    }
                    _ => return t,
                };
                if witnesses.contains(&Some(text)) {
                    witnesses.push(None);
                } else {
                    witnesses.push(Some(text));
                }
            } else {
                witnesses.push(None);
            }
        }
        let (start, end) = (clause.clause_start as usize, clause.clause_end as usize);
        let default_index =
            clauses.iter().position(|case| case.kind.kind == SyntaxKind::DefaultKeyword);
        let has_default =
            start == end || default_index.is_some_and(|index| index >= start && index < end);
        if has_default {
            // `getNotEqualFactsFromTypeofSwitch` (`flow.go:2012`).
            let mut facts = TypeFacts::empty();
            for (index, witness) in witnesses.iter().enumerate() {
                if (index < start || index >= end)
                    && let Some(text) = witness
                {
                    facts |= match *text {
                        "string" => TypeFacts::TYPEOF_NE_STRING,
                        "number" => TypeFacts::TYPEOF_NE_NUMBER,
                        "bigint" => TypeFacts::TYPEOF_NE_BIG_INT,
                        "boolean" => TypeFacts::TYPEOF_NE_BOOLEAN,
                        "symbol" => TypeFacts::TYPEOF_NE_SYMBOL,
                        "undefined" => TypeFacts::NE_UNDEFINED,
                        "object" => TypeFacts::TYPEOF_NE_OBJECT,
                        "function" => TypeFacts::TYPEOF_NE_FUNCTION,
                        _ => TypeFacts::TYPEOF_NE_HOST_OBJECT,
                    };
                }
            }
            return self.filter_type(t, |checker, constituent| {
                checker.get_type_facts(constituent).contains(facts)
            });
        }
        let range: Vec<Option<String>> = witnesses
            [start.min(witnesses.len())..end.min(witnesses.len())]
            .iter()
            .map(|w| w.map(str::to_string))
            .collect();
        let mut arms = Vec::with_capacity(range.len());
        for witness in range {
            let arm = match witness {
                Some(text) => self.narrow_type_by_typeof_literal(t, &text, true),
                None => self.intrinsics.never,
            };
            arms.push(arm);
        }
        self.get_union_type(&arms)
    }

    /// Per-clause case-expression types — `getSwitchClauseTypes`
    /// (`flow.go:2026`); a default clause contributes `never`. `None` when
    /// any case expression fails to type (the §16 decline).
    fn switch_clause_types(
        &mut self,
        switch: &tsr_ast::SwitchStatement<'_>,
    ) -> Option<Vec<TypeId>> {
        let case_block = switch.case_block?;
        let mut types = Vec::with_capacity(case_block.clauses.len());
        for case in case_block.clauses {
            if case.kind.kind == SyntaxKind::CaseKeyword {
                let expression = case.expression?;
                let checked = self.check_expression(expression);
                if checked == self.intrinsics.error {
                    return None;
                }
                let regular = self.get_regular_type_of_literal_type(checked);
                types.push(regular);
            } else {
                types.push(self.intrinsics.never);
            }
        }
        Some(types)
    }

    /// A decidable `areTypesComparable` for switch narrowing: literal and
    /// primitive pairs answer definitely; anything structural is `None`,
    /// which declines the narrowing whole. `checker.go`'s comparable
    /// relation is not ported; this is its unit-type fragment.
    /// `true`/`false` when `member` is an enum member type and `literal` is a
    /// unit literal, `None` when the pair is not that shape. §666.
    /// §751: the `(enum symbol, value key)` an enum MEMBER type was interned
    /// under (`enum_value_types`, §55), read back through the fresh→regular
    /// twin map. `None` for anything that is not a member. Keys are `n:<text>`
    /// / `s:<text>`, the same spelling [`Checker::plain_literal_key`] gives a
    /// plain literal, so the two compare directly. The value now comes directly
    /// from the enum literal payload rather than a reverse scan of the cache.
    pub(crate) fn enum_member_value(&self, member: TypeId) -> Option<(SymbolId, String)> {
        let regular = self.enum_member_regular.get(&member).copied().unwrap_or(member);
        let TypeData::EnumLiteral { owner, value, .. } = &self.store.get(regular).data else {
            return None;
        };
        let key = match value {
            crate::types::EnumLiteralValue::Number(value) => format!("n:{value}"),
            crate::types::EnumLiteralValue::String(value) => format!("s:{value}"),
        };
        Some((*owner, key))
    }

    /// §751: a plain (non-enum) literal's value in `enum_value_types`' key
    /// spelling. `None` for anything else.
    pub(crate) fn plain_literal_key(&self, literal: TypeId) -> Option<String> {
        let ty = self.store.get(literal);
        if ty.flags.intersects(TypeFlags::ENUM_LIKE) {
            return None;
        }
        let text = crate::printing::type_to_string(ty);
        if ty.flags.intersects(TypeFlags::NUMBER_LITERAL) {
            Some(format!("n:{text}"))
        } else if ty.flags.intersects(TypeFlags::STRING_LITERAL) {
            Some(format!("s:{}", text.trim_matches('"')))
        } else {
            None
        }
    }

    /// `areTypesComparable` (`relater.go`) as a Kleene answer. §763.
    ///
    /// Upstream is `isTypeComparableTo(a, b) || isTypeComparableTo(b, a)`, and
    /// since §750 this port HAS [`Relation::Comparable`] — so this is that
    /// query, not a stand-in for it. `None` is the relater declining in both
    /// directions, which the callers turn into "decline the whole narrowing":
    /// a dropped constituent is a confident wrong answer, and this relater's
    /// negatives are decidable only on the domains `checker-notes-assign.md`
    /// §2 lists.
    ///
    /// What this replaced (§50/§666's hand-rolled table — an identity check,
    /// an enum-member/literal lookup, and a same-base-primitive test over a
    /// hardcoded `simple` flag set) is now the relater's job, including the
    /// enum arms §751 gave it.
    fn comparable_ternary(&mut self, discriminant: TypeId, constituent: TypeId) -> Option<bool> {
        use crate::relater::{Relation, Ternary};
        if discriminant == constituent {
            return Some(true);
        }
        let forward = self.relate_ternary(discriminant, constituent, Relation::Comparable);
        if forward == Ternary::Related {
            return Some(true);
        }
        let backward = self.relate_ternary(constituent, discriminant, Relation::Comparable);
        match (forward, backward) {
            (_, Ternary::Related) => Some(true),
            (Ternary::NotRelated, Ternary::NotRelated) => Some(false),
            _ => None,
        }
    }

    /// `replacePrimitivesWithLiterals` (`flow.go:1907`), the string/number
    /// halves: a kept primitive constituent takes the discriminant's
    /// literals of that base kind, so `case "a"` narrows a `string` to
    /// `"a"`.
    fn replace_primitives_with_literals(&mut self, t: TypeId, literals: TypeId) -> TypeId {
        let literal_constituents: Vec<TypeId> = match &self.store.get(literals).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![literals],
        };
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let mut replaced = Vec::with_capacity(constituents.len());
        for constituent in constituents {
            let flags = self.store.get(constituent).flags;
            if flags.intersects(TypeFlags::STRING | TypeFlags::NUMBER | TypeFlags::BIG_INT)
                && !flags.intersects(TypeFlags::UNIT)
            {
                let base = self.get_base_type_of_literal_type(constituent);
                let mut matched = false;
                for &literal in &literal_constituents {
                    if self.store.get(literal).flags.intersects(TypeFlags::UNIT)
                        && self.get_base_type_of_literal_type(literal) == base
                    {
                        replaced.push(self.get_regular_type_of_literal_type(literal));
                        matched = true;
                    }
                }
                if !matched {
                    replaced.push(constituent);
                }
            } else {
                replaced.push(constituent);
            }
        }
        self.get_union_type(&replaced)
    }

    /// §100's door into the narrowing ladder: narrow `initial` (declared as
    /// `declared`) for `reference` under `condition`, outside any flow walk.
    /// The predicate-inference pair (`checkIfExpressionRefinesParameter`,
    /// `checker.go:20586`) is the one caller — it synthesises its own
    /// true/false condition nodes upstream too.
    pub(crate) fn narrow_reference_by_condition(
        &mut self,
        reference: NodeId,
        symbol: Option<SymbolId>,
        declared: TypeId,
        initial: TypeId,
        condition: NodeId,
        assume_true: bool,
    ) -> TypeId {
        let mut state = FlowState {
            array_elements: Vec::new(),
            reduce_labels: Vec::new(),
            reference,
            symbol,
            declared_type: declared,
            initial_type: initial,
            is_auto: false,
            is_auto_array: false,
            outer_reference: false,
            discriminant_pattern: None,
            flow_container: None,
            shared_flow_start: self.shared_flows.len(),
            element_dedupe_mark: 0,
            depth: 0,
        };
        self.narrow_type(&mut state, initial, condition, assume_true)
    }

    /// Native narrowType (flow.go) receives the instantiated semantic body.
    /// Expand the port's named alias carrier, retaining its identity when no
    /// narrowing occurs so a written alias does not become structural text.
    fn narrow_type(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        condition: NodeId,
        assume_true: bool,
    ) -> TypeId {
        let body = self.binding_type_alias_body(t);
        // The missing representation here is a union hidden by its alias.
        // Non-union mapped/reference types keep their existing narrowing road.
        let expanded = if self.store.get(body).flags.contains(TypeFlags::UNION) { body } else { t };
        let narrowed = self.narrow_type_worker(state, expanded, condition, assume_true);
        if narrowed == expanded { t } else { narrowed }
    }

    /// `narrowTypeByTruthiness` (flow.go:428): matching reference first,
    /// optional-chain facts next, then the discriminant property transform.
    fn narrow_type_by_truthiness(
        &mut self,
        state: &FlowState,
        t: TypeId,
        expr: NodeId,
        assume_true: bool,
    ) -> TypeId {
        if self.is_matching_reference(state, expr) {
            let facts = if assume_true { TypeFacts::TRUTHY } else { TypeFacts::FALSY };
            return self.get_adjusted_type_with_facts(t, facts);
        }
        // §51.4 (`checker-notes-narrow.md`): `if (o?.foo)` — under
        // strictNullChecks the true branch narrows the chain base
        // NE_UNDEFINED_OR_NULL, and FALLS THROUGH to the
        // discriminant filter (`flow.go:432`'s ordering).
        let t = if self.strict_null_checks
            && assume_true
            && self.optional_chain_contains_reference(state, expr)
        {
            self.get_adjusted_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL)
        } else {
            t
        };
        // `flow.go:434`: `if (s.done)` — the discriminant road,
        // `getDiscriminantPropertyAccess` + `narrowTypeByDiscriminant`
        // with the truthy/falsy facts as the member transform. §750
        // swapped this in for §51.3's inline
        // `filter_union_by_member_truthiness` (kept for the §84
        // sibling arm below).
        if let Some(access) = self.get_discriminant_property_access(state, expr, t) {
            let facts = if assume_true { TypeFacts::TRUTHY } else { TypeFacts::FALSY };
            return self.narrow_type_by_discriminant(t, access, |checker, prop| {
                checker.get_type_with_facts(prop, facts)
            });
        }
        t
    }

    fn narrow_type_worker(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        condition: NodeId,
        assume_true: bool,
    ) -> TypeId {
        // §758 (`flow.go:378`-`:381`), BEFORE the kind dispatch: for `a?.b`
        // and for `a ?? b`'s left operand, upstream emulates a synthetic
        // `a !== null && a !== undefined` condition rather than a truthiness
        // one. The distinction is observable — truthiness would also remove
        // `""`, `0` and `false`, which a chain root does not.
        if self.is_expression_of_optional_chain_root(condition)
            || self.is_left_operand_of_nullish_coalesce(condition)
        {
            return self.narrow_type_by_optionality(state, t, condition, assume_true);
        }
        let Some(node) = self.node_map.get(condition) else { return t };
        match node {
            // `narrowTypeByCallExpression` (`flow.go:444`), the
            // identifier-predicate half — `if (isNumber(x))` narrows `x`.
            // A call that answers no predicate falls through unchanged,
            // which is also the truthiness answer for a call condition
            // (upstream's dispatch reaches the call arm before any
            // truthiness question, exactly as here).
            Node::CallExpression(call) => {
                let narrowed = self.narrow_type_by_call_expression(state, t, call, assume_true);
                // SS150 (checker-notes-callres2.md): `if (o?.f())` — the
                // SS51.4 chain-base narrowing applies to CALL conditions
                // too (upstream's truthiness arm checks
                // optionalChainContainsReference regardless of the
                // condition's form); the predicate half answered first,
                // exactly as the dispatch orders it.
                if narrowed == t
                    && self.strict_null_checks
                    && assume_true
                    && self.optional_chain_contains_reference(state, condition)
                {
                    return self.get_adjusted_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL);
                }
                narrowed
            }
            // narrowType's ThisKeyword/SuperKeyword truthiness dispatch
            // (flow.go:399). These keyword nodes have no discriminant property.
            Node::KeywordExpression(keyword)
                if matches!(keyword.kind, SyntaxKind::ThisKeyword | SyntaxKind::SuperKeyword) =>
            {
                if self.is_matching_reference(state, condition) {
                    let facts = if assume_true { TypeFacts::TRUTHY } else { TypeFacts::FALSY };
                    self.get_adjusted_type_with_facts(t, facts)
                } else {
                    t
                }
            }
            // `if (x)`, `if (a.b)`, `while (o["k"])`: the reference itself as
            // the condition. Every form [`Checker::is_matching_reference`] can
            // decide belongs here — restricting it to `Identifier` is what kept
            // truthiness narrowing dead on property references long after the
            // matcher could have handled them (`bd tsr-6ka`).
            Node::Identifier(_)
            | Node::PropertyAccessExpression(_)
            | Node::ElementAccessExpression(_) => {
                // §82 (`checker-notes-narrow.md`): the ALIASED CONDITION —
                // `const isFoo = obj.kind === 'foo'; if (isFoo)` narrows as
                // the condition itself would (`narrowType`'s identifier arm,
                // `flow.go`: a CONST variable's initializer is inlined, depth
                // capped at 5 exactly as upstream's `inlineLevel`).
                if let Node::Identifier(identifier) = node
                    && !self.is_matching_reference(state, condition)
                    && self.alias_inline_level < 5
                    && self.is_constant_reference(state.reference)
                    && let Some(symbol) = self.binder.resolve_name(
                        self.nodes,
                        self.node_map,
                        condition,
                        identifier.text,
                        tsr_binder::SymbolFlags::VALUE,
                    )
                    && self.is_constant_variable(symbol)
                    && let Some(declaration) = self.binder.symbols().get(symbol).value_declaration
                    && let Some(Node::VariableDeclaration(variable)) =
                        self.node_map.get(declaration)
                    && variable.r#type.is_none()
                    && let Some(initializer) = variable.initializer.and_then(|i| i.node_id())
                {
                    self.alias_inline_level += 1;
                    let narrowed = self.narrow_type(state, t, initializer, assume_true);
                    self.alias_inline_level -= 1;
                    return narrowed;
                }
                self.narrow_type_by_truthiness(state, t, condition, assume_true)
            }
            Node::ParenthesizedExpression(inner) => {
                if self.is_jsdoc_type_assertion(condition) {
                    return t;
                }
                inner
                    .expression
                    .and_then(|e| e.node_id())
                    .map_or(t, |id| self.narrow_type(state, t, id, assume_true))
            }
            // `if (!x)`, which upstream reaches by flipping the assumption
            // rather than by a separate rule.
            Node::PrefixUnaryExpression(unary)
                if unary.operator.kind == SyntaxKind::ExclamationToken =>
            {
                unary
                    .operand
                    .and_then(|e| e.node_id())
                    .map_or(t, |id| self.narrow_type(state, t, id, !assume_true))
            }
            // `if (x !== undefined)`, `if (x != null)`. Upstream's
            // `narrowTypeByEquality` (`flow.go:556`), reached through
            // `narrowType`'s `BinaryExpression` arm.
            Node::BinaryExpression(binary) => {
                let (Some(left), Some(operator), Some(right)) =
                    (binary.left, binary.operator_token, binary.right)
                else {
                    return t;
                };
                // `narrowTypeByBinaryExpression` (flow.go:469): assignments
                // first narrow by the RHS condition, then by the truthiness of
                // the assigned reference. Commas inherit the RHS condition.
                if matches!(
                    operator.kind,
                    SyntaxKind::EqualsToken
                        | SyntaxKind::BarBarEqualsToken
                        | SyntaxKind::AmpersandAmpersandEqualsToken
                        | SyntaxKind::QuestionQuestionEqualsToken
                ) {
                    let (Some(left), Some(right)) = (left.node_id(), right.node_id()) else {
                        return t;
                    };
                    let narrowed = self.narrow_type(state, t, right, assume_true);
                    return self.narrow_type_by_truthiness(state, narrowed, left, assume_true);
                }
                if operator.kind == SyntaxKind::CommaToken {
                    return right
                        .node_id()
                        .map_or(t, |right| self.narrow_type(state, t, right, assume_true));
                }
                // `"p" in x` — `narrowTypeByInKeyword` (`flow.go:1001`),
                // known-property half; `checker-notes-narrow.md` §6.1. The
                // object-reference arm retains its written-literal boundary.
                // The missing-property arm below reads the semantic key only
                // after matching the accessed property's receiver.
                if operator.kind == SyntaxKind::InKeyword {
                    let Some(right_node) = right.node_id() else {
                        return t;
                    };
                    let right_node = self.get_reference_candidate(right_node);
                    // `flow.go:523`: presence of the accessed property removes
                    // intrinsic missing on the true branch and keeps only it
                    // on the false branch. Written undefined is not missing.
                    // Match the receiver by reference identity, not its text.
                    let contains_missing = t == self.intrinsics.missing
                        || matches!(&self.store.get(t).data, TypeData::Union { types, .. }
                            if types.first() == Some(&self.intrinsics.missing));
                    if contains_missing
                        // Native getFlowTypeOfAccessExpression uses the
                        // declared write type for definite assignment targets.
                        // The port's exact-mode legacy write flow must not
                        // receive the property's presence-read fact.
                        && self.assignment_target_kind(state.reference)
                            != crate::expressions::AssignmentTargetKind::Definite
                        && let Some(receiver) = self.expression_of_access(state.reference)
                        && self.references_match(receiver, right_node)
                        && let Some(reference) = self.node_map.get(state.reference)
                        && let Some(name) = self.accessed_property_name_at(reference)
                    {
                        let key = self.check_expression(left);
                        if self.property_name_from_index(key).as_deref() == Some(name.as_str()) {
                            let facts = if assume_true {
                                TypeFacts::NE_UNDEFINED
                            } else {
                                TypeFacts::EQ_UNDEFINED
                            };
                            return self.get_type_with_facts(t, facts);
                        }
                    }
                    // `flow.go:531`: the key is `getTypeOfExpression(left)`,
                    // usable as a property name — a literal, a template
                    // without substitutions, or a constant naming either.
                    if self.is_matching_reference(state, right_node) {
                        let key = self.check_expression(left);
                        if let Some(name) = self.property_name_from_index(key) {
                            return self.narrow_type_by_in_keyword(t, &name, assume_true);
                        }
                    }
                    return t;
                }
                // §83 (`checker-notes-narrow.md`): `x instanceof A` —
                // `narrowTypeByInstanceof` (`flow.go`), the class-identity
                // slice: constituents matching by IDENTITY or by the extends
                // CHAIN keep (true) or drop (false); an undecidable shape
                // declines whole rather than guessing.
                if operator.kind == SyntaxKind::InstanceOfKeyword {
                    let Some(left_id) = left.node_id() else { return t };
                    let left_id = self.get_reference_candidate(left_id);
                    if !self.is_matching_reference(state, left_id) {
                        // `flow.go:814` (§749): `o?.x instanceof C` proves
                        // the chain BASE non-null on the true branch.
                        if assume_true
                            && self.strict_null_checks
                            && self.optional_chain_contains_reference(state, left_id)
                        {
                            return self
                                .get_adjusted_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL);
                        }
                        return t;
                    }
                    let callee_type = self.check_expression(right);
                    // §111 slice 2: an RHS whose `[Symbol.hasInstance]` method
                    // carries a PREDICATE narrows by the predicate type —
                    // BOTH branches, exactly as a user guard would
                    // (`narrowTypeByInstanceof`'s hasInstance half). A
                    // boolean-returning hasInstance and every other shape keep
                    // the §83 structural road below.
                    if let Some(predicate_type) = self.has_instance_predicate_type(callee_type) {
                        return self
                            .narrowed_type_worker(t, predicate_type, assume_true, true)
                            .unwrap_or(t);
                    }
                    // §126 iteration 2: the false arm holds for TOP-LEVEL
                    // script vars — typeGuardOfFormInstanceOf's baseline
                    // keeps `C1 | C2` whole in the else on GLOBAL vars while
                    // instanceofWithStructurallyIdenticalTypes narrows the
                    // same shape on PARAMETERS; §83's "global var vs
                    // parameter" observation was the literal discriminator,
                    // measured again here (14 adverse ungated, all one case).
                    if !assume_true && self.reference_is_top_level_var(left_id) {
                        return t;
                    }
                    // §126: the FALSE branch narrows too — by DERIVATION.
                    // `getNarrowedTypeWorker`'s `!assumeTrue` arm
                    // (flow.go:861) filters constituents derived from the
                    // candidate (`isTypeDerivedFrom` = declared base chains,
                    // relater.go:4962). typeGuardOfFormInstanceOf's
                    // whole-union else stays whole because its constituents
                    // derive from nothing — the §83-era "21 adverse" used a
                    // structural test this trace retired.
                    let TypeData::Anonymous { symbol: class_symbol, .. } =
                        self.store.get(callee_type).data
                    else {
                        // Native 5b1047d narrowTypeByInstanceof admits a
                        // Function-derived callee, not only a constructor.
                        // isFunctionObjectType reads completed call OR construct
                        // signatures, or bind plus a Function subtype proof.
                        // A plain prototype-bearing object cannot pass this
                        // gate. Use the existing declaration/receiver-owned
                        // signature lookup; publish no new member or flow cache.
                        let has_signature = [
                            crate::signatures::SignatureKind::Call,
                            crate::signatures::SignatureKind::Construct,
                        ]
                        .into_iter()
                        .any(|kind| {
                            self.signature_candidates_of_named_type(callee_type, kind)
                                .is_some_and(|candidates| !candidates.is_empty())
                        });
                        if !has_signature && !self.is_bind_bearing_function_subtype(callee_type) {
                            return t;
                        }
                        let prototype_instance = self
                            .get_property_of_type(callee_type, "prototype")
                            .map(|p| self.get_type_of_symbol(p))
                            .filter(|&i| i != self.intrinsics.error && i != self.intrinsics.any);
                        let instance = if let Some(instance) = prototype_instance {
                            instance
                        } else {
                            {
                                // getInstanceType's second leg
                                // (flow.go:971-975): the UNION over construct
                                // signatures of the ERASED return (type
                                // parameters instantiated to any). The
                                // emptyObject third leg declines.
                                let Some(candidates) = self.signature_candidates_of_named_type(
                                    callee_type,
                                    crate::signatures::SignatureKind::Construct,
                                ) else {
                                    return t;
                                };
                                let mut returns = Vec::with_capacity(candidates.len());
                                for candidate in &candidates {
                                    if candidate.type_parameters.is_empty() {
                                        returns.push(candidate.r#type);
                                        continue;
                                    }
                                    let Some(parameters) = self.type_parameter_types(candidate)
                                    else {
                                        return t;
                                    };
                                    let names: Vec<&str> = candidate
                                        .type_parameters
                                        .iter()
                                        .map(|p| p.name.as_str())
                                        .collect();
                                    let any = self.intrinsics.any;
                                    let map: Vec<(TypeId, TypeId)> =
                                        parameters.iter().map(|&t| (t, any)).collect();
                                    let erased = self.instantiate_type(
                                        candidate.r#type,
                                        &map,
                                        &parameters,
                                        &names,
                                    );
                                    if erased == self.intrinsics.error {
                                        return t;
                                    }
                                    returns.push(erased);
                                }
                                if returns.is_empty() {
                                    return t;
                                }
                                self.get_union_type(&returns)
                            }
                        };
                        if instance == self.intrinsics.error || instance == self.intrinsics.any {
                            return t;
                        }
                        let instance_is_global = ["Object", "Function"].iter().any(|name| {
                            self.global_type_symbol_with_arity(name, 0).is_some_and(|symbol| {
                                matches!(
                                    self.store.get(instance).data,
                                    TypeData::Named { members: Some(owner), .. }
                                        if owner == symbol
                                )
                            })
                        });
                        if t == self.intrinsics.any && instance_is_global {
                            return t;
                        }
                        if !assume_true
                            && !self.store.get(instance).flags.intersects(TypeFlags::OBJECT)
                        {
                            return t;
                        }
                        if let Some(narrowed) =
                            self.narrowed_type_worker(t, instance, assume_true, true)
                        {
                            return narrowed;
                        }
                        return t;
                    };
                    if !self.binder.symbols().get(class_symbol).flags.intersects(SymbolFlags::CLASS)
                    {
                        return t;
                    }
                    let instance = self.get_declared_type_of_symbol(class_symbol);
                    if instance == self.intrinsics.error {
                        return t;
                    }
                    // SS149b (the hasInstance arc's last arm): a declared
                    // any/unknown/object narrows TO the class instance on
                    // the TRUE branch (upstream's constructor road,
                    // flow.go:836-843), EXCEPT any against the global
                    // Object/Function instances, which keeps any. The false
                    // branch keeps the declared type (an any/unknown/object
                    // minus one class is not expressible).
                    if t == self.intrinsics.any
                        || t == self.intrinsics.unknown
                        || t == self.intrinsics.non_primitive
                    {
                        if !assume_true {
                            return t;
                        }
                        let keeps_any = t == self.intrinsics.any
                            && ["Function", "Object"].iter().any(|name| {
                                self.global_type_symbol_with_arity(name, 0).is_some_and(|symbol| {
                                    matches!(
                                        self.store.get(instance).data,
                                        TypeData::Named { members: Some(owner), .. }
                                            if owner == symbol
                                    )
                                })
                            });
                        return if keeps_any { t } else { instance };
                    }
                    // SS159: the checkDerived worker answers first over its
                    // decidable domain; any undecidable rung falls through
                    // to the SS83/SS126 roads unchanged.
                    if let Some(narrowed) =
                        self.narrowed_type_worker(t, instance, assume_true, true)
                    {
                        return narrowed;
                    }
                    let TypeData::Union { types: members, .. } = &self.store.get(t).data else {
                        {
                            // Non-union: `x: Base` with `x instanceof Derived`
                            // narrows TO the derived instance when the chain
                            // relates them (true); §126: an identity or
                            // chain-DERIVED t is removed whole in the else —
                            // upstream's `t == candidate → never` and the
                            // derivation filter. Anything undecidable declines.
                            let derived = t == instance
                                || self.class_instance_symbol(t).is_some_and(|symbol| {
                                    self.class_extends_chain_contains(symbol, class_symbol)
                                });
                            if assume_true {
                                if t == instance {
                                    return t;
                                }
                                if self
                                    .class_instance_symbol(instance)
                                    .zip(self.class_instance_symbol(t))
                                    .is_some_and(|(derived, base)| {
                                        self.class_extends_chain_contains(derived, base)
                                    })
                                {
                                    return instance;
                                }
                            } else if derived {
                                return self.intrinsics.never;
                            }
                            return t;
                        }
                    };
                    let members = members.clone();
                    let matches: Vec<bool> = members
                        .iter()
                        .map(|&member| {
                            member == instance
                                || self.class_instance_symbol(member).is_some_and(|derived| {
                                    self.class_extends_chain_contains(derived, class_symbol)
                                })
                        })
                        .collect();
                    let kept: Vec<TypeId> = members
                        .iter()
                        .zip(&matches)
                        .filter_map(|(&member, &is_match)| {
                            (is_match == assume_true).then_some(member)
                        })
                        .collect();
                    if kept.len() == members.len() {
                        return t;
                    }
                    if kept.is_empty() {
                        // §126: the FALSE branch's empty remainder IS `never`
                        // (upstream's filterType); the TRUE branch's empty
                        // set still declines — its fallback needs
                        // assignability this port lacks.
                        if assume_true {
                            return t;
                        }
                        return self.intrinsics.never;
                    }
                    return self.rebuild_union_subset(t, &kept);
                }
                // §82.1: `&&`/`||` INSIDE an inlined aliased condition —
                // there are no flow branch nodes inside a const initializer,
                // so `const both = isA || isB; if (both)` needs the logical
                // arms `narrowType` itself has (`flow.go`'s
                // `narrowTypeByBinaryExpression`): `a || b` true is the
                // union of (a true) and (a false, then b true); the duals by
                // symmetry.
                if matches!(
                    operator.kind,
                    SyntaxKind::AmpersandAmpersandToken | SyntaxKind::BarBarToken
                ) {
                    let (Some(left), Some(right)) = (left.node_id(), right.node_id()) else {
                        return t;
                    };
                    let is_or = operator.kind == SyntaxKind::BarBarToken;
                    return if assume_true == is_or {
                        // `a || b` true / `a && b` false: two paths, unioned.
                        let first = self.narrow_type(state, t, left, is_or);
                        let other_base = self.narrow_type(state, t, left, !is_or);
                        let second = self.narrow_type(state, other_base, right, is_or);
                        self.get_union_type(&[first, second])
                    } else {
                        // `a && b` true / `a || b` false: one path, chained.
                        let after_left = self.narrow_type(state, t, left, assume_true);
                        self.narrow_type(state, after_left, right, assume_true)
                    };
                }
                if !matches!(
                    operator.kind,
                    SyntaxKind::EqualsEqualsToken
                        | SyntaxKind::ExclamationEqualsToken
                        | SyntaxKind::EqualsEqualsEqualsToken
                        | SyntaxKind::ExclamationEqualsEqualsToken
                ) {
                    return t;
                }
                let (Some(left), Some(right)) = (left.node_id(), right.node_id()) else {
                    return t;
                };
                let left = self.get_reference_candidate(left);
                let right = self.get_reference_candidate(right);
                // `typeof x === "…"` before the value-equality path, which is
                // upstream's dispatch order in `narrowTypeByBinaryExpression`
                // (`flow.go:500` region): a `TypeOfExpression` on either side
                // with a string literal on the other reaches
                // `narrowTypeByTypeof` (`flow.go:614`). §753 ports the last
                // of its three halves — the matching-reference half (SS-era)
                // and the optional-chain half (SS152) were already here; the
                // DISCRIMINANT half now goes through the pair.
                // `ast.IsStringLiteralLike` (`flow.go:477`): a template
                // without substitutions spells the same operand.
                let typeof_pair = match (self.node_map.get(left), self.node_map.get(right)) {
                    (Some(Node::TypeOfExpression(typeof_expr)), Some(other))
                    | (Some(other), Some(Node::TypeOfExpression(typeof_expr))) => {
                        string_literal_like_text(other).map(|text| (typeof_expr, text))
                    }
                    _ => None,
                };
                if let Some((typeof_expr, literal)) = typeof_pair {
                    let Some(target) = typeof_expr.expression.and_then(|e| e.node_id()) else {
                        return t;
                    };
                    let target = self.get_reference_candidate(target);
                    let negated = matches!(
                        operator.kind,
                        SyntaxKind::ExclamationEqualsToken
                            | SyntaxKind::ExclamationEqualsEqualsToken
                    );
                    if !self.is_matching_reference(state, target) {
                        // SS152: the CHAIN half of narrowTypeByTypeof — when
                        // the typeof target reads the reference through a
                        // `?.` chain and the branch implies the chain result
                        // is NOT undefined, the base strips undefined/null
                        // (`typeof o?.foo !== "undefined"` narrows `o`).
                        let effective = assume_true != negated;
                        let result_not_undefined = (effective && literal != "undefined")
                            || (!effective && literal == "undefined");
                        // §753: upstream ASSIGNS here and falls through
                        // (`flow.go:622`) — it does not return — so a
                        // `typeof o?.kind === "string"` composes the nullish
                        // removal WITH the discriminant filter below, the
                        // same composition §51.5 restored at the equality
                        // arm.
                        let mut t = t;
                        if self.strict_null_checks
                            && result_not_undefined
                            && self.optional_chain_contains_reference(state, target)
                        {
                            t = self
                                .get_adjusted_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL);
                        }
                        // `flow.go:624`-`:629`: the discriminant half, whose
                        // inner narrowing is `narrowTypeByLiteralExpression`
                        // — the same typeof filter, applied to the PROPERTY
                        // type rather than to the reference's.
                        if let Some(access) =
                            self.get_discriminant_property_access(state, target, t)
                        {
                            let literal = literal.to_string();
                            return self.narrow_type_by_discriminant(t, access, |checker, prop| {
                                checker.narrow_type_by_typeof_literal(prop, &literal, effective)
                            });
                        }
                        return t;
                    }
                    return self.narrow_type_by_typeof_literal(t, literal, assume_true != negated);
                }
                // §51.4 (`checker-notes-narrow.md`): the WHOLE containment
                // table (`flow.go:1032`), replacing §51.2's one quadrant —
                // facts NE_UNDEFINED_OR_NULL, loose operators included.
                let mut containment_narrowed: Option<TypeId> = None;
                {
                    let equals_operator = matches!(
                        operator.kind,
                        SyntaxKind::EqualsEqualsToken | SyntaxKind::EqualsEqualsEqualsToken
                    );
                    let loose = matches!(
                        operator.kind,
                        SyntaxKind::EqualsEqualsToken | SyntaxKind::ExclamationEqualsToken
                    );
                    let strict = matches!(
                        operator.kind,
                        SyntaxKind::EqualsEqualsEqualsToken
                            | SyntaxKind::ExclamationEqualsEqualsToken
                    );
                    if loose || strict {
                        let chain_pair =
                            [(left, right), (right, left)].into_iter().find(|&(candidate, _)| {
                                self.optional_chain_contains_reference(state, candidate)
                            });
                        if let Some((_, value_node)) = chain_pair {
                            let value_type = self
                                .node_map
                                .get(value_node)
                                .and_then(|node| tsr_ast::Expression::try_from(node).ok())
                                .map(|expression| self.check_expression(expression));
                            if let Some(value_type) = value_type
                                && value_type != self.intrinsics.error
                            {
                                let nullable =
                                    if loose { TypeFlags::NULLABLE } else { TypeFlags::UNDEFINED };
                                let parts: Vec<TypeId> = match &self.store.get(value_type).data {
                                    TypeData::Union { types, .. } => types.clone(),
                                    _ => vec![value_type],
                                };
                                let every = |checker: &Self, test: &dyn Fn(TypeFlags) -> bool| {
                                    parts.iter().all(|&part| test(checker.store.get(part).flags))
                                };
                                let remove = (equals_operator != assume_true
                                    && every(self, &|flags| flags.intersects(nullable)))
                                    || (equals_operator == assume_true
                                        && every(self, &|flags| {
                                            !flags.intersects(TypeFlags::ANY_OR_UNKNOWN | nullable)
                                        }));
                                if remove {
                                    containment_narrowed = Some(self.get_adjusted_type_with_facts(
                                        t,
                                        TypeFacts::NE_UNDEFINED_OR_NULL,
                                    ));
                                }
                            }
                        }
                    }
                }
                // §51.5 (`checker-notes-narrow.md`): upstream ASSIGNS the
                // containment result and falls through (`flow.go:491`), so
                // `o?.kind === 'a'` composes nullish removal WITH the
                // discriminant filter below.
                let t = containment_narrowed.unwrap_or(t);
                // §752 (`flow.go:496`-`:503`): the discriminant pair,
                // replacing §51.1's inline access test. Both operand orders,
                // reference-side access first — upstream's `leftAccess` then
                // `rightAccess`, with the OTHER operand as the value.
                for (candidate, value) in [(left, right), (right, left)] {
                    if let Some(access) = self.get_discriminant_property_access(state, candidate, t)
                    {
                        return self.narrow_type_by_discriminant_property(
                            t,
                            access,
                            operator.kind,
                            value,
                            assume_true,
                        );
                    }
                }
                // Upstream normalises with `getReferenceCandidate` on the left
                // and reads the value from the right; the reference can sit on
                // either side, so both orders are tried and the *other* operand
                // is the value.
                let (value, matched) = if self.is_matching_reference(state, left) {
                    (right, left)
                } else if self.is_matching_reference(state, right) {
                    (left, right)
                } else {
                    // §760 (`flow.go:504`-`:509`): `x.constructor === C`.
                    // Upstream places these arms BEFORE the boolean ones and
                    // after the discriminant pair, which is the order here.
                    let constructor_pair =
                        [(left, right), (right, left)].into_iter().find(|&(candidate, _)| {
                            self.is_matching_constructor_reference(state, candidate)
                        });
                    if let Some((_, identifier)) = constructor_pair {
                        return self.narrow_type_by_constructor(
                            t,
                            operator.kind,
                            identifier,
                            assume_true,
                        );
                    }
                    // §759 (`flow.go:510`-`:515`): `narrowTypeByBooleanComparison`.
                    // `x === true` / `x !== false` re-enters `narrowType` on the
                    // NON-boolean operand with the assumption folded in, so a
                    // condition that narrows on its own — a type-predicate call,
                    // a `typeof`, another comparison — keeps narrowing when it is
                    // compared to a boolean literal. Upstream requires the other
                    // operand NOT be an access expression, because an access
                    // beside a boolean literal is a DISCRIMINANT comparison and
                    // has already been offered to the pair above.
                    let boolean_pair =
                        [(right, left), (left, right)].into_iter().find(|&(boolean, other)| {
                            self.is_boolean_literal(boolean) && !self.is_access_expression(other)
                        });
                    if let Some((boolean, other)) = boolean_pair {
                        // `flow.go:807`. Upstream spells it
                        //   (assumeTrue != isTrue) != (op is an equality op)
                        // and `op is an equality op` is `!negated` here, so
                        // `x != !y` reduces to `x == y` — clippy's
                        // `nonminimal_bool` requires the reduced form. The
                        // upstream spelling is kept HERE so the
                        // correspondence stays checkable by eye; the two are
                        // the same truth table, and the test asserts all four
                        // combinations.
                        let is_true_keyword = self.nodes.kind(boolean) == SyntaxKind::TrueKeyword;
                        let negated = matches!(
                            operator.kind,
                            SyntaxKind::ExclamationEqualsToken
                                | SyntaxKind::ExclamationEqualsEqualsToken
                        );
                        let folded = (assume_true != is_true_keyword) == negated;
                        return self.narrow_type(state, t, other, folded);
                    }
                    return t;
                };
                // SS151: `o?.foo === value` - the chain result's undefined
                // is the CHAIN's, not the member's; the true branch strips
                // nullable from the filtered member (upstream's
                // optionalChainContainsReference strip after the
                // comparable filter, flow.go:585-600 region).
                let chain = self.spells_question_dot_chain(matched);
                self.narrow_type_by_equality(t, operator.kind, value, assume_true, chain)
            }
            _ => t,
        }
    }

    /// `flow.go:1851` `optionalChainContainsReference`: walk `source`
    /// inward while it IS an optional chain (`ast.IsOptionalChain` — the
    /// [`NodeFlags::OPTIONAL_CHAIN`] flag on a property/element access,
    /// call, or non-null expression) and answer whether any receiver on
    /// that walk is the matching reference.
    ///
    /// §748 replaced the §51.2 form, which walked `?.` TOKENS with a sticky
    /// bit: that form answered `true` for `a.b?.c` against reference `a`
    /// (the flag stops at `a.b`, which carries no `?.` and is not a chain
    /// link) and could not see a `NonNullExpression` link at all. The flag
    /// is set by the parser since §748 (`parser.go:5414`'s reparse).
    fn optional_chain_contains_reference(&mut self, state: &FlowState, node: NodeId) -> bool {
        let mut source = node;
        while self.is_optional_chain(source) {
            let Some(inner) = self.expression_of_chain_link(source) else { return false };
            if self.is_matching_reference(state, inner) {
                return true;
            }
            source = inner;
        }
        false
    }

    /// `ast.IsOptionalChain`: the flag AND one of the four link kinds.
    fn is_optional_chain(&self, node: NodeId) -> bool {
        self.nodes.flags(node).contains(NodeFlags::OPTIONAL_CHAIN)
            && matches!(
                self.nodes.kind(node),
                SyntaxKind::PropertyAccessExpression
                    | SyntaxKind::ElementAccessExpression
                    | SyntaxKind::CallExpression
                    | SyntaxKind::NonNullExpression
            )
    }

    /// `node.Expression()` for the four chain-link kinds.
    fn expression_of_chain_link(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::PropertyAccessExpression(access) => access.expression,
            Node::ElementAccessExpression(access) => access.expression,
            Node::CallExpression(call) => call.expression,
            Node::NonNullExpression(non_null) => non_null.expression,
            _ => None,
        }
        .and_then(|e| e.node_id())
    }

    /// `Checker.narrowTypeByEquality` (`flow.go:556`), **nullable-operand half
    /// only**.
    ///
    /// # Why half of it is a port and the other half is not
    ///
    /// Upstream's function splits on whether the compared value is nullable.
    /// The nullable branch reduces to picking one of six `TypeFacts` bits and
    /// filtering, and every one of those bits is fixed by `TypeFlags`
    /// ([`Checker::get_type_facts`]). The remaining branch needs
    /// `areTypesComparable`, `isUniformUnionType` and
    /// `replacePrimitivesWithLiterals` — assignability, which this port does
    /// not have — so `x === "a"` and `x === 3` return the type unchanged,
    /// which is `narrow_type`'s standing default and not a new guess. Same
    /// separation, and same reason, as the `&&` arm of `binary.rs`
    /// (`docs/architecture/checker-notes-armsplit.md` §3.1).
    ///
    /// `narrowTypeByCallExpression` (`flow.go:444`) reduced to the
    /// identifier-predicate half, plus `narrowTypeByTypePredicate`
    /// (`flow.go:316`) and `getNarrowedType`'s assignability filter with
    /// Kleene declines. See `checker-notes-narrow.md` §22.
    fn narrow_type_by_call_expression(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        call: &tsr_ast::CallExpression<'_>,
        assume_true: bool,
    ) -> TypeId {
        // SS165 (flow.go:457-465, TRANSCRIBED): the hasOwnProperty arm -
        // when the walked type CONTAINS the missing type, the reference is
        // an access expression, and the call is
        // `<reference-receiver>.hasOwnProperty("<accessed-name>")` with
        // exactly one string-literal argument naming the accessed property,
        // the branch adjusts by NE_UNDEFINED / EQ_UNDEFINED.
        'has_own: {
            let contains_missing = t == self.intrinsics.missing
                || matches!(&self.store.get(t).data, TypeData::Union { types, .. }
                    if types.first() == Some(&self.intrinsics.missing));
            if !contains_missing
                || self.assignment_target_kind(state.reference)
                    == crate::expressions::AssignmentTargetKind::Definite
            {
                break 'has_own;
            }
            let reference = state.reference;
            let Some(reference_receiver) = self.expression_of_access(reference) else {
                break 'has_own;
            };
            let Some(Node::PropertyAccessExpression(call_access)) =
                call.expression.and_then(|e| e.node_id()).and_then(|id| self.node_map.get(id))
            else {
                break 'has_own;
            };
            let Some(tsr_ast::MemberName::Identifier(method)) = call_access.name else {
                break 'has_own;
            };
            if method.text != "hasOwnProperty" || call.arguments.len() != 1 {
                break 'has_own;
            }
            let Some(call_receiver) = call_access.expression.and_then(|e| e.node_id()) else {
                break 'has_own;
            };
            let call_receiver = self.get_reference_candidate(call_receiver);
            if !self.references_match(reference_receiver, call_receiver) {
                break 'has_own;
            }
            let argument = match tsr_ast::Node::from(call.arguments[0])
                .node_id()
                .and_then(|id| self.node_map.get(id))
            {
                Some(Node::StringLiteral(argument)) => argument.text,
                Some(Node::NoSubstitutionTemplateLiteral(argument)) => argument.text,
                _ => break 'has_own,
            };
            let Some(reference) = self.node_map.get(reference) else {
                break 'has_own;
            };
            if self.accessed_property_name_at(reference).as_deref() != Some(argument) {
                break 'has_own;
            }
            let facts = if assume_true { TypeFacts::NE_UNDEFINED } else { TypeFacts::EQ_UNDEFINED };
            return self.get_type_with_facts(t, facts);
        }
        // `flow.go:445`: `hasMatchingArgument` (§749 — ported whole; the
        // first cut accepted only an argument that IS the reference).
        if !self.has_matching_argument(state, call) {
            return t;
        }
        // `assumeTrue || !isCallChain(callExpression)`: on the false branch
        // a `?.()` call answers no predicate — the chain may have short-
        // circuited. §749.
        if !assume_true
            && let Some(call_id) = call.node_id
            && self.is_optional_chain(call_id)
        {
            return t;
        }
        let Some(call_id) = call.node_id else { return t };
        let Some(signature) = self.get_effects_signature(call_id, call) else { return t };
        let Some(predicate) = &signature.predicate else { return t };
        // `TypePredicateKindThis || TypePredicateKindIdentifier`.
        if predicate.asserts {
            return t;
        }
        let Some(predicate_type) = predicate.r#type else { return t };
        let Some(argument) = self.get_type_predicate_argument(&signature, call) else { return t };
        self.narrow_type_by_type_predicate(state, t, predicate_type, argument, assume_true)
    }

    /// getTypePredicateArgument (internal/checker/flow.go:2451), shared by
    /// condition predicates and assertion-call flow effects.
    fn get_type_predicate_argument(
        &self,
        signature: &crate::signatures::Signature,
        call: &tsr_ast::CallExpression<'_>,
    ) -> Option<NodeId> {
        let predicate = signature.predicate.as_ref()?;
        if let Some(name) = &predicate.parameter_name {
            let index =
                signature.parameters.iter().position(|parameter| parameter.name == *name)?;
            return tsr_ast::Node::from(*call.arguments.get(index)?).node_id();
        }
        let invoked = self.skip_parentheses(call.expression?.node_id()?);
        let receiver = match self.node_map.get(invoked)? {
            Node::PropertyAccessExpression(access) => access.expression,
            Node::ElementAccessExpression(access) => access.expression,
            _ => None,
        }?
        .node_id()?;
        Some(self.skip_parentheses(receiver))
    }

    /// `narrowTypeByTypePredicate` (`flow.go:315`). The
    /// any-vs-global-`Object`/`Function` guard lives in
    /// [`Checker::narrow_by_predicate_type`].
    fn narrow_type_by_type_predicate(
        &mut self,
        state: &FlowState,
        mut t: TypeId,
        predicate_type: TypeId,
        predicate_argument: NodeId,
        assume_true: bool,
    ) -> TypeId {
        if self.is_matching_reference(state, predicate_argument) {
            return self.narrow_by_predicate_type(t, predicate_type, assume_true);
        }
        // `flow.go:324`: `isFoo(o?.x)` — the chain BASE is non-null on the
        // true branch when the predicate type cannot be `undefined`, and on
        // the false branch when every constituent of it is nullable.
        if self.strict_null_checks
            && self.optional_chain_contains_reference(state, predicate_argument)
        {
            let strips = if assume_true {
                !self.get_type_facts(predicate_type).intersects(TypeFacts::EQ_UNDEFINED)
            } else {
                self.every_type(predicate_type, |checker, part| checker.is_nullable_type(part))
            };
            if strips {
                t = self.get_adjusted_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL);
            }
        }
        // `flow.go:327`: `isFoo(x.kind)` — the discriminant road. §750.
        if let Some(access) = self.get_discriminant_property_access(state, predicate_argument, t) {
            return self.narrow_type_by_discriminant(t, access, |checker, prop| {
                checker.narrow_by_predicate_type(prop, predicate_type, assume_true)
            });
        }
        t
    }

    /// `getDiscriminantPropertyAccess` (`flow.go:1436`). §750, with
    /// `getCandidateDiscriminantPropertyAccess` split out at §755 to match
    /// upstream's shape.
    fn get_discriminant_property_access(
        &mut self,
        state: &FlowState,
        expr: NodeId,
        computed_type: TypeId,
    ) -> Option<NodeId> {
        let declared_is_union =
            matches!(self.store.get(state.declared_type).data, TypeData::Union { .. });
        let computed_is_union =
            matches!(self.store.get(computed_type).data, TypeData::Union { .. });
        if !declared_is_union && !computed_is_union {
            return None;
        }
        let access = self.get_candidate_discriminant_property_access(state, expr)?;
        let name = self.get_accessed_property_name(access)?;
        let t = if declared_is_union && self.is_type_subset_of(computed_type, state.declared_type) {
            state.declared_type
        } else {
            computed_type
        };
        self.is_discriminant_property(t, &name).then_some(access)
    }

    /// `isMatchingConstructorReference` (`flow.go:750`). §760.
    ///
    /// A property access named `constructor`, or an element access whose
    /// argument is the string `"constructor"`, whose RECEIVER is the matching
    /// reference.
    fn is_matching_constructor_reference(&mut self, state: &FlowState, expr: NodeId) -> bool {
        let named_constructor = match self.node_map.get(expr) {
            Some(Node::PropertyAccessExpression(access)) => {
                matches!(access.name, Some(tsr_ast::MemberName::Identifier(name)) if name.text == "constructor")
            }
            Some(Node::ElementAccessExpression(access)) => access
                .argument_expression
                .and_then(|e| e.node_id())
                .and_then(|id| self.node_map.get(id))
                .is_some_and(|node| {
                    matches!(node, Node::StringLiteral(literal) if literal.text == "constructor")
                }),
            _ => false,
        };
        named_constructor
            && self
                .expression_of_access(expr)
                .is_some_and(|receiver| self.is_matching_reference(state, receiver))
    }

    /// `narrowTypeByConstructor` (`flow.go:760`). §760.
    ///
    /// `x.constructor === C` keeps the constituents CONSTRUCTED BY `C` — the
    /// type of `C`'s `prototype` property. Only the equality operators narrow;
    /// upstream declines inequality outright (`:762`), because
    /// `x.constructor !== C` does not prove the constituent is not a subclass.
    fn narrow_type_by_constructor(
        &mut self,
        t: TypeId,
        operator: SyntaxKind,
        identifier: NodeId,
        assume_true: bool,
    ) -> TypeId {
        // `flow.go:762`, transcribed: narrow only on `==`/`===` in the true
        // branch and on `!=`/`!==` in the false branch.
        let equals =
            matches!(operator, SyntaxKind::EqualsEqualsToken | SyntaxKind::EqualsEqualsEqualsToken);
        let not_equals = matches!(
            operator,
            SyntaxKind::ExclamationEqualsToken | SyntaxKind::ExclamationEqualsEqualsToken
        );
        if (assume_true && !equals) || (!assume_true && !not_equals) {
            return t;
        }
        let Some(expression) =
            self.node_map.get(identifier).and_then(|node| tsr_ast::Expression::try_from(node).ok())
        else {
            return t;
        };
        let identifier_type = self.check_expression(expression);
        if identifier_type == self.intrinsics.error {
            return t;
        }
        // `isFunctionType || isConstructorType` (`:767`), read through the
        // signature road this port already uses for the instanceof arm (§83):
        // a type with a call or construct signature.
        let has_signature =
            [crate::signatures::SignatureKind::Call, crate::signatures::SignatureKind::Construct]
                .into_iter()
                .any(|kind| {
                    self.signature_candidates_of_named_type(identifier_type, kind)
                        .is_some_and(|candidates| !candidates.is_empty())
                });
        // A CLASS's static side is a constructor type upstream, but this
        // port's `signature_candidates_of_named_type` answers only for NAMED
        // types and a class constructor is `Anonymous`. Without this second
        // disjunct the guard rejected exactly the class case — measured: the
        // two class fixtures stayed put while the primitive ones moved.
        let is_class_constructor = matches!(
            self.store.get(identifier_type).data,
            TypeData::Anonymous { symbol, .. }
                if self
                    .binder
                    .symbols()
                    .get(self.binder.merged_symbol(symbol))
                    .flags
                    .contains(SymbolFlags::CLASS)
        );
        if !has_signature && !is_class_constructor {
            return t;
        }
        // The `prototype` property's type is the candidate (`:772`-`:781`).
        // `any` declines, and so do the global `Object` and `Function` types —
        // every object is constructed by those, so they narrow nothing.
        // Read through [`Checker::get_type_of_property_of_type`], NOT through
        // `get_property_of_type` + `get_type_of_symbol`: a class's static
        // `prototype` is SYNTHETIC in this port (§117 slice 2, `members.rs`)
        // and exists only on the type road, so the symbol road answers `None`
        // for exactly the class case this arm is most wanted for. The §83
        // instanceof arm takes the symbol road and survives it only because it
        // has an erased-construct-return fallback that upstream's
        // `narrowTypeByConstructor` does not.
        let Some(candidate) = self
            .get_type_of_property_of_type(identifier_type, "prototype")
            .filter(|&c| c != self.intrinsics.error && c != self.intrinsics.any)
        else {
            return t;
        };
        let is_global = ["Object", "Function"].iter().any(|name| {
            self.global_type_symbol_with_arity(name, 0).is_some_and(|symbol| {
                matches!(
                    self.store.get(candidate).data,
                    TypeData::Named { members: Some(owner), .. } if owner == symbol
                )
            })
        });
        if is_global {
            return t;
        }
        // `if IsTypeAny(t) { return candidate }` (`:785`).
        if t == self.intrinsics.any {
            return candidate;
        }
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let total = constituents.len();
        let mut kept = Vec::with_capacity(total);
        for constituent in constituents {
            if self.is_constructed_by(constituent, candidate) {
                kept.push(constituent);
            }
        }
        if kept.len() == total {
            return t;
        }
        if kept.is_empty() {
            return self.intrinsics.never;
        }
        self.get_union_type(&kept)
    }

    /// `isConstructedBy` (`flow.go:793`). §760.
    ///
    /// If EITHER side is a class type the check is symbol identity, not
    /// structure: two classes with identical members are structurally the same
    /// type, but `instanceOfA.constructor === B` is false. Everything else is
    /// the subtype relation.
    fn is_constructed_by(&mut self, source: TypeId, target: TypeId) -> bool {
        let source_symbol = self.class_symbol_of(source);
        let target_symbol = self.class_symbol_of(target);
        if source_symbol.is_some() || target_symbol.is_some() {
            return source_symbol.is_some() && source_symbol == target_symbol;
        }
        self.is_type_subtype_of(source, target)
    }

    /// The declaring symbol of a type that came from a CLASS declaration —
    /// this port's stand-in for `ObjectFlagsClass`, which it does not carry.
    /// §760.
    fn class_symbol_of(&self, t: TypeId) -> Option<SymbolId> {
        let TypeData::Named { members: Some(symbol), .. } = self.store.get(t).data else {
            return None;
        };
        self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::CLASS).then_some(symbol)
    }

    /// `ast.IsBooleanLiteral` — the `true` and `false` keywords. §759.
    fn is_boolean_literal(&self, node: NodeId) -> bool {
        matches!(self.nodes.kind(node), SyntaxKind::TrueKeyword | SyntaxKind::FalseKeyword)
    }

    /// `ast.IsAccessExpression` — a property or element access. §759.
    fn is_access_expression(&self, node: NodeId) -> bool {
        matches!(
            self.nodes.kind(node),
            SyntaxKind::PropertyAccessExpression | SyntaxKind::ElementAccessExpression
        )
    }

    /// `narrowTypeByOptionality` (`flow.go:415`). §758.
    ///
    /// The chain-root and `??`-left roads narrow by PRESENCE, not by truth:
    /// `NEUndefinedOrNull` / `EQUndefinedOrNull` rather than `Truthy` /
    /// `Falsy`. Both halves are ported — the matching reference, and the
    /// discriminant property through the §750 pair.
    fn narrow_type_by_optionality(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        expr: NodeId,
        assume_present: bool,
    ) -> TypeId {
        let facts = if assume_present {
            TypeFacts::NE_UNDEFINED_OR_NULL
        } else {
            TypeFacts::EQ_UNDEFINED_OR_NULL
        };
        if self.is_matching_reference(state, expr) {
            return self.get_adjusted_type_with_facts(t, facts);
        }
        if let Some(access) = self.get_discriminant_property_access(state, expr, t) {
            return self.narrow_type_by_discriminant(t, access, |checker, prop| {
                checker.get_type_with_facts(prop, facts)
            });
        }
        t
    }

    /// `IsExpressionOfOptionalChainRoot` (`ast/utilities.go:383`): the parent
    /// is an optional-chain ROOT (`:362` — a chain that is not a non-null
    /// expression and carries its own `?.`) and this node is its expression.
    fn is_expression_of_optional_chain_root(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        if !self.is_optional_chain(parent)
            || self.nodes.kind(parent) == SyntaxKind::NonNullExpression
            || !self.has_question_dot_token(parent)
        {
            return false;
        }
        self.expression_of_chain_link(parent) == Some(node)
    }

    /// `getQuestionDotToken` (`ast/utilities.go`), the three chain-link kinds
    /// that can carry one.
    fn has_question_dot_token(&self, node: NodeId) -> bool {
        match self.node_map.get(node) {
            Some(Node::PropertyAccessExpression(access)) => access.question_dot_token.is_some(),
            Some(Node::ElementAccessExpression(access)) => access.question_dot_token.is_some(),
            Some(Node::CallExpression(call)) => call.question_dot_token.is_some(),
            _ => false,
        }
    }

    /// The second half of `flow.go:379`'s condition: this node is the LEFT
    /// operand of a `??` or `??=`, whose short-circuit is nullish and not
    /// falsy.
    fn is_left_operand_of_nullish_coalesce(&self, node: NodeId) -> bool {
        let Some(parent) = self.nodes.parent(node) else { return false };
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else {
            return false;
        };
        let Some(operator) = binary.operator_token else { return false };
        matches!(
            operator.kind,
            SyntaxKind::QuestionQuestionToken | SyntaxKind::QuestionQuestionEqualsToken
        ) && binary.left.and_then(|e| e.node_id()) == Some(node)
    }

    /// `getCandidateDiscriminantPropertyAccess`'s first arm (`flow.go:1459`).
    /// §762.
    ///
    /// An identifier bound to a BINDING ELEMENT or PARAMETER declared directly
    /// in `pattern`, with no initializer of its own and no `...`. Upstream's
    /// test is `f.reference == declaration.Parent`, one hop — a binding
    /// element's parent IS the pattern, and a parameter's parent is the
    /// function that stands as the pseudo-reference.
    fn pseudo_reference_candidate(&mut self, pattern: NodeId, expr: NodeId) -> Option<NodeId> {
        let Some(Node::Identifier(identifier)) = self.node_map.get(expr) else {
            return None;
        };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            expr,
            identifier.text,
            SymbolFlags::VALUE,
        )?;
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        if self.nodes.parent(declaration) != Some(pattern) {
            return None;
        }
        let (initializer, rest) = match self.node_map.get(declaration)? {
            Node::BindingElement(element) => {
                (element.initializer.is_some(), element.dot_dot_dot_token.is_some())
            }
            Node::ParameterDeclaration(parameter) => {
                (parameter.initializer.is_some(), parameter.dot_dot_dot_token.is_some())
            }
            _ => return None,
        };
        (!initializer && !rest).then_some(declaration)
    }

    /// `getCandidateDiscriminantPropertyAccess` (`flow.go:1457`). §755.
    ///
    /// Upstream's identifier arm is now ported WHOLE; of its three top-level
    /// arms only the pseudo-reference one is missing:
    ///
    /// - **The access arm** (`:1468`-`:1472`): an access expression whose
    ///   receiver is the matching reference. This is the whole of what §750
    ///   had, inlined into the caller; splitting it out is what let the alias
    ///   arms be added beside it rather than bolted onto the caller.
    /// - **The `const x = obj.kind` alias arm** (`:1473`-`:1482`, first half,
    ///   §755): an identifier bound to a CONST whose initializer is an access
    ///   on the reference. The candidate returned is the INITIALIZER, so
    ///   everything downstream sees an ordinary access.
    /// - **The `const { kind: x } = obj` alias arm** (`:1483`-`:1489`, §756):
    ///   the declaration is a BINDING ELEMENT with no initializer, and the RHS
    ///   two parents up is matched WHOLE against the reference. The candidate
    ///   returned is the binding ELEMENT, which is why this half needs
    ///   `getAccessedPropertyName`'s binding-element arm and the first did not.
    ///
    /// Not ported, and it declines here rather than mis-narrowing: the
    /// binding-pattern/function pseudo-reference arm (`:1459`-`:1467`). This
    /// port models that road through `state.discriminant_pattern` (§50)
    /// rather than through a binding pattern standing as the reference, so
    /// the arm has no counterpart to test until the two models are
    /// reconciled.
    fn get_candidate_discriminant_property_access(
        &mut self,
        state: &FlowState,
        expr: NodeId,
    ) -> Option<NodeId> {
        // §762 (`flow.go:1459`-`:1467`): the PSEUDO-REFERENCE arm. Upstream
        // tests `IsBindingPattern(f.reference)` — it makes the pattern itself
        // the reference. This port carries the pattern in
        // `state.discriminant_pattern` instead (§50) and keeps the location as
        // the reference; the two models hold the same information, and this is
        // the join between them. The candidate returned is the DECLARATION,
        // which `getAccessedPropertyName` reads through its binding-element
        // arm (§756) or its parameter arm (above).
        if let Some(pattern) = state.discriminant_pattern {
            return self.pseudo_reference_candidate(pattern, expr);
        }
        if let Some(receiver) = self.expression_of_access(expr) {
            return self.is_matching_reference(state, receiver).then_some(expr);
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(expr) else {
            return None;
        };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            expr,
            identifier.text,
            SymbolFlags::VALUE,
        )?;
        if !self.is_constant_variable(symbol) {
            return None;
        }
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        // `getCandidateVariableDeclarationInitializer` (`flow.go:1495`): the
        // initializer of an UNANNOTATED variable declaration, parentheses
        // skipped. The annotation check matters — `const x: Kind = obj.kind`
        // is typed by its annotation, so the alias would be unsound.
        if let Some(initializer) = self.unannotated_declaration_initializer(declaration) {
            if let Some(receiver) = self.expression_of_access(initializer) {
                return self.is_matching_reference(state, receiver).then_some(initializer);
            }
            return None;
        }
        // §756 (`flow.go:1483`-`:1489`): `const { kind: x } = obj` — the
        // declaration is a BINDING ELEMENT with no initializer of its own,
        // and the thing to match is the RHS of the variable declaration two
        // parents up. The candidate returned is the binding ELEMENT, which is
        // why this half needs `getAccessedPropertyName`'s binding-element arm
        // and the first half did not.
        if self.nodes.kind(declaration) != SyntaxKind::BindingElement {
            return None;
        }
        let Some(Node::BindingElement(element)) = self.node_map.get(declaration) else {
            return None;
        };
        if element.initializer.is_some() {
            return None;
        }
        let pattern = self.nodes.parent(declaration)?;
        let owner = self.nodes.parent(pattern)?;
        let initializer = self.unannotated_declaration_initializer(owner)?;
        // `IsIdentifier(initializer) || IsAccessExpression(initializer)` —
        // the RHS is matched WHOLE here, not through its receiver.
        let is_candidate_shape = matches!(
            self.nodes.kind(initializer),
            SyntaxKind::Identifier
                | SyntaxKind::PropertyAccessExpression
                | SyntaxKind::ElementAccessExpression
        );
        if !is_candidate_shape {
            return None;
        }
        self.is_matching_reference(state, initializer).then_some(declaration)
    }

    /// `getCandidateVariableDeclarationInitializer` (`flow.go:1495`). §755.
    fn unannotated_declaration_initializer(&self, node: NodeId) -> Option<NodeId> {
        let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) else {
            return None;
        };
        if declaration.r#type.is_some() {
            return None;
        }
        let initializer = declaration.initializer.and_then(|e| e.node_id())?;
        Some(self.skip_parentheses(initializer))
    }

    /// `node.Expression()` of a property or element access.
    fn expression_of_access(&self, node: NodeId) -> Option<NodeId> {
        match self.node_map.get(node)? {
            Node::PropertyAccessExpression(access) => access.expression,
            Node::ElementAccessExpression(access) => access.expression,
            _ => None,
        }
        .and_then(|e| e.node_id())
    }

    /// `getAccessedPropertyName` (`flow.go:1727`), the access-expression
    /// arms: a property access's name, or an element access whose argument is
    /// a string/numeric literal. Binding elements and parameters also expose
    /// their property name or tuple index to pseudo-reference narrowing.
    fn get_accessed_property_name(&self, access: NodeId) -> Option<String> {
        match self.node_map.get(access)? {
            Node::PropertyAccessExpression(node) => match node.name? {
                tsr_ast::MemberName::Identifier(name) => Some(name.text.to_string()),
                tsr_ast::MemberName::PrivateIdentifier(name) => Some(name.text.to_string()),
            },
            // `getDestructuringPropertyName` (`flow.go:1792`): an array
            // binding uses its position, including omitted elements. Object
            // bindings use the explicit property name or shorthand name.
            Node::BindingElement(node) => {
                let parent = self.nodes.parent(access)?;
                if self.nodes.kind(parent) == SyntaxKind::ArrayBindingPattern {
                    let Node::BindingPattern(pattern) = self.node_map.get(parent)? else {
                        return None;
                    };
                    return pattern
                        .elements
                        .iter()
                        .position(|element| element.node_id == Some(access))
                        .map(|index| index.to_string());
                }
                if self.nodes.kind(parent) != SyntaxKind::ObjectBindingPattern {
                    return None;
                }
                let name = match node.property_name {
                    Some(property_name) => property_name.node_id()?,
                    None => node.name.and_then(|n| n.node_id())?,
                };
                match self.node_map.get(name)? {
                    Node::Identifier(identifier) => Some(identifier.text.to_string()),
                    Node::StringLiteral(literal) => Some(literal.text.to_string()),
                    Node::NumericLiteral(literal) => Some(literal.text.to_string()),
                    _ => None,
                }
            }
            // §762: `getAccessedPropertyName`'s PARAMETER arm
            // (`flow.go:1735`-`:1737`) — the parameter's INDEX in its
            // function's parameter list, as a string. Tuples answer numeric
            // member names, so a tuple union destructured across a parameter
            // list discriminates by position. This is §50.3's rule, moved from
            // its inline site onto upstream's function.
            Node::ParameterDeclaration(_) => {
                let function = self.nodes.parent(access)?;
                let parameters = match self.node_map.get(function)? {
                    Node::ArrowFunction(node) => node.parameters,
                    Node::FunctionExpression(node) => node.parameters,
                    Node::FunctionDeclaration(node) => node.parameters,
                    Node::MethodDeclaration(node) => node.parameters,
                    _ => return None,
                };
                parameters
                    .iter()
                    .position(|parameter| parameter.node_id == Some(access))
                    .map(|index| index.to_string())
            }
            Node::ElementAccessExpression(node) => {
                let argument = node.argument_expression.and_then(|e| e.node_id())?;
                match self.node_map.get(argument)? {
                    Node::StringLiteral(literal) => Some(literal.text.to_string()),
                    Node::NumericLiteral(literal) => Some(literal.text.to_string()),
                    Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text.to_string()),
                    _ => None,
                }
            }
            _ => None,
        }
    }

    /// `isDiscriminantProperty` (`relater.go:1087`), computed from the
    /// constituents rather than read off a synthetic union property — this
    /// port builds no `createUnionOrIntersectionProperty` symbol and carries
    /// no `CheckFlags`. The three conditions it encodes are transcribed:
    /// the property exists in SOME constituent (`singleProp != nil`), its
    /// types are NON-UNIFORM across the constituents that have it
    /// (`HasNonUniformType`), and at least one of them is a literal type
    /// (`HasLiteralType`), and the union property's type is not generic.
    /// §750.
    ///
    /// Not transcribed: the `ContainsPrivate | ContainsProtected` mismatch
    /// rule (`checker.go:21556`, answers nil) and `isPatternLiteralType`
    /// (this port has no template-literal types to be one). A constituent
    /// lacking the property makes the synthetic property partial upstream
    /// but does not stop it from being a discriminant, and does not here.
    fn is_discriminant_property(&mut self, t: TypeId, name: &str) -> bool {
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => return false,
        };
        let mut first: Option<TypeId> = None;
        let mut non_uniform = false;
        let mut has_literal = false;
        for constituent in constituents {
            let flags = self.store.get(constituent).flags;
            if constituent == self.intrinsics.error || flags.contains(TypeFlags::NEVER) {
                continue;
            }
            let Some(prop_type) = self.get_type_of_property_of_type(constituent, name) else {
                continue;
            };
            if prop_type == self.intrinsics.error {
                // A property this port cannot type is not evidence either way.
                return false;
            }
            match first {
                None => first = Some(prop_type),
                Some(f) if f != prop_type => non_uniform = true,
                Some(_) => {}
            }
            if self.is_literal_type(prop_type) {
                has_literal = true;
            }
            if self.store.get(prop_type).flags.intersects(TypeFlags::TYPE_PARAMETER) {
                // `!isGenericType(getTypeOfSymbol(prop))`, reduced to the
                // shape this port can see.
                return false;
            }
        }
        first.is_some() && non_uniform && has_literal
    }

    /// getReducedType / isDiscriminantWithNeverType (checker.go:21819).
    /// A required property must have nonuniform types, a literal constituent,
    /// no already-never constituent, and an intersection reducing to never.
    /// Keep this reduced view separate from the written intersection identity.
    pub(crate) fn intersection_has_never_discriminant(&mut self, ty: TypeId) -> bool {
        let TypeData::Intersection { types, .. } = self.store.get(ty).data.clone() else {
            return false;
        };
        if let Some(&reduced) = self.never_intersection_types.get(&ty) {
            return reduced;
        }
        self.never_intersection_types.insert(ty, false);
        let mut parts = Vec::new();
        let mut names = Vec::new();
        for part in types {
            let Some(part_names) = self.get_property_names_of_type(part) else { return false };
            for name in &part_names {
                if !names.contains(name) {
                    names.push(name.clone());
                }
            }
            parts.push((part, part_names));
        }
        'property: for name in names {
            // A property contributed by only one constituent cannot acquire
            // a conflicting discriminant or private declaration. Avoid forcing
            // its type while a recursive alias is still being resolved.
            if parts.iter().filter(|(_, names)| names.contains(&name)).count() < 2 {
                continue;
            }
            let symbols = self.intersection_property_symbols(ty, &name);
            if symbols.len() > 1
                && symbols
                    .iter()
                    .any(|&symbol| self.property_has_modifier(symbol, SyntaxKind::PrivateKeyword))
            {
                self.never_intersection_types.insert(ty, true);
                return true;
            }

            let mut values = Vec::new();
            let mut optional = true;
            let mut literal = false;
            for (part, names) in &parts {
                let part = *part;
                if !names.contains(&name) {
                    continue;
                }
                let Some(property) = self.get_property_of_type(part, &name) else {
                    continue 'property;
                };
                let Some(value) = self.get_type_of_property_of_type(part, &name) else {
                    continue 'property;
                };
                if self.is_error(value) || self.store.get(value).flags.contains(TypeFlags::NEVER) {
                    continue 'property;
                }
                optional &= self.property_is_optional(property);
                literal |= self.is_literal_type(value);
                values.push(value);
            }
            if !optional
                && literal
                && values.windows(2).any(|pair| pair[0] != pair[1])
                && self.get_intersection_type(&values, None) == self.intrinsics.never
            {
                self.never_intersection_types.insert(ty, true);
                return true;
            }
        }
        false
    }

    /// `isLiteralType` (`checker.go:25393`).
    fn is_literal_type(&self, t: TypeId) -> bool {
        let ty = self.store.get(t);
        if ty.flags.intersects(TypeFlags::BOOLEAN) {
            return true;
        }
        if let TypeData::Union { types, .. } = &ty.data {
            if ty.flags.intersects(TypeFlags::ENUM_LITERAL) {
                return true;
            }
            return types.iter().all(|&c| self.store.get(c).flags.intersects(TypeFlags::UNIT));
        }
        ty.flags.intersects(TypeFlags::UNIT)
    }

    /// `narrowTypeByDiscriminantProperty` (`flow.go:702`). §752 — the
    /// equality arm's swap onto the pair, replacing §51.1's inline test.
    ///
    /// The key-property fast path (`:703`-`:719`) is **not ported**: it needs
    /// `getKeyPropertyName` and `getConstituentTypeForKeyType`, and this port
    /// interns no key-property index for a union. Declining it is upstream's
    /// own behaviour for every union whose `keyPropertyName` is empty, which
    /// is the general case; for a union that HAS one, the filter below
    /// reaches the same constituent by comparability instead of by lookup,
    /// losing only the `removeType` shortcut on the negative `===` branch and
    /// the O(1). It is an optimisation with one behavioural edge, not a
    /// mechanism the arm depends on.
    fn narrow_type_by_discriminant_property(
        &mut self,
        t: TypeId,
        access: NodeId,
        operator: SyntaxKind,
        value: NodeId,
        assume_true: bool,
    ) -> TypeId {
        // Upstream passes `narrowTypeByEquality` with no chain flag: the
        // optional-chain strip belongs to `narrowTypeByDiscriminant`, which
        // does it on the RECEIVER before reading the property, so passing it
        // again on the property type would strip twice.
        self.narrow_type_by_discriminant(t, access, |checker, prop| {
            checker.narrow_type_by_equality(prop, operator, value, assume_true, false)
        })
    }

    /// `narrowTypeByDiscriminant` (`flow.go:725`). §750.
    ///
    /// The filter is `areTypesComparable(narrowedPropType, discriminantType)`
    /// over [`Relation::Comparable`]. Where that relation answers `Unknown`
    /// in BOTH directions for some constituent, the whole narrowing declines
    /// (answers `t`): a dropped constituent is a confident wrong answer and
    /// this relater's negatives are decidable only on the domains
    /// `checker-notes-assign.md` §2 lists.
    fn narrow_type_by_discriminant(
        &mut self,
        t: TypeId,
        access: NodeId,
        narrow: impl Fn(&mut Self, TypeId) -> TypeId,
    ) -> TypeId {
        let Some(prop_name) = self.get_accessed_property_name(access) else { return t };
        let optional_chain = self.is_optional_chain(access);
        let non_null_access = self
            .expression_of_access(access)
            .is_some_and(|e| self.nodes.kind(e) == SyntaxKind::NonNullExpression);
        let remove_nullable = self.strict_null_checks
            && (optional_chain || non_null_access)
            && self.maybe_type_of_kind(t, TypeFlags::NULLABLE);
        let non_null_type = if remove_nullable {
            self.get_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL)
        } else {
            t
        };
        let Some(mut prop_type) =
            self.union_property_type_for_discriminant(non_null_type, &prop_name)
        else {
            return t;
        };
        if prop_type == self.intrinsics.error {
            return t;
        }
        if remove_nullable && optional_chain {
            prop_type = self.get_optional_type(prop_type, false);
        }
        let narrowed_prop_type = narrow(self, prop_type);
        let narrowed_is_never = self.store.get(narrowed_prop_type).flags.contains(TypeFlags::NEVER);
        // `filterType` by hand: the predicate needs `&mut self`.
        let TypeData::Union { types: constituents, .. } = &self.store.get(t).data else {
            // A non-union survives whole or becomes `never`; a `never`
            // passes through (`checker.go:26588`); a declined verdict keeps `t`.
            if self.store.get(t).flags.contains(TypeFlags::NEVER) {
                return t;
            }
            return if self.discriminant_keeps(t, &prop_name, narrowed_prop_type, narrowed_is_never)
                == Some(false)
            {
                self.intrinsics.never
            } else {
                t
            };
        };
        let constituents = constituents.clone();
        let mut kept = Vec::with_capacity(constituents.len());
        for constituent in &constituents {
            match self.discriminant_keeps(
                *constituent,
                &prop_name,
                narrowed_prop_type,
                narrowed_is_never,
            ) {
                Some(true) => kept.push(*constituent),
                Some(false) => {}
                None => return t,
            }
        }
        if kept.len() == constituents.len() {
            return t;
        }
        self.rebuild_union_subset(t, &kept)
    }

    /// One constituent of `narrowTypeByDiscriminant`'s filter
    /// (`flow.go:745`): `discriminantType` is the constituent's property or
    /// index-signature type (`unknown` when it has neither), and it keeps
    /// when neither side is `never` and the two are comparable. `None` is
    /// the relater declining both directions.
    fn discriminant_keeps(
        &mut self,
        constituent: TypeId,
        prop_name: &str,
        narrowed_prop_type: TypeId,
        narrowed_is_never: bool,
    ) -> Option<bool> {
        use crate::relater::{Relation, Ternary};
        if narrowed_is_never {
            return Some(false);
        }
        let discriminant_type = self
            .get_type_of_property_or_index_signature_of_type(constituent, prop_name)
            .unwrap_or(self.intrinsics.unknown);
        if discriminant_type == self.intrinsics.error {
            return None;
        }
        if self.store.get(discriminant_type).flags.contains(TypeFlags::NEVER) {
            return Some(false);
        }
        let forward =
            self.relate_ternary(narrowed_prop_type, discriminant_type, Relation::Comparable);
        if forward == Ternary::Related {
            return Some(true);
        }
        let backward =
            self.relate_ternary(discriminant_type, narrowed_prop_type, Relation::Comparable);
        match (forward, backward) {
            (_, Ternary::Related) => Some(true),
            (Ternary::NotRelated, Ternary::NotRelated) => Some(false),
            _ => None,
        }
    }

    /// `getTypeOfPropertyOfType(nonNullType, propName)` as `narrowTypeByDiscriminant`
    /// reads it (`flow.go:736`): on a union that is the SYNTHETIC union
    /// property's type (`createUnionOrIntersectionProperty`, `checker.go:21452`)
    /// — each constituent's property type, or its applicable index signature's
    /// value type, or `undefined` for an object-literal type lacking both
    /// (`WritePartial`); a constituent with none of those makes the property
    /// `ReadPartial`, which `getPropertyOfUnionOrIntersectionType` answers `nil`
    /// for, so `None` here. §750. This port's general
    /// [`Checker::get_type_of_property_of_type`] (§49) declines on ANY missing
    /// constituent, which is what failed `discriminatedUnionTypes2`'s f30
    /// (`{ tag: true } | { tag: false } | { [x: string]: string }`) on the
    /// first pair.
    fn union_property_type_for_discriminant(&mut self, t: TypeId, name: &str) -> Option<TypeId> {
        let TypeData::Union { types, .. } = &self.store.get(t).data else {
            return self.get_type_of_property_of_type(t, name);
        };
        let constituents = types.clone();
        let mut parts = Vec::with_capacity(constituents.len());
        for constituent in constituents {
            let flags = self.store.get(constituent).flags;
            if constituent == self.intrinsics.error || flags.contains(TypeFlags::NEVER) {
                continue;
            }
            let apparent = self.apparent_type(constituent);
            if let Some(prop) = self.get_type_of_property_of_type(apparent, name) {
                parts.push(prop);
                continue;
            }
            let name_type = self.store.intern_literal(
                TypeFlags::STRING_LITERAL,
                TypeData::StringLiteral(name.to_string()),
                false,
            );
            if let Some(info) = self.get_applicable_index_info(apparent, name_type) {
                parts.push(info.value);
            } else if self.is_object_literal_type(apparent) {
                parts.push(self.intrinsics.undefined);
            } else {
                return None;
            }
        }
        if parts.is_empty() {
            return None;
        }
        Some(self.get_union_type(&parts))
    }

    /// `getTypeOfPropertyOrIndexSignatureOfType` (`checker.go`): the named
    /// property's type, else the value type of an index signature applicable
    /// to the name.
    fn get_type_of_property_or_index_signature_of_type(
        &mut self,
        t: TypeId,
        name: &str,
    ) -> Option<TypeId> {
        if let Some(prop) = self.get_type_of_property_of_type(t, name) {
            return Some(prop);
        }
        let name_type = self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral(name.to_string()),
            false,
        );
        self.get_applicable_index_info(t, name_type).map(|info| info.value)
    }

    /// `hasMatchingArgument` (`flow.go:1886`): some argument is, contains,
    /// or optionally chains onto the reference; or the callee is a property
    /// access whose receiver is or contains it (`x.isFoo()` for a `this is`
    /// predicate). §749.
    fn has_matching_argument(
        &mut self,
        state: &FlowState,
        call: &tsr_ast::CallExpression<'_>,
    ) -> bool {
        for argument in call.arguments {
            let Some(id) = tsr_ast::Node::from(*argument).node_id() else { continue };
            if self.is_or_contains_matching_reference(state, id)
                || self.optional_chain_contains_reference(state, id)
            {
                return true;
            }
        }
        if let Some(Node::PropertyAccessExpression(access)) =
            call.expression.and_then(|e| e.node_id()).and_then(|id| self.node_map.get(id))
            && let Some(receiver) = access.expression.and_then(|e| e.node_id())
            && self.is_or_contains_matching_reference(state, receiver)
        {
            return true;
        }
        false
    }

    /// `isOrContainsMatchingReference` (`flow.go:1898`).
    fn is_or_contains_matching_reference(&mut self, state: &FlowState, node: NodeId) -> bool {
        self.is_matching_reference(state, node) || self.contains_matching_reference(state, node)
    }

    /// `ast.SkipParentheses`.
    fn skip_parentheses(&self, mut node: NodeId) -> NodeId {
        while let Some(Node::ParenthesizedExpression(inner)) = self.node_map.get(node)
            && let Some(next) = inner.expression.and_then(|e| e.node_id())
        {
            node = next;
        }
        node
    }

    /// `everyType` (`checker.go:26544`): every union constituent, or the
    /// type itself.
    fn every_type(&mut self, t: TypeId, f: impl Fn(&mut Self, TypeId) -> bool) -> bool {
        match &self.store.get(t).data {
            TypeData::Union { types, .. } => {
                let parts = types.clone();
                parts.into_iter().all(|part| f(self, part))
            }
            _ => f(self, t),
        }
    }

    /// `IsNullableType` (`checker.go:18670`): `hasTypeFacts(t,
    /// TypeFactsIsUndefinedOrNull)`. This port's facts carry `IS_UNDEFINED`
    /// but no `IS_NULL`, so the flag test stands in: a nullable-flagged
    /// type, or `any` (whose facts are `All` upstream). `unknown` is not
    /// (`TypeFactsUnknownFacts` masks the bit out).
    fn is_nullable_type(&self, t: TypeId) -> bool {
        self.store.get(t).flags.intersects(TypeFlags::NULLABLE | TypeFlags::ANY)
    }

    /// §111: the PREDICATE a callee's `[Symbol.hasInstance]` method
    /// declares, when it has exactly one and it is not an assertion. The
    /// member key is the late-bound written bracket text, which is how this
    /// binder files well-known-symbol members.
    fn has_instance_predicate_type(&mut self, callee_type: TypeId) -> Option<TypeId> {
        // §111 residue: composite callees — an INTERSECTION answers through
        // its first predicate-bearing constituent; a UNION requires every
        // constituent to carry one and answers their union (Rhs15's
        // `x is Point`|`x is Line` narrows to Point | Line).
        match &self.store.get(callee_type).data {
            TypeData::Intersection { types, .. } => {
                let constituents = types.clone();
                for constituent in constituents {
                    if let Some(found) = self.has_instance_predicate_type(constituent) {
                        return Some(found);
                    }
                }
                return None;
            }
            TypeData::Union { types, .. } => {
                let constituents = types.clone();
                let mut candidates = Vec::with_capacity(constituents.len());
                for constituent in constituents {
                    candidates.push(self.has_instance_predicate_type(constituent)?);
                }
                return Some(self.get_union_type(&candidates));
            }
            _ => {}
        }
        // Late-bound members are filed `__computed` OUTSIDE the member
        // tables (binder.rs:4136) — the well-known name is found by reading
        // the owner's declarations.
        let owner = match &self.store.get(callee_type).data {
            TypeData::Named { members: Some(owner), .. } => *owner,
            TypeData::Anonymous { symbol, .. } => *symbol,
            _ => return None,
        };
        let declarations = self.binder.symbols().get(owner).declarations.clone();
        for declaration in declarations {
            // SS149a (correcting SS148.3's mis-attribution: Rhs9-13 are
            // PREDICATE-carrying and this leg skipped ClassDeclaration
            // whole): a class's STATIC [Symbol.hasInstance] method carries
            // the predicate the same way.
            if let Some(Node::ClassDeclaration(class)) = self.node_map.get(declaration) {
                for member in class.members {
                    let tsr_ast::ClassElement::MethodDeclaration(method) = member else {
                        continue;
                    };
                    if !method.modifiers.iter().any(|modifier| {
                        matches!(modifier, tsr_ast::ModifierLike::Token(token)
                            if token.kind == SyntaxKind::StaticKeyword)
                    }) {
                        continue;
                    }
                    let tsr_ast::PropertyName::ComputedPropertyName(computed) = method.name else {
                        continue;
                    };
                    let is_has_instance = matches!(
                        computed.expression,
                        Some(tsr_ast::Expression::PropertyAccessExpression(access))
                            if matches!(access.expression,
                                Some(tsr_ast::Expression::Identifier(root))
                                    if root.text == "Symbol")
                                && matches!(access.name,
                                    Some(tsr_ast::MemberName::Identifier(name))
                                        if name.text == "hasInstance")
                    );
                    if !is_has_instance {
                        continue;
                    }
                    let symbol = method.node_id.and_then(|id| self.binder.symbol_of(id))?;
                    let member_type = self.get_type_of_symbol(symbol);
                    let signatures = self.signatures_of_type(member_type)?;
                    let [signature] = signatures.as_slice() else { return None };
                    let predicate = signature.predicate.clone()?;
                    if predicate.asserts {
                        return None;
                    }
                    return predicate.r#type;
                }
                continue;
            }
            let members: &[tsr_ast::TypeElement<'_>] = match self.node_map.get(declaration) {
                Some(Node::TypeLiteralNode(node)) => node.members,
                Some(Node::InterfaceDeclaration(node)) => node.members,
                _ => continue,
            };
            for member in members {
                let tsr_ast::TypeElement::MethodSignatureDeclaration(method) = member else {
                    continue;
                };
                let tsr_ast::PropertyName::ComputedPropertyName(computed) = method.name else {
                    continue;
                };
                // `Symbol.hasInstance` spelled as a property access.
                let is_has_instance = matches!(
                    computed.expression,
                    Some(tsr_ast::Expression::PropertyAccessExpression(access))
                        if matches!(access.expression,
                            Some(tsr_ast::Expression::Identifier(root)) if root.text == "Symbol")
                            && matches!(access.name,
                                Some(tsr_ast::MemberName::Identifier(name))
                                    if name.text == "hasInstance")
                );
                if !is_has_instance {
                    continue;
                }
                let symbol = method.node_id.and_then(|id| self.binder.symbol_of(id))?;
                let member_type = self.get_type_of_symbol(symbol);
                let signatures = self.signatures_of_type(member_type)?;
                let [signature] = signatures.as_slice() else { return None };
                let predicate = signature.predicate.clone()?;
                if predicate.asserts {
                    return None;
                }
                return predicate.r#type;
            }
        }
        None
    }

    /// SS146.1: whether `owner`'s declared base chain (transitive,
    /// `base_symbols_of`) contains `target`. An unfollowable link answers
    /// `false` — the caller then DROPS, which is the oracle's answer for
    /// every non-declared relation in this rung's domain.
    /// SS188 (measured, +0): checker-1's SS202 lesson — "anywhere two callers
    /// share a conservative helper, only one may need the conservatism" —
    /// applied here and measured ZERO. These derivation walks inherit
    /// `base_symbols_of`'s type-argument refusal, which belongs to the
    /// INSTANCE road (a member table cannot be instantiated); derivation
    /// only needs the base SYMBOL, so the refusal is unnecessary. Switching
    /// them to `base_symbols_of_ex(_, false)` moved no case, so the arm
    /// stays as it is — the audit was right in principle and unpaid in this
    /// corpus. Recorded so it is not re-derived.
    pub(crate) fn heritage_chain_contains(
        &mut self,
        owner: tsr_binder::SymbolId,
        target: tsr_binder::SymbolId,
        visiting: &mut Vec<tsr_binder::SymbolId>,
    ) -> bool {
        if visiting.contains(&owner) {
            return false;
        }
        visiting.push(owner);
        // §357 re-ran §188's zero: `base_symbols_of`'s type-argument refusal
        // belongs to the INSTANCE road, and derivation needs only the base
        // SYMBOL (upstream's `hasBaseType` walks `getTargetType`). With the
        // subtype reducer as a new caller the switch stopped being +0:
        // `DerivedList<number> | List<number>` must reduce to `List<number>`
        // (`arrayLiteralsWithRecursiveGenerics`), and the refusal read its
        // generic heritage as "not derived".
        let Some(bases) = self.base_symbols_of_ex(owner, false) else { return false };
        bases
            .into_iter()
            .any(|base| base == target || self.heritage_chain_contains(base, target, visiting))
    }

    fn narrow_by_predicate_type(
        &mut self,
        t: TypeId,
        predicate_type: TypeId,
        assume_true: bool,
    ) -> TypeId {
        // `narrowTypeByTypePredicate`'s any arm (`flow.go`): a declared `any`
        // narrows TO the candidate on the true branch — the §111 probe's 16
        // declared=any flows were running the ladder on `[any]` and keeping
        // it. Identity test: `errorType` shares the flag and must not match.
        if assume_true && t == self.intrinsics.any {
            // ...EXCEPT to the global `Function`/`Object` interfaces —
            // upstream keeps `any` there (`flow.go`'s any arm mirrors the
            // typeof-function rule; `narrowFromAnyWithTypePredicate` wants
            // `any` on `x is Function`, 6 R→W measured without this).
            let keeps_any = ["Function", "Object"].iter().any(|name| {
                self.global_type_symbol_with_arity(name, 0).is_some_and(|symbol| {
                    matches!(
                        self.store.get(predicate_type).data,
                        TypeData::Named { members: Some(owner), .. } if owner == symbol
                    )
                })
            });
            if !keeps_any {
                return predicate_type;
            }
            return t;
        }
        self.narrowed_type_worker(t, predicate_type, assume_true, false).unwrap_or(t)
    }

    /// getNarrowedTypeWorker (internal/checker/flow.go:859).
    /// Preserve an undecidable relation rather than treating a missing relater
    /// operation as a negative answer. The key-property lookup is an optimization.
    fn narrowed_type_worker(
        &mut self,
        mut t: TypeId,
        candidate: TypeId,
        assume_true: bool,
        check_derived: bool,
    ) -> Option<TypeId> {
        use crate::relater::{Relation, Ternary};
        if !assume_true {
            if t == candidate {
                return Some(self.intrinsics.never);
            }
            if check_derived {
                let parts = match self.store.get(t).data.clone() {
                    TypeData::Union { types, .. } => types,
                    _ => vec![t],
                };
                let mut kept = Vec::new();
                for part in parts {
                    if !self.is_derived_from_decidable(part, candidate)? {
                        kept.push(part);
                    }
                }
                return Some(self.rebuild_union_subset(t, &kept));
            }
            if self.store.get(t).flags.contains(TypeFlags::UNKNOWN) {
                t = self.intrinsics.unknown_union;
            }
            let true_type = self.narrowed_type_worker(t, candidate, true, false)?;
            let parts = match self.store.get(t).data.clone() {
                TypeData::Union { types, .. } => types,
                _ => vec![t],
            };
            let kept: Vec<_> = parts
                .into_iter()
                .filter(|&part| !self.is_type_subset_of(part, true_type))
                .collect();
            let result = self.rebuild_union_subset(t, &kept);
            return Some(self.recombine_unknown_type(result));
        }
        if t == candidate || self.store.get(t).flags.intersects(TypeFlags::ANY_OR_UNKNOWN) {
            return Some(candidate);
        }
        let narrowed = self.map_narrowing_type(candidate, &mut |checker, candidate_part| {
            let directly_related = checker.map_narrowing_type(t, &mut |checker, part| {
                if check_derived {
                    return Some(if checker.is_derived_from_decidable(part, candidate_part)? {
                        part
                    } else if checker.is_derived_from_decidable(candidate_part, part)? {
                        candidate_part
                    } else {
                        checker.intrinsics.never
                    });
                }
                match checker.narrowed_constituent(part, candidate_part) {
                    NarrowedConstituent::Mapped(image) => Some(image),
                    NarrowedConstituent::Dropped => Some(checker.intrinsics.never),
                    NarrowedConstituent::Undecidable => None,
                }
            })?;
            if !checker.store.get(directly_related).flags.contains(TypeFlags::NEVER) {
                return Some(directly_related);
            }
            checker.map_narrowing_type(t, &mut |checker, part| {
                if !checker.maybe_type_of_kind(part, TypeFlags::INSTANTIABLE) {
                    return Some(checker.intrinsics.never);
                }
                if let Some(constraint) = checker.base_constraint_of_type(part) {
                    let related = if check_derived {
                        checker.is_derived_from_decidable(candidate_part, constraint)?
                    } else {
                        match checker.relate_ternary(candidate_part, constraint, Relation::Subtype)
                        {
                            Ternary::Related => true,
                            Ternary::NotRelated => false,
                            Ternary::Unknown => return None,
                        }
                    };
                    if !related {
                        return Some(checker.intrinsics.never);
                    }
                }
                Some(checker.get_intersection_type(&[part, candidate_part], None))
            })
        })?;
        if !self.store.get(narrowed).flags.contains(TypeFlags::NEVER) {
            return Some(narrowed);
        }
        for (source, target, relation, image) in [
            (candidate, t, Relation::Subtype, candidate),
            (t, candidate, Relation::Assignable, t),
            (candidate, t, Relation::Assignable, candidate),
        ] {
            match self.relate_ternary(source, target, relation) {
                Ternary::Related => return Some(image),
                Ternary::NotRelated => {}
                Ternary::Unknown => return None,
            }
        }
        Some(self.get_intersection_type(&[t, candidate], None))
    }

    /// mapType (internal/checker/checker.go:25561), preserving union origins
    /// and identity. None propagates an undecidable narrowing relation.
    fn map_narrowing_type(
        &mut self,
        t: TypeId,
        mapper: &mut dyn FnMut(&mut Self, TypeId) -> Option<TypeId>,
    ) -> Option<TypeId> {
        if self.store.get(t).flags.contains(TypeFlags::NEVER) {
            return Some(t);
        }
        let TypeData::Union { types, .. } = &self.store.get(t).data else {
            return mapper(self, t);
        };
        let parts = self
            .union_origin
            .get(&t)
            .filter(|origin| {
                origin.len() != 1
                    || !self.store.get(origin[0]).flags.contains(TypeFlags::INTERSECTION)
            })
            .unwrap_or(types)
            .clone();
        let mut images = Vec::with_capacity(parts.len());
        for &part in &parts {
            images.push(self.map_narrowing_type(part, mapper)?);
        }
        Some(if images == parts { t } else { self.get_union_type(&images) })
    }

    /// One rung of getNarrowedTypeWorker's predicate ladder: map a related
    /// constituent, drop an unrelated one, or preserve an undecidable result.
    fn narrowed_constituent(
        &mut self,
        constituent: TypeId,
        candidate: TypeId,
    ) -> NarrowedConstituent {
        use crate::relater::{Relation, Ternary};
        // SS148 (wall 2 of the SS145.1 census): a declared `object`
        // constituent narrows TO an object-flagged Named/Anonymous
        // candidate - upstream's `subtype(candidate, object)` rung answers
        // the candidate (`x is Point` on `lhs: object` wants `Point`).
        if constituent == self.intrinsics.non_primitive
            && matches!(
                self.store.get(candidate).data,
                TypeData::Named { .. } | TypeData::Anonymous { .. }
            )
            && self.store.get(candidate).flags.intersects(TypeFlags::OBJECT)
        {
            return NarrowedConstituent::Mapped(candidate);
        }
        for relation in [Relation::StrictSubtype, Relation::Subtype] {
            for (source, target, image) in
                [(constituent, candidate, constituent), (candidate, constituent, candidate)]
            {
                match self.relate_ternary(source, target, relation) {
                    Ternary::Related => return NarrowedConstituent::Mapped(image),
                    Ternary::NotRelated => {}
                    Ternary::Unknown => return NarrowedConstituent::Undecidable,
                }
            }
        }
        NarrowedConstituent::Dropped
    }

    /// Whether an interface/type-literal declaration lists a call or
    /// construct signature member (`checker-notes-narrow.md` §23).
    pub(crate) fn declaration_has_call_signature_member(&self, declaration: NodeId) -> bool {
        let Some(node) = self.node_map.get(declaration) else { return false };
        match node {
            Node::InterfaceDeclaration(interface) => interface.members.iter().any(|member| {
                tsr_ast::Node::from(*member).node_id().is_some_and(|id| {
                    matches!(
                        self.nodes.kind(id),
                        tsr_ast::SyntaxKind::CallSignature
                            | tsr_ast::SyntaxKind::ConstructSignature
                    )
                })
            }),
            Node::TypeLiteralNode(literal) => literal.members.iter().any(|member| {
                tsr_ast::Node::from(*member).node_id().is_some_and(|id| {
                    matches!(
                        self.nodes.kind(id),
                        tsr_ast::SyntaxKind::CallSignature
                            | tsr_ast::SyntaxKind::ConstructSignature
                    )
                })
            }),
            _ => false,
        }
    }

    /// SS151: does the expression at `node` read through at least one `?.`
    /// (walking receivers down the access/call chain)?
    fn spells_question_dot_chain(&self, node: NodeId) -> bool {
        let mut current = Some(node);
        while let Some(id) = current {
            match self.node_map.get(id) {
                Some(Node::PropertyAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    current = access.expression.and_then(|e| e.node_id());
                }
                Some(Node::ElementAccessExpression(access)) => {
                    if access.question_dot_token.is_some() {
                        return true;
                    }
                    current = access.expression.and_then(|e| e.node_id());
                }
                Some(Node::CallExpression(call)) => {
                    if call.question_dot_token.is_some() {
                        return true;
                    }
                    current = call.expression.and_then(|e| e.node_id());
                }
                Some(Node::ParenthesizedExpression(inner)) => {
                    current = inner.expression.and_then(|e| e.node_id());
                }
                _ => return false,
            }
        }
        false
    }

    fn narrow_type_by_equality(
        &mut self,
        t: TypeId,
        operator: SyntaxKind,
        value: NodeId,
        assume_true: bool,
        chain_strips_nullable: bool,
    ) -> TypeId {
        // `t.flags&TypeFlagsAny != 0` (`flow.go:557`): `any` narrows to
        // nothing. Identity against the intrinsic rather than the `ANY` flag,
        // because `errorType` carries that flag too and a gap must stay a gap.
        if t == self.intrinsics.any || t == self.intrinsics.error {
            return t;
        }
        // `!=` and `!==` are `==`/`===` with the assumption flipped
        // (`flow.go:560`), which is why there is one rule and not four.
        let negated = matches!(
            operator,
            SyntaxKind::ExclamationEqualsToken | SyntaxKind::ExclamationEqualsEqualsToken
        );
        let assume_true = assume_true != negated;
        let double_equals =
            matches!(operator, SyntaxKind::EqualsEqualsToken | SyntaxKind::ExclamationEqualsToken);

        // Pinned 5b1047d narrowTypeByEquality dispatches by the computed
        // operand's flags, not literal syntax. Keep the literal fast path and
        // the existing Checker-owned NodeId memo/re-entry guard for all other
        // operands. This routes an already-computed nullable leaf through the
        // same facts worker; no new cache, mapper image or forcing is added.
        let value_type = if let Some(literal) = self.nullable_literal_type(value) {
            literal
        } else {
            if !self.narrow_value_stack.insert(value) {
                return t;
            }
            let value_type = if let Some(&cached) = self.narrow_value_types.get(&value) {
                cached
            } else {
                let computed = self
                    .node_map
                    .get(value)
                    .and_then(|node| tsr_ast::Expression::try_from(node).ok())
                    .map_or(self.intrinsics.error, |expression| self.check_expression(expression));
                self.narrow_value_types.insert(value, computed);
                computed
            };
            self.narrow_value_stack.remove(&value);
            value_type
        };
        let value_flags = self.store.get(value_type).flags;
        if !value_flags.intersects(TypeFlags::NULLABLE) {
            // §52 (`checker-notes-narrow.md`): the comparable-filter half
            // (`flow.go:580`) for a NON-nullable value. Loose equality also
            // keeps the exact primitive pairs admitted by native
            // `isCoercibleUnderDoubleEquals`: number/string/boolean-literal
            // sources against number/string/boolean targets. This is
            // constituent-wise; declining the whole union loses
            // `narrowByEquality`'s broad primitive comparisons.
            if value_type == self.intrinsics.error
                || value_flags.intersects(TypeFlags::ANY_OR_UNKNOWN)
            {
                return t;
            }
            // §835: upstream's `unknown` arm (`flow.go:581-588`), which this
            // road never had. On an `unknown`, `x === v` under assume-true
            // narrows to **v's own type** when `v` is primitive or non-primitive,
            // and to **`object`** when `v` is an object type — so
            // `u === NumberEnum.A` gives the enum literal and `u === NumberEnum`
            // gives `object`, because an enum OBJECT is an object type
            // (`unknownType2`).
            //
            // `filter_type` cannot reach this: a non-union `unknown` either
            // survives whole or becomes `never`, so the narrowing was a no-op.
            //
            // The other upstream gate is `someType(t,
            // IsEmptyAnonymousObjectType)`. It matters after filtering
            // `unknown`: `unknown !== undefined` leaves `{} | null`, and a
            // subsequent strict equality to a primitive must narrow to that
            // primitive rather than decline and later recombine to `unknown`
            // (`unknownControlFlow`, the #50706 repro).
            let t_has_empty_anonymous = match self.store.get(t).data.clone() {
                TypeData::Union { types, .. } => {
                    types.into_iter().any(|part| self.is_empty_anonymous_object_type(part))
                }
                _ => self.is_empty_anonymous_object_type(t),
            };
            if assume_true
                && !double_equals
                && (self.store.get(t).flags.intersects(TypeFlags::UNKNOWN) || t_has_empty_anonymous)
            {
                if value_flags.intersects(TypeFlags::PRIMITIVE | TypeFlags::NON_PRIMITIVE)
                    || self.is_empty_anonymous_object_type(value_type)
                {
                    return value_type;
                }
                if value_flags.intersects(TypeFlags::OBJECT) {
                    return self.intrinsics.non_primitive;
                }
            }
            // Pinned 5b1047d flow.go:555 and checker.go:26558 filter named
            // unions too. Existing origin projection retains a nested alias
            // only when all its members survive; undecidable comparisons
            // below still leave the whole input unchanged. No new image/cache.
            let constituents: Vec<TypeId> = match &self.store.get(t).data {
                TypeData::Union { types, .. } => types.clone(),
                _ => vec![t],
            };
            let total = constituents.len();
            let mut kept = Vec::new();
            if assume_true {
                // SS151: a UNION comparand filters constituent-wise -
                // comparable to ANY comparand constituent keeps; all-false
                // drops; any undecidable declines whole (Kleene).
                let comparand_constituents: Vec<TypeId> = match &self.store.get(value_type).data {
                    TypeData::Union { types, .. } => types.clone(),
                    _ => vec![value_type],
                };
                for constituent in constituents {
                    let source_flags = self.store.get(constituent).flags;
                    let coercible_under_double_equals = double_equals
                        && source_flags.intersects(
                            TypeFlags::NUMBER | TypeFlags::STRING | TypeFlags::BOOLEAN_LITERAL,
                        )
                        && value_flags
                            .intersects(TypeFlags::NUMBER | TypeFlags::STRING | TypeFlags::BOOLEAN);
                    if coercible_under_double_equals {
                        kept.push(constituent);
                        continue;
                    }
                    let mut verdict = Some(false);
                    for &comparand in &comparand_constituents {
                        match self.comparable_ternary(constituent, comparand) {
                            Some(true) => {
                                verdict = Some(true);
                                break;
                            }
                            Some(false) => {}
                            None => verdict = None,
                        }
                    }
                    match verdict {
                        Some(true) => kept.push(constituent),
                        Some(false) => {}
                        None => return t,
                    }
                }
            } else {
                // The false branch acts only on a UNIT value: drop the
                // unit-like comparable constituents (`flow.go:601`).
                if !value_flags.intersects(TypeFlags::UNIT) {
                    return t;
                }
                for constituent in constituents {
                    // isUnitLikeType (pinned checker.go:25414) includes
                    // constrained and branded literals, but not an intersection
                    // whose base constraint has reduced to never.
                    let constrained = self.base_constraint_or_type(constituent);
                    let unit_like = match &self.store.get(constrained).data {
                        TypeData::Intersection { types, .. } => types
                            .iter()
                            .any(|&part| self.store.get(part).flags.intersects(TypeFlags::UNIT)),
                        _ => self.store.get(constrained).flags.intersects(TypeFlags::UNIT),
                    };
                    if !unit_like {
                        kept.push(constituent);
                        continue;
                    }
                    match self.comparable_ternary(constituent, value_type) {
                        Some(true) => {}
                        Some(false) => kept.push(constituent),
                        None => return t,
                    }
                }
            }
            if kept.is_empty() || kept.len() == total {
                if kept.is_empty() {
                    // SS155: an emptied filter is `never` on BOTH branches
                    // (upstream's filterType) — the false branch of
                    // `x == 1` on `const x = 1` was returning t, and the
                    // capturedLetConstInLoop family's `never` wants sat
                    // exactly there.
                    return self.intrinsics.never;
                }
                return t;
            }
            let filtered = self.rebuild_union_subset(t, &kept);
            if assume_true {
                let replaced = self.replace_primitives_with_literals(filtered, value_type);
                // SS151: the chain strip, after the filter.
                if chain_strips_nullable && self.strict_null_checks {
                    return self
                        .get_adjusted_type_with_facts(replaced, TypeFacts::NE_UNDEFINED_OR_NULL);
                }
                return replaced;
            }
            return filtered;
        }
        debug_assert!(value_flags.intersects(TypeFlags::NULLABLE));
        // `if !c.strictNullChecks { return t }` (`flow.go:565`). Upstream
        // narrows nothing by a nullable comparison in that mode, because every
        // type can be `null` there and no constituent is removable.
        if !self.strict_null_checks {
            return t;
        }
        // `checker.go:568`–`:578`, arm for arm. `x == null` tests both under
        // `==`'s coercion; `x === null` tests only `null`.
        let facts = if double_equals {
            if assume_true {
                TypeFacts::EQ_UNDEFINED_OR_NULL
            } else {
                TypeFacts::NE_UNDEFINED_OR_NULL
            }
        } else if value_flags.contains(TypeFlags::NULL) {
            if assume_true { TypeFacts::EQ_NULL } else { TypeFacts::NE_NULL }
        } else if assume_true {
            TypeFacts::EQ_UNDEFINED
        } else {
            TypeFacts::NE_UNDEFINED
        };
        self.get_adjusted_type_with_facts(t, facts)
    }

    /// `narrowTypeByInKeyword` (`flow.go:1001`), the known-property half:
    /// when some constituent declares the property, filter by
    /// `isTypePresencePossible`. The unknown-property half intersects with
    /// `Record<X, unknown>` through the global `Record` alias; alias
    /// instantiation is unported, and upstream itself answers `t` unchanged
    /// when that symbol is missing, so the same fallback is taken here by
    /// construction rather than by approximation.
    fn narrow_type_by_in_keyword(&mut self, t: TypeId, name: &str, assume_true: bool) -> TypeId {
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let mut known = false;
        for &constituent in &constituents {
            if self.is_type_presence_possible(constituent, name, true) {
                known = true;
                break;
            }
        }
        if !known {
            return t;
        }
        // `filterType`, unrolled: this port's `filter_type` takes a pure
        // predicate and the presence test needs `&mut self` (the property
        // lookup can instantiate), so the loop is written out with the same
        // identity short-circuit.
        let mut kept = Vec::with_capacity(constituents.len());
        for &constituent in &constituents {
            if self.is_type_presence_possible(constituent, name, assume_true) {
                kept.push(constituent);
            }
        }
        if kept.len() == constituents.len() {
            return t;
        }
        if kept.is_empty() {
            return self.intrinsics.never;
        }
        self.get_union_type(&kept)
    }

    /// `isTypePresencePossible` (`flow.go:1024`): a declared non-optional
    /// property is present exactly when the guard holds; an optional one is
    /// possible either way; an index signature makes both possible; absence
    /// is possible only on the false branch.
    fn is_type_presence_possible(&mut self, t: TypeId, name: &str, assume_true: bool) -> bool {
        if let Some(property) = self.get_property_of_type(t, name) {
            // Upstream reads `SymbolFlagsOptional`, which its binder stamps.
            // This port's binder writes no OPTIONAL flag — the first run of
            // this arm read one anyway, found `false` everywhere, and turned
            // the else-branch of every optional-property guard into `never`
            // (13 lines lost in `strictOptionalProperties1`, the bar's leg 2
            // doing its job). Optionality here lives on the *declaration*, and
            // `is_optional_declaration` is the one reader of it.
            let optional = self
                .binder
                .symbols()
                .get(property)
                .declarations
                .first()
                .copied()
                .is_some_and(|declaration| self.is_optional_declaration(declaration));
            return optional || assume_true;
        }
        // `getStringLiteralType` yields the regular form, which is all the
        // index-applicability test reads.
        let name_type = self.store.intern_literal(
            TypeFlags::STRING_LITERAL,
            TypeData::StringLiteral(name.to_string()),
            false,
        );
        if self.get_applicable_index_info(t, name_type).is_some() {
            return true;
        }
        !assume_true
    }

    /// `narrowTypeByLiteralExpression` (`flow.go:646`): the true branch
    /// narrows by the type the string names, the false branch filters by the
    /// matching `typeofNEFacts` bit (`flow.go:635`), with `TypeofNEHostObject`
    /// for a string outside the standard eight.
    fn narrow_type_by_typeof_literal(
        &mut self,
        t: TypeId,
        literal: &str,
        assume_true: bool,
    ) -> TypeId {
        if !assume_true {
            let facts = match literal {
                "string" => TypeFacts::TYPEOF_NE_STRING,
                "number" => TypeFacts::TYPEOF_NE_NUMBER,
                "bigint" => TypeFacts::TYPEOF_NE_BIG_INT,
                "boolean" => TypeFacts::TYPEOF_NE_BOOLEAN,
                "symbol" => TypeFacts::TYPEOF_NE_SYMBOL,
                "undefined" => TypeFacts::NE_UNDEFINED,
                "object" => TypeFacts::TYPEOF_NE_OBJECT,
                "function" => TypeFacts::TYPEOF_NE_FUNCTION,
                _ => TypeFacts::TYPEOF_NE_HOST_OBJECT,
            };
            return self.get_adjusted_type_with_facts(t, facts);
        }
        // `narrowTypeByTypeName` (`flow.go:657`), arm for arm.
        match literal {
            "string" => {
                let implied = self.intrinsics.string;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_STRING)
            }
            "number" => {
                let implied = self.intrinsics.number;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_NUMBER)
            }
            "bigint" => {
                let implied = self.intrinsics.bigint;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_BIG_INT)
            }
            "boolean" => {
                let implied = self.intrinsics.boolean;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_BOOLEAN)
            }
            "symbol" => {
                let implied = self.intrinsics.es_symbol;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_SYMBOL)
            }
            "object" => {
                // `if t.flags&TypeFlagsAny != 0 { return t }` (`flow.go:670`) —
                // a **flags** test, and this port wrote `t == anyType`, an
                // identity one. Upstream's `errorType` is
                // `newIntrinsicType(TypeFlagsAny, "error")` and so are
                // `wildcardType` and `blockedStringType`
                // (`crate::intrinsics`); every one of them is excluded by an
                // identity comparison and admitted by the flag.
                //
                // The cost was a **manufactured** type: a receiver this port
                // could not resolve went into `typeof x === "object"` as
                // `errorType` and came out as `object | null`, so the very next
                // access reported TS18047 — a `null` that exists in no program,
                // invented by a narrowing of a gap. Three separate real-repo
                // reports traced back here. §6 of `checker-notes-nnaccess.md`.
                if self.type_of(t).flags.intersects(crate::flags::TypeFlags::ANY) {
                    return t;
                }
                let non_primitive = self.intrinsics.non_primitive;
                let null = self.intrinsics.null;
                let object_half =
                    self.narrow_type_by_type_facts(t, non_primitive, TypeFacts::TYPEOF_EQ_OBJECT);
                let null_half = self.narrow_type_by_type_facts(t, null, TypeFacts::EQ_NULL);
                self.get_union_type(&[object_half, null_half])
            }
            "function" => {
                // The same flags test one arm down (`flow.go:675`).
                if self.type_of(t).flags.intersects(crate::flags::TypeFlags::ANY) {
                    return t;
                }
                // `c.globalFunctionType`. A lib-less program has no `Function`
                // interface; narrowing is then declined rather than
                // approximated with a made-up type.
                //
                // **§231: the arity is 0 and this asked for 1, so the decline
                // above was unconditional.** `global_type_symbol(name)` is
                // `global_type_symbol_with_arity(name, 1)`, and `interface
                // Function` (`es5.d.ts:257`) takes no type parameters — so the
                // `else` arm fired for *every* program, lib or not, and this
                // narrowing had never once run. The comment describing the
                // decline as the lib-less case was written in good faith and
                // was true of nothing.
                let Some(function_symbol) = self.global_type_symbol_with_arity("Function", 0)
                else {
                    return t;
                };
                let implied = self.get_declared_type_of_symbol(function_symbol);
                if implied == self.intrinsics.error {
                    return t;
                }
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_FUNCTION)
            }
            "undefined" => {
                let implied = self.intrinsics.undefined;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::EQ_UNDEFINED)
            }
            _ => {
                let implied = self.intrinsics.non_primitive;
                self.narrow_type_by_type_facts(t, implied, TypeFacts::TYPEOF_EQ_HOST_OBJECT)
            }
        }
    }

    /// `narrowTypeByTypeFacts` (`flow.go:687`): each constituent keeps itself,
    /// collapses to the implied type, intersects with it, or leaves — decided
    /// by the strict-subtype relation and the facts bit.
    fn narrow_type_by_type_facts(
        &mut self,
        t: TypeId,
        implied: TypeId,
        facts: TypeFacts,
    ) -> TypeId {
        let never = self.intrinsics.never;
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let mut mapped = Vec::with_capacity(constituents.len());
        for constituent in constituents {
            let image = if self.is_type_related_to(
                constituent,
                implied,
                crate::relater::Relation::StrictSubtype,
            ) {
                if self.get_type_facts(constituent).intersects(facts) { constituent } else { never }
            } else if self.is_type_subtype_of(implied, constituent) {
                implied
            } else if self.get_type_facts(constituent).intersects(facts) {
                self.get_intersection_type(&[constituent, implied], None)
            } else {
                never
            };
            if image != never {
                mapped.push(image);
            }
        }
        if mapped.is_empty() {
            return never;
        }
        self.get_union_type(&mapped)
    }

    /// The type of a **literally written** `null` or `undefined` operand, or
    /// `None` for anything else.
    ///
    /// `undefined` is an `Identifier` resolving to the synthesised global
    /// (`crate::checker`'s `undefined_symbol` seeding), not a keyword, so it
    /// is matched by name against that symbol rather than by syntax — a local
    /// called `undefined` shadowing it must not be read as the literal.
    fn nullable_literal_type(&self, value: NodeId) -> Option<TypeId> {
        match self.node_map.get(value)? {
            // `null` is a keyword in the grammar, `undefined` an identifier —
            // the same asymmetry `crate::expressions` documents at its own
            // literal arm.
            Node::KeywordExpression(node) if node.kind == SyntaxKind::NullKeyword => {
                Some(self.intrinsics.null)
            }
            Node::Identifier(identifier) if identifier.text == "undefined" => {
                let symbol = self.binder.resolve_name(
                    self.nodes,
                    self.node_map,
                    value,
                    identifier.text,
                    SymbolFlags::VALUE,
                )?;
                (Some(symbol) == self.binder.undefined_symbol())
                    .then_some(self.intrinsics.undefined)
            }
            _ => None,
        }
    }

    /// `getTypeWithFacts` (`checker.go:31150`): filter without changing the
    /// structure of the surviving constituents.
    pub(crate) fn get_type_with_facts(&mut self, t: TypeId, facts: TypeFacts) -> TypeId {
        self.filter_type(t, |checker, constituent| {
            checker.get_type_facts(constituent).intersects(facts)
        })
    }

    /// `getAdjustedTypeWithFacts` (`checker.go:31159`): expand unknown, filter,
    /// then remove nullable values from surviving instantiable constituents.
    pub(crate) fn get_adjusted_type_with_facts(&mut self, t: TypeId, facts: TypeFacts) -> TypeId {
        let expanded =
            if self.strict_null_checks && self.store.get(t).flags.contains(TypeFlags::UNKNOWN) {
                self.intrinsics.unknown_union
            } else {
                t
            };
        let reduced = self.get_type_with_facts(expanded, facts);
        let reduced = self.recombine_unknown_type(reduced);
        if !self.strict_null_checks {
            return reduced;
        }
        match facts {
            TypeFacts::NE_UNDEFINED => self.remove_nullable_by_intersection(
                reduced,
                TypeFacts::EQ_UNDEFINED,
                TypeFacts::EQ_NULL,
                TypeFacts::IS_NULL,
                self.intrinsics.null,
            ),
            TypeFacts::NE_NULL => self.remove_nullable_by_intersection(
                reduced,
                TypeFacts::EQ_NULL,
                TypeFacts::EQ_UNDEFINED,
                TypeFacts::IS_UNDEFINED,
                self.intrinsics.undefined,
            ),
            TypeFacts::NE_UNDEFINED_OR_NULL | TypeFacts::TRUTHY => {
                let constituents = match self.store.get(reduced).data.clone() {
                    crate::types::TypeData::Union { types, .. } => types,
                    _ => vec![reduced],
                };
                let mapped: Vec<_> = constituents
                    .iter()
                    .map(|&part| {
                        if self.get_type_facts(part).intersects(TypeFacts::EQ_UNDEFINED_OR_NULL) {
                            let result = self.get_global_non_nullable_type_instantiation(part);
                            self.record_non_null_refinement(result, part);
                            result
                        } else {
                            part
                        }
                    })
                    .collect();
                if mapped == constituents { reduced } else { self.get_union_type(&mapped) }
            }
            _ => reduced,
        }
    }

    /// `recombineUnknownType` (`checker.go:31201`): restore unknown after its
    /// three distinct constituents meet at a flow join or survive filtering.
    fn recombine_unknown_type(&self, t: TypeId) -> TypeId {
        if t == self.intrinsics.unknown_union { self.intrinsics.unknown } else { t }
    }

    /// `removeNullableByIntersection` (`checker.go:31179`). Whether the whole
    /// union already includes the opposite nullable determines each intersection.
    fn remove_nullable_by_intersection(
        &mut self,
        t: TypeId,
        target: TypeFacts,
        other: TypeFacts,
        includes_other: TypeFacts,
        other_type: TypeId,
    ) -> TypeId {
        let facts = self.get_type_facts(t);
        if !facts.intersects(target) {
            return t;
        }
        let empty = self.intrinsics.empty_object;
        let empty_and_other = self.get_union_type(&[empty, other_type]);
        let constituents = match self.store.get(t).data.clone() {
            crate::types::TypeData::Union { types, .. } => types,
            _ => vec![t],
        };
        let mapped: Vec<_> = constituents
            .iter()
            .map(|&part| {
                let part_facts = self.get_type_facts(part);
                if !part_facts.intersects(target) {
                    return part;
                }
                let preserve_other =
                    !facts.intersects(includes_other) && part_facts.intersects(other);
                let result = self.get_intersection_type(
                    &[part, if preserve_other { empty_and_other } else { empty }],
                    None,
                );
                self.record_non_null_refinement(result, part);
                result
            })
            .collect();
        if mapped == constituents { t } else { self.get_union_type(&mapped) }
    }

    /// Carry semantic subtypes back to their original variable for the port's
    /// limited union join reduction. Never register a shared primitive result.
    fn record_non_null_refinement(&mut self, result: TypeId, base: TypeId) {
        let parts = match self.store.get(result).data.clone() {
            TypeData::Union { types, .. } => types,
            _ => vec![result],
        };
        if !parts.iter().all(|&part| {
            matches!(&self.store.get(part).data,
            TypeData::Intersection { types, .. } if types.contains(&base))
        }) {
            return;
        }
        let base = self.non_null_refinement_bases.get(&base).map_or(base, |&base| base);
        for part in parts.into_iter().chain(std::iter::once(result)) {
            if part != base {
                self.non_null_refinement_bases.insert(part, base);
            }
        }
    }

    /// Keep the constituents of a union that satisfy `predicate`
    /// (`filterType`, `checker.go:26362`).
    ///
    /// A non-union either survives whole or becomes `never`, which is upstream's
    /// behaviour and is what makes `if (x)` on a plain `undefined` give `never`.
    pub(crate) fn filter_type(
        &mut self,
        t: TypeId,
        mut predicate: impl FnMut(&mut Self, TypeId) -> bool,
    ) -> TypeId {
        let constituents: Option<Vec<TypeId>> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => Some(types.clone()),
            _ => None,
        };
        if let Some(constituents) = constituents {
            let kept: Vec<TypeId> =
                constituents.iter().copied().filter(|c| predicate(self, *c)).collect();
            // Identity when nothing was removed, so `getTypeAtFlowCondition`'s
            // `narrowed == t` short-circuit fires and the walk keeps the
            // original type rather than an equal rebuild.
            if kept.len() == constituents.len() {
                return t;
            }
            // §53: an origin-carrying union projects its entries.
            return self.rebuild_union_subset(t, &kept);
        }
        // `checker.go:26588`: a `never`-flagged type passes through UNCHANGED
        // — which is what carries the `unreachableNeverType` sentinel across
        // `x === undefined` (§745, `assertionTypePredicates1`'s
        // `assert(false && x === undefined)`): the walk exit prints the
        // declared type only if the sentinel is still the sentinel.
        if self.store.get(t).flags.contains(TypeFlags::NEVER) || predicate(self, t) {
            t
        } else {
            self.intrinsics.never
        }
    }

    /// The second half of `isFunctionObjectType` (checker.go:31140): a type
    /// without signatures is still function-like when it has a `bind` member
    /// and is a subtype of the global `Function` — the lib's `Function`
    /// interface itself, and interfaces extending it. Only a decided subtype
    /// answer counts; an undecidable relation keeps `ObjectStrictFacts`.
    fn is_bind_bearing_function_subtype(&mut self, t: TypeId) -> bool {
        if self.get_property_of_type(t, "bind").is_none() {
            return false;
        }
        let Some(function_symbol) = self.global_type_symbol_with_arity("Function", 0) else {
            return false;
        };
        let function = self.get_declared_type_of_symbol(function_symbol);
        function != self.intrinsics.error
            && (t == function
                || self.relate_ternary(t, function, crate::relater::Relation::Subtype)
                    == crate::relater::Ternary::Related)
    }

    /// What is knowable about a type without narrowing it
    /// (`getTypeFacts`, `checker.go:30982`).
    ///
    /// Instantiable types use their base constraint. Empty anonymous objects
    /// admit falsy primitives; other objects use the native object facts.
    /// Unsupported representations retain both truthiness possibilities.
    pub(crate) fn get_type_facts(&mut self, t: TypeId) -> TypeFacts {
        let t = if self
            .store
            .get(t)
            .flags
            .intersects(TypeFlags::INTERSECTION | TypeFlags::INSTANTIABLE)
            || self.deferred_keyof_operands.contains_key(&t)
        {
            self.base_constraint_of_type(t).unwrap_or(self.intrinsics.unknown)
        } else {
            t
        };
        if self.is_empty_anonymous_object_type(t) {
            let nullable =
                TypeFacts::EQ_UNDEFINED | TypeFacts::EQ_NULL | TypeFacts::EQ_UNDEFINED_OR_NULL;
            let facts = TypeFacts::all() - TypeFacts::IS_UNDEFINED - TypeFacts::IS_NULL;
            return if self.strict_null_checks { facts - nullable } else { facts };
        }
        // SS201 `getIntersectionTypeFacts` (checker.go:31118-31134),
        // TRANSCRIBED after SS200's induced version failed:
        //
        // ```go
        // ignoreObjects := c.maybeTypeOfKind(t, TypeFlagsPrimitive)
        // oredFacts := None; andedFacts := All
        // for _, t := range t.Types() {
        //     if !(ignoreObjects && t.flags&TypeFlagsObject != 0) {
        //         f := getTypeFactsWorker(t, ...)
        //         oredFacts |= f; andedFacts &= f
        //     }
        // }
        // return oredFacts&OrFactsMask | andedFacts&AndFactsMask
        // ```
        //
        // Two rules SS200's plain AND had neither of: an intersection
        // holding a PRIMITIVE ignores its object constituents outright (they
        // are type tags — `string & { __kind__: "name" }`), which is
        // witness 1; and the fold is OR for exactly two bits
        // (`TypeofEQFunction | TypeofNEObject`, checker.go:478) and AND for
        // every other, which is witness 2.
        if let TypeData::Intersection { types, .. } = &self.store.get(t).data {
            let constituents = types.clone();
            let ignore_objects = constituents
                .iter()
                .any(|&c| self.store.get(c).flags.intersects(TypeFlags::PRIMITIVE));
            let or_mask = TypeFacts::TYPEOF_EQ_FUNCTION | TypeFacts::TYPEOF_NE_OBJECT;
            let mut ored = TypeFacts::empty();
            let mut anded = TypeFacts::all();
            let mut counted = false;
            for constituent in constituents {
                let is_object = self.store.get(constituent).flags.intersects(TypeFlags::OBJECT);
                if ignore_objects && is_object {
                    continue;
                }
                let facts = self.get_type_facts(constituent);
                ored |= facts;
                anded &= facts;
                counted = true;
            }
            if counted {
                return (ored & or_mask) | (anded & !or_mask);
            }
        }

        // SS200 (measured, reverted — an INDUCED rule, not a transcribed
        // one): an intersection currently falls to the undecidable default
        // (every bit) and so survives EVERY typeof query. Folding the
        // constituents' facts with plain AND fixes one witness and breaks
        // the other, both in `narrowingTypeofObject`:
        //   `x: number & { _foo: string }` under `typeof x === 'object'`
        //     wants `never` — AND gives it, because `number`'s facts carry
        //     no TYPEOF_EQ_OBJECT.
        //   `x: F & { foo: number }` (F is `{ (): string }`) under the
        //     function query wants the intersection KEPT — AND drops it,
        //     because `{ foo: number }`'s ObjectFacts carry no
        //     TYPEOF_EQ_FUNCTION while `F`'s FunctionFacts do.
        // So upstream's `getIntersectionTypeFacts` is not a plain AND over
        // all bits; the EQ bits and the NE bits compose differently. The next
        // attempt must READ that function — inducing the composition rule is
        // exactly what SS153/SS157/SS162/SS177/SS197 each cost.
        //
        // Upstream carries these as one aggregate per kind of type
        // (`TypeFactsUndefinedFacts` `checker.go:471`, `TypeFactsNullFacts`
        // `:472`, and the `NEUndefined | NENull | NEUndefinedOrNull` shared by
        // every `Base*StrictFacts`). Named here rather than spelled at each
        // return so the three sets can be read against upstream's three
        // constants.
        //
        // Object/non-primitive aggregates below select the null mode: their
        // loose nullable and falsy bits also affect truthiness narrowing,
        // even though loose equality narrowing does not ask for those bits.
        // Primitive aggregates retain their existing strict-mode limitation.
        let nullable_never =
            TypeFacts::NE_UNDEFINED | TypeFacts::NE_NULL | TypeFacts::NE_UNDEFINED_OR_NULL;
        let undefined_facts =
            TypeFacts::EQ_UNDEFINED | TypeFacts::EQ_UNDEFINED_OR_NULL | TypeFacts::NE_NULL;
        let null_facts =
            TypeFacts::EQ_NULL | TypeFacts::EQ_UNDEFINED_OR_NULL | TypeFacts::NE_UNDEFINED;
        // The eight `typeof` NE bits together — every `Base*StrictFacts`
        // aggregate is "EQ of my own kind, NE of the other seven", which reads
        // as `typeof_eq(kind) | (typeof_ne_all - typeof_ne(kind))` below
        // (`checker.go:432`-region, bit for bit).
        let typeof_ne_all = TypeFacts::TYPEOF_NE_STRING
            | TypeFacts::TYPEOF_NE_NUMBER
            | TypeFacts::TYPEOF_NE_BIG_INT
            | TypeFacts::TYPEOF_NE_BOOLEAN
            | TypeFacts::TYPEOF_NE_SYMBOL
            | TypeFacts::TYPEOF_NE_OBJECT
            | TypeFacts::TYPEOF_NE_FUNCTION
            | TypeFacts::TYPEOF_NE_HOST_OBJECT;
        // The undecidable default: **every** bit, so `filter_type` keeps the
        // constituent under any query. A wrong `NE` claim deletes a
        // constituent and prints a confident wrong type; the default must
        // never be the one that does that.
        let both = TypeFacts::TRUTHY
            | TypeFacts::FALSY
            | nullable_never
            | undefined_facts
            | null_facts
            | typeof_ne_all
            | TypeFacts::TYPEOF_EQ_STRING
            | TypeFacts::TYPEOF_EQ_NUMBER
            | TypeFacts::TYPEOF_EQ_BIG_INT
            | TypeFacts::TYPEOF_EQ_BOOLEAN
            | TypeFacts::TYPEOF_EQ_SYMBOL
            | TypeFacts::TYPEOF_EQ_OBJECT
            | TypeFacts::TYPEOF_EQ_FUNCTION
            | TypeFacts::TYPEOF_EQ_HOST_OBJECT;
        let ty = self.store.get(t);
        let flags = ty.flags;

        if flags.contains(TypeFlags::NEVER) {
            return TypeFacts::empty();
        }
        // A union's facts are the **union** of its constituents'
        // (`getTypeFactsWorker`, `checker.go:30987`). Without this arm a union
        // falls to the `_ => both` default below, which is the safe answer for
        // *narrowing* — where this function is only ever called on a
        // constituent, so the arm is unreachable from `filter_type` — and the
        // wrong answer for [`Checker::check_logical_and`], which asks about the
        // whole left operand. `undefined | null` must report `FALSY` alone.
        if let TypeData::Union { types, .. } = &ty.data {
            let types = types.clone();
            return types
                .iter()
                .fold(TypeFacts::empty(), |facts, &c| facts | self.get_type_facts(c));
        }
        // `undefined`, `null` and `void` are the whole of the falsy-only set
        // among the types this checker builds. They split three ways on the
        // nullable bits and **`void` goes with `undefined`, not with `null`**:
        // `TypeFactsVoidFacts` (`checker.go:470`) carries `EQUndefined |
        // EQUndefinedOrNull | NENull`, character for character
        // `TypeFactsUndefinedFacts` minus `IsUndefined`. Reading `NULLABLE |
        // VOID` as one bucket — which is what the truthiness answer above
        // allowed — would make `x !== null` fail to narrow `void` away.
        if flags.intersects(TypeFlags::UNDEFINED | TypeFlags::VOID) {
            // `TypeFactsUndefinedFacts`/`VoidFacts`: `typeof undefined` is none
            // of the eight strings' EQ side — all eight NE bits, no EQ. The
            // two sets differ by exactly `IsUndefined`, which `undefined`
            // carries and `void` does not (`checker.go:470`–`:471`).
            let is_undefined = if flags.contains(TypeFlags::UNDEFINED) {
                TypeFacts::IS_UNDEFINED
            } else {
                TypeFacts::empty()
            };
            return TypeFacts::FALSY | undefined_facts | typeof_ne_all | is_undefined;
        }
        if flags.contains(TypeFlags::NULL) {
            // `TypeFactsNullFacts`: `typeof null === "object"`, so EQ_OBJECT
            // joins and NE_OBJECT does **not** — the one aggregate whose NE
            // set is seven bits, not eight.
            return TypeFacts::FALSY
                | null_facts
                | TypeFacts::IS_NULL
                | TypeFacts::TYPEOF_EQ_OBJECT
                | (typeof_ne_all - TypeFacts::TYPEOF_NE_OBJECT);
        }
        // A symbol is `typeof … === "symbol"` (`TypeFactsSymbolStrictFacts`).
        if flags.intersects(TypeFlags::ES_SYMBOL) {
            return TypeFacts::TRUTHY
                | nullable_never
                | TypeFacts::TYPEOF_EQ_SYMBOL
                | (typeof_ne_all - TypeFacts::TYPEOF_NE_SYMBOL);
        }
        // An object or a non-primitive is always truthy and never nullable in
        // strict mode; loose native facts also admit falsy/nullable values.
        // Which `typeof` family it carries splits by shape, and the split errs
        // toward `both` — a wrong NE bit deletes a constituent:
        //
        // - a signature-shaped type (`Anonymous { signature: true }`, or one
        //   whose call signatures are recorded in `signature_types`) is
        //   upstream's `isFunctionObjectType` → `FunctionStrictFacts`;
        // - `typeof C` for a **class** is a constructor — `"function"` too;
        // - `typeof E` / `typeof N` for an enum or value module is a plain
        //   object at runtime → `ObjectStrictFacts`;
        // - a `Named` type with a members table cannot carry call signatures
        //   in this port (the binder deliberately files no `__call`,
        //   `binder.rs:3482`); its declared call-signature members, or
        //   isFunctionObjectType's `bind` + subtype-of-`Function` half, choose
        //   `FunctionStrictFacts`, and otherwise `ObjectStrictFacts`;
        // - remaining object representations use ObjectStrictFacts.
        if flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE) {
            let mode_facts = if self.strict_null_checks {
                TypeFacts::empty()
            } else {
                TypeFacts::FALSY
                    | TypeFacts::EQ_UNDEFINED
                    | TypeFacts::EQ_NULL
                    | TypeFacts::EQ_UNDEFINED_OR_NULL
            };
            let object_strict = TypeFacts::TRUTHY
                | nullable_never
                | TypeFacts::TYPEOF_EQ_OBJECT
                | TypeFacts::TYPEOF_EQ_HOST_OBJECT
                | (typeof_ne_all - TypeFacts::TYPEOF_NE_OBJECT - TypeFacts::TYPEOF_NE_HOST_OBJECT)
                | mode_facts;
            let function_strict = TypeFacts::TRUTHY
                | nullable_never
                | TypeFacts::TYPEOF_EQ_FUNCTION
                | TypeFacts::TYPEOF_EQ_HOST_OBJECT
                | (typeof_ne_all
                    - TypeFacts::TYPEOF_NE_FUNCTION
                    - TypeFacts::TYPEOF_NE_HOST_OBJECT)
                | mode_facts;
            if flags.intersects(TypeFlags::NON_PRIMITIVE) {
                return object_strict;
            }
            return match &ty.data {
                TypeData::Anonymous { signature: true, .. } => function_strict,
                _ if self.signature_types.contains_key(&t) => function_strict,
                TypeData::Anonymous { symbol, .. } => {
                    let symbol_flags = self.binder.symbols().get(*symbol).flags;
                    if symbol_flags.intersects(SymbolFlags::CLASS) {
                        function_strict
                    } else {
                        object_strict
                    }
                }
                // A named interface WITH call/construct signatures is
                // `typeof === "function"` — the lib's `Function` interface
                // is the head case (`checker-notes-narrow.md` §23). The
                // members field is a symbol; the signature question is asked
                // of its declarations' member lists.
                TypeData::Named { members: Some(symbol), .. } => {
                    let has_call_signature =
                        self.binder.symbols().get(*symbol).declarations.iter().any(
                            |&declaration| self.declaration_has_call_signature_member(declaration),
                        );
                    if has_call_signature || self.is_bind_bearing_function_subtype(t) {
                        function_strict
                    } else {
                        object_strict
                    }
                }
                _ => object_strict,
            };
        }
        // Every arm below is a **non-nullable** type, so each carries
        // `nullable_never` beside its truthiness — the `Base*StrictFacts`
        // half. The one type that must not reach here is `any`, which is
        // undecidable on both axes and falls to the default.
        let truthiness = match &ty.data {
            TypeData::StringLiteral(value)
            | TypeData::EnumLiteral {
                value: crate::types::EnumLiteralValue::String(value), ..
            } => {
                if value.is_empty() {
                    TypeFacts::FALSY
                } else {
                    TypeFacts::TRUTHY
                }
            }
            // The stored form is the *printed* form, so the falsy values are
            // exactly the spellings of zero a `.types` baseline would print.
            TypeData::NumberLiteral(value)
            | TypeData::EnumLiteral {
                value: crate::types::EnumLiteralValue::Number(value), ..
            } => {
                if matches!(value.as_str(), "0" | "-0") {
                    TypeFacts::FALSY
                } else {
                    TypeFacts::TRUTHY
                }
            }
            TypeData::BigIntLiteral(value) => {
                if matches!(value.as_str(), "0n" | "-0n") {
                    TypeFacts::FALSY
                } else {
                    TypeFacts::TRUTHY
                }
            }
            TypeData::BooleanLiteral(value) => {
                if *value {
                    TypeFacts::TRUTHY
                } else {
                    TypeFacts::FALSY
                }
            }
            // `string`, `number`, `bigint`, `boolean`: truthiness is
            // undecidable, nullability is not — a primitive is never `null` or
            // `undefined` under `strictNullChecks`, which is what
            // `BaseStringStrictFacts` and its siblings say.
            _ if flags.intersects(
                TypeFlags::STRING_LIKE
                    | TypeFlags::NUMBER_LIKE
                    | TypeFlags::BIG_INT_LIKE
                    | TypeFlags::BOOLEAN_LIKE,
            ) =>
            {
                TypeFacts::TRUTHY | TypeFacts::FALSY
            }
            // `any`, `unknown`, a type parameter, an unresolved name: nothing
            // is decidable, so every bit — see the note on `both`.
            _ => return both,
        };
        // Native BaseStringFacts/BaseNumberFacts admit falsy nullish values in
        // non-strict mode. Apply this to the new enum-literal payload domain;
        // the older ordinary-literal fact path retains its documented limit.
        let truthiness = if !self.strict_null_checks && flags.intersects(TypeFlags::ENUM_LITERAL) {
            truthiness | TypeFacts::FALSY
        } else {
            truthiness
        };
        // The `typeof` half of the `Base*StrictFacts` aggregate the arm above
        // belongs to, decided by the same flags that decided the arm.
        let typeof_family = if flags.intersects(TypeFlags::STRING_LIKE) {
            TypeFacts::TYPEOF_EQ_STRING | (typeof_ne_all - TypeFacts::TYPEOF_NE_STRING)
        } else if flags.intersects(TypeFlags::NUMBER_LIKE) {
            TypeFacts::TYPEOF_EQ_NUMBER | (typeof_ne_all - TypeFacts::TYPEOF_NE_NUMBER)
        } else if flags.intersects(TypeFlags::BIG_INT_LIKE) {
            TypeFacts::TYPEOF_EQ_BIG_INT | (typeof_ne_all - TypeFacts::TYPEOF_NE_BIG_INT)
        } else if flags.intersects(TypeFlags::BOOLEAN_LIKE) {
            TypeFacts::TYPEOF_EQ_BOOLEAN | (typeof_ne_all - TypeFacts::TYPEOF_NE_BOOLEAN)
        } else {
            // Unreachable while the match above returns `both` for everything
            // else; kept total rather than panicking, and kept SAFE.
            both
        };
        truthiness | nullable_never | typeof_family
    }

    /// Whether a symbol is one upstream would narrow a reference to.
    ///
    /// `checkIdentifier` reaches `getNarrowedTypeOfSymbol` for variables and
    /// parameters; a reference to a class, interface, enum or function is not
    /// narrowed, and asking anyway would be answering a question upstream does
    /// not ask.
    pub(crate) fn is_narrowable_symbol(&self, symbol: SymbolId) -> bool {
        // §843: an ALIAS narrows too. `checkIdentifier`
        // (`vendor/typescript-go/internal/checker/checker.go:11104-11118`) has
        // **three** outcomes, not two, and its own comment names only the first:
        //
        // ```go
        // isAlias := localOrExportSymbol.Flags&ast.SymbolFlagsAlias != 0
        // // We only narrow variables and parameters occurring in a non-assignment position. For all other
        // // entities we simply return the declared type.
        // if localOrExportSymbol.Flags&ast.SymbolFlagsVariable != 0 {
        //     ...
        // } else if isAlias {
        //     declaration = c.getDeclarationOfAliasSymbol(symbol)
        // } else {
        //     return t
        // }
        // ```
        //
        // An imported binding is an alias, so `if (a0) x = a0` narrows `a0`
        // from `number | undefined` to `number` upstream
        // (`narrowedImports.types` A11). Testing `VARIABLE` alone put every
        // alias into upstream's *third* outcome.
        //
        // Reading the comment rather than the code would have kept this bug:
        // it says "variables and parameters", and the alias arm is the line
        // below it.
        self.binder
            .symbols()
            .get(symbol)
            .flags
            .intersects(SymbolFlags::VARIABLE | SymbolFlags::ALIAS)
    }
}

/// The property name an access expression reads, for the two forms
/// `getAccessedPropertyName` (`flow.go`) decides.
///
/// A property access answers its member name; an element access answers only a
/// **string-literal** argument, because `a[i]` names a property only when `i`
/// is constant, and this port cannot prove that (see
/// [`Checker::references_match`]).
/// `ast.IsStringLiteralLike`'s text: a string literal or a template literal
/// without substitutions.
fn string_literal_like_text(node: Node<'_>) -> Option<&str> {
    match node {
        Node::StringLiteral(literal) => Some(literal.text),
        Node::NoSubstitutionTemplateLiteral(literal) => Some(literal.text),
        _ => None,
    }
}

fn accessed_property_name(node: Node<'_>) -> Option<String> {
    match node {
        Node::PropertyAccessExpression(access) => match access.name? {
            tsr_ast::MemberName::Identifier(name) => Some(name.text.to_string()),
            tsr_ast::MemberName::PrivateIdentifier(name) => Some(name.text.to_string()),
        },
        Node::ElementAccessExpression(access) => match access.argument_expression? {
            tsr_ast::Expression::StringLiteral(literal) => Some(literal.text.to_string()),
            _ => None,
        },
        _ => None,
    }
}

/// `ast.IsLeftHandSideExpression` (ast/utilities.go:391), used by
/// isMatchingReference's assignment target arm even for recovered syntax.
fn is_left_hand_side_expression(expression: tsr_ast::Expression<'_>) -> bool {
    use tsr_ast::Expression;
    match expression {
        Expression::PartiallyEmittedExpression(inner) => {
            inner.expression.is_some_and(is_left_hand_side_expression)
        }
        Expression::KeywordExpression(keyword) => matches!(
            keyword.kind,
            SyntaxKind::FalseKeyword
                | SyntaxKind::NullKeyword
                | SyntaxKind::ThisKeyword
                | SyntaxKind::TrueKeyword
                | SyntaxKind::SuperKeyword
                | SyntaxKind::ImportKeyword
        ),
        _ => matches!(
            expression,
            Expression::PropertyAccessExpression(_)
                | Expression::ElementAccessExpression(_)
                | Expression::NewExpression(_)
                | Expression::CallExpression(_)
                | Expression::JsxElement(_)
                | Expression::JsxSelfClosingElement(_)
                | Expression::JsxFragment(_)
                | Expression::TaggedTemplateExpression(_)
                | Expression::ArrayLiteralExpression(_)
                | Expression::ParenthesizedExpression(_)
                | Expression::ObjectLiteralExpression(_)
                | Expression::ClassExpression(_)
                | Expression::FunctionExpression(_)
                | Expression::Identifier(_)
                | Expression::PrivateIdentifier(_)
                | Expression::RegularExpressionLiteral(_)
                | Expression::NumericLiteral(_)
                | Expression::BigIntLiteral(_)
                | Expression::StringLiteral(_)
                | Expression::NoSubstitutionTemplateLiteral(_)
                | Expression::TemplateExpression(_)
                | Expression::NonNullExpression(_)
                | Expression::ExpressionWithTypeArguments(_)
                | Expression::MetaProperty(_)
        ),
    }
}

impl Checker<'_, '_> {
    /// TS2534 / TS2355 / TS2366 / TS7030 —
    /// `checkAllCodePathsInNonVoidFunctionReturnOrThrow` (`checker.go:3728`),
    /// for the function-likes whose check calls it: function and method
    /// declarations (`checker.go:3440`), get accessors (`checker.go:2976`),
    /// and function expressions, arrows and object-literal methods
    /// (`checker.go:10209`), when the body's end is reachable
    /// (`functionHasImplicitReturn`, `checker.go:20307`).
    ///
    /// The return type is what each native caller passes: the written
    /// annotation (`getReturnTypeFromAnnotation`) for functions, methods,
    /// function expressions and arrows, and `getTypeOfAccessors` for a get
    /// accessor (`checker.go:2976`). The error node is the annotation, else
    /// the function.
    ///
    /// The relations here are three-valued: an undecidable
    /// `isTypeAssignableTo(undefined, t)` or an inferred return type this
    /// port cannot compute stops the check, since every arm after it is a
    /// report. JavaScript declines: its return annotation is TS8010 and its
    /// JSDoc types are another lane's.
    pub(crate) fn check_all_code_paths_return_or_throw(&mut self, function: NodeId) {
        if self.file_has_parse_errors || self.in_js_file(function) {
            return;
        }
        let (annotation, body, modifiers, generator) = match self.node_map.get(function) {
            Some(Node::FunctionDeclaration(f)) => (
                f.r#type,
                f.body.and_then(|b| b.node_id()),
                f.modifiers,
                f.asterisk_token.is_some(),
            ),
            Some(Node::MethodDeclaration(m)) => (
                m.r#type,
                m.body.and_then(|b| b.node_id()),
                m.modifiers,
                m.asterisk_token.is_some(),
            ),
            Some(Node::FunctionExpression(f)) => (
                f.r#type,
                f.body.and_then(|b| b.node_id()),
                f.modifiers,
                f.asterisk_token.is_some(),
            ),
            Some(Node::ArrowFunction(a)) => {
                (a.r#type, a.body.and_then(|b| b.node_id()), a.modifiers, false)
            }
            Some(Node::GetAccessorDeclaration(g)) => {
                (g.r#type, g.body.and_then(|b| b.node_id()), g.modifiers, false)
            }
            _ => return,
        };
        let is_async = crate::check::has_modifier(modifiers, SyntaxKind::AsyncKeyword);
        let return_type = if self.nodes.kind(function) == SyntaxKind::GetAccessor {
            let Some(symbol) = self.binder.symbol_of(function) else { return };
            let symbol = self.binder.merged_symbol(symbol);
            Some(self.get_type_of_symbol(symbol))
        } else {
            annotation.map(|annotation| self.get_type_from_type_node_unprinted(annotation))
        };
        let unwrapped =
            return_type.map(|t| self.unwrap_return_type_for_code_paths(t, generator, is_async));
        if let Some(t) = unwrapped
            && self.is_unwrapped_return_type_undefined_void_or_any(t)
        {
            return;
        }
        // A signature, or an arrow with an expression body, has nothing to
        // check; nor does a body whose end the flow graph cannot reach.
        // Without an annotation only the `noImplicitReturns` arm can speak,
        // and only for a body with an explicit `return` (`checker.go:3768`);
        // both are asked **before** the reachability query, which types
        // `never`-returning calls in the body and can re-enter the function's
        // own inferred return type (`thisTypeInObjectLiterals2`'s TS7023 was
        // the measured cost). Upstream asks reachability first; the reorder
        // changes no answer, only which function pays for the query.
        let has_explicit_return =
            self.binder.facts(function).contains(tsr_binder::NodeFacts::HAS_EXPLICIT_RETURN);
        if return_type.is_none() && !(self.no_implicit_returns && has_explicit_return) {
            return;
        }
        let Some(body) = body else { return };
        if self.nodes.kind(body) != SyntaxKind::Block
            || !self.function_has_implicit_return(function)
        {
            return;
        }
        let message = match return_type {
            Some(t) if self.type_of(t).flags.intersects(TypeFlags::NEVER) => {
                &messages_flow::A_FUNCTION_RETURNING_NEVER_CANNOT_HAVE_A_REACHABLE_END_POINT
            }
            Some(_) if !has_explicit_return => {
                &messages_flow::A_FUNCTION_WHOSE_DECLARED_TYPE_IS_NEITHER_UNDEFINED_VOID_NOR_ANY_MUST_RETURN_A_VALUE
            }
            Some(t)
                if self.strict_null_checks
                    && self.relate_ternary(
                        self.intrinsics.undefined,
                        t,
                        crate::relater::Relation::Assignable,
                    ) == crate::relater::Ternary::NotRelated =>
            {
                &messages_flow::FUNCTION_LACKS_ENDING_RETURN_STATEMENT_AND_RETURN_TYPE_DOES_NOT_INCLUDE_UNDEFINED
            }
            _ if !self.no_implicit_returns => return,
            Some(_) => &messages_flow::NOT_ALL_CODE_PATHS_RETURN_A_VALUE,
            None => {
                // `checker.go:3771`: the inferred return type, unwrapped, must
                // not be `undefined`, `void` or any-like. A return type this
                // port cannot infer is upstream's `errorType`, which is
                // any-like, so it stays silent.
                let Some(signature) = self.get_signature_from_declaration(function) else {
                    return;
                };
                let Some(inferred) = self.get_return_type_of_signature(&signature) else {
                    return;
                };
                // `unwrapReturnType` (`checker.go:20388`) on the inferred type
                // too: a generator's `TReturn`, an async function's awaited
                // type (`generatorNoImplicitReturns`).
                let unwrapped = self.unwrap_return_type_for_code_paths(inferred, generator, is_async);
                if self.is_error(inferred)
                    || self.is_error(unwrapped)
                    || self.maybe_type_of_kind(unwrapped, TypeFlags::VOID)
                    || self.type_of(unwrapped).flags.intersects(TypeFlags::ANY | TypeFlags::UNDEFINED)
                {
                    return;
                }
                &messages_flow::NOT_ALL_CODE_PATHS_RETURN_A_VALUE
            }
        };
        // The error node is the return annotation, else the function itself
        // (`checker.go:3745`; this port has no `FullSignature` JSDoc node).
        let span = match annotation.and_then(|annotation| annotation.node_id()) {
            Some(at) => self.nodes.span(at),
            None => self.error_span(function),
        };
        let Some(file) = self.source_file_of_for_diagnostics(function) else { return };
        self.report(file, tsr_diagnostics::Diagnostic::new(message, span));
    }

    /// `isUnwrappedReturnTypeUndefinedVoidOrAny`'s test (`checker.go:3780`)
    /// on an already unwrapped type. An unresolved annotation is this port's
    /// error type, which upstream's `errorType` answers through its `Any` flag.
    pub(crate) fn is_unwrapped_return_type_undefined_void_or_any(&mut self, t: TypeId) -> bool {
        self.is_error(t)
            || self.maybe_type_of_kind(t, TypeFlags::VOID)
            || self.type_of(t).flags.intersects(TypeFlags::ANY | TypeFlags::UNDEFINED)
    }

    /// `unwrapReturnType` (`checker.go:20388`). A generator's return type is
    /// the `TReturn` argument of its annotated iterator type
    /// (`getIterationTypeOfGeneratorFunctionReturnType`); where that cannot be
    /// read, upstream's `nil` answer is `errorType`, which ends the check.
    pub(crate) fn unwrap_return_type_for_code_paths(
        &mut self,
        return_type: TypeId,
        generator: bool,
        is_async: bool,
    ) -> TypeId {
        let error = self.intrinsics.error;
        if generator {
            let Some(returned) = self.contextual_generator_iteration_type(return_type, 1) else {
                return error;
            };
            if !is_async {
                return returned;
            }
            let unwrapped = self.unwrap_awaited_type(returned);
            return self.awaited_type_no_alias(unwrapped).unwrap_or(error);
        }
        if is_async {
            return self.awaited_type_no_alias(return_type).unwrap_or(error);
        }
        return_type
    }
}

use tsr_diagnostics::messages as messages_flow;

#[cfg(test)]
#[path = "flow_query_this_tests.rs"]
mod query_this_tests;

#[cfg(test)]
#[path = "flow_object_facts_tests.rs"]
mod object_facts_tests;
