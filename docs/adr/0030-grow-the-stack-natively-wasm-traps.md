# ADR-0030: Grow the stack natively; wasm traps

- **Status:** accepted
- **Date:** 2026-08-05
- **Supersedes:** [ADR-0029](0029-stack-discipline-is-guards-plus-a-budget.md)
  entirely. Same problem, opposite conclusion, and ADR-0029's central premise —
  that depth guards could do the bounding — was falsified by implementing it.
- **Related:** [ADR-0028](0028-a-stack-policy-before-the-checker.md) (which
  proposed growth and was itself superseded by ADR-0029; this restores its
  mechanism on native for different reasons), [ADR-0001](0001-idiomatic-rewrite.md),
  `bd tsr-el3`.
- Upstream pinned at `5b1047d10`.

## The forcing constraint has not changed

Go grows a goroutine's stack on demand — roughly 8 KB initially, copy-and-double,
ceiling 1 GB on 64-bit. **typescript-go therefore contains no stack policy to
port**: `internal/binder/binder.go:2212` recurses plainly (`b.bind(expr.Left)`)
at any depth, and `internal/printer/printer.go:2859` does the same. Confirmed by
reading both at the pinned commit; neither has a trampoline, and `go.mod` has no
stack-management dependency.

A Rust thread's stack is fixed at spawn. The corpus is a *compiler* test suite and
contains inputs written to break compilers.

## What ADR-0029 claimed, and what implementing it showed

ADR-0029 decided that **depth guards would do the bounding** and the stack budget
would be headroom behind them. That was implemented in full: a `MAX_DEPTH` of
1,000 in `tsr-binder`'s `bind()`, a matching limit in `tsr-printer`'s expression
dispatch, a project-specific diagnostic outside upstream's code space, an
iterative left-spine emit for binary chains, and `WORKER_STACK` reduced from
32 MiB to 8 MiB.

**It worked, and it was still not enough.** With both guards in place and the
budget at 8 MiB, the conformance run still aborted with `has overflowed its
stack`, in `printer_round_trip`. The cause was a **third** recursive walk:
`tsr_ast::visit::walk_node`, the generated generic visitor.

That is the finding that decides this ADR. `walk_node` is not harness code and not
an oversight — it is the public tree walk that every consumer uses, including the
conformance harness today and the checker tomorrow. A depth guard fits it badly:
`Visit` is a trait with user-written implementations, so the counter has nowhere
to live that does not either change the trait's public shape or oblige every
implementor to carry it.

Generalising: **a guard-based policy requires every recursive walk in the codebase
to opt in, and is silently wrong until the last one does.** Three were needed to
get this far and the checker — the deepest-recursing component, not yet written —
would have added more. ADR-0028 raised exactly this objection ("pushes the problem
onto every caller"); it was theoretical then and is measured now.

## Decision

**Grow the stack on native targets. Accept that wasm32 traps.**

1. **`tsr_core::stack::ensure_sufficient`** wraps recursive tree-walk entry
   points, using `stacker::maybe_grow` with rustc's own constants (100 KiB red
   zone, 16 MiB growth). Applied at `tsr-binder`'s `bind()`, `tsr-printer`'s
   `emit_expression`, and the generated `walk_node`.
2. **On wasm32 it is a plain call.** wasm32 has one linear stack sized at link
   time; it cannot grow, and overflow is an uncatchable trap. Deep input traps
   there. This is a **known, documented ceiling**, not a diagnostic.
3. **wasm builds must link with an 8 MiB stack**
   (`-C link-arg=-zstack-size=8388608`), raising the ceiling from wasm-ld's 1 MiB
   default, which the existing corpus already exceeds.
4. **No depth limits, and no invented diagnostic.** Native behaviour now matches
   upstream's at any nesting depth.
5. **The printer's iterative left-spine emit is kept**, not because it is needed
   for stack safety any more, but because it is a pure win: the transformation is
   a reassociation of emit order with byte-identical output, it removes the
   dominant pathological case outright, and it is what strada does — that test's
   own header comment names the trampoline.

`WORKER_STACK` in the conformance harness comes down to 8 MiB, matching what a
wasm consumer gets, so the suites stop passing over trees no other consumer could
walk.

## The measurements behind it

Peak `bind()` recursion depth over **16,207 corpus files**
(`crates/tsr-conformance/examples/bind_depth.rs`):

| | depth |
|---|---|
| p50 | 7 |
| p90 | 11 |
| p99 | 16 |
| p99.9 | 25 |
| deepest excluding binary chains | 285 (`compiler/parsingDeepParenthensizedExpression`) |
| deepest overall | 4,958 (`compiler/binderBinaryExpressionStress`, 2 files) |

Minimum surviving stack by generated shape at nesting depth 5,000, debug profile
(`examples/stack_depth.rs`, bisecting powers of two, so each cell is within 2×):

