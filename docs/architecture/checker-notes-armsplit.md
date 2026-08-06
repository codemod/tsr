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
