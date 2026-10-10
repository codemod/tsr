# r7-declared: `declared.rs`, `mapped.rs`, `instantiation_expressions.rs`, `indexed.rs`, `unions.rs`, round 7

Lane `r7-declared` (`tsr-2zk.1273`), dispatched in `docs/parity/round7.md`.
Owned files: `declared.rs`, `mapped.rs`, `instantiation_expressions.rs`,
`indexed.rs`, `unions.rs`. Native source is `vendor/typescript-go` @
`5b1047d`. Only this lane writes this note.

## 0. Frozen base

`origin/main` at `9020aa67` (round-7 dispatch), frozen with
`scripts/parity_gate.sh freeze /tmp/box/base`:

- `verdictdump`: TOTAL 556,357, RIGHT 551,176, GAP 673, WRONG 4,508.

Every measurement below is an unfiltered `freeze` of the candidate tree and
`scripts/parity_gate.sh compare /tmp/box/base <candidate>`.

## 1. `tsr-2zk.1266`: r6-typesroots3's mapped stack, re-cut on the print-time plans

**What was held.** Batch CG+CH+CI backed out r6-typesroots3's four mapped
diffs (`1b5ef1a8` mapped alias-reference keys, `212e93b6` mapped member
names, `c786f303` mapped optional type, `53c5e4c4` intersection new alias;
reverted by `1751bac4`, `5aa8be47`, `63637259`, `0ff9e8e6`) because
mappedTypeIndexedAccessConstraint 0:96, 0:97 and 0:101 went from RIGHT to
`any` (`round5.md`, "Batch CG+CH+CI"). The four commits cherry-pick cleanly
onto `9020aa67`; the stack is
[`r7-declared-HELD-mapped-stack.diff`](r7-declared-HELD-mapped-stack.diff).

**The three lines were RIGHT by coincidence.** Probed with the base's
`probefile`: on `9020aa67` the declared type of

```ts
const mapper: { [K in keyof PartMappings]: (o: MapperArgs<K>) => PartMappings[K] } = { … };
```

