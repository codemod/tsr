# Checker notes: `symbols.rs` — `rank_board` rows 9 and 10

Working notes for the slice that owns `crates/tsr-checker/src/symbols.rs`. Every
upstream line number here was `grep -n`-verified against the pinned submodule at
`5b1047d10`; the anchors gate cannot see a briefing, a `bd` note or a message
(`docs/conventions.md`, "The anchors gate does not see your briefing"), and one
number handed to this slice was off by two.

---

## 0. The findings, in the order they change a decision

**Every number below is the full corpus unless it says otherwise.** The first
draft of this page quoted the `conformance/` subset throughout; §8 lists what
that got wrong and by how much, dated, because `docs/conventions.md` requires a
corrected number to be visibly corrected rather than silently edited.

1. **Row 9 is not a missing dispatch arm, and neither is row 10.** Both arms
   existed at the commit the board was measured on. `getTypeOfFuncClassEnumModule`
   landed in `198addb`/`df7b013`/`6b701cc` and `getTypeOfAlias` in `964ec88`, all
   ancestors of `33e3bd5`. The briefing's *"dispatch arm missing"* for row 9 and
   *"binder marker carries no value decl"* for row 10 are both wrong, and the
   cheapest possible check — reading the dispatch — says so.
2. **Row 9 is a *propagation* row that the classifier calls TERMINAL, and
   `TERMINAL` is `cause()`'s default arm.** Measured over all 2,618 lines:
   **45.8% is a parameter annotation that is itself a gap**, 17.2% a return
   expression that is itself a gap, 16.4% `async`/generator waiting on
   `Promise`/`Generator`, 11.8% a destructuring parameter. Nothing in it is
   `symbols.rs` work, and almost none of it is terminal.
3. **Row 10 splits 17.8% same-file / 82.2% cross-file, and the two halves are
   different kinds.** 423 lines are kind 1; **1,956 (77%) are kind 2 behind a
   `Checker::new` signature change** — a dependency `rank_board`'s classifier
   cannot see, for exactly the reason finding 2 gives. The board's
   "2,535 TERMINAL" should be read as "423 TERMINAL". `export { q }` is the
   largest same-file form at 198 lines (46.8% of the same-file half) and is what
   this slice built; `import a = b.c` at 158 is the second and is unexamined
   (`bd tsr-93f`).
4. **Row 10 has the best case-gate profile on the whole board** —
   `finishes 117`, higher than any other Ranking A row, median residual 6. The
   arm delivered 9.4 lines per case against the object-literal slice's 61. And
   three-quarters of it is blocked. §4.
5. **The existing instrument could not have produced either split, and the
   reason is structural**, not an oversight. §2 says why, and it was the first
   thing checked.
6. **The prediction MISSED**, and it decomposes: population right, rate wrong.
   §7 scores it.
7. **The split's row-10 total (2,430) does not reconcile with the board's
   (2,535)** at an identical compiler and an identical corpus pin, and row 9
   agrees exactly at 2,618/769 through the same code. §9 records the five
   exclusions and the single number that splits it.

## 1. The dispatch, read before believing either row name

`docs/conventions.md` records three rows in one cycle whose names pointed at the
wrong step, all three living in `get_type_of_symbol`'s dispatch. These two were
assigned as the fourth and fifth. They are not, and the check took ten minutes:

| # | row | its owner in this port | landed | ancestor of `33e3bd5`? |
|---|---|---|---|---|
| 9 | `SymbolFlags(FUNCTION)/FunctionDeclaration/neither` | `get_type_of_func_class_enum_module` (`symbols.rs:504`) | `198addb`, `df7b013`, `6b701cc` | **yes** |
| 10 | `SymbolFlags(ALIAS)/no value declaration` | `get_type_of_alias` (`symbols.rs:316`) | `964ec88` | **yes** |

So the arms were present when the board was measured and both rows are about
what happens *inside* them. `docs/architecture/checker-notes-enums.md`'s closing
section says this in the general case — the `+=` row was classified kind 1,
predicted to convert 507 lines, and `binary.rs:100` already had a working arm.
This is the same finding twice more, and the same one-command check would have
caught it both times.

### What `/ neither` actually means, which decides row 9 on its own

`gap_reason`'s `describe` (`crates/tsr-conformance/src/types_producer.rs:1013`)
appends the annotation-or-initialiser half from `Node::type_id()` and
`Node::initializer_id()`. For a `FunctionDeclaration`, `type_id()` returns
`n.r#type` (`crates/tsr-ast/src/generated/alias.rs:748`) — **the return
annotation**. A function never has an initialiser.

