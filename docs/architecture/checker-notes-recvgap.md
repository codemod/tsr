# What actually gaps the receiver

Measured at `7602d6b` by `crates/tsr-conformance/examples/receiver_gap.rs`,
which routes through `types_producer::assertions_for_case_with_ids` and so
measures the same lib-loaded compiler the `checker_types` gradient does.
Upstream anchors below are at the pinned submodule commit `5b1047d10`, each
taken from `grep -n` on the declaration.

Reproduce:

```
cargo run --release -p tsr-conformance --example receiver_gap
```

## 0. The claim this page was written to test

`bd tsr-6ph` says a type for the module object is

> **the only route to** the 1,590 lines in the two receiver-is-a-gap rows
> (`member name` 736 + 60, `property access` 734 + 60), because `ns.foo` needs
> the namespace's type and not the alias's target.

Two separate questions hide in that sentence, and they have different answers.

## 1. The rows are 19,329 lines, not 1,590

`types_producer::access_reason` emits `the receiver is a gap: {kind}` and is
reachable from exactly two call sites — the member name of an `a.b`
(`types_producer.rs:1095`) and the access expression itself (`:1150`). Counting
every gap line that carries the phrase:

| row | lines | cases | top-1 | top-10 |
|---|---:|---:|---:|---:|
| `member name, the receiver is a gap` | 9,652 | 1,298 | 12.7% | 52.9% |
| `property access, the receiver is a gap` | 9,677 | 1,306 | 12.6% | 52.8% |
| **total** | **19,329** | | | |

Top case in both: `compiler/temporal` at 1,222 lines, then
`conformance/parserRealSource11` 1,146 and `parserRealSource10` 797.

**1,590 is not the size of these rows.** It is `module_blocked`'s *seam-only*
subset of them — the lines whose blockers are **all** the cross-file alias seam.
The full rows are 12.2× larger. Anyone reading `tsr-6ph`'s sentence as "these
rows are 1,590 lines and modules are the only route to them" has the population
wrong by an order of magnitude, and this page exists so that reading stops.

The two rows being within 0.26% of each other is not a coincidence and is worth
carrying: `a.b` renders **two** assertion lines, the access and its member name,
and both take the same receiver. So a converted access converts two lines. That
pairing is structural, and it is the reason a single-line-per-access estimate of
this work would be half of what it should be.

## 2. Walking each line to the gap that actually stops it

For each such line the probe takes the access, takes its receiver, and asks
`gap_reason` **about the receiver** — descending again while the measured answer
is itself `the receiver is a gap` (`a.b.c`). The descent is driven by the
measured reason at every step, never assumed. Chain depth:

| depth | 0 (`x.foo`) | 1 | 2 | 3 | 4 | 5 | 6 |
|---|---:|---:|---:|---:|---:|---:|---:|
| lines | 17,069 | 1,952 | 258 | 28 | 12 | 8 | 2 |

88.3% are one hop. The terminal receiver is then classified by what declares it.

| what gaps the receiver | member | access | lines | share | cases | top-10 |
|---|---:|---:|---:|---:|---:|---:|
| `import * as ns` | 172 | 172 | 344 | 1.8% | 81 | 50.6% |
| `import a = …` | 618 | 618 | 1,236 | 6.4% | 156 | 58.1% |
| `export * as ns` | 19 | 19 | 38 | 0.2% | 12 | 89.5% |
| other import alias (an export symbol, not a module object) | 81 | 81 | 162 | 0.8% | 52 | 39.5% |
| local `namespace N {}` (same file) | 13 | 13 | 26 | 0.1% | 9 | 100.0% |
| a local declaration whose own type gaps | 5,050 | 5,062 | 10,112 | **52.3%** | 541 | 64.7% |
| the name does not resolve | 1,808 | 1,811 | 3,619 | 18.7% | 110 | 87.9% |
| the receiver is not a name (call, `this`, `super`, …) | 1,891 | 1,901 | 3,792 | 19.6% | 562 | 22.5% |
| **total** | | | **19,329** | | | |

**Module-object forms root 1,618 lines — 8.4%. Everything else is 17,711, 91.6%.**

### Controls, printed unconditionally

