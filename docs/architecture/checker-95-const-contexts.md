# Semantic const type-variable contexts

Baseline a8933f8a:448,856/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

isConstTypeVariable (checker.go:13645) follows semantic parameter identities,
union/intersection constituents, indexed object operands, deferred conditional
constraints, homomorphic mapped operands and variadic tuple elements. Indexed
and conditional walks use the upstream depth limit of five; a path stack breaks
cycles without suppressing sibling walks. Conditional aliases capture branches
lazily under their reference mapper. A constrained distributive check first
re-instantiates at its constraint; a never result falls back to the default
branch constraint. Default constraints elide any branches. Constraint captures
remain separate from the original branches used by mapped-template inference.

Literal const queries now inspect the argument's uninstantiated contextual target.
The callee's const-parameter presence is only a preflight guard; it no longer
makes unrelated parameter slots const. Nested literal carriers propagate a const
context. Primitive initializers participate alongside arrays and objects. Generic
rest parameters provide indexed accesses at argument positions, retaining their
const parameter identity instead of replacing it with a constraint element.

getIndexedMappedTypeSubstitutedTypeOfContextualType (checker.go:30607) validates
property keys against mapped key base constraints and substitutes the template.
The parameter, union/intersection and ordinary index base-constraint arms follow
computeBaseConstraint. Original mapped intersection operands survive normalization.
This supplies indexed const contexts even when the outer mapped target is not
homomorphic. The lookup also serves ordinary contextual mapped properties.
Numeric key origins, conditional key exclusion and remapped names remain separate
work; name remapping is declined by the existing metadata capture.

## Verification and limits

Checkpoint CONST_CONTEXT_CHECKPOINT:448,922/478,855 correct assertions (93.75%).
Complete cases:6,578/9,538 (68.97%).
Another5,991 correct assertions are needed for95%.
Aligned verdicts:474,243 total;448,922 right;4,405 gap;20,916 wrong.
Against a8933f8a:54 WRONG→RIGHT,12 GAP→RIGHT,zero RIGHT losses and no other
verdict transitions. The fixed denominator and pinned oracle are unchanged.

Early drafts lost const literals through conditional, indexed-rest and mapped
contexts. Those paths were repaired. Eager conditional capture also changed
unrelated mapper state, so captures are lazy. Non-const contextual queries and
callback boundary traversal re-entered signature resolution; the preflight guard
and the existing callback decline remain. An IIFE spread regression caught by the
workspace tests is repaired by recognizing its own type-parameter declarations
before computing the callee. That repair contributes three of the final gains.

Ten control assertions cover nested tuples, mixed const/non-const slots, wrapped
const targets, homomorphic maps, unions, unrelated array arguments and conditional
and indexed-rest own-node literals. Pinned tsgo declaration compilation exits0;
conditional/rest own-node spellings also match typeParameterConstModifiers.types.
Conditional-target call inference remains incomplete: tests assert its argument
views without claiming its call result is implemented. Release workspace tests,
clippy with warnings denied and3,379 upstream anchors pass. The checker snapshot
and whitespace checks are refreshed.

Readonly inference is still applied too broadly and too late. The expanded pinned
controls expose deep nested object candidates, mutable array constraints,
existing mutable variables/tuples, primitive variadic candidates and callback
returns. These are tracked in tsr-6.15. Callback boundaries, constructor/no-call
contexts, substitution types and complete conditional/base-constraint semantics
remain incomplete. Own-node array/object views retain the preceding mutable
representation while the inference source-view port is pending.

Code review: skipped (ce-code-review unavailable). Independent review dispatch
conflicts with the sequential main-thread instruction. Manual review checked
parameter identity, depth/cycle behavior, context guard cleanup, conditional mapper
lifetimes, mapped key validation, substitution, own-node view preservation and
negative mixed-slot/IIFE controls. This is not independent review.

Evidence:/tmp/tsr-95-const-context-iife-verdict.{tsv,log},
/tmp/tsr-95-const-context-verified-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-const-context-supported{.ts,-out/},
/tmp/tsr-95-oracle-const-context{.ts,-out/} (expanded pending controls).
Checker sources plus trace_case.rs SHA256:
6cc3cdba0dcbb3d0e9740153927ae5022318f815b4d2f5ffeac9f06da4a0d388.
The95% goal is not complete.
