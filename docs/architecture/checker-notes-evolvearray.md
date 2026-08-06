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

## Superseded numbers

None yet. When one on this page is corrected it gets a dated header here rather
than a silent edit (`CLAUDE.md`).
