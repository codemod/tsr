# Checker compatibility checkpoint: 2026-09-30

The active target is 95% correct assertion coverage against typescript-go
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`. This checkpoint reaches
447,414/478,855 assertions (93.43%) and 6,529/9,538 complete cases. The target
requires 454,913 correct assertions, leaving 7,499. The denominator and upstream
corpus are unchanged.

The initial working tree measured 443,835 correct assertions; it already contained
the user's tuple reduction and parameter printing work. Compared with that tree,
this checkpoint has 3,103 wrong-to-right and 476 gap-to-right transitions, with no
losses of previously correct assertions. It also has 284 gap-to-wrong and 16
wrong-to-gap transitions. Correctness coverage therefore improves by 3,579;
the new wrong answers remain part of the reported deficit. The committed parent
`ef028f95` previously recorded 443,743; the intervening 92 belong to the initial
working tree, rather than this continuation's measured attribution.

## Mechanisms and constraints

Inference candidates carry tsgo's priority and covariance/contravariance state.
Contextual return inference uses a separate mapper, and callback inputs fix only
the parameters they consume. Candidate checking can evict contextual expression
caches; an unsupported overload walk restores the previous entries. This
prevents one candidate's callback context from becoming another's answer.
The implementation retains declaration identities for type parameters and
substitutes through captured reference, indexed-access, anonymous-property and
tuple metadata instead of editing their printed names.

Signature metadata distinguishes calls from constructs, records instantiated
targets, and includes tuple-shaped rest arguments. Closed constraint filtering
is supported; recursive generic signature relations and general union matching
are incomplete (`tsr-6.1`). Reference variance is measured using related and
unrelated marker instances as in `getVariancesWorker`. Unmeasurable targets
retain the previous fallback; this is not a complete port of upstream's
Unmeasurable/Unreliable flags.

Resolved tuple elements carry required/optional, spread and label information.
`TupleNormalizer.normalize` expands substituted tuple operands, distributes
variadic unions and reduces concrete unbounded rests. Written annotations retain
their own spelling, which permits the semantic reduction previously refused in
STATUS.md §958 without changing the annotation's display.

The identity-template subset of `instantiateMappedType` recognizes
`{ [K in keyof T]: T[K] }` with optional and readonly modifiers. It maps tuple
elements before object member resolution, preserving labels and tuple flags.
Variadic operands retain a deferred mapped reference until substitution;
normalized array/tuple results do not acquire an enclosing alias name.
Transformed templates and key remapping still require the general mapped
template machinery. Reusing source members is valid only for the identity
template, so the factory declines other templates.

Strict optionality was checked against declaration output from a binary built
from the pinned oracle. Exact optional properties omit missing from fixed tuple
slots while optional rest/array elements still include undefined. Removing
optionality removes undefined, preserves null, and preserves explicit undefined
in a required tuple slot. Tuple relations check readonly state, bounds, optional
slots and concrete rests. Generic source rests use resolved array/tuple base
constraints; unresolved target variadics remain unknown.

`checkExpressionEx` resets the instantiation count for an independent expression
without resetting depth. Tuple-to-array relations exposed a previous exhausted
count that poisoned later property reads in
`recursiveTypeAliasWithSpreadConditionalReturnNotCircular`. Direct property
reads now reuse the checked access type, and uncached reads reset count. Tests
exercise both the count reset and the retained depth limit.

These choices would be falsified by a differing pinned-oracle control or by
losses in the full assertion transition matrix. Corpus totals alone cannot
validate alias spelling, optionality, or candidate cache restoration; directional
and counterexample controls cover those seams separately.

## Validation and review

Release workspace tests and workspace clippy with warnings denied pass. The
anchor check resolves all 3,383 references. The checker snapshot was regenerated
at 447,414 assertions; whitespace checks pass. Direct tsgo declaration controls
cover readonly, exact optional, concrete rest relations and mapped alias
normalization.

Code review: skipped (ce-code-review unavailable). The full review skill requires
independent subagents and finish leaves; the active repository instructions map
those tasks to sequential work in the main thread. The attempted review records
that constraint. A manual diff scan covers inference context restoration,
instantiation guards, tuple bound arithmetic, mapped modifiers, variance marker
recursion, option directives and the corresponding tests. It is not an
independent review. Simplification consolidated the mapped normalization admission
used by declared and computed aliases; broader rewrites were left out because
they would obscure comparison with upstream.

Evidence files are local scratch artifacts:

- `/tmp/tsr-95-baseline-verdict.tsv`
- `/tmp/tsr-95-normalized-mapped-instantiation-verdict.tsv`
- `/tmp/tsr-95-normalized-map-{tests,clippy,anchors,coverage}.log`
- `/tmp/tsr-95-checkpoint-workspace-{tests,clippy}.log`
- `/tmp/tsr-95-code-review-attempt.json`

The measured checker source plus `trace_case.rs` has SHA256
`eb28872c141265b27fe0a44589ed06450275d3fcb932010e8a466a0160453e0a`.
The goal and Beads parent `tsr-6` remain active.
