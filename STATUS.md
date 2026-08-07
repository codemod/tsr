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

Measured at **`097d6bc`**, 2026-08-07 (ninth session).

| suite | passed | rate | note |
|---|---:|---:|---|
| `corpus_ingest` | 12,444/12,444 | 100% | |
| `baseline_resolution` | 12,444/12,444 | 100% | |
| `scanner_termination` | 12,444/12,444 | 100% | |
| `scanner_clean_files` | 5,031/5,031 | 100% | |
| `module_resolution` | 95/95 | 100% | |
| `file_loader` | 96/96 | 100% | |
| `printer_round_trip` | 11,682/11,738 | 99.52% | **corrected** — the published 11,681/11,737 was one case stale; the snapshot has read 11,682/11,738 since before this session |
| `parser_typescript` | 5,001/5,031 | 99.40% | |
| `binder_symbols` | 8,293/8,460 | 98.03% | |
| `isolated_declarations` | 13/15 | 86.67% | |
| `dts_shape` | 618/912 | 67.76% | |
| `dts_emit` | 161/339 | 47.49% | |
| `parser_reachable_target` | 5,031/10,570 | 47.60% | |
| `dts_reachable_target` | 495/1,162 | 42.60% | |
| **`checker_types`** | **2,852/9,538** | **29.90%** | **gradient 73.81%** — the target |
| `diagnostics` | **717/5,488** | **13.06%** | **the check traversal landed, eighth session** — 80 → 717 across eighteen codes, see below |

### `checker_types`, the number the project is steered by

```
353,526 / 478,954 assertion lines = 73.81%      (measured at 097d6bc, ninth session)
  right 353,526 | gap 79,211 | wrong 36,178        right+gap+wrong = 468,915 exactly
```

**The ninth session's chain, every figure from one `verdictdump` pair per
build:** 352,727 + 246 (empty literal non-strict) + 377 (alias rename, split
filter) + 176 (async `Promise<void>`) = 353,526 exactly — no alignment drift
this time.

**Re-taken from one `verdictdump.rs` run at `9fe8056`**, and the arithmetic
check against the sixth session's triple names the session's six checker
builds: 347,530 + 128 + 293 + 297 + 58 + 83 + 1,153 + 11 + 1,500 + 1,674 = 352,726 (+1 alignment drift, re-taken whole from the run above).

**Re-taken, not carried.** This block read `347,384 / 81,977 / 39,554` for two
builds after those numbers stopped being true — the namespace deletion (+32) and
the untyped-call arm (+114) had both landed while §1's *table* was updated and
this block was not. Caught by a consistency pass rather than by a reader, which
is luck; the two numbers sat 20 lines apart in the same section. **A figure that
appears twice in one file will disagree with itself unless something re-derives
both**, and the arithmetic check that catches it is free: 347,384 + 32 + 114 =
347,530.

**All three figures are now ONE probe's, which is what this section has been
asking for.** `examples/verdictdump.rs` (new, this session) emits a verdict for
every aligned line in a single pass, so `right + gap + wrong = 468,915` is an
identity rather than a subtraction across instruments. The design-P pair read
`344,411 / 81,977 / 42,527` before and `347,384 / 81,977 / 39,554` after, from
the same probe at both ends — and its `wrong` before-figure of **42,527**
reproduces the qualified-naming build's `wrongdelta` total exactly, which is the
first time two instruments here have agreed on that number.

The remaining **10,039** lines of the 478,954 denominator are **unaligned**: the
baseline and this port disagree about the *expression*, so no comparison of
answers is meaningful and the line is in no bucket.

**This SETTLES the "9,145 short" discrepancy** that this section opened the
session with, and it is now a reconciliation rather than a hypothesis: the whole
triple is one probe's, taken at one commit, and it sums to 468,915 exactly.
`478,954 − 468,915 = 10,039`. The earlier 9,145 was the same phenomenon measured
with three instruments that do not share a denominator, which is why it was
neither stable nor equal to this figure.

> The session opened by recording that gap as *"worth a future session's first
> hour"*. It cost about ten minutes, and only because a build had already needed
> `verdictdump.rs` for an unrelated leg. **The instrument that answers a standing
> question is usually built for something else** — the cheap move is to notice
> when one arrives, not to schedule the question.

**Corrected in place: §1 carried `340,719` and the snapshot at `df13a69` read
`340,727`.** Eight lines, from commits landed after the `tsr-g30h` merge the
figure was taken at. The gap figure is `depend.rs`'s own walk at `a4e3991`, not
a subtraction.

**An open discrepancy, recorded rather than reconciled away — and now largely
answered.** `wrongdelta.rs`'s raw dump read **42,443 lines** at `a4e3991`, and
`right + gap + wrong` summed to 469,809 against a denominator of 478,954 — 9,145
short. The three instruments did not share a denominator (`depend` attributes gap
lines it can reach; `wrongdelta` dumps per-case). **The suggested fix — "one probe
reconciling the three denominators is worth a future session's first hour" — was
built this session** and is `verdictdump.rs`; see the paragraph above for what it
reads and for what it does *not* yet claim. The old warning still stands: **do not
substitute a figure from one instrument into another's series.**

**The wrong bucket re-split post-naming, seventh session (38,433 at
`ffb77fe`, coarse shapes over the verdictdump):** 12,325 want `any`
(ADR-0038/39 territory), 10,000 are `largeControlFlowGraph` alone, the naming
families are down to **1,235** (874 dotted-qualifier + 361 `import()`-form),
and the remainder is structural — unions 4,128, signature bodies 3,017,
generics 2,785, unsplit 14,943. The morning's dominant family is now the
smallest named one; what is left is ceiling and subsystems, which is §4.4's
conclusion measured from the wrong side.

**The wrong figure is carried forward by measured deltas, not re-derived.**
It was once quoted 4,000 lines stale, which nearly failed a bar by 20 lines: a
cross-instrument, cross-session subtraction is not a measurement. Re-run
`wrongflip.rs` at both ends of a pair if the number matters.

**The gate is whole-baseline and positional; the gradient is per-line. They are
nearly orthogonal** — a change can add 2,733 lines and flip zero cases. Say which
you are quoting.

### `diagnostics` — the traversal exists now, and the suite is a long tail

**Corrected, eighth session.** This section read *"`tsr-checker` emits no
diagnostics at all … nothing in the checker's type answers will move this
suite."* Both sentences were true and the *conclusion* the project drew from
them — recorded in two handoffs as "diagnostics is structurally blocked" — was a
statement about **why the number was flat, not a sizing of the work**. Nobody
had measured what the blocked cases were blocked *on*.

`examples/diaggap.rs` (new) does. Over the suite's own 5,488 judged cases it
splits the difference sets and ranks the codes by the only column that is a
forecast — **cases missing exactly one distinct code and reporting nothing
extra**, the bucket where one rule finishes a case on its own. At `7299a14`:
**3,258 of 5,488 were blocked on exactly one code**, across 469 codes. That is
59% of the suite behind single rules, not one wall.

ADR-0040's decisions (1) and (2) are built: `Checker::check_source_file`
(`crates/tsr-checker/src/check.rs`) is a second entry point beside the query
road, with the diagnostic collection on the `Checker` and drained by the
consumer. Three rules on top of it, each bar-scored:

| commit | rule | net cases |
|---|---|---:|
| `bc8045b` | TS2307 / TS2882 — module specifiers (`resolveExternalModule`) | +50 |
| `413174c` | TS2564 — `strictPropertyInitialization` | +133 |
| `7fab616` | TS2304 — `Cannot find name` | +115 |
| `1591c55` | TS2454 — used before being assigned | +233 |
| `12533e9` | TS2369 + TS2695 — two **syntactic** rules | +40 for **0** wrong |
| `60aae99` | `checkGrammarBreakOrContinueStatement` — TS1104/5/7, TS1115/6 | +25 for **0** wrong |
| `04a394b` | `checkGrammarStatementInAmbientContext` — TS1036, TS1183 | +24 for **0** wrong |
| this commit | `checkFunctionOrConstructorSymbol` — TS2390/1/2/3, implementation-expected arms only | +17 for 11 wrong |

`docs/architecture/checker-notes-diag2.md` carries the reasoning, the gates per
rule and the refusals. **`checker_types` is byte-identical across all of them**: the traversal is a second road and no query-path call site invokes it.

The suite's board is now `diaggap.rs`'s single-code column, re-run after every
rule because rows *grow* as rules land (a case blocked on two codes becomes a
case blocked on one). At **611** passing: TS2322 433, TS2339 133, TS2345 85,
TS2304 81, TS6133 80, TS2454 60, TS2564 35 — **TS2339 is refused with its
number**, §5.

**The metric a diagnostic rule is scored on is not its conversions.** A case
carrying a spurious diagnostic can never pass however many rules land later, so
the figure is `passing + single-code reachable`. TS2454 read **+28** on it — and
**−210** before its last refusal landed, which is the number that shows why the
declines are the design rather than the polish.

**And the ordering rule the sixth build bought:** a rule that reports on a
*syntactic* fact has no incompleteness to leak. TS2369 and TS2695 converted 40
cases for **zero** wrong lines and needed no tightening pass at all, where every
one of the four semantic rules needed two or three. The small `1xxx` and
syntactic `2xxx` rows — TS1212 22, TS1036 19, TS2391 17, TS1029 12, TS1107 12
and a long tail, ~150 cases — are therefore worth roughly their row at roughly
no risk, and rank **above** TS2322's 543, which needs a members subsystem before
it is worth anything at all (§9).

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
today                    333,651 / 476,752 = 69.98% of reachable   (carries the 1,539
                         right-lines-in-the-unreachable-set offset the 332,570 figure carried)
80% of the full          383,163 lines  =  80.37% of the reachable
gap to 80%               +47,184 lines (full-denominator terms: 383,163 − 335,979)
gap to 70%               CROSSED (70.003%; the threshold was 335,268)
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
| **checker** | **72.56% of lines** | the mountain; §4 and §5 |
| transformers | **not started** | |
| diagnostics | **started — 6.89%** | the check traversal (ADR-0040 (1) and (2)) plus three rules; §1 and `docs/architecture/checker-notes-diag2.md` |
| language service / LSP | **not started** | |

### Inside the checker — what has an arm

Landed across the three sessions to date, newest first:

