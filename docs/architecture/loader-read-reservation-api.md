# Source-qualified leased read preparation

`tsr-1yb.19.2.3` adds an opt-in private API witness on source `258d699a`.
It follows the [allocation contract](loader-read-allocation-contract.md).
The ordinary filesystem, decoder and production CLI are unchanged. This
prerequisite implements ownership and logical request admission; it establishes
no whole-project speed win or portable allocator/RSS ceiling.

## Operation and ownership

`examples/read_preparation/budget.rs::prepare_file` accepts an ordinary OS
`File` and a shared logical budget, returning `Result<PreparedText, Error>`.
The generic reader is private and supplies controlled failure/unwind tests.
Hosts with only an allocating `FileSystem::read_file` are unsupported by this
API; they need their own qualified raw-reader operation. Never wrap a returned
unbounded string behind this boundary or silently fall back to that call.

The operation reads through a 4,096-byte stack scratch area, grows raw storage
from bytes actually read, and reserves before each owned-buffer growth request.
Metadata is ignored, so absent, zero or stale hints do not authorize allocation.
Each charge is a non-cloneable RAII lease. The shared budget uses a private
mutex for accounting only; it contains no AST or checker state.

Raw growth conservatively reserves the **entire new layout while retaining the
old charge**. A successful reallocation releases the old charge after returning;
a failed reallocation releases the attempted reservation and keeps the old one
until its buffer drops. This models a possible old-plus-replacement overlap,
even when an allocator grows in place. It is a reserved-layout high-water value,
not an observed allocator allocation peak.

UTF-16 unit storage and the complete decoded output reserve independently while
raw storage remains alive. The output-length pass avoids unchecked String
growth; it is not a selected production decoding optimization. Decoded bytes
transfer into a String without a new buffer request. `PreparedText` exposes only
a borrowed `&str` and capacity: no Clone, mutable dereference or bare-String
escape. Moving it through a channel retains its lease. Buffer fields drop before
lease fields; decoded storage stays charged until its actual drop.

I/O failure, budget refusal, allocation failure, size overflow and an unsupported
reported capacity are distinct errors. A request that does not fit returns a
controlled resource failure, never an absent-file result or partial parsed
output. The legacy payload-only witness displays read failures as `missing` to
compare with the old helper; detailed audit preserves the I/O error. Budget
failure never enters that compatibility branch.

This API performs no pool scheduling. `tsr-1yb.19.2.2` owns pool admission,
oversized-input progress, shutdown and constrained 1/2/4-worker controls.

## Allocator qualification and limits

The public [Rust `try_reserve_exact` contract](https://doc.rust-lang.org/std/vec/struct.Vec.html#method.try_reserve_exact)
permits excess capacity. The prototype's exact reported-capacity assumption is
qualified against installed Rust **1.96.0**, commit
`ac68faa20c58cbccd01ee7208bf3b6e93a7d7f96`, on darwin/arm64. Local standard
library source hashes are retained in the evidence:

- `Global::alloc_impl_runtime` and `grow_impl_runtime` report requested layout
  size, while excluding allocator usable-size rounding.
- `RawVecInner::grow_exact` computes the requested capacity; `set_ptr_and_cap`
  records that capacity and explicitly notes that future allocator behavior
  could change this assumption.

The witness verifies each observed capacity against its pre-reserved request.
An unexpected capacity returns an unsupported error after observation. That
guard does **not** prove a pre-allocation bound on an unqualified implementation:
an unknown excess was already allocated. Requalify the toolchain/allocator
before using this prototype as admission evidence elsewhere; never promote
successful finite controls into a portable exact-capacity guarantee.

The model excludes allocator rounding/retention, the fixed stack scratch,
coordination and audit storage, reader/OS implementation storage and subsequent
parser/arena allocations. It does not claim a hard RSS cap. Blocking OS I/O has
no new cancellation mechanism; the physical control process has a 30-second
timeout and the controlled FIFO writer has bounded shutdown. Pool cancellation
and shutdown remain an integration obligation.

## Verified controls

[Evidence](loader-read-reservation.json) retains source/toolchain/binary/fixture
identities, complete payload fingerprints and process observations. Fourteen
physical cases use the ordinary read/parse helper from `30eded71` as reference
and the ordinary native reader pinned at `5b1047d1`. Every complete Rust payload
is identical, including AST/node-table/map/parse-diagnostic images. Native raw
bytes match for 13 cases; malformed UTF-8 retains the existing Rust replacement
versus native passthrough gap in `tsr-34z`. No native span/offset fidelity claim
is added by that comment fixture.

Twenty-nine audit processes exercise a 1 MiB budget, a 4,096-byte budget and an
empty input at zero budget. Every reservation balance returns to zero after
success or failure; acquired/released counts agree. A prepared buffer's retained
charge equals its reported String capacity. Selected high-budget logical peaks:

| Case | Peak reserved layout bytes | 4,096-byte outcome |
|---|---:|---|
| Unknown-size UTF-8 / FIFO | 98 | prepared |
| UTF-8 BOM | 101 | prepared |
| UTF-16 LE / BE | 247 | prepared |
| UTF-16 expansion | 37,023 | controlled budget failure |
| Malformed UTF-8 comment | 234 | prepared with existing Rust semantics |
| 65,589-byte input / stale 9-byte hint | 196,661 | controlled budget failure |
| Missing / directory read | 0 | I/O failure |

Raw growth doubles capacity, so this prototype can reserve more payload space
than the ordinary whole-file read. Its prepasses, scratch reads and accounting
also have costs. These choices are ownership/admission controls, not accepted
production performance improvements. Any retained runtime version still needs
current cost attribution, full work/output fidelity and independently confirmed
whole-project timing benefit.

Five release controls separately verify channel retention, raw/unit/output
refusal cleanup, read failure, reader/consumer unwind and existing malformed
UTF-8 decoding. Strict release example Clippy passes. No new dependency or
unsafe code is added.

## Reproduction

Build `read_preparation` at the owned source snapshot, and retain the reference
`read_allocations` helper and native reader from the recorded sources. Run:

```sh
cargo test --release -p tsr-compiler --example read_preparation
cargo build --release -p tsr-compiler --example read_preparation
python3 docs/architecture/loader-read-reservation-controls.py \
  --prepared /absolute/prepared-reader --reference /absolute/reference-reader \
  --native-reader /absolute/pinned-native-reader \
  --output /absolute/new-output-directory
```

The POSIX runner reuses the existing public encoding fixtures and timeout/resource
helper, checks source/binary/input stability, and captures full stdout/stderr.
RSS/CPU/wall observations include hexadecimal output, parsing and audit work;
they are not a decoder-only or normal full-project benchmark. The original
equivalent-work TSR/pinned-tsgo median wall ratio target remains <=0.50 and
unverified.
