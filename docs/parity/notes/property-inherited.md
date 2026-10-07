# Inherited concrete `this` recovery — tsr-2zk.16.225

## Result: candidate rejected; production unchanged

Recovery source was `0e7824ddb2f06af1dd4cbbe9779e0a17b720a0f3`.
The supplied `box/parity-property` ref was initially absent. After the parent
published it, explicit `git fetch origin box/parity-property` succeeded at
`6ab60e38b2bc6240a08ac54bf124aeff3c6b658a`. Its `.16.225` commits
`754c2d4d` and `6ab60e38` record rejected probes and the same contextual consumer
seam; they contain no recoverable inherited-this implementation. Its earlier
`a1780220` also records a reverted receiver-only experiment with erased-signature
and interface-heritage losses. The only post-checkpoint `members.rs` implementation
is `bfca6e08`, the separately scoped protected-origin port (`tsr-2zk.4.14`).
Constructor and checked-JS setter changes also belong to separate issues.
No unrelated issue scope, entire old branch, or `5f26bb1f` box-uncommitted
snapshot was cherry-picked.
Native source is pinned to `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

A reconstructed owned-file candidate forwarded the original concrete receiver
through `members.rs::generic_heritage_member`, selected the supplying base
rather than the receiving owner for nongeneric inherited symbols, and composed
base and outer type arguments without rebinding `this`. It was reverted after
completed corpus gates rejected it. No semantic production changes are retained.

## Native operation and state boundary

`resolveTypeReferenceMembers` (`checker.go:19095`) supplies the reference itself
as the final argument for the target's polymorphic `this` parameter.
`resolveObjectTypeMembers` (`:19106`, especially `:19133–19138`) instantiates
**each base** under the reference mapper, then calls `getTypeWithThisArgument`
with that final argument before adding inherited properties and signatures.
`getTypeWithThisArgument` (`:19573`) appends the argument to a reference whose
arguments do not already include it; it preserves an existing explicit tail
and maps intersection constituents. Supplier and receiver are distinct inputs.

Native publishes provisional structured members before traversing bases,
marks `ObjectFlagsUnresolvedMembers` during traversal, and publishes completion
only after inheritance finishes. A raw inherited declaration symbol is not an
instantiated member image. The reconstructed candidate added no cache or state:
its temporary walk used checker-local `SymbolId` cycle identities and existing
ordered reference arguments, `this_types`, instantiation owners and alias display
metadata. Its expensive boundary remained a base walk per inherited query;
no work-reuse or speed benefit was claimed.

## Fresh controls and completed gates

The direct native declaration control uses `Base<T>`, `Middle<T>`, and distinct
`Left extends Middle<string>` / `Right extends Middle<number>` receivers.
Native emits `(p: Left, value: string) => Left`,
`(p: Right, value: number) => Right`, and `Left` / `Right` self properties.
Checkpoint TSR emitted `Base<string>` / `Base<number>`; the candidate emitted
the native derived types. Two dedicated callback controls passed, including
nongeneric indirect inheritance and own-member shadowing.

Fresh target verdicts convert seven assertions in
`conformance/instancePropertiesInheritedIntoClassType` from WRONG to RIGHT.
`conformance/contextualThisType` retains four wrong contextual assertions.
Target-only improvements did not qualify the candidate.

Both unfiltered before/after type children completed over **477,970 assertions**:

| State | RIGHT | WRONG | GAP |
| --- | ---: | ---: | ---: |
| Checkpoint | 469,785 | 7,190 | 995 |
| Rejected candidate | 469,758 | 7,213 | 999 |

There are **39 previous RIGHT losses**, 12 WRONG-to-RIGHT gains, and no vanished
keys. Loss families include `builtinIterator`, `syncIteratorHelpers`,
`inferenceErasedSignatures`, `coAndContraVariantInferences2`,
`excessiveStackDepthFlatArray`, and `genericRestParameters3`.

Both diagnostic children completed over **10,570 cases**:

| State | EMPTY_RIGHT | RIGHT | WRONG | EMPTY_WRONG |
| --- | ---: | ---: | ---: | ---: |
| Checkpoint | 4,968 | 4,222 | 1,280 | 100 |
| Rejected candidate | 4,965 | 4,222 | 1,280 | 103 |

Five prior correct cases regress: `arrayOfSubtypeIsAssignableToReadonlyArray`,
`coAndContraVariantInferences2`, `genericCallAtYieldExpressionInGenericCall3`,
`implicitAnyGenericTypeInference`, and `inferenceErasedSignatures`. Two cases
improve. Diagnostic vectors also change inside already-WRONG cases; those
changes are not accepted merely because their case verdict is unchanged.
A direct pinned-native replay of `inferenceErasedSignatures.ts` exits 0;
the candidate adds false TS2415/TS2345 diagnostics.

Strict release checker all-target Clippy passes. The complete checker test
command stops after library tests: 186 pass, one fails because
`composite_this_member_keeps_the_whole_intersection_receiver` expects the
previous unsupported enumeration result. This command does not qualify the
package integration suite. No test expectation was repinned to hide rejection.
Interrupted initial corpus children had no completed receipts and are excluded.
No equivalent-complete-work performance measurement was run for a rejected
candidate; neither >=99.9% exact parity nor median wall ratio <=0.50 is claimed.

## Integration prerequisites; owned work exhausted

1. `contextual.rs:2435–2443` reads an inherited property's raw symbol type and
   instantiates under the receiving owner. It bypasses the semantic member-type
   supplier; this explains the remaining `contextualThisType` rows. The
   contextual owner must consume `get_type_of_property_of_type` while preserving
   its optionality, union-discrimination and mapper context. Exact minimal hunk:
   replace `Some(property) => self.get_type_of_symbol(property)` with
   `Some(_) => self.get_type_of_property_of_type(contextual, &name)?` in
   `contextual_type_for_object_literal_named_element`. This uses an existing
   semantic API; no additional helper or shared field is required.
2. `declared.rs::collect_keyof_property_names` refuses generic bases via
   `base_symbols_of`. Newly demanded `keyof this` substitution reaches this gap.
   Native `getLiteralTypeFromProperties` reads winning declaration metadata;
   key enumeration must preserve shadowing, numeric/nonpublic filtering and
   completion rather than reuse an uninstantiated value table.
3. The integration owner must supply a checker-private semantic reference view,
   `get_type_with_this_argument(base: TypeId, this_argument: TypeId) -> TypeId`,
   with ordered hidden-tail identity and tail-aware instantiation, signatures,
   presentation and relation consumers. `heritage_conformance.rs` can then
   compare against native's base-with-derived-formal-this view. No ambient
   override or forced relation success is acceptable. Coordinate signature
   substitution with `recover-calls`; this lane does not own signatures.

These files/interfaces are outside this Box's allocated ownership. The existing
issue stays in progress, with these findings recorded; no duplicate task is made.
A coherent cross-owner port must rerun native controls and a full pair after
integration. Historical C1/C2/C3 rejections in
`docs/architecture/checker-inherited-this.md` are evidence, not current passes.

## Base-first native-order experiment: rejected

A second owned experiment instantiated each heritage base under the enclosing
reference mapper before reading the supplying member with original derived
this (`resolveObjectTypeMembers`, checker.go:19138). Unlike member-first
composition, this retains all 47 `inferenceErasedSignatures` target type rows
as RIGHT and fixes all seven inherited-class rows. Native Left/Right callback
controls and indirect generic callback smoke pass. Four contextual target rows
still require the contextual caller cutover.

Completed full dumps reject this concrete implementation: 477,970 type rows
produce 465,882 RIGHT, 10,749 WRONG, 1,339 GAP, with **3,915 previous RIGHT
losses** and zero vanished keys. All 10,570 diagnostic cases complete, producing
4,936 EMPTY_RIGHT, 4,148 RIGHT, 1,354 WRONG, 132 EMPTY_WRONG: **111 prior
correct-case losses**, zero vanished keys. Therefore the base-first candidate
is reverted, not published as a native-order success. The source-linked wrapper
field exists only in the ignored copy and has no producer in these runs.

Next producer prerequisite is native `instantiateTypeWithAlias` admission
(checker.go:22104): unchanged types return immediately unless the type or its
alias arguments could contain type variables. Mapping arbitrary concrete base
references through the port's general instantiation worker demands completion
that the native fast path avoids. Attribution of every loss is not proved;
no heuristic mention test, error fallback or class-specific exception was added.
Calls/inference owner must establish that native admission/completion contract
before the owned base-first traversal can be retained. Existing completed-empty,
active/provisional and unsupported results must remain distinct.

Receipts: `target/recovery/property/base-first-{types,diagnostics}.tsv`,
`base-first-transitions.json`, `base-first-control.txt`, `base-first-targets.tsv`,
`base-first-smoke.log`, and `base-first-rejected.patch`. Initial background
children were interrupted and excluded; the recorded full TSVs are from
subsequent completed foreground children. No performance claim follows a
correctness rejection; the prior safe supplier APIs remain committed.

### Restoration correction and curated delivery

Intervening Box auto-commit `d39c1507` captured the rejected experiment before
restoration. A bare restore initially restored that snapshot, not the safe
supplier-API checkpoint. The final restoration explicitly uses `16ff7c45` for
`members.rs`; do not cherry-pick `d39c1507` as verified code. No base-first
production candidate survives this correction.

The four contextual rows require the raw-symbol branch to read
`get_type_of_property_of_type(contextual, &name)?`, then skip the subsequent
`instantiate_for_reference(contextual, property_type)` only for that already
semantic branch. Keep mapping for raw branches and the later distinct live /
intra-expression fixing mapper. Calls consume the resulting callback signature,
not original-declaration parameter/return reconstruction or a second derived-this
substitution. No shared field is needed for this contextual caller correction.
These unowned producers are tracked on the existing issue.

One additional reachable owned source-projection defect is fixed: after a source
member miss, semantic value lookup now uses only wrapper filtered-signature
augmentation, never its reused anonymous class symbol as a second member table.
Source-miss projection smoke and strict all-target checker Clippy pass in the
ignored parent-field copy. This does not certify the unimplemented parent mint
or the rejected inherited-this cutover.

## Actual contextual producer replay with available calls helper

Published calls commit `100e97f4` adds
`instantiate_signature_for_reference_with_this(supplier, this_argument,
signature)`. Its inference/signature changes, the owned member receiver
experiment, and the exact contextual semantic-property/no-double-instantiation
hunk were exercised together in the ignored integration copy. **All 11
originally wrong target type rows become RIGHT**, including the four contextual
parameter/body/signature rows. `contextualThisType` diagnostics are EMPTY_RIGHT.
This proves that caller correction reaches the original contextual signature;
it is not a proposed-only or mock-output result.

The integrated smoke still rejects completion: `inferenceErasedSignatures`
loses three RIGHT rows to GAP and `number` to `never`, and gains false TS2430.
`instancePropertiesInheritedIntoClassType` still lacks native TS6234 at (41,16),
so that full diagnostic case remains WRONG. No production candidate is retained
and no full/performance acceptance follows this narrow result.

Exact TS6234 consumer: `calls.rs::resolved_symbol_is_get_accessor` reads
`get_property_of_type` on the receiver; generic inherited getter values resolve
through member typing while that raw symbol query returns None. The calls
comment explicitly identifies this unsupported inherited-accessor case. A
faithful integrated member image must provide the winning origin under the same
instantiated-base traversal and receiver/publication context. Do not add a
second getter-only inheritance scan or expose uninstantiated generic values.

Receipts: `contextual-coherent-{types,diagnostics}.tsv`,
`contextual-coherent-build.log`, and `contextual-exact-hunk.diff` in
`target/recovery/property/`. Production owned tree remains the safe projection
and supplier-API delivery; the contextual/inference files in the checkout are
untouched. Existing issue tracks the signature/key/heritage and winning-origin
consumer dependencies, with no duplicate task.

## Calls-owner supplier mapper seam

Existing native member operations are now checker-visible:

```rust
pub(crate) fn instantiate_for_reference_with_this(
    &mut self, receiver: TypeId, declared: TypeId, this_argument: TypeId,
) -> TypeId;
pub(crate) fn get_type_of_property_with_this_argument(
    &mut self, id: TypeId, name: &str, this_argument: TypeId,
    skip_object_function_augment: bool,
) -> Option<TypeId>;
```

Here `receiver`/`id` is the **supplying reference**, not the derived owner.
Calls/contextual consumers pass the original derived this separately. The first
API reuses the existing ordered parameter/formal-this mapper and
`instantiate_type` publication/re-entry; it creates no extra cache or member
image. A direct supplier control maps Base<string>/Base<number> callbacks to
distinct Left/Right receivers and re-queries Left after Right. A wrong-owner
countercontrol preserves the unmapped base parameter/this, proving that the
receiving owner cannot stand in for the supplier. This smoke passes in the
ignored parent-field workspace copy, against the retained direct native control.

Calls-owner next action: `inference.rs::instantiate_signature_for_reference`
currently maps only generic reference parameters. Preserve original derived
this before signature instantiation/erasure; use this seam for declared member
signature types rather than inventing a second mapper. Signature constraints
containing `keyof this` still require the coordinated winning inherited-key
producer and formal-this heritage context. No rejected member traversal cutover
is re-enabled by exposing these APIs. Full parity/performance acceptance remains
blocked on those shared producers, not claimed by the supplier smoke.

## Shared instantiation-expression prerequisite — tsr-2zk.16.56.1

Owned member projections are implemented against the agreed parent field
`instantiation_expression_sources: FxHashMap<TypeId, TypeId>`. Native
`getInstantiationExpressionType` (`5b1047d`, checker.go:10660–10726) shares
resolved source members/index infos and replaces call/construct signatures.
Parent owns field initialization, `(NodeId, source)` mint/cache, serializer,
index readers, and atomic source-link plus both signature-set publication.
Completed-empty overrides must not fall through to source signatures.

`members.rs` forwards source own/inherited symbols, semantic reads, write types,
readonly status, intersection origins, names and absence ownership. Source
lookup skips augmentation; wrapper Object/Function augmentation uses filtered
wrapper signatures. Original concrete source this/alias context and static side
are retained. Anonymous class-symbol wrappers do not enter the ordinary class
static classification or unsupported-signature fallback. `readonly_target.rs`
forwards mapped/literal readonly metadata, constructor permission, accessibility
receiver context and private-name completeness. No member/index cloning, cache,
fake empty completion or standalone wrapper mint is added. Existing private
Checker/store TypeIds and source publication boundaries remain authoritative;
queries add source-link lookups, not speculative reuse or a speed claim.

Parent must also forward `member_completeness.rs` consumers
`declared_members_are_complete`, `declared_property_table`, and
`relation_property_table`. Calls/inference must retain source-link context under
outer mapper rebuilding and both completed-empty signature overrides. Do not
publish the wrapper before these serialized consumers work. This projection
preserves current source semantics, not the rejected inherited-this cutover.

Smoke verification uses an ignored workspace copy under
`target/recovery/projection-workspace/` with only the agreed default field added
to the copied Checker. Real owned code resolves inherited string values, own
readonly metadata, names, class-static boolean members and absent instance
members through source-linked views. The smoke passes; strict checker all-target
Clippy passes. Logs: `target/recovery/property/projection-smoke.log` and
`projection-clippy.log`. Main checker.rs is untouched. Native expression-mint
end-to-end controls, full absence-aware parity and complete-work timing are
blocked on parent field/mint and serialized consumers, and are not claimed.

## Next owned signature root — tsr-2zk.16.382

While `.16.225` is externally blocked, the now-owned `signatures.rs` fixes
native `getReturnTypeOfSignature`'s `NodeIsMissing(Body)` branch. Native
`ast.NodeIsMissing` (5b1047d utilities.go:66) includes a present zero-width
recovery node, not merely absent Body. `return_type_of_worker` now tests that
span after effective annotation/constructor precedence and returns any. Real
empty blocks have nonzero width and still infer void. No mapper/cache/traversal
or shared field is introduced; signature return completion retains its owner.

Direct native `objectTypesWithOptionalProperties2` recovery control emits any
for the missing method body. TSR before printed `{ x(): void; 1: any; }`, after
`{ x(): any; 1: any; }`. Real empty object methods and block arrows still print
void in the runtime control. All target type rows match.

Completed full 477,970-row type pair: RIGHT 469,785 -> 469,791, WRONG 7,190 ->
7,184, GAP 995 unchanged. Six WRONG-to-RIGHT gains: three target rows and three
`compiler/objectLiteralMemberWithoutBlock1` rows. Zero previous RIGHT losses,
zero vanished keys. Completed 10,570-case diagnostic pair has unchanged verdict
counts, zero previous correct-case losses and zero vanished keys. Strict
checker all-target Clippy passes. Evidence comes from the ignored parent-field
integration copy; no queued contextual fix or unsafe inherited-this candidate
is applied in these full runs. No equivalent-complete-work speed measurement
or release-target claim is made; issue remains open pending integration gates.

Receipts under `target/recovery/property/`: `missing-body-native*`,
`missing-body-before.txt`, `missing-body-after.txt`, `missing-body-target.tsv`,
`missing-body-{types,diagnostics}.tsv`, `missing-body-transitions.json`,
`missing-body-clippy.log`, and real-empty-body control/result. `.16.225` still
requires the serialized contextual, erased-key/heritage and winning-origin
producers; this next-root fix is not partial acceptance of inherited-this.

## Canonical signature-list and index projection delivery

Parent expression worker must reuse `calls.rs::head_signatures(t, kind)` with
`pub(crate)` visibility supplied by the calls owner. No duplicate getter is
added to signatures.rs. It reads the list without forcing signature returns;
unsupported None and completed-empty Some remain distinct. Both wrapper override
sets must win before class/function source fallback or merged-source refusal.

Source-this forwarding is pinned to `getInstantiationExpressionType`
(checker.go:10696–10698): the wrapper receives resolved source members unchanged,
not re-instantiated under wrapper this. Source resolveTypeReferenceMembers /
resolveObjectTypeMembers already supplies source this and ordered arguments.
This justifies the exact source TypeId in retained property reads; a wrapper-this
substitution would change the shared member semantics. Later composite receiver
projection is a separate context requiring its own qualification.

Temporarily owned `index_signatures.rs::get_index_infos_of_type` now delegates
source-linked views to the exact source before mapped/anonymous classification.
It returns the source Option unchanged, including unsupported None, readonly
and declaration/component provenance. No eager index image/cache is created;
the existing source getter owns computation and returned-vector copying.
Smoke resolves readonly string->number index metadata and declaration provenance
through a view; strict all-target checker Clippy passes in the ignored parent-
field copy. An initial test incorrectly expected intrinsic error source indexes
to be unsupported; current getter returns completed-empty for that source, and
that incidental assertion was removed rather than changing semantics. This
projection is not an end-to-end expression-mint completion or performance claim.
Receipts: index-view-smoke.log and index-view-clippy.log.

### Metadata forwarding work boundary correction

Wrapper readonly/write metadata queries now forward directly to the exact source
supplier. The earlier source-value existence probe was unnecessary and could
force signature/property values solely to obtain metadata. Removing it avoids
that duplicate work while retaining source active/unsupported semantics and
without introducing a cache or equal-empty completion assumption. Source-side
readonly, setter and declaration/origin handling remain unchanged. Projection
smoke and strict checker all-target Clippy pass (`metadata-forward-*` receipts).
The expression worker must dispatch on original source before apparent primitive
boxing; calls owner must expose the existing raw-list API with both override
kinds, not add a competing getter or force return types.

## Next runnable member root — tsr-2zk.16.279

`members.rs` union value projection now applies the existing native
`createUnionOrIntersectionProperty` privacy/common-declaration supplier when
any constituent origin is private/protected. The ordinary union value loop
previously bypassed that guard. Native pin 5b1047d checker.go:21554–21558 rejects
distinct nonpublic declarations without a common declaration. Distinct protected
A/B control rejects; shared inherited Root declaration retains number. No class
name, scalar or test-case exception is introduced. Query-local origin metadata
is not published as a completed member image; existing canonical composite
supplier owns privacy/partial/common-declaration decisions. No new cache.

Measured current two targets had 21 wrong rows. Final target type outputs all
match; both target diagnostic cases are RIGHT. Completed final full type pair
has 477,970 rows: 469,791 -> 469,812 RIGHT, 7,184 -> 7,163 WRONG, 995 GAP
unchanged. All 21 gains belong to the two targets. Zero prior RIGHT losses and
zero vanished keys. Completed 10,570-case diagnostic pair has unchanged verdict
counts, zero prior correct-case losses and zero vanished keys. Interrupted final
diagnostic child is excluded; the recorded diagnostic TSV is from its completed
replacement. Strict all-target checker Clippy passes.

Work risk remains explicit: every constituent performs an additional origin
lookup; only nonpublic cases enter the existing canonical composite traversal.
Avoidable temporary origin-vector allocation was removed in favor of that
supplier. Equivalent-complete-work performance/no-hotpath-regression gate is
not measured and must precede parent acceptance. This is a correctness delivery,
not issue/release completion or speed win. `.16.225` remains separately blocked
on serialized signature/contextual producers.

Receipts: union-private-native control/declarations, union-private-before/after
and final-target TSVs, union-private-types.tsv, union-private-diagnostics.tsv,
union-private-transitions.json, and union-private-final-clippy.log under
`target/recovery/property/`. Parent-field integration copy contains no source
view producer; no unsafe inherited-this candidate is applied in these full runs.

## JS inherited heritage arguments — tsr-2zk.16.342

`generic_heritage_member` now reuses `jsdoc_augments_type_arguments` when a JS
heritage entry has no written arguments. Native reparseHosted supplies these
arguments to the heritage reference before getBaseTypes/resolveObjectTypeMembers
(5b1047d); the existing base type worker already follows this rule. No new
factory, alias spelling heuristic, cache, member image or reserved explicit-this
consumer change is introduced. Existing heritage instantiation owns ordered
arguments, completion and receiver context. Work adds the existing JSDoc lookup
only on empty-written-argument JS entries.

Current target had three WRONG rows; after the fix the complete
jsdocAugments_withTypeParameter type case matches and diagnostics are EMPTY_RIGHT.
Direct pinned-native declaration control accepts Numeric/Textual classes with
@augments Base<number>/Base<string>; TSR now retains number/string independently.
Completed full477970 type/10570 diagnostic pair adds exactly those three RIGHT
rows, with zero prior RIGHT losses and zero vanished keys. Diagnostic verdict
counts remain unchanged. Strict checker all-target Clippy passes. Receipts are
js-augments-* under target/recovery/property. No complete-work performance
measurement is claimed; parent must gate acceptance on no slowdown before issue
closure. This correctness delivery is independent of reserved lazy-render files
and is not completion of inherited-this16.225.

## Own member order prerequisite — tsr-2zk.4.5

Parent requested a bounded own-table order cutover for the isolated wrapper
renderer. `collect_structured_property_names` and `collect_static_property_names`
now order original own symbols with existing `compare_symbols` (native
getNamedMembers/compareSymbols, 5b1047d checker.go:22049). Late-bound declarations
join their respective own partitions before ordering. Value-only filtering,
merged original identity, static/instance separation, name deduplication and
existing inherited traversal order are unchanged; inherited tables are not
sorted wholesale. No member values are forced, no cache is introduced. The
existing query-local name vector retains its ownership; comparison reads first
declaration/source position. Synthetic prototype remains at its existing caller
position; no new ordering convention is introduced for that metadata.

Focused control verifies tag-before-method own order, static tag/method order,
and absence of statics from the instance names. Smoke and strict checker all-
target Clippy pass in the ignored parent-field copy (`own-order-*` receipts).
Parent must run its coherent wrapper/native and full no-loss/performance gates
before accepting this requested prerequisite. This bounded own-table cutover
is not completion of the broader inherited partition `.4.5` contract.

## Method/assignment receiver widening — tsr-2zk.16.208

Property access now applies native getWidenedType when the access is an
assignment target or isMethodAccessForCall (5b1047d checker.go:11262/11466).
Direct call/new callee identity is checked through parenthesized expressions;
ordinary detached reads retain their non-widened receiver. Existing
widen_object_literal_freshness implements the worker/cache; no alternate
widening or alias heuristic is added. Reserved reference-this/readonly/order
helpers are untouched. Work adds callee-parent inspection and invokes existing
widening only on native selected contexts; no additional cache or value-metadata
forcing is introduced.

Direct native control: called/wrapped/null-containing array values() returns
ArrayIterator<any>, while detached [].values remains
()=>ArrayIterator<undefined>. TSR matches all four. Current cluster's ten wrong
rows become RIGHT. argumentsAsPropertyName2 type case matches; iterator cases
retain independent constructor/display failures, so whole-three-case completion
is not claimed. Completed full477970 type/10570 diagnostic pair adds exactly10
RIGHT rows, with zero previous RIGHT losses/zero vanished; diagnostic verdict
counts unchanged. Strict checker Clippy passes. No equivalentcompletework
performance/no-hotpath-regression measurement is claimed; parent gate required.
Receipts: receiver-widen-* in target/recovery/property. This is a bounded native
member consumer delivery, not closure of broad issue acceptance.

## Computed callable property names — tsr-2zk.16.45

Parent requested callable_property_name helper-only ownership in
callable_expandos.rs. The helper now reads the original ComputedPropertyName
and semantic UNIQUE_ES_SYMBOL type before quoting a nonidentifier name,
matching getPropertyNameNodeForSymbolFromNameType (5b1047d
nodebuilderimpl.go:2455). The existing entity-expression renderer provides
computed spelling; no raw display-string parsing or alias-name exception.
A literal 'Symbol.species' remains a quoted string, not a computed member.
No new name cache, table image or metadata fallback is introduced.

Focused computed-unique versus written-string control passes. Strict checker
Clippy passes. Completed full477970 type/10570 diagnostic pair is unchanged,
zero prior RIGHT losses and zero vanished keys. This is a dynamic wrapper-print
prerequisite, not a measured named corpus conversion. Parent Array wrapper
[Symbol.species] print smoke and source-qualified alias tracking must be
qualified at the actual renderer site. Expression name-type query remains at
print boundary; no complete-work perf claim. Receipts computed-name-* in
target/recovery/property; parent-reserved Signature/AST/readonly/order suppliers
are untouched.

## Object-literal late-bound accessors — tsr-2zk.16.351

late_bound_members_of now includes ObjectLiteralExpression properties alongside
class/interface/type-literal members. Native SymbolFlagsLateBindingContainer
includes object literals (5b1047d getResolvedMembersOrExportsOfSymbol / lateBindMember
checker.go:15930/16005); the existing accessor symbol worker already merges
same-name getter/setter declarations from this supplier. Original declaration
and computed-key identities, static partition and existing cache/publication
(owner SymbolId, static bool) are unchanged. No extra cache/memberimage/value
fallback. Expensive name expressions retain the existing completed late-binding
worker; the added container becomes part of that same computation.

Current target symbolDeclarationEmit10 had2WRONG rows; both are nowRIGHT and
diagnosticcase EMPTY_RIGHT. Directnative computed getter/setter declaration
control exit0. Completedfull477970types/10570diagnostics:7typegains (2target,
5computedgeneratoraccessor rows),1diagnosticcase gain(iteratorExtraParameters),
0previousRIGHTloss/0vanished. Strict checkerClippyPASS. Completework performance
is unmeasured and must gate parentacceptance. Receipts obj-accessor-* under
target/recovery/property. Parentreadonly/order/anonexports/computedname helper
reservations untouched; this is independent late-binding supplier code.

## Union-key indexed writes — tsr-2zk.16.213

Newly owned indexed.rs definite-write dispatcher enumerates semantic union
keys, obtains each divergent setter type through the existing member supplier
(or ordinary indexed type), and intersects results, matching 5b1047d
getIndexedAccessTypeOrUndefined checker.go:26993. Read dispatch is unchanged.
Original receiver/key TypeIds and existing resolution ownership remain; no
cache, fake key, dropped failed constituent or alias-resolver edit. The
query-local vector exists only for union write traversal.

Native control has boolean intersection for writes and number union for reads;
TSR matches. Completedfull477970types/10570diags adds4RIGHT targetrows with
0priorRIGHTloss/0vanished;diagnosticcounts unchanged;ClippyPASS. Fifth historical
row remains: computed symbol getter/setter split symbols; symbols.rs
write_type_of_accessors reads only its own declarations and misses the matching
setter when given the getter's symbol. This unowned merged-symbol producer must
preserve native getWriteTypeOfSymbol context. No computedname/setter exception
added. Broadtwo-case completion/perf gate unverified; parentacceptance required.
Receipts accessor-write-* in target/recovery/property. Parent indexedalias/
printerreceiver and reservedmemberhelpers untouched.

## Setter reference instantiation — tsr-2zk.16.282

write_type_of_property_of_type now applies existing instantiate_for_reference
to its divergent setter type, matching native getWriteTypeOfInstantiatedSymbol
(5b1047d). Previously raw T|undefined and callback S types escaped through
instantiated Test1<string>/Test2<{property:string}> receivers. No second mapper,
cache or alias construction; existing reference/instantiation completion and
ordered source arguments remain authoritative. Reserved reference mapper body
and readonly/order/callable helpers untouched. Inherited supplying-base mapper
qualification remains separate; this fixes the canonical reference consumer.

Both target type cases now match, with diagnostic RIGHT/EMPTY_RIGHT. Completed
full477970types/10570diags adds21RIGHT rows,0priorRIGHTloss/0vanished; one
case diagnostic WRONG->RIGHT. Directnative string/number Box setters retain
string|undefined versus number|undefined and reject wrongnumber-to-string.
StrictClippyPASS. Perf is unmeasured; parentno-slowdowngate pending before issue
closure. Receipts write-instantiation-* under target/recovery/property.

## Computed accessor write provenance — tsr-2zk.16.213

The remaining computedPropertiesWithSetterAssignment row now follows the
existing semantic symbol-entity key in indexed definite-write dispatch, then
write_type_of_property_of_type selects the setter declaration from the existing
late-bound (owner,static,name) partition. Native lateBindMember merges that pair
into one symbol; this binder keeps split symbols. Original declaration identity
and receiver mapper are retained; no synthetic alias key, cache, value forcing
or reserved indexed resolver change. Ordinary literal-key dispatch unchanged.

Wholecomputed setter targettypes+diagnostics match; directnativeCLI exit0.
Completedfull477970types/10570diags exactly1RIGHTgain,0priorRIGHTloss/0vanished,
diagcountsunchanged;ClippyPASS. Broader divergentAccessorsTypes8 stillWRONG for
independent single-key/contextual failures; no whole-two-case acceptanceclaim.
Computed-only latebinding query work remains performance-unmeasured and must be
qualified by parent. Receipts paired-setter-* in target/recovery/property.

## Namespace-member assignment typing — tsr-2zk.4.4

Existing receiver_alias_is_namespace_import helper is checker-visible and reused
in property/element semantic assignment typing. Native isAssignmentToReadonlyEntity
5b1047d:27279 checks parenthesized receiver resolving to NamespaceImport ALIAS,
not module type shape. A found imported member assignment returns error-family
any; readonly diagnostic reporter remains unchanged. Local namespaces and
ordinary module variables are not blanket-readonly. No alias cache, duplicated
predicate or reservedassignment_target_meaning/readonlysupplier edit.

Directnative imports/parenthesized indexes/update reject whilelocalnamespace
write remainslegal. Full477970type/10570diagpair adds36RIGHTrows,0priorRIGHTloss/
0vanished;externalModuleImmutableBindings diagnosticWRONG->RIGHT. Its whole type
case matches;importsImplicitlyReadonly retains2importedidentifier (notmember)
assignmenttypefailures, diagnosticsRIGHT. StrictClippyPASS. No completework
perfclaim; parentgatenecessary. Receipts namespace-write-* in recoverydirectory.

## Receipt location and hashes

Receipts are in repository-ignored `target/recovery/property/`, not `/tmp`:
full before/after TSVs, transition enumeration (including vanished-key checks),
raw direct native declarations and inference replay, target controls, test and
Clippy logs, binary identities, and `rejected.patch`. They are Box-local and
must be collected before machine destruction; ignored files are not fetched
with the commit.

SHA-256:

- Types before: `2c963ce2d05c0a010f857205b17689444b3fe75ebcd7ca86a37ea714c5506ba1`
- Types candidate: `c59b8dc486aa5743b0aadd42ce84941737e25f5c40039dc9a5e8ec17c59ef176`
- Diagnostics before: `30000e586b6a6acf1ac5aa05ef16b33d193d75254335c12d19e6c83386287994`
- Diagnostics candidate: `ece61336888ec87e3f82292c1d259dc6f67634c73abaf38ae796d26f69325264`
- Rejected owned patch: `947e2d5ff5cce5ed48184ee44e2e4de5dbaa60197e27e423768999834c32f6b0`
