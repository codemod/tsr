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
| **Declaration types** | `getTypeOfSymbol` for variables, parameters and properties (`bd tsr-4sc.2`) |
| **Type nodes** | `getTypeFromTypeNode` for the keyword, literal and parenthesised forms |
| **Literal freshness** | The `freshType`/`regularType` pair, which is what makes `const x = "a"` be `"a"` and `let x = "a"` be `string` |
| **Circularity detection** | The resolution stack, `pushTypeResolution`/`popTypeResolution` |
| **Identifier references** | Resolve the name, take the symbol's type — with no narrowing |

**What does not exist:** object/union/intersection/generic/conditional/
indexed-access types, type references (so `interface I` is unusable as an
annotation), functions, classes, enums, accessors, aliases, destructuring,
assignability, inference, overload resolution, control-flow narrowing, and every
one of the checker's diagnostics.

`checker_types` still reads **0/9,538**, and that is correct: the checker can now
type a declaration, but nothing renders those types in `.types` baseline form.
That producer is `bd tsr-4sc.3`, and it is what will first move the number.

## Reaching a declaration from a symbol — the blocker, now cleared

Found 2026-08-05 on starting `bd tsr-4sc.2`, and structural rather than a matter
of writing more code: **nothing mapped a `NodeId` back to a typed node.**

A binder `Symbol` holds `value_declaration: Option<NodeId>`. Computing its type
means reading that declaration's annotation and initialiser, which live in the
typed node — `VariableDeclaration<'a>` — while `NodeTable` stores only kind,
span, flags and parent (`crates/tsr-ast/src/lib.rs:159`–`186`). So the checker
could reach a declaration's *position* and not its *contents*. Upstream never
meets this: a Go `*ast.Symbol` holds a real `*ast.Node`, and the question does not
arise. Here it is [ADR-0003](../adr/0003-tree-plus-side-tables.md) meeting
upstream's design.

`ParsedSourceFile::node_map` now answers it — `NodeMap::get(id) -> Option<Node>`.

Three candidate structures were measured over the four benchmark fixtures
(`crates/tsr-binder/examples/node_lookup.rs`, each option in its own process;
419,572 nodes, 41,831 symbols):

| option | entries | exact KiB | RSS Δ KiB | populate | answers `parent(id)`? |
|---|---:|---:|---:|---:|:--:|
| dense `Vec<Option<Node>>` by `NodeId` | 419,565 | 6,555 | 6,656 | 4.02 ms | yes |
| per-symbol, one `Node` per symbol | 36,590 | 653 | 1,488 | 5.55 ms | no |
| sparse `FxHashMap` of declarations | 43,297 | 1,432 | 3,156 | 9.05 ms | no |

**Resolved by [ADR-0032](../adr/0032-reaching-a-typed-node-from-an-id.md), then
superseded the same day by
[ADR-0033](../adr/0033-the-parser-fills-the-node-map.md).** The dense shape won
on capability — the other two answer only "the declaration node of this symbol",
while `nodes.parent(id)` returns an id that 1,134 sites in upstream's checker
need resolved to a node. What changed is *who fills it*.

ADR-0032 put the fill in the binder, believing it free there. Two measurements
said otherwise: the bind walk reaches only 417,837 of 419,565 nodes (0.41% short
— `case`/`default` keywords and some zero-width `ForOfStatement`s, because
`push_children` includes token-valued fields and `bindChildren` does not), so it
needed a separate pre-pass; and that pre-pass cost **+16.1%** on `checker.ts`
parse+bind, mostly zeroing a 4.8 MB vector on every bind.

The parser fills it instead (`tsr_ast::NodeMap`, `bd tsr-4sc.5`). `NodeTable`
hands out ids sequentially and the parser allocates the typed node immediately
after, so recording is a `Vec::push` — no zeroed allocation, no second walk, and
coverage complete by construction rather than by argument. Measured against the
binder version: parse+bind **−17.0%** on `checker.ts`, binder memory back to
14,848 KiB, total bytes-per-source-byte 8.73 → 8.68. The cost lands on parse-only
consumers instead: **+7.4%** parse time and +6,400 KiB of AST, worth **+0.9%** on
a whole-corpus conformance run.

Two predictions this repository got wrong along the way, both corrected in the
records rather than quietly: the dense table is the *fastest* to populate despite
being ten times the size (it stores by index; the others probe a hash map per
node), and the cheap options' resident cost is 2.2–2.3× their byte counts because
both need a transient set during the walk — so the real spread is 4.5×, not 10×.

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

## What the declaration slice does *not* do, stated precisely

Three limits worth knowing before reading a `checker_types` failure, because each
produces a plausible-looking wrong answer rather than an obvious gap.

- **No control-flow narrowing.** `checkIdentifier` is ported only as far as
  "resolve the name, take the symbol's type", so a reference yields the
  *declared* type. Upstream reaches `getFlowTypeOfReference` here, so a reference
  to `let x = "a"` narrows to `"a"` where the assignment dominates; ours says
  `string`. The binder builds the flow graph already and nothing reads it.
