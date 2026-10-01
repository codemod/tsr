# Contextual constructor inference and propagated return signatures

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 42e7881a: 452,265/478,855 correct assertions (94.45%).

## Native rules and implementation

instantiateTypeWithSingleGenericCallSignature (checker.go:7599) accepts one
call or construct signature with type parameters. The source may have other
members, but the non-nullable contextual target must have exactly one signature
of the same kind, no type parameters, no properties and no index signatures.
getSingleSignature (checker.go:19354) also excludes a type with both signature
kinds. The port now resolves both kinds through signatures_of_type_kind instead
of reading only cached signatures or falling back to call signatures.

The single-signature helper shares the existing complete property-name walk
with structural comparison. That walk moved unchanged from the relater into
the checker; an unresolved member set still declines rather than being treated
as empty. It retains inherited names, class statics and computed names. Index
requirements use get_index_infos_of_type. Source members remain allowed, and
nullable contextual targets pass through get_non_nullable_type.

Higher-order inference uses the same selection for its outer return signature.
The existing inference algorithm still controls candidate overlap, unique type
parameter identities, contravariant parameter inference and covariant return
inference. A named return alias with a single signature now receives propagated
type parameters in a fresh anonymous signature type. This follows
getOrCreateTypeFromSignature (checker.go:19370), which creates an isolated
signature rather than preserving the alias name. No rendered text is parsed to
determine the signature or its type parameters.

## Experiments, controls and review

The first full run added 26 correct assertions with zero RIGHT losses. Two
remaining GenericComp2 answers changed from CompClass<GenericProps<unknown>>
to CompClass<GenericProps<T>>, exposing the missing return propagation through
a named alias. Creating the isolated return signature fixed those two answers;
the final gain is 28 WRONG->RIGHT, with no other transitions.

Thirteen declaration outcomes are checked against the pinned native executable
with explicit strict mode and ES2015 target. The five Banana factory variants
exercise any and literal rest inputs. Bag covers a generic class constructor
with static members, a returned generic function and its string instantiation.
GenericComp2 covers a generic inherited constructor through a constructor alias.
The remaining controls allow source properties and indexes, strip nullable
targets, reject target properties or indexes, and reject mixed call/construct
signatures. The native control emits declarations without diagnostics.

The Rust regression test is contextual_construct_inference.rs. The native
source and declarations are /tmp/tsr-95-contextual-construct.ts and
/tmp/tsr-95-contextual-construct/tsr-95-contextual-construct.d.ts. Native, probe,
red, first-green and alias-green logs retain the progression under that prefix.

Simplification and review run sequentially in the main thread under the user's
tool mapping. No independent agent or cross-model coverage is claimed. Review
checks signature-kind exclusion, member completeness, generic identity, alias
isolation, shared-helper relocation and both positive and negative controls.

## Checkpoint and remaining work

CODE_CHECKPOINT: 452,293/478,855 correct assertions (94.45%).
The aligned comparison has 474,243 assertions: 452,293 RIGHT, 3,402 GAP and
18,548 WRONG. Relative to 42e7881a, all 28 transitions are WRONG->RIGHT:
contextualSignatureInstantiation4 (10), genericFunctionInference1 (9),
asyncYieldStarContextualType (5), and genericCallWithFunctionTypedArguments2 (4).
There are zero RIGHT losses, zero new wrong answers and zero changed wrong
answers. The 95% threshold is 454,913; 2,620 matches remain.

Source-frozen evidence:
- /tmp/tsr-95-contextual-construct-final-verdict.tsv
- /tmp/tsr-95-contextual-construct-final-verdict.log
- /tmp/tsr-95-contextual-construct-final-transitions.txt
- Checker sources plus trace_case SHA-256: c748a863f88ba2676e6387aba18be07d1077b648ea7f5bb7d6fb18a0e83405c2

6,696/9,538 cases now pass completely (70.20%), up by two. Release workspace
tests, clippy with warnings denied, all 3,362 upstream anchors, format and
whitespace checks pass. checker_types refreshes the committed snapshot. Clippy
requested a let-else syntax rewrite; after that non-semantic change, the full
verification and corpus comparison were rerun against the final source hash.

depend walks 4,392 gap lines: C1 still reports 575 roots that no longer gap,
C2 reports 285 cycles and zero depth-cap hits, and C3 balances. C4 still cites
stale 127,736-gap history. tsr-6.29 owns instrument repair; the full aligned
comparison and checker_types snapshot supply the authoritative current counts.

Broader CheckMode propagation, generic mapped/conditional inference and
method-preserving serialization remain in tsr-6.30. The helper conservatively
declines types whose full property set cannot be enumerated, including synthetic
Named types without a member owner. This unit does not broaden those resolved
member representations. Implicit generic heritage instance-member substitution,
JS constructor defaults and broader relation work remain in tsr-6.28.
