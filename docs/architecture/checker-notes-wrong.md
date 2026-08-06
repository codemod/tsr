# The wrong bucket, ranked by whether closing a row flips a case

Status: measured 2026-08-06 at **`058b4a9`**, over the 9,538-case `.types`
population, from one pinned binary in an isolated worktree
(`CARGO_TARGET_DIR` not shared — `checker-notes-guard.md` records losing a
measurement to a shared target dir). The instrument is
`crates/tsr-conformance/examples/wrongflip.rs`, added in the same commit as this
file. Upstream references are to `vendor/typescript-go` @ `5b1047d10`.

**Unit, restated on every table below.** Every count is an **assertion line** in
a `.types` baseline unless the table's header says `cases`. The board has twice
turned a node count into a line count by omission
(`docs/architecture/checker-notes-rank.md`), so no table here omits it.

---

## Why this page exists

`checker-notes-rank.md` §6 is the only place in `docs/architecture/` where the
37,489 **wrong** lines appear at all, and it ranks them two ways: by node kind,
and by node kind × shape substitution. Neither is a work item. "31,067 wrong
`Identifier`s" does not name anything anyone can build, and `Identifier
intrinsic -> intrinsic` (5,453 lines, 82 finishes) barely narrows it.

A wrong line is strictly more valuable to fix than a gap line of the same count,
because it costs a case *and* a line. It is also strictly more dangerous,
because the code producing it exists and something depends on it.

Three questions this page answers that §6 does not:

1. **Is the wrong answer this row's own fault?** §6 has no TERMINAL/PROPAGATED
   split for wrong lines at all — that machinery was built for gaps only. A
   wrong `Identifier` whose declaration is *also* wrong is not a work item; it
   is a symptom. This page splits every wrong line by whether something it
   depends on also failed, by **three** independent tests, because one is not
   enough: a span test for descendants; a resolved-symbol test for identifiers,
   because the span test is blind on a leaf; and a sibling test for declaration
   names, because `x` in `var x = f()` spans only `x` while the thing it waits
   on is `f()` beside it. `docs/conventions.md` records the span-only version of
   exactly this classifier calling 6,749 gap lines TERMINAL that were nothing of
   the kind.
2. **Does closing the row finish a case?** Per row, over the cases it touches,
   how many would have **nothing left**. This is `residual_after == 0` and it is
   the bucket the decision rule below is registered on — not a `≤10 remaining`
   band. `docs/conventions.md` records a proxy saying *build* at 29.8% while the
   direct bucket one row above read 0.0%.
3. **Wrong type, or right type we cannot name?** Different work items in
   different files. Only the baseline's right-hand side distinguishes them, so
   every top row prints its exact `upstream -> ours` pairs.

---

## The pre-registered decision rule

Written here **before the probe was run**, and not edited afterwards. Whether it
fired is recorded in its own section below, verbatim, including if it did not.

> **RULE.** Build the top row only if a **single** row, restricted to lines the
> probe classifies `ROOT` (nothing it depends on also failed), satisfies **all
> three**:
>
> - **R1 — case flips.** `finishes ≥ 100` cases. `finishes` is an upper bound
>   (`checker-notes-rank.md` §8: the one measured analogue delivered 6.9% of
>   touched cases against a predicted 10–25%), so a ceiling of 100 projects to
>   roughly 25 real cases under a 4× haircut, i.e. **+0.26 points** — which is
>   the bar set by the last cycle's best gradient-shaped delta of **+0.34**.
>   Anything under 100 cannot be argued to beat work already on the board.
> - **R2 — concentration.** Largest single case ≤ **40%** of the row's lines.
>   Three of the top gradient rows evaporated on this check, one by 86.6%
>   (`checker-notes-rank.md` §3), and §6's own `Identifier intrinsic -> array`
>   is 100.0% top-1 across four cases.
> - **R3 — spellability.** Fewer than **25%** of the row's lines are ones where
>   we compute a plausible type and upstream prints a *name* we have no route
>   to. A row that fails R3 is a printer item, not a checker item, and building
>   it in the checker manufactures confident wrong answers
>   (`docs/conventions.md`, "can this port spell the answer?").
>
> **AND**, independently of the three: the row must not be closable by widening
> an `any` answer (`checker-notes-rank.md` §6: *"no row on this board should be
> closed by widening an `any` answer"*), and the fix must live in a file this
> workstream owns.
>
> If no row clears all three, **do not build**. The ranking alone is the
> deliverable.

### Why the rule is on `finishes` and not on a band

`finishes` **is** the question. The brief's question is "does closing this row
finish a case?"; `residual_after == 0` counts exactly the cases where it does.
There is no inference between the bucket and the decision, so this is not a
proxy and is not labelled one. R2 and R3 are guards on the *credibility* of R1,
not substitutes for it.

### The known error in `finishes`, stated before it is quoted

`finishes` assumes the row is closed **completely in every case it touches**. It
is a ceiling. It is also computed against a residual that cannot see lines *we*
emit which upstream does not (`checker-notes-rank.md` §7 records a +18 case
discrepancy against the gate on exactly this). Both errors run the same way —
they make `finishes` **too large** — which is the wrong direction for a build
decision, and is why R1 is set at 4× the delta it needs to beat.

---

## RESULTS

*(Nothing above this line was edited after the run.)*

### The verdict, first

**The rule did not fire. Nothing was built.**

The best ROOT row on the whole wrong bucket finishes **37 cases** against R1's
threshold of **100**. The second best finishes 33, and those two are largely the
*same lines* (a class name in an `extends` clause, seen once by position and
once by symbol). No row reaches half the bar. R2 and R3 were never reached,
because R1 is conjunctive and failed first.

This is not a near miss that better instrumentation would rescue. The wrong
bucket's case-flipping power is **not concentrated in any row at all** — it is
spread over hundreds of rows of two to forty lines each, which is exactly what
§7 of `checker-notes-rank.md` predicted for the corpus as a whole and had not
been checked for this bucket.

### Finding 1 — the wrong bucket is 81% symptom, and nobody knew

The number that changes the most decisions on this page:

| cause | lines | share of the wrong bucket |
|---|---:|---:|
| `propagated/declaration` — the name's declaration is also wrong | 15,966 | **42.34%** |
| `UNKNOWN` — no evidence either way | 8,016 | 21.26% |
| **`ROOT` — nothing it depends on failed** | **7,192** | **19.07%** |
| `propagated/sibling` — the initialiser or annotation beside it failed | 3,294 | 8.74% |
| `propagated/descendant` — something inside its span failed | 3,241 | 8.59% |

Unit: **wrong assertion lines**, 37,709 of them. `cases` appears nowhere in this
table.

`checker-notes-rank.md` §6 ranks `WRONG: Identifier` at 31,067 lines and **521
finishes** and calls it *"the largest case-flipping row on the whole board"*.
That number is correct and it is a **ceiling over a population that is four
fifths symptom**. There is no cause split for wrong lines anywhere in
`docs/architecture/` before this page, because the TERMINAL/PROPAGATED machinery
in `rank_board` is driven by `gap_reason`, which only fires on lines that
answered `error`.

The mechanism is worth stating because it is *worse* for wrong lines than for
gaps. A gap propagates as a gap and is visibly a gap all the way up the tree. A
**wrong answer propagates as a different wrong answer**: `var x = f(); x.p;`
puts one wrong line on `f()`, one on `x` and one on `x.p`, and in §6's histogram
those land in three different rows, each looking like its own work item. The
same defect is counted three times under three names.

**The `10,000`-line `any -> never[]` row in `compiler/largeControlFlowGraph`**
that §6 already discounted on concentration turns out to be discountable twice
over: this probe classifies **all 10,000 of them `propagated/declaration`**.
They are one wrong declaration, read ten thousand times.

### Finding 2 — 29.19% of the wrong bucket is not a checker item at all

| what the failure is | lines | share |
|---|---:|---:|
| same shape, different text | 10,735 | 28.47% |
| **upstream NAMED it, we printed a structure** | **10,463** | **27.75%** |
| different shape — a genuinely wrong type | 8,045 | 21.33% |
| we answered `any` and upstream did not | 7,286 | 19.32% |
| **upstream expanded it, we printed a name** | **545** | **1.45%** |
| `error` leaked into a printed type (`error[]`) | 635 | 1.68% |

Unit: **wrong assertion lines**. The two bolded rows — **11,008 lines, 29.19%** —
are ones where the answer's *shape* is right and its *spelling* is not. That is
a printer/`typeToString` work item and building it in the checker converts
nothing. `docs/conventions.md`'s *"can this port spell the answer?"* is normally
asked of gap rows before building; asked of the wrong bucket it says that
**nearly a third of it is already the failure that rule exists to prevent** —
these are lines where the checker did the work and the naming lost it.

> **This classifier is total and is therefore NOT a control.** No bucket of it
> reading zero would prove a partition. Said here rather than left for a reader
> to infer.

Two numbers on this table also **confirm `checker-notes-rank.md` §6 from a
different code path**: `we answered any and upstream did not` reads **7,286**
against §6's **7,288**, and `error` leaked reads **635** against §6's **672** —
the small deltas are the two commits between `33e3bd5` and `058b4a9`.

### RANKING W1' — the same key `rank_board` §6 uses, as a cross-instrument check

Unit: **assertion lines**; `finishes` and `cases` are **cases**.

| row | lines (here) | lines (§6) | finishes (here) | finishes (§6) | top-1 |
|---|---:|---:|---:|---:|---:|
| `Identifier intrinsic -> array` | 10,003 | 10,003 | 0 | 0 | 100.0% |
| `Identifier intrinsic -> intrinsic` | 5,440 | 5,453 | 83 | 82 | 22.6% |
| `Identifier function/signature -> function/signature` | 1,589 | 1,573 | 38 | 36 | 1.9% |
| `Identifier function/signature -> object literal` | 1,402 | 1,402 | 18 | 18 | 21.8% |
| `Identifier array -> array` | 1,215 | 1,215 | 6 | 6 | 50.3% |
| `Identifier object literal -> object literal` | 1,044 | 1,044 | 31 | 31 | 9.6% |

Three rows match to the line and to the case; the rest differ by amounts
consistent with two intervening commits. **This is the evidence that `wrongflip`
is measuring the same thing `rank_board` is** — two instruments by different
authors along different code paths, which `docs/conventions.md` calls the
strongest form available here. It is why the *new* columns on this page can be
quoted.

### RANKING W3 — position (parent kind > node kind), ROOT lines only, by case flips

Unit: **assertion lines**; `finishes` and `cases` are **cases**.

| row | lines | **finishes** | cases | top-1 | top-10 | median left | R3 |
|---|---:|---:|---:|---:|---:|---:|---|
| `ExpressionWithTypeArguments > Identifier` | 204 | **33** | 122 | 9.8% | 31.9% | 4 | PASS (9.8%) |
| `ExpressionStatement > Identifier` | 528 | 17 | 100 | 7.4% | 38.6% | 17 | PASS (10.8%) |
| `BinaryExpression > Identifier` | 2,273 | 15 | 182 | **54.1%** | 72.2% | 26 | PASS (1.3%) |
| `ExportAssignment > Identifier` | 81 | 13 | 69 | 4.9% | 27.2% | 5 | PASS (4.9%) |
| `CallExpression > Identifier` | 313 | 8 | 89 | 28.4% | 52.4% | 23 | PASS (4.8%) |
| `EnumMember > StringLiteral` | 37 | 6 | 22 | 18.9% | 67.6% | 5 | **FAIL (94.6%)** |
| `VariableDeclaration > Identifier` | 143 | 6 | 39 | 16.8% | 63.6% | 19 | — |

### RANKING W4 — what the identifier resolves to, ROOT lines only, by case flips

Unit: **assertion lines (identifiers only)**; `finishes` and `cases` are
**cases**. This is the axis that names a work item rather than a syntax
position, and it is new here.

| row | lines | **finishes** | cases | top-1 | top-10 | median left | R3 |
|---|---:|---:|---:|---:|---:|---:|---|
| `SymbolFlags(CLASS) / ClassDeclaration` | 269 | **37** | 161 | 7.4% | 32.7% | 4 | PASS (6.7%) |
| `SymbolFlags(BLOCK_SCOPED_VARIABLE) / VariableDeclaration` | 1,041 | 19 | 179 | 6.0% | 28.4% | 16 | PASS (4.1%) |
| `SymbolFlags(FUNCTION_SCOPED_VARIABLE) / VariableDeclaration` | 2,141 | 18 | 152 | **57.4%** | 76.6% | 10 | PASS (0.5%) |
| `SymbolFlags(FUNCTION_SCOPED_VARIABLE) / Parameter` | 1,277 | 13 | 182 | 8.4% | 35.4% | 20 | PASS (12.9%) |
| `SymbolFlags(FUNCTION) / FunctionDeclaration` | 127 | 7 | 55 | 7.9% | 46.5% | 16 | PASS (12.6%) |
| `SymbolFlags(EXPORT_VALUE) / ClassDeclaration` | 59 | 4 | 29 | 30.5% | 66.1% | 6 | PASS (0.0%) |
| `SymbolFlags(VALUE_MODULE) / ModuleDeclaration` | 25 | 3 | 19 | 12.0% | 64.0% | 8 | — |

The two rankings agree on the top row, and that agreement is the substance: the
33 flips at `ExpressionWithTypeArguments > Identifier` and the 37 at
`SymbolFlags(CLASS) / ClassDeclaration` are mostly the same lines seen from two
sides — **a class name used as a base type**. The exact substitutions say what
is wrong with them:

```
  11  upstream `A<Base>`     ours `A<T>`
   9  upstream `C3<U>`       ours `C3<T>`
   7  upstream `C3<V>`       ours `C3<T>`
   5  upstream `typeof C`    ours `C`
   4  upstream `AA<string>`  ours `AA<T>`
