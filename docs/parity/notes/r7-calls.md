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

**Decline removed** (after `ca1239ef`, r7-contextual's const-context
literal markers, landed in batch 6): `union_object_and_array_literal_candidates`
now unions a `const` type parameter's literal candidates, so the
`CandidateContext::Instantiated { const_type_parameters }` decline is deleted
with its field. Measured against `0b86f9d6` on `d3a34908`: diagnostics 0
gained, 0 lost; types 0/0 (typeParameterConstModifiers stays RIGHT on the
producer fix, now without the decline). Perf (21 samples): domain-model
0.959, generic-imports 0.939.

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

**Re-measured** on `d3a34908` (batch 6, with r7-flow's for-of and
private-identifier arms) against `0b86f9d6`: +2 (assertionTypePredicates1,
assertionTypePredicates2), 1 lost (requireAssertsFromTypescript,
EMPTY_RIGHT → EMPTY_WRONG). The remaining false TS2775 is the JS
`const { art } = require('./ex')` binding, which this port's binder does not
make an alias (`resolveSymbol` follows it to the declared function natively);
routed to r7-shared. The diff stays held while it costs a RIGHT case.

**Landed** on `432b94e3` (batch 40 base) after r7-shared's `7c17ce54` (a JS
require binding element is an alias). The port is applied as recorded,
with one change from the held diff. The jsTyping leg showed a false TS2775
at `debug.ts(269,9)`: an overloaded generic `asserts` callee, which
`get_effects_signature` fails to resolve because it resolves without the
call node. For a call statement whose resolved signature has a predicate,
native's `getEffectsSignature` is nil exactly when `getTypeOfDottedName`
is, so the test reads `get_type_of_dotted_name` (made `pub(crate)`, the
visibility the integrator granted) instead. `get_effects_signature` stays
private. `r7-calls-assertion-target.diff` stays as the record of the held
state.

