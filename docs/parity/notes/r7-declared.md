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

**Corrected (later in the round):** r7-reports found the real cause of the
deeplyNestedMappedTypes loss. It is not a cached overflow. It is the
relater's NoMembersTable fall-through plus isDeeplyNestedType's
instantiation-order count, which r7-reports fixes with a
`MappedTypeInfo.evaluation_start` field (stack + fix measured +83 / 0 by
r7-reports). The paragraph below records the first, wrong reading.

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

## 9. TYPE-ALIAS-DECLARED-BODY (part): an own indexed-access body resolves without the alias

**Forcing constraint.** `create_type_reference`'s indexed arm evaluated an
alias whose body is (or reaches) an indexed access. It then re-minted a
signature-typed result under the alias's name (`alias_named_signature_types`,
from `5fc064d4`'s "function-alias rendering"). Native separates two cases.
- An alias whose **own** body is the indexed access declares the resolved
  property type: getIndexedAccessTypeEx is handed the alias, but its
  resolved arm returns the property type as declared. getTypeAliasInstantiation
  then instantiates that type with no new alias. So `type BivariantHack<I, O>
  = { foo(x: I): O }["foo"]` at `<Animal, Animal>` is `(x: Animal) => Animal`
  (strictFunctionTypesErrors ×10). The same holds for `type Cb<T> = {
  noAlias: () => T }["noAlias"]` (nestedCallbackErrorNotFlattened ×5).
- An alias whose body is a **reference** to such an alias gets the outer
  alias as `newAlias`, and instantiateTypeWithAlias gives it to the
  anonymous object instantiation. So `type MouseEventHandler<T> =
  EventHandler<MouseEvent<T>>` prints `MouseEventHandler<any>`
  (jsxComplexSignatureHasApplicabilityError:0:163, which the first cut lost
  when it dropped the naming for both).

**Port.** The signature re-mint applies only when the body is not itself an
`IndexedAccessTypeNode` (`own_indexed`). No new state.

