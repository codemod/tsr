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

## Proposed issues (for the integrator to file)

- **Decorator inference over synthetic arguments.** `resolveDecorator`'s
  `resolveCall` infers a generic decorator's type arguments from
  `getEffectiveDecoratorArguments`' synthetic arguments
  (`inferTypeArguments`, `checker.go:9390`), and a failed resolution answers
  `getCandidateForOverloadFailure`'s instantiation
  (`inferSignatureInstantiationForOverloadFailure`). This port's inference
  (`check_generic_call_with`) takes argument expressions, so a generic or
  overloaded decorator declines after its arity pass. Cases: decoratorCallGeneric
  (TS1238), decoratorOnClassMethod8 (TS1270). Needs a types-only inference
  entry in `inference.rs` (r7-contextual) or a synthetic-argument path through
  it.
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
