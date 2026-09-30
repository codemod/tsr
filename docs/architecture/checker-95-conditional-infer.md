# Conditional infer evaluation

This continuation uses typescript-go `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Against `f6405d4d`, correct assertions increase from 447,501 to 447,657/478,855
(93.48%). Complete cases increase from 6,532 to 6,542/9,538 (68.59%). The 95%
target requires another 7,256 correct assertions. The corpus is unchanged.

## Mechanisms

`bindTypeParameter` and `getInferTypeContainer` file an infer declaration in the
conditional whose extends subtree contains it. Separate conditionals retain
separate symbol identities. `resolveNameHelper` exposes those locals only in the
true branch; check, extends and false branches continue to the enclosing scope.
Invalid infer declarations outside a conditional get anonymous symbols, as in
upstream. The scope control contrasts an outer alias and two inferred parameters
with the same name, checking resolved identities rather than symbol counts.

`getTypeFromInferTypeNode` supplies the parameter declaration's semantic type.
Written function annotations retain their `infer` spelling independently of
that identity. `getInferTypeParameters` reads conditional-local type parameters.
`getConditionalType` infers the concrete check against the extends type with
NoConstraints and AlwaysStrict flags, instantiates the extends target with the
mapper, then relates that target before evaluating the selected branch under
inferred bindings. Mapper, priority and variance state restore after each walk.
A target that cannot be instantiated declines. A failed inference path never
falls back to branch selection against an unmapped infer target.

The signature-less `getTypeFromInference` preserves literal candidates: it takes
their covariant union or contravariant intersection without signature argument
widening. AlwaysStrict applies even when strictFunctionTypes is disabled.
Concrete rest tuple targets infer fixed prefix and suffix positions and the
remaining array element union. A zero-length rest slice contributes no element
candidate; the conditional context then supplies unknown, rather than never.
The existing single-variadic parameter capture remains in use for tuple slices.

## Bounds and judgments

This first port evaluates concrete non-union checks with unconstrained infer
parameters. Union distribution, any branch unions, nested conditional evaluation,
explicit and dependent infer constraints and inferred reference constraints need
additional machinery (`tsr-6.3`). An unresolved print-only template target cannot
supply an inference mapper. A retained alias that lacks normalized array metadata
cannot prove a false array match; that case defers instead. NoConstraints skips
structural apparent reads for the instantiable/intersection source flags modeled
here. General mapped inference and recursive signature/source capture remain
incomplete. These limits are not inferred gains from the remaining deficit.

The final transition matrix has 88 wrong-to-right and 68 gap-to-right changes,
zero losses of previously correct assertions, and eight gap-to-wrong changes.
Those eight are still counted in the deficit; they include implicit constraints,
reverse mapped inference and recursive source capture. They are recorded rather
than hidden by the aggregate improvement of 156 correct assertions.

## Validation and review

Pinned tsgo declaration controls cover readonly array elements, tuple heads and
tails, covariant object candidates, contravariant function candidates, strict
contravariance in nonstrict mode, and empty versus populated concrete rest slices.
The bundled-library controls and binder scope control pass. Release workspace
tests and clippy with warnings denied pass; all 3,380 upstream anchors resolve.
Checker snapshot refreshed. Binder symbols remain 8,497/8,497 (100%).

Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule still conflicts with the skill's independent review requirement.
A manual diff scan checks infer symbol ownership and visibility, balanced binding
frames, inference priority/variance restoration, candidate directions, literal
preservation, empty slices, relation deferral and written signature reuse. This
is not an independent review. Simplification shares the structural inference walk,
creates tuple slices only on the variadic branch, and uses existing alias frames
rather than a separate substitution implementation.

Scratch evidence:

- `/tmp/tsr-95-tuple-slice-final-verdict.tsv`
- `/tmp/tsr-95-conditional-infer-guard-verdict.tsv`
- `/tmp/tsr-95-conditional-infer-guard-{tests,clippy,anchors,coverage}.log`
- `/tmp/tsr-95-conditional-infer-binder-coverage.log`
- `/tmp/tsr-95-oracle-conditional-infer*`

Checker sources plus `trace_case.rs` SHA256:
`566eb93965f0d6a121b2cb617057de92c360a16cbd38a9095299bbfcac580240`.
The goal and parent `tsr-6` remain active.
