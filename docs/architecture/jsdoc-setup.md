# JSDoc attachment and private setup costs

Frozen Rust `a0fa106e3f8ac60551292e98d1f0f47f34a55296`, pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`: attachment, binder copies and
per-checker registration together cost a median **13.431 ms** in the archived
observer on Next.js. This supports leaving those setup APIs unchanged for now.
Conservative plain TS comment bodies cost 81.088 ms. That body component is
still useful input to `tsr-1yb.9.2.1`, which owns lazy borrowed-result lifetimes;
it does not establish the benefit or full ceiling of deferral.

No production optimization, memory reduction or native speed ratio is accepted.
The complete equivalent-work TSR/native median wall target <=0.50 remains
unmet and unverified. [Source-qualified receipts](jsdoc-setup.json) preserve
the timed and quality snapshots separately. These measurements do not replace
the historical [eligible-body checkpoint](jsdoc-eligible.md).

## Disjoint boundaries

The observer brackets both compiler parse entry points, including duplicate
parses later discarded. Body timers start after valid-comment recognition and
coalesce nesting. Plain is a subset of body. Top-level range discovery and
document-list arena copying exclude comment bodies. Attachment runs after
leading parsing; nested attachment is reported separately and is zero here.
Binder collection stops before `bind_into_with_jsdoc`. Checker registration
includes borrowed-table iteration and insertion into both private maps, before
the normal semantic observer is attached. Constructor timing includes option
application and excludes registration.

| Next.js interval | Median ms | Sample range ms |
| --- | ---: | ---: |
| Inclusive compiler parse | 661.296 | 648.159–680.273 |
| Inclusive leading JSDoc | 252.401 | 249.036–256.200 |
| All outer comment bodies | 191.651 | 188.439–195.669 |
| Conservative plain bodies, subset | 81.088 | 80.530–83.454 |
| Top-level range discovery | 29.954 | 29.919–31.079 |
| Document-list arena copy | 3.363 | 3.290–3.434 |
| Parser attachment | 4.565 | 4.422–4.676 |
| Binder table vector collection | 1.263 | 1.202–1.399 |
| One checker's JSDoc registration | 7.806 | 7.493–8.025 |
| Checker construction/options | 0.048 | 0.045–0.121 |

The 13.431 ms combined setup value is the median of each sample's disjoint
attachment + binder-copy + registration sum, rather than a sum of medians.
It is about 0.28% of normal baseline wall, at observer scale. Plain body is
about 1.71%. These are component estimates with zero replacement cost, not
confirmed end-to-end savings. Leading includes the range/body/list intervals;
do not add them to leading or parse. The old observer's all-JSDoc guard also
enclosed leading parsing, so its 242.313 ms label must not be compared to this
191.651 ms outer-body interval as an optimization gain.

Every enabled app sample covers 14,746 parses, 202,572 leading and attachment
calls, 202,954 outer bodies and 135,767 plain comments. Binder collection visits
14,050 loaded files; registration visits 13,991 root/referenced files, excluding
the 59 bundled-library entries. Plain bodies request 24,126,296 arena payload
bytes; all bodies request 59,482,207. These are cumulative payload requests,
excluding arena capacity, temporary heap storage and retained residency.

The already-shipped typedef-kind guard declines 685,949 ordinary references;
there are zero eligible typedef document scans on this app. This confirms the
guard's boundary, without treating avoided historical scans as a new saving.

## Allocation origins and worker overlap

An archived System allocator prefix preserves the original owner through
resize and cross-thread free. Only allocations initiated by JSDoc registration
are assigned private owners 1–4. Parser attachment vectors use owner128 and
temporary binder collections owner129. Other checker stores, arena blocks,
stacks, native allocation and system usable sizes are outside these counters.
Successful allocation/resize requests accumulate full requested layout sizes;
they are not net retained bytes. Prefix padding is additional observer storage.

| Actual private checkers | Registration requests | Cumulative requested bytes | Observed simultaneous requested peak bytes |
| ---: | ---: | ---: | ---: |
| 1 | 34 | 17,825,928 | 12,189,720 |
| 2 | 68 | 35,651,856 | 24,379,440 |
| 4 | 136 | 71,303,712 | 48,758,880 |

These peaks come from one concurrent aggregate counter, not addition of
independent owner peaks. They include map growth transients and are not steady
retained bytes or portable RSS ceilings. At the pre-check boundary every
checker has 191,568 host-entry keys and 191,930 doc-host keys; both maps retain
capacity for 229,376 entries. Capacities are observed slots, not estimated
allocator bytes. All tagged live and padded requested bytes are zero after the
invocation returns.

Attachment vectors make 17,220 allocation/resize requests totalling 13,513,632
bytes, with a 7,211,424-byte aggregate requested peak. Temporary binder copies
make 9,237 requests totalling 4,774,248 bytes, with a 590,424-byte peak. These
origins include loaded files and discarded parse results; they do not isolate
only eligible plain comments. Do not subtract independently measured RSS
maxima to estimate them or combine them with historical private-store peaks.

## Preservation and uncertainty

Three fresh normal CLI samples take 4.710/4.730/4.758 seconds. Three alternating
probe-off/on pairs preserve 117 complete diagnostics, 14,050 loaded inputs,
14,746 parses, effective options and ordered physical source payloads. Off/on
wall medians are 5.146/4.944 seconds; paired deltas are -0.905/+0.665/-0.222
seconds. The allocator prefix remains in the disabled probe binary. Clock,
counter and allocation overhead, plus broad variance, prevent a performance
winner claim. Normal/probe-off/probe-on RSS medians are 1,251,295,232 /
1,220,739,072 / 1,242,873,856 bytes; all wall, CPU and RSS samples are retained.

A separately captured completed CLI work trace and all 1/2/4 helper runs have
the same ordered 1,397 actual full-file identities. Helpers preserve the exact
normalized 117 diagnostic payloads, with a different display order. This
qualifies full-file scope, not all initialization, lazy forcing, resolver inputs
or native equivalent work. The first trace capture lacked its own before-child
logical-input snapshot and is excluded; the replacement has independent
before/after input snapshots and passes the existing artifact reader.

Thirty fresh public children test normal/probe/native plus private1/2/4 across
checked JS, checkJs=false, noCheck, file no-check and bundled-library checking.
All complete diagnostics match: two deliberate errors when checked, zero when
excluded. Actual constructed map owners remain separate from checked work.
Another thirty children qualify the lint-corrected observer. The stronger mixed
JS negative still has **native4 versus normal0/probe0**; this explicitly rejects
any complete-work speed conclusion. Existing `tsr-6.65` ownership/reporting
tasks retain that gap.

The [standalone eager-query control](jsdoc-setup-query.rs) links the timed
release libraries and verifies that first and repeated table reads return the
same borrowed slice with zero additional arena bytes or global allocation
requests. Current tables are eagerly parsed; there is no lazy first/repeat
materialization to time. That future API remains `.9.2.1`. Dropping documents,
filtering transport by JS filename or adding a shared semantic cache is outside
this measurement.

## Reproduction and quality

Archive the frozen source with pinned vendor paths available, then apply the
[timed patch](jsdoc-setup-timed-probe.patch) using `patch -p1 --batch --fuzz=0`.
Build CLI and `checker_workers` with features
`tsr/jsdoc-setup-probe,tsr/work-trace,tsr-execute/jsdoc-setup-probe`. Set
`TSR_JSDOC_SETUP_PROBE` to a fresh output path. The trace is written only after
the invocation returns. [The reader](jsdoc-setup-controls.py) binds PID/start,
terminal child outcome, parse coverage, private owners, completion and separate
allocation domains. It rejects partial/stale captures, leaks, inconsistent
capacities and impossible subset/aggregate counters.

The [quality patch](jsdoc-setup-probe.patch), [observer module](jsdoc-setup-probe.rs)
and [meter](jsdoc-setup-meter.rs) differ only by four must-use annotations and a
private tuple type alias added after timing. Exact no-fuzz replay matches all
18 source hashes for each variant. Timed binaries and source files remain
immutable; quality timings are not substituted for original samples.
Strict release libraries/binaries and the worker example pass Clippy; 24
existing parser JSDoc tests, four allocator ownership/alignment/resize/failure
tests, the eager-query control and eight receipt tests pass. The feature cannot
be combined with the existing parser `alloc_profile` example's global allocator;
all-example Clippy is explicitly unsupported. No production corpus or runtime
throughput retention gate was appropriate for this archive-only attribution.
Independent review coverage is zero.

Keep `.9.2` open. The measured setup component does not justify an additional
cache or binder API rewrite on this workload. Settle the borrowed lazy-result
contract, retain required JS/tag/import/link behavior and fix native-required
diagnostics before testing a deferral candidate. Any retained implementation
still needs no previous RIGHT losses and two independently confirmed matching
fresh-process whole-CLI timing rounds.
