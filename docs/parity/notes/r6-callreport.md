# r6-callreport — the reporting pass of call resolution (`tsr-2zk.1153`, `.1158`, `.1163`, `.1169`, `.1172`)

Round-6 box on epic `tsr-2zk`, single owner of call-resolution error
reporting. Items are r6-triage's five call clusters
([`r6-triage.md`](r6-triage.md) §2, case lists in
[`r6-triage-issues.json`](r6-triage-issues.json)), 106 diagnostics cases.
`calls.rs` is MAIN, so every change to it ships here as a measured diff, in
apply order (§9). The new logic lives in
`crates/tsr-checker/src/call_reports.rs`, which this lane owns. Native anchors
are `vendor/typescript-go` @ `5b1047d`; every expectation was checked against
a native `tsgo` built by `scripts/offline-cargo/build-tsgo.sh`.

## 0. Base and setup

Frozen base: `claude/beautiful-shannon-ar5gh0` @ `eee504b` (batch BP plus the
snapshot refresh). Batch BW (r6-triage) had not landed when this lane started;
its notes and issue list were read from its branch.

- diagnostics: 12,238 cases, 5,630 RIGHT / 5,606 EMPTY_RIGHT / 963 WRONG /
  39 EMPTY_WRONG;
- types: 556,303 lines, 550,355 RIGHT / 5,196 WRONG / 752 GAP;
- Ir (`valgrind --tool=callgrind`, release `tsr -p <project>/tsconfig.json
  --noEmit --singleThreaded true --pretty false`): domain-model 1,091,280,076;
  generic-imports 343,119,095.

Setup as r5-operators3 §4: PyPI answers 403, so `assemble.py`'s three
`tomlkit` calls ran against a stdlib-only stand-in kept in the session
scratchpad (not committed).

## 1. The shape: one reporting pass

**Native.** `resolveCall` (`checker.go:8843`) runs `chooseOverload`
(`:9025`) once per relation. Each candidate that fails is remembered in
`CallState`: every arity-matching candidate rejected by
`isSignatureApplicable` (`:9256`, `reportErrors == false`) is appended to
`candidatesForArgumentError` (instantiated when generic), a generic rest that
instantiates to another arity is `candidateForArgumentArityError`, and a
written type argument failing its constraint is
`candidateForTypeArgumentError`. When nothing is chosen,
`reportCallResolutionErrors` (`:9649`) reports from that state:
`isSignatureApplicable` again, with `reportErrors == true`, on the last
`candidatesForArgumentError` entry (TS2769 over the chain when there were
several), else the arity or type-argument report. A single non-generic
candidate takes the same path (`isSingleNonGenericCandidate`).

**This port.** Resolution happens on the type road (`check_call_expression`
→ `resolve_call_signature_at` → `choose_overload` /
`transcribed_generic_set_walk` / `check_generic_call_with`), and reports come
from the diagnostic walk (`check_call_expression_diagnostics`,
`check_new_expression_diagnostics`, `check_tagged_template_diagnostics`).
Before this lane, three report sites in `calls.rs` each carried their own
copy of `isSignatureApplicable`'s argument loop, with different declines:

- `check_single_candidate_arguments` (a single non-generic candidate);
- `check_instantiated_candidate_arguments` (a single generic candidate's
  instantiation);
- `report_overload_argument_failure` and
  `check_overload_candidates_arguments` (the last failing overload).

**Ported.** `call_reports.rs` holds the one `isSignatureApplicable` with
`reportErrors`:

- `report_call_arguments` is `getEffectiveCallArguments` (`:30042`): the
  tagged template's synthetic `TemplateStringsArray` first, spread tuples as
  synthetic elements;
- `report_this_argument` is its `this` arm (moved from `calls.rs`'s
  `check_this_argument`);
- `report_signature_applicability` walks the arguments against
  `getTypeAtPosition`, relates each at `getEffectiveCheckNode`
  (parentheses and `satisfies` skipped) through
  `checkTypeRelatedToAndOptionallyElaborate` (`report_argument_failure`),
  and stops at the first failure.

Every report site hands its candidate here, so a decline lifted here is lifted
for all of them.

**What decides an argument's type.** Native checks each argument with
`checkExpressionWithContextualType(arg, paramType)`, uncached. The pass never
re-checks (a re-check would evict and re-publish argument types, which the
type dump reads), so it must know when the published type (`node_types`) is
that type. The rule is in `report_argument_type_is_certified`, asked only once
the pair fails:

- **`checked`**: the overload walk's own check under this candidate is
  native's type.
