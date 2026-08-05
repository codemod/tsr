# What the cross-file module seam is actually worth

Status: measured 2026-08-05 at **`b59dac0`** (`origin/main`), over the
9,538-case `.types` population, from **one pinned binary in an isolated
worktree with its own submodule checkout**. The instrument is
`crates/tsr-conformance/examples/module_blocked.rs`, added in the same commit as
this file. Upstream references are to `vendor/typescript-go` @ `5b1047d10`
(`git ls-tree HEAD vendor/typescript-go`, verified in the same worktree).

**Every number here is an assertion line in a `.types` baseline unless the
column says `cases`.** No count on this page is a node count.
`docs/architecture/checker-notes-calls.md` counts call-expression *nodes* and
the board has twice turned a node count into a line count by omission, so the
unit is restated on every table.

---

## 0. The findings, in the order they change a decision

1. **The 4,156 being quoted for module resolution is wrong in both
   directions.** 675 of those lines are same-file or neither-half forms that are
   not on this seam at all, and the seam blocks ~2,700 lines the two rows do not contain.
   The measured population is **6,074 lines**, in twenty-odd rows. §5.
2. **The population is not the conversion, and here the two differ by 9.4×.**
   With the seam mocked out — the specifier resolved by hand and the target
   export's type asked for directly — **647 of the 6,074 lines answer exactly
   what upstream answers**. 3,539 more wait on a *second* thing the seam does
   not provide: a type for a module object. §4.
3. **The two rows are not one item and must never be added.** Their KINDs are
   different mixtures, their case profiles are opposite (`finishes` 104 against
   0), and one of them finishes **no case in the corpus even if closed
   entirely**. §2, §3, §6.
4. **`TERMINAL` was, as suspected, uninformative for both rows — but the repair
   does not rescue them.** Establishing the kind from the alias *form* gives
   row A 1,953 kind-2 / 423 kind-1 / 47 neither, and row B 1,416 / 178 / 27.
   §2, §3.
5. **`bd tsr-4r4`'s 105-line disagreement is not an instrument defect.** It is a
   comparison across `c60b086`, which moved the row by **113 lines**. At one pin
   the three instruments agree to 1 line and to the case. §8.
6. **A number in `checker-notes-symbols.md` needs correcting as a consequence
   (`bd tsr-95i`):**
   `export { q }`'s "population 198" is the **post-build residue**, so the
   47.5% conversion rate is quoted against the wrong denominator. §8.

---

## 1. The instrument, and the reconciliation before anything is quoted

