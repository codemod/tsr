# ADR-0017: `Program` is parameterised by options, not by a tsconfig parser; and the resolver is written here

- **Status:** accepted
- **Date:** 2026-08-04
- **Relates to:** [ADR-0004](0004-oxc-inspiration-not-dependency.md) (which already
  decided the resolver question, and which an earlier recommendation in this
  session contradicted), [ADR-0016](0016-file-info-not-a-file-name.md)
- **Scope:** the ordering of Phase 3 in [PLAN.md](../../PLAN.md), and what its
  first slice contains

## The forcing constraint

The binder is at 97.98% and the parser at 99.36%; neither moves much further on
its own. What blocks both is that **there is no program**. Every file is bound in
isolation, which the conformance harness states outright: "there is no program and
no cross-file linking yet, so a symbol declared in `a.ts` is not visible from
`b.ts`." About 34 binder failures are exactly that, and every later phase — the
checker most of all — needs the thing that does not exist.

Three questions had to be settled before starting, and two of them had answers
that were not the obvious ones.

## Decision 1: the resolver is written here

**Reaffirming [ADR-0004](0004-oxc-inspiration-not-dependency.md), which already
said so.** An earlier recommendation in this session proposed building module
resolution "on `oxc_resolver`", which contradicts an accepted decision record that
names `oxc_resolver` explicitly among the crates rejected. Recording the slip
here because the archive is more useful with the wrong turns in it: a decision
record only works if it is *read*, and this one was not.

ADR-0004's reasoning stands on its own (pre-1.0 crates, weekly breaking changes,
foundational types in every signature). There is a second reason specific to
resolution that ADR-0004 does not give, and it is the stronger one:

**TypeScript's module resolution is not Node's.** It substitutes extensions — a
specifier ending `.js` resolves to `.ts`, `.tsx`, or `.d.ts` — and layers on
`paths` mapping, `typesVersions`, `moduleSuffixes`, `rootDirs`, declaration-file
preference, and the behavioural split between `node16`/`nodenext`/`bundler`/
`classic`. A general-purpose resolver would need all of that grafted on, at which
point the general-purpose part is doing very little of the work.

Upstream's `internal/module` is **3,196 lines** and there are **146
`.trace.json` baselines** committed to judge it against. That is the same shape
that made the scanner, parser, and binder tractable: a bounded body of code with
an oracle already in the repository.

## Decision 2: `Program` takes a `CompilerOptions`, not a tsconfig

Measured rather than assumed: `internal/compiler/program.go` imports
`internal/core` and **not** `internal/tsoptions`. `core.CompilerOptions` is a
plain struct (557 lines); `tsoptions` is the tsconfig.json parser and validator
that *produces* one (8,706 lines). The dependency runs one way, and only one way.

That matters here more than it does upstream, because **the conformance corpus
does not use tsconfig.json at all.** It sets options with `// @target:`-style
directives, and the harness already parses them — `case.rs` mirrors upstream's
`ParseTestFilesAndSymlinksWithOptions`. So a `CompilerOptions` struct fed from
directives is enough to build and measure a `Program` today, and the 8,706-line
parser is deferred to when a real project needs reading.

One coupling survives and is worth naming: `GetDefaultLibFileName` lives in
`tsoptions/enummaps.go`, mapping `target` to `lib.es5.d.ts` and friends. It is a
lookup table, and it comes across with the options struct rather than with the
parser.

## Decision 3: lib loading is a file list, not a phase

An earlier framing in this session treated "we cannot load `lib.d.ts`" as a
gating concern in its own right. It is not. Loading a lib is *adding files to the
program*, so it needs a `Program` to add them to and nothing else; `noLib` is a
flag on the same code path.

The measurements that settle how much it matters:

| | |
|---|---:|
| Corpus cases using `@noLib` | **9** |
| `.symbols` baselines referencing a lib declaration | 2,665 / 12,155 (22%) |
| `.types` baselines mentioning a lib-only type (approximate) | ~3,027 / 12,155 |

So `noLib` is a real setting and a real bootstrapping tactic — a checker can be
built and unit-tested against lib-free programs — but it gives a **nine-case**
oracle from this corpus, and a failure under it would not distinguish a checker
bug from an absent `Array`. It is a development convenience, not a strategy.

