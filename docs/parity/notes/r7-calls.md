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

## 2. CHECK-NON-NULL-CALLEE — the call resolves against the non-nullable callee

Routed by the integrator from r7-declared (their measured diff
`r7-declared-calls-nonnull-callee.diff` on `box/r7-declared`).

**Native.** `resolveCallExpression` (`checker.go:8495-8511`) strips an
optional chain (`getOptionalExpressionType`) and then passes every callee,
chained or not, through `checkNonNullTypeWithReporter`
(`checker.go:7413`, reporter `reportCannotInvokePossiblyNullOrUndefinedError`):
`unknown` under `strictNullChecks`, or a type whose non-nullable part is
nullable or `never`, becomes `errorType` and the call is `resolveErrorCall`'s;
otherwise the call resolves against `GetNonNullableType(funcType)`.

**Before.** `check_call_expression` applied `check_non_null_type` only on the
optional-chain arm; a plain call of `((o: number) => string) | undefined`
resolved against the union and typed as `error` where native gives `string`.
The reports (TS2721/TS2722/TS2723, TS18046/TS2571) were already the
diagnostic walk's (`check_non_null_callee`), so only the type road changes.

**Measured** against `5ea8f7e4`: types +11, 0 lost; diagnostics 0 gained,
0 lost (both dumps unfiltered). Perf (median child CPU, new/old): 21 samples
domain-model 0.981, generic-imports 1.250; at 41: 1.017 and 0.972.
generic-imports runs ~65 ms, so a 21-sample median there moves by ±25%
between runs; the 41-sample re-run is the measurement.

**Not ported here.** Native reports TS18048 ("'g' is possibly 'undefined'")
at the same callee as TS2722 for `const a = g(1)`: `getQuickTypeOfExpression`
(`checker.go:7376`, the variable initializer's quick type) and
`getEffectsSignature` (`flow.go:2065`) call `checkNonNullExpression` with the
default reporter. No corpus case misses such a TS18048 today; the one case
missing a report at a TS2722-class location (`mappedTypeIndexedAccessConstraint`
53:34) misses the TS2722 itself, not yet diagnosed.

Coverage bin on the commit: `checker_types` 8,678/9,538 (+1),
`checker_types_configured` 1,756/1,928 (+4), `diagnostics` 4,804/5,502
(unchanged).

## 3. A single generic construct signature reports (`.1153`, `.1158`, `.1172`)

r6-callreport's diff 3 ([r6-callreport.md](r6-callreport.md) §4,
[`r6-callreport-3-generic-new.diff`](r6-callreport-3-generic-new.diff)),
applied unchanged on `2745f162`. `chooseOverload`'s generic arm
(`checker.go:9046`) runs for a `new` whose single candidate is generic: its
written type arguments are checked against their constraints, and the
instantiation is checked with `reportErrors`. The instantiation is the one the
`new` type road published (`signatureLinks.resolvedSignature`); only when
none was published does the report run `check_generic_call_with`. A `new`
with no arguments and no generic rest needs no instantiation.

Round 6 held this diff for a +0.09% domain-model Ir cost over its diff 2.
Valgrind is not installed in this orb, so this gate's measure is the
protocol's median child CPU against the frozen `2745f162` binary at 41
samples: domain-model 0.979, generic-imports 1.000. The remaining cost r6
named (the published-signature reuse plus one argument relation on a
successful generic `new`) is the cost calls already pay on the same path.

**Measured** against `2745f162`: diagnostics +7, 0 lost; types 0 gained,
0 lost. Converted: classTypeParametersInStatics, dataViewConstructor,
genericClassWithStaticFactory, overloadresolutionWithConstraintCheckingDeferred,
exportAssignmentConstrainedGenericType, overloadResolutionClassConstructors,
typeArgumentInferenceConstructSignatures.

Coverage bin on the commit: `diagnostics` 4,811/5,502 (+7); type suites
unchanged.

## 4. TS2775 / TS2776 — assertion call targets (`checker.go:8353`)

Routed by the integrator from r7-flow.

**Native.** After resolution, `checkCallExpression` checks a call statement
(parent `ExpressionStatement`, no `?.`) whose signature returns `void` and has
a type predicate (an `asserts` signature): a callee that is not
`IsDottedName` is TS2776; otherwise, when `getEffectsSignature(node)` is nil
(some name in the dotted target has no explicit type, so
`getTypeOfDottedName` cannot reach the signature without flow analysis),
TS2775 at the callee, with `getTypeOfDottedName`'s TS2782 related
information.

