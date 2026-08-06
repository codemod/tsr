# What actually gaps the receiver

> **CORRECTED 2026-08-06. §8's "not explained" is now explained, and finding it
> found a defect in this probe's classifier. Every number below has been
> re-measured with the defect fixed.**
>
> **The defect.** This probe asked `type_string == "error"` *before* it asked
> whether the baseline matched, so a line where we answer `error` **and upstream's
> baseline also says `error`** was counted as a gap. It is a right answer. There
> are **389** of them corpus-wide — programs that declare a type named `error`,
> and some do. `rank_board`, `wrong_attribution` and `types_shapes` all test the
> baseline first; this file was the only one that did not, and the direction is
> the unflattering one: it moved correct answers into the column this project
> reads as *"not built yet"*.
>
> **The 0.09pp.** Not the defect, and not commit drift — the compiler is
> byte-identical at `7602d6b` and `058b4a9`, since the only checker commit between
> them (`3baeb70`) was reverted by `a618e3a`. It is one thing and one thing only:
> **the suite counts upstream's baseline lines and this probe counts the lines we
> rendered.** `types_suite::compare` opens with
> `assertion_count(expected)` and iterates the baseline; this probe iterates
> `our_file`. Measured by `crates/tsr-conformance/examples/reconcile.rs`, the
> bridge is exactly `479,060 − 964 surplus + 858 deficit = 478,954`, difference
> **0**. The two hypotheses that sounded better — files dropped by the id-length
> guard, cases that fail to load — both measure **0 files and 0 cases**. They
> never fire.
>
> **What changed, re-measured at `058b4a9`:**
>
> | | as published | corrected |
> |---|---:|---:|
> | gradient right | 292,217 (61.00%) | **292,606 (61.08%)** |
> | gradient gap | 143,509 | **143,120** |
> | gradient wrong | 43,334 | 43,334 |
> | the two rows, total | 19,329 | **19,318** |
> | `property access` row | 9,677 | **9,666** |
> | `member name` row | 9,652 | 9,652 |
> | module-object forms | 1,618 (8.4%) | **1,617 (8.4%)** |
> | everything else | 17,711 (91.6%) | **17,701 (91.6%)** |
> | `import * as ns` | 344 | **343** |
> | other import alias | 162 | **161** |
> | local declaration whose type gaps | 10,112 | **10,110** |
> | the name does not resolve | 3,619 | **3,616** |
> | the receiver is not a name | 3,792 | **3,788** |
>
> **Nothing on this page changes sign, and no share moves by more than 0.1pp** —
> every figure here is a share of the two rows, whose denominator this probe
> computes itself, which is exactly why the shares survived a denominator defect
> that the one absolute in §8 did not. That is the rule this cost, and it is now
> in `docs/conventions.md`: *a probe that quotes a gradient must reconcile its
> denominator against `assertion_count`, or quote shares of its own population and
> say so.*
>
> All five controls still hold at the corrected numbers: C1 0, C2 0, C3 0 with
> mirror **343**, A1 **19,318 == 19,318**. See `bd tsr-zlo`.

Measured at `7602d6b` and **re-measured at `058b4a9` on 2026-08-06** (see the
correction above; the compiler is identical at both, only this probe's classifier
changed) by `crates/tsr-conformance/examples/receiver_gap.rs`,
which routes through `types_producer::assertions_for_case_with_ids` and so
measures the same lib-loaded compiler the `checker_types` gradient does.
Upstream anchors below are at the pinned submodule commit `5b1047d10`, each
taken from `grep -n` on the declaration.

Reproduce:

```
cargo run --release -p tsr-conformance --example receiver_gap
```

## 0. The claim this page was written to test

`bd tsr-6ph` says a type for the module object is

> **the only route to** the 1,590 lines in the two receiver-is-a-gap rows
> (`member name` 736 + 60, `property access` 734 + 60), because `ns.foo` needs
> the namespace's type and not the alias's target.

