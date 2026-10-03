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
take — `bd tsr-mmd`, which also carries the sibling `reference … SymbolFlags(ALIAS)
/ no value declaration` row (1,621 lines, `finishes 0`, same blocker). The two
alias rows together are **4,156 lines**.

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

---

# Cycle 12 — the cross-file half of row 10

Everything above is the same-file half, landed in `c60b086`. This section is the
cross-file arm. Every upstream line number here came from `grep -n` on the
**declaration** at the pinned submodule `5b1047d10`, not from counting inside a
`sed` window — three anchors were wrong in three different ways in one session
before that rule was applied, and one of them (`checker.go:550-566`, described
as "the `Program` interface") *resolved* while naming a fragment of a method
list, so `cargo run -p xtask -- anchors` would have stayed at 0 unresolved.

## 11. The seam is far smaller than "the checker needs a program"

§4 recorded 1,956 cross-file lines as *"kind 2, blocked on a `Checker::new`
signature change"* and left it there. Reading the compiler first would have
shown the blocker is one step, not a subsystem:

**[ADR-0034](../adr/0034-a-program-needs-one-identity-space.md) already gave a
program one `NodeTable`, one `NodeMap` and one `BindResult`.**
`Program::bind_source_files` (`crates/tsr-compiler/src/lib.rs`) accumulates
every file into one `SymbolStore` through `tsr_binder::bind_into`, so a
`SymbolId` names one symbol across the program and its declarations index the
same node table the checker is reading. **A checker built over a `Program`
therefore already sees every file's symbols.** Nothing about cross-file *symbol
access* was missing.

What was missing is one map: specifier text to file. That is upstream's
`GetResolvedModule` (`internal/compiler/program.go:521`), a lookup on
`(file.Path(), {Name, Mode})` — and the whole of what
`crate::resolution::ModuleHost` asks for:

```rust
fn resolved_module(&self, importing_file: NodeId, specifier: &str) -> Option<NodeId>;
```

Both ids are `SourceFile` node ids, which is the identity ADR-0034 created. The
checker names no path type, no `ResolvedModule` and no `ResolutionMode`.

Upstream's `Program` interface (`checker.go:547`, held as the field at
`checker.go:581`) has eighteen methods because `resolveExternalModule`
(`checker.go:15149`) reports fourteen distinct diagnostics. **None of them
changes which symbol is returned**, and none is ported, so the trait is one
method. `tsr_compiler::Program` implements it; ADR-0041 records why the
dependency runs that way round.

### The correction this makes to §4

§4's phrase *"blocked on a `Checker::new` signature change"* was right about the
symptom and wrong about the size. The honest form: **blocked on a
specifier-to-file map, which did not exist anywhere.** The loader computed every
one of those answers during its walk and discarded them —
`crates/tsr-compiler/src/loader.rs` mentioned `resolvedModules` only in comments
describing what upstream does. So the item was *build the cache*, not *expose
it*, and that is a bigger job than "a signature change" while being a much
smaller one than "the checker needs a program".

## 12. The form to build first is decided by the *printed* answer, not by the resolution

The obvious order is cheapest-resolution-first: `import a = require("./m")` (433
lines) and `import * as ns from "./m"` (259) reach a module symbol directly and
need no name lookup at all. **Building those first would have converted zero
lines and added 692 wrong ones.**

Measured over `vendor/typescript-go/testdata/baselines/reference/submodule`,
the assertion on the declaration name:

| form | what the baseline records |
|---|---|
| `import * as X from …` | `typeof cjs` 236, `typeof type` 184, `typeof cjsi` 172, `typeof mjs` 156, `typeof React` 58, `typeof ns` 17 |
| `import X = require(…)` | `typeof React` 71, `typeof mod` 21, `typeof Backbone` 21, `typeof moduleA` 11 |

```
conformance/exportAsNamespace4(module=commonjs).types
  import * as ns from './0';
  >ns : typeof ns
```

It is always the **local alias**, never the module. Upstream's node builder
emits the shortest accessible chain to the symbol; a module symbol's name in
this port is the file path with its extension stripped
(`bind_source_file_as_external_module`), so this port would print `typeof /0`.
A gap turned into a wrong line, in the column ADR-0038 and ADR-0039 exist to
keep separable.

**This is not a new discovery, and that is what makes it credible.** It is the
mechanism already documented on `Checker::resolve_alias` for the qualified
`import a = foo.bar.baz` — `compiler/aliasBug.types` records `>booz : typeof
booz` for the three-link chain and `>provide : typeof foo` for the one-link one.
One known limitation reaching two more forms. `bd tsr-4jk` carries it, and the
three should be ranked together behind symbol accessibility rather than
separately.

