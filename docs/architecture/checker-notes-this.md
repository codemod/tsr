# `this` with a written `this` parameter: +2,733 lines, 91.9% of it one case

Status: measured 2026-08-06 at **`8229b58`** and built in the same commit. The
instrument is `crates/tsr-conformance/examples/thisparam.rs`. Upstream references
are to `vendor/typescript-go` @ `5b1047d10`, **each re-taken with `grep -n` on
the declaration** rather than carried from the spec that handed this over:

| symbol | line |
|---|---:|
| `checkThisExpression` | `checker.go:12077` |
| `tryGetThisTypeAt` | `checker.go:12134` |
| `tryGetThisTypeAtEx` | `checker.go:12146` |
| `getThisContainer` | `checker.go:12188` |
| `getThisType` | `checker.go:22908` |

All five matched the spec. Re-taking them cost one command and is the only thing
that distinguishes an anchor that is right from one that resolves.

**Every number here is an assertion line.**

## 1. The population and the rule, both registered before the number

> **T is every rendered `.types` line whose node kind is `ThisKeyword` and whose
> nearest this-container declares an explicit `this` parameter carrying a written
> type annotation.**

The this-container is `getThisContainer(node, includeArrowFunctions: false, …)` —
the nearest enclosing function-like node **skipping arrows**, which is what keeps
an arrow transparent to `this`. T is a function of the tree alone, so `|T|` is
invariant under any checker change; it read **128 before and after.**

- **RT-1 (size).** Ship only if the counterfactual converts **≥25% of T's gap**.
- **RT-2 (match).** And **≥70% of the lines that stop gapping match exactly.**

Thresholds identical to `RC2` and `RA`, on purpose.

### The forecast, with legs labelled and a bar on the inferred one

- **Annotated leg.** The type is a written node and the renderer is already
  exercised. Forecast 60–85% of T's gap.
- **Inferred leg. Excluded by construction, forecast 0.** An unannotated `this`
  parameter would take the implicit `any` from `get_type_of_symbol`, and
  `checker-notes-rank.md` §6 records 21,685 lines already banked on `any` with the
  computed and the defaulted unseparated. The corpus holds **3** such parameters
  and the probe prints the count as a control on the exclusion.
- **Point forecast: 70 converted, bars 45–95.**

## 2. Measured

| | before | after | delta |
|---|---:|---:|---:|
| **\|T\|** | 128 | **128** | **0** |
| T right | 10 | **117** | **+107** |
| T gap | 100 | **0** | −100 |
| T wrong | 18 | **11** | −7 |

- **RT-1: 107 against a 25-line bar. FIRES.** Over 100% because 7 previously
  *wrong* lines also became right.
- **RT-2: all 100 lines that stopped gapping match exactly — 100%. FIRES.**

**I was outside my own error bars, and that is a miss.** I forecast 70 with an
upper bound of 95 and it converted 107. The cause is over-applying `bd tsr-a5d`'s
lesson — *reachability inside a source is a second filter* — to a source where
the filter barely bites: a `this` annotation is nearly always a class name, an
interface name or a keyword, all of which resolve. **A forecast that is too
pessimistic is still wrong**, and the correction is that the reachability filter
has to be estimated from the *shapes the source contains*, which this page's own
baseline histogram showed before the build: `{ test: Test; }` 24, `any` 23, `T`
13, `C` 6 — all resolvable.

## 3. Corpus, `examples/casedelta.rs`

```
  matched  299,721 -> 302,454     +2,733 lines
  cases moved                     15
  cases that REGRESSED            0
  cases that newly finish         0
```

**91.9% of that is one case.** `compiler/binaryArithmeticControlFlowGraphNotTooLarge`
gains **2,512**; the rest is `conformance/controlFlowAliasing2` 72,
`thisTypeInFunctions` 46, `thisTypeInFunctionsNegative` 24,
`thisTypeAccessibility` 18, `inferParameterWithMethodCallInitializer` 14 and nine
smaller. **The distributed gain is 221 lines over 14 cases; the headline is a step
function**, and anyone planning against 2,733 is planning against one file.

