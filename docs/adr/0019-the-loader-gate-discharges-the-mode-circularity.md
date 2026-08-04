# ADR-0019: the file loader is judged on the requests it makes, mode included — which discharges ADR-0018's circularity

- **Status:** accepted
- **Date:** 2026-08-04
- **Relates to:** [ADR-0018](0018-splitting-the-resolution-oracle.md) (whose
  second half this is, and whose falsifiers this tests),
  [ADR-0017](0017-program-before-tsconfig.md),
  [ADR-0003](0003-tree-plus-side-tables.md) (why a specifier carries its context
  rather than a parent pointer)
- **Scope:** what the `file_loader` conformance suite claims, what the loader
  deliberately does not do, and what changed about the meaning of
  `module_resolution`'s 75

## The forcing constraint

ADR-0018 split the 146 `.trace.json` baselines along a seam: the resolver owns
everything *between* the `======== Resolving … ========` headers, the file loader
owns the headers and their order. It measured the first half at 75/75 and left
the second half unbuilt, with three named ways of being wrong:

1. *"The loader lands and the header sequence disagrees for cases this suite
   passes at 100%."* Then the decomposition was wrong, because the resolver's
   `package.json` cache is observable and depends on the order requests arrive.
2. *"The mode turns out to be resolver-derived after all."* `module_resolution`
   reads each resolution's mode out of the baseline's own
   `Resolving in {0} mode with conditions {1}.` line, because a file's format is
   program-level knowledge. If that knowledge belonged to `tsr-module` rather
   than to the loader, the suite was masking a component under test.
3. *"100% is reached and the resolver is still wrong in the field."*

This slice builds the loader, so (1) and (2) are now testable. (3) is not, and
still waits on `tsr-dts`.

## The decision

**The `file_loader` suite runs the real loader over the case and compares the
lines that describe a *request* — the opening header and the mode line — and
nothing else.**

```text
======== Resolving module 'foo' from '/a.ts'. ========      <- compared
Module resolution kind is not specified, using 'Bundler'.   <- not compared
Resolving in CJS mode with conditions 'import', 'types'.    <- compared
File '/package.json' does not exist.                        <- not compared
======== Module name 'foo' was not resolved. ========        <- not compared
```

The closing header is a *result*, and `module_resolution` owns it. Everything
between is the resolver's walk, and `module_resolution` owns that too. What is
left is exactly the loader's output: a specifier, a containing file, an order,
and a mode.

Including the mode line is the substantive part. Nothing is supplied from the
baseline here: `getImpliedNodeFormatForFile`, the nearest-`package.json` `type`
lookup, and `getModeForUsageLocation` all run for real, and their answer is
compared against upstream's. **This is the falsifier ADR-0018 asked for**, and it
is now discharged: 156 requests across 76 cases, every one of them at the
specifier, containing file, order, and mode upstream recorded.

**Result: 76/76, 100.00% — on the first run.** Which is also the answer to
falsifier (1): the header sequence does *not* disagree, so "resolver correct,
loader separate" was the right decomposition.

## Why a first-run 100% was checked rather than believed

A green number that arrives without a single red one is the least trustworthy
kind. Four mutations were applied to code the suite is supposed to be judging,
each re-measured against the full corpus:

