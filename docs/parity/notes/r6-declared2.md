# r6-declared2: `declared.rs`, `mapped.rs` and neighbours, round 6

Lane files: `declared.rs`, `instantiation_expressions.rs`, `unique_symbols.rs`,
`mapped.rs`, `intersections.rs`, `intrinsics.rs`. Issue `tsr-2zk.1143`.
Successor of r6-declared (`r6-declared.md`), whose commits (batch BM) had not
landed on the integration branch when this box started. Vendor pinned at
`5b1047d`.

## 0. Frozen base

`claude/beautiful-shannon-ar5gh0` at `0854d36` (batch BI's snapshot
refresh). Batch BM was not on the branch, so the brief's fallback (the
current tip) applies. Unfiltered release build:

- `diagverdictdump`: RIGHT 5595, EMPTY_RIGHT 5606, WRONG 998, EMPTY_WRONG 39
  (12,238 rows);
- `verdictdump`: RIGHT 550,131, WRONG 5,404, GAP 768 (556,303 rows).
- Callgrind Ir (`tsr -p <project> --singleThreaded --pretty false
  --noEmit`): domain-model 1,091,238,874; generic-imports 343,094,406.

Setup: PyPI answers 403, so `assemble.py`'s three `tomlkit` calls ran on a
stdlib-only stand-in in the session scratchpad, not committed
(`r5-operators3.md` §4). The native oracle is
`scripts/offline-cargo/build-tsgo.sh`'s tsgo; every expectation below was
checked against it (`--declaration --emitDeclarationOnly` prints, or an
assignment's diagnostic).

## 1. An unevaluable conditional alias reference in an alias body defers

**Forcing constraint.** `get_instantiated_type_reference` answered `error`
for a reference to a conditional alias in an alias-declared position (an
alias body, `in_alias_declared_position`) whenever
`evaluate_conditional_alias` declined. Native's getTypeAliasInstantiation
(checker.go:23641) reaches getConditionalType (checker.go:24339), which
**defers** when the instantiated check type is generic, or the extends type
is (`isDeferredType`): the result is a ConditionalType with its root, mapper
and the reference's alias, printed as the alias reference. Inside a mapped
node that `error` made `mapped_type_info` decline, so `Exclude<P, K>` in an
`as` clause or `Remap2<T[P]>` in a template fell back to written text
(`r6-mapped.md` §3a).

**The plain fallthrough was measured and refused.** Letting the arm fall
through to the named reference (the port's deferred conditional: a reference
whose `type_reference_targets` entry `alias_declares_conditional` reads as a
conditional) measured, on this base, types **+78 / −17**. The losses show
what the `error` stood for:

- **A decided check whose branch native also cannot compute**:
  `awaitedType:0:27/34`, `awaitedTypeStrictNull:0:27/34` (`Awaited<BadPromise>`),
  `excessivelyLargeTupleSpread:0:1` (`BuildTuple<…>`),
  `recursiveConditionalTypes:0:32` (`_Flatten<InfiniteArray<string>>`).
  Native reaches its instantiation guard (TS2589) and answers errorType; the
  port's evaluator declines at the same guard. `awaitedType:0:14`
  (`Awaited<any>`) is native `any` by evaluation, which this port's
  evaluator does not do for an `infer`-bearing nested conditional; the gap's
  `any` print coincides with it.
- **`intersectionWithIndexSignatures` ×5**: the fallthrough printed
  `constr<{}, …>` where native names the declaring alias `s`.
- **`contextualTypesNegatedTypeLikeConstraintInGenericMappedType3` ×5**: the
  mapped type now built, but its `as Exclude<P, K>` did not exclude
  `onChange`, so the parameter widened to `number | Event` (see §1.1).

**Port.** The arm falls through only when native defers:
`conditional_alias_check_is_deferred` resolves the alias's check type (and,
without `infer` parameters, its extends type) under the alias bindings and
asks the evaluator's own generic test (`INSTANTIABLE_NON_PRIMITIVE`, or a
registered type parameter mentioned). Every other decline keeps the gap. The
first and second loss groups have concrete check types, so they keep `error`.

**Divergence kept.** With `infer` parameters, native tests
`inferredExtendsType`, the extends type under the inference mapper; this test
cannot form it and asks only the check type. A check that is decided but whose
branch the evaluator cannot compute (`Awaited<any>`, the guard cases) keeps
`error`, as before. Seven of the plain fallthrough's gains
(`intersectionWithIndexSignatures:0:39–49`) came from such concrete checks
printing as references; they are not taken, since that print is not native's.

### 1.1 isExcludedMappedPropertyName reads the conditional itself

