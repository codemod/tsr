# Checker notes: `symbols.rs` — `rank_board` rows 9 and 10

Working notes for the slice that owns `crates/tsr-checker/src/symbols.rs`. Every
upstream line number here was `grep -n`-verified against the pinned submodule at
`5b1047d10`; the anchors gate cannot see a briefing, a `bd` note or a message
(`docs/conventions.md`, "The anchors gate does not see your briefing"), and one
number handed to this slice was off by two.

---

## 0. The findings, in the order they change a decision

1. **Row 9 is not a missing dispatch arm, and neither is row 10.** Both arms
   existed at the commit the board was measured on. `getTypeOfFuncClassEnumModule`
   landed in `198addb`/`df7b013`/`6b701cc` and `getTypeOfAlias` in `964ec88`, all
   ancestors of `33e3bd5`. The briefing's *"dispatch arm missing"* for row 9 and
   *"binder marker carries no value decl"* for row 10 are both wrong, and the
   cheapest possible check — reading the dispatch — says so.
2. **Row 9 is a *propagation* row that the classifier calls TERMINAL, and the
   mechanism is the exact trap `checker-notes-rank.md` §4 documents.** Measured
   on the `conformance/` half of the corpus: **56.4% of it is a parameter
   annotation that is itself a gap**, 17.6% is `async`/generator waiting on
   `Promise`/`Generator`, 11.1% is a destructuring parameter, 9.9% is a return
   expression that is itself a gap. Nothing in it is `symbols.rs` work, and
   almost none of it is terminal.
3. **Row 10 splits 23% same-file / 77% cross-file**, and the same-file half is
   **86% one form**: `export { q }`, an export specifier with no module
   specifier. That form was reachable all along — upstream's
   `getTargetOfExportSpecifier` branches on the *export declaration's* module
   specifier, not on the specifier — and it is what this slice built.
4. **The existing instrument could not have produced either split, and the
   reason is structural**, not an oversight. §2 says why, and it was the first
   thing checked.
5. One prediction, in §7, with what must not move and how it could be right for
   the wrong reason.

---

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

### Controls

| control | reads | what a non-zero would mean |
|---|---|---|
| `row 9: NO SYMBOL` | 0 | the probe's symbol re-derivation has drifted from `gap_reason`'s declaration-name branch |
| `row 10: NO SYMBOL` | 0 | as above |
| `row 10: UNCLASSIFIED KIND` | 0 | an alias declaration kind the split does not name |
| `row 9: UNEXPLAINED` | **11 of 1,852 (0.6%)** | lines no listed property describes — the honest residual |

Two of these started non-zero and each found a real defect:

- `UNCLASSIFIED KIND` read **19**, all `NamespaceExportDeclaration` —
  `export as namespace N`, a UMD global alias that is neither half. It now has
  its own bucket and is excluded from both.
- The first draft read `declarations.first()` only and attributed **819 lines**
  to *"an overload set"*. `getSignaturesOfSymbol` (`signatures.rs:142`) loops
  **every** declaration, so one gapping overload gaps the symbol; the property
  that actually held was a later signature's parameter annotation. Walking all
  declarations moved those 819 lines from a label that named the arity to one
  that names the cause.

### Attributed and independent, side by side

`docs/conventions.md`, "Size a positional arm from the flattened run": a
first-match-wins column answers a question about the attribution order. A
function can be `async` *and* have a destructuring parameter, so the probe prints
both columns, and the attribution order is by causal strength with the two purely
contextual properties last.

---

## 3. Row 9, measured (`conformance/` subset: 1,852 lines, 399 cases)

Concentration on the sub-population, re-run because the parent's shape is not the
child's: **top-1 2.2%** (`conformance/asyncWithVarShadowing_es6`), **top-10
13.0%**. Genuinely distributed, as the board said.

