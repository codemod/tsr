# The gap, ranked by root instead of by symptom

Status: measured 2026-08-06 at **`8a38b2c`**, over the 9,538-case `.types`
population, from one pinned binary in an isolated worktree
(`CARGO_TARGET_DIR` not shared — `checker-notes-guard.md` records losing a
measurement to a shared target dir). The instrument is
`crates/tsr-conformance/examples/gaproot.rs`, added in the same commit as this
file. Upstream references are to `vendor/typescript-go` @ `5b1047d10`.

**Unit, restated on every table.** Every count is an **assertion line** in a
`.types` baseline unless the table's header says `cases` or `steps`. The board
has twice turned a node count into a line count by omission
(`checker-notes-rank.md`), so no table here omits it.

**Denominator.** `expected` is 478,954 upstream baseline lines — the population
`types_suite::compare` takes from `assertion_count` — and this probe reproduces
the gradient **exactly**: `292,606 right / 138,585 gap / 37,709 wrong / 10,054
unaligned`. Every share below is over the **138,585 gap lines** unless it says
otherwise. `examples/reconcile.rs` was run at this commit and confirms the
bridge.

---

## Why this page exists

`checker-notes-rank.md`'s corrected `cause()` splits every gap line by **one
step** of evidence: TERMINAL 16.41%, propagated/span 24.77%, propagated/named
13.79%, DEPENDENT-UNKNOWN 33.06%, UNMATCHED 11.97%. So **row-by-row work
addresses 16% of the gap** and the other 84% is a line waiting on something
else, with no ranking saying what.

This page walks the wait. For every gap line it follows the dependency the
*measured* reason names — re-asking `types_producer::gap_reason` at every step,
never assuming — until it reaches a node with no gap under it, and files the
line under **that** node's row. The number beside a row is the one nobody had:
**if this root were fixed, how many gap lines stop being gaps?**

It generalises `examples/receiver_gap.rs`, which walks one propagation form.
There are five edges here, and the middle one is new:

| edge | from | to | why it is sound |
|---|---|---|---|
| `member-of-access` | the `b` of `a.b` | the access `a.b` | `gap_reason` types the member name **as the access** and says so in a comment on the arm that does it (`types_producer.rs`). The two baseline lines are one failure rendered twice. |
| `receiver` | `a.b` | `a` | `access_reason` measured `a`'s type as `error`. `receiver_gap.rs`'s edge. |
| `span` | a node | the outermost-leftmost rendered line strictly inside its span that gapped | `rank_board`'s `Below`, **both** fields. |
| `initialiser` | a declaration name / reference | the value declaration's initialiser | `gap_reason` prints `/ initialiser K` only when `getTypeOfSymbol` took that half — upstream `getTypeForVariableLikeDeclaration` (`checker.go:16652`, taken from `grep -n` on the declaration) reads the annotation when there is one and the initialiser otherwise. **This is the edge `checker-notes-rank.md` §9 says nobody had followed.** |
| *(stop)* `annotation` | a declaration name / reference | the annotation type node | Filed as a root of its own kind. Not descended — see the limit. |

### The limit, stated before any number

**A type node is where this instrument stops.** `/ annotation TypeReference
unresolved: Array` names a type node and the line is filed under it. This probe
does *not* ask whether that type node is itself downstream of another gap — a
`TypeReference` naming a type alias whose right-hand side gaps would be. So
every `ROOT/type-node` row — **24.85% of the gap** — is a **ceiling for the
type-node family, not a proof of rootness**. `bd tsr-4gq`.

---

## The pre-registered rule — and a deviation I have to declare first

> **This rule was written after the first two corpus runs, not before them.**
> That is a deviation from the assignment and from `docs/conventions.md`, and it
> is recorded rather than hidden: a rule written with the ranking already in
> view can be fitted to the ranking. What limits the damage is that every
> threshold below is lifted from a number this project had already published —
> the 6,610-line measured conversion (`checker-notes-rank.md` §8), the 40%
> concentration veto (§3), the 25% naming bar (`checker-notes-wrong.md` R3) —
> and none was chosen by looking at this table. It is stated as a caveat that
> **blocks** a claim rather than decorating one: nothing on this page should be
> read as "the rule was satisfied by an instrument that could not have been
> tuned to it".

