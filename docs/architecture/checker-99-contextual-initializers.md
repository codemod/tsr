# Contextual parameter initializers

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code f557e254, evidence d37ba310: 455,873/478,855 RIGHT.

The broader contextual constraint experiment exposed a missing prerequisite in
assignContextualParameterTypes (internal/checker/checker.go). When a default
initializer does not fit the contextual parameter, native widens the initializer
and uses that widened type only if the context fits it. Thus (x = 3) under a
1 | 2 context has type number, while a string default keeps 1 | 2 and diagnoses.
A compatible default keeps the contextual type. The same rule widens a never
context to number, but preserves unknown.

The port applies this adjustment when serving a contextual parameter, after the
cached checked-signature path. It reuses initializer widening and the ternary
assignability relation; only definite failure followed by definite success can
change the parameter. An unsupported relation does not authorize a guessed type.
Rest parameters retain their separate native path.

Native getTypeOfParameter includes undefined for optional and defaulted positions.
Stored signature parameters omit this wrapper for printing, so comparison adds
it under strictNullChecks. Defaults before required parameters are not printed
optional, but still need the wrapper; the declaration lookup handles that case
and offsets an explicit this parameter. Omitting the wrapper incorrectly widened
an optional 1 | 2 parameter to number. The helper reuses the existing this-parameter
predicate rather than adding another syntax test.

Two tests pin eighteen outcomes under strict true and strict false. Native input:
/tmp/tsr-99-contextual-initializer.ts; outputs under the same prefix ending in
-native and -loose-native. Each native invocation explicitly selects strictness,
target esnext and declaration emit. Invalid defaults diagnose but still emit.
The control with a default before a required parameter and explicit this also
pins the contextual parameter read; native diagnostics identify the contextual
1 | 2 | undefined constraint. An optional tuple-rest control remains a known
flow limitation: the port retains undefined in the parameter/body and inferred
return where native narrows it away. It is not a new regression and is not claimed
fixed by this unit (tsr-8).

The broad recursive contextual mapper experiment is not included. Both the eager
and lazy variants gain 24 assertions but lose 32 RIGHT rows, add six WRONG rows
from gaps, and change four already-WRONG rows. Chained generic methods account for
26 losses: retained own parameters share identities with enclosing parameters
where native instantiateSignatureEx clones them. Initializer handling accounts
for two; recursive mapped contextual constraints account for four. Lazy resolution
is semantically necessary but does not change those verdicts. Revisit the mapper
only after those prerequisites; do not add fixture-specific exceptions.
Experiment source: /tmp/tsr-99-contextual-constraints-lazy.rs. Verdict/delta:
/tmp/tsr-99-contextual-constraints-lazy.tsv and -lazy-delta.txt.

This isolated initializer change gains two WRONG-to-RIGHT rows in
contextuallyTypedParametersWithInitializers2, with zero RIGHT losses, zero new
WRONG rows and no other changed rows. Verdict:
/tmp/tsr-99-contextual-initializer-final.tsv. No corpus, denominator or oracle is
changed. Committed verification and final quality results follow below.

Main verification passes: 211 release workspace result blocks, clippy, formatting,
diff checks and 3,330 anchors. Sequential primary-thread correctness,
adversarial, standards and testing review found no retained actionable findings.
No independent or cross-model review is claimed under the user's no-delegation
rule. The simplification pass reused the existing this-parameter predicate and
kept the widening/relation helpers; no additional state or framework was added.
Receipt: /tmp/compound-engineering-501/ce-code-review/contextual-initializers/review.json.
Frozen source hash: 78f63e82b79ec13fa9182547bb673e59e2a91b0ce7ad284a52770fc6c9806296.
