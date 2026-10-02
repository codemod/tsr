# Agreeing generic overload returns still select by applicability

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Unit: tsr-6.1 (union and
overload inference over recursive generic signatures). Baseline: bb2d3a59 plus
552f16f3.

## The shortcut and why it was wrong

`choose_overload`'s §463 arm handles a set where two or more arity survivors
are all generic and their declared returns print identically — the
`Promise.resolve` shape, whose survivors both return `Promise<Awaited<T>>`. It
took the first survivor on the reasoning that the pick cannot change the
printed answer. A prior unit already routed context-sensitive calls through the
transcribed candidate walk because equal returns do not imply equal callback
contexts.

Equal return *spellings* do not imply equal *inferences* either. The
promisePermutations families declare

```ts
then<U>(success?: (value: T) => Promise<U>, ...): Promise<U>;
then<U>(success?: (value: T) => U, ...): Promise<U>;
```

Both print `Promise<U>`, but for an `() => IPromise<number>` argument the first
infers `U = number` and the second `U = IPromise<number>`. Native
`chooseOverload` (checker.go:9040) infers, instantiates and checks applicability
per candidate; `IPromise<number>` lacks `catch` and `[Symbol.toStringTag]`, so
the first candidate is rejected and the answer is `Promise<IPromise<number>>`.
The shortcut answered `Promise<number>`.

## The change

The §463 arm now asks `transcribed_generic_set_walk` first for every call
without written type arguments, not only context-sensitive ones. The walk is
Kleene-honest: any undecidable inference or relation returns `None`, and the
arm then keeps its previous return-agreement recovery. So a previously correct
answer can only change where the walk decides definitely.

## Alternatives

Comparing instantiated returns instead of declared spellings would still need
per-candidate inference, which is the walk. Restricting the walk to
Promise-like receivers would encode the witness rather than the rule. Removing
the recovery entirely would turn every undecidable set into a gap.

## Verification

Full scorepair against bb2d3a59 + 552f16f3: +109 assertions (51
promisePermutations, 51 promisePermutations3, 4 variadicTuples1, 2
underscoreTest1, 1 arrayFrom), all WRONG-to-RIGHT, zero RIGHT losses, zero
GAP-to-WRONG and zero changed already-WRONG rows. A control in
`crates/tsr-conformance/tests/structured_overloads.rs` was checked against
pinned tsgo.

promisePermutations2 and the mixed Promise/legacy union inference recorded in
checker-95-union-continuation.md remain open under tsr-6.1.