- **A function or arrow**: `assignContextualParameterTypes` runs once
  (`contextuallyCheckFunctionExpressionOrObjectLiteralMethod`, guarded by
  `NodeCheckFlagsContextChecked`, `checker.go:10155`). After resolution its
  type is the same under any context, so the published type is native's.
  This is the lift in §2.
- **The call's own declared signature** (`CandidateContext::Declared`):
  every argument was checked under this parameter.
- **Under an instantiation**:
  - an object literal is certified unless it is context-sensitive, its
    target mentions a literal type, either side could contain type
    variables, or the target has a member this port types but cannot
    resolve;
  - an array literal (tuple-ness follows the context) and a class expression
    (a re-check re-creates the class) decline.
- **Everywhere**: shapes whose type this port computes by a road native does
  not take decline (`??`/`||`/conditional unions without
  `UnionReductionSubtype`, an auto-typed `let x = []`, a mapped type with an
  `as` clause).

**Alternative rejected: re-check the argument under the candidate.** The
overload walk does this (`evict_subtree`, publish the candidate as the call's
inference signature, check, restore). It is the most faithful reading of
`checkExpressionWithContextualType`. It was rejected for the report pass for
two reasons. A context-sensitive function must not be re-assigned. And every
other re-check is a type-dump risk with no report that needs it yet: every
case converted so far is a function argument or a certified published type.
It becomes the right tool if an array-literal argument's report is needed
under an instantiation. The falsifier is a case whose missing report sits at
an array literal and whose type lines are RIGHT.

**Accepted:** the pass declines rather than guesses. A declined argument
leaves the call silent, as before.

## 2. Diff 1 — one applicability report; context-sensitive callbacks report (`.1153`)

[`r6-callreport-1-applicability.diff`](r6-callreport-1-applicability.diff).

**Forcing constraint.** The generic-candidate report
(`check_instantiated_candidate_arguments`) declined every context-sensitive
argument. In r6-triage's `CALL-ARGUMENT-APPLICABILITY-REPORT` cases that
decline was the stopping point most often (`inst#5`, measured by
instrumenting each decline). Example: `then<S extends Function>(cb: (x: T)
=> S)` called with `x => "abc"`, where native reports TS2322 at `"abc"`
through `elaborateArrowFunction`.

**Change** (`calls.rs`, by the diff):

- `check_single_candidate_arguments` and the generic arm of
  `check_single_generic_candidate_arguments` call
  `report_signature_applicability`, with `CandidateContext::Declared` and
  `CandidateContext::Instantiated` respectively;
- `check_instantiated_candidate_arguments`, `check_this_argument`,
  `argument_type_is_not_upstreams`, `head_could_contain_type_variables`,
  `type_mentions_literal` and `absent_member_flags_unreadable` move to
  `call_reports.rs` under the names it uses, and are deleted from `calls.rs`.

`call_reports.rs` carries `#![allow(dead_code)]` until the diff lands. The
diff also adds `crates/tsr-checker/tests/call_reports.rs`, which needs the
hook.

**Preserved orders.** Two orders from the old code are kept, because the
relater does not test excess properties:

- under the declared signature, `report_argument_failure` is called without
  a prior `relate_ternary`, so a union-target object literal's discriminated
  excess check still speaks (`deepExcessPropertyCheckingWhenTargetIsIntersection`);
- under an instantiation, a pair `relate_ternary` calls related continues
  before any certification decline, so a related array literal does not stop
  the walk before a later failing argument (`genericTypeArgumentInference1`).

Both were losses in a first draft that ran one order everywhere.

**Measured** with the diff on `eee504b`, both dumps unfiltered:

- diagnostics +13, 0 lost: chainedCallsWithTypeParameterConstrainedToOtherTypeParameter,
  chainedCallsWithTypeParameterConstrainedToOtherTypeParameter2,
  circularResolvedSignature, contextualTypingOfGenericFunctionTypedArguments1,
  fixingTypeParametersRepeatedly2, promiseChaining1, promiseChaining2,
  typeParameterFixingWithContextSensitiveArguments2,
  typeParameterFixingWithContextSensitiveArguments3,
  genericCallWithFunctionTypedArguments,
  genericCallWithGenericSignatureArguments2,
  genericClassWithFunctionTypedMemberArguments,
  partiallyAnnotatedFunctionInferenceWithTypeParameter;
- types byte-identical on `cut -f1,2`;
- slowcases clean on both dumps;
- Ir domain-model 1,090,055,291 (−0.11%), generic-imports 343,097,683
  (−0.006%).