The binder reaching 97.98% without any lib is not evidence against this: the
harness drops symbols declared in files it does not load, by rule.

## The first slice

1. **`CompilerOptions` and a minimal `Program`** — a set of files, the options,
   and each file's `BindResult`, addressable by path. Options come from the
   harness's existing directive parsing.
   *Gate:* the binder suite stops binding units in isolation.

   **Measured after building it: 8,278/8,449 before and after — no change.** An
   earlier draft of this ADR claimed the slice would make "~34 cross-file
   failures judgeable", and that was wrong. Binding is per-file by construction,
   upstream included; what crosses files is *resolution*, and upstream does that
   in the checker (`initializeTypeChecker` merges each script file's locals into
   `c.globals`). A program is the object those files have to live in for that to
   be possible, and it is not by itself sufficient. The falsifier below fired
   immediately, which is the intended use of one; the slice is justified by what
   it makes possible, not by a number it moves.
2. **Module resolution**, written here. *Gate:* the 146 `.trace.json` baselines.
3. **`tsr-tsoptions`** — tsconfig parse and validate. *Gate:* the 757
   configuration-varied cases currently skipped by three suites.

Upstream's `program.go` is 2,232 lines, but most of it is emit, project
references, redirect deduplication, and diagnostic plumbing. The part slice 1
needs is `processedFiles` — `files`, `filesByPath` — plus `BindSourceFiles`,
which is a parallel loop.

## Alternatives

**Start the checker now.** Rejected: it is 60,422 lines against ~18,000 for
module resolution plus options plus program, and its oracle (`.types`,
`.errors.txt`) is written with the default lib in place. Building against stubs
for the cross-file and lib questions means rewriting when they arrive. *This wins
if* the aim were a demo rather than a ratchet — but this project's whole method is
that every phase is gated on a measurable number, and the checker has no
measurable number available until it can load a program.

**Do tsconfig first, then `Program`.** The intuition — that a program is
configured, so configuration comes first — is right about the *struct* and wrong
about the *parser*, per the import graph above. Doing it first would spend 8,706
lines before anything is measurable. *This wins if* the next consumer were a
real-world build rather than the corpus.

**Skip `Program` and give the binder a cross-file symbol table directly.** Cheaper
for the ~34 cases, and wrong: it is the same object under another name, without
the file identity, options, and lifetime story that everything downstream needs.

## Consequences accepted

- **A `Path` type has to exist** before a program can key files by anything.
  That is `tsr-path`, and it arrives with slice 1 rather than on its own.
- **Options will be incomplete and will grow.** The struct starts as the subset
  the corpus directives set. Every field added later is a field some baseline
  needed, which is the right forcing function.
- **The 8,706-line tsconfig layer is deferred**, so nothing reads a real project
  yet. `tsr-dts` (Phase 3.5) is the first thing that would notice.
- **We take on module resolution's compatibility surface** rather than borrowing
  someone else's. 3,196 lines and 146 baselines is the bet.

## How we would know this was wrong

- **`Program` turns out to need `tsoptions` after all** — some option whose
  *value* depends on parsing (a `paths` base URL resolved against the config
  file's directory, say) reaching into program construction. Then the split above
  is in the wrong place.
- **The 146 trace baselines turn out not to exercise the modes we need**, leaving
  module resolution without a usable ratchet. Then borrowing a resolver and
  testing it end-to-end through the checker becomes the cheaper path, and
  ADR-0004 needs revisiting rather than reaffirming.
- ~~**Cross-file binding does not move the ~34 cases**~~ — **this fired, on the
  day it was written.** The rate is unchanged at 8,278/8,449. The cases were not
  misclassified; the prediction confused *binding files together* with
  *resolving across them*. The rest of the reasoning is unaffected, because none
  of it rested on that number — but a decision record that predicts a number and
  gets it wrong should say so where the prediction was, not in a footnote.
- **The resolver arrives without a program-level ratchet to prove it.** Slice 2
  is judged against the trace baselines, which test resolution in isolation.
  Nothing measures resolution *plus* binding until the checker exists, so slice 1
  and slice 2 are both bets on an architecture rather than steps up a curve. That
  is a real change in this project's method and it should be uncomfortable.
