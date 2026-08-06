# Names that do not resolve, and symbols with no value declaration

> **CORRECTED 2026-08-06 (cycle 13). This page's probe had the gap/right
> ordering defect, and §2's headline moved.**
>
> `examples/nameres.rs` inherited `receiver_gap.rs`'s classification order — it
> tested `type_string == "error"` **before** testing the baseline, so a line
> where this port answers `error` **and upstream's baseline also says `error`**
> was filed as a gap. It is a right answer. Found by the lead with
> `examples/reconcile.rs` (`9b10272`, `bd tsr-zlo`); `callres.rs` had the same
> defect independently.
>
> Fixed here by testing the baseline first. What moved, at `26efa2a`:
>
> | | as published | corrected |
> |---|---:|---:|
> | this probe's `right` | 292,217 | **294,871** |
> | DIRECT row | 4,673 | **4,569** |
> | "upstream also errors" | 4,467 (**53.9%**) | **4,360 (53.3%)** |
> | "upstream has a REAL type" | 2,904 | **2,904 — unchanged** |
>
> **The finding survives and the available-work figure does not move at all.**
> The 104 lines that left were `error`-on-both-sides, so they were never
> available work either; they were miscounted as gap rather than as right. Every
> §3 and §4 figure is over the "real type" sub-population and is unaffected.
>
> The 294,871 also reconciles exactly with the merged corpus run at `26efa2a`.
> §0's claim that this probe's agreement with `receiver_gap` was evidence
> *neither* was at fault **was wrong, and backwards**: two probes agreeing
> because one copied the other's classifier is not corroboration, it is a shared
> defect. `docs/conventions.md`'s "two independent instruments" rule requires the
> instruments to be independent, and these two were not.

Measured 2026-08-06 at `058b4a9` by
`crates/tsr-conformance/examples/nameres.rs`, which routes through
`types_producer::assertions_for_case_with_ids` and so measures the same
lib-loaded compiler the `checker_types` gradient does. Upstream anchors are at
the pinned submodule `5b1047d10`, each taken from `grep -n` on the declaration.

Reproduce:

```
cargo run --release -p tsr-conformance --example nameres
```

This probe re-derives the gradient as **292,217 right / 143,509 gap / 43,334
wrong**, identical to `receiver_gap.rs`'s figures in
[`checker-notes-recvgap.md`](checker-notes-recvgap.md) §8. The 0.09pp
disagreement with the committed `checker_types` snapshot recorded there is
therefore *not* a property of either probe; it is still unexplained and is still
not quoted here.

---

## 0. The findings, in the order they change a decision

1. **53.9% of both name-resolution rows is not a defect.** Over the 8,292 lines
   in the two rows, **4,467 (53.9%)** have `any` or `error` on the baseline's own
   right-hand side — upstream does not resolve the name either. An unresolved
   name is `errorType` upstream and upstream's node builder prints `errorType` as
   `any`. This port already computes the same answer; only the *renderer*
   differs, deliberately, so that a gap stays separable from a wrong. **No amount
   of resolution work moves these lines**, and every previous quotation of this
   row's size has included them. §2.
2. **The largest single named blocker is a harness limitation, not a compiler
   one.** `compiler/temporal` is 1,390 of the 3,619 cascade lines and 351 of the
   direct row. `Temporal` is declared only in
   `vendor/typescript-go/internal/bundled/libs/lib.esnext.temporal.d.ts`, the
   case's source says `// @lib: es6,…,esnext.temporal,dom`, and
   `trace_case::apply_test_directives` does not read `lib`. **The lib file is not
   in the program.** 1,860 lines corpus-wide are in this bucket, 1,784 of them
   with a real upstream type and 1,725 printing `typeof Temporal`, which is
   spellable. This is real, available work — in
   `crates/tsr-conformance/src/trace_case.rs`, which this slice does not own. §3.
3. **The pre-registered rule for the name-resolution item fired REFUSE**, and it
   was not close: the largest mechanism this port could actually fix is **434
   lines** against a threshold of 800. §4.
4. **The `no value declaration` half contains a located, in-scope defect.**
   `Checker::export_symbol_of` walks an export marker to its **source file** and
   reads that file's module symbol's `exports`. For `namespace N { export enum E }`
   the export symbol is on `N`; for a **script** file there is no module symbol at
   all. **2,175 assertion lines**, of which 988 fail at the missing-file-symbol
   step. Upstream does not reconstruct this — it stores `symbol.ExportSymbol`
   (`internal/ast/symbol.go:20`). §5.
5. **`SymbolFlags(ALIAS) / no value declaration` is now 3,455 lines over 914
   cases**, the largest member of the `no value declaration` family and larger
   than the 2,535 + 1,621 = 4,156 recorded in
   [`checker-notes-symbols.md`](checker-notes-symbols.md) §9 would suggest after
   ADR-0041 landed module resolution. Not this slice's; §6 hands it over with
   numbers.

---

## 1. What the two rows are

`types_producer::gap_reason` emits `the name does not resolve` from exactly one
arm — the `Expression::Identifier` arm — so the phrase has one meaning. Two
different populations carry it:

| row | what the line is | lines | cases | top-1 | top-10 |
|---|---|---:|---:|---:|---:|
| **DIRECT** | the assertion line *is* the unresolved name (board row 7) | **4,673** | 1,048 | 7.5% | 37.6% |
| **CASCADE** | the line is a member access whose receiver chain roots at one | **3,619** | 110 | 38.0% | 87.9% |

The cascade figure reproduces `checker-notes-recvgap.md` §4's **3,619** exactly,
through code that shares only the descent loop; the direct figure is **4,673**
against `checker-notes-rank.md`'s **3,621** for board row 7, and the difference
is that the board's `row_key` splits by the `what` prefix while this probe
matches the phrase under any prefix (`the name of a JsxNamespacedName, …`).
**Quote 3,621 for the board row and 4,673 for the phrase; they are not the same
number.**

The cascade row's shape is the warning `docs/conventions.md` keeps needing: 110
cases, top-10 87.9%. It is a handful of files.

---

## 2. 53.9% of it is not a defect

| | lines | share |
|---|---:|---:|
| upstream also errors (`any` on the baseline) | **4,360** | **53.3%** |
| upstream has a real type | 2,904 | 35.5% |
| the root's own baseline line could not be located | 921 | 11.3% |

*Corrected 2026-08-06; see the header. The `error`-on-both-sides lines that used
to sit in the first row are `right` and have left the table entirely, which is
why the first row shrank and the second did not move.*

An unresolved name is `errorType` in upstream, and `errorType` is printed as
`any` — which is why the `.types` baseline for `conformance/parserRealSource10`
says `any` for `NodeType` (the file's `///<reference path='typescript.ts' />`
names a file that does not exist in the case) and why `compiler/parsingDeep…`
says `any` for `a`.

**This port already reaches upstream's answer for those 4,467 lines.** What
differs is that this port renders `errorType` as `error` rather than `any`, on
purpose: the gap/wrong split is the instrument every ranking in `docs/` leans on,
and it exists because one `errorType` here means both *"upstream errors too"* and
*"not built yet"*. Printing `any` would score +4,467 right lines and turn the
other ~139,000 gap lines into `wrong`. That is a scoring change, not a porting
one, and it is not proposed.

**Consequence for ranking:** the honest size of "names this port fails to
resolve and upstream does not" is **2,904 lines**, not 8,292. Anyone quoting
3,619 or 3,621 as available work is over by ~2.9×.

The 921 unlocated roots are a real ceiling on this page and not an error bar
around it: they are cascade roots whose own assertion line the baseline does not
render, so their upstream answer is unknown. Every share above is taken over all
8,292, so each is a *lower* bound on its true share of the located population.

---

## 3. Where the remaining 2,904 lines are, by mechanism

Both rows, `upstream has a real type` sub-population only, since that is the
only part a resolution fix could convert:

| mechanism | direct | cascade | total | cases (direct) | top-1 |
|---|---:|---:|---:|---:|---:|
| **`@lib` directive dropped by the harness** | 404 | 1,380 | **1,784** | 20 | 86.9% |
| declared inside a namespace body (no exports arm in `resolve_name`) | 376 | 58 | **434** | 138 | 17.0% |
| declared elsewhere in this program | 238 | 59 | **297** | 97 | 16.8% |
| `arguments` (unsynthesised global) | 92 | 108 | 200 | 30 | 21.7% |
| `globalThis` (unported) | 96 | 44 | 140 | 14 | 63.5% |
| not declared anywhere | 31 | 6 | 37 | 5 | 74.2% |
| CommonJS ambient (`require`/`module`/…) | 4 | 8 | 12 | 1 | 100.0% |
| empty identifier text (parser recovery) | 0 | 0 | 0 | — | — |

### The `@lib` bucket, named

`compiler/temporal` is the top case in both rows. Its source
(`vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler/temporal.ts`,
lines 1–2) reads:

```
// @target: es2020
// @lib: es6,es2021.intl,esnext.date,esnext.intl,esnext.temporal,dom
```

`trace_case::apply_test_directives` (`crates/tsr-conformance/src/trace_case.rs:327`)
reads `target`, `module`, `moduleresolution`, `rootDirs`, `typeRoots` and more.
It does **not** read `lib`. Verified directly: the program built for
`compiler/temporal` holds 52 files, none of them `lib.esnext.temporal.d.ts`, and
`Temporal` is absent from `binder.globals()` while `Array` and `Promise` are
present.

So this is not a binder gap and not a checker gap. It is one field on
`CompilerOptions` plumbed into the lib selection the loader already performs.
**1,784 lines with a real upstream type, 1,725 of them printing `typeof Temporal`
— which is spellable, because a `declare namespace` has a name and not a file
path.** The spelling hazard `bd tsr-4jk` found for module objects does not apply.

Filed as `bd tsr-cug`. Not built here: `crates/tsr-conformance/src/**` belongs to
another owner this cycle.

### The `parserRealSource*` family, named

`conformance/parserRealSource10`, `11`, `7` and `12` contribute 344 + 336 + 209 +
108 = **997** direct-row lines, almost all of them in the
`not declared anywhere` bucket: each file opens with
`///<reference path='typescript.ts' />`, that file is not part of the case, and
so `NodeType`, `ErrorRecoverySet`, `TokenID`, `StringHashTable` and friends
resolve to nothing **in upstream too**. The baseline says `any`. These lines are
in finding 1, not in the available work.