| property | attributed | independent | share (attrib.) |
|---|---:|---:|---:|
| a parameter annotation that is itself a gap | **1,045** | 1,069 | **56.4%** |
| a destructuring parameter | 202 | 205 | 10.9% |
| a return expression that is itself a gap | 184 | 260 | 9.9% |
| `async` — the return type is `Promise<T>`, a global | 182 | 182 | 9.8% |
| a generator — the return type is `Generator<…>`, a global | 143 | 182 | 7.7% |
| a type-parameter constraint or default that is itself a gap | 42 | 54 | 2.3% |
| two or more distinct return types (needs subtype reduction) | 33 | 46 | 1.8% |
| a type parameter carrying a modifier (`const`/`in`/`out`) | 8 | 8 | 0.4% |
| an anonymous function (`export default`) | 1 | 1 | 0.1% |
| expando properties (`f.a = 1`) | 1 | 1 | 0.1% |
| **UNEXPLAINED** | 11 | — | 0.6% |
| *CONTEXT: more than one declaration* | 0 | 819 | — |
| *CONTEXT: no return expression anywhere* | 0 | 1,500 | — |

**1,271 of 1,852 (68.6%) name a dependency that is itself a gap** — a parameter
annotation, a constraint, or a return expression. Another 325 (17.6%) wait on a
global that needs the lib and `bd tsr-9or.1`. That leaves 243 lines (13.1%) of
genuinely local work, split across destructuring parameter names, subtype
reduction and type-parameter modifiers — **and not one of them is in
`symbols.rs`.** They are `signatures.rs` and `unions.rs`.

### What this does to the board

Row 9 is listed at 2,618 lines, cause **TERMINAL / kind 1**, on the strength of
which the size is the worth. On the conformance subset the terminal fraction is
at most 13.1%, and the module named beside it (`tsr-checker/src/symbols.rs`) owns
none of it. **Do not rank row 9 at 2,618. Rank it, at this measurement, at
roughly one-eighth of that, and against `signatures.rs`.**

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

## 4. Row 10, measured, and the split the briefing asked for

`conformance/` subset: **794 lines**.

