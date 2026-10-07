# Contextual recovery — tsr-2zk.16.425

## Recovered native operations

Recovery base: `0e7824ddb2f06af1dd4cbbe9779e0a17b720a0f3`.
Pinned native: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Inspected recovery source `box/parity-contextual` through `754673bc`;
no branch merge or snapshot cherry-pick. Owned implementation corresponds to
`c4b69044`, `2f3038f3`, `70f524fc`, `5c801fff`, and `8126dbbd`.
The array-rest change first appeared in unverified `ab1cd894`; this recovery
re-read native `getContextualTypeForBindingElement` and reproduced its failed
controls before implementing it. Historical receipts are not current passes.

- `getContextualTypeForBindingElement`: written holder annotation precedes
  recursive binding-holder or contextually typed parameter projection. Native
  array projection applies even to invalid initialized rest bindings. Defaults
  do not remove undefined from the projected context.
- `getContextualType` / `getContextualTypeForInitializerExpression`: parentheses
  preserve the declaration's implied binding-pattern context; written
  annotations retain precedence.
- `getContextualReturnType` / `getReturnTypeFromAnnotation`: getter annotation
  precedes paired setter effective annotation. Existing raw JSDoc return and
  setter-parameter metadata supplies native reparsed annotations. This lookup
  does not resolve the accessor's type or check its getter body.

### Ownership and work contract

Holder/function `NodeId`, annotation AST nodes, contextual parameter signatures,
`TypeId`, and JSDoc host metadata belong to the same private Checker and its
binder/node store. Strict-null options remain Checker-local. No new cache,
mapper, table, structural member image, or publication state is introduced.
Binding-holder and parentheses traversals follow AST parents; paired setters
are read from the bound accessor symbol's declaration list. Annotation reads
leave completion to the existing type-node/parameter/accessor producers;
missing raw context remains missing, never a completed `any` assumption.
Receiver and written alias context remain those of the actual annotation and
property projection. Expensive work remains existing type-node and contextual
parameter resolution; no duplicated accessor/body work or optimization claim.
Attribution remains the existing `tsr-1yb.11` boundary.

Unannotated holder initializer fallback still requires the inference owner's
explicit-context declaration-checking scope. Do not recursively infer a holder
from the same default initializer. Canonical accessor JSDoc symbol publication
remains the symbols owner's operation; correct contextual getter expressions
alone do not establish the setter's exported contract.

## Current verification

Durable ignored receipts: `target/recovery/contextual/` (not `/tmp`).

Before controls: callback/rest parameters were `any` rather than native
`number`; parentheses produced required rather than optional implied property;
four getter/JSDoc literal controls widened to `string`. Nine dedicated controls
now pass, including annotation-precedence and unannotated-getter controls.
Actual pinned-native declaration emit and TSR CLI positive controls complete
without errors. Invalid rest control produces native TS2322 at `(3,9)` in TSR;
native also produces TS1186 at `(3,14)`, which TSR still lacks. That forbidden
parser/diagnostic consumer is not repaired or suppressed here. The spurious
getter-return TS2322 observed in the baseline CLI is gone.

Unfiltered current type dump: **469785 -> 469801 RIGHT / 477970**, **+16**.
All **469785** prior RIGHT keys remain present and RIGHT. Diagnostic dump:
**9190** prior RIGHT/EMPTY_RIGHT keys, zero vanished keys, zero losses, zero
gains. Both diagnostic dumps contain 10570 rows. Current recovery-base numbers
differ from saved history; no historical baseline was substituted.

Gains: two `classExpressionNames`, six `contextualTypeFromJSDoc`, two
`instantiateTemplateTagTypeParameterOnVariableStatement`, six
`objectLiteralGettersAndSetters` rows. The first target converts completely;
other targets remain incomplete.

Workspace release tests, all-target workspace clippy with `-D warnings`, and
format check pass. All sixteen corpus suites were exercised in an ignored
source mirror to avoid touching protected snapshots. Corpus discovery: 12444
cases. Current checker case parity **8052/9538 (84.42%)**, type-line parity
**98.11%**; diagnostics **4222/5502 (76.74%)**. Other suite results are in
`all-suites.log`; running the suites is not a claim of complete parity.
The source mirror's printed upstream label is its copied root's git fallback
`0e7824dd`, not the oracle pin; the real vendor pin was independently checked.

Fresh-process interleaved measurements, two warmups, 21 baseline pairs and
11 pinned-native pairs, existing whole-project harness:

| Project | candidate/base wall | candidate/base CPU | TSR/native wall |
| --- | ---: | ---: | ---: |
| domain-model | 1.00347 | 1.01196 | 0.99814 |
| generic-imports | 1.00257 | 1.00720 | 0.95570 |

