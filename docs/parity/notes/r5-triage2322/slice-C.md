# Slice C: TS2322 MISS rows where the assignability report was attempted (169 rows, 161 DECLINED, 8 REPORTED)

Method. The `reasons` site mask is only reachable through `undecidable_split` (calls), and building was off-limits, so every
DECLINED row was attributed by **reading the path** `is_related_to_with_flags` (relater.rs:1118) -> `recursive_type_related_to`
(2708) -> `structured_type_related_to_worker` (2885) for the TSR types of both sides. TSR types were checked with `probefile` on
the real case files or reductions (in /tmp/box/an/repro/C/), and TSR's emitted diagnostics with `diagcase`. Native behaviour
comes from the pinned relater.go/checker.go and the reference baselines. Line numbers in relater.rs are at the working-tree
commit.

The TSR files contain three general Unknown sites. Most buckets end at one of them:
* **S-BOTTOM** `relater.rs:1556-1567`: the end of `is_related_to_with_flags`. A side that is not `flag_decidable` gets
  `Unknown` (NoMembersTable/UnportedFlag). `CONDITIONAL`, `STRING_MAPPING`, `UNIQUE_ES_SYMBOL`, type-parameter targets with a
  mapped source, and generic mapped targets all land here.
* **S-TERM** `relater.rs:3519-3522`: the end of `structured_type_related_to_worker`. It is labelled `CompositeShape`
  ("flags say union/intersection while data says otherwise"), **but it is also reached by indexed-access->indexed-access and
  primitive->`keyof` pairs**. So the `CompositeShape` histogram row is mislabelled. Native's worker ends `return TernaryFalse`
  (relater.go:3900).
* **S-TUPLE** `relater.rs:3566-3571`: `tuples_related_to` answers `Unknown` when any element is a generic spread.

## 1. Table

