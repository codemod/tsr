# Names that do not resolve, and symbols with no value declaration

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
| upstream also errors (`any` / `error` on the baseline) | **4,467** | **53.9%** |
| upstream has a real type | 2,904 | 35.0% |
| the root's own baseline line could not be located | 921 | 11.1% |

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
