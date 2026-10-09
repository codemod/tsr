# r5-relater6 — mapped indexed access, keyof Unknowns, TS2536 (`tsr-2zk.1035`)

Lane under `tsr-2zk.1035`, continuing r5-relater5 ([`r5-relater5.md`](r5-relater5.md) §5).
Owns `crates/tsr-checker/src/relater.rs`, `index_access_reports.rs` and the
tests for its items. Native anchors are `vendor/typescript-go` @ `5b1047d`.

## 0. Frozen base

`ccb48e7`, the integration head carrying r5-relater5's `fe31884` (merge
`e9cc3c8`).
- `diagverdictdump`: RIGHT 5368, EMPTY_RIGHT 5584, WRONG 1223, EMPTY_WRONG 63.
- `verdictdump`: RIGHT 544166, WRONG 7379, GAP 988.
- Callgrind `Ir` of the base `tsr` (`-p <project> --singleThreaded --pretty
  false`): generic-imports 343,420,025; domain-model 1,201,770,992.

Every `Ir` number below uses that command. The complete CLI output of each
run is compared with the base's (`cmp`).

## 1. IAM — getSimplifiedIndexedAccessType's mapped arm

**Forcing constraint.** Native's `isRelatedToEx` normalizes both sides
before anything else (relater.go:2619): source reading, target writing.
For an indexed access, `getSimplifiedIndexedAccessType` (checker.go:27915)
rewrites `{ [P in K]: E }[X]` to `E[P := X]` when the object is a generic
mapped type without a remapping `as` clause (:27963). It adds `undefined`
when the mapped type, or its modifiers type, has `?`
(`substituteIndexedMappedType`, :29291). So `Partial<T>[K]` relates as
`T[K] | undefined`, and `Readonly<U>[K]` as `U[K]`. The port related the
unsimplified indexed accesses. r5-relater5 made that pair answer `Unknown`
(its §2 decline), so the eight `mappedTypeRelationships` errors were missing.