**Measured** against `432b94e3` on `368b23ce`: diagnostics +2
(assertionTypePredicates1, assertionTypePredicates2), 0 lost; types 0/0;
jsTyping new_false 0, lost_true 0 (the `debug.ts` line is gone); Ir
generic-imports 1.0001, domain-model 1.0008 (the rule reads the callee's
call signatures for every call statement before it can answer); exact 0
gained, 0 lost (TS2782's related information is missing).

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

## 6. A tagged template's single generic candidate reports (TAGGED-TEMPLATE-EFFECTIVE-ARGS)

**Native.** `resolveTaggedTemplateExpression` (`checker.go:8719`) runs
`resolveCall` over `getEffectiveCallArguments`' tagged arm
(`checker.go:30042`): a synthetic `TemplateStringsArray` argument, then one
argument per substitution. A single generic candidate takes `chooseOverload`'s
generic arm (`checker.go:9046`) like a call's: written type arguments are
checked against their constraints and instantiate it, otherwise it is
inferred, and `reportCallResolutionErrors` re-runs `isSignatureApplicable`
with `reportErrors` over the instantiation.

**Before.** `check_candidates_arity` handed only calls and `new`s a
`CallArity::ApplicableGeneric`; a tagged template's single generic candidate
fell to `Applicable(None)`, which the tagged diagnostic walk ignores.

**Port.** `check_single_generic_tag_arguments`:

- written type arguments: `check_call_type_argument_constraints` (now reading
  a tagged template's type arguments too, through `call_type_arguments`), and
  the instantiation is `fillMissingTypeArguments`' mapper
  (`fill_written_type_arguments`, split out of the constraint check so both
  share it) over the candidate;
- otherwise the instantiation is inferred the way the tagged template's type
  road infers it (`check_tagged_template_expression`): over the candidate
  without its strings parameter, against the substitutions, and the strings
  parameter is put back;
- `report_signature_applicability` runs over `report_call_arguments`, which
  already builds the synthetic `TemplateStringsArray` first.

**Declines** (silent, as before): a strings parameter that mentions type
variables (that inference site is not modelled on the type road either), a
context-sensitive substitution with no instantiation published by the type
road, and a non-array rest (`getSpreadArgumentType`).

**Measured** against `23550f9d` (on `660718af`): diagnostics +3, 0 lost; types
0/0. Converted: taggedTemplateStringsTypeArgumentInference,
taggedTemplateStringsTypeArgumentInferenceES6, taggedTemplatesWithTypeArguments2.
Perf (median child CPU, 41 samples): domain-model 1.010, generic-imports 0.916
(21 samples: 1.052 / 0.919).

**Remaining in the cluster.** taggedTemplateContextualTyping1/2 miss TS2345
reported *inside* a callback substitution (`x<number>(undefined)`), where the
callback's parameter `x` should be contextually typed `<T>(p: T) => T` by an
overloaded tag. That is the substitutions' contextual typing under an
overloaded tag (`getContextualTypeForArgument` through the tag's resolved
signature), not this report. taggedTemplateStringsWithOverloadResolution1/3
are overloaded tags: the shifted-candidate selection in
`check_tagged_template_expression` (TS2769/TS2741/TS2551).

Coverage bin on the commit: `diagnostics` 4,853/5,502; type suites unchanged.

## 7. Decorators resolve as calls (RESOLVE-DECORATOR-CALL-ERRORS)

**Forcing constraint.** `resolveDecorator` (`checker.go:8743`) and
`checkDecorator` (`checker.go:6061`) were unported: no decorator ever
reported TS1238/TS1239/TS1240/TS1241 (resolution under the declaration
kind's head), TS1329 (potentially uncalled), or TS1270/TS1271 (return type).
`decorators.rs` already computed the decorator call signature
(`getDecoratorCallSignature`, `checker.go:30155`) for contextual typing.

**Port** (`calls.rs` `check_decorator_diagnostics`, dispatched from
`check.rs`'s node walk by the one arm the integrator granted):

- `checkDecorators`' gate: `NodeCanBeDecorated` (the shared
  `node_can_be_decorated`; a legacy private-named member is TS1206's alone,
  privateNamesAndDecorators);
- `resolveDecorator`: the untyped call; `isPotentiallyUncalledDecorator`
  with `getDecoratorArgumentCount` (`checker.go:9183`), TS1329 at the
  decorator, spelled from the dotted name (the checker holds no source text,
  so another expression shape declines); no call signature
  (`invocationErrorDetails` chained under the head);
- `resolveCall` over `getEffectiveDecoratorArguments` (`checker.go:30142`):
  synthetic arguments typed by the decorator call signature's parameters,
  `reorderCandidates`, then `hasCorrectArity` with the decorator argument
  count. No candidate of the right arity is `getArgumentArityError` with the
  decorator messages (`The_runtime_will_invoke_the_decorator_with…`) and the
  decorator as `getErrorNodeForCallNode`'s node; a single non-generic
  candidate is reported by `call_reports.rs`. Every report is chained under
  the head with `Diagnostic::new_chain` (`chain_reports_under`);
- `checkDecorator`'s return check against the decorator call signature's
  return type, TS1270, or TS1271 for a legacy property or parameter
  decorator, over the resolved candidate (the single candidate itself when
  resolution failed, `getCandidateForOverloadFailure`).

`decorators.rs` gains `decorator_call_signature`, which reads the signature
off the function type `contextual_type_for_decorator` already publishes for
the declaration. It adds no cache, owner or traversal (the existing
`DecoratorTypes` cache is the one key).

**Synthetic arguments are not elaborated.** Native `elaborateError` never
elaborates a `SyntheticExpression`, so `report_signature_applicability` now
relates a synthetic argument (a decorator's, a tagged template's strings
array, a spread tuple's element) and reports it through
`report_relation_failure` with no source node. Before, a synthetic argument
went through `report_argument_failure`, which elaborates at the node it is
given: for a decorator that is the decorator expression, an arrow function
in `@((a: any) => {})`. The same change reads `ReportArgument::Synthetic`'s
`spread` flag (`checkSyntheticExpression`: a spread synthetic is its element
type).

**Declines, stated:**

- a generic or overloaded decorator after its arity pass: inference over
  synthetic arguments (`inferTypeArguments` with no argument expressions) is
  not ported (decoratorCallGeneric, decoratorOnClassMethod8's TS1270);
- a candidate with a `this` parameter;
- the return check when the relation is undecided. ES member decorators'
  expected returns hold `decorator_function_type`'s function types, which
  are `TypeData::Named` with `signature_types`, not anonymous function types
  the relater decides, so potentiallyUncalledDecorators' member TS1270s stay
  missing. Re-minting them as `TypeData::Anonymous` needs a symbol choice
  for a type native creates symbol-less, and moves the contextual types
  those same types serve. It is a separate measured change.

The detail chain of `invocationErrorDetails` ("Type 'typeof CtorDtor' has no
call signatures") is not built (`invocation_error` emits the head only).

**Measured** against `f2f6696f`: diagnostics +12, 0 lost; types 0/0.
Converted: sourceMapValidationDecorators, constructableDecoratorOnClass01,
decoratorOnClass8, decoratorOnClassConstructor2, decoratorOnClassConstructor3,
decoratorOnClassConstructorParameter1, decoratorOnClassMethod10,
decoratorOnClassMethod6, decoratorOnClassProperty11, decoratorOnClassProperty6,
decoratorOnClassProperty7, esDecorators-arguments. A first draft without the
`NodeCanBeDecorated` gate lost privateNamesAndDecorators (TS1240/TS1241 on a
legacy private-named member, which native never checks). Perf (21 samples):
domain-model 1.002, generic-imports 1.005. Coverage: `diagnostics` 4,869,
`diagnostics_configured` 981.

## 8. Non-generic overloads re-check literal arguments per candidate (OVERLOAD-FAILURE-REPORT)

**Native.** `chooseOverload` (`checker.go:9025`) runs `isSignatureApplicable`
per candidate, which checks every argument with
`checkExpressionWithContextualType(arg, paramType)`, uncached. An object or
array literal's type follows the candidate's parameter (literal widening,
tuple-ness, member contexts). When every candidate fails,
`reportCallResolutionErrors` re-checks the last one with `reportErrors`,
chained under `The_last_overload_gave_the_following_error` and
`No_overload_matches_this_call`.

**Before.** `check_overload_candidates_arguments` declined any call with an
object or array literal argument, because it could read only published types.

**Change.**

- An object or array literal argument with no nested call, `new`, tagged
  template, function, arrow or class (`literal_subtree_has_resolution`) is
  re-checked under each candidate (`check_literal_argument_in_candidate`):
  - its subtree is evicted and checked with the candidate as the call's memo
    (`call_inference_signatures`);
  - its published state (`node_types`, `resolved_call_signatures` and
    property `symbol_types`, the set `evict_subtree` clears) is restored
    afterwards (`literal_subtree_state` / `restore_literal_subtree_state`).
- The last candidate's report is made while its literals hold the last
  candidate's types, so elaboration reads them, and is restored afterwards.
- `report_signature_applicability` now treats a written object literal as a
  fresh literal (the excess-property check first) whether its type is the
  published one or one checked under the candidate (the `written` slot).
- With several candidates the report is chained, not relabelled
  (`Diagnostic::new_chain` twice, TS2769 head).

Class literals, literals with a nested resolution, context-sensitive
functions and tagged templates (no call memo for their substitutions) keep
the decline.

**Accepted:** `hasExcessProperties`' rejection of a candidate during
selection is not modelled, because this port's relater does not test excess
properties. A candidate native rejects only for an excess property is taken
as applicable, and the call stays silent.

**Measured** against `ea1a1286`: diagnostics +7, 0 lost; types 0/0.
Converted: arrayConcatMap, functionOverloads40, functionOverloads41,
heterogeneousArrayAndOverloads, overloadResolutionTest1,
taggedTemplateStringsWithOverloadResolution1(_ES6). Perf (median child CPU,
41 samples, quiet machine): domain-model 0.975, generic-imports 0.988. A
first 41-sample run read generic-imports 1.041 while another build was
compiling; a same-binary self-comparison reads 1.000. Coverage:
`diagnostics` 4,880/5,502.

## 9. A generic overload set with written type arguments runs the overload walk

**Forcing constraint.** jsTyping's `checker.ts` 9968/9981 (exposed by the
held spread arm, `box/r7-calls-held`): `makeSerializePropertySymbol<ClassElement>(...)`
calls a set of two generic overloads with a written type argument. Native's
`chooseOverload` (`checker.go:9025`) skips a candidate failing
`hasCorrectTypeArgumentArity` (`:9214`), checks a generic one's written
arguments against their constraints (`checkTypeArguments`, `:9222`; a
failure makes it `candidateForTypeArgumentError`), instantiates it with them
(`getSignatureInstantiation`), and runs `isSignatureApplicable`. In this
port, `choose_ordered_overload` sent written type arguments to the
transcribed walk only when exactly one candidate was generic (§391), and
gapped otherwise. The call typed as `error`, `isArray(result)` narrowed it
to `readonly unknown[]`, and a spread of it related `unknown`.

**Change.**

- `overload_pass` applies `hasCorrectTypeArgumentArity` and a non-reporting
  `checkTypeArguments` (`check_call_type_argument_constraints_with`,
  `report == false`) per candidate when the call writes type arguments. A
  constraint failure skips the candidate; an undecided constraint is
  undecidable.
- `check_generic_call_with`'s written arm already instantiates a candidate
  with the written arguments without inferring.
- `choose_ordered_overload` lets written type arguments reach the walk in its
  all-generic and truncated-prefix branches, when the walk can read them from
  the call node (`walk_reads_written_type_arguments`).

Probe: `mk<CE>(mkCE, true)` over `mk<T>(c: () => T, u: true): …` and
`mk<T>(c: () => T, u: false): …` now answers the first overload's
instantiation, as tsgo does (it was `error`).

**Measured** against `9c79de88` on `f960020e`: types +15, 0 lost
(callbacksDontShareTypes 10, tupleTypeInference 3, overloadResolution 2);
diagnostics 0/0. jsTyping 127 → 127, new_false 0, lost_true 0. Perf (median
child CPU, 41 samples): domain-model 0.911, generic-imports 1.000.

## 10. The no-call-signature arm: `never`, incompatible generic unions, construct-only callees

**Native.** `resolveCallExpression` (`checker.go:8511-8560`) reports
TS2348 when `getSignaturesOfType(apparent, Call)` is empty and construct
signatures exist, and `invocationError` (TS2349) when both lists are empty
and the call is not untyped. Three callee shapes reach that arm in native
and did not here:

- **`never`**: `getSignaturesOfType(never)` is empty (not a structured
  type), and `isUntypedFunctionCall` excludes a `never` apparent type.
  This port's shared resolver (`signatures_of_type_kind`) answers `None`
  for `never`, so the head declined. `head_signatures` now answers the
  empty list (neverTypeErrors1/2).
- **A union of generic signatures with different type parameters**:
  `getUnionSignatures`' master-list pass (`checker.go:21155`) sets the result
  to nil when a member's type parameters are not
  `compareTypeParametersIdentical` (`relater.go:2247`) to a generic result's.
  `union_signatures.rs` tried to align them, and undecided when it could
  not. It now ports the identity test (`union_type_parameters_identical`:
  same count, each constraint identical to the other's instantiated into its
  parameters, defaults not compared) and answers the empty list
  (betterErrorForUnionCall's `fnUnion2`).
- **Construct-only callees with type variables** (`new (arg: T) => Date`,
  `I1<T>` with a construct signature): the head's
  `could_contain_type_variables_at_head` refusal declined. A type with
  construct signatures has a decided call list, so the refusal no longer
  applies when construct signatures exist (TS2348, genericConstructorFunction1).
  It is also narrowed for a union every member of which has a decided,
  non-empty call list (`union_members_have_call_signatures`). That union's
  empty composite is `getUnionSignatures`' own verdict
  (`invocationErrorDetails`' "Each member of the union type … has
  signatures, but none of those signatures are compatible with each other").

**Measured** against `0ff376e0`: diagnostics +4 (betterErrorForUnionCall,
genericConstructorFunction1, neverTypeErrors1, neverTypeErrors2), 0 lost;
types 0/0; jsTyping 127 → 127, new_false 0, lost_true 0. Perf (21 samples):
domain-model 0.975, generic-imports 1.025.

## 11. The `new` head: incompatible construct unions and TS2350 over an overload set

**Native.** `resolveNewExpression` (`checker.go:8576`) reports
`invocationError` (TS2351) when neither list has signatures. When there are
call signatures but no construct signatures, it `resolveCall`s them and,
without `noImplicitAny`, reports TS2350 (`Only_a_void_function_can_be_called_with_the_new_keyword`)
for a declared, non-`void` return.

**Change.**

- `check_new_expression_head`'s type-variable refusal is narrowed, as the
  call head's was (§10), for a union every member of which has a decided,
  non-empty construct list. `new ctorUnion2("")` over two generic construct
  signatures with different type parameters reports TS2351 (the empty
  composite is §10's `compareTypeParametersIdentical` verdict).
- TS2350 read only a sole non-generic call signature.
  `new_head_candidate` also answers for an overload set when the `new` has
  no argument list and no type arguments. `hasCorrectArity` then passes only
  a candidate whose minimum argument count is 0, and there is no argument to
  check, so `chooseOverload` picks the first such candidate in
  `reorderCandidates` order (declined when it is generic).

**Measured** against `3c35fa53` on `4a5c81bf`: diagnostics +1 (intTypeCheck's two TS2350),
0 lost; types 0/0; jsTyping new_false 0, lost_true 0. newOperator's
TS2351 at 46:5 now matches; the case stays WRONG on a missing TS7053
(56:20, an element access, outside this lane). Callgrind Ir (single-threaded):
generic-imports 1.0001, domain-model 1.0000.

## 12. A `new` over non-generic construct overloads reports like a call

**Native.** `resolveNewExpression` (`checker.go:8576`) runs `resolveCall`
over the construct signatures exactly as `resolveCallExpression` does over
call signatures: `chooseOverload` per candidate, then
`reportCallResolutionErrors` (TS2769 chained under
`The_last_overload_gave_the_following_error`).

**Before.** `check_candidates_arity` handed `CallArity::ApplicableOverloads`
only to calls and tagged templates. A `new` fell to `Applicable(None)`, which
reads the transcribed walk's verdicts; that walk does not run for an
all-non-generic set, so `new Foo(f1)` over `constructor(s: string)` /
`constructor(n: number)` was silent.

**Change.** A `new` with an argument list takes the same arm, and
`check_new_expression_diagnostics` runs `check_overload_candidates_arguments`
over it (§8's per-candidate literal re-check and chained report; the call
memo `contextual_type_for_argument` reads is shared by `new`).

**Measured** against `f5bcfcd7`: diagnostics +1 (constructorOverloads1), 0
lost; types 0/0; jsTyping new_false 0, lost_true 0; callgrind Ir
generic-imports 0.9999, domain-model 0.9999.

## 13. A single non-generic candidate reports over spread arguments (CALL-SPREAD-ARGUMENT-APPLICABILITY)

**Native.** `isSingleNonGenericCandidate` (`checker.go:9030`) does not look
at spreads. `getEffectiveCallArguments` (`checker.go:30042`) expands a tuple
spread into synthetic arguments (a rest or variadic element spread-flagged)
and keeps any other spread as one argument. `isSignatureApplicable` checks a
spread argument with `checkSpreadExpression` (its iterated element type) and
a spread synthetic with `checkSyntheticExpression` (the element type), each
against `getTypeAtPosition`.

**Before.** `check_candidates_arity` returned `Applicable(None)` for any
call with a spread, so a single non-generic candidate never reached the
report pass.

**Change.** The spread and count conditions are dropped from the
single-non-generic arm. The report pass already builds the effective
argument list (`report_call_arguments`, `tuple_spread_elements`); a written
spread is typed by `check_expression`'s spread arm (its element type) and a
spread synthetic by its element type (§7). A non-array rest
(`getSpreadArgumentType`) still declines. Generic and overloaded candidates
keep their spread declines.

**Measured** against `d0a068a2` on `41f3c70b`: diagnostics +4, 0 lost; types
0/0. Converted: callWithSpread2, callWithSpread3, callWithSpread5,
iteratorSpreadInCall6. jsTyping new_false 0, lost_true 0. Callgrind Ir:
generic-imports 0.9999, domain-model 1.0003. Exact oracle 9,582 → 9,584,
exact_lost 0.

It was held (`box/r7-calls-held`) while the jsTyping leg refused it: it
exposed three false TS2345s from type-road defects elsewhere. Two came from
the written-type-argument overload walk (§9); the third, `checker.ts:51482`,
from the evolving-array receiver after `result ||= []` (r7-flow, `3d2d17f8`).

## 14. The overload walk's failure report re-checks literal arguments under the last candidate

**Native.** `reportCallResolutionErrors` (`checker.go:9649`) re-runs
`isSignatureApplicable` with `reportErrors` over the last entry of
`candidatesForArgumentError`, the instantiated candidate for a generic set.
Every argument is checked with `checkExpressionWithContextualType` under
that candidate's parameter.

**Before.** `report_overload_argument_failure` read the walk's recorded
`checked` types and declined an object or array literal the walk had not
checked under the failing candidate. `new Map([["", true], ["", 0]])` (two
generic construct overloads, the literal's parameter an array, not a tuple)
was silent.

**Change.** Such a literal, with no nested resolution, is re-checked under the
last candidate (`check_literal_argument_in_candidate`, §8). The report is
made while it holds those types, and the published state is restored
afterwards. The TS2769 head is chained (`chain_overload_failure`, shared with
§8's site) instead of relabelling the argument's diagnostic. The related
information (`The_last_overload_is_declared_here`, possibly in another file,
and `addImplementationSuccessElaboration`) is not attached; this checker's
diagnostics carry no cross-file related location.

## 15. Non-generic overload sets with literal parameter types reach the walk

`non_generic_overload_candidates` declined a set in which a candidate had a
literal or `null` parameter type, because the walk read published argument
types. Since §8 the walk re-checks an argument whose type follows its
contextual type under each candidate, so the decline goes. Template
expressions join the re-checked shapes: their literal or template-literal
type follows the contextual type. `foo("um", [])` over `foo(x: "hi", …)` /
`foo(x: "bye", …)` reports native's chained TS2769 (overloadingOnConstants2).

## 16. A non-array rest parameter with no trailing argument

`isSignatureApplicable` (`checker.go:9256`) relates the arguments before a
non-array rest parameter (`getNonArrayRestType`) one by one
(`argCount = min(getParameterCount - 1, len(args))`), then the remainder as
one `getSpreadArgumentType` tuple, reported at the call when there is no
remainder (`restArgCount == 0`). `report_signature_applicability` declined
every such signature. It now relates the leading arguments and, when nothing
trails, the empty tuple: `foo()` over `(...args: never)`
(topFunctionTypeNotCallable) and `f1()` over `...args: [...F[], G]`
(contextualTypeTupleEnd) report TS2345 at the call. Trailing arguments still
decline. Their tuple is `inference.rs`'s `inference_spread_argument_type`,
private to that file (visibility requested from the integrator).

## 17. The `new` head reads a declared function's call list

`new Callbacks<number, string, boolean>('s')` over a function with three
call-signature overloads (no construct signature) gets native's TS2558 from
`resolveNewExpression`'s `resolveCall` over the call signatures
(`checker.go:8576`). The `new` head declined it through the
type-variable refusal, because the overloads are generic. A function or
method's own type carries its declaration's signatures, so its list is
decided: the call head already exempts it (`is_function_or_method_type`), and
the `new` head now does too (overloadsAndTypeArgumentArityErrors).

## 18. Agreeing generic survivors run the walk with written type arguments

§463's arm of `choose_ordered_overload` returns the first of several generic
arity survivors whose return types print the same. It ran the transcribed
walk first only without written type arguments, so `foo1<Date>("")` over
`foo1<T extends Date>(test: T)` / `foo1<T extends Number>(test: string)`
(both returning `any`) took the first survivor unchecked. Native's walk
(`chooseOverload`) rejects the first on its argument and the second on
`checkTypeArguments` (`Date` does not satisfy `Number`), and reports the
first's TS2345. §9's walk reads written type arguments, so this arm now runs
it too when it can (`walk_reads_written_type_arguments`)
(typeArgumentConstraintResolution1).

**Measured** for §14–§18, each against its parent (base `53dcad98` on
`41f3c70b`), with both dumps unfiltered, the jsTyping leg, callgrind Ir
(single-threaded) and the exact native oracle (`exact-candidate`, 2
workers):

| § | commit | diagnostics | types | jsTyping new_false / lost_true | Ir gi / dm | exact |
|---|---|---|---|---|---|---|
| 14 | literal re-check in the walk report | +2 (for-of39, iterableArrayPattern28), 0 lost | 0/0 | 0 / 0 | 0.9999 / 0.9997 | +0, 0 lost |
| 15 | literal-typed overload parameters | +1 (overloadingOnConstants2), 0 lost | 0/0 | 0 / 0 | 0.9999 / 1.0000 | +0, 0 lost |
| 16 | non-array rest, no trailing argument | +2 (topFunctionTypeNotCallable, contextualTypeTupleEnd), 0 lost | 0/0 | 0 / 0 | 0.9998 / 1.0000 | +1, 0 lost |
| 17 | `new` head, declared function | +1 (overloadsAndTypeArgumentArityErrors), 0 lost | 0/0 | 0 / 0 | 0.9999 / 1.0000 | +0, 0 lost |
| 18 | agreeing survivors run the walk | +1 (typeArgumentConstraintResolution1), 0 lost | 0/0 | 0 / 0 | 0.9999 / 1.0000 | +1, 0 lost |

The oracle legs ran with 2 workers. With 4, the orb's 7 GiB OOM-killed
`tsr-actual` workers alongside a concurrent build.

## 19. `candidateForTypeArgumentError`: TS2344 when every candidate fails its type arguments

`reportCallResolutionErrors`' third arm (`checker.go:9672`): with no
argument error and no arity error, the last candidate whose written type
arguments failed `checkTypeArguments` is checked again with `reportErrors`
(TS2344 at the type argument). The overload walk's `overload_pass` now
records that candidate (`OverloadArgumentFailure::type_argument_error`), the
walk publishes a failure that carries only it, and
`report_overload_argument_failure` reports it through the reporting
constraint check when no candidate failed on its arguments.
`fn4<boolean, Date>(null, null)` over `fn4<T extends string, U extends number>`
and `fn4<T extends number, U extends string>` reports native's TS2344 at
`boolean` against the last candidate's `number` (overloadResolution).

## 20. An annotated binding-pattern rest parameter is an ordinary rest

`check_candidates_arity` declined any candidate whose last parameter is a
rest binding pattern (`...[a, b]`). That pattern's type is
`getTypeFromBindingPattern`'s tuple only when the parameter has no
annotation, which this port's signature does not model (it carries
`any[]`). With an annotation, `...[[k1, v1], [k2, v2]]: [string, number][]`
is an ordinary array rest of the annotated type, so the decline now applies
only to an unannotated pattern (`has_binding_pattern_rest`)
(iterableArrayPattern16, 26 and 29: native's TS2345 at the argument where
the decline fell back to the syntactic arity check's TS2554).

## 22. A non-generic overload walk re-checks array literals under each candidate

`overload_pass` checked an array-literal argument under a candidate only when
that candidate's parameter was a tuple. Native's `isSignatureApplicable`
checks every argument under each candidate's parameter, and an array
literal's type follows the context beyond tuple-ness: under
`ConcatArray<[number, number]>` the inner literals of `[[3, 4], [5, 6]]`
are tuples. For a wholly non-generic set (`retain_context`), an array
literal free of nested resolutions (`literal_subtree_has_resolution`) is
now re-checked under each candidate like an object literal, and the picked
candidate's check is the published type (concatTuples:
`ijs.concat([[3, 4], [5, 6]])` prints `[number, number][]`).

**Measured** against `68ef70c0` on `127660ee`: types +3 (concatTuples),
0 lost; diagnostics 0/0; jsTyping new_false 0, lost_true 0; Ir
generic-imports 0.9999, domain-model 1.0000; exact +1, exact_lost 0.

## 23. `invocationErrorDetails`' chain under TS2349/TS2351

`invocation_error` emitted only the head (`This expression is not
callable.`/`… constructable.`), so every such report missed native's text.
It now builds `invocationErrorDetails`' chain (`checker.go:9940`) from the
apparent type every caller already holds (call, `new`, tagged template and
decorator heads):

- a non-union: `Type '…' has no call signatures.` (or construct);
- a union: the first constituent with no signatures, under
  `Not all constituents of type '…' are callable.`;
- every constituent without signatures: `No constituent of type '…' is
  callable.`;
- every constituent with signatures but no composite: `Each member of the
  union type '…' has signatures, but none of those signatures are compatible
  with each other.`

A constituent whose signature list this port cannot decide leaves the head
alone. `Did you forget to use 'await'?`'s related information and
`invocationErrorRecovery`'s import-originated related information are not
modelled (the checker's related diagnostics carry no file yet; §30 adds the
first two).

**Measured** against `14511e3e` on `0c8b7f43`: diagnostics 0/0 (the
diagnostics suite compares code and position), types 0/0; jsTyping
new_false 0, lost_true 0; Ir generic-imports 0.9999, domain-model 1.0001;
exact 0 gained, 0 lost. The chain text now matches native byte for byte on
probe files, but the exact oracle still differs. Native's chain children
carry the parent's file and span (`NewDiagnosticChain`); this port's
`Diagnostic::new_chain` children have no file (reported to the integrator
as shared infrastructure).

## 24. Decorator function types are anonymous function types

`decorator_function_type` minted ES member decorator signature types as
`TypeData::Named` carrying `signature_types` (the second proposed issue
below). The relater could not decide them, so `checkDecorator`'s return
check stayed silent for ES member decorators. They are now this port's
anonymous function type (`TypeData::Anonymous { signature: true }`). Native
creates them symbol-less (`newObjectType(ObjectFlagsAnonymous, nil)`); this
port's anonymous types carry a symbol, so the decorated declaration's
symbol stands in. Printing a signature type reads its text, not the symbol,
and the union now prints with native's parentheses
(`void | ((this: X, value: any) => any)`). Probe:
`declare function r(...args: any[]): number; class X { @r foo: any; @r bar() {} }`
reports native's two member TS1270s. potentiallyUncalledDecorators' TS1270s
stay missing: `OmniDecorator`'s generic call signatures leave the relation
undecided, which is the relater's.

## 25. A tagged template's literal substitutions are re-checked per overload

§8's per-candidate literal re-check excluded tagged templates: the call memo
it publishes is the candidate itself, while a substitution's contextual type
(`contextual_type_for_template_substitution`) reads a memo shifted past the
synthetic `TemplateStringsArray` parameter. `check_literal_argument_in_candidate`
now publishes the shifted candidate for a tagged template, and the exclusion
goes: ``fn1 `${ {} }` `` over `fn1(strs, s: string)` / `fn1(strs, n: number)`
reports native's chained TS2769 (taggedTemplateStringsWithOverloadResolution3
11:9; its generic-overload lines 64/65 are the tagged walk's, not yet ported).

**Measured** §24 and §25, each against its parent (base `74503679` on
`59ceea97`): diagnostics 0/0 and types 0/0 for both (§24's probe gains and
§25's 11:9 report sit in cases that stay WRONG on other lines); jsTyping
new_false 0, lost_true 0; Ir generic-imports 0.9999 / 0.9998,
domain-model 0.9999 / 1.0000; exact 0 gained, 0 lost for both.

## 26. A tagged template's overload walk publishes and reports its failure

`check_tagged_template_expression` chose among an overloaded tag's
candidates by passing them, shifted past the strings parameter, to
`choose_overload` with no call node. The transcribed walk could not read
the substitutions' contextual types through the call memo, and could not
publish `overload_argument_failures`, so a failing generic overload set was
silent. The walk now receives the tagged template's node; the shifted
candidates match `contextual_type_for_template_substitution`'s shifted memo.
`check_tagged_template_resolution` reports `CallArity::Applicable(None)`
through `report_overload_argument_failure`, which reads a tagged template's
substitutions as the arguments (positions are the shifted candidate's).
``fn4 `${ null }${ null }` `` reports native's TS2769
(taggedTemplateStringsWithOverloadResolution3 64:9, 65:18).

## 27. A tagged template resolves before its callback substitutions' bodies

`checkTaggedTemplateExpression` resolves the tag (`resolveCall`) before any
callback substitution's body is checked, so `assignContextualParameterTypes`
gives the callback the chosen candidate's parameter types once. The
diagnostic walk visits the tagged template before its substitutions, and
calls already get this ordering through `resolve_call_before_callback_bodies`
(`check.rs`). Tagged templates did not, so a callback body typed its
parameters through the stateless contextual road. `check_tagged_template_diagnostics`
now demands the tagged template's type first when a substitution is
context sensitive. ``fn5 `${ (n) => n.toFixed() }` `` over `fn5(strs, f: (n:
string) => void)` / `fn5(strs, f: (n: number) => void)` types `n` as
`string` and reports native's TS2551
(taggedTemplateStringsWithOverloadResolution3 71:18).

**Measured** §26 and §27, each against its parent (base `3cfd38a7` on
`d798df0f`):
- §26: diagnostics +1 (taggedTemplateContextualTyping1), types +18
  (taggedTemplateStringsWithOverloadResolution3 and its `_ES6` twin, 9 each),
  0 lost; Ir generic-imports 0.9999, domain-model 1.0000;
- §27: diagnostics +2 (taggedTemplateStringsWithOverloadResolution3 and its
  `_ES6` twin), 0 lost; types 0/0; Ir 0.9998 / 1.0000.

Both: jsTyping new_false 0, lost_true 0; exact 0 gained, 0 lost.

## 28. A generic decorator resolves as its synthetic-argument inference

§7 declined a generic decorator after its arity pass because this port's
inference worker (`check_generic_call_with`) takes argument expressions.
A decorator's effective arguments (`getEffectiveDecoratorArguments`,
`checker.go:30142`) are synthetic, so `inferTypeArguments`
(`checker.go:9390`) degenerates to its loop over the positions: infer from
each synthetic argument type to `getTypeAtPosition(signature, i)`. No
expression is checked with a contextual type, the contextual-return arm
excludes `KindDecorator` (`checker.go:9398`), and the
`couldContainTypeVariables` gate only skips work.
`Checker::infer_decorator_candidate` runs that loop with the existing
`infer_from_types` and `resolved_inference_map` (the entries the JSX
resolver already uses for its synthetic props argument), then
`instantiate_signature`. It adds no cache.

Two arms use it:

- `chooseOverload`: a single arity-matched generic candidate is checked as
  its instantiation (`CandidateContext::Instantiated`), and its failure is
  chained under the decorator head as before;
- `getCandidateForOverloadFailure` after an arity failure over a single
  generic candidate: `pickLongestCandidateSignature` instantiates it by
  `inferSignatureInstantiationForOverloadFailure`, the same inference, and
  `checkDecorator`'s return check reads that instantiation
  (decoratorOnClassMethod8: `dec<T>(target: T): T` on a method infers `C`
  from the prototype argument, so TS1270 reports `'C'` against
  `void | TypedPropertyDescriptor<() => void>`).

**Declines, stated:** a `this` parameter (`getThisArgumentOfCall` reads a
non-legacy decorator's expression, which this helper does not type); a
non-array rest (`getSpreadArgumentType`); an undecided position or
inference; several arity-matched candidates (the overload walk over
synthetic arguments is still not ported).

decoratorCallGeneric's TS1238 now reaches the applicability report. The
corpus run reports it at the decorator, but without native's elaboration
below `Argument of type 'typeof C' is not assignable to parameter of type
'I<C>'` (`The types returned by 'm()' are incompatible…`). A CLI probe of
the same file stays silent, and so does `const x: I<C> = C` there: the
relater's answer for `typeof C` → `I<C>` (a static method's `void` return
against `T`) is not settled. Routed to r7-reports.

## 29. A written type argument that relates to its constraint over type variables

`check_call_type_argument_constraints_with` (`checkTypeArguments`,
`checker.go:9222`) declined any written type argument or instantiated
constraint mentioning type variables, before relating them. That keeps out
a negative answer this port cannot trust over type variables, the same
refusal `constraints.rs` makes for type references. A positive answer is a
different matter: `Related` means the candidate passes `checkTypeArguments`,
so the call goes on to its argument checks exactly as native's does.
`foo<T, U>(c, d)` inside `function other<T, U extends T>()`
(genericCallWithObjectTypeArgsAndConstraints4/5) relates `U` to its
constraint `T` and now reports the argument failure (TS2345, `'T' could be
instantiated with an arbitrary type…`).

The order is now relate first, then refuse a non-`Related` answer over type
variables. A pair that still declines now pays for one relation it used to
skip; written type arguments over type variables are rare, and the Ir
measurement below covers it. **Falsifier:** a `Related` answer the relater
gives over type variables where native fails the constraint. That would
show up as a TS2345 (or other argument report) where native reports
TS2344.

## 30. Call resolution errors carry their related information

The checker's related diagnostics can now carry their file
(`related_diagnostic_at`, r7-grammar §34). That lifts the reason §23 and the
TS2769 chain left their related information off. Three native attachments
are ported:

- **`getArgumentArityError`'s too-short arm** (`checker.go:9770`): the
  closest signature (the first with the smallest `getMinArgumentCount`)
  names its declared parameter at the argument count, past a `this`
  parameter. The messages are `An_argument_for_0_was_not_provided` (TS6210),
  `An_argument_matching_this_binding_pattern_was_not_provided` (TS6211) and
  `Arguments_for_the_rest_parameter_0_were_not_provided` (TS6236). The
  parameter comes from the declaration's written list (`parameters_of`), so a
  synthetic or default signature, whose declaration lists no such
  parameter, adds nothing, as native's nil declaration does. A decorator's
  arity report gets it too and keeps it through `chain_reports_under`
  (`NewDiagnosticChain` copies related information).
- **`The_last_overload_is_declared_here`** (TS2771) on the TS2769 chain, at
  the last candidate's declaration. It is added only when that declaration
  is a signature declaration: this port's synthetic signatures borrow
  another node as their declaration where native's is nil.
- **`addImplementationSuccessElaboration`** (TS2793, `checker.go:9685`),
  after every argument report: when the failed candidate's (unmerged)
  symbol has several declarations and one is a function-like declaration
  with a body, `chooseOverload` runs against that implementation alone. If
  it would succeed, TS2793 is added at the implementation. The check is
  `isSingleNonGenericCandidate`'s: written type arguments fail, then
  `hasCorrectArity`, then `isSignatureApplicable` over the same plain
  arguments the non-generic overload walk relates (literals re-checked
  under the candidate and restored). That walk's per-candidate loop is now
  `plain_overload_arguments` and `plain_candidate_applicable`, shared by
  both. The binder binds no `__constructor` symbol, so a constructor's
  declarations are read off its class's members.

The invocation error (§23) gains its two related lines and its printer:

- `Did_you_forget_to_use_await` (`invocationErrorDetails`,
  `checker.go:9940`) when the awaited apparent type (`awaited_type`) has
  signatures of the kind; an undecided awaited type adds nothing;
- `Are_you_missing_a_semicolon` (`resolveCallExpression`,
  `checker.go:8545`) at the callee of a one-argument call whose callee ends
  its line (`SkipTriviaEx` with `StopAfterLineBreak`, ported byte for
  byte);
- the detail's types print through `type_to_string_for_message`, which
  reduces before printing as native's node builder does
  (`nodebuilderimpl.go:3228`): neverIntersectionNotCallable's apparent
  intersection prints `never`.

`GetErrorRangeForNode` (`scanner.go:2588`) places these related spans
differently from this port's `error_span` in two cases, so
`signature_declaration_error_span` handles them:

- a method signature, call or construct signature, or function or
  constructor type has no arm there and spans its whole node (this port's
  `error_span` narrows a method signature to its name);
- a constructor spans from its first token to the `constructor` keyword.

**Chain children carry the reported file.** Native's `NewDiagnosticChain`
(`ast/diagnostic.go:152`) builds each head from its child's file and
location, so every level of a reported chain has the file. The exact oracle
prints that file and position for each chain row. This port builds the
children before `report` gives the head its file, so they printed without a
location (§23's TS2349 detail, the TS2769 chain, the decorator heads).
`set_chain_file` now gives every file-less child under the three chain
builders (`chain_reports_under`, `chain_overload_failure`,
`invocation_error`) the reported file's image. Chains built by the relation
reporter (a lone TS2345's or TS2322's elaboration) are outside these
builders and still print without one: the general fix belongs in `report`
(proposed below).

**Declines, stated:** TS2793 for a generic implementation, a spread
argument, or any argument shape or relation the plain walk declines; the
elaboration's own related information (`The expected type comes from
property…`, TS6500) belongs to the relation reporter.

**Falsifier:** an exact-oracle case gaining a related line native does not
have. That would come from an implementation the plain relation accepts
where native's `chooseOverload` rejects it, or from a synthetic
signature's borrowed declaration passing `is_signature_declaration`.

**Measured** for §28–§30 (`e4c91f31`, `83c1a99e`, `482c4056`, stacked on
§4's `9092f916`) against a fresh freeze of main `57e45ae7`:

- corpus diagnostics: +6, 0 lost. Four of the six are this cluster's:
  decoratorCallGeneric, decoratorOnClassMethod8,
  genericCallWithObjectTypeArgsAndConstraints4/5; §4 has the other two,
  assertionTypePredicates1/2. Types 0/0.
- jsTyping: new_false 0, lost_true 0.
- Ir: generic-imports 1.0000, domain-model 1.0008. §4 alone measured the
  same 1.0008.
- exact oracle: 9,884 → 9,985 (+101, 0 lost; 1 worker, per-process,
  deadline 10). Nearly all of it is §30's related information and chain
  files: about 98 of the 387 WRONG cases carrying a call diagnostic became
  exact in a per-case run, and the 133 EXACT ones stayed exact.

## 31. TS2793 against a generic implementation

§30's `addImplementationSuccessElaboration` declined a generic
implementation. Native runs `chooseOverload` against it like any lone
generic candidate (`checker.go:9040-9080`): `inferTypeArguments`,
`getSignatureInstantiation`, the non-array-rest arity re-check, then
`isSignatureApplicable`. `generic_implementation_applicable` does the same
through the shared inference worker (`check_generic_call_with`'s
out-slot), then relates the plain arguments to the instantiation.

Inference checks the arguments under the implementation's parameters,
which this port publishes, while native's checks there are uncached. So
every argument subtree's published state (node types, resolved signatures,
symbol types: `literal_subtree_state`) is saved before the inference and
put back after. Only a call or `new` takes this arm. A tagged template
(its positions are shifted past the strings argument) and a non-array rest
decline.

overloadErrorMatchesImplementationElaboaration (`publish<T>(event: T)`
behind `publish(event: string, data?: any)`) becomes exact. A per-case
oracle run over the 190 EXACT cases carrying a call diagnostic (base
`cand17`) kept every one exact.

**A decorator's arity error keeps its related information on the head.**
`getArgumentArityError` chains its diagnostic under `headMessage` first and
adds the too-short arm's related information (TS6210) after, so only the
head carries it (decoratorOnClass8, decoratorOnClassProperty7,
esDecorators-arguments in the exact oracle). §30 had added it before
`chain_reports_under`, whose `NewDiagnosticChain` copy left it on the child
as well. `report_argument_arity_error_under` now builds the chain itself
in native's order, and the decorator path passes its head there.

## 32. A `new` stops at an inaccessible constructor or an abstract target

`resolveNewExpression` (`checker.go:8575`) does two things before
`resolveCall` over the construct signatures:

- `isConstructorAccessible(node, constructSignatures[0])` (`:8648`). A
  `private` or `protected` constructor declaration is accessible only
  from inside its class (any enclosing class, `isNodeWithinClass`). A
  `protected` one is also accessible from a class whose base chain reaches
  it (`typeHasProtectedAccessibleBase`, `:8680`). Otherwise the call reports
  TS2673/TS2674, naming the class's declared type.
- An abstract construct signature (or an abstract class declaration)
  reports TS2511.

Either way the call is `resolveErrorCall`, with no arity or argument
report. This port's `new` head ran `resolveCall`'s reports regardless.
typesWithPrivateConstructor/typesWithProtectedConstructor reported a false
TS2554 (`new C2()` against `private constructor(x: number)`) and missed
TS2673/TS2674. classConstructorAccessibility and
classConstructorAccessibility5 missed theirs.

`constructor_accessible` ports the first check and reports from the head.
For the abstract arm the head only stops: TS2511 is the check pass's
(`check_new_on_abstract_class`, `check.rs`, an identifier callee naming an
abstract class). The type road already answers `any` for both
(`expressions.rs`). A class declared `abstract` gives every construct
signature `SignatureKind::AbstractConstruct` (`signatures.rs`), so the kind
test covers native's `valueDecl` arm.

**Declines, stated:**

- a JavaScript file (`getSelectedModifierFlags` reads JSDoc accessibility
  tags there);
- a protected base chain through an intersection (`findMixins`) or a base
  whose symbol this port cannot name.

Both fall back to the old `Unknown` head.

TS2511 itself is split. The check pass keeps the shape it already reports
(`check_new_on_abstract_class`: an identifier naming an abstract class,
outside JavaScript). The `new` head reports every other abstract target:
an abstract construct-signature type, or a union holding one
(unionTypeConstructSignatures, abstractClassUnionInstantiation).
`names_abstract_class` mirrors the check pass's predicate so the two never
both report.

**Proposed:** move TS2511 wholly into `resolveNewExpression` and drop the
check pass's arm along with this split.

**TS2679.** `resolveNewExpression`'s call-signature arm also reports
`A_function_that_is_called_with_the_new_keyword_cannot_have_a_this_type_that_is_void`
when the resolved signature's `this` type is `void`
(`checker.go:8638`, under the same `noImplicitAny` guard as TS2350). The
`new` head reports it for the candidate it can tell
(`new_head_candidate`): thisTypeInFunctionsNegative's `new VoidThis()`.

## 33. A JSX overload failure chains like a call's

`reportCallResolutionErrors` (`checker.go:9649`) is shared by calls and JSX
elements: with several `candidatesForArgumentError` the last candidate's
report is chained under `The_last_overload_gave_the_following_error` and
`No_overload_matches_this_call`, `The_last_overload_is_declared_here` is
related, and `addImplementationSuccessElaboration` runs. `jsx_component.rs`
rewrote each report's head message to TS2769 in place, which lost the chain
level and the related line. The change (granted for jsx_component.rs) makes `chain_overload_failure`
`pub(crate)` and calls it there.

Per-case oracle against `cand21` (main `f666bbf2` plus §32): the 212 EXACT
`.tsx` cases stay exact; tsxElementResolution9 becomes exact. Diff lines drop
in tsxStatelessFunctionComponentOverload5 (52 → 6) and
tsxStatelessFunctionComponentsWithTypeArguments4 (34 → 20). What remains
there is the relation reporter's elaboration and a JSX TS2793 (the
implementation check through JSX applicability is not ported).

## 34. A literal argument whose nested calls cannot follow its context is re-checked per candidate

The non-generic overload walk (`check_overload_candidates_arguments`, §8)
declined an object or array literal argument holding any nested
resolution (`literal_subtree_has_resolution`). Re-checking it under each
candidate re-enters that resolution, and a resolution that reads the
literal's contextual type would publish a candidate-dependent answer.

A nested call or `new` whose callee's signatures of the kind are decided
and none generic, and none of whose own arguments is context sensitive,
does not read it. Its return type is its declared one, and its arguments
are checked against its own parameters, so the re-check resolves it to
the same signature (and the subtree's published state is restored after,
as before). `literal_resolutions_are_context_free` certifies a subtree
whose every resolution is such a call. Functions, classes, methods,
accessors, tagged templates, and generic or undecided callees still
decline.

`array.concat([...new SymbolIterator])` now reports native's TS2769
(iteratorSpreadInArray6); its elaboration below the head is the relation
reporter's. Only the plain-argument road (the non-generic walk's report and
§30's TS2793 check) reads the new predicate; the type road's walk keeps
`literal_subtree_has_resolution`.

## 35. A generic call whose return no type parameter reaches has that return

`getSignatureInstantiation` (`checker.go:19293`) instantiates the return
type with the inferred (or written) type arguments. When no type parameter
reaches the declared return, every instantiation returns it unchanged. The
call's type then does not wait on inference, and an overload failure's
`pickLongestCandidateSignature` over the same single candidate answers the
same type. Two type roads answered `error` (a GAP) when inference declined:

- the single generic call (`check_call_expression_worker`): `genericFunc<this>(undefined)`
  with an unresolvable written argument (thisInInvalidContexts);
- the tagged template with no substitution, which never infers
  (`` noParams`` `` over `function noParams<T>(n: T) {}`,
  taggedTemplateStringsTypeArgumentInference).

`type_parameter_free_return` instantiates the return with every type
parameter mapped to `unknown` and compares identities. If they match, it is
the answer. It is consulted only after the inference road answered `error`,
so every answer that road gives today stands.

## 36. Every reported chain child carries the file

§30 gave the call chain builders' children the reported file. This
change (granted for check.rs and checker.rs) does the same once, in `Checker::report`: any chain child without a
file (at any depth) takes the reported file's image. The image is built
once per file and kept in `chain_file_images`. Key: the source file node.
Owner: `report`. It is built on the first chained report in that file, and
nothing reads it before then. `set_chain_file` becomes `pub(crate)` for it.

Per-case oracle against `cand22`: of the 664 WRONG cases whose native
output has chain rows, 16 become exact, and no file-less chain row remains
in any of them. The 435 EXACT cases with chain rows stay exact. What
remains in the others is the chains' content (the relation reporter).

## 37. A reference argument's candidate-context check is not published (createToken)

Held since round 7's early batches (`box/r7-calls-held-createtoken`) because
the jsTyping leg refused it while the relater rejected `T["kind"]` → `SK`.
r7-reports' indexed-access-to-constraint relation is on main now, so it is
re-landed here.

**Repro.** `declare function createToken(token: SK.D): Tok<SK.D>;` followed
by `declare function createToken<TKind extends SK>(token: TKind): Tok<TKind>;`,
then `createToken(value)` with `value: TKind` inside
`f<TKind extends SK>(value: TKind): Tok<TKind>`. TSR answered `Tok<SK>`
(a false TS2322 at the `return`); tsgo answers `Tok<TKind>`.

**Cause.** The overload walk makes every argument's first check under the
first arity-matching candidate (`check_argument_in_candidate_context`, with
the call memo set to that candidate) and publishes it. Under the non-generic
`token: SK.D`, a contextual type with no generic types,
`getNarrowableTypeForReference` substitutes the generic reference's
constraint (`hasContextualTypeWithNoGenericTypes`), so `value` reads `SK`.
The generic candidate's inference then reads that published `SK`
(`TKind := SK`). Native's check is per candidate and uncached.

**Change.** A reference argument (an identifier, or a property or element
access, through parentheses) checked under a candidate's context keeps no
published state: its subtree state is restored after the check
(`literal_subtree_state`, the same set the literal re-check restores). After
a literal re-check, the argument types are re-read with the reference read
under that candidate again rather than context-free.

## 38. `GetErrorRangeForNode`'s unnamed-declaration, `satisfies` and arrow-function arms (check.rs, granted)

The integrator granted this lane the check.rs half of r7-flow's routed
error-span diff (`r7-flow-error-span-unnamed.diff`, r7-flow §38). Applied
to the shared `error_span`, so every report at such a node moves, not only
the call reports:

- a function declaration, method, class or function or class expression
  with no name (`errorNode == nil`, `scanner.go:2644`) reports at its first
  token (`function`, `class`, a leading modifier);
- a `satisfies` expression reports at its `satisfies` keyword
  (`scanner.go:2626`);
- an arrow function whose block body's full start (the end of `=>`) and
  end lie on different lines reports from the arrow's start to the end of
  that first line (`getErrorRangeForArrowFunction`, `scanner.go:2542`).

The diff as routed differed from native in the arrow arm, and is corrected
here. It ended the span one past the line break and searched for the line
break from the body's `{`. `GetECMAEndLinePosition(startLine) + 1`
(`scanner.go:2703`) is the line break's own position, and `startLine` is
the line of `body.Pos()`, the end of `=>`, so `() =>\n{ … }` ends at the
`=>` line's break. The ECMAScript line terminators (U+2028, U+2029) count as line breaks too. A probe file with all three shapes now prints
byte for byte as tsgo does.

## 39. The types dump reads `getTypeOfExpression`'s quick path

Consumer hunk in another lane's file, under the default grant for an atomic
hunk of the same root cause: `tsr-conformance/src/types_producer.rs`,
`type_id_at_location_arm_inner`'s expression arm.

Native's type writer asks `getTypeAtLocation`, which reaches
`getTypeOfExpression`. That function answers first from
`getQuickTypeOfExpression` (`checker.go`):

- a call or `new` whose non-nullable callee has exactly one non-generic
  signature of the kind (`getSingleSignature`) answers that signature's
  return type, without resolving the call;
- an `await` answers the awaited quick type of its operand;
- a non-`const` assertion answers its written type.

So `new A()` on an abstract class prints `A` natively, although
`resolveNewExpression` reports TS2511 and the checked type is the error
type. TSR's producer printed `check_expression`'s answer (`any`).

`Checker::quick_type_of_expression` (calls.rs) ports the quick path. It
excludes `super`, `import()`, `require("…")` and `Symbol()`/`Symbol.for()`
calls, as native does. Call chains are excluded as well
(`propagateOptionalTypeMarker` is not ported there), and so is a JavaScript
parenthesized expression, which may carry a JSDoc cast that native reparses
into an assertion. The producer still checks the expression first, because
this port's later reads (a callback parameter's contextual type) depend on
the call having been resolved, and then prints the quick type when there is
one. Printing it without the check moved six `contextualOverloadListFromArrayUnion`
lines to `any`.

Types dump against main `41e3bf25`: +10 RIGHT, 0 lost (abstractClassInLocalScopeIsAbstract,
fallbackToBindingPatternForTypeInference ×6, inferFromGenericFunctionReturnTypes3,
inferredRestTypeFixedOnce ×2).

## 40. A non-array rest parameter relates its trailing arguments' spread tuple

Consumer hunk in another lane's file, under the default grant: visibility
only, `inference.rs`'s `inference_spread_argument_type` made `pub(crate)`.

`isSignatureApplicable` (`checker.go:9256`) relates the arguments past a
non-array rest parameter (a tuple, a variadic tuple, `never`) as one
`getSpreadArgumentType` tuple. One trailing argument reports at its
`getEffectiveCheckNode`; several report over a synthetic node spanning
the first to the last. §16 handled the no-trailing-argument case (the
empty tuple) and declined the rest, because the tuple's construction (tuple
spreads expanded, each element widened under its contextual element type)
lived only in `inference.rs`'s `inference_spread_argument_type`.

That function is now `pub(crate)` (a visibility-only change in
inference.rs). The report pass hands it the written arguments and their
checked types, and relates its tuple to the rest type. A synthetic argument
anywhere in the list (a decorator's, a tagged template's strings, an
expanded tuple spread) keeps the written list from lining up with the
effective one, so it declines. So does a pair over type variables, the
refusal elsewhere in this file: `f(...args)` against `...args: T` builds
`[...T]`, which this port's relater rejects where native relates it.

A rest that is a bare type parameter is the one type-variable side
admitted: a concrete spread tuple against `T` fails the way native's does
(`'[{ a: true; }]' is assignable to the constraint of type 'T', but 'T'
could be instantiated with a different subtype…`;
typeSatisfaction_errorLocations1's `f({ a: true } satisfies unknown)` under
`(...args: T) => void`). The spread itself must hold no type variable.

variadicTuples2 gets five of native's missing TS2345s (`[]`, `[1, 2]`,
`["abc", 1, "def"]`, `["abc", "def", true]` against
`[...strs: string[], n2: number]`, and `boolean` against `number`). The
others there sit behind generic candidates whose inference declines.

## 41. Report-pass certification: empty literals, kept `NoInfer`

`report_argument_type_is_certified` decides whether a written argument's
published type is the type `checkExpressionWithContextualType(arg, target)`
gives it natively under an instantiated candidate. Two arms change, and a
third was tried and kept:

- **A class expression still declines.** Its type does not follow the
  context (`checkClassExpression` answers the class symbol's type), but this
  port's inference from a class expression (a generic candidate's `T` read
  off a member) is not native's. Certifying it turned
  typeArgumentInferenceWithClassExpression1/3 into false TS2345s on the
  corpus compare, so the arm stays.
- **An empty array literal under a tuple-like target is certified** when
  its published type is the empty tuple. It has no element to widen, so
  `f0([])` against `[T, ...U[]]` reports `'[]'` against
  `'[unknown, ...unknown[]]'` (restTupleElements1). Other array literals
  under a tuple-like side still decline (tuple-ness follows the context).
- **A literal argument against a kept `NoInfer` declines.** `getNoInferType`
  keeps the wrapper around an object type (`isNoInferTargetType`), and
  native's contextual type reads through the substitution, so
  `foo3('foo', ['bar'])` keeps `"bar"` literal and elaborates it against
  `"foo"`. This port's check under the wrapper widened it to `string[]`, so
  the pair is not native's. noInfer's three false TS2345s (and
  noInferUnionExcessPropertyCheck1's one) go. The contextual-type half is
  routed.

## 42. An overload failure with written type arguments instantiates by them

`pickLongestCandidateSignature` (`checker.go:9510`) instantiates the
longest generic candidate. With written type arguments it uses
`createSignatureInstantiation(candidate, getTypeArgumentsFromNodes(…))`:
the written list truncated to the candidate's type parameters, the tail
filled with each parameter's default, else its constraint, else `unknown`.
It infers (`inferSignatureInstantiationForOverloadFailure`) only when
there are none. The transcribed walk's failure answer always inferred, and
`check_generic_call_with` declines a written list longer than the
candidate's type parameters. So `_.map<number, string, Date>(c2, (x, y) => …)`
over `map<T, U>(…)` / `map<T, U, V>(…)` gapped: the walk was undone, the
callback was left without contextual parameter types, and the TS2345 was
lost.

`instantiate_with_written_type_arguments` is the getTypeArgumentsFromNodes
half that `written_type_argument_arity_failure` already had, now shared:
the walk's failure answer takes it when the call has written type
arguments. genericCombinators2 becomes exact (TS2345 at the callback,
`r5a: Collection<any, any>`).

## 43. A JSDoc name that resolves only as a value is TS2749

`check_type_reference_name` declined every name inside a JSDoc comment
(r6-names §6). Lifting the whole decline cost 6 cases, all on the TS2304
rung: typedef placement, `import()` types and `@callback` names still
resolve differently from native. The meaning arms of `onFailedToResolveSymbol`
(`checker.go:1564`) are a different question. TS2709
(`checkAndReportErrorForUsingNamespaceAsTypeOrValue`) and TS2749
(`checkAndReportErrorForUsingValueAsType`, `checker.go:1722`) fire only when
the name **does** resolve, under a meaning the position does not want.
tsgo has no JS value-as-type fallback: Strada's `getTypeFromJSDocValueReference`
and `@enum` are gone, and `resolveTypeReferenceName` (`checker.go:23087`) is
one `resolveEntityName` at the type meaning. So `@param {Image}`,
`@type {Thing}` for an `@enum` const, and a constructor-function name used
as a type are all TS2749 natively. This was the second bucket of the JS/JSDoc
rank on main `f4117160` (33 cases missing a TS2749).

The JSDoc arm of `check_type_reference_name` now calls
`report_jsdoc_meaning_mismatch` (`jsdoc_checks.rs`) and returns. The new
function keeps the same shape tests as the TypeScript path: an identifier
that is a `TypeReferenceNode`'s whole type name, and silence when the
`TYPE` lookup's alias chain carries `TYPE`. It then runs
`report_meaning_mismatch_in_type_position`. The TS2304 rung stays declined.

A qualified JSDoc name (`@type {Ns.One}`, `jsEnumCrossFileExport`) is not
covered. Its TS2749 comes from `resolveQualifiedName`'s right-hand lookup,
which `check_type_reference_name` never reaches for either language.

Consumer hunks under the default grant (same root):

- `check.rs::check_type_reference_name` splits the JSDoc half out of the
  early return.
- `check.rs::alias_chain_carries` becomes `pub(crate)`.

**Falsifier.** A JS baseline where a JSDoc type name resolves only as a
value and native reports nothing. That would mean tsgo kept a value-as-type
path for that declaration kind. The CommonJS `require` class alias is the
candidate, and the alias-chain test is what keeps it silent here.

## 44. `module.exports =` beside another value export is TS2309

`checkExternalModuleExports` (`checker.go:5693`) runs once per external or
CommonJS module, from `checkSourceFile`. Its `export=` arm reports TS2309 at
`getDeclarationOfAliasSymbol(exportEquals) ?? exportEquals.ValueDeclaration`
when either of two things holds:

- some other export carries a value meaning (`hasExportedMembersOfKind`
  over `getSymbolFlags`);
- the `export=` alias shadows a namespace that has type or namespace
  exports (`hasShadowedNamespace`).

This port had only `check_export_assignment`'s statement scan, which reads
written `export` modifiers beside an `export =` statement. A CommonJS
`module.exports = C` never reaches it. This was bucket 3 of the JS rank
(TS2309 missing, 21 cases: the `moduleExportAlias*`,
`moduleExportAssignment*` and `jsDeclarationsExport*` families).

`check_commonjs_export_equals_conflict` (`assignment_declarations.rs`) is the
symbol-based arm for JS files. It reads the module symbol's `export=` and
other exports the way upstream does, and reports at the last alias-shaped
`module.exports =` assignment, else the value declaration. A written
`export =` declaration stays with the statement scan, so nothing is reported
twice. The TypeScript half was not rewritten onto symbols: it is not in this
bucket, and that rewrite is a larger change in `check.rs`.

**The value declaration is the first assignment.** The binder overwrote
`value_declaration` on every `module.exports =`. Native's
`bindModuleExportsAssignment` calls `SetValueDeclaration`, which keeps the
first, and `declare_into` had already done the same for a property-flagged
assignment. `typedefCrossModule2` (two `module.exports = {…}`) reported at
the second assignment until the binder hunk below.

Consumer hunks under the default grant (same root):

- `check.rs::check_source_file` calls the arm for JS files.
- `tsr-binder` `binder.rs` `JsDeclaration::ModuleExports` sets
  `value_declaration` only when it is empty.

**Left.** `pushTypeGetTypeOfAlias` and `moduleExportAliasUnknown` are
`exports.x = <unresolvable entity>`. Natively the alias resolves to
`unknownSymbol`, whose flags (`SymbolFlagsAll`) make it a value export.
`get_symbol_flags` declines an unresolved alias except in the CommonJS
canonical-any shape, so these two stay silent rather than guess.

**Falsifier.** A CommonJS baseline with `module.exports = x` beside a value
`exports.y` and no TS2309, or a TS2309 placed at a later
`module.exports =` that is not alias-shaped.

## 45. JSDoc `@param` types: one typedef per comment, reused when printed

Bucket 5 of the JS rank: function type text where a `@param` type is
involved. Two roots.

**A `(`-led statement does not own its comment.** `parseExpressionOrLabeledStatement`
(`parser.go:1516`) records `hasParen` and clears `jsdocScannerInfoHasJSDoc`
before `withJSDoc` when the statement starts with `(`. The
`ParenthesizedExpression` (`parser.go:5594`) is itself `withJSDoc`-wrapped and
takes the comment. Without that clear, `reparseTags` would run twice and
reparse every `@typedef` into two `JSTypeAliasDeclaration`s. This port's
`parse_statement` attached the comment to both nodes, so the binder declared
each `@typedef` twice. `type_alias_body` then declines a two-declaration JSDoc
alias, which is correct for same-named aliases in different function
scopes. So `@param a {NotADuplicateIdentifier}` read `any`
(`jsdocTypedefBeforeParenthesizedExpression`). Only an expression statement
starts with `(`, so `parse_statement` skips the comment there.

**An unbracketed `@param` type is reused like a bracketed one.**
`reparseHosted` puts every `@param` type on its parameter, and
`serializeTypeForDeclaration` reuses that node when printing (`@param
{Array<*>}` prints `Array<any>`, `nodecopy.go` rewriting `*`). The
`signatures.rs` JS arm reused the node only for a bracketed `@param [x]`.
This lane measured lifting the gate at 8 better and 0 worse on the JS
subset. On rebase, main already carried the same change: r7-printer's
`eb499d70b` (*every JSDoc @param annotation is reused*). That
`signatures.rs` hunk is dropped from this commit and the parser half
stands alone.

Consumer hunk under the default grant (same root, JSDoc `@param` typing):
`tsr-parser` `statement.rs::parse_statement`.

**Left.** `jsdocTypedefBeforeParenthesizedExpression` still prints
`(a: number, b: number) => void` where native prints the typedef names: the
reused name-first `@param a {T}` reference is refused by the site visitor.
Not traced.

**Falsifier.** A `(`-led statement whose comment native reads from the
statement, for example a `@type` on `(x) = …`. The reparser's hosted arms
read the paren's comment, so none is expected.

## Proposed issues (for the integrator to file)

- **Chain children carry no file outside the call builders.**
  `Checker::report` (`check.rs`) stores a diagnostic's file beside it, but
  its `message_chain` children keep `file: None` and print with no location
  in the exact oracle; native's carry the head's file at every level
  (`NewDiagnosticChain`). §30 fixes the call builders locally. Giving every
  file-less chain child the reported file in `report` would fix the
  relation reporter's chains as well. On base16 (main `57e45ae7`), 60 of
  the first 400 `diag:chain` cases differ only in those rows.

- **Decorator inference over synthetic arguments.** Done for a single
  candidate in §28; the overload walk over synthetic arguments (several
  arity-matched decorator candidates) is still not ported.
- **Decorator function types are not relatable.**
  `decorators.rs::decorator_function_type` mints a `TypeData::Named` type
  carrying `signature_types` for the ES member decorator signatures (the
  method, getter, setter and field value/return types). The relater answers
  `Unknown` against it, so `checkDecorator`'s return check stays silent for ES
  member decorators (potentiallyUncalledDecorators' member TS1270s). Native
  mints a symbol-less anonymous object type. The fix re-mints it as this
  port's anonymous function type (`TypeData::Anonymous` with
  `signature: true`, which needs a symbol), measured against the contextual
  types the same type serves today.
- **Generic indexed access against its constraint's type.** TSR's
  `relate_ternary` rejects `T["kind"]` → `SK` for `T extends { kind: SK }` in
  the call-report and overload paths (`declare function h<TK extends SK>(t:
  TK): TK; h(k)` with `k: T["kind"]`, a false TS2345 since `9020aa67`).
  Native relates the indexed access through its constraint
  (`getConstraintOfIndexedAccess`). It blocks the held createToken fix
  (`box/r7-calls-held-createtoken`): with the reference argument
  unpublished, every `createToken` candidate fails and the fallback picks
  the first overload (jsTyping `parser.ts:2634` TS2352). Routed to
  r7-reports.