Two separate questions hide in that sentence, and they have different answers.

## 1. The rows are 19,318 lines, not 1,590

`types_producer::access_reason` emits `the receiver is a gap: {kind}` and is
reachable from exactly two call sites — the member name of an `a.b`
(`types_producer.rs:1095`) and the access expression itself (`:1150`). Counting
every gap line that carries the phrase:

| row | lines | cases | top-1 | top-10 |
|---|---:|---:|---:|---:|
| `member name, the receiver is a gap` | 9,652 | 1,298 | 12.7% | 52.9% |
| `property access, the receiver is a gap` | 9,666 | 1,302 | 12.6% | 52.8% |
| **total** | **19,318** | | | |

Top case in both: `compiler/temporal` at 1,222 lines, then
`conformance/parserRealSource11` 1,146 and `parserRealSource10` 797.

**1,590 is not the size of these rows.** It is `module_blocked`'s *seam-only*
subset of them — the lines whose blockers are **all** the cross-file alias seam.
The full rows are 12.2× larger. Anyone reading `tsr-6ph`'s sentence as "these
rows are 1,590 lines and modules are the only route to them" has the population
wrong by an order of magnitude, and this page exists so that reading stops.

The two rows being within 0.15% of each other is not a coincidence and is worth
carrying: `a.b` renders **two** assertion lines, the access and its member name,
and both take the same receiver. So a converted access converts two lines. That
pairing is structural, and it is the reason a single-line-per-access estimate of
this work would be half of what it should be.

## 2. Walking each line to the gap that actually stops it

For each such line the probe takes the access, takes its receiver, and asks
`gap_reason` **about the receiver** — descending again while the measured answer
is itself `the receiver is a gap` (`a.b.c`). The descent is driven by the
measured reason at every step, never assumed. Chain depth:

| depth | 0 (`x.foo`) | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---:|---:|---:|---:|---:|---:|---:|
| lines | 17,059 | 1,951 | 258 | 28 | 12 | 8 | 2 |

88.3% are one hop. The terminal receiver is then classified by what declares it.

| what gaps the receiver | member | access | lines | share | cases | top-10 |
|---|---:|---:|---:|---:|---:|---:|
| `import * as ns` | 172 | 171 | 343 | 1.8% | 81 | 50.7% |
| `import a = …` | 618 | 618 | 1,236 | 6.4% | 156 | 58.1% |
| `export * as ns` | 19 | 19 | 38 | 0.2% | 12 | 89.5% |
| other import alias (an export symbol, not a module object) | 81 | 80 | 161 | 0.8% | 52 | 39.8% |
| local `namespace N {}` (same file) | 13 | 13 | 26 | 0.1% | 9 | 100.0% |
| a local declaration whose own type gaps | 5,050 | 5,060 | 10,110 | **52.3%** | 541 | 64.7% |
| the name does not resolve | 1,808 | 1,808 | 3,616 | 18.7% | 110 | 87.9% |
| the receiver is not a name (call, `this`, `super`, …) | 1,891 | 1,897 | 3,788 | 19.6% | 562 | 22.5% |
| **total** | | | **19,318** | | | |

**Module-object forms root 1,617 lines — 8.4%. Everything else is 17,701, 91.6%.**

### Controls, printed unconditionally

Three are pinned by construction, which is what lets them see a semantic
inversion that leaves every sum intact (`docs/conventions.md`).

| control | reads | pinned by |
|---|---:|---|
| C1 the phrase under a third row prefix | **0** | `access_reason` has two call sites |
| C2 terminal reason still says receiver-is-a-gap | **0** | the descent only leaves a `PropertyAccessExpression`, and no other kind reaches `access_reason` |
| C3 `import * as ns` root with no module specifier | **0** | the grammar: a namespace import is a clause of an `ImportDeclaration` |
| C3′ the same roots **with** one (the mirror) | 343 | — |
| A1 rows == partition | 19,318 == 19,318, difference 0 | arithmetic |

