# Class this type annotations

Baseline `1609ae84`:447,754/478,855 correct assertions. Pinned tsgo:
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

getThisType delegates to getThisContainer. Arrows are transparent; ordinary
functions and member containers stop the walk. The container must belong to a
class or interface, must be nonstatic, and a constructor permits the type only
inside its body. Computed names and decorators walk outside the member's class
as specified by the AST utility. Class annotations share the class this identity
already used by expression checking; existing receiver substitution then maps
that identity in method signatures. Interface identities retain their current
representation. Controls contrast instance/static/nested-function/constructor
positions and ordinary class receiver substitution. The fixed-denominator
transition measurement and release gates determine acceptance.

The first draft gains 300 correct assertions but loses six RIGHT assertions.
Five reveal the written-type printer's missing ThisTypeNode arm: the semantic
this annotation can be invalid while the enclosing written signature still
retains its this spelling. One reveals globalThis property resolution exposing
lexical let/const/class/enum bindings; resolveAnonymousTypeMembers excludes
BlockScoped symbols and checkPropertyAccess returns any for missing globals.
Both mechanisms are repaired and remeasured, rather than guarding class this
annotations to conceal their dependent failures.

## Accepted measurement and limits

Measured checkpoint `CLASS_THIS_COMMIT`: 448,096/478,855 correct assertions
(93.58%), 6,557/9,538 complete cases (68.75%). Another 6,817 correct assertions
are required for 95%. Aligned verdicts: 474,243 total;448,096 right;4,628 gap;
21,519 wrong. Against 1609ae84:310 WRONG→RIGHT,32 GAP→RIGHT,zero RIGHT losses,
11 GAP→WRONG. Largest gains:thisTypeInFunctions76,strictBindCallApply1 62,
thisTypeInFunctionsNegative50,thisTypeInClasses41,thisTypeErrors17.

The eleven new wrong answers include conditional this substitution/parentheses,
recursive alias spelling, union receiver signature distribution and unchecked JS
contextual/expando property inference. They remain in the reported deficit.
Class this constraints and recursive/generic receiver semantics remain incomplete;
this unit supplies annotation identity and the existing receiver mapper.

Pinned tsgo declarations verify Base/Derived this parameters and fluent returns,
and runtime versus lexical/missing global properties. Checker controls verify ten
container boundaries, including static blocks, computed names, constructor body
versus parameters, arrows, nested functions and interface/property members.
The contextual-method test's old error expectation is corrected to the upstream
missing-global any recovery; its contrasting inherited receiver still yields
number. Release workspace tests/clippy pass;3,380 anchors resolve;whitespace
checks pass;snapshot refreshed.

Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
Manual review checks container traversal, static/constructor boundaries, shared
class identity, written error recovery, global lexical exclusions and receiver
substitution. This is not independent review.

Evidence: `/tmp/tsr-95-class-this-global-verdict.{tsv,log}`,
`/tmp/tsr-95-class-this-final-{tests,clippy,anchors,coverage}.log`,
`/tmp/tsr-95-class-this-boundaries.log`, `/tmp/tsr-95-oracle-class-this*` and
`/tmp/tsr-95-oracle-global-this*`.
Checker sources plus trace_case.rs SHA256:
`a9459e2ea9551e0e220218b183b8095cc01514af93a68f64239b764bbaedc311`.
Goal remains active.
