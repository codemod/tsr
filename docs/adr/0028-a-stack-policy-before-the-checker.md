# ADR-0028 — A stack policy is needed before the checker, and the choice is open

- **Status:** **superseded** by [ADR-0029](0029-stack-discipline-is-guards-plus-a-budget.md)
- **Date:** 2026-08-05
- Upstream pinned at `5b1047d10`.

> **This ADR recommended the wrong option, on a false premise, and is kept for
> that reason.** It recommended growing the stack on demand (`stacker`) and called
> it "the faithful option". Two things were wrong:
>
> 1. **It read as though upstream did something like this. Upstream does nothing.**
>    `stacker` is a Rust crate; typescript-go has no stack-management dependency in
>    `go.mod` and no explicit stack sizing anywhere. Go's runtime grows a goroutine's
>    stack transparently, so upstream never had to decide. "Faithful" here meant
>    faithful to Go's *runtime*, not to upstream's *code* — a distinction this ADR
>    blurred while proposing a new dependency.
> 2. **Its measurements do not isolate what it claims.** The `parse` column reports
>    what an input of a given nesting *costs*, not what recursion of that depth
>    costs, because `parser.rs` already caps recursion at `MAX_DEPTH = 192`. That
>    guard existed before this ADR was written and it went unmentioned.
>
> [ADR-0029](0029-stack-discipline-is-guards-plus-a-budget.md) has the corrected
> analysis and the decision taken. The measured table below is still valid as
> "what these inputs cost" and is reused there.

## Context

Go grows a goroutine's stack on demand, up to 1 GiB. A Rust thread's stack is fixed
at spawn. So every recursive descent this project ports has a depth limit upstream
does not have, and upstream therefore contains **no guard to copy**. This is the
one class of divergence where "port what upstream does" gives no answer.

`bd tsr-el3` recorded the first symptom: CI's debug conformance run aborted with
`has overflowed its stack` on `compiler/binderBinaryExpressionStress`, and the
harness now sizes its rayon pool at 32 MiB
(`crates/tsr-conformance/src/main.rs`). That unblocked the harness and does nothing
for a library consumer.

### What is actually measured now

`examples/stack_depth.rs` bisects the smallest power-of-two stack on which a stage
returns, over generated shapes so nesting depth is a parameter rather than whatever
a corpus case happened to contain. Each probe runs in a child process, because a
stack overflow aborts the process rather than the thread — without that, the first
overflow ends the run and every later number is silently missing.

At nesting depth 5,000:

| shape | parse (debug / release) | parse+bind (debug / release) |
|---|---|---|
| `a + a + a …` | 256 KiB / 256 KiB | 4 MiB / 4 MiB |
| `((((…))))` | 1 MiB / 256 KiB | **8 MiB** / 4 MiB |
| `Array<Array<…>>` | 1 MiB / 256 KiB | 4 MiB / 4 MiB |
| nested conditional types | 4 MiB / 1 MiB | 4 MiB / 2 MiB |

**This corrects two claims on `bd tsr-el3`, both of which were true of the one case
tested and false in general:**

1. "The parser is NOT the problem — it climbs precedence in a loop." True for the
   binary shape only. Nested parens, nested type arguments and nested conditional
   types each recurse in the parser, needing 1–4 MiB in debug.
2. "The binder half is unmeasured — `binder_symbols` passed at 2 MiB on this case."
   It does not pass in general. **The binder alone exceeds rayon's 2 MiB default at
   depth 5,000 on three of four shapes, in release as well as debug.**

The corpus case that started this is a binary chain, which is precisely the shape
the parser loops over — so the original measurement picked the input least likely
to show the parser or binder recursing.

### Why this is a pre-checker decision

The checker is the deepest-recursing component in the compiler and the largest
thing left to port. It recurses over the same nesting these shapes exercise, and
then again over type instantiation, which has no syntactic bound at all. Retrofitting
a stack discipline across a ported checker is far more expensive than deciding it
first — and the decision changes how every recursive function is written.

## The options, and what each costs

**1. Grow the stack on demand (`stacker`-style).** A helper called at each recursive
entry point that checks remaining headroom and allocates a new segment if it is low
— what `rustc` does as `ensure_sufficient_stack`, and the closest available
analogue to Go's behaviour. Deep input then simply works, with no invented limit.

- *For:* reproduces upstream semantics rather than approximating them; one call per
  recursive entry point; no diagnostic upstream does not have.
- *Against:* **a new dependency**, and `PLAN.md` §3.1 says the bill of materials
  mirrors oxc's. oxc does not use `stacker`. That makes this a decision about the
  project's dependency posture, not a technical call, which is why this ADR is
  *proposed* rather than accepted.

**2. A depth guard that reports.** Count recursion depth and emit a diagnostic past
a limit instead of aborting.

- *For:* no dependency; turns a crash into a message.
- *Against:* invents a diagnostic TypeScript does not emit, so every guarded case
  becomes a permanent conformance failure — and the corpus deliberately contains
  such inputs. It also has to be threaded through every recursive function.

**3. Size the stack at every entry point.** Each public entry spawns, or documents
that the caller must.

- *For:* no dependency, no invented diagnostic; already done in the harness.
- *Against:* pushes the problem onto every consumer, including the language server
  and any embedder; and "every entry point" is a growing set that nothing enforces.
  A number chosen once is also a number that goes stale — 32 MiB was picked against
  a 6–8 MiB measurement that is now known to be 8 MiB for one shape and unmeasured
  for the checker.

**4. Make each hot path iterative.** What strada does for binary expressions, and
what `binderBinaryExpressionStress.ts`'s own header comment names.

- *For:* no dependency and no limit, for the paths that get it.
- *Against:* per-site, and the sites are numerous. Viable as a *targeted* fix for a
  known-pathological path; not a policy.

## Recommendation

Option 1, with option 4 for any path that shows up hot afterwards. It is the only
one that neither invents a diagnostic nor delegates the problem to callers, and it
is what a Rust compiler front end conventionally does.

**The blocking question is the dependency**, and that is the project owner's to
answer: adding `stacker` departs from the oxc bill of materials that `PLAN.md` §3.1
commits to. If the answer is no, option 3 with an enforced convention is the
fallback, and the limit should be documented per entry point with a measurement
rather than a single global number.

## Consequences of not deciding

The harness stays green because of its 32 MiB pool. Everything else — the `dts`
pipeline, the language server when it exists, any embedder — inherits a 2 MiB
default and a 4–8 MiB requirement at depth 5,000. That is a latent crash, not a
conformance number, so no suite will report it.

## How we would know the measurement is wrong

- `examples/stack_depth.rs` bisects powers of two, so each cell is an upper bound
  within 2×. A precise byte count is noise; the order of magnitude is what the
  decision turns on.
- Depth 5,000 is chosen to match `binderBinaryExpressionStress`'s scale. Real-world
  generated code goes deeper; the numbers are a floor, not a worst case.
- The child-process method means a probe that fails to *launch* is indistinguishable
  from an overflow. If a whole row reads `>512M`, suspect the harness rather than
  the compiler.
