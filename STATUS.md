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

Measured at **`a4e3991`**, 2026-08-07 (sixth session).

| suite | passed | rate | note |
|---|---:|---:|---|
| `corpus_ingest` | 12,444/12,444 | 100% | |
| `baseline_resolution` | 12,444/12,444 | 100% | |
| `scanner_termination` | 12,444/12,444 | 100% | |
| `scanner_clean_files` | 5,031/5,031 | 100% | |
| `module_resolution` | 95/95 | 100% | |
| `file_loader` | 96/96 | 100% | |
| `printer_round_trip` | 11,681/11,737 | 99.52% | |
| `parser_typescript` | 5,001/5,031 | 99.40% | |
| `binder_symbols` | 8,293/8,460 | 98.03% | |
| `isolated_declarations` | 13/15 | 86.67% | |
| `dts_shape` | 618/912 | 67.76% | |
| `dts_emit` | 161/339 | 47.49% | |
| `parser_reachable_target` | 5,031/10,570 | 47.60% | |
| `dts_reachable_target` | 495/1,162 | 42.60% | |
| **`checker_types`** | **2,680/9,538** | **28.10%** | **gradient 71.91%** — the target |
| `diagnostics` | 80/5,488 | 1.46% | **structurally blocked**, see below |

### `checker_types`, the number the project is steered by

```
344,411 / 478,954 assertion lines = 71.91%
  right 344,411 | gap ~82,955 | wrong ~41,606   (41,189 at b00738d; …, +104, +81, +3, +84 by same-probe pairs through the qualified-naming build)
```

**Corrected in place: §1 carried `340,719` and the snapshot at `df13a69` read
`340,727`.** Eight lines, from commits landed after the `tsr-g30h` merge the
figure was taken at. The gap figure is `depend.rs`'s own walk at `a4e3991`, not
a subtraction.

**An open discrepancy, recorded rather than reconciled away.** `wrongdelta.rs`'s
raw dump reads **42,443 lines** at `a4e3991`, and `right + gap + wrong` sums to
469,809 against a denominator of 478,954 — 9,145 short. The three instruments do
not share a denominator (`depend` attributes gap lines it can reach; `wrongdelta`
dumps per-case). **Do not substitute 42,443 for the carried figure**: that would
be exactly the cross-instrument subtraction this section forbids two paragraphs
below. One probe reconciling the three denominators is worth a future session's
first hour.

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
| **checker** | **71.91% of lines** | the mountain; §4 and §5 |
| transformers | **not started** | |
| diagnostics | **not started, and blocked** | §1 |
| language service / LSP | **not started** | |

### Inside the checker — what has an arm

Landed across the three sessions to date, newest first:

| commit | what | net |
|---|---|---|
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
| ~~**~1,530**~~ | **LANDED at `8e28971`, +3,590 — 211% of the forecast, the session's largest build.** ~~qualified naming, design W — the written entity name. THE TOP OF THE BOARD, and it arrived by overturning a refusal rather than by finding a new row** (`checker-notes-qualname.md`). Reprint the written qualified name for a type reference whose leftmost identifier resolves as a namespace, declining when the reference site is **inside** the namespace it qualifies — a positional refusal upstream's own `needsQualification` licenses, worth 68 conversions to remove 79 wrong lines. Forecast **1,702 converts / 20 would-be-wrong / 0 at risk = 85× **, and the forecast is a **floor, not a ceiling**: 1,302 lines are excluded as unscorable rather than declining. Bar registered in §8 of that page **before any code** — `lost == 0`, `new wrong ≤ 40`, `gained ≥ 900` — with four falsifiers named. Effort 2: `resolve_entity_name` plus a written-text reprint, on a node the parser already records Residue, each with an owner: alias naming 20 (the missing `alias_symbol_for_type_node` call on the type-reference arm), enum narrowing 6, design **P** — the symbol chain on the *outer* name — 13 and now **created** by W's conversions, so §8's "W first, then re-measure P" is load-bearing rather than tidy | ~~1,702~~ landed | 2 | — | `declared.rs` |
| ~~869~~ **~78** | **call resolution — overload sets. RE-SCORED DOWN, fifth session.** `bd tsr-klm` is answered (`callgate.rs`, `checker-notes-callres.md` §13): the 9,660 was **the row, not the mechanism**. Summed from the gates overload selection actually owns — generic candidate 490, parameter 473, argument 28, ambiguous 50, arity 9, nothing-assignable 39, this/rest 5, spread 1 — its own population is **1,095 lines, ~880 net**, an **8× smaller** item, and third of the three the split found | 1,095 | 5 | 0.45 | `calls.rs`, `relater.rs` |
| ~~360~~ | **type-argument inference — FIRST SLICE LANDED, +372** (`bd tsr-g30h`). The counterfactual forecast 131 own-node conversions and it converted 372 (2.8x, cascade). **`bd tsr-g30h` stays open**: the contravariant bucket / priority lattice owns both the 3 lost lines and the largest share of the 33 new wrong, which is the argument for it being the next leg. Tuples (42), object-type members and intersections refused with numbers in §5.4 of that page. ~~old row:~~ The largest gate in the corrected split: a single candidate resolves and *inference* is what stops, **2,324 lines, want-any 28**. `inference.rs`'s own doc measures the cliff — 53% of generic calls have no type parameter written bare, so the candidate must be dug out structurally. A subsystem (`inference.go:53`: priority lattice, contravariant tracking), which is why the effort is 5 and not 3 | 2,324 | 5 | 0.75 | `inference.rs` |
| ~~340~~ | **a `Named` callee — LANDED, +1,018.** The counterfactual computed the at-risk column in the same pass as the target one, exactly as `docs/conventions.md` requires, and it earned its keep: the naive arm read **646 converts against 377 wrong**. Four measured refusals cut that to **8 new wrong** — `unique symbol` and qualified naming each cost **zero** conversions. Remainder, each refused with a returning condition: 926 generic candidates (inference), 274 class instance types, 291 `unique symbol`, 75 qualified naming, 48 disagreeing overload sets, 27 heritage. ~~old row:~~ 2,791 lines across the call and `new` halves — lib constructor *interfaces* (`DateConstructor`, `MapConstructor`) and interface-typed callees. Needs `getSignaturesOfType` → `resolveStructuredTypeMembers`. **Carries a measured wrong-manufacturing risk**: §5 of `checker-notes-callres.md` records 264 `unique symbol` lines that would print `symbol`, and half the row wants a generic instantiation this port has no members for | 2,791 | 4 | 0.50 | `signatures.rs` |
| ~~649~~ | **destructuring / binding patterns — LANDED at `4b81458`** (`tsr-o00`). Sized by the new `bindgap.rs` at 1,228 buildable of 2,632 classified (~1,111 net); converted **+1,081 = 97% of the sized net**, above the band again via downstream unblocks. The row's residue belongs to its owners: contextual pattern parameters 604 (the re-armed contextual refusal), pattern-implied tuple inference ~250+155 (`tsr-84iz`), flow-of-destructuring (`tsr-pqnh`), rest 172, computed 86, no-source 113 | — | — | — | `destructure.rs` |
| ~~374~~ | **element access remainder — decomposed to shards, none an arm.** After the fourth session's builds the row is **634** (`elemgap.rs` at `935a221`): 177 string-literal misses (want-any 27%; head cases are `noImplicitAnyStringIndexerOnObject` — option modelling — and `mappedTypeRelationships` — mapped types, unported), 118 numeric-literal misses (lib-array receivers under literal indexes), 108 enum/named indexes (enum machinery), 86 other. Each shard belongs to an unported subsystem, not to `indexed.rs`; the row stops being a board item and its shards go to their owners. | — | — | — | split complete |
| ~~230~~ | **JSX — re-scored DOWN at the fifth session's `jsxfeas.rs`** (`checker-notes-jsx.md`): of 1,199 element gap lines, **46% cannot resolve the `JSX` namespace at all** (it sits behind `declare global` augmentation, unported in the binder) and **29% resolve but print bare `Element`** — the refused qualified-naming family (2.7 wrong/right). Feasibility ~0.25; blocked on global augmentation, then namespace-qualified naming | 1,199 | 4 | 0.25 | blocked — prerequisites named |
| ~~165~~ | **template literal types / `TemplateExpression` — REFUSED with a ratio, fifth session** (`checker-notes-tmplexpr.md`). The old grounds ("the cheap leg is not separable") were re-tested on the fresh 1,890 row and are now measured: the cheap leg alone converts 446 against 590 new wrong (**0.76 per wrong**), the folder-plus-fallback design 515 against 521 (**0.99**). Both are worse than every refusal on this page. The legs **interleave** — 151 lines want `string` exactly where the folder fires | 1,036 | 3 | 0.15 | refused |
| ~~165~~ | ~~**template literal types.**~~ Refused once: the cheap leg is **not separable**, because upstream's `evaluate` is a syntactic folder consulting no types. Kept on the list because the row survived the session unchanged. | 1,237 | 3 | 0.40 | `declared.rs` |

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
| **the multi-distinct return aggregate (`tsr-4sc.9` leg)** | 81 | `retgap.rs`, fifth session: 60+18+3 gap lines across the whole corpus want a subtype-reduced union of return types. The machinery is `removeSubtypes` — refused at `tsr-eak` — for ~12–46 converted lines at the observed band |
| **`TemplateExpression`, both candidate designs** | 1,036 | fifth session, `checker-notes-tmplexpr.md`: cheap leg **0.76 gained per wrong**, folder-plus-fallback **0.99**. Supersedes the "not separable" wording with a number. Only a *complete* constant folder (arithmetic, booleans, const-enum members) changes either ratio |
| **the object-literal remainder as an item** | 2,223 quoted | fifth session, `checker-notes-objgap.md`: **77% is downstream** — every member kind has an arm and a member *value* gaps. The accessor item `bd tsr-32y` is **78 lines**, computed names 219. §4.3's "unknown proportion" warning was right and the proportion is 6% |
| **import aliases in value position** | 2,404 row / 840 measured | fifth session, `checker-notes-novaldecl.md`: the largest unopened gap root is 93% import aliases, and `resolve_alias` already refuses them **for a naming reason** — an alias prints its own name, so resolving `import * as ns` would print `typeof <stripped file path>`. `bd tsr-4jk`; same family as qualified naming |
| **the `this` half of `getApparentType`'s head** | 827 | fifth session, `checker-notes-apparent.md`: **zero** convertible. 522 find the member and gap on its own type; 305 are absent from the class's declared type entirely — filed as a separate members-table question |
| **the contravariant inference bucket** | 6 own-node / ~36 with cascade | fifth session, `checker-notes-infer2.md` §6. Recommended by me on "the same mechanism owns both the losses and the largest share of the new wrong" — and that was **a ceiling on a row, not a forecast of a mechanism**, this file's fourth rule catching its own author. Re-derived line by line: of the 120 wrong lines in the four contravariant-named cases, **4** are this mechanism; 116 belong to five other items. It also needs `strictFunctionTypes`, an option this port does not model, and guessing a sibling flag wrong once cost 1,221 lines. A tenth of the smallest thing ever refused here |
| **destructuring an array literal without the pattern's contextual type** | ~80 wrong minted | the first `tsr-o00` run answered `var [a, b] = [1, "x"]` elements as `string \| number`; upstream infers the **tuple** through `getTypeFromBindingPattern`'s implied contextual type (`checker.go:16748`). Approximating it passed the bar's ratio leg (6.0×) and was refused anyway — the construct refuses whole until `tsr-84iz` builds the mechanism |
| **`bd tsr-kmzf` — a three-valued relation, as an ITEM** | 33 / 0 | sixth session, `checker-notes-assign.md` §5–§6. Leg 1 forecast **33 against a floor of 150**; control C3 then showed **all 33 are conversions the existing BINARY relation already makes**, so the mechanism's own marginal yield is **0**. Refused as an item and **shipped anyway**, because it is what makes deleting `SELECTABLE` safe — the 52 `UNDECIDED` lines are what a naive removal decides wrongly. Do not re-open as "make the relation three-valued": that is done, and it converts nothing |
| **widening `SELECTABLE` as a flag set** | — | same measurement, and it is a *structural* refusal rather than a numeric one. Decidability is a property of the **pair**: for signature-bearing types the old relation was unsound in **both directions at once**, so no per-type flag predicate separates the trustworthy pairs from the rest. The flag set is now deleted rather than widened |
| **the `any`-parameter overload set** | 7 conversions | sixth session. `any` relates to everything both ways, so such a candidate is trivially applicable and declaration-order selection always stops on it — a **wrong rule, not a bad trade**. Refusing it positionally costs 7 conversions and removes **24** would-be-wrong lines: 40/26 → **33/2** |
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
