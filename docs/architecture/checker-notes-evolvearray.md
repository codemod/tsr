# `autoArrayType`, evolving arrays, and the 4.18 points that are not there

Status: measured 2026-08-06 at **`d612291`**, over the 9,538-case `.types`
population, from one pinned binary in an isolated worktree. The instrument is
`crates/tsr-conformance/examples/evolvearray.rs`, added in the same commit as
this file. Upstream references are to `vendor/typescript-go` @ `5b1047d10`.

**Unit, restated on every table.** Every count is an **assertion line** in a
`.types` baseline unless the table header says `cases`.

---

## The headline result

`compiler/largeControlFlowGraph` holds **20,001 unmatched assertion lines —
4.18 gradient points**, the largest single-case residual in the corpus. It was
put to this workstream as the evolving-array machinery, which
`crates/tsr-checker/src/flow.rs` documents as a deliberate omission.

**20,000 of those 20,001 lines are ADR-0038's ceiling, and no checker work of
any kind reaches them.**

| | lines | points |
|---|---:|---:|
| **reachable** by `autoArrayType`, corpus-wide | **1,775** | **0.371** |
| **unreachable** — upstream's `errorType` printed `any` under TS2563 | **20,000** | **4.176** |
| needs the full evolving machinery (concrete `T[]`) | 53 | 0.011 |
| other | 83 | 0.017 |
| *collected total, not already right* | *21,911* | *4.575* |

### Why 20,000 of them are a ceiling and not work

