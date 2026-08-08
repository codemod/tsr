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

use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::{FlowFlags, FlowId, SymbolFlags, SymbolId};

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
    /// Recursion depth, against the 2,000 cap.
    depth: u32,
}

/// Upstream's cap (`flow.go:118`), reproduced exactly rather than rounded.
const MAX_FLOW_DEPTH: u32 = 2_000;

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
    ///   and [`Checker::get_type_at_flow_assignment`]); the array half is
    ///   selected by an empty-array initialiser, which this port answers `false`
    ///   for, so the two do not interleave.
    /// - **The `unreachableNeverType` and non-null-assertion fallbacks** at the
    ///   end of `getFlowTypeOfReferenceEx`. Neither can fire: this port produces
    ///   no `unreachableNeverType`, because `isReachableFlowNode` — the only
    ///   thing that produces one — is not ported either.
    /// - **`flowContainer`**, so the `Start` arm does not continue outward into
    ///   an enclosing function's flow.
    pub(crate) fn get_flow_type_of_reference(
        &mut self,
        reference: NodeId,
        symbol: Option<SymbolId>,
        declared_type: TypeId,
    ) -> TypeId {
        self.get_flow_type_of_reference_ex(reference, symbol, declared_type, None)
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
            depth: 0,
        };
        let result = self.get_type_at_flow_node(&mut state, flow).t;
        self.shared_flows.truncate(state.shared_flow_start);
        result
    }

    /// `getFlowTypeOfReferenceEx`'s `initialType` parameter, which the caller
    /// above always leaves at its default.
    ///
    /// Upstream's default *is* the declared type (`checker.go`,
    /// `getFlowTypeOfReference`), and this port additionally substitutes
    /// `undefined` for an auto-typed declaration — the arm documented at
    /// [`Checker::get_flow_type_of_reference`]. `Some(initial)` overrides both.
    ///
    /// # The one caller that needs it, and why nothing else does
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
                None if is_auto && !self.strict_null_checks => self.intrinsics.any,
                None if is_auto => self.intrinsics.undefined,
                None => declared_type,
            },
            is_auto,
            discriminant_pattern: None,
            is_auto_array: symbol.is_some_and(|symbol| self.is_auto_array_declaration(symbol)),
            outer_reference: self.is_outer_reference(reference, symbol),
            flow_container: self.extended_flow_container(reference, symbol),
            shared_flow_start: self.shared_flows.len(),
            depth: 0,
        };
        let result = self.get_type_at_flow_node(&mut state, flow).t;
        self.shared_flows.truncate(state.shared_flow_start);
        result
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
            } else if flags.contains(FlowFlags::LOOP_LABEL) && !self.in_js_file(state.reference) {
                break self.get_type_at_flow_loop_label(state, flow);
            } else if flags.contains(FlowFlags::BRANCH_LABEL) {
                let mut antecedents = binder.flow().antecedents(flow);
                let Some(first) = antecedents.next() else {
                    // A label with no antecedents is unreachable.
                    break FlowType { t: self.declared_or_never(state), incomplete: false };
                };
                if antecedents.next().is_none() {
                    flow = first;
                    continue;
                }
                break self.get_type_at_flow_branch_label(state, flow);
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
                if let Some(container) = binder.flow().node(flow)
                    && state.flow_container.is_some()
                    && state.flow_container != Some(container)
                    && state.symbol.is_some()
                    && let Some(outer) = binder.flow_of(container)
                {
                    flow = outer;
                    continue;
                }
                if state.outer_reference
                    && (state.symbol.is_some_and(|s| self.symbol_has_any_assignment(s))
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
                // SWITCH_CLAUSE, CALL, ARRAY_MUTATION and REDUCE_LABEL. Each is
                // a narrowing this port does not do, and skipping to the
                // antecedent yields the *unnarrowed* type — today's answer, not
                // a wrong one. A `CALL` node additionally guards assertion
                // signatures, which need call resolution this checker lacks.
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
            // Upstream additionally returns `unreachableNeverType` for an
            // unreachable assignment (`flow.go:256`); `isReachableFlowNode` is
            // not ported, so the declared type is the whole of this arm.
            if self.contains_matching_reference(state, node) {
                return Some(FlowType { t: state.declared_type, incomplete: false });
            }
            return None;
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
                tsr_ast::SyntaxKind::FunctionExpression
                    | tsr_ast::SyntaxKind::ArrowFunction
                    | tsr_ast::SyntaxKind::MethodDeclaration
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
    /// accessor. The `CheckFlagsReadonly` and `Object.defineProperty` arms
    /// are unported (`checker-notes-narrow.md` §27).
    pub(crate) fn is_readonly_symbol(&self, symbol: SymbolId) -> bool {
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
        {
            return property.modifiers.iter().any(|modifier| {
                tsr_ast::Node::from(*modifier)
                    .node_id()
                    .is_some_and(|id| self.nodes.kind(id) == tsr_ast::SyntaxKind::ReadonlyKeyword)
            });
        }
        false
    }

    fn is_constant_variable(&self, symbol: SymbolId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        self.nodes.parent(declaration).is_some_and(|list| {
            self.nodes.kind(list) == tsr_ast::SyntaxKind::VariableDeclarationList
                && self.nodes.flags(list).intersects(tsr_ast::NodeFlags::CONST)
        })
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

    /// `isPastLastAssignment` (`flow.go:2668`): never assigned (0) or last
    /// assigned before the reference.
    fn is_past_last_assignment(&mut self, symbol: SymbolId, location: NodeId) -> bool {
        self.ensure_assignments_marked(symbol);
        let pos = self.last_assignment_pos.get(&symbol).copied().unwrap_or(0);
        pos == 0 || pos < i64::from(self.nodes.span(location).start)
    }

    /// `ensureAssignmentsMarked` (`flow.go:2674`): one marking walk per
    /// enclosing function/source-file root.
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
                && self.is_parameter_or_mutable_local_variable(symbol)
            {
                // `hasDefiniteAssignment` is written **outside** the
                // `lastAssignmentPos != MAX` guard upstream (`flow.go:2718`):
                // the guard governs the position, not the flag, and a symbol
                // already at `MAX` can still gain its first definite
                // assignment. Splitting the two writes apart is what keeps this
                // addition from moving `last_assignment_pos` by a single entry
                // — `checker-notes-diag2.md` §42.
                if kind == crate::expressions::AssignmentTargetKind::Definite {
                    self.definitely_assigned.insert(symbol);
                }
                if self.last_assignment_pos.get(&symbol) != Some(&i64::MAX) {
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
    /// evolves. This checker has no compiler options plumbed at all — nothing
    /// reads a `CompilerOptions` anywhere in `tsr-checker` — so the flag cannot
    /// be consulted and is assumed on, matching `strict`.
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
    fn is_auto_typed_declaration(&self, symbol: SymbolId) -> bool {
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
        // A binding pattern is excluded upstream: `let { a } = x` declares
        // through the pattern and the auto reduction never applies.
        if !matches!(node.name, Some(tsr_ast::BindingName::Identifier(_))) {
            return false;
        }
        node.r#type.is_none()
            && node.initializer.is_none()
            && !self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT)
    }

    /// Upstream's `autoArrayType` trigger (`checker.go`, the evolving-array
    /// machinery): a variable declared with no annotation and an empty
    /// array-literal initializer. Only the §14 depth accounting asks.
    fn is_auto_array_declaration(&self, symbol: SymbolId) -> bool {
        let Some(declaration) = self.binder.symbols().get(symbol).value_declaration else {
            return false;
        };
        let Some(Node::VariableDeclaration(node)) = self.node_map.get(declaration) else {
            return false;
        };
        node.r#type.is_none()
            && matches!(
                node.initializer,
                Some(tsr_ast::Expression::ArrayLiteralExpression(array))
                    if array.elements.is_empty()
            )
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
    /// `None` for every other form: `for..in` (upstream `string`), `for..of`,
    /// destructuring targets, `delete`. Each needs a parent walk this module
    /// does not do, or machinery — iteration protocol resolution — the checker
    /// does not have.
    fn get_initial_or_assigned_type(&mut self, node: NodeId) -> Option<TypeId> {
        if let Some(Node::VariableDeclaration(declaration)) = self.node_map.get(node) {
            // `getInitialTypeOfVariableDeclaration` (`flow.go:2244`). The
            // `for..in` / `for..of` arms below it are the unported ones.
            let initializer = declaration.initializer?;
            return Some(self.check_expression(initializer));
        }
        // `getAssignedTypeOfBinaryExpression` (`flow.go:2314`), restricted to a
        // plain `x = e`. A destructuring default (`[x = 1] = y`) reaches the same
        // upstream function by a different route and is not handled.
        let parent = self.nodes.parent(node)?;
        let Some(Node::BinaryExpression(binary)) = self.node_map.get(parent) else { return None };
        if binary.operator_token?.kind != SyntaxKind::EqualsToken {
            return None;
        }
        if binary.left.and_then(|left| Node::from(left).node_id()) != Some(node) {
            return None;
        }
        Some(self.check_expression(binary.right?))
    }

    /// Keep the constituents of a union declared type that the assigned type
    /// could be (`getAssignmentReducedType`, `flow.go:2399`).
    ///
    /// The memo upstream keeps (`assignmentReducedTypes`) is not ported: it
    /// guards repeated assignability queries, and this port's assignability is
    /// not yet the cost centre that makes a cache pay.
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
        for constituent in constituents {
            if self.type_maybe_assignable_to(assigned, constituent) {
                kept.push(constituent);
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
        let kept = if assigned_is_fresh_boolean {
            kept.into_iter().map(|t| self.get_fresh_type_of_literal_type(t)).collect()
        } else {
            kept
        };
        let reduced = self.get_union_type(&kept);
        // Upstream's own guard on its "crude heuristic" (`flow.go:2424`): when
        // the assigned type is not assignable to what the filter kept, give up
        // and narrow nothing rather than print a type the assignment refutes.
        if self.is_type_assignable_to(assigned, reduced) { reduced } else { declared }
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
        let key = (
            tsr_core::index::Idx::index(flow),
            match state.symbol {
                Some(symbol) => symbol.index() as u64,
                None => (1 << 63) | u64::from(state.reference.as_u32()),
            },
        );
        if let Some(&cached) = self.flow_loop_cache.get(&key) {
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
                if let Some(&cached) = self.flow_loop_cache.get(&key) {
                    state.depth = depth_mark;
                    self.flow_loop_stack.truncate(stack_index);
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
            if flow_type.t != self.intrinsics.never && !live.contains(&flow_type.t) {
                live.push(flow_type.t);
            }
            if !self.is_type_subset_of(flow_type.t, state.initial_type) {
                subtype_reduction = true;
            }
            if flow_type.t == state.declared_type {
                break;
            }
        }
        let types = self.flow_loop_stack[stack_index].1.clone();
        self.flow_loop_stack.truncate(stack_index);
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
        let incomplete = first.is_some_and(|f| f.incomplete);
        if std::env::var("TSR_TRACE_LOOP").is_ok() {
            eprintln!(
                "LOOP key={key:?} types={types:?} subtype_red={subtype_reduction} result={result:?} incomplete={incomplete}"
            );
        }
        if incomplete {
            return FlowType { t: result, incomplete: true };
        }
        self.flow_loop_cache.insert(key, result);
        FlowType { t: result, incomplete: false }
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
    fn get_type_at_flow_branch_label(&mut self, state: &mut FlowState, flow: FlowId) -> FlowType {
        let antecedents: Vec<FlowId> = self.binder.flow().antecedents(flow).collect();
        let mut types: Vec<TypeId> = Vec::with_capacity(antecedents.len());
        let never = self.intrinsics.never;
        for antecedent in antecedents {
            let t = self.get_type_at_flow_node(state, antecedent).t;
            // `never` from a path means that path cannot reach here, so it
            // contributes nothing — which is what makes `if (x) {} else { throw }`
            // narrow after the `if`.
            if t != never && !types.contains(&t) {
                types.push(t);
            }
        }
        let t = if types.is_empty() { never } else { self.get_union_type(&types) };
        FlowType { t, incomplete: false }
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
    fn is_matching_reference(&mut self, state: &FlowState, node: NodeId) -> bool {
        if node == state.reference {
            return true;
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
    /// Not ported, each because deciding it needs something this port does not
    /// have: an **element access with a non-literal argument**
    /// (`a[i]` matches `a[i]` only when `i` is a constant or an unassigned
    /// local, which needs `isSymbolAssigned`); `super`; `MetaProperty`; and the
    /// comma and assignment unwrapping on the target side. Each answers
    /// `false`, which costs a narrowing and never invents one.
    ///
    /// # Why `false` is the safe default here, unlike everywhere else
    ///
    /// This module's header records the one way narrowing produces a *wrong*
    /// answer rather than a gap: a match that is too **loose** narrows the
    /// wrong reference. So every unported arm answers `false` and every ported
    /// arm is an equality rather than a heuristic — an element access whose
    /// argument this port cannot prove constant is refused rather than matched
    /// on its text.
    fn references_match(&mut self, source: NodeId, target: NodeId) -> bool {
        if source == target {
            return true;
        }
        // `KindParenthesizedExpression` on either side (`flow.go`, both
        // switches), so `(a.b)` and `a.b` are one reference.
        if let Some(Node::ParenthesizedExpression(node)) = self.node_map.get(source)
            && let Some(inner) = node.expression.and_then(|e| e.node_id())
        {
            return self.references_match(inner, target);
        }
        if let Some(Node::ParenthesizedExpression(node)) = self.node_map.get(target)
            && let Some(inner) = node.expression.and_then(|e| e.node_id())
        {
            return self.references_match(source, inner);
        }

        match (self.node_map.get(source), self.node_map.get(target)) {
            // `this` matches `this` and nothing else.
            (Some(Node::KeywordExpression(left)), Some(Node::KeywordExpression(right))) => {
                left.kind == SyntaxKind::ThisKeyword && right.kind == SyntaxKind::ThisKeyword
            }
            // Two accesses: same property name, matching receivers. The name
            // comparison is on the **member name text**, which is what
            // `getAccessedPropertyName` answers for a property access.
            (
                Some(left @ (Node::PropertyAccessExpression(_) | Node::ElementAccessExpression(_))),
                Some(right),
            ) => {
                let (Some(left_name), Some(right_name)) =
                    (accessed_property_name(left), accessed_property_name(right))
                else {
                    return false;
                };
                if left_name != right_name {
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
    /// `getTypeAtSwitchClause` (`flow.go:1059`), the two matching arms —
    /// identifier discriminant and `typeof` witness. Every other shape
    /// (`switch (true)`, optional chains, discriminant property access)
    /// passes the antecedent's type through unchanged, which is today's
    /// answer, not a wrong one. See `checker-notes-narrow.md` §16.
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
        // §50.1: the switch expression is a SIBLING element of the
        // pseudo-reference pattern — the clause narrows the walked union
        // by that member.
        if let Some(member) = state.discriminant_pattern.and_then(|pattern| {
            expr.node_id().and_then(|id| self.sibling_member_of_pattern(pattern, id))
        }) {
            let narrowed = self.narrow_union_by_member_switch(incoming.t, &member, switch, &clause);
            return FlowType { t: narrowed, incomplete: incoming.incomplete };
        }
        // §51 (`checker-notes-narrow.md`): `switch (s.kind)` — a property
        // access whose RECEIVER is the reference narrows by the member,
        // through §50.1's filter (`narrowTypeBySwitchOnDiscriminantProperty`).
        if let tsr_ast::Expression::PropertyAccessExpression(access) = expr
            && access
                .expression
                .and_then(|receiver| receiver.node_id())
                .is_some_and(|id| self.is_matching_reference(state, id))
            && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
        {
            let member = name.text.to_string();
            let narrowed = self.narrow_union_by_member_switch(incoming.t, &member, switch, &clause);
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
            incoming.t
        };
        FlowType { t: narrowed, incomplete: incoming.incomplete }
    }

    /// §50's sibling test, shared by the equality arm and the switch arm
    /// (§50.1): is `id` an identifier bound to a sibling element of
    /// `pattern`? The value declaration may be the element OR its name
    /// node — walk up to two hops to the pattern.
    fn sibling_member_of_pattern(&self, pattern: NodeId, id: NodeId) -> Option<String> {
        let Some(Node::Identifier(identifier)) = self.node_map.get(id) else {
            return None;
        };
        let symbol = self.binder.resolve_name(
            self.nodes,
            self.node_map,
            id,
            identifier.text,
            SymbolFlags::VALUE,
        )?;
        let declaration = self.binder.symbols().get(symbol).value_declaration?;
        let mut current = Some(declaration);
        for _ in 0..2 {
            let node = current?;
            if self.nodes.parent(node) == Some(pattern) {
                // §50.3: a PARAMETER sibling of a pseudo-pattern FUNCTION
                // discriminates the tuple union by ELEMENT INDEX — tuples
                // answer numeric member names (tuple §8), so the filters
                // compose unchanged.
                if self.nodes.kind(node) == SyntaxKind::Parameter {
                    let parameters = match self.node_map.get(pattern)? {
                        Node::ArrowFunction(function) => function.parameters,
                        Node::FunctionExpression(function) => function.parameters,
                        _ => return Some(identifier.text.to_string()),
                    };
                    let index =
                        parameters.iter().position(|parameter| parameter.node_id == Some(node))?;
                    return Some(index.to_string());
                }
                return Some(identifier.text.to_string());
            }
            current = self.nodes.parent(node);
        }
        None
    }

    /// §50.1's switch half: filter the walked union's constituents by
    /// whether the named sibling MEMBER admits any clause-range literal.
    /// Default clauses and every undecidable pair decline whole
    /// (`checker-notes-narrow.md` §50.1).
    fn narrow_union_by_member_switch(
        &mut self,
        t: TypeId,
        member: &str,
        switch: &tsr_ast::SwitchStatement<'_>,
        clause: &tsr_binder::SwitchClause,
    ) -> TypeId {
        let Some(clause_types) = self.switch_clause_types(switch) else { return t };
        if clause_types.is_empty() {
            return t;
        }
        let (start, end) = (clause.clause_start as usize, clause.clause_end as usize);
        let slice = &clause_types[start.min(clause_types.len())..end.min(clause_types.len())];
        if start == end || slice.contains(&self.intrinsics.never) {
            return t;
        }
        let clause_list: Vec<TypeId> = slice.to_vec();
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let constituents_len = constituents.len();
        let mut kept = Vec::new();
        for constituent in constituents {
            let Some(member_type) = self.get_type_of_property_of_type(constituent, member) else {
                return t;
            };
            let mut admits = false;
            for &clause_type in &clause_list {
                let regular = self.get_regular_type_of_literal_type(clause_type);
                match self.comparable_ternary(regular, member_type) {
                    Some(true) => {
                        admits = true;
                        break;
                    }
                    Some(false) => {}
                    None => return t,
                }
            }
            if admits {
                kept.push(constituent);
            }
        }
        if kept.is_empty() {
            return t;
        }
        // §51: keeping EVERY constituent is the identity — rebuilding the
        // union would lose an alias-named type's name (`numericLiteralTypes1`
        // wants `Item`, not the re-formed constituent list).
        if kept.len() == constituents_len {
            return t;
        }
        self.get_union_type(&kept)
    }

    /// `narrowTypeBySwitchOnDiscriminant` (`flow.go:1092`), the
    /// comparable-filter path. Declines whole — answers `t` unchanged — when
    /// any clause type is missing, non-unit, or a comparability the relation
    /// cannot decide, so the failure mode is the unnarrowed status quo. The
    /// unknown-ground path is unported (stated in §16).
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
            return t;
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
    fn comparable_ternary(&mut self, discriminant: TypeId, constituent: TypeId) -> Option<bool> {
        if discriminant == constituent {
            return Some(true);
        }
        let simple = TypeFlags::UNIT
            | TypeFlags::STRING
            | TypeFlags::NUMBER
            | TypeFlags::BIG_INT
            | TypeFlags::BOOLEAN
            | TypeFlags::ENUM_LIKE
            | TypeFlags::NULL
            | TypeFlags::UNDEFINED;
        let discriminant_constituents: Vec<TypeId> = match &self.store.get(discriminant).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![discriminant],
        };
        for &d in &discriminant_constituents {
            let d_flags = self.store.get(d).flags;
            let c_flags = self.store.get(constituent).flags;
            if !d_flags.intersects(simple) || !c_flags.intersects(simple) {
                return None;
            }
            // Comparable in either direction: same type, a literal against
            // its own base primitive — but two DISTINCT literals of one base
            // are NOT comparable (`'A'` vs `'B'`), the §50 measurement's
            // fired leg.
            let d_unit = d_flags.intersects(TypeFlags::UNIT);
            let c_unit = c_flags.intersects(TypeFlags::UNIT);
            if d == constituent {
                return Some(true);
            }
            if d_unit && c_unit {
                let d_regular = self.get_regular_type_of_literal_type(d);
                let c_regular = self.get_regular_type_of_literal_type(constituent);
                if d_regular == c_regular {
                    return Some(true);
                }
                continue;
            }
            if self.get_base_type_of_literal_type(d)
                == self.get_base_type_of_literal_type(constituent)
            {
                return Some(true);
            }
        }
        Some(false)
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

    fn narrow_type(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        condition: NodeId,
        assume_true: bool,
    ) -> TypeId {
        let Some(node) = self.node_map.get(condition) else { return t };
        match node {
            // `narrowTypeByCallExpression` (`flow.go:444`), the
            // identifier-predicate half — `if (isNumber(x))` narrows `x`.
            // A call that answers no predicate falls through unchanged,
            // which is also the truthiness answer for a call condition
            // (upstream's dispatch reaches the call arm before any
            // truthiness question, exactly as here).
            Node::CallExpression(call) => {
                self.narrow_type_by_call_expression(state, t, call, assume_true)
            }
            // `if (x)`, `if (a.b)`, `while (o["k"])`: the reference itself as
            // the condition. Every form [`Checker::is_matching_reference`] can
            // decide belongs here — restricting it to `Identifier` is what kept
            // truthiness narrowing dead on property references long after the
            // matcher could have handled them (`bd tsr-6ka`).
            Node::Identifier(_)
            | Node::PropertyAccessExpression(_)
            | Node::ElementAccessExpression(_) => {
                if self.is_matching_reference(state, condition) {
                    let facts = if assume_true { TypeFacts::TRUTHY } else { TypeFacts::FALSY };
                    return self.get_type_with_facts(t, facts);
                }
                // §51.4 (`checker-notes-narrow.md`): `if (o?.foo)` — under
                // strictNullChecks the true branch narrows the chain base
                // NE_UNDEFINED_OR_NULL, and FALLS THROUGH to the
                // discriminant filter (`flow.go:432`'s ordering).
                let t = if self.strict_null_checks
                    && assume_true
                    && self.optional_chain_contains_reference(state, condition)
                {
                    self.get_type_with_facts(t, TypeFacts::NE_UNDEFINED_OR_NULL)
                } else {
                    t
                };
                // §51.3 (`checker-notes-narrow.md`): `if (s.done)` — an
                // access whose RECEIVER is the reference discriminates by
                // the member's truthiness (`narrowTypeByDiscriminant` with
                // the truthy facts as the member transform). Decidable
                // members only; any opaque one declines whole.
                if let Node::PropertyAccessExpression(access) = node
                    && access
                        .expression
                        .and_then(|receiver| receiver.node_id())
                        .is_some_and(|id| self.is_matching_reference(state, id))
                    && let Some(tsr_ast::MemberName::Identifier(name)) = access.name
                {
                    let member = name.text.to_string();
                    return self.filter_union_by_member_truthiness(t, &member, assume_true);
                }
                t
            }
            Node::ParenthesizedExpression(inner) => inner
                .expression
                .and_then(|e| e.node_id())
                .map_or(t, |id| self.narrow_type(state, t, id, assume_true)),
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
                // `"p" in x` — `narrowTypeByInKeyword` (`flow.go:1001`),
                // known-property half; `checker-notes-narrow.md` §6.1. The
                // name must be a written string literal (upstream reads it
                // from the operand's *type*; a literal is the only shape whose
                // type this port can read without re-entering the flow walk,
                // the same restriction `nullable_literal_type` states).
                if operator.kind == SyntaxKind::InKeyword {
                    let (Some(left_node), Some(right_node)) = (left.node_id(), right.node_id())
                    else {
                        return t;
                    };
                    if let Some(Node::StringLiteral(literal)) = self.node_map.get(left_node)
                        && self.is_matching_reference(state, right_node)
                    {
                        return self.narrow_type_by_in_keyword(t, literal.text, assume_true);
                    }
                    return t;
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
                // `typeof x === "…"` before the value-equality path, which is
                // upstream's dispatch order in `narrowTypeByBinaryExpression`
                // (`flow.go:500` region): a `TypeOfExpression` on either side
                // with a string literal on the other reaches
                // `narrowTypeByTypeof` (`flow.go:614`). Only the
                // matching-reference half is ported; the discriminant and
                // optional-chain halves of that function stay unported and
                // answer the type unchanged (`bd tsr-q9g`,
                // `checker-notes-narrow.md` §6).
                let typeof_pair = match (self.node_map.get(left), self.node_map.get(right)) {
                    (
                        Some(Node::TypeOfExpression(typeof_expr)),
                        Some(Node::StringLiteral(literal)),
                    )
                    | (
                        Some(Node::StringLiteral(literal)),
                        Some(Node::TypeOfExpression(typeof_expr)),
                    ) => Some((typeof_expr, literal.text)),
                    _ => None,
                };
                if let Some((typeof_expr, literal)) = typeof_pair {
                    let Some(target) = typeof_expr.expression.and_then(|e| e.node_id()) else {
                        return t;
                    };
                    if !self.is_matching_reference(state, target) {
                        return t;
                    }
                    let negated = matches!(
                        operator.kind,
                        SyntaxKind::ExclamationEqualsToken
                            | SyntaxKind::ExclamationEqualsEqualsToken
                    );
                    return self.narrow_type_by_typeof_literal(t, literal, assume_true != negated);
                }
                // §50: a condition on a SIBLING element of the pseudo-
                // reference pattern discriminates the walked union
                // (`checker-notes-narrow.md`).
                if let Some(pattern) = state.discriminant_pattern {
                    let pair = self
                        .sibling_member_of_pattern(pattern, left)
                        .map(|name| (name, right))
                        .or_else(|| {
                            self.sibling_member_of_pattern(pattern, right).map(|name| (name, left))
                        });
                    if let Some((member, literal_node)) = pair {
                        let literal_type = self
                            .node_map
                            .get(literal_node)
                            .and_then(|node| tsr_ast::Expression::try_from(node).ok())
                            .map(|expression| self.check_expression(expression));
                        if let Some(literal_type) = literal_type
                            && literal_type != self.intrinsics.error
                        {
                            let negated = matches!(
                                operator.kind,
                                SyntaxKind::ExclamationEqualsToken
                                    | SyntaxKind::ExclamationEqualsEqualsToken
                            );
                            let keep_match = assume_true != negated;
                            return self.filter_union_by_member_literal(
                                t,
                                &member,
                                literal_type,
                                keep_match,
                            );
                        }
                    }
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
                                    containment_narrowed =
                                        Some(self.get_type_with_facts(
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
                // §51.1 (`checker-notes-narrow.md`): `s.kind === 0` — a
                // property access whose RECEIVER is the reference
                // discriminates by the member, the §50 filter with the
                // member from the access (`narrowTypeByDiscriminantProperty`).
                let access_pair =
                    [(left, right), (right, left)].into_iter().find_map(|(candidate, value)| {
                        match self.node_map.get(candidate) {
                            Some(Node::PropertyAccessExpression(access))
                                if access
                                    .expression
                                    .and_then(|receiver| receiver.node_id())
                                    .is_some_and(|id| self.is_matching_reference(state, id)) =>
                            {
                                match access.name {
                                    Some(tsr_ast::MemberName::Identifier(name)) => {
                                        Some((name.text.to_string(), value))
                                    }
                                    _ => None,
                                }
                            }
                            _ => None,
                        }
                    });
                if let Some((member, literal_node)) = access_pair {
                    let literal_type = self
                        .node_map
                        .get(literal_node)
                        .and_then(|node| tsr_ast::Expression::try_from(node).ok())
                        .map(|expression| self.check_expression(expression));
                    if let Some(literal_type) = literal_type
                        && literal_type != self.intrinsics.error
                        && self.store.get(literal_type).flags.intersects(TypeFlags::UNIT)
                    {
                        let negated = matches!(
                            operator.kind,
                            SyntaxKind::ExclamationEqualsToken
                                | SyntaxKind::ExclamationEqualsEqualsToken
                        );
                        let keep_match = assume_true != negated;
                        return self.filter_union_by_member_literal(
                            t,
                            &member,
                            literal_type,
                            keep_match,
                        );
                    }
                }
                // Upstream normalises with `getReferenceCandidate` on the left
                // and reads the value from the right; the reference can sit on
                // either side, so both orders are tried and the *other* operand
                // is the value.
                let value = if self.is_matching_reference(state, left) {
                    right
                } else if self.is_matching_reference(state, right) {
                    left
                } else {
                    return t;
                };
                self.narrow_type_by_equality(t, operator.kind, value, assume_true)
            }
            _ => t,
        }
    }

    /// §51.2: does `node` spell an optional chain whose BASE (behind at
    /// least one `?.`) is the matching reference? Walks receivers through
    /// property/element accesses and call expressions.
    fn optional_chain_contains_reference(&mut self, state: &FlowState, node: NodeId) -> bool {
        let mut current = node;
        let mut saw_question = false;
        loop {
            match self.node_map.get(current) {
                Some(Node::PropertyAccessExpression(access)) => {
                    saw_question |= access.question_dot_token.is_some();
                    let Some(receiver) = access.expression.and_then(|e| e.node_id()) else {
                        return false;
                    };
                    if saw_question && self.is_matching_reference(state, receiver) {
                        return true;
                    }
                    current = receiver;
                }
                Some(Node::ElementAccessExpression(access)) => {
                    saw_question |= access.question_dot_token.is_some();
                    let Some(receiver) = access.expression.and_then(|e| e.node_id()) else {
                        return false;
                    };
                    if saw_question && self.is_matching_reference(state, receiver) {
                        return true;
                    }
                    current = receiver;
                }
                Some(Node::CallExpression(call)) => {
                    let Some(callee) = call.expression.and_then(|e| e.node_id()) else {
                        return false;
                    };
                    current = callee;
                }
                _ => return false,
            }
        }
    }

    /// §51.3: keep constituents whose MEMBER admits the assumed
    /// truthiness. Every member must be truthiness-DECIDABLE — a unit
    /// literal or boolean-family type — or the whole filter declines.
    fn filter_union_by_member_truthiness(
        &mut self,
        t: TypeId,
        member: &str,
        assume_true: bool,
    ) -> TypeId {
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let total = constituents.len();
        let facts = if assume_true { TypeFacts::TRUTHY } else { TypeFacts::FALSY };
        let mut kept = Vec::new();
        for constituent in constituents {
            let Some(member_type) = self.get_type_of_property_of_type(constituent, member) else {
                return t;
            };
            let decidable = {
                let flags = self.store.get(member_type).flags;
                flags.intersects(TypeFlags::UNIT | TypeFlags::BOOLEAN)
                    || matches!(&self.store.get(member_type).data, TypeData::Union { types, .. }
                    if types.iter().all(|&part| {
                        self.store.get(part).flags.intersects(TypeFlags::UNIT)
                    }))
            };
            if !decidable {
                return t;
            }
            let faceted = self.get_type_with_facts(member_type, facts);
            if !self.store.get(faceted).flags.intersects(TypeFlags::NEVER) {
                kept.push(constituent);
            }
        }
        if kept.is_empty() || kept.len() == total {
            return t;
        }
        self.get_union_type(&kept)
    }

    /// The §50/§51.1 shared discriminant filter: keep constituents whose
    /// MEMBER admits (or refutes) the literal. Declines whole — answers `t`
    /// — on a missing member, a Kleene unknown, an emptied set, or a
    /// kept-ALL set (the §51 alias-name identity).
    fn filter_union_by_member_literal(
        &mut self,
        t: TypeId,
        member: &str,
        literal_type: TypeId,
        keep_match: bool,
    ) -> TypeId {
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let total = constituents.len();
        let regular = self.get_regular_type_of_literal_type(literal_type);
        let mut kept = Vec::new();
        for constituent in constituents {
            let Some(member_type) = self.get_type_of_property_of_type(constituent, member) else {
                return t;
            };
            match self.comparable_ternary(regular, member_type) {
                Some(admits) => {
                    if admits == keep_match {
                        kept.push(constituent);
                    }
                }
                None => return t,
            }
        }
        if kept.is_empty() || kept.len() == total {
            return t;
        }
        self.get_union_type(&kept)
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
    /// # What is not ported inside the branch that is
    ///
    /// Upstream calls `getAdjustedTypeWithFacts` (`checker.go:31159`), which
    /// wraps `getTypeWithFacts` with two extras: recombining `unknown` into
    /// `unknownUnionType` first, and mapping surviving constituents through
    /// `getGlobalNonNullableTypeInstantiation` for the `NEUndefinedOrNull` and
    /// `Truthy` cases. Both act on `unknown` and on type parameters —
    /// `NonNullable<T>` — and neither changes the answer for a union of
    /// concrete constituents, which is what `string | undefined` is. They are
    /// left out rather than approximated, so those two shapes answer exactly
    /// as they do today.
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
        // `hasMatchingArgument`: some argument is the reference.
        let matching_index = call.arguments.iter().position(|argument| {
            tsr_ast::Node::from(*argument)
                .node_id()
                .is_some_and(|id| self.is_matching_reference(state, id))
        });
        let Some(_) = matching_index else { return t };
        let Some(callee) = call.expression else { return t };
        let callee_type = self.check_expression(callee);
        if callee_type == self.intrinsics.error {
            return t;
        }
        let Some(signature) = self.resolve_call_signature(callee_type, Some(call.arguments)) else {
            return t;
        };
        let Some(predicate) = &signature.predicate else { return t };
        if predicate.asserts {
            return t;
        }
        let (Some(name), Some(predicate_type)) = (&predicate.parameter_name, predicate.r#type)
        else {
            return t;
        };
        // `getTypePredicateArgument`: the argument at the predicate
        // parameter's position (recovered by name — this port's predicate
        // carries no index).
        let Some(index) = signature.parameters.iter().position(|parameter| parameter.name == *name)
        else {
            return t;
        };
        let Some(argument) = call.arguments.get(index) else { return t };
        if !tsr_ast::Node::from(*argument)
            .node_id()
            .is_some_and(|id| self.is_matching_reference(state, id))
        {
            return t;
        }
        // `getNarrowedTypeWorker`'s per-constituent ladder (`flow.go:915`):
        // strictSubtype(t,n) -> t; strictSubtype(n,t) -> n; subtype(t,n) -> t;
        // subtype(n,t) -> n; else drop — the asserted type wins mutual
        // relations (`narrowingMutualSubtypes`, the §22 bar's fired leg: a
        // plain assignability keep answered the wrong side). Kleene: any
        // undecidable rung declines the whole narrowing.
        let constituents: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        let mut kept = Vec::with_capacity(constituents.len());
        for constituent in constituents {
            match self.narrowed_constituent(constituent, predicate_type) {
                NarrowedConstituent::Undecidable => return t,
                NarrowedConstituent::Mapped(mapped) => {
                    if assume_true {
                        kept.push(mapped);
                    } else if mapped != constituent {
                        // The false branch keeps what the true branch mapped
                        // AWAY — upstream's `!isTypeSubsetOf(c, trueType)`
                        // (`flow.go:873`): a constituent that only reached
                        // the true side AS THE CANDIDATE is still possible
                        // when the predicate is false
                        // (`narrowingMutualSubtypes`' `{}` vs
                        // `Record<string, unknown>`, the third fired leg).
                        kept.push(constituent);
                    }
                }
                NarrowedConstituent::Dropped => {
                    if !assume_true {
                        kept.push(constituent);
                    }
                }
            }
        }
        if kept.is_empty() && assume_true {
            // Upstream intersects when the filter empties and the predicate
            // type is assignable INTO the declared — the `x is string` on an
            // `unknown` shape. One decidable check; anything else declines.
            return match self.relate_ternary(
                predicate_type,
                t,
                crate::relater::Relation::Assignable,
            ) {
                crate::relater::Ternary::Related => predicate_type,
                _ => t,
            };
        }
        // Identity preservation: a mapping that changed nothing answers the
        // ORIGINAL type — a named union alias keeps its name
        // (`narrowingMutualSubtypes`' `Union` positions, the second fired
        // leg), exactly as upstream's `filterType` identity short-circuit.
        let original: Vec<TypeId> = match &self.store.get(t).data {
            TypeData::Union { types, .. } => types.clone(),
            _ => vec![t],
        };
        if kept == original {
            return t;
        }
        self.get_union_type(&kept)
    }

    /// One rung of `getNarrowedTypeWorker`'s ladder: `Some(Some(image))`
    /// maps the constituent, `Some(None)` drops it, `None` is an
    /// undecidable rung.
    fn narrowed_constituent(
        &mut self,
        constituent: TypeId,
        candidate: TypeId,
    ) -> NarrowedConstituent {
        use crate::relater::{Relation, Ternary};
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
    fn declaration_has_call_signature_member(&self, declaration: NodeId) -> bool {
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

    fn narrow_type_by_equality(
        &mut self,
        t: TypeId,
        operator: SyntaxKind,
        value: NodeId,
        assume_true: bool,
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

        // Only the *literal* nullable operands are read, rather than typing
        // the expression: `checkExpression` from inside a flow walk would
        // re-enter narrowing for the operand's own reference and could recurse
        // through the same flow node. `null` and `undefined` written literally
        // are the whole of what the nullable branch can act on anyway —
        // upstream reaches the same two types through `getTypeOfExpression`,
        // and a non-literal operand lands in the unported comparability branch
        // either way, so nothing reachable is given up.
        let Some(value_type) = self.nullable_literal_type(value) else { return t };
        let value_flags = self.store.get(value_type).flags;
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
        self.get_type_with_facts(t, facts)
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
            return self.get_type_with_facts(t, facts);
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
                if t == self.intrinsics.any {
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
                if t == self.intrinsics.any {
                    return t;
                }
                // `c.globalFunctionType`. A lib-less program has no `Function`
                // interface; narrowing is then declined rather than
                // approximated with a made-up type.
                let Some(function_symbol) = self.global_type_symbol("Function") else {
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

    /// Keep only the constituents of `t` for which every bit of `facts` holds
    /// (`getTypeWithFacts`, `checker.go:31245`).
    pub(crate) fn get_type_with_facts(&mut self, t: TypeId, facts: TypeFacts) -> TypeId {
        self.filter_type(t, |checker, constituent| {
            checker.get_type_facts(constituent).contains(facts)
        })
    }

    /// Keep the constituents of a union that satisfy `predicate`
    /// (`filterType`, `checker.go:26362`).
    ///
    /// A non-union either survives whole or becomes `never`, which is upstream's
    /// behaviour and is what makes `if (x)` on a plain `undefined` give `never`.
    pub(crate) fn filter_type(
        &mut self,
        t: TypeId,
        predicate: impl Fn(&Self, TypeId) -> bool,
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
            return self.get_union_type(&kept);
        }
        if predicate(self, t) { t } else { self.intrinsics.never }
    }

    /// What is knowable about a type without narrowing it
    /// (`getTypeFacts`, `checker.go:30982`).
    ///
    /// **The default is both bits**, and that is a deliberate safety property
    /// rather than laziness: a type whose truthiness this port cannot decide
    /// keeps every constituent, so narrowing leaves the type alone and the
    /// answer is the one given today. Claiming a type is truthy when it is not
    /// would delete a constituent and print a plausible wrong type.
    pub(crate) fn get_type_facts(&self, t: TypeId) -> TypeFacts {
        // Upstream carries these as one aggregate per kind of type
        // (`TypeFactsUndefinedFacts` `checker.go:471`, `TypeFactsNullFacts`
        // `:472`, and the `NEUndefined | NENull | NEUndefinedOrNull` shared by
        // every `Base*StrictFacts`). Named here rather than spelled at each
        // return so the three sets can be read against upstream's three
        // constants.
        //
        // **The non-strict aggregates are deliberately not ported.** With
        // `strictNullChecks` off every type also carries `EQUndefined | EQNull
        // | EQUndefinedOrNull` (`checker.go:433`), and it does not matter:
        // `narrow_type_by_equality` returns the type unchanged in that mode
        // before it ever asks for facts, exactly as upstream does
        // (`flow.go:565`). Adding them would be an unreachable arm.
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
        // An object or a non-primitive is always truthy and never nullable.
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
        //   `binder.rs:3482`), so `ObjectStrictFacts` is consistent with the
        //   port's own model rather than a guess about upstream's;
        // - anything else object-flagged keeps every bit.
        if flags.intersects(TypeFlags::OBJECT | TypeFlags::NON_PRIMITIVE) {
            let object_strict = TypeFacts::TRUTHY
                | nullable_never
                | TypeFacts::TYPEOF_EQ_OBJECT
                | TypeFacts::TYPEOF_EQ_HOST_OBJECT
                | (typeof_ne_all - TypeFacts::TYPEOF_NE_OBJECT - TypeFacts::TYPEOF_NE_HOST_OBJECT);
            let function_strict = TypeFacts::TRUTHY
                | nullable_never
                | TypeFacts::TYPEOF_EQ_FUNCTION
                | TypeFacts::TYPEOF_EQ_HOST_OBJECT
                | (typeof_ne_all
                    - TypeFacts::TYPEOF_NE_FUNCTION
                    - TypeFacts::TYPEOF_NE_HOST_OBJECT);
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
                    } else if symbol_flags.intersects(SymbolFlags::ENUM | SymbolFlags::VALUE_MODULE)
                    {
                        object_strict
                    } else {
                        both
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
                    if has_call_signature { function_strict } else { object_strict }
                }
                _ => both,
            };
        }
        // Every arm below is a **non-nullable** type, so each carries
        // `nullable_never` beside its truthiness — the `Base*StrictFacts`
        // half. The one type that must not reach here is `any`, which is
        // undecidable on both axes and falls to the default.
        let truthiness = match &ty.data {
            TypeData::StringLiteral(value) => {
                if value.is_empty() {
                    TypeFacts::FALSY
                } else {
                    TypeFacts::TRUTHY
                }
            }
            // The stored form is the *printed* form, so the falsy values are
            // exactly the spellings of zero a `.types` baseline would print.
            TypeData::NumberLiteral(value) => {
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
        self.binder.symbols().get(symbol).flags.intersects(SymbolFlags::VARIABLE)
    }
}

/// The property name an access expression reads, for the two forms
/// `getAccessedPropertyName` (`flow.go`) decides.
///
/// A property access answers its member name; an element access answers only a
/// **string-literal** argument, because `a[i]` names a property only when `i`
/// is constant, and this port cannot prove that (see
/// [`Checker::references_match`]).
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
