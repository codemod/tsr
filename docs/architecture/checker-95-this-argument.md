# Generic call this argument inference

Baseline15643d99:448,096/478,855 correct assertions. Pinned tsgo:
5b1047d10d32e7d5b446be4de56b126ff42f82bb.

inferTypeArguments infers the this argument against a signature's this parameter
after contextual return inference and before ordinary arguments. A property or
indexed call supplies its receiver after transparent outer wrappers; a bare call
supplies void. getThisArgumentType removes optional-chain nullability/markers.
The structural collector supplies function parameter and return candidates, so
CallableFunction.call can infer R from the receiver's return. Controls compare
bare/member/element/wrapped/optional calls and simple bind/call receivers. Full
fixed-denominator measurement and release gates determine acceptance.

## Accepted measurement and limits

Checkpoint `THIS_ARGUMENT_COMMIT`:448,122/478,855 correct assertions (93.58%),
6,558/9,538 complete cases (68.76%). Another 6,791 correct assertions are needed
for 95%. Aligned verdicts:474,243 total;448,122 right;4,628 gap;21,493 wrong.
Against15643d99:26 WRONG→RIGHT,zero RIGHT losses,no new wrong answers.
strictBindCallApply1 contributes16 gains; contextualTypeBasedOnIntersectionWithAnyInTheMix5
contributes8;classFieldSuperAccessibleJs2 and unspecializedConstraints contribute1 each.

The pinned declaration control passes member,indexed,parenthesized,detached and
optional generic this calls,and CallableFunction.call return inference. Release
workspace tests and clippy pass;3,380 anchors resolve;snapshot refreshed;whitespace
checks pass. The helper shares the existing transparent-expression walk and
optional-chain marker removal. The collector and return/ordinary argument
priority ordering remain unchanged except for the receiver's new inference site.

Bind/apply still have deferred conditional alias and adjacent variadic tuple
inference limits. Recursive generic signature relations and union matching remain
in tsr-6.1; this unit does not claim to complete strict function helpers.
Code review: skipped (ce-code-review unavailable). The repository's sequential
main-thread rule conflicts with the skill's independent review requirement.
Manual review checks receiver ownership,void for bare calls,wrapper traversal,
optional roots/markers,context snapshot ordering and inference priority cleanup.
This is not independent review.

Evidence:/tmp/tsr-95-this-argument-final-verdict.{tsv,log},
/tmp/tsr-95-this-argument-{tests,clippy,anchors,coverage,controls}.log,
/tmp/tsr-95-oracle-this-argument*.
Checker sources plus trace_case.rs SHA256:
`7594ee7c1e88c979b4a0f2e7219e8b5bd5503d8aa45a5a9cd36c4348d2d8c9d6`.
Goal remains active.