C3 and C3′ are printed as a pair on purpose: a classifier that had collapsed the
import forms together would still give C3 = 0, and only the mirror distinguishes
"the arm is right" from "the arm never fires".

## 3. So does the "only route" claim survive?

**As a claim about the 1,590 seam-only lines: yes, and it is now corroborated by
a second instrument.** `module_blocked` reached 1,590 by following each gap
line's blockers to their leaves; this probe reached **1,617** by walking the
receiver chain to its root declaration and reading that declaration's kind.
Different code paths, different authors, **agreement to 1.7%**. That is the
strongest evidence form available here — not a control reading zero, but two
things that could have disagreed and did not.

A third, independent cross-check falls out of the terminal-reason histogram:
`reference, symbol has no type: SymbolFlags(ALIAS) / no value declaration` holds
**1,770** lines, against 1,778 for the four alias buckets summed
(343 + 1,236 + 38 + 161). The 8-line difference is symbols carrying `ALIAS`
alongside another declaration.

**As a claim about the rows: no.** "The only route to the receiver-is-a-gap rows"
is true of 8.4% of them. The other 91.6% are blocked on things a module-object
type cannot touch, and the largest single blocker is not module-shaped at all.

The correction is small and it matters: `tsr-6ph`'s **number** is sound, its
**scope sentence** is not. Fix the sentence, keep the number.

## 4. What the other 17,711 are actually waiting on

From the terminal `gap_reason` histogram, the top blockers behind the
non-module 91.6%:

| terminal reason | lines | cases | top-10 |
|---|---:|---:|---:|
| `reference, the name does not resolve` | 3,616 | 110 | 87.9% |
| `BLOCK_SCOPED_VARIABLE / VariableDeclaration / initialiser CallExpression` | 1,124 | 44 | 90.2% |
| `SymbolFlags(EXPORT_VALUE) / no value declaration` | 1,120 | 44 | 93.2% |
| `expression answered error: CallExpression` | 791 | 144 | 44.0% |
| `expression answered error: ThisKeyword` | 670 | 108 | 44.5% |
| `FUNCTION_SCOPED_VARIABLE / VariableDeclaration / initialiser NewExpression` | 526 | 60 | 68.4% |
| `property access, the property has no type` | 489 | 40 | 80.6% |
| `expression answered error: SuperKeyword` | 410 | 73 | 45.9% |
| `Parameter / annotation TypeReference unresolved: …` | 678 + 450 + 392 + 280 | 1–5 each | 100% |

Read as work items rather than as rows, that is roughly:

- **call and `new` resolution** — the `initialiser CallExpression` and
  `initialiser NewExpression` families, plus the bare
  `expression answered error: CallExpression`, together ≈ 3,200 lines;
- **`this` and `super`** — 1,080 lines, and both are *positions* this port does
  not type at all rather than lookups that failed;
- **unresolved annotations in a handful of very large cases** — the
  `TypeReference unresolved: Emitter / TypeCollectionContext / ControlFlowContext
  / TypeFlow` rows are each one or five cases at 100% top-10 concentration, all
  of them `parserRealSource*`, which is one hand-written compiler source file
  checked in as a test.

That last bullet is the concentration warning this project keeps needing.
`compiler/temporal` alone contributes 1,374 of the 3,616 `name does not resolve`
lines (38%), all of them `typeof Temporal`, and the whole `name does not resolve`
bucket is 87.9% top-10. **These are not 3,616 independent gaps; they are a
handful of files.** A row this concentrated converts as a step function, not
proportionally, and should be sized by naming the case rather than by quoting the
line count.

## 5. Can we spell the answers? Where the ceiling actually lands

