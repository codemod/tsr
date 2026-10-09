# r6-typesroots3 — mapped members, `Object.freeze` literals, the held conditional pair, import-type prints, constraint reuse, by-text renames

Round-6 parity box on epic `tsr-2zk`, the third pass after r6-typesroots
(`r6-typesroots.md`) and r6-typesroots2 (`r6-typesroots2.md`), whose method
it follows: re-measure, classify against native, port, and ship every hook in
another lane's file as a measured diff under
`docs/parity/notes/r6-typesroots3-*.diff`. Items: `tsr-2zk.1151`, `.16.69`,
`.16.74`. The lane owns r6-typesroots' and r6-typesroots2's files, the files
it creates, and this note. Native source is `vendor/typescript-go` @
`5b1047d`.

## 0. Baseline and setup

Frozen base: `f334de9` (tip of `claude/beautiful-shannon-ar5gh0` at
dispatch). Batch BU (r6-typesroots2) had not landed there, so the brief's
rule applies: measure on the current tip, merge BU when it lands.

- types 550,341 RIGHT / 5,205 WRONG / 757 GAP of 556,303;
- diagnostics: 12,238 cases;
- Ir (`valgrind --tool=callgrind`, release `tsr`, `-p <project> --noEmit
  --singleThreaded true --pretty false`): domain-model 1,090,863,706;
  generic-imports 343,115,857.

Setup as r5-operators3 §4: PyPI is blocked, so `assemble.py`'s three
`tomlkit` calls (`parse`, `inline_table`, `dumps`) ran against a stdlib-only
stand-in kept in the session scratchpad, outside the repo. The native probe
oracle is `scripts/offline-cargo/build-tsgo.sh`'s `tsgo` (Go 1.26.8,
`Version 7.1.0-dev`). Native types were read from its diagnostics (assign the
probed expression to `never`).

## 1. Items, largest first

| # | Item | Lines on `f334de9` | Section |
|---|---|---|---|
| 1 | mappedTypeIndexedAccessConstraint | 48 | §2 (four diffs) |
| 2 | objectFreeze* literal retention | 40 | §3 |
| 3 | held conditional-node consumers + INFERENCE-REVERSE-MAPPED-INTERSECTION | +35 held | §4 |
| 4 | the import-type print (importTypeGenericTypes/Local) | 8 | §5 |
| 5 | `.16.69` constraint reuse (typeParameterConstraints1, declFileRest…) | 2 | §6 |
| 6 | `typeParameterToName`'s by-text half | — | §7 |

Routed, not touched: the symbol-chain naming (`.16.6`, `.16.76`,
`typeof import("react").React`, `Test` against `import("./Test.js").default`)
is `tsr-2zk.39`, main's, assigned to a human; umd8 is r6-specifiers2's.

## 2. mappedTypeIndexedAccessConstraint: four roots, none of them an alias print alone