A named import has no such problem: its target is an ordinary export symbol
carrying its own name — `number`, `typeof A`, `() => void`, `0`.

### The general rule, which is not about aliases

`docs/conventions.md` already says *"count the lines a failure blocks, not the
lines where the form appears"* and *"bucket by the shape of the answer"*. This
adds a third question that comes **before** either, and it is the one that
decides whether a slice is worth building at all:

> **Can this port spell the answer?** Resolution and rendering are separate
> capabilities, and a form can be fully resolvable and still unprintable. Check
> the baseline's *right-hand side* for the form before ranking it, because a row
> that answers with a name only symbol accessibility can produce converts
> nothing and costs wrong lines.

Two minutes of `grep` over `.types` files answers it. It reversed the build
order here.

## 13. What was built: `getExternalModuleMember`, serving both forms

`getExternalModuleMember` (`checker.go:14667`) is reached from
`getTargetOfImportSpecifier` (`checker.go:14647`) and from
`getTargetOfExportSpecifier`'s module-specifier case (`checker.go:14966`). **One
function, so `import { x } from "./m"` and `export { q } from "./m"` are one arm
and not two** — 890 + 134 = **1,024 lines**.

The chain, and what each step reduces to here:

| upstream | ported as | reduction |
|---|---|---|
| `resolveExternalModuleName` `:15101` → `…Worker` `:15122` → `resolveExternalModule` `:15149` | `resolve_external_module_name` | 190 lines to 4 steps; the rest is diagnostics |
| `resolveESModuleSymbol` `:15568` | — | reduces to the next row: its synthetic-default arms are guarded by `namespaceImport != nil \|\| IsImportCall`, and a *named* import is neither |
| `resolveExternalModuleSymbol` `:15556` | `resolve_external_module_symbol` | the `export=` lookup |
| `getExportOfModule` `:14789` | `get_export_of_module` | the `MODULE` guard and one table lookup; `resolveSymbolEx` `:14432` is the identity under `dontResolveAlias` |

### Deliberately not ported, each a miss and never a wrong target

- **`export =` modules.** The site-independent primitive value-member slice is
  now ported: the member is read from `getTypeOfSymbol(targetSymbol)` through
  `getPropertyOfTypeEx(..., skipObjectFunctionPropertyAugment = true)`, while
  supplemental exports continue to come from the original module. The skip is
  semantically separate from primitive filtering: inherited `Function.length`
  and `Function.name` themselves have primitive types but are not exports of
  the target. Value members that are aliases or whose types are objects stay
  gaps: their correct rendering needs the importing site's shortest accessible
  symbol chain, owned by the site-aware spelling lane. The full run falsified
  the ungated form with `Foo.Bar` where native prints
  `import("./thing").Bar`. `ENUM`, `ENUM_LITERAL`, and `UNIQUE_ES_SYMBOL` stay
  out too: TypeScript classifies them as primitives, but their identity and
  rendering still carry a declaration symbol whose accessible name depends on
  the use site.

  Two combinations need no allocation and are represented directly. A
  same-name primitive value property plus a distinct type-only supplemental
  export still declines: upstream's `combineValueAndTypeSymbols` creates a
  synthetic symbol, whereas this checker intentionally has immutable access to
  the binder's symbol store. Returning either real symbol would erase one
  meaning and turn a semantic gap into a wrong answer. A value member rejected
  by the spelling gate likewise cannot fall through to a type-only supplement:
  native saw both meanings, so choosing only the supplement would erase the
  value. The name `default` also declines here because native routes it through
  `getTargetOfModuleDefault` before ordinary named-member lookup.
- **`export *` re-exports.** `getExportsOfSymbol` (`checker.go:15920`) resolves
  star exports in `getExportsOfModuleWorker` (`checker.go:16148`); this reads
  the binder's `exports` table directly.
- **Ambient modules.** `tryFindAmbientModule` (`checker.go:15154`) runs *before*
  the host is consulted, so a program whose `"fs"` is `declare module "fs"` gaps
  here.
- **`getMergedSymbol`** on the module symbol, and **string-literal export
  names** (`import { "a-b" as c }`).

### The alternative, taken seriously

**Resolve the module symbol and answer `getTypeOfSymbol` on it directly**,
skipping the name lookup — which is what the `import * as ns` form would need
anyway, so one arm could have served four forms instead of two. Rejected on §12:
three of those four print the alias's own name and would answer wrongly.

