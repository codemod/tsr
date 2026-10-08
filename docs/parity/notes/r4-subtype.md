# r4-subtype — lane notes

Round-4 lane for the `strictSubtypeRelation` pairs that the relater left
undecided, so that `removeSubtypes` declined the whole reduction to a gap
(`tsr-2zk.921`). Native reference: `vendor/typescript-go` @ `5b1047d`. Each
arm below was traced in the pinned source before it was ported. Each section
names the native code, what TSR did instead, and the cases it converts.

## 0. Census

A temporary, uncommitted `eprintln!` at the two `Ternary::Unknown` exits of
`union_with_subtype_reduction` (`unions.rs`) ran over the unfiltered types
corpus with `RAYON_NUM_THREADS=1`, so each pair could be attributed to its
case. The `reasons` mask came from `relater::reasons::last_unknown`. It found
the predecessor's 40 pairs (`r4-unions.md` §5), at `6417762`:

| pair (strict subtype) | n | where TSR stopped | section |
|---|---|---|---|
| `[number, number]` → `ConcatArray<[number, number]>` | 15 | no member table (tuple source) | §1 |
| `[number]` → `Promise<[0]>` | 4 | no member table, then absent property | §1 |
| `[1 \| 2, "bar" \| "foo"]` → `{ kind: 123; }` | 1 | as above | §1 |
| `[string, number]` → fresh `{}` (two shapes) | 2 | silent | §2 |
| `string` / `number` → `Function` (`anyFunctionType`) | 4 | no member table (target) | §3 |
| `"whatever"` → `{}` (`unknownEmptyObjectType`) | 2 | no member table (target) | §3 |
| `() => any` → `() => Base` / `() => T` | 6 | `any` → object / type parameter | §4 |
| `T extends E` → `typeof E` | 2 | silent (primitive apparent arm) | §5 |
| `QuickPickItem` → `unique symbol` | 1 | unported flag | §6 |
| `T[keyof T]` → `null` | 2 | silent (no base constraint) | §7, not ported |
| `NonNullable<Partial<Config>[T]>` → `Config[T]` | 1 | composite shape | §7, not ported |

## 1. Tuple source in the structural arm