| bucket | root cause (one line) | native anchor | TSR function | #lines | #cases | owning file |
|---|---|---|---|---|---|---|
| Q | A namespace-rooted qualified type reference (`First.E`, `Foo.Yep`) is minted as an OBJECT `Named` image instead of the enum/alias declared type. The relater then walks a fake object. Same-named enums also need `isEnumTypeRelatedTo` | relater.go:236 `isSimpleTypeRelatedTo` enum arms + relater.go:282 `isEnumTypeRelatedTo` | declared.rs:5384 `qualified_type_reference` (mint); relater.rs:2558 `is_simple_type_related_to` (same-name enum -> `None`) | 21 | 4 | declared.rs (then relater.rs) |
| CT | Conditional **target** has no arm (skipTrue/skipFalse, relate to both branches) | relater.go:3540 `structuredTypeRelatedToWorker` target-Conditional | relater.rs:1567 S-BOTTOM (primitive/TP sources); object sources reach the structural arm 3474 over the conditional's alias member image | 14 | 6 | relater.rs |
| CS | Conditional **source** has no arm (default constraint, then distributive constraint) | relater.go:3721-3770 source-Conditional | relater.rs:1567 S-BOTTOM (`IsArray<T>` is `Named{CONDITIONAL}`, not flag-decidable) | 7 | 4 | relater.rs |
| CC | Conditional->conditional (same root via alias/interface variance, or different roots) is never decided | relater.go:3727 (cond->cond) + relater.go:3392 alias variance `getAliasVariances` | relater.rs:2885 worker has no conditional arm; site not pinned (S-BOTTOM or structural arm over member image) | 8 | 5 | relater.rs |
| IA | `S[K]->T[J]` whose object/index comparison fails falls off the worker as Unknown. Native returns False after the target-IA write-constraint arm | relater.go:3443-3488 then 3900 `return TernaryFalse` | relater.rs:2889-2904 IA pair arm, then S-TERM 3519-3522 | 12 | 4 | relater.rs |
| TERM | `string`/`number`/`symbol` -> `keyof U` (unconstrained U) falls off the worker. Native target-Index arm then False | relater.go:3489-3539 + 3900 | relater.rs:3052-3083 keyof-target arm, then S-TERM 3522 | 4 | 1 | relater.rs |
| IAM | Indexed access of a mapped type is not simplified (`substituteIndexedMappedType`) before relating | checker.go:27930 `getSimplifiedIndexedAccessTypeWorker` via relater.go:2620 `getNormalizedType`; relater.go:3683 `isMappedTypeGenericIndexedAccess` | relater.rs:1118 `is_related_to_with_flags` (no simplification) | 3 | 2 | relater.rs (+ indexed.rs) |
| IAW | Concrete object with a generic key target `obj[Key]`: the write constraint `getIndexedAccessTypeOrUndefined(base, baseKey, Writing)` is not built | relater.go:3459-3480 | relater.rs:1496-1501 (returns `Unknown`) | 1 | 1 | relater.rs |
| M1 | A generic mapped **target** never reaches native's default "generic mapped target => False unless the source is also generic mapped" | relater.go:3805-3812 (default arm) after 3593 | relater.rs:1350-1364, strict-null arm 1517-1525 + S-BOTTOM 1567 (and 1629 when keys relate Unknown) | 12 | 5 | relater.rs |
| M2 | Mapped **source** -> type-parameter target: TSR explicitly excludes mapped sources from the "object -> T is false" arm | relater.go:3423-3433 target-TypeParameter arm, then default structural (False) | relater.rs:1431-1440 (`!mapped_types.contains_key(source)`) then S-BOTTOM 1567 | 9 | 3 | relater.rs |
| M3 | Generic mapped -> generic mapped: only the modifier gate is ported, not `mappedTypeRelatedTo` (constraint T->S, template S->T) | relater.go:3972 `mappedTypeRelatedTo` (from 3805) | relater.rs:1721 `mapped_modifiers_reject` only; `generic_mapped_target_related_to` returns None for a generic-mapped source (1610) -> S-BOTTOM | 10 | 4 | relater.rs |
| VT | Tuples with generic variadic elements, and the single-element `[...T]` rule | relater.go:4178 (variadic in `propertiesRelatedTo`), relater.go:3410-3421 `isSingleElementGenericTupleType` | relater.rs:3528 `tuples_related_to` S-TUPLE 3566-3571 | 11 | 3 | relater.rs (tuples) |
| SM | `Capitalize<string>`-style string-mapping source against a template-literal target | relater.go:3572 (template target) + 3782 (string-mapping source -> constraint) | relater.rs:1343-1347 gate omits a STRING_MAPPING source -> S-BOTTOM | 6 | 1 | relater.rs |
| PI | Primitive source -> index-signature-only target. The apparent type has no applicable index info and is not inferable-index, so the answer is False | relater.go:4603 `typeRelatedToIndexInfo` (+4630 `isObjectTypeWithInferableIndex`) | relater.rs:1259-1279 (returns `Unknown` unless properties fail) | 4 | 3 | relater.rs |
| SIG | Call/construct signature relation is undecided (inherited signatures, generic signatures, method params) | relater.go:4441 `signaturesRelatedTo` / 4543 `signatureRelatedTo` | relater.rs:1907 `related_signatures` / 3487-3491 (SignatureBearing). **Not traced per row** | 12 | 5 | relater.rs |
| NS | Value-module/namespace object types (`typeof import(...)`, `typeof N`, a function+namespace) have no member table | relater.go:3864 structural `propertiesRelatedTo` | relater.rs:1567 S-BOTTOM (NoMembersTable). **Pattern, not traced** | 4 | 4 | members.rs / relater.rs |
| US | `unique symbol` is not in FLAG_DECIDABLE, so a non-firing simple relation reads as Unknown | relater.go:206 `isSimpleTypeRelatedTo` (no arm, so false; not structured) | relater.rs:211 `FLAG_DECIDABLE` -> S-BOTTOM 1567 | 2 | 2 | relater.rs |
| TPC | Circular constraint `T extends T`: native treats `getConstraintOfType` = nil as `unknown`. TSR answers Unknown | relater.go:3653-3660 source TypeVariable arm | relater.rs:3178-3206 (constraint None / cycle -> `Unknown`) | 1 | 1 | relater.rs |
| REP | Reported, wrong head code: TSR emits TS2741/2739/2740 where native keeps a TS2322 head because the elaboration names a different type pair (base owner `A`, `Control`, `C` for `this`, `NumberTo<number>`, `ReactElement<any>`) | relater.go:4805-4830 `reportRelationError` `chainArgsMatch` suppression | relater.rs:797 `unmatched_property_report` / assignreport.rs:1725 `report_assignability_failure` | 7 | 6 | assignreport.rs |
| EPC | Reported as TS2353 (excess property) where native reports TS2322 (normalized object-literal union, property `b` incompatible) | relater.go:2714 `hasExcessProperties` (+ object-literal normalization) | assignreport.rs:1790 union-literal arm | 1 | 1 | assignreport.rs |
| U | Unclassified (see section 3) | - | - | 20 | 18 | - |