So `/ neither` is not "nothing to go on". It is precisely **"a function with no
return type annotation"**, i.e. a function whose return type must be inferred.
The row is a return-inference-and-parameters row, and its name says so once the
producer is read.

That also explains why `rank_board` calls it TERMINAL. `cause()`
(`crates/tsr-conformance/examples/rank_board.rs:127`) has three ways to reach
`Unknown`/`Propagated`: a reason containing `the receiver is a gap`, a gapped
line inside the node's span, or a reason containing `/ initialiser ` or
`/ annotation `. A declaration name spans only the identifier, and `/ neither`
matches none of the three — so the row falls through to `Terminal` **by
default**. §4 of `checker-notes-rank.md` documents exactly this failure for
`var x = f()` and repaired it by reading the reason string; the repair cannot
fire on a node whose dependencies are its parameters and its body, because those
are neither an initialiser nor an annotation of the node itself.

**This is a correction to the board, not to the probe's arithmetic.** Every
number in `rank_board` is right. The classifier answers "is there evidence of a
dependency?" and the board reads it as "is the prerequisite met?", and for a
function declaration those differ.

The lead verified this in the code and found it is worse than a mislabelled row:
`TERMINAL` is `cause()`'s **default arm**, so `cause()` is total and
`CONTROL UNATTRIBUTED by the TERMINAL/PROPAGATED split = 0` can never read
non-zero from that path — it has proved nothing on any run. Two rules now sit in
`docs/conventions.md` (`014208c`): a control bucket only proves a partition if
some input can reach it, and the semantically loaded label must not be the
default arm. Also recorded there: **the mutation discipline cannot catch this.**
Mutations were written for three of `cause()`'s four arms and each went red; a
mutation to a *default* branch is invisible, because everything that stops
matching the other arms still lands in it.

---

## 2. The cheap cut first: what the existing reason strings already carry

Asked before building anything, because `docs/conventions.md` records three
probes that reached for new machinery and measured a different compiler. One of
`rank_board`'s rows *is* already splittable with no new counters: `access_reason`
(`types_producer.rs:869`) ends with `checker.type_to_string(receiver_type)`, so a
receiver histogram is recoverable from the uncut strings.

**Neither of these two rows is, and the reason is structural.** Both come from
`describe`, which interpolates exactly three things: the symbol's flags, the kind
of its **value declaration**, and the annotation/initialiser half of that value
declaration.

- Row 10: an alias symbol **never** has a value declaration —
  `SymbolFlags::VALUE` does not include `ALIAS` — so field two is the constant
  string `no value declaration` and field three is empty. `import a = N`,
  `import { x } from "./m"` and `export { q }` produce one identical string. The
  row is one atom by construction.
- Row 9: `SymbolFlags(FUNCTION)` and `FunctionDeclaration` are both constants
  within the row, and `/ neither` is a third. Everything that distinguishes one
  member from another lives in the parameters, the type parameters or the body.

So a new instrument was necessary, and it is
`crates/tsr-conformance/examples/symbol_dispatch_split.rs`. It routes through
`types_producer::assertions_for_case_with_ids` and applies the suite's own skips,
so its denominator **is** the gradient's by construction and not by resemblance.

```
cargo run -p tsr-conformance --example symbol_dispatch_split --release
```

`SYMBOL_SPLIT_CASES=<substring>` filters cases for a smoke test and prints a
banner saying the counts are not a share of the gradient. Every number in this
document was produced with `SYMBOL_SPLIT_CASES=conformance/`, which is **5,907 of
the corpus's cases** — a subset, and labelled as one everywhere it is quoted. The
full-corpus run is a lead task.

### Controls, on the full corpus

| control | reads | what a non-zero means |
|---|---|---|
| `row 9: NO SYMBOL` | 0 | the probe's symbol re-derivation has drifted from `gap_reason`'s declaration-name branch |
| `row 10: NO SYMBOL` | 0 | as above |
| `row 10: UNCLASSIFIED KIND` | **1** (`InterfaceDeclaration`) | an alias declaration kind the split does not name |
| `row 9: UNEXPLAINED` | **38 of 2,618 (1.5%)** | lines no listed property describes |

Two read non-zero, so by this page's own rule **§3's sub-rows are lower bounds
of unknown depth**, not shares of a partition. Both are now actionable rather
than merely reported:

