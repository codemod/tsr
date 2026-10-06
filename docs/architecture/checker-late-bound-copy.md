# Late-bound member copying: measured lookup prototype rejected

The `tsr-1yb.16.2.3` observer and `tsr-1yb.7.5.1.1` prototype are frozen at
`48e300a476d6019aee36870fe4baa6d0d8115fab`, native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, and fixtures
`4d4f005c8541e0255a9d8791205fdce326e462bc`. The prototype eliminates measured
copies, but fails the required ordinary public wall-time gate. It is restored;
canonical runtime is unchanged. This does not establish a fix for the reported
PR #5 slowdown (`tsr-1yb.34`) or the complete-work TSR/tsgo target **<=0.50**.

[The receipt](checker-late-bound-copy.json) preserves identities, selected
counts, actual checked-file identity hashes, complete public output blobs,
reader controls, original forecasts, and the rejected measurements.
[The observer patch](checker-late-bound-copy-probe.patch) and
[the rejected candidate](checker-late-bound-copy-candidate.patch) are archive
artifacts, not changes loaded by the compiler.

## What the existing helper does

At this source, `members.rs::late_bound_members_of` caches an ordered
`Vec<(String, NodeId)>` under `(SymbolId, is_static)` in one private Checker.
A hit clones it. A miss first inserts an empty re-entry marker, evaluates
computed declaration keys, skips keys it cannot name, then publishes
`out.clone()` and returns `out`. Its only production writers are those two
inserts; test-only removal is not a production invalidation mechanism.

Six consumers are attributed separately: accessor pairing, method overloads,
static name enumeration, static lookup, instance name enumeration, and
instance lookup. The static wrapper also constructs a projected vector;
accessor and overload consumers can copy a selected name afterward. Those
copies have separate tags and are not charged again to the helper.

The pinned native operations are `getLateBoundSymbol`,
`getResolvedMembersOrExportsOfSymbol`, `lateBindMember`, and
`getPropertyOfType`/`getPropertyOfTypeEx` in `internal/checker/checker.go`.
Native publishes resolved member/export symbol tables. This Rust vector is
not that completed semantic image; neither a hit nor an empty vector proves
native-supported absence. Existing member publication and receiver contracts
remain prerequisites for broader reuse.

## Current copying cost

The Next.js comparison preserves four diagnostics, 14,051 ordered loaded
files and 1,397 actual full-file checks. Default uses four private owners;
single-worker mode uses one. All selected non-timing counts and retained
capacities repeat exactly. Every selected global count equals the private
owner sum, and site/kind allocation totals reconcile with allocator tag 8.

Default cache-hit copies on the app:

| Consumer | Calls | Copied strings | Allocations | Requested bytes |
| --- | ---: | ---: | ---: | ---: |
| Accessor pair | 1 | 1 | 2 | 50 |
| Method overloads | 82 | 128 | 210 | 6,234 |
| Static names | 143 | 10 | 20 | 520 |
| Static lookup | 219 | 0 | 0 | 0 |
| Instance names | 667,162 | 196,266 | 313,961 | 9,887,013 |
| Instance lookup | 4,116,738 | 500,987 | 843,476 | 25,336,972 |

Single-worker instance lookup copies 342,898 strings across 2,345,040 hits:
597,299 allocations and 17,314,687 requested bytes. Its copied string payload
is 6,341,951 bytes. Across all measured copy kinds, the app requests
35,251,910 bytes default and 23,352,996 single. These cumulative allocation
requests are distinct from retained capacity and peak RSS.

Instance lookup's clocked copy intervals total 258.7–269.2 ms default and
86.2–86.6 ms single. They include clock/TLS/allocator observer overhead and
overlap across workers; they are neither wall savings nor a saved-wall ceiling.
The public domain workload's corresponding intervals are only 9.5–12.1 ms.
The dispatch forecast was a precise allocation reduction with unknown ordinary
wall benefit, including the risk that the public 20 ms gate would fail.

At owner completion, the app retains 11,939 cache entries default, of which
11,666 are empty: 380 elements, 12,160 vector-capacity bytes and 6,753 string
capacity bytes. Single retains 9,128 entries, 9,031 empty, 138 elements,
4,416 vector-capacity bytes and 2,370 string-capacity bytes. Hash-table bucket
capacity is reported separately. Empty entries include more than one semantic
state and must not be relabeled as completed native answers.

## State and collector controls

There are 104 baseline normal/off/on/repeat runs and 13 listings across
13 projects and two worker modes. The eight initial public fixtures cover
computed, static/instance, symbol, numeric, empty, accessor, overload and
recursive-key behavior. Additional fixtures distinguish an unsupported key
that is not forced, one that is forced, and an actual recursive dependency.
The forced unsupported control records one `no_name` outcome, followed by a
successful independent computed lookup. The recursive dependency records two
actual active-marker reads, one unsupported-name outcome, and a later valid
lookup. Both repeat exactly in default and single modes. Same-owner recovery
after a test-only marker removal is covered separately; no production recovery
or invalidation protocol is claimed.