**Port.** `check_assertion_call_target` on the diagnostic walk, after the
resolution reports. It reads the resolved signature the way this port's
`getEffectsSignature` does (the non-nullable callee's sole non-generic call
signature, else `resolve_call_signature_at`), answering before any resolution
when no call signature of the callee has a predicate, and calls `flow.rs`'s
`get_effects_signature` for the TS2775 test. The integrator approved one
visibility-only change in `flow.rs`: `get_effects_signature` is `pub(crate)`.

**Accepted:** the TS2782 related information is not attached (the
diagnostics suite compares code and position).

**Held, not committed.** Measured against `41ba17fa`: +1
(assertionTypePredicates2), 3 lost (EMPTY_RIGHT → EMPTY_WRONG):
`privateNamesAssertion` (both targets) and `requireAssertsFromTypescript`.
assertionTypePredicates1 gains its four reports but also gets two false ones
(150:9, 192:9), so it stays WRONG. Each false TS2775 is a gap in `flow.rs`'s
`getTypeOfDottedName` / `getExplicitTypeOfSymbol` (`flow.go:2122`, `:2155`),
which the check now exposes:

- the for-of arm (`flow.go:2176`): a `for (let item of items)` variable has
  the explicit iterated type of `getTypeOfDottedName(items)`
  (assertionTypePredicates1 150:9 and 192:9). The same gap leaves
  `item.assertIsTest2(); item.z` un-narrowed, the extra TS2339 at 151:14;
- the private-identifier arm (`flow.go:2137`): `this.#p1(v)` looks up
  `GetSymbolNameForPrivateIdentifier` (privateNamesAssertion);
- a JS `const { art } = require('./ex')` binding is an alias that
  `resolveSymbol` follows to the declared function
  (requireAssertsFromTypescript 4:1).

The port is kept as
[`r7-calls-assertion-target.diff`](r7-calls-assertion-target.diff) until those
arms land in `flow.rs` (routed).

## 5. The overload walk checks a nested call argument under its first candidate

Routed by the integrator from r7-perf's jsTyping delta (cluster 5):
`new Map(xs.map((x, i) => [x, i]))` reported a false TS2769.

**Forcing constraint.** The failure is not specific to `new`: any overload
set of two generic signatures fails the same way
(`declare function f3<K, V>(entries?: readonly (readonly [K, V])[] | null)`
plus an `Iterable<readonly [K, V]>` overload, called `f3(xs.map(...))`); a
single generic signature passes. `transcribed_generic_set_walk_worker`
(native `chooseOverload`, `checker.go:9025`) checked every
non-context-sensitive argument once, with no candidate context, before trying
any candidate. Native never does: `inferTypeArguments` checks each argument
with `checkExpressionWithContextualType(arg, paramType, context)`
(`checker.go:9485`). For most arguments the type is the same either way. A
call argument's is not, because its own resolution infers from the
contextual return type (`checker.go:9419`) and is cached for the node
(`signatureLinks.resolvedSignature`) from its first check. Checked without
context, `xs.map((x, i) => [x, i])` resolves `U = (string | number)[]` and
no candidate relates. Under candidate 1's `readonly (readonly [K, V])[]`
the arrow's array literal is a tuple and `U = [string, number]`.

**Change.**

- `is_unchecked_resolving_argument`: a call, `new` or tagged-template
  argument (through parentheses) with no published type yet;
- the walk defers such an argument's first check;
- `overload_pass` makes it under the first candidate that relates it: a
  generic candidate publishes itself as the call's memo
  (`call_inference_signatures`, the uninstantiated parameter is the
  contextual type, as `inference.rs` already does for intra-expression
  inference sites) for that one check, before `check_generic_call_with`; a
  non-generic candidate checks it through `check_argument_in_candidate_context`;
- later candidates and the second relation read the cached check, as native
  reads the cached resolution;
- a declined walk restores the deferred subtrees (they join the restore
  stack).

**Rejected: deferring every argument.** Native defers all of them, but this
port's walk relates the context-free types in its subtype/skip passes, and an
identifier or literal argument's type does not depend on a nested resolution.
Widening the deferral is the move if a non-call argument is found whose
context-free type differs from its first candidate check.

**Measured** against `9137c27c` rebased on `7e9f37eb`: diagnostics +1
(inferFromGenericFunctionReturnTypes3), 0 lost; types +5 (all in
inferFromGenericFunctionReturnTypes3), 0 lost. Perf (median child CPU, 21
samples): domain-model 0.969, generic-imports 0.923.