**What would make it win:** symbol accessibility, so the printer chooses a name
per reference site instead of fixing it at type creation
(`crate::types::TypeData::Named`). At that point all four forms are one arm and
`bd tsr-4jk`'s 692 lines join `bd tsr-93f`'s 158.

## 14. The cycle guard: three candidates, and reading picked the wrong one twice

A cross-file re-export cycle is real —

```ts
// a.ts
export { q } from "./b";
// b.ts
export { q } from "./a";
```

— and `docs/conventions.md` is explicit that a port which passes the corpus and
hangs on a real program is the worse failure. So the first thing written was
upstream's own guard: `resolveAlias` pushes
`TypeSystemPropertyNameAliasTarget` (`checker.go:16272`), so a
`PropertyName::AliasTarget` variant and a push/pop went into `resolution.rs` and
`symbols.rs`.

**It could not fire, and it was deleted rather than shipped.**

| candidate | measured |
|---|---|
| remove `resolve_alias`'s `AliasTarget` frame | all 12 tests **green** |
| disable `get_type_of_alias`'s `PropertyName::Type` frame | all 12 tests **green** |
| delete `\|\| seen.contains(&target)` from `get_symbol_flags` | the cycle fixture **hangs** |

The real guard is `get_symbol_flags`'s visited set — upstream's own
`seenSymbols` (`checker.go:16368`). `getTypeOfAlias` takes its `VALUE` test over
the alias *chain*, so the loop is closed in the chain walk, and the walk stops
on a repeat.

The `AliasTarget` frame is unreachable **structurally, not incidentally**, which
is what makes deleting it safe rather than lucky: `resolve_alias` is not
self-recursive here. Its four arms reach `Binder::resolve_name`,
`export_specifier_target`, `import_specifier_target` and
`get_external_module_member`, and none of those calls back into `resolve_alias`
or `get_type_of_symbol` — they read symbol tables. Upstream's *does* recurse,
through `resolveIndirectionAlias` (`checker.go:16293`), which this port does not
have. **Whoever ports that must restore the frame**, and the cycle test is what
will hang if they do not.

### Why it was deleted rather than kept as insurance

`docs/conventions.md` records the same failure one level up: a control bucket
that could only ever read zero, and the rule that *a control only proves a
partition if a line can actually reach it*. A guard nobody can make fire reads
as safety and supplies none — and unlike the control bucket, this one carried a
doc comment asserting it was "the variant that makes cross-file aliases
terminate", which was false. Keeping it would have been documenting an intention
as though it were built.

**The generalisation is about method, not about aliases.** The frame was written
from a correct reading of upstream and a correct statement of the hazard, and it
was still the wrong mechanism. Faithfulness to upstream is not evidence that a
guard is load-bearing *here*, because what recurses upstream may not recurse in
a port that has left a function out. The check is one mutation, and it found the
answer that two rounds of reading had missed.

## 15. The tests, and the harness question the briefing raised

`crates/tsr-checker/tests/cross_file_aliases.rs`, 12 tests.

**The single-file unit harness does support two files, and no harness change was
needed** — this was raised as a likely blocker and is not one.
`tsr_parser::parse_into` takes the node table and map by `&mut` so ids continue
rather than restart, and `tsr_binder::bind_into` accumulates into one
`SymbolStore`. Both are already dev-dependencies of `tsr-checker`. The fixture
helper is `Program::parse` + `Program::bind_source_files` reduced to what a test
needs; `tsr-compiler` is *not* a dependency and could not be, since it depends
on `tsr-checker`.

Seven mutations, each confirmed with `grep -c` returning exactly 1 **before** the
test ran. The tests pair as (1,2), (4,5), (6,7) — one behaviour change each —
and **within every pair the two mutations are disjoint**. Across pairs they
overlap, structurally: a mutation that disables the export lookup disables every
arm that performs one. Three of the seven redden exactly one test each. The full
table is in the test file's module docs, including the two that did **not** bite.

Controls pinned by **construction** rather than arithmetic, per
`docs/conventions.md`'s newest section:

- `a_named_import_from_an_unresolved_module_is_a_gap` — the fixture set contains
  no file called `nope`, so the host *cannot* answer `Some` whatever the checker
  does.
- `the_same_fixture_gaps_with_no_host` — the identical program under
  `Checker::new`. This is the additivity claim as a measurement rather than an
  argument.