> **RULE.** The measurement licenses filing a build only if a **single root
> row** satisfies all three:
>
> - **R1 — size.** The row blocks **≥ 5,000 gap lines**. The largest single
>   conversion this project has measured is 6,610 lines
>   (`checker-notes-rank.md` §8); a root worth a workstream must be of that
>   order.
> - **R2 — concentration.** Largest single case **≤ 40%** of the row's blocked
>   lines, **and** top-10 **≤ 90%**. Three of the top gradient rows evaporated
>   on the first leg (§3); the second leg exists because a row that is 97.8% ten
>   cases is one library file.
> - **R3 — spellability.** **< 25%** of the blocked lines have a baseline
>   right-hand side this port has no route to naming (`typeof …`, `import(…)` —
>   the two families `docs/conventions.md` measured as unnameable here),
>   **and ≤ 25%** have a baseline right-hand side of `any`, because
>   `checker-notes-rank.md` §6 forbids closing any row by widening an `any`.
>
> **The direct bucket, and it is not a proxy.** The question is *"if this root
> were fixed, how many gap lines stop being gaps?"*; the `lines` column **is**
> that count. R2 and R3 are guards on its credibility, not substitutes for it.
>
> **Second rule, for the second question.** *"Is the remaining gap reachable by
> finishing a list of rows?"* — direct bucket: the cumulative share of the gap
> covered by the top **20** root rows. **≥ 50% ⇒ a list is a viable mechanism;
> < 50% ⇒ it is not.** 20 is the size of list a workstream can plausibly finish.
>
> No code is written here either way: this workstream owns no checker file. A
> row that clears the rule is filed as a sized `bd` issue.

---

## RESULTS

*(Nothing above this line was edited after the run.)*

### The verdict, first

**The rule fired, for exactly two rows, and they are the same file.**

| row | lines blocked | own | cases | top-1 | top-10 | unnameable | `any` | R1 | R2 | R3 |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|
| `property access, the receiver has no such property` | **13,206** | 4,030 | 1,078 | **3.7%** | 23.5% | 0.5% | 11.0% | PASS | PASS | PASS |
| `property access, the property has no type` | **6,612** | 1,861 | 308 | 14.6% | 51.3% | 0.3% | 10.4% | PASS | PASS | PASS |

Unit: **gap assertion lines**; `cases` is cases. Both are
`crates/tsr-checker/src/members.rs` — property lookup on a receiver whose type we
already have. Together they block **19,818 lines, 14.30% of the gap**, and they
are the *only* two rows on the whole board that clear all three conditions.

The second rule also fired: the top 20 root rows cover **72.43%** of the gap.
That answer needs the qualification in §"Is it a list?" below and does not
survive it intact.

### Finding 1 — half the gap has a root that is not the line

| where the walk ends | lines | share of the gap |
|---|---:|---:|
| `ROOT/own-rule` — the node has typed children and none gapped | 60,268 | **43.49%** |
| `ROOT/type-node` — an annotation or an alias RHS (a **ceiling**) | 34,440 | 24.85% |
| `UNMATCHED` — **the control**; no arm claimed it | 19,522 | 14.09% |
| `ROOT/name-unresolved` | 13,427 | 9.69% |
| `ROOT/no-value-decl` | 10,210 | 7.37% |
| `DEPTH-BOUND` — 64 steps without terminating | 492 | 0.36% |
| `ROOT/symbol-has-type` — the chain reached a node that does not gap | 186 | 0.13% |
| `CYCLE` — the walk returned to a node it had visited | 40 | 0.03% |

Unit: **gap assertion lines**, 138,585 of them.

