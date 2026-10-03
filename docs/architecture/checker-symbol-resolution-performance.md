# Symbol-resolution work and native completion reuse

Measured for `tsr-1yb.7.7` on 2026-10-03. The measurements identify a native
completion boundary worth porting; they do not establish an optimization gain.
No production cache or compiler change is included.

TSR caller attribution belongs to `e1a538485661f3eddda7c10691a04118396e55b6`.
Native attribution uses exact source
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`, exported while TSR main was
`beb727be30930a7c0d3693551f163bd0e77a2c5f`. Concurrent checker changes on
main are not retroactively covered by the TSR counts. Refresh that attribution
before using it to accept a production candidate.

## TSR callers

The temporary probe recorded 3,788,288 binder resolution queries, 30,271,006
ancestor visits and 623,630 distinct complete keys. Keys include BindResult
domain, starting NodeId, exact name, meaning bits and excluded declaration.
Every repeated complete key returned the same optional binder SymbolId in this
workload. That observation precedes alias resolution, diagnostics, substitution
and the `resolve_name` wrapper's global-symbol filter.

The largest repeated paths were:

| Caller at the measured source | Queries | Distinct keys | Repeated queries | Ancestors on repeats |
|---|---:|---:|---:|---:|
| `declared.rs:1338`, unqualified type reference | 820,609 | 27,668 | 792,941 | 5,009,966 |
| `members.rs:2823`, heritage entity symbol | 496,048 | 1,487 | 494,561 | 2,651,118 |
| `declared.rs:4543`, entity-name identifier | 420,125 | 7,100 | 413,025 | 2,356,990 |
| `flow.rs:3776`, value resolution | 192,065 | 12,372 | 179,693 | 1,323,325 |
| `symbols.rs:1254`, local export target | 165,443 | 1,365 | 164,078 | 995,325 |

The [complete 100 caller/meaning rows](checker-symbol-resolution-callers.csv)
retain every site, including sites with no repeats. Per-site distinct keys
cannot be summed to obtain the global distinct count: callers can share keys.
All caller queries and visits sum to the global totals.

Enabled and disabled probe runs preserved all 121 complete diagnostics and the
same 1,341 checked-file identities in order. Their single wall observations
were 5.922 and 5.081 seconds. The mutex, owned query names and tracking wrapper
add overhead; these are locating observations, not timing evidence for reuse.
Temporary production source instrumentation was restored exactly. The copied
probe remains separate from the normal CLI.

## Native completed entries and worker branches

The native probe counts three getter entry points, the symbol state at entry,
the result state, and branches that run the getter's resolution worker. It also
records checker construction and entry into the guarded full-source-check
worker. Atomics protect aggregate counters across native private checkers;
checked-file reporting uses a separate mutex. It does not intercept or replace
resolution answers, completion writes, alias fallback or diagnostics.

Each app mode has two enabled fresh-process runs. Their complete counter
payloads repeated exactly. Completed-entry hits are queries minus worker
branches; they are not ancestor visits or instantiation counts.

| Native mode/getter | Queries | Worker branches | Completed-entry hits |
|---|---:|---:|---:|
| Default, value symbol | 4,022,981 | 207,877 | 3,815,104 |
| Default, type-reference symbol | 308,671 | 194,053 | 114,618 |
| Single, value symbol | 2,820,752 | 138,939 | 2,681,813 |
| Single, type-reference symbol | 120,176 | 84,280 | 35,896 |

Default constructed four checkers; single constructed one. Types and symbol
links remain private to each checker, so these modes perform different amounts
of repeated semantic work even when the checked-file set is identical. Neither
mode licenses sharing raw SymbolIds or TypeIds across checker domains.

`getReferencedValueOrAliasSymbol` was not called in the app or public control.
Its source semantics were audited, but this workload gives no measured reuse
opportunity for that getter. App type-reference results included 17 unresolved
results in default mode and seven in single mode. These are getter result
counts, not counts of distinct symbols or emitted diagnostics.

## Native states that an implementation must preserve

Pinned `internal/checker/checker.go` distinguishes these operations:

| Entry point/state | Behavior |
|---|---|
| `getResolvedSymbol`, nil link | Resolve a nonmissing value name with its diagnostic/use context; publish the result or `unknownSymbol`. |
| `getResolvedSymbol`, completed link | Return that symbol, including a completed `unknownSymbol`, without repeating the worker. |
| `getReferencedValueOrAliasSymbol`, declared link | Return the existing nonunknown result. |
| Alias getter, nil or unknown link | Resolve with value/export-value/alias meaning and no missing-name diagnostic; this getter does not publish the fallback. |
| `getSymbolFromTypeReference`, nil link | Run `resolveTypeReferenceName` with type meaning and errors enabled, then publish the full result. |
| Unresolved type name | Construct or reuse a checker-owned unresolved TypeAlias symbol by parent-qualified path; its declared type is `unresolvedType`. Empty/missing names can return `unknownSymbol`. |

Source anchors are `getResolvedSymbol`, `getReferencedValueOrAliasSymbol`,
`getSymbolFromTypeReference`, `resolveTypeReferenceName` and
`getUnresolvedSymbolForEntityName`, rather than moving line numbers.

Rust's `node_types` memo caches expression TypeIds and has flow-loop controls.
It is a separate boundary. `get_type_from_type_reference` resolves a raw binder
symbol before consulting `alias_evaluation_bindings`; dynamic substitutions must
still apply after static symbol selection. Written aliases, merged targets and
qualified export/alias resolution also require distinct consumers. Existing
`unresolved_type_reference` creates error-like named TypeIds; that does not
establish the native unresolved-symbol ownership contract.

The raw-key repeat counts therefore do not justify a generic name cache or a
type-reference TypeId memo. `tsr-1yb.4.1.6` owns the implementable state/domain
and consumer contract plus focused characterization. It remains unfinished.
Current measurements do not exercise repeated completed unknown results,
alias-getter fallback, missing syntax, all query orders or augmentation changes.

## Complete-output and performed-work controls

The existing native binary, exact archive build, disabled probe and enabled
probe matched effective configuration, all loaded identities and complete
diagnostics in both modes, on the app and a generated public control. The
public sources and config are retained in
[the aggregate evidence](checker-symbol-resolution-counts.json).
They cover imports and aliases, qualified and missing type names, a missing
value, generic class references, a const assertion, JSDoc templates and two
intentional incompatible assignments. All four builds emitted the same five
complete control diagnostics. The probe observed a completed unknown value
result and unresolved type results; it did not establish every cache transition.

Native app runs loaded 13,098 files, emitted zero diagnostics and entered the
full-check worker for the same 1,339 unique files. Both enabled runs per mode
had identical checked identities and input contents. The earlier TSR probe
entered its check worker for those files plus these two JSON modules:

- `packages/ui/src/shiki/themes/codemod-dark.json`
- `packages/ui/src/shiki/themes/codemod-light.json`

Pinned `Program.SkipTypeChecking` and `canIncludeBindAndCheckDiagnostics`
exclude JSON from full-source checking. Native
`getTypeOfVariableOrParameterOrPropertyWorker` instead derives an imported JSON
module's type from its expression. JSON can still require semantic work outside
the full-source worker. Telemetry must describe these phases and eligibility;
removing two identities from a count alone would not prove equivalent work.
The historical TSR checked inputs were rehashed and remain unchanged.

The normal TSR CLI was rebuilt from `beb727be` after native measurement. It
matches all five complete public-control diagnostics and emits no probe stderr
when the old probe environment variable is enabled. The validation uses physical
project/cwd paths consistently. An initial `/tmp` versus `/private/tmp` control
had matching spans/messages but different rendered path prefixes; that result
is retained separately and is not equality proof.

The original TSR/native comparison remains incomparable: loaded scope,
effective options and diagnostics still differ. None of the single locating
wall observations is a controlled speed ratio, a cold-filesystem measurement,
or proof of the required ratio ≤0.50.

## Reproduction and identities

The opt-in [native probe patch](checker-symbol-completion-probe.patch) applies
to the exact pinned source with `patch -p1`. Build a scratch archive with the
existing Go 1.26 toolchain, default embedded libraries and `-buildvcs=false`;
keep the vendor checkout unchanged. Run normal CLI project checks with
`--noEmit --incremental false --composite false --pretty false
--extendedDiagnostics`, once with and once without
`TSR_NATIVE_SYMBOL_PROBE=1`. Add `--singleThreaded true` for the single mode.
The one stderr JSON record contains the counters and checked identities.
Build/setup and input hashing remain outside all measured process intervals.

SHA-256 identities:

- Exact-source manifest: `30380d7a8bfa9599238174c57217a3d4167aa28210a646f6bda2e2f5118f17f2`.
- Exact archive: `5ca755657cf63e435ecc53f9ac3de94a31220950a3c48e1fa5fbe5168e5420ac`.
- Existing native binary: `118d4efb63e59af858d6d0f5a718a8d2809d9f5551303a62caa5be3d5364c7ce`.
- Clean archive binary: `6340b230f887095fb7c82fedb41eecadd7dc49e4baa65a05ef4abb8ab4b74bc4`.
- Native probe binary: `9fa2725e6a929b1150552c9cb0a11bc197617f5c352cff8d378a0ddc68e89516`.
- TSR caller CSV: `9f359d65267da3473e3446369acb4fbc0751c02883caa478e720a332fb427f64`.
- Native patch: `fe9a22f1f02863af968621c8b3aab032339282c5356193af5fe049d172ebdb37`.

The existing native binary contains ambient VCS revision `40f72f56` and a
modified marker. That alone does not identify its native source; enclosing
repository metadata can enter a build. The exact archive supplies explicit
source provenance for this measurement, and matches the existing binary's
observed CLI behavior on these workloads. No historical oracle artifact was
overwritten.

Raw local artifacts are under `/tmp/tsr-scope-callers-e1a` and
`/tmp/tsr-native-symbol-completion-5b`: source manifest, before snapshots,
instrumented sources, binaries, complete stdout/stderr, per-process resources,
effective configuration, loaded/checked identities, input hashes and verified
result markers. The tracked CSV, JSON, patch and this report preserve the
measurement and its limits across machines.

Retained reuse still requires the native completion contract, refreshed TSR
attribution, complete diagnostics and performed scope, no previously RIGHT
corpus losses, and two independently confirmed same-source fresh-process
timing rounds. The full-project 2x requirement remains unmet and unverified.
