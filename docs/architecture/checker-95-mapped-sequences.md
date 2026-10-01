# Homomorphic mapped sequences

Baseline c52451f1: 448,415/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

The mapped alias factory now follows instantiateMappedType, instantiateMappedArrayType,
instantiateMappedTupleType and instantiateMappedTypeTemplate (checker.go:22527–22666).
It preserves primitive inputs, distributes over unions, transforms array elements,
retains tuple labels and optional/readonly flags, and maps intersections of sequences.
Fixed tuple positions substitute string keys; variable operands recursively instantiate
the mapped alias, and trailing variable positions substitute the numeric key through
an array. Optionality applies to both tuple flags and element types. Any inputs
transform to arrays only when the original homomorphic parameter has an exclusively
array/tuple constraint, following hasArrayOrTypeTypeConstraint.

The homomorphic parameter is captured by binder symbol identity. Recapture changes
only that parameter's argument slot, preserving other parameters even when their
actual argument types happen to be identical. Distributed unions retain mapped alias
names; enclosing nongeneric aliases retain their own names. Signature annotations
reuse written mapped names while parameter semantics remain normalized sequences.

getResolvedApparentTypeOfMappedType (checker.go:21772) now applies an alias to the
sequence base constraint of its homomorphic input. Nested generic mapped modifiers
recursively obtain their apparent sequence. Member and index lookup use this result.
An existing identity-map path now recognizes generic mapped inputs and captures their
metadata, which is required for Readonly<Boxified<T>> indexed reads.

## Verification and limits

Checkpoint e4fde75a: 448,476/478,855 correct assertions (93.66%),
6,571/9,538 complete cases (68.89%). Another 6,437 assertions are needed for 95%.
Aligned verdicts: 474,243 total; 448,476 right; 4,482 gap; 21,285 wrong.
Against c52451f1: 57 WRONG→RIGHT, 4 GAP→RIGHT, zero RIGHT losses;
one GAP→WRONG remains in discriminantPropertyCheck: BarEnum.bar1 | BarEnum.bar2 |
undefined still fails to reduce to BarEnum | undefined.

Observed gains include mappedTypesArraysTuples 14, mappedTypesGenericTuples 8,
inferrenceInfiniteLoopWithSubtyping 6, mappedTypes2 6 and mappedTypes4 6.
Earlier drafts dropped five distributed Partial union names and three written
mapped signature names. Union origin preservation and signature annotation reuse
repaired all eight before acceptance.

Twelve pinned declaration controls cover arrays, optional and rest tuples, readonly
and mutable tuples, primitive inputs, normalized outer aliases, tuple inference,
constrained any arrays, sequence intersections, generic pop and a nested readonly
indexed read. Release workspace tests and clippy with warnings denied pass; upstream
anchors resolve; the checker snapshot and whitespace checks pass.

This ports alias instances whose mapped body has no as clause. Inline anonymous
mapped sequences, remapped keys, complete root mapper provenance, generic enclosing
union alias display, constrained reverse key inference, partially inferable source
flags, expanding reverse recursion and full member widening remain incomplete.
The captured homomorphic source currently requires a direct type parameter reference;
parenthesized source aliases remain outside that recognition. tsr-6.9 stays active.

Code review: skipped (ce-code-review unavailable). The sequential main-thread rule
conflicts with independent review requirements. Manual review checked mapper slot
identity, canonical cache publication, union alias preservation, fixed versus variable
keys, optional undefined stripping and apparent-type cycle sentinels. This is not
independent review.

Evidence: /tmp/tsr-95-mapped-sequence-final-verdict.{tsv,log},
/tmp/tsr-95-sequence-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-mapped-sequence-apparent-controls.log,
/tmp/tsr-95-oracle-mapped-sequence-apparent{.ts,-out/}.
Checker sources plus trace_case.rs SHA256:
053ea2299948686533f69a6d8a831aefe4915d4fa575acb2aaf48551be4bb687.
Goal remains active.
