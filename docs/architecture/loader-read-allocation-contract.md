# Read preparation allocation boundary

Characterization for `tsr-1yb.19.2.1` uses TSR
`30eded711ee561daf99d03db40399deb295aefcd` and native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb` on darwin/arm64.
It retains a public helper and a reproducible temporary probe; it changes no
production filesystem, decoder or loader scheduling. This is an admission
prerequisite, not a whole-project speed result.

## The existing call cannot enforce a preparation budget

`FileSystem::read_file` in `crates/tsr-vfs/src/lib.rs` returns
`Option<String>`. It exposes neither a size hint nor an allocation reservation,
and the returned string carries no budget lease. `OsFileSystem::read_file` reads
the complete byte vector with `std::fs::read`, borrows it for `decode_bytes`,
then returns the owned decoded string. A caller can count the result's capacity
afterward, but cannot intercept those allocation requests through this call.
`CachedFileSystem` passes reads through; it adds no read-buffer admission.

Plain valid UTF-8 is copied from the borrowed bytes into an owned string.
UTF-16 decoding first collects a `Vec<u16>`, then collects decoded characters
into a `String`; the input bytes remain owned by the read closure. Replacement
characters can expand output, and collection capacity can exceed output length.
Missing files and directory read errors return `None`, with no successful buffer
snapshot. These observations preserve the existing call's behavior.

Metadata is an advisory observation. A real file can grow between an external
size query and the read, and the API offers no seekable size for every supported
input. The public FIFO control has metadata size zero but reads 49 bytes.
Reserving a metadata estimate, or limiting a queue to four slots, cannot by
itself enforce a live-byte budget.

## Measured boundaries

The [probe patch](loader-read-allocation-probe.patch) adds two observations only
in an isolated source-exact checkout:

- At UTF-16 decoder entry, record the actual unit vector length and capacity
  before creating the output string.
- After decoding and before the raw byte vector leaves the read closure, record
  both raw and decoded length/capacity. Both buffers are alive at this snapshot.

Capacity is expressed in payload bytes: raw `Vec<u8>` capacity, decoded string
capacity, and unit-vector capacity times `size_of::<u16>()`. The snapshots do
not measure allocator rounding, retained free blocks, logging allocations,
thread stacks, parser storage or total resident memory. The observations can
affect lifetime/overhead; they are not an uninstrumented RSS high-water proof.

| Control | Size hint | Actual raw bytes | Raw + decoded live capacity | Raw + units capacity at entry |
|---|---:|---:|---:|---:|
| Plain UTF-8, caller size unknown | unknown | 49 | 98 | — |
| FIFO, no seekable size | unknown (metadata 0) | 49 | 113 | — |
| Empty file | 0 | 0 | 0 | — |
| UTF-8 BOM | 52 | 52 | 101 | — |
| UTF-16 LE/BE | 100 | 100 | 150 | 198 |
| UTF-16 expansion | 8,300 | 8,300 | 24,900 | 16,598 |
| Unpaired UTF-16 surrogate | 110 | 110 | 218 | 218 |
| Odd trailing UTF-16 byte | 101 | 101 | 151 | 199 |
| Malformed UTF-8 comment | 85 | 85 | 255 | — |
| Oversized file | 65,589 | 65,589 | 131,178 | — |
| Growth after size query | 9 | 65,589 | 131,178 | — |

The example budget is 4,096 bytes and is deliberately unenforced. The oversized
and stale-size results prove that the existing call does not implement it.
These are measured instances, not universal capacity multipliers.

The unit-entry and post-decode observations are separate phases. Do not add
their maxima to claim a measured three-buffer peak. The unit allocation is
consumed during output collection, so admission must account for its lifetime
and output growth together; the two snapshots do not supply a strict bound for
that overlap. A budget-aware implementation must control every growth request,
including decoder temporaries, rather than infer a cap from these examples.

## Smallest enforceable preparation boundary

The API prerequisite (`tsr-1yb.19.2.3`) needs a budget-aware **private preparation
operation** that owns read/decode allocation and returns text with its lease.
It need not change the ordinary `FileSystem` trait or enable production workers.
An external wrapper around the existing whole-file call is insufficient.

The operation must carry these contracts:

1. Reserve modeled allocation bytes before controlled raw/decoder/output growth.
   Size hints can inform an initial reservation; missing or stale hints cannot
   authorize uncharged allocation. Hosts unable to implement this capability
   return an explicit unsupported result; bounded mode must not fall back to an
   unbounded read. Allocation policy and logical accounting must state any
   allowed slack.
2. Transfer a lease with the owned text through completion queues to the serial
   consumer. Moving or retaining text retains its charge. Release the decoded
   reservation only when its buffer is dropped or its charge explicitly
   transfers to another accounting owner. Raw and decoder temporary reservations
   release at their actual lifetime boundaries. Subsequent parser/arena
   allocation is a separate owner.
3. Distinguish missing/I/O failure from budget failure. Mapping resource refusal
   to `None` would misreport an existing source as missing. A constrained run
   must complete or report a controlled resource error, with no silent skipped
   input or changed diagnostic population.
4. Define progress for one oversized or initially unknown-size input. Either
   explicit exclusive oversize admission with disclosed overshoot, or a
   controlled budget failure, needs timeout and unwind controls. A task waiting
   forever for more credits than the entire budget is not a policy.

The admission prototype (`tsr-1yb.19.2.2`) consumes that API and tests pool
progress, oversized inputs and unwind. This is an API obligation, not an
implemented lease or a selected buffer layout.
An owned UTF-8 transfer could remove one copy, but would not solve unbounded
reads, UTF-16 temporaries or malformed-input semantics. Any decoder transfer
needs the native controls and current cost evidence in `tsr-1yb.2.1.3` first.
Logical byte admission still cannot promise a hard RSS ceiling.

## Controls, native limits and reproduction

[Control results](loader-read-allocation.json) retain source/binary/probe/input
identities, complete output fingerprints and process resource observations.
The runner records its checkout separately from binary provenance: the retained
normal/probe binaries use source `30eded71`, and their hashes stayed unchanged
during the later harness-validation run after concurrent checker changes.
Four Rust roles (normal, probe disabled, enabled, repeated) preserve complete
decoded text and parsed AST/node-table/map/diagnostic images for 14 physical
controls: 56 helper processes. The native reader exercises ordinary `osvfs.FS`
on the same inputs; all 53 compiled module dependency Go files match pinned
source. Thirteen native byte results agree; the malformed UTF-8 comment differs.

Native `internal/vfs/internal/internal.go::decodeBytes` passes malformed UTF-8
bytes through, whereas Rust replaces them. The 11 successful seekable fixtures
also have matching complete normalized CLI diagnostics and two loaded identities
across 22 normal/native processes. That comment's diagnostic agreement does not
prove raw-source/span fidelity; `tsr-34z` owns that bounded audit. Existing plain
output footer behavior remains `tsr-6.60`. No actual native checked-file telemetry,
whole-corpus measurement or whole-project performance ratio is claimed here.

The FIFO uses a controlled writer with bounded shutdown and process timeout.
The helper's hex output and parse-image construction happen after the filesystem
snapshot, but are included in process CPU/RSS. Normal/disabled/enabled resources
are therefore observations of this control driver, not an isolated observer
overhead or preparation-memory estimate. Never subtract independent RSS peaks
to infer private buffer bytes.

To reproduce, use an isolated checkout at the recorded TSR source, add the
[read/parse helper](../../crates/tsr-compiler/examples/read_allocations.rs), and
build the normal helper and CLI. Apply the probe patch in a separate checkout
and build its helper. In a scratch export of pinned native source, copy
[the native reader](loader-read-allocation-native.go) to
`cmd/read-allocation/main.go` and build `./cmd/read-allocation` with the native Go
toolchain. Use a clean pinned tsgo for the CLI controls. Then run:

```sh
python3 docs/architecture/loader-read-allocation-controls.py \
  --normal /absolute/normal-reader --probe /absolute/probe-reader \
  --native-reader /absolute/native-reader --normal-cli /absolute/normal-tsr \
  --tsgo /absolute/pinned-tsgo --output /absolute/new-output-directory
```

The runner is POSIX-specific because of the FIFO. Fixture construction stays
outside the child observations. It persists outputs and resource metadata and
verifies binary/input stability; those controls do not enable concurrency or
establish the original TSR/pinned-tsgo median wall ratio target of <=0.50.