- **The one `InterfaceDeclaration` was a real defect and is fixed.**
  `resolve_alias` read `declarations.first()` where
  `getDeclarationOfAliasSymbol` (`checker.go:16397`) is
  `core.FindLast(symbol.Declarations, ast.IsAliasSymbolDeclaration)`. For a
  merged symbol — `export interface I {}` beside `export { N as I }`, one symbol
  carrying `INTERFACE | ALIAS` — `declarations[0]` is the interface, a node with
  no alias target, and the symbol answered `errorType` for a reason that had
  nothing to do with aliases. **This is exactly the falsifier
  `checker-notes-arrays.md` named for the export-marker arm**, so the control
  found the predicted failure; one line is the *size*, not the reason to act.
  `Checker::declaration_of_alias_symbol` and a test that is red under
  `declarations.first()`.
- **The 38 UNEXPLAINED row-9 lines are now dumped, not bounded.** The probe
  prints each one with its case, file index, offset and subject text, plus a
  banner saying the sub-rows are lower bounds. A control that reads non-zero has
  to be explicable on the *next* run without building a second instrument; a
  count alone is not.

### A mechanism tested and eliminated: the probe does not perturb what it measures

`function_properties` calls `check_expression` and `get_type_from_type_node`,
and the first draft made those calls on the **same** `Checker` that `gap_reason`
interrogates. Since `get_type_of_symbol` memoises and the resolution stack is
order-sensitive, that could in principle have moved lines between rows — a
candidate explanation for §9's 105-line disagreement.

It was tested rather than argued. The classification now runs on a **second
`Checker` over the same program**, and `SYMBOL_SPLIT_SHARE_CHECKER=1` restores
the shared one. On the `conformance/` subset the two runs are **identical in
every bucket**, so the perturbation is a measured no-op and §9's gap is not
this.

The separate checker stays anyway. "Measured no-op today" is not "no-op": the
guarantee is cheap, and the knob is what makes the claim re-testable rather than
a note saying it was checked once.

### Attributed and independent, side by side

`docs/conventions.md`, "Size a positional arm from the flattened run": a
first-match-wins column answers a question about the attribution order. A
function can be `async` *and* have a destructuring parameter, so the probe prints
both columns, and the attribution order is by causal strength with the two purely
contextual properties last.

---

## 3. Row 9, measured on the full corpus (2,618 lines, 769 cases)

Concentration on the sub-population, re-run because the parent's shape is not the
child's: **top-1 1.5%** (`conformance/asyncWithVarShadowing_es6`), **top-10
9.2%** — which is the board's own figure for the row, so the row and this
sub-population have the same shape. Genuinely distributed.

**These sub-rows are LOWER BOUNDS.** 38 lines (1.5%) are UNEXPLAINED, so each
figure is what the probe can name and not what the property accounts for.

| property | attributed | independent | share (attrib.) |
|---|---:|---:|---:|
| a parameter annotation that is itself a gap | **1,198** | 1,240 | **45.8%** |
| a return expression that is itself a gap | 451 | 595 | 17.2% |
| a destructuring parameter | 309 | 314 | 11.8% |
| `async` — the return type is `Promise<T>`, a global | 269 | 269 | 10.3% |
| a generator — the return type is `Generator<…>`, a global | 160 | 201 | 6.1% |
| a type-parameter constraint or default that is itself a gap | 91 | 119 | 3.5% |
| two or more distinct return types (needs subtype reduction) | 69 | 84 | 2.6% |
| expando properties (`f.a = 1`) | 23 | 23 | 0.9% |
| a type parameter carrying a modifier (`const`/`in`/`out`) | 9 | 9 | 0.3% |
| an anonymous function (`export default`) | 1 | 1 | 0.0% |
| **UNEXPLAINED** | 38 | — | 1.5% |
| *CONTEXT: more than one declaration* | 0 | 824 | — |
| *CONTEXT: no return expression anywhere* | 0 | 1,860 | — |

**1,740 of 2,618 (66.5%) name a dependency that is itself a gap** — a parameter
annotation, a constraint, or a return expression. Another 429 (16.4%) wait on a
global that needs the lib and `bd tsr-9or.1`. That leaves 410 lines (15.7%) of
genuinely local work, across destructuring parameter names, subtype reduction,
expando members and type-parameter modifiers — **and not one of them is in
`symbols.rs`.** They are `signatures.rs`, `unions.rs` and the expando gap
`bd tsr-4sc.8` already owns.

### What this does to the board

Row 9 is listed at 2,618 lines, cause **TERMINAL / kind 1**, on the strength of
which the size is the worth. The terminal fraction is at most 15.7%, and the
module named beside it (`tsr-checker/src/symbols.rs`) owns none of it. **Do not
rank row 9 at 2,618. Rank it at roughly 410, and against `signatures.rs`.**

