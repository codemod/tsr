# r5-relater5 — generic mapped arms and decidable Unknowns (`tsr-2zk.1035`)

Lane under `tsr-2zk.1035`, continuing r5-relater4 ([`r5-relater4.md`](r5-relater4.md) §3).
Owns relation arms in `crates/tsr-checker/src/relater.rs` and `variances.rs`.
The buckets are the r5-triage2322 census's X6 (`tsr-2zk.977`) and M4
(`tsr-2zk.983`), plus `.976`'s B12 and the shared `isDiscriminantProperty`
([`r5-triage2322.md`](r5-triage2322.md) §3). Native anchors are
`vendor/typescript-go` @ `5b1047d`.

Frozen base: `a14116d`, the integration head `02a7110` merged with
r5-relater4's branch (`74b53d7`), whose §2 conditional arms this lane builds
on. `main`'s `2ae37af` conflicts in `string_mapping.rs` (not this lane's file)
and is left to the integrator. `diagverdictdump`: RIGHT 5328, EMPTY_RIGHT
5581, WRONG 1263, EMPTY_WRONG 66. `verdictdump`: RIGHT 543929, WRONG 7591,
GAP 1013.

## 1. `tsr-2zk.977` — the generic mapped relation arms

Native's worker has five places that decide a pair involving a generic mapped
type. The port had the first only in part. Each is now ported:

1. **The generic-mapped-target arm** (`generic_mapped_target_related_to`,
   relater.go:3593). Two gaps closed:
   - `!isGenericMappedType(source)` was the over-approximating
     `is_generic_mapped_target`. It is now `is_generic_mapped_type`, which
     mirrors checker.go:24908. The constraint must be a generic index type,
     or the `as` clause must be generic with the constraint substituted for
     the iteration parameter.
   - `getIndexTypeEx(source, IndexFlagsNoIndexSignatures)`. The port's
     `resolved_keyof_type` adds index-signature keys, so a source with index
     infos used to skip the arm (`None`). That made the arm look absent
     rather than failed. `keyof_without_index_signatures` removes those keys
     again: each info's key, and `number` beside a `string` key, as
     `getIndexType` adds them. A key it cannot account for leaves the keys
     unknown. `null`, `undefined` and `void` have no keys.
   Every case the arm cannot compute now answers `Unknown`. Before, it
   answered `None` ("the arm does not apply"), which item 2 would read as a
   decision.
2. **The source switch's default case** (`generic_mapped_target_default`,
   relater.go:3795-3812). Outside the subtype relations, an empty object type
   relates to a `?` mapped type. A generic mapped source relates through
   `mappedTypeRelatedTo`. Every other source is False: native returns before
   the apparent type, the structural arm and the discriminated arm. It runs
   in two places. One is the worker, after the conditional arms and before
   the structural arm. The other is the gate's lazily-captured mapped block,
   which handles pairs the structural gate never routes. Sources the switch
   sends elsewhere are excluded: type variables, `keyof`, template literals,
   string mappings, conditionals and unions.
3. **`mappedTypeRelatedTo`** (`mapped_type_related_to`, relater.go:3972):
   - the modifier gate (`getCombinedMappedTypeOptionality`);
   - the target constraint related to the source constraint;
   - the `as` clauses equal under `P := Q`;
   - the source template, instantiated with `P := Q`, related to the target
     template.
   **Stated divergence:** native instantiates the source constraint with
   `reportUnmeasurableMapper`/`reportUnreliableMapper`. Those only mark
   variance measurements, and this port has no markers (X7, `variances.rs`).
   `mapped_modifiers_reject` stays as the early negative it was. It is
   `mappedTypeRelatedTo`'s first test, reached before the arms native puts
   first. It decides only pairs whose answer is False either way.
4. **The type-parameter target arm for a mapped source**
   (`mapped_source_type_parameter_target`, relater.go:3423). `{ [P in Q]: X }`
   relates to `T` when `keyof T` relates to `Q`, there is no `?` and no `as`,
   and `X` relates to `T[P]`. Otherwise the source switch's default case
   ends False. The gate's "object → type parameter is False" arm used to
   exclude every mapped source, which left `Partial<T> → T` undecided.
