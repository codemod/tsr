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

### Commit 2: `isTypeRelatedTo` entry read

Corpus dumps byte-identical to the baseline again. Perf gate, 21 samples,
median child CPU new/old (wall): domain-model 0.9796 (0.9993),
generic-imports 0.9895 (0.9919), domain-model-large 0.9422 (0.9988).
Callgrind, generated 100 modules: 4,405,146,014 Ir. `jsTyping`: 27.1/28.8 s
(commit 1) → 24.6/24.7 s, interleaved runs, identical output.

Counter run on `jsTyping` (temporary, not shipped): 8.8 M top-level relation
calls; 1.28 M structured-walk entries, 1.03 M completed hits, 66 k `Unknown`
structured results. The call volume comes from narrowing callers
(`map_narrowing_type`, flow), which this lane does not own.

### Where `jsTyping`'s remaining time goes (after commit 2)

- 30 all-thread `gdb` samples: relation frames in 10 of 16 checker stacks,
  every one inside `recursive_type_related_to`; callers are narrowing
  (`map_narrowing_type`/`get_flow_type_of_reference`, 7-8) and call
  resolution (`check_call_expression`, 9; `choose_overload`, 4); member
  reads under the walk (`get_property_of_type_ex` 9, `heritage_entity_symbol`
  4, `resolve_name` 5).
- `reasons` counters (temporary probe, f3fad58): at 8.0 M top-level calls,
  3,230 top-level walks answered `Unknown`; site notes: no members table
  26,566, unfollowable base 232, unported flag 810. Uncacheable `Unknown`
  re-walks are therefore not the main remaining cost.
- What is left is call volume (8.8 M top-level relations, a flow/narrowing
  question) and the cost of first-time structured walks (member
  resolution and relation arms). Neither is owned by this lane.

## Left in the lane

- Generic-reference key equivalence (native `'g'` keys and the
  broadest-equivalent `maybeKeys` probe) — `tsr-1yb.4.1.4`.
- Reliability flags (`ReportsUnmeasurable`/`ReportsUnreliable`) and the
  overflow flags — `tsr-1yb.4.1.3`.
- The remaining 16x wall gap on `jsTyping` is not profiled here.