getIndexedMappedTypeSubstitutedTypeOfContextualType (checker.go:30607) skips
a property whose name the `as` clause excludes: isExcludedMappedPropertyName
(checker.go:30624) asks whether the name type is a conditional whose true
type is `never`, whose false type is its check type, and whose extends type
the property name is assignable to. The port answered only for an inline
conditional minted in a mapped template (`mapped_conditionals`), not for a
reference to a conditional alias, which `Exclude<P, K>` now is.

`conditional_root_operands` (`declared.rs`) answers `[check, extends, true,
false]` for both: the inline mint's recorded operands, or the alias root read
under its bindings through `with_conditional_inference_node`.
`is_excluded_mapped_property_name` (`mapped.rs`) asks it for any
conditional-flagged or alias-reference name type. No cache: the read is the
existing root walk.

**Measured** (unfiltered, both dumps, against §0):
- types **+70 RIGHT, 0 lost**: `conditionalTypes1` 33,
  `mappedTypesArraysTuples` 8, `conditionalTypes2` 8 (172–184, a brief
  target), `mappedTypeOverlappingStringEnumKeys` 5,
  `propTypeValidatorInference` 4, `genericIsNeverEmptyObject` 4,
  `mappedTypeAsClauses` 3 (98/100/107), and
  `recursiveTypeAliasWithSpreadConditionalReturnNotCircular` 2,
  `recursiveMappedTypes:0:24`, `literalTypeWidening`,
  `recursiveTupleTypeInference` 1 each;
- diagnostics **+1 case**, `mappedTypeOverlappingStringEnumKeys` EMPTY_WRONG →
  EMPTY_RIGHT, 0 lost;
- slowcases clean on both dumps;
- Ir: domain-model 1,091,238,874 → 1,094,345,197 (+0.28%), generic-imports
  343,094,406 → 343,210,798 (+0.03%). The deferral test itself is 112k Ir.
  The rest is diffuse: `core.ts`'s `DeepReadonly<T[K]>` and
  `DeepReadonly<U>` in its own body now build as deferred references, so the
  mapped template and what relates through it exist where `error` stood
  (`evaluate_mapped_type_node` +0.62M, the evaluator +0.75M inclusive, and
  sub-1% per-call growth across the relater and member roads). That is
  native's work: tsgo builds the same deferred conditional. CPU, interleaved
  31 runs each against the base binary: single-threaded 1.000, multi-threaded
  0.970 (the harness's blocked 41-sample run read 1.047; a base-vs-base run of
  it read 1.015, so its ordering drifts). CLI output identical on both
  projects.

**Still not RIGHT among the brief's targets.**
- `mappedTypeAsClauses:0:72` (`GetKeyWithIf<S, V>`) and `:0:108` (`TN4<T, U>`):
  `keyof` of a mapped type with an `as` clause whose conditional nests
  another conditional in its check type
  (`(K extends U ? T[K] : never) extends T[K] ? K : never`); native prints
  the `keyof { … }` form. The evaluator still declines on the nested check.
- `reactReduxLikeDeferredInferenceAllowsAssignment:0:82`: the thunk's
  `Promise<string>` prints `unknown`. The deferred
  `HandleThunkActionCreator<TDispatchProps[C]>` now builds; the return type
  is inference's (`inference.rs`, MAIN).

Tests: `tests/r6_declared2.rs`
`a_deferred_conditional_alias_in_an_alias_body_keeps_its_alias` (`b.v` of
`Box<T> = { v: Ex<T, null> }` is `Ex<T, null>`, as tsgo prints; errorType on
the base) and `a_concrete_check_does_not_defer`.

**Falsifier.** A conditional alias in an alias body whose check type is
generic and which native nonetheless evaluates (an any/never/error check is
decided before the generic test natively too), or a mapped `as` clause whose
exclusion native does not make: a contextual parameter typed `any` here where
native types it.

## 2. getMappedTypeNameTypeKind: native answers Filtering; the held diff's losses are contextual instantiation

**The brief's hypothesis.** r6-relater §4 held
[`r6-relater-write-constraint.diff`](r6-relater-write-constraint.diff)
(+23/−2) on the claim that native decides getMappedTypeNameTypeKind
(checker.go:26842) as Remapping for `as K extends Uppercase<string> ? K :
never`, where the port decides Filtering from the branch union `K | never`.

**Native probe** (tsgo, `--strict`), with the mapped type left generic so
getTypeOfPropertyOfContextualTypeEx (checker.go:30565) must ask the kind:

