# Project status

**The live dashboard. Updated at the end of every session, by whoever ran it.**

[PLAN.md](PLAN.md) is the roadmap — scope, phases, architecture, decisions, and it
changes rarely. **This file is the state**: what is ported, what the numbers are,
what is next, and what has already been refused and must not be re-derived. If the
two disagree about status, this file wins and `PLAN.md` needs a correction.

Rules for keeping it honest, which are the same rules the rest of the project runs on:

- **Every number carries the commit it was measured at.** A number without one is
  a number from an unknown compiler.
- **A refused item stays on this page with the number that refused it.** Deleting
  it invites the next session to spend a cycle rediscovering the same negative.
- **Correct in place and say it was corrected.** Silent edits destroy trust in
  every other number here.
- **Do not quote a row's population as work.** A population is a ceiling; the
  conversion is a different and usually much smaller number.

---

## 1. Where the port stands

Measured at **`a371ec8`**, 2026-08-06.

| suite | passed | rate | note |
|---|---:|---:|---|
| `corpus_ingest` | 12,444/12,444 | 100% | |
| `baseline_resolution` | 12,444/12,444 | 100% | |
| `scanner_termination` | 12,444/12,444 | 100% | |
| `scanner_clean_files` | 5,031/5,031 | 100% | |
| `module_resolution` | 95/95 | 100% | |
| `file_loader` | 96/96 | 100% | |
| `printer_round_trip` | 11,681/11,737 | 99.52% | |
| `parser_typescript` | 5,000/5,031 | 99.38% | |
| `binder_symbols` | 8,292/8,459 | 98.03% | |
| `isolated_declarations` | 13/15 | 86.67% | |
| `dts_shape` | 618/912 | 67.76% | |
| `dts_emit` | 161/339 | 47.49% | |
| `parser_reachable_target` | 5,031/10,570 | 47.60% | |
| `dts_reachable_target` | 495/1,162 | 42.60% | |
| **`checker_types`** | **2,275/9,538** | **23.85%** | **gradient 63.66%** — the target |
| `diagnostics` | 80/5,488 | 1.46% | **structurally blocked**, see below |

### `checker_types`, the number the project is steered by

```
304,885 / 478,954 assertion lines = 63.66%
  right 304,885 | gap ~127,736 | wrong ~37,117
```

**The gate is whole-baseline and positional; the gradient is per-line. They are
nearly orthogonal** — a change can add 2,733 lines and flip zero cases. Say which
you are quoting.

### `diagnostics` cannot be moved by `.types` work

`tsr-checker` emits no diagnostics at all. Upstream produces them from a **second
traversal** (`checkSourceFile` → `checkSourceElement`), and this port built
upstream's *query* road (`getTypeOfNode`) and none of the traversal road. See
[ADR-0040](docs/adr/0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md).
Nothing in the checker's type answers will move this suite.

---

## 2. The ceiling — 100% is not reachable and never was

[ADR-0038](docs/adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md).
Upstream renders `errorType` as **`any`**; this port prints **`error`** so that a
gap (we could not compute this) stays distinguishable from a wrong answer (we
computed something else). Every line where upstream's answer *is* an `errorType`
is unmatchable however good the checker gets.

| | lines | points |
|---|---:|---:|
| unreachable, best estimate | **~26,000** | ~5.4 |
| firm upper bound (`examples/ceiling.rs`) | 35,508 | 7.41 |
| of which `largeControlFlowGraph` under `TS2563` | 20,000 | 4.18 |

**Corrected 2026-08-06 from ~6,000 — a 4× error.** The original figure enumerated
*unresolved names* and never counted upstream *bailing out* of control-flow
analysis. The decision is unaffected and strengthened.

**Consequences for any target:**

```
reachable denominator   ~452,954 of 478,954
today                    304,885 / 452,954 = ~67.3% of reachable
80% of the full          383,163 lines  =  ~84.6% of reachable
gap to 80%               +78,278 lines
```

---

## 3. What is ported

Per-crate, by what the conformance suites actually assert — not by what exists.

