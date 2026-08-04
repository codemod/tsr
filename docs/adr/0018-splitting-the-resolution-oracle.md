# ADR-0018: the resolution trace oracle is split between the resolver and the file loader, and slice 2 judges only the resolver

- **Status:** accepted
- **Date:** 2026-08-04
- **Relates to:** [ADR-0017](0017-program-before-tsconfig.md) (whose slice 2 this
  is), [ADR-0004](0004-oxc-inspiration-not-dependency.md) (why the resolver is
  written here), [ADR-0006](0006-conformance-oracle.md) (assert against generated
  Go, not `ast.json`)
- **Scope:** how `module_resolution` conformance is measured, and what its number
  does and does not claim

## The forcing constraint

The oracle for module resolution is 146 committed `.trace.json` files. They are
not pass/fail records — they are **transcripts**. A typical one looks like:

```text
======== Resolving module 'foo' from '/a.ts'. ========
Module resolution kind is not specified, using 'Bundler'.
Resolving in CJS mode with conditions 'import', 'types'.
File '/package.json' does not exist.
Loading module 'foo' from 'node_modules' folder, target file types: TypeScript, JavaScript, Declaration, JSON.
Searching all ancestor node_modules directories for preferred extensions: TypeScript, Declaration.
Directory '/node_modules' does not exist, skipping all lookups in it.
======== Module name 'foo' was not resolved. ========
```

Every file probe, every directory probe, every `package.json` read, every
`exports` condition considered, in order. Across the corpus that is roughly 6,700
lines of assertion. A resolver that reaches the right *file* by a different route
fails, which is exactly the property that makes this worth having.

But a transcript has **two authors**, and they are different components:

| Part of the trace | Produced by | Ported? |
|---|---|---|
| The `======== Resolving module 'X' from 'Y'. ========` headers, and their order | the **file loader** — walks the program, collects each file's imports and reference directives, decides each file's module format | no (bd tsr-9or) |
| Everything between the headers | the **resolver** — `internal/module` | yes, this slice |

Measuring both at once was the obvious thing to do and would have been a mistake.

## The decision

**`module_resolution` replays the requests named in the baseline's own headers,
and compares everything between them.** The suite reads each
`======== Resolving module 'X' from 'Y'. ========` line, calls
`Resolver::resolve_module_name("X", "Y", mode)`, concatenates the traces, applies
upstream's baseline sanitiser, and compares the whole thing to the file.

One further value is taken from the oracle: the **resolution mode**, read from the
`Resolving in {0} mode with conditions {1}.` line. That is unavoidable at this
slice — a containing file's module format is derived from its extension and the
nearest `package.json` `type` field, which is program-level knowledge the resolver
is not given and the loader has not been written to compute. One token of one line
per resolution is circular; the other ~6,700 are not.

**Result: 75/75, 100.00%.** The denominator is derived in
[docs/architecture/module-resolution.md](../architecture/module-resolution.md) and
every excluded case is accounted for by name or by pinned count.

## The alternatives, taken seriously

**Build the file loader first and judge the whole trace.** This is what upstream's
own test does, and it is the eventual destination. Rejected for this slice because
it fuses two failure modes into one number: a mismatch at line 40 could be the
resolver probing in the wrong order *or* the loader having requested the files in
the wrong order, and a 6,700-line transcript gives no help telling them apart. The
first run of this suite is the evidence — 14 failures, all one root cause, found in
minutes precisely because the request list was not also under test.

*This wins if* the loader turns out to be small enough that its failures are
obvious anyway. It is not: upstream's `filesparser.go` + `fileloader.go` are 1,358
lines, it needs pragma parsing the parser does not yet do (`/// <reference />`),
and it buffers traces per task to replay them in a deterministic order precisely
because parallel resolution would otherwise scramble them.

**Judge only the resolution *results*, not the trace.** Much easier, and much
weaker: extension substitution order, the two-pass `node_modules` walk, and the
`exports` condition order are all invisible in the result and all places this port
could plausibly be wrong. Throwing away the transcript would be throwing away the
reason the oracle is good.

**Take the requests from the case source instead of the baseline.** Parse each
file's imports and derive the walk — i.e. write a small loader just for the
harness. Rejected: a harness-only loader that disagrees with the real one is worse
than no loader, and if it agrees it *is* the real one and belongs in
`tsr-compiler`.

## Consequences accepted

- **The number does not claim the loader works.** It claims: given the same
  requests upstream made, this resolver takes the same steps. That is a smaller
  claim than "module resolution conforms", and the suite's own documentation says
  so in as many words.
- **The mode is read from the output.** If the resolver ever emitted the wrong
  mode line, the suite would not catch it — it would have supplied it. The
  falsifier is below.
- **A whole category of bug is invisible until the loader lands**: asking for the
  wrong specifier, from the wrong file, in the wrong order, or not at all. The
  three corpus cases with an *empty* expected trace are exactly the shape of that
  blind spot, and they are skipped with that reason stated rather than passing
  vacuously.
- **The loader gets its own gate**, from the same files: the headers, in order.
  That is a clean second ratchet rather than a re-litigation of this one.

## What this already caught

Two bugs in the first two runs, both of which nothing else in the project would
have found:

1. **`CompilerOptions::module_resolution_kind` had an arm upstream does not
   have.** The Rust port returned `Node10` for an unspecified `moduleResolution`;
   `core.CompilerOptions.GetModuleResolutionKind` falls through to `Bundler`,
   because upstream has not ported `node10` or `classic` at all. 54 baselined
   cases derived an unimplemented kind and the resolver panicked. The
   `docs/architecture` note and the corrected function both record this.
2. **The harness misread bundler's mode line.** `bundler` has no ESM mode and so
   always prints `CJS`, while its *conditions* still distinguish an unspecified
   mode (`'import'`) from `CommonJS` (`'require'`). Reading the `CJS` word cost 14
   cases; reading nothing at all cost 21 more. The mode is now taken from the
   conditions.

The second is worth dwelling on: a large, uniform failure bucket turned out to be
the harness, not the compiler — for the fourth time in this project. The pattern
is reliable enough to be a rule.

## How we would know this was wrong

- **The loader lands and the header sequence disagrees for cases this suite
  passes at 100%.** Then "resolver correct, loader separate" was the wrong
  decomposition, because the resolver's own state (its `package.json` cache, which
  *is* observable in the trace) depends on the order requests arrive in. This is
  the most likely way to be wrong, and it is testable the day the loader exists.
- **The mode turns out to be resolver-derived after all.** If
  `getImpliedNodeFormatForFile` ends up living in `tsr-module` rather than
  `tsr-compiler`, then reading the mode from the baseline is masking a component
  under test rather than supplying an input.
- **100% is reached and the resolver is still wrong in the field**, on a real
  project with a real `node_modules`. Then 75 corpus cases were never a
  representative sample and the ratchet was measuring the corpus, not the
  algorithm. `tsr-dts` (Phase 3.5) is the first thing that would notice.
