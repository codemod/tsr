# r5-constraints2 — lane notes

Round-5 lane `tsr-2zk.1011`: TS2344 (`Type '{0}' does not satisfy the
constraint '{1}'`) and TS2403 (`Subsequent variable declarations must have the
same type`). Native reference: `vendor/typescript-go` @ `5b1047d`,
`internal/checker/checker.go` unless noted. Owned: `constraints.rs`,
`identity.rs`, this file and the `r5-constraints2-*.diff` files beside it.

Populations come from the frozen integration baseline (`2919d8c`)
`diagverdictdump`, WRONG/EMPTY_WRONG rows, expected-vs-actual
`(file, line, column, code)` multisets. Plain keys: 22 cases differ on TS2344
(9 on TS2344 alone), 12 on TS2403 (9 alone).

## §1 TS2344: type parameters come from every class, interface or alias declaration

`checkTypeReferenceOrImport` (`:2998`) asks `getTypeParametersForTypeAndSymbol`
(`:17198`), which for a class or interface is the reference target's
`LocalTypeParameters`: `appendLocalTypeParametersOfClassOrInterfaceOrTypeAlias`
walks **every** declaration of the symbol, skips the ones that are not a
class, interface or alias, and keeps each parameter once.

TSR read the symbol's **first** declaration (`local_type_parameters_of`,
`declared.rs`). `conformance/nonPrimitiveInGeneric` and
`conformance/nonPrimitiveStrictNull` write `interface Proxy<T extends object>`
in a global script, so the symbol merges with the lib's `declare var Proxy`,
whose declaration comes first. The first-declaration read found no type
parameters, the reference `Proxy<number>` resolved to the error type (an
arity mismatch with no TS2315 printed), and the check never ran.

Two halves:

- **Committed (`constraints.rs`).** `constraint_check_type_parameters` is the
  native walk for the check itself: every class/interface/alias declaration's
  parameters, deduplicated by declared type, with the default taken from the
  first declaration that writes one (`getDefaultFromTypeParameter`). Alone it
  changes no verdict, because the reference is still the error type.
