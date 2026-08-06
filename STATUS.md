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

Measured at **`3299f53`**, 2026-08-06.

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
| **`checker_types`** | **2,270/9,538** | **23.80%** | **gradient 63.34%** — the target |
| `diagnostics` | 80/5,488 | 1.46% | **structurally blocked**, see below |

### `checker_types`, the number the project is steered by

```
303,366 / 478,954 assertion lines = 63.34%
  right 303,366 | gap ~128,694 | wrong ~37,678
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
today                    303,366 / 452,954 = ~67.0% of reachable
80% of the full          383,163 lines  =  ~84.6% of reachable
gap to 80%               +79,797 lines
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
disjuncts), object-literal spread, `getMergedSymbol`.

Deliberately **not** ported, each with a reason on record: the evolving-array
`x.push(e)` widening (53 lines, all already wrong); `hadErrorBaseline`
([ADR-0039](docs/adr/0039-the-any-that-upstream-prints-is-the-baseline-writers-decision.md));
rendering `any` for `errorType` (ADR-0038).

---

## 4. What is next — the ranked board

**Unmeasured items are ranked above measured refusals on purpose: the cheapest
thing available is a row nobody has spent a cycle refusing yet.**

| # | item | own-root lines | file | state |
|---|---|---:|---|---|
| 1 | `BinaryExpression` | **1,418** | `binary.rs` | **never measured, unowned** |
| 2 | `NewExpression` | **1,049** | `calls.rs` | **never sized alone** — only ever folded into the call row |
| 3 | `ArrayLiteralExpression` | **637** | `array_literals.rs` | **never measured, unowned** |
| 4 | `tsr-jle` — wrong lines failing on **naming**, not typing | 11,008 (upper bound) | printer | never split |
| 5 | `tsr-n23` — parameter lines answered `any` **and wrong** | 1,809 | `symbols.rs` | in no gap histogram |
| 6 | `ArrowFunction` | 1,341 | `expressions.rs` | 51.7% own root |
| 7 | `ConditionalExpression` | 295 | `expressions.rs` | |
| 8 | JSX | 417 | jsx | |

**`ParenthesizedExpression` is 0.0% own root — there is no work item there.**
Confirmed twice by independent instruments.

### The three blockers that gate everything downstream

| blocker | blocks | state |
|---|---|---|
| qualified naming (`tsr-awa`) | `tsr-4qx` (~5,161), 1,318 of its own | mechanism **measured at 90.7%**, build refused — see §5 |
| `tsr-4qx` instantiated generics | ~5,161 | blocked on `tsr-awa`; `type_reference_text` bakes an unqualified name at type *creation* |
| call resolution | 48.8% of contextual typing, the IIFE rows | refused at **68.3% against a 70% bar — by 85 lines** |

**`members.rs` is done** until those land. Its own lookup is **104 lines**. That
was registered as a prediction before measurement and confirmed.

---

## 5. Refused, with the number that refused it

**Do not rebuild these without new evidence. Each cost a measured cycle.**

| item | population | why refused |
|---|---:|---|
| call resolution | 18,294 | spellability **68.3%** vs 70% bar — 85 lines short |
| contextual typing | 2,082 + 1,809 wrong | **86% entangled**, 48.8% behind call resolution |
| qualified naming build | 1,318 | 90.7% accurate on target row; counterfactual **lost 3,202 lines, regressed 753 cases** |
| element access | 1,590 | **59.3% want `any`** over the corrected population; refused 3× |
| `TemplateExpression` | 1,036 | cheap leg **not separable** — upstream's `evaluate` is a syntactic folder consulting no types |
| module object (`tsr-6ph`) | 3,539 | 2.1 and 2.5 wrong per right, two designs |
| ALIAS row | 5,207 | convertible set and spellable set are **disjoint** |
| `ArrayLiteral` wrong bucket | 1,773 | 36.7% one case; 42.3% is tuple inference in `contextual.rs` |
| wrong bucket case-flips | 37,709 | **81% symptom**; best actionable row flips 37 cases |
| `hadErrorBaseline` | 40,759 | ADR-0039 |

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
| 2026-08-06 | `3299f53` | **63.34%** | **2,270** | **+2.25 pts, +10,761 lines** | export-marker link (+2,265), `this` parameter (+2,733), `super` (+838), `@lib`/`@noLib` harness fidelity (+431), unit-return widening (+1,188), `getApparentType` (+973), `getMergedSymbol` (+430), `autoArrayType` (+1,005), object spread (+74) |

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
