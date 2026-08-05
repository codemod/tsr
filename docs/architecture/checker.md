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
one of the checker's diagnostics. `bd tsr-4sc.2` is the next slice, and it is
blocked — see below.

## What blocks declaration types

Found 2026-08-05 on starting `bd tsr-4sc.2`, and it is structural rather than a
matter of writing more code. **Nothing maps a `NodeId` back to a typed node.**

A binder `Symbol` holds `value_declaration: Option<NodeId>`. Computing its type
means reading that declaration's annotation and initialiser, which live in the
typed node — `VariableDeclaration<'a>` — while `NodeTable` stores only kind,
span, flags and parent (`crates/tsr-ast/src/lib.rs:159`–`186`). So the checker
can reach a declaration's *position* and not its *contents*. Upstream never meets
this: a Go `*ast.Symbol` holds a real `*ast.Node`, and the question does not
arise. Here it is [ADR-0003](../adr/0003-tree-plus-side-tables.md) meeting
upstream's design.

Three candidate structures were measured over the four benchmark fixtures
(`crates/tsr-binder/examples/node_lookup.rs`, each option in its own process;
419,572 nodes, 41,831 symbols, binder at 15,052 KiB):

| option | entries | exact KiB | RSS Δ KiB | populate | answers `parent(id)`? |
|---|---:|---:|---:|---:|:--:|
| dense `Vec<Option<Node>>` by `NodeId` | 419,565 | 6,555 | 6,656 | 4.02 ms | yes |
| per-symbol, one `Node` per symbol | 36,590 | 653 | 1,488 | 5.55 ms | no |
| sparse `FxHashMap` of declarations | 43,297 | 1,432 | 3,156 | 9.05 ms | no |

Medians of three readings, taken at a load average of 1.89.

**Resolved by [ADR-0032](../adr/0032-reaching-a-typed-node-from-an-id.md): the
dense table, built by the binder.** The decision is capability, not cost — the
other two answer only "the declaration node of this symbol", and
`nodes.parent(id)` returns a `NodeId` that 1,134 sites in upstream's checker need
resolved to a typed node.

Three results were not what the issue predicted, and are worth carrying forward:
the dense table is the **fastest** to populate despite being ten times the size
(it stores by index; the others probe a hash map per node); and the cheap
options' resident cost is 2.2–2.3× their byte counts, because both need a
transient set during the walk, so the real spread is 4.5× rather than 10×.

**A third prediction was wrong and was corrected on implementation.** The ADR
expected the table to ride free on the binder's existing walk. It cannot: the
bind walk reaches 417,837 of 419,565 nodes — 1,728 short, 0.41% — because
`push_children` is generated from `ast.json` and includes token-valued fields
while `bind_children` ports upstream's `bindChildren`, which does not visit them.
So the table is filled by a dedicated pre-pass, `Binder::record_nodes`, and the
extra walk is a real cost rather than the free rider the ADR assumed. The 0.41%
was `case`/`default` keywords and some zero-width `ForOfStatement` nodes.

The accepted price is **+6,656 KiB, +44% on binder memory**, bytes-per-source-byte
7.54 → 8.75.

## Two things upstream does that the issue text got wrong

Both found by reading `checker.go` rather than by trusting the summary, and both
recorded here because the summaries are what the next session will read first.

- **A circularity does not simply become `errorType`.**
  `reportCircularityError` (`checker.go:18822`) returns `errorType` only when the
  declaration has a *type annotation* that references itself; a circular
  *initialiser* reports a different diagnostic and returns **`anyType`**. Those
  print identically and behave differently, which is precisely the trap this
  crate already documents under "distinct types that print the same string".
- **The circularity mechanism is a stack, not a per-symbol flag.**
  `pushTypeResolution`/`popTypeResolution` (`checker.go:18758`) keep a
  `typeResolutions` stack keyed by (entity, property name), and on finding a
  cycle they mark **every frame from the cycle start onward** as failed — so all
  participants in the cycle resolve to an error, not only the symbol that closed
  it. A boolean "currently resolving" flag per symbol does not reproduce that.

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

## The oracle is proved; the producer is not

`checker_types` had no comparison code at all — every case returned
`Unsupported`, so its judging path had executed zero times, which made it exactly
as unproven as a suite reading 100% on its first run. That was closed on
2026-08-05, *before* any checker work was measured with it: `types_suite::compare`
now exists, positional and whole-line, with eight tests each verified against a
deliberately weakened judge. The seven mutations and what each turned red are
tabulated in [checker-oracle.md](checker-oracle.md).

What remains unproven is the **producer**. Nothing renders this crate's types in
`.types` baseline form yet, so `compare` has only ever been fed empty output from
the corpus side. The first non-zero `checker_types` number is therefore still not
evidence until something deliberately wrong has been pushed through the whole
path — checker to renderer to judge — and seen to go red.

## The gradient

`checker_types` now reports two numbers
([ADR-0031](../adr/0031-a-gradient-beside-the-gate.md)). The **case rate** is the
gate and is unchanged: all lines of all files, or the case fails. Beside it is a
**per-assertion-line tally** over the same population — 478,954 lines across the
9,538 judged cases — because a binary gate over 60,269 lines of upstream gives one
bit of feedback per case and would read 0% for months.

The gradient is always the more forgiving of the two, is a strict lower bound (a
missing line costs every line after it, since the comparison is positional), and a
full tally does **not** imply a pass — produce every expected line plus one extra
and `matched == total` while the case is wrong. Never quote it as a pass rate.

The same discipline applies to this crate's own tests, and already caught one of
them: a test asserting the expression memo worked by counting types passed with
the memo deleted, because interning already prevents a repeated literal from
creating a new type. It was replaced with a computation counter, and *that*
version fails when the memo is removed. Every test in
`crates/tsr-checker/tests/types.rs` has been verified to fail against a
corresponding mutation.
