# Mapped template construction attribution

Measured from canonical `f8f98157` (runtime `2e47ffaa`), private owner
continuation source158 and pinned native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
The [record](checker-mapped-template-attribution.json) preserves build receipts,
binary hashes, counter schemas, complete qualification rows, reductions and
observer drivers. No semantic change is retained. All six goal tickets remain
in progress; the equivalent complete-work TSR/native median target remains
`<=0.50`.

## The API slowdown expands semantic work

The initial accessible-chain sample pointed to alias-frame flattening and
sorting in `type_literal_key`. Fresh observers distinguish that preparation from
the worker executions beneath it:

| API observation, default workers | Canonical source95, complete | Private source158, capped at 30 seconds |
| --- | ---: | ---: |
| Key preparations | 1,304,256 | 21,622,784 |
| Conditional root workers | 345,454 | 14,902,889 |
| Conditional node workers | 408,068 | 34,685,736 |
| Maximum instantiation depth | 16 | 100 |
| Maximum frame/raw binding/visible binding counts | 7 / 11 / 8 | 51 / 104 / 13 |
| Checked files | 615 | Incomplete |

The canonical observations repeat with complete ordinary/off/on output equality.
The private counts are lower bounds from the last progress rows for unfinished
owners; its child was killed and reaped. These are not a whole-work performance
ratio. Clock sums include instrumentation and nested work, so they cannot price
an exclusive CPU saving. A sort-only optimization would leave the semantic
expansion unresolved.

On generated400, canonical/private default conditional root/node workers are
identical at 1,608/6,042; key preparations are 273,446/274,150. Both finish with
the same outputs as their ordinary binaries and 402 checked files. The API
regression therefore needs its own producer rather than inference from this
generated workload.

Native's API default object-instantiation observer records 1,739,153 requests,
30,632 outer-parameter preparations, 1,708,521 retained outer-parameter reads,
676,596 completed target-table hits and 551,110 miss workers. Native completes
615 checked files with ordinary/off/on output equality. These object requests
are a different operation from TSR's key preparations and conditional workers;
they establish the retained preparation boundary, not a ratio of equivalent
workers.

## Reduction to mapped construction

The original API root reduction isolates `useful-db-queries.ts`. Its other three
active roots finish independently. Native completes all four individual roots;
the private database helper root times out. Relocated fixtures preserve the
original package's dependency lookup and explicit Node type roots. The failed
relocations remain in the record.

Splitting the seven helpers shows four `PgSelect` helpers timing out and the
three other helpers finishing. Returning a `PgSelect`, using an unused generic
constraint, or explicitly returning the generic parameter finishes. Reading
`qb.limit` without calling it times out, as does a concrete `PgSelect` receiver.
The member demand triggers construction; the helper's generic constraint alone
does not.

Direct Drizzle imports remove the application code. Both
`SelectResultFields<Record<string, any>>` and `SelectResultField<any>` time out
in the private candidate while native and main finish. The `string` and
`unknown` controls finish. A further standalone reduction also reproduces in
main:

```typescript
type Equal<X, Y> = (<T>() => T extends X ? 1 : 2) extends
  (<T>() => T extends Y ? 1 : 2) ? true : false;
interface Table { _: { columns: Record<string, any> }; }
type Field<T, D extends boolean = true> = T extends Table
  ? Equal<D, true> extends true ? Field<T["_"]["columns"], false> : never
  : T extends Record<string, any> ? Fields<T, true> : never;
type Simplify<T> = { [K in keyof T]: T[K] } & {};
type Fields<S, D extends boolean = true> =
  Simplify<{ [K in keyof S]: Field<S[K], D> }>;
type R = Fields<Record<string, any>>;
```

Use this standalone configuration; it needs no installed package or Node types:

```json
{
  "files": ["input.ts"],
  "compilerOptions": {
    "target": "ES2022", "module": "ESNext", "strict": true,
    "skipLibCheck": true, "noEmit": true, "types": []
  }
}
```

Source226's actual off/on runs complete with native exit0; both Rust variants
exceed the three-second observation limit and are killed/reaped. Removing the
`Simplify` intersection wrapper finishes in both variants. This is a producer
for the existing mapped-construction bug, not a passing approximation of the
private API regression. The record contains the exact fixture/config strings.
Run a release TSR or the pinned native executable with
`--project /absolute/path/to/tsconfig.json --noEmit --incremental false
--composite false --pretty false --singleThreaded true`; use the existing
`scripts/whole_project_perf.py` process helper for a bounded observation.