- `no_lib_control` — `var x: number[]` is a gap *here*, so a reader who meets an
  unexpected gap can tell "missing library" from "missing arm" without
  re-deriving it. No fixture names `Array`, `Promise` or `number[]`.

## 16. PREDICTION — recorded before any measurement

**This arm converts nothing on its own.** `crates/tsr-conformance/src/types_producer.rs`
still builds its checker with `Checker::new`, so no host reaches it. The
prediction is for the tip at which the checker arm, the loader's resolution
cache and that one call site are all in.

### Population — a ceiling, from `examples/symbol_dispatch_split.rs`, full corpus

| form | lines |
|---|---:|
| `import { x } from "./m"` | 890 |
| `export { q } from "./m"` | 134 |
| **total** | **1,024** |

Predicted population **1,024**, and the split between the two forms should be
about **87% / 13%**.

### Rate — two multipliers, stated separately because that is what diagnoses a miss

§7 scored the previous prediction a MISS that decomposed: population held to 1%,
rate missed by 7.5 points. Quoting one range again would forfeit that.

- **Leg 1 — the type-only cap: 0.475.** Measured, not assumed: the sibling
  same-file form `export { q }` converted 94 of 198. It shares the mechanism
  exactly — the same `getTypeOfAlias` `VALUE` test, so `import { I }` for an
  interface answers `errorType` here where upstream prints `any`.
- **Leg 2 — the cross-file-only gaps: 0.80, and this is the leg I expect to be
  wrong.** `export =`, `export *`, ambient modules, `getMergedSymbol` and
  string-literal export names are all declined by this arm and **none of them
  exists in the same-file form**, so leg 1 contains no discount for any of them.
  0.80 is a guess. Nothing measures it, and I am recording it as a guess rather
  than dressing it as a bound.

**Predicted lines: 1,024 × 0.475 × 0.80 ≈ 389; range 300–490.**
**Predicted cases: +25 to +55, point +40.** The row's `finishes 117` is the best
case-gate profile on the board and the same-file arm delivered 9.4 lines per
case, but the cross-file half is half as concentrated (top-10 15.5% against
34.8%), so it should touch more cases and finish proportionally fewer.

### What must NOT move

- **`import * as ns from "./m"` (259), `import a = require("./m")` (433) and
  `import d from "./m"` (232) must contribute exactly zero** — 924 lines. Held
  by `resolve_alias`'s fallthrough. **If any of them moves, §12 is wrong and
  this arm is emitting the wrong-name form**, which is the failure that matters
  most here. No test is red without that fallthrough, because it is a default
  arm — so this is the check that has to be made on the run.
- **Wrong lines must not rise in the affected cases.** A rise means the arm is
  finding a symbol upstream would not; the merged-module-symbol case is where to
  look.
- **The same-file `export { q }` count (198 lines / 94 converted) must be
  unchanged.** Mutation 5 pins it locally; the corpus is what confirms it.
- **The `any`-credited count must not move.** Nothing in this arm answers `any`.

### How this could be right for the wrong reason

1. **The total lands in range but the split is not 87/13.** If most of the gain
   is `export { q } from` (134 lines, so at most 13% of the ceiling), the total
   can only be right by the `import { x }` form under-converting and something
   else over-converting. **Score the two forms separately or the prediction is
   unfalsifiable.**
2. **The gain comes from somewhere else the host unblocked.** Passing a host to
   the conformance checker enables this arm *and* changes nothing else only if
   nothing else consults it. Control: the same corpus run with `Checker::new`
   must be bit-identical to the baseline. Without that control, any movement is
   attributable to "cross-file resolution" in general rather than to this arm.
3. **Leg 1 and leg 2 cancel.** If the cross-file type-only share is *lower* than
   the same-file one (plausible — `export { q }` is idiomatic for re-exporting
   types, `import { x }` less so) while leg 2's gaps are commoner than 0.80,
   the product can land while both legs are wrong. The diagnostic is the
   answer histogram of the converted lines: if the `any`-upstream share of the
   *unconverted* remainder is not close to 52.5%, leg 1 is wrong whatever the
   total did.

## 16b. PREDICTION, REVISED — leg 1's prior was measured post-build

Registered **before** any run, and **beside** the prediction above rather than
replacing it, because the two disagree and the same run scores both. What
changed is an *input*, not an outcome; that is what makes revising legitimate
here and reinterpreting a result afterwards not.

### The escape hatch, closed

`bd tsr-95i` names the one thing that would kill the correction: a run of
`symbol_dispatch_split.rs` from a worktree holding its source on top of
`c60b086^` with the `symbols.rs` arm reverted, which would make 198 a genuine
*pre*-build population.

