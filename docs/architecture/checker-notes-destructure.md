# Destructuring / binding patterns — the plain leg (`bd tsr-o00`)

Registered at `1e40af4` **before any code**, per `docs/conventions.md`. Fifth
session. The board row is §4.2's 649-score item ("destructuring / binding
patterns", reachable 2,781 at `b00738d`); a fresh `depend.rs` at `1e40af4`
reads the row at 3,728 lines, 23.8% want-any, 370 cases, top-1 6.1% —
unchanged within the board's noise band.

---

## 1. Sizing — the mechanism's own population, not the row

`examples/bindgap.rs` (committed with this page) classifies every gap line
whose node is the *name* of a `BindingElement` by what upstream's
`getBindingElementTypeFromParentType` (`checker.go:17707`) would need.
Controls both pass: C1 (every classified line answers `errorType` today) = 0
violations; C2 (buckets sum to classified) exact. Classified: **2,632** of
the row's 3,728 — the remainder is lines `depend.rs` *propagates* to a
binding-element root from elsewhere plus varied/divergent cases the probe
excludes, and it is not this item's population.

| leg | lines | want-any | verdict |
|---|---:|---:|---|
| **plain or renamed element, source types today** | **1,228** | 117 (9.5%) | **the build** |
| default under a *typed annotation* | 69 | 10 | refused this cut — see §3 |
| contextual pattern parameters | 604 | 345 (57%) | refused — §3 |
| rest elements | 172 | 20 | refused — §3 |
| computed property names | 86 | 41 | refused — §3 |
| default on an initializer-typed source | 155 | 22 | refused — §3 |
| no-source holders (`for-of`/`for-in`/ambient) | 113 | 50 | refused — §3 |
| source itself GAPS (annotation or initializer) | 258 | 69 | downstream, stays a gap by construction |

Net reachable for the build: **1,228 − 117 ≈ 1,111**. The observed
conversion band is 15–57% (§4.1 of `STATUS.md`), so the honest expectation
is **+170 to +630 from the sized subset**, plus whatever downstream lines a
typed destructured symbol unblocks (the tuple build's precedent: 12 whole
cases finished on lines outside its sized row).

## 2. The mechanism, anchored

Upstream reaches a binding element's type through
`getTypeOfVariableOrParameterOrPropertyWorker` (`checker.go:16578`), which
sends `KindBindingElement` with the other variable-likes into
`getWidenedTypeForVariableLikeDeclaration`;
`getTypeForVariableLikeDeclaration` (`checker.go:16652`) dispatches
`IsBindingElement` → `getTypeForBindingElement` (`checker.go:17684`):

1. **the parent's type** — `getTypeForBindingElementParent`
   (`checker.go:17695`) → `getTypeForVariableLikeDeclaration` on the
   holder, which for a nested pattern is another `BindingElement`
   (recursion) and otherwise the annotation-else-initializer paths this
   port already has;
2. **the element's slice of it** — `getBindingElementTypeFromParentType`
   (`checker.go:17707`): an `any` parent answers `any`; an object pattern
   takes `getLiteralTypeFromPropertyName` into an indexed access at
   `AccessFlagsExpressionPosition` — which for a literal name is exactly
   this port's `get_type_of_property_of_type` + applicable-index-info seam
   (`indexed.rs` states the same observation for `a["b"]`); an array
   pattern takes the element's *position* as a numeric-literal indexed
   access when the parent is array-like (`checker.go:17768`), which the
   tuple reverse index (`tsr-5ll`) and the number index signature already
   answer.

What this port adds: a `SyntaxKind::BindingElement` arm in
`get_type_of_variable_or_parameter_or_property_worker`'s dispatch
(`symbols.rs`), a `get_type_for_binding_element` restricted to the legs
above, and a parent-type helper that **refuses with `errorType` where
upstream would consult unported machinery** rather than falling through to
the implicit-`any` mapping the identifier path uses — a pattern parameter
without an annotation *does* consult contextual typing upstream, and
answering `any` there would be a wrong line on the 259 of 604 lines whose
baseline wants a real contextual type.