The case's baseline sits under `error TS2563: The containing function or module
body is too large for control flow analysis`. Upstream's bailout is explicit,
and it does not produce an array type at all:

```go
// vendor/typescript-go/internal/checker/flow.go:120-125
if f.depth == 2000 {
    c.flowAnalysisDisabled = true
    c.reportFlowControlError(f.reference)
    return FlowType{t: c.errorType}
}
// and flow.go:81-83 — every later reference in the same body
if c.flowAnalysisDisabled { return c.errorType }
```

`c.errorType` is `c.newIntrinsicType(TypeFlagsAny, "error")` (`checker.go:979`),
and per [ADR-0038](../adr/0038-errortype-prints-error-and-the-corpus-has-a-ceiling.md) the node builder
renders anything carrying `TypeFlagsAny` as the `any` keyword. So the 10,000
`>data : any` and 10,000 `>data[0] : any` lines are **upstream's error type**,
printed `any`. We print `error`. That is exactly the divergence ADR-0038 costed
at ~6,000 lines and declared a ceiling, and
[ADR-0039](../adr/0039-the-any-that-upstream-prints-is-the-baseline-writers-decision.md) refused the workaround on
measured grounds.

`largeControlFlowGraph` was **not in ADR-0038's ~6,000**. So the ceiling is
larger than that ADR states — it is roughly **26,000 lines**, not ~6,000, and
`largeControlFlowGraph` alone is 3.4× the whole population that ADR was written
about. That is a correction to ADR-0038's *size*, not to its decision; the
decision is strengthened, because the payoff for reopening it is now visibly
concentrated in one pathological file.

Of the case's 20,001, exactly **one** line — `>data : any[]`, the declaration
name — is `autoArrayType` and is reachable.

**TS2563 appears in exactly one baseline in the entire corpus**
(`grep -rl TS2563 vendor/typescript-go/testdata/baselines/reference/submodule/`),
so this is not a class of cases. It is one file.

### The measurement that separates them, and what it is worth

The split is not asserted; it is a positive test on the case's own
`.errors.txt`. Mutation **N2** disables that test and nothing else:

| | reachable | unreachable |
|---|---:|---:|
| as measured | **1,775 (0.371 pts)** | 20,000 |
| N2 — reachability test never fires | **21,775 (4.546 pts)** | 0 |

**12×.** The 4.546 is the number that would have been quoted.

---

## The corpus-wide sizing of what *is* there

48 cases contain at least one `x = []` declaration; 78 such symbols; 21,962
assertion lines touch one.

### What upstream actually answers on those lines

| upstream's answer | lines | right | gap | wrong |
|---|---:|---:|---:|---:|
| `any[]` — `autoArrayType` alone | 988 | 0 | 0 | 988 |
| `any` — element, or `errorType` printed as `any` | 20,787 | 0 | 10,787 | 10,000 |
| `never[]` — upstream did **not** auto-array here | 51 | **51** | 0 | 0 |
| a concrete `T[]` — needs the full evolving machinery | 53 | 0 | 0 | 53 |
| other | 83 | 0 | 81 | 0 |

### Where the line sits

| site | lines | not yet right |
|---|---:|---:|
| the `x` of `const x = []` | 78 | 78 |
| the `[]` of `const x = []` | 78 | **27** |
| a reference to `x` | 10,938 | 10,938 |
| `x[i]` | 10,787 | 10,787 |
| `x.p` | 81 | 81 |

**The initialiser row is the design constraint, and it was not obvious.**
Upstream prints

```
const data = [];
>data : any[]      <- the symbol gets autoArrayType
>[] : never[]      <- the array literal keeps its own type
```

so `autoArrayType` is the type of the **symbol**, never of the expression. All
**51** currently-right lines in the collected set are `[]` initialisers. A fix
that typed the literal instead of the symbol would break every one of them
while appearing to do the same thing.

### The damage column, measured rather than argued

`docs/conventions.md` records two designs refused at 2.1 and 2.5 wrong-per-right
because the damage was in lines the slice never targeted. Here:

```
AT RISK (currently-right lines at any site the fix touches) = 0
```

Every site except the initialiser has **zero** currently-right lines — 78/78,
10,938/10,938, 10,787/10,787, 81/81 are already failing. The initialiser site is
out of scope by construction, because upstream types the symbol and not the
literal. So the reverse cascade for this item is **0 wrong per right at the
sites it touches**, which is the cleanest such number recorded on this project.

That is a statement about the *collected* lines only. It does not bound what a
fix makes computable elsewhere; see the open question below.

### Concentration, disclosed rather than smoothed

| reachable lines | share | case |
|---:|---:|---|
| 1,218 | **68.6%** | `compiler/deeplyDependentLargeArrayMutation2` |
| 220 | 12.4% | `compiler/typedArrays` |
| 76 | 4.3% | `compiler/deeplyDependentLargeArrayMutation` |
| 50 | 2.8% | `compiler/controlFlowArrays` |
| 49 | 2.8% | `compiler/assignmentToExpandingArrayType` |

46 cases hold a reachable line; **top-1 68.6%, top-5 90.9%.** Cases the item
would *finish* (upper bound, complete closure assumed, TS2563 case excluded):
**8**.

The 68.6% case was inspected rather than trusted to the classifier: it is a
`.js` file with `var arr = []` and **610** `>arr : any[]` assertions plus the
matching element accesses. Genuine `autoArrayType`, not a second bailout.

> **Concentration means two different things and they must not be collapsed.**
> On the **case gate** a concentrated row is a trap: 20,001 lines in one file
> flips exactly one case. On the **line gradient** concentration is leverage. Both
> are true here at once. Any quotation of the 0.371 points must carry the 68.6%
> beside it, and it must never be read as broad capability: it is two files.

---

## The decision, and an honest note about its status

**No code was written.** Two separate reasons, and only the first is about
merit.

### 1. The rule below is post-hoc, and is labelled so

`docs/conventions.md` requires the threshold to be fixed before the number is
seen. **It was not, here.** The corpus-wide sizing and the reachability split
were both visible before these conditions were written down. So this is a
**post-hoc justification, not a pre-registration**, and it is recorded as such
rather than presented as a rule that fired.

> **E1 — conversion ≥ 1,000 lines.** Measured **1,775**. The two items landed
> this session were +2,265 and +431; 1,000 sits between them.
> **E2 — ≤ 0.1 wrong per right.** Measured **0**.
> **E3 — concentration disclosed, not disqualifying.** 68.6% top-1, disclosed.

All three are satisfied. On merit this item is **available work**: 1,775 lines,
0.371 points, zero at-risk lines, spellable, anchored, and in files this
workstream owns.

### 2. The reason it stopped anyway

A post-hoc rule cannot license a build on this project — that is the whole point
of pre-registration, and treating it as though it could would be worth more
damage than 0.371 points is worth. The item is therefore handed over **sized**,
with the design constraint and the damage column already measured, so that
whoever takes it can pre-register against these numbers rather than re-derive
them. `bd tsr-5h0`.

**This is not a refusal on merit and must not be quoted as one.** The refusal on
this page is the 4.176 points, which is genuinely unavailable.

---

## Anchors, taken from `grep -n` on the declaration

The briefing that assigned this item cited `isEvolvingArrayOperationTarget` at
`checker.go:11182`. **It is `flow.go:1542`** — a different file. `xtask anchors`
would not have caught it, because `checker.go:11182` exists and resolves.

| what | where |
|---|---|
| `c.autoArrayType = c.createArrayType(c.autoType)` | `checker.go:1360` ✓ as briefed |
| `func (c *Checker) isEvolvingArrayOperationTarget` | **`flow.go:1542`** (briefed as `checker.go:11182`) |
| `c.errorType = c.newIntrinsicType(TypeFlagsAny, "error")` | `checker.go:979` |
| the depth-2000 bailout | `flow.go:120-125` |
| `if c.flowAnalysisDisabled { return c.errorType }` | `flow.go:81-83` |
| `func (c *Checker) reportFlowControlError` | `flow.go:1590` |

---

## Rendering: not needed, and this is measured

`crates/tsr-checker/src/printing.rs` was transferred to another workstream
mid-flight, with a request to say if this item needed a rendering change.

**It does not, and the probe shows it rather than assuming it.** The target
answers are `any[]` and `any`; this port already prints both — our *wrong*
answers on these very lines are `never[]`, a correctly printed array type, and
51 lines already match `never[]` exactly. The item is entirely about which type
is computed. `type_to_string` is untouched, and no file outside this
workstream's list is involved.

---

## Controls, printed unconditionally, with what pins each

| control | reads | pinned by |
|---|---:|---|
| **C1** auto-array declarations in `largeControlFlowGraph` | **1** | **the subject** — the file is one `const data = [];` followed by 10,000 assignments. True before a line was written; 0 or 2 means the finder is broken |
| **C2** annotated declarations collected | **0** | **construction** — `const x: number[] = []` takes its annotation and is excluded by a positive test |
| C2′ annotated empty-array declarations rejected | 88 | the mirror; pins the filter rather than one arm |
| **C3** a line attributed to an uncollected symbol | **0** | **construction** — attribution is through a resolved `SymbolId`, never a name match |
| A1 status split − collected lines | 0 | arithmetic |

Arithmetic tie-out, checkable by hand: `1,775 + 20,000 + 53 + 83 = 21,911`.

### The named mutations, each proven

| mutation | control | before | after |
|---|---|---:|---:|
| **N1** — stop excluding annotated declarations | C2 / C2′ | 0 / 88 | **401 / 0** |
| **N2** — the TS2563 reachability test never fires | reachable | 1,775 | **21,775** |

**N1 moved `REACHABLE TOTAL` by 2 lines** — 1,775 → 1,777 — while collecting 401
wrong symbols and doubling the symbol count from 78 to 166. An arithmetic check
on the headline number would have seen **0.1%** of that error. Only the paired
control saw it. Same family as the polarity inversion in
`checker-notes-wrong.md`: the defect moves lines *between* buckets.

---

## Open, and filed

- **The forward cascade is not measured.** `docs/conventions.md` measured a fix
  converting **1.66×** its own row. Giving 78 symbols `any[]` instead of
  `never[]` makes things computable outside the collected set — anything derived
  from `x[i]`. The 1,775 is the *row*, not the gradient move, and the multiplier
  for this row is unknown. It is more likely above 1.0 than below, because an
  array is declared in order to be read. `bd tsr-5h0`.
- **The 53 concrete-`T[]` lines are a negative for the cheap fix and a neutral
  in practice.** `autoArrayType` alone would answer `any[]` where upstream says
  `number[]`; all 53 are *already* wrong (`never[]`), so they move wrong → wrong.
  Stated so nobody scores them as conversions. `open`.
- **ADR-0038's ~6,000 is an undercount.** With `largeControlFlowGraph`'s 20,000
  the ceiling is roughly 26,000 lines. The ADR should carry a dated correction
  to its size; its decision is unaffected and is if anything strengthened.
  `bd tsr-zwi`.
- **`x.push(e)` widening is not sized separately.** Only 53 lines corpus-wide
  need a concrete element type, which bounds the *whole* evolving-array
  machinery beyond `autoArrayType` at 0.011 points. It is not worth building.
  This is a firm result, not an open question.

## ADDENDUM 2026-08-06 — the build was authorised and is blocked on a file this workstream does not own

`tsr-5h0` was authorised by the coordinator on the strength of the measured
`AT RISK = 0`, explicitly overriding the post-hoc-rule objection above and
taking the judgement as their own. **It was not built, for a different reason,
and the reason is checkable in two greps.**

### Where the type actually comes from

The design constraint says the fix must type the **symbol** and leave the array
literal at `never[]`. That rules out `array_literals.rs`
(`check_array_literal`, which this workstream owns and which correctly returns
`never[]` for an empty literal — `checker.go:8098`, `implicitNeverType`).

It also rules out `flow.rs`, which this workstream owns. `flow.rs` **takes**
`declared_type` as a parameter and never computes it:

```rust
// crates/tsr-checker/src/flow.rs:145-149
&mut self,
reference: NodeId,
symbol: SymbolId,
declared_type: TypeId,
```

and its own comment at `flow.rs:243` is explicit that the declared type arrives
already formed:

> *"an automatic declaration's `declared_type` in this port already **is**
> `anyType` — the two are one intrinsic here rather than two."*

`is_auto_typed_declaration` (`flow.rs:383`) has exactly one call site,
`flow.rs:159`, and it selects the *initial* type for the flow walk, not the
declared type.

The declared type of `const data = []` is computed at

```rust
// crates/tsr-checker/src/symbols.rs:1337-1339
let initializer = self.initializer_of(declaration)?;
let initializer_type = self.check_expression(initializer);
Some(self.get_widened_literal_type_for_initializer(declaration, initializer_type))
```

and `get_widened_literal_type_for_initializer` exists in **`symbols.rs` and
nowhere else** (`grep -rln`). `symbols.rs` belongs to another workstream.

**So the fix is one arm in `symbols.rs`** — return `any[]` when the declaration
has no annotation and its initialiser is an empty array literal, before
consulting the initialiser's own type. That single change gives the symbol
`any[]`, leaves `check_array_literal` untouched so the 51 `>[] : never[]` lines
stay right, and lets `flow.rs` propagate it to the references unchanged.

Stopped and reported rather than edited, per the standing instruction.

### The prediction, for scoring against the corpus run

If the arm lands as described:

| column | central | range | reasoning |
|---|---:|---|---|
| `right` | **+1,775** | +1,420 … +2,130 | the reachable set. The downside is narrowing intervening at a reference and giving something other than `any[]`; the upside is forward cascade beyond P |
| `gap` | **−787** | −650 … −850 | the `x[i]` lines answering `error` today |
| `wrong` | **−988** | −850 … −1,050 | the `any[]`-expected lines answering `never[]` today |
| `ArrayLiteralExpression` right/gap/wrong | **0 / 0 / 0** | exact | the fix types the symbol, never the literal |

`right = −gap − wrong` by construction: `787 + 988 = 1,775`.

**The last row is the falsifier.** If `ArrayLiteralExpression`'s columns move at
all, the fix typed the literal instead of the symbol and the 51 lines are going
red. That is condition 2 of the authorisation and it is what the named mutation
must make observable.

### On spellability: what was measured and what was not

The rule changed under this item mid-flight (`3f140c2`): spellability must be an
**exact match against the baseline**, not a shape check. Restating honestly what
this page's 1,775 is:

- **Upstream's side is exact.** Every line was classified on the baseline's
  literal right-hand side — `upstream == "any[]"`, `upstream == "any"` — never
  on a shape. That half of the test already met the new rule.
- **Our side is predicted, not measured.** The claim *"`autoArrayType` alone
  would answer this"* is an inference from the design, not a counterfactual run.
  It is exactly the half the new rule is about, and it is the half that
  measured 4.6 wrong per right on another row.

So **1,775 is an upper bound with one measured leg and one predicted leg**, and
it should not be quoted as a counterfactual result. The counterfactual is the
`symbols.rs` arm, applied and reverted, and it cannot be run from this
workstream. The prediction table above is the substitute and is offered to be
scored, not believed.

---

## `ArrayLiteralExpression` is more wrong than right, and this item does not touch it

Measured here over **P, a population pinned syntactically by node kind** — a
checker change cannot alter which nodes exist, so `|P|` is fixed across a
before/after pair, which a population defined as *"the lines that gap"* would
not be. Shape copied from `examples/fnexpr.rs`.

| node kind | \|P\| | right | gap | wrong | unaligned | |
|---|---:|---:|---:|---:|---:|---|
| `ArrayLiteralExpression` | 4,307 | 1,324 | 1,149 | **1,773** | 61 | **more wrong than right** |
| `ObjectLiteralExpression` | 7,252 | 4,585 | 1,691 | 725 | 251 | the opposite shape |
| `ElementAccessExpression` | 13,773 | 431 | 13,141 | 90 | 111 | almost all gap |

The `right` column reproduces the coordinator's independently tallied 1,324
**exactly**; `gap` and `wrong` differ by 15 and 46 because this probe counts
`unaligned` as its own arm rather than folding it into one of the others. Two
instruments, same conclusion.

### The overlap with this item is 78 lines, and the predicted movement is zero

| | lines |
|---|---:|
| `ArrayLiteralExpression` lines that are an auto-array initialiser | **78** |
| of those, currently **right** | **51** |
| currently wrong | 26 |
| unaligned | 1 |

Those 51 are the entire intersection of *"currently right"* and *"this item's
collected set"*, and they are the ones the design constraint protects. **The
item's predicted effect on all three `ArrayLiteralExpression` columns is 0**,
and that prediction is now on the record to be scored rather than asserted
afterwards.

**The other 1,747 wrong `ArrayLiteralExpression` lines are untouched by this
item and unranked by anything.** They are not the evolving-array machinery: only
53 lines corpus-wide need a concrete element type. `bd tsr-ejp`.

## Superseded numbers

None yet. When one on this page is corrected it gets a dated header here rather
than a silent edit (`CLAUDE.md`).

## §736 — the ASSIGNMENT half, built (+55 W→R, ZERO adverse, +0 cases)

§710 built the declaration half (`let x = []`). §712 named the remaining entry
point and §713 measured that the cheap version of it gains nothing. This is the
whole of it, and the shape is worth recording because **no one of its pieces
measures without the others**.

### What upstream does, in three lines

```go
// flow.go:233 — getTypeAtFlowAssignment
if f.declaredType == c.autoType || f.declaredType == c.autoArrayType {
    if c.isEmptyArrayAssignment(node) {
        return FlowType{t: c.getEvolvingArrayType(c.neverType)}
    }
    ...
}

