# ADR-0009: Gate performance against typescript-go, not against our own history

**Status:** Accepted for the two gated axes; the CI wiring (`bd tsr-cmh`) is still
outstanding. Both axes now have a measured baseline — see
`docs/architecture/performance.md` — which is what the "Proposed" status was
waiting on.
**Date:** 2026-08-03
**Relates to:** [ADR-0006](0006-conformance-oracle.md) (same principle: the oracle
is the artifact whose behaviour we are matching, not our own prior output)

## Context

PLAN.md §4 states that "Performance and memory are per-PR CI gates from Phase 0"
and that the first draft's "Phase 9 — Performance" was a mistake. `bd tsr-oqn`
records the same correction: the remaining tuning phase is *distinct from* the
continuous gate. The Phase 0 gate in PLAN.md §4 reads "CI publishes coverage and
benchmark numbers"; `bd tsr-5e7`'s acceptance criteria repeat it.

**The benchmark half of that commitment was never built.** As of this ADR:

- No `benches/` directory anywhere in the workspace.
- No `criterion`, `divan`, `iai`, or `hyperfine` in any `Cargo.toml`, despite
  `criterion2` appearing in the adopted bill of materials (`bd tsr-5e7`).
- `.github/workflows/ci.yml` runs format, clippy, test, conformance-snapshot, and
  codegen-staleness. There is no benchmark step and no Go toolchain.
- `bd tsr-5e7` had eleven children and none of them was this.

The coverage ratchet shipped; the benchmark ratchet did not. This ADR settles how
the gate is defined before `bd tsr-cmh` implements it.

The forcing constraint is timing, not principle. Only the scanner and the parser
exist today, so "faster than typescript-go in every aspect" currently ranges over
two subsystems. Every subsystem added later — binder, checker, transformers,
printer, driver — then lands against an existing gate instead of needing one
retrofitted. Retrofitting a performance gate onto a finished checker is how the
first draft got Phase 9 wrong.

## Decision

**1. The blocking comparison is against typescript-go at the pinned commit
(`5b1047d10`), not against our own previous commit.**

**2. Gated axes: wall-clock and peak RSS. Reported but not gated: binary size,
allocation count, and self-regression versus the previous commit.**

**3. The corpus is upstream's `fixtures.BenchFixtures`**
(`vendor/typescript-go/internal/testutil/fixtures/benchfixtures.go:10`) — five
fixtures: `empty.ts`, `checker.ts`, `dom.generated.d.ts`, `Herebyfile.mjs`, and
`jsxComplexSignatureHasApplicabilityError.tsx`.

**4. Thread count is pinned to 1 on both sides for the gated number.** Parallel
numbers are recorded separately and do not gate.

**5. The Go side reuses upstream's own benchmark where one exists.**
`internal/parser/parser_test.go:23` already defines `BenchmarkParse` over exactly
that fixture set.

## Reasoning

### Why ratio-vs-tsgo rather than a self-regression ratchet

A "never slower than the previous commit" ratchet has a baseline that moves on
every merge, which makes it both noisy and hostile to correctness work.

The parser went 95.18% → 99.36% conformance across `86212c2`, `5658d35`, and
`19cc1dc`. Closing a long tail means executing more code on the paths that were
previously wrong. A self-regression ratchet would have fired on that work and the
correct response every time would have been to override it — a gate whose correct
response is routinely "override" trains people to override it.

A ratio against a pinned tsgo does not have this problem, because **tsgo pays for
the same correctness we do.** If handling a conformance edge case costs time, it
cost upstream time too, and the ratio holds. The baseline moves only when the
submodule pin moves, which is a reviewed, deliberate event.

This is the same principle as [ADR-0006](0006-conformance-oracle.md): assert
against the artifact whose behaviour is the target, not against our own prior
output. Testing against your own history tests whether you changed, not whether
you are correct — or here, not whether you are fast.

### The first RSS measurement (added 2026-08-03)

The reasoning below was written before anything was measured. It holds up, and the
numbers are now in `docs/architecture/performance.md`: parsing the four non-empty
fixtures and holding every tree, peak RSS is **33.0 MB against typescript-go's
57.0 MB (1.77×)**, and the AST's own cost — peak minus a baseline taken after the
source text is read — is **25.0 MB against 37.8 MB (1.51×)**.

One caveat that matters for how this gate should be read: at that heap size Go's
collector never runs (`GOGC=off` produces an identical figure), so the comparison
is of data-structure size, not of GC headroom. The headroom effect — a collector
needing roughly twice the live set before collecting, which is the mechanism
behind `tsc` running out of memory on large projects — is **not captured here and
would only widen the margin**. A whole-project measurement belongs with `bd
tsr-oqn` when a driver exists.

### Why peak RSS gates and wall-clock alone does not

typescript-go's advantage over `tsc` was substantially memory and parallelism, not
only raw time. `docs/architecture/threading.md` records that "parallel checking is
the headline reason typescript-go is fast."

A wall-clock-only gate can therefore be fully green while regressing the axis that
made the upstream project worth porting — trading memory for speed until the LSP
case degrades, with CI reporting success throughout. Binary size and allocation
counts are worth watching but are not user-visible in the same direct way, so they
are reported without blocking.

