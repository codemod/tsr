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

Measured at **`856972a`**, 2026-08-06 (third session).

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
| **`checker_types`** | **2,409/9,538** | **25.26%** | **gradient 68.58%** — the target |
| `diagnostics` | 80/5,488 | 1.46% | **structurally blocked**, see below |

### `checker_types`, the number the project is steered by

```
328,472 / 478,954 assertion lines = 68.58%
  right 328,472 | gap 99,637 | wrong ~40,791   (wrong = 41,391 at 0fe102a − 600 measured Δ)
```

**The previous “wrong ~37,252” was stale and is corrected, and it was worth a
worktree to find out.** `wrongflip.rs` run at the pre-build commit `2d490b8`
reads **41,286**, so the `tsr-4qx` build's true wrong delta is **+105** — which
is also exactly what the registered leg-4 expression (−Δgap − Δright over
aligned lines) derived. The first post-build reading, 41,391 against the stale
37,252, looked like +4,139 and would have failed the leg by 20 lines; a
cross-instrument, cross-session subtraction is not a measurement.

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
| firmly unreachable (`examples/ceiling.rs` at `0fe102a`) | **2,202** | 0.46 |
| ~~unreachable, best estimate~~ | ~~26,000~~ | ~~5.4~~ |
| ~~firm upper bound~~ | ~~35,508~~ | ~~7.41~~ |

**Corrected a second time, 2026-08-06 (third session), from ~26,000 down to
2,202 — and the previous correction had itself quadrupled the estimate the
other way.** The ceiling's premise carried an unstated assumption: that a line
where upstream's answer is a rendered `errorType` (`any`) can never be matched
because this port refuses to print `any` for a gap. The `tsr-4qx` build showed
the assumption false: `Array<any>`'s instantiated index signature *honestly
computes* `any` on 10,000 `largeControlFlowGraph` element accesses, and the
computed answer coincides with upstream's bail-out. A ceiling built from
"upstream's answer is errorType" is an upper bound only on lines this port
*also* fails to compute — which is not a stable population. Treat `ceiling.rs`'s
attributed count as the only firm figure and expect it to move.

**Consequences for any target:**