5. **`indexSignaturesRelatedTo`'s generic-mapped-source arm** (relater.go:4590).
   A generic mapped source against a target with a `string` index relates its
   template to each index value. This covers `Record<K, T> → { [k: string]: T }`
   and `N2<T> → { [k: string]: number }`.

The census's "alias-reached generic mapped type is missed" (B13) was not a
registration gap. `capture_mapped_alias` already records `Record<K, T>`. The
misses were items 1 (index-signature sources) and 5.

Measured on the lane's cases against the frozen base:
- `mappedTypes6`, `indexSignatureAndMappedType` → RIGHT;
- `typeGuardOfFormTypeOfFunction` → EMPTY_RIGHT;
- lines converted in `genericMappedTypeAsClause` ×5,
  `reverseMappedTypeIntersectionConstraint` ×2 and `mappedTypeRelationships`.

**Accepted extra, recorded rather than fixed here:**
`genericMappedTypeAsClause.ts:18:11` (`const x5: MappedModel<T> = { a: 'bar',
b: 42 }`). The relation was `Unknown`. It is now native's False, so a
TS2322 head is printed where native prints TS2353 at `a`. Native's
`hasExcessProperties` fires first: `isKnownProperty` finds no `a` on a
generic mapped type. The port's excess pre-pass (`unions.rs`
`fresh_literal_has_excess_property`, reported by `assignreport.rs`) declines
on mapped targets. That is the X2 bucket (`tsr-2zk.974`), outside this lane.
The case was WRONG before and stays WRONG.

## 2. `tsr-2zk.983` — Unknown on decidable pairs

The worker's terminal `Unknown` (the `CompositeShape` site) stays. Native's
terminal is False (relater.go:3900), but every shape that reaches the port's
terminal would have to be audited arm by arm first. Instead, each shape the
census found reaching it, or reaching the gate's bottom `Unknown`, gets
native's own ending at the arm that owns it:

- **`S[K] → T[J]`** (`indexed_access_pair_after_components`, relater.go:3443,
  :3652). When the components do not relate, native tries the target's
  write constraint (assignable/comparable only, and only when neither base
  is generic). The source switch's type-variable case skips a pair of
  indexed accesses, so the worker ends False. Here only a generic **object**
  base decides False: a type variable or a generic mapped type. The port's
  base constraint of an index is not always native's (`keyof T`'s is
  `string | number | symbol` there). A concrete base stays `Unknown`, because
  the write constraint is not built (IAW).
- **primitive → `keyof U`** (the keyof-target arm, relater.go:3489). A
  primitive source has no later case that could relate it. The pair is
  decided False only for a type-parameter operand, whose constraint is
  `getConstraintOfTypeParameter` either way, and only when:
  - the operand has no constraint, or
  - the relation to its constraint's keys failed definitely.

  The gate used to compare the apparent `String` instead of `string`,
  because a written `keyof U` mint carries OBJECT flags. Deferred-keyof
  targets are now excluded from that conversion.

Three declines guard these decisions against gaps that the earlier
`Unknown` had masked. Each was forced by a measured loss:
- **A mapped iteration parameter in the index relation.** The port shares a
  mapped type's declared parameter `P` across instances, and `P` keeps its
  declared constraint. Native's `getTypeParameterFromMappedType`
  instantiates it per mapped instance. In `MyMap<U> = { [P in keyof T]:
  T[keyof T] }` the port relates `U[P]` to `U[keyof U]` through
  `P → keyof U`, which reads `keyof T → keyof U`, NotRelated.
  `mappedTypeParameterConstraint` lost its EMPTY_RIGHT that way. A failed
  index relation through such a parameter stays `Unknown`
  (`is_mapped_iteration_parameter`).