Matched observed options, loaded scope and diagnostics; observed inputs stable.
No observed baseline regression above 3%. Complete-input and actual-checked-work
proof remain false; verified ratio is null. **Neither 99.9% exact parity nor
verified complete-work wall <=0.50 is achieved.** These observations are not
release acceptance or proof of every hotpath's behavior.
Candidate CLI SHA-256:
`0579777d0991709cf9dc94baf76fafa4f097bcb33d95bc7120b344e8fe6efc26`.

## Serialized JSX contextual-stack prerequisite

No JSX file or shared `checker.rs` edit is included. The remote worker has no
cross-worker messaging tool. Exact minimal serialized integration request:

```rust
// Checker field; initializer Vec::new()
pub(crate) contextual_type_stack: Vec<crate::contextual::ContextualTypeFrame>;
```

The initial proposed push/pop helper API below was superseded by the requested
canonical `check_jsx_attributes_with_context` provider in the continuation.
Stack push/pop and query are inline; no separate trivial helper API is required.
Native `contextualInfos`/`pushContextualType`/`findContextualNode` scan
**oldest-first**. Explicit nil must stop fallback; absent frame must not.
Cached frames are excluded for nonzero `ContextFlags`, not automatically for
`CheckModeSkipContextSensitive`. Native context-free expression checking pushes
explicit `any` and retains its skip-context-sensitive check mode.

Native `getContextNode` maps non-self-closing JSX attributes to the containing
root JSX element so attributes and semantic children share candidate inference
context. The inference/calls owner must pair candidate props with that same
root's existing active inference context, preserve check modes and scope both
stacks around the real attribute/children check on every return path. A
self-closing attribute context stays on the attributes node. No JSX heuristic,
extra contextual-signature resolution, or premature completed signature.

Stack key/type identities and alias/receiver context remain private-Checker
local. A frame is active/provisional dynamic context, not completed expression
checking or inference; popping ends its lifetime. Cache frames capture an
already-queried result including nil and cannot publish recursive success.
Expensive checking/inference remains consumer-owned; lookup is only a small
linear stack scan. No second semantic cache is justified.

## Requested canonical provider continuation

Owned `contextual.rs` now contains the real
`check_jsx_attributes_with_context(opening, props, inference_context, check_mode)`
provider requested for calls issue `.16.168`. `inference_context: Option<NodeId>`
is the identity of an existing active context at the native root; None creates
no fake signature or inference snapshot. `ContextualCheckMode` mirrors all
native CheckMode bits. The provider adds Contextual and, only with a real
context, Inferential; it forwards the original skip flags to the canonical
JSX worker, clears intra-expression sites at native check completion, and pops
the dynamic scope even when that worker returns None. The provider does not
change completed callback/signature publication or infer from copied props.

Root frames are read directly by the opening attributes contextual query,
mirroring `getContextualJsxElementAttributesType`; self-closing attributes read
their own frame. `get_contextual_type` honors native with-statement refusal
before oldest-first frame lookup. The field retains the explicit nil domain;
query-time native context flags must exclude cache frames where applicable.

Exact shared integration hunk remains:
`Checker.contextual_type_stack: Vec<crate::contextual::ContextualTypeFrame>`,
initialized with `Vec::new()`. Calls owner must supply
`check_jsx_attributes_worker(opening: NodeId, check_mode: ContextualCheckMode)
-> Option<TypeId>` as its canonical real JSX image/check worker, preserving
SkipContextSensitive, callback ContextChecked lifecycle and source ordering.
Calls/inference owner must consult non-cache dynamic frames newest-first by
inclusive descendant scope in `live_inference_context`: a containing frame's
None shields nongeneric checking from any outer active inference context.
The existing parent-map lookup cannot represent that native nil boundary.
Cache frames do not introduce inference scopes. Real contexts are rooted at
the containing element for ordinary openings, attributes node for self-closing.

Current `cargo check -p tsr-checker` fails specifically at missing serialized
field and canonical JSX worker; receipt `provider-prerequisites.log`.
This continuation **does not compile standalone and is not verified JSX parity**.
No fake worker, foreign-file edits, narrow mock smoke, or old-binary after-gate
is substituted. The previously recorded gates apply only to `0eda302c`.
Four native target/message gates, no-RIGHT-loss and performance must run after
these real integration prerequisites land coherently. The four target identities
were not included in this Box's task payload; retrieve them from the calls
owner's authoritative existing issue rather than guessing test names.

Fetched `origin/box/recover-calls` through `0f60df78`: no JSX provider/shared
state changes are published there yet. Cross-worker messaging is not exposed
by the Box tools, so exact contracts were sent in commentary for parent relay.
## Inherited-this contextual supplier — tsr-2zk.16.225

Pinned native `getTypeOfConcretePropertyOfContextualType` consumes instantiated
property values, not the original property's declaration symbol. Owned
object-member contextual selection now uses `get_type_of_property_of_type`
for concrete properties and removes its redundant receiver-only remapping.
The canonical member producer retains supplying-reference arguments, original
receiver this and written alias metadata. Existing mapped/intersection/union
context roads remain separate; no additional heritage traversal or cache.

