# Type-literal copies before cache lookup

Rust source `09b65ead8d72894b0b95369413abf0ae4b55fc94` and pinned native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` qualify this locating measurement.
The [receipt](checker-literal-copy.json) binds the ordinary/observer binaries,
source files, options, input snapshots, complete diagnostic fingerprints,
per-owner counts and restoration. It supplies the hit-only partition missing
from the earlier [mapper pool attribution](checker-mapper-pool.md).
There is no production optimization or verified native wall ratio.

## What the observer measures

`instantiate_type_literal` checks its origin, clones property and signature
images, checks that metadata exists, then looks up its ordered object key.
The archived [hooks](checker-literal-copy-probe.patch) preserve that order and
every existing guard, key, worker and publication operation. A per-call guard
classifies the copies using the actual subsequent lookup: nonactive value hit,
active hit, nonactive error hit, miss, or refusal before lookup. Refusals without
an origin are separated from refusals without metadata.

The [child module](checker-literal-copy-probe.rs) reads thread-local allocation
deltas immediately around each clone. Other owners cannot enter those deltas.
Payload scans happen after the delta, outside the clone's allocation tag.
Owned property strings include setter-parameter metadata; signature strings
include parameters, constraints, written returns and predicates. A signature's
`Arc<Signature>` target increments a reference count without copying its nested
payload. Per-call copy totals reconcile with the existing tagged allocation
totals, independently for every owner and for the completed process.

Successful allocations/reallocations, requested bytes and macOS usable-byte
observations are cumulative. Cloned vector/string capacities describe temporary
copied images; retained cache capacities are reported separately. Neither is
live heap, peak RSS or a normal saved-wall ceiling. Nonactive value hits do not
certify native-complete publication or safe print/semantic context sharing.

## Real-project inventory

Two fresh normal/observer-off/on samples per mode preserve complete **73
diagnostics**, **14,050 loaded files**, **14,746 reported parses**, and **1,397
direct off/on full-file calls**. Actual admission/full-file peaks are **4/4**
in default mode and **1/1** in single mode. Global and per-owner non-timing
counts and retained capacities repeat exactly. This newer source contains the
CLI diagnostic-routing change; the earlier source's 124 diagnostics are not
relabeled as this measurement.

| Observed copy boundary | Default | Single |
| --- | ---: | ---: |
| Nonactive value hits | 7,294 | 7,097 |
| Property allocation requests on those hits | 2,598,963 bytes | 2,524,672 bytes |
| Successful property allocation events on those hits | 52,432 | 50,960 |
| Copied property elements on those hits | 15,057 | 14,637 |
| Owned string payload within those property copies | 310,299 bytes | 299,848 bytes |
| Property allocation requests on misses | 6,047,047 bytes | 3,388,601 bytes |
| Nonactive error-hit property requests | 0 | 265 bytes |
| Signature allocation requests on value hits | 616 bytes | 616 bytes |
| Calls declining because the origin is absent | 157,964 | 128,404 |

Only approximately **30% / 43%** of prelookup property-request bytes belong to
nonactive value hits. Most hit bytes are vector element storage, rather than
string payload. Signature copying contributes one hit containing two signatures
in each mode. Natural active hits and metadata refusals are zero in this app;
synthetic production-branch controls exercise them. These numbers locate a
bounded copy opportunity without establishing its CPU or wall benefit.

The receipt retains individual wall/CPU/RSS observations, medians and the input
identities. Two samples per role have substantial timing variation, with
external host activity uncontrolled. Apparent negative observer overhead cannot
be treated as a speedup. Pure clone CPU/wall cost remains unmeasured.

## Qualification and limits

Five Rust controls exercise actual lookup branches, empty/nonempty images,
ordered/distinct substitutions, print/semantic requests, active/error states,
both refusal routes, setter strings, owned signature fields, `Arc` targets and
thread isolation/nested allocation tags. Deliberately classifying successful
hits as misses makes four controls fail; the exact original module is restored.
Synthetic cached values and active frames qualify attribution, not native
semantic completeness or a context repair.

The [driver](checker-literal-copy-controls.py) extends the existing pool receipt
reader and fixtures. Twenty-one Python controls reject misclassification,
lost/doubled allocations, incomplete or foreign process/owner receipts and
counter-schema drift. Final compiler qualification runs **140 public children**,
**30 admission/noCheck children**, **two abort controls**, and the **12 project
children** above. Off/on/repeat outputs and observed scope agree. Native full
diagnostics match **24/28 public variants** and **6/6 pool variants**. Existing
private-brand detail (`tsr-6.68`) and recursive readonly `Flatten` rejection
(`tsr-6.58`) failures remain explicit. Abort children cannot emit a completed
aggregate; absent Drop receipts do not prove cleanup.

Strict release checker/execute library/test Clippy, formatting and the checker
test suite qualify the final archived source. Full quality totals are in the
receipt. A preliminary observer needed Clippy metadata/test-format fixes;
all reported compiler qualification reran with the final binary.

Ordinary CLI output/reported scope and directly traced off/on identities are
separate evidence. Loaded physical-file/config snapshots begin after the first
ordinary child and stay equal; unobserved queries, directory entries, transient
changes and embedded library bytes are excluded. Complete cross-tool inputs,
lazy/fixing/conditional work and wider publication/emit states remain unproved.

The benefit decision remains `tsr-1yb.16.1.4`. Production follow-up
`tsr-1yb.4.3` retains its mapper-context and lifetime prerequisites and unchanged
fidelity/timing gates. No existing RIGHT assertion, release target or rejected
candidate threshold is weakened by this inventory.

## Replay

Use a private checkout of the exact Rust source above, with the pinned native
submodule. Copy `checker-mapper-pool-probe.rs` and
`checker-literal-copy-probe.rs` into `crates/tsr-checker/src/` as
`mapper_key_probe.rs` and `literal_copy_probe.rs`, then apply the hooks patch.
The receipt verifies that this reconstructs all seven observed files exactly.
Run the Rust controls with `TSR_MAPPER_PROFILE=1` and one test thread. Build
ordinary and observed release binaries outside sampling; provide their source
and SHA-256 bindings to the driver. Its default mode reuses public fixtures;
`--pool-controls` covers admission and failures; `--project` selects a real
project and `--repeats` controls observer samples. Always use a fresh output
directory. The local `/tmp/tsr-literal-copy-path` points to retained raw receipts
and frozen binaries for this run.

After qualification, all five private runtime files and the previous shared
private target CLI are restored byte-for-byte; both observer modules are removed.
The canonical compiler/native sources are unchanged.