Total 169 lines.

## 2. Per bucket

### Q: qualified-name mint (21 lines, 4 cases)
Cases: enumAssignmentCompat3 68,70,71,72,75,76,77,78,82,83,86,87; enumAssignmentCompat6 a.ts 36,37,41,42,51,52;
enumLiteralAssignableToEnumInsideUnion 24,28; namespaceDisambiguationInUnion 10.

Evidence. declared.rs:5317-5395: an argument-less qualified reference whose namespace is **not** an alias returns
`new_named(TypeFlags::OBJECT, text, Some(resolved))`. The comment at 5359-5367 says the enum routing to the declared type was
scoped to ALIAS-rooted names only, and that "a mint is an OBJECT". `relater.rs:1795 is_qualified_alias_mint` documents the
same for aliases. Both sides of `abc = nope` are therefore OBJECT mints with `members: Some(enum symbol)`, so they enter the
structural walk (`has_members` is true for `Named{members:Some}`), which cannot answer. This applies even to `abc = nope`
(`First.E`->`Abc.Nope`), whose different names would otherwise be decided `Some(false)` at relater.rs:2562. Once routed to real enums,
the same-name pairs (`First.E`->`Abc.E`, the `DiagnosticCategory` pairs) still hit relater.rs:2557-2562 (`same_name` -> `None`,
"isEnumTypeRelatedTo member walk ... remain undecidable", relater.rs:209).
```ts
namespace First { export enum E { a, b, c } }
namespace Abc { export enum E { a, b, c } export enum Nope { a, b, c } }
declare var abc: First.E; declare var nope: Abc.Nope;
abc = nope;   // native TS2322 'Nope' -> 'E'; TSR: nothing (DECLINED)
```
```ts
namespace Foo { export type Yep = { type: "foo.yep" } }
namespace Bar { export type Yep = { type: "bar.yep" } }
const x = { type: "wat" };
const val1: Foo.Yep | Bar.Yep = x;  // native TS2322; TSR: nothing (target constituents are OBJECT mints)
```
The exact Unknown line inside the structural walk over a mint was not pinned (most likely `properties_related_to` at
relater.rs:3928-3934 or 4066). The cause (the mint) is certain.

### CT: conditional target (14 lines, 6 cases)
Cases: conditionalTypeAssignabilityWhenDeferred 39,46,63,106,116; deepComparisons 2,3,4; recursiveConditionalTypes 21,169;
conditionalTypes1 160; conditionalTypesExcessProperties 8,9; complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound 33
(target property is `ChannelOfType<T,...>["type"]`, an indexed access of a conditional; classified by chain).

Evidence. A deferred conditional alias instance is `new_named(TypeFlags::CONDITIONAL, .., Some(member_symbol))`
(declared.rs:6283-6307). `grep TypeFlags::CONDITIONAL relater.rs` finds no arm. CONDITIONAL is not in FLAG_DECIDABLE
(relater.rs:211-228). So `string -> Foo<T>` passes no gate (s is not TP/IA and target has no members-and-source-members pair)
and ends at S-BOTTOM 1567. Native relater.go:3540-3570 computes skipTrue/skipFalse and relates the source to both branches.
```ts
// @strict: true
type Foo<T> = T extends true ? string : "a";
function test<T>(x: Foo<T>, s: string) { x = s; }   // native TS2322; TSR: nothing
```
```ts
function f<T>(x: T) { let v1: Extract<T, string> = x; }  // native TS2322 (deepComparisons); TSR: nothing
```
For object sources (`{x:T;y:T}` -> `T extends T ? {...} : never`), both sides have members, so the pair goes through the
structural arm (relater.rs:3474) against the conditional's branch-literal member image (declared.rs:6276
`conditional_alias_branch_literal_symbol`). This is a second divergence: the comment at relater.rs:3469 says such an image "is not
an object", but the check there is `has_members`, not OBJECT.