Owned `contextual_type_with_this_argument(supplying_reference, declared_context,
this_argument) -> TypeId` exposes the domain seam for native
`resolveObjectTypeMembers`/`getTypeWithThisArgument` before contextual signature
selection and assignment. It delegates to property's existing canonical
`instantiate_for_reference_with_this`, made checker-visible by reviewed
property commit `16ff7c45`. The supplier identifies the formal-this parameter
and ordered generic arguments; the derived receiver identifies its replacement.
Passing the derived receiver as the supplier is incorrect for inherited members.
Calls must retain the substituted contextual signature across ContextChecked
publication, using the existing fixing/non-fixing mapper and active candidate
root. No new per-callback state or guessed declaration-owner traversal.

Focused native `contextualThisType.ts` declaration emit completes without errors.
An ignored coherent source mirror exercised the owned concrete-property read
change without the unavailable JSX cutover. Its target remains four WRONG rows
(base `this` instead of Y): canonical member production still needs the supplied
receiver mapper. This is **not** a target conversion or full-gate pass. An
accidental unfiltered mirror dump timed out; its partial TSV is not evidence.

Current production `cargo check` additionally reports the missing public member
mapper seam (E0624), alongside the previously missing JSX field/worker.
Receipt `this-provider-prerequisites.log`; source is not standalone buildable.
Integrate reviewed property `16ff7c45` plus the actual calls/parent APIs, then
exercise derived Left/Right with distinct generic arguments and inherited
callbacks, all previous RIGHT keys and equivalent-work performance once coherent.
No previously recorded gate is attributed to this continuation.

### Exercised concrete contextual-signature provider

`contextual_signature_with_this_argument(supplying_reference, declared_context,
this_argument, function) -> Option<Signature>` now applies the existing canonical
member mapper before contextual inference/apparent-type conversion and signature
selection. The ordinary function contextual query and this explicit supplier
query share one `getContextualSignature` arity/union policy. No separate union
fallback, second heritage traversal, or assigned-signature cache.

Dedicated consumer invariant test distinguishes Base<string>/Left from
Base<number>/Right and re-queries Left after Right; all parameter and return
slots match `(p: Left, value: string) => Left` and
`(p: Right, value: number) => Right`. Test passes in the ignored coherent source
mirror with the reviewed property's visibility change (the real helper body,
no mock). Pinned native strict declaration emit independently produces the same
three callback types. Receipts: `supplier-signature-smoke.log`,
`supplier-native.ts`, `.d.ts`, `.log`; format check passes.

This is supplier/provider smoke, not end-to-end target acceptance: parent/member
production must preserve the supplying-reference identity when reading inherited
`Y.a`, then calls retains the substituted signature under its ContextChecked
lifecycle. The previously measured four target rows cannot improve until that
identity reaches the provider. No new shared field is requested for this seam;
use the existing native member origin and assigned-signature owner. Production
still needs the serialized JSX field/worker and public member mapper. Full
absence-aware RIGHT/performance gates wait for that coherent atomic integration.

## Parameter initializer raw context — tsr-2zk.16.230

Pinned `getContextualTypeForVariableLikeDeclaration` gives an unannotated
parameter initializer its raw contextual parameter type. The old owned arm
accepted annotations only. The existing parameter getter also performed
assignContextualParameterTypes initializer widening, so invoking it from the
initializer query would recurse. The owned worker now separates raw projection
from that later widening; normal parameter consumers retain widening, initializer
context requests raw projection. No side table or additional publication;
parameter/function identities, options and fixing mapper remain existing
Checker-owned state. Expensive work remains existing signature resolution and
initializer checking, without recursively checking that same initializer to
supply its context.

Actual native strict declaration control and TSR corpus-pipeline probe:
`(handler = n => n)` under `(handler?: (n: number) => number) => void`
checks n as number. Before: n any, callback error. After: `(n: number) => number`.
Written string parameter annotation remains distinct. Dedicated regression and
complete checker release tests pass in the ignored coherent mirror (reviewed
member visibility seam, no JSX mock/field). Unfiltered mirror dump has
469804/477970 RIGHT, +3 over 0eda302c; missing-aware comparison has zero prior
RIGHT losses or vanished keys. Diagnostic rows unchanged in RIGHT verdicts.
The three gains are defaultArgsInFunctionExpressions; two target rows still
fail, so no whole-case conversion is claimed.

Fresh interleaved 21-pair mirror/base observations: domain-model wall 1.00100,
generic-imports 0.99044; observed scope/options/diagnostics match. Complete-work
proof remains false; no release/perf acceptance. Mirror also contains the
already-smoked supplier provider and reviewed mapper visibility change;
source/binary identities are in the harness receipts. Production still requires
atomic JSX/shared-field integration before branch-level full gates.
Receipts `parameter-*.log`, `.ts`, `.d.ts`, `.tsv`, comparisons and perf JSON
under target/recovery/contextual. Existing .16.230 claimed, remains open.
Existing .16.225 is now reachable in the Box database and claimed; its
supplying-reference member producer still blocks target completion. No duplicate.

