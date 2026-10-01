# Async callback contextual returns

Baseline 5fffc9af:449,141/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

getContextualTypeForReturnExpression now unwraps known Promise/PromiseLike
contexts without an Awaited alias and supplies S | PromiseLike<S> to async
return expressions (checker.go:29627). Generic variables retain their identity;
recursive promise/union paths use an active path guard. Written annotations
precede contextual-signature filtering. Signature results filter to known
promises or any/unknown/void/instantiable types (getContextualReturnType:29683).
Known async generator references supply yield/return/next slots. Structural
thenables and general generator protocols remain incomplete.

Async body inference now admits materialized contextual signatures, uses the
contextual undefined/void decision for empty bodies, and regularizes const body
sources before widening. Contextual return widening reads the promised type
(checker.go:20407); actual body operands still use the existing awaited query.
Callback source bookkeeping follows promised const parameters and generator
slots so existing mutable variables do not acquire a blanket readonly image.
Both promised-context and source-bookkeeping recursion guards unwind correctly.
Delegated array elements are awaited for async yield* (checker.go:11018).

S | PromiseLike<S> property contexts can use S when the property is absent from
the promise branch and its payload is exactly the remaining union. This checks
type identity and property membership; arbitrary union discrimination retains
its preceding conservative guard. Broadening that guard to count contributing
properties repaired four async object rows but lost eight previously correct
excess-property/discriminant/overload rows. The identity rule repairs the four
without those losses.

Exact source/target union constituent matching removes shared null/undefined
before signature inference (inference.go:101). This repairs contextual
Promise.reject inference through nullable then callbacks: the intended object
candidate previously disappeared, producing Promise<never>. The same-target
reference/alias close-match arm and general inferToMultipleTypes remain
incomplete; the earlier Promise-slot shortcut experiment is not restored.

## Verification and limits

Checkpoint 83c358c0:449,259/478,855 correct assertions (93.82%).
Complete cases:6,584/9,538 (69.03%). Another5,654 assertions are needed for95%.
Aligned verdicts:474,243 total;449,259 right;4,284 gap;20,700 wrong.
Against 5fffc9af:67 WRONG→RIGHT,51 GAP→RIGHT,zero RIGHT losses,
3 GAP→WRONG and3 WRONG→WRONG type changes. Denominator and oracle unchanged.

Twenty exported declaration controls match the pinned tsgo executable (exit0),
plus the contextual Promise.reject expression is checked against its pinned
corpus baseline. Controls cover concise/block async literals, deep tuple/object
returns, ordinary callbacks, mutable variables/constraints, fixed return slots,
async generator literals and variables, delegated promised array elements,
union promise contexts and nullable callback inference.

Three previous gaps now expose existing incomplete dependencies. The async
load().then callback in contextuallyTypeAsyncFunctionReturnTypeFromUnion returns
Promise<any> rather than Promise<boolean>; its target has multiple naked union
variables, still unanswered by the general union algorithm. Two
compiler/typeInferenceLiteralUnion rows have structurally equivalent tuple
unions but lose the Primitive alias in display. Exact constituent matching now
reaches that previously unanswered path; alias/close-match provenance remains
unported. tsr-6.1 tracks these union continuations. These are recorded wrong
outputs, not successful parity.

Release workspace tests and clippy with warnings denied pass;3,377 upstream
anchors resolve. The checker snapshot and whitespace checks are refreshed.
Async structural thenables, general iteration protocols, mapped/overloaded
callback contexts and composite/indirect const sources remain incomplete
(tsr-6.15 and related issues). This checkpoint does not complete the95% goal.

Code review: skipped (ce-code-review unavailable). Independent review dispatch
conflicts with the sequential main-thread instruction. Manual review checked
written versus contextual return precedence, generic identity, recursion guard
unwinding, source mutability, tri-state literal queries, exact union matching
and inference priority restoration. This is not independent review.

Evidence:/tmp/tsr-95-async-contexts-accepted-verdict.{tsv,log},
/tmp/tsr-95-async-contexts-accepted-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-async-contexts-controls.log,
/tmp/tsr-95-oracle-async-contexts{.ts,-out/}.
Rejected drafts:/tmp/tsr-95-async-context-{initial,filtered,source-union}-verdict.tsv.
Checker sources plus trace_case.rs SHA256:
2d9848d6ff1afd0ddafdcb804b59e0360e246337e30728b2dcc8b610e4cb48c1.
