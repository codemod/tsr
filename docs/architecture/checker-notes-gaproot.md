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

---

# Part 2 — the build decision for the two `members.rs` roots

## RULE-2, re-registered 2026-08-06 at `b9a4f5c`

Written **before** any new bucket was measured, before `members.rs` was opened,
and before a line of checker was written. It exists because the rule in Part 1
was written after two corpus runs and is therefore not a pre-registration; the
coordinator's instruction was to write the rule I would have written blind and
then check it. Nothing in this section was edited after the run that follows.

**The population, stated in the rule rather than assumed.** The **19,818 gap
assertion lines** that `gaproot` at `73f6dcd` attributes to the two roots
`property access, the receiver has no such property` (13,206) and
`property access, the property has no type` (6,612), out of the suite's
**138,585** gap lines over **478,954** upstream baseline lines. Not nodes, not
cases, not `.errors.txt` diagnostics.

> **RULE-2.** Build a row only if it clears **all three**. A row that fails any
> one is refused, and the refusal is the deliverable.
>
> - **B1 — the prerequisite is met in fact, not by label.** ≥ **60%** of the
>   lines a row blocks must have a **receiver whose rendered type equals the
>   baseline's type for that receiver**. `gaproot` calls these roots
>   `ROOT/own-rule` because nothing *inside the access's span* gapped — but a
>   receiver that is typed *wrongly* also does not gap, and a property lookup
>   that fails on the wrong receiver type is not `members.rs` work. This is the
>   direct form of the coordinator's *"prerequisite met is necessary and not
>   sufficient"*: an item sized at 1,784 converted 362, a 4.9× miss.
>   **Why 60%:** below it, a further haircut of that order leaves the row under
>   the +0.34 gradient bar the last cycle's best work set.
>
> - **B2 — the root is a root.** ≤ **25%** of a row's blocked lines may dissolve
>   when the descent is given a **property-declaration edge** — descend from
>   `the property has no type` to the property symbol's own declaration.
>   `gaproot`'s span test cannot see that declaration, which is the exact blind
>   spot that made a 2,618-line row read TERMINAL and measure 68.6% propagated
>   (`checker-notes-rank.md`, correction header). **I expect this to fail for
>   `the property has no type` and to hold for `the receiver has no such
>   property`**, and I am writing that prediction down before measuring it so
>   the result can falsify me rather than confirm me.
>
> - **B3 — the collateral is spellable.** On a before/after corpus pair through
>   `examples/casedelta.rs`, **wrong-per-right ≤ 0.5**, counted over the whole
>   corpus and not over the targeted rows. Three builds were refused this week
>   at 2.1, 2.5 and 1.0; the one that landed came in at 0.14. A **net** gradient
>   figure does not satisfy B3 — it is the one statistic guaranteed to hide a
>   change that helps and harms at once.
>
> **And, independently:** no row may be closed by widening an `any` answer, no
> line of `indexed.rs` may be touched (the coordinator sequences that), and the
> `any`-receiver identity guard in `members.rs` is not to be simplified.
>
> **Both counts are ceilings on ceilings.** 19,818 is *lines unblocked*, which
> already assumes the row is closed completely in every case it touches, and B1
> is the measurement of how much of that assumption survives contact.

## RESULTS — Part 2

*(Nothing above this line was edited after the run.)*

**Re-baselined at `b9a4f5c`.** Part 1's figures are at `8a38b2c` and stand as a
dated measurement. Between the two, other workstreams moved the checker: the
gradient is **295,302 right / 135,635 gap / 37,963 wrong** here, against Part 1's
292,606 / 138,585 / 37,709. Every Part 2 number is at `b9a4f5c` or later and
says so. `rank_board` re-run in the same worktree reproduces this probe's
one-step split to the digit (TERMINAL 22,764, DEPENDENT-UNKNOWN 44,342), with
the known 54-line `propagated/named` difference from the module host unchanged.

### The verdict, first

| row | RULE-2 | outcome |
|---|---|---|
| `property access, the property has no type` | **B2 FAIL** | **refused** — 96.8% of it dissolves |
| `property access, the receiver has no such property` | B1 PASS, B2 PASS, B3 PASS | **built, in part** — 976 lines converted |