`module_blocked.rs` routes through `types_producer::assertions_for_case_with_ids`
— the entry point the types suite scores — so its denominator is the gradient's
**by construction** and not by resemblance (`docs/conventions.md`, *"a probe
that re-implements the harness is measuring a different compiler"*).

```
cases judged:             9538
assertion lines upstream: 478954
  aligned:                468900 (97.90%)
  exactly right:          291895 (60.94% of upstream lines)
  gap  (we said `error`): 139497
  wrong (ported, defect): 37508
CONTROL A1 right+gap+wrong-aligned = 0 (must be 0)
```

291,895 / 478,954 = **60.94%**, which is the gradient at `origin/main` to the
line. Numerator *and* denominator agree, which is the check
`docs/conventions.md` says an unexplained numerator gap fails.

### Cross-checked against `rank_board` from the same binary set

`rank_board` was built and run from **this worktree at this commit**, so the two
columns below are not a comparison across pins:

| row | `rank_board` | `module_blocked` |
|---|---:|---:|
| declaration name … `SymbolFlags(ALIAS) / no value declaration` | 2,422 lines / 1,002 cases / top-1 3.5% / top-10 16.7% | **2,423** / 1,002 / 3.5% / 16.7% |
| reference … `SymbolFlags(ALIAS) / no value declaration` | 1,621 / 595 / 4.9% / 23.6% | **1,621** / 595 / 4.9% / 23.6% |

**The one-line difference is explained and is the same line control S1 caught.**
`rank_board` keys a row by `(row_key, cause)`, so a line of this row that
`cause()` calls `PropagatedSpan` prints as a separate row in Ranking B. Exactly
one row-A line has something gapped inside its span, and it is
`compiler/bigintArbirtraryIdentifier`'s `import { 0n as foo } from "./foo"` —
an import whose *name* is a numeric literal. The case counts being identical
(1,002 and 595) is the stronger half of the agreement.

### Controls

Three are pinned by **construction** — their value is fixed by a property of the
subject and was true before the file was written — and two by arithmetic.
`docs/conventions.md` records that only the first kind can see a semantic
inversion, and that every arithmetic control still read zero under the
polarity bug it describes.

| control | reads | verdict |
|---|---:|---|
| **S1** seed lines with a gapped line inside their span (a seed is a **leaf**) | **1** | explained: the `import { 0n as foo }` line above |
| **S2** same-file seeds carrying a module specifier (the grammar has no place for one) | **0** | clean |
| **S3** cross-file seeds with no module specifier | **10** | explained: every one is error-recovery syntax — `import { 0n as foo }`, `import type defer * as ns1`, `import { type as as as as }`, `import x = require(<non-literal>)`, an invalid-syntax namespace import. 10 lines of 3,369. |
| **A1** right + gap + wrong = aligned | 0 | clean |
| **A2** the blocker walk's four buckets = every gap line | 0 | clean |
| — seeds whose declaration form is unclassified | **3** | one merged `import`/`interface` symbol in `compiler/allowImportClausesToMergeWithTypes`; §8 |

S1 and S3 are the two that can fail loudly in the direction the sums cannot see,
and both found something real rather than reading a decorative zero.

---

## 2. Row A — `declaration name … SymbolFlags(ALIAS) / no value declaration`

**2,423 lines over 1,002 cases**, top-1 3.5%, top-10 16.7%.

| form | lines | reach |
|---|---:|---|
| `import { x } from "./m"` | 890 | cross file |
| `import a = require("./m")` | 433 | cross file |
| `import * as ns from "./m"` | 256 | cross file |
| `import d from "./m"` | 232 | cross file |
| **`export { q }`** | **198** | **same file** — the arm built in `c60b086`; see §8 |
| `import a = b.c` | 158 | same file |
| `export { q } from "./m"` | 134 | cross file |
| `import a = b` | 67 | same file |
| `export as namespace N` | 46 | neither |
| `export * as ns from "./m"` | 8 | cross file |
| `UNCLASSIFIED FORM: InterfaceDeclaration` | 1 | — |

### The KIND, established rather than inherited

`rank_board` labels this row `TERMINAL`. That label is `cause()`'s **default
arm** and a declaration name is a leaf, so the row lands there whatever it
depends on — `checker-notes-rank.md`'s correction header, applied to the row it
was written about. This probe therefore ignores the label and reads the kind off
the alias *form*, which is what actually decides whether a module graph is
needed:

| half | lines | cases | top-1 | top-10 | kind |
|---|---:|---:|---:|---:|---|
| **cross file** | **1,953** | 873 | 4.4% | 15.6% | **2** — the seam |
| same file | 423 | 193 | 6.6% | 34.8% | 1 — *not* this seam (R4) |
| neither | 47 | 39 | 8.5% | 38.3% | — |

The same-file half reproduces `checker-notes-symbols.md` §4 exactly (423 / 193 /
6.6% / 34.8%), from a different instrument. **17.5% same-file, 80.6%
cross-file**, against that page's 17.8% / 82.2% — the small move is the build in
`c60b086` and is discussed in §8.

---

## 3. Row B — `reference … SymbolFlags(ALIAS) / no value declaration`

**1,621 lines over 595 cases**, top-1 4.9%, top-10 23.6%. Nobody had split this
row before; the board carries it only as a total.

| form | lines | reach |
|---|---:|---|
| `import a = require("./m")` | 593 | cross file |
| `import { x } from "./m"` | 480 | cross file |
| `import * as ns from "./m"` | 213 | cross file |
| `import d from "./m"` | 130 | cross file |
| `import a = b.c` | 92 | same file |
| `import a = b` | 86 | same file |
| `export as namespace N` | 25 | neither |
| `UNCLASSIFIED FORM: InterfaceDeclaration` | 2 | — |

| half | lines | cases | top-1 | top-10 | kind |
|---|---:|---:|---:|---:|---|
| **cross file** | **1,416** | 530 | 4.2% | 23.8% | **2** — the seam |
| same file | 178 | 57 | 18.0% | 61.8% | 1 — *not* this seam |
| neither | 27 | 18 | 14.8% | 70.4% | — |

**The two rows are differently composed and that is the argument against adding
them.** Row A's largest form is `import { x } from` (37%); row B's is
`import a = require(...)` (37%), which is namespace-shaped and needs a module
*object* type rather than a symbol lookup. Row B also carries **no export
form at all** — an export specifier has a declaration name and no reference
position — so `export { q } from` work touches row A only. Their case profiles
are opposite (§6), and the same-file half of row B is nearly twice as concentrated
as row A's (top-10 61.8% against 34.8%), so a single reading of "the alias
problem" is wrong about at least one of them whichever way it goes.

---

## 4. Past the blocker: what these forms answer once the seam is removed

`docs/conventions.md`: *"for a dependency-gated form, probe past the blocker,
not at it… what does this form answer once the blocker is removed? Mocked,
hardcoded, however cheaply."*

The mock: resolve the module specifier by hand against the program the harness
already built, take the target file's module symbol
(`Checker::export_symbol_of`'s route: the file's own symbol is the module
symbol), look the imported name up in its `exports`, ask `get_type_of_symbol`
for it, and compare `type_to_string` with **upstream's own answer on that line**.
Upstream's route is `getTargetOfImportSpecifier` (`checker.go:14647`) →
`getExternalModuleMember` (`:14667`) → `resolveExternalModuleName` (`:15101`),
all verified present at `5b1047d10`.

| outcome | row A | row B | seam-only lines (§5) |
|---|---:|---:|---:|
| specifier names no file in the program | 408 (20.9%) | 155 (10.9%) | 708 (11.7%) |
| the file has no export of that name | 122 (6.2%) | 79 (5.6%) | 319 (5.2%) |
| **namespace-shaped target, not mocked** | 697 (35.7%) | 806 (56.9%) | **3,539 (58.3%)** |
| target found, still `error` — **next blocker** | 326 (16.7%) | 151 (10.7%) | 638 (10.5%) |
| target found, typed, **wrong** type | 111 (5.7%) | 57 (4.0%) | 223 (3.7%) |
| **WOULD CONVERT — exactly upstream's type** | **289 (14.8%)** | **168 (11.9%)** | **647 (10.7%)** |

Read as a rate over the lines the probe could actually mock:
**row A 289/726 = 39.8%, row B 168/376 = 44.7%.** Both sit just under
`export { q }`'s measured 47.5%, which is the only conversion rate ever measured
for this family. Three independent measurements of the same family landing at
39.8 / 44.7 / 47.5 is the closest thing to a prior this project has for it.

**The single biggest finding in this table is the 58.3% row.** `import * as ns`,
`import a = require(...)` and `export * as ns` do not resolve to an export
symbol at all — their answer is a *type for the module object*, which this
checker has no machinery for. That is a second work item behind the same door,
it is larger than the first, and it is the only route to the 1,590 receiver-gap
lines in §5, because `ns.foo` needs the namespace's type and not the alias's
target.

---

## 5. What else the seam unblocks — and the sum that must be refused

The two rows are where an alias *declaration name* and an alias *reference*
gap. Every property access through an import, every call to one and every
initialiser fed by one lands in a **different** row. To enumerate them, each gap
line is walked down to its blocking leaves — three ways down, in the order the
evidence is strongest: the line is itself an alias; the reason names a
dependency the span test cannot see (a receiver, an initialiser, an annotation)
and that dependency is followed to its line; otherwise the maximal gapped lines
strictly inside the span. A line counts as the seam's only if **every** leaf
under it is a cross-file alias.

| row (assertion LINES) | seam-only | touches | cases | row's board total |
|---|---:|---:|---:|---:|
| declaration name … `SymbolFlags(ALIAS)` | 1,953 | 1,953 | 873 | 2,422 |
| reference … `SymbolFlags(ALIAS)` | 1,416 | 1,416 | 530 | 1,621 |
| `member name, the receiver is a gap: Identifier` | 736 | 736 | 267 | 6,749 |
| `property access, the receiver is a gap: Identifier` | 734 | 734 | 265 | 6,751 |
| `expression answered error: CallExpression` | 336 | 357 | 131 | 1,854 |
| `expression answered error: NewExpression` | 133 | 133 | 57 | 1,027 |
| `member name … : PropertyAccessExpression` | 60 | 60 | 38 | — |
| `property access … : PropertyAccessExpression` | 60 | 60 | 38 | — |
| twelve `declaration name … SymbolFlags(FUNCTION_SCOPED / BLOCK_SCOPED / PROPERTY)` rows | 400 | 404 | — | — |
| every other row | 246 | 325 | — | — |
| **TOTAL** | **6,074** (4.35% of the 139,497 gap lines) | **6,178** | 874 | |

**Stating which question the sum answers, before summing** — the rule
`docs/conventions.md` records being broken twice:

- `seam-only` answers *"which lines would a module graph unblock"*. It is the
  work item. 6,074.
- `touches` answers *"which lines have a cross-file alias somewhere
  underneath"*. It is **not** a work item, because a line with a second,
  independent blocker converts nothing when the seam lands.

Here the two are **1.02×** apart — the mixed bucket is 104 lines, 0.07% of the
gap. That is the one place this seam is *better* behaved than the precedent:
the row that over-counted by 2.7× had 83% of its lines turned back earlier. This
one does not. The check still had to be run to know that.

The walk over all 139,497 gap lines:

```
    6074    4.35%  all blockers are the cross-file seam
     104    0.07%  a cross-file alias AND something else
  116035   83.18%  no cross-file alias underneath
   17284   12.39%  UNRESOLVED — the named dependency reached no line
```

**`UNRESOLVED` is a real arm with a positive test, not a default** (the load-bearing
label here is `seam-only`, and it has to be earned). 17,284 lines is the honest
size of what this walk does not account for: a reason that names an initialiser
or a receiver the walker emitted no assertion line for. It biases **6,074
downwards**, not upwards — some unknown share of those lines is also alias-
blocked — so the seam-only total is a floor for the population and a ceiling for
the conversion, which are different bounds on different quantities.

---

## 6. Level 4, and the rules that were pre-registered on it

Registered in the probe's module doc **before the first run**, each on the
bucket that *is* the question rather than a proxy computed from it
(`docs/conventions.md`, "pre-register on the most direct bucket"). Each row
below is the `0` bucket of a level-4 histogram: the cases that would have
**nothing left** if the population closed.

| population | cases | `0` bucket | rule | verdict |
|---|---:|---:|---|---|
| R1 ceiling — row A cross-file, whole population | 873 | 96 (**11.0%**) | — | ceiling |
| **R1 measured — row A cross-file, mock-converts** | 873 | 3 (**0.3%**) | ≥ 5.0% | **FAIL** |
| R1 on cases where every seed could be mocked | 225 | 3 (**1.3%**) | ≥ 5.0% | **FAIL** |
| R2 ceiling — row B cross-file, whole population | 530 | 0 (**0.0%**) | — | ceiling |
| **R2 measured — row B cross-file, mock-converts** | 530 | 0 (**0.0%**) | ≤ 2.0% | **HELD** |
| R2 on fully-mockable cases | 139 | 0 (**0.0%**) | ≤ 2.0% | **HELD** |
| the whole seam (all 6,074 lines), ceiling | 874 | 211 (**24.1%**) | not pre-registered | — |
| the whole seam, mock-converts | 874 | 17 (**1.9%**) | not pre-registered | — |
| the whole seam, fully-mockable cases | 225 | 17 (**7.6%**) | not pre-registered | — |

**R2 is the interesting one: it held exactly, and it is a prediction that
something would *not* move.** The board's `finishes 0` for row B is not an
artefact of row granularity — the row finishes no case in the corpus even if
every one of its 1,621 lines is closed, and its ceiling reads 0.0% too. Row B is
a pure gradient item.

**R1 failed on the bucket it named.** The seam's own declaration-name row
finishes 3 cases measured against 96 at the ceiling, and both readings are below
the 5.0% bar `export { q }` set.

**The whole-seam rows were not pre-registered and are reported as post-hoc.**
They are the reason the recommendation in §7 is not simply "don't". The
downstream lines — the receiver-gap and call rows — are where the case value is,
and 7.6% on the sub-population where the mock is complete is above the bar R1
set for the row. That number is quoted here as an observation, **not as a rule
that passed**, because a threshold chosen after seeing the histogram is not a
test.

### Against myself: R1 was registered on a bucket that is a lower bound

The mock-converts column counts every namespace-shaped line as non-converting,
because the probe cannot build a module object type. For row A that is 35.7% of
the cross-file half. So `0.3%` is a floor and `11.0%` is a ceiling, and the rule
I registered reads the floor while calling it "measured". The rule fired
correctly against the evidence available, but **the bracket straddles the
threshold**, so the honest verdict is "R1 fails on the direct bucket, and the
direct bucket cannot yet decide the question". The third row (fully-mockable
cases, 1.3%) is the unbiased reading on the sub-population where the mock *is*
complete, and it fails too — which is what stops this from being a shrug.