The brief called the case "44 lines of alias print". Re-measured on
`f334de9` it is 48 failing lines, and only 8 of them differ by an alias name
(`SetOptional<Mappings, "foo">` where native writes `PartMappings`). The rest
are three other producers. All four live in `mapped.rs` / `declared.rs`
(r6-declared2's lane), so each ships as a diff.

### 2.1 A mapping over an alias reference enumerates nothing (`…-mapped-alias-reference-keys.diff`)

**Forcing constraint.** `getTypeAliasInstantiation` (`checker.go:23641`)
instantiates the alias's DECLARED type. For `type Omit<T, K> = Pick<T,
Exclude<keyof T, K>>` that declared type is `Pick`'s mapped type, so
`Omit<M, "a">` IS a mapped type with `Pick`'s parts.
`resolveMappedTypeMembers` (`checker.go:20894`) over `{ [K in keyof
Omit<M, "a">]: … }` reads `getPropertiesOfType(modifiersType)`
(`forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType`, `:22727`).
`capture_mapped_alias` captured parts only for an alias whose body is a
mapped type node, so `Omit<…>`, minted as a print-named type keyed to `Omit`,
had no parts, no member image, and enumerated no keys: `mapper` printed `{}`
and its three arrow functions were all `any`. The same held for an
intersection holding one (`PartMappings`, `Omit<…> & Partial<Pick<…>>`):
`composite_modifiers_property_names` kept a name only when
`get_property_of_type(intersection, name)` found it, and the constituents'
member images carry no binder symbols, so it found none.

**Port.**

- `capture_mapped_alias_through_reference`: a body that is a reference to
  another generic alias evaluates the inner reference's written arguments
  under the outer alias's binding frame and captures the inner alias onto the
  same `TypeId`. The capture gives up its node key afterwards, because it is
  `Omit`'s own instantiation of `Pick`'s declared type, a different type from
  `Pick<…>`'s: left in place, `Pick<…>` evaluated afterwards answered the
  outer type, and `evaluate_alias_body` projected each into the other forever
  (`GenericStructure<K> = Record<K, number>`, awaitedType: a stack overflow
  on the first build).
- An intersection modifiers type lists every constituent's names
  (`getPropertiesOfUnionOrIntersectionType`, `checker.go:18861`;
  `createUnionOrIntersectionProperty` never drops an intersection's name). A
  constituent without a table of its own (`Partial<Pick<M, "a">>`, minted with
  `Pick`'s alias symbol as its member owner) is projected to its evaluated
  body first, the projection the spread and binding readers already use
  (`binding_type_alias_body`).
- Each member's modifiers are the intersection property's
  (`intersection_property_modifiers`): optional only when every constituent
  declaring it is optional, readonly only when every one is readonly, linked
  to the first declaration. The key's literal type is the first declaring
  constituent's (`modifiers_property_literal_type`), so `42` stays a number
  literal (`MapperArgs<42>`, not `MapperArgs<"42">`).

No new table: every projection is per query.

**Measured** (alone on `f334de9`): types **+27 WRONG→RIGHT**, zero losses on
both dumps, diagnostics unchanged. mappedTypeIndexedAccessConstraint 23,
reverseMappedPartiallyInferableTypes 4.

### 2.2 A mapped symbol's printed name and member order (`…-mapped-member-names.diff`)

**Forcing constraint.** A mapped symbol's `nameType` is its key
(`resolveMappedTypeMembers`, `:20940`), so the builder names it through
`getPropertyNameNodeForSymbolFromNameType` (`nodebuilderimpl.go:2455`): a
non-identifier name is a string literal when the symbol is string-named
(every linked declaration's name is a string literal, `isStringNamed`,
`:2405`) or the name is not numeric; single quotes survive when every
declaration was single-quoted. And `setStructuredTypeMembers` sorts the table
(`getNamedMembers`, `checker.go:22049` → `compareSymbols`,
`utilities.go:366`): linked members by declaration position, the rest by
name. `resolve_mapped_type_members_worker` copied a captured image's printed
name or the bare name, and kept key order. Probed with the pinned `tsgo`:

| Written | TSR before | Native |
|---|---|---|
| `{ [K in keyof { "12": 1, 'x-y': 2, 3: 3 }]: 0 }` | `{ 12: 0; x-y: 0; 3: 0; }` | `{ "12": 0; 'x-y': 0; 3: 0; }` |
| `{ [K in "b" \| "a" \| "1x" \| 2]: 0 }` | `{ 1x: 0; a: 0; b: 0; 2: 0; }` | `{ "1x": 0; 2: 0; a: 0; b: 0; }` |
| `{ [K in keyof (Pick<M2, "bar"> & Pick<M2, "foo">)]: K }` | `{}` (2.1's root) | `{ foo: "foo"; bar: "bar"; }` |

`x-y: 0` and `1x: 0` are not even valid types. **Port:** `mapped_property_printed_name`
is that function over the linked declarations; the worker sorts by
`(no declaration, first declaration's position, name, creation order)` and
re-indexes each slot after the sort (the slot index is the member's position,
ADR-0050).

**Measured** (on top of 2.1): types **+3** (declarationQuotedMembers), zero
losses. The case's own `"12": (o: MapperArgs<"12">) => number` needed it.

### 2.3 An optional mapped property's own type (`…-mapped-optional-type.diff`)

**Forcing constraint.** `getTypeOfMappedSymbol` (`checker.go:20993`):
`strictNullChecks && symbol optional && !maybeTypeOfKind(propType,
Undefined|Void) ? getOptionalType(propType, true) : …`. So
`{ [K in keyof Partial<Obj>]: Obj[K] }` prints `{ a?: 1 | undefined; b?: 2 |
undefined; }` (addPropertyToElementList reads `getNonMissingTypeOfSymbol`,
which removes nothing outside exact mode). `get_type_of_mapped_symbol`
published the bare template; property READS added the `undefined`
themselves, so reads were right and prints wrong. **Port:** the arm, before
the existing strip arm. Readers adding `undefined` again see the same union.

**Measured** (on top of 2.1-2.2): types **+22**, zero losses:
mappedTypeIndexedAccessConstraint 11, mappedTypeGenericIndexedAccess 11.

### 2.4 An intersection alias reference takes the declaring alias (`…-intersection-new-alias.diff`)

**Forcing constraint.** `getTypeFromTypeAliasReference`'s `newAliasSymbol`
(`checker.go:23609`) hands `PartMappings` to `getTypeAliasInstantiation`, and
an intersection body passes it to `getIntersectionTypeEx(types, flags,
alias)` (`:26056`), whose cache keys on the alias too. So `type PartMappings
= SetOptional<Mappings, "foo">` declares a type printed `PartMappings`.
`new_alias_instantiation` (`declared.rs`) answered only a `Named` result and
returned an intersection unchanged. **Port:** an intersection result is
re-interned with the new alias's name and symbol, the target pair kept in
`type_reference_targets`, recorded in `alias_of` and cached in
`deferred_alias_references` like the `Named` image. The reduced-to-one case
already returns earlier (`alias_union_body_receives_new_alias`).

**Measured** (on top of 2.1-2.3): types **+23**, zero losses:
intersectionTypeInference3 13, mappedTypeIndexedAccessConstraint 6,
caseInsensitiveFileSystemWithCapsImportTypeDeclarations 2,
declarationEmitExactOptionalPropertyTypesNodeNotReused ×2 1+1.

### 2.5 The stack, and what remains

Diffs 2.1-2.4 in order on `f334de9`: types **+75 WRONG→RIGHT**, **zero
losses** on both dumps, diagnostics unchanged; slowcases clean on both
dumps; Ir domain-model 1,091,932,827 (×1.00098), generic-imports
343,069,416 (×0.99986). mappedTypeIndexedAccessConstraint: 48 → 8 failing
lines.

Remaining in the case, with causes:

- `m3 : 1 | 2` where native has `1 | 2 | undefined` (5 lines):
  `{ [K in keyof Identity<Partial<M0>>]: M0[K] }`'s modifiers type is a
  homomorphic mapped alias over another; the indexed-access constraint does
  not read through it. `getModifiersTypeFromMappedType`'s chain, mapped.rs.
- `mapper[key](o)` answers `error` where native gives `PartMappings[K]`
  (3 lines): a call through `((o) => R) | undefined` (TS2722 natively, the
  call still resolves). `calls.rs`, main.

## 3. objectFreeze* literal retention: three producers, one ported

40 failing lines on `f334de9` across objectFreeze (15), objectFreezeLiteralsDontWiden
(14) and objectFromEntries (11). Classified against the pinned `tsgo`:

### 3.1 A generic overload infers from a context-free object literal (`…-overload-object-literal-context.diff`)

**Forcing constraint.** `Object.freeze` has three overloads; the object goes
to the second, `<T extends { [idx: string]: U | null | undefined | object },
U extends string | bigint | number | boolean | symbol>(o: T): Readonly<T>`.
`chooseOverload` (`checker.go:9025`) runs `inferTypeArguments` per
candidate, and that checks each argument with
`checkExpressionWithContextualType(arg, paramType, context, checkMode)`,
which is not cached. Under the second candidate each member's contextual type
is the index signature's `U | null | undefined | object`, and
`checkExpressionForMutableLocation`'s `isLiteralOfContextualType`
(`checker.go:25522`) keeps the literal through `U`'s primitive constraint.

TSR's `transcribed_generic_set_walk` checked every non-context-sensitive
argument once, context-free, before the walk (`argument_types`), and a
generic candidate's `check_generic_call_with` then read that cached node
type. The single-declaration form of the same signature was already right
(`o : Readonly<{ a: 1; …}>`), which isolated the cause: the cache, not the
literal test. The walk already evicted an EMPTY array-literal argument for
the same reason (its tuple-ness depends on the candidate).

**Port.** A generic candidate evicts an object-literal argument's subtree
before its inference, so the candidate's context decides. The candidate that
is picked returns at once, so the published type is its check, as native's
printed type is the check under the resolved signature.

**Measured** (alone on `f334de9`): types **+29 WRONG→RIGHT**, diagnostics
**+1** (objectFreezeLiteralsDontWiden WRONG→RIGHT), zero losses on both dumps,
slowcases clean, Ir ×1.00011 / ×0.99991. objectFreezeLiteralsDontWiden 14
(fully RIGHT), objectFreeze 9, generatorReturnContextualType 6. `calls.rs` is
main's, so a diff.

**Refused, with the number.** Evicting every array-literal argument the same
way (`objectFromEntries`'s `[['a', 5], ['b', 6]]` needs the tuple context):
**+38 / −16 types, −1 diagnostics** (tupleTypeInference 3 and its
EMPTY_RIGHT diagnostics, strictBindCallApply1 10, variadicTuples1 3). That
is the hazard the existing empty-literal comment names: re-checking elements
under a rejected candidate's context re-enters their own calls' resolution,
whose published answers the walk does not undo. objectFromEntries' 11 lines
wait on that (speculative call resolution that can be rolled back; main's
`calls.rs`).

### 3.2 What remains: the function's own contextual signature (6 lines)

`Object.freeze(function foo(a: number, b: string) { return false; })` prints
`(a: number, b: string) => false` natively. `getReturnTypeFromBody`
(`checker.go:20203-20212`) keeps the unit return when
`getContextualSignatureForFunctionLikeDeclaration(fn) ==
getSignatureFromDeclaration(fn)`: the return type is resolved lazily, after
the call has fixed `T` to the function's own type, so the contextual
signature IS the function's. Probed: `declare function id<T extends
Function>(f: T): T; id((a: number) => false)` is `(a: number) => false`
natively and `(a: number) => boolean` here, with no overload involved. This
port infers the return type eagerly, inside the argument's inference check,
when `T` is still unfixed and its constraint `Function` has no call
signature. Emulating the identity ("the parameter is a naked type parameter
inferred from this very argument") would re-derive native's decision outside
its algorithm (`box-protocol.md` §3a), so it is not attempted. Owner: lazy
signature returns (main; the same wall as r5-instexpr §2.5).

## 4. The held conditional-node pair: its blocker is five ports deep, two shipped

**Re-base.** Batch BU had not landed when this item started, so this branch
merged `claude/beautiful-shannon-ar5gh0-r6-typesroots2` (BU's commits) and
applied BU's three diffs in the working tree for every measurement in this
section: base `BU` = `f334de9` + that merge + diffs 1-3 of
`r6-typesroots2.md` §6: types 550,408 RIGHT / 5,151 WRONG / 744 GAP,
diagnostics 12,238 cases, Ir 1,091,010,319 / 343,080,835. (BU measured +67
on `e6eadf4`; on `f334de9` the same three diffs are 58 WRONG→RIGHT, 9
GAP→RIGHT, 4 GAP→WRONG, zero losses, which is BU's own transition set.)

**The held diff re-measured.** `r6-typesroots2-HELD-conditional-node-consumers.diff`
on base `BU` with this note's §2 diffs and §4.2's first half (the
`couldContainTypeVariables` arm): types **+112**
(98 WRONG→RIGHT, 14 GAP→RIGHT; 15 GAP→WRONG, 1 WRONG→GAP), **zero type
losses**, and still the one diagnostics loss:
complicatedIndexesOfIntersectionsAreInferencable EMPTY_RIGHT → EMPTY_WRONG
(TS2339 on `props.foo`, `props : object`).

### 4.1 What native actually does with that call (the brief's name is not the mechanism)

r6-typesroots2 §2 named the blocker INFERENCE-REVERSE-MAPPED-INTERSECTION,
reverse inference into `Readonly<FormikConfig<Values> & ExtraProps>`. Read
against the pinned source and probed piece by piece with the pinned `tsgo`,
that branch infers nothing natively:

- `getIndexType(FC<V> & EP)` is the union `keyof FC<V> | keyof EP`
  (literals plus a deferred `keyof EP`), so `inferToMappedType`
  (`inference.go:948`) walks the union, reaches the `Index` constituent, finds
  no inference for `FC<V> & EP` (it is not a type parameter), and returns
  `true` (`:971`): no structural inference follows.
  `declare function F1<V = object, EP = {}>(x: Readonly<FC<V> & EP>): V`
  infers `V = object` natively (probed).
- `Values` comes from the conditional's FALSE branch,
  `Pick<Readonly<FC<V> & EP>, "validate" | "initialValues" | Exclude<keyof EP,
  "validateOnChange">> & Partial<Pick<…>>` (probed: the Pick alone infers
  `{ foo: string }`). That needs, in order:
  1. `couldContainTypeVariables`' union-OR-intersection arm (an
     intersection argument was not walked) — ported, §4.2;
  2. inference to the simplified indexed access (`inference.go:217`):
     the Pick member `Readonly<FC<V> & EP>["initialValues"]` → the mapped
     arm `(FC<V> & EP)["initialValues"]` → the intersection arm
     `V & EP["initialValues"]`, whose one naked variable is `V` — ported,
     §4.2;
  3. the Pick's member keys: `getLowerBoundOfKeyType` (`checker.go:21021`)
     maps the distributive `Exclude<keyof EP, …>` through `keyof EP`'s lower
     bound (`keyof {}` = `never`). `mapped_member_keys` declines any generic
     constituent, so `Pick<…, "validate" | "initialValues" | Exclude<…>>` has
     no members here. Owner: `mapped.rs` (r6-declared2), with the conditional
     instantiation in `declared.rs`. Not ported;
  4. the candidate-free `EP` must resolve to its default:
     `check_generic_call_worker` (`inference.rs`, main, "Structural inference
     is still incomplete") refuses the whole call when a parameter with no
     candidate is mentioned by a parameter type that received a source.
     `EP` occurs only as an indexed-access object (`EP["initialValues"]`),
     where native infers nothing. Measured with the refusal bypassed (a
     probe, not a port): `F13<V = object, EP = {}>(x: (FC<V> &
     EP)["initialValues"])` and `Pick<FC<V> & EP, …>` then infer `{ foo:
     string }` as native. A port needs a certificate that every occurrence
     is a non-inferring position; not attempted;
  5. inference to the conditional target's two branches
     (`inferToConditionalType`), unverified here because 3-4 stop first.

So the held diff still waits, now on 3 and 4 rather than on reverse mapped
inference.

### 4.2 `…-inference-intersection-indexed-access.diff` (inference.rs, main)

Steps 1 and 2 above:

- `could_contain_parameter_inner`'s union arm also takes an intersection
  (`couldContainTypeVariables`, `TypeFlagsUnionOrIntersection`). Without it
  `Readonly<FC<V> & E>` (concrete `E`) was treated as parameter-free and
  `G1({ initialValues: { foo: "" }, … })` inferred `object`; native `{ foo:
  string }` (probed).
- `inference_simplified_indexed_access`: `getSimplifiedIndexedAccessType`'s
  generic-mapped arm (`substituteIndexedMappedType`) and intersection arm
  (`distributeIndexOverObjectType`, gated by `shouldDeferIndexType`'s
  intersection clause), applied where `inferFromTypes` applies it, before the
  structural arms, which still run. Stated gaps: the union arms, the
  generic-tuple arm, and the modifiers-type optionality of the mapped arm.

**Measured** (on base `BU`, alone): types **+2 WRONG→RIGHT**
(defaultDeclarationEmitNamedCorrectly), zero losses on both dumps, slowcases
clean, Ir ×1.00044 / ×1.00000. Small alone; it is the half of the held pair
this lane can reach. The test pins both arms and fails without the diff.