**And the headline correction is against my own Part 1 numbers: the 19,818 lines
I filed as two build items are 976 lines of work in the file I own — a 20×
overstatement.** Both `bd` issues have been updated in place rather than left
to be read as sized.

### B2 — one of the two rows was a symptom, and I predicted which

`gaproot` gained a **property-declaration edge**: for `property access, the
property has no type`, `get_property_of_type` *found* the member and
`get_type_of_symbol` answered `error` for it, so the thing that gapped is the
property's own declaration — somewhere else in the program entirely, which is
why nothing inside the access's span gapped and the probe called the access its
own root. Measured both ways at `b9a4f5c`, unit **gap assertion lines**:

| row | edge off | edge on | dissolved |
|---|---:|---:|---:|
| `property access, the property has no type` | **10,012** | **319** | **96.8%** |
| `property access, the receiver has no such property` | 13,156 | 13,156 | **0.0%** |

RULE-2's B2 allows ≤25%. The first row fails by a factor of four and is
**refused**. The second is unchanged by the edge, which is the same fact from the
other side: a property that was never found has no declaration to be downstream
of.

**The prediction registered before the run was that B2 would fail for the first
and hold for the second.** It did. That is the one thing on this page a
post-hoc rule could not have delivered, and it is why RULE-2 was written with a
prediction in it.

The 319 survivors are all `BROKEN-DESCENT` — a property whose symbol has no
named value declaration — not roots. The whole row is gone from the ranking.

### B1 — the prerequisite is met for 78.31%, and that is not the binding number

For every line rooted at `property access, the receiver has no such property`,
is the receiver typed the way upstream types it? Unit: **gap assertion lines**,
13,156 of them.

| verdict | lines | share |
|---|---:|---:|
| receiver typed as upstream types it | **10,303** | **78.31%** |
| receiver typed DIFFERENTLY | 2,845 | 21.63% |
| receiver not rendered / line unaligned | 8 | 0.06% |

B1's threshold is ≥60%: **PASS**. 2,845 lines fail a lookup on a type
`members.rs` was never given a chance to search; they are not this row's work at
any size.

**But RULE-2 has no leg for *which file the work is in*, and that is what binds.**
Splitting the 10,303 by receiver kind:

| receiver kind | lines | share | owner |
|---|---:|---:|---|
| instantiated generic (`Promise<boolean>`, `Record<string, number>`) | 3,896 | 36.70% | `bd tsr-4qx` — **not** `members.rs` |
| named / other (`SymbolConstructor` 906) | 3,026 | 28.50% | cross-file global merging, `bd tsr-9or.1` |
| array (`string[]`, `T[]`) | 1,024 | 9.64% | `Array<T>` instantiation, `bd tsr-4qx` |
| **primitive + string/number literal** | **1,165** | **11.31%** | **`members.rs` — mine** |
| union / intersection | 779 | 7.34% | `intersections.rs` |
| `this` | 727 | 6.85% | the `this` type |

Unit: gap assertion lines. **13,156 → 10,303 → 1,165.** The row is not one work
item and never was; the part in the file this workstream owns is 8.9% of the
headline. That is a larger haircut than the 4.9× the coordinator cited as the
week's cautionary case, and it is the direct answer to *"size the conversion, not
the population"*.

### What was built: `getApparentType`, primitive arms only

`members.rs`'s own doc comment said *"there are no lib files (`bd tsr-9or.1`), so
a primitive receiver has no members here"* and attributed the gap to lib. **That
has been false since the program started loading `internal/bundled/libs`**, and
the gap was this function's. Corrected in place, with the correction marked —
`docs/conventions.md` records four stale comments outliving their truth in one
session and this was a fifth.

`Checker::apparent_type` ports the five primitive arms of upstream's
`getApparentType` (`checker.go:21729`, from `grep -n` on the declaration) in
upstream's own order, and is called at the access site, where upstream calls it
(`checker.go:11265`) — **not** inside `get_property_of_type`, which would change
what `relater.rs` sees. Eight upstream arms are deliberately not ported and each
is named with its owner in the rustdoc.