| subsystem | state | evidence |
|---|---|---|
| scanner | **done** | 100% termination and clean-files |
| parser | **done for TypeScript** | 99.38%; `parser_reachable_target` is a wider target set |
| binder | **near done** | 98.03%; `getMergedSymbol` redirect landed 2026-08-06 |
| module resolution | **done** | 95/95, `file_loader` 96/96, [ADR-0041](docs/adr/0041-the-checker-asks-its-program-for-a-module.md) |
| printer | **near done** | 99.52% round-trip |
| declaration emit | **partial** | `dts_shape` 67.76%, `dts_emit` 47.49% |
| **checker** | **63.34% of lines** | the mountain; §4 and §5 |
| transformers | **not started** | |
| diagnostics | **not started, and blocked** | §1 |
| language service / LSP | **not started** | |

### Inside the checker — what has an arm

Landed and measured this cycle: cross-file alias resolution, export-marker symbol
link, `getApparentType` for primitives, `autoArrayType` for `const x = []`,
unit-return widening, `this` with a written `this` parameter, `super` (both
disjuncts), object-literal spread, `getMergedSymbol`, and **the `&&` arm of
`checkBinaryLikeExpression`** (`5290e1a`, +958 lines, 0 lost).

Deliberately **not** ported, each with a reason on record: the evolving-array
`x.push(e)` widening (53 lines, all already wrong); `hadErrorBaseline`
([ADR-0039](docs/adr/0039-the-any-that-upstream-prints-is-the-baseline-writers-decision.md));
rendering `any` for `errorType` (ADR-0038).

---

## 4. What is next — the ranked board

**Rewritten 2026-08-06 from `docs/architecture/checker-notes-armsplit.md`.
Three of the previous board's top four rows were not what their numbers said,
and the previous ordering rule — *"unmeasured items rank above measured
refusals, because the cheapest thing available is a row nobody has refused
yet"* — is **withdrawn**. It is sound about cost and silent about value, and it
put an item worth ~1,004 diffuse lines in position 4 on a figure of 11,008.
The replacement rule: **rank by the conversion, and where the conversion is
unknown, rank by how cheap it is to find out.**

### The ranking

| # | item | converts | file | the rule that would license it | what would falsify the estimate |
|---|---|---:|---|---|---|
| 0 | ~~`removeSubtypes` + a `Subtype` arm~~ (`tsr-eak`) | **REFUSED 2026-08-06** | — | — | 255 right lines broken against ≤263 changed — see §5 |
| 0 | ~~Union parenthesisation~~ (`tsr-xm9`) | **DONE `39a3853`, +481, 0 lost** | — | — | — |
| 0b | **Optionality in a declaration-name position** (`tsr-e10`, **rediagnosed**) | ≤483, of which **204 confirmed** | `optionality.rs` / `symbols.rs` | **none yet** — partition the 204 by declaration shape and by the case's `strictNullChecks`, then read the baselines per bucket | the baselines contradict a blanket rule: `classWithOptionalParameter` records `>x : string \| undefined`, other cases record `>opt : number` 19× |
| 0c | ~~Union **constituent order**~~ (`tsr-bgz`) | **DONE, +81/−1** | — | the reshape it required was **already stored** in `type_reference_targets` | — |
| 1 | **`new C()` — the lib `*Constructor` arm** | unsized; **not 1,074** | `calls.rs` | **R2′ 75.9% on 1,074 lines, 229 cases, top-1 15.1%** — passed its registered test on 2026-08-06 | the typed arrays in it are generic instantiations (`tsr-4qx`, blocked). Size the `*Constructor` population **minus** those before quoting anything — one more bucket in `callres.rs` |
| 2 | ~~Re-take the call row's bar with `new` scored separately~~ | **run 2026-08-06** | — | — | see §5 — R2′ has stopped discriminating and the answer is a counterfactual |
| 3 | `tsr-n23` + contextual typing, as **one** cross-file item | 2,082 gap **+ 1,809 wrong** | `symbols.rs` **and** `signatures.rs` | 48.8% of it sits behind call resolution — item 2 gates this | a build confined to either file converts half of each function's lines, which is the measured reason it is one item |
| 4 | `ArrowFunction` | ≤ 1,341, and **17.8%** of the lines that stop gapping matched last time | `expressions.rs` | needs a *new* rule: the shape test read 99.6% and the counterfactual 4.6 wrong per right | the failure is parameter types, i.e. item 3 — this row may be item 3 wearing a different node kind |
| 5 | `undefined \| null` sorts backwards (`tsr-iiu`) | unsized, **two-line repro** | `unions.rs` | count `null \| undefined` against `undefined \| null` in the baselines first | it is one pair and not a class, in which case it is small and still free |
| 6 | `ConditionalExpression` | ≤ 295 | `expressions.rs` | unmeasured | |
| 7 | JSX | ≤ 417 | jsx | unmeasured | |