The unit evidence behind each bucket, from a per-shape probe (no libs loaded, so
lib-dependent shapes are marked):

| fixture | answer |
|---|---|
| `function f() {}` | `() => void` |
| `function f(x: number) {}` | `(x: number) => void` |
| `function f() { return 1; }` | `() => number` |
| `function f(a: number); function f(a: string); function f(a) {}` | `{ (a: number): any; (a: string): any; }` |
| `declare function f();` | `() => any` |
| `function f({a, b}) {}` | **error** — destructuring parameter |
| `async function f() {}` | **error** — `Promise<T>` |
| `function* f() {}` | **error** — `Generator<…>` |
| `function f(x: number) { if (x) { return 1; } return 2; }` | **error** — two distinct return types |
| `function f() { return () => 1; }` | **error** — the return expression gaps |
| `function f<const T>(x: T) {}` | **error** — type-parameter modifier |
| `function f() {} f.a = 1;` | **error** — expando, a deliberate gap |

### Two wrong answers found on the way, in someone else's file

Not this slice's to fix, and recorded rather than dropped because a wrong answer
is invisible in the aggregate (`docs/adr/0038`, `0039`):

- `function f(b: boolean) { return b ? 1 : 2; }` answers `(b: boolean) => 1 | 2`.
  Upstream widens an inferred return type, so this should be
  `(b: boolean) => number`.
- `function f() { return null; }` answers `() => null` and
  `function f() { return undefined; }` answers `() => undefined`. Upstream widens
  `null` and `undefined` in an inferred return position.

Both are `inferred_return_type` in `crates/tsr-checker/src/signatures.rs`. The
check that settles them is one `.types` baseline each; they are reported, not
asserted, because this slice did not verify them against the corpus.

---

## 4. Row 10, measured on the full corpus, and the split the briefing asked for

**2,430 lines** — see §9, which does not reconcile with the board's 2,535.

| form | lines | reach |
|---|---:|---|
| `import { x } from "./m"` | 890 | cross-file |
| `import a = require("./m")` | 433 | cross-file |
| `import * as ns from "./m"` | 259 | cross-file |
| `import d from "./m"` | 232 | cross-file |
| **`export { q }`** | **198** | **same file** |
| **`import a = b.c`** | **158** | **same file**, deliberately gapped today |
| `export { q } from "./m"` | 134 | cross-file |
| `import a = b` | 67 | same file, ported — these are targets that themselves gap |
| `export as namespace N` | 50 | neither half |
| `export * as ns from "./m"` | 8 | cross-file |
| `UNCLASSIFIED KIND: InterfaceDeclaration` | 1 | a merged symbol; fixed, see §2 |

- **SAME FILE: 423 lines over 193 cases**, top-1 6.6%
  (`compiler/privacyLocalInternalReferenceImportWithExport`), top-10 34.8%.
- **CROSS FILE: 1,956 lines over 875 cases**, top-1 4.3%, top-10 15.5%.