## Binding-pattern initializer candidate rejected — tsr-2zk.16.63

Existing issue claimed; current 19 named case population has only
classExpressionNames entirely RIGHT, 18 incomplete. A direct owned fallback
from unannotated variable initializer context to the existing
binding_pattern_implied_type producer was investigated against pinned
getContextualTypeForInitializerExpression. Full unfiltered coherent-mirror
comparison rejects it: 84 prior RIGHT type losses, 3 prior correct diagnostic
case losses, no vanished keys, despite 96 type gains. Candidate reverted;
production has no binding fallback change or target conversion claim.

Native getContextualType/getCovariantInference/getWidenedTypeForVariableLikeDeclaration
callers distinguish ContextFlagsSkipBindingPatterns (checker.go9400/9405,
29431,31540). The existing single-context query cannot faithfully classify a
binding pattern as contextual shape but not sole inference source. Losses include
bindingPatternCannotBeOnlyInferenceSource, genericObjectSpreadResultInSwitch,
intraBindingPatternReferences and sibling initializer diagnostics. Existing
binding_patterns.rs also refuses function-valued default producers needed by
objectBindingPatternContextuallyTypesArgument. Canonical flag plumbing,
initializer publication and that producer's explicit-context checking are
cross-owner prerequisites; do not add contextual syntax heuristics or suppress
these failures. Actual pinned-native positive/negative controls and both full
rejected dumps/comparisons retained as binding-* receipts; no performance or
passing gate is claimed for rejected code. Highest owned cluster remains open.

## Explicit contextual this parameter — tsr-2zk.16.289

Native getContextuallyTypedParameterType subtracts an explicit this declaration
from ordinary signature positions; assignContextualParameterTypes supplies an
unannotated this slot from the contextual signature. Owned lookup previously
refused every function containing this. It now reads contextual this separately,
uses declaration index for initializer/rest syntax and signature index for
ordinary/rest projections. Existing assigned signature and fixing state remain
canonical; no cache, duplicate mapping or declaration-name heuristic added.

Current target thisTypeInFunctions converts **645/645 RIGHT** (previous six
wrong rows). Coherent ignored mirror full dumps: **+9 RIGHT type rows**, zero
prior RIGHT losses or vanished keys; diagnostic prior RIGHT keys unchanged.
Aligned whole-RIGHT row groups increase 8103 to 8104; these raw row groups are
not the suite's eligibility-adjusted 9538-case denominator. Dedicated ordinary
parameter/this offset regression passes; complete checker release tests pass.
Actual pinned-native strict normal/rest callback controls are clean; actual
mirror probe matches this C, number ordinary parameter and string rest tail.

Observed candidate/0eda302c baseline domain wall was 1.05677 for first 21 pairs
(CPU 0.98789); a 61-pair follow-up observes wall **0.99671**, CPU **1.00574**.
Generic 21 pairs: wall **0.99017**, CPU **0.98308**. Both receipts retained;
first slowdown observation is not erased. Scope/options/diagnostics match in
these harness observations. Complete-work proof remains false, verified ratio
null. No accepted <=0.50 or exhaustive hotpath/no-regression claim. Mirror
includes parameter-context continuation and supplier seam but not unavailable
production JSX cutover; integration must rerun coherent branch-level gates.
Receipts explicit-this-*; existing .16.289 claimed and remains open for full
campaign acceptance, despite this whole target conversion.

## Object spread operand context — tsr-2zk.16.182

Pinned getContextualType's SpreadAssignment arm forwards the containing literal
context to its operand. Owned contextual query now follows exactly that parent
arm, retaining the same candidate/inference, receiver and alias identities.
No new mapper/cache/publication or extra signature resolution; downstream work
and checked-member completion remain with existing object/calls producers.

Whole target contextualTypeObjectSpreadExpression converts **8/8 RIGHT**.
Other named targets remain incomplete: reverseMappedTypeIntersectionConstraint
199/227, intraExpressionInferences 599/626. Full coherent mirror type comparison
**+7 RIGHT**, zero prior RIGHT losses or vanished keys; diagnostics **+1 correct
case**, zero prior-correct losses or vanished keys. Actual native target CLI
is clean. Complete checker release tests and dedicated callback spread regression
pass; format check passes. Mirror uses real reviewed member visibility, parameter
context and explicit-this changes, not an unavailable JSX worker mock.

Fresh interleaved domain 41 pairs wall **0.98459**, generic 21 pairs **0.98505**
versus verified 0eda302c base; harness matched observed scope/options/diagnostics.
Complete-work proof remains false, verified ratio null. No release acceptance
or exhaustive hotpath claim. Current production still needs serialized
JSX/shared integration before branch-level gates. Receipts spread-* under
ignored target/recovery/contextual. Existing .16.182 claimed; remains open
because all named targets and campaign criteria are not complete.

## Union contextual element projection — tsr-2zk.16.176