**Item 2 was run on 2026-08-06 and did not decide what it was meant to decide.**
Scored alone the call half clears the bar at **70.6%** and `new` does not at
64.8% — so the aggregate was `new` holding the call half under. But the call
half is itself **58.9% and 76.3%**, so there is no level at which "the call row"
is one population, and a bar registered against a row that turns out to be a
mixture is void. `callres.rs`'s standing registration settles the rest: **R2′ is
necessary and not sufficient and no value of it licenses a build.** What the
call row needs is a counterfactual, which is the expensive thing R2′ existed to
avoid paying for. Full working in
[`docs/architecture/checker-notes-armsplit.md`](docs/architecture/checker-notes-armsplit.md) §8.

### Off the board, with the number that took it off

| was | ranked | now | why |
|---|---|---|---|
| `BinaryExpression` | 1 (1,418) | **partly done, rest blocked** | 659 `&&` lines **landed** (`5290e1a`, +958). Of the remainder: 340 want `any`, 252 need tuples *and* destructuring patterns, 454 (`\|\|`, `??`) need assignability |
| `tsr-jle` naming | 4 (11,008) | **gone** | **10,000 of 11,004 are `compiler/largeControlFlowGraph`** — ADR-0038's ceiling. Real size 1,004 over 362 cases in 566 pairs, head 21 lines. `bd tsr-q54` |
| `ArrayLiteralExpression` | 3 (637) | **blocked** | 602 of 604 own-root lines are the object-reduction guard; **355 are one case**. Needs assignability. `bd tsr-rn4` |
| `ParenthesizedExpression` | — | **not work** | 0.0% own root, confirmed twice |

### The three blockers that gate everything downstream

Unchanged, and now joined by a fourth that this cycle's measurements kept
arriving at.

| blocker | blocks | state |
|---|---|---|
| ~~**`removeSubtypes`**~~ — **refused, not a blocker worth clearing** | `\|\|` 358, `??` 96, `ConditionalExpression` 295, `ArrayLiteral` 355, return-inference from a body, `intersections.rs` | **CORRECTED 2026-08-06, same day.** I wrote *"assignability, newly identified as a shared blocker"* from three refusals' **stated reasons** — the exact "a prerequisite quoted three times is not established" trap I had written a convention about that morning. Grepped: `relater.rs` **exists and is not a stub** — `is_simple_type_related_to`, a structural arm with a results cache and cycle closure, composite handling. `Relation` has one variant, `Assignable`, and the file says at `:115` and `:231` that it was shaped so **"adding `Subtype` later is an arm rather than a refactor"**. The blocker is `removeSubtypes` in `unions.rs` plus that arm — a far smaller and better-specified item |
| qualified naming (`tsr-awa`) | `tsr-4qx` (~5,161), 1,318 of its own | mechanism **measured at 90.7%**, build refused — see §5 |
| `tsr-4qx` instantiated generics | ~5,161 | blocked on `tsr-awa`; `type_reference_text` bakes an unqualified name at type *creation* |
| call resolution | 48.8% of contextual typing, the IIFE rows, and `new` | **the R2′ refusal no longer stands on its stated grounds** (§5). Its next step is a counterfactual, not another probe |

**`members.rs` is done** until those land. Its own lookup is **104 lines**. That
was registered as a prediction before measurement and confirmed.

**Item 0 was added after the board was written and then refused the same day.**
Sized at `0a1fbdd` by `examples/subtypes.rs`: the mechanism would break **255
lines that are right today** against at most 263 changed, and only **500 of the
2,146** structured wrong lines are even its population — the rest are narrowing
(483), printing (438) and answers that differ outright. What replaced it on the
board is two items that fell out of that sizing and are better specified than it
ever was. `docs/architecture/checker-notes-armsplit.md` §9.

### Is 80% reachable at the implied rate?

**Not at this session's rate, and the previous session's rate should not be
carried forward either.** The arithmetic, stated plainly:

- Distance to 80%: **+78,839 lines**.
- Last session: +10,761 lines. This session: **+958**.
- Everything now ranked and unblocked on this board sums to **under 3,000 lines.**

