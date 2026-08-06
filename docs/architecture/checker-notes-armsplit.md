# Which arm gapped — the three rows the board ranked first, measured

Instrument: `crates/tsr-conformance/examples/armsplit.rs` and
`crates/tsr-conformance/examples/namesample.rs`, both new. Measured at
`6e3c549`, which is the tree `STATUS.md` §1's numbers were taken at.

This page exists because `STATUS.md` §4 ranked four items 1–4 on the strength of
figures produced by `gaproot.rs`, and **three of the four are not what the
figures say they are.** In every case the error is the same shape and it is not
an arithmetic error: a number that is true of a population, quoted as though it
described a *change someone could make*.

---

## 1. The framing the board inherited, and why reading the code broke it first

`STATUS.md` §4 rows 1–3 are `BinaryExpression` (1,418), `NewExpression` (1,049)
and `ArrayLiteralExpression` (637), each marked **"never measured, unowned"**,
and the handover describes the family as *"the form's own rule is simply
missing"*.

That is `gaproot.rs`'s `ROOT/own-rule` label, and the label is accurate about
what it measures. It is **one cell for "the form's own rule did not fire"**, and
a form with eleven arms lands its entire population in that one cell. It cannot
distinguish *no rule exists* from *ten rules exist and the eleventh is withheld
on purpose*.

Opening the two files says which of those it is, before any probe runs:

| file | ported | deliberately withheld |
|---|---|---|
| `binary.rs` | assignment, arithmetic/bitwise/shift, `+`, relational, equality, `in`, `instanceof`, comma | the **logical** operators (`bd tsr-5s2`), destructuring assignment |
| `array_literals.rs` | the whole non-tuple tail of `checkArrayLiteral` | spreads, omissions, a gapped element, two object-typed constituents, tuples |

**Neither row is a missing rule. Each is a named set of withheld arms**, every
one of which already carries a written reason. "Unowned" was wrong too: both
files have an owner in their module docs and a `bd` id against each gap.

> **The rule this buys.** A classifier's bucket is named for the *question the
> classifier asked*, not for the work behind it. Before ranking a row out of a
> histogram, open the file that owns it — a row labelled "no rule" that turns
> out to be "one arm of eleven, withheld for a reason on record" is a different
> item at a different price, and the check costs one `Read`.

---

## 2. Own root, pinned by the operands rather than by a span

`gaproot` decides own-root by descending a node's span and asking whether an
inner rendered line gapped. For these three forms that is both unnecessary and
weaker: **a binary expression's roots are exactly its two operands.** So
`armsplit` asks the checker directly, via `types_producer::type_at_location`, for
the rendered type of `binary.left` and `binary.right`, of a `new` expression's
callee and arguments, and of each array-literal element. If one renders `error`,
the root is that operand.

That is the *construction*-pinned form `docs/conventions.md` prefers: the value
is fixed by what the operands are, and no span predicate can invert without the
operand strings changing with it.

### The controls, registered before the run

- **C2.** Plain assignment with a non-pattern left-hand side cannot be an
  own-root gap — `binary.rs` returns the right operand's type unconditionally.
  Expect 0. **Read 0.**
- **C3.** The comparison family answers `intrinsics.boolean` *without consulting
  the operands*. So no comparison operator can appear in the gap population under
  any own-root verdict. Expect 0 in every bucket. **Read 0.**
- **C4, frozen.** Against `STATUS.md` §4's published 1,418 / 1,049 / 637, which
  `gaproot` produced at `3299f53` and which no mutation of `armsplit` can move.
  Read **1,708 / 1,052 / 604**. `NewExpression` agrees to 3 lines and
  `ArrayLiteral` to 33; `BinaryExpression` disagrees by 290, and the disagreement
  is the expected direction — `gaproot`'s `OwnRule` additionally requires that
  *some* inner rendered line exist, so a binary expression over two leaf operands
  falls into its `UNMATCHED` bucket instead. Neither number is wrong; they answer
  different questions and this page quotes its own.

---

## 3. `BinaryExpression` — 1,708 own-root lines in seven arms

