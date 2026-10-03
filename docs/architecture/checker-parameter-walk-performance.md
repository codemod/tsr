# Type-parameter walk allocation experiment

This experiment belongs to `tsr-1yb.7.3`. Its source is
`d303c84b62199960632e8abea4704381b72a3871`; timing compares two saved TSR
binaries built from that source. It does not establish a native speed ratio.

## Current attribution

A two-second sample, delayed 1.8 seconds into a fresh local Next.js check,
captured 1,537 main-thread samples, all beneath checker frames. Subtracting
direct child counts from each stack node produced disjoint self attribution.
The top three nearest TSR owners were `mentions_type_parameter_inner` (105),
binder `resolve_name_excluding` (82), and structured property-name collection
(63). The full parameter-walk ancestry contained 109 samples. These locate
cost within the captured interval, not the entire checker phase or CLI.

Opt-in counters on the same source observed:

| Metric | Count |
|---|---:|
| Root predicate queries | 1,445,847 |
| Recursive predicate visits | 6,591,011 |
| Maximum previously visited length | 299 |
| Printed fallback calls | 356,806 |
| Fallback calls with no names | 295,505 |
| Rendered payload bytes with no names | 3,832,094 |
| Primitive/non-owned parameter leaf exits | 2,714,260 |
| Those exits at query roots | 563,202 |
| Cumulative root-leaf vector capacity bytes | 9,011,232 |

The sum of previously visited lengths was 87,415,417. It is an observation of
potential linear scan lengths, not an exact comparison count: owned parameter
matches return before scanning, and membership searches can terminate early.
Payload and capacity bytes exclude allocator metadata and other allocations.
Enabled/disabled counters retained the baseline's complete 123 diagnostics,
13,097 loaded files, 1,341 checked files and 13,560 parsed files. All temporary
instrumentation was removed before building either timed candidate.

## Separate candidates

The first candidate skipped only the final printed fallback when its name
list was empty. Its initial five-pair median improved from 4.896445 s to
4.849578 s. Independent confirmation measured 4.183730 s before and
4.190790 s after, inside the unchanged 20 ms comparison threshold.
The decision was inconclusive, and the original fallback was restored.
All 474,251 assertion rows were byte-identical. The initial apparent gain is
retained in the record and is not counted as a shipped improvement.

The retained candidate delays inserting a cycle marker until the type has
followed edges. String mappings and templates still insert their marker before
recursion, preserving their precedence over flag-based exits. Other structural
types insert it after the terminal flag check. Owned parameter identity is
still checked first. A terminal leaf has no traversed edges and returns false,
so leaving it unmarked cannot conceal a cycle. Later visits still check the
current parameter membership; no result is memoized between queries.

| Independent five-pair comparison | Reference median | Candidate median | Reduction |
|---|---:|---:|---:|
| Initial | 4.872008 s | 4.826506 s | 0.934% |
| Confirmation | 4.848724 s | 4.823153 s | 0.527% |

Both comparisons exceeded the existing 20 ms absolute threshold. Confirmation
user CPU medians were 3.789931/3.762777 s and system CPU 0.444034/0.449378 s;
peak RSS ranges overlapped. Every sample, including wall outliers, is retained.
Builds and other CPU-heavy checks from this task ran outside timed intervals.
The harness verified stable input content, loaded identities, effective options
and complete diagnostics between the two TSR binaries. It has not established
native checked-scope equivalence.

An unfiltered full-corpus comparison retained all 474,251 rows byte-for-byte:
459,451 RIGHT, 2,195 GAP and 12,605 WRONG. The common TSV SHA256 is
`f01585f8cafc7fd2fc198b22350c097d1baaa0a6037421369613895e34db55fb`.
The baseline CLI SHA256 is
`7b2b63565fac287ca4d0c6b0595283f11a2d0914da0862914eb8b661d11cd69d`;
the retained candidate is
`20dbca1746735f146acc5fdacc40c753b5f432732d42ee8ae49cd9738b6bbb30`.

All 136 selected release checker tests passed, including parameter identity,
current-registry, cyclic graph, generic-call, conditional and anonymous-object
controls. Checker lib/tests Clippy with warnings denied, formatting and diff
checks passed. Fresh CLI telemetry retained all 123 diagnostics and the same
loaded/checked/parsed counts. Bounded unresolved-Flatten and MCP controls
matched complete pinned-native diagnostics; the public smoke fixture retained
its existing three TSR versus one native diagnostics and does not prove parity.

## Reproduction and remaining scope

Raw profile/counter records are `/tmp/tsr-1yb-checker-d303c84b-direct.json`,
`/tmp/tsr-parameter-probe.json` and `/tmp/tsr-parameter-leaf-probe.json`.
The two candidates' distinct comparisons are
`/tmp/tsr-parameter-{empty,leaf}-{paired,confirmation}.json`; full assertion
audits are `/tmp/tsr-parameter-{empty,leaf}-corpus.json`.
Bounded CLI controls are `/tmp/tsr-parameter-leaf-controls.json`.
Counter patches and the experiment log are archived under
`.context/compound-engineering/ce-optimize/tsr-whole-project/`.

Timing uses the existing `scripts/whole_project_perf.py`, five alternating
fresh-process pairs, one warmup per binary and incremental/composite reuse
disabled. In these isolated experiments its `--tsgo` slot holds the saved TSR
reference. The actual native target remains a verified whole-project ratio
at most 0.50 with equivalent work. This small allocation gain does not achieve
that target. Broader traversal, instantiation and relation cost audits remain
open; semantic reuse still requires the native cache-key/lifetime contract.