The 107-against-2,733 gap is the cascade: T counts the `this` lines themselves,
and a typed `this` un-gaps every `this.x`, `this.x.y` and expression built on
them. Multiplier **25.5×**, the largest this workstream has measured, and it is a
property of that one file rather than of the form.

## 4. The two things the spec said would bite, and what happened

### Arm 1 shadows arm 2, and a mutation proves it

`tryGetThisTypeAtEx` (`checker.go:12146`) asks `ast.IsFunctionLike(container)`
**before** `ast.IsClassLike(container.Parent)`. A method is function-like, so a
method carrying a `this` parameter answers the annotation and never reaches the
class's `thisType`. The natural port tests class-ness first and prints `this`
where upstream prints the annotation — a wrong line, not a missing one.

It is not taken on trust. **M2** — make `this_parameter_type` return `None` for a
`MethodDeclaration`, which is exactly what "class-ness first" amounts to, since a
method is the only container both arms can claim — goes red at
**`left: "this"`, `right: "I"`**. That is the predicted wrong answer, produced on
demand.

A third rearrangement was tried first, moving the test below the whole match. It
went red on the *function* fixture at `"error"` and says nothing about the class
arm, so it is recorded as insufficient rather than quoted as evidence.

### `check_this_expression` already existed

`expressions.rs` typed the class case before this commit, so
`checker-notes-recvgap.md` §4's *"positions this port does not type at all"* was
stale for `this`. This extends a walk rather than starting one, and the class arm
is untouched — the fourth fixture pins that a method **without** a `this`
parameter still falls through to it, which is also upstream's order.

## 5. The regression the corpus found, and the line that fixes it

The first version returned `get_type_from_type_node(annotation)` unconditionally.
Measured, it **regressed two cases**: `conformance/looseThisTypeInFunctions` −4
and `compiler/unusedParametersThis` −1.

The cause is `explicitThis(this: this, m: number)`. The annotation is a
**`ThisTypeNode`**, which this port has no arm for, so arm 1 correctly took
precedence and then answered `errorType` — turning 5 lines the class arm had been
getting *right* into gaps.

The fix is one line, and it is upstream's own shape rather than a patch: an
annotation this port cannot resolve **falls through** to the class arm.
`getThisTypeOfSignature` answering `nil` is what sends `tryGetThisTypeAtEx` on to
`ast.IsClassLike`, and *"we could not read the annotation"* is this port's `nil`.
After it: **+2,733, 0 regressed.**

**This is the case for measuring rather than reasoning, in its cheapest form.**
The arm was faithful, the tests were green, and it still destroyed 5 correct
lines — because taking precedence and being able to *answer* are different
things, and only the corpus knew the difference. `casedelta`'s per-case join is
what surfaced it; a net figure would have shown +2,695 and hidden it.

## 6. What was deliberately not built

- **The 163 `this` lines wanting `any`** and the **63 wanting
  `typeof globalThis`** that `bd tsr-tjz` excludes stay excluded. T's own
  baseline histogram holds 23 `any` answers, and those *did* convert — but from a
  **written** `this: any`, which is a computation, not a default. The exclusion
  is about answering `any` where nothing said so.
- **An unannotated `this` parameter.** 3 in the corpus, deliberately gapped, with
  the count printed as a control.
- **Arms 2–6 of `tsr-tjz`** — free function, source file, module, accessor,
  object literal. Untouched and unsized here.

## 7. How you would know this page is wrong

- **`|T|` differing between the runs** would mean the pair is not a pair. It
  reads 128 both times.
- **The 2,512 case.** If `binaryArithmeticControlFlowGraphNotTooLarge` were
  excluded the item is 221 lines, and any plan quoting 2,733 as a rate should be
  re-derived. The per-case join is printed so this cannot be missed.
- **RT-2 reading 100%** is unusually clean and deserves suspicion. The mechanism
  is that a written annotation is exactly what upstream prints, so there is no
  inference step to get wrong — the same reason the `Named`-callee row in
  `checker-notes-callres.md` §5 was *not* clean, since there the answer had to be
  computed rather than read.
- **The fall-through hides future gaps.** An annotation this port cannot resolve
  now silently defers to the class arm, so a *wrong* class answer would look like
  arm 2 working. The falsifier is a `.types` line under a `this`-parameter
  container whose baseline is neither the annotation nor `this`.

---