TS2563 excluded throughout (0 lines here, printed as the control).

| arm | lines | cases | top-1 | what it needs |
|---|---:|---:|---:|---|
| logical `&&` | **659** | 95 | 15.6% | `getDefinitelyFalsyPartOfType` + one facts bit |
| logical `\|\|` | 358 | 107 | 27.9% | `removeDefinitelyFalsyTypes` + **subtype reduction** |
| addition fallthrough | 297 | 68 | 9.1% | nothing — **277 (93.3%) want `any`** |
| destructuring assignment | 252 | 68 | 14.3% | destructuring patterns **and** tuples |
| logical `??` | 96 | 20 | 21.9% | `getNonNullableType` + subtype reduction |
| arithmetic | 43 | 2 | **97.7%** | nothing — 42 of 43 want `any` |
| logical assignment | 3 | 2 | 66.7% | the above, plus `checkAssignmentOperator` |

**340 of the 1,708 want `any`** and are not work: they are the two places
`binary.rs` deliberately answers `errorType` where upstream reports a diagnostic
and answers `any` (`check_addition`'s tail, and mixing `bigint`). ADR-0038 and
ADR-0039 both forbid closing them, and the `arithmetic` row is additionally
97.7% one case.

**252 more** are destructuring assignment, which needs the pattern *and* the
tuple machinery — two unported subsystems, and the wants confirm it
(`readonly [9, 0]`, `[number, string, boolean]`).

That leaves **1,113 logical-operator lines**, and they are not one item.

### 3.1 `&&` is separable from `||` and `??`, and `binary.rs`'s own comment says otherwise

`binary.rs:126` states:

> The logical operators build a union of the operands … `&&` unions
> `extractDefinitelyFalsyTypes(left)` with the right type
> (`checker.go:12495`), and `extractDefinitelyFalsyTypes` reaches
> `getTypeFacts` (`checker.go:30982`), a large table this port does not have.
> `||` and `??` additionally reduce with `UnionReductionSubtype` …

**The middle clause is wrong.** Taken with `grep -n` on the declarations:

```
func (c *Checker) extractDefinitelyFalsyTypes   checker.go:29110
func (c *Checker) removeDefinitelyFalsyTypes    checker.go:29106
func getDefinitelyFalsyPartOfType               checker.go:29114
```

`extractDefinitelyFalsyTypes` is `mapType(t, getDefinitelyFalsyPartOfType)`, and
`getDefinitelyFalsyPartOfType` is **a pure switch on `TypeFlags` that consults no
facts table at all** — `String → ""`, `Number → 0`, `BigInt → 0n`, and the
definitely-falsy values (`false`, `void`, `undefined`, `null`, any/unknown, the
empty string literal, the zero literals) map to themselves. Everything else is
`never`. It is `removeDefinitelyFalsyTypes` — the `||` arm — that calls
`hasTypeFacts(t, TypeFactsTruthy)`.

The `&&` arm does need **one** facts bit, as a gate (`checker.go:12497`):

```go
resultType := leftType
if c.hasTypeFacts(leftType, TypeFactsTruthy) {
    t := leftType
    if !c.strictNullChecks { t = c.getBaseTypeOfLiteralType(rightType) }
    resultType = c.getUnionType([]*Type{c.extractDefinitelyFalsyTypes(t), rightType})
}
```

and `TypeFactsTruthy` for one type is the complement of the definitely-falsy set
the switch above already enumerates — the same predicate read the other way, not
the thirty-bit table. The `!strictNullChecks` branch is dead here: this crate
assumes `strictNullChecks` **on** throughout (`array_literals.rs`,
`crate::unions`). And the union is plain `getUnionType`, *not* the
`UnionReductionSubtype` that `||` and `??` need and that this port cannot do
without assignability.

So the separation is real and it runs where the comment says it does not:

| arm | subtype reduction | facts table |
|---|---|---|
| `&&` | **no** | one bit, computable from `TypeFlags` |
| `\|\|` | **yes** | `Truthy`, via `filterType` |
| `??` | **yes** | `EQUndefinedOrNull`, via `getTypeWithFacts` |

`docs/conventions.md` records the mirror of this under *"the easy leg of three
may be a leg that cannot be reached without the others"* — the `TemplateExpression`
row, where the cheap leg turned out to be inseparable. Here the check comes back
the other way, and it came back that way only because the *declarations* were
grepped rather than the comment believed. **A prerequisite recorded in a doc
comment is a hypothesis about upstream, and it is checked the same way a
prerequisite quoted in a handover is.**

### 3.2 What the `&&` row wants

```
  218  (boolean)      && (boolean)   ->  boolean
   56  (any)          && (true)      ->  any
   13  (number)       && (number[])  ->  0 | number[]
    9  (any)          && (any)       ->  any
    7  (() => boolean)&& (boolean)   ->  boolean
    5  (10)           && (number)    ->  number
    5  (boolean)      && (any)       ->  any
```

Every one of those is reproduced by the rule above. `boolean` is `false | true`;
mapping the falsy part gives `false | never` = `false`; unioning with `boolean`
gives `boolean`. `number && number[]` gives `0 | number[]` — which is exactly the
baseline, and is the row that makes it clear this is a real rule rather than
"answer the right operand": the naive designs score **377 == right** and
**401 == left** out of 659, and `left | right` scores **0**.

---

## 4. `new C()` — 1,052 own-root lines, and the row nobody has sized alone

449 cases, **top-1 4.1%** — the least concentrated large row on the board.
Only **144 of 1,052 (13.7%) want `any`**, against 59.3% on element access and
93.3% on the addition fallthrough.

The cheapest conceivable design — *the callee types `typeof C`, so strip the
`typeof`* — exact-matches **23 of 1,052, 2.2%**, and on **712 of 1,052 the callee
is not `typeof X` at all.** The callee's own type says why, and splits the row
into two arms:

| the callee's type | lines (top 10 of 192 distinct) | what upstream reads |
|---|---:|---|
| `ErrorConstructor` | 106 | the construct signature's **declared return type** |
| `DateConstructor` | 95 | " |
| `ArrayConstructor` | 32 | " |
| `ObjectConstructor` | 27 | " |
| `SetConstructor` | 25 | " |
| `MapConstructor` | 24 | " |
| `typeof C` / `typeof D` / `typeof Foo` | 56 / 45 / 30 | the class's declared instance type |
| `any` | 54 | `any` — forbidden here |

**309 lines in the top ten alone are lib `*Constructor` interfaces**, where the
answer is a written return-type annotation on a construct signature — no argument
inference, no overload resolution, no contextual typing. That is a materially
cheaper mechanism than the general call path, and it has never been costed
because `new` has only ever been folded into the call row.

**This is the strongest available argument for re-taking the call row's
spellability figure.** That refusal stands at **68.3% against a 70% bar and
survives by 85 lines** (`docs/conventions.md`, *"report the margin when a
threshold is close"*). The `new` row was inside the 18,294-line population but
its right-hand sides were never scored separately, and they are visibly more
nameable than the call row's. Re-measuring the bar with `new` scored on its own is
one probe. `bd tsr-td1`.

---

## 5. `ArrayLiteralExpression` — 604 own-root lines, 58.8% of them one case

| arm | lines | own root | note |
|---|---:|---|---|
| union / object reduction / global `Array` | 602 | yes | **355 are `conformance/generatedContextualTyping` wanting `(Derived1 \| Derived2)[]`** |
| empty and still a gap | 2 | yes | `compiler/noCrashOnNoLib` |
| an element gapped | 314 | **no** | the root is the element |
| spread element | 190 | no | needs `isArrayLikeType` and the iterated type |
| omitted element | 32 | no | needs the tuple element-flags model |

The 355 are `array_literals.rs`'s documented `object_constituent_count > 1`
guard, which exists precisely because upstream reduces the element union with
`UnionReductionSubtype` and this port has no assignability. **There is no
available work in this row until assignability lands**, and what there is, is one
case. `bd tsr-rn4`.

---

## 6. `tsr-jle` — 11,008 lines is 1,004, and 90.9% of the difference is one file

`bd tsr-jle` reports 11,008 wrong lines (29.19% of the wrong bucket) whose
failure `wrongflip.rs` classifies as *naming* rather than *typing*, and records
the caveat that blocks quoting it: the classifier's `is_nominal`/`is_structural`
are syntactic tests on the printed string, so 11,008 is an upper bound and *"the
first thing to do is take a sample of ~50 and classify by hand."*

`namesample.rs` does that step frequency-weighted instead — a 50-line sample from
a long tail describes the tail, and a planner needs the head. Its two arms are
copied character for character from `wrongflip.rs`, so the population is the same
one the 11,008 came from.

```
naming population 11,004 = 10,462 theirs-named/ours-structural + 542 the reverse
  spread over 363 cases; the largest five:
   10,000  compiler/largeControlFlowGraph
       47  conformance/unknownControlFlow
       21  conformance/mappedTypeAsClauses
       15  compiler/declarationEmitGlobalThisPreserved
       15  compiler/narrowingByTypeofInSwitch

THEIRS NAMED, OURS STRUCTURAL — 10,462 lines in 239 distinct pairs
  10,001   2 cases   any  ->  any[]    e.g. `data` in compiler/largeControlFlowGraph
      21  11 cases   object -> {}
      20  13 cases   any  ->  () => void
```

**One pair is 10,001 of 11,004 lines — 90.9% — and 10,000 of them are
`compiler/largeControlFlowGraph`.** That case is ADR-0038's ceiling: under
`TS2563` upstream disables control-flow analysis and prints `errorType` as
**`any`**, and this port answers the auto-array type `any[]`. It is not a naming
failure at all. `is_nominal("any")` is true and `is_structural("any[]")` is true,
and both are meaningless about a line where upstream declined to compute
anything.

Outside that file the item is **1,004 lines over 362 cases in 566 distinct
pairs**, and the largest coherent pair is **21 lines** (`object` → `{}`, which is
a printer difference, not a naming one). The second-largest case holds 47.

**There is no row here.** Board item 4 comes off. `bd tsr-q54`.

> The corollary is the one `docs/conventions.md` already records under *"a
> denominator that contains an unreachable population makes a rate
> uninterpretable"* — except that here it did not merely make the rate
> uninterpretable, it inverted the ranking. 11,008 put this item first among
> unmeasured work; 1,004 in 566 pieces puts it nowhere. The classifier was never
> wrong; it was answering *"do these two strings differ in nominal-versus-
> structural shape"*, which is a true statement about 11,008 lines and a claim
> about none of them.

---

## 7. What this leaves, as a registration rather than a conclusion

Of the 3,101 own-root lines the board's top three rows contain:

```
  340   want `any`               ADR-0038 / ADR-0039 — never work
  252   destructuring            blocked on tuples and patterns
  355   subtype reduction        blocked on assignability, and one case
  358   logical ||               blocked on assignability
   96   logical ??               blocked on assignability
1,052   new C()                  construct signatures; folds into the call item
  659   logical &&               NOT BLOCKED — §3.1
```

**`&&` is the only unblocked arm in the three rows the board ranked first**, and
`bd tsr-jle`, the fourth, is gone. That is the finding of this page, and the
ranking in `STATUS.md` §4 is rewritten from it.

### The rule registered before the `&&` build, at `6e3c549` — `bd tsr-rmi`

The change cannot make a currently-right line wrong *in its own row*: today
`check_binary_expression` returns `errorType` for `&&` unconditionally, so every
line in the row is already a gap and the row can only gain. **The whole risk is
the cascade** — 478 further gap lines are blocked by an `&&` operand and will
start receiving answers, along with whatever is downstream of those.

So the bar goes where the risk is, which is not the target row:

> **KEEP** if the corpus counterfactual shows **gained ÷ lost ≥ 3.0** and the
> **net is ≥ +400 lines** and **fewer cases regress than finish**.
> **REVERT** otherwise.

Two of those are deliberately not what previous items were judged on. The
gained/lost ratio is the inverse of the *wrong-per-right* figure that refused
`tsr-6ph` at 2.1 and 2.5 and qualified naming at 2.7, so ≥ 3.0 is a bar those
three would each have failed. The net floor is set at +400 against a 659-line
target row because §3.2's rule reproduces the head of the row exactly and the
cascade should add rather than subtract — **if the net comes in under 400 the
premise of §3.1 is wrong, not the arithmetic.**

**How this would be shown wrong:** if `casedelta` shows the gain concentrated in
one or two cases, the row was never the diffuse 95-case population measured here
and the `&&` rule is not what converted it.

### And the result, scored against that rule at `5290e1a`

```
  87 cases moved | gained 87 cases +958 lines | lost 0 cases -0 lines
  top gainer 212 lines = 22.1% of the gain
  gradient 63.34% -> 63.54%   cases 2,270 -> 2,275
```

Every other suite byte-identical. The falsifier did not fire: 22.1% top-1 over
87 cases is the diffuse population §3 measured, not one case.

Three things to say honestly about the number rather than around it:

1. **The ratio leg is vacuous, not passed.** Nothing was lost, so
   *gained ÷ lost* has no denominator. That was foreseeable and should have
   been foreseen when the rule was written: the row can only gain, because the
   arm returned `errorType` unconditionally, and the only way to lose was a
   cascade that turned someone else's right line wrong. It did not, and the
   evidence for that is the 0, not the ratio. **A bar whose denominator the
   change cannot produce is not a bar** — the net floor and the case-regression
   count are what actually did the work here.
2. **The row was 659 and the conversion is 958 — a 1.45× cascade**, against a
   registered floor of +400 that was set as a floor and not a point estimate.
   Say which ratio: 1.45× is against the *predicted row*, which is the planner's
   number (`docs/conventions.md`, *"two ratios, both true"*).
3. Re-running `armsplit` afterwards is the control that says the rule is what
   converted them: the `logical &&` own-root bucket is **gone entirely**, and
   `logical ||` rose **358 → 366** as lines it had been blocking became their
   own root. Total `BinaryExpression` aligned gap fell 7,559 → 6,812, i.e. 747,
   against a gradient gain of 958 — so **211 lines of the gain are outside the
   `BinaryExpression` row altogether.**

### What the tests found and the corpus could not

The corpus says a change is good on net. It cannot say a fixture is wrong,
and two things came out of writing `crates/tsr-checker/tests/logical_and.rs`
that no corpus run would have surfaced:

- **Three of seven expectations were written from intuition and were wrong**,
  in a file whose own header says every expected string must come from a real
  baseline first. The port was right in each case: `"a" | 0` not `0 | "a"`,
  `number | ""` not `"" | number`. `CompareTypes` (`utilities.go:415`) sorts by
  *increasing flag value*, and the baselines say so outright —
  `>x && y : 0 | false`, `>-a.x && b.y() : void | ""`,
  `>authToken && { authToken } : "" | { authToken: string; }`. The rule the
  header states is the one that caught it, applied late. **A convention you
  quote in a file header and then do not follow inside it is worth exactly
  nothing**, and the tell was that the failing assertions all disagreed about
  *order* rather than about *content* — a shape a type error does not have.
- **`undefined | null` prints as `null | undefined`**, and does so for the bare
  declared type with no `&&` anywhere, while `string | number` comes out right.
  Two-line repro in `bd tsr-iiu`. It is pre-existing and it is **invisible to
  every gap histogram on this project**, because it produces a *wrong* line
  rather than a missing one, in a family `strictNullChecks` makes common. That
  is the second time this cycle a real defect was found sitting in the wrong
  bucket, and both times the bucket was the one nobody ranks.

---

## 8. Board item 2, run: the call row's bar with `new` scored separately

`bd tsr-4tw`, registered at `d75cf16` before the split was computed. Measured
at that tree with `examples/callres.rs`, extended so the per-row breakdown sits
**beside** the aggregate rather than replacing it — the published figure has to
stay reproducible from the expression that produced it, or the split becomes an
unfalsifiable re-derivation of a number nobody can check.

### First, a correction: 68.3% is stale and the margin was never 85 lines

`STATUS.md` §5 and `docs/conventions.md` both quote R2′'s corpus-vocabulary leg
as **68.3%**, refusing the call row against a 70% bar and *"surviving by 85
lines"*. Re-run unchanged at `d75cf16` the same expression reads
**3,745 of 5,398 = 69.4%**, and the margin is **34 lines**.

Neither number is wrong. The 68.3% was taken at `058b4a9` when the gradient was
61.09%; it is now 63.54%. **R2′'s numerator moves with the compiler**, because
it asks what this port renders anywhere in the corpus — so every unrelated
checker commit changes it. That is a property of the instrument, not a defect,
and it has one consequence for planning: **a bar this instrument crosses by tens
of lines is not deciding anything**, because a session of work elsewhere moves
it by more than the margin.

### The split

| half | in corpus vocabulary | vs the 70% bar |
|---|---:|---:|
| **CALL** (`InitCall` + `ExprCall`) | 3,003 / 4,253 = **70.6%** | **+25 lines** |
| — of which `InitCall` | 824 / 1,399 = 58.9% | −156 |
| — of which `ExprCall` | 2,179 / 2,854 = **76.3%** | +181 |
| **NEW** (`InitNew`) | 742 / 1,145 = 64.8% | −60 |
| `ExprNew`, **never scored before** | 826 / 1,160 = **71.2%** | +14 |
| the aggregate, for comparison | 3,745 / 5,398 = 69.4% | −34 |

**The aggregate was `new` holding the call half under the bar.** Scored alone
the call half clears at 70.6%, and the refusal's stated grounds do not hold for
it.

`ExprNew` deserves a line of its own: it was excluded from the admitted
population entirely by `row.assigned()`, so the 1,052-line own-root `new` row
§4 measured **had never been scored by this instrument at all**. Nobody hid it;
it was filed as a "companion" row four cycles ago and the exclusion was never
revisited.

### And the reason not to act on that

**Every level of this split separates a high half from a low half.** The
aggregate at 69.4% is 70.6% and 64.8%. The call half at 70.6% is **58.9% and
76.3%**. There is no level at which "the call row" is one population, and the
next split would find another.

That cuts against the split just performed exactly as hard as it cuts against
the aggregate. A figure that moves from 69.4% to 76.3% depending on where the
line is drawn is not measuring a property of call resolution; it is measuring
how the drawer chose. The bar was registered at 70% against *the row*, and the
row has turned out not to exist.

> **A rule registered against a population is void when the population turns out
> to be a mixture, and re-registering it against one half is the error the
> element-access refusal already names** — *"a population cannot be sliced by the
> answer the baseline expects"*. This slice is by **row**, not by expected
> answer, so it is not that error outright; but the reason it feels licensed is
> the same, and the honest verdict is that R2′ has stopped discriminating.

`callres.rs`'s standing registration settles what follows regardless of the
number: **R2′ is necessary and not sufficient, and no value of it licenses a
build.** So the outcome of board item 2 is *not* "build call resolution". It is:

- the refusal **no longer stands on its stated grounds** for the call half, and
- what it needs instead is a **counterfactual**, which for call resolution is
  the expensive thing R2′ existed to avoid paying for.

### Board item 1 survives its own test, and it is the only clean positive

Within the `new` rows, split by **the callee's own type** — a property the
implementation can test, unlike the baseline's right-hand side:

| the callee | in vocabulary | cases | top-1 |
|---|---:|---:|---:|
| a `*Constructor` interface | 815 / 1,074 = **75.9%** | **229** | 15.1% |
| every other callee | 753 / 1,231 = 61.2% | 271 | 8.7% |

It clears the bar by 63 lines, clears the 300-line denominator floor registered
with the rule, and clears concentration outright at 229 cases and 15.1% top-1
(R3's bar is a top-10 share ≥ 68%).

What those lines want is two clean families and one that is not:

```
   129  Date                          109  Error
    50  Uint8Array<ArrayBuffer>        41  Float32Array<ArrayBuffer>
    40  Float64Array / Int16Array / Int32Array / Int8Array / Uint16Array …
    43  Set<number>                    <-- never rendered anywhere in the corpus
```

`Date` and `Error` are the arm as advertised: a written construct-signature
return type, no inference, no overload resolution. **The typed arrays are not** —
they are generic instantiations, which is `bd tsr-4qx`, and `tsr-4qx` is blocked
on `tsr-awa`. `typedArraysCrossAssignability01` alone supplies the 15.1%.

So the sizing to carry forward is **not 1,074**. It is the `*Constructor`
population minus the generic instantiations, and that has not been measured.
Splitting it is one more bucket in a probe that already runs.

### What was registered, and what happened

| registered | outcome |
|---|---|
| NEW ≥ 70% and CALL < 70% → the mixture was hiding a separable `new` item | **did not happen** — the reverse |
| NEW < CALL → "folding `new` in was **helping** the call row's number" | **the direction was written backwards.** `NEW < CALL` means folding it in *lowered* the mixture. The branch fired and its stated consequence was wrong |
| both < 70% → item 1 argued on a sub-row | did not happen |
| any half ≥ 70% on < 300 lines licenses nothing | did not fire; every half is over 1,000 lines |

The second row is mine and it is the interesting failure. **Writing a rule down
in advance does not make it correct — it makes it checkable**, and this one was
checked by the arithmetic rather than by rereading it. The registration
discipline still did its job: the branch it named fired, the number it predicted
was there, and the sentence attached to the branch was visibly false the moment
the two figures sat next to each other. A rule registered in advance and
silently reinterpreted afterwards would have been worse than no rule; a rule
registered in advance and **contradicted in writing** costs one paragraph.

### And a process failure worth more than the result

The previous section of this page reports the `&&` build's gates as green. They
were not. `cargo test --workspace` was run and its output piped through
`head -30`, which cut it off at the 30th of 96 test binaries, and
`the_logical_operators_are_still_a_gap` — a test whose entire purpose is to go
red when `&&` lands — was below the cut. **`5290e1a` shipped with a failing
test and this page said the gates were green.**

Fixed at the head of this section's commit: the test now asserts `1 && 2` is
`2`, and `||` and `??` still `error`, with the reason they parted company. The
count is now taken with `grep -c` over the whole run rather than read off a
truncated head.

> **A gate you sampled is not a gate you ran.** The failure mode is specific and
> cheap to close: any command whose *whole* output is the evidence must be
> reduced by counting, not by `head`. `grep -cE "^test result: ok"` and
> `grep -c FAILED` are two lines and cannot hide the thirty-first binary.

It is the same shape as the two errors already recorded on this page — a shape
test standing in for a match test, a header rule quoted and not followed. In all
three the instrument was fine and the *reading* of it was truncated.

---

## 9. `removeSubtypes`, sized — and refused

`bd tsr-eak`, sized by `crates/tsr-conformance/examples/subtypes.rs` (new) at
`0a1fbdd`. The registered instruction was to size it by **counting unions whose
answer would change**, never by summing the five rows it blocks, because those
rows are in four files and do not ship together.

### Reading upstream cut the population before any probe ran

`removeSubtypes` (`checker.go:25934`) only admits a constituent as a removal
*source* when it is `StructuredOrInstantiable` (`:25955`), and upstream says why
on the line above: redundant primitives are assumed already gone, so the only
possible supertype of a primitive is an empty object type. **A union of pure
primitives reduces identically under `UnionReductionLiteral` — which this port
has — and `UnionReductionSubtype`.** Of 26,140 union lines this port answers,
21,093 carry no structured constituent at all and are outside the item by
construction.

### The number that refuses it

| | lines | the proxy says would change |
|---|---:|---:|
| structured constituent, **RIGHT today** | **2,901** | **255** |
| structured constituent, wrong today | 2,146 | 263 |
| no structured constituent, right | 19,467 | **0** (C1) |
| no structured constituent, wrong | 1,626 | **0** (C1) |

The proxy is `Relation::Assignable`, which is *weaker* than upstream's
`strictSubtypeRelation` and therefore over-removes, so both columns are upper
bounds.

**255 lines that are right today would be broken, against at most 263 wrong
lines changed** — and *changed* is not *fixed*. That is **1.03 gained per lost at
the theoretical ceiling** and below break-even on any realistic reading. The
refusals on record are `tsr-6ph` at 2.1 and 2.5 wrong-per-right and qualified
naming at 2.7; this is worse than all three.

### And only a quarter of the target row is even this item

The decisive step was refusing to treat "the baseline is shorter than our union"
as the item's population. Partitioned by **why** the 2,146 structured wrong lines
are wrong, on the two strings alone:

| family | lines | share |
|---|---:|---:|
| a strict subset survives — **`removeSubtypes` candidate** | **500** | 23.3% |
| narrowing: a nullable was not stripped | 483 | 22.5% |
| not a union on one side — a different answer entirely | 437 | 20.4% |
| **printer: parenthesisation** | 301 | 14.0% |
| neither: the constituents themselves differ | 288 | 13.4% |
| **printer: constituent order** | 137 | 6.4% |

**500, not 2,146** — and 500 is itself an upper bound on an upper bound, because
the arm admits any case where the baseline's constituents are a strict subset of
ours, which includes narrowing. `Set<number> | Set<string>` → `Set<number>` (24
lines) is in that arm and `Set<string>` is not a subtype of `Set<number>`.

So the item is: **at most 500 candidates, of which the proxy says at most 263
would move, against 255 certain losses.** Refused.

### Two items fell out of it, and they are better than the item was

- **`bd tsr-dto` — 438 lines of pure printer work with no prerequisite.**
  Parenthesisation (301): we print `A & B | C & D` where upstream prints
  `(A & B) | (C & D)`, and `() => boolean | undefined` where upstream prints
  `(() => boolean) | undefined`. Constituent order (137): `number[] | string[]`
  against `string[] | number[]`. **This supersedes `bd tsr-iiu`'s sizing**, which
  called the ordering defect a two-line repro of unknown size; it is 137 lines
  for order alone and `undefined | null` is one instance of it.
- **`bd tsr-e10` — 483 lines wanting a nullable stripped.** `T | undefined` → `T`.
  That is `getNonNullableType`/narrowing, and `removeSubtypes` would leave every
  one of them alone: `undefined` is not a subtype of `T`. It is the largest
  single family and it was sitting inside the population this item was about to
  be sized by.

Both carry the same caution that refused the parent, and it must not be lost in
the enthusiasm: **a change to union rendering fires on all 26,140 union lines, of
which 22,368 are right today.** The bar belongs on that population, not on the
438. What makes them tractable where `removeSubtypes` is not is that they are
deterministic string rules, so the counterfactual is one build and one
`casedelta` rather than a relation nobody has ported.

### C1 fired, and it fired against a sentence I had written as upstream's

C1 was registered as *"a union with fewer than **two** structured constituents
cannot change; expect 0"*, and read **61**.

The gate at `:25955` is **per source**, and the target loop at `:25984` ranges
over every other constituent whatever its flags. So `T extends string` inside
`T | string` is a structured source removed against a *primitive* target — one
structured constituent, and upstream's own comment three lines below is about
exactly that case. The correct partition is **one or more**, and on it C1 reads
**0**.

The error is the same one this page records at §3.1 against `binary.rs`: read
upstream, infer a rule, write the inference down as though it were upstream's.
One cycle later, and this time with a control on it — which is the whole
difference. Had C1 been written as *"the buckets sum to the population"* it would
have passed, and the item's population would have been understated by the entire
one-structured bucket: **2,639 lines**, more than the item's whole candidate set.

> **Pin a control to the upstream construct you are claiming, not to your
> summary of it.** The claim here was a sentence about a gate; the control that
> caught it was the same sentence turned into a bucket that had to read zero. An
> arithmetic control over the same partition cannot see a wrong partition.