is `{}` (the alias reference `PartMappings = SetOptional<Mappings, "foo">`
enumerates no keys, r6-typesroots3 §2.1), `mapper[key]` is `{}[K]`, and the
call `mapper[key](o)` resolves through that deferred access's template to
`PartMappings[K]`, which is the native print reached by the wrong road. With
the stack, `mapper` is native's
`{ foo?: ((o: MapperArgs<"foo">) => boolean | undefined) | undefined; "12": …; 42: …; }`
and `mapper[key]` is native's `((o: MapperArgs<K>) => PartMappings[K]) |
undefined`. Then the call answers `error` (printed `any`).

**The cause is in `calls.rs`.** `resolveCallExpression` (`checker.go:8511`)
passes every callee through `checkNonNullTypeWithReporter`
(`checker.go:7413`): it reports TS2722/TS2721/TS2723 and then resolves the
call against `GetNonNullableType(funcType)`. TSR's head reports the
diagnostic (`check_non_null_callee`, `calls.rs`), but the type road
(`check_call_expression`) strips the nullable half only for an optional
chain. Any call through `F | undefined` therefore answers `error`; this is
not specific to mapped types:

```ts
declare const g: ((o: number) => string) | undefined;
const a = g(1);   // native: string (with TS2722); TSR on 9020aa67: error
```

The port is [`r7-declared-calls-nonnull-callee.diff`](r7-declared-calls-nonnull-callee.diff):
the non-optional branch takes `check_non_null_type(raw_callee_type)`
(`members.rs`, the existing non-reporting `checkNonNullType`), and an
`errorType` result returns `error` as native's `resolveErrorCall` does.
`calls.rs` belongs to r7-calls, so it is routed, not committed here.

**Measured** (unfiltered against §0):

| Tree | types | diagnostics |
|---|---|---|
| strip alone (`calls.rs` diff) | +11 RIGHT, 0 lost | 0 / 0 |
| mapped stack alone | +57 RIGHT, **3 lost** (0:96, 0:97, 0:101) | 0 / 0 |
| stack + strip | **+68 RIGHT, 0 lost** | 0 / 0 |

The strip's own +11: logicalAssignment5 ×4 targets (2 each),
controlFlowOptionalChain 2, interfaceClassMerging 1. The stack's rows on
top of it: mappedTypeIndexedAccessConstraint 39, mappedTypeGenericIndexedAccess
11, reverseMappedPartiallyInferableTypes 4, declarationQuotedMembers 3. (r6
measured the stack at +75 on `f334de9`; intersectionTypeInference3 and the
caseInsensitive/exactOptional rows it named are RIGHT on `9020aa67` without
it.) mappedTypeIndexedAccessConstraint keeps 5 wrong lines, the
`Identity<Partial<M0>>` modifiers chain (r6-typesroots3 §2.5).

**Landing order.** The strip first (r7-calls, or the integrator), then the
four cherry-picks. The stack is not committed on `box/r7-declared` while the
strip is not on main, because alone it loses the three lines.

## 2. `tsr-2zk.1265`: the deferred conditional fall-through, re-landed on native's `isGenericType`

**What was held.** r6-declared2's `c06e47b2` (r6-declared2 §1) let
`get_instantiated_type_reference`'s alias-declared `return error` fall
through to the deferred conditional reference when
`conditional_alias_check_is_deferred` said getConditionalType defers, and
made `is_excluded_mapped_property_name` read a conditional alias reference's
root operands (`conditional_root_operands`). Batch BY+BZ backed it out
(`46aa9e32`): compiler/returnTypePredicateIsInstantiateInContextOfTarget
went EMPTY_RIGHT → a false TS2769 on `<TestComponent />`.

**Root cause.** The deferral test was not native's. It called a check type
deferred when it `mentions_registered_type_parameter`, the evaluator's own
decline gate. In the failing case React's `LibraryManagedAttributes` reaches
`Defaultize<P, D>` with

```ts
P = Readonly<{ children?: ReactNode; }> & Readonly<{ isAny: <T>(obj: any) => obj is T; }>
```

The only type parameter `P` mentions is `T`, which `isAny`'s own signature
binds. Native's test is `isDeferredType(checkType, checkTuples)`
(`checker.go:24475`), meaning `isGenericType`, meaning
`getGenericObjectFlags` (`:24880`). That flag is set only for an
instantiable non-primitive, an index or generic string-like type, a generic
mapped type, a generic tuple, or a union/intersection containing one. An
intersection of two `Readonly` mappings over concrete objects is none of
these, so native evaluates `Defaultize`. TSR deferred it. Probed with
`diagcase` and a debug print on the JSX road: `jsx_effective_first_argument`
received the deferred reference `Defaultize<…concrete…>`, which the
overload road related and rejected.

**Port.** `is_generic_type` (`declared.rs`) is getGenericObjectFlags'
`IsGenericObjectType | IsGenericIndexType` over this port's
representations. Three of native's instantiable kinds are minted here
without `INSTANTIABLE_NON_PRIMITIVE`, so each is read from its table:

- a deferred indexed access (`deferred_indexed_access_types`);
- an inline conditional (`mapped_conditionals`);
- a deferred conditional alias reference (`type_reference_targets` whose
  target `alias_declares_conditional`).

Generic mapped types use `is_generic_mapped_type` after
`ensure_mapped_type_info` (native's lazy getConstraintTypeFromMappedType),
and generic tuples use `is_generic_tuple_type`. `is_deferred_type` adds
`checkTuples`: both nodes are simple tuple types of the same length
(`isSimpleTupleType`), and an element is generic.
`conditional_alias_check_is_deferred` asks that test, and c06e47b2's
fall-through and its `conditional_root_operands` reader are restored
unchanged. No cache: per query, reads only.

**Divergence kept.** Native also defers a check that is not generic when
the permissive relation does not reject and the restrictive one does not
accept (`checker.go:24415`). This port's evaluator declines any check
mentioning a registered type parameter before it reaches that relation
(`evaluate_conditional_node`), so such a check (`{ a: T } extends { a:
string }`) keeps the `error` gap here, as before c06e47b2. That gate is
CONDITIONAL-DEFERRAL-GATE (§3).

**Measured** (unfiltered against §0): types **+99 RIGHT, 0 lost**;
diagnostics 0 gained, 0 lost (returnTypePredicateIsInstantiateInContextOfTarget
stays EMPTY_RIGHT). Gains by case: conditionalTypes1 33,
reactDefaultPropsInferenceSuccess 24, propTypeValidatorInference 16,
mappedTypesArraysTuples 8, conditionalTypes2 4, genericIsNeverEmptyObject 4,
mappedTypeAsClauses 3, recursiveTypeAliasWithSpreadConditionalReturnNotCircular
2, intersectionConstraintReduction 2, recursiveMappedTypes 1,
literalTypeWidening 1, recursiveTupleTypeInference 1. r6-declared2 measured
+70 on `f334de9`. The 24 reactDefaultPropsInferenceSuccess lines are new,
and they are the JSX road the old test broke.

**Falsifier.** A conditional native defers whose check type this test
calls non-generic. That would be a fourth instantiable kind minted without
its flag, and it shows up as a reference printed where native prints the
alias, or as `error` where native prints a deferred conditional.

## 3. CONDITIONAL-DEFERRAL-GATE: the evaluator defers on `isDeferredType`, and relates instantiated check types

**Forcing constraint.** `evaluate_conditional_node` (`declared.rs`) declined
any check type that `mentions_registered_type_parameter`. In the general
path it also declined any check that `mentions_any_type_parameter`. Native
getConditionalType (`checker.go:24339`) defers only when
`isDeferredType(checkType, checkTuples)`. For any other check it decides by
two relations:
- false when the permissive instantiations (`getPermissiveInstantiation`,
  every type parameter to the wildcard, `:24479`) of the check and extends
  types are unrelated (`:24377`);
- true when the restrictive ones (`:24492`) are related (`:24415`);
- deferred otherwise.

The port instantiated only the extends type. Probed with the pinned tsgo:

| Written | TSR before | Native |
|---|---|---|
| `Outer<{ isAny: <T>(obj: any) => obj is T }>`'s `v` (`v: Ex<P, null>`) | `Ex<{ isAny: …; }, null>` | `{ isAny: <T>(obj: any) => obj is T; }` |
| `Outer2<{ isAny: … }>` (`type Outer2<P> = Ex<P, null>`) | `Outer2<{ isAny: …; }>` | `{ isAny: <T>(obj: any) => obj is T; }` |
| `Has<T>` (`{ a: T } extends { b: string } ? 1 : 0`) in a generic body | `Has<T>` | `0` |

**Port.**
- The early decline asks `is_deferred_type(check, check_tuples)` (§2's
  port). `check_tuples` is now `conditional_check_tuples`, shared with
  `conditional_alias_check_is_deferred`.
- The general path drops the `mentions_any_type_parameter(check)` decline.
  It adds `!is_deferred_type(extends, check_tuples)` beside the existing
  flag test. `indexed_access_index_is_generic` (a retained `keyof`
  operand) stays.
- `definite_conditional_outcome` relates `permissive(check)` to
  `permissive(extends)`, then `restrictive(check)` to
  `restrictive(extends)`. It is asked when either operand mentions a
  registered type parameter (`conditional_mentions_type_parameters`).

The instantiations reuse `extends_instantiation`'s cache. That cache is
keyed by type identity, as native's `CachedTypeKindPermissiveInstantiation`
/ `RestrictiveInstantiation` are keyed by the type, so a check type is
cached like an extends type (r6-declared3 §1's convention record holds:
owner `instantiation_expressions.conditional_extends`, each slot written
once, never invalidated).

**Measured** (unfiltered, against §2's freeze):
- types **+1 RIGHT, 0 lost**: genericCallInferenceInConditionalTypes1:0:33
  GAP → RIGHT. :0:23 went GAP → WRONG: `{ ref?: Ref<HTMLElement>; }` where
  native prints `| undefined` on the instantiated optional member, which
  is a printing gap downstream of the now-evaluated conditional;
- diagnostics 0 / 0;
- child CPU vs the frozen binary: 21 samples gave domain-model 0.990,
  generic-imports 1.035; the 41-sample rerun gave 1.004 / 0.987.

**Why so few lines.** The cluster's own cases fail elsewhere. Hypotheses
read from the rows, not yet verified:
- conditionalTypeGenericInSignatureTypeParameterConstraint's `H_inline1 :
  x` is the declared type of a generic alias printed under its own
  parameters, which never reaches this evaluator;
- quickinfoTypeAtReturnPositionsInaccurate's `Extract<Entries[EntryId], …>`
  needs `Entries[EntryId]` to stay a deferred indexed access where this
  port resolves it;
- strictBindCallApply1's `OmitThisParameter<(...args: T) => void>` is the
  function-type check `unknown extends ThisParameterType<T>`;
- inferenceContextualReturnTypeUnion2 is a `Parameters<…>` rest print.
Each is recorded as the cluster's remainder, not attempted in this commit.

## 4. Routed: a union of mapped types loses `-readonly` (r7-perf cluster 6)

r7-perf's jsTyping delta has 8 false TS2540 on `Mutable<ModuleDeclaration |
SourceFile>`-shaped receivers (`type Mutable<T extends object> = { -readonly
[K in keyof T]: T[K] }`). The mapped side is already native's:
`Mutable<A | B>` distributes to `Mutable<A> | Mutable<B>` with the alias
origin (`distribute_mapped_union`, `mapped.rs`). The false report also
fires with no alias: `declare const u: Mutable<A> | Mutable<B>; u.flags = 1`.

**Cause** (`readonly_target.rs`, r7-flow).
`is_assignment_to_readonly_property`'s union arm reads each constituent
through `get_property_of_type(part)` and `is_readonly_symbol`. A mapped
member has no symbol of its own: the found symbol is the modifiers type's
`A.flags`, which is declared readonly. createUnionOrIntersectionProperty
(`checker.go:21452`) reads the transient mapped symbol's
`CheckFlagsReadonly`, which `-readonly` cleared. The non-union tail of the
same function already reads `mapped_identity_optionality` and the member
image for that reason.

**Diff**
([`r7-declared-readonly-union-mapped-member.diff`](r7-declared-readonly-union-mapped-member.diff)):
`constituent_property_is_readonly` asks the constituent the way the
non-union tail asks a receiver. First the homomorphic modifier, then the
member image (a mapped image is final), then the symbol.

**Measured:**
- Against the `.1265` freeze, both corpus dumps are unchanged (no corpus
  case has the shape).
- On `src/jsTyping`, TS2540 goes 8 → 0 (native 0) and every other code's
  count is identical.
- Controls report as tsgo does: `Readonly<A> | A`, `A | B` with a readonly
  member, and `Mutable<B> | Readonly<A>`.

## 5. A parenthesized conditional branch in a mapped template (prerequisite of §1 on `660718af`)

**Found while re-landing §1.** On `660718af` the stack also turned
deeplyNestedMappedTypes from RIGHT to WRONG on diagnostics (TS2322 at 69,5 and 77,5
missing). TypeBox's `RequiredPropertyKeys<T> = keyof Omit<T,
ReadonlyOptionalPropertyKeys<T> | …>` evaluated to `never` instead of
native's `"level1"`. Each key alias is `{ [K in keyof T]: T[K] extends
TReadonly<TSchema> ? (T[K] extends TOptional<T[K]> ? K : never) : never
}[keyof T]`, and its template never built. Probed:

```ts
type M7<T> = { [K in keyof T]: T[K] extends number ? (T[K] extends string ? 2 : 1) : 0 };
declare const m7: M7<{ a: TString }>;  m7.a   // TSR: error; tsgo: 0
```

Without the parentheses the same template is `0`. The deferred
conditional in a mapped template is minted with its written text
(`declared.rs`, the §906 mint). `written_type_text` (`signatures.rs`)
declines a parenthesized non-union node, the mint fell to `(None, _) =>
error`, and `mapped_type_info` declined the whole mapped type. So the
`ReadonlyOptionalPropertyKeys<T1>` reference stayed unevaluated, and
`Exclude<"level1", …>` related `"level1"` to that unevaluated mint. On main
the outer `Omit` had no parts, which hid this. With the stack, `Omit`
captures `Pick`'s parts, so the wrong `never` reached the relation.

**Port.** `conditional_mint_text` (`declared.rs`) supplies the mint's text
when `written_type_text` declines a conditional. The node builder prints a
deferred conditional from its parts through `createConditionalTypeNode`,
which parenthesizes only the check type
(parenthesizeCheckTypeOfConditionalType) and the extends type
(parenthesizeExtendsTypeOfConditionalType). A branch is printed bare, so
its written parentheses are dropped, recursively for nested conditionals.
The check and extends types keep the written-text rules and still decline.
`signatures.rs` is not touched. Per mint, no table.

**Measured** (alone, unfiltered against `660718af`): types +1
(conditionalTypeAssignabilityWhenDeferred:0:94 WRONG → RIGHT), 0 lost;
diagnostics 0 / 0.

## 6. `.1266` re-measured on `660718af`: two more prerequisites, one outside this lane, and a relater blocker

The four cherry-picks rebase cleanly onto `660718af` (refreshed
[`r7-declared-HELD-mapped-stack.diff`](r7-declared-HELD-mapped-stack.diff)).
Measured unfiltered against `660718af` with r7-calls' strip on main: types
+59 / −2 (destructuringUnspreadableIntoRest:0:23, :0:25), diagnostics −1
(deeplyNestedMappedTypes). Neither loss is the stack's own error.

**destructuringUnspreadableIntoRest: the receiver's `this` substitution
(`members.rs`, r7-shared).** `rest1 : Omit<this, "getter" | "method" |
"setter">`. With the stack, `Omit` carries `Pick`'s mapped parts, so
`rest1.publicProp` reads the template `this["publicProp"]`, native's print.
Then `access_member_lookup`'s §164/§165 arm replaced the minted `this` with
the receiver, giving `Omit<this, …>["publicProp"]`. Native substitutes a
this-argument only through `getTypeWithThisArgument` on a class or interface
reference. resolveMappedTypeMembers (`checker.go:20894`) instantiates a
template with the key alone, so the `this` stays as written. On main the
same line was RIGHT through `property_type_via_shape`'s `Omit` arm, which
the mapped parts now pre-empt.
[`r7-declared-members-mapped-receiver-this.diff`](r7-declared-members-mapped-receiver-this.diff)
skips the substitution for a receiver in `mapped_types`.

**deeplyNestedMappedTypes: the template never built.** That is §5,
committed (`e291ff89`). It turned the case's TypeBox types native
(deeplyNestedMappedTypes +11 type lines: `{ level1: { level2: { foo: string; }; }; }[]`
where main printed `PropertiesReduce<…>[]`).

**Stack + §5 + the `members.rs` diff**, against §5's freeze: types **+69,
0 lost** (mappedTypeIndexedAccessConstraint 39, mappedTypeGenericIndexedAccess
11, deeplyNestedMappedTypes 11, reverseMappedPartiallyInferableTypes 4,
declarationQuotedMembers 3, thislessFunctionsNotContextSensitive1 1). It
still loses **one diagnostics case**: deeplyNestedMappedTypes'
TS2322 at (69,5) and (77,5) (`Input[]` to `Output[]`). On main these were
RIGHT by coincidence over the unevaluated `PropertiesReduce<…>` types.

**The blocker is in the relater.** A trace of `is_related_to` on a cut-down
file (`declare const i: Input; const o2: Output = i;`) shows the top-level
`{ level1: { level2: { foo: string; }; }; } :: { level1: { level2: { foo:
string; bar: string; }; }; }` answering `Unknown` with no nested walk.
`cached_object_relation` holds a `ComplexityOverflow`/`StackDepthOverflow`
entry for the pair, recorded by an earlier nested walk through
`TObject<…>`'s `static: PropertiesReduce<T, this['params']>`. `Unknown`
reports nothing. Native reports TS2322 with the elaboration, so its walk
neither overflows nor caches an overflow: isDeeplyNestedType's recursion
identities stop it first. The result also depends on statement order: with
more statements after the assignment, the same `o2 = i` reports. That is
the cached-overflow signature. Owner: `relater.rs` (r7-reports), the
deeply-nested detection and recursion identities for these evaluated
mapped types. Not attempted here.

`.1266` stays held. It lands once the overflow is fixed, together with the
`members.rs` diff.

## 7. A self-mentioned type-literal alias's placeholder gets its members (prerequisite of §8)

**Forcing constraint.** `get_declared_type_of_type_alias`'s §29 arm answers
a mention of an alias inside a construct native resolves lazily with a NAME
placeholder, `Named { text: "F", members: None }`. For

```ts
type F = { kind: 'foo'; children: N };  type B = { kind: 'bar' };  type N = F | B;
```

`N`'s union is built while `F`'s literal is printed, so its constituent is
the placeholder. `F`'s declared type is published afterwards as `Named {
text: "F", members: Some(literal) }`, and nothing ever reached the
placeholder again. Native creates the literal's type once
(getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode, `checker.go:24210`)
and resolves its members lazily, so `N`'s constituent is `F` itself. A
`B :: F` relation inside `N` answered `Unknown` from the member-less side
(traced: `src=Named B members Some, tgt=Named F members None`). So
`isNode(d)` narrowing `B | { kind: "document" }` by `N` stayed whole, the
loss r6-typesroots §9 held the slice gate on.

**Port.** `complete_alias_placeholder`: once the declared type is published,
a placeholder minted for the alias is completed in place with it
(`TypeStore::complete_object`). Same printed name, same members symbol, so
every place that stored it reads the declared type. Only a
`Named` with the body literal's members symbol is copied: its members live
in the binder table, so the copy needs no side-table entry keyed by the
declared type's own id. Checker port convention: key the alias symbol
(`alias_placeholders`, owner `get_declared_type_of_type_alias`);
publication: written once, when the declared type completes; no new table;
receiver/alias context unchanged. The twin keeps a distinct `TypeId` from
the declared type, where native has one identity: identity-keyed caches
see two types with one structure.

**Measured** (alone, unfiltered against §5's freeze): types 0 / 0,
diagnostics 0 / 0. It only matters once §8 lets these unions exist. Probe:
`isNode(d)` now narrows to `B` (tsgo: `B`).

## 8. ORIGIN-SLICE-GATE: a non-union object enters a union's origin

**Forcing constraint.** getUnionTypeWorker (`checker.go:25705-25728`)
builds a denormalized origin whenever named unions meet other
constituents without overlap. The origin's entries are the named unions
plus every other reduced constituent, objects included.
`build_origin_union`'s §53 slice gate (`unions.rs`) answered `error` when an
entry was a non-union object. So `type CC = C1 | C2; declare const y: CC |
C3` was `error` (tsgo: `C3 | CC`). r7-contextual traced about 25 jsTyping
TS2345 to it: `ReadonlyArray<CC>.concat` and `T | CA<T>` at `T := CC` fail
to instantiate.

**Port.** r6-typesroots' held one-line diff: an object entry is admitted as
a plain entry. Its two losses (subtypeReductionUnionConstraints 0:23/0:32)
were the placeholder §7 completed.

**Measured** (unfiltered against §7's tree): types **+75 RIGHT, 0 lost**:
TypeGuardWithEnumUnion 16, tsxGenericAttributesType1 9,
checkJsxChildrenProperty3/4 7+7, subtypeReductionUnionConstraints 6,
iterableWithNeverAsUnionMember 6, checkJsxChildrenProperty12 4,
tsxGenericAttributesType2 3, stableTypeOrdering 3, and 10 more lines across
7 cases. Diagnostics 0 / 0.

On `src/jsTyping` (TSR vs `660718af`, native reports only one TS2688):
- TS2345 74 → 32, TS2339 32 → 1, TS2678 15 → 0, TS2769 6 → 4,
  TS2740 3 → 0, TS2454 2 → 0, TS2352 3 → 1, TS2367 1 → 0;
- TS2322 12 → 17. Those five are new reports on now-typed code, not
  examined here.
