# ADR-0029: Stack discipline is guards plus a budget, not a growable stack

- **Status:** superseded by [ADR-0030](0030-grow-the-stack-natively-wasm-traps.md)
  on 2026-08-05. Its central claim — that depth guards could do the bounding —
  was falsified by implementing it: with guards in both the binder and the printer
  and the budget at 8 MiB, the conformance run still overflowed, in a *third*
  recursive walk (`tsr_ast::visit::walk_node`) that a guard fits badly. Left
  unedited below; the wrong turns are the useful part of the archive.
- **Date:** 2026-08-05
- **Supersedes:** [ADR-0028](0028-a-stack-policy-before-the-checker.md) entirely —
  same problem, opposite conclusion, and two of its premises were wrong.
- **Related:** [ADR-0001](0001-idiomatic-rewrite.md) (why a trampoline is expensive
  here), `bd tsr-el3`.
- Upstream pinned at `5b1047d10`.

## The forcing constraint

Go's runtime grows a goroutine's stack on demand — roughly 8 KB initially,
copy-and-double, ceiling 1 GB on 64-bit, with the bounds check emitted into every
function prologue. So **typescript-go contains no stack policy to port.** It has no
stack-management dependency (`go.mod` lists go-winio, go-json-experiment, go-cmp,
go-osstat, patience, xxh3 and `golang.org/x` packages, and nothing else) and no
explicit stack sizing in `internal/`. The one construct that looks like it —
`nodebuilder.go:75`, `stackSize := len(b.ctxStack)` — is a data stack, a
`[]context` slice.

A Rust thread's stack is fixed at spawn, and **wasm32 cannot grow its stack at
all**: one linear stack sized at link time (`-z stack-size`, wasm-ld defaults to
1 MiB), with overflow arriving as a trap that kills the instance rather than
something catchable. wasm is a wanted target.

This is therefore the one class of divergence where "port what upstream does" gives
no answer, and it has to be decided from first principles before the checker — the
deepest-recursing component left — is written, because the answer changes how every
recursive function is written.

### Two kinds of recursion, and only one is ours to invent

**Algorithmic recursion is bounded by upstream, with real diagnostics.** These are
not stack-safety devices; they exist because infinite generic types perpetually
generate new type identities. They are ordinary ported behaviour:

| Limit | Diagnostic | Upstream |
|---|---|---|
| instantiation depth 100, count 5M | `Type_instantiation_is_excessively_deep_and_possibly_infinite` | `checker.go:22111` |
| relation comparison depth | `Excessive_stack_depth_comparing_types_0_and_1` | `relater.go:379`, `:3077`, `:4809` |
| relation comparison count | `Excessive_complexity_comparing_types_0_and_1` | `relater.go:379` |
| flow analysis size | `The_containing_function_or_module_body_is_too_large_for_control_flow_analysis` | binder/checker |
| node builder depth 10 | — | `nodebuilderimpl.go:3158` |

Because these cap at small numbers (100, 10), algorithmic recursion is **not a
stack problem**. Porting them is required work under any policy.

**Syntactic recursion is unbounded upstream and bounded by input.** Nesting depth is
at most the source size. Two measured facts matter:

- **The corpus's deepest *real* nesting is 69** (recorded at `parser.rs:199`).
- `parser.rs` already caps its own recursion at **`MAX_DEPTH = 192`**, via
  `descend()`, reporting rather than overflowing, with a test pinning it
  (`deeply_nested_input_reports_rather_than_overflowing_the_stack`). So a syntactic
  depth guard is **already this project's policy**; it predates both ADRs and is a
  deliberate, documented deviation from upstream.

### The actual hole

`a + a + a …` is parsed **iteratively** by precedence climbing, so `descend()` never
fires — but the tree it builds is 5,000 nodes deep and left-leaning. Consumers then
walk that tree recursively with **no guard at all**. That is exactly why, on the
same input at depth 5,000:

| | parse (debug / release) | parse+bind (debug / release) |
|---|---|---|
| `a + a + a …` | 256 KiB / 256 KiB | **4 MiB / 4 MiB** |
| `((((…))))` | 1 MiB / 256 KiB | **8 MiB / 4 MiB** |
| `Array<Array<…>>` | 1 MiB / 256 KiB | 4 MiB / 4 MiB |
| nested conditional types | 4 MiB / 1 MiB | 4 MiB / 2 MiB |

