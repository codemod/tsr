# Ambient module lookup performance

`tsr-1yb.7.6` isolates construction of the quoted name used by
`BindResult::ambient_module`. The baseline is `0fea91bf`; the native source pin
is `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

Native `checker.go::tryFindAmbientModule` rejects relative names, looks up
`"\"" + moduleName + "\""` with value-module meaning, and returns the merged
symbol when requested. TSR retains those policies at their existing owners:
the binder constructs the quoted key and follows redirects; the checker
handles relative/rooted rejection and module flags. This change adds no
semantic cache and does not alter declaration merging or wildcard resolution.

## Attribution and implementation

An opt-in full-app probe counted 470,108 lookups across 741 distinct
specifiers: 149,351 hits and 320,757 misses. The original `format!` constructed
470,108 temporary owned keys containing 8,651,611 payload bytes. Both enabled and disabled
controls retained all 121 complete diagnostics, 13,097 loaded files,
1,341 checked files and 13,560 parsed files. Probe timings are excluded.

469,338 keys fit in 64 bytes, including both quotes. The retained implementation
copies the complete specifier into a 64-byte stack buffer, adds the quotes,
validates UTF-8, and performs the same globals lookup and redirect traversal.
The remaining 770 calls retain the original allocating path. These counts
describe owned key constructions and payload bytes, not underlying allocator
calls, spare capacity, retained memory or peak RSS. The buffer and borrowed key
live only for the lookup.

A first variant using the existing `SmallVec` dependency measured
4.579081 s before versus 4.560970 s after in five fresh-process pairs.
Its 18.112 ms difference was below the unchanged 20 ms threshold; it was
classified inconclusive and rejected before the direct-buffer experiment.

## Isolated whole-project measurements

Both slots in these comparisons contain TSR binaries built from the same
baseline with only the bounded lookup change. The harness's `tsgo` slot is
a saved TSR reference in round one and the TSR candidate in round two.
These comparisons do not establish a native speed ratio.

| Five-pair round | Before median wall | Candidate median wall | Reduction |
|---|---:|---:|---:|
| First | 3.689150 s | 3.617458 s | 1.943% |
| Confirmation, reversed initial order | 3.734661 s | 3.699249 s | 0.948% |

Each round uses one warmup per binary, fresh compiler processes, a warmed
filesystem, no emit, and disabled incremental/composite reuse. Builds and
probes run outside timing. Every sample is retained, including the second
round's approximately 4.4 s outliers on both sides. Median user CPU changed
3.309269 to 3.255884 s and 3.340399 to 3.315844 s; system CPU changed
0.365026 to 0.364857 s and 0.378903 to 0.380456 s. Peak RSS ranges overlap,
so no memory reduction is established.

Both rounds preserve effective options, loaded identities, input fingerprints,
exit status and all complete diagnostics. A separate temporary probe confirms
identical checked-file identities and order for all 1,341 files, with fingerprint
`04d5f55be4e9253b8d9291375c46f171c6a829377ae41e28a2590b6480ede474`.
Production source was restored exactly afterward and the rebuilt CLI matches
the timed candidate SHA-256
`9787054f832ca8d6680eaee29b1d03bb12d2e104de901fed0e9e66736bed67c9`.

## Fidelity checks

The complete 474,251-row type verdict output is byte-identical to the baseline:
459,451 RIGHT, 2,195 GAP and 12,605 WRONG. The complete diagnostic probe,
including eligible clean cases, is also byte-identical across 10,570 judged
cases, with 7,630 matching cases. No previously correct assertion or diagnostic
is lost. Resolver and loader oracle transcripts retain 95/95 and 96/96 passes.

Controls cover empty, short and long names, both sides of the byte boundary,
multibyte UTF-8, embedded quotes/backslashes/newlines, missing names,
ordinary-global collisions, exact wildcard keys, merged redirects and actual
cross-file ambient declarations. Four CLI controls match complete pinned-native
diagnostics for merged ambient exports, UTF-8/boundary imports, an exact module
alongside a wildcard declaration, and missing relative/rooted/bare imports.
The existing imported-augmentation fixture preserves its baseline output but
still differs from native; `tsr-6.49` remains open and blocks worker readiness.

Binder/compiler/executor tests, 14 checker module-naming controls, affected
Clippy with warnings denied, formatting and whitespace checks pass. Temporary
instrumentation and the diagnostic-probe example are removed. Private app
sources and detailed diagnostics are not committed.

Local artifacts are `/tmp/tsr-ambient-lookup/{counts,controls,corpus,
checked-scope-result,stack-source,gates}.json` and the paired reports. The
[whole-project target](whole-project-performance.md) remains verified comparable
TSR/tsgo median wall at most 0.50; this isolated gain does not satisfy it.

A separate fresh five-pair native observation records 4.195727 s TSR versus
3.199680 s tsgo, observed ratio 1.311296. It remains incomparable: 13,097 versus
13,098 loaded files, 121 versus zero diagnostics, and existing reported-option
differences. Native additionally loads `@lingui/conf/dist/index.d.ts`.
The verified ratio is null and the target is unmet; scope/options alignment
and actual cross-tool checked telemetry remain `tsr-1yb.1.1`/`.1.2` work.