**Measured** (unfiltered against §8's tree): types **+18, 0 lost**
(strictFunctionTypesErrors 10, nestedCallbackErrorNotFlattened 5,
stringMappingReduction 3); diagnostics 0 / 0. The first cut (`!indexed`)
measured +18 / −1, the `MouseEventHandler` line.

**Remains.** A parameter written `cb: BivariantHack<Dog, Animal>` still
prints the alias. The type is right; the signature printer reuses the
written annotation. Native's node reuse rejects an alias reference whose
type does not carry that alias. Owner: `node_reuse.rs` (r7-printer).

## 10. TYPE-ALIAS-DECLARED-BODY (part): a union-bodied alias instance is a union

**Forcing constraint.** getTypeAliasInstantiation (`checker.go:23641`)
instantiates a union declared type through instantiateTypeWithAlias. That
maps the constituents and hands the alias to getUnionType, so `R<number>`
over `type R<T> = string | ((i: T) => any)` is `string | ((i: number) =>
any)`, printed `R<number>`. `create_type_reference` evaluated a union body
only in the closed-literal slice (`is_closed_literal_union_alias`). Every
other union alias instance was a print-only `Named` with no constituents,
so contextual typing through it found nothing: `const r: R<number> = w =>
w` typed `w : any`. React's `Ref<T> = string | ((instance: T) => any)` is
that shape, so every `ref={x => …}` on a class component was untyped
(tsxStatelessFunctionComponents2, tsxSpreadAttributesResolution4,
tsxAttributeResolution15).

**Port.**
- `is_union_alias_body` admits a union body at full arity whose
  constituents are keywords, literals, or function or constructor types.
  The existing union arm builds `create_union(…, Some((alias, text)))` with
  its `type_reference_targets` entry.
- `evaluate_union_or_indexed_alias_body` guards the instantiation per alias
  symbol (`instantiation_expressions.union_alias_in_progress`: key the
  alias symbol; inserted before the body is evaluated, removed after; never
  published). A re-entrant reference keeps the named mint, which is where
  native reaches only through a lazily resolved member.

**A slip, recorded.** The first build wrote `&& let union_body = !indexed
&& self.is_union_alias_body(…)` inside a let-chain. The second `&&` parsed
as a chain separator, so every indexed alias stopped reaching the arm:
−206 types, BivariantHack's §9 lines back to the alias name. The
parenthesized form is the fix. Both earlier measurements (−165 broad, −206
narrow) were of that bug, not of the port.

**Measured** (unfiltered, against `f853238f` on `0aa00136`):
- **this slice: types +32, 0 lost**; diagnostics +1
  (coAndContraVariantInferences6 WRONG → RIGHT), 0 lost.
  tsxStatelessFunctionComponents2 19, tsxSpreadAttributesResolution4 6,
  tsxAttributeResolution15 5, coAndContraVariantInferences6 2.
- every union body, held: types +79 / −13, diagnostics +4 / −1. Losses:
  discriminateWithOptionalProperty2 ×6, recursiveGenericUnionType1/2 ×4,
  typeGuardsAsAssertions ×2, jsxComplexSignatureHasApplicabilityError ×1,
  unionAndIntersectionInference3 (EMPTY_RIGHT → EMPTY_WRONG). Extra gains
  over the slice: assignmentTypeNarrowing 9, jsxComplexSignatureHasApplicabilityError
  8, unionAndIntersectionInference1 4, typePredicateFreshLiteralWidening 4,
  discriminateWithOptionalProperty3 ×2 8, for-of58 3, awaitedType(StrictNull)
  6, and more. Not yet analysed.

### 10.1 The union arm of the new-alias instantiation, and what holds the wider slice

`new_alias_union_instantiation` is `new_alias_intersection_instantiation`'s
union twin. getTypeFromTypeAliasReference's `newAliasSymbol` reaches
getUnionType, so `type X = R<number>` declares the union printed `X`, and
`type IStringContainer = Container<string>` declares the union printed
`IStringContainer`. The target's aliased union is re-created under the
declaring alias's name, cached in `deferred_alias_references[(alias,
canonical)]`. Alone it measures 0 / 0 (no corpus line has the slice's shape
under a declaring alias). With every union body admitted it removes the
recursiveGenericUnionType1/2 losses: that wide slice goes from +79 / −13 to
**+47 / −9 types and +3 / −1 diagnostics** over §10's slice.

Each of the nine remaining losses is a reader outside this lane that drops
a union's alias identity:
- **typeGuardsAsAssertions 0:23/0:31** (`Optional<r>` printed `None |
  Some<r>`): native getUnionOrEvolvingArrayType (`flow.go:1314`) returns
  the declared type when the junction's union has the same constituents
  (`slices.Equal(result.types, declaredType.types)`). TSR's
  `union_or_evolving_array` (`flow.rs`, r7-flow) lacks that arm.
- **discriminateWithOptionalProperty2 ×6** (`Promise<IteratorResult<U, R>>`
  printed with its constituents): the async return's awaited type maps the
  aliased union and rebuilds it. Native mapType returns the union itself
  when no constituent changed. Owner: the awaited-type reader
  (`expressions.rs`, main).
- **jsxComplexSignatureHasApplicabilityError 0:52**
  (`HandlerRendererResult` printed `false | JSX.Element | null`): the
  same unchanged-union rebuild on another reader, not yet located.
- **unionAndIntersectionInference3** (diagnostics EMPTY_RIGHT →
  EMPTY_WRONG, false TS2345 at 77 and 87): inference into `C &
  ComponentType<P>` and `T & AB<U>` now that the alias instances are
  unions. Owner: `inference.rs`.

## 11. TYPE-ALIAS-INSTANTIATION-NEW-ALIAS (part): the new alias follows the declared type's flags

**Forcing constraint.** instantiateTypeWorker (`checker.go:22220`) picks its
arm by the declared type's flags. A declared type that is a union reaches
getUnionTypeEx with the new alias whatever its syntax. `type T02<T> = keyof
(T & B)` declares `"b" | keyof T`, so `type T05 = T02<A>` declares `"a" |
"b"` printed `T05` (keyofIntersection T05/T06/T07, Result1, Result5).
`new_alias_union_instantiation` (§10.1) gated on a `UnionTypeNode` body,
the node-versus-type split r6-declared3 §2 recorded for `.1184`.

**Port.** An unaliased union result whose target's declared type
(`get_declared_type_of_symbol`) is a union is re-named with the declaring
alias through the existing `get_named_union_type`, as the indexed arm above
it already does. The alias's own union keeps the `type_reference_targets`
road. No new state.

**Measured** (unfiltered against `8d4c10cc` on `86d9406e`): types **+5,
0 lost** (keyofIntersection 0:8/9/10/12/19); diagnostics 0 / 0. jsTyping
gate: new_false 0, lost_true 0.

## 12. unwrapAwaitedType keeps an unchanged union (granted: `expressions.rs`, that function only)

**Forcing constraint.** unwrapAwaitedType (`checker.go:31440`) maps a union
through mapType, which answers the union itself when no constituent
changed. `unwrap_awaited_type` always rebuilt the union. Once §10's union
alias instances exist, an async function returning `IteratorResult<U, R>`
printed `Promise<IteratorReturnResult<R> | IteratorYieldResult<U>>`
(discriminateWithOptionalProperty2 ×6, a wide-slice loss in §10.1). The
integrator granted this one function.

**Port.** If every constituent comes back identical, return the union. No
state.

**Measured** (alone, unfiltered against §11's tree): types 0 / 0,
diagnostics 0 / 0, jsTyping new_false 0 / lost_true 0. With §10's wide
slice it removes the six discriminateWithOptionalProperty2 losses (probed:
`async function g() { return ir; }` is `() =>
Promise<IteratorResult<number, string>>`, as in tsgo). The wide slice's
remaining losses are typeGuardsAsAssertions ×2 (r7-flow),
unionAndIntersectionInference3 (r7-contextual) and
jsxComplexSignatureHasApplicabilityError 0:52 (`HandlerRendererResult`'s
own declaration prints its constituents). A cut-down file with
`React.ComponentType` does not reproduce 0:52, so its reader is not
located yet.

## 13. CONDITIONAL-INLINE-NODE-INSTANTIATION (part): a deferred conditional with a function-type operand has a print

**Forcing constraint.** An inline conditional that does not evaluate is
minted with its written text (the §906 mint). `written_type_text`
(`signatures.rs`) has no arm for a function or constructor type node, so a
conditional whose extends type is one had no text and minted `error`:
`declare function useState1<S>(initialState: (S extends (() => any) ? never
: S) | (() => S)): S` made `useState1` itself `error`, and every call to it
(conditionalTypeBasedContextualTypeReturnTypeWidening). The node builder
prints a deferred conditional through createConditionalTypeNode over its
typed parts. It parenthesizes the check type when that is a function,
constructor or conditional type, and the extends type only when it is a
conditional.

**Port.**
- `conditional_mint_text`'s operands follow those two rules for a
  parenthesized operand.
- `conditional_typed_mint_text`, the last resort, prints the check and
  extends **types** (resolved under the current frames, as the evaluator
  already resolves them) with those rules, and the branches with their
  written text. It answers only for a conditional getConditionalType
  defers (`is_deferred_type` on either operand).

**Refused, with the numbers.**
- A first cut resolved the branch types too: −5 types, and awaitedType ran
  10 s per case. Resolving `Awaited<V>` in a branch at mint expands what
  native resolves lazily (getTrueTypeFromConditionalType).
- A second cut without the deferral gate: +28 / −5. awaitedType 0:27/0:34
  (×2 targets) and infiniteConstraints 0:61 printed a deferred conditional
  over a concrete check, where native evaluates (or answers errorType at its
  instantiation guard). Those keep the gap.

**Measured** (unfiltered against `a1e71942`): types **+20, 0 lost**
(conditionalTypeBasedContextualTypeReturnTypeWidening 12,
inferTypesWithExtends1 3, inferTypes2 2,
arrayFakeFlatNoCrashInferenceDeclarations 2, mappedTypeAsClauses 1);
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0. The case's
remaining 16 lines are the call results' literal widening under a
conditional contextual type (`() => 0` printed `() => number`), which is
getReturnTypeFromBody's contextual literal test, not this mint.

## 14. A type reference through an alias whose target merges an alias with a value (exportNamespace9 3:2)

**Forcing constraint.** In `/c.ts`, `import { A } from "./b"; const A = 1;
export { A }` merges an import alias with a const. `/d.ts`'s `import { A }
from "./c"` therefore resolves (resolveAlias) to that merged symbol and
stops there: it is no pure alias (`isNonLocalAlias` excludes a symbol with
a value meaning). resolveName admitted `type _ = A` because getSymbolFlags
accumulates the chain's flags. getTypeReferenceType then reads
tryGetDeclaredTypeOfSymbol, whose alias arm is getDeclaredTypeOfAlias
(`checker.go:14523`): the declared type of the merged symbol's own alias
target, `a.ts`'s `type A = number`. The alias road (`declared.rs`) followed
only pure aliases, found no TYPE meaning on the merged symbol, and answered
`error` (`_ : any` where tsgo prints `number`).

**Port.** After the pure-alias walk, a target with no TYPE meaning that
still carries ALIAS is followed through its own alias
(`resolve_alias`), up to the same 8 steps. No state.

**Measured** (unfiltered against `77418645`): types **+1, 0 lost**
(exportNamespace9:3:2); diagnostics 0 / 0; jsTyping new_false 0 /
lost_true 0.

## 15. A variadic or rest tuple's Array base members (granted: `members.rs`, the Array-base fallback only)

**Forcing constraint.** getTupleBaseType (`checker.go:19206`) gives every
tuple target the base `Array<E>` (`ReadonlyArray<E>` when readonly), with
`E` the union of its element type arguments, a variadic element
contributing `T[number]`. `get_type_of_property_with_this_argument`'s
Array-base fallback handled only fixed tuples (`tuple_element_lists`). A rest
tuple (`variadic_tuple_elements`) never reached `Array<E>`, so `.slice` on
`[string, ...any[]]` was `error` (mappedTypesGenericTuples2 0:20-27 once
r7-contextual's Promise.all boundary types the receiver). The integrator
granted this one fallback.

**Port.** The same fallback with `E = variadic_tuple_index_union(id)`
(`tuples.rs`, which computes getTupleBaseType's element union for these
tuples already). `.length` keeps its own arm.

**Measured** (alone, unfiltered against `77418645`): types **+3, 0 lost**
(variadicTuples2); diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0.
Probe: `[string, ...any[]]`'s `slice(1)` is `any[]`, and `[string,
...number[]]`'s is `(string | number)[]`, as in tsgo.

## 16. The `keyof X` origin keeps its operand (for r7-printer's site arm)

**Forcing constraint.** getLiteralTypeFromProperties attaches `origin =
newIndexType(t)` to a resolved key union. The node builder prints that
origin's Index arm (`nodebuilderimpl.go:3472`) from the operand's own node at
the print site, so the operand is qualified where the site needs it (`keyof
Intl.NumberFormatOptionsStyleRegistry`). This port baked only the text
`keyof <operand printed at creation>` (`union_with_origin_text`), so a site
arm had nothing to print from (intlNumberFormatES2020/ES2023,
es2018IntlAPIs, formatToPartsFractionalSecond: 10 lines, r7-printer's
naming re-triage).

**Port.** `union_with_keyof_origin` (`unions.rs`) builds the same text and
records the operand beside it. Checker port convention: key the
origin-carrying union (a distinct type from the bare union, minted there);
owner `instantiation_expressions.keyof_origin_operands`; written once at
mint, never invalidated; no receiver or alias context; no extra work. The
three `keyof` origin sites in `declared.rs` (the deferred-keyof
evaluation, the tuple key union, the index-key union) call it. The print-time
arm is r7-printer's.

**Measured** (unfiltered against the rebased `5e8714ed`): types 0 / 0,
diagnostics 0 / 0, jsTyping new_false 0 / lost_true 0, callgrind Ir gi
0.9999, dm 1.0000.

## 17. An alias-rooted qualified reference into an augmented module resolves (umd-augmentation-1)

**Forcing constraint.** `alias_rooted_reference_declines` (`declared.rs`)
had a module-augmentation arm. Any alias-rooted qualified type reference
(`Math2d.Point` through `import * as Math2d from 'math2d'`) whose own file
augments the alias's module was kept as the unresolved written-text mint,
because the binder did not run mergeModuleAugmentation (`checker.go:1407`)
and the augmented export table was incomplete. Its doc said to delete the
arm when that landed, and named the falsifier. The binder now has
`merge_module_augmentations` (tsr-binder). So the arm only produced the
mint: in umd-augmentation-1, `reverse(): Math2d.Point` inside `declare
module 'math2d' { interface Vector { … } }` was `Named { "Math2d.Point",
members: None }`, and `v.reverse()` printed `Math2d.Point` where tsgo prints
`m.Point` (r7-printer's naming re-triage).

**Port.** The arm is removed; the generic-heritage arm stays.

**Measured** (unfiltered against `cd49eafb`): types **+28, 0 lost**; diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 1.0002, dm 1.0000. A side measurement
on `5e8714ed` the removal alone gave types +28 / 0 lost:
moduleAugmentationDoesInterfaceMergeOfReexport 12 and
moduleAugmentationDoesNamespaceMergeOfReexport 12 (the arm's own named
falsifiers), umd-augmentation-1 2, and
moduleAugmentationDoesNamespaceEnumMergeOfReexport 2. Diagnostics 0 / 0;
jsTyping new_false 0 / lost_true 0; Ir gi 1.0000, dm 1.0000.

## 18. HELD: a type alias's declared type is resolved outside alias-evaluation frames

`get_declared_type_of_type_alias` can be reached from inside another alias's
body evaluation, under `alias_evaluation_bindings`. Those frames switch off
the body's own alias naming, so in styledComponentsInstantiaionLimitNotReached
`type ReactNode = ReactChild | ReactFragment | …`, first resolved under
`ComponentType<P>`'s frame, was published without its alias, and every
later `React.ReactNode` printed its constituents. getDeclaredTypeOfTypeAlias
(`checker.go:23837`) resolves with no mapper. Setting the frames aside for
the resolution fixes the print, but alone it measures types +1 / 0 and
**diagnostics −1** (deeplyNestedMappedTypes: TypeBox's types change, and the
relation then hits the gap r7-reports is fixing with `evaluation_start`).
It is held until that fix is on main
(`/tmp/r/iso.diff` on the box; it is a two-function wrapper).

The wide union-alias slice (§10.1) on top of it measures types +51 / 0 and
diagnostics +4 / −1, with the same deeplyNestedMappedTypes loss. Its
ReactNode and HandlerRendererResult losses were this frame leak.

## 19. The alias-cycle gate narrowed to targets that can close a cycle (conformance/enums)

**Forcing constraint.** `qualified_type_reference`'s cycle gate answered
`error` for any alias-rooted qualified reference written as a type alias's
body while that alias's declared type was resolving. That is every such
body (`type TypeFlag = SymbolFlags.Type` through `import type { SymbolFlags
}`): conformance/enums printed `any` where tsgo prints `SymbolFlags.Type`.
The gate exists for circular4 (`export type T = ns2.nested.T` across two
importing files), where native reports the circularity and yields `any`. A
declared-type cycle can only run through type aliases, or aliases that reach
them: an enum member, class or interface target cannot close one.

**Port.** The resolved-target arm of the gate applies only when the target
(merged) is a type alias or an alias. The unresolved arm is unchanged.

**Measured** (unfiltered against `9b77d498`): types **+5, 0 lost**
(conformance/enums 4, checkerInitializationCrash 1; circular4 stays RIGHT);
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 1.0000, dm
1.0000.

## 20. A homomorphic mapping over a class drops its non-public members (routed from r7-reports)

**Forcing constraint.** resolveMappedTypeMembers enumerates the modifiers
type's properties through getLiteralTypeFromProperty (`checker.go:26746`),
which answers `never` for a private or protected member. So `Req<T> = { [K
in keyof T]-?: T[K] }` over `class PrivateState { private a; b }` has
only `b`, and `r.a` is TS2339 natively. TSR had two roads that kept `a`:
- `mapped_member_keys` (`mapped.rs`) took every property name;
- `instantiate_identity_mapped_alias`'s members-owner image (`declared.rs`)
  reads the source class's own table.

TSR therefore reported TS2341 ("private and only accessible within class")
where tsgo reports TS2339.

**Port.** `mapped_member_keys` skips a non-public property
(`is_non_public_member`), and the identity image declines a source with a
non-public member, so it takes the mapped road. No state.

**Measured** (unfiltered against `4af98855`): types 0 / 0, diagnostics
0 / 0 (no corpus case reads such a member); jsTyping new_false 0 /
lost_true 0; Ir gi 1.0001, dm 1.0002. Probe: `Req<PrivateState>`'s `a` is
TS2339, as in tsgo.

## 21. HELD: `keyof` of a non-generic object that mentions a type parameter resolves now

getIndexTypeEx (`checker.go:26684`) defers only under shouldDeferIndexType
(`:26834`). `keyof Dictionary<T>` over `interface Dictionary<T> { [key:
string]: T }` is `string | number` whatever `T` is, but TSR's keyof arm
deferred on any mentioned type parameter, so `Dictionary<T>[keyof
Dictionary<T>]` was `error`, and r7-reports' `m4(foos, f => f.foo)` typed
`f : any` (tsgo: `{ foo: string }`, result `Dictionary<string>`).
`keyof_operand_resolves_now` admits a plain object operand that is not
generic (`is_generic_type`). Measured alone against `4af98855`: types +2 / 0
(deeplyNestedMappedTypes 0:164, thislessFunctionsNotContextSensitive1 0:5),
**diagnostics −1: deeplyNestedMappedTypes**. That is again the TypeBox
relation that was RIGHT over wrong types, the one .1266's stack fixes. It is
held until .1266 lands, and will be re-measured then (`/tmp/r/keyof-now.diff`).

## 22. A generic alias over a class or interface reference is that reference (tsxStatelessFunctionComponentOverload5, routed from r7-reports)

**Forcing constraint.** `type MouseEventHandler<T> =
EventHandler<MouseEvent<T>>` over `interface EventHandler<E> { (event: E):
void }`: the body node has an alias symbol, so isDeferredTypeReferenceNode
makes it a deferred type reference created with the alias. Instantiating it
keeps the alias at the instantiated arguments: the instance is
`EventHandler<MouseEvent<any>>`, printed `MouseEventHandler<any>`. TSR
minted a print-only `Named` with the alias as its member owner, which has no
call signature. `onClick={(e) => …}` through `React.MouseEventHandler<any>`
typed `e : any`, and anyFunctionType against the alias was `Unknown`.

**Port.** `instantiate_interface_reference_alias` (`create_type_reference`):
- the body is evaluated under the alias's bindings;
- the resulting class/interface reference (a `Named` with the target's
  member owner and its `type_reference_targets` pair) is re-minted under
  the alias's printed name, with `alias_of` recording the alias.

Two declines, each with its reason:
- the global `Array`/`ReadonlyArray` targets. The site renderer prints their
  reference pair with the array shorthand before an alias (`type Options<T>
  = Array<Option<T>>` printed `Option<T>[]` inside another alias's
  arguments: −1, jsxComplexSignatureHasApplicabilityError:0:222);
- an alias whose body re-enters itself (`type InfBox<T> =
  Box<InfBox<T>>`). Native resolves that deferred reference lazily; eagerly
  it built a partial type, and `unbox(b3)` lost `InfBox<string>` (−1,
  recursiveConditionalTypes:0:83). A re-entry is recorded per alias
  (`interface_alias_reentered`) and keeps the named mint.

Checker port convention: the in-progress set (owner
`instantiation_expressions.interface_alias_in_progress`, inserted and
removed around the evaluation) and the re-entered set (written once, never
cleared) are keyed by alias symbol. The instance itself is cached in the
existing `instantiations[(alias, arguments)]`.

**Measured** (unfiltered against `4af98855`, the declines in place): types
**+7, 0 lost** (tsxStatelessFunctionComponentOverload5 4,
tsxReactPropsInferenceSucceedsOnIntersections 3); diagnostics 0 / 0;
jsTyping new_false 0 / lost_true 0; Ir gi 1.0000, dm 1.0001. The first cut
without the declines measured +7 / −2.

## 23. HELD: the non-generic type-literal alias body records `alias_of` (TYPE-ALIAS-ACCESSIBILITY-GATE (a), with r7-printer)

`type AX = { readonly A: unique symbol }` minted its type-literal object with
no `alias_of`, so the printer could not apply IsTypeSymbolAccessible to the
alias (getAliasSymbolForTypeNode gives the literal its alias).
[`r7-declared-HELD-type-literal-alias-of.diff`](r7-declared-HELD-type-literal-alias-of.diff)
records it in `get_type_from_type_literal`. Measured against `41d542bb`'s
base: types **+9 / 0**, diagnostics **−1**
(declarationEmitExpressionWithNonlocalPrivateUniqueSymbol loses TS2527).
With the alias recorded, the printer no longer expands AX down to the
inaccessible `unique symbol`. That diagnostic is what r7-printer's
IsTypeSymbolAccessible arm restores, so the two land together.

## 24. A qualified import type with type arguments instantiates its class or interface (IMPORT-TYPE-NODE, part)

**Forcing constraint.** getTypeFromImportTypeNode (`checker.go:24575`)
resolves the qualifier and calls getTypeReferenceType with the node's type
arguments: `import("./box").Box<{ boxed: import("./box").Box<number>; }>` is
`Box` at that argument. TSR answered `error` for any import type with type
arguments (declarationEmitTopLevelNodeFromCrossFile2,
importUsedInGenericImportResolves).

**Port.** `generic_import_type_reference` instantiates a class or interface
target at the written arguments (`create_type_reference`). It mints the
written import spelling with the printed arguments, the target's member
owner and its reference pair, as the argument-less arm mints the written
spelling over the target's members. The cache is the existing
`qualified_reference_types[(text, symbol)]`. Declines, each keeping
`error`:
- a type alias target;
- an arity mismatch;
- **an argument that mentions a type parameter**. The baked argument text
  would survive instantiation while the pair instantiates. The first cut
  without this decline measured +8 / −6
  (declarationEmitIsolatedDeclarationErrorNotEmittedForNonEmittedFile:
  `initTRPC.create()`'s members read `any` where they were `{}`).

**Measured** (unfiltered against `41d542bb`): types **+6, 0 lost**
(declarationEmitTopLevelNodeFromCrossFile2 5,
importUsedInGenericImportResolves 1); diagnostics 0 / 0; jsTyping
new_false 0 / lost_true 0; Ir gi 0.9999, dm 1.0000. importTypeGenericTypes'
`import("./foo2").Bar.I<{ x: number; }>` now prints `Bar.I<…>` (GAP →
WRONG): the site renderer names the reference by its chain. That is
r7-printer's symbol-chain arm.

## 25. An indexed access over an unresolved reference answers the reference (TYPEREF-UNRESOLVED-ALIAS-TARGET-SYMBOL, part)

**Forcing constraint.** getPropertyTypeForIndexType (`checker.go:27084`)
answers an `any` object itself. An unresolved type reference is native's
errorType carrying the written alias (getUnresolvedSymbolForEntityName), and
IsTypeAny holds for it, so `C1.Red["toString"]` (where `C1` is a type
alias, not a namespace) is `C1.Red` (errorForUsingPropertyOfTypeAsType03
c2/d2). TSR's indexed-access type-node arm answered `error`.

**Port.** In that arm, a non-gap unresolved mint (`unresolved_types`, not
`is_gap`: the OBJECT-flagged deferred mints are the port's own gaps and stay
declined) answers itself, unless the index is `never`. No state.

**Measured** (unfiltered against `b414e1af`): types **+2, 0 lost**
(errorForUsingPropertyOfTypeAsType03 0:19, 0:25); diagnostics 0 / 0;
jsTyping new_false 0 / lost_true 0; Ir gi 1.0001, dm 1.0001.

**Not taken, with cause:** importDeclWithClassModifiers / importDeclWithDeclareModifier
(`var b: a` through `import a = x.c` of an unexported member: native `a`).
Answering the written reference moves the merged `import b` declaration's
own line, which native prints as the alias's errorType (`any`) while TSR
prints the merged var's type. That line is `getTypeOfAlias`'s, owned by
`symbols.rs`.

## 26. `keyof` over a lone string index is stringOrNumberType, with no origin

**Forcing constraint.** getLiteralTypeFromProperties (`checker.go:26717`)
lists one entry per property and per included index info, and a string index
contributes the single `stringOrNumberType` entry. getUnionTypeEx
(`checker.go:25632`) answers a one-entry list with that entry and never
reads the origin, so `keyof T1` over `type T1 = { [key: string]: X }` is
`string | number` (tsgo: `Type 'string | number' is not assignable…` for
`c10<T1>` with `type c10<Tg> = keyof Tg`). TSR pushed `string` and `number`
as two keys and attached the `keyof T1` origin to the resulting union, which
printed `keyof T1`.

**Port.** `resolved_keyof_type_worker` counts native's list (a string index
counts once) and returns the bare union when the count is one. No state.

**Measured** alone (unfiltered against `cf8bae56`): types 0 / 0, diagnostics
0 / 0, jsTyping 0 / 0, Ir 1.0000 / 1.0000. No corpus row prints this, so the
port is recorded for the probe (`keyof c18<T1>` style reads), not for a
number.

## 27. A generic alias over a homomorphic mapped alias reference is the inner instance

**Forcing constraint.** `type ObjMapReadOnly<T> = Readonly<ObjMap<T>>` at
`Resolver` records `Readonly<{ [key: string]: Readonly<Resolver>; }>`
(inferrenceInfiniteLoopWithSubtyping), and `type Q<T> = Partial<T>` at
`{ a: number }` is `Partial<{ a: number; }>`. getTypeAliasInstantiation
instantiates Q's declared type, which is Partial's homomorphic mapped type,
and instantiateMappedType maps a non-union homomorphic image through
instantiateConstituent with a nil alias, so the instance keeps the inner
alias (`alias_instantiation_keeps_declared_alias` already names which
declared types keep theirs). TSR minted a print-only `Named` over the outer
alias, which printed `Q<…>` and, for inference, was an opaque reference.

**Port.** `instantiate_homomorphic_reference_alias`: a generic alias whose
body is a reference to a generic mapped alias, where the outer alias does not
receive a new alias and keeps the declared one, answers the evaluated body
when that is the inner alias's mapped instance. A union image (mapTypeWithAlias
hands the outer alias to the union) is not the inner instance and keeps the
existing mint. No new state: the answer is cached in `instantiations` like
every other arm of `create_type_reference`.

**Measured** with §26 (unfiltered against the rebased tip `d1120e60`): types
**+11, 0 lost** (reverseMappedPartiallyInferableTypes 6, mappedTypeWithAny 3,
inferrenceInfiniteLoopWithSubtyping 1, ramdaToolsNoInfinite 1); diagnostics
0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 1.0000, dm 1.0002.

**Refused, with the number that refused it: the non-homomorphic image.** The
same arm extended to an outer alias that RECEIVES the new alias (`type c18<T>
= c17<T, string>` over `type c17<T, K extends keyof T> = { [P in K]: T[P] }`;
`Omit` over `Pick`), re-minting the inner mapped instance under the outer
name with its mapped parts, measured **+13 / −73 types and −5 diagnostic
cases** (first cut, reference pair on the inner alias), and after moving the
pair to the outer alias the Omit losses remained in two shapes:
`Omit<this, …>["publicProp"]` (the mapped road cannot enumerate a generic
`Exclude<keyof this, K>` constraint; native reads getLowerBoundOfKeyType) and
`bar.c : error` for a key Omit removed (the mapped road leaves a missing
property a gap where `members.rs`'s Omit arm answered). The port that would
win is getLowerBoundOfKeyType in `mapped_member_keys` plus the missing-key
answer on the mapped road; until both exist the outer alias keeps the
existing mint.

**Routed: the index road of an alias reference.** What `c18<T1>["x"]`
actually lacks is an alias arm in `index_signatures.rs::get_index_infos_of_type`
(the twin of `members.rs::property_type_via_shape`'s alias arm: a `Named`
whose owner is a TYPE_ALIAS answers its evaluated body's index infos).
Prototyped (`r7-declared-ROUTED-index-alias-arm.diff`) against `cf8bae56`+§26: types
**+10 / 0** (intersectionWithIndexSignatures' `q["asd"]` among them), but
diagnostics **−2**: conformance/unionTypeInference gains a TS2322 at 62:11
(`DeepPromised<T>` to `DeepPromised<{ [name: string]: … }>`, whose target now
has its string index; the relation that native passes there is the
relater's), and compiler/contextuallyTypedJsxAttribute2 turns its `onClick`
gap into `(e: any) => void` and reports TS7006 (the JSX attributes'
contextual type through `Omit<ComponentPropsWithRef<…>, "as">`). Not owned
here; offered to the integration owner with both causes.

## 28. A definite generic conditional alias declares its branch; the infer arm defers on isDeferredType

**Forcing constraint.** getConditionalType (`checker.go:24377`, `:24415`)
attaches the alias only to a deferred result: a definitely false or true
check answers `instantiateType(branch, mapper)` with no alias. So a generic
alias whose conditional body is not deferred declares its branch:
`type P<V> = ((h: V, b: string) => void) extends (...a: infer R) => void ?
R : any` is `[h: V, b: string]`, and `type T07<T extends any[]> =
Parameters<(...args: T) => void>` is `T` (genericRestParameters1), because
getTypeFromTypeAliasReference hands the declaring alias to
getConditionalTypeInstantiation and the same rule drops it. TSR minted the
alias name for every generic conditional alias, and its infer arm declined
any check that mentioned a type parameter (`(h: V) => void`), which native
does not defer (the check is not generic, isDeferredType).

**Port.**
- `evaluate_conditional_inference` no longer declines a check that merely
  mentions a type parameter: the caller already deferred a generic one. When
  the check or the inferred extends type mentions a parameter, a deferred
  inferred extends type defers, and the branch is chosen by
  `definite_conditional_outcome` (permissive instantiations not related:
  false; restrictive related: true; otherwise deferred), the §3 relation.
  Without a mention the plain relation stands, which is the same three
  outcomes.
- `get_declared_type_of_type_alias`: a conditional body
  (`DeclaredBody::Conditional`) publishes its evaluated branch when neither
  operand defers (`conditional_alias_check_is_deferred`) and the answer is
  not the written deferred mint; a body that references a conditional alias
  (`DeclaredBody::ConditionalReference`) publishes the reference's answer
  when that is not the target's deferred reference or mint.

No new state.
- Containment: `signature_rest_relation_undecided`. compareSignaturesRelated
  compares a non-array rest type (getNonArrayRestType: `(...t: T) => any`)
  against the other signature's rest at the shorter count, so `Tail<T> =
  ((...t: T) => any) extends ((_: any, ...tail: infer TT) => any) ? TT : []`
  relates `T` to `[_: any, ...tail: T[number][]]`, fails restrictively, and
  stays deferred (ramdaToolsNoInfinite `>Tail : Tail<T>`). The relater here
  pairs the positions and relates them, so such a check over a different
  parameter count is not decided here. Without this, `Last<Tail<T>>` turned
  into a gap and TS2502 at Last (ramdaToolsNoInfinite: 1 type lost, 1
  diagnostic case lost). Routed: the relater's non-array-rest arm.
- A first cut published a conditional-reference body when the reference's
  answer was not the target's deferred mint: −3 (ramdaToolsNoInfinite2
  `_Reverse`, `GapsOf`, `_ListOf`), where the reference's check
  (`__Reverse<…>`, a generic indexed-access alias) defers natively and this
  port answered an unresolved `Cast`. The arm now asks
  `conditional_reference_resolves` (the written arguments, then
  `conditional_alias_check_is_deferred`) and declines on any error answer.

**Measured** (unfiltered against `488e514e`): types **+20, 0 lost**
(inferenceContextualReturnTypeUnion2 6, genericRestParameters1 4,
strictBindCallApply1 3, arrayFlatNoCrashInference 2,
arrayFlatNoCrashInferenceDeclarations 2, conditionalTypeGenericInSignatureTypeParameterConstraint
1, deferredConditionalTypes 1, ramdaToolsNoInfinite2 1); diagnostics **+1
case, 0 lost** (tsxNotUsingApparentTypeOfSFC); jsTyping new_false 0 /
lost_true 0; Ir gi 1.0000, dm 1.0000.

**Not reached:** `type Prepend<V, T extends any[]> = ((head: V, ...args: T)
=> void) extends (...args: infer R) => void ? R : any` still declares
`Prepend<V, T>` (native `[head: V, ...args: T]`): the containment above
declines it (2 parameters against 1), and inferring `R` from a generic rest
is `inference.rs`'s.

## 29. A non-generic alias over a generic class/interface-reference alias carries its own name

**Forcing constraint.** `type GenericSettingsResult1 =
GenericSettingsAccess<"audio">` over `type GenericSettingsAccess<T extends
keyof SettingsTypes> = Settings<SettingsTypes[T]>` records
`>GenericSettingsResult1 : GenericSettingsResult1` (mappedTypeRelationships
0:426/427). GenericSettingsAccess's declared type is a deferred type
reference carrying its alias (§22), and getTypeFromTypeAliasReference
instantiates it with the new alias, which getObjectTypeInstantiation attaches.
`new_alias_instantiation` declined any result that already carried
`alias_of`, so §22's image kept the inner alias's name
(`GenericSettingsAccess<"audio">`).

**Port.** `new_alias_reference_instantiation`: a result whose `alias_of`
names the referenced generic alias and whose reference pair targets a class
or interface (§22's image) is re-minted under the new alias, keeping the
member owner, reference pair and signatures, with `alias_of` recording the
new alias. Cache: the existing `deferred_alias_references[(alias, result)]`.

**Measured** (unfiltered against `fb7388a3`): types **+2, 0 lost**
(mappedTypeRelationships 0:426, 0:427); diagnostics 0 / 0; jsTyping
new_false 0 / lost_true 0; Ir gi 1.0000, dm 1.0000.

## 30. Refused: a union-bodied alias instance that reduces to one type

getUnionTypeEx answers a set that reduced to one type with that type, before
the alias is considered, so `type Narrow<A> = (A extends Narrowable ? A :
never) | { [K in keyof A]: Narrow<A[K]> }` at `{ value: "1" }` is `{ value:
"1"; }` and `type U<A> = (A extends string ? A : never) | { k: A }` at
`number` is `{ k: number; }`. TSR prints `Narrow<{ value: "1"; }>` and
`U<number>`: `is_union_alias_body` admits only keyword, literal, function and
constructor constituents (§10).

A `reduced_union_alias_instance` arm evaluated any other union body and took
a single, non-gap answer, guarded by an `(alias, arguments)` stack (the
per-alias guard of §10 kept the nested `Narrow<"1">` on the mint; the keyed
stack lets it resolve, bounded at eight nestings). Measured with §29 against
`fb7388a3`: types +2 (§29's two), **diagnostics −1**
(typePredicateFreshLiteralWidening gains TS2322 at 20:35, where TSR's
inference answers `TNarrow = { value: string }` and the reduced `Narrow<…>`
now relates structurally), Ir dm **1.0018**. No type row gained: the
corpus's `Narrow` rows also need the inference (`satisfies<Item>()({ value:
"1" })` infers `{ value: "1" }` natively, an `inference.rs` question), so
the arm was not taken.

## 31. A mapped property's type includes undefined when the property is optional

**Forcing constraint.** getTypeOfMappedSymbol (`checker.go:20984`) adds
undefined to an optional property's instantiated template under
strictNullChecks (getOptionalType, isProperty) unless the template already
admits undefined or void. So `{ [K in keyof Obj]?: Obj[K] }` over `{ a: 1 }`
declares `a?: 1 | undefined` and prints it, and `{ [K in keyof
Partial<Obj>]: Obj[K] }` (optional through its modifiers type) does too
(mappedTypeIndexedAccessConstraint `mapped`). TSR stored the bare template
and added undefined only on reads, so reads were right and the structural
print was `{ a?: 1; }`.

**Port.** `get_type_of_mapped_symbol` adds the optional type when the slot is
optional and the instantiated template may not be undefined or void, as
native's first arm; the strip arm is native's second. Reads through
`get_type_of_property` already unioned undefined, so they are unchanged (the
union dedupes). No state.

**Measured** (unfiltered against `fb7388a3`): types **+14, 0 lost**
(mappedTypeGenericIndexedAccess 11, mappedTypeIndexedAccessConstraint 3);
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 1.0001, dm
1.0002.

**Not this port:** a homomorphic mapping whose modifiers type is an alias of
a mapped type (`{ [K in keyof O]: O[K] }` with `type O = Omit<Mappings,
"foo">`) still enumerates no key (`{}`; native `{ "12": number; 42: string;
}`), and an optional property linked to a source declaration prints the
declaration's annotation (`b?: 2`, node reuse of `b?: 2` whose declared type
equals the property type) where TSR prints `b?: 2 | undefined`, which is the
printer's reuse arm.

## 32. A homomorphic mapping over a mapped-alias reference enumerates the instance's keys

**Forcing constraint.** getModifiersTypeFromMappedType takes `X` from a
`keyof X` constraint, and an alias reference `X` IS its instantiated body
natively: `{ [K in keyof O]: O[K] }` with `type O = Omit<Mappings, "foo">`
enumerates Pick's instance (`{ "12": number; 42: string; }`), and over
`Partial<Pick<Mappings, "foo">>` it reads `foo?`. TSR's alias reference is a
`Named` over the alias symbol whose member table the mapped producer cannot
list, so the mapping enumerated no key and printed `{}`.

**Port.** `mapped_modifiers_instance` (both producers that read the
modifiers type): a reference to a mapped alias answers its own mapped capture
(`ensure_mapped_type_info`); a reference whose evaluated body is a mapped
instance (`Omit` over `Pick`) answers that instance; anything else is
unchanged. And a key that names its property by a string literal which is not
an identifier prints quoted when no source property supplies the printed name
(createPropertyNameNodeForIdentifierOrLiteral: `"12"`, while the numeric key
`42` stays bare), the rule the `as`-clause arm already applied.

**Measured** with §29 (unfiltered against the `fb7388a3`+§31 freeze, whose
difference from this commit's parent is §29's two mappedTypeRelationships
rows): types **+8, 0 lost** beyond §29 (mappedTypeIndexedAccessConstraint 5,
declarationQuotedMembers 3); diagnostics 0 / 0; jsTyping new_false 0 /
lost_true 0; Ir gi 1.0000, dm 1.0001.

**Not this port:** an intersection modifiers type of such references
(`{ [K in keyof PartMappings]: … }` over `Omit<…> & Partial<Pick<…>>`) still
enumerates nothing. Admitting it through
`composite_modifiers_property_names` listed the names but lost the
optionality and the numeric key provenance of the mapped constituents (a
mapped member has no property symbol for the composite's
`get_property_of_type` to find), printing a wrong member table where `{}`
stood; the members' modifiers need the type road through composites first.

## 33. An intersection modifiers type with a mapped constituent lists its members

**Forcing constraint.** `{ [K in keyof PartMappings]: (o: MapperArgs<K>) =>
PartMappings[K] }` over `type PartMappings = SetOptional<Mappings, "foo">`
(`Omit<T, K> & Partial<Pick<T, K>>`) enumerates every constituent's
properties (getPropertiesOfUnionOrIntersectionType), each with
createUnionOrIntersectionProperty's modifiers: an intersection property is
optional, and readonly, only when every constituent that has it is
(`checker.go:21452`). Its keys keep the providing constituent's provenance
(the numeric `42`). §32's mapped instances have no property symbol for the
composite symbol lookup, so the mapping listed nothing (`{}`), and the
`mapper` literal's methods lost their contextual signatures.

**Port.** `composite_modifiers_property_names` resolves each intersection
constituent through `mapped_modifiers_instance` and takes a mapped
constituent's names as listed; `intersection_modifier_member` answers the
modifiers and key of a name over an intersection with a mapped constituent
(mapped slots for mapped constituents, the symbol road for the rest), read by
the key producer and the member worker. Unions are unchanged. No state.

**Measured** (unfiltered against `8d1ae151`): types **+31, 0 lost**
(mappedTypeIndexedAccessConstraint); diagnostics **+1 case, 0 lost**
(mappedTypeIndexedAccessConstraint); jsTyping new_false 0 / lost_true 0; Ir
gi 0.9999, dm 1.0001.

**Residue:** the five `mapper` lines print `"12"` and `42` before `foo?`,
native `foo?` first. getIntersectionTypeEx keeps insertion order
(`orderedSet`, `checker.go:26057`) and getPropertiesOfUnionOrIntersectionType
walks constituents in order, which would put Omit's keys first too, so the
native order comes from somewhere this note has not found (the apparent
intersection's rebuild in getApparentTypeOfIntersectionType is the next
place to read).

## 35. A non-distributive infer conditional infers from a whole union check

**Forcing constraint.** getConditionalType distributes only over a naked
type-parameter check (`root.isDistributive`); any other check is inferred
from as a whole, so `type ChannelType = Channel extends { type: infer R } ?
R : never` over `Channel = TextChannel | EmailChannel` infers `R` from both
constituents (getCommonSupertype of two string literals is their union) and
declares `"email" | "text"` (complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound).
`evaluate_conditional_inference` declined every union check, which is right
only for the distributive shape the caller already distributed.

**Port.** The union decline is kept for a distributive conditional (its
check node is a naked parameter, `distributive_conditional_parameter`) and
dropped otherwise; `never` checks still decline. No state.

**Measured** (unfiltered against the rebased `89f0f0df`): types **+10, 0
lost** (complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound 7,
coAndContraVariantInferences3 3); diagnostics 0 / 0; jsTyping new_false 0 /
lost_true 0; Ir gi 0.9999, dm 1.0000.

## 34. A generic mapped constraint enumerates its lower bound (getLowerBoundOfKeyType)

**Forcing constraint.** resolveMappedTypeMembers enumerates a non-homomorphic
mapped type over `forEachType(getLowerBoundOfKeyType(constraintType), …)`
(`checker.go:21021`): `keyof T` lowers to the index type of `T`'s apparent
type, a distributive conditional (`Exclude<keyof T, "a">`) is instantiated at
its check type's lower bound, unions and intersections map their
constituents. So `Pick<T, Exclude<keyof T, "a">>` over `T extends { a:
string, b: string }` has the member `b: T["b"]`, and `obj.b` is `T["b"]`
(mappedTypeConstraints). TSR's key walk declined every generic key, leaving
the mapping memberless and `obj.b` a gap.

**Port.** `lower_bound_of_key_type` (bounded at eight levels): a deferred
`keyof` mint (`deferred_keyof_operands`) answers `resolved_keyof_type` of its
operand's apparent type unless that is the operand itself or a generic tuple
(getKnownKeysOfTupleType is not ported here); a deferred conditional alias
reference whose body checks a parameter nakedly
(`distributive_alias_check_index`) is re-referenced with that argument's lower
bound; unions and intersections map, keeping the `string & {}` pair. The key
walk starts from the lower bound; a key that is still generic declines as
before. No state.

The lower bound is for member resolution only: getIndexTypeForMappedType
reads the constraint itself (a generic mapped type's index type defers before
it), and a first cut that lowered there too turned `keyof { [P in keyof T &
keyof NameMap as NameMap[P]]: string }` and `keyof DoubleProp<U>` into
`never` (−3 mappedTypeAsClauses, −2 mappedTypeGenericWithKnownKeys before
the containment below).

**Containment.** A lowered generic constraint publishes its property keys
only. Its index keys (`keyof Shape` over `Shape extends Record<string,
string>` lowers to `string | number`) are native's too, but the access sites
here do not decline an index signature for a write to a generic object
(checkPropertyAccessExpressionOrQualifiedName's `assignmentKind == None ||
!isGenericObjectType(leftType)`), so `obj.unknownLiteralKey = 3` read
`number` and `obj['' as string] = 4` became a gap (mappedTypeGenericWithKnownKeys
−2). Routed: the write-access guard, to the access sites' owner.

**Measured** (unfiltered against the rebased `89f0f0df`; this commit's
parent adds §35, which touches only the conditional evaluator): types **+16,
0 lost** (mappedTypeConstraints); diagnostics 0 / 0; jsTyping new_false 0 /
lost_true 0; Ir gi 0.9998, dm 1.0001.

## 36. A reference to an instantiation-expression alias is the instantiated body

**Forcing constraint.** getTypeFromTypeQueryNode's result takes no alias, so
an alias whose body is an instantiation expression (`type ErrAlias<U> =
typeof Err<U>`) declares an alias-free type, and getTypeAliasInstantiation's
instantiateTypeWithAlias finds no alias to carry (no new alias at a plain
reference, and the declared type's own is nil): `ErrAlias<number>` is `{
new (): ErrImpl<number>; prototype: ErrImpl<any>; } & (() => number)`
(aliasInstantiationExpressionGenericIntersectionNoCrash1). The declared type
already answered the body (`alias_free_generic_alias_body`); a reference
minted the alias name.

**Port.** `instantiate_type_query_alias`, beside the template-literal arm of
`create_type_reference`: a body that is a type query with type arguments is
evaluated under the alias's bindings and answered when it is not a gap. Cached
in `instantiations` like the other arms.

**Measured** (unfiltered against the `89f0f0df`+§34 freeze; this commit's
parent also has §35, whose ten rows are excluded): types **+3, 0 lost**;
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 0.9999, dm
1.0003 (with §35).

## 37. A reference to an alias declared as a deferred conditional reference is the root's instance

**Forcing constraint.** getConditionalTypeInstantiation (`checker.go:22485`)
gives a deferred result `alias` when one is passed, else
`instantiateTypeAlias(root.alias, mapper)`: the alias that owns the
conditional node. `type ComponentPropsWithoutRef<T> =
PropsWithoutRef<ComponentProps<T>>` declares the deferred conditional under
its own name (the declaring alias is newAliasSymbol at the declaration), but a
reference elsewhere re-instantiates with no new alias, so `props:
ComponentPropsWithoutRef<T>` at another `T` is `PropsWithoutRef<ComponentProps<T>>`
(genericCallInferenceInConditionalTypes1), and `type W<T> =
ComponentPropsWithoutRef<FunctionComponent<T>>` instantiates to
`PropsWithoutRef<T>`. TSR minted the referenced alias's name.

**Port.** `deferred_conditional_reference_alias`, after
`evaluate_conditional_alias` declines in `create_type_reference`: a generic
alias whose body is a reference to an alias chain ending in a conditional
(`alias_chain_ends_in_conditional`) resolves the reference's arguments under
its bindings and references the inner alias; only a deferred conditional
answer is taken. A distributive root over a union or never argument is not:
mapTypeWithAlias distributes, and the union carries the declared type's own
alias instantiated (`WithName<T>` over `DiscriminateUnion<BigUnion, "name",
T>` keeps its name; taking it cost
conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable
one row in the first cut). No new state.

**Measured** (unfiltered against `6dc72ec9`): types **+7, 0 lost**
(divideAndConquerIntersections 3, coAndContraVariantInferences3 2,
genericCallInferenceInConditionalTypes1 1,
indexedAccessKeyofNestedSimplifiedSubstituteUnwrapped 1); diagnostics 0 / 0;
jsTyping new_false 0 / lost_true 0; Ir gi 0.9998, dm 1.0002.

## 39. `keyof` over a deferred indexed access is one deferred index type

**Forcing constraint.** getIndexTypeEx defers under shouldDeferIndexType
(`checker.go:26834`) for an instantiable operand, so `keyof Original[T]` is an
index type printed from its operand, and `type NestedKeyOfOriginalFor<T
extends keyof Original> = keyof Original[T]` declares `keyof Original[T]`
(correlatedUnions). TSR's evaluated `keyof` arm deferred type parameters,
mapped types and `any`/`never`/`unknown`, and sent a deferred indexed access
to the key walk, which answered `error`.

**Port.** A deferred indexed-access operand (`deferred_indexed_access_types`,
not an unresolved mint) joins the arm that answers `resolved_keyof_type`,
whose instantiable branch mints the deferred index type. No state.

**Measured** (unfiltered against the rebased `42fc9dcc`): types **+4, 0
lost** (correlatedUnions 0:439, 448, 449, 454); diagnostics 0 / 0; jsTyping
new_false 0 / lost_true 0; Ir gi 1.0000, dm 0.9999.

**Measured and not taken: the own-access test for an indexed-access alias.**
`type Format<I, F> = { s: I[2]; n: I[3] }[F]` at `"n"` resolves the access
and answers the property type `I[3]` with no alias (getIndexedAccessTypeOrUndefined's
resolved arm), so `type Pos<I> = Format<I, "n">` declares `I[3]`; the indexed
arm of `create_type_reference` re-mints any deferred answer under the
alias's name. Re-minting only when the deferred access is the body's own
(object or index under the bindings) fixed a local probe and measured 0 / 0
on the corpus: the corpus rows (ramdaToolsNoInfinite2 `Pos`, `Key`) reach
`Format` through an `import` alias, which `alias_reference_target`
(`resolve_entity_name` with the type meaning) does not follow, and following
it through `resolve_alias_fully` sent the reference down the import road,
which mints `Format<I, "n">` itself. Both pieces wait for one reference road.

## 40. Refused: function and constructor branches in a conditional mint's text

`type ParentConfig<T> = Partial<{ [P in keyof T]: Required<T>[P] extends
(...args: any) => any ? (...args: Parameters<Required<T>[P]>) =>
ReturnType<Required<T>[P]> : T[P] }>` (thislessFunctionsNotContextSensitive3)
declares the mapped type printed from its parts; TSR declares the alias
name, because the conditional template's written mint
(`conditional_mint_text`, `conditional_typed_mint_text`) has no text for a
function-type branch (`written_type_text` has no function arm), so the
template, the mapped type and the alias body are `error`.

Two arms were measured, both refused for the same case:

- printing a function or constructor branch from its resolved type: OOM in
  recursiveConditionalTypes2 (5966 MiB at 8 s);
- printing it syntactically (`written_signature_type_text`, parameters, rest,
  optional, type parameters and return composed from `written_type_text`;
  nothing resolved): the same OOM (5981 MiB at 7 s).

The second shows the cost is not the branch print. A conditional that used to
be `error` now has a mint, so `UnionToIntersection<{ [P in keyof T]:
SimplifyPrivateSpec<T[P]> }[keyof T]>` keeps evaluating through
`MaybeMergePrivateSuperSpec`'s recursion where the `error` stopped it. Native
stops by deferring a generic check. Tracing where this port forces that
recursion (`type_to_string` over the mapped check while it builds the mint is
the first suspect) comes before either arm. Local probes: `ParentConfig` and
`T[P] extends string ? (...args: Parameters<Required<T>[P]>) => 1 : 2` both
print as native with either arm.

### 40.1 Follow-up: a nested-capture guard ends the OOM, and the case then regresses

The OOM traced to `capture_inline_conditional_constraint`: computing an inline
conditional mint's distributive constraint at mint time reaches other
aliases' branches (`capture_conditional_alias_branches`), whose own inline
mints capture again, so `SimplifyPrivateSpec`'s mutually recursive alias set
recursed through constraint captures without bound. Native computes that
constraint lazily (getConstraintOfType on demand). A guard that skips a
capture nested in another capture (a depth counter in
`instantiation_expressions`; the inner mint publishes no constraint, as an
undecidable one publishes none) removed the OOM under both §40 arms.

**Refused all the same.** With the guard, `ParentConfig` resolves and
thislessFunctionsNotContextSensitive3 goes from 3 to 35 probe diffs against
native: the resolved mapped type reaches contextual-typing roads that answer
differently from native's. Neither the arms nor the guard is committed; the next attempt
starts from that contextual difference, not from the print.

## 41. An argument-less qualified reference to a fully defaulted generic is its defaults' instance

**Forcing constraint.** fillMissingTypeArguments (`checker.go:19458`) fills
an argument-less reference whose every type parameter has a default, and the
node builder prints the filled arguments: `NS.RT` over `type RT<P = any> =
…` is `NS.RT<any>` (callsOnComplexSignatures' `React.ReactType<any>`,
reactSFCAndFunctionResolvable, the jsxNamespace cases), while a reused
annotation keeps the written `NS.RT` (`f : (x: NS.RT) => NS.RT<any>`). The
unqualified road already fills defaults (§890); the qualified one minted the
bare written text for every argument-less reference.

**Port.** `defaulted_qualified_reference`, ahead of the argument-less mint
in `qualified_type_reference`: for a type alias, class or interface whose
parameters all have defaults that resolve without mentioning a type
parameter, the mint is §42 v2's generic one at the defaults (qualified
print with the arguments, `qualified_generic_reference_types` key,
`type_reference_targets` pair), and the written bare name goes to the
`qualified_written_text` reuse channel. A forward default (one that mentions
a parameter) and an intersection-bodied alias keep the existing road. No new
state.

**Measured** (unfiltered against `5367da5b`): types **+18, 0 lost**
(callsOnComplexSignatures 5, reactSFCAndFunctionResolvable 4,
jsxNamespaceGlobalReexport 3, jsxNamespaceGlobalReexportMissingAliasTarget 3,
jsxNamespaceImplicitImportJSXNamespace 3); diagnostics 0 / 0; jsTyping
new_false 0 / lost_true 0; Ir gi 0.9998, dm 1.0000.

## 42. A partially written qualified reference fills its defaults

**Forcing constraint.** fillMissingTypeArguments fills a written list that is
shorter than the parameter list when the missing parameters have defaults:
`NS.CC<string>` over `interface CC<P = {}, S = any>` is `NS.CC<string,
any>`, and a reused annotation keeps the written `NS.CC<string>`. §42 v2's
generic qualified arm admitted only full lists, so a shorter one fell
through to `unresolved_type_reference`: an `any` mint carrying the written
text, so the reference was an unresolved type, not the interface instance.

**Port.** `qualified_reference_missing_defaults` supplies the missing tail
when every missing parameter has a default that resolves without mentioning
a type parameter, and the §42 v2 arm builds its mint with the full argument
list (printed) and the written list (reuse channel), as it does for a full
list. No new state.

**Measured** (unfiltered against the rebased `007e770e`): types 0 / 0,
diagnostics 0 / 0, jsTyping 0 / 0, Ir gi 0.9997, dm 1.0000. Kept for its
semantics (the reference is now the instance the relater and member roads
read, not an `any` mint); the corpus's rows of this shape
(jsxExcessPropsAndAssignability's `React.ComponentClass<P, any>`) reach
`React` through react16's `export =` namespace, a road this arm does not
cover.

## 43. A declined alias-rooted reference spells its filled defaults

**Forcing constraint.** `React.ComponentClass<P>` through `import * as React`
over `interface ComponentClass<P = {}, S = ComponentState> extends
StaticLifecycle<P, S>` prints `React.ComponentClass<P, any>`
(jsxExcessPropsAndAssignability, reactReadonlyHOCAssignabilityReal):
fillMissingTypeArguments fills `S`. The reference is declined by
`alias_rooted_reference_declines`' generic-heritage arm (the relater
containment that keeps it the unresolved gap it was before alias-rooted names
resolved), and the unresolved mint spelled the written list.

**Port.** `declined_reference_with_defaults`: when the written list is
shorter than the parameter list and the missing parameters have defaults
(`qualified_reference_missing_defaults`, §42), the declined reference's
unresolved mint (still an `unresolved_types` member, still `is_error`) prints
every argument, and the written list goes to the reuse channel. The
containment itself is unchanged: nothing reads through the mint.

**Measured** (unfiltered against `7df4626a`): types **+6, 0 lost**
(jsxExcessPropsAndAssignability 4, reactReadonlyHOCAssignabilityReal 2);
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 1.0001, dm
1.0000.

## 44. `readonly T` over a type parameter declares the parameter

**Forcing constraint.** getTypeFromTypeOperatorNode's `readonly` arm answers
its operand's type (the readonly-ness of an array or tuple is read by
getTypeFromArrayOrTupleTypeNode from its parent; on any other operand the
operator is only an error), so `type T31<T> = readonly T` declares `T`
(readonlyArraysAndTuples `>T31 : T`), like §282's `type Id<T> = T`.
`type_parameter_body_index_at` looked through parentheses only.

**Port.** The `readonly` operator is transparent in that walk, as
parentheses are. No state.

**Measured** (unfiltered against `58a511f1`): types **+1, 0 lost**
(readonlyArraysAndTuples 0:7); diagnostics 0 / 0; jsTyping new_false 0 /
lost_true 0; Ir gi 0.9999, dm 1.0000.

## 45. §21 landed: `keyof` of a non-generic object that mentions a type parameter resolves now

§21's held diff (`keyof_operand_resolves_now`: a plain object operand that is
not generic, `is_generic_type`, has its keys now, as getIndexTypeEx defers
only under shouldDeferIndexType) re-measured on the rebased `cfe2d4e5`: the
deeplyNestedMappedTypes diagnostic it cost is gone (other lanes' relation
work since `4af98855`), and the mapped-type rows it unblocks now land too.

**Measured** (unfiltered against `cfe2d4e5`): types **+12, 0 lost**
(deeplyNestedMappedTypes 11, thislessFunctionsNotContextSensitive1 1);
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 0.9998, dm
1.0000.

## 46. §18 and the wide union-alias slice landed together

§18's frame isolation (getDeclaredTypeOfTypeAlias resolves with no mapper:
`get_declared_type_of_type_alias` sets the alias-evaluation frames aside for
the resolution, so React's `type ReactNode = ReactChild | …` first reached
under `ComponentType<P>`'s frame is published with its alias) and §10.1's
wide union-alias slice (`is_union_alias_body` admits any constituents, so a
generic union-bodied alias instance is its instantiated union carrying the
alias, getUnionTypeEx's alias argument) were held for one
deeplyNestedMappedTypes diagnostic that §45 now lands. Alone, the slice
still loses ReactNode's alias (styledComponentsInstantiaionLimitNotReached)
and HandlerRendererResult's (jsxComplexSignatureHasApplicabilityError):
that is the frame leak, so the two go in one commit. Measured alone on
`cfe2d4e5`: the slice +50 / −2 types; with the isolation +61 / 0.

**Measured** (unfiltered against `56aba53c`): types **+50, 0 lost**
(assignmentTypeNarrowing 9, jsxComplexSignatureHasApplicabilityError 8,
typePredicateFreshLiteralWidening 4, discriminateWithOptionalProperty3 8,
unionAndIntersectionInference1 4, awaitedType/awaitedTypeStrictNull 6,
for-of58 3, and others); diagnostics **+4 cases, 0 lost**
(coAndContraVariantInferences5, jsxElementTypeLiteralWithGeneric,
typeAssignabilityErrorMessage, for-of58); jsTyping new_false 0 / lost_true
0 (one native-absent error fewer); Ir gi 0.9999, dm **1.0012** (the union
instances are built where the mint was a name).

## 47. A mapped type's members are ordered as compareSymbols orders them

**Forcing constraint.** setStructuredTypeMembers → getNamedMembers
(`checker.go:22049`) sorts a members table with compareSymbols
(`utilities.go:366`): symbols with declarations first, by their first
declaration's position, then by name, then by id. A mapped symbol carries
its modifiers property's declarations when resolveMappedTypeMembers links
them. So `{ [K in 2 | 10 | 1]: 1 }` prints `{ 1: 1; 10: 1; 2: 1; }` (names
compare as strings), and the §33 mapping over `Omit<Mappings, "foo"> &
Partial<Pick<Mappings, "foo">>` prints `foo?` first, at its declaration in
`Mappings` (mappedTypeIndexedAccessConstraint's five `mapper` rows, §33's
residue). TSR kept key order, and an intersection member linked nothing.

**Port.** The member producer sorts its table by `(no origin, the origin's
first declaration position, name, creation order)`, reusing
`compare_symbols_key`, and renumbers each mapped slot to its new table index.
`intersection_modifier_member` now also answers the providing constituent's
property (the captured slot's origin, or the property symbol), which the
producer links like any other modifiers property. No state.

The held r6-typesroots3 "mapped member names as native" patch (`afeaa2bce`
on `box/r7-declared-held`) carries the same ordering over its own member
pipeline; its name arm is partly §32's.

**Measured** (unfiltered against `27c5925b`): types **+6, 0 lost**
(mappedTypeIndexedAccessConstraint 5, deeplyNestedMappedTypes 1);
diagnostics 0 / 0; jsTyping new_false 0 / lost_true 0; Ir gi 0.9999, dm
1.0000.

## 48. A mapped member's name is printed from its linked declarations

**Forcing constraint.** getPropertyNameNodeForSymbolFromNameType
(`nodebuilderimpl.go:2455`) names a mapped symbol from its name type and the
declarations it links: a non-identifier name is a string literal when every
linked declaration is string-named (single-quoted when they all are) or the
name is not numeric; a negative numeric name is computed (`[-1]`); any other
is classified (`classifyPropertyName`). So `{ [K in keyof S]: 1 }` over
`{ 'a-b': 1; "12": 2; 3: 3 }` prints `{ 'a-b': 1; "12": 1; 3: 1; }`, and
`{ [K in "12" | -1]: 1 }` (nothing linked) prints `{ [-1]: 1; 12: 1; }`.
§32's rule quoted every non-identifier string key, and the `as`-clause arm
quoted by the key's literal kind.

**Port.** `mapped_property_printed_name` (the held r6-typesroots3 patch
`afeaa2bce`'s, which reads the origin's declarations) replaces both arms;
the origin is computed before the name. No state.

**Measured** (unfiltered against `41565caf`): types 0 / 0, diagnostics 0 / 0,
jsTyping 0 / 0, Ir gi 1.0001, dm 1.0002. Kept for its prints, which match
native on every probe above; no corpus row has these shapes.

## 49. A renamed import of a literal-key alias gives a mapped constraint its keys

**Forcing constraint.** declarationEmitAliasFromIndirectFile 1:4:
`import { key as LocaleKey } from "./locale"` over `export type key = "ar" |
"bg"`, then `l10ns: {[k in LocaleKey]?: CustomLocale } & { default: Locale }`.
Native records `{ ar?: CustomLocale | undefined; bg?: CustomLocale |
undefined; } & { default: Locale; }`; TSR printed the mapped type from its
parts, `{ [k in LocaleKey]?: CustomLocale; }`. resolveTypeReferenceName
resolves the renamed specifier to `key` whatever its local name, so the
constraint is the union and resolveMappedTypeMembers enumerates it. This
port's annotation road keeps a renamed reference a written mint (only its
printed name depends on the site), so the mapped producer met a mint it
cannot enumerate.

**Port.** `mapped_constraint_type` (mapped.rs) gains a TypeReference arm:
when the written reference answers `error`, `renamed_import_key_constraint`
resolves an argument-less ALIAS symbol fully; a non-generic TYPE_ALIAS target
whose declared type is a string or number literal, or a union of them,
answers that declared type. A mapped constraint is never printed once its
keys are concrete (the mapped type prints its members), so the site-printed
name the mint carries has no reader here. No state.

**Alternatives.** Resolving every renamed import to its declared type on the
annotation road is native's shape, but it moves every printed renamed
reference, which is the printer's, not this lane's. Admitting any declared
type (an object, a generic alias) would hand the producer constraints this
arm has not been measured on; the literal-key slice is what the forcing row
needs.

**Measured** (unfiltered against `d87e190a`): types **+1, 0 lost**
(declarationEmitAliasFromIndirectFile 1:4); diagnostics 0 / 0; jsTyping
new_false 0 / lost_true 0; Ir gi 0.9999, dm 0.9998.

**Not reached:** 2:6 and 2:8 print the same type in `app.ts`, where native
qualifies the template, `import("./locale").CustomLocale | undefined`. A
plain `CustomLocale | undefined` union read in another file already prints
unqualified (probe: `d: CustomLocale | undefined` in an interface), so the
residue is the union printer's qualification, not the mapped type.

## 50. Routed, not ported: three roads met while looking for §49

- **An interface extending a generic alias of an interface reference**
  (`interface TaggedPair<T> extends Pair<T>` over `type Pair<T> = AB<T, T>`;
  genericTypeAliases 0:80–0:87, `p.a : number`): `interface_heritage_type`
  (base_types.rs) answers the alias reference, and the base is valid, but
  members.rs's `generic_heritage_member` walks `base_symbol_of_heritage_entry`
  and finds no class or interface behind the alias, so `p.a` is `error`.
  Even a non-generic `interface X extends Pair<number>` misses. Members.rs's.
- **`unknown` against a signature type under `strictNullChecks: false`**
  (awaitedType 0:6, `Awaited<{ then(): void }>`, native `never`):
  `relate_ternary(unknown, (value: number) => any)` answers `Unknown`, so
  the conditional's false branch is not taken and the alias is `error`.
  Strict mode decides it (unknown includes null). The relater's.
- **Local and non-exported aliases printed structurally**
  (genericTypeAliases 0:94–0:112, inlineMappedTypeModifierDeclarationEmit,
  keyRemappingKeyofResult 0:62): native expands an alias the enclosing
  declaration cannot name (`Foo : T | { x: T | any; }`). The printer's
  accessibility arm.


## 51. An unconstrained parameter is its own restrictive form, and an open conditional reference defers

**Forcing constraint.** deferredConditionalTypes 0:7 and 0:8: over `type
A<T> = { x: T } extends { x: 0 } ? 1 : 0`, native declares `type T4<T> =
[A<T>] extends [0, 0] ? 1 : 0` as `0` (and `T5` likewise); TSR declared
`T4<T>`. Two gaps sat on the road:

1. `A<T>` in an alias-declared position answered `error`. Its check `{ x:
   T }` is not generic, so `conditional_alias_check_is_deferred` said no,
   and §36's site keeps the gap for every other refusal. Native's
   getConditionalType (checker.go:24339) relates the permissive
   instantiations (related: `{ x: any }` to `{ x: 0 }`), then the
   restrictive ones (not related), and falls through to the deferred
   ConditionalType carrying the reference's alias. An open outcome defers
   exactly as a generic check does.
2. The restrictive relation answered `Unknown`, because
   `restrictive_type_parameter` always minted a clone the relater cannot
   trace to a declaration. getRestrictiveTypeParameter (checker.go:24512)
   returns the parameter itself when it has no constraint and no constraint
   declaration; the clone exists only to drop a constraint.

**Port.** `restrictive_type_parameter` returns the parameter when
`declares_no_constraint` holds (no known constraint, and every declaration
a type parameter declaration without a constraint node; untraceable
parameters keep the clone). `conditional_alias_outcome_is_open` binds the
alias's parameters as `conditional_alias_check_is_deferred` does and
answers true only when the permissive relation is `Related` and the
restrictive one is `NotRelated`; infer parameters, an `any` check, an
`any`/`unknown` extends type, a deferred operand, or any `Unknown` answer
false. §36's site keeps its gap unless one of the two deferral tests holds.
No new state: the restrictive cache keeps its key and owner, and a parameter
that is its own form is never inserted.

**Alternatives.** Teaching the relater to treat an unregistered type
parameter as unconstrained would decide the clone too, but the clone is
wrong in the first place for these parameters: native never makes one, so
a cache keyed by it would hold types native never builds. Accepting
`Unknown` from the restrictive relation as "open" would publish a deferred
name where native might take the true branch.

**Measured** (unfiltered against `e9011546`): types **+11, 0 lost**
(mappedTypeOverlappingStringEnumKeys 5, conditionalTypes2 4,
deferredConditionalTypes 2); diagnostics **+1 case, 0 lost**
(mappedTypeOverlappingStringEnumKeys); jsTyping new_false 0 / lost_true 0;
Ir gi 0.9998, dm 1.0000; exact oracle +1 (mappedTypeOverlappingStringEnumKeys),
0 lost.

**Not reached:** `[q, r]` over `q: [A<T>]` and `r: A<T>` is still `error`.
Subtype reduction relates the tuple and the conditional-alias mint, and the
relater answers `Unknown` (relater.rs, routed).

## 52. A generic union body that reduces to one type declares that type

**Forcing constraint.** unknownType1 0:17: `type T23<T> = T | unknown`
records `>T23 : unknown`; TSR declared `T23<T>`, and a reference `T23<T>`
stayed the alias name where native reads `unknown`. getDeclaredTypeOfTypeAlias
is getTypeFromTypeNode of the body, and getUnionTypeEx (checker.go:25672)
answers `any`/`unknown` for a set that includes one, and the sole survivor
of a reduced set, before an alias is considered. Only a set that is still a
union is built with the alias.

**Port.** Three sites in `declared.rs`, each the same rule:

- `get_type_from_union_type_node`'s generic-alias arm, which answered
  `error` for every body (the alias name could not be printed with its
  arguments), now answers the reduced type when it is not a union;
- `get_declared_type_of_type_alias_worker` publishes a union body's
  resolved type when it is not a union (`DeclaredBody::Union`);
- `evaluate_union_or_indexed_alias_body` accepts a non-union evaluation of
  a union body, so a reference answers the reduced type with no alias
  (`T23<T>` is `unknown`, `type U<T> = T | never` at `string` is `string`).

A union answer keeps every existing road (the mint, and the named union
over the reference). No state.

**Alternatives.** Admitting only `any`/`unknown` answers is the forcing
row's slice; the single-survivor half (`T | never`, `T | T`) is the same
early return in getUnionTypeEx, so splitting them would leave a rule half
ported.

**Measured with §53, in one commit.** Alone, §52 lost one exact case
(mappedTypes4, `Extract<keyof T, string>` read as `string`). It
reached the for-in road's `keyof (T & object)`, which declined on `object`
until §53. Gated against `6559089e` + §51 (each against its parent, then
together): types **+12, 0 lost** (8 gap → right, 4 wrong → right; §52
alone +1, unknownType1); diagnostics 0 / 0; jsTyping new_false 0 /
lost_true 0; Ir gi 0.9998–1.0001, dm 1.0000; exact oracle **+1**
(extractInferenceImprovement), 0 lost, and keyofAndIndexedAccess2 moved
from TSR_TIMEOUT to WRONG.

## 53. `keyof object` is `never`

**Forcing constraint.** mappedTypes4 0:13–0:19: `obj[k]` over `obj: T &
object` and a generic key printed `error` where native reads `(T &
object)[Extract<keyof T, string>]`. checkIndexedAccessIndexType admits a
generic key by relating it to `keyof` of the receiver; `keyof (T &
object)` is `keyof T | keyof object`, and `resolved_keyof_type` answered
`None` for the `object` constituent, which declined the whole union and so
the access.

**Port.** `resolved_keyof_type_worker` answers `never` for the
`non_primitive` intrinsic, beside the `unknown` arm: getIndexTypeEx's
default arm reads the literal keys of the apparent type, and `object`'s
apparent type is the empty object type. No state.

**Measured** with §52; see §52.

## 54. A mapped type keeps and reads its unique-symbol keys

**Forcing constraint.** keyRemappingKeyofResult: over `const sym =
Symbol("")` and `type O2 = { str: any, [sym]: any }`, native's
`{ [K in keyof O2]: 1 }` is `{ str: 1; [sym]: 1; }`, `keyof { [K in keyof
O2 as K]: 1 }` is `keyof O2`, `{ [K in "a" | typeof sym]: 1 }` prints
`{ a: 1; [sym]: 1; }`, and reading `[sym]` from any of them is `1`. TSR
dropped the `[sym]` member of the homomorphic mapping, answered `"str"` for
the `keyof`, ordered `[sym]` first, and read `error` through the symbol.
Three causes, all in this lane's files:

1. The homomorphic key walk (`mapped_member_keys`) listed the modifiers
   type's names with `property_names_of`, which reads binder members; a
   late-bound member is bound late and is not among them.
   forEachMappedTypePropertyKeyTypeAndIndexSignatureKeyType reads every
   property (checker.go:22731).
2. getIndexTypeForMappedType (checker.go:26918) ends by answering the
   constraint itself when the mapped keys are its own constituents, which
   keeps the constraint's `keyof O2` origin; `mapped_index_type` always
   built a fresh union.
3. A unique-symbol key names its member with native's escaped name
   (`crate::unique_symbol_keys`, spelled `__@name@id` here). Native's prefix
   is `"\xFE"`, so compareSymbols sorts such a name after every printable
   one; `__@` sorted first. And `unique_symbol_property_type`
   (indexed.rs) only scanned written `[…]` members, so the escaped name was
   never looked up.

**Port.** (1) the late-bound names of `get_property_names_of_type` that
the binder walk missed are appended; their keys come from
`literal_type_of_property` (the computed name's type), and §47 orders the
members. (2) `mapped_index_type` answers `info.constraint` when the result
union's constituents are the constraint union's. (3) §47's sort key puts a
`__@` name after the other unlinked names, and `unique_symbol_property_type`
first reads the escaped name (`unique_symbol_property_name`), as native's
one-table read does. No state.

**Alternatives.** Renaming every symbol-keyed member to one convention
(written `[s]` against native's escaped name) is `r6-accessible.md` §4's
open question and crosses `objects.rs`; the read in (3) bridges the two
conventions at the one site that needs both.

**Not reached:** `type Remapped = { [K in keyof Orig as {} extends
Record<K, any> ? never : K]: any }` still keys `"str"` only. The
`[sym]` key's name type evaluates `{} extends Record<typeof sym, any>` as
true: `{}` relates to any `Record`/`Pick` instance whose keys include a
unique symbol (`const x: Record<typeof sym | "a", number> = e` with `e: {}`
reports nothing, while `Record<"a", number>` reports TS2741). That
relation is the relater's (routed).

**Measured** (unfiltered against §52–§53 on `6559089e`): types 0 / 0,
diagnostics 0 / 0, jsTyping 0 / 0, Ir gi 1.0000, dm 1.0002, exact oracle
0 / 0. No corpus row reaches the three causes alone (keyRemappingKeyofResult
needs the routed relation); kept for the prints above, which match native,
and as §56's base.
