# Source constraints and callable reference substitution

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline e5ee6cc5: 451,812/478,855 correct assertions (94.35%).

## Native rules and implementation

The source type-variable branch in relater.go:3664 follows constraints for
assignability as well as subtype relations. The port had restricted it to
subtyping. Ordinary constrained parameters now use that branch for all supported
relations, preserving the existing cycle guard. Indexed sources also compare
through their base constraint, except against another indexed access, where the
object/index comparison owns the relation. Missing constraints remain undecided
where the port lacks the native fallback machinery.

Target union/intersection comparisons run before source-constraint expansion.
This preserves the identity proof in T -> T | U. The native fast path at
relater.go:2640 compares a parameter directly with its exact constraint before
decomposing unions or rejecting a never target. Overload subtype selection now
lets type-variable arguments use these relations rather than declining them
without comparison.

Stronger relations exposed three dependent omissions. A generic index can be
assignable to an index signature while still needing deferred evaluation;
expression indexing now checks deferral before index-signature applicability.
The noUncheckedIndexedAccess mode is passed through that resolution. Named
callable references now instantiate their declared signatures with the receiver's
arguments, using the existing mapper that preserves signature-owned parameters.
Generic function arguments can use those resolved interface signatures during
contextual instantiation, instead of erasing their own parameters to unknown.

Conditional evaluation recognizes instantiable non-primitive check types directly.
Synthetic polymorphic this is not an ordinary declared type parameter in the
port's symbol map; relying only on that map eagerly evaluated this["prop"] when
the new relation could prove its constraint. Native isDeferredType/isGenericType
(checker.go:24475) retains the conditional until substitution.

## Experiments and controls

Removing the relation gate alone gained 41 matches and lost seven. Generic
indexes must defer before applicable indexes, callable interface signatures need
their receiver mapper, and union identity must precede constraint expansion.
The combined implementation gained 70 and lost five: four generic-callback
inferences and one conditional indexed access on this. Contextual callback
instantiation and deferred-check recognition restored all five. The final pair
has zero RIGHT losses.

Pinned native declarations and Rust controls cover constrained, chained, union,
nullable and unconstrained overload arguments; indexed source constraints;
generic indexes with noUncheckedIndexedAccess; callable interface substitution;
recursive promise unwrapping; empty-array and any-array generic callback
inference; and polymorphic-this conditional return types. Direct relation tests
also cover reverse rejection, T -> T | U, exact never constraints and constraint
cycles. The polymorphic-this call intentionally produces the same native
argument diagnostic while retaining its deferred result type. Corpus expectations
and the pinned oracle are unchanged.

Simplification and review ran sequentially in the main thread under the user's
tool mapping. The review checked comparison order, cycle termination, deferred
index flags, reference/owned-parameter substitution and callback fixing. No
independent agent review is claimed. Existing substitution helpers are reused;
temporary tracing and experimental rollbacks are removed.
Review receipt: /tmp/compound-engineering-501/ce-code-review/source-constraints-710gh2my/review.json.

## Checkpoint and remaining work

a529367a: 451,884/478,855 correct assertions (94.37%).
6,679/9,538 complete cases (70.03%).
Aligned verdicts: 474,243 total; 451,884 RIGHT; 3,481 GAP; 18,878 WRONG.
Relative to e5ee6cc5: 69 WRONG→RIGHT, 3 GAP→RIGHT, zero RIGHT losses,
one GAP→WRONG and four changed wrong answers. The 95% threshold is 454,913;
3,029 additional correct assertions remain.

The new wrong row is underscoreMapFirst's ISeries[] inference becoming unknown[].
Changed wrong answers include genericFunctionInference1's lost higher-order
parameter, getParameterNameAtPosition's callback tuple, and a still-inexact
noUncheckedIndexedAccess narrowing. These remain explicit failures, not gains.
Combined intersection constraints, source-parameter/target-union constraint
fallback, constraint instantiation with this, mapped indexed relation fallback
and broader CheckMode propagation remain in tsr-6.28. Generic mapped/conditional
representation work remains in tsr-6.30.

Evidence: /tmp/tsr-95-source-constraints-final-verdict.{tsv,log},
/tmp/tsr-95-source-constraints-final-transitions.txt,
/tmp/tsr-95-source-constraints-{workspace-tests,clippy,anchors,coverage,depend}.log,
and /tmp/tsr-95-oracle-source-constraints-full.ts with its native declarations.
Checker sources plus trace_case.rs SHA256:
63fdcbb101fbd58d93747317a18b5031822332e454ec25ed6a13b65f262de224.

Release workspace tests, clippy across all targets with warnings denied, format
and whitespace checks pass. All 3,363 upstream references resolve; the checker
snapshot is refreshed. A fresh depend run walks 4,485 gap lines with 577 C1 roots
no longer gapping, 287 cycles, zero depth-cap hits and balanced C3 arithmetic.
C4 still quotes a historical population. Instrument repair remains tsr-6.29;
the full aligned comparison above is authoritative.
