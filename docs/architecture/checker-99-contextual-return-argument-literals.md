# Contextual return inference reaches argument literals

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Unit:
tsr-djf call inference. Baseline: 8f8f4e1a, 458,022/478,855 matching
assertions.

## Native order

`inferTypeArguments` (checker.go:9390-9460) first infers from a call's
contextual return type twice: weak `ReturnType` candidates for final inference,
and an ordinary-priority `returnMapper` used only while arguments are checked.
It then checks each argument through `checkExpressionWithContextualType`
(checker.go:9485). That function pushes the same inference context while
checking the expression (checker.go:7486), so `instantiateContextualType` can
apply the return mapper to nested object, array, and function members before
their literals widen.

The port collected both candidate sets, but ordinary non-context-sensitive
arguments were checked while the active context's `inferential` flag was false.
Top-level primitive literals had a later regularization helper, while nested
literals had already widened. For example, a call returning
`Wrap<{ kind: "ctx" }>` inferred under that written context still checked
`{ kind: "ctx" }` as `{ kind: string }`.

## Port boundary

`check_generic_call_worker` now exposes the already-active context while
checking object, array, arrow, and function expressions when—and only when—a
real contextual return map exists. Calls and other expressions retain the
existing nested-call inference order. This boundary is observable: enabling
the context for every ordinary argument gained the same target family but lost
seven RIGHT rows, including three generic composition rows where a nested call
prematurely fixed its outer parameter to `unknown`. That broader draft was
rejected.

Pinned controls cover object and tuple arguments plus a callback returning an
object. Each has a no-written-context twin that must continue widening; this
prevents contextual literal retention from leaking into ordinary calls.

## Deferred `Awaited<T>` prerequisite

`Promise.resolve(true)` under a written `Promise<true>` context remains
`Promise<boolean>`. The contextual type is found, but the signature returns
`Promise<Awaited<T>>`, and the port collects no candidate through the deferred
conditional target. Native reaches `inferToConditionalType`
(inference.go:252,554). That mechanism belongs to the conditional-inference
unit; this call-inference change deliberately adds no `Awaited` special case.

## Measurement

Full scorepair against 8f8f4e1a gains 47 assertions, all WRONG-to-RIGHT:
21 in `inferFromGenericFunctionReturnTypes3`, 9 in `arrayLiteralInference`, 8
in `contextualParamTypeVsNestedReturnTypeInference1`, and 9 across smaller
cases. There are zero RIGHT losses and zero GAP transitions. The earlier claim
of zero changed already-WRONG payloads was unsupported: scorepair compares
verdict statuses, not payloads. The combined eight-unit build has eight changed
already-WRONG rows in `arrayLiteralInference` and `callChain.3`; retained worker
artifacts do not establish whether these are unit-local or interactions.