### CS: conditional source (7 lines, 4 cases)
Cases: distributiveConditionalTypeConstraints 10,15,19,38; recursiveConditionalTypes 22; conditionalTypes1 159;
keyofAndIndexedAccessErrors 103 (`Extract<keyof T,string>` -> `K`).

Evidence. probefile on the case prints `x : IsArray<T>` / `x : Foo<T>`, so the conditional is deferred. `IsArray<T> -> false`: the
simple relation gives None, `has_members(false)` is false, s is not TP/IA, and the pair falls to S-BOTTOM 1567. For
`Extract<keyof T,string> -> K`, the TP-target arm at 1431 requires `s ⊂ OBJECT|PRIMITIVE|UNKNOWN`, which CONDITIONAL is not.
Native relater.go:3721: default constraint (`true|false`) -> fail; distributive constraint -> fail -> False.
```ts
// @strict: true
type IsArray<T> = T extends unknown[] ? true : false;
function f2<T extends unknown[]>(x: IsArray<T>) { let f: false = x; }  // native TS2322; TSR: nothing (diagcase confirms the miss)
```
```ts
type Foo<T> = T extends "abc" | 42 ? true : false;
function f20<T extends string>(x: Foo<T>) { let t: false = x; }  // native TS2322 'boolean'->'false'
```

### CC: conditional -> conditional (8 lines, 5 cases)
Cases: conditionalTypes2 15,19,24,25 (interfaces whose member is a conditional; variance measurement or property relation
compares `A extends string ? ...` with `B extends string ? ...`); conditionalTypes1 288 (`T95<U>`->`T94<U>`);
recursiveConditionalTypes 50 (`TupleOf<number,M>`->`TupleOf<number,N>`); flatArrayNoExcessiveStackDepth 20
(`FlatArray<Arr,any>`->`FlatArray<Arr,D>`); exactOptionalPropertyTypesIdentical 2 (generic signature returns
`T extends {a?:string}?0:1` vs `...|undefined`). Native: relater.go:3727-3750 (identical extends + related check types +
branches), and for same-alias pairs relater.go:3392 (`getAliasVariances`). Classified by pattern from the same "no conditional
arm" fact as CT/CS. The exact TSR Unknown site differs per row and is unverified.

### IA: indexed-access pair falls off the worker (12 lines, 4 cases)
Cases: keyofAndIndexedAccessErrors 108,111,114,117; mappedTypeRelationships 11,16,41,46,61,66;
errorInfoForRelatedIndexTypesNoConstraintElaboration 6; templateLiteralTypes5 10 (signature return
`TypeMap[T2]`->`TypeMap[\`${T2}\`]`, by chain).

Evidence (traced for `T[keyof T] -> U[keyof T]`, f3, and `T[K] -> T[J]`). probefile confirms that TSR types match the native .types
(`y[k] : U[keyof T]`). The gate (s is INDEXED_ACCESS) leads to the worker. The IA-pair arm (2889-2904) computes objects
`T->U`: T is unconstrained, so it relates `unknown -> U`, which is NotRelated (1431). The arm is therefore skipped.
None of the later arms apply. The source-IA constraint arm needs `!target IA` (3144), and an IA mint is
`Named{members:None}` (indexed.rs:747), so `has_members` is false. The pair reaches S-TERM 3522 = Unknown. Native: the target-IA
arm finds the base object generic, so it is skipped. The source TypeVariable arm is skipped when both sides are IA, so the result
is `return TernaryFalse` (3900).
```ts
function f3<T, U extends T>(x: T, y: U, k: keyof T) { y[k] = x[k]; }  // native TS2322; TSR: nothing
```
```ts
function f<T, K extends keyof T, J extends keyof T>(tk: T[K], tj: T[J]) { tj = tk; }  // native TS2322; TSR: nothing
```