Twelve of the thirteen are `.1153` cases; circularResolvedSignature is
outside the triage list.

**How it would be wrong.** A context-sensitive callback whose published type
is not native's would now report where native does not. objectGroupBy shows
the shape. `Object.groupBy('a string', c => c)` infers `T = unknown` from
`Iterable<T>` over a string, where native infers `string`, so the report is
TS2322 `'unknown'` at the second call as well as native's at the fourth.
That case was WRONG before and stays WRONG. The inference defect is
`tsr-2zk.1167`'s family (inference candidate guards, routed, §8).

## 3. Certified literals under an instantiation (`.1153`, `call_reports.rs`)

**Forcing constraint.** With diff 1 applied, the certification decline
(`report_argument_type_is_certified`) was the most frequent stop among the
remaining `.1153` cases. Measured per case:
- an array literal (`setMethods` `union([])`;
  `mismatchedExplicitTypeParameterAndArgumentType` `[1, ""]` against
  `number[]`);
- an object literal whose target "mentions a literal type" anywhere
  (`destructuringParameterDeclaration5` `{ y: new Class() }` against
  `{ y: D }`);
- a context-sensitive object literal (`mappedTypeInferenceErrors`).

**Change** (in `call_reports.rs`, committed directly; it needs diff 1 to be
reached):
- the context-sensitive object literal decline goes;
- the blanket "target mentions a literal" decline becomes
  `literal_member_may_keep_literal`, which models
  `getWidenedLiteralLikeTypeForContextualType`. It declines only when a
  member written as a fresh literal meets a target member under which
  `isLiteralOfContextualType` keeps it: a literal of the same kind, a string
  context (template, string mapping, `keyof`) for a string literal, or a type
  variable. Nested literals are followed three levels;
- an array literal is certified unless either side is tuple-like or could
  contain type variables, or an element keeps a literal by the same rule.

**Refused, with the numbers:**
- *Lifting the class-expression decline* lost
  typeArgumentInferenceWithClassExpression1 and 3 (RIGHT → WRONG). It stays.
- *Lifting the literal-mention decline outright* gave +6 with 0 lost, but
  cost domain-model +0.75% Ir (1,098,221,548). The evidence was
  `{ kind: "model039.created", … }`, published as `{ kind: string; … }`:
  widened under the inference context, so its relation fails on the success
  path and `report_argument_failure` runs for nothing. That is exactly the
  widening the decline protects. The precise rule keeps the decline there.
- *Ordering*: running `absent_member_is_unreadable` before the literal rule
  cost +0.4% Ir. The cheap rule now runs first.

**Measured** (diffs 1 and 2 applied, this file, on `eee504b`):
- diagnostics +5 over diff 1 (18 over base), 0 lost:
  mismatchedExplicitTypeParameterAndArgumentType, setMethods,
  destructuringParameterDeclaration5, mappedTypeInferenceErrors,
  typeParameterAsTypeParameterConstraint2;
- types identical on `cut -f1,2`. This was measured on the first-draft
  precise rule; the final rule adds only the literal-kind test, which can
  only certify more;
- slowcases clean;
- Ir domain-model 1,091,881,927 (+0.05%), generic-imports 343,092,472
  (−0.008%).

[`r6-callreport-2-certified-literals.diff`](r6-callreport-2-certified-literals.diff)
adds only the test, which needs diff 1's hook.

