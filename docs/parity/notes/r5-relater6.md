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
