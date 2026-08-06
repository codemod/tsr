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

Measured at **`cf33aee`**, 2026-08-06 (fourth session).

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
| **`checker_types`** | **2,509/9,538** | **26.31%** | **gradient 69.59%** — the target |
| `diagnostics` | 80/5,488 | 1.46% | **structurally blocked**, see below |

### `checker_types`, the number the project is steered by

```
333,321 / 478,954 assertion lines = 69.59%
  right 333,321 | gap ~93,840 | wrong ~41,684   (41,189 at b00738d; +423, −58, +3, +127 by same-probe pairs through cf33aee)
```

**The wrong figure is carried forward by measured deltas, not re-derived.**
It was once quoted 4,000 lines stale, which nearly failed a bar by 20 lines: a
cross-instrument, cross-session subtraction is not a measurement. Re-run
`wrongflip.rs` at both ends of a pair if the number matters.

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
today                    332,570 / 476,752 = 69.76% of reachable
80% of the full          383,163 lines  =  80.37% of the reachable
gap to 80%               +50,593 lines
gap to 70%               +2,698 lines
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
| **checker** | **69.03% of lines** | the mountain; §4 and §5 |
| transformers | **not started** | |
| diagnostics | **not started, and blocked** | §1 |
| language service / LSP | **not started** | |

### Inside the checker — what has an arm

Landed across the three sessions to date, newest first:

| commit | what | net |
|---|---|---|
| `cf33aee` | nullable receivers strip, optional chains propagate `undefined` (`checker-notes-nnaccess.md`) | +590 |
| `9eaa2f1` | `t[0]` — a tuple's numeric-literal property is its element (`checker-notes-tuple.md` §8) | +103 |
| `0d56467` | the tuple arm of `compare_types` (`tsr-5ll`) — 64 wrong lines fixed; three bar legs fired and are overridden loudly, `checker-notes-tuple.md` §7 | +58 |
| `ff49871` | `typeof x` in type position (`tsr-4sc.10`), plus written-node reuse for `typeof` annotations in signature prints | +1,958 |
| `b00738d` | object-literal method members | +420 |
| `bf5681b` | plain tuple type nodes (74.9% of printed tuples; modifiers still refuse) | +1,227 |
| `c72ebf2` | narrowing for property/element references (`tsr-6ka`) | +58 |
| `2642e7b` | equality narrowing against `null`/`undefined` | +30 |
| `385fb60` | calls through instantiated members, default type arguments (`tsr-1uz`) | +405 |
| `856972a` | signature-typed members instantiate, `strictNullChecks` plumbed (`tsr-0hc`) | +2,266 |
| `0fe102a` | instantiated generic members (`tsr-4qx`) — property access, element access, index signatures and the relater all through one seam | +12,357 |
| `40970d7` | instantiation depth/count guard (`checker.go:22111`) | 0, by design |
| `d356450` | an unresolved type reference prints the written name (`tsr-eep`) | +4,645 |
| `3b7fa44` | namespace exports resolve (`tsr-56r`) | +4,319 |
| `5290e1a` | the `&&` arm of `checkBinaryLikeExpression` | +958 |
| earlier | cross-file aliases, export markers, `getApparentType`, `autoArrayType`, unit-return widening, `this` parameter, `super`, object spread, `getMergedSymbol`, union parenthesisation and ordering | +11,000 approx |

Deliberately **not** ported, each with a reason on record: the evolving-array
`x.push(e)` widening (53 lines, all already wrong); `hadErrorBaseline`
([ADR-0039](docs/adr/0039-the-any-that-upstream-prints-is-the-baseline-writers-decision.md));
rendering `any` for `errorType` (ADR-0038).

---

## 4. What is next — the scored board

Measured at **`b00738d`** by `examples/depend.rs`, re-confirmed unchanged by a
fresh run at `7cecc02` (fourth session; every row within noise). Two lists, because the
project's ordering rule has two halves: **rank by the conversion, and where the
conversion is unknown, rank by how cheap it is to find out.**

