# Callable export properties and generic substitution

Baseline 5d47e663: 450,417/478,855 matching assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

## Why the whole-function refusal could be removed

The preceding assignment-declaration port supplies types for binder value exports,
but getTypeOfFuncClassEnumModuleWorker still refused a function carrying any such
property. Upstream resolveAnonymousTypeMembers (checker.go) combines call
signatures with value exports. createTypeNodeFromObjectType (nodebuilderimpl.go)
prints a bare function only when no properties remain. The port now stores both
signatures and AnonymousProperty records on the same type, renders signatures
before properties, and retains site-specific type qualification. Type-only
namespace exports contribute no properties. Unsupported instance members and
unresolved property types still decline the callable.

The binder export table is unordered. Sorting by each symbol's first declaration
identity restores the source registration walk, rather than alphabetical order or
hash order. This is sufficient for currently bound exports; late-bound names
remain incomplete. Numeric property spelling follows classifyPropertyName's
string-named distinction, and synthesized quoted names use escapeNonAsciiString's
UTF-16 surrogate spelling. A shared quoting helper retains the existing control
character escape rules.

A print-only implementation would silently discard properties when a returned
callable was instantiated. instantiateSignatureType now substitutes property
TypeIds as well as signatures and records instantiated property metadata so lookup
does not return the original symbol's type. mentionsTypeParameter follows both
structures. Crucially, a negative structural result must remain terminal: falling
through to textual name matching lost 22 formerly correct assertions in generic
contextual typing. Those losses disappeared when structural traversal remained
authoritative. Controls cover a type parameter mentioned only by a property and a
callable with its own independent generic signature.

getContextualTypeForAssignmentExpression (checker.go) also prevents a function's
expando initializer from recursively resolving that same callable. An annotated
variable may supply context; an unannotated function expando does not. TypeScript
classes preserve ordinary prototype context, including merged class/function
symbols: treating those as expandos lost 16 targetTypeTest1 assertions in an
earlier draft. Dynamic annotated element keys and JS assignment contextual typing
remain outside this slice.

## Measured result and remaining limits

Checkpoint CALLABLE_CHECKPOINT: **450,628/478,855 (94.11%)**, with
**6,627/9,538 complete cases (69.48%)**. The 95% target needs 4,285 more matches.
Aligned verdicts: 474,243 total; 450,628 RIGHT; 3,914 GAP; 19,701 WRONG.
Against the rebuilt 5d47e663 baseline: 105 WRONG→RIGHT, 106 GAP→RIGHT,
zero RIGHT losses, 24 GAP→WRONG and 12 changed wrong types. Denominator and
oracle are unchanged. Numeric/Unicode naming adds 10 matches over the initial
201-match callable checkpoint without losing a correct assertion.

The new GAP→WRONG rows expose existing prerequisites: 18 incomplete late-bound
function objects in declarationEmitLateBoundAssignments/JSAssignments, five JS
JSDoc/nullable-return rows, and one template-literal keyof row. Changed wrong rows
include eight merged namespace/function property-precedence results. These are
recorded rather than treated as correctness gains; tsr-6.27 remains open for
late-bound names, constructor instance members and recursive boundaries. Fixing
those paths should improve both property queries and full callable shapes.

Pinned declaration controls pass with explicit strict=true and target=es2020,
including call/member results, outer type substitution, property-only references,
independent inner generics, numeric versus string names and Unicode escaping.
Two older tests that asserted the deliberate expando gap now assert complete
single-signature and overloaded callable objects. Release workspace tests,
clippy with warnings denied, 3,366 upstream anchors and whitespace checks pass.
The checker snapshot is refreshed. Binder retains the preceding measured 100%.

Code review: manual sequential review; no independent agent review. Checked
value-only export filtering, stable order, recursion guards, property lookup
after substitution, signature parameter identity, class prototype context and
numeric name spelling. The prior independent review workflow was unavailable
under the sequential execution instruction.

Evidence: /tmp/tsr-95-callable-expandos-names-verdict.{tsv,log},
/tmp/tsr-95-callable-transitions.txt, /tmp/tsr-95-callable-tests.log,
/tmp/tsr-95-callable-{workspace-tests-pass,clippy-pass,anchors,coverage}.log,
/tmp/tsr-95-oracle-callable-expandos.ts and its emitted declarations.
Checker sources plus trace_case.rs SHA256:
061e9a24d3c42a584f9076e36c95a0579fef75cda23221c4d67c957a9ea1bedd.