Pinned getContextualTypeForElementExpression uses mapTypeEx(noReductions),
dropping nil element projections even from an object/Promise constituent.
Owned union reader removes the object/type-parameter-specific abort and combines
only returned element types with the existing no-reduction union constructor.
No new cache, mapper or traversal; the existing union recursion, tuple/iterated
producer completion and Checker-local type identities remain authoritative.
Concrete tuple elements and alias/receiver context are not rebuilt or widened.

Whole target asyncFunctionReturnType converts **128/128 RIGHT**. Other targets:
asyncFunctionContextuallyTypedReturns 101/105 (previous99), assignmentTypeNarrowing
59/74 unchanged. Full coherent mirror comparison **+3 RIGHT**, zero prior RIGHT
or diagnostic-correct losses, zero vanished keys. Native strict target/control
and actual mirror type probe exercised. Complete checker release tests pass.
Deleted the existing contextual_tuple_reader_preserves_incomplete_union_refusal
test, which pinned an implementation refusal contrary to native mapType behavior;
it is not repinned to an incidental implementation answer.

Fresh interleaved domain41 wall **0.99841**, generic21 **0.99027** against verified
0eda302c base; observed options/scope/diagnostics match. Complete-work proof
false, verified ratio null; production JSX/shared prerequisites still block
branch-wide coherent gates. No release or exhaustive hotpath claim. Existing
.16.176 claimed, remains open until all targets/campaign acceptance. Receipts
element-* in ignored target/recovery/contextual.

## Static optional outer context — tsr-2zk.16.305

Pinned getContextualTypeForStaticPropertyDeclaration consumes
getTypeOfPropertyOfContextualType, whose union map omits non-object undefined
constituents. Owned static-field initializer now uses existing
contextual_property_type rather than ordinary get_type_of_property_of_type.
No new mapper/cache/traversal/publication; original class context, receiver,
written alias and fixing mapper stay with their canonical producers.

Whole sole target staticFieldWithInterfaceContext converts **124/124 RIGHT**,
all 18 formerly wrong rows. Full coherent mirror: **+18 RIGHT types, +1 correct
diagnostic case**, zero prior RIGHT/correct losses or vanished keys. Actual
pinned-native and mirror CLI strict target controls are clean. Complete checker
release tests before adding the static unit and its isolated regression passed;
the resulting added unit later failed the complete library suite due to quote
presentation. The incidental quote-pinning unit is removed in the correction
below; complete checker tests then pass. Format passes.
Fresh interleaved domain41 wall **0.98258**, generic21 **0.99541** vs0eda302c;
observed scope/options/diagnostics match. Complete-work verification remains
false; no <=0.50 release acceptance or exhaustive hotpath claim. Production
branch coherent gates remain pending serialized JSX/shared prerequisites.
Existing .16.305 claimed, kept open for campaign criteria although sole named
target is complete. Receipts static-* under ignored target/recovery/contextual.

## IIFE error context rejected — tsr-2zk.16.306

Native scalar IIFE arm preserves getWidenedLiteralType(checkExpression(arg))
even when errorType. Candidate removed owned error-to-None conversion. Full
coherent mirror rejected it: +7 gains but **3 prior RIGHT-to-GAP losses** in
parsingDeepParenthensizedExpression, no vanished keys or diagnostic-correct
losses. Target3899/3902, remaining3GAP, so no complete conversion. Reverted;
canonical error parameter/consumer publication must be fixed by its owner,
not contextual filtering or a target special case. Actual native target run
and full rejected receipts retained under iife-*; no performance/pass claim.

The complete checker test run exposed the static unit added after .16.305's
full-suite run: isolated quote-presentation assertion passes alone but fails
full library suite (`'a'` versus `"a"`). Removed this incidental presentation
unit, not re-pinned. Complete checker release tests after candidate revert and
unit removal pass (`iife-reverted-tests.log`), format passes. This correction
supersedes any claim that 2fdefa2f's resulting tests were full-suite-safe.
No static production behavior changed; sole124/124 target/full parity/perf
receipts still apply. Existing .16.306 claimed, remains open.

## Union literal property candidate rejected — tsr-2zk.16.130

Five named targets remain incomplete:21/26,14/21,24/27,106/108,17/19.
Investigated native getTypeOfPropertyOfContextualTypeEx mapType leaf retention
by removing the owned post-discrimination unit-leaf refusal. Current coherent
mirror full gate rejects it:44type gains but **six prior RIGHT losses**, zero
vanished keys;1diagnostic gain, zero diagnostic-correct losses. Candidate reverted.
Losses excessPropertyCheckWithUnions/MultipleDiscriminants retain z:true where
native gives z:boolean after contextual discriminant selection.

