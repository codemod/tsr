# The checker's oracle (`checker_types`)

**Status:** the instrument exists; the subject does not. `checker_types` reports
**0/9,538** and will until `bd tsr-4sc` computes a type.

**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.

## Why this was built before the checker

`internal/checker` is 60,269 lines — a third of the core port, and 120–200
sessions by PLAN.md's estimate. Starting that without a gate means months of work
before the first honest number.

Nothing in this repository currently measures a type. `binder_symbols` reads
97.98% and that is *symbol tables* — name resolution, declaration merging, scope
walking. A checker can be arbitrarily wrong while every symbol resolves.

This repository keeps relearning that the instrument comes first:
`dts_reachable_target` sized the emitter's target before an emitter existed, and
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) committed to a
falsifier that could fire on the measurement alone. The same applies here at ten
times the scale.

## The target

Measured at the pin across `compiler/` and `conformance/`:

| | |
|---|---|
| `.types` baselines | **12,155** |
| positioned type assertions in them | **594,122** |
| cases this suite judges | **9,538** |

Upstream writes `.types` and `.symbols` in lockstep — 12,155 of each, at the same
positions. That pairing is worth knowing when the checker lands: `.symbols` tests
*resolution* and `.types` tests *inference*, so a failure in one localises against
the other.

The 12,155 baselines and 9,538 judged cases differ for the reasons the suite
prints, and the difference is files-versus-cases as much as exclusions — the same
reconciliation as the `.d.ts` denominator:

```
 1397  configuration-varied baseline (bd tsr-bb4.1)
  924  upstream recorded no .types baseline
  450  upstream records a known divergence from TypeScript
  135  the .types baseline has no assertions
```

## What a pass will mean

Every `>expression : type` line reproduced **verbatim, in order, for every file of
the case**.

Whole-line, not expression-and-type separately. A line is
`>{expression} : {type}` and **the expression can contain `" : "`** — a
conditional does:

```text
>Math.random() > 0.5 ? "abc" : "def" : "abc" | "def"
```

773 assertion lines in `compiler/` alone carry two or more, so there is no
unambiguous split. Rendering the line ourselves and comparing strings sidesteps
that and is the stricter check. `TypeAssertion::split` exists only to make a
failure readable, and its test demonstrates it getting that line wrong.

The first draft of this claimed the ambiguity came from type annotations and used
`>function (this: any) { } : (this: any) => void`. That is not ambiguous —
`this: any` has no space before the colon, so it never looks like the separator.
The unit test asserting the wrong split failed, which is the only reason the real
shape was looked up. Both forms are now tested.

## The suite reports 0%, deliberately

[conventions.md](../conventions.md): *a conformance suite whose subject does not
exist reports 0%, loudly, with the reason stated. It does not skip, and it does
not omit the row.* Every judged case comes back `Unsupported { "no checker
(bd tsr-4sc)" }`, so the row is in the summary table from today and the number
moves the first time a type is computed.

## A skip reason that was wrong in two suites

`checker_types` first checked for a plain `.types` file before checking whether
the case was configuration-varied. A varied case has **no** plain baseline — only
`case(target=es5).types` — so all 1,397 of them landed in the "no baseline" bucket:
the right exclusion under a label that said something else, which is how a skip
count stops meaning anything.

`binder_symbols` had the identical flaw and had had it since it was written. It
was never checking for varied `.symbols` at all.

**Its pass rate was not affected**, and that was checked rather than assumed: no
case in the corpus has both a plain and a varied baseline, so no case was ever
judged against an arbitrary configuration. 8,278/8,449 before the fix and after.
What was wrong was the breakdown — 2,321 cases reported as "upstream recorded no
baseline" when 1,397 of them were varied, which overstates how much of the corpus
upstream never ran.

Both suites now test varied first, and their skip lists are directly comparable.

## What is still missing for Phase 4

This gates *types*. Two other oracles the checker needs, neither built:

- **Full `.errors.txt` comparison.** `isolated_declarations` compares only the
  `9000..9100` range; every other diagnostic the checker produces is unmeasured.
  That is the larger half of a checker's conformance.
- **Per-configuration runs** (`bd tsr-bb4.1`), which would return 1,397 cases here
  and comparable numbers to the parser and binder suites.
