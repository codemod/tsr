# Three OSS project checks: memory blockers fixed, diagnostic parity remains open

## Post-fix checks complete, but there is no verified native speedup

The parser and observed `showConfig` mismatches are fixed. Anonymous/mapped
instance keys now include only lexical outer parameters, and non-generic
recursive receivers no longer repeatedly freshen their methods' parameters.
Zod and Excalidraw now finish instead of exhausting memory. This is completion
of previously blocked checking, **not a verified speed improvement over tsgo**.

Measured source, including the streaming benchmark-reader correction:
[`9d62eb1`](https://github.com/codemod/tsr/commit/9d62eb1e02b69fcfaf28ab5d657ac01790e1ecfb).
Compiler corrections:
[`676e60e`](https://github.com/codemod/tsr/commit/676e60e1ddec7b3743c3a65ac1ea003ef2e3d3b8)
and [`a927433`](https://github.com/codemod/tsr/commit/a927433cd42effcc245b9e9aa946cad35eb21cce).
The native pin, prepared project revisions, config migrations and selected
scopes remain those recorded below. The
[post-fix receipt](oss-projects-2026-10-13-fixed.json) preserves binary, harness,
input and source identities, all 60 measured process samples, complete
diagnostics once per tool/project, checked-scope evidence and validation hashes.

Five fresh-process samples plus one warmup per tool/project/profile, sequential
outside builds and tests, alternating tool order, default workers, warm
filesystem, and the same `--noEmit --incremental false --composite false
--pretty false` flags. Resources come from each compiler child's `wait4`, not
the benchmark reader. These are **raw complete-check observations**:

| Project (scoped tracked TS lines) | dist TSR median wall (range), s | dist tsgo median wall (range), s | Median peak RSS TSR / tsgo, MiB | Observed wall ratio |
|---|---:|---:|---:|---:|
| Zod (61,826) | 2.631 (2.523–2.731) | 2.771 (2.723–2.941) | 1,328.1 / 833.3 | 0.949 |
| Effect core (164,428) | 2.655 (2.512–2.721) | 4.043 (3.873–4.132) | 568.0 / 1,189.6 | 0.657 |
| Excalidraw (215,080) | 7.004 (6.766–7.337) | 3.546 (3.428–3.783) | 855.9 / 1,089.3 | 1.975 |

Release-profile confirmation retains exactly the same diagnostics:

| Project | release TSR / tsgo median wall, s | Median peak RSS TSR / tsgo, MiB | Observed wall ratio |
|---|---:|---:|---:|
| Zod | 2.786 / 2.812 | 1,358.2 / 845.5 | 0.991 |
| Effect core | 2.874 / 3.915 | 566.3 / 1,202.6 | 0.734 |
| Excalidraw | 6.946 / 3.556 | 854.0 / 1,085.4 | 1.954 |

| Project | TSR / tsgo diagnostics | Exactly shared occurrences | TSR-only / native-only occurrences | Loaded files, both | Actually checked files, both |
|---|---:|---:|---:|---:|---:|
| Zod | 1,083 / 46 | 19 | 1,064 / 27 | 943 | 400 |
| Effect core | 1,311 / 0 | 0 | 1,311 / 0 | 661 | 360 |
| Excalidraw | 1,977 / 0 | 0 | 1,977 / 0 | 1,502 | 655 |

All samples complete with stable diagnostics and exit statuses. Independent
untimed traces certify matching nonempty checked-file/owner lists, matching
loaded identities and bytes, unchanged observed inputs and matching serialized
compiler options. Diagnostic comparison preserves full multiline messages,
positions, codes and duplicate occurrences. Unshared Zod diagnostics may
include differently presented versions of native errors; counts alone do not
classify each occurrence as a distinct semantic defect.

**Diagnostic parity fails on every scope.** Excalidraw remains about twice as
slow in raw wall time, and Zod retains more memory than native. Neither the
lower Effect time/RSS nor the lower Excalidraw RSS establishes efficiency on
equivalent semantic work. `verified_wall_ratio` remains null and
`target_verified` remains false; the <=0.50 release target is not achieved.
Matching serialized options also does not prove all option semantics match.
The reused original config/package/lockfile inventory is explicitly partial,
not a fresh complete resolver-query capture.

## Allocation failures are removed without changing legacy answers

The first lexical-key correction let Zod finish, but Excalidraw still exceeded
6 GiB. Allocation stacks and retained-state probes identified repeated fresh
signature/receiver images, not giant display strings. The receiver correction
preserves declared literal identity only for registered foreign lexical
parameters. Captured/instantiated images, unregistered sources and
function-owned contextual captures retain the previous instantiation path.
The [declared-type boundary notes](r6-declared.md#101-recursive-receiver-literals-do-not-capture-their-methods-parameters)
record the native operation, owner, publication and alias/receiver contracts.

The 21-line array-intersection receiver control previously exhausted 4 GiB;
the final release binary completes in 0.054 seconds at 72 MiB, with exactly the
native TS2322 and TS2345 negative controls. The permanent regression verifies
the inferred string/null and literal-item result types through bundled libs.
No conditional cut-off, completed unsupported fallback or new semantic cache
was added.

The first Excalidraw benchmark attempt also exposed a **reader**, not compiler,
failure: Python materialized a 1,299,137,210-byte work trace and was kernel-killed
at about 6 GiB RSS after all timed compiler checks had finished. The reader now
streams records and retains only file-check evidence. Reading the same trace
under a 128 MiB address-space cap peaks at 18,680 KiB and retains all 655 checks.
A public CLI regression with 150,000 unrelated spans fails before and passes
after; all 27 harness tests pass. Incomplete, unmatched, duplicated and malformed
completion controls remain rejected. The interrupted attempt is excluded from
the final tables; all final dist/release runs finish through trace verification.

Unfiltered frozen legacy outputs before/after the combined compiler fixes are
**byte-identical after removing only runtime `ms`/`mib` guard columns**:
557,344 type assertions (554,678 RIGHT, 374 GAP, 2,292 WRONG) and 12,238
diagnostic cases. There are zero answer changes, RIGHT gains/losses/missing,
or GAP-to-WRONG changes. Comparing entire normalized output preserves literal
line separators that a line-oriented TSV comparison would lose. The frozen
compiler patch hash matches the committed receiver correction; the later
benchmark-reader commit changes no compiler source.

`cargo test -p tsr-execute -p tsr-tsoptions -p tsr-parser` passes, as do 230
checker unit tests, 9 anonymous-instantiation, 5 mapped-relation, 19 lazy-return
and 4 targeted receiver/contextual-generic conformance tests. Formatting and
clippy pass. **The full gate is not green:** the existing
`contextual_this_objects::unannotated_this_uses_its_contextual_slot_or_implicit_any`
debug underflow at `implicit_any.rs:625` reproduces on the untouched baseline;
the latest suite has 24 passed / 1 failed. It remains tracked as `tsr-11s`.

Resolved slices: `tsr-2zk.2.13.1`, `tsr-6wu`, `tsr-we2.1`, `tsr-we2.2` and
`tsr-we2.3`. Remaining diagnostic parity is `tsr-2zk.1268`; remaining
Excalidraw wall cost/Zod RSS work is `tsr-2zk.1269`, dependent on parity.
Detailed expensive-worker/copy attribution remains `tsr-1yb.11`.

## Original parser-blocked baseline (retained historical evidence)

**TSR fails diagnostic parity on all three projects and checks zero files.**
Its shorter elapsed time and smaller RSS measure parsing and early exit, not
equivalent complete checking. No verified TSR/tsgo ratio or release-target claim
can be made from these runs.

Measured TSR source:
[`66fcf1f`](https://github.com/codemod/tsr/commit/66fcf1f5a51f6dabbea02f861f2644b98af394e5).
Native oracle:
[`5b1047d`](https://github.com/microsoft/typescript-go/commit/5b1047d10d32e7d5b446be4de56b126ff42f82bb),
reporting `Version 7.1.0-dev`. Both compilers were built from these checkouts
before measurement. The [JSON receipt](oss-projects-2026-10-13.json) contains
binary and harness hashes, source pins, setup patches, all 60 measured child
identities and resource samples, complete diagnostics, and option differences.

## One small and two medium scopes

| Project | Pinned revision | Repository TS lines | Loaded, tracked TS lines | Selected config |
|---|---|---:|---:|---|
| Zod 4.1.12 | [`3b94610`](https://github.com/colinhacks/zod/commit/3b946107b6c94b2ac8ff9fb451160c34dc4dd794) | 64,024 | 61,826 / 331 files | `tsconfig.json` |
| Effect 3.10.19 | [`753d120`](https://github.com/Effect-TS/effect/commit/753d120544f2e21d130311e9b007b299c54c02f6) | 362,695 | 164,428 / 360 files | `packages/effect/tsconfig.src.json` |
| Excalidraw | [`541653c`](https://github.com/excalidraw/excalidraw/commit/541653c7c9d587d463a47daacff9fb30e45a52e7) | 217,821 | 215,080 / 668 files | `tsconfig.json` |

Counts are physical lines (`bytes.splitlines()`) in tracked `.ts`, `.tsx`,
`.mts`, and `.cts` files. They include comments, blanks, tests and declarations,
but exclude dependencies and untracked generated output. The scoped count is the
intersection of tracked TypeScript paths and the native loaded-file list. Both
repository and selected-scope counts satisfy <100k for the small project and
100k–500k for the two medium projects.

Effect tests the core production package, **not the entire monorepo**. Zod's
root configuration includes benchmarks, tests and resolution fixtures; its
46 native diagnostics are part of this baseline, not a clean library build.
Excalidraw's root configuration covers the application and packages, including
their tests, and passes the native checker.

## Diagnostic parity fails before semantic checking

| Project | TSR diagnostics | tsgo diagnostics | Exactly shared diagnostics | Loaded files, both tools | Actually checked files, TSR / tsgo |
|---|---:|---:|---:|---:|---:|
| Zod | 4 | 46 | 0 | 943 | 0 / 400 |
| Effect | 13 | 0 | 0 | 661 | 0 / 360 |
| Excalidraw | 10 | 0 | 0 | 1,502 | 0 / 655 |

All five samples in both TSR profiles have stable diagnostics and exit codes.
TSR exits 1 on every full check. Native exits 1 for Zod and 0 for the other two
projects. Diagnostic comparison retains complete multiline messages, positions,
codes and duplicate occurrences; it does not compare counts alone. None of the
TSR parser diagnostics appears in native output.

The parser failures start at these generic-arrow expressions:

- Zod `packages/zod/src/v3/types.ts:5038`: `<T extends typeof Class>(...) => ...`.
- Effect `packages/effect/src/internal/matcher.ts:353`: a multiline inner arrow
  with `<R, P extends string, Ret, B extends Ret>` and template-literal types in
  its parameters.
- Excalidraw `packages/excalidraw/tests/data/reconcile.test.ts:69`:
  ``<U extends `${string}:${"L" | "R"}`>(...) => ...``.

A standalone native-supported control reduces the template-constraint failure:

```ts
export const f = <U extends `${string}:${"L" | "R"}`>(value: U) => value;
```

With `target: ESNext`, `module: NodeNext`, `strict: true`, `skipLibCheck: true`
and `types: []`, native exits 0 without diagnostics. TSR exits 1 with four
TS1005 diagnostics (`?`, `)`, `,`, and `;` expected). This establishes that the
failure is not dependent on missing project dependencies. It does not establish
that the three production failures all have the same parser root cause.

Untimed work traces are complete and reproduce the timed diagnostics and exits.
They show zero TSR `source_file_check` spans in every scope. The native counts
include any imported dependency sources it actually checks, not just tracked
project roots.

Both tools load identical normalized file identities, and all observed input
bytes remain stable. Their serialized `--showConfig` options still differ:
9 fields for Zod, 13 for Effect, and 3 for Excalidraw. In particular, TSR omits
`paths` from the last two configurations despite matching loaded-file lists.
These are serialization observations, **not proof that all differing options
are ignored**. They independently prevent the strict equivalence gate passing.

## Elapsed time and memory are early-exit observations

Five fresh-process samples per tool/project/profile, one warmup per tool,
alternating process order, default worker scheduling. Every invocation uses
`--noEmit --incremental false --composite false --pretty false`. Filesystem
caches are warm; builds, installation, input hashing and tracing are outside the
timed intervals. Linux orb: 4 vCPUs, Intel Xeon 2.60 GHz, 7.8 GiB RAM; no worker
environment overrides. Resource measurements use the harness's child `wait4`.

| Project | dist TSR median wall (range), s | dist tsgo median wall (range), s | Median peak RSS TSR / tsgo, MiB |
|---|---:|---:|---:|
| Zod | 0.246 (0.216–0.282) | 2.640 (2.549–3.038) | 235.6 / 857.7 |
| Effect | 0.241 (0.209–0.271) | 3.741 (3.443–3.922) | 313.4 / 1,206.6 |
| Excalidraw | 0.421 (0.368–0.432) | 3.430 (3.392–3.824) | 427.7 / 1,089.1 |

The release-profile confirmation has the same diagnostics and checked scope:

| Project | release TSR / tsgo median wall, s | Median peak RSS TSR / tsgo, MiB |
|---|---:|---:|
| Zod | 0.234 / 2.660 | 236.8 / 840.9 |
| Effect | 0.245 / 3.723 | 320.7 / 1,213.6 |
| Excalidraw | 0.443 / 3.404 | 427.1 / 1,078.4 |

**Neither table is a speedup or memory-efficiency comparison.** Native performs
semantic checking; TSR does not. `work_comparable` and `target_verified` remain
false and `verified_wall_ratio` remains null in the receipt.

## Setup migrations and reproduction

Install the pinned projects using their frozen lockfiles and skip lifecycle
scripts:

```sh
# In the respective repository roots:
pnpm install --frozen-lockfile --ignore-scripts                  # Zod
pnpm install --frozen-lockfile --ignore-scripts --filter effect... # Effect
yarn install --frozen-lockfile --ignore-scripts --non-interactive # Excalidraw
```

pnpm selects the project-declared versions (10.12.1 for Zod, 9.10.0 for Effect).
The installed Yarn is 1.22.19. No lockfile or TypeScript source file is modified.
These compiler-option migrations are necessary for the pinned TS7 oracle:

- Zod `.configs/tsconfig.base.json`: remove `downlevelIteration: true`. Its ES2020
  target already has native iteration support; checks disable emit.
- Effect `tsconfig.base.json`: remove `downlevelIteration: true`, `baseUrl: "."`,
  and `esModuleInterop: false`; add `"*": ["./*"]` to `paths`. Its ES2022 target
  does not need downlevel iteration. NodeNext now uses the required interop
  default. The wildcard retains the old root fallback for module lookup.
- Excalidraw `tsconfig.json`: replace `baseUrl: "."` with the same wildcard
  path fallback, as recommended by native TS5102.

The exact patches are in the receipt. The original Zod and Excalidraw native
preflight failures are preserved in the raw evidence. Effect measurements made
before removing `esModuleInterop: false` produced native TS5108 and zero checks;
they are labeled rejected configuration evidence and excluded from every table.

Build and reproduce the strict gate from the measured TSR checkout:

```sh
cargo build --profile dist -p tsr --features work-trace
cargo build --release -p tsr --features work-trace
mkdir -p target/oss-bench
go -C vendor/typescript-go build -o "$PWD/target/oss-bench/tsgo" ./cmd/tsgo

TSR_LIB_PATH="$PWD/vendor/typescript-go/internal/bundled/libs" \
  python3 scripts/whole_project_perf.py \
  --project /path/to/prepared/project/tsconfig.json \
  --tsr target/dist/tsr --tsgo target/oss-bench/tsgo \
  --samples 5 --timeout 120 --require-comparable --output /tmp/project.json
```

Use Effect's `packages/effect/tsconfig.src.json` instead of its solution config.
The strict harness rejects TSR's `--listFilesOnly` preflight on all three
projects because it emits parser diagnostics. The separate scratch probe in the
raw evidence reuses `whole_project_perf.process`, diagnostic normalization and
resource summarization to collect full CLI observations after that rejection;
it never relaxes the gate or publishes a verified ratio. It snapshots the union
of loaded sources plus a supplemental tracked config/package/lockfile inventory
before and after each child. This is partial input coverage, not complete
resolver-query capture. The probe explicitly points TSR at the pinned native
standard libraries.

Validation: `python3 -m unittest discover -s scripts -p test_whole_project_perf.py -v`
passes all 26 tests. The receipt was independently checked against raw samples:
60 distinct measured PIDs, five stable samples per tool/project/profile,
unchanged observed inputs, equal loaded scopes, zero TSR checks and nonempty
native checks. No compiler implementation was changed in this investigation.

Follow-up work is tracked in **tsr-2zk.2.13.1** (parser blockers and real-project
recheck) and **tsr-6wu** (effective-option versus serialization differences).
Fixing the parser is a correctness prerequisite; throughput remains unmeasured
until both tools perform equivalent complete work.