**Native.** A tuple is a type reference to a tuple target whose resolved
members are its element properties, `length` and the `Array` (or
`ReadonlyArray`) members (`resolveTupleTypeMembers`). Against an object target
that is neither the same reference nor an array, `structuredTypeRelatedToWorker`
reaches the structural arm (`relater.go:3864`): `propertiesRelatedTo`, then call
and construct `signaturesRelatedTo`, then `indexSignaturesRelatedTo`. In
`propertiesRelatedTo` (`:4232`), `requireOptionalProperties` is false for a
tuple source. `getUnmatchedProperty` then rejects a required target property
the tuple lacks (`Promise`'s `then`, `{ kind }`).

**TSR before.** `is_related_to_with_flags` routed a tuple source into the
recursive walk only against another tuple or an array. Otherwise it fell
through to the final `Unknown` and noted "no members table". Inside
`properties_related_to_with_optionals`, a tuple has no
`get_property_names_of_type`, so even a missing `then` read as "absent,
unknown".

**Change.** `is_structural_tuple_source` holds for a tuple whose spreads are all
concrete arrays. Generic tuples still relate through their base constraint
first (`:3849`) and are left out. Such a source enters the recursive walk
against a target with members, and the worker's structural arm accepts it. Its
member reads already go through `get_type_of_property_of_type`. Missing target
properties use `tuple_source_lacks_property`, which reads the complete list that
`Checker::tuple_target_properties` already builds. Symbol-named members are not
in that list, so a missing one stays undecided. A missing optional target
property is related for a tuple source, as `requireOptionalProperties` says. No
new state.

The target must carry `TypeFlags::OBJECT`, as the native arm requires
(`target.flags&TypeFlagsObject`). The first version gated on `has_members`
alone. TSR gives some deferred conditional types (`Tail<Tail<T>>` in
`compiler/ramdaToolsNoInfinite`) a member image, so a tuple source was compared
member by member against a conditional. Reading the tuple's members then
re-entered the alias under construction and raised a false TS2502. That was
one diagnostics loss and one types loss in the unfiltered run, and both are
gone with the flag check. Native takes such a pair through the
conditional-target arm, which this lane does not own.

A tuple's index infos come from its `Array`/`ReadonlyArray` base: one numeric
index whose value is the element union. `get_index_infos_of_type` does not
supply that for a tuple. Through `related_index_signatures`, a target with a
numeric index (`interface CustomArray<T> extends Array<T> {}`) therefore read
"no applicable source info, no inferable index", which is NotRelated. That
broke `const_sources_preserve_literal_origins_and_mutable_variables`:
`custom<const T extends CustomArray<string>>(["a","b"])` printed
`CustomArray<string>`. `tuple_index_signatures_related` is
`indexSignaturesRelatedTo` (`relater.go:4578`) for a tuple source. It relates
the element union (`tuple_element_union`, now shared with
`tuple_array_related_to`) to every target info a numeric key applies to.
Any other target info fails, because a tuple reference is not an object type
with an inferable index (`typeRelatedToIndexInfo`, `:4603`).

**Beside r4-arrays' arm.** The integration branch also carries r4-arrays'
`tuple_source_non_array_target` (`56a3fa3`). It is a negative-only check of
the same `propertiesRelatedTo` rule and sits in the fallthrough after the
structural gate. The two agree wherever both apply, so the merge keeps both.
The full walk here runs first for an object target with a member table.
Theirs still answers where this gate does not fire.

**Converts** `compiler/concatTuples` (2 lines) and `conformance/literalTypes2`
(4 lines).

## 2. Fresh empty object literal target under the subtype relations

**Native.** `structuredTypeRelatedToWorker` (`relater.go:3853`): under
`subtypeRelation` or `strictSubtypeRelation`, an empty-object target carrying
`ObjectFlagsFreshLiteral` rejects every non-empty source. So `options || {}`
with a tuple `options` keeps `{} | [string, number]`.

**TSR before.** The pair reached no deciding arm.

**Change.** The arm, keyed on `fresh_object_literal_types` (the port's
`FreshLiteral`) and `is_empty_anonymous_object_type`. The source must be
certified non-empty: a structural tuple (it has `length`) or a type whose
property names enumerate to a non-empty list. Any other source keeps its
earlier path, because `isEmptyObjectType` is not ported for it here.

**Converts** `compiler/destructuringAssignmentWithDefault` lines 58, 59, 110,
111.

## 3. Intrinsic object types with no members

**Native.** `emptyObjectType`, `unknownEmptyObjectType` and the
`anyFunctionType` wildcard (`checker.go:1029`) are anonymous types with no
properties, signatures or index infos. A primitive source reaches them through
its apparent type. An object source reaches the structural arm: properties and
index infos require nothing, so `String → {}` is True. `signaturesRelatedTo`
(`relater.go:4449`) answers False when the target is `anyFunctionType`, so
`String → anyFunctionType` is False.

**TSR before.** These intrinsics are `Named` images with `members: None`.
`has_members` is false, so the structural gate never routed to them and the walk
fell through to `Unknown`. The `§369` text-match arm for `{}` sits after the
primitive apparent arm and was never reached for these pairs.

**Change.** `is_memberless_object_intrinsic` recognises the three by identity.
The structural gate admits them as targets of a source with members. The worker
answers them before the general structural arm, folding in a failed
source-intersection result as the general arm does.

**Converts** `conformance/nullishCoalescingOperator2` and
`conformance/nullishCoalescingOperator_es2020` (lines 35, 36 each). It also
decides `string`/`number` → `Function` in `mappedTypeRecursiveInference2` and
`reverseMappedPartiallyInferableTypes`; their lines are blocked elsewhere.

## 4. `any` source under the subtype relations

**Native.** `isSimpleTypeRelatedTo` relates an `any` source to anything only
under the assignable and comparable relations (`relater.go:261`). Under the
subtype relations only `any` and `unknown` targets accept it, and under the
strict one `unknown` does not (`:212`). In `structuredTypeRelatedToWorker` the
type-parameter target arm (`:3423`) relates only a mapped source or a
comparable pair. The default arm's apparent `any` is not an object, so the
structural arm (`:3864`) does not run. `() => any` is therefore not a strict
subtype of `() => Base` or `() => T`, and `[() => any, () => Base]` keeps both.

**Change.** An `any` source against a type parameter, or against an object
target, is `NotRelated` under `Subtype`/`StrictSubtype`. The object target must
not be a mapped type or a generic mapped target, which keep the keyof-based arm
(`:3593`), and must not be a qualified alias mint, whose flags are not evidence.

**Converts** 12 lines of `conformance/heterogeneousArrayLiterals`, plus
`compiler/coAndContraVariantInferences6` (1 line), `conformance/parserArgumentList1`
(2), and `conformance/taggedTemplateStringsWithOverloadResolution3` and its
`_ES6` twin (2 each). Attribution was checked by disabling each arm in turn on
a filtered run.

## 5. Primitive apparent arm against an enum object

**Native.** `T extends E` relates to `typeof E` through its constraint, the
enum literal `E.A`. That literal's apparent `Number` fails `propertiesRelatedTo`
on the missing `A`.

**TSR before.** The primitive arm in `is_related_to_with_flags` takes a target
with index infos (a numeric enum object carries the reverse-mapping index) only
when it can prove a property failure. It required `has_members(target)`, and the
`Anonymous` enum object has no member table, so the pair stayed `Unknown`.

**Change.** That proof needs only the target's names, so the arm also accepts a
target whose `get_property_names_of_type` enumerates.

**Converts** `conformance/subtypesOfTypeParameterWithConstraints2` lines 271,
272, 276 and 277. It also converts the diagnostics cases
`compiler/enumAssignmentCompat` and `compiler/enumAssignmentCompat2`
(WRONG → RIGHT), where an assignment that should report now does.

## 6. Object source against `unique symbol`

**Native.** `isRelatedToEx` (`relater.go:2605`) answers an object source against
a primitive target by `isSimpleTypeRelatedTo` alone. That function's only object
arms are the `any`/`unknown`/`never`/`object` targets, so an object never
relates to a `unique symbol`.

**TSR before.** The `§357` arm decides object → primitive only for
`flag_decidable` targets, and `UNIQUE_ES_SYMBOL` is deliberately absent from
that set (its own simple arms are unported).

**Change.** The `§357` arm also takes a target whose flags are exactly
`UNIQUE_ES_SYMBOL`. `FLAG_DECIDABLE` is unchanged, because its contract is about
the simple arms for that flag as a *source*.

**Converts** `conformance/generatorYieldContextualType` line 93.

## 7. Not ported here

- **`T[keyof T]` → `null`** (`indexedAccessConstraints`, 2 pairs). Native's
  source type-variable arm (`relater.go:3665`) relates `getConstraintOfType(source)`,
  or `unknown` when it is nil. For an indexed access that is
  `getConstraintFromIndexedAccess`, which can build `T[string | number | symbol]`
  rather than nil. TSR's `base_constraint_of_type` answers `None` here, and the
  relater cannot tell native's nil from an unported constraint. Answering
  `unknown → null` would guess at that. The faithful fix is the indexed-access
  constraint supplier, outside this lane's files.
- **`NonNullable<Partial<Config>[T]>` → `Config[T]`** (`correlatedUnions`, 1
  pair). An intersection source against an indexed-access target is the
  indexed-access target arm, which belongs to main (`f2c97d13`).

## 8. Measured

Against a baseline frozen at integration head `0cd6c43`, with all six arms
applied, unfiltered:

- types: 470069 → 470107 RIGHT (+38 lines, 0 lost); GAP 946 → 921.
- diagnostics: +2 cases (`enumAssignmentCompat`, `enumAssignmentCompat2`), 0
  lost.
- callgrind on `generate_perf_project.py --modules 100`: 4,245,527,171 →
  4,252,710,937 Ir (+0.17%). The new arms run only where the walk previously
  fell through to `Unknown`, plus one flag test in the structural gate.
- The census exits now hold 3 of the original 40 pairs (§7).

After merging the integration head `5ad60b1` (which carries r4-arrays'
tuple-source negative), against a baseline frozen there: types 470138 →
470172 RIGHT (+34, 0 lost), diagnostics +2 (the same two), 0 lost.
`literalTypes2`'s four lines are already RIGHT at `5ad60b1` through
r4-arrays' arm.

## 9. `r4-unions-array-literal-subtype-gate.diff` (`tsr-2zk.922`)

Re-measured on top of these arms, against the `0cd6c43` baseline: still **10
lost**, the same 10 as the predecessor reported. None of them is a relater
pair. With the census instrumentation re-applied to the gated build, no pair
reached a `removeSubtypes` `Unknown` exit in those four cases.

- `compositeContextualSignature:4,5`, `typeInferenceLiteralUnion:24` and
  `arrayLiteralInference:33,44,58` go GAP at the contextual-position decline
  inside the gated block of `array_literals.rs`
  (`!uncontextual && !literal_free → return error`). That decline runs
  before any relation is asked. The old `object_constituent_count > 1` gate
  kept these literal unions out of the block entirely.
- `enumBasics:78,79,98,99` go WRONG on order: `(E7 | E8 | E3 | E4)[]` prints
  `(E3 | E4 | E7 | E8)[]`. The subtype path rebuilds the union from the
  sorted list and drops the named-union origin. This is the union-origin
  entry-order cluster in `unions.rs` (`r4-unions.md` §5).

So the gate's losses need the `array_literals.rs` contextual decline and the
union-origin construction, not more relater arms. Against the arms-only state
the gate adds 22 lines (`heterogeneousArrayLiterals` 18,
`typeArgumentInferenceTransitiveConstraints` 3, `typeRelationships` 1).
Against the `0cd6c43` baseline, arms plus gate: +60 / −10. This was measured
before the tuple index fix in §1; the fix changes no corpus line.