- **An indexed access of a generic mapped type.** getNormalizedType's
  getSimplifiedIndexedAccessType (checker.go:27965) substitutes
  `{ [P in K]: E }[X]` to `E[P := X]` before any relation. It adds
  `undefined` when the mapped type has `?`. Native therefore never relates
  such a pair as two indexed accesses. The substitution is not ported (the
  census's IAM bucket), so the IA→IA terminal answers `Unknown` when either
  object is, or may be, a generic mapped type. In `mappedTypes5`,
  `Readonly<Partial<T>> → Partial<T>` compares templates
  `Partial<T>[P] → T[P] | undefined`. Native's simplified source relates.
  The port's unsimplified pair decided False and lost the case's RIGHT.
- **A generic mapped return compared after `instantiateSignatureInContextOf`.**
  Native infers the source signature's type parameters through
  `inferFromObjectTypes`' mapped-to-mapped arm (inference.go:699). That
  arm relates constraint to constraint and template to template. The
  port's `instantiate_signature_in_context` (`inference.rs`, main's) has no
  such arm, so `<U>() => { [K in keyof U]: U[K] }` against `<T>() => { [K in
  keyof T]: T[K] }` erases `U` and its return becomes `{}`. With item 2 of
  §1, `{}` against the generic mapped return is native's False, and
  `higherOrderMappedIndexLookupInference` lost its EMPTY_RIGHT. In
  `one_signature_related_to`, a NotRelated return relation stays `Unknown`
  when the source was instantiated in context and both uninstantiated
  returns are generic mapped types. The decline retires with the inference
  arm.

**On the `CompositeShape` label.** The census found it misattributed. After
these two, `S[K] → T[J]` with a concrete base and primitive →
`keyof (non-parameter)` still reach the terminal. The label's measurement
comment remains wrong for them. Changing the terminal itself to False is
refused until the remaining arms above it are audited, per shape.

Not done (each needs a piece this lane does not own, or a separate audit):
- PI: held (§2a).
- `unique symbol` decidability (US) was refused by r5-relater3 at −3 cases
  (`tsr-2zk.1005`).
- IAW: the write constraint `getIndexedAccessTypeOrUndefined(.., Writing)`.
- SM: a string-mapping source against a template target
  (`isTypeMatchedByTemplateLiteralType`).
- TPC: `T extends T`.
- SIG and NS were not traced per row.

### 2a. Held diff: primitive → index-signature target (`sourceIsPrimitive`)

[`r5-relater5-primitive-index.diff`](r5-relater5-primitive-index.diff)
ports the structural arm with `sourceIsPrimitive` (relater.go:3864, :4588).
It runs properties, then signatures, then index signatures. The `any`-valued
string-index exemption is off for a primitive, and its apparent interface
infers no index (`isObjectTypeWithInferableIndex`):
- `y: { [k: string]: any } = "foo"` is False;
- `{ [k: number]: any }` is True through `String`'s number index.

Measured with §1 and the rest of §2:
- `assignmentCompat1` and `indexTypeCheck` go WRONG → RIGHT;
- `unionTypeWithIndexedLiteralType` goes EMPTY_RIGHT → EMPTY_WRONG.

