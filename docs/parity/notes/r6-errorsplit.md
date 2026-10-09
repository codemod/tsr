# Lane notes: r6-errorsplit (tsr-2zk.1130, continuing r5-errorsplit6)

Single owner of the intrinsic/error contract, round 6. Round 5's last step is
[r5-errorsplit6](r5-errorsplit6.md). The decision record is
[ADR-0048](../../adr/0048-errortype-is-a-producer-identity-and-the-writer-keeps-its-rewrites.md):
switch a producer only where the probe confirms it line by line, and narrow a
rewrite only where that costs zero RIGHT lines. Pinned upstream:
`vendor/typescript-go` @ `5b1047d`. Owned files: `indexed.rs`, `spreads.rs`,
`jsx_intrinsic.rs`; everything else ships as a measured diff here.

## §1 Baseline and instrument

**Base.** `claude/beautiful-shannon-ar5gh0` at `b18aec06` (main `17265fac`
plus bookkeeping), frozen unfiltered:

- types 549,853 RIGHT / 843 GAP / 5,607 WRONG (556,303 aligned lines);
- diagnostics 5,530 RIGHT / 5,596 EMPTY_RIGHT / 1,063 WRONG / 49 EMPTY_WRONG.

`ceiling` at the base: credited gap **2,164** (49 attributed); `native_error`
lines 31,245 (31,139 matched); wholesale narrowing would cost 2,679 RIGHT
lines (`HadErrorBaseline` 2,013, `AtLocation` 575, `AccessOrQualifiedParent`
62, `StatementName` 29). The dispatch's 2,162 is r5-errorsplit6 §7's figure
with diff S applied; diff S is not on this base (§5).

**The probe** is r5-errorsplit4 §2.1's instrument, rebuilt as r5-errorsplit6
§1 did: the pinned tsgo from `scripts/offline-cargo/build-tsgo.sh`, the
compiler test runner built with `go test -c -overlay` over
`type_symbol_baseline.go` (tag ` @@E` for `GetErrorType()`, ` @@A` for
`GetAnyType()`, write the `.types` text to `$TSR_ERRPROBE`, skip `.symbols`)
and `compiler_runner.go` (only `verifyTypesAndSymbols` under the variable).
`-test.run '^TestSubmodule$'` writes 12,157 baselines in 42 s. A join on
`(case, file, position)` against `ceiling`'s `TSR_CEILING_DUMP` rows tags each
moved line.

**Control.** Of the base's 31,139 matched `native_error` lines, 31,133 are
`errorType` natively, 5 untagged and 1 `anyType`: the same residue
r5-errorsplit6 §1 reports (its 4 unaligned rows are folded into
"untagged" here, because this join does not re-check the line text).

**Setup note.** PyPI is still blocked; a stdlib-only stand-in for
`assemble.py`'s three `tomlkit` calls (`parse` over `tomllib`, `inline_table`
as a marked `dict`, `dumps` as a small TOML writer) was kept outside the
repository, as r5-operators3 §4 describes.

### §1.1 The exclusions, re-probed on this base

Each exclusion of `element_access_receiver_is_complete` was lifted behind a
temporary environment switch (measurement only, never committed), alone and
all together. All together, joined with the probe:

| exclusion | `errorType` | false claims (lines) | where |
|---|---:|---:|---|
| a function's type | 19 | 30 | `declarationEmitLateBoundAssignments{,2,JS}`, `expandoFunctionExpressionsWithDynamicNames`, `expandoFunctionSymbolProperty{,Js}` |
| generic index or receiver | 13 | 47 + 1 `anyType` | `isomorphicMappedTypeInference`, `mappedTypes4`, `keyofAndForIn`, `typeGuardsTypeParameters`, `correlatedUnions`, `propertyAccessOnTypeParameterWithoutConstraints`, `mappedTypeConstraints2`, `keyofAndIndexedAccessErrors`, `extractInferenceImprovement`, `conditionalTypes1`; `classExtendingAny` (`anyType`) |
| `unique symbol` index | 6 | 2 | `uniqueSymbols`, `uniqueSymbolsDeclarations` (`o[N["s"]]`) |
| union with an object literal | 1 | 8 | `destructuringAssignmentWithDefault`, `propertyAccessWidening` |
| mapped receiver | 0 | 2 | `numericEnumMappedType` |
| named reference without members | 7 | 5 | `intersectionWithIndexSignatures` (false); every `errorType` line is `typeof globalThis` |