---

## 7. The recommendation, and how it would be shown wrong

**Build it, as two items, and quote 647 rather than 4,156.**

1. **Specifier → file → export symbol** (`resolveExternalModuleName` +
   `getTargetOfImportSpecifier` / `getTargetOfImportClause` /
   `getTargetOfExportSpecifier`'s module-specifier branch). Measured worth:
   **647 lines convert**, of which 457 are in the two alias rows themselves and
   190 are downstream. A further 223 lines would turn from `error` into a
   **wrong** answer, which is a real cost in the column this project uses to
   decide a row is safe to leave alone, and 638 hit a next blocker immediately.
2. **A type for a module object** — `bd tsr-6ph` (`getTypeOfSymbol` for a
   namespace import; `getTargetOfNamespaceImport`, `checker.go:14628`). This is
   the larger item — 3,539 seam-only lines of ceiling, unmeasured
   — and it is the **only** route to the 1,590 receiver-gap lines, because
   `ns.foo` needs the namespace's type and not the alias's target.

Doing (1) without (2) leaves 58% of the seam's population untouched, and the
case gate almost entirely untouched.

### The prediction, for scoring

Recorded before the seam lands, with the mechanism named and a falsifier, per
`docs/conventions.md`:

- **Lines: +600 to +900** when (1) lands alone, point estimate **+650**.
- **Cases: ≤ 20**, and the level-4 table says most of them come from the
  downstream rows rather than the alias rows.
- **What must NOT move:** `member name, the receiver is a gap: Identifier`
  (6,749) and `property access, the receiver is a gap: Identifier` (6,751) must
  stay within a few dozen lines of where they are. Their seam-only share is 736
  and 734, and all but a handful of those sit behind a namespace-shaped import.
  If those rows move by more than ~200 lines on item (1) alone, my reading of
  the namespace forms is wrong.
- **How this could be right for the wrong reason:** the line count could land
  in range because the `still error` and `no such export` buckets convert for
  unrelated reasons while the mocked-convertible lines do not. The
  discrimination is per-form: the gain should be concentrated in
  `import { x } from` and `import d from`, and `import a = require(...)` — the
  single largest cross-file form by lines across both rows, 1,026 — should
  contribute almost nothing until (2) exists.
- **How the whole analysis would be shown wrong:** if item (1) converts more
  than ~1,500 lines, either the `NotMocked` bucket was not really blocked on a
  module object type, or this probe's resolver under-resolved and the
  `no file in the program` bucket is smaller than it reads.

---

## 8. Corrections to the record

`docs/conventions.md`: *"Correct the record when a number turns out to be wrong,
and note that it was corrected."*

### `bd tsr-4r4` — the 105-line disagreement is a pin difference, not a defect

`checker-notes-symbols.md` §9 reports `symbol_dispatch_split` at 2,430 against
`rank_board` at 2,535 and calls it an unexplained 105-line gap between two
instruments, adding that *"if the case count comes back at 1,027, the shared
suspect is `rank_board`"*.

It is neither instrument. The two numbers were taken **across a commit that
moved the row**:

- `rank_board`'s 2,535 was measured at **`c60b086^`** (§9 says so).
- `symbol_dispatch_split.rs` was **added by `c60b086`** (`git show --stat
  c60b086`: `crates/tsr-checker/src/symbols.rs`, `tests/export_specifiers.rs`,
  the example, the notes). Any run of it is therefore at `c60b086` or later —
  i.e. **with** the `export { q }` arm in the checker.
- At `b59dac0`, with no commit touching `crates/tsr-checker`, `tsr-binder`,
  `tsr-parser` or `tsr-compiler` since `c60b086` (`git log c60b086..HEAD --` is
  empty), `rank_board` reads **2,422** and this probe reads **2,423**.

So the row fell **2,535 → 2,422, by 113 lines, across `c60b086`**, and 105 of
the "gap" is that build. The residual is **7 lines**, and it is localised: this
probe and the split disagree only on `import * as ns from` (256 against 259) and
`export as namespace N` (46 against 50), plus the unclassified merged
`import`/`interface` symbol (3 against 1). All three sit in forms that carry
none of §7's conclusions. `bd tsr-4r4` carries this resolution and should be requoted
as a 7-line form-classification difference in two named forms.

### `export { q }`'s population of 198 is the post-build residue

This follows from the same fact and matters more, because it is quoted as a
scored prediction. `checker-notes-symbols.md` §4 lists `export { q }` at **198
lines** and §7 scores *"population 200–300 predicted, 198 measured — hit"*, then
computes the conversion rate as **94/198 = 47.5%**.

But the 198 was measured by an instrument that only exists in the commit that
built the arm, and this probe measures **the same 198 today, with the arm in
the checker**. 198 is therefore what the arm did **not** convert. The row lost
113 gap lines across that commit; if they came from this form — and no other
same-file form changed — the pre-build population was **~311**, and the rates
are:

| quantity | as published | as it reads once the pin is accounted for |
|---|---:|---:|
| `export { q }` population | 198 | **~311** |
| gap lines closed | — | **113** |
| net right lines | +94 | +94 (19 became `wrong`, not `right`) |
| conversion rate | 94/198 = **47.5%** | 94/311 = **~30%** (gap closure 113/311 = ~36%) |

`bd tsr-95i` carries the correction. The prediction's *population* leg was
scored a hit against a number measured after the thing it predicted had already
happened. The **direction of the
correction is against the more optimistic reading**, which is why §4's 39.8% and
44.7% should be read as the better prior for the cross-file forms, not 47.5%.

The one escape hatch, stated because it would overturn this: if the split was
run from a worktree holding its own source on top of `c60b086^` with the
`symbols.rs` change reverted, 198 is a pre-build population and this correction
is wrong. Nothing in `checker-notes-symbols.md` describes such a run, and its
§9 explicitly describes the *rank_board* run as the one taken at `c60b086^`.

---

## 9. What this page does **not** measure

- **The 12.39% `UNRESOLVED` walk bucket** (17,284 lines). A named dependency the
  walker emitted no line for. It can only make 6,074 larger.
- **The namespace-shaped half.** 3,539 seam-only lines whose answer is a module
  object type. Their conversion is unmeasured, and by §4's logic an estimate of
  it would be a population quoted as a conversion — exactly what this page
  exists to refuse.
- **The specifier resolver's own coverage.** 708 seam-only lines name no file in
  the program. **225 of them name a `declare module "x"` in the same program** —
  an ambient-module table (`tryFindAmbientModule`, `checker.go:15533`), which is
  a third work item — `bd tsr-okq` — and needs no file graph at all. The rest are bare specifiers
  (`react`, `foo`, `jquery`, `path`) that no module graph in this corpus
  resolves either. The probe prints the top unresolved specifiers on every run
  so this coverage is visible rather than asserted.
- **Whether the intermediate rules are ported.** A seam-only `CallExpression`
  line converts only if `check_call_expression` can then do its job. The mock
  answers for the *alias*, not for the node above it, so every downstream figure
  assumes the intermediate rule works. `checker-notes-calls.md` is where that
  assumption should be checked.

---

## 10. Commands

```bash
# The measurement. ~30s over the full corpus once built.
cargo run -p tsr-conformance --example module_blocked --release

# The cross-check in §1. Must be run from the SAME worktree at the SAME commit;
# the whole of §8 is what happens when it is not.
cargo run -p tsr-conformance --example rank_board --release
```
