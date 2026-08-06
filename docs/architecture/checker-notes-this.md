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