Lines count the access and its downstream lines (an initialized variable, an
enclosing call), as r5-errorsplit6 §2.2's table does not; that is why the
function row reads 30 where §2.2 read 24.

## §2 Commit 1: `getPropertyNameFromType`'s unique-symbol arm

`getPropertyNameFromType` (`utilities.go`) names a `unique symbol` index by
the symbol's escaped name (`__@s@<id>`), so `o[N["s"]]` reads the member
declared `[N.s]` whatever expression spells the index. The port names a
late-bound symbol member by its declaration's entity text (`[N.s]`,
`late_bound_symbol_member_name`), and the element-access road read that
spelling off the index's *syntax* (`late_bound_entity_name`). An index whose
syntax is no entity name missed.

`unique_symbol_property_type` recovers the key identity the other way round:
it scans the apparent receiver's bracketed members for the one whose computed
name checks to the index's unique symbol type. It is reached only after the
entity-name read missed.

Alternatives taken seriously:

- **Rename every late-bound symbol member to `__@name@id`.** That is
  upstream's representation, and it would make the lookup one table read. It
  changes the key of every member table, printer and relater site that reads
  the bracketed spelling (members.rs, objects.rs, printing.rs, all other
  lanes'), so it is not this lane's to make. It wins once a lane owns the
  member-naming contract.
- **Spell the index's symbol back into entity text** (`N.s` from the symbol's
  parent chain). It fails when the member was declared through an alias of
  the same symbol, and it is still a spelling.

Checker port boundary: no cache or side table. The native operation is
`getPropertyOfType(apparent, getPropertyNameFromType(indexType))`; the key is
the unique symbol type (owner `unique_es_symbol_types`); the receiver is the
same apparent type the named road reads; the expensive work is one name
enumeration and one `check_expression` per bracketed member, paid only by a
unique-symbol index the entity read missed.

With the arm ported, the exclusion is lifted. Lifting it moved **6 lines, all
`errorType` natively**: `a[Symbol.isConcatSpreadable]`
(`modularizeLibrary_UsingES5LibES6ArrayLibES6WellknownSymbolLib`), `o[sym]`
(`noImplicitAnyStringIndexerOnObject`), `both[sym]`
(`unionTypeWithIndexSignature`) and three lines of
`lateBoundAssignmentDeclarationSupport1`. The two former false claims now
compute their member: `uniqueSymbols:0:376` and
`uniqueSymbolsDeclarations:0:373`, `o[N["s"]] : "b"`, GAP-printed-`any` →
RIGHT.

## §3 Commit 1: `typeof globalThis` is a complete receiver

r5-errorsplit6 §2.4 ported `typeof globalThis`' element-access lookup
(`global_this_property_type`, the non-block-scoped globals table). The
named-reference exclusion still covered it, because the `typeof globalThis`
type has no members table, which is the exclusion's test. The exclusion's
reason is a member image the port cannot read; for `globalThis` the port
reads the same table `resolveAnonymousTypeMembers` builds. So the receiver is
complete, and its misses are upstream's `errorType`.

Measured with the probe: **7 lines move, all `errorType`**
(`globalThisBlockscopedProperties` ×2, `globalThisUnknown` ×2,
`globalThisUnknownNoImplicitAny` ×2, `thisPropertyAssignmentComputed`, whose
top-level `this` in a script is `typeof globalThis`). The named-reference
exclusion keeps its other arm: `intersectionWithIndexSignatures`'
`constr<{}, …>` (5 false-claim lines) is an unexpanded alias, which waits on generic alias expansion.

### §3.1 Commit 1, measured

Unfiltered against §1's base, both dumps:

| | base | commit 1 |
|---|---:|---:|
| types RIGHT / GAP / WRONG | 549,853 / 843 / 5,607 | **549,855 / 843 / 5,605** |
| diagnostics | 5,530 / 5,596 / 1,063 / 49 | unchanged, zero transitions |
| type / diagnostics losses | — | **0 / 0** |
| credited gap | 2,164 | **2,151** (−13) |
| `native_error` lines (matched) | 31,245 (31,139) | 31,258 (31,152) |
| `HadErrorBaseline` RIGHT→GAP | 2,013 | 2,001 |

