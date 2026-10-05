# Conservative plain-documentation opportunity

`tsr-1yb.9.2` remains in progress. The current-source conservative slice costs
83.374 ms of documentation-body work on Next.js; no production deferral or
final no-change decision is accepted. A supported private lazy-document owner,
attachment/consumer costs and final correctness/timing gates are still needed.
The native full-work median wall ratio <=0.50 remains unverified.

## Source and measurement

Rust source is `a24cacb2764b5882e38a83fa95eec65c5d10bfb7`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. [Receipts](jsdoc-eligible.json)
record exact archive/patch/source/binary hashes, effective options, physical
payload and complete diagnostic fingerprints, fresh PID/start/outcome,
all individual wall/CPU/RSS samples, raw artifact hashes and work-trace verdicts.
The observer exists only in clean archives; canonical runtime source is unchanged.
The public source generator and native boundary controls are the existing
[JSDoc cost tools](jsdoc-cost.md).

The classifier requires a valid JSDoc comment, a TS/TSX/declaration filename
(`.ts/.tsx/.mts/.cts`, case insensitive), and no ASCII `@` anywhere in the
comment. JS, JSON, semantic/unknown tags, inline links, see/deprecated/imports,
email text and fenced text containing `@` remain eager. This is deliberately
more conservative than native TS lazy eligibility; it does not assert all
tag-bearing TS comments require eager parsing. Original parsing still runs.

Three alternating off/on pairs on each of the app and mixed public fixture
preserve complete diagnostics, options, reported counts and ordered physical
source payloads. Separate intrusive off/on worker traces qualify the same
1,397 actual app full-file workers and six public workers. App output remains
117 complete diagnostics, 14,050 loaded and 14,746 parsed files. The public
positive fixture remains zero errors. Loaded payloads and root config are
partial input coverage; all resolver queries, initialization and lazy forcing
are not established by these traces.

| Next.js body attribution | Median | Sample range |
| --- | ---: | ---: |
| Conservative plain body time | 83.374 ms | 83.318–84.153 ms |
| All JSDoc body interval time | 242.313 ms | 241.869–243.943 ms |
| Inclusive compiler-file parse intervals | 643.017 ms | 642.216–647.915 ms |
| Plain comments | 135,767 | identical |
| Plain cumulative requested arena bytes | 24,126,296 | identical |
| Plain source-range exposure bytes | 18,124,337 | identical |
| All cumulative JSDoc requested arena bytes | 61,105,359 | identical |

Declarations contribute 134,536 plain comments and 82.565 ms (category median);
ordinary TS contributes 1,018 and 0.674 ms, TSX 213 and 0.151 ms. JS/JSON plain
counters are zero. Category medians are not added as if they formed one sample.
The public median plain body time is 5.910 ms; its bundled libraries are included.

The observed plain body component is about 34.4% of all documentation-body time
and about 1.8% of the 4.618803 s probe-off wall median. Removing that entire
component at zero replacement cost is an optimistic **body-component** estimate,
not a ceiling on full deferral: range discovery, attachment, table growth and
downstream consumers are outside the per-comment interval. Their contribution
and private lazy-owner construction/query costs remain unknown. Do not count
the 24.1 MB as retained storage or RSS; requested arena sizes exclude chunks,
padding, temporary heap allocations and discarded duplicate parses.

## Timing uncertainty and observer limits

Normal wall samples are 4.983483 / 5.238760 / 5.819804 s. Probe-off samples are
4.522473 / 4.618803 / 5.193998 s; on samples 5.527876 / 5.471020 / 5.600951 s.
The paired observer wall deltas are 1.005403 / 0.852217 / 0.406954 s, median
0.852217 s. Wall/RSS variability and observer cost prevent a normal performance
winner claim. App off/on user medians are 4.086171/4.318908 s, system medians
0.454785/0.910791 s, peak RSS medians 1,196,818,432/1,209,352,192 bytes; complete
ranges are in the JSON. These independent RSS maxima are not subtracted to
estimate comment memory.

The existing leading guard brackets range scanning and full JSDoc work. A
nested-safe per-comment guard brackets only eligible outer bodies and requested
arena growth; subset counters cannot exceed their enclosing counters. Filename
classification happens before inclusive file timing, and `@` classification
precedes the plain-body timer. Per-file writes happen after parser return.
Measurement includes all compiler parse calls, including duplicate results later
discarded. No mutable Program AST, shared arena allocation or TypeId cache is
introduced.

## Documentation preservation and missing checking

