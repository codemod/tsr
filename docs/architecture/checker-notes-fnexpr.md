# Function expressions: one guard priced and kept, one priced and removed

Status: measured 2026-08-06 at **`d612291`**, over the 9,538-case `.types` corpus
population, from one pinned binary in an isolated worktree. The instrument is
`crates/tsr-conformance/examples/fnexpr.rs`, added in the same commit as this
file. Upstream references are to `vendor/typescript-go` @ `5b1047d10`, each taken
from `grep -n` on the declaration or the call site named.

Reproduce:

```
cargo run --release -p tsr-conformance --example fnexpr
```

**Every number here is an assertion line in a `.types` baseline**, unless a row
says otherwise.

> **§1–§8 are the first measurement, taken at `d612291` with the checker
> unchanged, and they are left as they were — except their
> `crates/tsr-checker/src/signatures.rs` line anchors, which were **re-taken with
> `grep -n` on each declaration** after §9's change moved the file. An anchor
> that resolves while pointing at the wrong construct is worse than one that
> fails, and `xtask anchors` does not check Rust paths. §9 is a second item on the same
> page, measured at `3f140c2` and **built** in the same commit as this section.
> Nothing in §1–§8 is edited to reflect it; where a number moved, §9 says so.**

**§1–§8's verdict is a refusal, and it is a measurement rather than an
argument.** Removing the candidate blocker converts **229** lines and
manufactures **1,061** — 4.6 wrong per converted inside the population, 3.8
corpus-wide. Both pre-registered rules fail, and they fail on either of the two
defensible populations. No checker code was written.

---

## 0. The findings, in the order they change decisions

1. **The contextual guard in `get_type_of_function_expression` is load-bearing,
   and now measured.** Lifting it un-gaps 1,290 lines in P and gets 229 of them
   right. §4.
2. **The row's concentration claim holds** — 3.6% top-1 over 958 cases is the
   least concentrated large row anyone has sized here. It is the one property
   this row has that every other candidate this week lacked, and it is not
   enough. §2.
3. **The blocker splits almost exactly in half**, and the other half is a
   different item: 2,082 lines stop at the contextual guard, 2,000 stop inside
   the signature build with every syntactic gate clear. §3.
4. **P already contains 495 wrong lines, and a single rule would fix 192 of
   them** — widen a `null`/`undefined` inferred return to `any`. Measured
   whole-gradient it is **360 gained against 214 destroyed**, and the 214 are
   `strictNullChecks` cases. Not built; §6 says why and what it is waiting on.
5. **A control fired and its premise was mine.** §7.
6. **The other half of the row was split, and part of it was built.** `bd tsr-4e1`
   is 2,000 lines; 44.2% of it is kind 2, `bd tsr-4sc.9` inside it turned out to
   be **9 lines**, and the 693-line widening gate converted **565 against 98**
   — 85.2% match, +1,188 corpus-wide, 29 cases newly finished, **0 regressed**.
   Both pre-registered rules fired and it is in this commit. §9.

---

## 1. The population, named before the rules were registered

`docs/conventions.md` records a rule whose top-1 leg read 44.5% on the direct row
and 65.3% combined, with the rule never saying which. So this page says it first.

> **P is every `.types` assertion line this port renders whose node kind is
> `ArrowFunction` or `FunctionExpression`** — whole-gradient, all three outcomes,
> not restricted to the lines that currently gap.

**P is syntactic on purpose.** A checker change cannot alter which nodes exist,
so `|P|` is fixed across two runs of this probe — that is control C0, and it is
what makes a before/after pair comparable. A population defined as *"the lines
that gap"* would not be: the gap set moves under the very change being measured.

| form | right | gap | wrong | total | in P |
|---|---:|---:|---:|---:|:--:|
| `ArrowFunction` | 2,115 | 3,401 | 383 | 5,899 | yes |
| `FunctionExpression` | 536 | 883 | 112 | 1,531 | yes |
| `ObjectLiteralExpression` | 4,585 | 1,801 | 881 | 7,267 | **no** |
| `ArrayLiteralExpression` | 1,324 | 1,164 | 1,819 | 4,307 | **no** |
| **P** | **2,651** | **4,284** | **495** | **7,430** | |

