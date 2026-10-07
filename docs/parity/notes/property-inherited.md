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