Two details that are load-bearing and were nearly wrong:

- **A missing global is a gap, never `any`.** `globals()` may not hold `String`.
  Returning `anyType` there would convert every primitive member access in a
  lib-less configuration into a confident wrong answer *and would look like a
  large win*. Test `a_missing_global_is_a_gap_and_not_an_answer` pins it, with
  the mirror beside it.
- **`declared::global_type_symbol` could not be reused.** It gates on the symbol
  having exactly one type parameter — right for `Array<T>`, and wrong for every
  interface here, all of which have none. Reusing it would have made the arm
  silently dead.

I also deleted the `any`-receiver identity guard by accident while editing and
restored it in the same sitting. Mutation **MA3** now pins it from the outside.

### B3 — the corpus, gains and losses read separately

`examples/casedelta.rs`, `b9a4f5c` against the slice. Unit: **assertion lines**,
and `cases` where it says cases.

```
  matched   before 295,302   after 296,272   delta +970
  total     before 478,954   after 478,954   (equal, as it must be)
  cases that moved  133      gained 133 (+970 lines)      LOST 0 (-0 lines)
```

And the gap/wrong split, which is what B3 actually gates on:

| | before | after | delta |
|---|---:|---:|---:|
| right | 295,302 | 296,272 | **+970** |
| gap | 135,635 | 134,659 | **−976** |
| wrong | 37,963 | 37,969 | **+6** |

**976 gap lines closed: 970 right and 6 wrong. 0.0062 wrong per right.** B3
allows 0.5; this is 80× inside it, and better than the 0.14 of the best build
this week. **No case regressed** — the `LOST 0` column is the one that matters,
because a net figure of +970 is consistent with 133 cases gaining and none
losing *or* with 200 gaining and 67 losing, and those license different next
moves.

### The prediction against the outcome

| | lines |
|---|---:|
| predicted ceiling for the primitive slice (before building) | **1,165** |
| gap lines actually closed | **976** |
| ratio | **0.84** |

The ceiling over-predicted by 16%, in the conservative direction. For contrast,
`checker-notes-rank.md` §8 records the one prior analogue landing at **6.9% of
prediction**. The difference is not skill: it is that this ceiling was taken
*after* B1 removed the lines whose receiver was typed wrongly and *after* the
work-item split removed the lines belonging to other files. **A ceiling taken
before those two filters was 13,206 and would have missed by 13.5×.**

The residue is visible in the same bucket: of the primitive-receiver lines still
gapping, 84 want `any` — which this port must not answer — and 46 want `string`.

### Controls and mutations for Part 2

| control | reads | pinned by |
|---|---:|---|
| C1–C4, A1, A2, A3 | 0 | as Part 1 |
| **C7** re-measured board split − `rank_board`'s | **0 / 0** | **another instrument** |
| casedelta `total` before == after | equal | arithmetic — a change to the skip set would show as an unmatched key |
| casedelta cases LOST | **0** | — a **measurement**, and the one a net figure hides |

**C7 went stale twice in one session and both times read as a defect in this
probe.** Its constants are the *compiler's*, not the probe's: 22,739 / 45,814 at
`b5decc5`, 22,764 / 44,342 at `b9a4f5c`, 22,793 / 44,254 after this slice landed
— the last move caused by this workstream's own change. That is the standing cost
of the only control here that can see a span-test inversion, and it is now
written into the printed line so the next reader checks the commit before
checking the walk.

| mutation | what it does | result |
|---|---|---|
| **MA1** | `apparent_type` returns its argument unchanged | **both** new tests red (`error` where `number` is expected) |
| **MA2** | a missing global answers `anyType` instead of the original type | `a_missing_global_is_a_gap_and_not_an_answer` red — `any` where `error` is expected; the other test **stays green**, which is what makes the pair a partition |
| **MA3** | the `any`-receiver guard tests `TypeFlags::ANY` instead of identity | the file's **existing** `a_property_access_on_an_untypeable_receiver_is_still_a_gap` red — the standing warning holds, and my edit did not weaken it |
| **B2-off** | `PROPERTY_DECLARATION_EDGE = false` | the refused row reappears at 10,012 lines; this is the measurement, not a defect |