```

We print the **uninstantiated** type parameter where upstream prints the type
argument. That is generic instantiation, which is `inst`/`subst` work and lives
in files this workstream does not own. Filed rather than built.

### The other rankings, and why they are on the page but not ranked

**W2 (the exact substitution, ROOT only)** is the flattest table produced this
session: the best row flips **11** cases (`any -> undefined`, 543 lines), then
6, then 4, then a long run of 2s and 1s — many of them string-escape
divergences (`"\t\n\v\f\r"` vs `"\t\n\r"`, 8 cases across four rows)
that are a **scanner** item and cost one line each. A whole-corpus exact-pair
ranking cannot support a build decision and is recorded so nobody re-derives it.

**W1 split by cause** is where the §6 ceiling gets its haircut. Its largest ROOT
row — `Identifier intrinsic -> intrinsic`, 2,065 lines — flips **18** cases and
is **59.5% one case** (`compiler/binaryArithmeticControlFlowGraphNotTooLarge`,
1,229 lines). §6's merged row reads 5,440 lines and 83 finishes; the ROOT part
of it is 38% of the lines and 22% of the finishes.

---

## Did the rule fire?

**No.** Verbatim against the three conditions:

| | threshold | best row measured | verdict |
|---|---|---|---|
| **R1** case flips | `finishes ≥ 100` | **37** (`SymbolFlags(CLASS) / ClassDeclaration`) | **FAIL** |
| **R2** concentration | top-1 ≤ 40% | 7.4% on that row — would have passed | not reached |
| **R3** spellability | naming < 25% | 6.7% on that row — would have passed | not reached |

R1 is the binding constraint and it fails by 2.7×. **No build was licensed and
none was made.** The ranking is the deliverable.

Two things follow that are worth more than a build would have been:

1. **The `WRONG: Identifier` row at the top of `checker-notes-rank.md`'s RANKING
   B is not available work at the size stated.** Its 521 finishes are a ceiling
   over a population that is 81% symptom; the ROOT part of the whole wrong
   bucket flips at most 37 cases in any single row. Anyone briefed off §8's
   table should read this page first.
2. **The largest single lever inside the wrong bucket is not a checker row.**
   11,008 lines (29.19%) fail on naming rather than typing, and 15,966 (42.34%)
   are downstream of a wrong declaration. Neither is addressed by the row-shaped
   work the board is organised around.

---

## Controls, printed unconditionally, with what pins each

| control | reads | pinned by | what makes it non-zero |
|---|---:|---|---|
| **C1** leaf kind classified `propagated/descendant` | **0** | **construction** — nothing can be properly inside a leaf, true before the file was written | a scan-direction inversion (proven, below) |
| C1′ non-leaf so classified (mirror) | 3,241 | — | pins C1 against being zeroed by disabling the whole test |
| **C2** a line blaming its **own** declaration | **0** | **construction**, re-tested at the verdict rather than inferred from the `continue` | deleting the self-exclusion (proven, below) |
| C2′ the self-exclusion fired | 5,198 | — | a **population, not a violation** |
| C2″ another declaration inspected | 21,047 | — | mirror |
| **C3** a row scored `finishes` in a case carrying an unaligned line | **0** | **construction** — an unaligned line is in the residual and is not a wrong line, so no wrong row can take that case to 0 | a residual computed off the wrong denominator |
| **C6** cases with residual 0 | **2,191** | **another instrument** — the gate reports 2,173, and §7 records a known **+18** | any other value |
| C5 declaration names blamed on a sibling | 3,294 | — | the sibling arm's population, printed so a reader can see the arm is reachable |
| A1 `right + gap + wrong − aligned` | 0 | arithmetic | a lost line |
| A2 / A2′ / A2″ each ranking's row sum − wrong total | 0 / 0 / 0 | arithmetic | a double count |
| A3 / A4 cause split, naming split − wrong total | 0 / 0 | arithmetic | a double count |

**C6 landing on exactly 2,173 + 18 is the strongest single line on this page.**
`checker-notes-rank.md` §7 recorded that +18 at `33e3bd5` from `rank_board`;
this probe, written independently, reproduces it to the case at `058b4a9`.

### A control that was written and deleted before the first run

*"A wrong line whose answer is `error` = 0."* It sits after the branch that
sends every `error` answer to the gap bucket and `continue`s, so **no input can
make it non-zero** — the exact failure `docs/conventions.md` records under *"a
control bucket over a classifier whose last arm is a default cannot fire"*. It
was replaced by C6, which can fire. Recorded rather than silently removed, so
the next reader does not reinvent it.

### A caption that was wrong for one run, and is corrected here

The first run printed *"a declaration blamed as its own dependency = 5,198
(must be 0)"*. The counter was measuring how often the self-exclusion **fired**
— a population — under a caption claiming it was a violation. A control that
reads 5,198 beside the words *must be 0* is worse than no control, because it
trains the reader to discount the whole block. It is now two lines: C2 (the
violation, 0, re-tested at the verdict) and C2′ (the population, 5,198). No
number in this document was measured under the wrong caption.

---

## The named mutations, each proven red before anything was concluded

Run on the whole corpus, not on a fixture, because the controls are corpus-level
counts. Each was applied, measured, and reverted.

| mutation | control | before | after |
|---|---|---:|---:|
| **M1** — use `rank_board`'s **inclusive** containment instead of proper | C1 | 0 | **0 — did not fire** |
| **M2** — scan **backwards** (ancestors, not descendants) | C1 | 0 | **15,993** |
| **M3** — delete the self-exclusion `continue` | C2 | 0 | **5,197** |
| **M4** — test the shape arm **before** the `any` arm | `check_classifier` | pass | **panics** |

**M1 is reported because it failed, and it changes what C1 is evidence of.** The
properness clause in the containment test is **inert** on this corpus: a leaf is
already unreachable because the forward `take_while` stops at the first later
line not inside the leaf, and for a leaf that is the very first one. So C1 is
pinned by the **leaf property**, not by properness. The clause is kept as a
statement of intent, and this paragraph exists so nobody cites it as a guard.
`docs/conventions.md`: *a guard no mutation can make observable is decoration,
and decoration that claims a safety property is worse than none.*

**M2 and M3 are the ones that matter, and they demonstrate the conventions rule
they were chosen for.** Under M2 the `ROOT` share moves 19.07% → 20.65%; under
M3 it moves 19.07% → 18.97% while `propagated/declaration` jumps 42.34% →
55.79%. **Every arithmetic control reads zero under both.** A1, A2, A3, A4 are
all still 0, all row sums still reconcile, and the report still looks correct —
because the defect moves lines *between* buckets rather than losing them. Only
C1 and C2, both pinned by construction, see it.

---

## What is *not* measured here, and is filed

- **The 8,016 `UNKNOWN` lines (21.26%).** An identifier that does not resolve in
  value position, or one whose declaration the walker rendered no line for. Not
  evidence of `ROOT` and not counted as such. `bd tsr-853`.
- **The naming bucket is a syntactic test, not a semantic one.** `is_nominal` /
  `is_structural` read the printed string; they cannot tell a type this port
  *has and cannot name* from one it computed differently and printed
  structurally. The 11,008 is therefore an **upper bound on the printer item**
  and a lower bound on nothing. `bd tsr-jle`.
- **The declaration blamed for `propagated/declaration` is the first rendered
  child of a name-leading declaration form**, which for every form in
  `name_leads()` is its name because the walker is preorder and those forms put
  the name before the type and the initialiser. Forms with no name at all
  (`ExportAssignment`) are deliberately excluded and reach `UNKNOWN` rather than
  `ROOT`. Not independently verified against the AST. `open`.
- **The reverse cascade for wrong rows is not measured.** `docs/conventions.md`
  measured a *gap* fix manufacturing 2.1 and 2.5 wrong lines per right one on
  two designs of the same item. The mirror question for a wrong row — how many
  currently-*right* lines a fix would break — is not asked by this probe and
  would need a before/after pair. `bd tsr-6b5`.
- **Whether the 15,966 `propagated/declaration` lines collapse when their
  declarations are fixed.** This page shows they are downstream; it does not
  show the multiplier. `open`.

## ADDENDUM 2026-08-06 — two items measured in one probe, both refused

Measured at `f67b8e5` by `examples/wrongflip.rs`, extended with **P, a
population pinned syntactically by node kind** (`fnexpr.rs` shape), so `|P|`
cannot move under the thing being measured.

| node kind | \|P\| | right | gap | wrong | unaligned | TS2563-excl |
|---|---:|---:|---:|---:|---:|---:|
| `ElementAccessExpression` | 13,773 | 431 | **13,141** | 90 | 111 | **10,000** |
| `ArrayLiteralExpression` | 4,307 | 1,324 | 1,149 | **1,773** | 61 | 1 |
| `ObjectLiteralExpression` | 7,252 | 4,585 | 1,691 | 725 | 251 | 0 |
| `PropertyAccessExpression` | 24,072 | 6,914 | 15,215 | 1,621 | 322 | 0 |

`TS2563-excl` is **printed as its own column and never netted away**. Those
lines are upstream's `errorType` (`flow.go:81`), ADR-0038's ceiling. A silent
exclusion would read as *"we covered everything"*.

### Item 1 — `ElementAccessExpression`: 13,141 gap, 786 actionable

Split by what the **receiver** did. Every arm is a positive test;
`NotRendered` is the default, and `Right` — the loaded arm, the one meaning
*"we have the type and still cannot index it"* — is positive.

| receiver | all | TS2563 excluded |
|---|---:|---:|
| **RIGHT — we have the type and cannot index it** | **786** | **786** |
| gapped — symptom | 1,403 | 1,403 |
| wrong — symptom | 10,928 | 928 |
| unaligned | 24 | 24 |
| not rendered | 0 | 0 |
| TOTAL | 13,141 | 3,141 |

**10,000 lines (76.1%) are the TS2563 ceiling.** The residual is 3,141 — which
matches the coordinator's estimate — but **only 786 of it is this file's own
work**; 2,331 is propagated from a receiver that gapped or is wrong.

**786 lines = 0.164 gradient points**, and the receiver types behind them are a
long tail with no head (`bd tsr-65p`):

```
  73 T        32 string[]   29 any[]    29 number[]   24 typeof N
  20 Record<number, boolean>  19 U      19 string     18 T[]   18 this
