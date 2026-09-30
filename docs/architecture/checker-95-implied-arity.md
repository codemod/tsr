# Adjacent variadic tuple inference

Baseline a88f8a1e:448,122/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferTypeArguments records impliedArity for a type-parameter non-array rest
when its supplied arguments have no spread. inferFromObjectTypes uses that
arity to split a source tuple against adjacent variadic target parameters.
The inference record must retain this metadata across candidate collection,
argument buckets and merges. This is a tuple inference rule shared by calls
and receivers, including partial binding.

Acceptance requires pinned declaration controls, a fixed-denominator corpus
comparison with zero previously RIGHT losses, and release workspace gates.
Goal remains active below 95%.

## Accepted measurement and limits

Checkpoint `c438a8aa`:448,134/478,855 correct assertions (93.58%),
6,558/9,538 complete cases (68.76%). Another 6,779 assertions are needed for
95%. Aligned verdicts:474,243 total;448,134 right;4,628 gap;21,481 wrong.
Against a88f8a1e:12 WRONG→RIGHT,all in variadicTuples1;zero RIGHT losses and
no new wrong answers. The denominator and pinned upstream are unchanged.

Pinned declarations pass binding zero,one,two or all parameters through a
signature with adjacent variadics,including labels and an optional trailing
parameter. The standard-library work.bind control still declines during
signature selection/inference; this checkpoint does not fix that control or
claim full bind support. Variable source tuples,adjacent variadic/array-rest
constraints and optional target suffix speculation remain in tsr-6.7.
The source is still required to be fixed and both adjacent target parameters
must belong to the inference context. Spread calls retain the existing decline.

Release workspace tests and clippy pass;3,380 upstream anchors resolve;
checker snapshot refreshed;whitespace checks pass. Candidate priority resets
preserve arity,cloning and merging retain it,and argument buckets share only
arity metadata so positional candidate order is unchanged. Slice arithmetic
models upstream's signed end-skip clamp through saturating subtraction.
Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
Manual review checked metadata lifetime,zero supplied rest arguments,candidate
priority and fixing,mutable slice labels/optionality and source bounds. This
is not independent review.

Evidence:/tmp/tsr-95-implied-arity-verdict.{tsv,log},
/tmp/tsr-95-implied-arity-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-implied-arity*.
Checker sources plus trace_case.rs SHA256:
`4d4669f91d5d0bf667d5d8b50b4668a3d59e2e15036b904399a01acb6ce39569`.
Goal remains active.