The last two are `objects.rs` and `array_literals.rs`, another workstream's
files. They are printed because the same table answers them for free; **no rule
on this page is registered over them** and nothing here should be quoted as
though one were. The `ArrayLiteralExpression` line is worth one sentence anyway:
**1,819 of its 4,307 lines are already answered and wrong** — more wrong than
right — which is a fact about `array_literals.rs` and not about this row.

### On the 1,888 in the assignment

The assignment sizes `expression answered error: ArrowFunction` at **1,888 lines,
524 cases**. This probe measures **3,401 lines, 958 cases** for the same reason
string. Both are right and they answer different questions: 1,888 is
`rank_board`'s **TERMINAL half** of the row (`checker-notes-rank.md` §4 — the row
headline was 3,174 with 1,893 terminal at `33e3bd5`), and 3,401 is the row.

Sizing on the terminal half is the *methodologically correct* choice — a
propagated line is not converted by fixing this form — so this is a labelling
matter rather than an error. It is recorded because the same substitution has now
happened three times on this project in the other direction, and because **the
refusal below has to survive both denominators.** It does: §4.

## 2. Concentration

| form / outcome | lines | cases | top-1 | top-10 | three largest |
|---|---:|---:|---:|---:|---|
| `ArrowFunction` gap | 3,401 | 958 | **3.6%** | **17.5%** | `conformance/generatedContextualTyping` 123, `compiler/fatarrowfunctionsOptionalArgs` 100, `compiler/promiseType` 66 |
| `FunctionExpression` gap | 883 | 286 | 16.5% | 40.4% | `conformance/generatedContextualTyping` 146, `compiler/isolatedDeclarationErrorsReturnTypes` 38, `compiler/duplicateLocalVariable1` 33 |
| `ArrowFunction` right | 2,115 | 489 | 32.4% | 48.6% | `compiler/resolvingClassDeclarationWhenInBaseTypeResolution` 686 |
| `ArrowFunction` wrong | 383 | 133 | 11.5% | 36.8% | `compiler/promiseType` 44 |

**The assignment's central claim about this row is confirmed and is if anything
understated**: 3.6% top-1 and 17.5% top-10 over 958 cases, measured over the
whole row rather than its terminal half. Nothing evaporates on the concentration
check. This row fails for a reason no concentration check can see.

The two halves of P are *not* alike, and the difference would have been hidden by
a combined figure: `FunctionExpression`'s gap is 4.6× more concentrated by top-1
than `ArrowFunction`'s.

## 3. What stops them, in the checker's own order

`get_type_of_function_expression` (`crates/tsr-checker/src/signatures.rs:1039`)
is four gates. The probe reproduces the first three **syntactically**, in the
checker's order — the contextual guard runs before anything reads a parameter's
name, so a function with both an unannotated parameter and a binding pattern
belongs to the guard.

| gate | lines | share | cases | top-10 |
|---|---:|---:|---:|---:|
| unannotated parameter, contextual type not excluded (`signatures.rs:1043`) | **2,082** | 48.6% | 638 | 18.7% |
| every syntactic gate clear; the signature build failed | **2,000** | 46.7% | 584 | 30.9% |
| the function is generic | 142 | 3.3% | 73 | 36.6% |
| a parameter's name is a binding pattern (`signatures.rs:794`) | 60 | 1.4% | 25 | 75.0% |
| **total** | **4,284** | | | |