```

The two largest are **generic type parameters**. This is not an indexing item;
it is generic instantiation seen through an index, and it lives outside this
workstream. **Refused.**

### Item 2 — `ArrayLiteralExpression`: 78.7% ROOT, and it still refuses

The cause split, run as I proposed in `tsr-ejp`. **The expectation was wrong and
in the interesting direction.**

| kind | wrong lines | ROOT | descendant | sibling | **ROOT share** |
|---|---:|---:|---:|---:|---:|
| `ArrayLiteralExpression` | 1,773 | **1,396** | 152 | 225 | **78.7%** |
| `ObjectLiteralExpression` | 725 | 76 | 457 | 192 | **10.5%** |

Corpus-wide the wrong bucket is **19.07% ROOT**; `ArrayLiteralExpression` is
**78.7%**, and `ObjectLiteralExpression` is the mirror image at 10.5%. Both the
coordinator and I predicted a small share. The **mechanism** is in
`array_literals.rs` and explains it: `check_array_literal` returns `error` the
moment any element is `error`, so a *gapping* element makes the literal a gap
and never a wrong answer. What survives to be wrong therefore has all-right
elements almost by construction. The split is not tautological — 152 lines do
have a *wrong* descendant — but the high ROOT share is a property of the
existing guard, not evidence of available work.

**And the 1,396 does not survive its own detail.** Exact substitutions:

```
  512  upstream `E[]`               ours `error[]`
  179  upstream `undefined[]`       ours `never[]`
   72  upstream `[number, number]`  ours `number[]`
   69  upstream `[]`                ours `never[]`
   56  upstream `[number, string]`  ours `(string | number)[]`
   45  upstream `[number]`          ours `number[]`
   38  upstream `[string, number]`  ours `(string | number)[]`
   34  upstream `[number, number, number]`  ours `number[]`