MA2 is the one worth keeping: it is the only mutation whose two assertions
disagree, and *that* is the evidence the arm distinguishes "the global is
missing" from "the global is there and the member is not".

---

# Part 3 — `PropertyAccessExpression` as a population, and `tsr-4qx` re-derived on lines

## RULE-3, registered before either measurement

Written before the probe was changed and before `tsr-4qx`'s slice was opened,
in its own commit, with predictions attached. Same handling as RULE-2, for the
same reason: RULE-1 was post-hoc and this is what a rule has to look like to be
worth quoting.

**Population for P1, pinned syntactically.** Gap assertion lines whose **node
kind is `PropertyAccessExpression`** — `checker-notes-wrong.md`'s addendum
measures **|P| = 24,072, of which 15,215 gap, 1,621 wrong, 322 unaligned, and
TS2563 exposure 0**. The kind is a property of the AST, so `|P|` cannot move
under the thing being measured. That is `fnexpr.rs`'s shape and I am copying it
deliberately rather than inventing a fourth population.

**Population for P2.** Gap assertion lines blocked by a receiver that is an
**instantiated generic**, which Part 2 measured at 36.70% of 10,303 on one row
(~3,781) plus 9.64% arrays (~1,024) — but that was one row, and P2's whole point
is to size it over the board rather than over a row.

> **RULE-3 / P1 — the property-access population.**
> The actionable arm is `receiver RIGHT`: **we hold upstream's own type for the
> receiver and still cannot look the member up**. Every other arm is a symptom
> of something upstream of it. Arms mirror `checker-notes-evolvearray.md`'s
> `ElementAccessExpression` split arm for arm, so the two are comparable:
> `RIGHT / gapped / wrong / unaligned / not rendered`, with `not rendered` as
> the default and `RIGHT` carrying the positive test.
>
> Build only if `RIGHT` clears **all three**:
> - **P1a — size ≥ 2,000 lines.** The slice landed this session converted 976.
>   A follow-on has to be worth at least twice that to beat the alternatives on
>   the board.
> - **P1b — top-1 case ≤ 40%**, top-10 ≤ 90%.
> - **P1c — ≤ 25% of the arm's lines want `any`, and ≤ 25% want a name this
>   port has no route to** (`typeof …`, `import(…)`).
>
> **Prediction, recorded to be falsified:** I expect `RIGHT` to land between
> **3,000 and 6,000** lines — proportionally far above `ElementAccess`'s 786 of
> 13,141 (6.0%), because that population is 76% TS2563 and this one has **zero**
> TS2563 exposure, and because Part 2 already measured 78.31% correct receivers
> on one of its rows. If it comes in under 2,000 I was wrong about the whole
> population and P1a refuses it.
>
> **RULE-3 / P2 — `tsr-4qx`, re-derived on the line gradient.**
> The parked verdict — *"of the 225 affected cases, ZERO have nothing else
> failing"* — is a **case-gate** fact. It is true and it is **not inherited
> here**: `cad4a4c` establishes that for the line gradient a row that finishes
> no case can still be the largest lever available. So:
> - **P2a — size ≥ 2,000 gap lines blocked**, measured over the board and not
>   over one row.
> - **P2b — the seam is landable in files this workstream owns.** Steps 1–3 of
>   the issue move no line, so they are testable on their own; step 4 is the
>   only one that moves the gradient.
> - **P2c — wrong-per-right ≤ 0.5** on a `casedelta` pair, gains and losses read
>   separately, and **0 cases lost** is not assumed.
>
> **P2 is not a threshold but a boundary, and it outranks all three:** the issue
> says step 3 needs "a class/interface sibling of `type_parameter_types`
> (`inference.rs:314`)", and `inference.rs` is **not mine**. If that turns out
> to be *required* rather than optional, **stop and report** — do not build
> across the line, do not reimplement it locally in a file I do own. A local
> copy of another workstream's function is the worse failure, because it passes
> every gate.
>
> **Prediction:** I expect P2b to hold for steps 1–3 and the `inference.rs`
> boundary to be **real at step 4**, because instantiating a member's type needs
> a type-parameter→argument mapping and that is what `type_parameter_types`
> builds.

