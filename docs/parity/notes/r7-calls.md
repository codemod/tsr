# r7-calls — call resolution and its reports (`tsr-2zk.1269`)

Round-7 lane box on epic `tsr-2zk` ([round7.md](../round7.md)). Owned files:
`calls.rs`, `call_arity.rs`, `call_reports.rs`, `decorators.rs`,
`union_signatures.rs`, `type_argument_arity.rs`. Native anchors are
`vendor/typescript-go` @ `5b1047d`; expectations were checked against the
pinned native oracle `target/tsgo-pinned`.

## 0. Base

Frozen base: `origin/main` @ `9020aa67` (`scripts/parity_gate.sh freeze`).

- diagnostics dump: 12,238 cases, 5,741 RIGHT / 5,610 EMPTY_RIGHT /
  852 WRONG / 35 EMPTY_WRONG;
- types dump: 556,357 lines, 551,176 RIGHT / 4,508 WRONG / 673 GAP.

## 1. `.1267` — the `isSignatureApplicable(reportErrors)` hook re-landed

**What was backed out, and why.** r6-callreport's applicability diff
([r6-callreport.md](r6-callreport.md) §2–3) replaced `calls.rs`'s
`check_instantiated_candidate_arguments` with `call_reports.rs`'s single
native-shaped pass. Batch CJ+CK+CL measured four losses on top of the rows
landed after r6's base (`eee504b`), and backed the hook out
([round5.md](../round5.md), "Batch CJ+CK+CL: r6-callreport's hook diffs backed
out"). Re-measured on this lane's base, the hook alone is +18 / −2:

- **indexedAccessRelation.** `this.setState({ a: a })` against
  `Pick<S & State<T>, "a">`. r6's certification declined every object literal
  whose source or target could contain type variables. The replaced code had
  no such decline, and it is not a native branch: the variables are the
  enclosing declaration's (`S`, `T`), which the call's inference leaves alone,
  and the literal's published type was checked under the uninstantiated
  parameter whose property names and non-literal member types do not follow
  the contextual type. The decline is removed for object literals. The two
  that remain (a member that could keep a literal type,
  `literal_member_may_keep_literal`; a target member this port types but
  cannot resolve, `absent_member_is_unreadable`) are the ones whose published
  type can differ under the instantiated target. Array literals keep the
  type-variable decline: their tuple-ness follows the context.
- **typeParameterConstModifiers.** `f5<const T>(obj: { x: T, y: T })` with
  `{ x: [1, 'x'], y: [2, 'y'] }`. Native infers
  `readonly [1, "x"] | readonly [2, "y"]`; TSR infers `readonly [1, "x"]`
  and the pass reported TS2322 at `2` and `'y'`. The defect is upstream of
  the report: native marks const-context literal types
  `ObjectFlagsObjectLiteral` / `ObjectFlagsArrayLiteral` (`checkObjectLiteral`,
  `createArrayLiteralType`), so `unionObjectAndArrayLiteralCandidates`
  (`inference.go:1470`) unions them into one candidate. TSR's
  `union_object_and_array_literal_candidates` (`widening.rs`) does not
  recognize them (probe: `f5({ x: { a: 1 }, y: { b: 'y' } })` prints
  `{ readonly a: 1; }` where native prints the normalized union). The faithful
  fix is in the const-context literal producers (`array_literals.rs`,
  `objects.rs`; r7-contextual), routed in the report. Until then an object or
  array literal argument of a candidate declaring a `const` type parameter
  declines (`CandidateContext::Instantiated { const_type_parameters }`). This
  is a decline waiting on a named missing piece, not a native branch; the
  falsifier is the producer fix landing, after which the decline must go
  (it can only hide reports).
- **The excess-property arm** (batch CD, `relater.go:2714`
  `hasExcessProperties` before the literal gate) is carried into
  `report_signature_applicability`: under an instantiation, a failing fresh
  object literal against a non-union target reports its excess property
  before certification, since the property names do not follow the context.
  excessPropertyCheckWithEmptyObject and reverseMappedTypeLimitedConstraint
  stay RIGHT with it.

**Measured** (base `9020aa67`, both dumps unfiltered, `parity_gate.sh
compare`): diagnostics +18, 0 lost, 0 missing; types 0 gained, 0 lost.
Converted: chainedCallsWithTypeParameterConstrainedToOtherTypeParameter,
chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2,
circularResolvedSignature, contextualTypingOfGenericFunctionTypedArguments1,
fixingTypeParametersRepeatedly2, mismatchedExplicitTypeParameterAndArgumentType,
promiseChaining1, promiseChaining2, setMethods,
typeParameterFixingWithContextSensitiveArguments2,
typeParameterFixingWithContextSensitiveArguments3,
destructuringParameterDeclaration5, genericCallWithFunctionTypedArguments,
genericCallWithGenericSignatureArguments2,
genericClassWithFunctionTypedMemberArguments, mappedTypeInferenceErrors,
partiallyAnnotatedFunctionInferenceWithTypeParameter,
typeParameterAsTypeParameterConstraint2.

Perf (median child CPU, new/old, against the frozen base binary): 21 samples
domain-model 1.042, generic-imports 1.116; re-run at 41: 0.994 and 1.029.
The pass does the relation work of the code it replaces (one relation per
argument up to the first failure, on the single-candidate report sites); the
certification rules run only once a pair fails.

`ReportArgument::Synthetic::spread` (`isSpreadArgument`) is set but not yet
read; it is read once `getSpreadArgumentType` is ported
(CALL-SPREAD-ARGUMENT-APPLICABILITY).

Coverage bin on the commit: `checker_types` 8,677/9,538 (unchanged),
`diagnostics` 4,804/5,502 (4,786 at base).