- **Diff (`r5-constraints2-declared-local-type-parameters.diff`, `declared.rs`,
  r5-typeparams2's file).** `local_type_parameters_of` picks the first
  class/interface/alias/typedef declaration instead of the first declaration.
  **It stops at a namespace too**, which upstream does not. The full native
  skip measured +3 diagnostics cases (`nonPrimitiveInGeneric`,
  `nonPrimitiveStrictNull`,
  `privacyCheckExportAssignmentOnExportedGenericInterface1`) and +89 type
  lines, but lost 3 type lines in `compiler/complexRecursiveCollections`
  (910, 916, 924): `Collection` there is `namespace Collection` +
  `function Collection` + `interface Collection<K, V>`, and once the
  reference is generic the namespace members print unqualified
  (`Indexed<Z_1>` for `Collection.Indexed<Z_1>`), the NB-SYMBOL-CHAIN
  printer limit `declared.rs` already records at its namespace-rooted mint.
  With the namespace stop the three lines stay RIGHT; measured numbers are in
  §8.

**How this would be wrong.** If a merged class/interface ever lists type
parameters in a later declaration that differ from the first type
declaration's, `local_type_parameters_of` (one slice) under-reports them; the
committed walk does not.

## §2 TS2344: a `typeof` argument is not a generic node

`type_argument_node_is_generic` declined every type argument whose syntax
could be resolved eagerly to a type upstream keeps deferred. A `TypeQuery`
was in that list, but `getTypeFromTypeQueryNode` is the widened type of the
named value — never a deferred type of its own. A query over a generic value
is still declined by the type-side `relation_undecidable_for_constraint`; a
query over a concrete value is decidable.

`compiler/parameterListAsTupleType` (`Parameters<typeof C>` for a class `C`)
converts. Measured on both dumps against the baseline: diagnostics +1 case,
0 lost; type lines unchanged (543,275 RIGHT).

## §3 TS2403: an object type against an enum is not identical

`identity.rs` declined every flags difference involving an enum, for two
reasons recorded in `decls.md` §2: one enum has two representations in this
port, and a qualified enum annotation (`M3.Color` in
`conformance/instantiatedModule`) resolves to an object-flagged type.

Probing `instantiatedModule` shows what that object-flagged type is: a
`TypeData::Named` interface image whose symbol is the enum itself. Upstream's
only object type carrying an enum symbol is `typeof E`, an anonymous type, so
the image is identifiable. The decline is narrowed to:

- both sides enum-like (the two-representation hazard): `Unknown`, unchanged;
- one side enum-like, the other not an object type (a narrowed enum that came
  back as `number`): `Unknown`, unchanged;
- one side enum-like, the other the misresolved image: `Unknown`;
- otherwise (`typeof E` vs `E`, `Object` vs `E`): `NotRelated`, which is
  upstream's flags-differ answer in `isTypeRelatedTo`.

Converts `compiler/duplicateLocalVariable4` and
`conformance/enumAssignabilityInInheritance` (messages identical to the
baseline). Measured numbers in §8.

**Falsifier.** A TS2403 that upstream does not report on an enum-vs-object
pair means TSR built the object side wrong for some other reason; the fix is
then in that producer, and the image test above is the place to extend.

## §4 Census: the generic declines are relater limits, not constraint-site bugs

Most remaining TS2344 misses relate a type parameter, an indexed access, a
conditional or a generic reference (`subclassThisTypeAssignable01`'s `State`
against `Lifecycle<Attrs, State>`, `constraintWithIndexedAccess`,
`genericDefaultsErrors`' `T` against `number`). The check declines those
through `type_argument_node_is_generic` and
`relation_undecidable_for_constraint`.

Measured with all three declines disabled (a local probe, not committed),
against the same build with them on: **0 cases gained, 37 lost** (28
EMPTY_RIGHT and 9 RIGHT turned WRONG by extra TS2344). The relater's
`NotRelated` on these pairs is wrong more often than right, so the declines
stay; the generic cases wait on relater arms (`relater.rs`, r5-relater4).

## §5 TS2344: a type parameter's default against its constraint

`checkTypeParameter` checks a parameter that has both a constraint and a
default: `checkTypeAssignableTo(defaultType, getTypeWithThisArgument(
instantiateType(constraintType, newSimpleTypeMapper(tp, defaultType)),
defaultType), tpNode.DefaultType, Type_0_does_not_satisfy_the_constraint_1)`.
TSR had no counterpart. `check_type_parameter_default_constraint` ports it,
called from the `TypeParameterDeclaration` arm of `check_node` right after the
circular-default check, which is native's order.

- The default is `getResolvedTypeParameterDefault`'s `Resolved` answer;
  `Circular` reports nothing (TS2716 has), and `Unsupported` (a default this
  port cannot build) declines.
- A declaration without its own default node reports nothing: native's error
  node is nil there.
- The relation is gated by the same generic declines as the reference check
  (§4).
- `getTypeWithThisArgument` is not ported, so a constraint that is a class or
  interface type declines; no measured case has one.

Measured with the commits above, both dumps: 0 lost, no verdict changed;
`compiler/genericDefaultsErrors` gains its two concrete lines (3,41 and
26,34). Its other six lines relate a type parameter (§4), and it also misses
TS2706. Median child CPU against the baseline binary at 21 samples:
domain-model 0.990, generic-imports 0.963.

## §6 TS2344: a bare type-parameter argument with a decided constraint

The §4 declines refuse every argument that names a type parameter. One shape
is decidable here: a written argument that is exactly a reference to a
declared type parameter (`F<U>`, no type arguments of its own) whose
`getConstraintOfType` is decided (`constraint_of_type`: `Nil`, or a
constraint that is itself not generic). The relater's type-parameter source
arm (`relater.go:3665`) relates that constraint, or `unknown`, to the
target; the target keeps its own decline.

Two further declines came from the first measurement (6 cases lost to extra
TS2344, all `EMPTY_RIGHT`/`RIGHT`):

- **A reference in a conditional type's true branch.** Upstream reads it
  through `getConditionalFlowTypeOfType`, which substitutes the check's
  implied constraint; TSR's reference is the bare parameter
  (`conditionalTypeClassMembers`, `privatePropertyInUnion`,
  `recursiveConditionalTypes2`, `signatureCombiningRestParameters3`/`4`).
- **A parameter of a class/interface with several declarations.**
  `getConstraintDeclaration` reads the merged parameter's first constrained
  declaration; TSR's parameter symbols are per declaration
  (`interfaceMergedUnconstrainedNoErrorIrrespectiveOfOrder`).

Measured with both, against the baseline, both dumps: 0 lost, no verdict
changed; type lines unchanged. `genericDefaultsErrors` gains 4 more correct
lines (4,59; 5,44; 27,52; 28,37). One wrong line is added in a case that was
already WRONG: `compiler/ramdaToolsNoInfinite2` (564,95). There `F extends
Function` resolves `Function` to the **global** interface instead of the
module import, the same import-resolution failure behind that case's extra
TS2552s, so the producer is the resolver, not this check. Median child CPU
against the baseline binary at 21 samples: domain-model 0.965,
generic-imports 0.977.

## §7 TS2403: a circular initializer's `any` is upstream's `any` (diff)

`conformance/witness` misses 7 TS2403 lines, all `Variable 'x' must be of
type 'any', but here has type 'number'` (and one `{ m: any; }`) for
`var co1 = (co1, 3); var co1: number;` and the like. Upstream's `any` is
`reportCircularityError`'s (`checker.go:18822`): the variable's type
resolution reaches itself and answers `anyType`. TSR reaches the same point,
`report_circularity_error` in `symbols.rs`, and answers `any` too. But
TS2403's `identity_side_is_trusted` (`check.rs`, `decls.md` §1) trusts an
`any` only where it was written, because elsewhere this port's `any` is
often "no better answer".

The fix belongs to the producer, not to a syntax guess (`box-protocol.md`
§3a). `r5-constraints2-circular-any-trust.diff` (`checker.rs`,
`symbols.rs`, `check.rs`; none owned here):

- `report_circularity_error` records the unannotated declaration in a new
  `circular_any_declarations` set when it answers `any`;
- `identity_side_is_trusted` trusts `any` for a variable or parameter
  declaration in that set.

It is limited to variables and parameters because the same fallback is
too coarse for functions (`symbols.rs` §221: upstream keeps the signature
and degrades only the return slot).

Measured on top of the §6 commit, both dumps: diagnostics +1 case
(`witness`, all 9 TS2403 lines with the baseline's messages), 0 lost; type
lines 543,275 RIGHT, 0 lost.

## §8 Measurements

Baseline: integration `2919d8c`, frozen dumps. Both loss checks empty unless
stated.

| State | diagnostics RIGHT+EMPTY_RIGHT | type lines RIGHT |
|---|---|---|
| baseline | 10,746 | 543,275 |
| §1 committed + §2 | 10,747 (+`parameterListAsTupleType`) | 543,275 |
| + §3 | 10,749 (+`duplicateLocalVariable4`, `enumAssignabilityInInheritance`) | 543,275 |
| + §5 | 10,749 (+2 lines in `genericDefaultsErrors`) | 543,275 |
| + §6 | 10,749 (+4 lines in `genericDefaultsErrors`, +1 wrong line in `ramdaToolsNoInfinite2`) | 543,275 |

Diagnostics counts are over all dump keys (plain and configured). The §1
diff measured with the full native skip (no namespace stop), on top of the
committed `constraints.rs`: diagnostics +3 cases, type lines +89 RIGHT and 3
lost (the `complexRecursiveCollections` lines of §1). With the namespace
stop, measured on top of the §6 commit: diagnostics +2
(`nonPrimitiveInGeneric`, `nonPrimitiveStrictNull`), 0 lost; type lines
543,275 → 543,317 RIGHT (+42), 0 lost.

## §9 Remaining clusters (plain keys, after the commits)

TS2344, 19 cases still differ on it:

- **Generic-pair relations, `relater.rs` (r5-relater4), about 11 cases.**
  The pairs are an indexed-access source (`constraintWithIndexedAccess`), a
  generic reference (`circularlyConstrained…`,
  `reactReduxLikeDeferredInferenceAllowsAssignment`: `GetProps<C>` against
  `Shared<…>`), `styledComponentsInstantiaionLimitNotReached`, a generic
  target (`subclassThisTypeAssignable01`'s `Lifecycle<Attrs, State>`,
  `genericDefaultsErrors`' `U extends T = number`), `complexRecursiveCollections`,
  `variadicTuples1`, `instantiationExpressionErrorNoCrash` and
  `tsxTypeArgumentResolution`. It also gives an extra TS2344 in
  `relatedViaDiscriminatedTypeNoError2`. §4 measures the relater on these
  pairs: wrong more often than right.
- **JSDoc type nodes are not visited, 3 cases.** Upstream reparses `@type`,
  `@extends` and `@template` into the tree and checks them with
  `checkTypeReferenceNode`; TSR's `check_node` walk does not reach JSDoc type
  expressions (`checkJsdocTypeTag4`, `extendsTag5`,
  `unmetTypeConstraintInJSDocImportCall`). This needs the JSDoc walk in
  `check.rs`/`jsdoc_annotations.rs`.
- **Generic import types, 1 case.** `get_type_from_import_type_node`
  (`declared.rs`) answers the error type for `import("./m").Foo<T>`, so
  `checkImportType`'s constraint check has no type
  (`unmetTypeConstraintInImportCall`).
- **Call-site overloads, 3 cases, `calls.rs` (main, `.980`).**
  `overloadResolution`, `…ClassConstructors` and `…Constructors` need
  `chooseOverload`'s type-argument filtering across several candidates
  (`candidateForTypeArgumentError` is the last candidate whose type
  arguments failed). TSR only checks a single generic candidate
  (`check_single_generic_candidate_arguments`). The same cases also miss the
  TS2345s that this filtering produces.

TS2403, 10 cases still differ on it (9 once the §7 diff lands):

- **Enum against enum** (`typeOfEnumAndVarRedeclarations`, `typeof E`
  against a written literal type whose members are `E`). The two-
  representation decline in `identity.rs` stays until enum member literals
  and the enum union are built consistently.
- **Inferred operands judged by assignability** (`parserCastVersusArrowFunction1`'s
  optional-parameter arrow, `objectLiteralContextualTyping`, `objectRest`,
  `indexSignatureTypeInference`, `contextualSignatureInstantiation`'s
  inferred `unknown`). The inferred side's producer is unsure (`decls.md`
  §2) or answers the error type.
- **Generic mapped identity** (`noExcessiveStackDepthError`'s
  `FindConditions<any>` against `FindConditions<Entity>`). `identity.rs`
  declines mapped types; the generic-mapped identity arm is unported.
- `FunctionAndModuleWithSameNameAndCommonRoot` and `conditionalTypes1` were
  not investigated.
- The brief's witness `tsr-2zk.20` (`declare var Symbol: number`) already
  reports at the baseline: `ES5SymbolProperty3`–`5` are RIGHT.
