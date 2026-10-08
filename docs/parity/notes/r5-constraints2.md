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
  §5.

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
baseline). Measured numbers in §5.

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

## §6 TS2344: a type parameter's default against its constraint

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

## §5 Measurements

Baseline: integration `2919d8c`, frozen dumps. Both loss checks empty unless
stated.

| State | diagnostics RIGHT+EMPTY_RIGHT | type lines RIGHT |
|---|---|---|
| baseline | 10,746 | 543,275 |
| §1 committed + §2 | 10,747 (+`parameterListAsTupleType`) | 543,275 |
| + §3 | 10,749 (+`duplicateLocalVariable4`, `enumAssignabilityInInheritance`) | 543,275 |

Diagnostics counts are over all dump keys (plain and configured). The §1
diff measured with the full native skip (no namespace stop), on top of the
committed `constraints.rs`: diagnostics +3 cases, type lines +89 RIGHT and 3
lost (the `complexRecursiveCollections` lines of §1). With the namespace
stop, diagnostics +2 (`nonPrimitiveInGeneric`, `nonPrimitiveStrictNull`), 0
lost; its type-line numbers are in the final report.