---

## 4. The rule, and that it fired REFUSE

Registered in the probe's module docs before the split was run:

> **BUILD if the largest single mechanism in the
> `RESOLVABLE-AND-UPSTREAM-HAS-A-REAL-TYPE` bucket exceeds 800 lines (direct +
> cascade) over more than 40 cases with a top-1 case share below 40%. Otherwise
> REFUSE.**

800 is ~2.5× the largest arm landed in this area (`export { q }`, 198 lines
same-file, +94 measured). The rule is registered on the bucket the probe prints,
not on a quantity computed from it — `docs/conventions.md`, *"pre-register on the
most direct bucket your instrument produces, not on a proxy"*.

Measured, the largest mechanism this port owns is
**`declared inside a namespace body` at 434 lines** (138 cases, top-1 17.0%,
top-10 49.8%). The case and concentration legs pass comfortably; **the size leg
fails, 434 against 800.** The runner-up, `declared elsewhere in this program`,
is 297 and also fails. Their sum is 731 and still fails.

**REFUSED: no namespace-exports arm was added to
`BindResult::resolve_name`.** The arm is upstream's — `r.lookup(GetExports(…))`
in `nameresolver.go` — and it is genuinely missing here (the function's own doc
comment lists it under *"what is not ported"*). It is simply not worth 434 lines
this cycle when a 2,175-line defect in the same assignment is one field away.
`bd tsr-sgd` carries it with these numbers.

Two things about that bucket that a later agent should not have to re-measure:

- Its top case, `compiler/privacyLocalInternalReferenceImportWithExport`, is 96
  of the 434 (22.1%), so its distributed size is nearer **340**.
- `ElsewhereInProgram` is an **upper bound**, not a measurement: its test is
  "some symbol of this name exists anywhere in `SymbolStore`", which includes
  members and unrelated scopes. Only 238 of its 1,086 direct lines have a real
  upstream type, and that ratio is the tell.

### A control that was worth printing

**C5 — lines that resolve under `SymbolFlags::all()` where the `VALUE` lookup
failed — reads 0 over all 8,292.** So none of this row is a *meaning* bug. Every
one of these names is missing from every table `resolve_name` consults, at every
meaning. That was not obvious in advance, and it rules out the cheapest possible
fix outright.

---

## 5. What was built: the export-marker link

### The forcing constraint

`gap_reason`'s `no value declaration` family, keyed by the symbol's flags:

| flags | lines | cases | top-1 | top-10 |
|---|---:|---:|---:|---:|
| `SymbolFlags(ALIAS)` | 3,455 | 914 | 3.8% | 18.4% |
| **`SymbolFlags(EXPORT_VALUE)`** | **1,346** | 258 | 36.7% | 57.4% |
| `SymbolFlags(FUNCTION)` | 133 | 27 | 50.4% | 86.5% |
| `SymbolFlags(INTERFACE)` | 57 | 47 | 7.0% | 35.1% |
| everything else | 62 | — | — | — |
| **total** | **5,053** | | | |

An **export marker** is the local the binder leaves behind for `export var x`:
the real symbol goes into the container's `exports`, the marker into its
`locals` carrying `SymbolFlags::EXPORT_VALUE` and nothing else. An unqualified
reference resolves to the marker, so `get_type_of_symbol` has to get from one to
the other. `Checker::export_symbol_of` did it like this:

```rust
let file = self.source_file_of(*entry.declarations.first()?)?;
let module = self.binder.symbol_of(file)?;
self.binder.symbols().get(module).exports.get(name).copied()
```

**That is right only when the container is the file.** Split by where the
marker's declaration actually sits:

| row | container | lines | cases | top-1 | top-3 cases |
|---|---|---:|---:|---:|---|
| direct | **inside a namespace** | **1,111** | 185 | 44.5% | `parserRealSource10` 494, `typeArgumentsWithStringLiteralTypes01` 90, `parserRealSource11` 60 |
| direct | at file top level | 235 | 74 | 7.7% | — |
| cascade | **inside a namespace** | **1,064** | 31 | 87.0% | `parserRealSource10` 926, `enumMerging` 32, `statics` 22 |
| cascade | at file top level | 56 | 13 | 42.9% | — |

Two preconditions of the old lookup, counted per line:

```
source file has NO module symbol  = 988 lines   (a script file; the lookup cannot start)
file exports DO hold the name     = 225 lines   (the lookup succeeded; the gap is downstream)
```

**988 of these lines never got past the first step.** `parserRealSource10` is
`namespace TypeScript { export enum TokenID … }` in a file with no top-level
import or export — a *script*, which has no module symbol at all.

### The decision, and the rule it was judged against

Registered before the split above was run:

> **BUILD the container fix if the `EXPORT_VALUE / no value declaration` lines
> whose marker declaration sits inside a `ModuleDeclaration` exceed 500 lines
> (direct + cascade) over more than 40 cases with a top-1 case share below 50%,
> and no top-10 baseline right-hand side for them names a file path. Otherwise
> REFUSE.**