### TERM: primitive -> `keyof U` (4 lines, 1 case)
keyofAndIndexedAccessErrors 82,83,86,87 (`keyof T | keyof U` -> `keyof T & keyof U`). Traced: for the constituent `keyof T -> keyof U`,
the direct relation `U->T` is NotRelated and U has no constraint. The source `keyof T` then becomes `string|number|symbol`.
`string -> keyof U` enters recursion through `deferred_keyof_operands.contains_key(target)` (1341). In the worker, no arm fires
(the keyof arm 3052-3083 does nothing without a constraint) and the pair reaches S-TERM. Native relater.go:3489 does nothing either, then False.

### IAM: indexed access of a mapped type not simplified (3 lines, 2 cases)
conditionalTypes1 115,117 (`NonFunctionPropertyNames<T>` = `{[K in keyof T]: ...}[keyof T]`; the native chain shows the
substituted conditional `T[keyof T] extends Function ? keyof T : never`); mappedTypeConstraints2 42 (the native chain
"Mapped6<K>[`_${string}`]" comes from relater.go:3683). TSR has no `getSimplifiedType` (grep finds only
constraints.rs:385 `simplified_type_or_constraint`).

### IAW: write constraint not built (1 line)
noUncheckedIndexedAccess 98 (`myRecord2[key] = undefined`, `Key extends string|number`). Traced: relater.rs:1471-1501 shows
base object concrete, base index `string|number` (not generic), `constrained` false, so the arm returns `Unknown` at 1500. Native
builds `getIndexedAccessTypeOrUndefined(obj, string|number, Writing)` = `string`, and `undefined -> string` is False.

### M1: generic mapped target default (12 lines, 5 cases)
Cases: genericMappedTypeAsClause 14,15,16,17,19; mappedTypeAsClauseRelationships 12,22; mappedTypes6 56,57;
indexSignatureAndMappedType 6,16; inferenceOuterResultNotIncorrectlyInstantiatedWithInnerResult 35 (target `Omit<T,"x">`;
the keys `Exclude<keyof T,"x">` are conditional, so the keys relation is Unknown and 1629 returns Unknown).
Evidence. `{} -> Required<T>`: `generic_mapped_target_related_to` returns None for a `-?` target (1597). Then
`mapped_modifiers_reject` is false. The strict-null arm excludes generic mapped targets (1517-1525), and the comment there says
"this port leaves undecided". The pair ends at S-BOTTOM. For `{[key:string]:T} -> Record<K,T>`, the arm returns None because the
source has index infos (1610). Native relater.go:3805: `isGenericMappedType(target)` and source not generic mapped => False.
```ts
// @strict: true
function f<T>(x: Required<T>) { x = {}; }  // native TS2322; TSR: nothing
```
```ts
type MappedModel<S extends string> = { [K in "a"|"b" as `${K}${S}`]: string };
function f1<T extends string>() { const x1: MappedModel<T> = 42; }  // native TS2322; TSR: nothing (diagcase: only line 11 reported)
```

### M2: mapped source -> type parameter (9 lines, 3 cases)
Cases: mappedTypes6 27,47; mappedTypeRelationships 30,35,40,45,72; reverseMappedTypeIntersectionConstraint 59,69.
Traced: `Partial<T> -> T`. The gate is not entered (`has_members(T)` is false). Line 1431 skips because
`mapped_types.contains_key(source)`. The pair ends at S-BOTTOM. Rows 30-45 are `Partial<T>[keyof T] -> T[keyof T]`: the
IA-pair arm's objects `Partial<T> -> T` are Unknown, so `all(Unknown, Related)` is Unknown and returned at 2902. Native: the
target-TP arm (relater.go:3423) skips an optional mapped source, and the structural default gives False. Native also normalizes
`Partial<T>[keyof T]` to `T[keyof T] | undefined` (IAM), which is why its message differs.
```ts
// @strict: true
function f<T>(y: T, z: Partial<T>) { y = z; }  // native TS2322; TSR: nothing
```