| commit | what | net |
|---|---|---|
| `1b02597` | **the single-signature literal collapse, with written carriage at three slots** (`bd tsr-d4li`, §10.15–16) — first built without carriage, **fired its leg at 112 lost, reverted whole** (`dbc1ae9`), rebuilt to the revert's own spec; three measurement residuals each named the next carriage slot, converging 112 → 55 → 25 → **7** with conversions untouched. Δwrong **−1,666**, +48 cases | **+1,668** |
| `ffb77fe` | **the composite-print twin** — `signature_to_string_at`, every rendered slot of a single-signature type through the site-aware naming stack (`checker-notes-modobj.md` §10.13–14, `bd tsr-2ghn`). **Forecast delivered to the line: 1,500 forecast, 1,500 net**, RIGHT→WRONG 2 = the counterfactual's at-risk count, Δwrong −1,500 — because the probe's `compose()` and the build are the same function, proven by the self-check leg before either ran. The enabler (`0796633`, the Union/Intersection symbol arm in `qualified_name_at`, +11) is what raised the forecast from 674 to 1,500 | **+1,500** |
| `9fe8056` | **a JSX element expression has the `JSX.Element` type** (`checkJsxElement`, `jsx.go:72`) — buildable only after the `/.lib` mount; the fifth session's "46% cannot resolve JSX" premise predated it and is corrected in `checker-notes-jsx.md`. 152% of the 759 forecast via the arrow-function-return cascade; own new wrong **4**, lost **4** (baselines that record `error`), residual = the composite-signature-print boundary (285, design P's baked-text seam) | **+1,153** |
| `cc8c422` | **`import d from "m"` resolves the real `default` export** (`getTargetOfModuleDefault`, plain half; synthetic default declined) — plus the rendering refusal that a symbol named `default` **never prints as a name**, which converted 40 would-be wrongs into gaps and cleaned 67 pre-existing ones. Δwrong **−38** (§10.12) | **+83** |
| `67949ee` | **a chain segment prints under its best name** — `React.Component`, never `__React.Component`. One edit on `symbol_chain`'s parent segment; measured WRONG→RIGHT 58 with **no other transition of any kind** (`checker-notes-modobj.md` §10.10) | **+58** |
| `952b328` | **`export = X` resolves through the assignment, and the rename prints the alias** — `resolve_alias`'s `ExportAssignment` arm (`checker.go:14889`), `import a = require` following `resolveExternalModuleSymbol` (`:15556`), and `Checker::best_name` (the innermost-table walk of `getAccessibleSymbolChain`). Sized at 302 seed converts / 23 would-wrong (§10.8). **Leg 4 FIRED at 130 lost on the first measurement** — a same-file `import a = b` alias renamed `privacyGloImport`'s namespaces, and the counterfactual's alias reduction was blind to that form, a false 0 at-risk — fixed with upstream's own `useOnlyExternalAliasing` flag (`symbolaccessibility.go:568`) and re-measured at **RIGHT→WRONG 0** (§10.9). The React/tsx `typeof React` family converts | **+297** |
| `897abdd` | **a chain through a module container qualifies** — `symbol_chain`'s three new arms in upstream's order: `trySymbolTable`'s direct-alias guard (an in-scope alias naming the symbol itself stops the chain), the container-alias arm via the tri-state `module_alias_at` (ambiguity declines outright), and the ambient `import("x").` branch (`nodebuilderimpl.go:1260`). Sized at 104 strict converts (§10.6), landed **283%** of that through composite prints; RIGHT→WRONG was **exactly the 3 at-risk lines the counterfactual named in advance** (`checker-notes-modobj.md` §10.7) | **+293** |
| `f62582e` | **`tryFindAmbientModule`** — `declare module "x"` is a resolution target, consulted before the host (`checker.go:15533`, `:15154`), selected by declaration shape because this binder stores ambient names unquoted. The type-creation refusal for bodied ambient modules moved to the rendering path (the thirteenth unported-stand-in fixture came due). Mechanism-own new wrong ~15; 328 more GAP→WRONG were correct types wearing unqualified names — attributed by arithmetic over two `verdictdump` runs and converted by `897abdd` the same session | **+128** |
| `93b540a` | **the `/.lib` test-library folder mounts** (`harnessutil.go:39`, `:141`) — measured at **zero** and shipped with the zero stated: react.d.ts loads and `"react"` is in globals, but the tsx corpus's `typeof React` head is blocked one mechanism later, on `export = __React` (the export= arm, `tsr-e2u` family) plus alias-preferred naming | 0, stated |
| design **P** | **`getSymbolChain` — a name declared in a namespace prints its qualifier** (`checker-notes-qualname.md` §11). The *printing* half of qualified naming, refused for four cycles on "lost 3,202 lines"; re-sized post-W at **2,990 converts / 14 at risk** and built to its registered bar. **All four legs passed**, the conversion column landed on **2,990 exactly** (100.0% of forecast), and `Δgap` was **0** — the leg that could only be written because P is a renaming and cannot make a gap line computable. `getMergedSymbol` measured ON against OFF: **13 losses removed for 0 conversions**. 64.1% of the gain is `compiler/temporal`, and leg 3's `cases gaining ≥ 60` (read 174) is what proves it is not a one-case build | **+2,973** |
| `8e28971` | **qualified type names reprint what was written** — `resolve_entity_name` plus a written-text reprint in `declared.rs`, with the *inside-the-namespace* positional refusal kept at 1.2:1 and an *enum-root* one **declined on principle** (upstream prints `Choice.Yes` verbatim). **Its bar's third leg fired and is overridden loudly** in `checker-notes-qualname.md` §9.6, by a third party, on arithmetic over the bar's own stated rule | **+3,590** |
| `d8590ff` | **the overload gate asks about the PAIR** — `relater.rs` made three-valued (`Related`/`NotRelated`/`Unknown`, Kleene composition) and `calls.rs`'s `SELECTABLE` **flag set deleted**, plus an `any`-parameter positional refusal priced at 40/26 → 33/2. The ternary itself converts **zero**; it is what makes removing the gate safe (`checker-notes-assign.md` §5–§6) | +94 |
| `tsr-g30h` | **structural type-argument inference** — the candidate walk for `T[]`, `(x: T) => U` and friends; leg 2 fired **twice** and both were build defects a +311 net had hidden (`checker-notes-infer2.md`) | +372 |
| `tsr-4sa` | **a `Named` callee reaches signature lookup** — call and construct signatures resolved off an interface's members, with `unique symbol` (291) and namespace-qualified naming (75) **refused positionally**; the naive design measured 646 converts against 377 wrong and those two refusals cost **zero** conversions (`checker-notes-namedcallee.md`) | +1,018 |
| `tsr-84iz` | **an array pattern implies a tuple over its literal** — the construct `tsr-o00` refused; a shared `create_tuple_type` keeps an inferred tuple interned with a written one (`checker-notes-patctx.md`) | +206 |
| type predicates | **`x is T` in return position** — `getTypeFromTypeNode`'s `KindTypePredicate` arm plus the node builder's return slot, and a parser ASI fix (`next_is_is_keyword` lacked `!hasPrecedingLineBreak`) that moved `parser_typescript` and `binder_symbols` **up** (`checker-notes-typepred.md`) | +1,041 |
| `tsr-jril` | **constructor type nodes** — `new (x: T) => U` and `abstract new`; a `SignatureKind` on `Signature` so one renderer serves both spellings (`checker-notes-ctortype.md`) | +1,358 |
| `tsr-rppd` | **`getApparentType` reads a type parameter through its `extends` constraint** — its bar fired at net 0 first and the diagnosis was a missing symbol route (`checker-notes-apparent.md`) | +156 |
| `tsr-0opd` | **private names** — `this.#x`; the binder already filed `#x` members, only the access-side name extraction was missing (`checker-notes-privname.md`) | +590 |
| `tsr-tgov` | **`new C<T>()` instantiates from written type arguments** — the call side already substituted them and `check_new_expression` refused at its first line; plus upstream's one name-independent quoting rule, a **method** named `new` (`checker-notes-callres.md` §14–15) | +686 |
| `4b81458` | **binding elements** — the plain destructuring leg (`tsr-o00`): object patterns via the `a["b"]` lookup pair, array patterns by position through the tuple reverse index; the parser records array-binding holes; six refused legs each with a number (`checker-notes-destructure.md`) | +1,081 |
| `acdeed5` | the `in` guard narrows by property presence; its bar's leg 2 caught the OPTIONAL-flag bug pre-ship (`checker-notes-narrow.md` §6.1) | +43 |
| `e7a65fb` | `typeof` guard narrowing — subtype relations, sixteen facts bits, three flow arms (`checker-notes-narrow.md` §6) | +745 |
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

### 4.0 The gap-root board, re-measured at `138fb45` (seventh session, closing)

`examples/depend.rs`, fresh. **Gap 80,315** (was 81,769 at the session's first
board run — the naming family's conversions came mostly from the *wrong*
bucket, which this board does not walk). The head rows are unchanged in kind
and all owned: `PropertyAccessExpression / dependency` 10,210 (the §4.3
symptom), unresolved VALUE identifiers 7,678 (74% want-any),
`CallExpression` 6,610 (call family, refused legs), member name 4,786,
`ArrowFunction` 3,836 at 0.9% want-any (contextual typing, refused),
`NewExpression` 3,318, FunctionDeclaration-cycle 2,943, ObjectLiteral 2,383
(contextual), BindingElement-cycle 2,363, ArrayType 2,120 (94.3% one case),
TemplateExpression 1,890 (refused). ~~**No unowned row remains above 1,000.**~~

> **CORRECTED, eighth session, and the correction is about this file rather
> than about the compiler.** The board was re-run at `7299a14`: **gap 80,315,
> every row unchanged**. But *"no unowned row remains above 1,000"* was true
> only of the rows `depend.rs` **attributes**. Two of its five endings attribute
> nothing — `cycle` 5,394 and `depth cap` 1,330, 8.4% of the gap — and the kind
> printed for those is an arbitrary member of a loop, not a cause. `cyclegap.rs`
> (new) measured them; `docs/architecture/checker-notes-cyclegap.md`:
>
> - **The `cycle` ending is not a cycle.** 5,319 of 5,394 have loop length
>   **one** — the node's step is itself — and all 5,319 are declarations with
>   **neither an annotation nor an initialiser**, so `step` falls through its
>   declaration arm into its reference arm and returns the node it started from.
>   Both rows are `NO STEP ARM` wearing the `cycle` label. The genuinely
>   recursive shape `depend.rs`'s own C2 describes is **75 lines**, not 5,394.
>   **`FunctionDeclaration`-cycle is therefore UNOWNED**: 2,943 lines, want-any
>   **1.8%** (net 2,889), 611 cases, top-1 3.7%, and **1,894 of them want a
>   signature** — an un-annotated function declaration whose own name gaps. It
>   is the most diffuse, lowest-want-any row above 2,000 on the board.
>   `BindingElement`-cycle (2,369, want-any 32.5%) belongs to `tsr-84iz` /
>   `tsr-pqnh`.
> - **`ArrayType` 2,120 is 99.7% propagation, not a root** — 2,114 of it has a
>   gapping *child* type node. Across all type-node kinds `step` has no arm for,
>   **4,696 of 8,499 lines are propagating** and belong inward.
> - **`depth cap` is not an item**: all 1,330 lines are two pathological cases.
>
> Two `depend.rs` fixes are named and deliberately not made, so the board stays
> comparable: relabel a length-1 cycle, and add `step` arms for
> `ArrayType`/`TupleType`/`UnionType`/`IntersectionType`.

### 4.0b The gap by what upstream's ANSWER LOOKS LIKE — new, eighth session

`cyclegap.rs` at `7299a14`, over the same 80,315 lines — and **re-run unchanged
at `0d0971a`**, after the parallel session's check traversal landed (it touches
`types_producer.rs`, so the re-take was not optional): every figure on this page
and in §4.0's correction is byte-identical across that merge. The first whole-gap view
that is not a node-kind histogram, and it re-frames the board:

```
any (ADR-0038) 19,970 (24.9%) | primitive 14,652 (18.2%) | signature/arrow 12,606 (15.7%)
bare name 6,411 | anonymous object 5,799 | array 5,152 | generic ref 4,177 | union 4,112
typeof query 2,109 | qualified name 1,425 | tuple 1,414 | string literal 1,406 | rest 1,082
```

**A quarter of the remaining gap wants `any`.** That is *not* §2's firm ceiling
of 2,202 — it is the unstable population §2 warns about (lines this port fails
to compute *and* whose baseline answer is `any`). Neither figure supersedes the
other and neither may be substituted for the other.