Read canonical symbols.rs::discriminate_union_root (not flow): it collects every
unit initializer then contextual-property matches it. Native
getApparentTypeOfContextualType/discriminateContextualTypeByObjectMembers uses
only actual applicable discriminant properties and discriminates before member
context. That existing canonical producer is outside contextual ownership;
no second traversal or property-name heuristic added to bypass it. Exact owner
handoff recorded in existing .16.130 notes, issue claimed/open. Actual pinned
native control and rejected full dumps/comparisons retained under union-*.
No implementation, target conversion or performance claim for this rejection;
owned source returns exactly to previous committed behavior. Format passes.

## Contextual effective-rest probe not retained — tsr-2zk.16.175

Read pinned combineUnionOrIntersectionParameters and implemented an owned
nongeneric array-rest combination probe using signature_type_at_position and
existing Array type-reference construction. Fresh successful-build unfiltered
mirror dumps show **zero verdict changes, zero prior RIGHT losses or vanished
keys**. Three named targets remain22/24,18/22,17/21. Reverted the unproven
extension; no partial second rest/generic convention retained.

signatureCombiningRestParameters2's four remaining rows are optional/effective
rest shape: native `(args_0?: any, ...args: any[]) => void` and
`[any?, ...any[]]`, TSR `(...args: [arg0: any, ...optionalParams: any[]]) => void`
and required arg0 tuple. The existing signature supplier/variadic rest and
minimum-argument metadata owns that producer; parent signature/parameter model
is reserved. This probe did not establish a missing array-rest combiner as the
root. Native strict target CLI is clean. Existing .16.175 claimed/open with
exact target evidence; no gain/performance claim.

First build failed on a nonexistent create_array_type API; commands then ran
old binaries. Those first rest dumps/comparisons are **invalid**, not passes
or candidate results. Only rest-*-after successful-build receipts qualify the
zero-change observation. Existing Array constructor pattern was reused on the
second build. Owned source reverted exactly; format passes.

## Conditional and generator contextual prerequisites — .16.227/.16.229

Claimed existing .16.227. Fresh owned apparent-context delegation probe removes
preemptive base-constraint-to-unknown conversion and calls canonical apparent_type.
Targets remain **21/29 and5/10**, no target change. Reverted; conditional branch
capture/default-constraint production in constraints.rs owns the absent context,
not a contextual branch fallback. Actual pinned target control exercised, no
full/perf acceptance inferred from unchanged focused rows.

Claimed existing .16.229. Current generator targets **46/51 and91/105**.
Read pinned getContextualReturnType and checked owned keep-mask: it already
matches ANY|UNKNOWN|VOID|INSTANTIABLE_NON_PRIMITIVE exactly. Failed async
sequence/yield rows are GAP, not a wrong keep-mask. Canonical iteration.rs
`generator_instantiation_assignable_to_return_type` constructs actual yield,
return,next slots then returns Unsupported on unknown relation. That iteration/
relation/signature producer is outside contextual ownership. Do not convert
Unknown to true/false or widen the union mask. Actual pinned esnext sequence
control clean; no generator implementation needed/retained by this investigation.
Receipts conditional-* and generator-native.log. Both existing issues remain
open with exact owner prerequisites; owned source unchanged, format passes.
No whole-case/full no-loss/performance pass claimed for these investigations.

## Return-mapper boolean unit probe — tsr-2zk.16.132

Existing issue claimed; current five target RIGHT counts295/307,192/212,79/84,
56/83,130/141. Read pinned instantiateContextualType30817: signature queries
use nonFixingMapper only when candidates/defaults exist; ordinary contextual
queries use returnMapper, filtering regular false/true units when both occur.
An owned identity-based filter at the existing return-mapper branch compiles,
but fresh focused dumps change **zero target rows**. Reverted rather than retain
unexercised scheduling-dependent code. Actual native target command recorded.

The existing getter always consults live_contextual_mapper first, before its
returnMapper branch; faithful context-flags and candidate/nonFixing/return
mapper plumbing are a coordinated inference/signature-owner prerequisite.
No independent local fallback, type-name shortcut or fake inference scope is
introduced. Owned await/contextual return masks inspected and already match
native operations. Issue remains open with exact boundary; no whole-target,
full prior-RIGHT/performance acceptance claimed for the reverted probe.
Receipts boolean-mapper-*; owned source unchanged, format passes.

## Super contextual candidate not retained — tsr-2zk.16.154

Claimed existing four-target root. Owned sole-base-signature probe uses existing
check_super_expression-produced constructor type and single_call_or_construct_signature
before contextual_argument_type; no overload arity heuristic. Focused result:
targetTypeBaseCalls29/29, other targets9/15,10/13,20/26. Actual pinned-native
control and full mirror dump exercised. Full mirror shows78priorRIGHT losses,
3gains,zero vanished,1diagnostic gain. **Confounded:** mirror still contains the
previous effective-rest probe, so losses are not attributed solely to super.
Owned super probe reverted; no target win/performance acceptance retained.

Native getResolvedSignature(superCall) must publish the actual base constructor
candidate, type arguments and resolving state in calls owner. Parent signature
fields are reserved; no second stateless constructor resolution is landed from
this partial control. Need coherent baseline and calls producer cutover before
retesting; existing issue notes carry exact limitation. Receipts super-*;
owned source unchanged and format passes.