Three are pinned by construction, which is what lets them see a semantic
inversion that leaves every sum intact (`docs/conventions.md`).

| control | reads | pinned by |
|---|---:|---|
| C1 the phrase under a third row prefix | **0** | `access_reason` has two call sites |
| C2 terminal reason still says receiver-is-a-gap | **0** | the descent only leaves a `PropertyAccessExpression`, and no other kind reaches `access_reason` |
| C3 `import * as ns` root with no module specifier | **0** | the grammar: a namespace import is a clause of an `ImportDeclaration` |
| C3′ the same roots **with** one (the mirror) | 344 | — |
| A1 rows == partition | 19,329 == 19,329, difference 0 | arithmetic |

C3 and C3′ are printed as a pair on purpose: a classifier that had collapsed the
import forms together would still give C3 = 0, and only the mirror distinguishes
"the arm is right" from "the arm never fires".

## 3. So does the "only route" claim survive?

**As a claim about the 1,590 seam-only lines: yes, and it is now corroborated by
a second instrument.** `module_blocked` reached 1,590 by following each gap
line's blockers to their leaves; this probe reached **1,618** by walking the
receiver chain to its root declaration and reading that declaration's kind.
Different code paths, different authors, **agreement to 1.8%**. That is the
strongest evidence form available here — not a control reading zero, but two
things that could have disagreed and did not.

A third, independent cross-check falls out of the terminal-reason histogram:
`reference, symbol has no type: SymbolFlags(ALIAS) / no value declaration` holds
**1,772** lines, against 1,780 for the four alias buckets summed
(344 + 1,236 + 38 + 162). The 8-line difference is symbols carrying `ALIAS`
alongside another declaration.

**As a claim about the rows: no.** "The only route to the receiver-is-a-gap rows"
is true of 8.4% of them. The other 91.6% are blocked on things a module-object
type cannot touch, and the largest single blocker is not module-shaped at all.

The correction is small and it matters: `tsr-6ph`'s **number** is sound, its
**scope sentence** is not. Fix the sentence, keep the number.

## 4. What the other 17,711 are actually waiting on

From the terminal `gap_reason` histogram, the top blockers behind the
non-module 91.6%:

| terminal reason | lines | cases | top-10 |
|---|---:|---:|---:|
| `reference, the name does not resolve` | 3,619 | 110 | 87.9% |
| `BLOCK_SCOPED_VARIABLE / VariableDeclaration / initialiser CallExpression` | 1,124 | 44 | 90.2% |
| `SymbolFlags(EXPORT_VALUE) / no value declaration` | 1,120 | 44 | 93.2% |
| `expression answered error: CallExpression` | 791 | 144 | 44.0% |
| `expression answered error: ThisKeyword` | 670 | 108 | 44.5% |
| `FUNCTION_SCOPED_VARIABLE / VariableDeclaration / initialiser NewExpression` | 526 | 60 | 68.4% |
| `property access, the property has no type` | 490 | 40 | 80.4% |
| `expression answered error: SuperKeyword` | 412 | 73 | 45.6% |
| `Parameter / annotation TypeReference unresolved: …` | 678 + 450 + 392 + 280 | 1–5 each | 100% |

Read as work items rather than as rows, that is roughly:

- **call and `new` resolution** — the `initialiser CallExpression` and
  `initialiser NewExpression` families, plus the bare
  `expression answered error: CallExpression`, together ≈ 3,200 lines;
- **`this` and `super`** — 1,082 lines, and both are *positions* this port does
  not type at all rather than lookups that failed;
- **unresolved annotations in a handful of very large cases** — the
  `TypeReference unresolved: Emitter / TypeCollectionContext / ControlFlowContext
  / TypeFlow` rows are each one or five cases at 100% top-10 concentration, all
  of them `parserRealSource*`, which is one hand-written compiler source file
  checked in as a test.

That last bullet is the concentration warning this project keeps needing.
`compiler/temporal` alone contributes 1,374 of the 3,619 `name does not resolve`
lines (38%), all of them `typeof Temporal`, and the whole `name does not resolve`
bucket is 87.9% top-10. **These are not 3,619 independent gaps; they are a
handful of files.** A row this concentrated converts as a step function, not
proportionally, and should be sized by naming the case rather than by quoting the
line count.