**No such run happened.** `git log --diff-filter=A --
crates/tsr-conformance/examples/symbol_dispatch_split.rs` returns `c60b086` —
the instrument was added *by* the commit that built the arm — and §10 above
records the full-corpus run as a lead task performed after it. This slice ran no
corpus measurement of any kind this cycle. So the correction stands, and
**§7's 47.5% and §4's `198` are post-build figures throughout this page**.

### Two independent methods now bracket the answer, and they do not agree

| method | leg 1 | scope discount | lines |
|---|---:|---:|---:|
| **A — the sibling, corrected.** `export { q }` pre-build ≈311, of which 94 became right | 0.30 | 0.80 | **246** |
| **B — the direct measurement.** `module_blocked` mocked past the blocker: 39.8% and 44.7% on the two cross-file halves | 0.42 | 0.85 | **366** |

**B is the more direct bucket and A is the proxy**, which by
`docs/conventions.md`'s own rule — *pre-register on the most direct bucket your
instrument produces* — argues for B. But B's mock resolves forms this arm
**declines** (`export =`, ~~`export *`~~, ambient modules, string-literal export
names — **`export *` no longer declines**, see
`Checker::get_export_from_star`), so it is an upper bound on what this arm can
do; the 0.85 is the
correction for that and it is a guess, labelled as one, exactly as leg 2 was.

**Revised: 300 lines, range 246–366** — the bracket the two methods span, point
between them. I am not confident which method is right, and saying so is the
point: **the run adjudicates between two sizing methods as well as scoring the
arm**, which is worth more than either alone. If the answer lands near 246, the
corrected-sibling method wins and every future arm in this family should be
sized that way; near 366, the mocked measurement wins.

**The original prediction (389, floor 300) sits above both brackets and is
therefore likely a miss. Recorded here before the run rather than explained
after it.**

### Cases, revised down hard

**+5 to +20, point +10**, from +25 to +55 / point +40.

The level-4 probe measured the sibling alias row finishing **0.0% of its cases
even at the ceiling**, its pre-registered rule holding exactly. And walking every
gap line to its blocking leaves found **6,074 lines wholly behind cross-file
aliases**, of which **3,539 — 58.3% — wait on a *second* item**: a type for a
module object, `bd tsr-6ph`, which is the only route to the receiver-gap lines.

**This arm is the first of at least two, and the second is not this slice's.**
That is the honest frame, and it demotes §4's *"best case-gate profile on the
board"* for this half specifically: a row can have a high `finishes` at its
ceiling and finish almost nothing at the first of two steps toward it. The
case-efficiency of the same-file arm (9.4 lines per case) does not transfer,
because the same-file arm *was* the whole chain and this one is not.

**This is a gradient item, not a case item.**

## 17. Open, and named rather than left implicit

- **The wire-up is not this slice's.** `types_producer.rs:768` must become
  `Checker::with_module_host(…, Some(program))` before anything is measurable.
- **§9's 105-line disagreement is RESOLVED, and it was never an instrument
  defect** (`bd tsr-4r4`). It is a **pin difference**: `rank_board`'s 2,535 was
  measured at `c60b086^`, and `symbol_dispatch_split.rs` was added *by*
  `c60b086`, across which the row fell 113 lines. Residual disagreement is
  **7 lines**, in `import * as ns` and `export as namespace N`.

  §9 spent two cycles excluding compiler drift, keying, cause splits and probe
  perturbation — every one of those exclusions was sound, and the question was
  wrong. **Two instruments were compared as though they stood at one commit when
  the second could not exist at the first.** That is the same class as the
  `writer_guards` denominator problem: a comparison whose two sides are not over
  the same population, where every check *inside* each side passes. The habit it
  argues for is cheap — before reconciling two instruments, run
  `git log --diff-filter=A` on both and confirm they can stand at the same
  commit.
- **Nobody has split the 433 `import a = require` lines by whether the target
  module writes `export =`.** That sub-case *is* spellable — the target is an
  ordinary named symbol — so it is buildable today while the other 692 are not.
  One bucket in `symbol_dispatch_split.rs` sizes it. `bd tsr-4jk`.

---

## 18. SCORED — both predictions miss on lines, and the decomposition says why

Measured by the lead in isolated detached worktrees with their own submodule
checkouts. `fa29e66^..fa29e66` verified as one commit.

```
  baseline (8d20c28)   291,895 / 478,954   2,138 cases   60.94%
  fa29e66              292,606 / 478,954   2,173 cases   61.09%
                          +711 lines          +35 cases
```

