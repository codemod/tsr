# The §12 fixpoint patch, verbatim — for the trace-instrument session

This is the exact reverted implementation from the ninth session's two
attempts (`checker-notes-narrow.md` §12–§12.2), preserved so the next
session applies it mechanically instead of re-deriving. Apply, add the
trace, run `parsingDeepParenthensizedExpression`, find where `error` enters.

## 1. Checker fields (`checker.rs`, beside `symbol_assignment_scan`)

```rust
/// `(loop junction, reference key) -> the fixpointed type` — upstream's
/// `flowLoopCache` (`flow.go:1336`).
pub(crate) flow_loop_cache: FxHashMap<(u32, u64), TypeId>,
/// The in-process loop junctions with their types-so-far — upstream's
/// `flowLoopStack` (`flow.go:1347`).
pub(crate) flow_loop_stack: Vec<((u32, u64), Vec<TypeId>)>,
```
(plus the two `::default()` inits in the constructor)

## 2. Dispatch arm (`flow.rs`, before the BRANCH_LABEL arm)

```rust
} else if flags.contains(FlowFlags::LOOP_LABEL) {
    break self.get_type_at_flow_loop_label(state, flow);
```

## 3. The function (before `get_type_at_flow_branch_label`)

```rust
fn get_type_at_flow_loop_label(&mut self, state: &mut FlowState, flow: FlowId) -> FlowType {
    let key = (
        tsr_core::index::Idx::index(flow) as u32,
        match state.symbol {
            Some(symbol) => symbol.index() as u64,
            None => (1 << 63) | u64::from(state.reference.as_u32()),
        },
    );
    if let Some(&cached) = self.flow_loop_cache.get(&key) {
        return FlowType { t: cached, incomplete: false };
    }
    for (stacked, types) in &self.flow_loop_stack {
        if *stacked == key {
            let so_far = types.clone();
            let t = if so_far.is_empty() { self.intrinsics.never }
                    else { self.get_union_type(&so_far) };
            return FlowType { t, incomplete: true };
        }
    }
    let antecedents: Vec<FlowId> = self.binder.flow().antecedents(flow).collect();
    let mut types: Vec<TypeId> = Vec::new();
    let mut subtype_reduction = false;
    let mut first: Option<FlowType> = None;
    for antecedent in antecedents {
        let depth_mark = state.depth;
        let flow_type = match first {
            None => { let e = self.get_type_at_flow_node(state, antecedent); first = Some(e); e }
            Some(_) => {
                self.flow_loop_stack.push((key, types.clone()));
                let shared_mark = self.shared_flows.len();
                let back = self.get_type_at_flow_node(state, antecedent);
                self.shared_flows.truncate(shared_mark);
                self.flow_loop_stack.pop();
                if let Some(&cached) = self.flow_loop_cache.get(&key) {
                    state.depth = depth_mark;
                    return FlowType { t: cached, incomplete: false };
                }
                back
            }
        };
        state.depth = depth_mark;
        if flow_type.t != self.intrinsics.never && !types.contains(&flow_type.t) {
            types.push(flow_type.t);
        }
        if !self.is_type_subset_of(flow_type.t, state.initial_type) {
            subtype_reduction = true;
        }
        if flow_type.t == state.declared_type { break; }
    }
    let result = if types.is_empty() { self.intrinsics.never }
        else if subtype_reduction {
            match self.union_with_subtype_reduction(&types) {
                Some(reduced) => reduced,
                None => state.declared_type,
            }
        } else { self.get_union_type(&types) };
    let incomplete = first.is_some_and(|f| f.incomplete);
    if incomplete { return FlowType { t: result, incomplete: true }; }
    self.flow_loop_cache.insert(key, result);
    FlowType { t: result, incomplete: false }
}

fn is_type_subset_of(&mut self, sub: TypeId, superset: TypeId) -> bool {
    if sub == superset || self.store.get(sub).flags.contains(TypeFlags::NEVER) { return true; }
    let super_constituents: Vec<TypeId> = match &self.store.get(superset).data {
        TypeData::Union { types, .. } => types.clone(), _ => vec![superset], };
    let sub_constituents: Vec<TypeId> = match &self.store.get(sub).data {
        TypeData::Union { types, .. } => types.clone(), _ => vec![sub], };
    sub_constituents.into_iter().all(|c| {
        let regular = self.get_regular_type_of_literal_type(c);
        super_constituents.iter().any(|&s| s == c || self.get_regular_type_of_literal_type(s) == regular)
    })
}
```

## 4. The branch-label incomplete fix (real defect, apply with the arm)

In `get_type_at_flow_branch_label`, accumulate `incomplete |=` across
antecedent walks and return it instead of the hardcoded `false`.

## 4b. REQUIRED: the JS decline (§12.4 — the 196-line mystery's answer)

Gate the dispatch arm:
```rust
} else if flags.contains(FlowFlags::LOOP_LABEL)
    && !self.in_js_file(state.reference)
{
    break self.get_type_at_flow_loop_label(state, flow);
```
With it the parsingDeep family (a JS file) is untouched in both directions
and the residue is the .ts loop-iteration family only.

## 5. The measured state with all of the above

WITHOUT the JS decline: +241 / 196 lost / 77 wrong / 2 regressed. WITH it
(§12.4): **+111 / 63 wrong / 12 lost / 2 regressed** — the residue is the
`.ts` loop-iteration family, candidate mechanism: upstream's
`antecedentTypes` aliasing into the on-stack entry (`flow.go:1367`) versus
this port's frozen `types.clone()` snapshot.