**17.8% same-file, 82.2% cross-file.** Concentration re-run on the
sub-population, per the requirement: the same-file half is twice as concentrated
as the row it came from (top-10 34.8% against the row's 16.5%) — the same
relationship `checker-notes-rank.md` found between the 557-node row and its
parent. At 193 cases it is not one file, so it survives; but it is a *narrower*
item than the parent's shape suggested, and anyone quoting the parent's top-10
for it is quoting the wrong number.

**`export { q }` is 46.8% of the same-file half, not 86%** — see §8. It is still
the largest same-file form, and `import a = b.c` at 158 is the second and is
unexamined (`bd tsr-93f`).

### Row 10 is in Ranking A on the same grounds row 9 was, and 77% of it is kind 2

§1 established that `TERMINAL` is `cause()`'s **default arm** and that row 9
landed there by construction — a declaration name is a leaf, so `gapped_below`
can never fire, and the reason names no initialiser and no annotation. **Every
one of those three facts is equally true of row 10**, and the first draft of this
page stopped one step short of applying its own finding to its own row. It argued
from that structure only that *"every row-10 line is `Terminal`, so the board's
2,535 is the whole row"* — which is a sound statement about `rank_board`'s
**arithmetic** and says nothing whatever about the **kind**.

The kind is not one thing:

| half | lines | kind | blocked on |
|---|---:|---|---|
| same-file | **423** | **1 — genuinely terminal** | nothing; `export { q }` was 198 of it and is done at 47.5% |
| cross-file | **1,956 (77%)** | **2** | module resolution: `Checker::new` takes `(binder, nodes, node_map)` and `BindResult` exposes no specifier-to-file map |
| neither (`export as namespace N`) | 50 | — | its target is the file's own module symbol; needs no module graph and is not a local form either |

`Checker::new`'s signature change is a dependency **the classifier cannot see,
for precisely the reason §1 identifies**: it is not a gapped line inside the
node's span, and it is not named in the reason string. So the honest statement of
the row is **423 kind-1 lines and 1,956 kind-2 lines**, not "2,535 TERMINAL" —
and the second reading is the one that would send the next agent at a workstream
believing it is a slice.

**This is the same trap one row over, and finding it in someone else's row did
not stop me making it in mine.** That is worth more than the correction: a
structural finding is only as useful as the number of places its author then
applies it, and the natural stopping point is the row that was assigned to
someone else.

### The case-gate profile, which is the best on the board

`rank_board` at `c60b086^` reports **`finishes 117`** for this row — the number
of cases that would have *nothing left* if the row closed. That is the **highest
of any row in Ranking A**, above `reference, the name does not resolve` (114),
`SymbolFlags(FUNCTION)/FunctionDeclaration` (67), `CallExpression` (41),
`ArrowFunction` (18) and `member name, the receiver has no such property` (**0**).
Its median residual is 6 other failing lines, also among the lowest.

The `export { q }` arm delivered **+10 cases from 94 lines — 9.4 lines per case**,
inside a row whose profile predicted exactly that. Given
`checker-notes-rank.md`'s finding that the case gate is ~2.9× cheaper than the
gradient, **the rest of this row may be the best-shaped remaining work anywhere
on `checker_types`** — and 1,956 lines of it is the cross-file half, which is
kind 2.

Those two sentences are in tension and both are true, which is the point:
**the row with the best case-gate profile on the board is three-quarters blocked
behind a `Checker::new` signature change.** That makes the signature change an
item worth ranking on its own, rather than a caveat attached to a row nobody can
take.

### The adjacent board row

The board also carries *"a symbol with no value declaration at all (alias, export
marker)"* at 5,743 lines / 4.11% — `rank_board`'s own family, added because
`types_shapes` hid it inside `other`. Row 10 (2,535) is one member of it; the
export-marker rows are the rest, and those were closed in `af7f12a`. **One arm
does not serve both**: an export marker has a declaration and reaches its export
symbol through the file's `exports` table (`export_symbol_of`), while an export
specifier has an alias declaration and reaches its target through an ordinary
scope lookup. They meet only at `get_type_of_symbol`, which is where they should.

---

## 5. What was built: `export { q }`

`getTargetOfExportSpecifier` (`checker.go:14951`), reached from
`getTargetOfAliasDeclaration`'s `KindExportSpecifier` case (`checker.go:15751`),
which is also where the meaning comes from —
`SymbolFlagsValue | SymbolFlagsType | SymbolFlagsNamespace`.

**The branch is on the export declaration, not on the specifier.** Upstream's
three cases in order: a module specifier on the grandparent `ExportDeclaration`
→ `getExternalModuleMember`; a string-literal name → `nil`; otherwise
`resolveEntityName` in the ordinary scope. `export { q }` and
`export { q } from "./m"` share a node kind and nothing else.

That is the whole finding. A row named `SymbolFlags(ALIAS) / no value
declaration` reads as module work, and a fifth of it was a local name lookup.

### The alternative, taken seriously

**Dispatch on the alias's declaration kind inside `get_type_of_alias` and answer
the target's type directly**, without going through `resolve_alias`. Rejected,
and the reason is recorded in `checker-notes-arrays.md`: the export-marker arm's
*first* attempt did exactly that and half-worked, because `export function f`
took a path that reads `declarations` while `export var x` took one that reads
`value_declaration`. Routing through `resolve_alias` and letting
`get_type_of_symbol` answer means every exported form is served by the arm that
already knows how.

**What would make the rejected option win:** if `resolve_alias` had to answer a
form whose target symbol does not exist — an inline expression rather than a
name. `export default 1 + 1` is that shape. It contributes no lines to this row,
so it does not.

### The second half, which the first draft got wrong

`export { a }` naming `import a = N` answered `errorType` after the arm landed,
and the arm looked correct. `getTypeOfAlias`'s value test is
`c.getSymbolFlags(targetSymbol)&ast.SymbolFlagsValue` (`checker.go:18612`) — over
`getSymbolFlagsEx` (`checker.go:16367`), which **walks the alias chain** with a
`seenSymbols` set (`checker.go:16368`). `SymbolFlags::ALIAS` is disjoint from
`SymbolFlags::VALUE`, so a raw flags test rejects a namespace that plainly has
the type `typeof N`.