Measured: **2,175 lines** (1,111 + 1,064), **185 cases** on the direct row, and
the spelling check is clean — the top right-hand sides are
`typeof OperatorPrecedence` 218, `typeof TokenID` 128, `typeof Reservation` 109,
`typeof c1` 38, `typeof C1_public` 36, `"Hello"` 35, `typeof publicClass` 20.
Every one is a declared name; **no file path appears**, so the hazard that killed
the module-object item (`docs/conventions.md`, *"can this port spell the
answer?"*) is absent here.

The **top-1 leg is where the rule is ambiguous, and the ambiguity is my error.**
Read over the combined direct+cascade population, `conformance/parserRealSource10`
holds 494 + 926 = 1,420 of 2,175, **65.3%** — above the 50% bar. Read over the
direct row alone it is 44.5%, below it.

Resolved by following what `docs/` already does with a concentrated row rather
than by picking whichever reading agreed: `checker-notes-rank.md` §3 responds to
86.6% concentration by **re-sizing** (*"rank #1 as ~1,553, not 13,143 and not
11,553"*), not by vetoing. Re-sized the same way — excluding `parserRealSource10`
and `parserRealSource11` entirely — the row is **≈695 lines over ~183 cases with
a top-1 share near 13%**, which clears every leg of the rule including the size
one. **BUILT.**

Stated plainly so the next reader can disagree with it: had the rule said
"top-1 over the combined population", it would have refused, and the correct
response would have been to refuse. The rule should have named its population.

### What was built, in two commits

**The declaration, alone:** `tsr_binder::Symbol::export_symbol`, the port of
`ast.Symbol.ExportSymbol` (`internal/ast/symbol.go:20`), set in the binder's
export-marker branch exactly where upstream sets it —
`local.ExportSymbol = b.declareSymbol(ast.GetExports(container.Symbol()), …)`
(`internal/binder/binder.go:407`, inside `declareModuleMember`, `:373`).

**The consumer:** `Checker::export_symbol_of` reads the field instead of
reconstructing it. `getExportSymbolOfValueSymbolIfExported`
(`internal/checker/checker.go:14383`) is the same two lines upstream.

### The alternative, taken seriously

**Walk the marker's declaration to its nearest exporting container** — the
enclosing `ModuleDeclaration`, else the `SourceFile` — and read *that* symbol's
exports. It needs no change to `tsr-binder` and would convert the same lines.

Rejected, for a reason that is not "upstream does it the other way". The walk has
to decide *which* ancestor owns the exports table, and that decision is exactly
what `declare_module_member` already made when it chose `container`. A second
copy of it is a second thing to keep in step, and the first copy has already been
wrong once — this page exists because of it. `Symbol::export_symbol` costs one
`Option<SymbolId>` per symbol and has no way to disagree with the binder, because
the binder is what writes it.

**What would make the walk win:** if `Symbol` were shared across programs, so a
per-program link could not live on it. ADR-0034 established the opposite (a
`BindResult` *is* one program), which is the same premise that lets
`Binder::merge_symbol` mutate in place.

### Consequences accepted

- One `Option<SymbolId>` on every symbol. `SymbolStore` holds 41,529 symbols on
  the largest benchmark fixture, so this is ~166 KB there — and `Symbol` already
  carries two `FxHashMap`s, so it is not the field that decides the layout.
- The link is set only in the two-symbol branch of `declare_module_member`. An
  **unnamed default** (`export default class {}`) creates no local, so there is
  no marker and no link; that branch returns the export directly and is
  unaffected. An **alias** (`export { x }`) gets one symbol, not two, and is also
  unaffected — its arm returns before this point.
- Nothing merges `export_symbol` across declarations. Two declarations of the
  same exported name merge into one marker and one export symbol, so the link is
  written twice with the same value; a marker whose declarations disagree about
  their container would silently take the last. No such shape is known and none
  is tested for.

### The tests, and the mutations that make each red

Four tests over two files, each proven red under a **named** mutation before the
change landed, with `grep -c` = 1 on every string keyed on.

| mutation | reddens |
|---|---|
| **M1** — drop the `export_symbol` assignment in `declare_module_member` | `nameres_export_symbol::a_namespace_member_marker_links_…` only; the mirror test stays green |
| **M2** — set `export_symbol` on every local rather than only on markers | both binder tests |
| **M3** — restore the file-level reconstruction verbatim | `a_reference_inside_a_namespace_…` and `exporting_from_a_namespace_…`; the **gap** test stays green, because a tuple annotation is unported either way and that is exactly what that test is for |
| **M4** — answer `anyType` for a marker with a link, instead of re-entering `get_type_of_symbol` | all three checker tests, the gap test included |

M3 is the one worth reading: it is the exact code this commit replaces, so its
red is the measurement's unit-level counterpart. M4 exists because the arm's
whole risk is manufacturing an answer rather than finding one, and
`docs/conventions.md` requires the loaded label to earn a positive test.

Note the fixtures are all **script** files — no top-level `import` or `export` —
which is deliberate: 988 of the 2,175 corpus lines are in script files, where the
old lookup failed at `binder.symbol_of(file)` before reaching any table. A
fixture that was a module would have exercised only half the defect.

### How this would be shown wrong

- **A marker whose `export_symbol` is not the symbol a qualified reference
  reaches.** `nameres_export_symbol.rs` pins the namespace case from the
  binder's side, comparing against `N.exports["E"]` by identity.
- **The gradient's `wrong` column rising in the affected cases.** The arm can
  only turn `errorType` into whatever `get_type_of_symbol` says about the export
  symbol, so a new wrong line means the export symbol this port reaches is not
  the one upstream reaches.
- **`binder_symbols` falling.** The binder change adds a field and writes it; it
  does not move a symbol between tables, so the symbol baseline must not move at
  all. That is the rail to watch on the corpus run.

### The result, measured, and it is not clean

Re-run of the same probe at `c23b4cc` against `058b4a9`, same binary path, same
corpus pin. **This is `examples/nameres.rs`'s own gradient, not the
`checker_types` suite's** — the two agree on the base (§0) but only the suite's
number is the gradient.

```
                 058b4a9    c23b4cc    delta
  right          292,217    294,484   +2,267
  gap            143,509    140,933   -2,576
  wrong           43,334     43,643     +309
```

And on the rows this targeted:

| row | before | after |
|---|---:|---:|
| `SymbolFlags(EXPORT_VALUE) / no value declaration` | 1,346 | **371** |
| markers inside a namespace (direct + cascade) | 2,175 | **160** |
| markers at file top level (direct + cascade) | 291 | 283 |
| precondition: source file has no module symbol | 988 | 131 |

**2,015 lines left the targeted rows; 2,576 gap lines converted.** The cascade
multiplier against the predicted rows is **1.28×** — smaller than the 1.66×
`docs/conventions.md` measured for cross-file aliases, and for the reason that
page gives: the multiplier is a property of the form, and an exported enum is
referenced through `E.A` far less often than an imported name is called.

**The +309 wrong is the part to argue with.** It is 12% of the conversion, or
0.14 wrong per right — against the module-object item's 2.5, which is why that
one was refused and this one was not. But it is not the **zero** the arm's shape
suggested: `get_type_of_export_value` can only replace `errorType` with whatever
`get_type_of_symbol` says about the export symbol, so every one of those 309 is
either (a) an export symbol whose own type this port computes incorrectly —
which was already wrong and merely invisible behind a gap — or (b) the export
symbol this port reaches is not the one upstream reaches, which is §5's
falsifier firing. **Those two are not separated here, and separating them is the
first thing to do before this arm is quoted as settled** (`bd tsr-d7c`).

The honest prediction to carry forward, with the error bar the measurement
gives: **+2,267 right, +309 wrong on `checker_types`**, ±10% on the right column
for the 0.09pp instrument disagreement recorded above, and no bar at all on the
wrong column because its composition is unknown.

Gradient pair to measure: `033277f^..c23b4cc` — two commits, the declaration and
its consumer.

---

## 6. What was deliberately not built, with numbers for whoever takes it

| item | lines | why not here | `bd` |
|---|---:|---|---|
| `@lib` in `apply_test_directives` | **1,784** (1,725 spellable as `typeof Temporal`) | `crates/tsr-conformance/src/**` is another owner's this cycle | `tsr-cug` |
| namespace-exports arm in `resolve_name` | 434 (≈340 distributed) | the registered rule refused it at 800 | `tsr-sgd` |
| `SymbolFlags(ALIAS) / no value declaration` | **3,455** over 914 cases, top-1 3.8% | the cross-file alias workstream's, and ADR-0041 has just changed its premise | `tsr-mmd` (existing) |
| `globalThis` | 140 | one global, 63.5% in one case | `tsr-e63` |
| `arguments` → `IArguments` | 200 | needs the lib item `bd tsr-9or.1`, already owned | — |

The `ALIAS` row deserves one sentence rather than a shrug: at **3,455 lines over
914 cases with top-1 3.8% and top-10 18.4%** it is the least concentrated large
row anywhere in `docs/architecture/`, and `checker-notes-symbols.md` §4 measured
77% of its predecessor as blocked on module resolution — which ADR-0041 landed.
Whoever owns it should re-measure before reading anything on this page or that
one as current.

---

## 7. How you would know this page is wrong

- **The two rows are not disjoint.** Control C3 counts lines carrying both
  phrases; `gap_reason` returns from one arm, so it is 0 by construction, and it
  reads 0.
- **The direct row is not what its name says.** Control C1 counts direct-row
  lines whose node is not an `Identifier`. `gap_reason` emits the phrase only
  from the `Expression::Identifier` arm, so it is 0 by construction; it reads 0
  against a mirror of 4,673.
- **The descent stops early**, attributing a cascade line to the wrong root.
  Control C2 counts terminal reasons still saying `the receiver is a gap`; 0.
- **The `@lib` bucket is a label, not a mechanism.** Control C4 counts lines in
  it whose name *is* in the loaded program's globals — 0, against a mirror of
  1,860. If the harness ever reads `@lib`, C4 stays 0 and the bucket empties,
  which is the intended way for this finding to expire.
- **§2's claim that upstream prints `errorType` as `any`.** The falsifier is a
  baseline line under the `not declared anywhere` bucket whose right-hand side is
  a real type. There are 31, 74.2% of them in one case; they are the residue that
  bounds the claim rather than refuting it, and they are unexamined (`open`).
- **§5's spelling check.** The falsifier is a right-hand side under the
  `InNamespace` markers that names a file path. The probe prints the top 12 and
  none does.

## 8. Open

- `open` — the 921 cascade roots whose own baseline line could not be located.
  11.1% of both rows, upstream answer unknown, so every share in §2 and §3 is a
  lower bound.
- `open` — the 31 `not declared anywhere` lines with a real upstream type. Either
  the name-existence test is too narrow or upstream resolves something this probe
  cannot see.
- `open` — the 235 direct + 56 cascade `EXPORT_VALUE` markers at file top level
  that the *old* lookup was addressed at correctly and still missed. 225 of them
  had the name in the file's exports, so `export_symbol_of` returned a symbol and
  the gap is downstream of it; the remainder is unexplained.

---

# Cycle 13 — the `SymbolFlags(ALIAS) / no value declaration` row

Measured 2026-08-06 at `26efa2a` by the same probe, extended. §6 handed this row
over as *"the least concentrated large row anywhere in `docs/architecture/`"* and
noted that ADR-0041 had just changed its premise. Both were right. It is also
**refused**, on a leg that reads 0.0% against a required 80%.

## 9. The row, with the seam live

`checker-notes-symbols.md` §4 split this row's 2,430-line predecessor by form and
found 77% blocked on module resolution. Module resolution landed
(ADR-0041, `fa29e66`). This is the first measurement with it live.

**5,207 lines — 3,437 declaration-name (direct) + 1,770 behind a receiver gap
(cascade) — over 912 cases, top-1 5.3%, top-10 24.8%.** Larger than the 3,455 I
handed over, because that figure was the direct row only. Top cases:
`privacyImportParseErrors` 276, `privacyFunctionCannotNameParameterTypeDeclFile`
187, `ramdaToolsNoInfinite2` 131. It survives the concentration check outright —
no other large row in `docs/architecture/` does.

### By form, and whether the target is reachable *and* typed

The right column replays the target lookup through the same seam
`Checker::resolve_alias` uses — `ModuleHost::resolved_module`, then the module
symbol, then `export =`, then the exports table — and asks
`get_type_of_symbol` whether the result has a type. Deliberately **not** a call
into `resolve_alias`: that function declines four forms on purpose, so asking it
would answer "no" for exactly the population under measurement.

| form | direct | cascade | total | convertible |
|---|---:|---:|---:|---:|
| `import a = require("m")` | 1,026 | 988 | **2,014** | **962** |
| `import { x } from "m"` *(handled)* | 820 | 62 | 882 | 0 |
| `import * as ns from "m"` | 475 | 341 | 816 | 512 |
| `import d from "m"` | 377 | 95 | 472 | 41 |
| `import a = b.c` | 251 | 80 | 331 | 1 |
| `import a = b` *(handled)* | 153 | 166 | 319 | 289 |
| `export { q }` *(handled)* | 180 | 0 | 180 | 27 |
| `export as namespace N` | 72 | 38 | 110 | 0 |
| `export { q } from "m"` *(handled)* | 75 | 0 | 75 | 0 |
| `export * as ns from "m"` | 8 | 0 | 8 | 8 |

| verdict | lines | share | cases | top-1 |
|---|---:|---:|---:|---:|
| **reached AND typed — convertible** | **1,840** | 35.3% | 215 | 9.7% |
| reached, its own type gaps (kind 2) | 1,275 | 24.5% | 354 | 2.9% |
| module not resolved | 1,620 | 31.1% | 348 | 10.2% |
| module resolved, name not exported | 362 | 7.0% | 129 | 8.0% |
| no replay for this form | 110 | 2.1% | 39 | 11.8% |

## 10. The rule, its population named this time, and the leg that refused it

Registered before the two spellability legs were run. §5 records that Rule 2's
top-1 leg did not say which population it was over; this one does, in the
sentence itself.

> **Population: the ALIAS row's direct *and* cascade lines together, under a
> form `Checker::resolve_alias` does not handle, whose target this probe reaches
> and types — excluding the forms `bd tsr-4jk` already measured unspellable
> (`import * as ns`, `export * as ns`).**
>
> BUILD if that population exceeds **800 lines** over more than **40 cases**
> with a **top-1 case share below 40% taken over that same population**, AND
> **≥80% of those lines reach their target through an `export =`** — so the
> printed name is the target's own rather than the module's file path — AND
> **fewer than 25% of the lines the form unblocks** have a baseline right-hand
> side containing `import(`, which is upstream's syntax for a symbol with no
> accessible name and which this port cannot produce.
>
> Otherwise REFUSE.

Measured:

| leg | required | measured | |
|---|---|---:|---|
| size | > 800 | **1,004** (962 + 41 + 1) | pass |
| cases | > 40 | 75 on the largest form alone | pass |
| **spellable row: `export =` share** | **≥ 80%** | **0.0%** | **FAIL** |
| collateral: `import(` share of what it unblocks | < 25% | 24.9% | pass, barely |

**REFUSED**, on the third leg, by the largest margin available.

### What 0.0% means, and the control that proves it is not a probe defect

`resolve_external_module_symbol` (`checker.go:15556`) hands back the `export =`
target when there is one and the **module symbol** otherwise. A module symbol's
name in this port is the stripped file path. So a bare-module target prints
`typeof /privacyCannotNameVarTypeDeclFile_exporter` where upstream prints
`typeof exporter`.

**Not one of the 962 convertible `import a = require` lines reaches its target
through an `export =`.** Nor do the 512 `import * as ns` lines, nor the 8
`export * as ns`. Read the largest case rather than the number
(`vendor/typescript-go/_submodules/TypeScript/tests/cases/compiler/privacyCannotNameVarTypeDeclFile.ts:56`):

```ts
import exporter = require("./privacyCannotNameVarTypeDeclFile_exporter");
```

and the target file exports functions — it writes no `export =`. The baseline
says `>exporter : typeof exporter`: **the local alias name**, which is
`tsr-4jk`'s finding exactly, now confirmed for `import a = require` as well as
for `import * as ns`.

**Control C9, and it is the reason this is a finding rather than a fourth probe
defect.** Three earlier readings on this page turned out to be defects in this
probe, not facts about the port — control C8 fired at 2 when it must read 0
(a specifier that was *not a string literal* was being reported as *absent*),
and `import a = require` first read **0 convertible of 2,014** because that
form's specifier is the argument of its own `require(...)` rather than a
`module_specifier` on any ancestor, so a parent walk found nothing. Both were
fixed before anything here was quoted.

So `export =` at 0.0% needed a control that could distinguish "the mechanism
never fires" from "the mechanism is not wired". C9 counts `export =` over **all**
alias lines rather than the convertible ones, and it is pinned by construction:
`compiler/es6ExportEqualsInterop.ts` writes `export = Foo` five times, so a
corpus-wide zero would be provably wrong.

```
C9  import a = require("m")   218 export=  /  1,796 bare   (10.8%)
C9  import * as ns from "m"    72 export=  /    744 bare   ( 8.8%)
C9  import d from "m"          66 export=  /    406 bare   (14.0%)
C8  NamespaceImport with no module specifier = 0 (mirror 475)
```

The mechanism fires. It fires on 218 `import a = require` lines — **and every one
of those 218 is in a non-convertible bucket**, because the `export =` target's
own type gaps or its module is not in the program. The convertible set and the
spellable set are disjoint here, and that is the whole result.

### Consequences, stated as a number

Building the largest unhandled form would convert **962 lines** and print a file
path on **962 of them**. That is not 0.14 wrong per right, as the export-marker
arm measured; it is **1.0 wrong per right on the row itself**, before any
collateral. Add the collateral — 24.9% of what `import a = require` unblocks has
`import(` in its baseline, which this port cannot emit — and the item is
**negative**, in the same way and for the same reason as `bd tsr-6ph` (2.1) and
its lookup-only variant (2.5).

**What would make this build win:** a printer that emits the *alias's own name*
for a module-symbol-typed reference. That is one mechanism, it is the same one
`tsr-4jk` and `tsr-6ph` and `tsr-6j2` are all blocked on, and it would unlock
962 + 512 + 8 = **1,482 lines of this row alone**, on top of what those issues
already count. It is now the single highest-value unbuilt thing this workstream
has measured, and `bd tsr-6j2` is where it lives (`bd tsr-e2u` carries this measurement).

## 11. Two things the split found that are not this row

### `import a = b` is the `tsr-sgd` item wearing a different hat

The `import a = b` form shows **289 of 319 lines convertible under my replay
while the checker answers `errorType`**. That discrepancy is the finding, not the
289: my replay looks the name up with `SymbolFlags::all()`, and
`resolve_alias`'s Identifier arm re-checks `SymbolFlags::NAMESPACE`
(`symbols.rs`, and upstream's `resolveEntityName` passes the same meaning).

Reproduced at unit level, and the pair is the whole diagnosis:

```ts
namespace m1 { export namespace P { export var v: string; } import im = P; export var q = im; }
//                                                                              -> error
namespace m1 {        namespace P { export var v: string; } import im = P; export var q = im; }
//                                                                              -> typeof P
```

**Exporting the namespace changes the answer**, which cannot be right. From
inside `m1`, `P` resolves to the **export marker** in `m1`'s locals, whose flags
are `EXPORT_VALUE` and nothing else, so the `NAMESPACE` test fails.

My first diagnosis was that this is a one-line meaning fix in `resolve_alias`,
using the `Symbol::export_symbol` link added in `033277f`. **That is wrong, and
reading upstream says so.** Upstream's `lookup` (`internal/binder/nameresolver.go`)
filters the `locals` hit by meaning, so the marker fails there *too* — and
upstream then falls through to the enclosing `ModuleDeclaration`'s **exports**
table and finds the real `P` with full flags. The divergence is not the meaning
test; it is the missing exports arm.

So **this is `bd tsr-sgd` — the namespace-exports arm in
`BindResult::resolve_name` — and not a separate item.** I refused that at 434
lines from the name-resolution row; it is worth more than that, because this
row contributes to it as well. The two populations were measured by different
routes and must not simply be added (`docs/conventions.md`: *"sharing a
downstream function is not the same as being blocked by it"*), so the honest
statement is **434 confirmed plus an unmeasured share of these 319**, and sizing
it properly is one probe pass. `tsr-sgd` now carries this.

Refused again this cycle on the standing 800-line threshold this workstream has
applied three times. Fixing it with a marker-following hack in `resolve_alias`
was specifically declined: it would reach the right symbol for this shape while
being a second copy of a decision the binder already makes, which is the exact
argument §5 used to reject the walking reconstruction — and that argument does
not stop applying because the second copy would be mine.

### The `handled` column is a measurement of my replay, not of available work

316 lines are "convertible" under forms `resolve_alias` already handles. Every
one is a place where my replay is **wider** than the checker — `all()` rather
than the meaning upstream passes, and no `export =` guard on the named-import
path. **Do not read that column as work.** It is reported because a reader would
otherwise compute 1,840 − 1,524 and think it was something.

## 12. How you would know this section is wrong

- **C8** — a `NamespaceImport` with no module specifier — is 0 by the grammar,
  against a mirror of 475. It read **2** on the first run and that was a probe
  defect, which is the strongest thing that can be said for a control.
- **C9** — `export =` over all alias lines — is non-zero (218 / 72 / 66) against
  a construction anchor (`es6ExportEqualsInterop.ts` writes it five times). A
  zero here would mean §10's refusal rests on a mechanism that never fires.
- **The 0.0% leg.** The falsifier is a convertible `import a = require` line
  whose target comes through an `export =`. There are none; the probe prints the
  split per form and would show it.
- **The `import a = b` diagnosis.** The falsifier is upstream resolving the
  export marker rather than the namespace's export. The two-line fixture above
  is the test to write when `tsr-sgd` is built.

## 13. Open, cycle 13

- `open` — the 1,620 `module not resolved` lines. Some are bare specifiers no
  case supplies (upstream fails too); the split is unmeasured.
- `open` — the 41 convertible `import d from "m"` lines. Their target is
  `exports["default"]`, a declared symbol, so leg 1's `false` is a false alarm
  for that form; whether this port prints `typeof A` or `typeof default` for it
  is **unverified** and 41 lines did not justify checking.
- `open` — the 218 `export =` lines that are not convertible. If their targets'
  types become computable, they are the spellable half of this row, and the
  refusal in §10 would have to be re-taken against them rather than against the
  962.

---

# Cycle 14 — naming a module object at the reference site

Measured 2026-08-06 at `c99006d` by the same probe, extended, and scored with
`examples/casedelta.rs`. This is `bd tsr-6j2`, and it is the constraint every
refusal in this workstream bottomed out on.

## 14. The mechanism, anchored

Upstream does **not** read the name off the declaration. `symbolToTypeNode`
reaches `NodeBuilderImpl.lookupSymbolChain`
(`internal/checker/nodebuilderimpl.go:1061`), which calls `getSymbolChain`, which
calls `Checker.getAccessibleSymbolChain`
(`internal/checker/symbolaccessibility.go:373`). Its `trySymbolTable` iterates
**the alias symbols of every table in scope from `enclosingDeclaration`**, keeps
each one that resolves to the target, and then:

```go
if len(candidateChains) > 0 {
    // pick first, shortest
    slices.SortStableFunc(candidateChains, c.compareSymbolChains)
    return candidateChains[0]
}
```

`compareSymbolsWorker` (`internal/checker/utilities.go:366`) breaks the tie on
`compareNodes(s1.Declarations[0], s2.Declarations[0])`, and `compareNodes`
(`:392`) is **file index in the program, then source position**. Every line
number here is from `grep -n` on the declaration.

So a module symbol's printed name is always *some alias's* name and never the
symbol's own — which in this port is the stripped file path.

## 15. The rule, and why the first rule was wrong

The obvious rule falls straight out of §14: **among the aliases in scope that
resolve to this module, the earliest-declared one supplies the name.** It
explains both counter-examples `tsr-6j2` records —
`compiler/es6ImportNameSpaceImport` (`nameSpaceBinding2` prints
`typeof nameSpaceBinding`) and `compiler/modulePreserve4` (`g2` prints
`typeof g1`) — which are the same mechanism twice.

Measured over the corpus: **706 lines print a module object's name**, 168 cases,
top-1 8.5%. The earliest-declared rule predicts **677 of them (95.9%)**.

**And it is not the mechanism.** The residue contradicts itself in *both*
directions, which a merely-incomplete rule cannot do:

```
  3  baseline `typeof g1`   rule `typeof g2`     <- upstream took the EARLIER
  3  baseline `typeof r`    rule `typeof ns`     <- upstream took the LATER
  2  baseline `typeof ns3`  rule `typeof ns`     <- upstream took the LATER
```

`compiler/unusedImports_entireImportDeclaration` is the decisive one: three
namespace imports of `./a` in one file, and `ns`, `ns2`, `ns3` **each print their
own name**. Upstream distinguishes that case from `es6ImportNameSpaceImport`
through `cloneTypeAsModuleType` in `resolveESModuleSymbol`
(`checker.go:15568`), which gives a namespace import a *cloned* type carrying the
alias's own symbol. This port has no clone, so **no tie-break reproduces both**.

A control says the same thing from the other side: the naive *"print the local
alias"* rule gets **659** right against the earliest-declared rule's 677. The
whole tie-break is worth **18 lines**, and it breaks cases the naive rule gets
right. Neither rule is the mechanism; both are ~96% coincidences.

### What was built instead: refuse to name when it is ambiguous

Ambiguity is detectable **at the reference site, before anything is printed** —
count the aliases in scope that resolve to this module. That turns the residue
into a **gap** rather than a wrong line, which is the one thing this port could
never do while the name was baked at type creation.

| | lines | match | wrong-name | other |
|---|---:|---:|---:|---:|
| **exactly one alias in scope → PRINT** | **634** | **630** | **4** | 0 |
| two or more → GAP | 72 | — | — | — |

**630 right against 4 wrong — 99.4%**, 145 cases, top-1 9.5%. The 72 ambiguous
lines gap, which is what they already did.

## 16. What was built

**Two entry points, and the split is the safety property.**
`Checker::type_to_string` is untouched — 110 call sites across five checker
modules and a dozen test files, two of them being edited by other agents this
cycle. `Checker::type_to_string_at(id, reference) -> Option<String>` is new, and
`types_producer` routes only the rendering path through it. A caller with no
reference node **cannot** get a context-sensitive name, so nothing regresses by
omission; the property is structural rather than maintained.

`None` means *this port cannot name this type here*, and the caller renders a
gap. It is an `Option` and not a fallback string for exactly one reason: the
baked text for a module object is the file path, so a fallback would turn every
unnameable case into a confidently wrong line — the outcome that got `tsr-6ph`
refused twice.

`Checker::resolve_alias` gained the three module forms that make it reachable —
`getTargetOfNamespaceImport` (`checker.go:14724`),
`getTargetOfNamespaceExport` (`checker.go:14742`) and the `require("m")` half of
`getTargetOfImportEqualsDeclaration` (`checker.go:14441`) — each narrowed to
**exclude `export =`**, which is a different, unmeasured population (218 lines,
`bd tsr-e2u`).

**`printing.rs` was transferred to this slice and did not need to change**, which
is itself the finding: `printing::type_to_string` takes a `&Type` whose `text` is
already a `String`, so a context-sensitive name cannot live there. The fix has to
sit where the binder and the node table are, which is `Checker`.

### The tests, and four mutations that each redden exactly one

| mutation | reddens |
|---|---|
| **M1** — ambiguity picks the first candidate instead of gapping | `a_module_named_by_two_aliases_is_a_gap` |
| **M2** — the module test is `SymbolFlags::VALUE_MODULE` instead of "a declaration is a `SourceFile`" | `a_namespace_declaration_keeps_its_declared_name` |
| **M3** — drop the `export =` guard in `module_object_of` | `a_module_writing_export_equals_is_left_alone` |
| **M4** — drop the `NamespaceImport`/`NamespaceExport` arm | `a_namespace_import_prints_its_own_name_and_not_the_file_path` |

Each reddens **one** test and leaves the other five green. M2 is the one worth
keeping: `SymbolFlags::VALUE_MODULE` is carried by every `namespace N {}` as well
as by a file's module symbol, so a flag test would send every namespace through
the alias lookup and gap all of them.

## 17. Scored, per case, because a net hides a change that helps and harms

`examples/casedelta.rs`, before and after, joined per case:

```
  matched      295,302 -> 296,125     +823
  gradient      61.656% -> 61.827%    +0.17 points
  cases at 100%   2,202 ->   2,229     +27
  cases moved       150   (gained 823, LOST 0)
```

**Not one case lost a line.** That is the strongest per-case result available
here and it is exactly what the net cannot show. 27 cases finished outright,
including `compiler/aliasAssignments`, `conformance/moduleScoping` (31 lines),
`compiler/collisionExportsRequireAndAlias` and six `nodeResolution*`.

### The wrong column, which I did not predict and must not round away

The probe's own gradient, before → after:

```
  right   294,871 -> 296,125   +1,254
  gap     140,546 -> 138,793   -1,753
  wrong    43,643 ->  44,142     +499
```

**1,753 gap lines converted: +499 of them wrong.** On the suite's basis that is
**0.61 wrong per right**; on the probe's, 0.40. Against this workstream's other
measurements: 2.1 and 2.5 (the two refused `tsr-6ph` designs), 1.0 (the ALIAS row
as previously scoped), 0.14 (the export-marker arm).

**The +499 is not the naming.** The naming half was measured at 630/634 = 99.4%.
It is the **cascade** half — member accesses through a namespace object, which
were gaps and are now answered incorrectly because the receiver has a type at
last. Where exactly they come from is **unmeasured**; `TypeData::Anonymous`'s own
docs say a module symbol's `exports` table is read by nothing, so `ns.foo` ought
still to gap. `bd tsr-441` separates them, and the arm should not be widened to
`export =` modules until it is.

My prediction covered the row and not the collateral, which is the same error
`docs/conventions.md` records for cross-file aliases at 1.66×, pointed the other
way. The honest prediction, restated for whoever measures next: **the naming half
is 99.4% accurate and the resolution half is 0.61 wrong per right.**

## 18. Rule 3, re-derived rather than inherited

§10 refused the ALIAS row on one leg: *"≥80% of convertible lines reach their
target through an `export =`"*, measured 0.0%.

**That leg was a proxy**, and saying so is the point. The question it stood in
for was *"will the printed name be right?"*, and `export =` was the only way to
answer it while a module object's name was its file path. `type_to_string_at`
answers the real question directly: **630 of 634 unambiguous lines name
correctly, and the ambiguous ones gap.**

So the refusal **reverses, and it reverses because the constraint was removed,
not because the numbers moved.** The row after the change:

| form | before | after |
|---|---:|---:|
| `import a = require("m")` | 2,014 | **1,052** |
| `import * as ns from "m"` | 816 | **304** |
| `export * as ns from "m"` | 8 | **0** |
| **the whole ALIAS row** | **5,207** | **3,671** |

1,536 lines left the row, which is the 1,482 §10 predicted plus 54. `bd tsr-e2u`
is closed by this for the three module forms and stays open for `export =`.

## 19. How you would know this section is wrong

- **The naming rule is a coincidence.** The falsifier is the ambiguous bucket:
  if a single tie-break existed, the 72 two-alias lines would agree with it. They
  do not, in both directions, and `unusedImports_entireImportDeclaration` and
  `es6ImportNameSpaceImport` are the pair that cannot both be satisfied.
- **The literal is being typed rather than the symbol** — the general form of the
  M2 failure. `a_non_module_type_is_rendered_exactly_as_before` pins that
  `type_to_string_at` is the identity off the module path.
- **A case regressed.** `casedelta` joins per case and reads 0 lost over 150
  moved. A net would have hidden it.
- **The +499.** If `bd tsr-441` finds they are naming errors rather than cascade,
  §15's 99.4% is wrong and this arm needs the ambiguity test widened.

## 20. Open, cycle 14

- `open` — `bd tsr-441`, the +499 wrong, unseparated.
- `open` — `export =` modules, deliberately excluded (218 lines). Their target has
  a declared name and would print through the baked text; whether that text is
  right is unmeasured.
- `open` — `cloneTypeAsModuleType` (`checker.go:15568`) is the mechanism that
  would let the 72 ambiguous lines be named instead of gapped. Not ported, and
  it is a type-identity change rather than a printing one.

---

# Cycle 14b — `const data = []` types the symbol, not the literal

`bd tsr-5h0`, authorised by the lead without a pre-registered threshold on the
strength of a **measured zero** damage column. Sized by another agent in
`docs/architecture/checker-notes-evolvearray.md`; built here because the fix is
in `symbols.rs` and that agent correctly declined to reach into a file it does
not own (`git diff --cached --stat -- crates/tsr-checker/` = 0).

## 21. The anchor in the handover was wrong, and the file was right

The message located the fix at **`symbols.rs:1337-1339`**, "beside
`get_widened_literal_type_for_initializer`". Verified with `grep -n` on the
declaration:

```
crates/tsr-checker/src/symbols.rs:1399:    fn get_widened_literal_type_for_initializer(
```

`1337` is inside `report_circularity_error`, an unrelated function. The **file**
was right and the **line** was not — which is the third time this cycle a line
number handed over in prose has been wrong, and the reason `docs/conventions.md`
says the number comes from `grep -n` on the declaration every time.

The actual insertion point is `get_type_for_variable_like_declaration`
(`symbols.rs:1367`), between the annotation branch and the initialiser path.
That ordering is upstream's: `getTypeForVariableLikeDeclaration`
(`checker.go:16652`) returns `c.autoArrayType` at **`checker.go:16709`**, after
`declaredType != nil` has already returned at `:16694` and *before* the
initialiser's type is consulted. `autoArrayType` is `createArrayType(autoType)`
(`checker.go:1360`), which prints `any[]`.

## 22. What was built, and which guards are ported

Upstream's condition (`checker.go:16696`-`:16698`) is
`noImplicitAny && IsVariableDeclaration && !IsBindingPattern(name) && no export
modifier && not ambient`, then `isEmptyArrayLiteral(initializer)`.

The **three syntactic guards are ported** and each has a test.
**`noImplicitAny` is not**: this port models no compiler options and assumes
strict throughout, the same assumption `array_literals.rs` and `unions.rs`
already state for `strictNullChecks`. A case compiled with `noImplicitAny` off
takes a different upstream path; that is a known divergence, recorded rather than
discovered later.

### The falsifier, which is the whole design

`check_array_literal` (`array_literals.rs`, `checker.go:8098`) correctly answers
`never[]` for `[]`, and **51 currently-right `>[] : never[]` lines depend on it.**
A fix that typed the *literal* instead of the symbol would read identically and
break all 51 while the gradient still appeared to rise.

`the_literal_is_still_never` pins it over `const`/`let`/`var`, and the named
mutation is **M5 — make `check_array_literal` answer `any` for an empty
literal**. It reddens that test and **only** that test; the other five stay
green. `array_literals.rs` belongs to another agent and was restored unmodified.

## 23. Scored, and it misses the inherited prediction low

`examples/casedelta.rs`, joined per case against the naming commit `c91314c`:

```
  matched      296,125 -> 297,130   +1,005
  cases at 100%  2,229 ->   2,235       +6
  46 cases moved, gained 1,005, LOST 0
```

**No case lost a line**, which is the falsifier holding at corpus scale as well
as at unit scale.

| | inherited estimate | measured |
|---|---:|---:|
| `right` | **+1,775** (range 1,420–2,130) | **+1,005** |

**The measurement is 43% below the central estimate and below the bottom of the
stated range.** `checker-notes-evolvearray.md` already flagged 1,775 as having
one measured leg and one *predicted* leg and asked for it to be treated as an
upper bound; it was, and the upper bound was not tight. I did **not** register my
own prediction before building — the lead authorised this without a threshold and
I took that as licence to build directly, which means this cycle produced a
scored number and no scored *forecast*. That is the weaker of the two outcomes
and it is mine.

**Concentration, quoted with the number as required:**
`compiler/deeplyDependentLargeArrayMutation2` alone is **+614 of the 1,005
(61.1%)**, then `typedArrays` +132, `deeplyDependentLargeArrayMutation` +51,
`controlFlowArrays` +46. Top-5 is **91.3%**. This is a concentrated fix in a
handful of large control-flow files, not broad capability — the same shape the
handover warned of (top-1 68.6%, top-5 90.9%), reproduced independently at 61.1%
and 91.3%.

`x.push(e)` widening was **not** built: 53 lines corpus-wide, all already wrong.

## 24. Open, cycle 14b

- `open` — `noImplicitAny` is assumed on. Unmeasured how many corpus cases turn
  it off and therefore take upstream's other path.
- `open` — the +1,005 is `matched` only. The `wrong` column for this arm was not
  separated, for the same reason as `bd tsr-441`: one probe run per column and
  the budget went on the falsifier instead. The arm can only replace a gap or a
  wrong answer with `any[]`, so it cannot un-match a matching line — which the
  zero-loss column confirms — but "how many wrong lines did it *create*" is
  genuinely unanswered.

---

# Cycle 15 — the +499 separated, and the rule it yields

Measured 2026-08-06 at `5eb252c` by `examples/nameres.rs`, extended with a
`WrongCause` classifier. `bd tsr-441`.

## 25. The attribution, and why it is sound

§17 measured the naming arm converting 1,753 gap lines into +1,254 right and
**+499 wrong**, and asserted — as an inference, not a measurement — that the
+499 was "the cascade half, member accesses whose receiver now has a type".

The attribution rests on a property of the **previous** state rather than on a
reason string: before `c91314c` a module object had no reachable type at all, so
a line whose receiver chain roots at one could not have been answered. Any such
line that is wrong today is therefore new. The classifier walks each wrong line's
receiver chain by AST — a wrong line has no `gap_reason` — and asks whether any
receiver's type is `TypeData::Anonymous` over a symbol one of whose declarations
is a `SourceFile`, the same positive test `Checker::is_module_symbol` makes.

| cause | lines | cases | top-1 |
|---|---:|---:|---:|
| the line **is** a module object's name, named wrongly | **6** | 3 | 50.0% |
| a member access **rooted at** a module object | **328** | 37 | 18.9% |
| unrelated to the naming arm | 43,092 | 3,627 | 23.2% |

The 6 are the naming rule's own residue and they are the expected ones —
3 in `compiler/modulePreserve4`, the `g1`/`g2` pair §15 already named. **The
inference in §17 was right about the mechanism.**

334 against the 499 measured at `c91314c`: the difference is that this run is at
`5eb252c`, three merges later, so other agents' work has moved part of it. The
*shape* is what this section is for, and the shape is stable.

## 26. It is not a type error. It is the same naming problem one level down

The pairs are the finding, and they are almost uniform:

```
  32  upstream `() => import("./…_Widgets").Widget1`      ours `() => Widget1`
  24  upstream `() => import("./…").SpecializedWidget.Widget2`  ours `() => Widget2`
  10  upstream `typeof Backbone.Model`                    ours `typeof Model`
   8  upstream `PropTypes.Requireable<boolean>`           ours `Requireable<boolean>`
   8  upstream `typeof m4.d`                              ours `typeof d`
   6  upstream `typeof Widgets.SpecializedWidget`         ours `typeof SpecializedWidget`
```

**The type is right. The name is missing its qualifier.**

Upstream's `getAccessibleSymbolChain` (`internal/checker/symbolaccessibility.go:373`)
returns a **chain** — `[Widgets, Widget1]` — and `symbolToEntityName` prints it
dotted. `Checker::type_to_string_at` ports the *last hop only*: it finds one
alias naming a module object and prints that one name. Symbols reached
**through** that alias are printed by the pre-existing baked text, which is the
declared name with no qualifier at all.

So the collateral is wrong **by construction**, not by accident: I ported half of
one mechanism, and the half I did not port is the half that renders everything
the ported half unblocks.

The residual 52 lines are a different and benign family — `any [ours: 0]` 34 and
`any [ours: number]` 18 — where upstream itself answers `errorType` and this port
now computes a real type. Those are the mirror of §2's finding and are not
damage in any sense the gradient measures.

## 27. The rule, for `docs/conventions.md`

`docs/conventions.md` already records the cascade running both ways: a fix
converts 1.66× its row, and it can also *manufacture* wrong lines in rows it
never targeted. Both of those are stated as facts about **size**. Neither says
when the sign flips, and the answer turns out to be structural:

> **A fix that makes X computable hands X's *contents* to a renderer that was
> never asked to render them before. If those contents are rendered by the same
> mechanism you have just ported *partially*, the collateral is wrong by
> construction — every line of it — and no amount of care inside the target row
> changes that.**
>
> The check is one question, asked before building: **what will render what this
> unblocks, and is it the same mechanism I am half-porting?** If it is, forecast
> the collateral at the *un*ported half's failure rate, not at the ported half's.

Applied backwards, it predicts this cycle exactly. `type_to_string_at` names the
last hop at 99.4%. Qualification of everything reached through that hop is
**0%** ported. The 328 collateral lines are members reached through a module
object, and they are wrong at essentially 100% wherever upstream qualifies.

It also predicts the two refusals that came before it. `tsr-6ph`'s printing
design and its lookup-only twin measured 2.1 and 2.5 wrong per right and
`docs/conventions.md` records the tell — *"two designs that differ in what they
print, and agree to within 20% on what they break, are not two designs"*. They
agreed because both left the same renderer half-ported.

**How you would know this rule is wrong:** a fix whose unblocked contents are
rendered by a mechanism that is fully ported, producing collateral at the target
row's accuracy rather than at the unported half's. That is the case worth
finding, because it would bound the rule rather than refute it.

## 28. Item 1, re-derived: the ALIAS row was already built

The lead asked for the ALIAS row to be built now that §18's refusal had reversed.
Re-derived at `5eb252c`, it should not be, and the reason is that `c91314c`
**is** the build.

| verdict | lines | share |
|---|---:|---:|
| reached AND typed — convertible | **376** | 10.2% |
| reached, its own type gaps (kind 2) | 1,203 | 32.8% |
| module not resolved | 1,620 | 44.1% |
| module resolved, name not exported | 362 | 9.9% |
| no replay for this form | 110 | 3.0% |

Of the **376 convertible**, **323 sit under forms `resolve_alias` already
handles** — 296 `import a = b` and 27 `export { q }` — where the discrepancy is
that this probe's replay looks names up with `SymbolFlags::all()` while the
checker re-checks the meaning upstream passes. Those are `bd tsr-sgd`, not new
work. That leaves **53 lines under unhandled forms** (52 default imports, 1
qualified `import a = b.c`), against the 800-line threshold this workstream has
now applied four times.

**And widening to `export =` — the thing §16 deferred and the lead asked me to
gate on `tsr-441` — converts exactly zero lines.** Control C9 shows `export =`
firing on 218 `import a = require` lines and 72 `import * as ns` lines, and Leg 1
shows **0 of the convertible set** reaching its target that way, for every form.
The 218 are non-convertible because the `export =` target's own type gaps. So the
deferral costs nothing and the gate is moot.

**REFUSED, and no rule was pre-registered for it** — the measurement above was
taken for §25's purpose and I read the decision off it afterwards, which is
exactly the shape `docs/conventions.md` warns is caught by nothing. It is
recorded as a re-derivation against a standing threshold rather than dressed up
as a pre-registered rule.

## 29. What is actually left in this row, for whoever takes it

- **`bd tsr-sgd`** — the namespace-exports arm in `BindResult::resolve_name`.
  Now carries three separate populations: 434 lines from the name-resolution row,
  296 from `import a = b`, and 27 from `export { q }`. Still not summed; they were
  measured by different routes.
- **Qualified naming** — the mechanism §26 names. It would fix the 328 collateral
  lines *and* is the prerequisite for `import a = b.c` (331 lines, "resolvable,
  unprintable" since the first cycle). This is `bd tsr-awa`, the natural
  successor to `tsr-6j2`, and the largest single thing this page has pointed at.
- 1,620 `module not resolved`, still unmeasured as to how many upstream also
  fails (`bd tsr-m41`).

## 30. Item 3, cross-file interface merging: the premise is wrong

`bd tsr-9or.1` is quoted as *"28.50% of 10,303 lines — ~2,936 — are named
interfaces we fail to **merge** across files"*, and it was handed to this slice
because declaration merging is the binder's and `declared.rs`'s, both owned here.

**The binder already merges them.** Two files, each declaring `interface I`,
bound into one `BindResult`:

```
BINDER : symbol I has 2 declarations, 2 members: ["a", "b"]
CHECKER: v : I
  get_property_of_type(a) = false      <- declared in the OTHER file
  get_property_of_type(b) = true       <- declared in v's own file
```

`Binder::merge_globals` (`crates/tsr-binder/src/binder.rs:552`) calls
`merge_symbol` (`:630`) for every name already in `globals`, and `merge_symbol`
unions the `members` tables. The merged symbol is correct and complete.

**The checker's member lookup does not see it.** `get_declared_type_of_class_or_interface`
(`declared.rs:699`) stores `members = Some(symbol)` — the *symbol*, whose table
holds both — so the type is pointing at the right place, and the lookup that
answers `false` for `a` is downstream of that.

Two consequences, and they change who owns the item:

1. **It is not a merging defect and no binder or `declared.rs` change addresses
   it.** Anyone sizing "declaration merging" as binder work is sizing the wrong
   subsystem — the same class of error as `checker-notes-rank.md`'s rows whose
   *name* pointed at the wrong step.
2. **`get_property_of_type` lives in `members.rs`**, which this slice does not
   own. So this is a hand-over with a reproduction, not a build.

**Not sized.** The ~2,936 figure is another agent's, over a population this page
has not re-derived, and §28 is this cycle's second reminder that a number taken
for one purpose should not be read as a decision for another. The measurement
that would settle it is one pass: over the corpus, count gap lines whose receiver
type is a named type whose symbol has **more than one declaration**, and split by
whether the property being looked up is declared in the same file as the
reference. The asymmetry above says that split is the whole item.

`bd tsr-9or.1` updated with this; the reproduction is the six lines above.

---

# Cycle 16 — `bd tsr-awa`, the qualified name: REFUSED at 30.7%

Measured 2026-08-06 at `c592d0f`. The lead scoped this as *"328 collateral +
331 `import a = b.c` ≈ 659 lines"*. Both halves of that are wrong, in opposite
directions, and the item still refuses.

## 31. My own rule, applied to my own build, before anything else

§27's rule asks: *what will render what this unblocks, and is it the same
mechanism I am half-porting?* Asked of a chain renderer, upstream answers in one
line. `symbolToTypeNode` (`internal/checker/nodebuilderimpl.go:644`):

```go
if core.Some(chain[0].Declarations, hasNonGlobalAugmentationExternalModuleSymbol) {
    // module is root, must use `ImportTypeNode`
    ...
    specifier = b.getSpecifierForModuleSymbol(chain[0], core.ResolutionModeNone)
```

**When the chain's root is a module with no accessible alias, upstream does not
print a dotted name at all — it prints `import("<specifier>").Rest`,** and that
specifier comes from `getSpecifierForModuleSymbol`
(`nodebuilderimpl.go:1249`), which is module-specifier *generation*: resolution
modes, `node_modules` rewriting, import attributes. **0% ported.** So a chain
renderer that printed those would be wrong at essentially 100%, and the rule says
to forecast that half at the unported mechanism's rate rather than the chain's.

Measured, over the 328 collateral lines:

| | lines | cases | top-1 |
|---|---:|---:|---:|
| a plain dotted name over accessible symbols — portable | **206** | 36 | 17.5% |
| needs `import("…")` — `getSpecifierForModuleSymbol` unported | **122** | **4** | 45.9% |

The 122 are four cases, all `privacy*`. They must gap, and the rule found them
before a line was written.

Of the 206, a further **74 are nested inside a larger baked form** —
`() => Widgets.Widget1`, whose text was fixed when the signature type was
created and which no reference-site entry point can reach without re-rendering
the whole form. That leaves **132** of the lead's 328.

## 32. The population is 1,318, not 659 — and 1,207 of it is not mine

The 328 was the wrong population to start from: it is only what *my* naming arm
broke. The right question is how many wrong lines **anywhere** differ from the
baseline by exactly a missing qualifier. A purely textual test — the baseline is
a bare dotted name whose last segment is exactly our answer, `typeof` agreeing on
both sides — so it needs no checker state and cannot be biased by what this port
models:

| | lines | cases | top-1 | top-10 |
|---|---:|---:|---:|---:|
| missing qualifier, **pre-existing** | **1,207** | 208 | 13.8% | 38.8% |
| missing qualifier, from the naming arm | 111 | 33 | 23.4% | 66.7% |
| **total** | **1,318** | 241 | | |

**1,207 lines were always wrong for this reason and nobody had measured them.**
That is 3.7× the collateral the item was scoped on, it is genuinely distributed,
and it survives the concentration check outright. The `331 import a = b.c` lines
are *not* part of it: their baselines are `any` 70, `number` 23, `string` 7 — they
need qualified-name **resolution**, a different item, and mostly no chain at all.

**So the item is much larger than stated and still refuses**, which is the useful
combination: the size was never the binding constraint.

## 33. The forecast, legs labelled, and the bar on the inferred one

Per `e6ab9c9`: say which leg is measured and which is inferred, and put the bar
on the inferred one.

- **Measured leg — the population.** 1,318 lines, 241 cases, top-1 13.8%.
  Textual, direct, and it passes every size bar this workstream uses.
- **Inferred leg — whether a portable chain construction reproduces upstream's
  dotted name.** All the uncertainty is here and it is one-sided: the fix cannot
  convert more than the population.

**The bar: ≥90% on the inferred leg.** Justified by the only two data points
this project has for a naming rule — cycle 14 *shipped* one measured at 99.4%
and *rejected* one measured at 95.9% as "not the mechanism". A bar below 95.9%
would license what was already rejected; 90% is the loosest defensible line.

**Measured: 30.7%.**

| verdict | lines | share |
|---|---:|---:|
| chain matches the baseline | **405** | **30.7%** |
| chain differs from the baseline | 157 | 11.9% |
| no chain could be built at all | **756** | **57.4%** |

**REFUSED**, by a factor of three.

## 34. Why — and it is the same shape as cycle 14, one level up

The construction measured is the obvious one: walk `Symbol::parent` upward,
prepending each container's name, stopping when the leading name resolves to that
same symbol from the reference site (upstream's `needsQualification`, reduced).
Two independent things kill it.

**1. The mechanism is not a container walk.** The misses say so verbatim:

```
  56  baseline `m1_im2_private.c1`   chain `m1_M2_private.c1`
  28  baseline `m1_im2_private.c1`   chain `m2_M2_private.c1`
  12  baseline `provide.Provide`     chain `foo.Provide`
   6  baseline `privateModule.publicClass`  chain `publicModuleInGlobal.publicClass`
   4  baseline `a.Point`             chain `A.Point`
```

Upstream prints `im2` — an **import alias** — where the walk prints `M2`, the
**declaration's container**. `getAccessibleSymbolChain` searches the symbol
tables in scope for an *alias* at **every** level of the chain. That is exactly
the mechanism cycle 14 ported for the last hop, and a container path is a
different thing that coincides 30.7% of the time. **This is §15's finding
repeated one level up, and the same 95.9%-looking coincidence.**

**2. `Symbol::parent` is not populated.** The binder sets it only for enum and
class members (`crates/tsr-binder/src/binder.rs`, the
`ENUM_MEMBER | CLASS_MEMBER` guard). For everything else there is no container
recorded at all, which is why **57.4% produce no chain**. Populating it is a
binder change with its own blast radius, and it would only supply the input to a
mechanism just shown to be the wrong one.

## 35. What is not the argument, and the risk that decides it anyway

It is tempting to note that all 1,318 lines are **already wrong**, so converting
405 and leaving 157 differently-wrong costs nothing, and gapping the 756 would
even improve the gap/wrong split. On the target row that is true.

**It is not the risk.** A change to how a named type is rendered touches **every
named type in the corpus**, and the 30.7% was measured only over the 1,318
already-wrong lines. What it does to the ~299,000 **right** lines is
**unmeasured**. A mechanism that is 30.7% accurate where we can see it, applied
to a population 200× larger where we cannot, is the exact shape of a build that
looks positive on its row and is negative everywhere else — and §27's rule is
about precisely this asymmetry, pointed at my own work.

Measuring that would mean running the whole gradient with the change in, which is
building it. So the honest order is: **fix the mechanism first, then measure.**

## 36. What would make this build win

1. **Port `getAccessibleSymbolChain` properly** — the alias search at every
   level, not a container walk. Cycle 14 ported its last hop and that half is
   live; the recursion is `getSymbolChain` (`nodebuilderimpl.go:1086`) and
   `getContainersOfSymbol`.
2. **Gap on any chain whose root is a module symbol**, so the 122 `import("…")`
   lines never print. That is `type_to_string_at`'s `Option` doing the same job
   it already does for ambiguity, and the lead is right that it must stay an
   `Option`.
3. **Then** the 1,318 is available, and the nested 74 plus whatever else lives
   inside baked text needs the node builder rather than a name.

`bd tsr-awa` updated with all of it. The population, the split, the 30.7% and the
verbatim misses are the expensive half of the next attempt and they are now done.

## 37. How you would know this section is wrong

- **The 1,318 is a textual artefact.** The test requires the baseline to be a
  bare dotted name whose last segment is *exactly* our answer, with `typeof`
  agreeing. A false positive would need us to answer the correct final segment
  of a name we never computed. The falsifier is a sample; none was taken, and
  that is the weakest claim on this page (`open`).
- **The 30.7% is an artefact of an unpopulated `Symbol::parent`.** It is not:
  the 157 *built* chains are wrong for a stated, verbatim reason, and 405/562 =
  72.0% even among lines where a chain existed — still far below the bar.
- **`import("…")` is rarer than 122/328.** The falsifier is the split, printed
  per line, concentrated in four named cases.

---

# Cycle 17 — `getMergedSymbol`: the redirect the binder never recorded

`bd tsr-9or.1`. Anchors re-taken with `grep -n` on the declarations:
`getMergedSymbol` **`internal/checker/checker.go:14355`**, `recordMergedSymbol`
**`:14372`**, the `mergedSymbols` field **`:666`**. All three as briefed — the
first prose line numbers this session to survive checking.

## 38. The forecast, registered before the measurement was run

- **Measured leg — the floor, 702 lines.** Another agent's, over the
  named-receiver arm: 702 merged across files, 494 declared in one file and not
  the item, 1,403 whose receiver is not a bare global name. **I have not
  re-derived it**, so I carry it as theirs and quote the floor as instructed.
- **Inferred leg — the 1,403 unmeasured receivers.** The test undercounts by
  construction and cannot overcount, so the interval is `[702, 2,105]`,
  asymmetric and open upward, not a ± around 702.
- **A second inferred leg, downward, which is mine and is not in that
  interval.** My redirect is applied at `BindResult::symbol_of` as well as at
  `resolve_name`, because upstream's `getSymbolOfDeclaration` is
  `getMergedSymbol(node.Symbol())`. That is **broader than the reproduction
  needs**, and it can move lines that are currently *right*: a declaration name
  in the source's file now reports the target's symbol. Nothing bounds that from
  the reproduction.

**Forecast: ≥ +702, central +900, upside open to ~+2,100, and a downward risk
on the `symbol_of` leg that I cannot bound in advance.** The bar sits on the
inferred legs, per `e6ab9c9`; the floor is the only part I would defend.

## 39. The renderer question, verified rather than inherited

`c592d0f` asks what renders what this unblocks. The answer here is the
favourable one and it is checkable: what becomes reachable is **ordinary
interface members** — `a: string`, `b: number` — whose types come from written
annotations through `get_type_from_type_node`, the same path that already prints
them for a reference in their own file. There is no second mechanism involved
and no half-ported one: the member was always renderable, it was simply not
reachable from the other file.

That is the opposite of `tsr-4qx`, whose collateral is *instantiated* types
going to a printer nobody has checked — the shape that produced the 328.

## 40. Built, and scored against the forecast — which it misses low

`Binder::merge_symbol` now records `merged[source] = target`
(`recordMergedSymbol`, `checker.go:14372`), `BindResult::merged_symbol` is the
port of `getMergedSymbol` (`:14355`), and **`resolve_name` applies it at every
scope-table hit** — which is exactly where upstream applies it: the
`NameResolver.Lookup` hook is `c.getSymbol` (`checker.go:1474`), whose first
line is `c.getMergedSymbol(symbols[name])` (`:2176`).

### The `symbol_of` leg was forecast as a risk, measured, and dropped

§38 registered a second inferred leg, downward and unbounded: applying the
redirect at `BindResult::symbol_of` too, because upstream's
`getSymbolOfDeclaration` is `getMergedSymbol(node.Symbol())`. Both variants were
measured against the same baseline:

| | matched | cases moved | gained | **lost** |
|---|---:|---:|---:|---:|
| `resolve_name` + `symbol_of` | 299,695 | 83 | 484 | **80** |
| **`resolve_name` only** | **299,721** | 55 | 459 | **29** |

**The broader variant is worse on both columns**, so it is not shipped. That leg
existed in the forecast precisely so that this comparison would be made rather
than assumed, and it is the only reason the shipped version is the narrow one.

### Scored

```
  matched   299,291 -> 299,721   +430
  cases moved 55, gained 459, LOST 29
  cases at 100%  2,277 -> 2,277     +0
```

| | forecast | measured |
|---|---:|---:|
| lines | floor **702**, central +900, open to ~2,105 | **+430** |
| case gate | not forecast | **0** |

**It misses the floor by 39%**, and the floor was supposed to be the part that
could be defended. §38 recorded that I had not re-derived the 702 myself and was
carrying another agent's measurement; that caveat is the only thing that makes
this a *known* risk rather than a surprise, and `docs/conventions.md` is explicit
that flagging a number and then quoting it is worse than not flagging it. I
quoted it. **The floor was not a floor.**

Two candidate reasons, unseparated: the 702 was measured over the named-receiver
arm with a test that undercounts, so its population and this change's population
are not the same set; and this change reaches only `resolve_name`, while the 702
may include lines reached through paths that still hand out a stale symbol.

### 29 lines lost, and they are not zero

Every other build this session lost **nothing**. This one loses 29 —
`compiler/underscoreTest1` 11, `compiler/thisBinding2` 4,
`conformance/nonPrimitiveInGeneric` 4, `conformance/nonPrimitiveStrictNull` 4.
They are not investigated (`open`, `bd tsr-9or.1`). A merge that redirects a
reference to a symbol carrying *more* declarations can change an answer that was
right for the narrower symbol, and `nonPrimitive*` and `thisBinding2` are the
shape to look at first.

The trade is +459 for −29, 15.8 : 1, and the case gate does not move at all.

### The waiting test, inverted

`crates/tsr-checker/tests/members_cross_file_merge.rs` asserted `(false, true)`
on purpose, with a comment saying to invert rather than "fix" it when the
redirect landed. It is inverted to `(true, true)` in this commit and the comment
replaced. It failed loudly first, which is what it was written to do.

### A record corrected in the binder's own tests

`tests/program.rs::a_local_shadows_a_global` asserted that `b.ts`'s `declare var
name` wins over `a.ts`'s. **It no longer does, and the old expectation was
pinning the absence of declaration merging rather than a scoping rule.** Two
script files writing the same global do not shadow — they merge, into one symbol
carrying both declarations, and `SetValueDeclaration` keeps the first. The
assertion is replaced with the merge itself, which is strictly stronger.

## 41. `tsr-4qx` — not started, and the reason is budget not judgement

Queued behind this deliberately: the two change different things — this changes
**which** symbol a lookup receives, `tsr-4qx` changes **what** that symbol's type
carries — so a shared before/after pair could not attribute a gradient move. The
pair for this one is `<this commit>^..<this commit>`.

The renderer question is **already answered against it and unfavourably**: what
`tsr-4qx` unblocks is *instantiated* types going to a printer nobody has checked,
which is the shape that produced the 328 (§26). That is not a refusal — it is the
first thing the next agent should measure, before the line count.

---

# Cycle 18 — two measurements, no build

## 42. `tsr-awa` re-measured on the post-redirect binder: the refusal survives

§34 gave two reasons the parent-walk chain fails. I later flagged a possible
confound against my own number: `predicted_chain` stopped when
`resolve_name(...) == Some(id)`, an **identity** comparison, and before
`a05bf94` one logical symbol had several `SymbolId`s — so the test could fail
spuriously wherever a name was declared in more than one file, and the 30.7%
would be a lower bound.

Corrected: both sides now go through `BindResult::merged_symbol`, which is
idempotent and total, and the probe re-run at `8229b58`.

| | pre-redirect | post-redirect, merged comparison |
|---|---:|---:|
| chain matches the baseline | 405 (**30.7%**) | 409 (**31.0%**) |
| chain differs | 157 | 158 |
| no chain could be built | 756 (57.4%) | 751 (57.0%) |

**+0.3 points. The confound was not the confound**, and the refusal survives its
own correction against a bar of ≥90%. The 57% that build no chain are
`Symbol::parent` being unpopulated outside `ENUM_MEMBER | CLASS_MEMBER`, which
§34 gave as the second reason and which is now the only one.

Worth keeping separate from the result: I raised the confound, tested it, and it
was not there. A flagged risk that measures to nothing is still worth the run —
the alternative was a refusal carrying an untested asterisk.

## 43. `tsr-4qx`: the renderer question, answered before the line count

The lead asked for the renderer measured **first**, on the grounds that
instantiated types go to "a printer nobody has checked". The printer is checked,
and the answer is sharper than that.

Steps 1–2 are done (`8fa6a3e`). Steps 3–4 are `declared.rs` and mine. Step 4
flips `create_type_reference` to carry members; step 3 instantiates inside the
seam. `getTypeOfPropertyOfType` re-taken with `grep -n`: **`checker.go:18951`**,
as cited.

### What renders what this unblocks, in two hops

**Hop 1 — `instantiate_type` (`inference.rs:262`) is partially ported, and its
failures are honest.** It handles exactly three shapes: a hit in the
substitution map, a type reference, and a union. Everything else — a signature
type `(value: T) => void`, a type literal `{ a: T }` — falls to `error`
(`inference.rs:299`). Those are **gaps, not wrong lines**, so this hop does not
trip `c592d0f`: an unported branch that answers `errorType` manufactures
nothing.

**Hop 2 — `type_reference_text` (`declared.rs`) is the problem, and it is my own
328 again.** When instantiation *succeeds*, the result is printed by:

```rust
let name = self.binder.symbols().get(symbol).name.to_string();
```

**the symbol's own unqualified name**, plus `self.type_to_string(argument)` —
the context-free entry point — for each argument. So an instantiated reference to
a namespace-scoped generic prints `Requireable<boolean>` where upstream prints
`PropTypes.Requireable<boolean>`, and a qualified type *argument* is unqualified
too.

That is **exactly** the mechanism §26 identified and §33 measured: the qualified
name, whose portable construction reproduces the corpus at **31.0%** against a
bar of 90%. And because `type_reference_text` bakes its result at type creation,
`type_to_string_at` cannot repair it afterwards — the same baked-text wall §16
recorded, hit a second time from a different direction.

### The verdict the rule gives

`c592d0f`: *forecast the collateral at the unported half's failure rate.* Hop 1
is safe. **Hop 2 is a mechanism this workstream has already measured at 31.0% and
refused**, and every instantiated member that needs a qualifier is drawn from
that pool.

So `tsr-4qx` is **blocked on `tsr-awa`**, not on its own size — and its size
(5,161 lines, ~1.08 points, measured by another agent at `5eb252c` and **not
re-derived here**) is not the binding constraint. Building it now converts the
members whose names need no qualifier and manufactures wrong lines for the rest,
in an unmeasured ratio.

**No forecast is registered and nothing is built**, because the honest next step
is a measurement rather than a threshold: *of the lines `tsr-4qx` would unblock,
what share print a qualified name?* That is one probe pass over the
instantiated-generic receiver population, joining each unblocked member's
baseline against whether its type reference is dotted. It is the number that
decides the item and nobody has taken it.

**What would unblock it:** `tsr-awa` — the alias search at every chain level
(`getSymbolChain`, `nodebuilderimpl.go:1086`) — after which `type_reference_text`
can qualify, and then `tsr-4qx`'s size becomes the question.

### And a note on ordering that survives either answer

Whatever the share turns out to be, steps 3 and 4 must land in **separate
commits** from `getMergedSymbol` (`a05bf94`) and from each other: `a05bf94`
changed *which* symbol a lookup receives, step 4 changes *what* its type carries,
and step 3 changes nothing observable. Only step 4 moves a line, which is what
makes the pair attributable.