### M3: mapped -> mapped (10 lines, 4 cases)
Cases: mappedTypeRelationships 143,148,153,158,163,168; mappedTypeInferenceFromApparentType 10; conditionalTypes1 106,108
(`Pick<T,X>`->`Pick<T,Y>`); mappedTypes6 37 (`Required<T>`->`Denullified<T>`). relater.rs:1706-1720 says "The rest of
`mappedTypeRelatedTo` ... is not ported, so only this definite negative is taken". `generic_mapped_target_related_to`
returns None for a generic mapped source (1610).

### VT: variadic tuples (11 lines, 3 cases)
Cases: variadicTuples1 149,151,152,181,182,191; variadicTuples2 76,77; variadicTuples3 5,10,15.
Traced: `any[] -> [...T, ...P]` is a `tuple_array_pair`, so the pair goes through the worker to `tuples_related_to`. The target is
generic, and the generic spread element makes it return `Some(Unknown)` at 3570. `[...T] -> [...U]` is the same. Native
relater.go:4178 reports "Source provides no match for variadic element" (False). For `[...T]`, native relater.go:3410-3421
relates `T` to `U`.
```ts
// @strict: true
function test1<T extends any[], P extends any[]>(): [...T, ...P] { let x: any[] = []; return x; }  // native TS2322; TSR: nothing (diagcase)
```

### SM: string mapping -> template (6 lines, 1 case)
stringMappingOverPatternLiterals 129,130,131,147,148,149. The gate at relater.rs:1343-1347 admits a template target only for a
`STRING_LITERAL|TEMPLATE_LITERAL|STRING` source, so a `STRING_MAPPING` source skips it. Line 1528 needs a decidable target.
The pair ends at S-BOTTOM. Native relater.go:3782: `Capitalize<string>` -> constraint `string` -> `` `A${string}` `` is False.

### PI: primitive -> index-signature target (4 lines, 3 cases)
assignmentCompat1 8,10; unknownType1 129; indexSignatures1 289. relater.rs:1259-1279 returns `Unknown` unless
`properties_related_to` is NotRelated. Native relater.go:4603: `Boolean`/`Number` has no applicable index info and is not
`isObjectTypeWithInferableIndex`, so the answer is False.
```ts
var y: { [index: string]: any }; y = "foo";   // native TS2322; TSR: nothing (diagcase)
```

### SIG (12 lines, 5 cases), NS (4), US (2), TPC (1)
* SIG: intTypeCheck 168,169,171,173,182,183,187 (`i6 extends i2` inherits `(): number`); errorsWithInvokablesInUnions01 14,16;
  lastPropertyInLiteralWins 8; derivedClassTransitivity4 18; assignmentCompatWithGenericCallSignatures4 12. Classified by message
  ("provides no match for the signature", parameter incompatibility). **The TSR site is not traced.** For intTypeCheck,
  `declares_call_or_construct(i6)` reads only own members (relater.rs:2349), so the inherited signature is suspect.
* NS: aliasAssignments_1 3; everyTypeWithAnnotationAndInvalidInitializer 50; importCallExpressionCheckReturntype1 4;
  assignmentToObjectAndFunction 29. By pattern: a namespace's exports table is not a relater member table (types.rs comment
  on `Anonymous`, "a namespace's exports live in the symbol's exports table and nothing reads it").
* US: symbolType2 2, uniqueSymbolsErrors 87. relater.rs:202-206 documents UNIQUE_ES_SYMBOL as absent from FLAG_DECIDABLE.
* TPC: typeParameterHasSelfAsConstraint 2 (`T extends T`; return `x: T` as `number`).