**"Signature / arrow" at 12,606 lines is the largest shape that is neither
ceiling nor primitive, and it matches no single board row** — `ArrowFunction`
3,328, the mislabelled `FunctionDeclaration`-cycle 1,894,
`PropertyAccessExpression` 908, `FunctionExpression` 832, `FunctionDeclaration /
dependency` 664, long tail. This is §4.4's *structured signature types*
capability measured from the answer side for the first time, and it is the
largest single thing the board has ever shown.

### 4.0a The board it replaces, re-measured at `d9a730b` (sixth session)

`examples/depend.rs`, run fresh so this section's numbers are this session's.
**Gap 86,642 → 82,871 across the session** (−3,771). The single most useful
thing in it is a row that is *no longer there*:

```
  10,342  12.5%  PropertyAccessExpression   dependency types    want-any 47.9%   top-1 14.9%
   7,685   9.3%  Identifier, no VALUE       no further dep      want-any 73.9%   top-1 14.9%
   6,692   8.1%  CallExpression             dependency types    want-any 12.5%   top-1  4.2%
   4,827   5.8%  Identifier, member name    dependency types    want-any 43.7%   top-1 13.9%
   3,917   4.7%  ArrowFunction              NO STEP ARM         want-any  0.8%
   3,337   4.0%  NewExpression              dependency types    want-any 15.7%
   2,402   2.9%  Identifier, no value decl  no further dep      — the refused import-alias row
   1,563   1.9%  TypeReference              dependency types    (was 1,939)
       —         TypeReference              NO FURTHER DEP      — GONE. Was 4,010 and ranked 5th
```

**`TypeReference / no further dependency` was 4,010 lines at the start of this
session and does not appear at all now.** That is independent confirmation of the
qualified-naming build from a different instrument than the one that scored it:
`typerefgap.rs` said the row was 99.8% one mechanism, and removing that mechanism
removed the row.

Two head rows are *known* not to be items and must not be re-scored as such:
`PropertyAccessExpression / dependency types` is §4.3's downstream symptom
(`bd tsr-mcd`), and `Identifier / no value declaration` (2,402) is the import-alias
family refused on naming grounds (`bd tsr-4jk`, §5).

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
| ~~**~2,976**~~ | **LANDED, design P — `getSymbolChain`, +2,973. The board's top item, and the second refusal retired by re-measurement in two sessions** (`checker-notes-qualname.md` §10–§11). Sized post-W at **2,990 converts / 14 at risk / 213:1**, against a refusal that had stood four cycles on "lost 3,202". Bar registered at `90c4e70` **before any code**, four legs, all four PASSED — net 2,973, lost 17, cases regressed 0, cases gaining 174, and **gap→wrong exactly 0**. Conversion **100.0%** of forecast, because §10.5 registered in advance that this forecast was a point estimate rather than W's floor. Two facts that must travel with the number: **64.1% of the gain is `compiler/temporal`** (outside it, 1,072 lines), and 17 lines were lost — 9 to `bd tsr-4jk`'s alias chain, 4 to `getContainersOfSymbol`'s unported multi-parent ordering | ~~2,990~~ landed | 3 | — | `checker.rs` |
| ~~**~1,530**~~ | **LANDED at `8e28971`, +3,590 — 211% of the forecast, the session's largest build.** ~~qualified naming, design W — the written entity name. THE TOP OF THE BOARD, and it arrived by overturning a refusal rather than by finding a new row** (`checker-notes-qualname.md`). Reprint the written qualified name for a type reference whose leftmost identifier resolves as a namespace, declining when the reference site is **inside** the namespace it qualifies — a positional refusal upstream's own `needsQualification` licenses, worth 68 conversions to remove 79 wrong lines. Forecast **1,702 converts / 20 would-be-wrong / 0 at risk = 85× **, and the forecast is a **floor, not a ceiling**: 1,302 lines are excluded as unscorable rather than declining. Bar registered in §8 of that page **before any code** — `lost == 0`, `new wrong ≤ 40`, `gained ≥ 900` — with four falsifiers named. Effort 2: `resolve_entity_name` plus a written-text reprint, on a node the parser already records Residue, each with an owner: alias naming 20 (the missing `alias_symbol_for_type_node` call on the type-reference arm), enum narrowing 6, design **P** — the symbol chain on the *outer* name — 13 and now **created** by W's conversions, so §8's "W first, then re-measure P" is load-bearing rather than tidy | ~~1,702~~ landed | 2 | — | `declared.rs` |
| ~~**~1,400**~~ **REFUSED** | **type-argument inference's REMAINING LEGS — sized and refused, sixth session** (`checker-notes-infer2.md` §7). The 2,056 was a **row**: the mechanism's own population is 1,231, of which **77.3% is two things that are not the lattice** — 494 lines where no candidate is found at all and 458 downstream of a gapping argument. Both remaining legs measured as deltas: contravariant bucket 6 converts / 1 wrong, priority lattice 11 / 1, **~17 conversions together**, every leg's at-risk column **0** over 792 admitted right lines. The lattice **alone** is worse (11 / 4), so sequencing it first to dodge `strictFunctionTypes` does not rescue it. `bd tsr-g30h` closes. ~~old row:~~ **type-argument INFERENCE — the largest gate in the call funnel by 4x.** Re-measured at `a57a04b` (`callgate.rs`, sixth session): of 7,303 admitted call lines, **2,056 stop at "inference gapped"** at only **1.6% want-any**, against 497 more at "a generic candidate in the set". `bd tsr-g30h` landed the first slice (+372, structural candidate walk); the priority lattice and contravariant tracking are the remainder. **The contravariant bucket alone is refused at 6 own-node lines (§5) — that refusal is about the BUCKET, not about this row**, and re-reading it as a refusal of inference would be the "population identified by the shape of the answer" error | ~~2,056~~ 17 | 5 | — | refused |
| ~~869~~ ~~**~78**~~ **~41** | **call resolution — overload sets. RE-SCORED DOWN A SECOND TIME, sixth session**, on a funnel re-run after `SELECTABLE` was deleted (`d8590ff`). The gates overload selection actually owns now sum to **658 lines**, not 1,095: `any` parameter 324, undecidable pair 163, ambiguous return 81, nothing-assignable 67, arity 17, this/rest 5, spread 1. Want-any across them is ~200, so reachable is ~460. **Two of those gates are this session's own refusals and are working as designed**, and `undecidable_split.rs` shows the undecidable 163 is 92 signature-bearing + 41 no-members-table + 36 unported-flag + 16 absent-property. Three of the relation's six `Unknown` sites essentially never fire | 658 | 5 | 0.45 | `calls.rs`, `relater.rs` |
| ~~869~~ ~~**~78**~~ | ~~**call resolution — overload sets. RE-SCORED DOWN, fifth session.**~~ `bd tsr-klm` is answered (`callgate.rs`, `checker-notes-callres.md` §13): the 9,660 was **the row, not the mechanism**. Summed from the gates overload selection actually owns — generic candidate 490, parameter 473, argument 28, ambiguous 50, arity 9, nothing-assignable 39, this/rest 5, spread 1 — its own population is **1,095 lines, ~880 net**, an **8× smaller** item, and third of the three the split found | 1,095 | 5 | 0.45 | `calls.rs`, `relater.rs` |
| ~~360~~ | **type-argument inference — FIRST SLICE LANDED, +372** (`bd tsr-g30h`). The counterfactual forecast 131 own-node conversions and it converted 372 (2.8x, cascade). **`bd tsr-g30h` stays open**: the contravariant bucket / priority lattice owns both the 3 lost lines and the largest share of the 33 new wrong, which is the argument for it being the next leg. Tuples (42), object-type members and intersections refused with numbers in §5.4 of that page. ~~old row:~~ The largest gate in the corrected split: a single candidate resolves and *inference* is what stops, **2,324 lines, want-any 28**. `inference.rs`'s own doc measures the cliff — 53% of generic calls have no type parameter written bare, so the candidate must be dug out structurally. A subsystem (`inference.go:53`: priority lattice, contravariant tracking), which is why the effort is 5 and not 3 | 2,324 | 5 | 0.75 | `inference.rs` |
| ~~340~~ | **a `Named` callee — LANDED, +1,018.** The counterfactual computed the at-risk column in the same pass as the target one, exactly as `docs/conventions.md` requires, and it earned its keep: the naive arm read **646 converts against 377 wrong**. Four measured refusals cut that to **8 new wrong** — `unique symbol` and qualified naming each cost **zero** conversions. Remainder, each refused with a returning condition: 926 generic candidates (inference), 274 class instance types, 291 `unique symbol`, 75 qualified naming, 48 disagreeing overload sets, 27 heritage. ~~old row:~~ 2,791 lines across the call and `new` halves — lib constructor *interfaces* (`DateConstructor`, `MapConstructor`) and interface-typed callees. Needs `getSignaturesOfType` → `resolveStructuredTypeMembers`. **Carries a measured wrong-manufacturing risk**: §5 of `checker-notes-callres.md` records 264 `unique symbol` lines that would print `symbol`, and half the row wants a generic instantiation this port has no members for | 2,791 | 4 | 0.50 | `signatures.rs` |
| ~~649~~ | **destructuring / binding patterns — LANDED at `4b81458`** (`tsr-o00`). Sized by the new `bindgap.rs` at 1,228 buildable of 2,632 classified (~1,111 net); converted **+1,081 = 97% of the sized net**, above the band again via downstream unblocks. The row's residue belongs to its owners: contextual pattern parameters 604 (the re-armed contextual refusal), pattern-implied tuple inference ~250+155 (`tsr-84iz`), flow-of-destructuring (`tsr-pqnh`), rest 172, computed 86, no-source 113 | — | — | — | `destructure.rs` |
| ~~374~~ | **element access remainder — decomposed to shards, none an arm.** After the fourth session's builds the row is **634** (`elemgap.rs` at `935a221`): 177 string-literal misses (want-any 27%; head cases are `noImplicitAnyStringIndexerOnObject` — option modelling — and `mappedTypeRelationships` — mapped types, unported), 118 numeric-literal misses (lib-array receivers under literal indexes), 108 enum/named indexes (enum machinery), 86 other. Each shard belongs to an unported subsystem, not to `indexed.rs`; the row stops being a board item and its shards go to their owners. | — | — | — | split complete |
| **RE-OPENED, seventh session** | **JSX — the fifth session's blocker measurement is STALE**: `jsxfeas.rs`'s "46% cannot resolve the JSX namespace" was measured before the `/.lib` mount (`93b540a`), and react.d.ts declares a *plain global* `namespace JSX` that now loads. First-order size at `cc8c422`: **1,955 gap + 96 wrong** lines want `Component<…>`/`JSX.*` shapes over 220 cases; the whole tsx/jsx non-right population is 4,374. `bd tsr-fpti`; re-run `jsxfeas.rs` before costing. ~~old row:~~ **JSX — re-scored DOWN at the fifth session's `jsxfeas.rs`** (`checker-notes-jsx.md`): of 1,199 element gap lines, **46% cannot resolve the `JSX` namespace at all** (it sits behind `declare global` augmentation, unported in the binder) and **29% resolve but print bare `Element`** — the refused qualified-naming family (2.7 wrong/right). Feasibility ~0.25; blocked on global augmentation, then namespace-qualified naming | 1,199 | 4 | 0.25 | blocked — prerequisites named |
| ~~165~~ | **template literal types / `TemplateExpression` — REFUSED with a ratio, fifth session** (`checker-notes-tmplexpr.md`). The old grounds ("the cheap leg is not separable") were re-tested on the fresh 1,890 row and are now measured: the cheap leg alone converts 446 against 590 new wrong (**0.76 per wrong**), the folder-plus-fallback design 515 against 521 (**0.99**). Both are worse than every refusal on this page. The legs **interleave** — 151 lines want `string` exactly where the folder fires | 1,036 | 3 | 0.15 | refused |
| ~~165~~ | ~~**template literal types.**~~ Refused once: the cheap leg is **not separable**, because upstream's `evaluate` is a syntactic folder consulting no types. Kept on the list because the row survived the session unchanged. | 1,237 | 3 | 0.40 | `declared.rs` |

