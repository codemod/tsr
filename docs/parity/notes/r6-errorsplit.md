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
commit 3 (`c0c143f`, a comment-only reorder of the exclusion doc; it was
first cut on commit 2 and re-cut there with the pinned test below). `createUnionOrIntersectionProperty` (`checker.go:21545`) lets a
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
it, and flips `tests/element_access_miss.rs`' pin of the old exclusion
(`(x || {})["a"]` was asserted to stay the gap; it now reads
`string | undefined`). That pin is the exclusion's falsifier, and it fired as
intended: the first cut of the diff missed it, and the workspace run caught
it before the re-cut.

## §6 Diff L (`binder.rs`, `members.rs`, `callable_expandos.rs`): late-bound assignment members

[`r6-errorsplit-late-bound-assignments.diff`](r6-errorsplit-late-bound-assignments.diff)
applies on diff O (apply order: O, then L). The binder is main's,
`members.rs` is main's and `callable_expandos.rs` belongs to no lane, so the
whole piece ships as a diff, with the lift of the function exclusion in this
lane's `indexed.rs` on top.

**The forcing constraint.** `foo[strMem] = "ok"` on an expando target names
its member through a key the binder cannot evaluate. Upstream's binder
(`bindDeferredExpandoAssignment`, `binder.go:1050`) binds such an assignment
as an anonymous `__computed` property (`bindAnonymousDeclaration`) and files
it under the target's `InternalSymbolNameAssignmentDeclaration` export
(`addLateBoundAssignmentDeclarationToSymbol`, `binder.go:1000`).
`getResolvedMembersOrExportsOfSymbol`'s static arm (`checker.go:15962`) then
late-binds each with `lateBindMember`, keyed by the type of the element
access argument. The port's binder dropped the assignment
(`let name = name?;`), so the function's type had no such member: every read
gapped, and its printed type lacked the members
(`{ (): void; bar: number; }` for upstream's
`{ (): void; bar: number; [_private]: string; strMemName: string; … }`).

**The port.**

- *Binder* (`bind_late_bound_assignment_declaration`): a dynamic-named
  `foo[k] = v` (an element-access left whose key the binder could not name)
  gets a `__computed` symbol (`PROPERTY | ASSIGNMENT`, parented to the
  container's symbol as `bindAnonymousDeclaration` does for a class member,
  value declaration the assignment), and the assignment is appended to the
  declarations of the target's `__assignment` export, created flagless on
  first use. Same key and shape as upstream's table.
- *Checker* (`late_bound_members_of`): the static arm reads `__assignment`'s
  declarations after the computed members, as upstream does. A declaration
  binds when its argument is an entity name expression (`isLateBindableAST`)
  whose type is usable as a property name; literals name the member by
  value, a unique symbol by this port's bracketed entity spelling (§2's
  convention). The table is `late_bound_member_names`' existing
  `(symbol, is_static)` entry: same owner, same publication states (an empty
  in-progress entry, then the completed list), no new cache.
- *Lookup* (`get_property_of_anonymous_symbol`): a function, like a class,
  falls back to its late-bound static members after its exports.
- *Image* (`callable_export_properties`): the late members follow the early
  exports in declaration order, an early name shadowing a late one
  (`combineSymbolTables(early, late)`, `checker.go:15975`). A late member
  prints by its name type, so `const numStr = "10"` prints `"10"` where a
  numeric key prints `10` (`late_bound_assignment_printed_name`).

**Limits accepted.** `lateBindMember` gathers every declaration of one late
name into one symbol whose type is `getWidenedTypeForAssignmentDeclaration`
over all of them. The port reads the first declaration's `__computed`
symbol. No corpus line has two late assignments to one name with different
types; a test that did would falsify this. The `this[k] = v` arm of
`bindThisPropertyAssignment` (JS classes) also files into the table upstream
and is not ported here. And `foo[k] = true` under a declared callable type
widens to `boolean` where upstream reads the declared `true`
(`expandoFunctionExpressionsWithDynamicNames2`, 2 lines that stay WRONG, now
with the member present): that is `getWidenedTypeForAssignmentDeclaration`'s
declared-type arm, the same gap a named expando has.

**Measured** on commit 2 (unfiltered, both dumps) without the lift, then with
it: zero losses either way. With the lift:

- types **549,942 / 813 / 5,548**: 30 GAP→RIGHT, 57 WRONG→RIGHT, in
  `declarationEmitLateBoundAssignments{,2}`,
  `declarationEmitLateBoundJSAssignments`,
  `expandoFunctionExpressionsWithDynamicNames{,2}`,
  `expandoFunctionSymbolProperty{,Js}` and
  `lateBoundAssignmentDeclarationSupport*`;
- diagnostics **5,530 / 5,597 / 1,063 / 48**: `expandoFunctionSymbolProperty`
  EMPTY_WRONG→EMPTY_RIGHT;
