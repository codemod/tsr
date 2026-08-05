# 0038 — `errorType` prints `error`, and the corpus therefore has a ceiling

**Status:** accepted, 2026-08-05
**Supersedes:** nothing. Makes explicit a choice taken implicitly in `intrinsics.rs` and never costed.

## The forcing constraint

Upstream's error type is created as

```go
c.errorType = c.newIntrinsicType(TypeFlagsAny, "error")   // checker.go:979
```

The intrinsic name is `"error"`, but that name is internal. The node builder
renders anything carrying `TypeFlagsAny` as the `any` keyword, so **upstream's
`errorType` prints `any` in a `.types` baseline.**

Measured, not inferred: `>NodeType : any` appears **636 times** in the corpus
baselines. `NodeType` is declared in `parserRealSource3.ts` and referenced from
`parserRealSource4`, `7`, `10` and `11` — which are *separate cases*. The name
is undeclared within its own case, so **upstream fails to resolve it too**, and
prints `any`.

This port prints `error` (`crates/tsr-checker/src/printing.rs`, the intrinsic
created at `intrinsics.rs`). That is deliberate: it is what makes a **gap**
(we could not compute this) distinguishable from a **wrong answer** (we computed
something else), which every ranking, every histogram bucket and every
prediction in this project depends on.

So for every name upstream *also* fails to resolve, we compute the same type and
print a different string, and the line is scored against us.

## The size of it

Two rows, cross-checked from both directions by different agents:

| population | lines |
|---|---:|
| bare unresolved names (`>NodeType : any` 636, `>ErrorRecoverySet : any` 109, `>div : any` 1,026, …) | ~4,500 |
| accesses *through* an unresolved receiver (`>NodeType.X : any` 536, `>ErrorRecoverySet.X : any` 109) | ~1,457 |
| **total** | **~6,000** |

The second was found while bucketing a different row and independently
confirmed to be the same defect seen from a second position: upstream propagates
`errorType` through the access, and `errorType` renders as `any`.

**These ~6,000 lines are unreachable by any amount of checker work.** They are
not outstanding work; they are a ceiling. Before this was measured, the gradient
was being treated as though 100% were approachable and the gap to it was
entirely work not yet done.

## The alternative, taken seriously

**Render `errorType` as `any` in the producer only**, keeping `error` internally
so the instrument can still bucket by `TypeId` identity. The harness *can* do
this — `types_producer` holds the `TypeId` and could render one way while
bucketing another. It gains all ~6,000 lines and appears to cost nothing.

**Rejected, and the arithmetic is why.** Upstream prints `any` for a *genuine*
`any` as well as for an error. Rendering our `errorType` as `any` would match
the baseline regardless of which one upstream meant — so it would score as
**right** every line where upstream genuinely computed `any` and we merely
failed.

That population was measured independently, before this question arose: **7,068
corpus lines where upstream answers `any` and we gap**, of which only 950 moved
when the real fix for them landed. So ~6,100 lines would be **falsely credited**,
and undetectably — the instrument would agree with the baseline.

It buys ~6,000 real lines for ~6,100 fake ones, and destroys the ability to tell
that it did.

## Consequences accepted

- **`checker_types` cannot reach 100%.** Any target should be stated against a
  ceiling of roughly 98.7% of aligned lines, not 100%.
- ~6,000 lines will remain in the histogram's gap buckets forever, and must be
  excluded by hand from any ranking. `bd tsr-4sc` records them as a ceiling
  rather than an item.
- A reader comparing a single line against a baseline will see `error` where
  upstream shows `any` and may reasonably think it a defect. It is not.

## How we would know this was wrong

- If a way existed to tell, *from the baseline alone*, whether upstream's `any`
  came from `errorType` or from a genuine `any`, the middle option becomes
  correct and should be taken. Nothing in a `.types` file distinguishes them
  today; a `.errors.txt` cross-reference might, since upstream reports a
  diagnostic for the unresolved name — that is the one avenue worth trying.
- If the falsely-credited population were small rather than ~6,100, the trade
  would flip. It was measured once; it should be re-measured before anyone
  reopens this.
- If the project ever stops ranking work from the gap/wrong split, the
  separability this protects is worth less and the trade changes.
