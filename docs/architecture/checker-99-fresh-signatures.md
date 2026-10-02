# Fresh generic signature identities

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code cd658415, evidence bd73c88e: 455,889/478,855 RIGHT.

Instantiating a generic class or interface must give each retained method type
parameter a fresh identity. Otherwise two generations of the same declaration
collide during inference. The port now follows cloneTypeParameter and
instantiateSignatureEx in internal/checker/checker.go: map the signature's own
parameters first, compose the receiver's mapper afterward, and retain the target
and mapper for lazy constraint resolution. Explicit type arguments, contextual
erasure and print-only substitutions keep using the ordinary instantiator.
Type parameter names cannot decide shadowing; distinct identities do that.

Fresh identities expose several dependencies previously hidden by collisions:

- Expanding recursive types no longer repeat the same type pair. The relation
  and inference walks now share isDeeplyNestedType's increasing-ID count, using
  the native thresholds of three and two respectively. Relation recursion flags
  record which side advanced. Once both sides expand, relation accepts the
  coinductive assumption and inference stops collecting candidates, matching
  recursiveTypeRelatedTo and invokeOnce in relater.go and inference.go.
- signaturesRelatedTo compares same-origin instantiated signatures through
  erasure. Re-inferring their fresh parameters instead changes Op's variance and
  loses eight nonInferrableTypePropagation1 assertions. Canonicalizing generic
  targets through their original unconstrained parameters also preserves
  recursiveConditionalTypes.P0 (getCanonicalSignature/createCanonicalSignature).
- Erasing a generic property key to any needs getPropertyTypeForIndexType's
  computed-any fallback after property and index-signature lookup. Unresolved
  alias placeholders also carry ANY in this port, so they must be excluded.
  Treating those placeholders as computed any loses five ramdaToolsNoInfinite2
  assertions. This boundary follows computed state, not fixture names.
- getContextualTypeForYieldOperand uses silentNeverType for an absent delegated
  return context. Ordinary never is a real inference candidate. Native inference
  instrumentation showed the contextual generator union marked non-inferrable
  through silentNever; using ordinary never instead loses two async-yield
  assertions. The existing sentinel allocation is now shared by both callers.
- A deferred mapped conditional must retain its root and mapper even when a
  branch cannot yet be computed. After substituting a tuple key, the root can
  resolve that branch. Retaining only already-computable branches had converted
  two gaps in recursiveTypeAliasWithSpreadConditionalReturnNotCircular to wrong
  answers. Capturing the root independently turns both into exact matches.

## Bounds of this port

The new recursion identities cover generic parameter symbols, generated
class/interface references, conditional roots and indexed-access object roots.
Anonymous, tuple and homomorphic mapped origins without a faithful native origin
representation keep unique IDs. Written references are excluded from symbol
recursion identity; the port's reference interner does not yet distinguish all
written/generated origins. The existing raw relation depth cap of 100 and outer
inference depth cap of 16 remain safety bounds. Native Maybe is represented by
the port's existing coinductive Related assumption; full relation-state caching
is still incomplete. These are limitations, not claims of a complete relater.

The broader recursive contextual mapper remains deferred (tsr-8). The earlier
fresh-signature refusal in checker-99-generic-assignability.md is superseded only
for the mechanisms above; it remains evidence for that earlier tree. Native
printing of colliding generic parameter names also remains incomplete.

## Evidence and falsifiers

The full candidate /tmp/tsr-99-fresh-roots.tsv has 474,243 aligned rows:
455,928 RIGHT, 2,768 GAP and 15,547 WRONG. Against the baseline it gains 39 exact
matches (32 WRONG-to-RIGHT, seven GAP-to-RIGHT), with zero RIGHT losses and zero
GAP-to-WRONG. Six already-WRONG rows change in arrayFlatMap and
flatArrayNoExcessiveStackDepth: ReadonlyArray becomes readonly-array syntax, but
optional thisArg still omits undefined. They remain incorrect and are not gains.

Eight regression tests use the pinned upstream fixtures and exact expected type
strings. Runtime fixture loading follows the repository's missing-submodule skip
policy; a missing individual fixture in an otherwise present corpus is a failure.
Direct native declaration controls are /tmp/tsr-99-fresh-signatures.ts and
/tmp/tsr-99-fresh-signatures-native/, run with explicit --strict true. They cover
own-parameter shadowing, receiver constraints, defaults and const parameters;
the deliberately invalid string argument diagnoses while declaration inference
retains native results. Native yield inference traces are
/tmp/tsr-99-fresh-native-async3.log, produced with a Go overlay without modifying
the vendor checkout.

The main release workspace, clippy, formatting and upstream-anchor checks are
run before committing. A detached checkout of the resulting commit will repeat
the full verdict, coverage and quality gates. Isolated mutations will remove
each dependency and require its corresponding regression assertion to fail.
Those checks falsify this implementation if identity isolation, constraint
mapping, absence-of-inference semantics or deferred branch resolution diverges.

Simplification reuses one recursion-identity helper and one silentNever allocator.
Sequential primary-thread correctness, adversarial, maintainability, standards
and testing passes cover the diff; no independent or cross-model review is
claimed under the user's no-delegation rule. Review found and fixed stale depth
documentation and compile-time fixture inclusion. Verification results and
remaining measured limitations are recorded below after committed validation.

Committed verification at 7ef88ba8 in /tmp/tsr-99-fresh-verify matches the full
candidate verdict byte-for-byte and the frozen source hash
299d6ddb83bfb69ec61a499edb6f9564efcbdcd214f6b7af5b25171c19c1a6e9.
Coverage is 455,928/478,855 RIGHT (95.21%) and 6,917/9,538 complete cases (72.52%),
leaving 18,139 matches to 99%. All 213 release workspace result blocks, clippy
and 3,329 anchors pass in both checkouts. One old anchor disappeared when the
misleading depth-cap comment was corrected. Fresh depend has 503 non-gapping
roots, 210 cycles, zero depth caps and 3,583 walked gaps; C3 balances and C1/C4
remain stale (tsr-6.29). The snapshot is copied from this checkout. Verdict and
logs use /tmp/tsr-99-fresh-verified; the primary-thread review receipt is
/tmp/compound-engineering-501/ce-code-review/fresh-signatures/review.json.

Seven isolated mutations compile and fail their intended exact assertions:
fresh allocation (promiseChaining.z), same-origin erasure (result1), canonical
targets (P0), any-index recovery (T1), unresolved-placeholder exclusion (DropForth),
silentNever (g's unknown return inference), and conditional-root capture
(zipped1). The async mapper assertion alone did not fail when silentNever was
removed, so it is not claimed as a falsifier for that dependency; the dedicated
missing-return-context test does fail. Each mutation restores the file in a
finally block, and the final restored hash equals the committed hash. Scripts
and aggregate logs are /tmp/tsr-99-fresh-mutations.py and
/tmp/tsr-99-fresh-mutations-tail.py (with .log siblings); per-mutation logs use
/tmp/tsr-99-fresh-mutation-{name}.log. The recursion guard's termination is also
exercised by the full corpus, including mapGroupBy; no destructive unbounded
recursion mutation is claimed.

After restoring every mutation, all eight fresh-signature tests and formatting
pass (/tmp/tsr-99-fresh-restored-tests.log). Recompiling that test without the
vendor link succeeds and all eight cases explicitly report the missing-submodule
skip; /tmp/tsr-99-fresh-no-submodule.log records it. The vendor link is restored
before the final source-hash check, then removed safely during checkout cleanup.