### Why thread count is pinned to 1

`docs/architecture/threading.md` measures the conformance harness at ~25 s of CPU
in ~4.6 s wall, about 5.7× on that machine. Numbers of that shape make the
comparison trivially riggable in either direction: benchmark single-threaded tsr
against parallel tsgo and we look terrible; do the reverse and we look excellent.
Neither number means anything.

Single-threaded is the honest default because it measures the work rather than the
scheduling, and because tsr's parallelism is currently confined to the conformance
harness — `threading.md` states plainly that "parallel parsing in any real driver"
is not built. Gating on parallel numbers today would gate on a capability we have
not shipped. When a real driver parallelises, the parallel comparison becomes
meaningful and this decision should be revisited.

### Why upstream's fixture set rather than our own

It is upstream-defined, it moves with the pin, and it already spans the script
kinds that exercise different scanner modes: TypeScript, a large `.d.ts`, plain
JavaScript, and JSX. Choosing our own corpus would invite — even unintentionally —
selecting inputs we happen to be fast on. Adopting theirs removes that degree of
freedom.

`empty.ts` is in the set deliberately: it measures fixed per-file overhead, which
is what an LSP reopening files actually pays.

## Alternatives considered

| Alternative | Why rejected |
|---|---|
| **Self-regression ratchet** (never slower than last commit) | Fires on legitimate correctness work; moving baseline; correct response is routinely "override". Kept as a *reported* signal, since it catches drift the ratio can hide when both sides are slow. |
| **Wall-clock only** | Misses memory, which is upstream's actual edge over `tsc`. Green CI while degrading the LSP case. |
| **Defer to a final tuning phase** | Already tried and already rejected — this is the first draft's Phase 9 error, recorded in `bd tsr-oqn` and PLAN.md §4. Restated here only so the rejection is not re-litigated. |
| **Instruction counts (iai/cachegrind) instead of wall-clock** | Not rejected — deferred pending measurement. Instruction counts are near-deterministic and would remove the noise problem entirely, but they do not capture memory behaviour or allocator effects, and they cannot be compared meaningfully against a Go binary. Adopt as the *self-regression* metric if runner variance proves too wide; the tsgo ratio stays wall-clock. |
| **Our own benchmark corpus** | Adds a degree of freedom in which inputs get chosen. Upstream's set is fixed by the pin. |

## Consequences accepted

- **CI needs a Go toolchain.** `ci.yml` currently has none. Build time increases
  by a full typescript-go compile, mitigated by caching the built binary against
  the submodule SHA — it changes only when the pin moves.

- **CI needs recursive submodules.** `ci.yml:21` uses `submodules: true`, which is
  not recursive. `BenchFixtures` resolves paths through
  `repo.TypeScriptSubmodulePath()`, i.e. the `TypeScript` submodule *inside*
  typescript-go. This must become `submodules: recursive`, matching the
  `git submodule update --init --recursive` already documented in CLAUDE.md.

- **There is no upstream scanner benchmark.** Only `internal/parser` and
  `internal/checker` define `Benchmark*` functions. The scanner comparison
  therefore requires a Go benchmark we write ourselves, which is weaker evidence
  than reusing upstream's: we control both sides of a scanner comparison and could
  write a favourable one without noticing. It lives outside the submodule (the pin
  stays clean) and should be reviewed as an adversarial artifact rather than a
  convenience.

- **The gate is only as honest as thread parity.** Pinning thread count to 1 is an
  assumption recorded in the harness, and nothing enforces that a future change
  does not quietly parallelise one side.

- **Pin bumps require benchmark review.** When the submodule moves, ratios shift
  for reasons unrelated to our code. This is correct behaviour but means a pin bump
  is no longer a trivially reviewable diff.

- **Two subsystems only.** Today this gates the scanner and the parser. Calling it
  "faster in every aspect" overstates the coverage until the binder and checker
  exist, and this ADR should not be cited as though it covered them.

## How we would know this was wrong

- **The gate never fires for several months of active work.** Then the band is too
  wide, or the corpus does not exercise what we are changing, and it is decoration.

- **The gate fires and the fix is routinely "re-run CI".** Then we are measuring
  runner noise, not the code, and the metric should move to instruction counts.

- **A conformance fix is legitimately blocked by the gate.** This is the specific
  failure the ratio design exists to prevent. If it happens anyway, the premise —
  that tsgo pays for the same correctness — is false for that code path, and the
  gate needs a documented escape hatch rather than silent overrides.

- **Peak RSS turns out not to be measurable stably enough to gate.** Plausible;
  RSS is sensitive to allocator behaviour and OS accounting. If so, demote it to
  reported and gate allocation counts instead, which are deterministic.

- **We are fast on the five fixtures and slow on real projects.** The fixture set
  is small and file-scoped. A whole-project suite belongs in `bd tsr-oqn` when a
  driver exists; until then this gate covers per-file work only, and a divergence
  between the two is the signal that the fixture set stopped being representative.