## RESULTS — Part 3

*(Nothing above this line was edited after the run.)*

Measured at **`5eb252c`** (`main` at 62.49%, 2,259 cases). Gradient: **299,291
right / 131,950 gap / 37,659 wrong / 10,054 unaligned**.

### P1 — the property-access population, and the prediction held

Unit: **gap assertion lines**. `|P|` is every gap line whose **node kind** is
`PropertyAccessExpression`; the kind is a property of the AST, so `|P|` cannot
move under what is being measured.

| arm | lines | share |
|---|---:|---:|
| `gapped` — the receiver is itself a gap; **symptom** | 8,241 | 56.10% |
| **`RIGHT` — we hold upstream's own type and cannot look the member up** | **5,441** | **37.04%** |
| `wrong` — the receiver is confidently wrong; **symptom** | 999 | 6.80% |
| `not rendered` — the default arm | 8 | 0.05% |
| **TOTAL** | **14,689** | |

`checker-notes-wrong.md`'s addendum reads **15,215** for this kind at `f67b8e5`;
this reads 14,689 at `5eb252c`. The 526-line difference is the corpus moving
between the two commits — this session's `getApparentType` slice alone converted
976 lines, most of them in this very population. **Two instruments, two commits,
and the difference is in the direction and of the order the intervening work
predicts.**

| RULE-3 leg | threshold | measured | verdict |
|---|---|---:|---|
| **P1a** size | ≥ 2,000 | **5,441** | **PASS** |
| **P1b** concentration | top-1 ≤ 40%, top-10 ≤ 90% | **6.8%** / 30.9%, over **1,123 cases** | **PASS** |
| **P1c** spellability | `any` ≤ 25%, unnameable ≤ 25% | **10.3%** / **1.7%** | **PASS** |

**The prediction registered before the run was 3,000–6,000. It came in at
5,441.** For contrast, `ElementAccessExpression`'s equivalent arm is 786 of
13,141 (6.0%) — this population's `RIGHT` share is **six times** that, and the
reason predicted in advance is the right one: that population is 76% TS2563 and
this one has **zero** TS2563 exposure.

**5,441 lines, 1,123 cases, top-1 6.8%, 1.7% unnameable — this is the least
concentrated large actionable population measured on this board**, and it is
5.6× the slice that landed this session.

### But it is not one work item either, and the split names four owners

The `RIGHT` arm by receiver kind. Unit: **gap assertion lines**.