The gap is not a list of missing expression arms. **22,596 lines were measured
as `ROOT/own-rule` and this cycle took the three largest of those rows apart:
one landed at 958, one is one case, one is the call item.** What is left is
concentrated behind four capabilities — assignability, call resolution,
qualified naming, and contextual typing — and every one of them is a subsystem
rather than a row.

So the honest statement is: **80% is reachable and it is not reachable by
ranking rows.** What would have to change is that a session takes on one of the
four blockers as its whole deliverable, accepting that it converts nothing until
it is finished. `casedelta` and the pre-registered-rule discipline make that
safe to attempt; the row-by-row board does not make it *unnecessary*, which is
what four cycles of ranking have now established.

---

## 5. Refused, with the number that refused it

**Do not rebuild these without new evidence. Each cost a measured cycle.**

| item | population | why refused |
|---|---:|---|
| call resolution | 18,294 | ~~spellability **68.3%** vs 70% bar — 85 lines short~~ **CORRECTED 2026-08-06.** That figure was taken at `058b4a9`; re-run unchanged at `d75cf16` the same expression reads **69.4%, 34 lines short**. R2′'s numerator moves with the compiler, so a bar it crosses by tens of lines decides nothing. Split by row: **CALL 70.6% (+25), `new` 64.8% (−60), `InitCall` 58.9%, `ExprCall` 76.3%.** The refusal no longer stands on its stated grounds and R2′ has stopped discriminating — §4 item 2 |
| contextual typing | 2,082 + 1,809 wrong | **86% entangled**, 48.8% behind call resolution |
| qualified naming build | 1,318 | 90.7% accurate on target row; counterfactual **lost 3,202 lines, regressed 753 cases** |
| element access | 1,590 | **59.3% want `any`** over the corrected population; refused 3× |
| `TemplateExpression` | 1,036 | cheap leg **not separable** — upstream's `evaluate` is a syntactic folder consulting no types |
| module object (`tsr-6ph`) | 3,539 | 2.1 and 2.5 wrong per right, two designs |
| ALIAS row | 5,207 | convertible set and spellable set are **disjoint** |
| `ArrayLiteral` wrong bucket | 1,773 | 36.7% one case; 42.3% is tuple inference in `contextual.rs` |
| wrong bucket case-flips | 37,709 | **81% symptom**; best actionable row flips 37 cases |
| `hadErrorBaseline` | 40,759 | ADR-0039 |
| **`tsr-jle` naming** | 11,008 | **10,000 of 11,004 are one case** — `largeControlFlowGraph`, ADR-0038's ceiling. Real size 1,004 in 566 pairs, head **21 lines** |
| **`BinaryExpression` addition fallthrough** | 297 | **277 (93.3%) want `any`** — ADR-0038/0039 forbid it |
| **`BinaryExpression` arithmetic (bigint mixing)** | 43 | 42 of 43 want `any`, and **97.7% is one case** |
| **`BinaryExpression` destructuring assignment** | 252 | needs destructuring patterns **and** tuples — two unported subsystems |
| **`ArrayLiteral` own-root row** | 604 | 602 are the object-reduction guard; **355 are one case**; needs assignability |
| **`new C()`, the cheap design** | 1,052 | *strip `typeof` from the callee* exact-matches **23 of 1,052 — 2.2%**, and on 712 the callee is not `typeof X` at all |
| **`\|\|` and `??`** | 358 + 96 | both need `UnionReductionSubtype`. `&&` does not, which is why only `&&` landed |
| **`tsr-iiu` — `undefined \| null` prints backwards** | — | **NOT A DEFECT.** Upstream prints `null \| undefined` too — 6+3+2+2… baseline instances, the other order **zero**. I filed a defect against correct code from an expectation I never checked |
| **`removeSubtypes` (`tsr-eak`)** | 5 rows, ~1,100 quoted | **255 right lines broken vs ≤263 changed — 1.03 gained per lost at the ceiling**, worse than the 2.1 / 2.5 / 2.7 that refused three earlier items. And only **500 of 2,146** structured wrong lines are its population; 21,093 of 26,140 union lines carry no structured constituent and are outside it by construction |

---

## 6. Instruments

Built and maintained; **use them, do not rebuild them.**

