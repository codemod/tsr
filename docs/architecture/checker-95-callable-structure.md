# Callable structure and mapped method filters

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 5fc064d4: 451,723/478,855 correct assertions (94.33%).

## Native rule and implementation

Native structuredTypeRelatedTo compares object properties and signatures. The
port admitted named objects to that path but excluded anonymous signature-bearing
objects. A method type therefore could not prove assignability to Function, and
`T[K] extends Function ? never : K` remained conditional even after K and T were
concrete. The mapped machinery itself already substituted those operands.

Anonymous signature-bearing objects now enter the structural relation. Their
own property requirements come from value exports and captured anonymous
properties. Existing signature checks and augmented Function/Object property
lookup remain in force. No special acceptance for Function was added: a callable
object must still satisfy required target properties. Class static types without
signature metadata retain their existing path.

The stronger relation exposed a failure-recovery error. When an overloaded call
has one arity survivor and the argument relation rejects it, a set containing a
generic candidate must use getCandidateForOverloadFailure's longest-candidate
branch (checker.go:9498). It cannot intersect uninstantiated generic returns.
The port now returns that original candidate for inference. A shared longest
candidate selector also serves the existing generic overload walk.

## Evidence and review

The initial comparison gained 45 assertions but lost 14 in promisePermutations2.
Those calls deliberately have incompatible callbacks. The generic failure-rule
repair restores every loss, and the focused case matches its preceding results.
An independent small native control reports TS2345 yet emits a string result;
the Rust control checks that exact erroneous call's inferred type.

Other pinned strict controls verify arrows and constructors as Function,
primitive and plain-object rejection, a missing required field on an arrow,
and method-filtered mapped data exposing string and number fields. Controls
exercise the shared relation, not a fixture-name or mapped-type shortcut.

Manual sequential review checked source/target property enumeration, signature
requirements, generic failure candidate selection, argument-count ordering and
retained inference. No independent agent review was performed. The corpus
expectations, denominator and oracle remain unchanged.
The old function-narrowing unit test pinned an unnecessary Function intersection;
it now expects the callable alone, as the pinned native control confirms.

## Measured checkpoint

6786ef93: 451,768/478,855 correct assertions (94.34%).
6,672/9,538 complete cases (69.95%).
Aligned verdicts: 474,243 total; 451,768 RIGHT; 3,495 GAP; 18,980 WRONG.
Relative to 5fc064d4: 29 WRONG→RIGHT, 16 GAP→RIGHT, zero RIGHT losses,
2 GAP→WRONG and 7 changed wrong answers. The 95% target requires 454,913;
3,145 additional matches remain.

The two new wrong rows are typePredicateTopLevelTypeParameter results retaining
undefined. Changed wrong answers concern generic inference,Function narrowing,
and JSDoc callback types. They remain visible in the comparison; none is treated
as a successful match. Generic mapped-value extraction still has alias-display,
generic-deferral and callback-inference gaps, and recursive eager rendering
remains incomplete (tsr-6.30).

Evidence: /tmp/tsr-95-callable-structure-final-verdict.{tsv,log},
/tmp/tsr-95-callable-structure-final-transitions.txt,
/tmp/tsr-95-callable-structure-{controls,workspace-tests,clippy,anchors,coverage,depend}.log,
/tmp/tsr-95-oracle-callable-structure.ts and
/tmp/tsr-95-oracle-callable-overload-failure.ts with native declarations/diagnostics.
Checker sources plus trace_case.rs SHA256:
824b0f43ab2758ea36895a6bd45c2b641d8a4b63a3f244ebabc3da942240d7be.

Release workspace tests and clippy across all targets with warnings denied pass.
All 3,364 upstream anchors resolve; format and whitespace checks pass. The checker
snapshot is refreshed. A fresh depend run walks 4,501 gap lines but reports 575
C1 roots no longer gapping, 287 cycles, zero depth-cap hits and balanced C3 counts.
Its C4 still quotes a historical population, so it cannot establish reachability;
instrument repair remains tsr-6.29. The full aligned comparison is authoritative.