`Gate::SignatureBody` is a **positive** test — every earlier gate false *and* the
node function-like — not an `else` absorbing the remainder
(`docs/conventions.md`, *"a control bucket over a classifier whose last arm is a
default cannot fire"*). The two failure arms are positive tests on the node map
and both read 0.

**The row is two items of almost equal size**, and only the first is what the
row's name suggests. What the baseline prints for each is the tell:

- behind the **contextual guard**: `(x: number) => number` 72, `(s: string) => number` 44,
  `(n: unknown) => unknown` 38, `(x: any) => any` 36 — parameters upstream got
  from a contextual type.
- behind the **signature build**: `() => number` 307, `() => (Derived1 | Derived2)[]` 195,
  `() => any` 161, `() => string` 124, `() => void` 122 — **zero-parameter
  arrows whose return type inference failed.** That is `return_type_from_body`
  (`signatures.rs:392`) and its `bd tsr-4sc.9` multiple-returns arm, not
  contextual typing at all. **Which of its four documented refusals holds these
  2,000 is not measured** — one more bucket in the same probe, filed as
  `bd tsr-4e1`. Until it is, the 2,000 could be one change or four, and one of
  the four (a return expression that itself answers `error`) is not work at all.

## 4. The counterfactual, and the rules

### The rules, pre-registered before the first run

Both are written on lines that **match the baseline**, never on lines that become
computable — `docs/conventions.md`'s *size the conversion, not the population*,
bought by an item sized at 1,784 lines that converted 362.

- **RC (size).** Build only if the counterfactual converts **≥25% of P's gap**.
- **R2 (spellability / quality).** Of P's lines that **stop gapping**, **≥70%
  must match the baseline exactly.**

Conjunctive.

### The counterfactual is a mutation, applied and reverted

`docs/architecture/checker-notes-calls.md` is explicit that probing a blocker is
too weak — *"pick the shape, hardcode past the blocker, and see whether the form
then answers"*. **M-CF**: delete the guard at `signatures.rs:1043` so an
unannotated parameter no longer forces `errorType`. Run, record, revert. **The
relaxation is not in this commit; the number is.**

### The result

| | run A | run B (guard deleted) | delta |
|---|---:|---:|---:|
| **|P|** (C0) | 7,430 | **7,430** | **0** |
| P right | 2,651 | 2,880 | **+229** |
| P gap | 4,284 | 2,994 | −1,290 |
| P wrong | 495 | 1,556 | **+1,061** |

**1,290 lines stopped gapping. 229 of them are right. 1,061 are wrong.**

- **RC: 229 of 4,284 = 5.3%.** Threshold 25%. **Does not fire.**
  On the assignment's terminal-half denominator (≈2,168 lines) it is **10.6%**.
  **Does not fire on either population**, which is the point of naming the
  population first: the answer does not depend on the choice.
- **R2: 229 of 1,290 = 17.8%.** Threshold 70%. **Does not fire.**
- **4.6 wrong per converted, inside P.**

### And the collateral is worse than the target

The whole gradient under the same mutation:

| | run A | run B | delta |
|---|---:|---:|---:|
| right | 295,302 | 295,744 | **+442** |
| gap | 140,171 | 138,048 | −2,123 |
| wrong | 43,587 | 45,268 | **+1,681** |

**3.8 wrong per right corpus-wide**, against 4.6 inside P — so the 833 lines that
moved *outside* P moved in the same direction. They are visible in the table:
`ObjectLiteralExpression` gap 1,801 → 1,664 with wrong 881 → 999, and
`ArrayLiteralExpression` gap 1,164 → 1,140 with wrong 1,819 → 1,840. A function
expression that acquires a type is immediately usable as an object-literal
property's value and as an array element, and both then print it.

This is the reverse cascade `docs/conventions.md` records at 2.1 and 2.5 wrong per
right for two module-object designs. **Three independent items on this project now
measure between 2.1 and 4.6**, which is starting to look like a property of the
port rather than of any one row: un-gapping a form whose *content* is still
unported converts the shape and not the answer.

### What the guard is, restated

It is not conservatism. `signatures.rs:1043` is the statement *"this port has no
`getContextualSignatureForFunctionLikeDeclaration` (`checker.go:20226`), so an
unannotated parameter's type is unknown"*, and the counterfactual prices it: the
port would answer `any` where upstream has the contextual parameter type, 1,061
times. `has_no_contextual_type` (`signatures.rs:782`) recognising exactly one
position — the initialiser of an unannotated `var`/`let`/`const` — is what keeps
those 2,082 lines honest gaps instead of confident wrong answers.

**The item behind this row is contextual typing itself** (`contextual.rs`), not
the guard. Nothing smaller converts it, and that is now measured rather than
asserted.

## 5. R2's independent evidence: the shape is not the problem

For P's gap lines, what the **baseline** prints — no answer of ours enters this:

| form | lines with a comparable RHS | a function type | `any` | not a function type |
|---|---:|---:|---:|---:|
| `ArrowFunction` | 3,173 | **3,161 (99.6%)** | 1 | 10 |
| `FunctionExpression` | 795 | **793 (99.7%)** | 0 | 2 |

**99.6% of the answers this row needs are function types**, which this port can
render — `signature_to_string` (`signatures.rs:1056`) is ported and exercised. So
unlike the call-resolution row, this one does **not** fail because the answer is
unspellable. It fails because the *contents* of the signature — the parameter
types — are not computable, and R2 caught that only because it is registered on
exact matches rather than on the shape of the string. A shape-only R2 would have
read 99.6% and licensed the build.

That is worth carrying: **the spellability check has to be a match test, not a
shape test.** On the call row a shape test was adequate; here it would have been
exactly wrong, and in the confident direction.

## 6. The item this measurement found, and why it is filed rather than built

P holds **495 lines we already answer and answer wrongly**. The largest pattern:

```
   109  ours () => undefined     them () => any
    42  ours () => null          them () => any
    17  ours () => undefined     them () => string
    16  ours () => undefined     them () => number
```

A single rule — widen a `null`/`undefined` **inferred return** to `any` — turns
our string into upstream's exactly on **192 of P's 495** wrong lines (38.8%),
tested by substitution against the baseline rather than by a claim about the
cause. Whole-gradient, over every node kind, because a return-type rule cannot
tell a function expression from a function declaration:

| measure | gain (wrong → right) | risk (right → wrong) | ratio | gain cases | gain top-10 |
|---|---:|---:|---:|---:|---:|
| any position in a function type | 1,161 | 532 | 0.46 | 213 | 27.8% |
| **the return position only** — the rule a checker could write | **360** | **214** | **0.59** | 65 | 58.9% |

**Net +146 lines, and it is a swap rather than a gain.** Three reasons it is
filed (`bd tsr-f1p`) rather than built:

1. **The 214 are `strictNullChecks` cases.** `compiler/promiseTypeStrictNull` 44,
   `conformance/tsxTypeArgumentsJsxPreserveOutput` 31,
   `compiler/transformsElideNullUndefinedType` 12,
   `compiler/functionsMissingReturnStatementsAndExpressionsStrictNullChecks` 9.
   Upstream widens `undefined`/`null` to `any` **only when `strictNullChecks` is
   off**. An unconditional rule does not fix the port; it moves which half of the
   corpus the port is wrong on.
2. **The correct conditional cannot be written today.** Nothing in `tsr-checker`
   reads a `CompilerOptions` — stated at `crates/tsr-checker/src/unions.rs:26`
   and `crates/tsr-checker/src/flow.rs:364`, and `signatures.rs:450` already
   gaps a different case for exactly this reason. Giving the checker a strictness
   flag is a seam through `checker.rs` and the program, which are another
   workstream's files.
3. **No rule was pre-registered for it.** It was found *in* the data. Registering
   a threshold now, on a number already seen, and having it fire is precisely the
   retrofit `docs/conventions.md` calls *"a rule written on a quantity derived
   from the answer is caught by nothing"*. The measurement is the deliverable;
   the decision belongs to a cycle that registers its rule first.

Its concentration also argues for care: 58.9% top-10 on the gain side is a step
function over a handful of files, not a distributed row.

## 7. Controls

| control | reads | pinned by |
|---|---:|---|
| C0 `|P|`, both runs | **7,430 == 7,430** | **construction** — a checker edit cannot change which nodes the grammar produces |
| C1 a P gap line whose `gap_reason` names another kind | **0** | `gap_reason`'s expression arm prints `nodes.kind(id)` |
| C1′ the mirror | 4,284 | — |
| C2 an `ArrowFunction` with a `this` parameter | **4** | see below |
| C2′ a `FunctionExpression` with one | 14 | — |
| A1 P's gap == the gate total | 4,284 == 4,284 | arithmetic |

**C0 is the control this page rests on.** It is not arithmetic: it says the two
runs are the same 7,430 lines, so `+229` and `+1,061` are movements of individual
lines and not a change of denominator. Had P been defined as "the lines that gap",
every delta in §4 would have been uninterpretable.

**C2 fired, at 4, and the premise was mine.** It was registered as *"an arrow
function cannot have a `this` parameter, so this reads 0 by the grammar"*. That is
false: *An arrow function cannot have a 'this' parameter* is a **checker
diagnostic**, TS2730, reported at
`vendor/typescript-go/internal/checker/checker.go:2688` from
`An_arrow_function_cannot_have_a_this_parameter`
(`internal/diagnostics/diagnostics_generated.go:1729`). The parser builds the
node; the checker rejects it. So the 4 are real arrow functions in the corpus
that upstream errors on, the control is kept as a **diagnostic** rather than a
grammar control, and the pair (4 against 14) still does its original job.

Same family as the C2 that fired on `checker-notes-callres.md` — both times a
control I had called *pinned by construction* was pinned by something narrower
than I had written, and both times the pair-with-mirror is what showed it. A
control whose premise is wrong is still doing its job when it fires; the failure
would have been printing `0` and believing the premise.

The classifiers are asserted before the corpus runs (`check_classifiers`), and
two were proven red under named mutations:

| mutation | assertion that went red |
|---|---|
| `Form::of` maps `ArrowFunction` to `Form::FunctionExpression` | `assert_eq!(Form::of(SyntaxKind::ArrowFunction), Some(Form::Arrow))` — `left: Some(FunctionExpression)` |
| drop `carries_a_bare_nullish`'s `contains("=>")` early return | `assert!(!carries_a_bare_nullish("undefined"), "not a function type; the rule cannot reach it")` |

## 8. How you would know this page is wrong

- **The pair is not a pair.** C0 would differ between the runs. It reads 7,430
  both times.
- **M-CF is not the guard.** The mutation deletes exactly the two-line test at
  `signatures.rs:1043`; if it also changed something else, the gate table in §3
  and the delta in §4 would disagree about which lines moved. 2,082 lines are
  behind that gate and 1,290 stopped gapping, which is consistent with the guard
  releasing most of its own bucket plus part of the signature-build bucket
  (a parameter that stops being `errorType` also unblocks some return inferences)
  and **not** consistent with a wholesale change (`open` — the per-gate split of
  the 1,290 was not taken, and it is one more bucket in the same probe,
  `bd tsr-gkv`).
- **229 converted is a floor, not a ceiling, for a *real* implementation.** M-CF
  answers `any` for an unannotated parameter; a genuine contextual typing pass
  would answer the contextual type and convert more. So this page bounds *"what
  lifting the guard alone buys"*, which is what it set out to price, and it does
  **not** bound contextual typing. Nobody should quote 5.3% as an estimate for
  `contextual.rs`.
- **§6's gain and risk are string tests**, not cause tests. A line where we print
  `=> undefined` for a reason other than return-type widening is counted in the
  gain; a line where upstream's `undefined` return comes from an explicit
  annotation is counted in the risk. Both would have to be split by whether the
  declaration carries a return annotation before the item is built.
- **The 479,060 here is the count of lines this probe rendered, not the
  gradient's denominator** — the suite counts baseline lines and gets 478,954
  (`examples/reconcile.rs`, `bd tsr-zlo`). Every share on this page is over P or
  over a bucket this probe computes itself.


## 9. `bd tsr-4e1`: the other half, split, and the part of it that was built

Measured at **`3f140c2`**, and **built in the same commit as this section**. §1–§8
stand as taken at `d612291`; the only number of theirs this section moves is P's
outcome split, which is restated at the end.

### 9.1 The population, named before the buckets were written

> **Q is every rendered `.types` line whose node kind is `ArrowFunction` or
> `FunctionExpression` **and** which passes all three syntactic gates of §3** —
> every parameter annotated or the node is the initialiser of an un-annotated
> `var`/`let`/`const`; no parameter's name is a binding pattern; no type
> parameters.

Q is a function of the tree alone, so `|Q|` is invariant under any checker
change — **control C0′**, and it read **4,913 on all three runs** below.

**Q at `3f140c2`, before: 4,913 lines = 2,463 right + 2,000 gap + 450 wrong.**

Pre-registered, before the buckets existed:

- **RC2-whole**: license the item only if a counterfactual converts **≥25% of Q's
  2,000 gap lines** — 500 — to exact baseline matches.
- **RC2-part**: a **named** sub-bucket B licenses a *partial*, reported as such
  with B named, at ≥25% of B.
- **R2**: of the lines that **stop gapping**, **≥70% must match exactly.** A
  match test, not a shape test.

### 9.2 The split of the 2,000, and what it does to two filed issues

`return_type_of` and `return_type_from_body` reproduced in the probe in their own
order, using the public `check_expression` for the one step that needs a type:

| refusal | lines | share | cases | top-1 | what the baseline wants |
|---|---:|---:|---:|---:|---|
| a return expression answers `error` — **kind 2, not ours** | **885** | 44.2% | 257 | 24.1% | `() => (Derived1 \| Derived2)[]` 195, `() => any` 160 |
| one return type and it still gapped — the widening gate | **693** | 34.6% | 175 | 7.2% | `() => number` 236, `() => string` 102, `() => true` 23 |
| `async` or generator | 206 | 10.3% | 82 | 10.7% | `() => Generator<…>` |
| no `return` at all: `void` or `never` — needs reachability | 155 | 7.8% | 106 | 5.2% | `() => void` 111 |
| a return annotation is written, and it gaps | 52 | 2.6% | 28 | 23.1% | `typeof globalThis.isNaN` shapes |
| **two or more distinct return types** — `bd tsr-4sc.9` | **9** | 0.4% | 6 | 33.3% | `() => "ELSE" \| "SOMETHING"` |
| **total** | **2,000** | | | | |

**Two filed items are resized by this table, one up and one down.**

- **`bd tsr-4sc.9` — the union of return types — is 9 lines.** It has been carried
  as an item since it was written and it is the smallest thing on this page.
  Nine. Six cases. That is the fifth row on this project to collapse on contact
  with a measurement, and it collapsed by three orders of magnitude against the
  2,000 it sat inside.
- **44.2% of the item is kind 2 and belongs to nobody in this file.** A `return`
  expression that itself answers `error` is another row — 213 of the 885 are
  `conformance/generatedContextualTyping` alone. So `tsr-4e1`'s 2,000 is a
  ceiling holding at most **1,115** lines of work that `signatures.rs` owns.

### 9.3 The counterfactual, the test that caught it, and the build

The 693-line bucket is `inferred_return_type` refusing to widen a unit return
type unless the declaration could be *shown* to have no contextual type — which
for a function expression or an arrow meant everything except `const f = …`.

**M-W**: widen unconditionally. Applied, measured, and it **broke an existing
test** — `an_unannotated_parameter_is_any_only_where_no_contextual_type_can_supply_one`
(`crates/tsr-checker/tests/types.rs:1430`), which asserts that
`const f: () => 1 = () => 1` is a **gap**. It is right to: a written annotation
*is* a contextual type, it *is* a literal, and `isLiteralOfContextualType`
answers yes there. The corpus measurement had priced that position at 23+6 lines
wanting `() => true` / `() => false` and I had read those as unavoidable. The
test said otherwise for the one position it covers, and it was correct.

This is `docs/conventions.md`'s *"faithfulness is not evidence that a guard is
load-bearing"* running the other way: that rule deletes a guard no mutation can
make observable. Here a guard that looked like pure caution turned out to be
observable at exactly one position, and a test written for it fired.

**M-W2, shipped**: widen unless a contextual type is **written down at this
position** — the parent is a `VariableDeclaration` with an annotation whose
initialiser is this node, the exact complement of `has_no_contextual_type` over
the one position either can see. Everything between — a call argument, an
object-literal property, a `return` expression — widens, which is what upstream
does there because the contextual return type at those positions is almost never
a literal.

| | before | after (shipped) | delta |
|---|---:|---:|---:|
| **\|Q\|** (C0′) | 4,913 | **4,913** | **0** |
| Q right | 2,463 | **3,028** | **+565** |
| Q gap | 2,000 | 1,337 | −663 |
| Q wrong | 450 | 548 | **+98** |

- **RC2-whole: 565 of 2,000 = 28.3% ≥ 25%. FIRES.** The item is licensed whole,
  not as a partial, so RC2-part is not invoked.
- **R2: 565 of 663 = 85.2% ≥ 70%. FIRES.**
- **0.17 wrong per converted.** For scale, this project has refused items at 2.1,
  2.5 and 4.6 (§4 is the last of those). This is an order of magnitude the other
  side of everything refused.

The narrowing costs 4 converted lines against the unconditional version (569 →
565) and avoids 6 wrong ones (104 → 98) — so it is very nearly free, and the
reason to prefer it is not the ratio but that it refuses where the source says
something this port cannot read, rather than guessing over it.

The mechanism attribution is confirmed by the bucket that emptied: **the
widening-gate refusal falls 693 → 39.**

### 9.4 Why this is a port and not a policy

The tempting reading is *"we chose to widen because the numbers were good"*. That
is not what happened and the distinction is checkable.

`getWidenedLiteralLikeTypeForContextualReturnTypeIfNeeded` (`checker.go:20221`)
hands the type to `getWidenedLiteralLikeTypeForContextualType`, which widens
unless `isLiteralOfContextualType(t, contextualType)`. That function is
`checker.go:25522`, and **its last statement is `return false`** when
`contextualType` is `nil` (`:25551`). This port computes no contextual types —
`getContextualSignatureForFunctionLikeDeclaration` (`:29711`) is unported — so
`nil` is the only value that argument can take wherever the source does not write
one down. **Widening there is upstream's own function evaluated on this port's
inputs**, and refusing where an annotation *is* written is the acknowledgement
that at that one position the argument would not have been `nil`.

The 98 remaining wrong lines are not a mistake in this change; they are the
already-known cost of the missing contextual type, showing up in one more place,
and they are countable rather than diffuse.

### 9.5 Corpus-wide, and the case gate

`examples/casedelta.rs`, before and after, joined per case:

```
  matched  295,302 -> 296,490     +1,188 lines   (478,954 baseline lines)
  61.657%  -> 61.905%             +0.248pp
  cases moved                     171
  cases that REGRESSED            0
  cases that newly finish         29
```

**Zero cases regressed, and that is a structural fact rather than luck.**
`casedelta` counts *matched* lines, and a gap and a wrong both fail to match — so
a zero here says every one of the 156 newly-wrong lines corpus-wide came from the
**gap** column and none from the **right** column. The arithmetic says the same
thing independently: gap −1,344 = right +1,188 + wrong +156. Two instruments,
one written by someone else, and they agree.

The gain is distributed: the largest case is
`compiler/isolatedDeclarationErrorsReturnTypes` at 130 lines, 10.9% of the total,
over 171 cases. Next: `compiler/noImplicitAnyStringIndexerOnObject` 89,
`compiler/fatarrowfunctionsOptionalArgs` 74.

### 9.6 What §1–§8's tables now read

P's outcome split moves, and §1's table is *not* edited:

| | §1 (at `d612291`) | after this commit |
|---|---|---|
| P right | 2,651 | **3,245** |
| P gap | 4,284 | **3,592** |
| P wrong | 495 | **593** |

`|P|` is 7,430 in both, as C0 requires. §4's counterfactual on the *contextual
guard* was measured against the earlier baseline and has not been re-taken; its
conclusion is a ratio over lines that guard owns and nothing in this commit
touches that guard, but **the 229/1,061 figures should be re-measured before they
are quoted again** (`open`, `bd tsr-94m`).

### 9.7 What was deliberately not built

- **The 885 kind-2 lines.** They are blocked on `check_expression` inside the
  body and belong to whatever gaps there.
- **The 206 `async`/generator lines.** The answers are `Promise<T>` and
  `Generator<…>`; this port builds neither, so converting them would need
  `create_type_reference` to carry members (`bd tsr-4sc.7`).
- **The 155 reachability lines.** `void` against `never` is the flow graph, and
  `flow.rs` is another workstream's file.
- **`bd tsr-4sc.9`, at 9 lines.** Left open, resized, and not worth a union
  implementation at that size.
- **The 39 residual widening-gate lines.** A parameter annotation that gapped,
  which is a type-node row.