| instrument | answers |
|---|---|
| `examples/gaproot.rs` | root/cause split — ranks **causes**, not symptoms |
| `examples/casedelta.rs` | **per-case joinable TSV.** A net hides a change that helps and harms at once |
| `examples/reconcile.rs` | a probe's denominator against the suite's |
| `examples/ceiling.rs` | the ADR-0038 unreachable bound |
| `examples/rank_board.rs` | the gradient board, `TERMINAL`/propagated split |
| `examples/wrongflip.rs` | the only cause split for **wrong** lines |
| `fnexpr` · `nameres` · `evolvearray` · `thisparam` · `receiver_gap` | per-workstream |

Five gates, all green before every commit:

```
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo run -p xtask -- anchors      # every upstream file:line resolves
cargo run -p xtask -- issue-ids    # every `bd <id>` cited in docs/ exists
```

---

## 7. Session log

Append one row per session. Keep it to what a future session needs.

| date | commit | gradient | cases | net | what moved it |
|---|---|---:|---:|---|---|
| 2026-08-05 | `058b4a9` | 61.09% | 2,173 | — | baseline for the session below |
| 2026-08-06 | `a371ec8` | **63.66%** | **2,275** | **+0.02 pts, +81 lines, −1** | `compareTypeNames` for type references (`tsr-bgz`) — the reshape the issue said it needed was already stored in `type_reference_targets`. The single loss is `bd tsr-a2c`, a different mechanism |
| 2026-08-06 | `39a3853` | **63.64%** | **2,275** | **+0.10 pts, +481 lines, 0 lost** | union-constituent parenthesisation (`tsr-xm9`) +353, and the same predicate fixing a pre-existing defect in `array_element_text` +128. Also this session: `removeSubtypes` sized and **refused**, `tsr-jle` and `tsr-iiu` withdrawn, the call row's bar re-scored with `new` separated |
| 2026-08-06 | `5290e1a` | **63.54%** | **2,275** | **+0.20 pts, +958 lines, +5 cases** | the `&&` arm of `checkBinaryLikeExpression` — the only unblocked arm in the board's top three rows. The session's main product is the **board rewrite**: `tsr-jle` fell 11,008 → 1,004, `ArrayLiteral` and `\|\|`/`??` were shown blocked on assignability, and `new` was sized alone for the first time |
| 2026-08-06 | `3299f53` | **63.34%** | **2,270** | **+2.25 pts, +10,761 lines** | export-marker link (+2,265), `this` parameter (+2,733), `super` (+838), `@lib`/`@noLib` harness fidelity (+431), unit-return widening (+1,188), `getApparentType` (+973), `getMergedSymbol` (+430), `autoArrayType` (+1,005), object spread (+74) |

**2026-08-06, second session — and a gate failure to record.** `5290e1a` was
committed with a **failing test**: `cargo test --workspace` was piped through
`head -30`, which cut it off at the 30th of 96 test binaries, and
`the_logical_operators_are_still_a_gap` — a test whose purpose is to go red when
`&&` lands — was below the cut. Fixed at `HEAD`. A gate you sampled is not a
gate you ran; reduce its output by counting, never by `head`.

**2026-08-06, second session:** two new instruments (`armsplit.rs`,
`namesample.rs`), one correction to a **doc comment that was acting as a
prerequisite** — `binary.rs` claimed `extractDefinitelyFalsyTypes` reaches
`getTypeFacts`; grepped on the declarations it does not, and that is what
separated `&&` from `||` and `??`. One pre-existing defect found by a unit test
and invisible to every gap histogram: `undefined | null` prints as
`null | undefined` (`bd tsr-iiu`). Three of seven test expectations in the new
file were written from intuition and were wrong; the port was right each time.

**2026-08-06 also corrected three instruments and one ADR**, which changed what
the project believes is worth building: `TERMINAL` was a default arm and is
16.41% not 34.92%; the wrong bucket is 81% symptom; ADR-0038's ceiling is
~26,000 not ~6,000. Six builds were refused on measured grounds.

---

## 8. Updating this file

**At the end of every session**, whoever ran it updates §1 (numbers + commit),
§4 (what moved off the board, what arrived), §5 (anything newly refused, with its
number), and appends one row to §7.

If a number here turns out to be wrong, **correct it and say so** — do not
silently edit. Three of this project's most expensive mistakes were numbers that
were true of a different population than the one they were quoted about.
