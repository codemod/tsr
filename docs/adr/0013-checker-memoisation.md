# ADR-0013: The checker computes through `&mut self` and returns ids

- **Status:** accepted
- **Date:** 2026-08-03
- **Settles:** `bd tsr-6n3` (SPIKE: checker memoization vs the borrow checker)
- **Related:** [ADR-0003](0003-tree-plus-side-tables.md) (side tables),
  [ADR-0011](0011-unsafe-is-opt-in.md) (which constrained the options),
  [ADR-0012](0012-ast-is-sync.md) (which this must not undo)

## The forcing constraint

Every lazily-computed value in typescript-go's checker has one shape
(`internal/checker/checker.go:16544`):

```go
func (c *Checker) getTypeOfVariableOrParameterOrProperty(symbol *ast.Symbol) *Type {
    links := c.valueSymbolLinks.Get(symbol)   // 1. mutable borrow of a side table
    if links.resolvedType == nil {
        t := c.getTypeOfVariableOrParameterOrPropertyWorker(symbol)  // 2. recurses into c
        links.resolvedType = t                // 3. writes through the borrow from (1)
        return t
    }
    return links.resolvedType
}
```

Steps 1–3 are a borrow of `self`'s field held across a call that needs `self`
again. Rust rejects it, there is no local workaround, and the checker does this in
dozens of places across six `LinkStore`s (`checker.go:668–673`). Whatever replaces
it is the calling convention for ~60k lines, so it has to be settled by working
code rather than by preference — and there is no prior art: `oxc_type_checker` is a
144-line no-op and oxc's type conformance is 0.93%.

## The decision

**Checker methods take `&mut self` and return `TypeId`. No method hands out a
reference into checker state.** A type's contents are reached by asking the
checker again.

The Go pattern becomes read, drop, recurse, write:

```rust
if let Some(cached) = self.memo[symbol.index()] { return cached; }  // borrow ends here
if self.resolving.contains(&symbol) { return self.intern(Type::Circular); }
self.resolving.push(symbol);
let resolved = self.compute(symbol);   // recursion owns `self` outright
self.resolving.pop();
self.memo[symbol.index()] = Some(resolved);
```

Because `TypeId` is `Copy`, nothing survives into the recursion, and the whole
subsystem is ordinary safe Rust with no interior mutability at all.

## The evidence

Three styles were built as complete implementations over the same vertical slice —
memoisation, mutual recursion, circularity detection, and union interning — in
`crates/tsr-checker-spike`. All three are checked to produce identical results on
five program shapes, and to compute each symbol exactly once.

| Style | Issue's label | Shape |
|---|---|---|
| `ids` | (b) | `&mut self`, `TypeId` everywhere |
| `arena` | (a)/(c) | `&self`, types in the bump arena, memo behind `RefCell` |
| `cells` | (c) refined | as `arena`, memo as `Cell<Option<&Type>>` |

Resolution time, one core, median of repeated runs:

| Program | Symbols | `ids` | `arena` | `cells` |
|---|---:|---:|---:|---:|
| wide | 2,000 | **38.8 µs** | 50.4 µs | 44.5 µs |
| wide | 20,000 | **413 µs** | 467 µs | 442 µs |
| chain (deep aliases) | 2,000 | **10.2 µs** | 12.4 µs | 11.0 µs |
| cycle in the middle | 20,000 | **423 µs** | 473 µs | 452 µs |
| diamond | 42 | 2.04 µs | 1.88 µs | **1.84 µs** |

**`ids` is 7–23% faster on every realistic shape**, losing only on a 42-symbol
program where the whole run is microseconds. The likely reason is mundane: `ids`
holds types in one contiguous `Vec`, while the arena styles scatter them and add a
pointer hop per member.

### An earlier version of this benchmark was wrong

It timed resolution *plus* building a structural description of every result. The
description is identical in all three styles and large enough to flatten every
difference to under 4% — from which the first reading was "they are all the same,
choose on ergonomics", with `cells` marginally ahead. Removing the shared work
reversed the ranking and roughly quintupled the spread. A benchmark that includes
enough common work will report that any two things are equivalent.

## Why the fastest style is also the safest

This decision would be harder if it were a trade-off. It is not.

The `arena` and `cells` styles need `&self` methods and therefore interior
mutability, which moves a real invariant from compile time to run time: a `RefCell`
borrow held across a recursive call panics. `crates/tsr-checker-spike/tests/hazards.rs`
demonstrates it — the bad version compiles cleanly, passes a test that only
exercises the base case, and panics only once the input recurses. In a subsystem
that recurses through dozens of mutually-referential functions, that is a bug class
with no compile-time defence and poor test visibility.

`cells` narrows the hazard by making the memo table `Cell`-guarded, which is
genuinely better — `Cell::get` copies out, so there is no borrow to hold. But the
intern map is a `HashMap` and cannot be a `Cell`, so the hazard is reduced rather
than removed, and the resulting rule ("memo tables may be `Cell`, everything else
must be handled carefully") is exactly the kind of distinction that erodes.

`ids` has no such rule because it has no interior mutability.

## What it costs

Stated plainly, because the cost is real and will be felt on every one of those
60k lines:

- **A second lookup on the miss path.** Read the memo, miss, compute, write. The
  benchmark includes this and `ids` still wins.
- **Type contents are only reachable through the checker.** Where Go writes
  `t.Target.Symbol`, we write `self.type_data(t).target()` and then ask again.
  Chained field access becomes chained method calls.
- **No long-lived `&Type`.** The `arena` style can hold a resolved `&'a Type`
  across further resolution; `ids` cannot. This is the one thing the rejected
  styles genuinely do better, and where the port will feel most awkward.
- **Type data has to be cheap to copy or read behind an index.** That constrains
  the eventual `Type` representation, and it constrains it *before* the type
  representation is designed, which is the point of settling this now.

## Consequences for parallelism

`ids` keeps [ADR-0012](0012-ast-is-sync.md) intact and does not depend on it. The
checker is `Send` and not `Sync`, which matches upstream: `checkerpool.go` runs N
independent `Checker` instances (default 4) over a partitioned file set, each with
its own state and its own arena. Nothing here needs a shared checker.

The requirement this pushes onto the binder is that its output — the symbol table
and node→symbol mapping — must be immutable once built, since every checker reads
it concurrently. That is a side table per [ADR-0003](0003-tree-plus-side-tables.md),
and it is worth noting that we cannot follow upstream here even if we wanted to:
typescript-go stores `Symbol` on the node (`ast.go:239`), and we cannot, because a
mutable field on a shared node is what ADR-0012 rules out.

## How we would know this was wrong

- **The "ask the checker again" convention becomes unbearable at scale.** The
  slice is a few hundred lines; the checker is 60k. If the indirection compounds
  into unreadable code, that shows up early — within the first real subsystem —
  and the answer is to revisit with that code as evidence rather than to soldier on.
- **Real type representations invert the benchmark.** The slice's `Type` is small.
  If the real one is large enough that copying or re-indexing dominates, the arena
  styles' ability to hold a reference starts to pay. Re-run
  `cargo bench -p tsr-checker-spike` against a representative `Type` before
  concluding either way.
- **Deferred resolution needs a reference held across a call.** Upstream has
  patterns (`getTypeOfSymbolWithDeferredType`) not modelled here. If one of them
  genuinely cannot be expressed with ids, that is a concrete counter-example and
  this ADR should be superseded rather than stretched.

The spike crate stays in the tree for exactly that purpose: it is the harness these
questions get re-asked with, not scaffolding to delete once the checker starts.