| Mutation | Rate |
|---|---|
| `getModeForUsageLocation` always returns an unspecified mode | 39/76 |
| the replay emits module resolutions before type resolutions | 71/76 |
| every unit is a root file (drop the harness's last-unit heuristic) | 66/76 |
| `require()` is followed in TypeScript files, not only JavaScript | 74/76 |

The suite also compares against 263 request-describing lines in total, and three
cases whose expected request list is *empty* — checked to load their root file
and request nothing, not to have quietly loaded nothing at all.

## What the 75 now means

`module_resolution` did not move, and it could not have: it replays requests
taken from the baseline, so a loader cannot change it. What changed is the
claim behind it. Before, "the resolver takes the same steps upstream did, given
requests we did not derive and a mode we read out of the answer." Now, next to
it, sits an independent measurement that the requests and the mode are the ones
upstream made. **Neither number is a rate that moved; together they cover a
trace the baselines record end to end.**

The one asymmetry, stated so it is not read as a regression: `file_loader`
judges **76** where `module_resolution` judges **75**. It gains the three
empty-trace cases and loses two `libReplacement` cases (bd tsr-9or.5), which the
loader cannot run at all. A suite whose denominator differs from its neighbour's
for a reason nobody wrote down is how a corpus quietly shrinks.

## The alternatives, taken seriously

**Feed the loader's requests into `module_resolution` and judge the whole trace
against the baseline, byte for byte.** This is what upstream's own test does and
it is where this ends up. Rejected for this slice for exactly ADR-0018's reason,
which the mutation table above vindicates: the "always CommonJS mode" mutation
failed 37 cases, and in a fused suite each of those would have surfaced as a
mismatch somewhere in a 90-line transcript with no signal as to which component
put it there. Two suites, two localisable failures.

*This wins when* both suites have been at 100% long enough that the marginal
value of localisation drops below the cost of maintaining a second harness — or
when a trace turns out to have a third author. Neither is true yet.

**Judge the header sequence only, and leave the mode alone.** Cheaper, and it
is literally what ADR-0018 promised ("the headers, in order"). Rejected: the
mode is the single most load-bearing thing the loader computes — it decides
which `exports` conditions apply, which is most of what the `node16`/`nodenext`
corpus is about — and leaving it out would have left ADR-0018's second falsifier
untested while appearing to close it.

**Port `filesParser`'s concurrency along with its shape.** Upstream loads files
on a work group, keys task data by path *and* by file-name casing, and re-walks
a task reached at a lower depth than before. Rejected as premature: see below.

## Consequences accepted

Each of these is a way the header sequence could be wrong, so each is named:

- **Loading is single-threaded and claims each path once.** Upstream's per-casing
  task map (one path reached as `A.ts` and `a.ts` loads twice) and its
  lowest-depth reprocessing are both absent. *How you would know:* a case that
  reaches one file under two spellings would produce one set of resolutions where
  upstream produces two; a file first reached inside `node_modules` and later
  from a root would keep the elided-at-depth answer. Neither occurs in the 76.
- **Lib files are not loaded at all.** A lib task resolves nothing — upstream
  gives it a fixed `CommonJS` metadata specifically to skip the `package.json`
  lookup — so it contributes no trace. The exception is `libReplacement`, which
  *does* resolve, and which is skipped by name (bd tsr-9or.5).
- **`importHelpers` synthesises no `tslib` import.** `Resolving module 'tslib'`
  appears zero times across all 146 baselines, so building it would be untested
  code. If a baseline ever grows one, this suite fails rather than passes
  vacuously.
- **`moduleDetection` is assumed `auto`.** It is not a ported option. A case
  setting `legacy` or `force` would misclassify whether a file is an external
  module, which changes whether a `declare module "x"` inside it is an
  augmentation (resolved) or an ambient declaration (not).
- **`import.meta` does not make a file a module**, because this parser builds it
  as a `PropertyAccessExpression` rather than a `MetaProperty` (bd tsr-9or.4).
  The check is written against upstream's node, so fixing the parser fixes this;
  it is asserted as a known gap rather than left silent.

And one deviation that is not an omission:

- **A module specifier carries its syntactic context instead of a parent
  pointer.** `getModeForUsageLocation` reads `usage.Parent` to tell an `import`
  statement from a `require()` call from a dynamic `import()`. The tree here has
  no back-edges (ADR-0003), so that would be a side-table lookup followed by a
  six-way kind match — rebuilding, one read at a time, what the collecting walk
  already knew. `tsr_parser::ModuleSpecifier` records it directly, along with the
  `resolution-mode` override, which lives two nodes further up than the parent
  anyway.

## How we would know this was wrong

- **A `.trace.json` case appears whose header sequence we get wrong for a reason
  in the omission list above.** That converts a stated simplification into a
  bug, and the list is ordered by how likely that is: per-casing tasks first,
  `moduleDetection` last.
- **`module_resolution` and `file_loader` disagree about the same case.** They
  share a corpus, a file system, and an options builder; if one passes and the
  other fails on a case both judge, the shared half is the suspect, not the
  compiler. This has happened four times in this project and is a rule now.
- **The two suites are merged and the fused number is lower than either.** Then
  something in the *interaction* — most likely the `package.json` cache state
  that the request order determines — is wrong in a way neither half sees.
- **`file_loader`'s denominator drifts from `module_resolution`'s by anything
  other than `+3 empty-trace, −2 libReplacement`.** That is an arithmetic
  invariant, not a judgement, and it is the cheapest tripwire here.