### REP (7 lines, 6 cases) and EPC (1)
REP: classImplementsClass4 16, inheritance1 40,46, fuzzy 21, privateNamesUnique-4 6, checkJsxChildrenProperty5 27,
objectTypeWithStringAndNumberIndexSignatureToAny 91. diagcase shows TSR emits TS2741/TS2739/TS2740 at the same position. Native
keeps the `TS2322` head because the missing-property elaboration's type names differ from the top pair (`'C'` vs `'A'` in
classImplementsClass4), so `chainArgsMatch` (relater.go:4811-4824) does not drop the head.
EPC: objectLiteralNormalization 17. TSR reports TS2353 at 17,18. Native reports TS2322 at 17,1 ("Types of property 'b' are incompatible").

## 3. Unclassified (20 lines, 18 cases), with best hypotheses
* identicalTypesNoDifferByCheckOrder 32,36: the target intersection includes `Required<Pick<..>> & Omit<..>`. Hypothesis: the member image
  of `Omit` (`Pick` over the `Exclude` conditional) is undecided (CT/M family).
* inferFromNestedSameShapeTuple 42: recursive tuple aliases `T1<U>`->`T2<U>`. Hypothesis: `tuple_relation_elements` is
  unavailable for the recursive alias, or the depth cap is hit.
* inferTypePredicates 133: `object -> Date`. Hypothesis: `relation_property_table(Date)` is None (multi-declaration lib interface), so the
  `NON_PRIMITIVE` arm (1314-1324) skips and the pair reaches S-BOTTOM.
* intersectionPropertyCheck 7: `T & {a:boolean}` -> `{a?: string|undefined}`. Hypothesis: the retry through
  `effective_constraint_of_intersection` in `structured_type_related_to` (2815-2829) returns Unknown, and `any([NotRelated, Unknown])` is Unknown.
* invariantGenericErrorElaboration 3: recursive generic interfaces. Hypothesis: variance under measurement returns Unknown (3392-3394).
* objectFreezeLiteralsDontWiden 7: probefile shows TSR infers `Readonly<{chromium:string; firefox:string}>`, while native keeps literals.
  This is an **inference/widening** difference in the `Object.freeze` overload, not a relater arm. The DECLINED gate is unexplained.
* requiredMappedTypeModifierTrumpsVariance 18,19: `Foo<T>{a: Required<T>}`. Native variance is Unmeasurable and falls back to a structural check.
  TSR does not represent Unmeasurable (relater.rs:3396 comment).
* strictOptionalProperties1 60: `t[0] = undefined` on an optional tuple element under exactOptionalPropertyTypes. Hypothesis: write type
  of the tuple element / missing type.
* typeAssignabilityErrorMessage 40: `Foo<string>` -> `Bar<number>` (alias = object literal | boolean). Unknown site not found.
* for-of47 4: destructuring default `y = E.x` in for-of. probefile shows the pattern type `{ x: string; y?: E }`. The target is likely
  the enum-typed pattern.
* intersectionWithIndexSignatures 17,35: an intersection source against a string index target. Hypothesis: `related_index_signatures` does not do
  `membersRelatedToIndexInfo` over intersection properties (relater.go:4634).
* noUncheckedIndexedAccess 39: `strMap.qua = undefined`. probefile shows TSR's target as the read type `boolean | undefined`.
  Native uses the write type `boolean`. This is a property-access write type issue, and the DECLINED gate is unexplained.
* objectTypeWithStringAndNumberIndexSignatureToAny 88: `Obj` -> numeric index target ("Index signature for type 'number' is
  missing"). Hypothesis: `related_index_signatures` is Unknown for an interface source without an inferable index.
* privateNamesUnique-5 12: `#foo` from a different class. Hypothesis: `get_property_names_of_type` declines private names
  (members.rs, `declaration_names_a_private`), so UnfollowableBase at relater.rs:3932.
* restElementWithAssignmentPattern2 2: probefile gives `[...{0: a = "", b}] : unknown[]`. The destructuring rest-pattern type is wrong.
* varianceAnnotations 116: merged `interface Baz<out T>` + `interface Baz<in T>`. Hypothesis: the variance annotation merge.
* undefinedAssignableToGenericMappedIntersection 5: `undefined -> Errors<T>[keyof T]`. The IA-target arm at 1471-1503 would answer
  NotRelated for a generic index, so the Unknown comes from elsewhere (possibly the write target is not this IA mint). Not resolved.