## 3. Refused legs, each with its number

- **Contextual pattern parameters — 604 lines, 57% want-any.**
  `getContextuallyTypedParameterType` for patterns is the contextual-typing
  refusal re-armed at `0d56467`; the 345 want-`any` lines are upstream's
  *implicit any* (contextual absent), and this port cannot tell those
  apart from the 259 real contextual answers without the subsystem. A gap
  beats guessing which of the two it is.
- **Rest elements — 172.** Object rest needs `getRestType`
  (`checker.go:17792`): spreadability, modifier filtering, `Omit` for the
  generic case. Array rest needs `sliceTupleType`. Both whole-construct
  refusals.
- **Computed property names — 86.** `getLiteralTypeFromPropertyName` on a
  computed name needs late-bound names (`bd tsr-y4u.11`'s family).
- **Defaults — 155 on initializer-typed sources, 69 under annotations, and
  all defaults in this cut.** The annotation-less default path is
  `getUnionTypeEx(..., UnionReductionSubtype, ...)` (`checker.go:17789`) —
  the same reduction §5 of `STATUS.md` refused for `||`/`??`. The
  annotated path (`checker.go:17784`) needs only `getNonUndefinedType`
  under a facts test and is separable, but it is 69 lines against the risk
  of a half-modelled construct; it stays refused in this cut and is the
  natural second iteration. **The element gaps whole when a default is
  present**, not just the default's contribution.
- **No-source holders — 113.** A `for-of`/`for-in` holder needs iteration
  (`checkRightHandSideOfForOf`) or index-key machinery; a genuinely
  source-less pattern needs `getTypeFromBindingPattern`
  (`checker.go:17904`), the implied-type constructor. Both unported; the
  parent-type helper answers `errorType` for a holder with neither
  annotation nor initializer, so the whole bucket stays a gap.
- **`getFlowTypeOfDestructuring` (`checker.go:17849`) — unported, and it
  is the named risk.** It flow-narrows the element through a synthetic
  reference; without it this arm answers the *declared* slice where
  upstream may answer a *narrowed* one. The predicted wrong-family is
  `conformance/dependentDestructuredVariables` (50 lines in the probe's
  top cases), attributed in advance exactly as `cf33aee` attributed
  `controlFlowOptionalChain`. Filed with the build's issue; if the
  gap→wrong leg fires *outside* this family, that is a build bug, not the
  predicted residual.

## 4. The bar — registered before the code

> **KEEP** if **net ≥ +400**, **lost ≤ 20 with every loss diagnosed as a
> cascade** (this arm fires only where the worker's catch-all answered
> `errorType`, so a loss cannot be the arm's own line — asking what input
> makes the leg non-zero, per conventions: only a *cascade* through a
> consumer of the newly typed symbol), **fewer cases regress than
> finish**, and **gained ≥ 3 × new wrong** by `wrongdelta`.
> **REVERT** otherwise.

- The net floor is 36% of the sized subset's 1,111 net — the same
  mid-band discipline as `t[0]`'s bar, not the top of the band.
- The gap→wrong leg is the live one, and its predicted failure family is
  named in §3: `dependentDestructuredVariables` and flow-dependent
  destructuring generally. New wrong concentrated there is the accepted
  residual; new wrong *elsewhere* indicts the arm.
- **Falsifier:** if more than 25% of the gain lands in a single case, the
  population was never the 370-case row this was sized on
  (`destructuringParameterDeclaration1ES6`'s family are the biggest cases
  at ~2.8% each, so concentration above 25% means the mechanism reached
  something else entirely).

If a leg fires: first hypothesis is the build is wrong, second is the
bar's premise is wrong, there is no third. Overriding needs evidence
independent of this page's author, recorded loudly.

## 5. The result, scored against §4's bar

Two iterations, both measured over the same `git stash` before-state at
`982bfe4`; the first is recorded because its numbers forced a design change.

**First run** (no array-literal guard, no widening wrap):
net **+1,239**, lost 1, 0 regressed, 16 finished, new wrong **+207** (6.0×,
leg 4 passes). But the new-wrong families included ~80 lines this arm was
*minting* rather than inheriting: `var [a, b] = [1, "x"]` elements printed
`string | number` because upstream types the initializer under the
pattern's implied contextual type (tuple inference,
`checker.go:16748` → `:17904`) — a mechanism this port does not have. Leg 4
passing does not license a family the faithfulness rule forbids: the
construct now **refuses whole** (the array-literal guard in
`get_type_for_binding_element_parent`), filed as `bd tsr-84iz`.