# `TemplateExpression` and `SuperKeyword`: both rules fail, neither ships

Measured at **`150b8ba`**. `RM` and `RS` were registered in their own commit
(`14174b3`) before the probe ran once; the instrument is
`crates/tsr-conformance/examples/tmplsuper.rs`. Anchors re-taken with `grep -n`:
`checkTemplateExpression` **`checker.go:7976`**, `checkSuperExpression`
**`checker.go:7854`**. **No checker code is in this commit.**

## Both rows are perfectly terminal, and that is confirmed

| row | lines | right | gap | wrong |
|---|---:|---:|---:|---:|
| `TemplateExpression` | 1,067 | 0 | **1,067** | 0 |
| `SuperKeyword` | 643 | 0 | **643** | 0 |

Every line of both rows is a gap — no right answers to protect and no wrong ones
to fix. The root-split's 92.3% and 100% own-root shares are corroborated by a
second instrument that shares no code with it.

## `TemplateExpression`: the premise I registered was false

**RM's forecast rested on a claim, and the probe falsified it before any code was
written.** I registered:

> A span that is not constant makes leg 1 unreachable **by construction**, so a
> conservative port answers `string` only when at least one span's type is *not*
> a unit type.

Cross-tabbing that test against the baseline:

| predicted leg (some span is not a unit) | lines with a comparable RHS | want `string` |
|---|---:|---:|
| yes | 557 | **262 — 47.0%** |

The other 295 want a folded literal: `"-1"`, `"05"`, `"2-1"`, `"125"`. They come
from `conformance/templateStringBinaryOperations*`, where the span is
`` `${1-2}` `` — this port types `1 - 2` as `number`, not a unit, so my test calls
it non-constant, and **upstream's `evaluate` folds the arithmetic anyway**.

**`evaluate` is a syntactic constant folder and consults no types at all.** So:

- **RM-1: 262 of 1,067 = 24.6%**, against a 25% bar — short by **4 lines**.
- **RM-2: 262 of 557 = 47.0%**, against 70%.

**Both fail. Refused.** And the useful part is stronger than the refusal: **the
`string` leg cannot be separated from the folding leg by any type-level test**,
because upstream's discriminator is syntactic constant-evaluability. Separating
them *is* building the evaluator. A row that looks like "the easy leg of three"
is one leg that cannot be reached without one of the others.

## `SuperKeyword`: built, measured, reverted

The arm was written — `getSuperContainer(node, stopOnFunctions: true)`, the
static/instance split on `ast.IsStatic(container)`, the base through
`base_symbols_of` (`crate::members`) — and all checker tests stayed green.

| | before | after |
|---|---:|---:|
| S right | 0 | **198** |
| S gap | 643 | 188 |
| S wrong | 0 | **257** |

- **RS-1: 198 converted of 643 = 30.8% ≥ 25%. Fires.**
- **RS-2: 198 of the 455 that stopped gapping = 43.5%**, against 70%. **Fails.**

**Reverted.** 1.3 wrong per converted is worse than three items this workstream
has already refused.

### The diagnosis, which is one bug and is not fixed here

```
   51  ours Base    them typeof Base
   43  ours A       them typeof A
   26  ours C       them typeof C
    9  ours B       them typeof B
    9  ours Base    them any
```

**~151 of the 257 are the instance answer given where upstream gives the static
one** — one classification error, not a diffuse problem. The static/instance
split is `ast.IsStatic(container)` and this port's version of it is reading
something wrong; the modifier test itself is character-for-character the binder's
(`crates/tsr-binder/src/lib.rs:509`), so the fault is more likely in *which node*
the walk calls the container.

**I did not fix it, and so I am not claiming it.** RS-2 is what it measured, the
arm is reverted, and `bd tsr-h1s` carries the diagnosis. On the evidence the row
is worth more than 198 — if the static split is corrected, ~151 wrong lines
become right and RS-2 would read roughly 349 of 455, **76.7%** — but that is an
arithmetic projection from a histogram, not a measurement, and this page has
refused three items for less.

## What this cost and what it bought

Two rows described as the purest terminal population on the board produced **zero
shipped lines**. That is the correct outcome for both: one had a false premise
and the other manufactured 1.3 wrong per right. The registration in `14174b3`
is what makes that legible — the forecast, the legs and the bars were on the
record before either number existed, so neither refusal is a rationalisation.

