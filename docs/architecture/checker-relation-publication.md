# Recursive relation proof publication

This records correctness prerequisites `tsr-6.51` and `tsr-6.52`, plus
bounded execution experiments under `tsr-1yb.15`. Completed results became
checker-lifetime under `tsr-2zk.902`; see
[Checker-lifetime results](#checker-lifetime-results-tsr-2zk902), which
supersedes the per-walk ownership recorded in the audit table below. The
remaining relation-cache contract items of `tsr-1yb.4.1.3` (reliability flags,
generic key equivalence) stay open. The comparison starts at TSR
`738b1a797a65c62a418da34d27daad591b1599de`, against pinned tsgo
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

## Applying the port convention

This documentation-only audit inspected TSR `08f2487b` and native
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. It applies the
[checker port convention](../conventions.md#checker-ports-preserve-ownership-and-work-boundaries)
to the existing recursive relation walk; it adds no runtime reuse or new timing
measurement. The historical experiments below retain their original sources.

| Boundary | Current evidence and remaining obligation |
|---|---|
| Native operation | `internal/checker/relater.go::recursiveTypeRelatedTo` and `resetMaybeStack` distinguish persistent relation results from dependent assumptions. `internal/checker/checker.go::getRelationKey` includes intersection context and eligible generic-reference equivalence; error elaboration can re-execute a cached failure. |
| Identity and owner | Rust `Relater` owns ordered `(source TypeId, target TypeId, IntersectionStateTarget)` results and assumptions for one walk, borrowing one private Checker with a fixed relation kind. The third component mirrors `getRelationKey`'s intersection-state suffix: a pair related inside a target intersection's constituent walk (`typeRelatedToEachType`) skips the excess and common-property checks and the intersection property-check pass, so its result is not the unqualified pair's. `IntersectionStateSource` is not represented. `relate_ternary` and `compare_signature_ternary` construct fresh stores. This does not prove those keys sufficient for checker-wide reuse. |
| Publication | `recursive_type_related_to` returns `Maybe` on active re-entry. `reset_maybe_stack` promotes a successful dependent scope; failed and unsupported scopes discard assumptions. Top-level `CircularVariance` remains unpublishable. The public `Unknown` is unsupported/depth-refused work, distinct from native circular variance. |
| Receiver and presentation | `properties_related_to_with_optionals` reads through `get_type_of_property_of_type` on the concrete source/target, retaining the original intersection receiver. Declaration-symbol types cannot replace those reads. Relation results do not certify a reusable diagnostic elaboration or alias/display image. |
| Expensive work | A completed hit skips that walk's structured comparison; an active hit supplies only an assumption. Count executed structured/property/signature/variance work separately from repeated API calls. Historical counters with rejected property exits are not current production counts. |
| Controls | `tests/relater.rs` covers both invalid union orders, a valid later recursive proof, depth-refusal assumption discard, generic variance and strict-function option changes. Internal circular-variance controls check that no top-level proof is published. These do not certify native persistent key equivalence or all metadata-forcing contexts. |

*Correction (tsr-2zk.902):* the "Identity and owner" row describes `08f2487b`.
`relate_ternary` and `compare_signature_ternary` no longer construct fresh
result stores; completed results are checker-owned, below.

The concrete omissions already have owners: `tsr-1yb.4.1.4` must specify generic
key equivalence, constraints, direction and intersection context;
`tsr-1yb.4.1.3` must settle persistent lifetime, metadata forcing,
diagnostic/reliability flags and publication; `tsr-1yb.11.1` must provide current
expensive-worker counts and overhead evidence. These tasks remain open or in
progress. The audit creates no competing implementation task and selects no key
encoding or cache lifetime. Repeated ordered pairs alone cannot discharge those
obligations. Query/emit worker ownership remains `tsr-1yb.3.2`, while concrete
receiver API coverage remains `tsr-1yb.4.1.5`.

## Checker-lifetime results (tsr-2zk.902)

Measured at integration head `b23dd3d` against pinned tsgo `5b1047d`.

**Forcing constraint.** Until this change every `relate_ternary` call built a
fresh `Relater` with an empty results map, so a repeated
`(source, target, relation)` pair repeated its whole structural walk.
TypeScript's own `src/jsTyping` and `src/typingsInstallerCore`
(`vendor/typescript-go/_submodules/TypeScript/src`) never finished: still
running at 120 s and 300 s, with three `gdb` stack samples of every thread at
30-40 s all in `narrow_type_by_type_predicate`/`map_narrowing_type` →
`relate_with_signature_diagnostic` → 23-30 nested `recursive_type_related_to`
frames re-resolving inherited members. Each narrowing query re-walked the
same deep declaration-type pairs (`Node`, `Declaration`, ... of
`compiler/types.ts`).

| Boundary | Decision |
|---|---|
| Native operation | `Relation` (`relater.go:99`) with `get`/`set`; `recursiveTypeRelatedTo` (`relater.go:3061`) reads it before the active-assumption check and publishes failures (`:3162`); `resetMaybeStack` (`:3169`) publishes discharged assumptions as succeeded. One map per relation kind on the `Checker`. |
| Key identity | `crate::relation_cache::RelationKey` = `(source TypeId, target TypeId, IntersectionStateTarget)`, one map per [`Relation`](../../crates/tsr-checker/src/relater.rs) kind. Native's `getRelationKey` (`checker.go:17613`) additionally writes generic type references (`'g'` keys) with type parameters by position, so several TypeId pairs share one native key; a TypeId pair is strictly finer, so the port can miss a hit native takes but never shares a result native keeps apart. The `constrained`/broadest-equivalent `maybeKeys` probe (`relater.go:3097`) is therefore not ported (`tsr-1yb.4.1.4`). `IntersectionStateSource` remains unrepresented, as before. |
| Owner and lifetime | `Checker::relation_results`, one private checker, its whole lifetime. TypeIds are never reused (the `TypeStore` has no rollback). Native fixes options at construction; the port's `apply_compiler_options` and its tests change them on a live checker, so the store records the options its results were computed under — `strictNullChecks`, `strictFunctionTypes`, `exactOptionalPropertyTypes`, `noImplicitAny` — and a walk that starts under different ones discards every result (`RelationResults::validate`). |
| Publication | Unchanged from the per-walk rules above: `Related` and discharged `Maybe` scopes publish `Succeeded`; `NotRelated` publishes `Failed` ("false under assumptions is false without them"); `CircularVariance` and `Unknown` (unsupported work, depth refusal) never publish. A completed hit returns at once; an active assumption is still walk-local (`maybe_keys`). Native's `Reported`, `ComplexityOverflow`/`StackDepthOverflow` and `ReportsUnmeasurable`/`ReportsUnreliable` flags are not represented: the port has no `relationCount` budget, its depth refusal is an unpublished `Unknown`, and it propagates no reliability flags (`tsr-1yb.4.1.3`). |
| Context frames | A walk opened while a conditional-alias evaluation frame (`alias_evaluation_bindings`) or a mapped-template frame (`mapped_template_depth`) is active reads member and template types through that frame, so its answers are not the frame-free pair's. Native has neither frame. Such a walk keeps a walk-local map (exactly the pre-change behavior) and neither reads nor publishes the checker store. Print frames (`render_type_parameter_*`) change names only and are not excluded. An open type resolution is not excluded either: native publishes under open resolutions too, and the corpus below is byte-identical. |
| Diagnostics | Native re-runs a cached failure when it elaborates (`relater.go:3069`). The port elaborates only the direct pair's signature arity (`diagnostic_pair`), so a cached failure is re-run only for that pair; a cached success needs no elaboration. |
| Entry read | Native `isTypeRelatedTo` (`relater.go:193`) reads the results of two object types under `IntersectionStateNone` before it opens a walk. `Checker::cached_object_relation` ports it at the `relate_ternary` entry (not for the diagnostic pair, not in a frame). It is equivalent to the walk it skips: `recursive_type_related_to` is the store's only writer and is reached only after `is_related_to_with_flags`' arms decline the pair; those arms read only the pair, the relation and the validated options, and for two object types the walk's normalizations reduce to `get_regular_type_of_literal_type`, applied before the read. Variance-marker pairs are answered before the store is ever written. |
| Expensive work | A hit skips the structured walk (`structured_type_related_to` and everything below it); an entry hit also skips `is_related_to_with_flags`' arms. Callgrind on `scripts/generate_perf_project.py --modules 100`: 4,530,691,360 → 4,416,164,874 instructions (−2.5%) with the store, 4,405,146,014 (−2.8%) with the entry read. On `jsTyping` a temporary counter (not shipped) saw 8.8 million top-level relation calls, 1.28 million structured-walk entries, 1.03 million of them completed hits, and 66,048 structured results `Unknown` (recomputed every time, as they may not publish). |

**Alternatives rejected.** (1) Keying by native's `getRelationKey` string:
needs the generic-reference equivalence audit (`tsr-1yb.4.1.4`) and a hash
per lookup, and buys hits only for structurally equal generic references;
TypeId pairs already close the measured hang. Revisit if a profile shows
repeated walks between distinct TypeIds of the same generic reference.
(2) Refusing publication while any type resolution is open: more
conservative, but every relation inside a variable's initializer inference
would stay per-walk; native publishes there, and no corpus line moved
without it. (3) Clearing the store from `apply_compiler_options`: misses
the tests that assign option fields directly.

**Accepted consequences.** A result now outlives the walk that computed it,
so a TSR-specific context dependence that is not one of the two excluded
frames would become a stale answer rather than a per-call one. None was
found: the unfiltered corpus is byte-identical (types 477,985 lines,
469,946 RIGHT; diagnostics 10,570 cases, 4,232 RIGHT + 4,968 EMPTY_RIGHT).
Memory grows with distinct related pairs for the program's lifetime, as in
native.

**How we would know we were wrong.** A verdict that changes with the order
in which files or expressions are checked (e.g. a single-file run that
differs from the same file in a project), or a corpus line that changes
when this store is replaced with the walk-local map. The controls are
`relater.rs` `relation_cache_tests`: results outlive the walk (success with
its discharged assumption, and a failure), framed walks stay walk-local,
and an option change discards results.

**Outcome.** `jsTyping` and `typingsInstallerCore` (with `types: []`,
because the submodule has no `@types/node` and native otherwise stops at
TS2688 before checking) finish in 25.1 s and 24.9 s; native tsgo takes
1.50 s and 1.57 s. The entry read takes `jsTyping` from 27.1-28.8 s to
24.6 s in an interleaved pair of runs on the same box (output
byte-identical). TSR reports 875 diagnostics on each against native's 163
and 169, with 56 location/code keys in common. TSR's largest extra codes are
TS2339 (293), TS2345 (194), TS2769 (91) and TS18048 (79); native's largest
code TSR lacks is TS6307 (77, project file list). The baseline never
finished, so there is no before/after diagnostic comparison on these
projects; the differences are not attributed to this change, and neither is
the remaining wall gap.

## The failed proof

The regression in `tests/relater.rs` compares mutually recursive `A/B` with
`C/D`. Their `value` properties conflict. A third type, `E`, has a matching
`value` but refers to `D` and therefore also fails against `A`.

Previously, `A -> C` parked a successful entry before recursing. `B -> D`
re-entered that entry and was recorded as a completed success. The later
`value` mismatch rejected `A -> C`, but left `B -> D` successful. The next
`A -> E` branch reused it and incorrectly accepted the union assignment.
Both `C | E` and `E | C` demonstrate this failure. Pinned tsgo rejects them
with TS2322; the previous TSR binary accepted them without diagnostics.

## Native publication and the port boundary

Native `internal/checker/relater.go::recursiveTypeRelatedTo` and
`resetMaybeStack` separate completed `Relation.results` from active
`maybeKeys`/`maybeKeysSet`. Re-entry returns `TernaryMaybe`. A conjunction
retains that state until an enclosing proof either succeeds or fails.
An independently true proof, or a successful recursive proof at depth zero,
publishes its dependent keys together. A failed branch discards its
assumptions; a failure under assumptions can be cached as a failure.

TSR now carries this distinction in a private `RelationResult`. Completed
answers and recursive assumptions have separate storage. Member, signature,
tuple, variance and union/intersection composition preserve `Maybe`, rather
than turning it into an independent success. Public `Ternary` remains the
existing three-state API.

The public port's `Unknown` covers unsupported work and depth refusal. Native's
`TernaryUnknown` describes circular variance, rather than an unimplemented arm.
TSR represents that result privately as `CircularVariance`: it is non-false
during measurement, dominates recursive assumptions in conjunctions, and retains
nested keys for an enclosing proof. At depth zero its keys are discarded, never
published as completed successes. Unsupported work retains the existing Kleene
policy and discards its scope even below depth zero. Native reliability flags
and their reuse obligations remain part of the relation contract.
The depth-refusal regression checks that such a branch cannot supply a
successful recursive child to a later union branch.

This change retains per-top-level-walk result ownership. It does not establish
that ordered TypeId pairs suffice for checker-wide reuse. Native generic
equivalence keys, intersection context, diagnostic/reliability flags and
metadata completion remain obligations of `tsr-1yb.4.1.3`.

## Active variance and execution order

The correct eager proof-publication candidate exposed repeated structural
work: five alternating real-app pairs measured 4.857 seconds on the old
checker versus 11.040 seconds on the corrected candidate. It was not shipped.
A bounded CPU sample placed 1,486 of 2,316 checker samples beneath variance
measurement. Later signature/property/composite counters found 359,262 worker
executions, including 316,980 repeated pair executions within their own
relation walks. One `ResultAsync<__varianceSub, E>` to
`ResultAsync<__varianceSuper, E>` query accounted for 315,868 executions over
929 distinct pairs.

Native `getVariancesWorker` returns an empty slice for a recursively active
generic target. `structuredTypeRelatedToWorker` (`relater.go:3834`) stops with
`TernaryUnknown`. TSR already returned an empty vector from
`inference_variances` for that active target, but fell back to its members.
The fix stops only the active target with nonempty type arguments. It does
not cache the uncomputed result or treat an unsupported target as successful.
The circular result must be non-false during measurement: otherwise recursive
occurrences obscure direct witnesses, and an independent parameter incorrectly
falls back to covariance. This distinction restores native acceptance in
`checkInfiniteExpansionTermination` and `recursiveTypeComparison`, and native
input contravariance in `varianceMeasurement`. The regression checks that the
circular result publishes no proof; after measurement ends, real rejection and
directional literal covariance recover.

Two isolated counter runs with the rule agreed on 43,395 worker executions,
2,041 repeated pair executions and 12 active-target refusals. These counts
compare two experimental variants that both included the later rejected
property exits; they are not final production counts. Temporary probes were
archived and production files restored byte-for-byte. No instrumentation or
process cache ships.

Signature comparison now follows native's direction order, skips an unneeded
second bivariant comparison, and stops after a failed `this` or parameter.
Union/intersection branches are evaluated lazily, preserving constituent order
and the port's policy that unsupported work must not hide a later decisive
branch.

Property exits were rejected: two previously RIGHT rows in
`arrayDestructuringInSwitch1` lost a callable union constituent at
`operands.every` and `every`. Saved binaries isolate the losses to property
stopping; signature stopping alone preserves them. Property traversal was
restored, and `tsr-6.53` tracks the missing member-forcing or identity behavior
before retrying. There is no fixture-specific exception.

## Verification

The original regression fails before the fix with `Related` instead of
`NotRelated`. Afterward, all 74 relation tests pass, including valid recursive
pairs, a valid later union branch, branch ordering and depth refusal. An
overload regression also selects native's fallback for both invalid unions
and the concrete overload for a valid recursive union. Pinned native
declaration output independently confirms these three selections.

The checker/execute release suite passes 1,298 tests with four existing ignored
tests. All four structured overload tests pass. Checker/execute library and
test Clippy, formatting and diff checks pass. Local review covered the public
API projection, proof promotion/discard scopes, native comparison order,
active-target guard and regression fixtures.

The unfiltered before/after corpus has 474,251 rows, byte-for-byte identical:
459,451 RIGHT, 2,195 GAP and 12,605 WRONG. There are no previously RIGHT losses.
The real Next.js workload retains all 123 complete diagnostics and the same
counts: 13,097 loaded, 1,341 checked and 13,560 parsed files.

The negative CLI controls match native's rejection, code and location. TSR
still omits native's nested `next.next.value` diagnostic elaboration; complete
diagnostic fingerprints therefore differ on these controls. This is not a
claim of complete diagnostic parity or the required 2x throughput.

The retained variant excludes early property exits. Its full corpus, CLI
controls, gates and source/binary hashes are preserved in
`/tmp/tsr-relation-no-property-{corpus,controls,gates,source}.json`. The corpus TSV
fingerprint is
`f01585f8cafc7fd2fc198b22350c097d1baaa0a6037421369613895e34db55fb`.

The retained CLI SHA-256 is
`34e81bb8eb340a985ddd0deff9ddbb0774355ba5473ca05d7543d2d62a4c812e`.
Counter and bisect artifacts are `/tmp/tsr-relation-counter-results.json`,
`/tmp/tsr-relation-counter-hot-results.json`,
`/tmp/tsr-relation-active-counter-results.json`, and
`/tmp/tsr-active-variance-bisect.json`.

## Whole-project confirmation

Each row below is five alternating fresh-process pairs on the same real app.
The second run is an independent confirmation. Builds and quality gates ended
before timing; incremental/composite reuse was disabled and the filesystem
was warmed. The unchanged keep threshold was 20 milliseconds.

| Comparison | First-run median | Confirmation median |
| --- | --- | --- |
| Corrected eager proof → retained variant | 10.763 → 4.808 s | 10.782 → 4.685 s |
| Previous main → retained variant | 4.836 → 4.747 s | 4.877 → 4.769 s |

The complete package improves main by 89 and 108 milliseconds (1.84% and
2.21%). Main-comparison user CPU medians decrease from 3.752 to 3.636 seconds
and from 3.772 to 3.658 seconds. Median peak RSS changes from 1.124 to 1.097 GB
and from 1.123 to 1.085 GB. This does not attribute a separate timing gain to
each signature/composite edit.

All samples preserve the 123 complete diagnostic fingerprint
`cdb777a1930fee9c86ab978511539234e820c77a69a4586d522409488e074074`,
13,097 loaded, 1,341 checked and 13,560 parsed files. Before/after loaded
identities hash to
`7ffb6eb6de272445848f5afd7442ed0f50fc9517a9a8ceb1017467fa108015d4`;
loaded input contents hash to
`395a32b083501c1cac01a17c6585385849f997fc10a3faab37b2f3d2d7bd9b16`.
Configuration and lockfile hashes also remain stable. Raw individual samples,
CPU/RSS, binary hashes and scope checks are in
`/tmp/tsr-relation-no-property-paired.json`.

The full-project TSR/pinned-tsgo median wall target remains ≤0.50 on equivalent
semantic work. These are TSR before/after comparisons; they do not establish
the required native ratio or erase the remaining diagnostic-parity gaps.

## Circular-variance correction against debb68b6

The fresh 10,570-case diagnostic audit includes all 5,082 empty baselines and
compares duplicate-aware occurrences. Exactly three cases improve:
`checkInfiniteExpansionTermination` becomes `EMPTY_RIGHT`; `recursiveTypeComparison`
and `varianceMeasurement` become `RIGHT`. Expected matches increase from 20,596
to 20,597; extras decrease from 3,911 to 3,908. No previously correct case or
matched occurrence is lost, and no extra diagnostic or population change appears.

Fresh pinned-native execution confirms the three cases and asymmetric recursive
controls by diagnostic code and location. The full unfiltered checker audit is
byte-identical at 474,251 rows: 459,451 RIGHT, 2,195 GAP and 12,605 WRONG. The
checker/execute release suite passes 1,305 tests with three ignored tests; six
conformance controls, formatting and strict workspace/all-target Clippy pass.
This follow-up does not repeat the real-app performance measurements above.
