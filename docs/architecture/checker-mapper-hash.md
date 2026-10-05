# Mapper hash and equality attribution

The observer counts the original hash and whole-key equality delegates for all
four instantiation tables under the actual checker pool. It preserves current
outputs and exposes a remaining construction-coverage gap. No production key
representation or cache change is selected. The normal saved-wall opportunity
and the comparable TSR/pinned-tsgo ratio remain unknown.

Measurements freeze TSR `1ccf5337b91d248784c51a95043c92ec86acf8cc`, native Go
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, and TypeScript fixture revision
`4d4f005c8541e0255a9d8791205fdce326e462bc`. Source/module, binary, harness,
options, observed inputs, complete outputs and private raw-log hashes are in
[the receipt](checker-mapper-hash.json). Later main changes do not refresh these
measurements.

The transparent stored-key wrapper moves the existing key into the same
FxHashMap. A borrowed query delegates to the original key through safe `Borrow`
and trait-object views. It introduces no additional key vector or unsafe cast.
Hash and equality callbacks execute inside the table operation, before any
nested semantic worker can run. Lookup/insert/unscoped counters distinguish
object, signature, reference and predicate tables. Insert counts include table
growth rehashes. Global and per-owner receipts reconcile sums and maxima.

Whole-key equality invocations and supplied vector items do not count scalar
element comparisons. Hash identity fields omit the Vec length prefix and do not
count Hasher writes or instructions. Delegate clocks include observation and
scheduling effects; map clocks include the surrounding observation work.
Neither is a normal CPU share or a saved-wall ceiling.

The real application controls preserve all three diagnostics, 14,051 loaded
files and 1,397 directly checked file identities in both default and single
modes. Each mode runs two normal, probe-disabled and probe-enabled processes.
Direct off/on ownership/order and non-timing counters repeat. Individual normal
default walls range from 5.965 to 9.113 seconds; single walls range from 9.678 to
10.475 seconds. These observations cannot establish causal overhead or a win.
Input snapshots cover physical loaded/config files, rather than every resolver
query or embedded library. Native whole-project performed-work equivalence is
still unproved.

| Mode | Selected reference producer requests | All reference lookups | Selected key storage requested |
| --- | ---: | ---: | ---: |
| Default | 609,036 | 653,112 | 4,530,108 bytes |
| Single | 522,997 | 561,840 | 4,055,272 bytes |

The difference includes borrowed index-signature/member queries, the empty
reference key, deferred `arguments.to_vec()` construction and target-pair clone,
and NonNullable singleton lookup/publication keys. It is not an allocation
estimate. The subsequent [reference-key coverage archive](checker-reference-key.md)
qualifies the exhaustive site inventory and construction counters under
**tsr-1yb.16.1.2.1.1** on a separate frozen source. It does not relabel these
older measurements. **tsr-1yb.16.1.4** retains its opportunity and semantic
prerequisites. Reference refusal,
error and publication controls remain independently owned by
**tsr-1yb.16.1.2.2**. The rejected mode-key timing gate and mapper-context repair
prerequisites remain unchanged.

Four ordinary-process profiles use an earlier five-second sampling interval,
twice per mode. Default captures the main thread and all four checker threads;
the main thread's join waits do not become checker CPU. Nearest-owner and
allocator stacks point to instantiation, structured member-name collection,
property consumption and binder name lookup. They do not establish material
pure-hash cost. These interval samples include loading and waits and contain
truncated owner rankings; they are not allocation counts or CPU percentages.
An earlier completed sample with an empty call graph was rejected and retained.

Validation includes 1,497 checker tests with three existing ignored tests,
strict release checker/execute Clippy, formatting and 61 Python controls.
Five focused Rust tests verify exact original hash feeds, real TypeId/SymbolId
identity, ordering/composition/distinct keys, pointer/layout preservation,
test-only collision equality, growth rehashes and two-owner reconciliation.
Removing the actual hash or equality invocation counter makes two of those
tests fail; both temporary mutations were restored.

All 476,787 type rows and 10,570 diagnostic cases match the fresh normal corpus
byte for byte, with zero prior RIGHT losses. Corpus observation is disabled;
enabled behavior is covered by the focused tests and actual application/public
controls. The initial empty fixture-tree result was rejected before initializing
the pinned TypeScript submodule. The 140 public compiler children preserve Rust
outputs; 24/28 native diagnostic comparisons match. Existing private-brand
explanation (**tsr-6.68**) and recursive Flatten (**tsr-6.58**) failures remain
failed comparisons. Another 30 pool children cover small, skewed/two-worker and
noCheck admission, with six native matches. Both release abort injections are
rejected as incomplete; absent Drop receipts prove no cleanup.

For an isolated checkout at the frozen TSR revision, run
`python3 docs/architecture/replay_checker_mapper_hash.py --source /path/to/checkout`
from the artifact repository. The replay refuses the shared repository, another
revision, modified originals and existing observer modules, then verifies every
reconstructed file hash. Enable `TSR_MAPPER_PROFILE=1` for the focused Rust tests.
`checker-mapper-hash-controls.py` reuses the existing pool/public driver with a
`mapper-pool-hash-v1` reader; bindings must match the qualified native binary.
The archive includes the complete adapted pool module in the patch.

Temporary source hooks and the three prior private target binaries are restored
after validation. Raw failures remain in the private experiment directory with
their hashes in the receipt. The new coverage task and existing growth receipt,
generic/shared-base/member-width, reference-state and member-ownership tasks can
advance during fidelity work; production candidates retain their correctness,
ownership and measured-benefit dependencies. The full-project 2x goal remains
active and unverified.