**50.35% of gap lines root somewhere other than themselves.** The board's
TERMINAL bucket is 16.41%; the `ROOT/own-rule` bucket here is 43.49%, and the
difference — 27 points, ~37,500 lines — is entirely lines the board could
classify only as *propagated* or *unknown* and this walk carries to a named
node. That is the whole point of the page: those lines now have an address.

### Finding 2 — the largest gradient row on the board is not work, for a new reason

`checker-notes-rank.md` §3 ranks `expression answered error:
ElementAccessExpression` first at 11,553 lines and discounts it to ~1,553 on
concentration (86.6% in `compiler/largeControlFlowGraph`). Walked to root it is
**13,163 lines**, still 76.0% one case — and the new column kills it outright:

```
  upstream's own RHS for the lines it blocks:
      any 11,639 | string 294 | number 263 | boolean 74 | () => string 56
```

**88.4% of the lines this root blocks are lines where upstream answers `any`.**
Closing it means answering `any` on 11,639 lines, which is precisely the failure
`checker-notes-rank.md` §6 forbids: *"no row on this board should be closed by
widening an `any` answer"*. It fails R2 **and** R3, and the R3 failure is the
one that would have been invisible without the baseline's right-hand side.

The same check disqualifies the row that ranks **first** by size:

| row | lines | top-1 | `any` share of blocked lines | verdict |
|---|---:|---:|---:|---|
| `reference, the name does not resolve` | 13,357 | 29.3% | **43.3%** | R3 FAIL |
| `annotation TypeReference unresolved` | 7,660 | 37.5% | **62.7%**, top-10 **97.8%** | R2 and R3 FAIL |

Three of the five rows that clear R1 are killed by the spellability column, and
in each case by the `any` leg rather than the naming leg. `docs/conventions.md`
records this check killing a 692-line item and a 1,617-line one; here it kills
**34,180 lines** of apparent work. `bd tsr-phd`, which also records what would
bring them back: nobody has separated an `any` upstream **computes** from one it
defaults to, and if these are the former the leg is too strict.

### Finding 3 — the twin rows are one root, and it is 3.3× its own size

`checker-notes-rank.md`'s correction header notes that `member name, the receiver
has no such property` (4,016) reads `UNMATCHED` while its twin `property access,
the receiver has no such property` (4,005) reads `propagated/span`, and suggests
*"where a member-name row reads UNMATCHED, its access twin's label is the better
estimate"* — explicitly flagged there as an inference, not a measurement.

**It is now measured, and it is stronger than the inference.** The two are not
two rows to be read against each other; they are **one node**, because
`gap_reason` types the member name as the access. Walked:

```
  property access, the receiver has no such property   13,206 lines blocked
                                                        4,030 of them are itself
                                                        3.3x cascade
  property access, the property has no type             6,612 lines blocked
                                                        1,861 of them are itself
                                                        3.6x cascade
