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

The existing issue remains open: `bd prime` ran, but `bd show` and history cannot
find `tsr-2zk.16.425` in this Box's local database. No duplicate issue created.
Integration owner must update the authoritative existing issue.
