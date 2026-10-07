# Member query, publication and storage costs

At frozen TSR `53896fa42557cce7e09ca57337e4c3412c52c38c`, the rejected private
member-publication replay avoids many declaration lookups but leaves typed-member
forcing largely intact. It creates 56,360 private members across default workers
and reserves **12,058,624 inline bytes** in their actual symbol stores. The current
ordinary compiler has only the stores' sentinel records on this workload.
This selects a storage/publication design investigation, not another member memo
or a production optimization. The complete-work TSR/native wall target remains
unmet; this measurement supplies no native ratio.

The [receipt](checker-member-field-cost.json) preserves complete outputs,
source/build/input hashes, private drivers, setup failures, counters and both
replays. The [five-file patch](checker-member-field-cost.patch) is the ordinary
compiler's private observer. A combined fourteen-file publication-plus-observer
patch is embedded in the receipt. Neither is installed in canonical runtime.
The prior [rejected performance comparison](checker-member-builder-current.md)
remains source-bound to `bb982558`; its timings are not relabeled as this run.

## Actual query and publication boundaries

Private observers count calls at the actual typed-property, structured-name and
declared-symbol helpers. Timed mode measures inclusive root intervals; nested
helpers and observer clock cost overlap. These are locating intervals, **not
additive CPU shares, saved wall time or a performance ceiling**. Count, time and
repeat-count outputs match the unordered owner-counter records exactly after
excluding time fields; the observer does not emit stable owner IDs.

Default mode, summed across four checker owners:

| Actual boundary | Ordinary | Private publication replay |
| --- | ---: | ---: |
| Typed-property calls |233,754|245,519|
| Typed-property root interval |322.066ms|312.871ms|
| Structured-name calls |63,914|64,714|
| Structured-name root interval |38.537ms|39.194ms|
| Declared-symbol calls |837,654|244,495|
| Declared-symbol root interval |36.831ms|12.422ms|

The replay removes 593,159 declared calls, but their entire ordinary interval is
small and includes clocks and other work. Its publication boundary receives
646,700 requests: 640,627 completed hits, zero active hits, 6,067 actual image
workers and six unsupported requests. Completed hits are 99.06%. Declared fields
have 1,256 actual workers and 4,811 hits. Image root intervals total 40.196ms,
including bookkeeping and clocks. Read requests divide into 95,037 hits and
24,961 misses. Instantiation divides into 1,709 invariant reuses and 56,360 new
members; 611 reset observations are retained. At owner drop all 6,067 stored
images are completed, with no active image remaining. These counts do not prove
the broader active-frame/composite state machine: composites are zero here.

## Copy payload and real retained stores

The observer records five actual structured-field clone sites. Counts below are
default mode, not allocation events. String payload excludes allocator overhead,
signature vector contents, additional individual-entry name clones and other
copies outside these sites.

| Clone site | Operations | Member entries | String payload bytes |
| --- | ---: | ---: | ---: |
| Declared-cache hit |4,811|50,452|349,145|
| Declared provisional publication |1,256|10,866|71,819|
| Declared final publication |1,256|10,903|72,486|
| Initial image publication |6,067|61,355|421,631|
| Inherited base image |2,203|7,210|47,962|

The measured string payload totals 963,043 bytes. Final and initial publication
each copy nine call and five construct signature entries; index entries and full
arrays remain in the receipt. The image store retains 67,264 member entries,
457,376 name payload bytes and 56,360 links. Capacities are separately recorded.

The existing read-only `CheckerSymbols::storage_usage()` runs at its actual
owner's drop, without forcing queries. Default private publication stores hold
56,364 records (including four sentinels), with capacity 65,536. A record occupies
184 inline bytes, yielding 12,058,624 reserved inline bytes, plus 365,776 owned
name-capacity bytes and 265,464 declaration-capacity bytes. Single mode holds
42,098 records, including its sentinel, and reaches the same 65,536 capacity.
Ordinary default/single stores retain four/one sentinel records (736/184 inline
bytes). Inline capacity is a component count, **not total heap or RSS attribution**.
The stored handle size is 16 bytes in each owner, not a four-owner sum of 64.
Reference counts are retained snapshots, not a clone/drop operation census.