```

Unit: gap assertion lines. The board's four rows (4,016 + 4,005 + 1,861 + 1,860
= 11,742) are two roots blocking **19,818**. This is
`docs/conventions.md`'s cascade rule measured on the gap side: **a row counts the
lines that name a defect, not the lines downstream of them**, and the factor is
1.7× here against the 1.66× measured on the alias arm.

### Finding 4 — the initialiser edge, followed for the first time

`checker-notes-rank.md` §9 lists the 37,278 (now 45,814) `DEPENDENT-UNKNOWN`
lines as *"the largest open measurement"*, and says following them needs the
initialiser's `NodeId`, which `gap_reason` formats away into a string. It does
not: the symbol is re-resolvable from the node the same way `gap_reason`
resolved it (`symbol_of` for a declaration name, `resolve_name` in
`SymbolFlags::VALUE` for a reference), and the value declaration's
`initializer_id()` is one call away. **34,776 initialiser steps were taken.**

Where the 45,814 `DEPENDENT-UNKNOWN` lines end up is the answer to §9's
prediction. `checker-notes-rank.md` §9 predicted *"mostly dependent — I predict
the opposite of TERMINAL"*. Measured, the bucket splits three ways: the
annotation half stops at a **type node** (24.85% of the gap overall, a ceiling),
the initialiser half is **carried through to an expression root**, and the
`no value declaration` / `does not resolve` halves are **roots in the binder**
(7.37% + 9.69%). **The prediction is upheld in direction** — almost none of it
is the row's own work — and the destinations are now named.

### The depth histogram

Unit: **gap assertion lines**. This is the shape that says the walk walked.

| depth | lines | share |
|---:|---:|---:|
| 0 | 68,804 | 49.65% |
| 1 | 32,702 | 23.60% |
| 2 | 19,553 | 14.11% |
| 3 | 7,277 | 5.25% |
| 4 | 3,880 | 2.80% |
| 5 | 2,068 | 1.49% |
| 6 | 1,540 | 1.11% |
| 7 | 514 | 0.37% |
| 8–62 | ~1,740 | 1.26% |
| **63 (the bound)** | **505** | **0.36%** |

`receiver_gap`'s histogram over one edge is `17,059 / 1,951 / 258 / 28 / 12 / 8 /
2`; this one over five edges reaches depth 63 and **stops there by construction**
— 492 lines are filed `DEPTH-BOUND` and 40 `CYCLE`, both printed as their own
buckets rather than folded into a root. The deep tail is real and is
concentrated in deeply nested sources (`compiler/parsingDeepParenthensizedExpression`,
`compiler/deeplyDependentLargeArrayMutation2`); a bound of 64 truncates 0.36% of
the gap and the truncation is visible, not silent.

### The branching factor — what this attribution does not credit

Unit: **span steps** (176,593 of them), not lines.

| top-level gapped children | steps | share |
|---:|---:|---:|
| 1 | 115,825 | 65.57% |
| 2 | 58,004 | 32.84% |
| 3 | 1,530 | 0.87% |
| 4–8 | 1,234 | 0.70% |
| >8 | 54 | 0.03% |

**Attribution is to the first (outermost-leftmost) top-level gapped child**, and
the choice is stated because it is a choice. Two alternatives were considered:
attributing to *all* of them double-counts (the same line would be credited to
several roots and the shares would not sum), and attributing to the *deepest*
requires descending every branch and picking, which is a different question —
*"what is the deepest thing under this line"* rather than *"what is this line
waiting on"*. The first-child rule keeps the partition exact (control A1 = 0)
and **undercounts every root that is not first**: 34.4% of span steps had more
than one independent gapped child, so roots reachable only as a second operand
are systematically under-credited here. That is the largest known error on this
page and it runs in the *conservative* direction for a build decision.

### Is it a list? — cumulative coverage

Unit: **gap assertion lines**, over 138,585, across **305 distinct root rows**.

| top-N root rows | lines | share of the gap |
|---:|---:|---:|
| 1 | 13,357 | 9.64% |
| 3 | 39,726 | 28.67% |
| 5 | 53,998 | 38.96% |
| **10** | **76,504** | **55.20%** |
| **20** | **100,372** | **72.43%** |
| 30 | 113,106 | 81.61% |
| 50 | 126,411 | 91.22% |
| 100 | 135,939 | 98.09% |

The second rule's threshold (≥50% at 20 rows) is cleared with room. **But the
raw answer overstates the case and the qualification is the finding**: of the top
20 rows, three fail R3 on the `any` leg (34,180 lines), 34,440 lines corpus-wide
are `ROOT/type-node` ceilings rather than proven roots, and 19,522 are
`UNMATCHED` — the control — where the walk has no evidence at all. **The list
exists; roughly half of what is on it is not work.** The sentence that answers
the question is in the report at the end of this page.

### The lib names `row_key` cuts out

`annotation TypeReference unresolved` is one row of 7,660 blocked lines. It is
not one work item — it is a set of names, and the names say which:

```
  1,402 Emitter | 922 AST | 801 TypeCollectionContext | 782 ControlFlowContext
    534 TypeFlow | 469 TypeSymbol | 455 Type | 379 ParseNode | 208 TokenSpan