```ts
function f<T extends { type: string }>() {
  const x: { [K in T["type"] as K extends Uppercase<string> ? K : never]?: (ev: K) => void } = { bar: (ev) => { const n: number = ev; } };
  const y: { [K in T["type"] as Exclude<K, "x">]?: (ev: K) => void } = { bar: (ev) => { const n: number = ev; } };
  const z: { [K in T["type"] as K extends "x" ? never : K]?: (ev: K) => void } = { bar: (ev) => { const n: number = ev; } };
  const w: { [K in T["type"] as `p${K}`]?: (ev: K) => void } = { bar: (ev) => { const n: number = ev; } };
}
```

`x`, `y` and `z` report TS2322 on `const n: number = ev` (so `ev` is typed
through getIndexedMappedTypeSubstitutedTypeOfContextualType: the kind is
Filtering); only `w`, the template-literal remap, reports TS7006 (Remapping).
That matches relater.go's conditional-source arm: the default constraint
`K | never` relates to `K`. With the held diff applied the port answers the
same kinds: `ev` is typed for `x`/`y`/`z` and `any` for `w`. So the kind is
already native's, and `mapped.rs` keeps it.

**What `:107/108` actually need.** In `contextualTypeFunctionObjectPropertyIntersection`,
native types `bar`'s `ev` as `any` because the contextual type of `on` is
instantiated with the first inference pass (`TEvent := { type: "FOO" } | {
type: "bar" }`, instantiateContextualType): the mapped type is then concrete,
`"bar"` fails `Uppercase<string>`, and no property `bar` exists. The port
keeps it generic with `TEvent` at its constraint. The same cause shows on the
base in the `"*"` lines of that case, already WRONG: `:40–47` and `:85–92`
print `ev: { type: string; }` where native prints the inferred union. Owner:
contextual instantiation (`contextual.rs`/`inference.rs`, MAIN).

**The diff, measured on top of §1's commit** (unfiltered): types **+23 / −2**
(gains `contextuallyTypedSymbolNamedProperties` 13,
`contextualTypeFunctionObjectPropertyIntersection` 10; losses `:107/:108`),
diagnostics unchanged. It stays held on the contextual instantiation above,
not on `mapped.rs`.

(Native prints `ev` as `string`, not `"bar"`, in the TS2322 for `x`/`y`/`z`;
the port gives `"bar"`. Not chased here.)

## 3. Homomorphic mapped alias arms

### 3(a), 3(c): instantiateConstituent's unmapped constituents (landed)

**Forcing constraint.** instantiateMappedType (checker.go:22535) distributes a
homomorphic mapped type over its instantiated type variable
(mapTypeWithAlias), and instantiateConstituent (:22551) returns a constituent
unchanged unless it is any/unknown, a non-primitive instantiable, an object or
an intersection (or returns errorType as is). `mapped.rs` had that arm only on
the path with captured mapped info, and there as `PRIMITIVE | NEVER`. A
recursive alias whose capture declines (`RequiredDeep<T> = { [K in keyof T]-?:
RequiredDeep<T[K]> }` over `undefined`: its template `undefined[K]` does not
resolve) reached the union-only arm and minted `RequiredDeep<undefined>`.

**Port.** `is_unmapped_homomorphic_constituent` is native's flag test; both
`instantiate_mapped_alias_sequence` paths ask it before distributing.

Native probe (tsgo `--declaration`):
- `RequiredDeep<undefined>` is `undefined` (port: `RequiredDeep<undefined>`
  before, `undefined` now);
