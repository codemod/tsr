# Implicit infer constraints and any checks

Measured at `IMPLICIT_INFER_COMMIT`, against `90bccd3b`: 447,754/478,855
correct assertions (93.51%); 6,544/9,538 complete cases (68.61%). Another 7,159 correct assertions are needed for 95%.
Pinned tsgo remains `5b1047d10d32e7d5b446be4de56b126ff42f82bb`; the corpus is unchanged.

## Mechanisms

`getInferredTypeParameterConstraint` walks merged infer declarations, skipping
parenthesized types. Rest parameters/tuple elements contribute unknown[], template
spans contribute string, and mapped key parameters contribute string | number |
symbol. Reference arguments inherit the corresponding parameter constraint,
instantiated through their effective argument map. An instantiated constraint
identical to the inferred parameter is discarded. Multiple constraints intersect.
This reference leg handles fully supplied argument lists; default argument fill,
import type constraints and the mapped-template special case remain incomplete.

Direct dependencies between inferred parameters use getInferredType's lazy order:
install the provisional candidate, resolve the referenced inferred parameter,
then apply constraint fallback. Resolving all parameters eagerly would evaluate
unrelated constraints in a different order. Composite dependent constraints still
defer. The same candidate/constraint mapper instantiates the extends target before
relation and the chosen true branch afterwards.

`getConditionalType` includes both branches for an any check whose instantiated
extends type is concrete; an any or unknown extends type selects only the true
branch. For infer locals, the true branch uses the inference mapper and the false
branch uses the outer mapper. Unknown candidates and rest constraints supply the
same defaults as tsgo. Error and unresolved/deferred extends targets decline.

Implicit constraints feed conditional inference directly. Applying them to the
port's general apparent-type/tuple-normalization road also requires preserving
constrained variadic parameter identities there, rather than replacing infer
parameters with their array constraints during target construction. That broader
integration remains incomplete and is not silently approximated here.

## Measurement and verification

Aligned verdicts: 474,243 total; 447,754 right; 4,671 gap; 21,818 wrong.
The transitions are 10 WRONG→RIGHT and 2 GAP→RIGHT, with zero RIGHT losses and
no new wrong answers. Implicit constraints contribute eight correct assertions;
any checks without infer locals contribute two; any checks with infer locals
contribute two. Direct dependent constraints pass their control but contribute
zero corpus conversions in this unit. variadicTuples1 contributes eight gains;
four other cases contribute one each.

All nine conditional controls pass, compared with pinned tsgo declaration output.
New controls cover reference constraints, merged self-constraint removal,
dependent candidate refinement, both any branches, any/unknown extends checks,
extends never, nested any branches, and any infer defaults for bare, array,
property and rest-function targets. Release workspace tests/clippy pass,
3,380 upstream anchors resolve, whitespace checks pass and snapshot is refreshed.

Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
Manual review checks merged declaration ownership, transparent parent traversal,
self-constraint removal, provisional cache order, branch mapper cleanup, error
provenance and deferred-target handling. This is not independent review.
The implementation reuses the structural collector, type instantiation and
union/intersection factories; the constraint resolver is confined to the
signature-less context it implements.

Scratch evidence:

- `/tmp/tsr-95-implicit-final-verdict.{tsv,log}`
- `/tmp/tsr-95-implicit-final-{tests,clippy,anchors,coverage}.log`
- `/tmp/tsr-95-oracle-{implicit-constraints,dependent-constraints,any-conditional,any-infer}*`

Checker sources plus `trace_case.rs` SHA256:
`6ca38a6f1375641ff08624942b78dee26c5fadc0133b823ab0c4a04153e4a41b`.
Composite non-fixing constraint mapping, reverse mapped inference and recursive
source capture remain in `tsr-6.3`. The goal remains active.
