# The checker

`crates/tsr-checker`, ported from `internal/checker` at the pinned commit
`5b1047d10`. Upstream is **60,269 lines** — a third of the core port, and by
PLAN.md's estimate 120–200 sessions. This document describes what exists, what
does not, and the two decisions that shape everything that follows.

## Status: a foundation, not a checker

Stated first because the distinction matters to anyone reading the conformance
table. `checker_types` reads **0/9,538** and that is correct, not a bug.

**What exists:**

| | |
|---|---|
| `TypeFlags` | Ported one-for-one from `types.go:427`, bit positions included |
| `TypeStore` / `TypeId` | The handle model, per [ADR-0013](../adr/0013-checker-memoisation.md) |
| Intrinsics | Created in upstream's order (`checker.go:975`–`1015`) |
| Literal & keyword expression types | With interning |
| `type_to_string` | For the above, in `.types` baseline form |
| Literal widening | The direction only — see below |

**What does not exist:** declaration types (`getTypeOfSymbol`), type nodes,
object/union/intersection/generic/conditional/indexed-access types,
assignability, inference, overload resolution, control-flow narrowing, and every
one of the checker's diagnostics. `bd tsr-4sc.2` is the next slice.

## Two decisions that shape the rest

### Types are handles, and nothing hands out a reference

Settled by [ADR-0013](../adr/0013-checker-memoisation.md) with three working
implementations before it constrained anything (`crates/tsr-checker-spike`).

Every lazily-computed value in upstream has this shape
(`checker.go:16544`): take a mutable borrow of a side table, recurse into the
checker, then write the result *through the borrow taken before the recursion*.
That is precisely what Rust's borrow checker rejects, and it is not a corner case
— it is how the entire checker computes.

The resolution is that checker methods take `&mut self` and return `TypeId`, and
a type's contents are reached by asking the checker again. Because `TypeId` is
`Copy` and 4 bytes, nothing survives into the recursion:

```rust
if let Some(cached) = self.memo.get(&id) { return *cached; }  // borrow ends here
let resolved = self.compute(id);                              // recursion owns self
self.memo.insert(id, resolved);                               // fresh borrow
```

The measured cost is a second lookup on the miss path. The measured benefit is
17–25% over both interior-mutability styles, *and* no runtime borrow hazard —
the fastest style is also the only one that cannot panic.

### The bit values of `TypeFlags` are load-bearing

Upstream's comment at `types.go:420` is explicit: the numeric values determine
the order `CompareTypes` computes, and therefore **the order of constituents in a
union type**. Union order is printed in every `.types` baseline. So renumbering
these would silently reorder printed unions and fail conformance in a way that
reads like a formatting bug rather than a data-model one. They are ordered by
increasing potential complexity so union processing can bail out early, with
indexed-access and conditional last because those are potentially infinite.

## Distinct types that print the same string

Upstream creates several types that render identically and distinguishes them by
pointer identity:

- `anyType`, `errorType`, `wildcardType`, `blockedStringType`,
  `nonInferrableAnyType`, `autoType`, `intrinsicMarkerType` — all `TypeFlagsAny`.
- `neverType`, `silentNeverType`, `implicitNeverType`, `unreachableNeverType`,
  `uniqueLiteralType` — all `TypeFlagsNever` printing `never`.
- `undefinedType`, `missingType`, `optionalType` — all printing `undefined`.

Merging any pair would be invisible in output and wrong in behaviour:
`errorType` suppresses cascading errors where `anyType` does not, and
`silentNeverType` does the same against `neverType`. `TypeStore::new_intrinsic`
therefore **never interns**, while literal types **always** do — literal identity
is `TypeId` equality, which is what lets a relation check compare `"a"` to `"a"`
without comparing strings.

This is also why every unported expression form yields `errorType` rather than
`anyType`. Both print `any`; only one of them is a claim that the answer *is*
`any`. A gap must never be indistinguishable from a result.

## Printing is under test, not a convenience

A `.types` baseline compares **whole lines, verbatim**, for every file of a case.
So `type_to_string` is the thing being measured, and one wrong character fails a
case exactly as an outright wrong type does. Two places a port drifts:

- **Numbers.** TypeScript prints a numeric literal *type* as `Number::toString`
  of its value, not as written: `1.0`, `1e0` and `0x1` are all the type `1`.
  Source text cannot be used. Narrowing to `f64` is not a defect but the
  specified behaviour — ECMAScript numbers *are* `f64`.
- **Strings.** Double-quoted with TypeScript's escape table, regardless of how
  the literal was quoted in source.

Both have known gaps, recorded in `bd tsr-4sc.1` rather than papered over.

## The oracle is unproven, and that is the live risk

From [checker-oracle.md](checker-oracle.md), and it governs how the first
non-zero number must be treated:

> `checker_types`'s judging path has never executed. All 9,538 cases are
> classified `Unsupported`, so the comparison code has run zero times.

A suite that has only ever reported 0% is as unproven as one reading 100% on its
first run — the failure `file_loader` was caught by, where 76/76 held until four
deliberate mutations were applied to the code under test. **The first time
`checker_types` is non-zero, mutate it before believing it.**

The same discipline applies to this crate's own tests, and already caught one of
them: a test asserting the expression memo worked by counting types passed with
the memo deleted, because interning already prevents a repeated literal from
creating a new type. It was replaced with a computation counter, and *that*
version fails when the memo is removed. Every test in
`crates/tsr-checker/tests/types.rs` has been verified to fail against a
corresponding mutation.
