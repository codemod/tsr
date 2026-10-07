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

## F6/G6 query-return signature prerequisite

Parent reported `type F6 = ({ a: string }) => typeof string` collapsing its
callable body; constructor `G6` has the same boundary. Reproduction showed
parameter construction succeeds, but `return_type_of_worker` declines because
the existing unresolved-annotation spelling channel excludes type queries.
Native `getTypeFromTypeLiteralOrFunctionOrConstructorTypeNode` publishes the
anonymous identity before members; `getSignatureFromDeclaration` preserves
shape and `getReturnTypeFromAnnotation` resolves the return independently.

Owned change: the established unresolved return annotation path also consults
`type_query_written_text` for a query with no type arguments. This retains
its existing any/error-return representation and written-node channel rather
than declining the entire signature. It applies equally to function and
constructor signatures, not alias names. Instantiated queries remain outside
this spelling seam. No new identity table, cache, mapper or shared field.
Existing captured declaration keys and `signature_returns` own completion;
`qualified_written_text` retains annotation presentation. This does not change
the alias owner's anonymous publication or certify broader lazy admission.

Receipts: `f6-test-before2.log` fails with `error`; `f6-test-after.log` passes
exact `F6`/`G6` expectations. `f6-native.log` and `f6-cli.log` are byte-identical
actual CLI diagnostics. Complete checker tests/clippy pass (`f6-tests.log`,
`f6-clippy-final.log`). Case `renamingDestructuredPropertyInFunctionType` moves
156/177 to 162/177 type lines, with F10 still the first mismatch. Full existing
12,444-key types/diagnostics ratchets retain all prior passes and keys
(`f6-ratchet.json`); no new whole-case pass or release/perf claim.

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