### 4.1 How the score is built, and what it is not

```
score = (reachable / effort) x feasibility
```

- **reachable** is *measured*: the root's gap lines minus its `want-any` share.
  It is a **ceiling, never a forecast** — this file's fourth rule. Observed
  conversion over the seven builds of the third session ran **15% to 57%** of
  the sized population (tuples 57%, property references 35%, member
  instantiation 18% ex-windfall, object methods 15%), so read a score as an
  *ordering*, not as a line count. The fourth session's `typeof` build then
  converted **122%** of its sized row (+1,958 against 1,600), because its
  mechanism — written-node reuse in signature prints — reached lines whose
  `depend.rs` root was not the `TypeQuery` node: a population is a ceiling
  *for the row it was measured on*, and a mechanism can turn out wider.
- **effort** is 1–5, anchored to builds that actually happened rather than to
  intuition: **1** = one arm on machinery that exists (tuples, object methods);
  **2** = a few arms plus new data (the six narrowing facts bits); **3** = a new
  side table or a reshape (signature instantiation); **4** = a subsystem with a
  partial already in place (contextual typing); **5** = a subsystem from
  scratch (overload resolution).
- **feasibility** is 0–1: are the prerequisites ported, and has the item been
  refused before *with a number*? This is the only judgement column, and it is
  the one to argue with.

### 4.2 The scored list

| score | item | reachable | eff | feas | file |
|---:|---|---:|---:|---:|---|
| **869** | **call resolution — overload sets.** The largest reachable mass and the sole owner of what `tsr-1uz` left behind, including every multi-signature instantiated member. Needs the relater's subtype relation, so it is genuinely a subsystem; the old R2′ refusal no longer stands (§5) but a **counterfactual** does. | 9,660 | 5 | 0.45 | `calls.rs`, `relater.rs` |
| **649** | **destructuring / binding patterns.** Zero references in the checker today. `BindingElement` cycles are the root. Unblocked by tuples landing, since array patterns need a tuple element list. | 2,781 | 3 | 0.70 | new module |
| **374** | **element access remainder.** Decomposed by `examples/elemgap.rs` at `ec981ae`: 757 lines have both sides typed. The tuple families landed at `9eaa2f1` (+103); the remainder is **"other receiver" families** — 204 string-literal lookup misses, 210 non-literal indexes without index info, 48 optional chains, 75 union receivers — each needing its own mechanism split before costing. | ~650 | 2 | 0.55 | `indexed.rs` |
| **230** | **JSX.** `JsxSelfClosingElement` 742 + `JsxElement` 570. Self-contained and entirely unported. | 1,312 | 4 | 0.70 | new module |
| **165** | **template literal types.** Refused once: the cheap leg is **not separable**, because upstream's `evaluate` is a syntactic folder consulting no types. Kept on the list because the row survived the session unchanged. | 1,237 | 3 | 0.40 | `declared.rs` |
| **147** | **`typeof` guard narrowing** (`bd tsr-q9g`) — re-priced twice at `8298d72`, both times by a probe before a build: access lines are only 27 (refmatch split), but `examples/idtypeof.rs` counts **440 identifier lines, 417 of them in the WRONG bucket** (we print the un-narrowed union; wants `string` 110, `number` 64, `never` 39) — the column no gap histogram ranks. Blocker named from `flow.go:687`: `narrowTypeByTypeFacts` needs the **subtype and strictSubtype relations**, of which `relater.rs` has only `Assignable`. Three parts: relation simple-arms, eight facts bits, three flow arms. | 440 | 3 | 0.75 | `relater.rs`, `flow.rs` |

### 4.3 Measurement first — cheap probes that unlock a score

None of these can be scored yet, and each is one probe. **Quoting any of these
populations as work would break this file's fourth rule.**

