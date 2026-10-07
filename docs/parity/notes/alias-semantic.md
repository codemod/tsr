# Alias semantic recovery — tsr-2zk.16.2

Native pin: `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Recovery checkpoint: `0e7824ddb2f06af1dd4cbbe9779e0a17b720a0f3`.

## Implemented boundary

`getDeclaredTypeOfTypeAlias` resolves a generic alias's body, not an opaque
nominal object. `getTypeFromLiteralTypeNode` returns the existing regular literal;
`instantiateTypeWithAlias` leaves it unchanged. `declared.rs` now preserves that
identity for literal bodies, including parenthesized bodies. Parameter-bodied
aliases use the same parenthesis-transparent body projection at declaration and
reference sites. Explicit unions retain their existing alias construction.

- **Identity/owner:** existing Checker-owned `declared_types`, keyed by binder
  `SymbolId`; existing `instantiations`, keyed by symbol and ordered `TypeId`
  arguments. No printed-name keys, new cache, shared fields, or symbol-domain
  conversion.
- **Publication:** literal declarations use the existing DeclaredType resolution
  push/pop and circularity reporting. References force the existing declared
  owner before publishing the unchanged literal into the instantiation map.
  No provisional literal or recursive assumption becomes a completed result.
- **Receiver/alias context:** literal and parameter types are pre-existing native
  types; these operations do not attach the enclosing alias. No member receiver
  or written-reference presentation path changes.
- **Work boundary:** the existing body-node resolver runs under declared-type
  completion; cache reuse is unchanged. No new traversal or broad body evaluation
  is introduced. Expensive-work attribution remains `tsr-1yb.11`.

## Observed verification

Receipts are in ignored `target/recovery/alias/`, not `/tmp`.

- `alias_semantic_literals`: two failures before, three passes after. Covers
  string, signed numeric, boolean, null, bigint literal bodies, parenthesized
  parameter identity, and the distinct explicit-union presentation case.
- Direct pinned-native CLI control: literal assignments accepted, incompatible
  literal rejected with TS2322. TSR previously emitted two extra assignment
  errors; after, `native-focused.txt` and `after-focused.txt` compare byte-for-byte.
- 40 existing tests passed across `aliases`, `alias_naming`,
  `conditional_alias_members`, `interned_type_identity`, `variadic_tuple_alias`.
- Existing unfiltered checker-types case oracle: 8,051 passed, 1,487 failed,
  2,906 skipped, unchanged. Before/after raw verdict dumps compare byte-for-byte:
  summary 477,970 assertions, 469,785 RIGHT, 995 GAP, 7,190 WRONG. Thus no RIGHT
  losses or vanished keys in this oracle. These are not full-configuration totals.
- Anchor gate: 4,501 upstream references checked, zero unresolved.
- Fresh-process synthetic 3,000-variable CLI smoke, seven samples: TSR median
  before 0.263191 s, after 0.241101 s, native 0.291136 s. After/before 0.9161;
  after/native 0.8281. The baseline emits false positives and does different
  diagnostic work: **not equivalent complete work, not a speed claim**.

## Unmet campaign prerequisites

The saved source was initially absent. After the parent published it, explicit
fetch supplied `origin/box/parity-alias`. Its two commits after the recovery
checkpoint (`16f9a7aa`, `6e099017`) are uncommitted snapshots, not verified commits;
neither was cherry-picked. The owned hunks were reviewed against native and
re-derived, with current verification below.

The broader alias target/alias presentation infrastructure belongs to the
integration and symbols owners. An unchanged control still rejects assignment
from `Choice<string>` to `"a" | "b"` although native accepts it; declaration and
reference presentation alone are not semantic target publication. No shared API
was introduced in this slice. Generic recursive bodies, enclosing aliases,
imported written arguments, and general instantiation publication remain open
under the existing issue.

## Recovered alias-free instantiation

The recovered owned hunks complete `keyof` and argument-free `typeof` alias bodies
through the existing `instantiate_type` API. Native `getTypeAliasInstantiation`
completes the declared owner before cache lookup and constructs a mapper from
ordered local parameters to arguments; `instantiateTypeWorker` maps an index type
through `getIndexType`. Successful declared resolution seeds the own-parameter
instantiation. Templates retain their existing worker, and queries with written
type arguments remain outside this route. Written alias annotations reuse the
existing `qualified_written_text` channel, not semantic cache keys.

Identity/owner and publication follow the existing maps above. `instantiate_type`
owns the expensive walk on misses; no duplicate cache or active-mapper result is
introduced. Concrete value identity is retained for `typeof`; there is no new
receiver/alias-bearing image. Shared hunk/API requirement: **none**. General
alias-bearing mapper/presentation publication requires integration/symbol owners;
`typeof f<T>` requires the calls/expression owner's instantiation-expression
worker before this route can extend to it.

Current recovery evidence, replacing historical saved-branch claims:

- New `alias_semantic_declared_body` fails before and passes after: cold reverse
  argument order, distinct object-key arguments, warm reuse, actual value identity.
  All 44 targeted tests pass (including the preceding literal controls).
- Direct native-supported assignments remove all five false positives. A
  single-literal rejection control compares byte-for-byte with native. The
  union rejection control still omits native's nested TS2322 detail; this
  diagnostics-owner prerequisite is explicitly not exact parity.
- Unfiltered verdict oracle against checkpoint: 11 WRONG→RIGHT, zero RIGHT
  losses, zero vanished recognized keys. Summary: 477,970 assertions, 469,796
  RIGHT, 995 GAP, 7,179 WRONG. Case totals remain 8,051 passed / 1,487 failed /
  2,906 skipped. Gains: two `recursiveTypeRelations`, one `keyofIntersection`,
  eight `mappedTypeAsClauses`. Other already-WRONG rows change without becoming
  exact; no broad success is claimed.
- Anchor gate: 4,501 checked, zero unresolved.
- Seven-sample interleaved fresh-process 3,000-reference `keyof` smoke: baseline
  median 0.133485 s, recovered 0.137989 s, native 0.128580 s. After/before **1.0337**;
  after/native **1.0732**. All exit zero with empty output, but the baseline skips
  semantic mapping: equivalent complete work is unverified. The measured smoke
  is 3.37% slower; **no hotpath/no-slowdown gate or speed target is certified**.
  Receipts: `recovered-perf.txt`, `recovered-transitions.txt`,
  `recovered-target-tests.txt`, `semantic-focused-*.txt`.

## Binding value root — tsr-2zk.16.2.1

The parent fixed the preserved F6 test at `db726c9c` in `destructure.rs`:
`get_type_for_binding_element_parent` now admits type-signature containers and
uses the existing implied binding-pattern worker. Native
`getTypeForVariableLikeDeclaration` reaches `getTypeFromBindingPattern`; the
initializer-free identifier leaf in `getTypeFromBindingElement` is regular `any`.
Thus the bound `string` and its `typeof string` query are actual `any`, not an
unsupported return requiring an alias-name fallback.

The owned callable-publication workaround from `6163a7bd` and its
workaround-specific test are removed. `function_types.rs` remains unchanged.
The independent literal and alias-free semantic body changes remain. The user
F6 test was neither rerun here, modified, nor committed.

Native rename controls cover `ordinary`, `string`, `number`, `boolean`, `any`,
`unknown`, `never`, `object`, `symbol`, `bigint`, `undefined`, each through a
function and constructor alias. Native emits exactly 22 TS7031 diagnostics, no
missing-name diagnostic; declaration emit produces 22 `any` call/construct
results. `alias_semantic_binding_renames.rs` checks those actual consumer types,
not alias naming. It passes against the pinned parent-root worktree `db726c9c`
with unchanged `declared.rs` and `function_types.rs`. It requires that parent
commit when integrated; the old Box checkpoint lacks its binding-root fix.

Receipts: `binding-renames.ts`, `native-renames-diagnostics.txt`,
`native-renames/binding-renames.d.ts`, `binding-renames-parent-tests.txt` under
ignored `target/recovery/alias`. Tests for the independent alias-body changes
also pass after removal (`without-workaround-tests.txt`). No full-workspace gate
or parent-reported F6 failure was rerun.

## Parenthesized intrinsic alias references

The declaration path already unwraps parentheses around keyword bodies, but
reference construction recognized only directly written keywords. References now
reuse `original_generic_keyword_alias_body` and the completed declared owner,
like literal bodies. The existing ordered-argument cache and intrinsic identities
remain unchanged; no new worker/cache or spelling key. An eleven-keyword regression
fails before and passes after; related alias targets pass. Native declaration emit
confirms all eleven inferred result types are the corresponding intrinsic.

Existing unfiltered verdict oracle versus `215d1b31` is unchanged: no RIGHT losses
or vanished recognized keys. Anchor gate remains 4,501 checked / zero unresolved.
Seven fresh-process interleaved samples on 3,000 primitive references: after/before
0.9650, after/native 0.8193. Baseline emits false positives and omits the semantic
answer, so this is not equivalent complete work or a release speed claim. Receipts:
`keyword-before-tests.txt`, `keyword-after-tests.txt`, `keyword-transitions.txt`,
`keyword-perf.txt`. TSR declaration CLI returned success without an artifact on
this control; native declaration evidence is not claimed as TSR emit parity.

TypeQuery caller migration is prepared for parent atomic integration with the
integration-owned real
`get_instantiation_expression_type(expression_type: TypeId, node: NodeId) -> TypeId`
API. It removes the argument refusal, resolves the actual expression (including
`this` receiver), invokes the worker with the original node, then follows existing
widening/regularization. The worker owns canonical raw-list nil/empty/span metadata,
argument checking, constraints, TS2635, and `(NodeId, source TypeId)` publication.
No adapter, stub or duplicate constraint worker is introduced.

This caller commit intentionally cannot compile alone: the parent explicitly
requested a curated hash for atomic integration before publishing the wrapper.
Fetched main `db726c9c` and calls `0f60df78` do not supply that method. Caller runtime
verification is therefore pending, not passing. Native typed-argument controls
already prove direct result `string`, qualified constrained object result,
TS2635 at (10,39), and TS2344 at (11,47). Receipts: `type-query-native.txt` and
`native-type-query/type-query-control.d.ts`. Parent must apply caller and actual
wrapper together before target/full parity/performance verification.

## Parenthesized indexed alias body dispatch

Native `getTypeFromTypeNodeWorker` unwraps parentheses before dispatch. The
existing indexed-alias evaluator was selected only for directly written indexed
bodies. Its entry now unwraps parentheses and reuses the same evaluator,
ordered-argument cache, deferred operands and alias-origin publication. No new
semantic cache, receiver substitution or general-body fallback.

The new indexed projection regression fails before and passes after. Verification
uses frozen `caa7cc2b` plus only this owned hunk in a recovery worktree because the
separate TypeQuery caller requires an unpublished parent API. Existing indexed
and conditional alias target tests pass; real CLI string projection acceptance
and number rejection diagnostics compare byte-for-byte with native. Unfiltered
477,970-assertion oracle unchanged: no former RIGHT losses or vanished recognized
keys; no whole-case gain claimed. Seven interleaved fresh-process 3,000-reference
samples: after/before 0.9903, after/native 1.5721. Baseline omits correct semantic
projection and emits errors, so not equivalent complete work or release speed
certification. Receipts: `indexed-before.txt`, `indexed-after.txt`,
`indexed-native.txt`, `indexed-smoke.txt`, `indexed-transitions.txt`,
`indexed-perf.txt` under `target/recovery/alias`.

## Parenthesized conditional alias root

`evaluate_conditional_alias` now unwraps parentheses before selecting the existing
conditional/reference worker, following native `getTypeFromTypeNodeWorker`.
Ordered parameter frames, recursion/depth guard, deferred refusal and publication
remain unchanged; no branch/name heuristic or extra cache.

Regression fails before and passes after; existing conditional targets pass.
Verification uses the same frozen pre-TypeQuery worktree plus indexed/conditional
owned hunks. Real CLI exercises both conditional branches; incompatible assignment
diagnostics compare byte-for-byte with native. Existing unfiltered 477,970-row
oracle unchanged, no formerly RIGHT losses or vanished recognized keys, no
whole-case gain claimed. Seven interleaved fresh-process samples: after/before
0.99845, after/native 1.57815. Baseline emits false positives and lacks the correct
projection, so equivalent complete work is unverified; no release performance
claim. Receipts `conditional-paren-*` under `target/recovery/alias`.

## Qualified conditional alias chain target

`evaluate_conditional_alias_reference` now resolves its actual entity name through
existing `resolve_entity_name_ex`, as native `getTypeFromTypeAliasReference` does,
rather than refusing everything except a bare identifier. The canonical resolver
owns qualified/import alias resolution; no duplicate namespace walk or printed-name
key. Ordered argument/default mapping, evaluation frame, depth guard and result
alias remain unchanged. Unsupported targets still return None.

Qualified-chain regression fails before and passes after; conditional and qualified
reference targets pass. Real CLI acceptance/rejection diagnostics compare
byte-for-byte with native. Existing unfiltered 477,970-assertion oracle unchanged,
zero former RIGHT losses/vanished recognized keys; no whole-case gain claimed.
Seven interleaved fresh-process samples: after/before 0.99970, after/native 1.64152.
Baseline emits false positives and lacks the semantic projection, so equivalent
complete work and release speed remain uncertified. Receipts
`qualified-conditional-*` under `target/recovery/alias`; frozen verification
excludes the pending TypeQuery worker dependency.

## Keyword body under an outer alias mapper

The ordinary keyword-instantiation constructor now unwraps parentheses before
keyword dispatch, as native `getTypeFromTypeNodeWorker` does. The original-body
preflight still rejects active alias frames; the ordinary worker resolves a
keyword intrinsically in that context instead of minting `Primitive<number>`.
No general body fallback, name predicate, mapper cache or metadata publication
change. Intrinsic-marker handling remains on its existing route.

Regression through an outer indexed alias fails before and passes after;
related indexed/conditional targets pass. Real CLI accepted string assignment and
number rejection diagnostics compare byte-for-byte with native. Existing
unfiltered 477,970-assertion oracle unchanged, zero former RIGHT losses/vanished
recognized keys; no whole-case gain claimed. Seven interleaved fresh-process
samples: after/before 0.96618, after/native 1.60513; baseline emits false positives,
so no equivalent-complete-work or release-speed claim. Frozen verification
excludes the pending TypeQuery wrapper. Receipts `keyword-mapper-*` under
`target/recovery/alias`.

## Parenthesized variadic tuple alias body

The existing rest-tuple instantiation entry now unwraps parentheses before tuple
construction, following native `getTypeFromTypeNodeWorker`. Ordered mapper,
rest-element metadata and normalized tuple worker are unchanged; no new cache,
name fallback or unrelated signature ownership change.

Regression fails before and passes after; variadic/tuple targets pass. Real CLI
checks ordered tuple elements and rejection at index 1; diagnostics compare
byte-for-byte with native. Existing unfiltered 477,970-assertion oracle unchanged,
zero former RIGHT losses/vanished keys; no corpus whole-case gain claimed.
Seven interleaved fresh-process samples: after/before **1.12613**,
after/native **1.43080**. Baseline skips correct normalization, so work equivalence
is unverified, but the observed 12.61% slower smoke does not certify the required
no-hotpath-regression gate. This is a correctness prerequisite, not a speed win.
Receipts `variadic-paren-*` under `target/recovery/alias`. Frozen verification
excludes the pending TypeQuery wrapper dependency.

## Parenthesized identity mapped body

The existing identity-mapped body projection now unwraps parentheses before
mapped construction, following native `getTypeFromTypeNodeWorker`. Primitive
arguments reach the existing `instantiateMappedType` primitive-preservation arm.
Existing homomorphic eligibility, mapper, member ownership and publication stay
unchanged; no new cache or general mapped-body fallback. A separate canonical
resolver-only mapped-chain experiment did not convert its target and was removed.

Primitive consumer regression fails before and passes after; existing mapped/
variadic targets pass. Real CLI diagnostics compare byte-for-byte with native.
Existing unfiltered 477,970-assertion oracle unchanged, zero former RIGHT losses/
vanished keys; no whole-case gain claimed. Seven interleaved fresh-process samples:
after/before 0.97240, after/native 1.54266. Baseline omits the correct semantic
answer and emits errors, so equivalent complete work/no-hotpath regression/release
speed are not certified. Receipts `mapped-paren-*` under `target/recovery/alias`;
verification uses the frozen pre-wrapper worktree.

## Distinct mapped argument consumer controls

A single Checker now tests mapped arguments `[string, number]`,
`[number, string]`, `[boolean]` and a warm repeat. Ordered tuple images retain
separate TypeIds; the repeat retains the original identity. Real CLI index
consumers and the rejection from the reversed argument compare byte-for-byte
with native (`mapped-distinct-*` receipts). No implementation/cache change or
source-alias spelling heuristic accompanies these controls. The parent-reserved
source-wrapper metadata block around alias-body reference construction is untouched.

## Identity mapped operands use binder identity

The existing identity-template projection now resolves its constraint/object/index
operands to actual binder SymbolIds and compares them with the alias and mapped
parameter owners. Parentheses are transparent as in native
`getHomomorphicTypeVariable`/`getTypeFromTypeNodeWorker`. The old spelling-only
helper is removed. This is the existing identity-template subset, not a general
homomorphic evaluator; no new cache or wrapper-body change.

Parenthesized-operand regression fails before and passes after; mapped/variadic
targets pass. Native CLI diagnostics compare byte-for-byte. Existing unfiltered
477,970-assertion oracle unchanged, zero former RIGHT losses/vanished keys.
Seven interleaved fresh-process samples: after/before 1.00250, after/native 1.62336;
no equivalent-complete-work/no-hotpath/release performance certification.
Receipts `mapped-identity-*` under `target/recovery/alias`. Parent-reserved wrapper
metadata blocks remain untouched.

## Parameter-bodied alias owner

`type_parameter_body_index` now resolves the body reference and matches the actual
binder parameter SymbolId in declaration order, instead of comparing names.
Parentheses remain transparent; mapper image publication/cache behavior unchanged.
Ordered second-parameter control and existing alias targets pass. Native CLI
rejection diagnostics compare byte-for-byte. Existing unfiltered 477,970-assertion
oracle unchanged, zero former RIGHT losses/vanished keys; no case gain claimed.
Seven fresh-process samples: after/before 0.99963, after/native 1.60427; complete
checked-work equivalence and release performance remain uncertified. Receipts
`parameter-owner-*`; parent wrapper blocks and TypeQuery entry untouched.

## Mapped constraint/template transparent resolution

The identity-template projection unwraps parentheses around the `keyof` constraint
and indexed-access template, matching native type-node resolution before
homomorphic mapper selection. Binder-owner checks and existing mapped worker/
publication are unchanged. Regression fails before and passes after; related
mapped/variadic targets pass. Native CLI diagnostic text/spans compare byte-for-byte.
Existing unfiltered 477,970-assertion oracle unchanged, zero former RIGHT losses/
vanished keys; no case gain claimed. Seven fresh-process samples: after/before
0.96994, after/native 1.56847; equivalent work/no-hotpath/release performance remain
uncertified. Receipts `mapped-template-*`; parent wrapper/TypeQuery entry untouched.

## Identity mapped object cache owner cutover

The object-image branch now uses the existing native-style
`instantiations[(SymbolId, ordered TypeIds)]` owner instead of the separate
qualified-reference key containing rendered argument text. Lookup precedes
formatting. Publication follows member-owner acquisition and modifier metadata;
active/unresolved source work still declines. No new cache or reuse domain.
Alias/receiver context remains the existing identity-template object image;
expensive member work attribution remains `tsr-1yb.11`.

Distinct object arguments and warm reuse controls pass; native CLI property
consumers/diagnostics compare byte-for-byte. Existing unfiltered 477,970-assertion
oracle unchanged, zero former RIGHT losses/vanished keys. Seven fresh-process
samples: after/before 0.99695, after/native 1.71889. Output agrees, but complete
checked-work equivalence and release performance remain uncertified; no speed
claim. Receipts `mapped-cache-owner-*`; parent wrapper/TypeQuery entry untouched.

## Unique type operator operand gate

Native `getTypeFromTypeOperatorNode` checks exact `SymbolKeyword` operand kind
before `getESSymbolLikeTypeForNode`. The owned arm now follows that order: invalid
operands return errorType without publishing a unique identity. Existing valid
position/identity publication unchanged; no name fallback or diagnostic suppression.

Regression fails before and passes after; type/unary targets pass. Real CLI valid/
invalid operand diagnostics compare byte-for-byte with native. Available unfiltered
oracle gains one WRONG→RIGHT (`uniqueSymbolsErrors` invalidUniqueType), zero former
RIGHT losses/vanished keys. Seven valid-operand fresh-process samples:
after/before 1.00203, after/native 1.63265; full checked-work/release performance
remain uncertified. Receipts `unique-operand-*`. Constrained homomorphic `any` was
also probed and already passes; only its behavior control is retained.

## Type literal call/construct grouping

Native `createTypeNodesFromResolvedType` emits CALL signatures, then CONSTRUCT,
then index infos, then properties. `build_type_literal` now keeps separate
construct buckets for rendered members and semantic signatures and appends them
after calls, preserving source overload order within each set. No signature-kind
field added to Member, no source-name/test exception, and no wrapper entry change.

Source rendering fails before and passes after; semantic vector control confirms
CALL,CALL,CONSTRUCT,CONSTRUCT for interleaved source. Signature-member/constructor
targets pass. Native CLI overload call/construct consumers and rejection diagnostics
compare byte-for-byte. Available unfiltered oracle: 3 WRONG→RIGHT, zero former
RIGHT losses/vanished keys (`typeName1` one, generic overloaded constructor arguments
two). Seven fresh-process samples after/before 1.01599, after/native 1.58315;
no-hotpath/release performance not certified. Receipts `literal-signature-order-*`.

Full-configuration >=99.9% parity, preservation against disappeared historical
RIGHT-key receipts, and verified equivalent-complete-work median <=0.50 remain
uncertified. Existing oracle skips cannot certify those gates.