```
reachable denominator   476,752 of 478,954
today                    326,206 / 476,752 = 68.42% of reachable
80% of the full          383,163 lines  =  80.37% of the reachable
gap to 80%               +56,957 lines
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
| **checker** | **68.11% of lines** | the mountain; §4 and §5 |
| transformers | **not started** | |
| diagnostics | **not started, and blocked** | §1 |
| language service / LSP | **not started** | |

### Inside the checker — what has an arm

Landed and measured this cycle: cross-file alias resolution, export-marker symbol
link, `getApparentType` for primitives, `autoArrayType` for `const x = []`,
unit-return widening, `this` with a written `this` parameter, `super` (both
disjuncts), object-literal spread, `getMergedSymbol`, **the `&&` arm of
`checkBinaryLikeExpression`** (`5290e1a`, +958 lines, 0 lost), the
**instantiation depth/count guard** (`40970d7`, `checker.go:22111`,
corpus-neutral by measurement), and **instantiated generic members**
(`0fe102a`, `tsr-4qx`: +12,357 lines, 0 lost, +26 cases — property access,
element access, index signatures and the relater all substitute through the
`get_type_of_property_of_type` seam).

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

**Rewritten 2026-08-06 from `examples/depend.rs`, which walks declaration edges
and now splits every root by whether the baseline wants `any`** — ADR-0038's
ceiling, which no root histogram can see. The `want-any` column is why this
board looks nothing like the previous one.

| root of the gap | lines | want `any` | **reachable** | cases | top-1 |
|---|---:|---:|---:|---:|---:|
| **`a.b` — receiver types, lookup fails** | 10,494 | 45.2% | **5,755** | 1,091 | 13.9% |
| `BinaryExpression` (probe has no step arm) | 7,542 | 16.5% | 6,295 | 814 | 19.9% |
| unresolved **value** name | 6,960 | **79.5%** | 1,425 | 858 | 16.2% |
| **`CallExpression` — callee types** | 6,863 | **11.4%** | **6,084** | 1,063 | **4.4%** |
| **the member *name* of `a.b`** | 5,240 | 40.0% | **3,144** | 1,016 | 12.8% |
| `NewExpression` — callee types | 4,078 | 13.0% | 3,548 | 487 | 6.1% |
| `ElementAccessExpression` | 1,391 | 21.2% | 1,096 | 221 | 7.8% |

**Re-measured at `856972a` after the two builds of the third session.**
Element access collapsed 12,544 → 1,391 at the first build (§2); the
property-access rows fell 12,921 → 10,494 and 6,047 → 5,240 at the second,
whose gains also made the **call row grow** 5,303 → 6,863 with
`compiler/promiseType` as its new top case — lines that used to gap at the
member access now gap at the *call through it*, because
`resolve_call_signature` deliberately refuses an instantiated signature type
rather than resolving its uninstantiated symbol. That population is
`bd tsr-1uz` and it is the cleanest item this session leaves behind: the
recorded `Vec<Signature>` is already on the type, and upstream's shape —
`getSignaturesOfType` reads the *type*, not the symbol (`checker.go:18959`) —
says exactly where the arm goes.

| # | item | reachable | file |
|---|---|---:|---|
| 1 | **calls through instantiated members** (`bd tsr-1uz`): one arm in `resolve_call_signature` reading `Checker::signature_types` off the callee type before falling back to the symbol — upstream's `getSignaturesOfType` shape. Single-signature members are the cheap half; overload sets fold into item 2 | inside the 6,084 call row; unmeasured — register a rule first | `calls.rs` |
| 2 | **call resolution** — grew to the second-largest reachable row and its ceiling *dropped* to 11.4%. Needs a **counterfactual**, which is the expensive thing R2′ existed to avoid | **6,084** | `calls.rs` |
| 3 | **strict-flag gating of the remaining strict-only arms** — optionality's added `undefined` (`bd tsr-e10`, whose hidden variable is now named and plumbed), the `&&` arm's non-strict widening, `unknown` narrowing. Each is now a small measurable change where before the flag did not exist | `tsr-e10` alone ≤483 | `optionality.rs` |
| 4 | `BinaryExpression` roots — needs a step arm in `depend.rs` before it can be ranked at all | ≤6,295 | measurement |
| 5 | `tsr-y9x` — 16 lines printing `undefined`/`error` where upstream prints `any` | 16 | resolver |

**Landed or closed 2026-08-06 (third session):** the depth/count guard
(`40970d7`, corpus-neutral alone), member instantiation (`0fe102a`, +12,357),
signature instantiation plus `strictNullChecks` plumbing (`856972a`, +2,266),
`tsr-4qx`/`tsr-0hc` closed with scores, `tsr-mcd` closed earlier as a symptom
count.

**Item 0 was added after the board was written and then refused the same day.**
Sized at `0a1fbdd` by `examples/subtypes.rs`: the mechanism would break **255
lines that are right today** against at most 263 changed, and only **500 of the
2,146** structured wrong lines are even its population — the rest are narrowing
(483), printing (438) and answers that differ outright. What replaced it on the
board is two items that fell out of that sizing and are better specified than it
ever was. `docs/architecture/checker-notes-armsplit.md` §9.

### Is 80% reachable at the implied rate?

The arithmetic, restated at `856972a`:

- Distance to 80%: **+54,691 lines** (was +69,314 at the session's start).
- This session: **+14,623**, of which 10,000 is the one-case windfall §2
  records — the repeatable rate is nearer **+2,300 per build**.
- The board's measured items sum to ~6,800; items 1 and 2 (the call rows) are
  the unmeasured mass, and this session moved their prerequisite into place.

The gap is not a list of missing expression arms. **22,596 lines were measured
as `ROOT/own-rule` and this cycle took the three largest of those rows apart:
one landed at 958, one is one case, one is the call item.** What is left is
concentrated behind five capabilities — assignability, call resolution,
qualified naming, contextual typing, and (named by the `tsr-4qx` survivor
analysis, third session) **structured signature types** — and every one of them
is a subsystem rather than a row.

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
| **annotation reuse, naive form (`tsr-a2c`)** | 740 | **6,736 right lines broken against 740 converted — 9.1 lost per gained**, worse than every refusal below. And the 740 is a string coincidence: its head is `string \| undefined` → `string`, i.e. `tsr-e10`'s optionality population, not reuse |
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
| `examples/wrongdelta.rs` | **`casedelta`'s sibling for the wrong bucket** — raw joinable `want`/`got` dump; two runs over a `git stash` attribute every gap→wrong line, which `casedelta` cannot see by construction |
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
| 2026-08-06 | `856972a` | **68.58%** | **2,409** | **+0.47 pts, +2,266 lines, 46 lost, +19 cases, 0 regressed** | **signature-typed members instantiate** (`tsr-0hc`): `Signature` side-table + `instantiateSignature` arm + **`strictNullChecks` plumbed** (union constructor only). First run **failed its leg 4** (+538 wrong vs 381) and the new `wrongdelta.rs` attributed it: instantiated lib signatures rendered under the wrong strict mode; the harness default was then measured off the baselines (strict-ON) after a wrong first guess lost 1,221 lines. Δwrong finished at **−600**. Residual: 264 annotation-reuse lines (`tsr-a2c` note). Calls through instantiated members filed as `tsr-1uz` |
| 2026-08-06 | `0fe102a` | **68.11%** | **2,390** | **+2.58 pts, +12,357 lines, 0 lost, +26 cases** | **instantiated generic members** (`tsr-4qx` steps 3+4, one change) behind the **instantiation depth/count guard** (`40970d7`, corpus-neutral alone, `tsr-el3.2` half). Scored against a bar registered at `2d490b8`; all legs passed, the concentration falsifier fired and is decomposed in `checker-notes-inst.md` — 10,000 of the gain is `largeControlFlowGraph` via `Array<any>` index signatures, which **collapsed ADR-0038's ceiling estimate to 2,202 firm** (§2). Ex that case: +2,357 diffuse over 147 cases. Also corrected §1's stale wrong-bucket figure by re-running `wrongflip` at the pre-build commit in a worktree: 41,286 → 41,391, Δ+105, confirming the registered leg-4 expression exactly |
| 2026-08-06 | `d356450` | **65.53%** | **2,364** | **+0.97 pts, +4,645 lines, 0 lost** | **an unresolved type reference prints the name that was written** (`tsr-eep`) — upstream reports `TS2304 Cannot find name` *and renders the name*; answering `errorType` was the divergence |
| 2026-08-06 | `3b7fa44` | **64.56%** | **2,335** | **+0.90 pts, +4,319 lines, +60 cases** | **namespace exports resolve** (`tsr-56r`) — `resolve_name` never read a namespace's `exports`, and its locals lookup never filtered by meaning, so **exporting a declaration made it unresolvable**. Found by `examples/depend.rs` (`tsr-550`), the first instrument to walk declaration edges rather than span edges |
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