| population | why it cannot be scored | the probe |
|---:|---|---|
| 6,233 | **`BinaryExpression` roots** — `depend.rs` has no step arm for the kind, so the row has never been decomposed at all | add the step arm, as was done for property access |
| 3,855 | **`TypeReference`, no further dependency** — but **top-1 is 51.1%** (`resolvingClassDeclarationWhenInBaseTypeResolution`), so ~1,988 is one case and the real row is ~1,867 of unknown cause | split by case, then by why the reference resolves to nothing |
| 3,739 | **property access, "the property has no type"** — a *downstream symptom*: the property's own declaration gaps elsewhere. `bd tsr-mcd` established this and it is not an item | follow to the type-node roots, which is how tuples were found |
| 2,223 | **object-literal remainder** — accessors (`bd tsr-32y`) and computed names both fall into the catch-all, in unknown proportion. Accessors are **not** a copy of the method arm: upstream prints an accessor as a *property* | split the catch-all by member kind |
| 1,425 | unresolved **value** names — 79.5% want `any`; the reachable remnant has never been characterised | split the 1,425 by what the baseline wants |

### 4.4 What the scores say about 80%

- Distance to **80%** is **+52,551 lines**; to **70%**, **+4,656**.
- The scored list's *reachable* column sums to ~22,000. At the observed 15–57%
  conversion that is **+3,300 to +12,500** — so 70% is reachable from this
  board, and **80% is not**, even if every item on it lands.
- The rest is behind the five capabilities named repeatedly by four cycles of
  ranking: **assignability, call resolution, qualified naming, contextual
  typing, structured signature types.** Two of those five moved this session.

So the standing conclusion holds and is now quantified: **80% is reachable and
it is not reachable by ranking rows.** A session has to take one capability as
its whole deliverable and accept that it converts nothing until finished.

---

## 5. Refused, with the number that refused it

**Do not rebuild these without new evidence. Each cost a measured cycle.**
Rows marked **WITHDRAWN** are kept because the rule is never to delete a
refusal — but their stated grounds have since been contradicted by a
measurement, which is named in the row. A withdrawn refusal is not a licence:
it means the item returns to §4 needing a fresh bar, not that it is now good.