`Checker::get_symbol_flags` is now ported, with upstream's own visited set rather
than a defensive cap — this port's `resolutions` stack keys on `PropertyName`,
which has no `AliasTarget` variant, and `resolution.rs` is not this slice's file.
One divergence, in the safe direction: where upstream returns `SymbolFlagsAll`
for `unknownSymbol`, `resolve_alias` answers `None`, the walk stops, and the
value test fails — a gap rather than a claim about a symbol that was not found.

### Consequences accepted, including the bad one

`export { I }` for an interface, `export { T }` for a type alias and
`export { N }` for a type-only namespace **stay gapped**. Upstream prints them as
`any` — `conformance/exportsAndImports1.types` records `>I : any`, `>N : any`,
`>T : any` — because upstream prints `errorType` as `any` and this port prints it
as `error` so that a gap stays separable from a computed answer (`docs/adr/0038`).
Those lines are inside the row and this arm will not convert them. That caps what
the arm is worth and it is stated here rather than discovered in the scoring.

### How this would be shown wrong

An export specifier whose local lookup finds a **different** symbol from the one
`resolveEntityName` would. Two places to look: a name declared in an enclosing
namespace and shadowed at file scope, and a merged declaration
(`interface I {}` beside `const I = 1`) where `Binder::resolve_name`'s
locals table holds one half. Both would show up as a *wrong* type rather than a
gap, so the falsifier is the wrong-line column of the next corpus run for these
cases, not the gap column.

### The tests, and the mutation that does not bite

`crates/tsr-checker/tests/export_specifiers.rs`. Three mutations were applied one
at a time, each confirmed present with `grep -c` returning exactly 1 before the
test ran, and each reddens **exactly one** test, disjointly:

| mutation | reddens |
|---|---|
| drop the `module_specifier.is_some()` guard | `an_export_specifier_that_names_a_module_stays_a_gap` |
| `specifier.name` instead of `property_name.or(name)` | `export_q_as_r_looks_up_the_property_name` |
| raw flags instead of `get_symbol_flags` in the `VALUE` test | `an_export_specifier_naming_an_alias_follows_the_chain` |

A fourth was tried and **does not bite**, which is recorded because a fixture
that runs the right code and cannot discriminate is the most common test defect
in this project (`checker-notes-enums.md`, six instances in one workstream):
deleting `get_type_of_alias`'s `SymbolFlags::VALUE` test leaves all six tests
green, because `get_type_of_symbol` has no arm for `INTERFACE` or `TYPE_ALIAS`
either and both routes reach `errorType`. The guard stays because it is
upstream's, with the same standing as `Binder::resolve_name`'s
locals-before-members order (`crates/tsr-binder/src/lib.rs:340`, *"stated rather
than pinned by a test that could not bite"*). It becomes observable the moment
`get_type_of_symbol` grows an arm for a type-only shape, and whoever adds one
should re-run this test with the guard removed and expect red.

No fixture in the file names `Array`, `Promise` or `number[]`: the unit harness
builds one file with no lib, so any of those would measure the missing library.
The probe in §3 shows what that looks like — `function f(x: number[]) {}` answers
`error` in the unit harness and would not in the corpus.

---

## 6. What this slice did **not** do, and why

- **Row 9.** Nothing was built. 66.5% of it is propagation, 16.4% waits on lib
  globals, and the remaining 15.7% is in `signatures.rs`, `unions.rs` and the
  expando gap `bd tsr-4sc.8` already owns — none of it in this file. Building anything in `symbols.rs` for it would have
  converted zero lines — the `+=` outcome, avoided by the check that was skipped
  there.
- **`export default q` / `export = q`.** `getTargetOfExportAssignment`
  (`checker.go:14976`) is a small function and the form is same-file. It
  contributes **zero lines** to this row on the full corpus, because the
  baseline records the *expression* `q` (an expression position) rather than the
  `default` alias's declaration name. Left unbuilt on the measurement rather than
  on an argument.
- **`import a = b.c`.** **158 lines** — the second-largest same-file form, and
  the first draft of this page dismissed it at the subset's 18. It is a
  deliberate print-fidelity gap documented on `Checker::resolve_alias`: the
  qualified form prints the *alias's own* name (`compiler/aliasBug.types`:
  `>booz : typeof booz`) because upstream's node builder emits the shortest
  accessible chain, and this port has no symbol-accessibility machinery, so it
  would print `typeof baz`. **Nobody has checked how many of the 158 would in
  fact print the alias's own name** — the rule is about chain length, and a
  two-link `a.b` may not behave like the three-link `foo.bar.baz` the
  documentation is built on. `bd tsr-93f`.