---

# `super`, second round: both bars cleared, +838 lines, 0 regressed

Measured at **`4fb6fe6`** and built in the same commit. Same bars as the round
that refused it — **RS-1 ≥25%, RS-2 ≥70%, neither softened.**

## The defect was the rule, not the walk

Last round's diagnosis was *"the fault is likelier in **which node** the walk
calls the container, because the modifier test was character-for-character the
binder's"*. **That hypothesis was wrong**, and checking it cost one command: the
`SyntaxKind` variants for `Constructor`, `GetAccessor` and `SetAccessor` all
exist and the walk was finding them.

Reading upstream's tail is what found it. `checker.go:7946`:

```go
if ast.IsStatic(container) || isCallExpression {
    …
    return c.getBaseConstructorTypeOfClass(classType)
}
```

`isCallExpression` is *"this `super` is the callee of its own call"*
(`checker.go:7855`). **A `super(...)` call answers the base constructor type —
`typeof Base` — even inside an ordinary instance constructor**, because what it
calls is the base constructor. The walk was right; the rule had a second
disjunct.

That is the corrective three agents have now hit: **ask the code, not the data
structure.** The data structure had nothing to confess — every node kind, every
modifier test and every container was correct. Only upstream's own condition
said the split had two inputs and this port was reading one.

## Measured

| | before | after |
|---|---:|---:|
| S right | 0 | **374** |
| S gap | 643 | 218 |
| S wrong | 0 | **51** |

- **RS-1: 374 of 643 = 58.2% ≥ 25%. Fires.**
- **RS-2: 374 of the 425 that stopped gapping = 88.0% ≥ 70%. Fires.**

Against the first round's **198 right / 257 wrong**: the one disjunct moves 176
lines from wrong to right and converts 176 more that had stayed gaps.

### The projection was low, and it was right not to be built against

Last round's arithmetic-from-a-histogram said a corrected split would read
**~76.7%**. It reads **88.0%**. The projection assumed the 151 misclassified
lines would flip and nothing else would move; in fact fixing the disjunct also
un-gapped 176 lines the wrong classification had been suppressing. **A histogram
can say which lines are wrong and cannot say what a fix does to the lines that
are not in it** — which is the same blind spot as sizing a row without its
cascade, arriving from the error side.

## Corpus, `examples/casedelta.rs`

```
  matched  302,454 -> 303,292     +838 lines
  cases moved                     153
  cases that REGRESSED            0
  cases that newly finish         6
```

**Distributed**, unlike the `this` row above: the largest case is
`conformance/asyncMethodWithSuper_es6` at 183 lines — **21.8%**, against 91.9%
for `this`. Then `asyncMethodWithSuper_es2017` 37,
`asyncMethodWithSuperConflict_es6` 37, `superCallInNonStaticMethod` 30. 153 cases
moved and 6 finished outright.

## The forecast leg that missed, named

The registration in `14174b3` said the **`any` leg is excluded by construction,
forecast 0.** It is not, and it did not.

```
    4  ours P              them any
    4  ours typeof Base    them any
    4  ours typeof OtherBase them any
    3  ours C              them any
    3  ours typeof C       them any
```

**50 of the remaining 51 wrong lines are `them any`** — positions where upstream
answers `any` because the base is unresolvable or the `super` is in an error
position, and this arm resolved the base anyway. The exclusion was written as
*"where the base does not resolve, this must gap"*, which is true and
insufficient: upstream also answers `any` where the base **does** resolve but the
reference is illegal. RS-2 clears its bar with all 51 counted, so nothing here is
load-bearing for the decision — but a leg forecast at 0 that delivered 50 is a
miss and is recorded as one. `bd tsr-h1s`.

## How you would know this is wrong

- **`|S|` differing between runs.** It reads 643 both times.
- **The `any` leak growing.** 50 lines today; if a later change resolves more
  bases it grows, and RS-2 falls. The probe prints the ours-against-upstream
  table on every run.
- **`parserRealSource11` still holds 51 of the 218 remaining gaps** — 23.4%,
  one file. The residual is more concentrated than the gain was.
