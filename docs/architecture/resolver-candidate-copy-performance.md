# Resolver candidate-copy experiment

The experiment at `143b2eedd47fd837775bc8904f864f9b4a444f51` is rejected.
Removing path copies did not establish a repeatable whole-project improvement.
All production source was restored. `tsr-1yb.2.1.5` records this decision;
the comparable TSR/pinned-native wall-ratio target of 0.50 remains unverified.

## Located copies

An opt-in resolver probe counts seven existing copy sites and times construction
before its own bookkeeping. The source, binary and patch hashes are in
[the aggregate evidence](resolver-candidate-copy-performance.json). Payload
bytes are string lengths, not allocator totals, capacities or retained RSS.
Intervals include clock/probe effects and exclude freeing the strings, caller
work and filesystem calls. They are locating evidence, not wall savings or a
hard benefit ceiling.

The fixed app snapshot has 13,133 loaded files, 1,346 actual checked identities
in the scope controls, and 119 complete diagnostics. Repeated probe accounting
matches exactly:

| Copy site | Calls | Payload bytes |
| --- | ---: | ---: |
| Original extension | 14,703 | 47,625 |
| Extensionless candidate | 14,703 | 1,358,855 |
| Candidate directory | 25,376 | 1,898,348 |
| Module-suffix payload | 69,744 | 0 |
| Suffix stem | 0 | 0 |
| Failed result path | 46,244 | 4,198,895 |
| Successful result path | 23,500 | 2,225,252 |

The four avoidable nonzero sites copy 7,503,723 bytes. Their initial instrumented
construction intervals total 3.971 / 4.353 ms. The app's empty suffix list
clones no owned payload; the nonempty-list and suffix-stem branches are instead
exercised by the public controls. Successful returned paths still need ownership.

## Candidate and native boundary

[The rejected candidate](resolver-candidate-copy-rejected.patch) borrows the
original extension, stem, directory and immutable module-suffix inputs. Its
private `try_file` returns an owned path only when lookup succeeds. All four
callers previously ignored the filename returned on failure. The three-valued
`Search` protocol remains unchanged: failure at this private file-probe boundary
continues searching, while existing stop-without-result branches remain distinct.

Pinned native `internal/module/resolver.go:1582` borrows string inputs and
iterates the configured suffixes without copying them. Source SHA-256
`bfd63b170178ec30977bf9c3bbf1bef574184901b12fc5eaebbbef2f41ac4ff1`
matches the clean oracle manifest at
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. This is an allocation experiment,
not permission to skip failed probes, infer missing parents or change caches.

## Whole-project result

The comparison uses frozen uninstrumented TSR baseline/candidate binaries from
the same source and release toolchain. It is **not a TSR/native ratio**. Each
round has one warmup per role and five alternating fresh-process pairs; the
starting role reverses in the second round. Incremental/composite checking is
disabled, inputs and pinned libraries are shared, and `--listFiles` directly
checks the entire ordered loaded section in every scored process.

| Round | Baseline median wall | Candidate median wall | Candidate minus baseline |
| --- | ---: | ---: | ---: |
| 1 | 3.777694 s | 3.781903 s | +4.209 ms |
| 2 | 3.812613 s | 3.806820 s | -5.793 ms |

Neither round confirms the existing 20 ms retention gate; the signs also
disagree. Median user/system CPU is recorded separately. Candidate median RSS
is higher in both rounds, with overlapping ranges; no memory saving is claimed.
Owned builds, tests and measured processes ran serially. Other agents' host
activity was not controlled.

All twenty scored runs and four warmups preserve complete diagnostics, loaded
order and physical input/config hashes. Normal binaries have no checked marker.
Independent probe-off and candidate-scope binaries verify the same 1,346 ordered
actual checking identities and checked-input hashes before and after each round
(eight additional processes). Effective options and normal/probe off/on/repeat/
candidate/scope output agree in the separate controls. These separate markers
must not be described as direct normal-binary checked telemetry.

## Controls and a discovered fidelity issue

All 49 release module tests pass. Supported native suites pass 95/95 resolver
and 96/96 loader cases, with both committed snapshots byte-identical to source.
The canonical suites sanitize version/package-cache messages and skip other
unsupported/no-baseline cases; this does not establish all raw native fields or
CLI trace parity. Strict module Clippy, including tests, and formatting pass.
Full checker type/diagnostic corpora were not run for a completely discarded
candidate.

Four physical public cases preserve all TSR variants' complete diagnostics,
loaded order, actual scoped identities and inputs. Three match pinned native in
both worker modes: suffix priority, arbitrary-extension declaration resolution
with `allowArbitraryExtensions`, and a failed relative lookup. Nonempty suffix
payload and stem counters prove those branches execute.

The empty-suffix fallback case **fails native parity**. Configured
`moduleSuffixes: [".native", ""]` becomes `[".native"]` in TSR's effective
options because `declarations::string_list` drops falsy entries. With only
`dep.ts` present, both original and candidate omit that file and report TS2307.
Native retains the empty suffix, loads `dep.ts`, and reports the deliberate
TS2322 assignment error. Equal diagnostic counts would conceal this mismatch.
New P1 `tsr-6.59` owns the options conversion fix and a separate CLI override
audit. The allocation candidate did not cause or repair it. The public driver
finishes recording the failed comparison and exits 1; it is not a green gate.

## Reproduction and next action

[The temporary probe](resolver-candidate-copy-probe.patch) includes actual
checked-scope telemetry in the CLI. Build/save the normal binary, apply the
probe and save that binary, then restore source. Apply the candidate and save
its normal binary. For its scope control, apply only the probe patch's two
`compile.rs` marker additions. Use one build job and the recorded pinned source.

[The controls driver](resolver-candidate-copy-controls.py) accepts `--normal`,
`--probe`, optional `--candidate` plus `--candidate-scope`, `--project` and a
new `--output` directory. [The timing driver](resolver-candidate-copy-timings.py)
accepts `--baseline`, `--candidate`, `--baseline-scope`, `--candidate-scope`,
the completed control `--controls` JSON and a new `--output` directory.
[The public driver](resolver-candidate-copy-public.py) accepts `--normal`,
`--probe`, `--candidate`, `--candidate-scope`, `--tsgo` and a new `--output`.
All fixtures are generated public inputs. Drivers explicitly set the pinned
library path rather than depending on a build worktree remaining present.

Raw observations remain under `/tmp/tsr-resolver-candidate-copy`; committed
aggregates omit private paths, config contents, source and diagnostic text.
Portable driver replay was checked on all public cases; its expected exit 1
retains the `tsr-6.59` failure. The original measured driver hashes are retained
separately from the portable exports.

This result deprioritizes the measured path-copy boundary. Broader attribution
`tsr-1yb.2.1.3.1` still owns remaining discovery/condition cost and raw native
lookup controls. Dynamic discovery replay, byte budgets and private checker
worker policy remain the larger concurrency prerequisites. No repeat of this
copy experiment is justified without new locating evidence.
