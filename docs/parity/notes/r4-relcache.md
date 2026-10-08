# r4-relcache — persistent relation results (`tsr-2zk.902`)

Round-4 lane notes. The design record (key, owner, lifetime, publication,
frames, diagnostics, work boundary, alternatives, falsifier) is
[checker-relation-publication.md](../../architecture/checker-relation-publication.md#checker-lifetime-results-tsr-2zk902);
this file keeps the lane's measurements and what was left.

## Hypothesis confirmed

`relate_ternary` built a fresh `Relater` and results map per call. Baseline
binary (`b23dd3d`) on `src/jsTyping`: still running at 120 s (`timeout`
exit 124). Three all-thread `gdb` samples at 30-40 s: every sample in
`narrow_type_by_type_predicate`/`map_narrowing_type` → `relate_*` → 23-30
nested `recursive_type_related_to` frames with
`properties_related_to_with_optionals`, `get_property_of_declared_symbol`,
`generic_heritage_member` — inherited-member re-resolution under repeated
narrowing queries.

## Measurements (box, 4 cores, stable 1.97 toolchain)

| Check | Before | After |
|---|---|---|
| `verdictdump` (types) | 477,985 lines, 469,946 RIGHT | byte-identical |
| `diagverdictdump` | 10,570 cases, 4,232 RIGHT, 4,968 EMPTY_RIGHT | byte-identical |
| `jsTyping` (`types: []`) | >300 s (never finished) | 25.1 s; native 1.50 s |
| `typingsInstallerCore` (`types: []`) | >300 s | 24.9 s; native 1.57 s |
| callgrind, generated 100 modules | 4,530,691,360 Ir | 4,416,164,874 Ir (−2.5%) |

Perf gate, 21 samples, median child CPU new/old (wall new/old):
domain-model 1.0019 (1.0115), generic-imports 0.9991 (1.0024),
domain-model-large 0.9826 (1.0253); `diagnostics_match` true on all three.

## Left in the lane

- Generic-reference key equivalence (native `'g'` keys and the
  broadest-equivalent `maybeKeys` probe) — `tsr-1yb.4.1.4`.
- Reliability flags (`ReportsUnmeasurable`/`ReportsUnreliable`) and the
  overflow flags — `tsr-1yb.4.1.3`.
- The remaining 16x wall gap on `jsTyping` is not profiled here.
