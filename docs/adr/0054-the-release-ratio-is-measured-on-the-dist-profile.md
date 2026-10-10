# ADR-0054: The release ratio is measured on, and shipped from, the fat-LTO `dist` profile

- **Status:** accepted (round 7, `r7-perf`, `tsr-2zk.1277`; build decision
  approved by the round-7 integrator, 2026-10-10)
- **Date:** 2026-10-10
- **Supersedes:** [ADR-0009](0009-performance-gate.md) in part. ADR-0009's
  gate (`.github/workflows/perf.yml`) builds with `--release` and never names
  a profile for the whole-project target. This record names one for the
  CLAUDE.md release target, the verified TSR/tsgo median wall ratio
  <= 0.50 on equivalent complete work, and for a shipped `tsr`. ADR-0009's
  parser/binder gate and every box-protocol gate keep `release`.
- **Related:** [`docs/architecture/whole-project-performance.md`](../architecture/whole-project-performance.md),
  [`docs/parity/notes/r6-checkperf.md`](../parity/notes/r6-checkperf.md) §5,
  [`docs/parity/notes/r7-perf.md`](../parity/notes/r7-perf.md) §6

## The forcing constraint

The two compilers were being compared in different build states.
typescript-go's binary is Go's normal optimized build: whole-program
compilation, with inlining across packages. TSR's ratio was measured on
Cargo's `release` profile, which is `lto = false` and `codegen-units = 16`.
`Cargo.toml` chose those settings for the inner loop: the conformance corpus
is rebuilt after touching one checker file dozens of times a session, so
relink time is on the critical path. Nothing ships that build. The
`Cargo.toml` comment says so: "This port has no released binary".

Measured with fat LTO and one codegen unit, CPU over the same tree falls by
4–11%:

- r6-checkperf §5, 31 rounds: dml 0.745 → 0.692 and gi 0.686 → 0.628 of
  tsgo's wall.
- r7-perf §6 (21 samples, child CPU, `dist`/`release` on the same source):
  domain-model **0.905**, generic-imports **0.959**,
  domain-model-large **0.900**, jsTyping **0.942** (5 samples).

CLI output is byte-identical between the two profiles on all four projects.
The release target is a statement about the compiler a user runs. Measuring
a build configuration nobody ships overstates TSR's cost by that 4–11%.

## The decision

1. `[profile.dist]` is `inherits = "release"`, `lto = "fat"`,
   `codegen-units = 1`. It inherits `opt-level = 3` and `panic = "abort"`
   from `release`. It was previously `lto = "thin"`.
2. **The release ratio is measured with `dist`.** Build with
   `cargo build --profile dist -p tsr` and pass `--tsr target/dist/tsr` to
   `scripts/whole_project_perf.py`. Every ratio report shows the `release`
   ratio beside it, so the two series stay comparable with the history
   (all earlier ratios are `release`).
3. **A shipped `tsr` is built with `dist`.** No workflow builds a shipped
   binary today: `perf.yml` builds only the parser example and bench, and
   `ci.yml` builds nothing it ships. A future release or packaging job must
   use `--profile dist`. A whole-project perf job must also build
   `target/dist/tsr` and report both profiles. This record is that job's
   specification. The workflow change belongs to the integration owner,
   not to this lane.
4. **Gates stay on `release`.** Box-protocol §5's self-comparison (new/base
   child CPU against the frozen binary), the conformance dumps and the
   coverage bin all keep `release`, whose incremental relink takes seconds.
   A perf regression is judged within one profile and never across
   profiles.

## Alternatives

- **Thin LTO (the previous `dist`).** r6-checkperf §5 measured it recovering
  jsTyping's share (2.866 against fat's 2.844 of tsgo) but not
  generic-imports' (0.687 against `release`'s 0.686 and fat's 0.628). Fat is
  the setting that moves every project. Thin wins only if fat's build time
  becomes the binding cost of a release job.
- **Change `release` itself to fat LTO.** Rejected: a from-scratch fat-LTO
  build of `tsr` takes about 3–4 minutes on a 4-core box, against seconds
  for `release`'s incremental relink, and every box rebuilds after each
  checker edit. That is the trade `Cargo.toml` already records. It would
  win only if gate turnaround stopped mattering.
- **PGO on top of `dist`.** Not measured here. It needs a training corpus
  that is not the measured projects, or the number is fitted to the
  benchmark. A later record can add it, with that separation stated.
- **Keep measuring `release`.** Rejected: it compares a development
  configuration against an optimized one, and that is not the work a user
  runs.

## Consequences accepted

- The release ratio moves by 4–11% with no source change. Ratios before
  this record are `release` ratios. Reports carry both, so a reader can
  tell a profile change from a code change.
- A ratio regression can be profile-sensitive: inlining and layout can
  differ between the two builds. The self-comparison gate stays on
  `release`, so a change that slows only `dist` is not caught by the gate.
  It shows up in the side-by-side ratio report instead.
- Measuring `dist` costs about 3–4 minutes of build per measurement point.

## How we would know this was wrong

- `dist` and `release` disagree on output for some input. Fat LTO must not
  change behaviour, so that would be a miscompile or undefined behaviour,
  and it would make `dist` unusable for shipping until explained. The
  falsifier is a CLI `cmp` on the four projects and a dump comparison when
  a release is cut.
- The `dist`/`release` CPU ratio drifts to about 1.0 on the four projects.
  The profile then buys nothing, and its build cost is waste.