| receiver kind | lines | share of `RIGHT` | owner |
|---|---:|---:|---|
| named / other — an interface whose member we did not find | **2,599** | **47.77%** | mixed; `SymbolConstructor` is `bd tsr-9or.1` |
| `this` — the `this` type | 1,165 | 21.41% | not ported |
| instantiated generic | 958 | 17.61% | `bd tsr-4qx` |
| array (`Array<T>`'s members) | 358 | 6.58% | `bd tsr-4qx` |
| union / intersection | 295 | 5.42% | `intersections.rs` (mine) |
| primitive + literals | 66 | 1.21% | **`members.rs` — and this is the residue of the slice already landed** |

The primitive bucket is down to 66 lines from 1,165, which is the same fact as
Part 2's corpus result seen from the population side, and is the cleanest
confirmation available that the slice did what it was sized to do.

**The lesson repeats at a third scale.** 24,072 → 14,689 gap → 5,441 actionable
→ 2,599 in the largest single sub-item. Every time this board has offered a
large number, the number has been a sum over owners. `docs/conventions.md`'s
*"a row named after a node kind is usually not about that node kind"* is now
measured three times on three populations.

### P2 — `tsr-4qx` re-derived on lines: the size passes and the boundary fails

**P2a — size, on the line gradient.** Gap lines blocked by a receiver that is an
instantiated generic or an array, at `5eb252c`, unit gap assertion lines:

| receiver kind | lines blocked |
|---|---:|
| instantiated generic | 3,896 |
| array | 1,265 |
| **total** | **5,161** |

Threshold ≥ 2,000: **PASS**, by 2.6×. The parked case-gate verdict — *"of the
225 affected cases, ZERO have nothing else failing"* — is true and is **not**
what refuses this on the line axis. `cad4a4c` is right about that.

**P2b — the seam is landable in files I own: FAIL. And the boundary is not the
file that was flagged.**

Traced, function by function:

| step | what it needs | where it lives | mine? |
|---|---|---|---|
| 1. add `get_type_of_property_of_type` | — | `members.rs:145` | **already done** (`8fa6a3e`) |
| 2. route `indexed.rs`, `relater.rs` through it | — | `indexed.rs:122`, `relater.rs:403-404` | **already done** |
| 3. instantiate inside the seam | `instantiate_type` | `inference.rs:262`, **`pub(crate)`** | **callable — not a boundary** |
| 3. …and the type-parameter list for a class/interface | `local_type_parameters_of` | `declared.rs:789`, **private** | **NO** |
| 4. flip `create_type_reference` to carry members | `create_type_reference` | `declared.rs:486` | **NO** |

**Steps 1 and 2 are already landed.** There is nothing left for this workstream
to do on `tsr-4qx` in its own files — not "a little", *nothing*.

**The flagged risk was `inference.rs` and that is not the boundary.**
`instantiate_type` is already `pub(crate)` and callable from `members.rs`
without touching the file. The real boundary is **`declared.rs`**, and it blocks
*both* remaining steps:

- Step 3 needs `local_type_parameters_of` (`declared.rs:789`), which is
  **private**. Making it visible is a one-word edit in a file I do not own.
- Step 4 **is** `create_type_reference` (`declared.rs:486`), whose
  `self.store.new_named(TypeFlags::OBJECT, printed, None)` — `members: None` —
  is precisely why `get_property_of_type` misses on `Promise<boolean>`.

**My own prediction was wrong in the same way the briefing was**: I predicted the
`inference.rs` boundary would be real at step 4. It is not a boundary at all,
and the one that is real sits in a different file. Recorded because a prediction
that is wrong in the same direction as the briefing it was checking is worth more
than one that agrees.

**And the order is not negotiable, which is what makes a local workaround
wrong.** `create_type_reference`'s own comment says a member of `class C<T>`
answered without instantiation gives `T` where upstream gives `number`. So
flipping step 4 without step 3 does not convert 5,161 gap lines — it converts
them into **wrong** lines. The two steps are one change, they are both in
`declared.rs`, and re-implementing `local_type_parameters_of` in `members.rs` to
get around that would be a second copy of another workstream's function that
passes every gate. **Refused, reported, not built.**

| RULE-3 leg | verdict |
|---|---|
| **P2a** size ≥ 2,000 lines | **PASS** — 5,161 |
| **P2b** landable in my files | **FAIL** — steps 3 and 4 are both `declared.rs` |
| **P2c** wrong-per-right ≤ 0.5 | **not reached** — P2b's boundary outranks |

`tsr-4qx` is handed back with the axis corrected and the boundary named: it is
worth **5,161 gap lines / ~1.08 points** on the line gradient, and it is
**`declared.rs`'s owner's to build**, in one change that does steps 3 and 4
together.

### C7 was one control doing two jobs, and it is now two

C7 went stale a **third** time on this run — 22,739/45,814 at `b5decc5`,
22,764/44,342 at `b9a4f5c`, 22,793/44,254 after my own slice, 22,354/43,250 at
`5eb252c`. Four values in one session. Chasing the constants a third time was
the wrong response, so the control was split by asking what each leg is pinned
by. Measured against `rank_board` at `5eb252c`, unit **gap assertion lines**:

| bucket | `rank_board` | `gaproot` | delta |
|---|---:|---:|---:|
| `TERMINAL` | 22,354 | 22,354 | **0** |
| `propagated/named` | 16,844 | 16,458 | **−386** |
| `propagated/span` | 33,064 | 33,257 | +193 |
| `UNMATCHED` | 16,438 | 16,730 | +292 |
| `DEPENDENT-UNKNOWN` | 43,250 | 43,151 | −99 |
| **sum** | 131,950 | 131,950 | **0** |

`rank_board` builds `Checker::new`; this probe builds
`Checker::with_module_host`, which is what `render_case` itself builds and so is
the configuration the gradient is scored through. The host can only ever *give a
receiver a type*, and the only arm that reads is `the receiver is a gap`.

**`TERMINAL` requires that no dependency is named at all, so no line the module
host affects can enter or leave it.** That makes **C7a an invariant across two
instruments rather than a coincidence**, and it is the leg carrying the
evidence. The other legs are **C7b, a measurement**: the divergence is 386 lines
here against 54 at `b9a4f5c`, and it *should* grow as the cross-file seam
answers more.

The general form is worth stating, because I got it wrong twice before getting
it right: **a cross-instrument control whose constants must be re-taken on every
commit is a tripwire, not an invariant.** The fix is not fresher constants — it
is finding the sub-quantity the two instruments must agree on *by construction*,
and letting the rest be a printed measurement.

---

# Part 4 — the cross-file merge item changed owner twice, and it is 702 lines

Measured at **`5eb252c`**. No rule is registered for this section because
**nothing was built and nothing could be**: the section is a reproduction, a
re-diagnosis and a size, and the boundary was found before any threshold could
matter.

## The premise, corrected twice

`bd tsr-9or.1` was briefed as *"the globals are never merged across files"*.
Then, corrected by the naming agent, as *"the binder merges them; the lookup
that answers `false` is downstream of both — in `get_property_of_type`, in
`members.rs`. Yours."*

**The first is wrong and the second is wrong**, and
`crates/tsr-checker/tests/members_cross_file_merge.rs` is the reproduction that
shows both.

| asked how | file `a`'s member | file `b`'s member |
|---|---|---|
| through `globals()` — the merge **target** | found | found |
| from a reference **in file `a`** | found | found |
| from a reference **in file `b`** | **NOT found** | found |

Two script files, each `interface I` with one property. The binder's merged
table holds both — the first briefing is refuted by the first row. And the
lookup is not wrong either: it is handed a **different symbol**.

`merge_symbol` (`binder.rs:630`) unions `source` into `target` and **records no
link back**. `grep` for `merge_id`, `merged_symbol` or `mergeId` across the
binder and the checker returns **nothing**. So a reference inside the *source's*
file resolves through `resolve_name` → `lookup_local`, reaches the **source**
symbol, and that symbol's members table legitimately holds only its own file's
members. `get_property_of_type` answers correctly for the symbol it was given.

Upstream's mechanism is **`getMergedSymbol`** (`checker.go:14355`, from `grep -n`
on the declaration), backed by a `mergedSymbols` source→target map and applied
at every symbol read. This port has neither the map nor the redirect.