// flow.go:283
func (c *Checker) isEmptyArrayAssignment(node *ast.Node) bool {
    return ast.IsVariableDeclaration(node) && node.Initializer() != nil && isEmptyArrayLiteral(node.Initializer()) ||
        !ast.IsBindingElement(node) && ast.IsBinaryExpression(node.Parent) && isEmptyArrayLiteral(node.Parent.AsBinaryExpression().Right)
}

// flow.go:106 — the unfinalized print
if evolvedType.objectFlags&ObjectFlagsEvolvingArray != 0 && c.isEvolvingArrayOperationTarget(reference) {
    resultType = c.autoArrayType   // any[]
}
```

`isEmptyArrayAssignment`'s **first** disjunct is the declaration half §710 built.
Its **second** — *"the node's parent is a binary expression with `[]` on the
right"* — is `let x; x = [];`, and it is a per-flow-NODE test.

### Where this port has to answer earlier than upstream does

`state.is_auto_array` decides whether the `ARRAY_MUTATION` arm accumulates into
`state.array_elements`, and the walk meets the mutations on the way **back** to
the assignment. So the flag must be set before the walk starts, and the port
answers the node question with a declaration-shaped stand-in:

> an auto-typed declaration (`let x;`) in whose control-flow container `x = []`
> is written — `symbol_has_empty_array_assignment`, the same scan shape as
> `symbol_has_any_assignment`, memoised per symbol with a pessimistic `false`
> inserted before the scan so name resolution cannot re-enter into a cycle.

### The pieces, and why §713 measured zero with only one of them

1. The widened predicate above.
2. `get_type_at_flow_assignment` returning the **evolving array** for an
   empty-array assignment rather than the assigned `never[]`. This port has no
   evolving-array type object (ADR-0003 keeps the accumulation in a side vector),
   so the unfinalized array is spelled as `state.declared_type` — which is
   exactly what §710's finalisation gate (`answer.t == state.declared_type`)
   tests for. **Without this, the gate can never hold for `let x;`** and every
   accumulated element is discarded. That is the entirety of §713's zero.
3. `flow.go:106`'s `autoArrayType` branch at an operation target.

### Piece 3 is the generalisable lesson

§710 never needed it. In the declaration half `state.declared_type` already **is**
`any[]` (`autoArrayType`, minted at `symbols.rs:4270`), so answering the declared
type at an operation target was right **by coincidence of two values being
equal**. In the assignment half the declared type is `any`, and the identical
code answered `x : any`, so `x.push(…)` no longer resolved:

```
want=number  got=any        ×9 RIGHT→WRONG on the first cut
```

> **A stand-in that is correct because two values happen to coincide does not
> announce itself.** It behaves perfectly until a caller arrives where they
> differ, and then it fails as a regression in the new caller rather than as a
> flaw in the old one. The port had one such coincidence here; the way it was
> found was measurement, not reading.

### Piece 5 — the same coincidence again, at the loop junction

`while (cond()) { x.push("hello") }` answered `(number | boolean)[]` against
upstream's `(string | number | boolean)[]`. This looked like the incomplete-type
loop fixpoint (§12.6) and it was not.

Upstream's loop-label antecedent walk ends with (`flow.go:1387`):

```go
// If the type at a particular antecedent path is the declared type there is no
// reason to process more antecedents since the only possible outcome is subtypes
// that will be removed in the final union type anyway.
if flowType.t == f.declaredType {
    break
}
```

**That break cannot fire on the array track upstream** — the value flowing there
is an evolving array, a distinct type object, never `==` `f.declaredType`. Here
the unfinalized array is SPELLED as `state.declared_type`, so it was true, the
antecedent loop broke before the back edge was walked, and the loop body's
element never reached `state.array_elements`. Gating on `!state.is_auto_array`
restores upstream's behaviour rather than departing from it.

> **This is piece 3's coincidence a second time, and the general form is now
> statable: making a stand-in value EQUAL to an existing one changes the meaning
> of every `==` test that value takes part in.** Two of the three such tests in
> the walk wanted the identity (both finalisation arms); the third did not, and
> nothing distinguished them at the site. The file was audited for
> `== state.declared_type` afterwards — three sites, all accounted for. **Re-run
> that audit if a fourth is added**; it is the falsifier for this whole design.

### A fourth piece the corpus named: `getReferenceRoot`

`f16` writes `(x = [], x).push(5)`. `is_evolving_array_operation_target` tested
the **node's** parent; upstream tests the parent of `getReferenceRoot(node)`
(`flow.go:1876`), which climbs through parentheses, `=` left operands and comma
right operands. Identical for every shape §710 met; wrong here, because the
reference is a comma's right operand inside parentheses and `.push` is two levels
out. Ported verbatim, and it took the last RIGHT→WRONG to zero.

### What remains in `controlFlowArrays` — 21 lines, and NOT this lane

- **`f7`'s `let x = null;`** (6 lines) — declared `null`, neither auto nor
  auto-array. A different entry point.
- ~~**Branch-merge residue** (~13 lines)~~ — **diagnosed and fixed by §737
  below; it was not a merge problem at all.**

### Why a +0-case build was kept

The case does not convert: `controlFlowArrays` still holds 21 wrong lines, so all
53 conversions land inside cases that keep failing. Kept on the standing rule
that a faithful arm with zero adverse transitions is worth its lines, and because
it closes §712's stated reopening condition — leaving it out would send the next
session at the same three-piece build with §713's zero as its only evidence.


## §737 — finalisation belongs at the QUERY, not at every recursion (+15 W→R, ZERO adverse)

§736 left 13 lines in `controlFlowArrays` labelled *"branch-merge residue"* with
a shape it could not explain:

```
f4:  want (string | number)[]     got  number[] | (string | number)[]
```

A union of two *different* finalisations of the same variable. That is not what a
merge of two branches produces; it is what **two finalisations** produce.

### The defect

§710 placed `finalizeEvolvingArrayType` at the tail of
`get_type_at_flow_node`. **That function is recursive.** Every nested call
therefore finalised, each against whatever `state.array_elements` held at that
moment in the walk. At a branch label, antecedent 1 finalised with `[number]` and
antecedent 2 with `[number, string]`, and the join unioned `number[]` with
`(string | number)[]`.

Upstream calls it **once**, in the outer `getFlowTypeOfReference`
(`flow.go:105-109`) — never inside `getTypeAtFlowNode`. Moving it there is the
fix, and `finalize_evolving_array` is now called from the two reference-query
entry points and nowhere else.

### Moving it alone breaks the other case, and that is the interesting part

With finalisation at the query, `f4` became exact and **`f6` regressed** — 2
RIGHT→WRONG, `want=number | string[] got=string[]`:

```ts
function f4() {                         // EVERY path evolving
    let x = [];
    if (cond()) { x.push(5); } else { x.push("hello"); }
    return x;                           // (string | number)[]
}
function f6() {                         // ONE path evolving, one not
    let x;
    if (cond()) { x = 5; } else { x = []; x.push("hello"); }
    return x;                           // number | string[]
}
```

- `f4` needs the junction to **stay evolving**, so the one finalisation at the
  end sees both elements.
- `f6` needs the junction to **finalise the evolving branch there**, because the
  union of `number` with the unfinalized `any` stand-in collapses to `any`, which
  then finalises whole and drops the `number` arm.

Upstream distinguishes them in one function, `getUnionOrEvolvingArrayType`
(`flow.go:1314`), whose comment states the rule outright:

```go
// At flow control branch or loop junctions, if the type along every antecedent code path
// is an evolving array type, we construct a combined evolving array type. Otherwise we
// finalize all evolving array types.
```

`isEvolvingArrayTypeList` is *"every non-`never` constituent is an evolving array,
and at least one is"*. In this port *"is an evolving array"* reads as
`t == state.declared_type`, the §736 stand-in.

### A workaround was built first, then measured inert and dropped

Before the recursion defect was found, the `SHARED` flow cache was gated off the
array track: the cache returns **before** the walk, so a shared node visited a
second time skips the element accumulation entirely. That is a real effect and
the gate did move lines.

**With the finalisation moved and the junction rule in place it measures exactly
zero** — 435,531 with the gate and 435,531 without — so it was reverted.

> **A workaround that stops measuring once the real cause is fixed is evidence
> the real cause was found.** Re-measuring it is what turns that from a hope into
> a fact, and it costs one build. The alternative — keeping both — would have
> left a permanent unexplained gate on a hot path, and the next reader with no
> way to tell which of the two was load-bearing.

### What remains: 7 lines, 6 of them one entry point

- **`f7`'s `let x = null;`** (6 lines) — wants `string[] | null`, gives `any`. A
  `null`/`undefined` initialiser is a **third** way onto the auto road, beside
  the no-initialiser form (§736) and the `[]`-initialiser form (§710). **Next
  item in the lane**, and it is a declaration-side question rather than a flow
  one.
- **One line in `f10`** (`0:185`) — wants `(string | number)[]`, gives `any[]`.

## §738 — the THIRD entry onto the auto road: `let x = null` (+1 case, +17 W→R, ZERO adverse)

§737 left 7 wrong lines in `controlFlowArrays`, 6 of them `f7`:

```ts
function f7() {
    let x = null;
    if (cond()) { x = []; while (cond()) { x.push("hello"); } }
    return x;  // string[] | null
}
```

Upstream names all three entries in one comment (`checker.go:16697-16704`):

```go
// use control flow tracked 'any' type for non-ambient, non-exported var or let variables
// with no initializer or a 'null' or 'undefined' initializer.
if c.getCombinedNodeFlagsCached(declaration)&ast.NodeFlagsConstant == 0 &&
        (initializer == nil || c.isNullOrUndefined(initializer)) {
    return c.autoType
}
// Use control flow tracked 'any[]' type for non-ambient, non-exported variables with an empty array
// literal initializer.
if initializer != nil && isEmptyArrayLiteral(initializer) {
    return c.autoArrayType
}
```

- no initializer — §736 (flow side), long-standing (declared side)
- `[]` initializer — §710
- `null`/`undefined` initializer — **this item**

### Why it hid: the declared half was already correct

`symbols.rs:4304` already mints `any` for a null/undefined initialiser, under the
same guards, and cites the same upstream line. So `let x = null` **had the right
declared type all along**; the FLOW predicate
(`is_auto_typed_declaration`) was the short one, requiring
`initializer.is_none()`. One upstream condition, two spellings in this port, and
only one of them complete.

> **Diff a predicate against its sibling before measuring it, not after.** When
> two spellings of one upstream condition disagree, the corpus reports it as a
> regression in whichever moved last — which reads as *"my change broke this"*
> rather than *"these two never agreed"*.

### The gate, located by 8 regressions

First cut: **+17 W→R against 8 RIGHT→WRONG** — `compiler/forIn` 7,
`compiler/null` 1, all `want=any got=null`. Both fixtures are `@strict: false`,
so **`noImplicitAny` is off**, and upstream's entire block is guarded by
`if c.noImplicitAny`. Ungated, the port sent `var arr = null` down the auto road
and answered the walk's initial `null`; upstream never enters the block and takes
the widened `any`.

The gate is already present at the port's own mint site (`symbols.rs:4304`).
Adding it to the flow predicate **removed all 8 regressions and kept all 17
gains**.

### A known asymmetry, recorded rather than closed

The **no-initializer** half is left ungated by `no_implicit_any`. It has been
unconditional since §9.7 and is measured that way; upstream gates both. Recorded
as a stated asymmetry rather than a claim the gate does not apply — closing it is
its own build with its own blast radius, and no corpus case currently asks for
it. **If a future build widens the no-initializer half, this is the first thing
to check.**

### Lane status

`controlFlowArrays`: **21 → 7 → 1** wrong lines across §736, §737, §738. All
three entry points onto the auto road are built. The single remaining line is
`0:185` — `(string | number)[]` wanted, `any[]` given, inside `f10`.

## §739 — the loop-label cache starves every query after the first (+1 case, +1 W→R, ZERO adverse)

§738 left one wrong line in the lane, `f10`'s post-loop reference —
`want=(string | number)[] got=any[]`. The cause was not accumulation and not
finalisation; both were already correct. It was **the flow-loop cache**.

### The defect, from the trace

`TSR_TRACE_LOOP` showed the query order plainly: the `return x` query runs
FIRST (it computed the loop label with full accumulation — elements
`[number, boolean, string]` — and cached the label's result), and the
post-loop `x;` query then HIT the cache, skipped the entire antecedent walk,
accumulated nothing, and finalised over an empty list — `any[]`.

Upstream cannot have this defect **by representation**: its `flowLoopCache`
(`flow.go:1336`) stores the label's *type*, and on the array track that type
is an EVOLVING ARRAY carrying its element union — the cache hit hands the
next query the elements along with the verdict. This port's unfinalized
stand-in is `state.declared_type`, which carries nothing, so a bare-`TypeId`
cache was faithful for every track except the one where the cached value has
state.

### The fix: cache the label's element slice beside its result

`flow_loop_cache` becomes `key → (TypeId, Vec<TypeId>)`, the second field the
`array_elements` slice contributed while computing the label (marked at
entry, taken at insert). A hit replays the slice into the querying state —
exactly the accumulation the skipped walk would have done. Off the array
track the slice is empty and nothing changes.

### The rider: dedupe must be scoped, or the slice lies

The `ARRAY_MUTATION` arm deduped against the WHOLE accumulator. An element
already seen *downstream* of the label (between the reference and the label —
`f10`'s `x.push(99)` contributes `number` before the walk reaches the label)
would suppress the same element's contribution *inside* the label, and the
cached slice would silently miss it — wrong for every later query, invisible
to the query that cached it. `state.element_dedupe_mark` scopes the dedupe to
the innermost in-flight label's region, making each label's slice
self-contained; outside any label it is `0`, which is §710's original
whole-accumulator behaviour. Union construction dedupes anyway, so the only
cost is a possible duplicate in the accumulator across regions.

### Measured

```
cases    6,072 → 6,073   (+1: compiler/controlFlowArrays, PASS whole)
right    435,548 → 435,549  (+1 W→R, the f10 line)
R→W 0 | G→W 0 — zero adverse, full-corpus scorepair over a baseline
freshly accepted on clean 3ffc5857
```

### Lane status

`controlFlowArrays`: **21 → 7 → 1 → 0**. The evolving-array lane's flagship
case passes whole; all three auto-road entries built (§736/§737/§738), the
junction rule built (§737), finalise-at-query built (§737), and the cache
made element-aware (this item). Known remaining asymmetry: the
no-initializer half is still ungated by `no_implicit_any` (§738, recorded
there).