## Primitive-constrained literal cluster — tsr-2zk.16.94

Claimed current eight-target root; last coherent measured RIGHT counts49/58,
28/35,19/28,44/68,3/19,25/43,199/227,30/34. Direct pinned strict declaration
controls emit a:"value",b:"value",c:42,d:string for flat/nested string,
nested number and unconstrained arguments. Actual verified-base corpus probe
agrees flat/unconstrained but nested string/number widen to string/number.

Read existing objects.rs mutable-location literal context exclusion and
signatures.rs isLiteralOfContextualType primitive constraint query. Owned
contextual lookup alone cannot remove consumer bypass: actual candidate-root
context and safe fixing/publication must reach the nested check before that
exclusion can be deleted atomically by its owner. Those files are forbidden in
this turn. No primitive/type-name heuristic, stateless raw-signature retry,
second cache or no-op provider is added. Existing 4bc4705e/9375b1e1 provider
contracts remain the integration seams. No implementation/target conversion
or full/parity/performance acceptance claimed for this investigation. Receipts
literal-constraint-* in ignored recovery directory; existing .16.94 open.

## Tagged effective-argument identity — tsr-2zk.16.92

Claimed highest remaining nine-case tagged root. Current target counts265/267
(twice),80/84(twice),36/38(twice),166/180(twice),15/17; none complete. Read
native getEffectiveCallArguments/resolveTaggedTemplateExpression and owned
contextual substitution plus calls writer. Native signature domain includes
synthetic TemplateStringsArray slot0 and concrete tag receiver this. Current
active call_inference_signatures writer shifts strings away; its contextual
reader intentionally uses index. A completed native signature reader must use
index+1 and substitution_count+1, but calls must first publish **unshifted**
resolved_call_signatures with receiver/type-argument mapper intact. No field
addition required, but writer/domain atomic cutover is calls-owned.

Exact owner API requirement sent: completed tagged resolved signature uses
unshifted effective arguments; active shifted memo remains separate until
clean migration. No retry guessing based on parameter names/counts, no double
signature cache or stateless overload fallback introduced. Actual native strict
receiver/string/substitution declaration control and verified-base corpus probe
both produce result:"receiver"; positive control is not a target conversion.
No implementation/full no-loss/perf acceptance claim. Receipts tagged-*;
existing .16.92 claimed/open. Owned source unchanged.

## Numeric contextual property identity — tsr-2zk.16.49

Native getLiteralTypeFromPropertyName uses a numeric literal's value, not its
source spelling. Owned named object-member context now uses existing
printing::normalise_number for numeric keys (0x10 ->16), consistent with the
already canonical static/computed-name paths. No new lookup cache, mapper,
receiver transformation or fallback. Bound numeric key identity stays canonical
within the same Checker; actual declaration/signature and alias context retained.

Actual pinned strict declaration and TSR corpus probe: hexadecimal property
callback n was any and its type error before; after n:number and callback
(n:number)=>number, with decimal string callback countercontrol unchanged.
Dedicated regression and complete cleaned-mirror checker tests pass. Full
cleaned mirror dumps have **zero changed verdicts, zero prior RIGHT/diagnostic
losses or vanished keys**; no whole existing corpus case converted. This is
native-supported consumer correctness, not completion of the historical .16.49
cluster. Existing issue remains open.

Fresh cleaned-mirror domain41 wall **0.96325**, generic21 **0.98770** vsverified
0eda302c; observed scope/options/diagnostics match, complete-work proof false.
No release/exhaustive hotpath claim. Earlier numeric dumps were confounded by
reverted super/mapper probe leftovers and are invalidated; only numeric-clean-*
receipts support current no-loss result. Clean rebuild removed all those probes.
Receipts numeric-* under ignored recovery directory; production still needs
atomic shared/JSX integration before full branch verification.

## Concrete contextual optionality probe — tsr-2zk.16.228

Claimed existing two-case contextual own-member root. Read pinned
getTypeOfConcretePropertyOfContextualType removal of optional missing type and
existing symbols.rs concrete_contextual_property_type implementation. An owned
consumer delegation to contextual_property_type compiles and full cleaned
mirror dumps show **zero verdict changes, zero prior RIGHT or diagnostic losses,
zero vanished keys** versus numeric-clean baseline. Reverted the unproven
consumer change; no redundant optionality state or function-apply special case.

contextualTypeCaching and unionTypeWithIndexedLiteralType remain incomplete;
canonical own-property/function augmentation and contextual union discrimination
are their producer boundaries, not optionality removal alone. Native actual
target command recorded; no conversion/performance acceptance claimed for this
investigation. Receipts optional-member-*; source unchanged, existing issue open.
Parent TypeId-scoped signature-print work and main push are noted, not rerun
or treated as this worker's verification.

## Certified nil contextual property probe — tsr-2zk.16.180