**So the ownership conclusion is stronger than "not mine".** The information the
fix needs *does not exist outside the binder*. There is no version of it that
can be written in `members.rs` — not a partial one, not a local one — because
nothing anywhere connects symbol `b-I` to symbol `a-I`. This is not a case of
declining to build half a mechanism; it is a case where half a mechanism is
unreachable.

## Sized, as an interval, and the ~2,936 is withdrawn

**Do not carry ~2,936 forward.** It was my own `named / other` root-blocked
figure from Part 2's B1 split and it conflates several causes. Measured directly
over the `RIGHT` arm's 2,599 named-receiver lines. Unit: **gap assertion lines**.

| verdict | lines | share |
|---|---:|---:|
| **MERGED ACROSS FILES — the `getMergedSymbol` item** | **702** | 27.01% |
| declared in one file — **not** this item | 494 | 19.01% |
| receiver's type is not a bare global name — **unmeasured** | 1,403 | 53.98% |

The test looks the printed type name up in `globals()` and then asks which
**files** its declarations live in. **It undercounts by construction** — a
receiver typed through a local alias or a namespace member is invisible to it —
**and it cannot overcount**, because a name with declarations in two files
really has them.

**So the size is the interval [702, 2,105]**: 702 measured, 494 positively
excluded, and the 1,403 unmeasured bounded above by the arm itself. Quote the
floor and the interval; there is no point estimate inside it that this
measurement supports. That is the same discipline as `e6ab9c9` — **the error bar
belongs on the leg that is inferred**, and here the inferred leg is the 1,403,
so the interval is asymmetric and open upward rather than a tidy ± around 702.