---

## 7. Prediction — SCORED, and it is a MISS that decomposes

Recorded before the measurement; scored by the lead in isolated detached
worktrees with their own submodule checkouts, `c60b086^..c60b086` verified as one
commit.

```
  c60b086^   291,799 / 478,954    2,128 / 9,538
  c60b086    291,893 / 478,954    2,138 / 9,538
  delta          +94 lines            +10 cases
```

| leg | predicted | measured | verdict |
|---|---|---|---|
| lines | +110 to +225, point +165 | **+94** | **MISS**, 15% under the floor |
| population (`export { q }`) | 200–300 | **198** | hit, 1% under the floor |
| conversion rate | 55–75% | **94/198 = 47.5%** | **MISS**, 7.5 points under the floor |

**The population model held and the rate model did not**, which is the whole
reason `docs/architecture/checker-notes-arrays.md` requires the two to be stated
separately. A single wrong number would have said only "wrong"; two legs say
*which* model to repair. `bd tsr-5xi` — split row 10 by the target's *meaning* —
is now pointed at a measured 47.5% instead of a guess, and it is the follow-up
that would have prevented the miss.

**The named risk was the right one.** §7 of the first draft said: *"the type-only
share is the single number that decides the prediction, and it was not
measured"*, and 47.5% is what an unmeasured type-only share looks like when it
runs against you. Naming the risk did not make the prediction right; it made the
miss diagnosable in one step.

### The number that is better than the one that was predicted

**94 lines flipped 10 cases — 9.4 lines per case.** For comparison, the
object-literal-member slice converted 6,605 lines and flipped 109 cases: **61
lines per case**, which `docs/conventions.md` records as the signature of a
broad, shallow fix. This is **6.5× more case-efficient** and is close to the best
ratio measured in this project.

That was not predicted and is not to this slice's credit as a forecast — it is
recorded because `checker-notes-rank.md`'s two rankings disagree on purpose, and
the case gate is the one nothing on the board was being ranked against. **A
prediction that names only lines cannot be scored on cases**, and a slice whose
best result is one it did not forecast should say so rather than claim it.

### What must not have moved, checked

- **`export { q } from "./m"` contributing zero** — held by the module-specifier
  guard, and one test is red without it.
- **The `any`-credited count** — nothing in this arm answers `any`.
- **Wrong lines for these cases** — the +94/+10 has no wrong-line component
  reported. If a later run shows wrong lines rising in the affected cases, the
  local lookup is finding a symbol `resolveEntityName` would not, and §5's
  falsifier fires.

---

## 8. Corrections to this page, dated

`docs/conventions.md`: *"Correct the record when a number turns out to be wrong,
and note that it was corrected. Silent edits destroy trust in every other
number."*

**2026-08-05 — the first draft quoted the `conformance/` subset as though it
generalised, and for row 10 it does not.**

| claim (first draft) | full corpus | error |
|---|---|---|
| same-file share of row 10: **23.1%** | **17.8%** | 1.3× high |
| `export { q }` is **86% of the same-file half** | **46.8%** | **1.8× high** |
| same-file half: 179 lines / 61 cases | 423 / 193 | subset |
| row 9: parameter annotation gaps **56.4%** | **45.8%** | 1.2× high |
| row 9: return expression gaps 9.9% | 17.2% | 1.7× *low* |
| row 9: expando 1 line | 23 lines | 23× low |

The direction is not uniform, which is the point: the subset was not a scaled
copy of the corpus, it was a *differently shaped* one. `conformance/` is
ES-module-heavy, which inflates the export-specifier form relative to
`import a = b.c`; the latter is 158 lines corpus-wide and was **18** in the
subset.

**The build decision survives the correction and the analysis does not.**
`export { q }` was and is the largest same-file form, so the arm was the right
thing to build; but *"86% of the same-file half"* was used on this page as the
reason not to look at anything else in that half, and at 46.8% that reasoning is
wrong. `import a = b.c` at 158 lines is now visible as a peer, not a footnote
(`bd tsr-93f`).

**The rule this cost — the third sampling error in this project, and the first
stated as a rule:** a filtered run's *shares* do not transfer even when its
*totals* look proportionate. The probe printed a banner saying the counts were
not a share of the gradient, and the banner was obeyed for the totals and
ignored for every ratio computed from them.

