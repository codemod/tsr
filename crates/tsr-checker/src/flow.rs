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
    /// What that reference resolves to.
    ///
    /// Upstream has no such field: it compares reference *expressions* with
    /// `isMatchingReference`, because a reference can be `a.b.c` and there is no
    /// single symbol for it. Here the match is identifier-only, so the symbol is
    /// the whole of it — see [`Checker::is_matching_reference`].
    symbol: SymbolId,
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
    /// families, `EQUndefined`, `NEUndefinedOrNull`, and the rest. Only the two
    /// truthiness bits are here, because they are the only ones this module's
    /// ported guards ask for. The rest arrive with their consumers rather than
    /// as a table nothing reads, which is the same discipline `FlowFlags`
    /// followed in the binder.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub(crate) struct TypeFacts: u32 {
        /// The type can be truthy.
        const TRUTHY = 1 << 0;
        /// The type can be falsy.
        const FALSY = 1 << 1;
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
        symbol: SymbolId,
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
        let is_auto = self.is_auto_typed_declaration(symbol);
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
        // The binder records an assignment's flow node against the *target*
        // node, which for `x = 1` is the identifier `x` and for a declaration
        // with an initialiser is the declaration itself. Both answer through
        // the symbol they declare or resolve to.
        if let Some(symbol) = self.binder.symbol_of(node) {
            return symbol == state.symbol;
        }
        if self.nodes.kind(node) != SyntaxKind::Identifier {
            return false;
        }
        let Some(Node::Identifier(identifier)) = self.node_map.get(node) else { return false };
        self.binder
            .resolve_name(self.nodes, self.node_map, node, identifier.text, SymbolFlags::VALUE)
            .is_some_and(|resolved| resolved == state.symbol)
    }

    /// Narrow `t` by a condition expression known to be true or false
    /// (`narrowType`, `flow.go:2226`).
    ///
    /// The unported forms return `t` unchanged, which is upstream's own default
    /// arm — see this module's header for why that makes a partial port safe.
    /// Not ported: `typeof` guards, equality and discriminant comparisons,
    /// `instanceof`, `in`, and user-defined type predicates. Each needs
    /// machinery this checker does not have yet (comparability, call
    /// resolution), and each currently leaves the declared type.
    fn narrow_type(
        &mut self,
        state: &mut FlowState,
        t: TypeId,
        condition: NodeId,
        assume_true: bool,
    ) -> TypeId {
        let Some(node) = self.node_map.get(condition) else { return t };
        match node {
            // `if (x)` and `while (x)`: the reference itself as the condition.
            Node::Identifier(_) => {
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
            _ => t,
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
    fn filter_type(&mut self, t: TypeId, predicate: impl Fn(&Self, TypeId) -> bool) -> TypeId {
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
        let both = TypeFacts::TRUTHY | TypeFacts::FALSY;
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
        // among the types this checker builds.
        if flags.intersects(TypeFlags::NULLABLE | TypeFlags::VOID) {
            return TypeFacts::FALSY;
        }
        // An object, a symbol, or a non-primitive is always truthy.
        if flags.intersects(TypeFlags::OBJECT | TypeFlags::ES_SYMBOL | TypeFlags::NON_PRIMITIVE) {
            return TypeFacts::TRUTHY;
        }
        match &ty.data {
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
            // `string`, `number`, `bigint`, `boolean`, `any`, `unknown`, a class
            // or interface instance, a type parameter: either, or undecidable
            // here. Both bits, so nothing is filtered out.
            _ => both,
        }
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