## Sequencing, and a premise of the handover I have to correct

The handover says *"you are in `get_type_of_property_of_type` for the generics
seam"*. **I am not.** Part 3 refused `tsr-4qx` and handed it back — steps 3 and
4 are both `declared.rs` — so there is nothing of mine in that call path to
collide with, and no conflict to sequence.

The two items *do* land in the same lookup, and both are in files this
workstream does not own:

| item | what it changes | file |
|---|---|---|
| `tsr-4qx` | **what** the symbol's type carries (instantiated members) | `declared.rs` |
| `getMergedSymbol` | **which** symbol the lookup receives | the binder |

They are independent — one is the argument, the other the answer — but they will
both be measured through `get_property_of_type`, so a before/after pair for
either must name which one moved.

## And the renderer question, asked before it was needed

`c592d0f` says half a mechanism renders the collateral of the half you built.
Asked of this item: what would print the members it unblocks? They are ordinary
interface properties — `a: string`, `b: number` — whose types are written
annotations, so the renderer involved is the same one already printing them in
their own file. **That is a materially lower risk than the generics seam**,
whose unblocked members are instantiated types handed to a printer nobody has
checked. Recorded here because the question is cheap and the answer differs
sharply between two items that look adjacent.

## Everything filed from this page

| id | what | sized as |
|---|---|---|
| `tsr-pnf` | `property access, the receiver has no such property` | ~~13,206~~ — **976 converted** by the primitive slice; the rest is `bd tsr-4qx` and `bd tsr-9or.1`, not `members.rs` |
| `tsr-mcd` | `property access, the property has no type` | ~~6,612~~ — **refused**, 96.8% dissolves under the property-declaration edge |
| `tsr-4gq` | type-node roots are ceilings, not proofs | 34,440 lines of the ranking are affected |
| `tsr-qgk` | only the first gapped operand is credited | 34.4% of 176,593 span steps had a choice |
| `tsr-phd` | the `any` leg disqualifies apparent work, and may be too strict | 34,180 lines held out |
| `tsr-wii` | the primitive-receiver residue after the slice: 84 lines want `any` (must not be closed), 46 want a real type | 46 lines |
| `tsr-iks` | the `PropertyAccessExpression` RIGHT arm — the largest clean population on the board | **5,441 gap lines**, of which the largest sub-item is 2,599 |

Both build items are sized in **lines they unblock**, not lines they contain,
and both numbers are ceilings.

## Superseded numbers

**2026-08-06, corrected by Part 2 of this same page.**

| number | as published in Part 1 | corrected | why |
|---|---:|---:|---|
| `property access, the property has no type` blocks | 6,612 | **319** | Part 1 had no property-declaration edge, so the access claimed to be its own root. 96.8% of the row is downstream of the property's declaration. |
| the two rows together are work worth | 19,818 | **976 converted** | 78.31% have a correct receiver (B1), and of those only 11.31% have a receiver kind `members.rs` can act on. |
| `ROOT/own-rule` share of the gap | 43.49% | **39.73%** at `b9a4f5c` with the edge on | the edge moves 9,693 lines out of `own-rule` and into type-node and other roots. |

Part 1's figures are **not wrong at `8a38b2c`**; they are what that instrument
measured. What was wrong was reading them as sizes for work in one file, and
that reading was mine, in the two `bd` issues and in the report I sent. The
`ROOT/type-node` ceiling declared in Part 1's "limit" section is where most of
the dissolved 9,693 lines went, which is the limit doing exactly what it was
declared to do.

## Reproducing this

```bash
git worktree add --detach /tmp/gaproot 8a38b2c
cd /tmp/gaproot && git submodule update --init --recursive
CARGO_TARGET_DIR=/tmp/gaproot/target cargo run --release -p tsr-conformance --example gaproot
CARGO_TARGET_DIR=/tmp/gaproot/target cargo run --release -p tsr-conformance --example reconcile
```

`gaproot` takes about 30 seconds after the build. **Take it in a worktree, not in
the shared checkout** (`checker-notes-guard.md`).
