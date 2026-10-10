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