**Known text divergence.** setMethods' four TS2739 now match native by code
and position, but print `ReadonlySetLike<number>` where native prints
`ReadonlySetLike<unknown>`. TSR's inference gives `U = number` for
`union<U>(other: ReadonlySetLike<U>)` from `[]`; native gives `unknown`.
That is an inference defect (`tsr-2zk.1167`'s family, routed), not this
report.

## 4. Diff 3 — a single generic construct signature reports (`.1153`, `.1158`, `.1172`)

[`r6-callreport-3-generic-new.diff`](r6-callreport-3-generic-new.diff).

**Forcing constraint.** `CallArity::ApplicableGeneric` was call-only. A
`new` whose single construct signature is generic fell to
`report_overload_argument_failure`, which reads only the overload walk's
verdicts, so it reported nothing. `typeArgumentInferenceConstructSignatures`
is the example: `new someGenerics1<string, number>(3, 4)` gets TS2345
natively.

**Change** (`calls.rs`):
- `ApplicableGeneric` admits a `new` with an argument list;
- `check_single_generic_candidate_arguments` and
  `check_call_type_argument_constraints` read a call's or a `new`'s type
  arguments and arguments.

The instantiation is the one the `new` type road published in
`resolved_call_signatures` (`signatureLinks.resolvedSignature`). Only when
none was published does the report run `check_generic_call_with`, so a
resolved `new` costs no second inference. A `new` with no arguments and no
generic rest needs no instantiation: its `this` arm is skipped, so there is
nothing to relate once the constraints pass.

**Refused, with the numbers:**
- *Re-inferring every generic `new`*: +7, but domain-model +0.9% Ir
  (1,101,057,923).
- *Restricting to written type arguments*: +2, and still +0.22%.

**Measured** (diffs 1–3 and §3's file, on `eee504b`):
- diagnostics +7 over §3 (25 over base), 0 lost: classTypeParametersInStatics,
  dataViewConstructor, genericClassWithStaticFactory,
  overloadresolutionWithConstraintCheckingDeferred (`.1172`),
  exportAssignmentConstrainedGenericType, overloadResolutionClassConstructors
  (`.1158`), typeArgumentInferenceConstructSignatures. Confirmed on the
  final state, with the zero-argument shortcut: 25 over base, 0 lost;
- types identical on the earlier re-inferring draft (`p6`);
- Ir domain-model 1,092,931,848: +0.09% over §3, +0.15% over base. That is
  above the ±0.05% run-to-run noise. **Held for the integrator's
  judgement**: the remaining cost is the published-signature reuse plus the
  argument relation on successful generic `new`s, which calls already pay.

## 5. What remains, with causes

`.1153` cases still WRONG after diffs 1–3 (24 at triage):
- **Type-road gaps** (`calls.rs`/`inference.rs`, MAIN; the report has no
  types to relate):
  - genericCombinators2: an overloaded generic call with written type
    arguments; the arrow types as `error`;
  - partiallyAnnotatedFunctionInferenceError: an arrow with more parameters
    than its contextual signature types as a gap (`contextual.rs`).
- **Inference results differ from native**, so the argument relates
  (`inference.rs`, `tsr-2zk.1167`'s family):
  - paramsOnlyHaveLiteralTypesWhenAppropriatelyContextualized,
    recursiveTupleTypeInference, indexSignatureTypeInference,
    genericCallWithObjectTypeArgsAndConstraints4 and 5;
  - objectGroupBy additionally reports TS2322 `'unknown'` (§2).
- **The class-expression decline** (typeArgumentInferenceWithClassExpression2):
  §3 records the refusal.
- **Single declared candidate relates where native fails**
  (argumentExpressionContextualTyping, controlFlowGenericTypes): the relater
  or the published argument type, not yet split.
- **Overload paths** (typeArgumentConstraintResolution1,
  recursiveTypeRelations, parenthesizedContexualTyping2): the walk publishes
  no `OverloadArgumentFailure`. These are `.1158`/`.1172` work.
- **`super` calls and the remaining no-arm cases** (superWithTypeArgument3,
  templateLiteralTypes3, mixinWithBaseDependingOnSelfNoCrash1): not
  diagnosed.

Not started:
- `.1158` (overload-failure candidate) beyond the two cases above;
- `.1163`: route tagged templates through the report pass. The pass already
  builds the synthetic `TemplateStringsArray` argument
  (`report_call_arguments`); the shifted pick in
  `check_tagged_template_expression` still decides the type;
- `.1169`: spread arguments. The pass synthesizes tuple spreads and keeps a
  spread element as an argument whose type is its element type, but
  `check_candidates_arity` still hands spread calls to `Applicable(None)`,
  and `getSpreadArgumentType` (non-array rest) is declined;
- `.1172`: `non_generic_overload_candidates`' declines are untouched.

The integration tip moved to batch BR (`c3c42d0`) during this session, and
BW had not landed. The diffs were measured on `eee504b` and have not been
re-measured on the new tip.

## 8. Routed, not touched

- JSX overload reports: r6-jsx2.
- Decorator call errors: `tsr-2zk.1173`, unassigned.
- Inference candidate guards: `tsr-2zk.1167`.

## 9. Diffs, in apply order

Each applies on `eee504b` after the ones above it.

1. [`r6-callreport-1-applicability.diff`](r6-callreport-1-applicability.diff):
   §2. +13, 0 lost.
2. [`r6-callreport-2-certified-literals.diff`](r6-callreport-2-certified-literals.diff):
   §3. A test only; the change is in `call_reports.rs`. +5, 0 lost.
3. [`r6-callreport-3-generic-new.diff`](r6-callreport-3-generic-new.diff):
   §4. +7, 0 lost; domain-model Ir +0.09%, held for judgement.