`bd tsr-4jk` measured that upstream prints the **local alias** for a namespace
import — `>ns : typeof ns` — never the module, and a module symbol's name in this
port is the stripped file path, so a naive implementation prints `typeof /0`: a
gap converted into a *wrong* line. The probe prints the baseline's own
right-hand side for each terminal receiver, and it reproduces that finding
exactly. For `import * as ns` roots: `typeof PropTypes` 48, `typeof stuff` 36,
`typeof React` 28, `typeof keys` 24 — always the local name. For `import a = …`:
`typeof exporter` 240, **`any` 162**, `typeof React` 110.

**But that ceiling binds a different line than these rows.** The receiver's own
assertion line is in the *alias* rows, and it is the one that must print
`typeof ns`. The lines counted here print the **member's** type — `ns.foo` is
rendered as whatever `foo` is — and upstream reaches it through
`getPropertyOfType` (`checker.go:18887`) and `getTypeOfPropertyOfType`
(`checker.go:18951`) on the module object produced by
`resolveExternalModuleSymbol` (`checker.go:15556`), from
`checkPropertyAccessExpression` (`checker.go:11244`). The module object has to be
*constructed and looked up in*; it never has to be *printed*.

So the two halves come apart, and this is the load-bearing consequence for
`tsr-6ph`: **a module-object type that cannot be spelled can still convert these
1,617 lines**, because they never render it. Only the receiver line does. If
`tsr-4jk`'s spelling problem turns out to be unsolvable, the right move is to
build the type, use it for lookup, and leave the receiver line gapping — which
converts 1,617 lines and manufactures **zero** wrong ones. That option was not
visible while the two rows were being treated as one item.

The 162 `any` receivers under `import a = …` are the opposite warning: upstream
itself answers `any` there, so those lines are already answerable and are not
evidence for the module-object type.

## 6. What was deliberately not built

No arm was added to `members.rs` or `indexed.rs`. Nothing in this page implies
one: the module-object half belongs to `tsr-6ph` and the agent settling
`symbols.rs`/`resolution.rs`, and the 91.6% majority is call resolution, `this`
and `super`, and annotation resolution — none of which live in these two files.

In particular the `any`-receiver arm in `members.rs` (upstream-faithful,
`checker.go:11318`, landed at a measured +555 wrong lines against 894 gaps
closed) was **not** touched, and neither was its identity-vs-flag guard —
`object_type == self.intrinsics.any` by identity rather than `TypeFlags::ANY`,
because `errorType` also carries `ANY`. See
`docs/architecture/checker-notes-arrays.md`. Note from this measurement what
that guard is protecting: 143,509 gap lines are in the gradient, and a flag test
would answer `any` for every receiver among them.

## 7. How you would know this page is wrong

- **The partition is a list, not a partition.** A1 would print a non-zero
  difference. It prints 0 over 19,329.
- **The descent stops early**, attributing an outer access to its inner one's
  cause. C2 would be non-zero; it is 0, and the depth histogram would collapse to
  depth 0 rather than showing a 1,952 / 258 / 28 / 12 / 8 / 2 tail.
- **The two instruments agree by construction rather than by measurement.**
  They do not share code: `module_blocked` walks blockers through reason strings
  and line positions, this probe walks the receiver chain through the AST and
  reads `Symbol::declarations`. If the 1.8% became 0.0% after a refactor that
  made one call the other, the corroboration in §3 would have to be withdrawn.
- **§5's claim that these lines never print the module object.** The falsifier is
  a baseline line under one of these rows whose right-hand side is `typeof <a
  module>`; the probe already prints the receiver's RHS per bucket and would show
  it.

## 8. Reconciliation — settled 2026-08-06

**The version of this section published on 2026-08-05 said the difference was
"not explained" and asked someone to reconcile it. This is that reconciliation.**
It is kept in full rather than replaced, because what it got right is as useful
as what it got wrong: it refused to quote the absolute, and that refusal is the
only reason the defect underneath it stayed contained to one number.