- the lift moves **19 lines to `native_error`, all `errorType` natively**
  (`augmentedTypeBracketAccessIndexSignature`' `(() => { })[0]` and the
  variable it initializes, and 17 lines of `propertyAccess`, `noIndex[…]` on
  a function receiver). The 30 former false claims now compute their
  members. Credited gap 2,150 → **2,131**.

O and L stacked, measured on commit 2: **+106** (43 GAP→RIGHT, 63
WRONG→RIGHT), zero losses, the same diagnostics transition: the two diffs
are additive. slowcases clean. Perf against the base binary read 1.032 /
0.985 (21 samples) and 1.051 / 0.940 (41); an A/B against the commit-2 binary
directly reads domain-model 0.999 and 0.986 on two 41-sample runs, so the
base-binary ratio is drift between sessions of the bench, not this diff. The
added work is one exports read per function-symbol member miss, plus the
late-bound list, which `late_bound_member_names` computes once per symbol.

Tests: the diff carries `tests/late_bound_assignment_members.rs` (a late
string key reads back its member; the function's printed type carries the
late members after the early one, named by key type; a key the function
lacks is `errorType`), and flips `tests/element_access_miss.rs`' function
pin from the gap to `native_error`.

**Correction.** Commit `71d8704` claimed to re-cut diff O but committed the
first cut: a `git checkout -- .` that reset the measured working tree also
reverted the copied diff. The re-cut diff O lands in the next commit, and
`git apply` of O, L and G in that order was re-verified there.

## §7 Diff G (`members.rs`, `readonly_target.rs`): an unconstrained type parameter's apparent type

[`r6-errorsplit-unconstrained-apparent.diff`](r6-errorsplit-unconstrained-apparent.diff)
applies on diff L (order: O, L, G). It is the cause of one family of the
generic-receiver exclusion's false claims (§1.1):
`propertyAccessOnTypeParameterWithoutConstraints`' `x['toString']()` on an
unconstrained `T` is `string` natively, and the port gapped it.

`getApparentType`'s head (`checker.go:21731`) is

```go
if t.flags&TypeFlagsInstantiable != 0 {
    t = c.getBaseConstraintOfType(t)
    if t == nil { t = c.unknownType }
}
```

and the port's `apparent_type` kept the type itself when no base constraint
was found (`unwrap_or(id)`). So an unconstrained `T` fell through every arm
and stayed `T`, whose lookup finds nothing, where upstream reads `unknown`
and, without `strictNullChecks`, its apparent empty object, which falls back
to `Object`'s members. The comment beside it described upstream's behaviour
(*"An unconstrained parameter becomes `unknown`"*) while the code did not do
it. The diff maps a missing base constraint of an instantiable type to
`unknown`, as upstream does; a non-instantiable type is unchanged.

One reader depended on the old answer. `enclosing_class_from_this_parameter`
(`readonly_target.rs`, TS2445's `this`-parameter road) read `this: T` through
`apparent_type` and took a non-class answer as "no class". With `{}` it
answered `Unsupported` and dropped three TS2445 reports
(`protectedMembersThisParameter`, RIGHT→WRONG on the first measurement).
Upstream's `getEnclosingClassFromThisParameter` (`checker.go:11987`) reads
`getConstraintOfTypeParameter`, which is nil for an unconstrained parameter,
and then has no class. The diff ports that: an unconstrained type parameter
answers no class before the apparent read.

**Measured** on commit 2 (unfiltered, both dumps): zero losses; types +32
(14 GAP→RIGHT, 18 WRONG→RIGHT: `propertyAccessOnTypeParameterWithoutConstraints`
30, `typeParameterExplicitlyExtendsAny` 2); diagnostics unchanged, zero
transitions. No line moves to or from `native_error`; credited gap
unchanged. slowcases clean. `apparent_type` is hot, so perf was taken A/B
against the commit-2 binary: domain-model 1.003, generic-imports 1.008 (21
samples). The added work is a flags test on the no-constraint branch.

The diff carries `tests/unconstrained_type_parameter_apparent.rs`, which
fails without it.

**The stack.** O, L and G applied together on commit 3, measured against
commit 2: types **549,993 / 786 / 5,524** (57 GAP→RIGHT, 81 WRONG→RIGHT,
+138), diagnostics 5,530 / **5,597** / 1,063 / **48** (+1), zero losses on
both dumps; credited gap **2,131**; `HadErrorBaseline` 1,983. The parts sum
(19 + 87 + 32).

The generic exclusion stays: its other false claims are the
`Extract<keyof T, string>` keys of a `for…in` variable (`isomorphicMappedTypeInference`,
`mappedTypes4`, `keyofAndForIn`, `typeGuardsTypeParameters`,
`correlatedUnions`, `mappedTypeConstraints2`, `keyofAndIndexedAccessErrors`),
which upstream admits by relating the key to `keyof T`
(`checkIndexedAccessIndexType`, `checker.go:8220`) and the port's relater
cannot (a conditional-type source; r6-relater's lane), and
`classExtendingAny`'s `this['wot']`, an `any`-based class.