```

Three populations, none of them buildable here:

1. **512 lines (36.7%) are one case.** `compiler/enumLiteralsSubtypeReduction`,
   `E[]` against `error[]` — a gap that escaped into a printed type. Its case
   residual is 513, so closing it flips **one** case. Top-1 36.7% and this is
   the row evaporating on the concentration check, exactly as three gradient
   rows did in `checker-notes-rank.md` §3.
2. **591 lines (42.3%) are the tuple family.** Upstream infers a **tuple**
   where we infer an array: `[number, number]` against `number[]`,
   `[number, string]` against `(string | number)[]`. An array literal becomes a
   tuple only from a contextual type, a `const` assertion, or rest-parameter
   inference — `contextual.rs`, which this workstream does **not** own. Same
   boundary finding as the `symbols.rs` arm, and reported rather than crossed.
   `bd tsr-un1`.
3. **~290 lines (20.8%) are same-shape, different-text** — scattered element-type
   differences across 288 cases. **0.061 gradient points.**

Concentration over the whole 1,396: 288 cases, top-1 **36.7%**, top-10 49.8%,
and it would **finish 4 cases** at the optimistic bound.

**Refused.** After removing the one-case leak and the tuple family that belongs
to another workstream, what is left in `array_literals.rs` is ~290 lines across
~288 cases — 0.061 points, roughly one line per case.

### Spellability, both legs stated

For item 2 the naming split was taken on the baseline's **verbatim** right-hand
side, per the rule that landed in `3f140c2`: `error leaked` 515 (36.9%),
`different shape` 591 (42.3%), `same shape different text` 290 (20.8%), and
**naming failures 0 (0.0%)**. So R3 passes — but that measures only *upstream's*
leg. **Our leg is again predicted, not measured**, because no counterfactual was
run; the refusals do not rest on it, since both fail on size by 5–20×.

### On pre-registration, for a refusal

Both bars here were written after the numbers were visible, which would be
disqualifying for a **build**. It is not for a **refusal**: pre-registration
exists to stop a number being rationalised into a build, and the failure
direction is asymmetric. Both items miss any plausible bar by 5–20×, so the
post-hoc objection cannot change the verdict. Recorded so the asymmetry is
argued rather than assumed.

### Controls

| control | reads | pinned by |
|---|---:|---|
| **C7** an `ElementAccess` whose receiver line **precedes** it | **0** | **construction** — the walker is preorder, `a[b]` emits `a[b]`, then `a`, then `b` |
| C7′ the receiver line follows it | 13,141 | the mirror; the arm is fully reachable |
| C1 leaf classified `propagated/descendant` | 0 (mirror 3,288) | construction |
| C2 a line blaming its own declaration | 0 (mirrors 5,200 / 21,166) | construction |
| C3 a `finishes` in a case with an unaligned line | 0 | construction |
| C6 cases with residual 0 | 2,202 | another instrument (gate 2,173 + the known +18, now +29 after two merges) |
| A1–A4 | 0 | arithmetic |

## Superseded numbers

None yet. When one on this page is corrected, it gets a dated header here rather
than a silent edit (`CLAUDE.md`).
