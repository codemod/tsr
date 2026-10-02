# Candidate-free inference defaults and constraints

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline code 948a8f5b, evidence cd51e9c8: 455,750/478,855 matching assertions.

The normal generic-call path returned unknown for a missing inference even when
its parameter had a constraint. It applied constraints only in the
SkipContextSensitive recovery pass, and instantiated defaults and constraints
against parameters already visited from left to right. Native getInferredType
(internal/checker/inference.go) uses a non-fixing mapper: constraints may depend
on later parameters. A extends B, B extends string must resolve both to string.

The final call mapper now collects existing candidates first, then resolves
candidate-free parameters on demand. A provisional unknown (any at JS call
sites) enters the map before resolving constraints, matching the native cycle
sentinel. Recursive and mutually dependent constraints terminate against that
provisional value. Defaults use newBackreferenceMapper (internal/checker/mapper.go):
earlier references resolve through the inference mapper; self and forward
references map to unknown, even in JS and even if a later parameter has a
candidate. Defaults are checked against instantiated constraints; a definite
failure selects the constraint. Unsupported instantiation remains an error.

This is the candidate-free leg of inference, not completion of all dependent
candidate constraints. The existing structural-source refusal remains. Removing
it wholesale gained 95 assertions but lost 36 RIGHT assertions, produced 120
GAP→WRONG transitions and changed 161 WRONG rows. The lost cases were
circularReferenceInReturnType2, inferenceContextualReturnTypeUnion3 and
nonInferrableTypePropagation3. Those require structural/contextual inference and
circular resolution work, not a guessed default. The experiment is retained at
/tmp/tsr-99-empty-inference-{candidate.tsv,delta.txt}; tsr-8 owns the remainder.

Applying constraints with the old left-to-right mapper gained 71 and lost one
RIGHT assertion. The recursive mapper gained 88 and retained the same loss.
The loss in strictBindCallApply1 exposed a separate partial-relation problem:
a conditional-rest receiver compared with any[] is unknown under the subtype
pass, but the ordinary thisArg argument definitely mismatches. Native
isSignatureApplicable rejects the candidate. The port returned undecidable
before checking that argument. Overload applicability now carries an unknown
receiver result into its argument verdict, so a definite argument mismatch can
reject it; a compatible argument cannot turn the unresolved receiver into an
acceptance. This preserves the native error-candidate selection without claiming
that conditional-rest subtyping is fully implemented.

Four pipeline tests pin 22 native outcomes: absent/defaulted/constrained type
arguments, backward/forward dependencies, candidate references, self/mutual
constraint cycles, invalid defaults, function constraints, JS AnyDefault and
conditional-rest overload failure. Inputs under /tmp/tsr-99-inference-defaults*.ts
and /tmp/tsr-99-inference-js were checked with /tmp/tsr-95-tsgo and explicit
--strict true. Invalid default references produce native diagnostics but still
emit the pinned result types. The bind control is also pinned by the upstream
strictBindCallApply1 baseline. Tests exercise the real corpus pipeline and libs.

The final measured candidate gains 88 assertions (76 WRONG→RIGHT, 12 GAP→RIGHT),
with zero RIGHT losses and zero new WRONG rows. Three already-WRONG results
change: two mappedTypes1 rows now have the right properties in the wrong order;
esDecorators-contextualTypes.2 supplies any[] for its constrained rest parameter
but still lacks its contextual class/parameter inference. These remain explicit
limitations. No oracle, denominator, or corpus expectations change.

Candidate verdict: /tmp/tsr-99-inference-defaults-candidate2.tsv.
Comparison: /tmp/tsr-99-inference-defaults-delta2.txt.
Final quality gates and committed verification are recorded below after running.

Sequential primary-thread correctness, adversarial, standards and coverage review
checked mapper cycles, native backreferences, error propagation and overload
acceptance. The simplification pass keeps provisional entries in the existing
map and records their stable vector index instead of searching it for updates.
Review caught that a failed recursive dependency could leave its provisional
unknown cached; error results now replace that entry and propagate to callers.
This does not treat unsupported instantiation as a successful fallback.
No independent or cross-model review is claimed under the no-delegation rule.

Initial main gates: 209 passing release workspace result blocks, clippy, formatting
and 3,330 checked anchors. Final workspace/clippy checks repeat after the reviewed
error-propagation correction; isolated committed measurement follows.

Frozen checker/trace_case SHA-256: 9905fc83edc0125ad038aa114fb8ddf88c1889be71c619695060fb0d92085671.

Final main checks pass: all 209 release workspace result blocks, clippy and
formatting. Review receipt:
/tmp/compound-engineering-501/ce-code-review/inference-defaults/review.json.

Committed verification at 0ba7ce80 in /tmp/tsr-99-inference-defaults-verify matches
the candidate verdict byte-for-byte and the frozen source hash. Coverage is
455,838/478,855 RIGHT (95.19%), 6,912/9,538 complete cases (72.47%), 12 more
complete cases. The 99% target still needs 18,229 matches. Aligned counts:
455,838 RIGHT, 2,775 GAP, 15,630 WRONG, 474,243 total. All 209 release workspace
result blocks, clippy, formatting and 3,330 anchors pass in that checkout. Fresh
depend: 504 non-gapping roots, 210 cycles, zero depth caps and 3,591 walked gaps;
C3 balances, C1/C4 remain stale (tsr-6.29). The snapshot is copied from the isolated
checkout. Verdict: /tmp/tsr-99-inference-defaults-verified.tsv; logs:
/tmp/tsr-99-inference-defaults-verified-*.log.

Five isolated mutations compile and fail the intended assertions: removing
constraint fallback fails closed; resolving forward defaults instead of mapping
them to unknown fails forwardDefault; restricting constraint dependencies to
earlier parameters fails later; replacing the recursive TS sentinel with any
fails recursiveConstraint; immediately abandoning an unresolved receiver fails
callback.bind(2). Sources restore in a finally block after every mutation and
the final hash equals the committed source hash. Script:
/tmp/tsr-99-inference-defaults-mutations.py. Logs:
/tmp/tsr-99-inference-defaults-mutation-{constraint,forward,dependency,sentinel,receiver}.log.

All four focused tests pass after restoration:
/tmp/tsr-99-inference-defaults-restored-tests.log.