| form | lines | reach |
|---|---:|---|
| `import { x } from "./m"` | 264 | cross-file |
| **`export { q }`** | **154** | **same file** |
| `import a = require("./m")` | 129 | cross-file |
| `import * as ns from "./m"` | 89 | cross-file |
| `export { q } from "./m"` | 61 | cross-file |
| `import d from "./m"` | 47 | cross-file |
| `export as namespace N` | 19 | neither |
| `import a = b.c` | 18 | same file, deliberately gapped (prints the alias's own name) |
| `import a = b` | 7 | same file, ported — these are targets that themselves gap |
| `export * as ns from "./m"` | 6 | cross-file |

- **SAME FILE: 179 lines over 61 cases**, top-1 11.7%, top-10 62.0%.
- **CROSS FILE: 596 lines over 292 cases**, top-1 3.4%, top-10 23.0%.

**23.1% same-file, 76.9% cross-file**, and the same-file half is 86% one form.
Concentration re-run on the sub-population, per the requirement: the same-file
half is four times more concentrated than the row it came from (top-10 62.0%
against the row's 16.5%) — the same relationship `checker-notes-rank.md` found
between the 557-node row and its parent. It is still 61 cases and it is not one
file, so it survives; but it is a *narrower* item than the parent's shape
suggested, and anyone quoting the parent's top-10 for it would be quoting the
wrong number.

The cross-file half is blocked exactly as `checker-notes-arrays.md` records:
`Checker::new` takes `(binder, nodes, node_map)` and `BindResult` exposes no
specifier-to-file map, so ES imports need module resolution plumbed in — a
`Checker::new` signature change, not an arm in this file.

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

- **Row 9.** Nothing was built. 68.6% of it is propagation, 17.6% waits on lib
  globals, and the remaining 13.1% is in `signatures.rs` and `unions.rs`, which
  this slice does not own. Building anything in `symbols.rs` for it would have
  converted zero lines — the `+=` outcome, avoided by the check that was skipped
  there.
- **`export default q` / `export = q`.** `getTargetOfExportAssignment`
  (`checker.go:14976`) is a small function and the form is same-file. It
  contributes **zero lines** to this row on the measured subset, because the
  baseline records the *expression* `q` (an expression position) rather than the
  `default` alias's declaration name. Left unbuilt on the measurement rather than
  on an argument.
- **`import a = b.c`.** 18 lines, and it is a deliberate print-fidelity gap
  already documented on `Checker::resolve_alias`: the qualified form prints the
  alias's own name and this port has no symbol-accessibility machinery.

---

## 7. Prediction

**Rows:** row 10's same-file half, and only the `export { q }` form of it.
**Commit pair:** `61aa1e5^..61aa1e5` — verify with `git log --oneline 61aa1e5^..61aa1e5`
returning exactly one line. *(Filled in at commit; see the git log if this reads
as a placeholder.)*
**Mechanism:** `resolve_alias` now dispatches `ExportSpecifier` to a local
`resolveEntityName`, and `get_symbol_flags` follows the alias chain so the value
test does not reject an alias target.

**The measured population is 154 lines on the `conformance/` subset (5,907
cases).** `checker-notes-arrays.md` scored a prediction whose range floor *was*
the whole measured population and whose ceiling was twice it — "it had no room to
be right". So the population and the rate are stated separately:

- **Population:** the full-corpus `export { q }` count is unmeasured. The
  conformance subset is roughly 60% of the corpus's cases, so a full-corpus
  population of **200–300 lines** is the extrapolation, and it is an
  extrapolation and not a measurement.
- **Expected conversion rate: 55–75% of the population.** Not higher, because
  three things inside the form do not convert: a type-only target (`export { I }`,
  `export { T }`) which upstream prints as `any` and this port reports as a gap;
  a target whose own `get_type_of_symbol` still gaps (the `import a = b` row's 7
  lines are exactly that shape, already inside this row); and a target in an
  enclosing scope `Binder::resolve_name` reaches differently from
  `resolveEntityName`.
- **Therefore: +110 to +225 lines, point estimate +165.**

**What would make the rate low:** the type-only share. `exportsAndImports1`
exports ten names of which three (`I`, `N`, `T`) are type-only — 30% in the one
case that was read. If that ratio holds across the form, the rate lands at the
bottom of the range; if that case is unusual in exporting so many interfaces, at
the top. **This is the single number that decides the prediction, and it was not
measured** — the probe buckets by alias form, not by target meaning. A run with a
target-meaning bucket added would settle it, and that bucket costs one line.

**What must NOT move:**

- **The wrong-line count for these cases must not rise.** A local lookup that
  finds a different symbol from `resolveEntityName` converts a gap into a
  confident wrong answer, which is worse than the gap. If wrong lines rise while
  gaps fall, the arm is wrong even if the net is positive.
- **`export { q } from "./m"` must contribute zero.** The module-specifier guard
  is what holds it, and one test is red without it.
- **The `any`-credited count must not rise.** Nothing here answers `any`.
- **Rows other than 10 must not move by more than noise.** `get_symbol_flags` is
  reached only from `get_type_of_alias`, so `import a = b` targets that are
  themselves aliases may convert a handful; anything larger means the change
  reached further than its call graph says it should.

**How it could be right for the wrong reason:** the point estimate could land
inside the range because the type-only share and the population extrapolation
err in opposite directions — a larger-than-extrapolated population with a
worse-than-expected rate gives the same number. The cross-check that separates
them is the probe's own `export { q }` bucket on the full corpus, run *before*
and after: if the before-count is outside 200–300, the range was right for the
wrong reason regardless of what the gradient did.

**Scoreboard discipline:** this project is 4 hits, 8 misses, and every miss
arrived with a mechanism story and no cross-check — 12 for 12. This prediction
has a mechanism story. Discount it accordingly; the cross-check above is the only
part of it worth trusting.

---

## 8. Commands for the lead

```bash
# The split, full corpus. Every number in §3 and §4 is the conformance/ subset;
# these are the numbers to quote.
cargo run -p tsr-conformance --example symbol_dispatch_split --release

# The gradient before and after, for the prediction in §7.
cargo run -p tsr-conformance --bin coverage
```