The 13 moved lines are all `errorType` natively. Gains: the two `o[N["s"]]`
lines, WRONG→RIGHT. slowcases is clean on both dumps. Perf, median child CPU
over 21 samples against the base binary: domain-model 0.995,
generic-imports 1.028 (diagnostics match).

Tests: `tests/unique_symbol_index.rs` pins `o[N["s"]]` as `"b"`, a unique
symbol the receiver lacks as `errorType`, and `globalThis['y']` for a `let`
as `errorType` beside `globalThis['x']` for a `var` as `number`.

## §4 Commit 2: the element-access receiver is widened for a write or a call

`checkElementAccessExpression` (`checker.go:8148`) widens the receiver of an
assignment target or a called method before the lookup
(`getWidenedType(objectType)`). The property-access road already did
(`crate::members`, `widen_object_literal_freshness`); the element road only
passed a flag to its object-literal fallback. So `(options || {})["a"] = 1`
looked the name up on the unwidened `{ a: string; b: number; } | {}`, where
`getWidenedType` reduces the union to `{}` (an empty object constituent asks
for subtype reduction) and the name misses.

Measured on commit 1 (unfiltered, both dumps): zero transitions. One line
moves from the gap to `native_error`, `propertyAccessWidening:0:63`, and it
is `errorType` natively. Credited gap 2,151 → **2,150**. slowcases clean.
Perf, median child CPU against the base binary: domain-model 1.030 over 21
samples, re-run over 41 as 1.023; generic-imports 1.029, then 1.016. The
added work is one `widen_object_literal_freshness` per written or called
element access, which returns a non-fresh type unchanged.

The read of the same union is §5's diff: `createUnionOrIntersectionProperty`'s
object-literal arm lives in `crate::members`.

Tests: `tests/element_access_widening.rs` pins the write as `errorType` and a
declared receiver's write as its member type.

## §5 Diff O (`members.rs`, main's): the object-literal arm of a union property

[`r6-errorsplit-objlit-union.diff`](r6-errorsplit-objlit-union.diff) applies on
commit 2. `createUnionOrIntersectionProperty` (`checker.go:21545`) lets a
union constituent that lacks the name still contribute: an applicable index
signature contributes its value type, and a spread-free object-literal type
(`isObjectLiteralType(t) && t.objectFlags&ObjectFlagsContainsSpread == 0`)
contributes `undefined` and makes the property `WritePartial`. Anything else
makes it `ReadPartial`, which `getPropertyOfUnionOrIntersectionType` drops.

The port's union projection in `get_type_of_property_with_this_argument` had
the index arm and not the object-literal one; the predicate exists one
function away (`get_property_of_union_or_intersection_type` reads
`object_literal_spread_flags` for the same arm). The diff adds the arm, and on
top of it lifts the union exclusion of `element_access_receiver_is_complete`
in `indexed.rs` (this lane's file; the lift is part of the diff because it is
only sound with the arm).

**Measured** on commit 2 (unfiltered, both dumps): zero losses;
types **549,874 / 830 / 5,599** (13 GAP→RIGHT, 6 WRONG→RIGHT); diagnostics
unchanged, zero transitions. The 19 lines are `destructuringAssignmentWithDefault`
(13, `(options || {})[0]`, `.color`, `["color"]` and the bindings they
initialize) and `propertyAccessWidening` (6: `x1`, `(options || {}).a`, `x2`,
`(options || {})["a"]`). Credited gap unchanged (2,150): the moved lines were
the uncredited gap or WRONG. The lift moves no line to `native_error`, so no
false claim remains under the old exclusion. slowcases clean; perf against
the base binary domain-model 1.029, generic-imports 1.027 (the same noise
band commit 2 read before its 41-sample re-run).

The property-access write `(options || {}).a = 1` still answers the gap: the
property road's miss is `crate::members`', not this lane's.

The diff carries `tests/union_object_literal_property.rs`, which fails without
it.