- **The annotation branch of `reportCircularityError` is unreachable today.**
  A circular *initialiser* returns `anyType` and is tested end to end; a circular
  *type annotation* returns `errorType` and cannot yet be triggered, because the
  type nodes that could close such a cycle — `typeof x`, a type reference — are
  unported and yield `errorType` without recursing. The branch is written because
  it is upstream's behaviour, and it is recorded here as untested rather than
  presented as working.
- **The symbol-flags dispatch is currently redundant.** `getTypeOfSymbol` tests
  `SymbolFlags::VARIABLE | PROPERTY` before taking the variable path, exactly as
  upstream does. For every symbol shape this slice reaches, deleting that test
  changes nothing — a non-variable symbol's declaration kind is rejected by the
  worker's match and lands on the same `errorType`. Mutating it turns no test
  red, and that is stated rather than papered over with a test that would not
  bite.

## A correction the tests forced

The first version of `a_reference_takes_the_type_of_what_it_resolves_to`
asserted that `const a = "x"; let b = a;` gives `b : "x"`, on the assumption that
a *reference* could not carry a fresh literal type. It gives `string`, and the
implementation was right.

**Freshness propagates through a `const`.**
`getWidenedLiteralTypeForInitializer` (`checker.go:16897`) returns the
initialiser's type *unchanged* when the declaration is constant — and that type
is the fresh literal `checkExpression` produced. So `a` holds a fresh `"x"`, and
the `let` still has something to widen. The contrast that pins it is
`let a: "x" = "x"; let b = a;`, where `a`'s type came from a type *node* and is
therefore regular, so `b` stays `"x"`.

That contrast is also the only thing in the suite that distinguishes freshness at
all: deleting the freshness check from `getWidenedLiteralType` turned **no test
red** until that case was written, because every other path either starts from a
fresh literal or returns an annotation without widening.

## The order the rest is built in, and why

Ranked 2026-08-05 by impact against effort and feasibility, with dependencies
respected. The evidence is the distribution of **answer shapes** across all
512,800 assertion lines in the corpus baselines:

| upstream's answer | share | do we produce it? |
|---|---:|---|
| intrinsic (`string`, `number`, `any`…) | 35.30% | partly |
| literal (`"a"`, `1`, `true`) | 18.86% | yes |
| named type reference (`C`, `M.I`) | 14.25% | no |
| function/signature (`() => void`) | 12.96% | no |
| object literal type | 4.83% | no |
| `typeof X` | 3.44% | no |
| union | 3.33% | no |
| generic reference | 3.03% | no |
| array | 2.47% | no |

**Read that table as an upper bound per feature, not a work estimate.** A bucket
is the shape of the *answer*, not the feature needed to compute it: `f()` →
`string` is an intrinsic answer that requires full call resolution.

**The most important open question is in the first two rows.** We nominally cover
intrinsic + literal = 54.16% and the gradient reads 22.39%, so we are getting
under half of what we supposedly support, and nothing currently explains it.

1. **`bd tsr-4sc.6` — bucket the failures by answer shape.** Cheap; the walker,
   the checker wiring and the comparison all exist. It answers the 54%→22%
   question and decides whether lib files or object types are the bigger prize.
   *Nothing below should start before this reports.*
2. **`bd tsr-4sc.1` — printing gaps.** Number boundaries at 1e21/1e-6 and the
   escape table. Literals are 18.86% of lines and we *claim* them, so a printing
   bug fails them silently and gets misattributed to the checker.
3. **`bd tsr-9or.1` — lib files.** Without `lib.d.ts` every reference to a global
   is `errorType`. High feasibility: `module_resolution` and `file_loader` are
   both at 100%, so the machinery exists.
4. **`bd tsr-4sc.7` — object types and `getDeclaredTypeOfSymbol`.** ~19% directly
   and it gates 5, 6 and 7.
5. **`bd tsr-4sc.8` — signatures and function types.** 12.96%. Needs 4, because
   upstream models a function type as an object type with call signatures.
6. **`bd tsr-4sc.9` — unions**, which is also what lets `boolean` stop being a
   fake intrinsic. Reach beyond its 3.33%, because it changes how `boolean`
   prints everywhere.
7. **`bd tsr-4sc.10` — `typeof` queries.** 3.44%, nearly free once 4 and 5 land.
8. **`bd tsr-4sc.11` — control-flow narrowing.** Adds no bucket; fixes the
   *correctness* of reference lines across all of them, so its true impact is
   probably larger than this rank. Step 1 will say.
9. **`bd tsr-bb4.1` — per-configuration runs.** Returns 1,397 cases to
   `checker_types`. Worth doing when the rate is high enough that the denominator
   matters; doing it now only makes the number look worse for no information.

Not in the ranking on purpose: **`bd tsr-el3.2`** (upstream's algorithmic
recursion limits) is a standing constraint rather than a task — port each limit
*with* the code it belongs to. And **`bd tsr-pum.11`** (~2,675 parser
over-reports) is 9 points of the `diagnostics` ceiling but ~300 separate
investigations with no dominant cause, which scores poorly until the checker
emits diagnostics at all.

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