These prototype stores report zero table-edge capacity. The member constructor
sets `members` and `exports` to `None`; zero capacity alone would not distinguish
absent from present-empty tables. A future private layout must preserve all
three states and owned identity. It must not reuse the rejected bound-table
compact-layout result as a private-layout certificate.

## Verification and limits

Two complete twenty-child public batches cover ordinary/off/count/time/repeat
in both modes for both compiler roles. Every child preserves its role's complete
diagnostics and exit, ordered 265 loaded paths, reported 202 checked files, and
observed physical inputs. Extended timing lines differ, so the claim is not
byte-identical entire extended output. The actual checked identities and complete
filesystem query inputs remain unverified. Full originals and input snapshots
are preserved. First off-mode launches take 9.854/10.129s in the storage batch
and 11.851/11.659s in the earlier batch; these cold rows are retained and excluded
from speed selection. Ordinary final-batch observations are 0.526/0.659s default
and 1.150/1.462s single for main/replay. These single observations on an
uncontrolled host are not paired medians or a new retention decision.

Sixty-six terminal control children cover inherited generic call/construct/index
fields, ordered arguments and same-spelled namespace declarations, empty fields,
and a natural circular-default/reset case. Each role's observer modes preserve
its full ordinary stdout and exit. Native `5b1047d` and both compiler roles agree
on the first two controls; the reset case has native/replay TS2310, while ordinary
main incorrectly reports no error. One reset is observed in each mode, but no
positive active-hit or composite scope is certified by these controls. Counter
partition checks and repeated owner-counter multisets pass for all 106 children. A
reader-only counter mutation is caught; no compiling runtime mutation is claimed.

Builds pin Rust 1.96.0, native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`,
fixtures `4d4f005c8541e0255a9d8791205fdce326e462bc` and 108 embedded libraries.
The ordinary main executable is reused after all 632 compiled Rust files, 25 build
inputs and libraries equal the prior build. A fresh thirteen-file replay matches
all 633 prior publication-source hashes, with the same inputs/libraries, before
reusing that ordinary executable. These are not whole-repository censuses.
Four final observer builds exit zero with unchanged before/after source hashes.
Ordinary observer warnings concern unused private publication helpers; the
publication replay retains its unused `MemberLinks.containing` warning. No fresh
strict lint, full package, full type/diagnostic corpus or speed win is claimed.
Both observer patches apply in fresh directories and match final build hashes.

An initial observer injection assertion matched both own and inherited returns.
A partial build started before handling that failure; it was never measured and
is explicitly excluded. Corrected sources were restored from the frozen revision
before complete injection. A second dry-run failure, an initial patch replay
missing a new-file header, and a receipt-inspection type error remain recorded.
The exported reader initially omitted redacted loaded paths and used the wrong
checked-count label; both assertions were corrected against the original outputs
without rerunning compilers. The ordinary reuse proof initially treated library
basenames as relative paths; its corrected directory lookup passes. Driver hashes
bind originals before path redaction.
The staged whitespace check caught a standard blank context-space in the patch
artifact; its normalized blank line still applies and reproduces source hashes.
Documentation gates resolve 4,495 references with zero unresolved and zero
dangling section citations. The issue-ID gate retains 191 historical failures;
their containing files equal frozen main and all new cited task IDs resolve.
No user code, OS security configuration, executable signature or other agent's
process is changed.

## Existing ownership and next choice

`tsr-1yb.7.7.1.2` already owns `symbol_access.rs` storage/access cost acceptance.
Continue private-layout attribution there, coordinating with `tsr-1yb.33.1`'s
publication/identity work; do not cut a duplicate task. A layout proposal needs
native absent/empty/populated semantics, owned handles, actual integrated costs
and full previously-RIGHT preservation before retention. Its present opportunity
belongs to the private prototype, not today's ordinary sentinel-only stores.
`tsr-1yb.4.2.1` remains unfinished: one project's small declared interval does
not rule out every eligible costly builder. It must select actual expensive work
before implementation. `tsr-1yb.34` still needs the exact failing user invocation
and PR-specific source attribution; the separate first-launch diagnosis stands.