- `x.a` of `RequiredDeep<{ a?: 1 }>` is `1`: the template reads
  `RequiredDeep<1 | undefined>`, which distributes into `1 | undefined` with
  the alias kept for print (`vv : RequiredDeep<1 | undefined>` matches), and
  getTypeOfMappedSymbol's StripOptional removes `undefined`. The port printed
  `RequiredDeep<1 | undefined>`; now `1`. The `-?` removal itself was already
  in place (`PropertySlot::of_mapped`'s `strip_optional`); only the
  distribution's unmapped constituent was missing.

**Measured** (unfiltered against §1's commit): no verdict moves in either
dump; slowcases clean; Ir flat (domain-model 1,094,335,565, generic-imports
343,215,617). The relation over `RequiredDeep<1 | undefined>` was already
native's (r6-relater §2(2) normalizes the memberless image); this changes the
type itself.

Tests: `tests/r6_declared2.rs`
`a_required_deep_member_distributes_and_strips_undefined` and
`a_homomorphic_alias_over_a_primitive_is_the_primitive`, both failing on §1's
commit.

**Falsifier.** A template literal, string mapping or `never` argument to a
homomorphic alias that native maps: native's flag test returns those as is
too, so a mapped print for one here would be a port difference elsewhere.

### 3(b): `Required<Pick<…>>` — held as a diff

**Cause.** `instantiate_identity_mapped_alias` (§952's identity road) mints
`Required<X>` with X's member *owner* and reads property types through X.
When X is itself a resolved mapped image (`Pick<SomeProps, "x">`, `PX`), its
members live per instance in `anonymous_properties`; its owner is the mapped
alias's symbol, shared by every instance and declaring nothing. The relater
enumerates the minted image's names from that owner and finds none, so
`{ x?: string }` relates to `Required<PX>` and to `Required<Pick<SomeProps,
"x">>` without a property check. Native (tsgo, `--strict`) reports TS2322 on
both (and TS2741 for `= {}`); the port reported neither unless an earlier
object-literal check happened to resolve members first.

**Port (diff).** [`r6-declared2-mapped-source-members.diff`](r6-declared2-mapped-source-members.diff):
- `declared.rs`: the identity road declines a source in `mapped_types`, so
  `Required<PX>` takes instantiateMappedType's general road
  (`capture_mapped_alias`, per-member modifiers in
  `resolve_mapped_type_members_worker`). Native has no identity shortcut.
- `members.rs` (MAIN): `property_names_of_type_worker` resolves a non-generic
  mapped image's members before reading them (getPropertiesOfType ->
  resolveStructuredTypeMembers). Without it a relation that enumerates
  `Required<PX>` before any property read sees the mapped node symbol's empty
  table.

**Measured** on top of §3(a)'s commit `9d6bd2c`, unfiltered:
- diagnostics **+4 cases**, 0 lost: `identicalTypesNoDifferByCheckOrder` (the
  brief's target), `mappedTypeRecursiveInference`, `checkJsdocSatisfiesTag10`,
  `typeSatisfaction_propNameConstraining`, all WRONG → RIGHT;
- types **−2**: `destructuringParameterDeclaration10(strict=false):0:20/24`.
  The contextually typed arrow `({ additionalFiles: { json = [] } = {} } =
  {}) => …` against `{ additionalFiles?: Partial<Record<…, string[]>> }`
  types `json` as `any[]` where native (and the identity image) give
  `string[]`. A top-level `({ json = [] })` against the same type is right on
  the general road; only the nested pattern with a default reads the mapped
  image through the identity-optionality channel. The reader is contextual
  destructuring (`contextual.rs`/`destructure.rs`, MAIN). The declared.rs half
  alone measured −2 and +0, so neither half lands without that reader.

A narrower decline (only sources whose owner enumerates no names) was not
taken: spreads and property reads answer correctly through either road, and
the only principled boundary is native's, which has no identity road.

## 4. `NoStrictNullChecks3`: an optional member read adds `undefined` without strictNullChecks (diff, MAIN)

**Forcing constraint.** With `strictNullChecks: false`, tsgo types
`h(1)()` for `declare function h<D>(d: D): () => Id<{ a?: never; d?: D }>`
(`type Id<T> = { [K in keyof T]: T[K] } & {}`) as `{ a?: never; d?:
number; }`; the port printed `a?: undefined`. r6-accessible §3c placed it in
the intersection alias body under an outer mapper. Probed further, the
intersection is not needed: a plain `M<T> = { [K in keyof T]: T[K] }` under
the same outer mapper reads `.a` as `undefined` (printed `any` after
widening), and so does any instantiated type literal's optional member. The
non-generic `{ a?: never }["a"]` and the uninstantiated `Id<{ a?: never }>`
are right.

**Cause.** `get_type_of_property_with_this_argument` (`members.rs`, MAIN)
reads an instantiated object's (`anonymous_properties`) optional member with
`get_optional_type` unconditionally. Native adds optionality only under
strictNullChecks: addOptionality (`strictNullChecks && isOptional`), and
getTypeOfMappedSymbol (checker.go:20993) the same. Every `declared.rs` and
`mapped.rs` site already gates on `strict_null_checks`; `never | undefined`
is `undefined`, which is the printed `a?: undefined`.

**Diff.** [`r6-declared2-nonstrict-optional-read.diff`](r6-declared2-nonstrict-optional-read.diff)
gates that read on `strict_null_checks`. Measured on §3(a)'s commit
`9d6bd2c`, unfiltered:
- types **+1**, 0 lost: `declarationEmitOptionalMappedTypePropertyNoStrictNullChecks3:1:11`;
- diagnostics unchanged; slowcases clean; Ir flat (domain-model
  1,094,340,348, generic-imports 343,216,186).

The case's other three lines (`:1:1/1:2/1:12`) now print `originalArgs?:
never` too, but stay WRONG on the alias: native expands the non-exported
`Id<…>` of another file at the declaration site (IsTypeSymbolAccessible),
which is r6-accessible §3(b)/ADR-0045's cross-file accessibility, not this
cause.