## Native ownership and demand boundary

At the pinned native revision, `getTypeFromMappedTypeNode` publishes the
original object with its declaration and alias before eagerly resolving only
the constraint. `getTemplateTypeFromMappedType` separately resolves and retains
the template when a consumer asks for it. `instantiateAnonymousType` keeps the
target, mapper, alias and a fresh mapped parameter; it does not serialize or
enumerate the entire mapped result during construction.

TSR's `mapped_type_info` resolves the template while preparing metadata, and
`instantiate_mapped_type_worker` instantiates that template and renders/resolves
the mapped result. Source224 observes the actual boundary:

| Single-worker producer | Native original mapped nodes / template getter requests / template workers | Canonical template preparations | Private template preparations |
| --- | ---: | ---: | ---: |
| Basic mapped member with deliberate TS2322 | 1 / 2 / 1 | 2 | 2 |
| Drizzle fields over `string` | 3 / 0 / 0 | 2 | 16 |
| Drizzle fields over `any` | 3 / 0 / 0 | 2 | >=52,439, incomplete |
| Standalone recursive `Simplify`, original source224 config | 3 / 0 / 0 | >=150,442, incomplete | >=114,899, incomplete |

The positive basic-member control proves that the native template counter can
observe real demand. Source225's **72 complete runs**, covering default and
single modes, compare ordinary, disabled, and two enabled runs. All preserve
each compiler's complete exit/stdout/semantic-stderr tuple, and enabled semantic
counts repeat. Timeout rows are excluded from that complete-output claim.

These counts identify premature template execution. They do not certify a
replacement mapper, cache completion policy or a speed win. The implementation
owner is the existing `tsr-1yb.16.3.10.4`, linked from `tsr-1yb.11.7`. A port must
preserve original declaration/alias identity, private Checker lifetime, ordered
mapper composition, fresh mapped parameters, receiver context and separate
uncomputed/active/completed/unsupported states. Preserve early constraint errors;
delay the template's work to its actual consumer. Do not add a global cache,
cache an unsupported result as completed native nil, or substitute a recursion
cap for this work boundary. Full native/legacy RIGHT preservation and complete
API work plus ordinary paired measurements are required before runtime retention.

## Replaying the probes and qualification limits

The Rust probes are feature-gated under `work-trace` and enabled with
`TSR_LITERAL_KEY_PROBE=1`. They keep thread-local counters and bounded NodeId
observations; their keys do not retain semantic SymbolRefs or force identity IDs.
Native's `TSR_NATIVE_LITERAL_PROBE=1` counters use a measurement mutex, which is
not proposed production concurrency. Disable `TSR_NATIVE_CHAIN_PROBE` when using
the extended native observer.

Apply the [canonical probe](checker-mapped-template-canonical-probe.patch) to
the qualified source95 snapshot and the
[private probe](checker-mapped-template-private-probe.patch) to the source158
snapshot reconstructed by the
[owner qualification](checker-owner-performance-qualification.md).
The [native probe](checker-mapped-template-native-probe.patch) is a delta from
qualified source174, after the
[accessible-chain native replay](checker-accessible-chain-native-probe.patch).
It is not a patch directly against uninstrumented native source. Exact base and
result manifest hashes and changed-file hashes are in the record. Use
`git apply --unidiff-zero` in a scratch copy, with a fresh isolated Cargo target;
build `tsr --release --features work-trace`. Each successful Rust probe also
passes an ordinary release check. Native is built with the pinned Go1.26.0
toolchain and recorded offline cache environment, `-buildvcs=false`.

Failed observer builds (depth type, TypeId accessor, array Default), ambiguous
native anchors, comparison-order repeat assumptions, partial capped trace
parsing, unavailable stack sampling and invalid dependency relocations remain
recorded. Sort comparisons vary with hash iteration; the old canonical sort
counter is unobserved, not zero work. Existing literal hits can include
provisional values, and native conditional entry counts omit tail-loop
iterations. No production cache state or equivalent-worker claim follows.

The evidence-only delivery passes formatting, internal section references,
scoped source/hash/count checks and staged whitespace validation. The full
anchor check still has the existing `full_oracle_native.go` reference failure
(`tsr-1yb.11.4.1`); the issue-reference check reports190 historical missing IDs
(`tsr-2zk.18`/`tsr-10`). The tickets cited by this update resolve in the
authoritative Beads database. These failures do not qualify the broader runtime
candidate; its review and acceptance remain unfinished.