In that case `const u: U = { x: "lit" }` with `U = Idx | I | "lit"` and
`interface Idx { [k: string]: U }`. Native keeps `x: "lit"` because the
contextual type of `x` over the union includes `Idx`'s index type `U`. The
port widens `x` to `string`, the X12 contextual-union-property bucket
(`contextual.rs`, main's). `string → Idx` is then native's False where
it used to be `Unknown`. The relation is right and the input is wrong, so
the diff is held until the contextual gap closes.

## 3. `.976` B12 — union-target fallthrough for type variables (held)

`unionOrIntersectionRelatedTo`'s failure falls through for an instantiable
source (relater.go:3380). r5-relater4 let a conditional source through. The
type-variable arms are now one function, `type_variable_source_related_to`:
the indexed-access constraint, synthetic `this` and the declared type
parameter. The worker calls it in its old place. This refactor is in the
commit.

[`r5-relater5-union-target-fallthrough.diff`](r5-relater5-union-target-fallthrough.diff)
calls it from the union- and intersection-target arms after a failed walk.
`E[K]` whose constraint is `A | B` then relates to `A | B`. All 3 TS2345
extras of `quickinfoTypeAtReturnPositionsInaccurate` convert, but its type
lines lose 6 RIGHT (`:0:41`, `:42`, `:44`, `:68`, `:69`, `:71`). The call
`isNumClass(entry)` now checks, so its predicate `Extract<Entries[EntryId],
NumClass<any>>` narrows `entry`. Native keeps the deferred `Extract`. The
port's relation `Entries[EntryId] → Extract<…>` is `Unknown` (a
conditional-target decline, r5-relater4 §2), and the narrowing in `flow.rs`
then falls back to the constraint union `NumClass<number> |
StrClass<string>`, where `numExclusive` is missing. Before, the failed call
left the narrowing on another road whose result happened to print right.
Held until the narrowing stops treating an undecided relation as a
decision (`flow.rs`, main's) or the conditional target decides.

## 3b. Measured (§1, §2 and the §3 refactor; the held diffs excluded)

Both dumps unfiltered against the frozen base. Both loss checks are empty.

| | base | after |
|---|---|---|
| diagnostics RIGHT / EMPTY_RIGHT | 5328 / 5581 | 5330 / 5582 |
| diagnostics WRONG / EMPTY_WRONG | 1263 / 66 | 1261 / 65 |
| types RIGHT / WRONG / GAP | 543929 / 7591 / 1013 | 543931 / 7591 / 1011 |

Cases converted:
- `mappedTypes6` and `indexSignatureAndMappedType` → RIGHT;
- `typeGuardOfFormTypeOfFunction` → EMPTY_RIGHT.

33 diagnostic lines converted:
- `keyofAndIndexedAccessErrors` 7;
- `mappedTypeRelationships` 9;
- `mappedTypes6` 5;
- `genericMappedTypeAsClause` 5;
- `indexSignatureAndMappedType` 4;
- `reverseMappedTypeIntersectionConstraint` 2;
- `typeGuardOfFormTypeOfFunction` 1.

Three new extra lines, all in cases that were already WRONG. Each is a
decision native also makes, on an input this port computes differently:
- `genericMappedTypeAsClause.ts:18:11`, the excess-property reporter (§1);
- `keyofIsLiteralContexualType.ts:4:9` and `:5:9`. Native types
  `["a", "b"]` against a `(keyof T)[]` context as a literal tuple of keys.
  That literal contextual type is the case's subject. The port widens it to
  `string[]`, and `string → keyof T` (`T extends { a: string, b: string }`)
  is now native's False through the constraint's keys, where it was
  `Unknown`. The contextual typing belongs to `contextual.rs` (main's).

Perf against the base binary, by median child CPU (new/old):
- 21 samples: domain-model 0.973, generic-imports 0.951;
- 41 samples: domain-model 1.034, then 0.971 on a re-run;
  generic-imports 1.006.

In the 1.034 run the base binary's own median moved by 3%. Callgrind `Ir`
(`--singleThreaded --pretty false`, which repeats exactly):
- domain-model 1,260,420,138 → 1,259,250,972 (−0.09%);
- generic-imports 399,718,803 → 399,716,152.

The new arms run only after earlier arms have failed. The default arm ends
pairs that used to walk on to the gate's bottom. Workspace tests pass
(`cargo test --workspace --release`), including
`tests/generic_mapped_relations.rs`, which pins §1-§2's answers.

## 4. One `isDiscriminantProperty`

`Checker::is_discriminant_property_of_types` (`relater.rs`, `pub(crate)`) is
now the relater's computation. The method moved off `Relater` unchanged, and
the relater's own call uses it. `flow.rs` (`is_discriminant_property`, main's
file) and `assignreport.rs` (`is_discriminant_property_of_union`) keep their
private copies until the held diff below lands. The shared function is not
named `is_discriminant_property` yet: flow.rs's private method holds that
name on `Checker`.