Measured by `examples/stack_depth.rs`, bisecting powers of two, so each cell is
within 2×. `a + a + a …` at ~4,971 operands is a **real corpus case**
(`compiler/binderBinaryExpressionStress`), so **wasm's default 1 MiB stack already
fails on the existing corpus**, before any checker exists.

The `parens`, `arraytype` and `conditional` rows also show the parser consuming
1–4 MiB where `MAX_DEPTH` should have capped it at 192 levels — which means some
recursive paths do not call `descend()`. That gap is a finding, not a conclusion,
and is tracked separately.

## Decision

**Guards do the bounding; the stack budget is headroom, not the mechanism.**

1. **Port upstream's algorithmic limits** (the table above) as the checker is
   written. Faithful, required, and they keep semantic depth at 100 or less.
2. **Guard recursive tree walks** in the binder, printer and checker, mirroring
   `parser.rs`'s `descend()`. This is the hole: the parser protects itself and
   leaves consumers exposed to the deep trees it legitimately produces.
3. **Size the stack modestly** — 8 MiB native, 8 MiB on wasm via
   `-C link-arg=-zstack-size=8388608` — as margin behind the guards, chosen from the
   table above rather than picked round.
4. **No `stacker`, and no dependency.**

## Alternatives

Scored on faithfulness, runtime cost, wasm, effort and failure mode:

| Approach | Verdict |
|---|---|
| **Guards + budget** (this) | Zero per-call cost on unguarded paths, works on wasm, nothing restructured, no dependency. |
| `stacker` | **Does not work on wasm at all.** Departs from the oxc bill of materials (`PLAN.md` §3.1). Emulates a Go runtime feature rather than porting upstream code. |
| `stacker` cfg-gated native, sized wasm | Strictly more work for a benefit the budget already covers; two behaviours and a CI matrix to protect a case that no longer exists once wasm forces correctness at a fixed size. |
| Explicit work stack / trampoline | **Rejected as a policy, retained as a targeted tool.** A hand-rolled explicit-stack `check_expression` has no recognisable upstream counterpart, so every upstream fix must be re-derived across a different control structure — a permanent tax on precisely what [ADR-0001](0001-idiomatic-rewrite.md)'s anchors exist to make cheap. Correct for one known-pathological path, as strada does for binary expressions. |

The decisive argument against `stacker` is not its cost but its irrelevance under the
wasm requirement: **if a fixed stack must be correct anyway, growth only raises a
native ceiling that 8 MiB already puts far beyond anything real.**

## Consequences accepted

- **A syntactic depth guard is a divergence.** Upstream reports nothing for deep
  nesting; we will. It fires only above `MAX_DEPTH`-scale nesting, where the corpus
  peaks at 69 and strada itself stack-overflows — but it is a divergence, and if a
  corpus case ever reaches it, that case will fail.
- **Two failure modes to keep distinct in diagnostics.** An algorithmic limit
  (TS2589 and friends) is upstream behaviour; a syntactic guard is ours. Reusing an
  upstream code for the latter would be worse than inventing one, because it would
  be wrong at a position upstream reports nothing.
- **wasm pays real memory.** A native thread stack is virtual, committed lazily —
  every RSS reading taken on 2026-08-05 (14.8–15.1 MiB) was under the harness's
  32 MiB × N-worker pool, unpaid because untouched. On wasm the stack lives *in
  linear memory* and counts from instantiation, so 8 MiB is 8 MiB. That asymmetry is
  the whole reason the budget is modest and the guards do the work.
- **Codebase size is not the axis.** Stack depth scales with nesting inside a single
  file, not with project size; a large monorepo buys more heap and more parallelism,
  not more stack. The pathologies are single-file artefacts: generated code, one
  4,971-operand chain, huge nested literals.
- The harness's 32 MiB rayon pool stays for now. It is oversized under this policy
  and should come down to the budget once the guards land, so that the guards are
  what the suites actually exercise.

## How we would know this was wrong

- **`examples/stack_depth.rs` bisects powers of two**, so every figure is an upper
  bound within 2×, and the `parse` column measures input cost rather than recursion
  depth wherever `descend()` intervenes. A tighter per-level number would change the
  8 MiB choice; it would not change the shape of the decision.
- **Depth 5,000 is not a worst case.** It matches `binderBinaryExpressionStress`'s
  scale. Generated code goes deeper, and the numbers are a floor.
- **If a real-world input trips the syntactic guard**, the guard is too low or the
  budget too small, and the trade should be revisited — that is the falsifier for
  "69 is representative".
- **If wasm is dropped as a target**, `stacker` becomes competitive again on
  faithfulness grounds and this ADR should be revisited rather than assumed.