> This probe re-derives the gradient as **479,060 lines = 292,217 right (61.00%)
> \+ 143,509 gap (29.96%) + 43,334 wrong (9.05%)**, against the committed
> `checker_types` snapshot's **61.09%**. The 0.09pp difference is **not
> explained**. It is small and it does not move any share on this page — every
> figure here is a share of the two rows, whose denominator this probe computes
> itself — but the absolute right-count above should not be quoted as the
> gradient's until someone reconciles it.

There were **two** differences hiding in that one paragraph, and they have
nothing to do with each other. The instrument is
`crates/tsr-conformance/examples/reconcile.rs`, which runs both bridges and
prints four controls.

### The denominator: 106 lines, and it is a unit mismatch

`types_suite::compare` (`crates/tsr-conformance/src/types_suite.rs:98`) opens
with `let total = types_baseline::assertion_count(expected)` and then iterates
`expected_file.assertions`. **Every number the suite produces is over upstream's
baseline lines.** This probe iterates `our_file` — the lines *we rendered*. Where
we render an assertion upstream does not, our denominator grows and the suite's
does not; where upstream asserts and we render nothing, the suite counts a line
we never see.

```
  probe total (rendered lines)                        479,060
  - surplus: we rendered more than upstream asserts       964
  + deficit: upstream asserts more than we rendered       858
  + baseline lines in 0 no-section files                    0
  + baseline lines in 0 id-guard files                      0
  + baseline lines in 0 cases that did not load             0
  = suite total                                       478,954   (actual: 478,954)
```

**The two hypotheses that sounded better were both wrong, and cheaply.** The
`if our_file.len() != line_ids.len() { continue; }` guard and the
`case.load().ok()?` early return each drop a whole file or a whole case from this
probe's denominator while the suite keeps every baseline line in it — a real
divergence, correctly identified, that fires on **0 files and 0 cases**. Writing
them down and measuring them cost one bucket each; arguing about them would have
cost more and settled nothing.

### The numerator: 389 lines, and it is a defect in this probe

Not accounting — a bug, in the direction that flatters us.

```rust
// before
if assertion.type_string == "error" { report.gap += 1 }
else if baseline matches            { report.right += 1 }
else                                { report.wrong += 1 }
```

A line where we answer `error` **and upstream's baseline also says `error`** is a
right answer, and this arm filed it under `gap`. There are 389 corpus-wide;
programs may declare a type named `error`, and some do. `rank_board`,
`wrong_attribution` and `types_shapes` all test the baseline first — this file
was the only one that did not. Fixed in place, with the ordering and its reason
in a comment beside it, because the next reader's instinct will be to test the
cheap condition first.

### Controls

Four, and only the first two are worth anything on their own.

| control | reads | pinned by | fires under |
|---|---:|---|---|
| C1 probe right − `compare` matched, over counted files | **0** | **construction**: the two are the same predicate at the same positions, so they are one number reached two ways whatever the lengths are | M1, at **−276,213** |
| C2 rendered lines under a no-section file | **0** | **construction**: the arm *is* `ours.get(i).is_none()` | — |
| C3 reconstructed suite total − actual | **0** | arithmetic | M2, at **212** |
| C4 suite matched − (probe right + matched-in-dropped) | **0** | arithmetic | M1 |

Both mutations were run, and each moved exactly one control and left the other at
zero, which is the whole point of keeping both kinds:

- **M1 — shift the probe's positional test by one** (`get(position + 1)`).
  C1 **−276,213**, C3 **0**. An alignment error is invisible to every sum.
- **M2 — invert the surplus/deficit polarity**, the exact failure
  `docs/conventions.md` records passing every roll-up elsewhere in this repo.
  C3 **212**, C1 **0**.

### The rule this leaves

**A probe that quotes a gradient must reconcile its denominator against
`assertion_count`, or quote shares of its own population and say so.** This page
did the second thing correctly for every figure on it and then quoted one
absolute in the last paragraph. That is the entire failure, and it cost 389 lines
of misfiled gap plus a `.md` file's worth of numbers that had to be re-taken.

`bd tsr-zlo`.