```

Unit: gap assertion lines. **These are not lib types.** They are declarations in
the corpus's own multi-file cases (`conformance/parserRealSource*`, 2,875 +
2,287 + 823 lines in the top three cases) — a cross-file *type*-space resolution
item, not a `getTypeFromTypeNode` item. A row named after a node kind is again
not about that node kind.

---

## Controls, printed unconditionally, with what pins each

| control | reads | pinned by | what makes it non-zero |
|---|---:|---|---|
| **C1** a stop whose reason still names a dependency | **0** | **construction** — every such reason has a descent arm, so no classification stop can carry one | deleting a descent edge (M2, proven) |
| C1′ stops naming none (mirror) | 118,531 | — | pins C1 against being zeroed by disabling the test |
| **C2** `ROOT/own-rule` on a node with nothing inside its span | **0** | **construction** — `has_inner` **is** the arm's positive test | making `own-rule` the default arm (M3, proven) |
| C2′ `ROOT/own-rule` with something inside it (mirror) | 60,268 | — | mirror |
| **C3** a `member name,` line whose parent is not a property access | **0** | **construction** — `gap_reason` emits that prefix *only* from that parent | the row matcher drifting from the producer |
| C3′ `member name,` lines whose parent is one (mirror) | 15,432 | — | mirror |
| **C4** a line the board calls TERMINAL that walks deeper | **0** | **construction** — identical predicates, so the walk cannot move | a divergence between the descent filter and the board predicate |
| C4′ board-TERMINAL lines stopping at depth 0 (mirror) | 22,739 | — | mirror; **equals `rank_board`'s published TERMINAL exactly** |
| **C7** re-measured board split − `rank_board`'s published totals | **0 / 0** | **another instrument** | a polarity inversion of the span test (M1, proven) |
| C5 descent steps landing on an unrendered node | 1,150 | — | **a measurement, not a zero**: no span evidence there, the reason alone decided |
| C6 span steps that skipped an access's own member name | 63,394 | — | **a population, not a violation** |
| A1 roots attributed − gap total | 0 | arithmetic | a lost or double-counted line |
| A2 right+gap+wrong − aligned | 0 | arithmetic | a lost line |
| A3 aligned+unaligned − expected | 0 | arithmetic | a lost line |

**C4′ reading exactly 22,739 is the strongest line on this page.** `rank_board`
publishes 22,739 TERMINAL at `b5decc5` from a different code path; this probe
recomputes it, and `DEPENDENT-UNKNOWN` (45,814) lands on the digit too. Two
instruments that could disagree and do not.

### C7 exists because M1 proved C1–C4 could not see the error they were for

The first control set had no cross-instrument row. Under M1 — the span test's
polarity inverted, which is the exact defect `docs/conventions.md` records an
agent shipping in another probe — **C1, C2, C3, C4 and A1 all still read zero**
while 46,269 lines moved between buckets. C4 is fed by the same `inner_of` the
descent uses, so inverting both sides moved them together and the difference
stayed 0. That is a control that looks construction-pinned and is not: it pins
*agreement between two consumers of one function*, not the function. C7 was
added afterwards, and it fires at **−17,329**.

Recorded rather than quietly fixed, because the defect class — a control whose
two sides share the code under test — is not in `docs/conventions.md` yet and
reads exactly like the construction-pinned kind it recommends.

---

## The named mutations, each run on the whole corpus

| mutation | what it does | control that fired | before → after |
|---|---|---|---|
| **M1** | inverts the span test's polarity (`== "error"` → `!=`) | **C7** | 0 → **−17,329**; C1–C4 and A1 all **still 0**; `ROOT/own-rule` 60,268 → 8,262, `ROOT/symbol-has-type` 186 → 57,398 |
| **M2** | deletes the `member-of-access` descent edge | **C1** | 0 → **15,432**; `UNMATCHED` 19,522 → 33,971; C7 and A1 still 0 |
| **M3** | drops `has_inner` from `ROOT/own-rule`, making it the default arm | **`check_classifier`** | pass → **panics**: *"a leaf with no named dependency reaches the control, not a ROOT"* |
| **M4** | truncates the descent at depth 2 | **none** | `DEPTH-BOUND` 492 → **37,079 (26.76%)**; every control still 0 |
| **M5** | drops the `PropertyAccessExpression` clause from `is_member_name_of` | **none** | C6 63,394 → 63,653; ~450 lines change bucket; every control still 0 |

**M1 is the inversion that leaves every sum intact**, and it is reported first
because it also indicts the control set that existed when it was run (above).

**M4 is reported because no control caught it.** A truncated descent is visible
only in the `DEPTH-BOUND` bucket, which is printed but is not a must-be-zero;
there is no construction-pinned control for "the walk stopped early", because
any bound is a choice and 0.36% of the corpus legitimately reaches 64. Anyone
reading this page should treat the depth histogram, not a control, as the
evidence that the descent descends.

**M5 moved something and no control saw it.** The clause is therefore
*load-bearing but unguarded*: 259 span steps and roughly 450 lines change bucket
without it. It is not inert — so it may be cited as doing work — but it is not
protected either, and that is worth knowing before someone simplifies it away.

---

## What is *not* measured here, and is filed

- **Type-node roots are ceilings, not proofs.** 34,440 lines (24.85%) stop at an
  annotation or an alias right-hand side because descending into type space
  needs declared-type machinery `gap_reason` does not expose. `bd tsr-4gq`.
- **Second and later operands are not credited.** 34.4% of span steps had more
  than one independent gapped child and only the first was followed. Every root
  reachable only as a non-first operand is under-counted by an unmeasured
  amount. `bd tsr-qgk`. `open`.
- **`finishes` is not computed on this page.** It is a *case* statistic and this
  page is a *line* statistic; `checker-notes-rank.md` §8 and
  `checker-notes-wrong.md` both show the two rankings disagree, so no case claim
  should be read off these rows. `open`.
- **The two rows the rule fires for are ceilings, like every row population.**
  13,206 and 6,612 are *upper bounds on what closing them converts*, not
  predictions. `docs/conventions.md` records five rows collapsing on contact.
- **A 54-line disagreement with `rank_board`, explained but not verified.** This
  probe builds its checker with `Checker::with_module_host` (what `render_case`
  itself uses, ADR-0041); `rank_board` uses `Checker::new`. `propagated/named`
  reads 19,058 here against the board's 19,112, with the 54 landing in
  `propagated/span` (+27) and `UNMATCHED` (+27). The likely mechanism is 54
  receivers that resolve through the module host and stop reporting *the
  receiver is a gap*. Not confirmed. `open`.

## Everything filed from this page

| id | what | sized as |
|---|---|---|
| `tsr-pnf` | `property access, the receiver has no such property` — the row the rule fires for | **13,206 gap lines unblocked** (4,030 of them its own) |
| `tsr-mcd` | `property access, the property has no type` — the second | **6,612 gap lines unblocked** (1,861 its own) |
| `tsr-4gq` | type-node roots are ceilings, not proofs | 34,440 lines of the ranking are affected |
| `tsr-qgk` | only the first gapped operand is credited | 34.4% of 176,593 span steps had a choice |
| `tsr-phd` | the `any` leg disqualifies apparent work, and may be too strict | 34,180 lines held out |

Both build items are sized in **lines they unblock**, not lines they contain,
and both numbers are ceilings.

## Superseded numbers

None yet. When one on this page is corrected, it gets a dated header here rather
than a silent edit (`CLAUDE.md`).

## Reproducing this

```bash
git worktree add --detach /tmp/gaproot 8a38b2c
cd /tmp/gaproot && git submodule update --init --recursive
CARGO_TARGET_DIR=/tmp/gaproot/target cargo run --release -p tsr-conformance --example gaproot
CARGO_TARGET_DIR=/tmp/gaproot/target cargo run --release -p tsr-conformance --example reconcile
```

`gaproot` takes about 30 seconds after the build. **Take it in a worktree, not in
the shared checkout** (`checker-notes-guard.md`).
