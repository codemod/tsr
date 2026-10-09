# Accessible-chain work and private owner qualification

Measured at canonical `a6b8a68f` (runtime unchanged from `2e47ffaa`), private
borrowed owner source158, and native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. This is locating evidence for
`tsr-1yb.11.5.1`, `tsr-1yb.11` and `tsr-1yb.16.3.10`. No runtime cache or
optimization is retained. The [record](checker-accessible-chain-attribution.json)
contains source/build/input identities, counters, failure history and drivers.

## What the native cache saves

Native `getAccessibleSymbolChainEx` (`symbolaccessibility.go:441-476`) refuses
nil symbols and property/method declaration symbols before cache lookup. The
selected symbol's `symbolContainerLinks` owns completion. Its key contains
external-alias-only mode, the first relevant scope node (nil for globals), and
meaning. Presence admits a completed nil result; absence starts a worker.
Query-local table visiting is separate from that completion cache and from the
eligible-alias slice cache. The probe records these boundaries without forcing
extra native symbol IDs or changing publication.

Two repetitions in each default and single mode give:

| Native workload | Requests | Distinct full keys | Chain walks | Success hits | Completed-nil hits |
| --- | ---: | ---: | ---: | ---: | ---: |
| Generated400, no emit | 0 | 0 | 0 | 0 | 0 |
| Real API project, no emit | 42 | 1 | 1 | 0 | 41 |
| Declaration-emission control | 9 | 2 | 2 | 7 | 0 |

The declaration control retains identical diagnostics and both emitted `.d.ts`
files across ordinary, probe-disabled and enabled runs. It proves the zero
generated count is an exercised instrument's observation. An earlier diagnostic
control rendered incompatible classes but made zero requests; its failed
positive-control assertion is preserved. The API control demonstrates completed
negative reuse, rather than treating a missing entry as a cached result.

The generated check enters 402 source-file workers. Native API enters 615.
Loaded identities and contents agree across native, private ordinary TSR and
TSR probe: generated 465 (402 physical plus 63 bundled), API 9861 (9802 plus 59).
The TSR API listing still reports four false parser diagnostics; existing
`tsr-1yb.35` owns them. Loaded-file agreement does not prove equal semantic work.

## Work TSR performs before display is requested

The private owner candidate's generated check performs **2,000 `best_name_ref`
workers**, with 2,000 distinct selected-owner/first-legacy-scope keys and 6,000
primary table visits. Of these, 1,600 return a name and 400 decline. Its explicit
TYPE-chain helper makes 1,200 requests, all declining before its table worker.
Default and single modes repeat those counts exactly; both check 402 files and
preserve the complete ordinary exit/stdout/stderr after removing probe records.

These are worker observations, not native completed-cache keys. `best_name_ref`
has no explicit meaning argument and its scope walk differs from the native
walk. Exact symbol/site keys also remain distinct. TYPE-helper declines can be
unsupported or active; they are not qualified completed nil entries. Counts
therefore authorize neither a generic name cache nor caching every `None`.

On this workload, even the first legacy scope keys do not repeat, while native
performs no accessible-chain work at all. Adding native completion caching
cannot explain away TSR's extra naming requests. This supports the existing
semantic/presentation separation work in `tsr-1yb.16.3.10`; it does not establish
its recoverable wall-time benefit. The [object boundary](checker-object-rendering-boundary.md)
still requires original/transformed presentation ownership and natural pending
return/publication controls before deferral.

## Real-project refusal changes the next action

The same ordinary private source158 candidate exceeded a 120-second API deadline.
A second, sampled run was killed and reaped at 120.293s, after 471.655s user CPU,
3.561s system CPU and 11,888,148,480 bytes peak RSS. Frozen canonical runtime
finished in 5.816s, 9.344s user CPU and 1,805,795,328 bytes peak RSS. These are
individual locating observations. The sampled run is not an ordinary performance
pair, and its incomplete diagnostics/checked scope prevent a speed ratio.

The sample confirms execution in checker threads, with the main thread joining
them. It contains conditional-alias/type instantiation and
`signature_bearing_type_node -> type_literal_key -> compare_symbols_ref`.
The flat summary includes map insertion, declaration comparison and allocator
routines. Recursive-inclusive sample counts are not call counts or exclusive
CPU percentages, and cannot assign the whole slowdown to one helper.

Private `declared.rs:2142-2151` builds every type-literal key by flattening all
alias binding frames into a new map, collecting a vector, then sorting selected
symbols through declaration/name/identity comparison. Native
`getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode` (`checker.go:22933`)
retains the original anonymous type in node links. `getObjectTypeInstantiation`
(`checker.go:22304`) retains potentially referenced outer parameters, then uses
mapped effective type arguments for instantiated images. These are different
preparation boundaries. `tsr-1yb.11.7` owns actual key/frame/worker attribution
and repair alongside the existing mapper contract `tsr-1yb.4.1.2`.

The earlier synthetic full-corpus preservation and small preparation wins do
not qualify source158 for integration. The real API refusal is now an explicit
retention gate. No private TSR API counter totals are published: its prerequisite
ordinary run timed out before the enabled comparison. Fixing the four parser
errors alone is also not a speed claim.

## Replay and limits

The [native probe](checker-accessible-chain-native-probe.patch) applies to an
exact native archive, built with Go 1.26.0 and `-buildvcs=false`; run
`TSR_NATIVE_CHAIN_PROBE=1` to emit its joined-worker record. The global probe
mutex and retained pointer keys add measurement overhead. They are not production
concurrency or timed-performance designs. Build caches and dependencies are task
local; two earlier sandbox/cache setup failures remain recorded.

The [Rust probe](checker-accessible-chain-rust-probe.patch) applies to frozen
source159 from the [owner replay](checker-owner-performance-qualification.md).
Final source185 guards observation with `work-trace`; its feature build and
ordinary release check both pass. It reproduces source176's counts with complete
ordinary outputs. Source176's original feature build is preserved separately.
Thread-local maps reset before Checker construction and assert no unfinished
observed workers when collected. They do not publish semantic completion.

All guarded canonical runtime inputs remain unchanged. No new full-corpus
optimization certificate, current-main counter attribution, general cache
implementation, full 92-file runtime review, six-ticket completion, or verified
equivalent-work TSR/tsgo median ratio<=0.50 is claimed.