## 5. Can we spell the answers? Where the ceiling actually lands

`bd tsr-4jk` measured that upstream prints the **local alias** for a namespace
import — `>ns : typeof ns` — never the module, and a module symbol's name in this
port is the stripped file path, so a naive implementation prints `typeof /0`: a
gap converted into a *wrong* line. The probe prints the baseline's own
right-hand side for each terminal receiver, and it reproduces that finding
exactly. For `import * as ns` roots: `typeof PropTypes` 48, `typeof stuff` 36,
`typeof React` 28, `typeof keys` 24 — always the local name. For `import a = …`:
`typeof exporter` 240, **`any` 162**, `typeof React` 110.

**But that ceiling binds a different line than these rows.** The receiver's own
assertion line is in the *alias* rows, and it is the one that must print
`typeof ns`. The lines counted here print the **member's** type — `ns.foo` is
rendered as whatever `foo` is — and upstream reaches it through
`getPropertyOfType` (`checker.go:18887`) and `getTypeOfPropertyOfType`
(`checker.go:18951`) on the module object produced by
`resolveExternalModuleSymbol` (`checker.go:15556`), from
`checkPropertyAccessExpression` (`checker.go:11244`). The module object has to be
*constructed and looked up in*; it never has to be *printed*.

So the two halves come apart, and this is the load-bearing consequence for
`tsr-6ph`: **a module-object type that cannot be spelled can still convert these
1,618 lines**, because they never render it. Only the receiver line does. If
`tsr-4jk`'s spelling problem turns out to be unsolvable, the right move is to
build the type, use it for lookup, and leave the receiver line gapping — which
converts 1,618 lines and manufactures **zero** wrong ones. That option was not
visible while the two rows were being treated as one item.

The 162 `any` receivers under `import a = …` are the opposite warning: upstream
itself answers `any` there, so those lines are already answerable and are not
evidence for the module-object type.

## 6. What was deliberately not built

No arm was added to `members.rs` or `indexed.rs`. Nothing in this page implies
one: the module-object half belongs to `tsr-6ph` and the agent settling
`symbols.rs`/`resolution.rs`, and the 91.6% majority is call resolution, `this`
and `super`, and annotation resolution — none of which live in these two files.

In particular the `any`-receiver arm in `members.rs` (upstream-faithful,
`checker.go:11318`, landed at a measured +555 wrong lines against 894 gaps
closed) was **not** touched, and neither was its identity-vs-flag guard —
`object_type == self.intrinsics.any` by identity rather than `TypeFlags::ANY`,
because `errorType` also carries `ANY`. See
`docs/architecture/checker-notes-arrays.md`. Note from this measurement what
that guard is protecting: 143,509 gap lines are in the gradient, and a flag test
would answer `any` for every receiver among them.

## 7. How you would know this page is wrong

- **The partition is a list, not a partition.** A1 would print a non-zero
  difference. It prints 0 over 19,329.
- **The descent stops early**, attributing an outer access to its inner one's
  cause. C2 would be non-zero; it is 0, and the depth histogram would collapse to
  depth 0 rather than showing a 1,952 / 258 / 28 / 12 / 8 / 2 tail.
- **The two instruments agree by construction rather than by measurement.**
  They do not share code: `module_blocked` walks blockers through reason strings
  and line positions, this probe walks the receiver chain through the AST and
  reads `Symbol::declarations`. If the 1.8% became 0.0% after a refactor that
  made one call the other, the corroboration in §3 would have to be withdrawn.
- **§5's claim that these lines never print the module object.** The falsifier is
  a baseline line under one of these rows whose right-hand side is `typeof <a
  module>`; the probe already prints the receiver's RHS per bucket and would show
  it.

## 8. Reconciliation caveat

This probe re-derives the gradient as **479,060 lines = 292,217 right (61.00%) +
143,509 gap (29.96%) + 43,334 wrong (9.05%)**, against the committed
`checker_types` snapshot's **61.09%**. The 0.09pp difference is **not
explained**. It is small and it does not move any share on this page — every
figure here is a share of the two rows, whose denominator this probe computes
itself — but the absolute right-count above should not be quoted as the
gradient's until someone reconciles it.
