# Calls recovery: concrete receiver signature images

Issue: `tsr-2zk.16.168`. Recovery checkpoint: `0e7824dd`.
Native: `vendor/typescript-go` pinned `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

## Implemented boundary

`resolveTypeReferenceMembers` pads the target's ordinary type arguments with
its concrete receiver for the polymorphic `this` parameter. In
`resolveObjectTypeMembers`, each instantiated heritage reference retains that
same derived receiver before inherited call/construct signatures are read.

`instantiate_signature_for_reference_with_this(receiver, this_argument,
signature)` supplies this mapping through the existing signature instantiator.
Ordinary arguments belong to `receiver`; inherited polymorphic `this` belongs
to `this_argument`. `signature_candidates_of_interface_symbol` threads the
original derived receiver through every base. Declared-before-inherited order,
heritage argument substitution, original declaration/alias identity and the
signature's own fresh generic parameter identities remain intact.

Identity is Checker-local `TypeId`/merged `SymbolId`, not printed names.
No shared fields or cache are added. The existing visiting-symbol stack marks
active traversal; a recursive/unsupported result is `None`, not a completed
empty signature list. Successful ephemeral images enter the existing candidate
consumer; existing signature caches retain their ownership. Expensive work is
heritage traversal plus the existing `instantiate_signature_with_fresh_parameters`
worker. An identity mapper returns the original signature without allocating a
fresh image. This is a correctness prerequisite, not a performance claim.

## Verification receipts

Ignored durable directory: `target/recovery/calls/`.

- `receiver-before.log`: all three regression controls fail on the checkpoint,
  returning `this` instead of `Derived`, `Right`, and `Derived<string>`.
- `receiver-after.log`: all three pass with the owned signature changes.
- `native-controls.txt`: pinned tsgo accepts concrete-derived assignment and the
  contextual discriminant/callback control; the negative ordinary-argument
  control reports TS2345 for number versus string.
- `owned-tests.log`: complete `tsr-checker` tests pass.
- `owned-clippy.log`: checker all-target clippy with `-D warnings` passes.
- CLI smoke: `receiver-project.ts`/`tsconfig.json`, TSR and pinned tsgo both exit
  zero; `smoke-cli.txt` records TSR output.
- Full existing harness, 12,444 case keys each: `owned-types.tsv` has 8,051
  passes, 1,487 failures, 2,906 skips; `owned-diagnostics.tsv` has 4,222 passes,
  1,280 failures, 6,942 skips. `owned-ratchet.json`: zero formerly-PASS losses,
  zero vanished keys, zero gained whole-case passes. These are the existing
  historical baseline harness, not a current-native full-configuration oracle.
- `perf.json`: five fresh-process samples; matching diagnostics and scope,
  observed TSR/tsgo median wall ratio 1.7343. **Not equivalent-work certified**:
  query-input coverage, performed checker work/worker budgets and options
  equivalence are unverified. Verified ratio is null; <=0.50 is not met.

## JSX recovery prerequisites remain unresolved

The saved `box/parity-calls-r2` branch was initially absent from this Box.
After the parent published it, explicit fetch succeeded. JSX commits
`d03f4839`/`b1294b64` contain only evidence/docs, not a resolver implementation.
No old snapshot or historical gate claim was treated as verified.

An owner-local JSX overload experiment was exercised and discarded. Native
`chooseOverload` requires a retained generic inference context across skipped
applicability and the normal retry. The current single-candidate JSX worker
finishes both passes eagerly; reusing it fixes context-sensitive inputs too
early. A discriminant/callback smoke returned `any` where pinned native returns
`string`. Shipping that experiment would violate the native algorithm contract.
No JSX implementation from that experiment is included.

All four named JSX targets remain failing at the final owned-only state.
The issue stays open. Needed integration work: recover saved owned history;
coordinate contextual candidate publication with recover-contextual and exact
TS2769 chains with recover-diagnostics; retain inference/fixing context for the
native skipped/normal retry; run the current-native full-configuration oracle
and relevant equivalent-work performance controls. No >=99.9% campaign or
hotpath/no-slowdown certification is claimed by this slice.

## Recovered semantic rest-pattern cutover

Reviewed owned code hunks from saved `1ffe82b7` and `cfc7ef12`; recovered only
`call_arity.rs`, `signatures.rs`, `calls.rs`, and dedicated rest tests. No old
branch merge, snapshot, or documentation claims imported. Tracked under the
existing recovery issue `tsr-2zk.16.168`; historical `tsr-2zk.9.8` is absent
from this Box's issue database, so no duplicate task was created.

Pinned `getParameterCount`, `getMinArgumentCountEx`, and
`hasEffectiveRestParameter` read the rest symbol's semantic type. A written
`number[]` annotation does not become a fixed tuple merely because its name is
`...[a, b]`; an unannotated rest pattern uses its implied binding-pattern type.
The recovery removes the syntax-count heuristic and the now-obsolete
binding-pattern applicability decline. Existing signature publication owns
preparation; no cache, mapper or shared field added.

Fresh receipts in `target/recovery/calls`:

- `rest-before.log`: recovered native-rest regression fails before code recovery.
- `rest-tests.log`/`rest-clippy.log`: full checker tests and all-target clippy pass.
- `rest-native-final.log`/`rest-cli-final.log`: actual pinned-native and TSR CLI
  output is byte-identical, including implicit-any and three TS2554 diagnostics.
- `rest-ratchet.json`: 12,444 keys in both suites, no prior-PASS losses or vanished
  keys. Diagnostics newly pass `iterableArrayPattern17` and
  `iterableArrayPattern26`; whole-case types passes unchanged.

No current-native full-configuration or performance acceptance is inferred
from these focused controls and historical harness ratchets.

## Instantiation-expression signature entry

Parent owns `tsr-2zk.16.56.1`, pinned `getInstantiationExpressionType`
(checker.go:10660–10738), including object members/indexes, union/intersection
applicability, TS2635 and the private `(NodeId, source TypeId)` cache.

Owned API:

- `Checker::signature_accepts_type_argument_count(&Signature, usize)` implements
  the existing required-prefix/default arity rule. Parent additionally filters
  nongeneric signatures, exactly as native.
- `instantiate_signature_with_type_arguments(&Signature, &[TypeNode]) ->
  Option<Option<Signature>>`: outer `None` means unsupported/incomplete;
  `Some(None)` means a reported native constraint rejection, so the parent keeps
  the original signature; `Some(Some(image))` is the substituted image.

The existing call constraint worker now accepts an arbitrary written node list
and returns its filled argument vector. Instantiation consumes that same vector,
completes the original return through the canonical getter, substitutes with
existing `instantiate_signature`, and clears the image's own parameters. No new
cache or object image. Declaration/captured mapper identity and existing return
publication remain authoritative. Bounds with unsupported relations still
return outer `None`; this API does not certify all native constraint forms.

`calls_instantiation_arguments.rs` exercises dependent defaults (`U = T`) in
parameter and return, exact arity bounds, and a rejected string constraint with
TS2344. Full checker tests/clippy pass. Both complete 12,444-key harness ratchets
retain all prior passes/keys (`instantiation-ratchet.json`). Actual native CLI
control emits TS2344; TSR's standalone CLI still exits zero because the parent's
expression integration is absent here. Thus the signature API is verified, not
end-to-end instantiation expressions. Native complete-work/perf gates remain
unmet; no new whole-case pass is claimed.

### Complete default mapper and constructor control

`fillMissingTypeArguments` now preloads every unfilled parameter slot with
errorType before evaluating defaults against the complete ordered mapper.
JavaScript unknown/empty-object defaults use any, matching the existing native
JS default rule. Unsupported default computation still declines, not a fake
successful argument. The new constructor control proves both its parameter
becomes number and its instance return becomes `Box<number>` through the same
checked instantiation API. Constraint failure still reports TS2344 and returns
`Some(None)` for parent retention of the original signature.

Three signature API controls pass; full checker tests and clippy pass.
`constructor-ratchet.json` preserves all prior passes and all 12,444 keys in
both suites. The final JS empty-object adjustment was followed by target tests
and clippy; no JS corpus parity claim is made for that final adjustment.
Parent's structured instantiation-expression images and end-to-end native CLI
controls are still required; this API does not choose an arity survivor.

### Separate checked-arguments and instantiation entries

Parent's wrapper consumer may use the same operations separately:
`check_signature_type_arguments(&Signature, &[TypeNode]) ->
Option<Option<Vec<TypeId>>>` is public; the filled vector then enters
`get_signature_instantiation(&Signature, &[TypeId]) -> Option<Signature>`.
The latter requires the complete vector, does not repeat constraint diagnostics,
and owns no property/index/wrapper image. The combined API delegates to it.
Dependent-default and pending-query controls pass, clippy passes. Full checker
unit/integration output passes; the command was interrupted at doc-test startup,
so this run is not claimed as a completed full-suite pass.

The existing anonymous signature-type instantiator must not consume the parent's
signature-only wrapper. Its source link/cache and members/index forwarding are
serialized parent/member-owner contracts. Outer mapper support requires the
actual source-link field and complete parent expression worker before adding an
owned `instantiate_type_worker` branch; no stub or heuristic image is added.

### Canonical expression-signature outcome

`get_instantiation_expression_signature(&Signature, &[TypeNode]) ->
Option<InstantiationExpressionSignature>` now provides the native per-signature
expression operation directly: nongeneric/incorrect arity is `Inapplicable`,
checked constraint rejection is `ConstraintRejected` (diagnostic emitted,
original retained by caller), and successful checked substitution is
`Instantiated(Signature)`. Outer None remains unsupported. It performs real
arity/default/constraint checking and invokes the completed substitution worker;
no arity-only winner selection or property/index copies. No redundant expression
NodeId parameter: argument nodes supply constraint spans and parent owns
expression identity/cache/TS2635.

Constructor parameter/instance substitution and exact TS2344 rejection with
unchanged original generic return/parameters are exercised by the existing
behavioral tests. Target tests and clippy pass (`expression-signature-*.log`).
Parent's structured source images and end-to-end expression controls remain
required; this commit does not fabricate a wrapper consumer or full-gate pass.

### Erase-before-mapping and reporting correction

`get_signature_instantiation` now takes the original own parameter metadata out
before invoking `instantiate_signature`, matching native eraseTypeParameters.
It no longer maps own constraints/defaults only to discard them afterward.
Parameter names are borrowed from the moved metadata rather than cloned Strings;
constraint checking also borrows names from its immutable candidate.

The low-level checked-arguments API now explicitly accepts `report_errors: bool`:
`check_signature_type_arguments(&Signature, &[TypeNode], bool) ->
Option<Option<Vec<TypeId>>>`. Ordinary call diagnostics pass true as before;
expression/combined entries always pass true; speculative checking may pass
false. A failed speculative check returns Some(None) without diagnostics; the
expression check then emits the exact single TS2344. Unsupported head/typevar
relation boundaries were not widened.

Behavioral controls verify dependent defaults, constructor substitution,
erased image/target own metadata and speculative-versus-reported rejection.
Target tests, full checker tests and clippy pass (`erase-api-*.log`). No parent
wrapper integration or full compiler/native performance acceptance inferred.

## Owned outer-instantiation source traversal

Applied parent-agreed source/node split directly in `inference.rs`:
`instantiation_expression_sources: FxHashMap<TypeId, TypeId>` and
`instantiation_expression_nodes: FxHashMap<TypeId, NodeId>`. Outer instantiation
maps the concrete linked source with the current mapper and re-enters parent
`get_instantiation_expression_type(source, node) -> TypeId` using the original
written argument node. A missing node link or failed source mapping returns
unsupported error, never a partial wrapper. `mentions_type_parameter_inner`
follows the actual source edge before ordinary signature discovery, including
wrappers whose filtered signatures are empty. Existing visited identities guard
cycles; no new cache, signature/property copy or local completion publication.
Parent owns completed `(NodeId, source TypeId)` cache and active/publication
protocol. Receiver/alias/member/index context remains on the mapped source.

Native control `outer-source-native.ts` verifies generic outer aliases for call
and construct signatures plus preserved readonly member. Pinned native emits
only TS2540 for readonly assignment and two TS2345 bad-argument diagnostics;
positive assignments have no diagnostics (`outer-source-native.log`).

Integration verification is blocked: fetched origin/main remains `db726c9c`;
actual parent fields and expression worker are not present. `outer-source-check.log`
records only missing agreed fields/method. Owned inference changes are delivered
as integration-dependent code, not a verified standalone feature. No temporary
no-op/mock worker was installed; actual parent worker must be supplied before
passing-after, full RIGHT ratchet or performance acceptance can be measured.

## CHOOSE-OVERLOAD-GENERIC-WALK: nongeneric array context

Claimed existing `tsr-2zk.16.66`. All 16 historical target cases fail in a fresh
query of the last runnable binary; integrated current-parent results remain
unmeasured. Native chooseOverload/isSignatureApplicable contextually checks array
arguments against each candidate. The existing nongeneric candidate-walk entry
excluded arrays and only tuple-headed parameter contexts were checked later.
Owned `calls.rs` now admits arrays into that existing nongeneric contextual walk,
including ConcatArray<T> numeric-index contexts. Generic arrays retain their
existing inference-owned scheduling; an attempted expansion there caused a real
`destructuringTuple` diagnostic loss and was rejected, not suppressed.

`concat-native-shape.ts` producer smoke changes nested argument number[][] to
[number, number][] and inner arrays to tuples. `compiler/concatTuples` moves
13/18 to 18/18. Final full historical harness ratchet has all 12,444 keys and
zero prior-PASS losses; one whole types gain and eight diagnostics gains
(`array-context-final-ratchet.json`). Full checker tests/clippy pass. Checks
excluded only the integration-dependent a42 outer source-link hunks temporarily;
those owned hunks were restored afterward. No parent file modified.

Exact native negative diagnostic text/span acceptance is NOT satisfied: the
standalone invalid boolean-array control still emits TSR's flat TS2769 head at
the whole array rather than native's two elaborated property errors. This is the
existing structured diagnostic/elaboration prerequisite, not a passing native
control. Original handwritten ConcatArray control also duplicated the bundled
index signature; that comparison is not certified full parity. No >=99.9%,
full-current-native or equivalent-work/no-hotpath-regression claim. The issue
remains in progress; parent diagnostic and expression integrations plus native
negative and performance gates remain required before acceptance.

## Head signature list API

`head_signatures(&mut self, TypeId, SignatureKind) -> Option<Vec<Signature>>`
is now pub(crate) for parent instantiation-expression object consumers. No
algorithm change: baked complete ordered vectors are filtered by call/construct
kind without completing returns, including authoritative empty vectors. Missing
or unsupported lists still differ from empty. Non-baked shapes use the existing
shared resolver. Parent must not assume that fallback guarantees lazy return
construction for every previously unsupported shape. Existing three calls remain
unchanged. Target signature/receiver tests and clippy pass (`head-api-*.log`)
with integration-dependent outer-link hunk excluded temporarily then restored.

## Native review: empty written list and constraint this

Corrected hasCorrectTypeArgumentArity to accept count zero unconditionally,
matching checker.go:9214. A present-empty instantiation list still filters
nongeneric signatures in the expression entry; required generic parameters fill
with unknown/defaults. The control proves both return and dependent parameter
unknown after an empty list; target tests/clippy pass (`empty-arity-*.log`).

Native checkTypeArguments also applies getTypeWithThisArgument to the instantiated
constraint with the actual type argument. This remains an explicit unmet API
obligation: current reference metadata stores only ordinary arguments, while
member lookup carries concrete this ephemerally. Padding that ordinary vector or
mapping all known this identities would violate native reference ownership.
Member/integration owner must supply a canonical reference-with-this image and
intersection traversal through `get_type_with_this_argument(TypeId, TypeId,
bool) -> TypeId`; owned constraint worker then applies it after ordinary
constraint instantiation and before the relation/report. No eager member copies
or fake successful relation added. Helper parity is not certified until this
native obligation is implemented and exercised.

## Native review: authoritative call lists and lazy instantiation

Minted signature-type call resolution now filters Call signatures. Ordinary
resolution with a present signature_types vector and zero calls no longer falls
back to its retained source symbol. Thus construct-only/empty wrapper call lists
remain authoritative. A construct-only call rejection control passes; the
constructor instantiation test now reads the actual constructor signature rather
than erroneously obtaining it through call resolution. Target tests, full checker
tests and clippy pass (`authoritative-kinds-*.log`), excluding pending outer-source
integration hunk temporarily then restoring it.

Native getSignatureInstantiationWithoutFillingInTypeArguments caches by actual
signature identity and ordered arguments; instantiateSignatureEx leaves return
and predicate lazy under the signature mapper. Existing port metadata has no
signature-owned mapper/pending predicate/composite identity; declaration/captured
return caches cannot substitute for that key. The current helper still completes
returns eagerly and is NOT certified native-lazy/native-cached. Simply removing
completion would lose delayed substitutions. No duplicate declaration cache or
unmapped pending return fallback added. Parent serialized signature representation
must supply actual identity/mapper publication before this work-boundary
obligation can be implemented end-to-end.

## Canonical return demand in generic inference

`check_generic_call_worker` now obtains its semantic return through
`get_return_type_of_signature` before contextual-return inference and spread/final
substitution, rather than consuming the direct provisional return slot. An
unsupported demand returns error without publishing a successful inference.
Existing original pending protocol remains the owner; full checker tests and
clippy pass (`return-demand-*.log`) with parent-dependent outer links temporarily
excluded then restored.

Full lazy writer/consumer cutover still requires parent-owned Signature target
mapper metadata and canonical getter changes in signatures.rs (outside current
calls/inference ownership). Proposed mapper metadata lives on the existing
Signature.target edge, not a declaration-only or duplicate signature cache.
The current get_signature_instantiation remains eager until that getter can
resolve target return/predicate under the exact ordered map. No claimed lazy
completion, target mapper stub or unkeyed side table is delivered.

## Signature-target lazy mapper writer cutover

Applied the actual owned writer: get_signature_instantiation no longer completes
returns. instantiate_signature_lazily substitutes input slots, retains the
original target Arc and ordered SignatureMapper metadata, and leaves return and
predicate unresolved. mapped_signature_return demands the target through the
canonical existing pending getter then maps the actual returned type.
map_signature_predicate maps a completed target predicate without inventing
absence. Existing eager instantiate_signature clears inherited mapper metadata
because its slots are already mapped. No new cache or declaration-only key.

Parent-owned Signature.mapper: Option<Arc<inference::SignatureMapper>> and
canonical signatures.rs getter branches are required; every original constructor
initializes mapper None. Return getter dispatches mapper-bearing signatures to
mapped_signature_return before declaration keys. Completion must retain mapped
predicate/return on the image without publishing to the original declaration
key. This metadata is a native correctness prerequisite, not a speed claim.

Updated return-on-demand behavioral controls use the actual mapped return worker.
Compilation is currently blocked by missing parent Signature field and prior
outer-expression fields/method (lazy-mapper-check.log). No mock/shared-file
placeholder installed, no passing-after/runtime/full-parity/perf claim. Native
signature-identity ordered-argument cache and complete predicate publication
remain unresolved integration obligations; the writer alone is not complete
feature acceptance.
