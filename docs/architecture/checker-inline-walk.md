# Query-local inline cycle markers: rejected

The four-marker inline candidate is **not retained**. Its required single-worker
round regressed by 129.988 ms (2.745%), despite removing over one million scratch
allocation events. Canonical checker behavior and storage remain unchanged.
This completes the bounded experiment in `tsr-1yb.7.3.1`; it does not complete
the whole-project `tsr-1yb` target.

The frozen Rust source is `77e099605c7602f9ce785938739e083bbc60c398`.
The [machine-readable report](checker-inline-walk.json) binds source files,
ordinary and observed binaries, corpus outputs, raw local receipts and decisions.
The native oracle is pinned at `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

## Candidate and measured work

The [candidate patch](checker-inline-walk-candidate.patch) keeps four TypeIds
inline for each parameter-identity query, then spills larger walks into an
ordered vector with initial capacity eight. Unused initialized slots never
become cycle markers. Parameter checks still precede membership checks;
mapping/template markers still precede terminal string flags. No result cache,
global state, unsafe lifetime extension or dependency is added.

Fresh baseline and qualified candidate observations on the local Next.js app
have identical global and per-owner semantic counts, query-shape counts,
comparison counts, traversal branches, complete output and directly checked
file identities. Each mode has two enabled observations plus ordinary and
disabled controls. All retain 46 diagnostics, 14,050 loaded files, 14,746
reported parses and 1,397 directly observed full-file checks.

| Mode | Baseline scratch events | Candidate events | Removed requested bytes |
| --- | ---: | ---: | ---: |
| Default | 1,046,289 | 145,536 | 14,412,048 |
| Single worker | 1,305,843 | 238,642 | 17,075,216 |

The remaining candidate requested traffic is 5,865,728 and 9,551,328 bytes,
respectively. Both modes reach 50 markers and heap capacity 64. These are
cumulative successful allocation/reallocation requests, **not retained heap,
copied bytes or RSS savings**. Candidate capacity fields count heap backing;
the fixed four stack slots are excluded. The observer reader consequently
allows up to four visited identities with zero heap capacity. Larger walks
explicitly copy the four inline identities when spilling. Their copy cost was
not separately measured.

## Ordinary CLI decision

The frozen retention rule requires greater than 20 ms median wall improvement
and at most 5% median peak-RSS growth in each required default/single comparison,
with two independent five-pair rounds before retention. Builds, probes and
corpus runs are outside timing. Each completed round uses one warmup per binary
and five alternating fresh-process pairs with incremental/composite disabled.

**Both performance slots are TSR.** The harness's `tsgo` slot contains the saved
ordinary TSR baseline. These comparisons cannot establish a native speed ratio;
the harness leaves `work_comparable` false and `verified_wall_ratio` null.

| Mode, first round | Baseline median | Candidate median | Median change | Decision |
| --- | ---: | ---: | ---: | --- |
| Default | 4.924066 s | 3.094677 s | -1.829390 s | Numeric eligibility only; very noisy |
| Single worker | 4.736096 s | 4.866084 s | +0.129988 s | Revert |

Default samples span 2.919–5.348 s for the baseline and 2.869–4.757 s for the
candidate. That variability prevents a confirmed causal gain claim. A separate
coverage process was observed and allowed to finish before timing; desktop and
metadata-indexing activity remained observable. No globally idle-host claim is
made. Single-worker median user/system CPU is 4.227740/0.547262 s for the baseline
and 4.246070/0.572296 s for the candidate. All sample CPU/RSS values and ranges
are preserved in the report.

Median RSS changes are -0.360% default and -2.704% single. Both pass the memory
gate, but cannot override the required wall failure. `decide.mjs` returns
terminal `revert`, with no further measurement requested, for single mode.
The independent confirmation is not run for this rejected candidate. No rounds
or modes are averaged, no threshold is relaxed, and two independent rounds
remain mandatory for any future retention.

The allocation reduction does not establish the cause of the wall result.
Inline/spill membership branching and larger root stack state are additional
work; this experiment did not isolate their individual time costs.

## Correctness and restoration

Both initial and lint-qualified candidates reproduce the entire baseline
476,787-row type output and 10,570-case diagnostic output byte-for-byte. The
type corpus retains all 465,643 previously RIGHT assertions; diagnostic cases
include empty expected outputs and duplicate occurrences. No case filter is
introduced by this experiment.

Three focused controls exercise empty prefixes, ordered spill/growth and
duplicates, cycles, declaration identity, registry/metadata changes between
queries, and mapping/template precedence. Treating unused slots as markers
deliberately fails all three initial controls. The mutation is restored exactly.
A later lint correction passes the four-byte TypeId by value; its new ordinary
binaries receive fresh full-corpus and real-project receipts. The report keeps
the initial mutation/source identity separate from the qualified source.

The qualified candidate passes 1,493 checker tests with three existing ignored,
eight archive observer tests, strict checker/execute Clippy and repository
formatting. Four public fixture families have eight complete native diagnostic
matches across default/single modes. Direct full-file agreement is a scoped
preservation check, not proof of complete cross-tool lazy semantic-work
equivalence. Observed physical inputs exclude embedded libraries and absent,
directory, resolver-query and transient state.

All four private runtime source files and the three pre-existing private target
binaries are restored to their recorded original hashes. The shared vendor and
canonical runtime are unchanged. The release requirement remains complete,
comparable fresh-process TSR/pinned-tsgo median wall ratio at most 0.50; it is
unproved. Further graph candidates need new evidence of whole-CLI benefit.

## Source replay

Use an isolated checkout of the frozen source, not the active shared checkout:

```sh
python3 docs/architecture/replay_checker_inline_walk.py \
  --source /path/to/private/frozen-checkout --variant candidate
```

For the counter variant, start from another clean frozen checkout and use
`--variant observer`. The [observer patch](checker-inline-walk-observer.patch)
and existing graph observer reconstruct all five measured source files; the
replay checks their hashes. The observer's `contains` implementation counts
the same scalar comparisons over the populated slice, while its tagged `push`
calls the candidate storage operation. Its unused helper membership method
therefore produces an archive-only dead-code build warning; ordinary code
passes strict Clippy.

After arranging the pinned vendor checkout as in the
[graph audit](checker-graph-walk.md), build ordinary binaries with:

```sh
cargo build --release --offline -p tsr -p tsr-conformance \
  --bin tsr --example verdictdump --example diagverdictdump -j 2
```

Save ordinary binaries before installing any observer. Observed allocation runs
use `TSR_GRAPH_PROFILE=1` and `TSR_GRAPH_TRACE`; ordinary timing clears those
variables and uses the saved binaries. Existing graph controls need the frozen
source binding and the explicit four-inline-slot capacity invariant for this
candidate. The archived patches support investigation; they are not a proposed
production change.