Rails flat: `module_resolution` 95/95, `file_loader` 96/96, `binder_symbols`
8,292/8,459, `printer_round_trip` 11,681/11,737, `parser_typescript` 5,000/5,031.

| leg | §16 original | §16b revised | measured | verdict |
|---|---|---|---:|---|
| lines | 389 (300–490) | 300 (246–366) | **+711** | **both MISS**; original 1.83× under, revised 2.37× |
| cases | +40 (+25 to +55) | +10 (+5 to +20) | **+35** | **original HITS**, revised MISSES |
| population | 1,024 | 1,024 | rows fell 461 | held as a ceiling on rows |
| form split | 87 / 13 | 87 / 13 | **86.2 / 13.8** | **HIT** |
| the 924-line zero | unchanged | unchanged | **unchanged, exactly** | **HIT** |

```
  import { x } from   890 -> 521   -369   86.2%
  export { q } from   134 ->  75    -59   13.8%
  import * as ns      259 -> 259      0
  import a = require  433 -> 433      0
  import d from       232 -> 232      0
```

### The miss is one factor, and it is a factor nobody had

**389 is an excellent prediction of the wrong quantity.** The two target rows
lost **428** lines; 389 is **91%** of that — a 9% error on rows, the tightest
number this slice has produced. The prediction was built from row rates, and it
was reported as a *gradient* number with an implicit cascade multiplier of
**1.0**.

The measured multiplier is **1.66**. `389 × 1.66 = 646`, against 711.

So the miss does not decompose into "population right, rate wrong" as §7's did.
Population held, rate held, **and a third factor existed that neither
prediction had a slot for**. §19 is that factor.

### `bd tsr-95i` is reopened against its own evidence, and the correction moved leg 1 the wrong way

This is not "the revised one was close in spirit". It is quantitative:

```
  row conversion, the two predicted forms   428 / 1,024 = 41.8%
  implied leg 1, holding leg 2 at 0.80              0.522
```

- original leg 1: **0.475** — 9% below the implied truth
- corrected leg 1: **0.30** — **43% below it**

**The 47.5% prior was closer to the measured rate than the 30% that replaced
it.** Counting the same-file row's 33 lines as well (§20 shows they are this
arm's) the row rate is 45.0% and implied leg 1 is 0.56, further above 0.475
still.

The correction's reasoning was sound — 198 *is* a post-build population, and I
verified that independently — and its conclusion about the *direction* of the
error was wrong. `bd tsr-95i` should carry that: **a post-build population makes
the denominator too small, which inflates the rate; but the same build also
removed lines from the numerator's row, and the two do not cancel in the
direction assumed.** Nobody worked out which effect dominated before the number
was revised, and the honest reading is that the revision was made under time
pressure on an argument, against a prior that had been *measured*.

The rule this buys, and it is the one I would want applied to me next time:
**do not replace a measured prior with a reasoned correction unless the
correction is itself measured.** 0.475 was measured on 198 real lines. 0.30 was
inferred. A prediction is not improved by making its inputs more sophisticated;
it is improved by making them more measured.

#### The caveat on this verdict, closed rather than flagged

Everything above holds **leg 2 at the guessed 0.80**, and a first draft of this
section said so and then quoted the verdict anyway. `docs/conventions.md`:
*"Flagging a risk and then propagating the number is worse than not flagging
it… If you write one and then quote past it, delete the number, not the
caveat."* So the caveat is worked out instead:

| leg 2 | implied leg 1 | \|0.475 − t\| | \|0.30 − t\| | closer |
|---:|---:|---:|---:|---|
| 0.6 | 0.697 | 0.222 | 0.397 | 0.475 |
| 0.7 | 0.597 | 0.122 | 0.297 | 0.475 |
| 0.8 | 0.522 | 0.047 | 0.222 | 0.475 |
| 0.9 | 0.464 | 0.011 | 0.164 | 0.475 |
| 1.0 | 0.418 | 0.057 | 0.118 | 0.475 |

The two candidates are equidistant only at implied leg 1 = 0.3875, which needs
**leg 2 = 1.079**. Leg 2 is a *discount* for forms the arm declines, so
**leg 2 ≤ 1.0 by construction** and the crossover is unreachable.

**For every physically possible value of the unmeasured input, 0.475 is closer
to the truth than 0.30.** The verdict does not rest on the guess. That is a
control pinned by construction rather than by arithmetic — the class
`docs/conventions.md` prefers — and it is what the caveat should have been the
first time. The leg-2 bucket is still worth measuring, for the *next*
prediction rather than for this verdict.