The existing `a_documented_node_can_be_found_from_its_id` test passes with the
observer. A separately archived deliberate mutant returns no document for
untagged comments; that test fails at the documented-node lookup. Omission is
not lazy parsing. Keep standalone parser defaults, full comment/node identity
and raw comment emit. The private owner/range/ID requirements in
[the original handoff](jsdoc-cost.md#bounded-experiment-and-ownership-handoff)
remain prerequisites; native's file mutex cache is not a Rust ownership design.

The stronger mixed checkJs negative fixture still reports native four errors
versus TSR zero; all ordinary/off/on TSR outputs agree. Parent `tsr-6.65` remains
open. A current-source consumer audit additionally finds that conformance
`configured_checker` calls `set_jsdoc` for Program root/referenced files, while
the production CLI and worker helper omit that transport. A minimal actual CLI
negative checks one JS file but emits zero errors for `@type {number}` assigned
a string. An isolated transport-only candidate also emits zero: the missing
table is not a sufficient causal explanation. The initializer reporter also
excludes every JS declaration and reads only syntactically written annotations.
An archived candidate supplying tables and using the existing effective JSDoc
annotation reader passes the minimal primitive negative and its checkJs=false /
noCheck controls. It still emits zero errors on the stronger mixed negative;
that candidate is not retained. `tsr-6.65.1` owns transport, `.2` owns effective
initializer checking, and the parent retains template/typedef/import/overload
and complete-output obligations.
Do not score faster incomplete JS checking as an optimization.

A separate instrumented host-edge prototype resolves the previously `any`
typedef/imported targets and unresolved template return, recovering three
complete native errors. The overload error remains missing. `tsr-6.65.3` owns
the lexical-owner contract and current-main corpus validation; this locating
observation is not a retained fix or a speed receipt.

The subsequent [CLI fidelity checkpoint](jsdoc-cli-fidelity.md) records full
corpus component isolation, rejection of the blanket host fallback, effective
initializer/context transport and the real-project document-scan failure.
Its exact source and runtime gates are separate from this frozen observer
experiment; the original attribution samples are not relabeled as current
compiler performance or complete checking.

Next steps within `.9.2` are attachment/consumer attribution and an executable
private lazy-document ownership/query contract before an actual deferral
candidate. Preserve JS/tags/link/import/deprecated behavior, supported queries
and emit, complete diagnostics and prior RIGHT assertions; retain only after
two independently confirmed normal-CLI timing rounds on matching work. This
checkpoint completes eligibility attribution, not the implementation task.
P1 `tsr-1yb.9.2.1` owns the borrowed-result query contract; `.9.2.2` owns costs
beyond the parse body, including attachment and per-checker transport/retention.

## Reproduction and validation

Archive the exact Rust source above with pinned vendor available. Apply
[the ten-file observer patch](jsdoc-eligible-probe.patch) with `patch -p1 --batch
--fuzz=0`; fresh replay matches every recorded source hash. Build an ordinary
release `tsr` and a separate `--features jsdoc-cost-probe,work-trace` executable.
The [observer module](jsdoc-eligible-probe.rs) supplies no semantic query results.

```sh
rtk proxy python3 docs/architecture/jsdoc-cost-controls.py fixtures /tmp/plain-doc-public
rtk proxy python3 docs/architecture/jsdoc-eligible-controls.py pairs \
  --normal /absolute/normal-target/release/tsr \
  --probe /absolute/probe-target/release/tsr \
  --project /tmp/plain-doc-public --output /tmp/plain-doc-pairs
rtk proxy python3 docs/architecture/jsdoc-eligible-controls.py work \
  --binary /absolute/probe-target/release/tsr --source /absolute/probe-source \
  --source-sha a24cacb2764b5882e38a83fa95eec65c5d10bfb7 \
  --project /tmp/plain-doc-public --output /tmp/plain-doc-work
rtk proxy python3 docs/architecture/jsdoc-eligible-controls.py negatives \
  --normal /absolute/normal-target/release/tsr --probe /absolute/probe-target/release/tsr \
  --native /absolute/pinned-native --output /tmp/plain-doc-negatives
rtk proxy env PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s docs/architecture -p test_jsdoc_eligible_controls.py
```

Run pairs/work again on the real project in fresh output directories. One
classifier test, seven reader tests, 26 existing parser JSDoc tests, the rejected
omission mutant, strict archived release all-target Clippy and probe rustfmt pass.
The reader rejects stale/wrong PID/version, partial/signaled outcomes, missing
parse coverage, non-TS eligibility and counters exceeding enclosing work.
No full-corpus runtime gate or retained-speed confirmation is claimed for this
observer-only checkpoint. Independent review coverage is zero.