| item | population | why refused |
|---|---:|---|
| **WITHDRAWN** — call resolution | 18,294 | ~~spellability **68.3%** vs 70% bar — 85 lines short~~ **CORRECTED 2026-08-06.** That figure was taken at `058b4a9`; re-run unchanged at `d75cf16` the same expression reads **69.4%, 34 lines short**. R2′'s numerator moves with the compiler, so a bar it crosses by tens of lines decides nothing. Split by row: **CALL 70.6% (+25), `new` 64.8% (−60), `InitCall` 58.9%, `ExprCall` 76.3%.** The refusal no longer stands on its stated grounds and R2′ has stopped discriminating — §4 item 2 |
| contextual typing — **withdrawal itself withdrawn, refusal RE-ARMED** | 2,082 + 1,809 wrong | **86% entangled**, 48.8% behind call resolution. Marked WITHDRAWN at `b00738d` as possibly stale; **re-measured at `0d56467` (fourth session) and it reproduces exactly** — |G| 2,082, every row within 14 lines. The landed builds changed what a member's type is, not whether an argument position can be typed without resolving its call. `checker-notes-fnexpr.md` §10. Off the scored board until call resolution exists |
| qualified naming build | 1,318 | 90.7% accurate on target row; counterfactual **lost 3,202 lines, regressed 753 cases** |
| **WITHDRAWN** — element access | 1,590 | **59.3% want `any`**; refused 3×. **Superseded at `b00738d`:** the `tsr-4qx` build collapsed the row 12,544 → 1,391 and the `want-any` share with it, to **19.2%**. The refusal was true of a population that no longer exists |
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
| **strict-gating the optionality arm** (`tsr-e10` as an item) | 256 | **converts ZERO**, by two independent measurements: **244 of 256 lines are `@strict: true` and none is non-strict**, so a rule that only fires when strictness is off cannot reach them; and the union constructor already neutralises the added `undefined` in non-strict cases (`add_type_to_union` drops it, `get_union_type_from_sorted_list` collapses the remainder). `tsr-e10` is re-diagnosed as a **four-mechanism symptom row** — optional chains (26.6%), equality, `in`, `instanceof` — not an item. `docs/architecture/checker-notes-narrow.md` §1–§2 |
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
| `examples/refmatch.rs` | what a narrowing **matcher** can reach — in-range lines split by guard form and by current verdict, with a strict and a loose bound reported together |
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
| 2026-08-06 | `cf33aee` | **69.59%** | **2,509** | **+590 lines, 0 lost, 0 cases moved** | **nullable receivers and optional chains** — `checkNonNullType` (diagnostics-less), `getOptionalExpressionType`, `propagateOptionalTypeMarker`, wired at all three access sites. Sized by the new `nnaccess.rs` (704 lines, want-any 3%), bar registered before code; conversion **86%** of the sized population. **The registered falsifier fired exactly as named**: 106 of 129 new wrong lines are `controlFlowOptionalChain` wanting the post-access *narrow* — attributed in advance and filed as `tsr-97d` against the flow matcher. A types.rs fixture asserting "optional chains are unported" came due and was rewritten from `elementAccessChain.types` |
| 2026-08-06 | `9eaa2f1` | **69.47%** | **2,509** | **+103 lines, 0 lost, +1 case, 0 regressed** | **`t[0]` answers the element** — one arm at `get_type_of_property_of_type`, reading `tsr-5ll`'s reverse index; sized by the new `elemgap.rs` (56-line row, converted 184% — the seam serves more consumers than the row). All four bar legs passed (34× on the gap→wrong leg; 3 residuals are narrowing/instantiation). §8's registration guessed out-of-range is a gap and the **baseline corrected it before the code ran**: `>strNumTuple[2] : undefined` — the diagnostic and the type answer are separate channels. §3's "no members" safety argument is deliberately spent, on record |
| 2026-08-06 | probe | — | — | 0 lines, by design | **the contextual-typing withdrawal is itself withdrawn**: `fnexpr` re-run at `0d56467` reproduces the 86%-entangled table exactly (|G| 2,082, every row within 14 lines), so the refusal stands re-armed on a fresh number and the 761-score row leaves §4.2. One probe decided a 5,074-line item's session priority — the cheap-probe-first ordering paying out |
| 2026-08-06 | `0d56467` | **69.45%** | **2,508** | **+58 net (+61/−3), 64 wrong fixed, 6 new wrong, 1 case regressed** | **the tuple arm of `compare_types`** (`tsr-5ll`) — a tuple's text no longer poses as a *name*, and two tuples compare by `compareTupleTypes` (readonly, arity, elementwise). Sized to 34 lines from the live wrong dump; **three bar legs fired and are overridden loudly** (`checker-notes-tuple.md` §7): all 9 bad lines are written annotations in signature prints, `tsr-5o2`'s family, proven by baselines that record an order `CompareTypes` cannot produce. The obvious wider fix — blanket written-union reuse — was built, measured **net-negative** (+323/−270), and reverted; `tsr-5o2` carries the number. The tuples.rs two-tuple expectation was intuition and wrong; the comparator was right |
| 2026-08-06 | `ff49871` | **69.44%** | **2,509** | **+1,958 lines, 0 lost, +57 cases, 0 regressed** | **`typeof x` in type position** (`tsr-4sc.10`, fourth session) — sized by `examples/tquery.rs` (1,728-line row decomposed by mechanism form), bar registered and committed **before** code (`30d1ce8`). **The bar's gap→wrong leg FIRED** (+1,341 vs +1,053) and the diagnosis was a mechanism boundary, not a bad build: upstream reuses the **written** `typeof a` node in signature prints. Ported as `Parameter::written_text`/`Signature::written_return`; an intermediate refuse-parameters narrowing measured +1,084/+1,024 and was removed for the mechanism. Final legs all pass at **4.5×** gained/wrong. Δwrong **+423** (436 new in owned families — accessibility chains `tsr-93f`, module internal names, alias naming, signature-position reuse beyond `typeof`, filed `tsr-5o2`). Conversion **122% of the sized row** — the mechanism reached beyond it (§4.1) |
| 2026-08-06 | `b00738d` | **69.03%** | **2,452** | **+1,647 lines, 0 lost, +25 cases, 0 regressed** | **plain tuple type nodes** (`bf5681b`, +1,227) and **object-literal method members** (`b00738d`, +420). The tuple arm was registered with a bar and passed all four legs, its falsifier not firing; the method arm **was not registered**, the second such miss in two sessions, and its Δwrong/Δright of 0.35 sits just over the 1-in-3 the last three registrations used — recorded in `checker-notes-tuple.md` §6 rather than rounded down. Ten fixtures across nine files had used a tuple as their stand-in for "unported" and all came due at once |
| 2026-08-06 | `c72ebf2` | **68.68%** | **2,427** | **+58 lines, 6 lost, +2 cases, 0 regressed** | **narrowing reaches property and element references** (`tsr-6ka`) — `isMatchingReference` made structural, both access forms wired to the flow walk (the binder had recorded their flow nodes all along), and `containsMatchingReference` added after the corpus named it: five over-narrowed lines in `destructuringControlFlow`, the one direction this module can produce a wrong line rather than a gap. **Sized through the matcher first** with the new `refmatch.rs` — 181 strict, 2,172 loose, delivered 64 gained. **No bar was registered before the build**, recorded as a process miss in `checker-notes-narrow.md` §5 |
| 2026-08-06 | `2642e7b` | **68.67%** | **2,425** | **+30 lines, 3 lost, +1 case, 0 regressed** | **equality narrowing against `null`/`undefined`** (`narrowTypeByEquality`'s nullable half; the other half needs `areTypesComparable`). **Its registered bar fired on the floor — 33 gained against 150 — and is overridden, loudly**, in the commit, the issue, here and `checker-notes-narrow.md` §4: the build is right (six fixtures from two baselines, Δwrong **−18**) and the floor was derived from what upstream's *users* write rather than from what this port can *reach* — `is_matching_reference` is identifier-only, so no property-access guard narrows anything. That constraint is now the board's item 1 (`tsr-6ka`). The session's other product is a **refusal with its number**: strict-gating the optionality arm converts zero |
| 2026-08-06 | `385fb60` | **68.67%** | **2,424** | **+0.09 pts, +405 lines, 0 lost, +15 cases** | **calls through instantiated members** (`tsr-1uz`): signatures resolve from the type's recorded `Vec<Signature>`, plus `fillMissingTypeArguments`' no-candidate default fallback — built as one registered iteration after the first arm read +280 against a 300 floor and the registration's own branch sentence named the missing arm. `p.then(f)` / `p.catch()` / `arr.push(x)` resolve; overload sets stay with call resolution |
| 2026-08-06 | `856972a` | **68.58%** | **2,409** | **+0.47 pts, +2,266 lines, 46 lost, +19 cases, 0 regressed** | **signature-typed members instantiate** (`tsr-0hc`): `Signature` side-table + `instantiateSignature` arm + **`strictNullChecks` plumbed** (union constructor only). First run **failed its leg 4** (+538 wrong vs 381) and the new `wrongdelta.rs` attributed it: instantiated lib signatures rendered under the wrong strict mode; the harness default was then measured off the baselines (strict-ON) after a wrong first guess lost 1,221 lines. Δwrong finished at **−600**. Residual: 264 annotation-reuse lines (`tsr-a2c` note). Calls through instantiated members filed as `tsr-1uz` |
| 2026-08-06 | `0fe102a` | **68.11%** | **2,390** | **+2.58 pts, +12,357 lines, 0 lost, +26 cases** | **instantiated generic members** (`tsr-4qx` steps 3+4, one change) behind the **instantiation depth/count guard** (`40970d7`, corpus-neutral alone, `tsr-el3.2` half). Scored against a bar registered at `2d490b8`; all legs passed, the concentration falsifier fired and is decomposed in `checker-notes-inst.md` — 10,000 of the gain is `largeControlFlowGraph` via `Array<any>` index signatures, which **collapsed ADR-0038's ceiling estimate to 2,202 firm** (§2). Ex that case: +2,357 diffuse over 147 cases. Also corrected §1's stale wrong-bucket figure by re-running `wrongflip` at the pre-build commit in a worktree: 41,286 → 41,391, Δ+105, confirming the registered leg-4 expression exactly |
| 2026-08-06 | `d356450` | **65.53%** | **2,364** | **+0.97 pts, +4,645 lines, 0 lost** | **an unresolved type reference prints the name that was written** (`tsr-eep`) — upstream reports `TS2304 Cannot find name` *and renders the name*; answering `errorType` was the divergence |
| 2026-08-06 | `3b7fa44` | **64.56%** | **2,335** | **+0.90 pts, +4,319 lines, +60 cases** | **namespace exports resolve** (`tsr-56r`) — `resolve_name` never read a namespace's `exports`, and its locals lookup never filtered by meaning, so **exporting a declaration made it unresolvable**. Found by `examples/depend.rs` (`tsr-550`), the first instrument to walk declaration edges rather than span edges |
| 2026-08-06 | `a371ec8` | **63.66%** | **2,275** | **+0.02 pts, +81 lines, −1** | `compareTypeNames` for type references (`tsr-bgz`) — the reshape the issue said it needed was already stored in `type_reference_targets`. The single loss is `bd tsr-a2c`, a different mechanism |
| 2026-08-06 | `39a3853` | **63.64%** | **2,275** | **+0.10 pts, +481 lines, 0 lost** | union-constituent parenthesisation (`tsr-xm9`) +353, and the same predicate fixing a pre-existing defect in `array_element_text` +128. Also this session: `removeSubtypes` sized and **refused**, `tsr-jle` and `tsr-iiu` withdrawn, the call row's bar re-scored with `new` separated |
| 2026-08-06 | `5290e1a` | **63.54%** | **2,275** | **+0.20 pts, +958 lines, +5 cases** | the `&&` arm of `checkBinaryLikeExpression` — the only unblocked arm in the board's top three rows. The session's main product is the **board rewrite**: `tsr-jle` fell 11,008 → 1,004, `ArrayLiteral` and `\|\|`/`??` were shown blocked on assignability, and `new` was sized alone for the first time |
| 2026-08-06 | `3299f53` | **63.34%** | **2,270** | **+2.25 pts, +10,761 lines** | export-marker link (+2,265), `this` parameter (+2,733), `super` (+838), `@lib`/`@noLib` harness fidelity (+431), unit-return widening (+1,188), `getApparentType` (+973), `getMergedSymbol` (+430), `autoArrayType` (+1,005), object spread (+74) |

**Process failures worth carrying, all now written up in
`docs/conventions.md`:** a gate piped through `head` reported green while a
test failed (`5290e1a`); a registered bar fired and was overridden on
independent evidence (`2642e7b`); two builds shipped with **no bar registered
at all** (`c72ebf2`, `b00738d`); and five test expectations across the sessions
were written from intuition and were wrong — the port was right every time.
`docs/architecture/checker-notes-*.md` hold the per-item reasoning; this table
holds only the numbers.

---

## 8. Updating this file

**At the end of every session**, whoever ran it updates §1 (numbers + the
commit they were measured at), §3 (what landed), §4 (re-score from a fresh
`depend.rs` run; move finished items off, move measured items up from §4.3),
§5 (anything newly refused, **with its number**), and appends one row to §7.

**On §4's scores:** `reachable` is measured and must be re-taken, never
carried; `effort` and `feasibility` are judgement and should be argued with
rather than inherited. If an item lands, record its *actual* conversion against
the `reachable` it was scored on — that ratio is what keeps the 15–57% band in
§4.1 honest.

If a number here turns out to be wrong, **correct it and say so** — do not
silently edit. Three of this project's most expensive mistakes were numbers
that were true of a different population than the one they were quoted about.