| shape | parse | parse+bind before | parse+bind after |
|---|---|---|---|
| `a + a + a …` | 256 KiB | 4 MiB | 1 MiB |
| `((((…))))` | 1 MiB | 8 MiB | 1 MiB |
| `Array<Array<…>>` | 1 MiB | 4 MiB | 1 MiB |
| nested conditional types | 4 MiB | 4 MiB | 4 MiB |

The `parse` column measures what an *input* costs, not what recursion of that
depth costs, because `parser.rs`'s own `descend()` intervenes. The conditional-type
row is dominated by parsing and is unchanged by this ADR; that some parser type
paths appear not to call `descend()` is tracked separately as `bd tsr-el3.4`.

### What it costs

`ensure_sufficient` sits on the binder's hottest path, so the cost was measured
rather than assumed. Three readings each, same session, load average under 2,
`cargo bench -p tsr-binder --bench bind`, parse+bind nanoseconds:

| | `checker.ts` | `dom.generated.d.ts` |
|---|---|---|
| baseline (no growth) | 25,761,279 | 8,920,128 |
| check on **every** `bind()` level | 26,317,754 (**+2.2%**) | 9,097,674 (**+2.0%**) |
| check every **32nd** level | 25,871,212 (+0.4%) | 8,933,244 (+0.15%) |

Checking on every level is a real and reproducible ~2% regression. Checking every
32nd level returns the benchmark to inside run-to-run noise, and is safe because
32 binder frames are roughly 26 KiB against `ensure_sufficient`'s 100 KiB red
zone. `tsr-binder` therefore checks at an interval; `walk_node` checks on every
call, because `Visit` has no counter to interval against — if that shows up in a
profile, the answer is a counter on the walk, not a change of policy.

Peak RSS is unchanged: 15,048–15,056 KiB across three runs of
`cargo run -p tsr-binder --example rss --release`, against 14.8–15.1 MiB recorded
on 2026-08-05. Native thread stacks are virtual and committed lazily, so the
harness's reduction from 32 MiB to 8 MiB per worker does not show up here either
— which is the same asymmetry that makes wasm's 8 MiB expensive and native's
free.

## Alternatives

| Approach | Verdict |
|---|---|
| **Grow natively, trap on wasm** (this) | Native matches upstream exactly, nothing to opt into, the checker is written without thinking about depth. wasm keeps a ceiling it cannot report. |
| Depth guards + 8 MiB budget (ADR-0029) | **Tried and abandoned.** Correct where applied, but requires every walk to opt in and is silently wrong until all do. `walk_node` has no clean place for the counter. |
| `stacker` natively, guards retained for wasm | Strictly more code: two mechanisms, two behaviours, and a CI matrix, to give wasm a diagnostic instead of a trap on inputs that are already pathological. Reconsider if wasm becomes a first-class consumer. |
| Explicit work stack / trampoline everywhere | Rejected as policy by ADR-0029 and still rejected: no recognisable upstream counterpart, so every upstream fix must be re-derived across a different control structure — a permanent tax on what [ADR-0001](0001-idiomatic-rewrite.md)'s anchors exist to make cheap. Retained for the one binary-expression path, as strada does. |

## Consequences accepted

- **Native and wasm now differ in behaviour, not just in performance.** A file
  that compiles natively can trap on wasm. Conformance results are therefore
  native results, and a wasm consumer has a ceiling this project does not measure.
  This is the largest cost of this decision and the most likely reason to revisit
  it.
- **A dependency**, departing from the oxc bill of materials that `PLAN.md` §3.1
  commits to. Taken deliberately: `stacker` is what rustc uses for the same
  problem, and the alternative was a policy that had already failed once.
- **wasm's failure mode is bad**: an uncatchable trap that kills the instance,
  with no diagnostic and no recovery. The 8 MiB link-time stack raises the ceiling
  well past anything the corpus contains, but it does not change the failure mode.
- **`ensure_sufficient` is on hot paths** — every `bind()` and every `walk_node()`.
  The check is a stack-pointer comparison when there is room, which is the
  overwhelmingly common case, but it is not free.
- **The guard work was not wasted but it was not kept.** The measurement
  infrastructure (`bind_depth.rs`) and the printer's spine flattening survive; the
  limits and the invented diagnostic were removed.

## How we would know this was wrong

- **If wasm becomes a first-class target**, trapping is not an acceptable failure
  mode and the guards should come back for that target — the third row of the
  alternatives table. This is the most likely falsifier.
- **If `ensure_sufficient` costs measurable throughput** on the binder benchmark
  or the checker's hot paths, the call sites should move from "every level" to
  "every N levels" or to the specific deep walks, rather than the policy changing.
- **If a fourth, fifth and sixth recursive walk appear and each needs wrapping
  anyway**, then the difference from a guard policy is smaller than argued here,
  and the simplicity claim is weaker than it looks. The distinction that matters
  is that a *missed* `ensure_sufficient` degrades to the status quo ante on native
  only under genuinely pathological input, whereas a missed guard was an abort.
- **Depth 4,958 is not a worst case.** It is the deepest thing in *this* corpus.
  Generated code goes deeper, and the wasm ceiling is a fixed 8 MiB.