**Final run**, scored:

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ +400 | **+1,081** (1,082 gained / 1 lost, 154 cases) | pass |
| 2 | lost ≤ 20, each a diagnosed cascade | **1** — `objectBindingPatternKeywordIdentifiers02`, `var { while: while }`: upstream's parse-error recovery answers `any` where the now-typed member answers `number`; error-recovery poisoning, not this arm's rule | pass |
| 3 | regressions < finished | **0 < 16** | pass |
| 4 | gained ≥ 3 × new wrong | **1,082 vs 109 = 9.9×** | pass |

**Falsifier:** top gaining case `declarationEmitBindingPatternsUnused` at
88/1,082 = **8.1%**, under the 25% line. The gain is distributed as the
370-case population was.

**KEEP.** Conversion is **97% of the sized 1,111 net** — above the 15–57%
band again, because a typed destructured symbol unblocks its consumers
(the tuple build's precedent; 16 whole cases finished).

The 109 residual new wrong, attributed: **21 got-`any`** are faithful
`IsTypeAny` propagation of parents whose own printed line was already
wrong; **~35** are the flow families the bar named in advance
(`destructuringTypeGuardFlow`, `controlFlowDestructuring*`,
`noUncheckedIndexedAccessDestructuring` — `getFlowTypeOfDestructuring`
unported, filed `bd tsr-pqnh`); **~10** are shorthand members inside
newly-typed functions printing `true` for `boolean` — a **pre-existing
flow divergence exposed, not minted**: assignment narrowing selects the
*regular* `true` from `boolean`'s union, so mutable-location widening's
fresh test skips it (filed `bd tsr-xs0`, it is `flow.rs` code); **10** are
`declarationEmitBindingPatternsUnused`'s `typeof alias` annotation-reuse
lines, `bd tsr-5o2`'s standing family; the remainder are singles in the
same families.

**A fixture came due**, the eleventh: `types.rs`'s
`a_declaration_kind_this_slice_does_not_port_is_an_error_type` used a
binding element as its "unported declaration kind" and went red when the
kind ported. Rewritten as the positive control, with the worker's
catch-all still pinned by its sibling test.

**The parser change this carried:** array-binding holes. The old loop
skipped `[, a]`'s hole entirely, silently renumbering every element after
it; upstream records an all-nil `BindingElement` (`parser.go:1663`) and
indexes by slice position (`checker.go:17750`). The printer emits nothing
for an all-`None` element and the binder binds nothing, so the change is
shape-only; `tests/destructure.rs` pins the numbering.

## 6. Test discipline

Expectations come from baselines, never intuition — five intuition
expectations have been wrong across four sessions. The fixtures to draw
from, verified to exist in the corpus: `conformance/declarationsAndAssignments`,
`compiler/renamingDestructuredPropertyInFunctionType`,
`conformance/destructuringParameterDeclaration1ES5` (annotated-parameter
patterns), `conformance/destructuringWithLiteralInitializers2`. Every
refused leg gets a pinned pair — the refused form beside the ported one —
so the frontier moving turns the test red instead of silently widening.
