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
    /// Where this invocation's entries in `shared_flows` begin.
    shared_flow_start: usize,
    /// Recursion depth, against the 2,000 cap.
    depth: u32,
}

/// Upstream's cap (`flow.go:118`), reproduced exactly rather than rounded.
const MAX_FLOW_DEPTH: u32 = 2_000;

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
        if self.flow_analysis_disabled {
            return self.intrinsics.error;
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
            initial_type: if is_auto { self.intrinsics.undefined } else { declared_type },
            is_auto,
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
        if state.depth == MAX_FLOW_DEPTH {
            // Not a return value: upstream disables flow analysis for the rest
            // of the containing function and reports an error. The error is not
            // ported (no checker diagnostics yet, `bd tsr-4sc`), but the state
            // change is, because it is observable in every later answer.
            self.flow_analysis_disabled = true;
            return FlowType { t: self.intrinsics.error, incomplete: false };
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
                // `flowContainer` is not ported, so this never continues outward
                // into an enclosing function — see this method's siblings.
                break FlowType { t: state.initial_type, incomplete: false };
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
        // A binding pattern is excluded upstream: `let { a } = x` declares
        // through the pattern and the auto reduction never applies.
        if !matches!(node.name, Some(tsr_ast::BindingName::Identifier(_))) {
            return false;
        }
        node.r#type.is_none()
            && node.initializer.is_none()
            && !self.combined_node_flags(declaration).intersects(tsr_ast::NodeFlags::CONSTANT)
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
    ///
    /// Upstream's fresh-boolean-literal step (`flow.go:2421`) is not ported
    /// either — freshness is not modelled here.
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
        let mut kept = Vec::with_capacity(constituents.len());
        for constituent in constituents {
            if self.type_maybe_assignable_to(assigned, constituent) {
                kept.push(constituent);
            }
        }
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
    fn narrow_type(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        condition: NodeId,
        assume_true: bool,
    ) -> TypeId {
        let Some(node) = self.node_map.get(condition) else { return t };
        match node {
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
                    self.get_type_with_facts(t, facts)
                } else {
                    t
                }
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
            // of the eight strings' EQ side — all eight NE bits, no EQ.
            return TypeFacts::FALSY | undefined_facts | typeof_ne_all;
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
                TypeData::Named { members: Some(_), .. } => object_strict,
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