### 4.2a What is actually left, after a session that emptied most of §4.2

Six of the rows above are struck through and §4.3 lost two more, so the scored
list no longer reads as a board. Stated plainly, at `a57a04b`:

**Live and scoreable**

| lines | item | note |
|---:|---|---|
| 285+ / **1,437 loose-converts ceiling** | **the composite-print seam** (`bd tsr-2ghn`) — baked text inside signature prints cannot take site-aware naming. Fresh loose columns at `b228524`: **1,437 converts / 724 at-risk** (the pre-W 4,801 is stale), and the at-risk head is the *inside-the-container* family design P's shipped stop conditions already refuse — the loose model just lacks them. Refine the probe with the shipped predicates before building; the temporal-case concentration caveat travels with the converts | new, P1 |
| 658 / ~460 reachable | overload selection's own gates | score ~41; effort 5. Two of its largest gates are this session's own refusals working as designed |
| 2,276 / ~1,833 | the **callee-type** family — `callee type is not an object type` 1,398 + `identifier: symbol types as a non-object` 878 | **NOT PROBED. Start here.** The largest unexplored gate in the call funnel, and the only live row whose population has never been split by mechanism. A probe was dispatched this session and produced nothing; no partial instrument was kept, so there is no half-number to inherit. **The open question is whether the two gates are one mechanism or two** — plausibly both are "the callee does not type as something with signatures", but that is a hypothesis and testing it is the probe. Note `tsr-4sa`'s four positional refusals are prior art here, and **one of them (qualified naming) is no longer refused** — both halves shipped this session — so any bucket blocked on it must be re-examined rather than carried |
| ~~494~~ | ~~inference finds **no candidate at all**~~ **SIZED BY RE-INSPECTION, seventh session** — a fresh `infergen.rs` run at `cc8c422` shows the row's own sub-buckets are the *already-refused* legs of `tsr-g30h` §5.4 plus three small unported sub-mechanisms, each with its head named: rest parameters 56 (`getSpreadArgumentType`), spread arguments 39, `null`/`undefined` candidates 32 (93.8% one case, needs `getWidenedType`), return-not-rebuildable 88 (45.5% one case). The remaining ~279 are the structural candidate-walk shapes (object members, intersections) the `tsr-g30h` bar refused with numbers. **Not a board item** |
| **1,094** | **`getSpecifierForModuleSymbol` — the symbol chain stops at a module container** | **NEW, and it is the named prerequisite of a refusal.** `qualnamep.rs`'s *"why a chain was NOT built"* table: 44,982 need no qualifier, **1,094 stop because the container is an external module**, 229 have no container at all. `checker.rs:654` returns `None` there by design, because upstream names such a container with `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:667`) and this port has no port of it. **Effort CORRECTED and the item SPLITS IN TWO**, from reading `nodebuilderimpl.go:1249` rather than from the row: the general path calls `modulespecifiers.GetModuleSpecifiers`, a whole package taking compiler options, a host, import-specifier preferences, resolution modes, `baseUrl` and a per-symbol cache — **effort 4–5**, not the 3 first filed. But `:1260` has a cheap **exact** branch: for an **ambient** module (`declare module "x"`) the specifier is the name with quotes stripped, and `IsAmbientModuleSymbolName` (`ast/utilities.go:1656`) is just *starts and ends with a quote* — **effort 1**. **The sizing question is therefore not "1,094"**; it is how many of the 1,094 are ambient containers versus real file modules, and that split decides whether this is a small effort-1 item or a large effort-5 one. **A corpus-level PRIOR, offered as a prior and not as the measurement**: an ambient module declaration appears in only **276 of 18,876 cases (1.5%)**, and `compiler/temporal` — a head case of this very family — contains **zero**. That is a count over *cases*, not over the 1,094 *lines*, so it cannot settle the split; it does say the cheap half is likely small, which would leave an effort-4–5 item that does not clear this board. **Take the real split before building either half.** `bd tsr-xpb8` |
| **810 / 779 net** | **the un-annotated `function f` types as its own signature** — `fnsiggap.rs`, eighth session | **NEW, sized, and small.** `cyclegap.rs` found the 2,943-line row (§4.0's correction); the split refuses **72.5% of it as downstream** — 1,016 a parameter gaps, 985 a returned expression gaps, 132 both — leaving 810 lines in **four** mechanisms: plain 366, `async` 228 (wants `Promise<T>`), generator 173, async generator 43. 548 of the 810 take no parameters. Half the row (1,468 of 2,943) is reached only through the cascade. **≈0.16 points at 100% conversion**, and nothing converts at 100%. Buildable and honest about its size; `docs/architecture/checker-notes-cyclegap.md` §7 |
| 399 | `arguments` | §4.3 — three mechanisms wearing one spelling, needs the split first |

**Found by `calleegap.rs`, sixth session** — the callee-type family is **not** an
item, but two things fell out of splitting it, both effort 1:

| lines | item | note |
|---:|---|---|
| ~~25~~ **32** | ~~delete the stale namespace refusal~~ **LANDED at `a662de1`** | predicted stale by the probe's header *before* it ran; all four legs passed |
| ~~213~~ **LANDED, +114** | ~~**`resolveUntypedCall`**~~ `a1cbd93` | `checker.go:9935` → `:9902` → `anySignature`. 16.4 gained per wrong **after** one positional refusal (decline when the `any` originates in an **unannotated parameter** — that is contextual typing, §5's 2,082-line refusal arriving through a new door, and refusing it removes 64 of 77 misses for 36 conversions). **Two risks, both stated: 47% is one case** (`compiler/duplicateLocalVariable1`, 107 of 121), **and it needs an ADR-0038 argument written, not assumed** — `anySignature`'s return is `anyType`, an honest computation, not a rendered `errorType`, the same argument §2 used when `Array<any>` collapsed the ceiling |

**Small, owned, and honest about being small**: accessors 78 (`bd tsr-32y`),
computed names 219, design P's outer symbol chain 13, discriminated-union
narrowing on `switch` (`bd tsr-5kii`, **unsized** — do not quote the 20 that
exposed it), the value-name convertible bucket 70 (a *lower* bound).

**Refused, each with its number** — §5. Do not re-derive: inference's remaining
legs (~17), the ternary as an item (0 marginal), template literals (0.76 / 0.99),
`removeSubtypes` (1.03), contextual typing (86% entangled, reproduced a **third**
time this session *after* three call-resolution builds), JSX (blocked on
`declare global` augmentation).

> **The honest read: no single item on this list is worth more than a few hundred
> lines, and §4.4's conclusion has now been paid for twice over.** The two builds
> that moved this session (+3,590, +2,973) did not come from this table at all —
> they came from re-measuring a refusal that had stood for four cycles on a cost
> belonging to a different design. **The highest-expected-value move available is
> not the top row of §4.2; it is re-measuring §5.** Two of the entries there are
> older than four builds, and the last three sessions have retired more gradient
> by re-reading refusals than by ranking rows.

### 4.3 Measurement first — cheap probes that unlock a score

None of these can be scored yet, and each is one probe. **Quoting any of these
populations as work would break this file's fourth rule.**

| population | why it cannot be scored | the probe |
|---:|---|---|
| ~~6,233~~ | **`BinaryExpression` roots — DECOMPOSED at `edaf0e4`** by the new step arm in `depend.rs`: own-root is **1,712 lines at 37.6% want-any (~1,068 net)**, top case `logicalOrOperatorWithEveryType` — i.e. mostly the `\|\|`/`??` family §5 already refused on `UnionReductionSubtype`; 738 more are one pathological depth-cap case; the remaining ~5,000 of the old row propagate to operand roots and were never this row's | measured — nothing left to probe |
| ~~3,855~~ | **`TypeReference` — MEASURED AND DISSOLVED, sixth session** by the new `typerefgap.rs`. The row is **4,010** at `a4e3991`, and **4,002 of it (99.8%) is one mechanism: namespace-qualified naming**, `resolveEntityName` refused. Another **471** arrives through the `dependency types` ending, so the family owns **4,473** of this root. The "~1,867 of unknown cause" the row was carried on does not exist — the cause is known and it is already refused (§5). All six controls read their expected 0 | measured — nothing left to probe |
| 3,739 | **property access, "the property has no type"** — a *downstream symptom*: the property's own declaration gaps elsewhere. `bd tsr-mcd` established this and it is not an item | follow to the type-node roots, which is how tuples were found |
| 399 | **`arguments` — the largest single NAMED mechanism `valgap.rs` found**, and the only one of its buckets that is neither want-`any` nor already refused. Top-1 case 36.6%, and the name is literally `arguments` on all 399. **Do not cost it as "synthesise `IArguments`" without splitting it first**: across the corpus's `.types` baselines the name resolves to `IArguments` 132 times, `any` 120, and to ordinary user declarations that merely share the name (`number` 78, `any[]` 46, `string` 37, `"arguments"` 26) — three different mechanisms wearing one spelling. This is `docs/conventions.md`'s *"a population identified by the shape of the answer is not thereby attributed to a mechanism"*, caught before anything was costed | split the 399 by whether the name resolves to a real declaration in scope, a synthesised function-scope `arguments`, or nothing |
| 2,223 | **object-literal remainder** — accessors (`bd tsr-32y`) and computed names both fall into the catch-all, in unknown proportion. Accessors are **not** a copy of the method arm: upstream prints an accessor as a *property* | split the catch-all by member kind |
| ~~1,425~~ **7,685** | **unresolved value names — RE-SIZED AND SPLIT, sixth session** by the new `valgap.rs`, and **the published 1,425 was a different cell**: `depend.rs` at `a4e3991` reads this root at **7,685 lines, 73.9% want-any**. Split by what the baseline wants, the bucket that could convert — *a candidate exists, types, and prints **exactly** what is wanted* — is **70 lines**, in three scope families (another file 27, out of scope 28, namespace body 15). **Two of its controls fired and are reported rather than tuned**: C3 (31 depth-0 lines declared nowhere yet wanting a real type) and C4 (the mirror reproduces 85.2% against a ≥95% bar, because the identifier arm does more than read the symbol — which makes the 70 a **lower bound**, not an upper one) | measured; the 70 needs its floor re-taken against a counterfactual that models the producer's arm, not `get_type_of_symbol` |
| ~~4,088~~ | **`FunctionDeclaration` rows — DECOMPOSED fifth session** by `retgap.rs` (`checker-notes-callres.md` §12): 879 return-annotation gaps + 507 parameter-annotation gaps belong to unported type nodes (template literal types, variadic tuples, `const` type-parameter modifiers), 797 are downstream return-expression gaps, 466 async/generator, 257 annotated-everything-types unsplit, **81 multi-distinct aggregate refused with its number (§5)**. Return-type inference itself is ported; the row is its inputs | split complete — shards to their owners |

### 4.3a The call row cannot reach 10% of the gradient, at any conversion

Asked out loud this session and worth a line, because the figure has been
carried informally. Measured, not estimated (`callgate.rs`, §13 of
`checker-notes-callres.md`):

```
admitted call+new lines (callee already typed)   8,398  =  1.75% of 478,954
the widest call-shaped population ever measured  20,721 =  4.33%   (18,294 carrying + 2,427 cascade)
observed conversion band                         15–57%
```

So **every call-shaped line in the corpus, converted at 100%, is 4.3 points**,
and the band puts a realistic ceiling for the whole family near **+0.6 to +2.5
points**. Call resolution is the largest *family* on the board and it is not a
double-digit item. The three mechanisms it decomposes into are scored above.

### 4.3b ~~Structural assignability is unported~~ — **CORRECTED, same session**

> **This section was WRONG when first written, hours earlier in this same
> session, and it is corrected rather than edited away.** It claimed
> *"`relater.rs` compares object types only to themselves"*. Structural
> comparison of object types landed at **`e24b7ca`**, *387 commits before this
> section was written* and an ancestor of the commit that wrote it:
> `properties_related_to` / `property_names_of` / `collect_property_names` walk
> every property of the target, own and inherited over base symbols, and six of
> `tests/relater.rs`'s eighteen tests assert it. Verified against the code, not
> taken on report.
>
> **The source of the error was `crates/tsr-checker/src/lib.rs`'s crate doc**,
> frozen at the day `checker_types` read 0% and still saying "Object types
> relate only to themselves — structural comparison is not ported", under a
> heading "Why `checker_types` still reads 0%". It was quoted here in good
> faith. `bd tsr-7wkn`; the file now carries a STALE banner naming this page as
> the authority.

**The numbers survive; the diagnosis does not.** The seven dependents are real
and still blocked — `selectable.rs`'s 303 object-parameter lines and 44 union
lines, `namedcallee.rs`'s 48 disagreeing overload sets, `||`/`??`'s 358 + 96,
`removeSubtypes`, the destructuring defaults, the `ArrayLiteral` row. What
blocks them is **not a missing relation**. It is that the relation cannot say
*"I could not tell"*.

`checker-notes-assign.md` §2 names six sites that answer `false` without
knowing — an unfollowable base, an absent property whose counterpart may be
optional, either side lacking a members table (function and index-signature
types never reach the structural arm at all), the depth cap, generic member
types, and signature-bearing types. The last is the load-bearing one: for
signature-bearing types the relation is unsound **in both directions at once**,
so *no per-type flag predicate can separate the trustworthy pairs from the
rest*. **Decidability is a property of the pair, not of either type** — which
is precisely why widening `SELECTABLE` with more flags cannot work, and
`checker-notes-selectable.md`'s refusal stands on better grounds than the ones
it was written with.

~~So the real item is a **three-valued relation**~~ — **MEASURED AND BUILT,
sixth session, and the diagnosis in that sentence was wrong.** `bd tsr-kmzf` is
closed; `checker-notes-assign.md` §5–§6 carries it.

Leg 1 forecast **33** conversions against a floor of 150 — the falsifier the
registration flagged as likely. **Control C3 fired harder and is the finding:
all 33 are conversions the EXISTING BINARY relation already makes.** The
cross-tab it forced carries no `[NEEDS the ternary]` row anywhere, and
`stringLiteralTypesOverloads01` settles why: its overloads take
`"boolean" | "string"` parameters, which the binary relation decides without
difficulty and which `SELECTABLE` — a **flag set** containing `STRING_LITERAL`
but not `UNION` — never *asked* about.

> **The blocker was the gate, not the relation.** This section's own premise
> conflated the two, for the second time on this item: §4.3b was wrong about
> whether the relation existed, and its replacement was wrong about what the
> relation lacked. Both times the numbers were right.

Three-valuedness is worth **zero conversions** and is load-bearing for the
*safety* of deleting the gate — the 52 `UNDECIDED` lines are precisely what a
naive removal decides wrongly. Both landed together: **+94 net, 0 lost, 6 cases
finished, 0 regressed, Δwrong +3**, against a fresh bar registered before the
code (§6.1).

**What this does to §4.4.** Assignability is first of the five capabilities that
four cycles of ranking said the remaining gradient hides behind. On the
population it was named to unblock it is now measured at **33 lines**, and the
whole build — reaching wider than its counterfactual, through property-access
callees the probe never classified — was **+94**. One of the five is answered,
and the answer is that it was not where the mass is.

### 4.4 What the scores say about 80%

- Distance to **80%** is **+47,184 lines**; **70% is crossed** (70.15% at
  the `tsr-tgov` build, measured on the coverage instrument).
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
| contextual typing — **withdrawal itself withdrawn, refusal RE-ARMED** | 2,082 + 1,809 wrong | **86% entangled**, 48.8% behind call resolution. Marked WITHDRAWN at `b00738d` as possibly stale; **re-measured at `0d56467` (fourth session) and it reproduces exactly** — |G| 2,082, every row within 14 lines. **Reproduced a THIRD time at `a4e3991` (sixth session), and this time it means more**: the re-run comes *after* three call-resolution builds (`tsr-4sa`, `tsr-g30h`, and the ternary gate), and |G| is still **2,082** with stands-alone still **291 = 14.0%**. Improving call resolution has not moved the entanglement, which is the one thing that could have. The landed builds changed what a member's type is, not whether an argument position can be typed without resolving its call. `checker-notes-fnexpr.md` §10. Off the scored board until call resolution exists |
| **WITHDRAWN** — qualified naming build | ~~1,318~~ **4,557 classified; design W forecasts 1,702** | 90.7% accurate on target row; counterfactual **lost 3,202 lines, regressed 753 cases**. **The refusal STANDS on that measured cost — but its population was understated**, sixth session, `typerefgap.rs`: the `TypeReference` root alone contributes 4,473 lines of this family (4,002 of the 4,010 `no further dependency` row plus 471 more), and `compiler/temporal` supplied 2 of the 3 wrong lines the ternary gate build minted. A refusal priced against 1,318 has not been priced against 4,473, and the *cost* side (3,202 lost) was measured on a compiler seven builds older. **This is the strongest candidate on the page for a fresh counterfactual**, and it needs a new bar rather than an inherited one. **RUN, same session (`qualname.rs`, `checker-notes-qualname.md`): the refusal was of the WRONG DESIGN.** The item is two halves with different at-risk columns — **W** reprints the *written* entity name (1,770 converts, 99 wrong, **0 at risk**, because it fires only where the line already gaps) and **R** resolves and prints the bare name (384 / 1,025 / 381). **The 3,202-line loss was measured on the printing half and then quoted against the resolution half's population**, which is how a 1,318-line refusal came to block a 4,557-line row. Design **R stays refused on its own fresh number, 0.37 gained per wrong.** Design **W is now §4.2's top item** |
| **WITHDRAWN** — element access | 1,590 | **59.3% want `any`**; refused 3×. **Superseded at `b00738d`:** the `tsr-4qx` build collapsed the row 12,544 → 1,391 and the `want-any` share with it, to **19.2%**. The refusal was true of a population that no longer exists |
| `TemplateExpression` | 1,036 | cheap leg **not separable** — upstream's `evaluate` is a syntactic folder consulting no types |
| **WITHDRAWN** — module object (`tsr-6ph`) | 3,539 | ~~2.1 and 2.5 wrong per right, two designs~~ **RETIRED by re-measurement, seventh session** — both priced designs printed the module's *file path*; the alias-search naming that shipped after the refusal (c91314c, then `f62582e`/`897abdd`) was a third design neither number priced. Landed **+421 for 3 lost**. The remainder is the `export =` family (react/tsx head) and the no-alias file-module `import(…)` forms — see `checker-notes-modobj.md` §10 |
| ALIAS row | 5,207 | convertible set and spellable set are **disjoint** |
| `ArrayLiteral` wrong bucket | 1,773 | 36.7% one case; 42.3% is tuple inference in `contextual.rs` |
| wrong bucket case-flips | 37,709 | **81% symptom**; best actionable row flips 37 cases |
| `hadErrorBaseline` | 40,759 | ADR-0039 |
| **WITHDRAWN** — the export= follow (`tsr-e2u`'s stated blocker) | 218 C9 lines + the react/tsx head | The ALIAS-row refusal's spellability leg (0.0% export=) priced a compiler with no alias naming and no export= arm. **Both shipped, seventh session** (`952b328`): the chain converts 302 seed lines at 23 would-wrong measured, and the ALIAS row itself needs a fresh `nameres.rs` re-take before anything further is claimed about it |
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
| **the multi-distinct return aggregate (`tsr-4sc.9` leg)** | 81 | `retgap.rs`, fifth session: 60+18+3 gap lines across the whole corpus want a subtype-reduced union of return types. The machinery is `removeSubtypes` — refused at `tsr-eak` — for ~12–46 converted lines at the observed band |
| **`TemplateExpression`, both candidate designs** | 1,036 | fifth session, `checker-notes-tmplexpr.md`: cheap leg **0.76 gained per wrong**, folder-plus-fallback **0.99**. Supersedes the "not separable" wording with a number. Only a *complete* constant folder (arithmetic, booleans, const-enum members) changes either ratio |
| **the object-literal remainder as an item** | 2,223 quoted | fifth session, `checker-notes-objgap.md`: **77% is downstream** — every member kind has an arm and a member *value* gaps. The accessor item `bd tsr-32y` is **78 lines**, computed names 219. §4.3's "unknown proportion" warning was right and the proportion is 6% |
| **import aliases in value position — REFUSAL RE-CHECKED AND IT STANDS** | 2,404 row / 840 measured | fifth session, `checker-notes-novaldecl.md`: the largest unopened gap root is 93% import aliases, and `resolve_alias` already refuses them **for a naming reason** — an alias prints its own name, so resolving `import * as ns` would print `typeof <stripped file path>`. `bd tsr-4jk`; same family as qualified naming. **Re-checked at `a57a04b`, sixth session, on the theory that shipping both halves of qualified naming had dissolved its premise. It has not.** The stated cause is a *naming* one — resolving `import * as ns` would print `typeof <stripped file path>` — and the machinery that would name a module container, upstream's `getSpecifierForModuleSymbol` (`nodebuilderimpl.go:667`), is **still unported**: `symbol_chain` (`checker.rs:654`) returns `None` the moment the container is a module symbol, deliberately. The refusal stands **and now names its own unlock**, which is the row directly above in §4.2a at 1,094 lines. Cost of the re-check: one grep |
| **the `this` half of `getApparentType`'s head** | 827 | fifth session, `checker-notes-apparent.md`: **zero** convertible. 522 find the member and gap on its own type; 305 are absent from the class's declared type entirely — filed as a separate members-table question |
| **the two callee-type gates as an item** | 2,276 | sixth session, `checker-notes-calleegap.md`. The two rows are **positionally disjoint** — 1,398 is 100% `new`, 878 is 100% call, and the shared label is **two counters spelled the same**, not one mechanism. **58.8% (1,338 lines) belongs to refusals that already exist**: generic candidate 923 (inference, refused this session at ~17), class instance types 268, `unique symbol` 291 — which reproduces `tsr-4sa` §4.1 **exactly, five builds later** — disagreeing overloads 88, heritage 27, type-parameter return 3. The *"everything handled, something downstream gapped"* bucket reads **0**; this is not the object-literal row. Neither gate is an item. What they are is **a second door onto seven populations that already have owners**, and the two things worth having came out of the split rather than the row: a stale refusal deleted (+32) and `resolveUntypedCall` (+114) |
| **the flag-test form of `isUntypedFunctionCall`** | 328 gained / **248 wrong** | sixth session, `checker-notes-calleegap.md`. Porting `IsTypeAny(funcType)` literally manufactures **248 wrong lines against 328 gained — 1.3 per wrong**, worse than every refusal on this page bar `removeSubtypes`. The predicate is a *faithful* port and it is still unsound here, because **the two compilers disagree about which types are `any`**: upstream reaches it only where the source said `any`. The written-annotation form landed instead at **114 / 0** |
| **the priority LATTICE, the other inference leg** | 11 converts / 1 wrong | sixth session, `checker-notes-infer2.md` §7. Sized because the contravariant refusal below had the obvious objection that it priced one leg of a two-leg mechanism and the lattice was the one `inference.go`'s structure makes look larger. **It is 11 lines**, and it is the *better* of the two — both together are ~17. Measured as a delta over the shipped arm so the legs cannot double-count, with at-risk **0** in the same pass over 792 admitted right lines. Its control C4 under-reports in the safe direction (239 of 792 the probe gaps, 0 it answers differently), so the zero is readable. **The row that carried this at 2,056 was a row: the mechanism's own population is 1,231 and 77.3% of it is elsewhere** — 494 find no candidate at all, 458 are downstream of a gapping argument |
| **the contravariant inference bucket** | 6 own-node / ~36 with cascade | fifth session, `checker-notes-infer2.md` §6. Recommended by me on "the same mechanism owns both the losses and the largest share of the new wrong" — and that was **a ceiling on a row, not a forecast of a mechanism**, this file's fourth rule catching its own author. Re-derived line by line: of the 120 wrong lines in the four contravariant-named cases, **4** are this mechanism; 116 belong to five other items. It also needs `strictFunctionTypes`, an option this port does not model, and guessing a sibling flag wrong once cost 1,221 lines. A tenth of the smallest thing ever refused here |
| **destructuring an array literal without the pattern's contextual type** | ~80 wrong minted | the first `tsr-o00` run answered `var [a, b] = [1, "x"]` elements as `string \| number`; upstream infers the **tuple** through `getTypeFromBindingPattern`'s implied contextual type (`checker.go:16748`). Approximating it passed the bar's ratio leg (6.0×) and was refused anyway — the construct refuses whole until `tsr-84iz` builds the mechanism |
| **`bd tsr-kmzf` — a three-valued relation, as an ITEM** | 33 / 0 | sixth session, `checker-notes-assign.md` §5–§6. Leg 1 forecast **33 against a floor of 150**; control C3 then showed **all 33 are conversions the existing BINARY relation already makes**, so the mechanism's own marginal yield is **0**. Refused as an item and **shipped anyway**, because it is what makes deleting `SELECTABLE` safe — the 52 `UNDECIDED` lines are what a naive removal decides wrongly. Do not re-open as "make the relation three-valued": that is done, and it converts nothing |
| **widening `SELECTABLE` as a flag set** | — | same measurement, and it is a *structural* refusal rather than a numeric one. Decidability is a property of the **pair**: for signature-bearing types the old relation was unsound in **both directions at once**, so no per-type flag predicate separates the trustworthy pairs from the rest. The flag set is now deleted rather than widened |
| **the `any`-parameter overload set** | 7 conversions | sixth session. `any` relates to everything both ways, so such a candidate is trivially applicable and declaration-order selection always stops on it — a **wrong rule, not a bad trade**. Refusing it positionally costs 7 conversions and removes **24** would-be-wrong lines: 40/26 → **33/2** |
| **the overload braces form of the composite-print seam** | 939 ceiling | seventh session, `sigprint.rs` braces mode (self-checked): **14 converts / 30 would-wrong (0.47 per wrong) / 330 of the ceiling unmodelable** — the wrongs are inside-signature defects (Intl/Temporal option unions), not naming. The single-signature twin's +1,500 is the seam's harvest; this shard is not its sibling |
| **TS7026 as a `diagnostics` rule** | 27 cases | eighth session, `checker-notes-diag2.md` §13. Built and measured at **12 conversions for 47 wrong lines, 0.26 per wrong**. One cause, already on record: the corpus's JSX-heavy cases declare `namespace JSX` inside `declare global { … }`, and **global augmentation is unported in this binder** — the `/.lib` mount made `react.d.ts`'s *plain* global namespace visible and an augmented one is still not. So "`JSX.IntrinsicElements` does not resolve" reads as *"this binder cannot see it"*, the third subsystem in one session where an absence and an unbuilt table are the same answer. Returns when `declare global` merging lands; that item's `.types` side is in `checker-notes-jsx.md` |
| **TS2339 as a `diagnostics` rule** | 133 cases | eighth session, `checker-notes-diag2.md` §9. Built to the tightest available bound — fire only where the receiver type carries a resolved members table — and measured at **2 conversions against 254 wrong lines, 0.008 per wrong**, two orders of magnitude below the worst refusal on this page. The diagnosis is structural and is the reusable part: **an absent property and an unbuilt members table are the same `None`**, and `Named { members: Some(_) }` says a table was built, not that it is complete — heritage, mapped, conditional and mixin members are resolved lazily by other arms. Same shape as the `SELECTABLE` refusal, in a second subsystem. Returns when `resolveStructuredTypeMembers` carries an explicit resolved state per type rather than an `Option` that conflates "no members" with "not yet" |
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
| `examples/verdictdump.rs` | **the transition probe, and the only one that can say where a wrong line CAME FROM.** One verdict per aligned line (`case:file:position`), so two runs give the exact matrix — `WRONG→RIGHT`, `RIGHT→WRONG`, `GAP→WRONG`. `wrongdelta` cannot distinguish a gap→wrong arrival from a right→wrong one and that distinction is what `docs/conventions.md` requires a bar's absolute to be written against; design P's fourth leg is scored on it. Also the first probe whose `right + gap + wrong` is an identity rather than a cross-instrument subtraction — §1 |
| `examples/qualname.rs` · `qualnamep.rs` | the two halves of namespace-qualified naming, priced separately: `qualname` the **resolution** half (designs W and R, which convert *gap* lines), `qualnamep` the **printing** half (design P, which converts *wrong* lines and cannot touch a gap). **`qualname.rs`'s at-risk-P column of 36 is superseded by 23** — it omits `getMergedSymbol` |
| `examples/cyclegap.rs` | **what the gap board does NOT attribute, and what the gap WANTS.** `depend.rs`'s `cycle` and `depth cap` endings resolved (the first is 98.6% a length-1 self-loop — a missing step arm, not a recursive shape), plus the whole-gap **want-shape** histogram, which is the only view of the gap that is not a node-kind histogram. Its C2 is frozen against `depend.rs`'s cycle/depth-cap counts: if they stop reproducing exactly, the copied walk has drifted and nothing it prints is readable |
| `examples/fnsiggap.rs` | the un-annotated `FunctionDeclaration` row split **downstream-first** — parameters and return expressions checked before the line may enter the wiring bucket, so the flattering answer is the one that has to earn it. Its C1 froze against `cyclegap.rs` and **fired on the first run** (1,475 vs 2,943): the probe had tested the line's own node while the row is defined by where the *walk ends*. Half the row is not at its own node — a distinction any forecast on this family has to carry |
| `examples/wrongdelta.rs` | **`casedelta`'s sibling for the wrong bucket** — raw joinable `want`/`got` dump; two runs over a `git stash` attribute every gap→wrong line, which `casedelta` cannot see by construction |
| `examples/diaggap.rs` | **the `diagnostics` board.** Ranks the suite's failures by the code each case is blocked on, and prints the only forecastable column — cases missing **exactly one** distinct code and reporting nothing extra — beside the ceiling column that must not be mistaken for one. Also the false-positive table: codes this port emits where the baseline does not, which is the cost side every diagnostic rule has to be scored against. **Re-run it after every rule**: rows grow as rules land, because a case blocked on two codes becomes a case blocked on one |
| `examples/diag2307.rs` | the per-rule counterfactual for `diagnostics`. Calls `diagnostics_suite::reported_for` — the suite itself — and reconstructs the *before* side by removing the codes under test, so probe and build cannot diverge. `CONVERTS` / `LOST` / `RIGHT` / `WRONG`, with `LOST` the leg that must read 0 |
| `fnexpr` · `nameres` · `evolvearray` · `thisparam` · `receiver_gap` | per-workstream |

Five gates, all green before every commit — **on the toolchain
`rust-toolchain.toml` pins (1.96.0)**, which is the only version their verdict
is defined against. Do not report a lint gate's colour without saying which
rustc read it; one session published a red clippy that was 1.89.0's opinion.

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
| 2026-08-07 | `0fab830` | untouched | untouched | **`diagnostics` 80 → 717 (1.46% → 13.06%), +637 cases, 0 regressed, 0 lost** | **The eighth session: the suite two handoffs called "structurally blocked" moves 8.75×, and the unblocking was a measurement rather than a build.** ADR-0040's diagnosis was right about *why* the number was flat and was being quoted as a *sizing*; `diaggap.rs` (new) asked what the blocked cases are blocked **on** — **3,258 of 5,488 on exactly one code**. Then ADR-0040 decisions (1) and (2), a `Checker`-owned collection and a `check_source_file` traversal, and eight builds: **TS2307/2882** +50, **TS2564** +133, **TS2304** +115, **TS2454** +233, **TS2369/2695** +40, **break/continue grammar (5 codes)** +25, **TS1036/1183** +24, **the overload implementation-expected family** +17. Two **refusals with numbers**: TS2339 at 2 converts / 254 wrong and TS7026 at 12 / 47. `checker_types` byte-identical throughout — the traversal is a second entry point and no query call site invokes it. **Four transferable findings.** (1) *The gates are the design*: upstream's `resolveExternalModule` carries fourteen messages and TS2307 is what is left when thirteen decline; ungated 45/0/100/**60**, gated 50/0/101/**21**. (2) *`tsr_ast::NodeFlags::AMBIENT` is declared, documented and set by nothing* — it caused **three** separate residuals (86 wrong lines, 4,781, 2) and a flag with no writer is a landmine with one instance per reader. (3) *A wrong column dominated by one case is one predicate*: `genericDefaults`'s 227 lines were `declare const`. (4) *A syntactic rule has no incompleteness to leak* — the three grammar builds converted 89 cases for **zero** wrong lines and needed no tightening pass, where all four semantic rules needed two or three. Forecast **exact twice** (50, 133) because the counterfactual calls the shipped code. Scored on `passing + single-code reachable`, the only currency that prices a false positive: TS2304 +56, TS2454 +28 — and **−85 / −210** respectively before their last refusals landed |
| 2026-08-07 | `7299a14` | 73.65% | 2,841 | 0 lines, by design | **the eighth session opens by falsifying the seventh's closing sentence.** Board re-run fresh: gap **80,315**, every row unchanged — and *"no unowned row remains above 1,000"* holds only over the endings `depend.rs` **attributes**. `cyclegap.rs` (new) measured the two that attribute nothing (8.4% of the gap): the `cycle` ending is **98.6% a length-1 self-loop**, every one a declaration with neither annotation nor initialiser, i.e. `NO STEP ARM` under a label that says "a real shape here". **`FunctionDeclaration`-cycle is unowned: 2,889 net, want-any 1.8%, 611 cases, 1,894 wanting a signature.** `ArrayType` 2,120 is 99.7% *propagation* (2,114 have a gapping child type node); 4,696 of 8,499 type-node roots are the same. And the first whole-gap **want-shape** histogram: **24.9% wants `any`**, and **signature/arrow is 12,606 lines (15.7%)** spread across six rows — §4.4's structured-signature capability seen from the answer side. Both `depend.rs` fixes named and not made, so the board stays comparable. **Then the row was split the same session (`fnsiggap.rs`) rather than left as a candidate: 72.5% is downstream (a parameter or a returned expression gaps) and the item is 810 lines / 779 net in four mechanisms — plain 366, async 228, generator 173, async generator 43. ≈0.16 points at 100% conversion.** Its C1 fired on the first run (1,475 vs 2,943) and diagnosed the *probe*: it had tested the line's own node, while half the row is reached only after ≥1 step. **So the strongest unowned candidate the board has had since the seventh session closed is worth under two tenths of a point** — §4.4 arriving through another door. `docs/architecture/checker-notes-cyclegap.md`. **Also recorded, then CORRECTED the same session: ~~the clippy gate is RED at `7299a14`~~** — reported as red on `crates/tsr-ast/tests/kind_conformance.rs:83` (`inefficient_to_string`). **It is green.** The reading came from a machine whose default toolchain was **1.89.0**; on the 1.96.0 the workspace now pins, `clippy --workspace --all-targets -- -D warnings` finds **nothing**, and CI's `@stable` had been green on that file since the scaffold commit. The diagnosis in the original note ("toolchain drift, not a regression") was right and the *verdict* was wrong — **an unpinned lint gate is not a gate, because its answer is a property of whoever ran it**. Fixed at the cause: `rust-toolchain.toml` pins 1.96.0 and both workflows pin the same version at the action. **Left standing and unresolved: `Cargo.toml` claims `rust-version = "1.85"` and the workspace uses let-chains, stable in 1.88 — the MSRV claim is false and is NOT what this session pinned; it needs its own measurement** |
| 2026-08-05 | `058b4a9` | 61.09% | 2,173 | — | baseline for the session below |
| 2026-08-07 | `154653b` | **73.30%** | **2,793** | **+2,664 in the session's third act, 6 lost, +26 cases, 0 regressed** | **The JSX element arm and the composite-print seam, each a stale premise re-measured.** JSX (+1,153, `9fe8056`): the "46% cannot resolve" figure predated the `/.lib` mount; the arm is 30 lines and converted 152% of forecast. The seam (`bd tsr-2ghn`, filed, sized and SHIPPED in one session): symbol-exact counterfactual (`sigprint.rs`, self-check leg) → the Union/Intersection naming arm it exposed (+11, `0796633` — a class-typed return qualified while an alias-typed parameter did not, ONE missing match arm) → the site-aware twin `signature_to_string_at` (+1,500 — **forecast delivered to the line**, RIGHT→WRONG exactly the measured 2, because probe and build are the same function). Plus the default-import arm (+83, `cc8c422`) with the `default`-never-prints refusal cleaning 67 pre-existing wrongs |
| 2026-08-07 | `cc8c422` | **72.74%** | **2,767** | **+859 across five checker builds + one harness mount, 5 lost, +17 cases, 0 regressed** | **The seventh session's second half: the whole export= / naming family, each slice with a registered bar.** After the ambient and container-qualifier slices (rows below): the **export= chain** `952b328` (+297 — `ExportAssignment` arm, `import a = require` follows `resolveExternalModuleSymbol`, and `best_name`, the innermost-table walk; **leg 4 fired at 130 lost on the first measurement** and the fix was upstream's own `useOnlyExternalAliasing` flag — the counterfactual's alias reduction was blind to same-file `import a = b`, a recorded probe defect); the **chain-segment rename** `67949ee` (+58, WRONG→RIGHT 58 with *no other transition*); the **default-import arm** `cc8c422` (+83, Δwrong −38 — its first measurement fired leg 2 at 67 and 40 of those printed **`default` as a name**, which upstream never does; the rendering refusal cleaned 67 pre-existing wrongs too). `bd tsr-6ph` and `tsr-6j2` CLOSED; residue filed as `tsr-epnz`, `tsr-wwum`, `tsr-fpti`. Fresh `depend.rs` at `cc8c422`: gap 81,769; every head row is owned (contextual typing, call/inference, property-access symptom, modulespecifiers, JSX instance typing) |
| 2026-08-07 | `93b540a` | **72.65%** | **2,754** | **+421 across three builds, 3 lost, +4 cases, 0 regressed** | **The seventh session: the module-object refusal retired by re-measurement, exactly as the handoff predicted.** `tsr-6ph` stood refused at "2.1 and 2.5 wrong per right, two designs" — both designs printed the module's *file path*, and the alias-search naming that shipped since (c91314c, designs W/P) was never priced against it. Fresh counterfactuals (module_object.rs alias-search columns, qualnamep.rs container-qualifier arms): (1) **`tryFindAmbientModule`** `f62582e` +128 — its leg-2 global fired at +345 and split by arithmetic into ~15 mechanism-own + 328 correct-types-wearing-unqualified-names with named owners; (2) **the container-qualifier arms** `897abdd` +293, RIGHT→WRONG = exactly the 3 pre-named at-risk lines, Δwrong −293; (3) **the `/.lib` mount** `93b540a`, measured 0, shipped with the zero stated — it bought the finding that the tsx `typeof React` family (largest remaining head) sits behind `export = __React` + alias-preferred naming. **Two probe defects caught before quoting**: both ambient tests keyed on upstream's *quoted* symbol name, which this binder never stores (binder.rs:4091) — `tsr-xpb8`'s "ambient = 0" was re-taken on a sound test and survived. `bd tsr-xpb8`'s ambient half is shipped; its file half still needs modulespecifiers for the no-alias cases (107 want `import(…)`) |
| 2026-08-07 | design **P** | **72.53%** | **2,741** | **+2,973, 17 lost, +61 cases, 0 regressed, gap→wrong 0** | **`getSymbolChain` — the printing half of qualified naming, and the second four-cycle refusal retired by re-measurement in two sessions** (`checker-notes-qualname.md` §10–§11). The refusal's cost (3,202 lost) was cycle 20b's; re-taken on a post-W compiler it is **14**. Sized at 2,990/14, bar registered at `90c4e70` **before any code**, **all four legs passed** — and the conversion column landed on **2,990 exactly**, 100.0% of forecast, because §10.5 registered in advance that this forecast was a *point estimate* rather than W's floor and said not to expect another 211%. **Leg 4 is the one worth copying**: P is a renaming, so it cannot make a gap line computable, and `gap→wrong == 0` was registered *by construction* — making a non-zero reading a **diagnosis** ("the build changed resolution too") rather than a trade. That is `docs/conventions.md`'s post-W rule finally written as a leg instead of a caveat, and the instrument that scores it (`verdictdump.rs`) is the reusable part. Two corrections: **`qualname.rs`'s at-risk 36 is wrong and is 23** (it omitted `getMergedSymbol`, `symbolaccessibility.go:696` before `:702`), then measured ON against OFF on the real build at **13 losses removed for 0 conversions**; and the unit fixture written to pin that clause **does not** — checked by deleting the merge and re-running, it still passes, because the binder resolves a single-file merge as it binds and all 13 lines are *cross-file*. Conceded: 17 real losses (9 to `bd tsr-4jk`'s alias chain, 4 to `getContainersOfSymbol`'s unported multi-parent `sortByBestName` ordering, named case `compiler/giant`), and **64.1% of the gain is `compiler/temporal`** — leg 3's `cases gaining ≥ 60`, read 174, is what proves it is not a one-case build |
| 2026-08-07 | `a1cbd93` | **72.56%** | **2,750** | **+114, 0 lost, +9 cases, 0 regressed, gap→wrong 0** | **an untyped call answers `any`** (`resolveUntypedCall`, `checker.go:9902`). The **ADR-0038 objection was answered from upstream before a bar was written**: `anySignature` returns `anyType` (`checker.go:1042`) and `unknownSignature` returns `errorType` (`:1043`) — two adjacent lines, two situations, and upstream's *error* path is the separate `resolveErrorCall`. The ADR was **not amended**; it is immutable and unchanged. **The first design was wrong and its number is the finding**: the naive flag test read **328 gained / 248 manufactured wrong**, leg 4 fired at 248 against 20, and the bar's *second named falsifier* had predicted exactly that check. The residual said why in one line — `want string got any` was 137 of 248. **This port's `any` and upstream's are not the same claim**: upstream reaches it only where the source *said* `any`, this port also reaches it wherever an unported mechanism gives up, so `IsTypeAny` is sound upstream and unsound here. Narrowed to *written in an annotation*: **114 gained, and nothing else moved at all**. All four legs pass; **falsifier 1 did not fire** — it predicted 47% in one case, measured top case is 30 of 114 and net-minus-top is 84 against a threshold of 50, so the item is more diffuse than it was sold as. The narrowing costs **214 conversions to remove 248 wrong lines** and is taken anyway: answering off a premise the two compilers do not share is a wrong rule, not a bad trade. It also **subsumed** the registered positional refusal, whose predicate was deleted rather than left dead |
| 2026-08-07 | `a662de1` | **72.54%** | **2,741** | **+32, 0 lost, 0 regressed, gap→wrong 3** | **the callee-type family split (`calleegap.rs`) — and it is NOT an item.** 2,276 lines, and the two gates are **positionally disjoint**, proved by a cross-tab against the blocking node's kind rather than by impression: `new` owns all 1,398, `call` owns all 878, and the label collision is a `define_counters!` declaration-order shadow (`calls.rs:737` vs `:400`). The "downstream" bucket, put first as the rule requires, reads **0** — no inflation, unlike the object-literal row's 77%. **58.8% is inherited refusals** (`tsr-4sa`'s six families plus inference, refused this session at ~17); the generic-candidate bucket alone is 923 lines and is inference's. Two things fell out, both effort 1: a **stale namespace refusal deleted** (+32, all four legs passed — it had declined 35 lines to avoid **3** wrong ones, right about the phenomenon and an order of magnitude wrong about its size), and **`resolveUntypedCall`** sized at 213 with a named positional refusal. Two controls fired and both diagnose the **probe**: C6's 292 is 291 `SymbolConstructor` reproducing `tsr-4sa`'s 291 **exactly, five builds later**, and C1's 53 is inherited from `callgate.rs` itself. **Process miss recorded: `246f004` shipped with clippy at 8** — the count was printed in the same compound command as the commit, the third instance of this failure here, and the fix is that the gate and the commit must be separate invocations |
| 2026-08-07 | `a57a04b` | **72.53%** | **2,741** | **+2,973, 17 lost, +61 cases, 0 regressed, Δwrong −2,973** | **design P — the symbol chain**, the other half of qualified naming and the second half of a refusal that stood for four cycles. §8 sequenced it (*"W first, then re-measure P against whatever W leaves"*) and that turned out load-bearing rather than tidy: **W's conversions created P's population**. Re-sized from scratch (every prior P number was pre-W), bar registered at `90c4e70` **before any code**, and **all four legs passed** — net 2,973 against a floor of 800, trade 175.9:1, 0 cases regressed, and leg 4's `gap→wrong == 0` reading a structural **0** because P is text-only. **The forecast was exact: 2,990 forecast conversions, 2,990 delivered.** The four-cycle refusal ("lost 3,202, regressed 753") was of the design that qualifies *everything* — measured here at **717 at risk, 5.4:1** — while the design honouring upstream's `needsQualification` stop conditions loses **17**. A control (CP6) fired during sizing and was traced to a missing `getMergedSymbol` in the *earlier* instrument (`symbolaccessibility.go:696` before `:702`); §11.2 then re-measured that clause on the **build**, ON against OFF: 0 conversions cost, 13 wrong lines removed. New `verdictdump.rs` makes `right + gap + wrong` an identity from one pass, and offers 10,039 unaligned lines as the **probable** explanation of §1's "9,145 short" — flagged as a hypothesis, not a reconciliation |
| 2026-08-07 | `8e28971` | **71.91%** | **2,680** | **+3,590, 0 lost, +17 cases, 0 regressed, Δwrong +84** | **qualified type names reprint what was written — and it exists because a REFUSAL was re-examined, not because a new row was found.** §5 had refused this on "lost 3,202 lines, regressed 753 cases"; the fresh counterfactual showed that cost was measured on the **printing** half and then quoted against the **resolution** half's population, so a 1,318-line refusal had been blocking a 4,557-line row. Design **W** (reprint the written entity name) has *no at-risk population at all* — it fires only where the line already gaps — and forecast 1,702/20/0. Built, it converted **3,590 = 211%**, because the counterfactual's own first named falsifier ("the unscored buckets are conversions") fired. **The bar's third leg fired at 84 against 40 and is OVERRIDDEN, loudly**, by a third party who wrote neither bar nor build (`checker-notes-qualname.md` §9.6): hypothesis one was eliminated *by measurement* (the refusal was disabled and re-run), and the bar's own stated rule — "twice the forecast 20" — re-evaluated against the population that turned up gives `2 × 20 × 3590/1702 = 84.4` against a measured **84**. 0 lost, 0 regressed, 0 right→wrong traffic; 47 of the 84 are other mechanisms' defects newly *exposed*, which §9.5 measured cannot be refused away except at 11.8 conversions per wrong line. Two sub-refusals priced: INSIDE kept at 1.2:1, ENUM **declined on principle** — `enumLiteralTypes3.types:9` records `>Yes : Choice.Yes`, so the lead's suggested refusal would have refused a shape the port already gets right. The rule it bought: **an absolute on *global* Δwrong tightens as the build improves**, and belongs against the mechanism's own new wrong |
| 2026-08-07 | `8965426` | 71.16% | 2,663 | 0 lines, by design | **three rows retired by measurement, two teammates in parallel with the build above.** `typerefgap.rs`: the `TypeReference` row is **99.8% one mechanism** — namespace-qualified naming — so §4.3's "~1,867 of unknown cause" does not exist, and the qualified-naming refusal is **re-sized from 1,318 to 4,473** on this root alone. The refusal stands on its cost (lost 3,202, regressed 753) but that cost is seven builds stale, which makes it the page's strongest candidate for a fresh counterfactual. `valgap.rs`: §4.3's 1,425 was **a different cell** — the root is 7,685 — and the convertible bucket is **70 lines**, with **two controls firing** and C4's diagnosis making the 70 a *lower* bound. `fnexpr` reproduced |G| 2,082 / 14.0% a **third** time, now after three call-resolution builds: the one thing that could have moved contextual typing's entanglement did not |
| 2026-08-07 | `a4e3991` | **71.16%** | **2,663** | **+94, 0 lost, +6 cases, 0 regressed, Δwrong +3** | **the sixth session — assignability answered, and it was not where the mass is.** `bd tsr-kmzf`'s bar said run leg 1 before writing checker code and named its own falsifier as likely; both fired. The ternary relation is real (`Related`/`NotRelated`/`Unknown`, Kleene composition, **behaviour-neutral by construction** — `is_type_related_to` is *defined* as `relate_ternary(..) == Related`, and `checker_types` was bit-identical across the refactor, which is what made the forecast readable). Leg 1 read **33 against a floor of 150**, and **control C3 fired harder**: all 33 are conversions the existing **binary** relation already makes. The blocker was `calls.rs`'s `SELECTABLE` **flag set**, not the relation — my own C3 premise had conflated the gate with the relation, the **second** time this item's diagnosis was wrong while its numbers were right. Gate deleted, `choose_overload` now asks about the **pair**; the `any`-parameter positional refusal priced at 40/26 → **33/2** before shipping. The registered falsifier (net > 60) fired and was followed: `objectCreate`/`objectCreate2` supply 18 of the 94 through a **property-access callee the counterfactual never classified**. Residual read despite a passing ratio — 2 of the 5 lines entering the wrong bucket are the already-refused qualified-naming family, 2 were wrong before and are wrong differently, **exactly 1 is new.** A workspace example failing to compile made `cargo test` print **zero** result blocks; `grep -c` caught it where `head` would not have |
| 2026-08-07 | `1c20e57` | **70.85%** | **2,617** | **+3,352 across six builds** | **the fifth session's second half, two teammates in isolated worktrees.** Landed: constructor type nodes **+1,358**, type predicates **+1,041** (with a parser ASI fix that moved `parser_typescript` and `binder_symbols` **up**), private names **+590**, `tsr-84iz`'s pattern-implied tuple **+206**, `getApparentType`'s instantiable head **+156**. **Five rows were retired by measurement rather than converted** — `TemplateExpression` refused with a *ratio* (0.76 and 0.99 gained per wrong, replacing "not separable"), the object-literal row shown **77% downstream** with the accessor item at 78 lines not 2,223, the `FunctionDeclaration` row decomposed to unported type-node inputs, JSX re-scored to 0.25 feasibility, and the largest unopened root (2,404) shown to be **93% import aliases already refused on naming grounds**. Two registered bars fired and both were build bugs, caught by the bar: `tsr-rppd` at net **0** (no symbol route existed) and `tsr-84iz`'s leg 3 vacuous at `0 < 0` |
| 2026-08-06 | `tsr-tgov` | **70.15%** | **2,564** | **+686, 0 lost, +24 cases, 0 regressed, Δwrong −47** | **`new C<T>()` instantiates** — the call side already substituted written type arguments (`inference.rs:153`) while `check_new_expression` refused at its first line; 826 lines sat in that asymmetry. Sized by a **counterfactual** (`newgen.rs` forecasts the printed string against the baseline: 166 exact, 1 miss named in advance) rather than by the row, bar committed first. Conversion **413% of the forecast** — the cascade, named as upside in the registration. Reading the 68 residual instead of banking an 8.4× ratio found upstream's one name-independent quoting rule (a **method** named `new` prints `"new"`, `nodebuilderimpl.go:2384`), worth **+114 and 84 pre-existing wrong lines**. Twelfth stand-in fixture came due |
| 2026-08-06 | probe | — | — | 0 lines, by design | **`bd tsr-klm` answered — and it re-scored the board's top item down 8×** (`callgate.rs`, `checker-notes-callres.md` §13). Control C3 fired on 2,569 of 8,398 lines and the diagnosis was the *instrument*: `check_new_expression` had no counters at all, and it is the more admitted half of the row. Six gates added there, four splitting `single candidate`, `checker_types` unchanged across the change. The result: **overload selection owns 1,095 lines, not 9,660** — that figure was the row, not the mechanism — the largest gate is **inference at 2,324**, and the whole call family cannot reach 10% of the gradient at any conversion (§4.3a) |
| 2026-08-06 | probes | — | — | 0 lines, by design | **two rows leave the board by measurement** (fifth session): `retgap.rs` decomposes the `FunctionDeclaration` row — return-type inference is already ported; the mass is unported type-node *inputs*, and the multi-distinct aggregate is refused at **81** (`checker-notes-callres.md` §12). `jsxfeas.rs` walks `getJsxElementTypeAt`'s path per line — **46% of the JSX row cannot resolve the namespace** (`declare global` augmentation, a binder prerequisite now blocking two items) and **29% is the refused qualified-naming family** (`checker-notes-jsx.md`). The cheap-probe-first ordering closed two items for two probes' cost |
| 2026-08-06 | `tsr-xs0` | **70.003%** | **2,540** | **+37, 0 lost, +3 cases, 0 regressed, Δwrong −37** | **assignment narrowing keeps a fresh boolean literal fresh** (`flow.go:2421`, `checker-notes-narrow.md` §7) — found by `tsr-o00`'s wrongdelta, sized 73+47 from the live wrong dump, bar registered before the four-line fix; every leg passed with zero downside and the mechanism's named case (`literalFreshnessPropagationOnNarrowing`) converted. **This crossed the exact 70% threshold** |
| 2026-08-06 | defaults | **69.998%** | **2,537** | **+66, 0 lost, 0 finished, 0 regressed, Δwrong 0** | **defaults under a typed annotation** (`tsr-o00` §6) — the `IS_UNDEFINED` facts bit (the port's facts comment had already drawn the `UndefinedFacts`/`VoidFacts` line it splits), the `checker.go:17782` strip, sized 69/net 59, converted 112%, every bar leg passed with zero downside. The coverage display now reads 70.00% **by rounding**: the exact threshold is 12 lines away, said so it is not quoted as crossed |
| 2026-08-06 | `4b81458` | **69.98%** | **2,537** | **+1,081, 1 lost, +16 cases, 0 regressed, Δwrong +109 (9.9×)** | **binding elements** (`tsr-o00`, fifth session) — sized by the new `bindgap.rs` (1,228 buildable of 2,632, six refused legs each with a number), bar registered and committed before code (`982bfe4`). The first run read +1,239 at 6.0× and **passed every leg while minting ~80 wrong lines** from approximating pattern-contextual tuple inference — refused whole on the faithfulness rule, not the ratio (`tsr-84iz`). The parser now records array-binding holes as all-nil `BindingElement`s (upstream `parser.go:1663`); skipping them renumbered every element after a hole. The eleventh unported-stand-in fixture came due (`types.rs`). Residuals filed: `tsr-pqnh` (flow-of-destructuring, the family the bar named in advance), `tsr-xs0` (assignment narrowing drops freshness — pre-existing, exposed). 70% now sits **+78 lines** away |
| 2026-08-06 | `acdeed5` | **69.76%** | **2,521** | **+43, 0 lost, 0 regressed, Δwrong −19** | **the `in` guard** (`tsr-q9g` §6.1). The first run measured +45/−13 — numerically passing every leg — and **the 13 losses in one case were a bug the ratio would have priced as a trade**: the presence test read `SymbolFlags::OPTIONAL`, which this binder never writes, collapsing optional-property else-branches to `never`. Fixed to read the declaration's question token; the pair is pinned |
| 2026-08-06 | `e7a65fb` | **69.75%** | **2,521** | **+745 net (767/22, 34.9×), 0 regressed, +12 cases, Δwrong −448** | **`typeof` guard narrowing** (`tsr-q9g`'s typeof form) — `Relation::Subtype`/`StrictSubtype` in the relater, the sixteen typeof facts bits with per-kind aggregates, and the `narrowTypeByTypeof` arm family. Sized twice by probes before building (access lines 27, identifiers **440 with 417 wrong**); the bar's primary leg was `wrongdelta` for the first time, and it read **−448**. Conversion **169%** of the sized row. A third "unported stand-in" fixture came due (`narrowing.rs`'s typeof guard) and was replaced with a comparability pair. Residual 260 new wrong in three owned families: loop fixpoints (unported incomplete-types iteration), further narrows (`tsr-97d`), want-`any` ceiling |
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

**Fourth session's process miss:** `180bcb0` shipped with clippy RED — the
compound command printed the count (5) and committed anyway, the same class as
the `head`-piped gate: instrument correct, reading skipped. Fixed and recorded
at `5a6d735`.

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