---

## 9. STILL UNRESOLVED: the split reads 2,430 for row 10, the board reads 2,535

`bd tsr-4r4`. A 105-line disagreement, 4.1%. `docs/conventions.md` treats an
unexplained gap between two instruments as a finding, not a rounding difference,
and this is the second such disagreement this cycle — the members agent hit a
3-line version.

### The localising run happened, and it narrowed the question

`rank_board` at `c60b086^`, isolated worktree, own submodule checkout:

```
declaration name … SymbolFlags(FUNCT…   2618  1.88%   769   1.5%   9.2%   finishes 67   median 12
declaration name … SymbolFlags(ALIAS…   2535  1.82%  1027   3.4%  16.5%   finishes 117  median  6
reference        … SymbolFlags(ALIAS) / no …
                                        1621  1.16%   595   4.9%  23.6%   finishes 0    median  8
```

Two things follow immediately:

1. **Row 9 agrees exactly — 2,618 lines and 769 cases, both instruments.** The
   selection code is *the same code* for both rows, so a selection difference
   that loses 105 row-10 lines while losing zero row-9 lines is not a plain
   filter difference. That is the strongest constraint available and it came for
   free.
2. **There are two ALIAS rows, differing only in position**: this one
   (declaration name, 2,535) and `reference … SymbolFlags(ALIAS) / no value
   declaration` (1,621, `finishes 0`). The population of the alias problem is
   **4,156 lines**, not 2,535. This probe measures only the declaration-name
   row.

### What is now excluded, with the evidence

- **Not compiler drift.** `git log --oneline 33e3bd5..c60b086^ -- crates/tsr-checker
  crates/tsr-binder crates/tsr-parser` is **empty**, and `git ls-tree` gives
  `5b1047d10` for `vendor/typescript-go` at both commits.
- **Not a keying difference — and this is no longer an argument.** The first
  draft reasoned that `row_key` must be the identity on this row because it cuts
  only at `"has no such property: "` and `"unresolved: "`. That reasoning was
  correct and it was still *an argument*, which is what the `+=` row was. The
  probe now **replicates `row_key` verbatim** (`rank_board_row_key`) and prints
  the two rows keyed both ways in one process over one set of lines. On the
  `conformance/` subset the two columns are identical (746 and 746), so `row_key`
  is measured to be the identity here rather than argued to be.
- **Not a cause split.** Row 10's node is a leaf, so no line's span lies inside
  it and `cause()` cannot return `PropagatedSpan`; the reason names no
  dependency, so not `PropagatedNamed`; it contains neither `/ initialiser ` nor
  `/ annotation `, so not `Unknown`. **This is an argument about arithmetic only
  — see §4 for why the same structure says nothing about the row's kind.**
- **Not this probe perturbing the checker.** Measured, not argued — §2.
- **Not `rank_board`'s two skip paths**, both of which would make it count
  *fewer* lines than this probe, not more.

### The number that splits it, and it is now instrumented

The board says **1,027 cases**. This probe now prints its own row-10 case count
in the reconciliation block:

- **If it reads 1,027**, the 105 lines are inside cases this probe reached, and
  the difference is per-line. Next step: a per-case dump from each instrument and
  a diff — the differing cases name the cause in one step.
- **If it reads fewer**, the 105 are in cases this probe never reached, which
  given the exact row-9 agreement would mean a case-level difference that is
  somehow row-specific — a stranger result, and a more interesting one.

The subset run reports 312 cases for 746 lines, which says nothing on its own;
the full-corpus number is one run away.

**Worth stating plainly, because it may not be a property of either probe:** two
independent instruments have now disagreed with `rank_board` in one cycle. If the
case count comes back at 1,027, the shared suspect is `rank_board`, not the
probes built afterwards.

## 10. Commands for the lead

```bash
# The split, full corpus. §3 and §4 quote this; the UNEXPLAINED dump and the
# lower-bound banner are new since the run they quote.
cargo run -p tsr-conformance --example symbol_dispatch_split --release

# The control mutation for §2's eliminated mechanism. Must be identical.
SYMBOL_SPLIT_SHARE_CHECKER=1 cargo run -p tsr-conformance --example symbol_dispatch_split --release

# §9's reconciliation: the split's own row-10 CASE COUNT is the number that
# splits it. It is printed at the top of the split's output now; the board says
# 1,027. rank_board itself only needs re-running if a per-case diff is wanted.
cargo run -p tsr-conformance --example rank_board --release
```