The active label requires a live helper worker frame with the same Checker,
owner and static domain. An empty vector alone does not produce that label.
The observer does not count empty calls separately when widths are mixed.
Twenty existing helper tests, including active-marker/reset, static/instance,
inheritance and receiver controls, pass with the observer enabled. Two collector
tests distinguish active identity, stored-empty reads and downstream copies.
The actual executed reader accepts two positive controls and rejects eleven
schema, PID, completion, duplicate, ownership and double-attribution mutations.

Twenty-two public native runs complete; ten match complete diagnostics.
Preexisting mismatches remain explicit: computed/static/numeric assignments
omit property-type chains (`tsr-6.55`), computed accessor writes omit TS2322
(`tsr-6.72`), and recursive key/dependency initializers omit native circular
diagnostics (`tsr-6.73`). Matching primary lines is not full native parity.

## Bounded candidate and ordinary rejection

Only `get_property_of_declared_symbol` changes in the private prototype.
On a miss it calls the same helper, retaining its key, forcing, re-entry marker
and publication. On a hit it borrows the existing ordered vector and returns
the first matching declaration **with an available binder symbol**. A plain
first-name search could stop at an unbound duplicate; a focused test protects
the original filtering behavior. Early-bound priority and base traversal stay
at their existing boundaries. The borrow does not survive a mutable query or
introduce an interner, new cache, or cross-checker identity.

All 78 candidate off/on/repeat runs preserve complete baseline output and
actual check identities. The exclusive copy audit across 26 project/mode
groups shows exactly 843,476 allocations / 25,336,972 requested bytes removed
on the default app, and 597,299 / 17,314,687 in single mode. Other selected
non-timing counters, worker starts, publications and retained capacities stay
unchanged per owner. The copies are eliminated, not shifted to another tagged
site. The ordinary candidate's 187 library tests pass, including two new
duplicate-identity and inherited-marker/reset controls.

The unchanged retention policy requires two independent five-pair rounds on
the public domain workload and private app, in both worker modes. Each must
save at least 20 ms against ordinary baseline and a byte-identical A/A
reference, with median CPU/RSS ratios <=1.05 and complete output/input controls.
Full previously-RIGHT and changed-diagnostic corpus/native checks are also
required before production retention.

The first required public round completes all 36 children, including warmups:

| Domain mode | Baseline median | A/A median | Candidate median | Saving vs baseline / A/A |
| --- | ---: | ---: | ---: | ---: |
| Default | 0.557122 s | 0.564955 s | 0.555741 s | 1.38 / 9.21 ms |
| Single | 1.281903 s | 1.292044 s | 1.291063 s | -9.16 / 0.98 ms |

Both wall gates fail. CPU and RSS pass against both references. Host activity
is uncontrolled, so the single-mode difference is a failed eligibility check,
not proof of a causal regression. No ordinary app candidate timing, second
round, candidate full-corpus audit, strict Clippy, formatting or anchor gate
follows this refusal. There is no implementation rollout handoff, coverage
update or retained speed claim.

## Restoration, replay and remaining work

All 674 baseline source/manifest/lock/harness files are restored byte-for-byte,
with no added Rust modules. Rebuilding the ordinary CLI reproduces its frozen
`f2d25b27…` SHA-256. Canonical runtime and vendor are untouched. The 208
baseline/candidate control-matrix children, including listings, are distinct.
The 36 ordinary timing children, 22 native children, initial baseline samples,
builds, tests and failed runner are counted separately.

The complete observer patch replays to all six recorded source hashes in a
fresh private tree. For a replay, archive the recorded commit, supply the
pinned native checkout for bundled libraries, freeze an ordinary release CLI,
apply the observer patch and freeze its release CLI separately. Use the public
fixtures and executed control/reader sources in the receipt. They require the
recorded archive layout; redacted project paths are placeholders to replace.
The exported sources are AST-checked, while their original executed hashes are
recorded separately. Private app inputs and full output stay local, so a new
host must capture its own snapshot and cannot inherit this timing qualification.

Setup failures are retained: the first installer/build overlap failed before
the qualified observer; an initial collector test used an inaccessible SymbolId
constructor and was repaired; the candidate runner initially reused an old
trace path and correctly aborted on `create_new`. The corrected run uses fresh
paths. None of these children enters a comparison. Reused observer helpers
have two dead-code warnings; strict Clippy is not claimed.

This exact allocation opportunity is material, but its required public wall
benefit is unqualified. Do not repeat the same borrowed lookup without new
workload-specific evidence or a separately settled performance policy. Existing
native member publication/receiver lanes and performance-policy task `.13.1`
remain independent. The reported PR #5 regression still needs its failing
invocation; neither this attribution nor the earlier unreproduced 20–30x claim
establishes its cause.