**Ported** (`Relater::simplified_indexed_access`, called from
`is_related_to_with_flags` right after the `NoInfer` normalization; the
simplified pair re-enters the gate, as native's loop normalizes again):
- the object is simplified first (a nested indexed access);
- a union index over a generic mapped object distributes
  (`distributeObjectOverIndexType`): a union when reading, an intersection
  when writing;
- the substitution instantiates the template (`mapped_template_type`, which
  already carries the mapped type's own `?`) with `P := X`;
- `isOptional`: the type's own `+?`, else `getCombinedMappedTypeOptionality`
  of the modifiers type (`isGenericType(objectType)` always holds for a
  generic mapped type). A `-?` mapped type over a `?` modifiers type is
  therefore optional, as native's is;
- `mapType(.., getSimplifiedType)` over the result's indexed-access
  constituents, and `removeType(result, t)`.

**Alternatives.**
- The whole of `getSimplifiedIndexedAccessType`: distribution over a union
  index for every object, over union and intersection objects, and the
  generic-tuple arm. Rejected for this commit. Each of those changes what
  an existing pair relates through, so each needs its own loss measurement,
  and no census case asks for them. Stated as a divergence on the function.
- A memo, as native's `cachedTypes`. Not built: the arm reaches
  `instantiate_type` only for an indexed access whose object is a mapped
  type. Domain-model makes 15 substitutions, costing 212,730 `Ir` in all.
  Would win if a project made that number large.

**Accepted.**
- A remapping or filtering `as` clause is left unsimplified. The filtering
  kind needs `getMappedTypeNameTypeKind` (item 3).
- A step the port cannot compute leaves the whole side unsimplified, and
  the indexed-access pair arm keeps its decline for a generic mapped object.
  Such steps are: an unclassifiable mapped type, a failed instantiation, an
  unknown modifiers type, or 32 nested levels.
- The printed source of the TS2322 head is still the unsimplified
  `Partial<T>[keyof T]`, where native prints `T[keyof T] | undefined`
  (`reportRelationError` prints the normalized type). The suite compares
  position and code only. The head is `assignreport.rs`'s (r5-missingprop).

**Not converted, and why.**
- `conditionalTypes1` 114-117 (`f8`): the substitution happens.
  `FunctionPropertyNames<T>` becomes the template's conditional, `T[keyof
  T] extends Function ? keyof T : never`. That conditional is a mapped
  template conditional (`mapped_conditionals`), and r5-relater4's gate
  declines every such pair (`Unknown`, r5-relater4 §2). That decline is
  about `as`-clause filtering, so it moves with item 3.
- `mappedTypeConstraints2` 42 (`Mapped6<K>[keyof Mapped6<K>]`): a
  remapping `as` clause, which native does not substitute either. It
  belongs with item 3.

**Measured** against §0, both dumps unfiltered, both loss checks empty:
- diagnostics: case verdicts unchanged; 10 lines converted:
  - `mappedTypeRelationships` 30, 35, 40, 41, 45, 46, 61, 66 (missing
    TS2322 now reported);
  - `inferenceShouldFailOnEvolvingArrays` 5, 13 (extra TS2322 gone);
- types: unchanged.

**Perf.**
- `Ir`: generic-imports 343,420,025 → 343,418,605 (−0.0004%); domain-model
  1,201,770,992 → 1,203,251,922 (+0.12%).
- The gate's per-entry cost is one flags test per side; generic-imports
  shows none. Domain-model's increase is downstream: 212,730 `Ir` is the
  15 substitutions, and the rest is relating the simplified pairs, which
  used to stop at the `Unknown` decline. Examples are `(() => void)[] |
  undefined → never[]` and `never[] → T[]` from narrowing `{ [K in keyof
  Events]?: … }[K]`. Native does that work too. CLI output is identical.
- Median child CPU, new/old: 21 samples gave domain-model 1.027 and
  generic-imports 1.040. The 41-sample re-run gave 1.018 and 0.991.

**Falsifier.** A pair whose simplified form decides differently from
native's printed normalized type. Also a `-?` mapped type over a
non-optional modifiers type that gains `undefined`.

Test: `tests/generic_mapped_relations.rs`
`indexed_access_of_a_generic_mapped_type_is_substituted`.

## 2. `tsr-2zk.1049` — generic keys against `keyof`, and the TS2536 reporter

### 2.0 Rebased baseline

Before this commit the branch fast-forwarded to the integration head
`617fd8d`. That head carries §1 (`2465ac0`) and batch W's relater change
(r5-declared's `isEnumTypeRelatedTo`). The baseline was frozen again
there ("base2"):
- `diagverdictdump`: RIGHT 5374, EMPTY_RIGHT 5584, WRONG 1217, EMPTY_WRONG 63;
- `verdictdump`: RIGHT 544661, WRONG 6907, GAP 965;
- `Ir`: generic-imports 343,578,498; domain-model 1,233,235,307.

### 2.1 Method

`checkIndexedAccessIndexType` (checker.go:8220) was ported first, as
r5-index4 had prototyped it ([`r5-index4.md`](r5-index4.md) §3). Every false
TS2536 was then traced to the relation that answered NotRelated. On the
`ccb48e7` base with §1, the first measurement gave 23 false reports in 17
cases, and 5 of the 20 baseline lines. Each false report had one of five
causes. Three are relater bugs, fixed in this commit (§2.2). Two belong to
the reporter (§2.3).

### 2.2 Relater fixes (this commit)

1. **An alias-reference image is related as its body.** `declared.rs` gives
   a type parameter constraint written `Keyof<Registry>` (`type Keyof<T> =
   keyof T & string`) an OBJECT-flagged `Named` image whose symbol is the
   alias. The gate's "object against a decidable primitive" arm (§357) then
   answered `Keyof<Registry> → "a"` NotRelated. Native's alias
   instantiation is its body, `"a" | "b"`
   (`templateLiteralTypes6`, `S extends Keyof<Registry>` indexing
   `Registry`). The gate now relates such an image as its evaluated body
   (`non_object_alias_image_body`, from `evaluate_alias_body`'s
   `(symbol, arguments)` cache).

   **Stated divergence.** A body with an object, intersection or `object`
   constituent keeps the image. The first version substituted every
   non-object body. `Either<L, A> = Left<L, A> | Right<L, A>` then ran
   `varianceProblingAndZeroOrderIndexSignatureRelationsAlign` past 8 GB
   (base: 41 s), the unbounded expansion r5-relater4 §1 met in the same case.
   The image keeps those pairs on the alias-variance road.

   Also measured on the way: answering `Unknown` for every alias image
   lost two `parenthesisDoesNotBlockAliasSymbolCreation` TS2352s and added a
   `controlFlowFavorAssertedTypeThroughTypePredicate` TS18048. Both are
   object-bodied aliases (`InvalidKeys<"a">`, `Record<string, unknown>`),
   which the body test keeps.
2. **The template-literal and string-mapping source cases after a failed
   union or intersection walk** (`string_like_source_constraint`,
   relater.go:3772, :3782). `unionOrIntersectionRelatedTo`'s failure falls
   through for an instantiable source (relater.go:3380). `` `${T2}` `` with
   `T2 extends "a" | "b"` relates to `keyof TypeMap` (`"a" | "b"`) through
   its base constraint. No single constituent accepts it
   (`templateLiteralTypes5`).
3. **The same fallthrough for a type-parameter source.** This is
   r5-relater5's held B12 ([`r5-relater5.md`](r5-relater5.md) §3), applied
   to type parameters only. `S extends Keyof<Registry>` reaches `"a" | "b"`
   whole, after (1) gives the constraint its body.

   **Stated divergence.** An indexed-access source keeps the walk's answer.
   Native falls through for it too, but that still loses the 6
   `quickinfoTypeAtReturnPositionsInaccurate` type lines r5-relater5
   measured (re-measured here: the same 6). Its narrowing reads an undecided
   relation as a decision (`flow.rs`, main's).

Measured against base2, both dumps unfiltered, both loss checks empty:
- `stringMappingDeferralInConditionalTypes` EMPTY_WRONG → EMPTY_RIGHT (its
  extra TS2322 at 18:5 is gone). Its type lines `:0:13` and `:0:15` go
  WRONG → RIGHT; cause (2).
- **Accepted extra lines, in a case already WRONG:**
  `thislessFunctionsNotContextSensitive1` 163:3 and 176:3, TS2322. The
  port infers `target: string` for the object literal argument of
  `test55124<OptionsData extends SetType<OptionsData>>`. Native keeps the
  literal `"$test4"`, so 163 is OK there, and 176 is TS2820 at the same
  position. `string → ExtractFields<…> | undefined` was `Unknown` through
  the alias image and is now native's NotRelated through its body. The
  relation is right; the input is wrong. The literal inference belongs to
  `inference.rs`/`contextual.rs` (main's).
- `Ir`: generic-imports 343,578,498 → 343,574,526 (−0.001%); domain-model
  1,233,235,307 → 1,232,346,522 (−0.07%). CLI output is identical.
- Median child CPU, new/old: 21 samples gave domain-model 1.071 (with `Ir`
  down) and generic-imports 1.005. The 41-sample re-run gave 0.991 and
  0.996.

Unit test: `relater.rs` `generic_key_tests`. On base2 it fails: `V →
"a" | "b" | "c"` answers NotRelated.

### 2.3 Held: the TS2536 reporter

[`r5-relater6-ts2536-reporter.diff`](r5-relater6-ts2536-reporter.diff)
ports `checkIndexedAccessIndexType` into `index_access_reports.rs`. It
covers the type-node site (`checkIndexedAccessType`) and the element-access
site (`checkElementAccessExpression`):
- every index constituent must be assignable to `keyof T`, or applicable
  to a number index of `T`'s apparent type (`getIndexInfoOfType` reads
  `getReducedApparentType`);
- a remapping `as` clause uses `getIndexTypeForMappedType`;
- a constituent whose relation is `Unknown` declines;
- a literal key into a generic object declines (TS4105's private-member
  arm is not ported).

The type-node site carries `getConditionalFlowTypeOfType`'s constraint
walk (checker.go:24952, `getImpliedConstraint` :24989):
- an index reference in a conditional's true branch relates as `K & C`;
- an object reference with a constraint, or another node kind inside a
  true branch, declines;
- the homomorphic mapped-type arm declines.

Without that walk, `conditionalTypeSubclassExtendsTypeParam`,
`neverAsDiscriminantType` ×2 and `stringMappingReduction` each got a false
report.

**Measured** with §2.2, on the `ccb48e7` + §1 base:
- 5 TS2536 lines converted: `constraintWithIndexedAccess` 29,
  `intersectionsOfLargeUnions` 21, `intersectionsOfLargeUnions2` 31,
  `mappedTypeErrors2` 13 and 15;
- 2 false reports: `ramdaToolsNoInfinite2` 45:83 and 60:42;
- no case verdict changes; no type changes.

**Why held.** The bar is zero extra TS2536. Both false reports have one
cause: imports inside an ambient module declaration in a script file do
not resolve. Repro:

```ts
declare module "B/B" { export type Boolean = 0 | 1; export type Bit = 0 | 1; }
declare module "U" { import { Boolean, Bit } from "B/B"; export const x: Boolean; export const y: Bit; }
```

The port reports `Cannot find name 'Bit'. Did you mean 'Bit'?` (TS2552),
and `Boolean` resolves to the global `interface Boolean`. So `strict extends
Boolean` indexes `{ 1: …; 0: … }` with a constraint native reads as `0 |
1`. Native's `declareModuleMember` (binder.go:397) files such an import in
the ambient module's locals, with an export symbol under ExportContext.
The port's binder routes it through `export_context` into the module's
exports only (`tsr-binder` `binder.rs`, the `exported` match near :3348),
where name resolution declines it. That file is not this lane's. The
reporter lands once that resolution is fixed, re-measured to zero extras.

**Not reached by the reporter**, and why:
- `mappedTypeRelationships` 20, 21, 25, 26 (`x[k]`, `T` indexed by `keyof
  U`): the element-access producer answers `error` instead of minting
  `T[keyof U]` (`members.rs`/`expressions.rs`, main's), so there is no
  indexed access to check.
- `keyofAndIndexedAccessErrors` 73/74, `infiniteConstraints`,
  `circularIndexedAccessErrors`, `unknownControlFlow` 283,
  `assignmentToAnyArrayRestParameters` 18 (a literal key into a generic
  object, declined above): not traced.

## 3. As-clause mapped types — measured, blocked outside relater.rs

Target lines: `mappedTypeAsClauseRelationships` 12 and 22;
`mappedTypeConstraints2` 10, 16, 59 and 90. `conditionalTypes1` 114-117
moved here from §1.

**The gate.** Every pair with a mapped-template conditional
(`mapped_conditionals`) answers `Unknown` (r5-relater4 §2). That covers an
`as` clause such as `T[P] extends Function ? P : never`, and a substituted
template conditional such as `FunctionPropertyNames<T>`'s. All the target
lines pass through it.

**Experiment** (relater.rs only; lift that decline). Diagnostics against
§2's dumps:
- no target line converted;
- 3 new extras:
  - `mappedTypeAsClauseRelationships` 11:9 (`T → Filter<T>`, native OK);
  - `mappedTypeConstraints2` 32:7 and 50:7 (`obj[key]` with `key: keyof
    Mapped5<K>` against `` `_${string}` ``, native OK).

Refused.

**Cause 1: the mapped iteration parameter's identity.** `T → Filter<T>`
enters the generic-mapped-target arm. It relates the `as` type `T[P]
extends Function ? P : never` to `keyof T` through its default constraint
`P`. Native's `P` is `getTypeParameterFromMappedType(Filter<T>)`, the
declared parameter instantiated with the instance's mapper, so its
constraint is `keyof T` of the *caller's* `T`. The port shares the
declaration's `P` (TypeId 10499 in the trace), whose constraint is the
*alias's* `keyof T`. `keyof T_alias → keyof T_fun` needs `T_fun →
T_alias`, which is NotRelated. `T → Modify<T>` (line 12) stops at the same
step and ends `Unknown`.

This is r5-relater5 §2's "mapped iteration parameter" decline, met from
the name-type side. The faithful fix gives each mapped instance its own
parameter, with the instance's constraint, and substitutes it in the
template and the `as` type. That is a capture change in `mapped.rs`
(r5-mapped3) plus a type-parameter identity the checker does not have. A
relater-scoped constraint override for the target's `P` was considered
and not built. It would cover only pairs where one instance's `P` is in
play, and the extras of cause 2 would keep the lift refused anyway.

**Cause 2: the filtering kind's keys.** With the decline lifted, `name →
P` relates, so `mapped.rs`'s `mapped_indexed_access_constraint` classifies
`Mapped5`'s `as` clause as filtering (native's
`getMappedTypeNameTypeKind`). It then substitutes as native does. But the
base constraint the port computes for `keyof Mapped5<K>` is the whole key
domain. Native's is the filtered keys (`getIndexTypeForMappedType` over a
generic constraint, `forEachType(constraintType, addMemberForKeyType)`,
checker.go:26892). So `obj[key]` reads the template at keys the filter
removes. `mapped_index_type` computes remapped keys only for non-generic
constraints (`mapped.rs`, r5-mapped3).

**Remapping lines** (`mappedTypeConstraints2` 10, 16, 42, 59 and 90) are
`Mapped2<K>[`get${K}`]`-shaped sources. Native relates them through
`computeBaseConstraint` of the indexed access, since the remapping kind is
not substituted. Not traced further. They wait on the same `keyof`
computation.

**Needed outside this lane:**
- per-instance mapped type parameters (`mapped.rs` capture, checker type
  identity);
- `getIndexTypeForMappedType` for a generic constraint with an `as` clause
  (`mapped.rs` `mapped_index_type`).

With both in place, re-run the experiment above: lift the
`mapped_conditionals` decline in the relater gate.

## 4. IAW — the indexed-access target's write constraint

**Forcing constraint.** `structuredTypeRelatedToWorker`'s indexed-access
target arm (relater.go:3443-3488) relates `S` to `T[K]` through
`getIndexedAccessTypeOrUndefined(baseConstraintOrType(T),
baseConstraintOrType(K), Writing | (NoIndexSignatures when T had a
constraint))`, when neither base is generic. The port built no write
constraint. It answered `Unknown` there, both for a non-indexed source in
the gate and after a failed `S[K] → T[J]` component pair
(r5-relater5 §2).

**Ported** (`indexed_access_write_constraint_related_to`,
`indexed_access_write_constraint`):
- a union key gives the intersection of the constituents' write types
  (writing), and nil when any is nil;
- a key naming a property gives the property's type;
- otherwise an applicable index signature's value, without
  `noUncheckedIndexedAccess`'s `undefined`, since this is a write;
- with index signatures excluded, a non-literal key selects nothing: nil.
- `S[K] → T[J]` takes the same step once the components fail and `T`'s base
  is not generic.

Each step the port cannot certify answers `Unknown`:
- an accessor (write types are not ported);
- a property this port cannot find;
- a tuple, array, union, intersection or `any` object.

**Held decline: a constrained object's property key.** With index
signatures excluded (`T` had a constraint), a property key's write type is
native's constraint. Deciding it lost `contextuallyTypedSymbolNamedProperties`
(EMPTY_RIGHT → EMPTY_WRONG, two TS2339 on `ap.description`):
- `typeof A → T['type']` became Related (write constraint `string |
  symbol`), as in native;
- that turned on `mapped.rs`'s `generic_mapped_contextual_property_type`;
- that function keys the computed symbol property by its display name, a
  string literal `"[A]"`, where native passes the name's type (`typeof A`,
  `getIndexedMappedTypeSubstitutedTypeOfContextualType`'s `nameType`);
- so `ap` was typed `"[A]"`.

The relation is right and the key is wrong. The path stays `Unknown` until
`mapped.rs` (r5-mapped3) passes the name type.

**Forced declines: the mapped substitution an indexed-access source cannot
reach.** Deciding the write constraint also lost `correlatedUnions` (three
TS2322s). Native relates each of those sources through
`isMappedTypeGenericIndexedAccess` (checker.go:21715): an indexed access
with a generic index into a non-generic mapped type. Its constraint is the
substitution `E[P := X]` (`getConstraintFromIndexedAccess`, :17227), and
after a failed union walk native explores `{ [P in K]: E }[constraint of
X]` (relater.go:3681). The port misses this in two ways:
1. **A concrete instance of a mapped alias is resolved to its members**
   (`Partial<Foo1>`, `Partial<Config>`). `constraint_of_type` then cannot
   see the mapped identity and answers the weaker
   `{ … }[constraint of X]`: `string | number | undefined` for
   `Partial<Foo1>[K]`, where native's is `Foo1[K] | undefined`.
2. **The union/intersection-target fallthrough is not taken for an
   indexed-access source** (§2.2(3)'s stated divergence).

Three declines, each answering `Unknown`, and each naming the native step it
stands in for:
- the type-variable arm, for a source whose object is such a resolved
  instance (`mapped_substitution_out_of_reach`);
- the intersection-source effective constraint, when a constituent is one
  (`NonNullable<Partial<Config>[T]> → Config[T]`);
- a failed union/intersection-target walk, for a source that is
  `isMappedTypeGenericIndexedAccess` (`is_mapped_type_generic_indexed_access`,
  native's predicate including its no-`-?`, no-`as` conditions):
  `Funcs[K] → Func<"a"> | Func<"b">`, which native relates as `Funcs[keyof
  ArgMap]`.

An earlier version declined every failed indexed-access union walk. It lost
`mappedTypes6`, §1's `mappedTypeRelationships` 41 and 46, `conditionalTypes1`
29 and the 6 quickinfo type lines. It was narrowed to the native predicate.

**Also changed:** the type-variable arm for an indexed-access source now
asks `getConstraintOfType` first (`constraint_of_type`, which carries the
mapped substitution for a captured mapped object), as relater.go:3667
does. Before, it asked the base constraint. The base constraint remains
where the port's constraint is undecided. A lazily captured mapped object is
captured first.

**Measured** against §2's dumps (`aea460a`), both loss checks empty:
- `errorInfoForRelatedIndexTypesNoConstraintElaboration` WRONG → RIGHT;
- `templateLiteralTypes5` WRONG → RIGHT (its 10:7 TS2322);
- `noUncheckedIndexedAccess` 98:5 TS2322 converted;
- types: RIGHT unchanged; `correlatedUnions:0:469` and `:0:475` move WRONG
  → GAP.

**Not converted:**
- `noUncheckedIndexedAccess` 39 and 85 are element-access writes
  (`strMap["baz"] = undefined`). Their target is the access's write type
  (`members.rs`/`expressions.rs`, main's), not an indexed-access relation.
- `keyofAndIndexedAccessErrors` 114: `T[K] → T[J]` fails on `K → J`, where
  `K extends Extract<keyof T, string>`. That ends `Unknown` in the
  conditional source arms (r5-relater4 §2).
- `keyofAndIndexedAccessErrors` 122 and 123: the `keyof T` constraint of `T
  extends { [K in keyof T]: string }` meets §3's mapped-parameter
  identity.

**Perf.**
- `Ir`: generic-imports 343,574,526 → 342,944,726 (−0.18%); domain-model
  1,232,346,522 → 1,197,029,508 (−2.9%). CLI output is identical. Pairs
  that used to walk on to an `Unknown` now end at the write constraint.
- Median child CPU (21 samples): 0.996 and 1.000.

Unit test: `relater.rs`
`generic_key_tests::an_indexed_access_target_relates_through_its_write_constraint`
(`string → R[K]` Related, `number → R[K]` NotRelated). On §2's commit the
first answers `Unknown`.

## 5. B16 — a homomorphic mapped source over a tuple-constrained `T`

**Forcing constraint.** Before the structural arm,
`structuredTypeRelatedToWorker` replaces the source by `getApparentType`
(relater.go:3815). For a generic homomorphic mapped type, that is
`getResolvedApparentTypeOfMappedType`. When every constituent of the
modifiers type's base constraint is an array or tuple, the mapped type is
applied to that constraint. So `{ [P in keyof T]: X }` with `T extends
[number] | [string]` has the mapped tuples `[null] | [boolean]` as its
apparent type.

The array case (relater.go:3841) then relates its number-index type to an
array target when:
- the target is readonly and every constituent is an array or tuple; or
- every constituent is a mutable tuple.

Otherwise a union apparent type is not an object. Neither the structural
nor the discriminated arm applies, and the worker ends False. The port
walked the mapped type's own member table (`length`, `toString`, …)
against the array's, and failed
(`mappedTypeUnionConstrainTupleTreatedAsArrayLike`, three extra TS2322s).

**Ported** (`generic_mapped_apparent_source_related_to`): `mapped.rs`'s
`apparent_mapped_type` supplies the apparent type. The array case uses
the port's tuple/array readers (`tuple_element_union`,
`tuple_spread_array_element`, `tuple_is_readonly`). If none of the cases
applies:
- a union apparent type is NotRelated;
- a single apparent type is related in the source's place.

The arm sits before the tuple arms, which is native's order.

**Measured** against §4's dumps, both loss checks empty:
- `mappedTypeUnionConstrainTupleTreatedAsArrayLike` WRONG → RIGHT (its 3
  extras are gone);
- types unchanged.
- `Ir`: generic-imports 342,944,726 → 342,966,309 (+0.006%); domain-model
  1,197,029,508 → 1,196,501,747 (−0.04%). CLI output is identical.
- Median child CPU (21 samples): 1.012 and 0.994.

Unit test: `relater.rs`
`generic_key_tests::a_homomorphic_mapped_source_over_tuples_meets_arrays_as_its_apparent_type`.
On §4's commit it answers NotRelated for `H<T> → any[]`.

## 6. One `isDiscriminantProperty`, cached per `(union, name)`

r5-relater5's held [`r5-relater5-discriminant-property.diff`](r5-relater5-discriminant-property.diff)
lands here (§4a there). It deletes `flow.rs`'s and `assignreport.rs`'s private
copies and adds `Checker::is_discriminant_property(t, name)` in `relater.rs`.
`flow.rs`'s `get_discriminant_property_access` (a one-line call swap)
and `assignreport.rs`'s `discriminate` caller now ask the relater's
computation. `flow.rs` had not moved, so the diff applied unchanged.

**Rebased baseline.** Before this commit the branch merged the integration
head again (`a05e5b1`, "base6"):
- `diagverdictdump`: RIGHT 5388, EMPTY_RIGHT 5585, WRONG 1203, EMPTY_WRONG 62;
- `verdictdump`: RIGHT 545046, WRONG 6579, GAP 908;
- `Ir`: generic-imports 342,964,276; domain-model 1,197,122,112.

**The cache** (checker port convention):
- **Native operation** (corrected after the first push of this section,
  which named a nonexistent `propertyCacheWithoutObjectFunctionPropertyAugment`
  and a `links.isDiscriminantProperty` field). `isDiscriminantProperty` (relater.go:1087) sets
  `CheckFlagsIsDiscriminantComputed` and `CheckFlagsIsDiscriminant` on the
  synthetic union property. `getUnionOrIntersectionProperty`
  (checker.go:21428) interns that property per union and name: this call
  passes `skipObjectFunctionPropertyAugment = false`, so it uses the union's
  `propertyCache`. The answer is therefore computed once per
  `(union, name)`. Consumers are
  `flow.go`'s `getDiscriminantPropertyAccess`, `isDiscriminantWithNeverType`
  and the relater's discriminated-target arm.
- **Identity and owner.** `Checker::discriminant_properties`: union
  `TypeId` → property name → `bool`, private to the checker, for its
  lifetime. The key is native's: one synthetic property per union type and
  name. A name lookup allocates nothing.
- **Publication.**
  - Only a completed answer is stored.
  - `None` (a member type this port could not compute, a gap) is
    recomputed on the next query, so an unsupported answer never becomes
    a completed false.
  - Inside a conditional-alias or mapped-template evaluation frame, member
    types are read through the frame, so the cache is neither read nor
    written. That is the relation store's exclusion (`Relater::new`).
  - There is no active state: the computation does not re-enter itself.
- **Consumer context.** The answer depends only on the union's constituents
  and the name, as native's does. The callers that pass a list of
  constituents rather than a union (the relater's discriminated arm,
  `assignreport.rs`) recompute, since they have no union identity to key
  on.
- **Work boundary.** The worker is `is_discriminant_property_of_types`: each
  constituent's apparent type, member type and member symbol.

**Second cost, removed.** The worker built `getUnionType(memberTypes)` only
to test genericity, and minting that union printed it
(`create_union_with_text` → `best_name` → name resolution). That was most of
the remaining `Ir` after the cache. A union's generic flags are its
constituents', and union reduction never drops a generic constituent, so the
flags are now read per member.

**Measured** against base6:
- `diagverdictdump` verdicts and lines are identical (only the timing
  columns differ). `verdictdump` verdicts are identical. No case or line
  moves.
- `Ir`, base6 → this commit: generic-imports 342,964,276 → 342,984,727
  (+0.006%); domain-model 1,197,122,112 → 1,197,483,504 (+0.030%). CLI
  output is identical.
- Along the way: the diff alone cost +0.41% on domain-model (r5-relater5);
  with the cache, +0.096%; without the union mint as well, +0.030%.
- An attempt to read member symbols only after the non-uniform and literal
  tests measured +0.15%, worse, and was dropped.
- Median child CPU (21 samples): 0.975 and 1.003.

**Falsifier.** A narrowing whose discriminant answer changes after a
member table completes (a `false` published while a constituent's
properties were still incomplete). Native's synthetic property would also
have been created at first query, so this would be a divergence in when
tables complete, not in the cache.
