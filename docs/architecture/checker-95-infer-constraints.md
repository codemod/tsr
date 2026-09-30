# Infer constraints and conditional continuation

Measured source checkpoint `INFER_CONSTRAINTS_COMMIT`, against `ac8d33ed`:
447,742/478,855 correct assertions (93.50%), an increase of 85. The 95% target
requires another 7,171 correct assertions. Pinned upstream remains
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`; the corpus and denominator are unchanged.

## Mechanisms

`getConstraintDeclaration` searches all merged type parameter declarations for an
explicit constraint. Conditional inference distinguishes no candidate from an
actual unknown candidate. `getInferredType` falls back to the constraint when no
candidate exists or a candidate is incompatible, before relating the instantiated
extends target. An explicit any constraint becomes unknown. The branch retains
compatible literal candidates. Outer alias bindings can supply a concrete
constraint; dependent infer constraints still defer.

Nested conditionals continue evaluation under the outer alias/infer mapper.
`getConditionalTypeInstantiation` distributes a naked parameter's substituted
union, evaluating each constituent with a prepended binding and restoring that
binding before advancing. A substituted never remains never. Tuple-wrapped
checks stay nondistributive. `mapTypeWithAlias` retains the enclosing alias name
on a reduced multi-member result; a single result collapses before naming it.
The enclosing alias belongs to the distributed result, not to each branch.

## Measurement and limits

Aligned verdicts: 474,243 total; 447,742 right; 4,673 gap; 21,828 wrong.
The transitions are 50 WRONG→RIGHT, 35 GAP→RIGHT and 10 GAP→WRONG, with zero
losses of previously correct assertions. Largest gains are inferTypesWithExtends1
(35), conditionalTypes1 (18), awaitedType (10), variadicTuples1 (8) and
recursiveConditionalTypes (5). The ten new wrong answers all occur in
inferTypesWithFixedTupleExtendsAtVariadicPosition: concrete constraints let the
conditional proceed, but generic constrained tuple inference still creates any
or unknown slots instead of retaining the source's fixed tuple positions.
They remain counted in the deficit.

This unit handles explicit closed constraints and concrete union distribution.
Implicit reference constraints, non-fixing dependent infer constraint mappers,
any branch unions, general reverse mapped inference and recursive source capture
remain incomplete in `tsr-6.3`. The preexisting array alias metadata guard and
unresolved-target deferral remain in force. Counts are measured conversion, not
claims that conditional evaluation is complete.

## Validation and review

Pinned tsgo declaration controls cover compatible/incompatible constrained
candidates, a constraint on a later merged declaration, an outer alias constraint,
nested false branches, distributed array inference, tuple-wrapped checks,
never distribution, alias retention and single-result collapse. A second filter
of the inferred named union verifies that its constituents survive alias naming.
All five conditional controls pass. Release workspace tests and clippy with
warnings denied pass; all 3,380 upstream anchors resolve; checker snapshot is
refreshed; whitespace checks pass.

Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
The manual diff scan checks balanced mapper frames and depth restoration on
refusal, constraint lookup across merged declarations, absent candidate handling,
variance reuse, alias placement and union reduction. This is not independent
review. The implementation shares the existing conditional worker, structural
inference collector, relation engine and union factory.

Scratch evidence:

- `/tmp/tsr-95-distribution-alias-verdict.{tsv,log}`
- `/tmp/tsr-95-infer-constraints-{tests,clippy,anchors,coverage}.log`
- `/tmp/tsr-95-oracle-infer-constraints*`
- `/tmp/tsr-95-oracle-conditional-distribution*`

Checker sources plus `trace_case.rs` SHA256:
`50d919dd716b88841f9c48de57a516edbe52600788e0f339d85890a89db29cf9`.
The goal and parent `tsr-6` remain active.