### Method B beat method A on the quantity both were estimating

§16b framed the run as adjudicating two sizing methods. On **rows** — which is
what both methods actually estimate — it did:

| | predicts rows | measured 428 | error |
|---|---:|---:|---:|
| A, the corrected sibling proxy | 246 | | 1.74× under |
| B, `module_blocked`'s direct measurement | 366 | | 1.17× under |

**The direct measurement won, as `docs/conventions.md`'s "pre-register on the
most direct bucket" predicts.** My §16b split the difference between them and
was therefore worse than B alone. Averaging a measurement with a proxy discards
the reason the measurement is better.

### Cases: I overrode my own row's profile with a different row's

+35 measured, inside §16's +25 to +55 and well outside §16b's +5 to +20.

The revision cut cases 4× on the finding that *"the sibling alias row finishes
0.0% of its cases even at the ceiling"*. **That is the `reference …
SymbolFlags(ALIAS)` row, which §4 of this page records at `finishes 0`. This arm
is in the `declaration name …` row, which §4 records at `finishes 117` — the
highest on the board.** Both numbers were already on this page, four sections
above the revision, and I applied the wrong one.

Same disease as everything else catalogued here: **a number that is true and
answers a different question.** The tell was available for free — the two rows
differ by one word in their names and by 117 in the statistic being reasoned
about.

## 19. The fifth sizing question: conversions cascade past the row

The four levels in `docs/conventions.md` predict lines *within a row* and cases.
None of them predicts this:

```
  lines that left the two predicted rows          428
  lines that left every row that moved            461   (+ same-file, §20)
  gradient gain                                   711
  converted while in NO alias row at all          250
```

**1.66× against the predicted population; 1.54× against every row that moved.**
Both are true and answer different questions — 1.66 is what a *planner* wants
(how much does my row population under-count the gain), 1.54 is what a
*mechanism* reader wants (what share of the gain landed outside any row that
moved).

The 250 lines are references to the imported names and accesses through them.
`import { f } from "./m"; f();` puts one line in the alias row — the declaration
name `f` — and the *call* is a separate assertion line that answered `errorType`
only because its callee did. Nothing in any row histogram attributes that line
to the alias.

So, stated as a question to ask before sizing:

> **How many lines answer wrongly only because this one does?** A row counts the
> lines that *name* the defect. It does not count the lines downstream of them,
> and for anything a *reference* can point at — a symbol, a module, a callee —
> that downstream set is larger than the row.

**Now canonical in `docs/conventions.md` (`431a5ac`)**, with both refinements
below; this section is the measurement behind it, not the rule.

This is the mirror image of a rule already recorded. `docs/conventions.md` warns
that summing rows which share a downstream function **over**-counts, because
83% were turned back earlier. This is the same edge walked the other way, and it
**under**-counts. The two together say the honest form: *walk the dependency
edge in both directions and count once.*

**Why the same-file arm did not show this and this one does:** `export { q }`
converted 94 lines and flipped 10 cases with no cascade visible, because a
re-exported name is mostly not *used* in the file that re-exports it. An
imported name is imported in order to be used. **The cascade multiplier is a
property of the form, not of the fix**, which is why it cannot be carried
forward as a constant — the next arm needs its own.

## 20. The must-not-move condition that moved, explained and tested

```
  export { q }  (SAME FILE)   198 -> 165   -33
