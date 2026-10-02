# Primitive assignability to generic parameters

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 9afcc9f3, evidence 61908a39: 455,875/478,855 RIGHT.

A primitive or unknown value cannot satisfy an arbitrary target type parameter.
The old relation returned Unknown for these assignability questions, allowing
inference to retain a candidate that native rejects. The port now returns
NotRelated after the simple relation has handled any, never and non-strict
nullability. The rule is limited to Assignable; subtype and comparable keep their
existing paths. Native sources are isSimpleTypeRelatedTo and
structuredTypeRelatedToWorker in internal/checker/relater.go.

This requires the native getConditionalType deferral check before ordinary
assignability chooses a branch. A concrete check such as number against a generic
extends operand T must remain deferred; rejecting number -> T is not proof that
number extends T is permanently false. The helper ports the
InstantiableNonPrimitive/Index flags and union/intersection propagation from
getGenericObjectFlags/isDeferredType. Generic mapped/tuple/string-like cases and
the permissive/restrictive instantiation machinery remain incomplete; references
that merely contain a parameter are deliberately not classified as generic.

Three relation tests cover primitive/literal domains, unknown with five different
constraints, direction, comparable behavior, any/never and strict/non-strict null
checks. Native inputs /tmp/tsr-99-primitive-parameter.ts and
/tmp/tsr-99-unknown-parameter.ts were checked with explicit --strict true and false.
Only the intended assignment directions diagnose. The conformance test pins 11
outcomes for deferred generic operands, concrete substitution and callback
inference after an invalid argument. Native declaration input/output:
/tmp/tsr-99-deferred-extends.ts and /tmp/tsr-99-deferred-extends-native/.
Native diagnoses the three invalid callback calls while retaining string results.

The full candidate verdict /tmp/tsr-99-primitive-assignable-union.tsv gains 14
WRONG-to-RIGHT assertions, seven each in genericCallWithFunctionTypedArguments
and genericClassWithFunctionTypedMemberArguments. There are zero RIGHT losses
and zero GAP-to-WRONG rows. Fourteen already-WRONG rows change: four in
complicatedIndexedAccessKeyofReliesOnKeyofNeverUpperBound, four in
genericFunctionInference1, two in typeInferenceCacheInvalidation and four in
conditionalTypes2. They remain incorrect and are not counted as improvements.
In particular NewChannel<EmailChannel | TextChannel> becomes NewChannel<never>
where native expects NewChannel<TextChannel>; permissive conditional comparison
and constraint/contextual inference still need work (tsr-8).

## Measured prerequisites still deferred

The fresh-signature experiment allocates distinct retained own parameters,
retains their constraint mapper, and adds native recursion identities to bound
expanding relations. Its full verdict /tmp/tsr-99-fresh-signatures-recursion.tsv
has 35 gains, 13 RIGHT losses, two GAP-to-WRONG rows and six changed WRONG rows.
The losses are asyncYieldStarContextualType (2), chainedCallsWithTypeParameter-
ConstrainedToOtherTypeParameter2 (2), nonInferrableTypePropagation1 (8) and
recursiveConditionalTypes (1). Source copies are /tmp/tsr-99-fresh-current/.
Without the recursion guard, mapGroupBy expands until multi-gigabyte allocation;
the owned processes were stopped and their incomplete files are not measurements.
The new unknown rejection recovers the two chained-call losses in a focused run.

Combining fresh signatures with the recursive contextual mapper gives 65 gains
and 17 RIGHT losses, plus two GAP-to-WRONG and six changed WRONG rows:
/tmp/tsr-99-fresh-context.tsv. This removes the old identity-collision losses but
retains four mappedTypeContextualTypesApplied losses. Its inference source is
/tmp/tsr-99-fresh-context-inference.rs. Neither experiment is included here.

Recursion alone, including a follow-up port of source/target recursion flags,
gives two GAP-to-RIGHT rows and two GAP-to-WRONG rows in
recursiveTypeAliasWithSpreadConditionalReturnNotCircular. The latter exposes
uninstantiated conditional tuple elements. Verdict /tmp/tsr-99-recursion-flags.tsv;
source copies /tmp/tsr-99-recursion-flags-{checker,declared,relater}.rs.

Extending the primitive rejection to subtype relations gains 36 but loses 35
RIGHT rows without generic conditional deferral. With deferral it still loses
two RIGHT rows and adds two GAP-to-WRONG rows: generic predicate branch unions
need their native reduction, and non-null generic constraints remain incomplete.
Verdicts /tmp/tsr-99-primitive-parameter.tsv and
/tmp/tsr-99-primitive-parameter-deferred.tsv. The Assignable-only scope is a
relation boundary, not a fixture exception. Unknown-only rejection has no corpus
movement, but directly fixes the chained-call prerequisite above.

Main verification passes: 212 release workspace result blocks, clippy, formatting,
diff checks and 3,330 upstream anchors. The simplification pass kept the existing
relation ordering and added only the recursive generic-flag helper. Sequential
primary-thread correctness, adversarial, standards and testing review found no
retained actionable findings; no independent or cross-model review is claimed
under the user's no-delegation rule. Receipt:
/tmp/compound-engineering-501/ce-code-review/generic-assignability/review.json.
Frozen source hash: 1f9ae55328452b7bda1c2c1a929c15e80b876cddf1a4c3ddcc266e960de99caa.
