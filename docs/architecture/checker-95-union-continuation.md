# Union inference continuation

Baseline 83c358c0:449,259/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

The union walk now matches exact/regular literal constituents, then same-origin
references and named objects in descending nesting depth (inference.go:101).
Each phase visits all matching pairs before removing matched constituents.
Remaining targets follow observed inference priority and naked-variable source
remainders (inferToMultipleTypes). Observed priority is separate from the input
priority; nested contextual signature instantiation saves and restores it.
Unexplored depth-limited paths mark the walk incomplete. The earlier Promise
slot shortcut is removed. Generic alias provenance, full structural identity,
and the shared variable of intersection targets remain incomplete (tsr-6.1).

Subtype and strict-subtype comparisons now follow readable type-parameter
constraints, reject concrete object/unknown sources against arbitrary target
parameters, and reject nullable sources against objects under strict null
checking. Primitive wrapper required-property failures can prove rejection of
indexed targets. Indexed acceptance remains unknown. Parameter-only constraint
cycles remain unknown rather than proving an object relation; synthetic this
parameters retain their existing member comparison path (relater.go:3664).

These additions deliberately retain preceding assignability declines. Broader
relation drafts lost correct generic-call recovery and contextual callback
answers: the last broad draft lost14 RIGHT assertions, including8 parenthesized
callback answers. Subtype-only scope preserves the baseline. The first union
draft lost13 promisePermutations2 answers; constraint comparisons supply the
missing common-supertype prerequisite without reinstating the Promise shortcut.

Uninitialized identifier recovery now inspects the computed optional flow
before applying the existing unported-condition guard. A supported predicate's
false branch retaining undefined recovers the declared union (checker.go:11189).
An unchanged flow retains the guard. This remains a bounded recovery arm, not a
complete definite-assignment or narrowing port. Pinned tsgo reports TS2454 and
TS2322 for assigning that false-branch read to number; the older local control
expected number and is corrected to string | number.

## Verification and limits

Checkpoint 25215cc8:449,486/478,855 matching assertions (93.87%).
Complete cases:6,589/9,538 (69.08%). Another5,427 assertions are needed for95%.
Aligned verdicts:474,243 total;449,486 right;4,200 gap;20,557 wrong.
Against83c358c0:187 WRONG→RIGHT,40 GAP→RIGHT,zero RIGHT losses,
44 GAP→WRONG and49 WRONG→WRONG type changes. Denominator and oracle unchanged.

Eight pinned declaration controls pass, covering nested array unions, promised
and nullable unions, multiple naked variables, primitive priority and recursive
Promise/IPromise callbacks. Two subtype controls pin constraint direction and
cyclic constraints. Release workspace tests and clippy with warnings denied
pass;3,375 upstream anchors resolve. Checker snapshot and whitespace checks pass.

The44 new wrong outputs expose incomplete paths:12 contextual return inference
answers ignoring any/unknown,6 nested literal variable answers,5 async yield*
context answers,18 answers across for-of37/38/40/45/49/50, and one each in
objectFromEntries,typeParameterFixingWithConstraints,iterableArrayPattern27.
Candidate priority/contextual defaults, implicit key constraints and general
iteration candidate origins remain work in tsr-6.1,tsr-6.3,tsr-6.15. These wrong
outputs are recorded deficits. Module-local shadowing Promise structures also
remain outside the global augmentation control. This unit does not reach95%.

Code review: skipped (ce-code-review unavailable). Independent dispatch conflicts
with the sequential main-thread instruction. Manual review checked matching
order, merged symbol identity, priority save/restore, incomplete-walk handling,
constraint cycles, synthetic this handling and recovery guards.

Evidence:/tmp/tsr-95-union-continuation-final-verdict.{tsv,log},
/tmp/tsr-95-union-continuation-final-{tests,clippy,anchors,coverage}.log,
/tmp/tsr-95-oracle-union-continuation{.ts,-out/},
/tmp/tsr-95-oracle-recursive-promises{.ts,-out/},
/tmp/tsr-95-oracle-call-guard.ts (diagnostic probe, nonzero exit).
Rejected broad drafts:/tmp/tsr-95-union-continuation-{parameter-relation,
unknown-parameters,flow-recovery}-verdict.tsv.
Checker sources plus trace_case.rs SHA256:
d1ea01d1e55a8f1bcfd722317956c72ff23ef7cc6cb46911253325ea06d787ce.