```

§16 named this row unchanged. It fell 33, and the explanation is **not** that
the shared function "also helps same-file cases" — that would be a
rationalisation, and it is wrong: `export_specifier_target`'s
`module_specifier.is_none()` branch is **byte-identical** across
`fa29e66^..fa29e66`, verified on the diff. The same-file lookup did not change
at all.

A same-file specifier can therefore only have converted through its **target**,
and there are exactly two routes, enumerated from the diff rather than guessed:

1. the target is an **import specifier** — `import { x } from "./m"; export { x };`
2. the target is a **re-export** — `export { x } from "./m"` in another file

`getTypeOfAlias` takes its `VALUE` test over `getSymbolFlags`
(`checker.go:16367`), which walks the alias *chain*. Both shapes previously
ended the walk at an alias carrying no `VALUE` bit, so the answer was
`errorType` **for a reason that had nothing to do with the export specifier**.

Both are now tests —
`a_same_file_export_specifier_naming_an_import_converts_too` and
`a_same_file_export_specifier_naming_a_re_export_converts_too` — and the first
asserts the same fixture **gaps with no host**, which is what makes it evidence
that this arm did it rather than a story that it could have.

### What the row actually is, which is the finding

**The same-file / cross-file split in §4 is a split by *syntax*, not by *work*.**
33 of the 198 "same-file" lines were blocked cross-file all along, through their
targets. §4 presented 423 same-file lines as *"kind 1 — genuinely terminal,
blocked on nothing"*, and at least 8% of the largest form in it was kind 2.

That is a correction to §4 and it generalises past this row: **classifying a
line by the syntax at its own position says nothing about where its
dependencies live.** The alias chain is exactly the mechanism that carries a
dependency somewhere the classifier cannot see it — which is the same reason
`rank_board` calls this whole family `TERMINAL` by default (§1).

## 21. The zero-control was run, and it reads exactly zero

**CORRECTED 2026-08-06.** The first version of this section, written the day
before, was titled *"The zero-control could not be run, so failure mode (b) is
untested"* and concluded that the attribution of +711 to this arm was *"an
argument rather than a measurement"*. **That is no longer true.** The recipe it
gave was run and the section is rewritten rather than annotated, because the
status it described — not the numbers — is what changed. What it got right is
kept below.

### The result

Worktree at `fa29e66`, `git checkout fa29e66^ -- crates/tsr-checker/src/symbols.rs`
with the trait retained so the tree builds, `grep -c resolved_module
crates/tsr-checker/src/symbols.rs` returning **0**, full corpus:

```
  control (seam present, arm reverted)   291,895 / 478,954   2,138 cases   60.94%
  baseline                               291,895 / 478,954   2,138 cases   60.94%
                                              0 lines            0 cases
  module_resolution 95/95    file_loader 96/96
```

**Exactly zero, to the line and to the case.** So §16's registered failure mode
(b) — *"the gain comes from something else the host unblocked"* — is
**excluded by measurement**, the seam is provably inert without the arm, and
**the whole +711 is attributable to `fa29e66`.**

### Why it was worth running when the argument was already strong

The argument was: nothing else in the tree calls `resolved_module`, and one
`grep` confirms the only callers are `Checker::resolve_external_module_name` and
the trait impl, so the host is inert without the arm. **That argument was
correct** — the control agreed with it exactly.

Which is the case where a measurement teaches least, and it still had to be run.
`docs/conventions.md` records four separate occasions where a correct-sounding
mechanism story accompanied a wrong number, and the standing rule is *trust a
number that arrives with a cross-check; discount one that arrives with a
mechanism story.* An argument that turns out right does not retroactively become
evidence; it was a guess that won. **The only way to know which kind you had is
to run it**, and the cost here was one corpus pass against an attribution the
whole prediction rests on.

The narrower form, worth keeping because it is checkable: **a `grep` shows what
calls a function, not what a run does.** Inertness is a claim about execution.

### What the original section got right, kept

`ce83aed` and `1e4bddb` do not compile (`bd tsr-iv7`): `checker.rs` imports
`ModuleHost` at `ce83aed`, and `trait ModuleHost` first appears in `fa29e66`.
The runs against them failed and **left the committed snapshots in place, which
read exactly the baseline** — so the control very nearly reported "exactly zero"
while having measured nothing at all. The zero above is a real zero; that one
would have been a zero produced by not running.

**A failed run that leaves a stale snapshot is a control that reports success by
failing to execute** — the same family as a control bucket that cannot read
non-zero, and now in `docs/conventions.md` as its own entry. The two zeroes are
indistinguishable in the output and are distinguished only by reading the run's
own stdout, which is what caught it.

The ordering rule that would have prevented the whole thing, `bd tsr-iv7`:
**the first commit must be the one that builds alone, which is the declaration,
not the consumer.** Compressed: *blocking is not the same as first.*

### The state of §16's registered conditions, all now resolved

| condition | verdict |
|---|---|
| the 924-line zero (`import * as ns`, `import a = require`, `import d from`) | **held exactly**, not one line |
| form split 87 / 13 | **held**, measured 86.2 / 13.8 |
| same-file `export { q }` unchanged | **VIOLATED**, 198 → 165; §20 explains it with two tests |
| wrong lines must not rise in affected cases | no wrong-line component reported |
| the `any`-credited count must not move | unchanged |
| (b) the gain comes from something else | **excluded by measurement**, this section |
| (a) the total lands but the split is not 87/13 | excluded, the split is scored |
| (c) legs 1 and 2 cancel | superseded — §18 shows the miss is the cascade factor, not either leg |