Claimed existing three-case root; current47/50,97/100,2707/2720. Native
missing contextual property/applicable index yields computed nil, distinct from
an unsupported port. Owned probe used existing declared_members_are_complete,
property absence and applicable-index checks to supply ContextualSignature::Absent
for a certified concrete miss. Actual native/probe ad hoc control improves;
all three named target rows **unchanged**. Reverted repeated contextual property
queries rather than introduce a second completion convention.

Canonical mapped/contextual property producer must expose computed absence vs
unsupported, including remapped-key exclusion and completed applicable index
lookup. Current generic_mapped_contextual_property_type returns None for both
unsupported and excluded keys; broad declared completion cannot prove that
boundary. Exact result-domain requirement sent; no blanket None-to-Absent
fallback or test-name special case. Native CLI controls and focused target
receipts nil-* retained; no wholecase/full no-loss/performance claim. Existing
.16.180 open; owned source unchanged, format passes.

## Getter JSDoc annotation precedence — tsr-2zk.16.200

Native getReturnTypeFromAnnotation reads a getter's effective own return
annotation before paired-setter effective annotation. Owned lookup previously
consulted setter before the getter's separately stored JSDoc return tag. It now
uses existing jsdoc_return_annotation immediately after written getter type,
before paired setter. No parser host metadata, signature field, cache or receiver
mapper added; raw annotation identity/completion stay canonical and Checker-local.

Dedicated divergent getter/setter control fails before, passes after; five prior
getter controls preserved. Pinned allowJs/checkJs strict declaration emit clean;
complete cleaned checker tests pass. Full type/diagnostic dumps **zero verdict
changes, zero prior RIGHT/correct losses or vanished keys**. No whole historical
JSDoc target conversion; accessor symbol/reparser producer still owns residuals.
Fresh domain41 wall **0.98402**, generic21 **0.98562** vsverified0eda302c;
complete-work prooffalse, no release/exhaustive hotpath acceptance. Clean mirror
contains no reverted nil/optionality experiments. Format correction also fixes
numeric regression layout from766eaec8; no numeric behavior changed. Receipts
getter-precedence-*; existing .16.200 claimed/open for broader target acceptance.

## Effective JSDoc parameter initializer context — tsr-2zk.16.230

Ready binding roots .16.294/.16.296 require binding_patterns.rs or parent
parameter producer changes, not owned contextual fallback. Inspected ready
.11.1 IIFE return arm: existing implementation and native/probe callback/object/
written-return controls already agree; no duplicate implementation. Claimed
existing issue and recorded current positive controls.

Runnable owned parameter-default omission: a reparsed native JSDoc parameter
annotation is its effective type before contextual-signature inference. Owned
ParameterDeclaration initializer now reuses jsdoc_parameter_annotation before
raw parameter projection, preserving written annotation precedence. No new
reparser matching/metadata, mapper or completion side table. Existing host AST
identity and Checker options/receiver context stay canonical.

Default callback returning tag object regression fails before and passes after;
all prior getter controls and complete cleaned checker tests pass. Actual native
allowJs/checkJs strict declaration control clean. Full cleaned type/diagnostic
dumps **zero verdict changes, zero protected losses/vanished keys**. No whole
existing corpus conversion; binding-pattern JSDoc tag-by-index matching remains
symbols/reparser owner. Fresh domain41 wall **0.97793**, generic21 **0.99110**
vsverified0eda302c, complete-work proof false; no release/exhaustive hotpath claim.
Receipts jsdoc-parameter-* and iife-return-*; .16.230 updated/open.

## Paired setter value slot — tsr-2zk.16.200

Native utilities.GetSetAccessorValueParameter selects slot1 only when exactly
two parameters and slot0 is this; otherwise slot0. Owned raw getter-context
annotation lookup now mirrors that operation instead of taking parameters.first.
No Parameter structure, mapper, parser or canonical accessor producer edited.
Invalid syntax still receives native value-annotation context before reporting.

Dedicated raw contextual getter test fails before and passes after; prior controls
and complete cleaned checker tests pass. Full cleaned type/diagnostic dumps
**zero changed verdicts, zero protected losses or vanished keys**. No whole
corpus target conversion. Fresh domain41 wall **0.97444**, generic21 **0.98836**
vsverified0eda302c, complete-work false.

Actual native control emits TS2784 at(3,13), accessors cannot declare this.
Actual mirror CLI still emits TS1049 at(3,7) and false TS2322 at(2,26).
Canonical accessor type/check/report consumers outside this lane still need the
same native value-slot operation. Raw contextual test is not end-to-end diagnostic
acceptance; no suppression or foreign edits added. Existing .16.200 remains open.
Receipts setter-this-*; curated code only changes owned annotation projection.

The existing issue remains open: `bd prime` ran, but `bd show` and history cannot
find `tsr-2zk.16.425` in this Box's local database. No duplicate issue created.
Integration owner must update the authoritative existing issue.
