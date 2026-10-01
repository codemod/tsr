# Receiver and rest-parameter overload applicability

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
Baseline 5f7bb6e4: 452,293/478,855 correct assertions (94.45%).

## Native rules and implementation

chooseOverload (checker.go:9025) checks effective arity before inference and
again after instantiating a non-array rest. isSignatureApplicable (:9256)
checks the this receiver before ordinary arguments and compares each effective
argument position. The generic overload walk now shares the existing semantic
parameter-count, minimum-count, rest and position helpers. Every array-rest
argument is checked; a fixed tuple rest contributes its expanded positions.
A resolved this parameter participates in the relation unless its type is void.
Unknown relations, unresolved non-array rests, spread arguments and explicit
type arguments retain the existing fallback. Empty-argument or synthesized
calls whose receiver cannot be recovered also retain the fallback.

getLongestCandidateIndex (:9541) uses effective parameter counts and effective
rest presence even during error recovery. Both callers now share those rules.
A fixed tuple rest no longer automatically wins as if it were an unbounded rest.

getSignatureInstantiation (:19318) erases the signature's own type parameters
before substituting this and ordinary parameters. The former port substituted
only ordinary parameters and left this unresolved. Reusing instantiate_signature
also preserves predicate substitution and clears obsolete written parameter
text. Clearing own type parameters before this helper prevents constraint
reinstantiation from re-entering recursive aliases.

getPropertyOfTypeEx (:18900) chooses CallableFunction or NewableFunction from
resolved signatures before falling back to Function. Class constructor statics
now follow this order. strictBindCallApply:false still uses Function; unresolved
class signatures retain their previous fallback.

Native return types are lazy. The port constructs a function and its return
together, so checking a recursive asserted expression's operand can poison the
function's return while its symbol is being resolved. concise_return_type now
resolves an outer non-const assertion's annotation first. checkAssertion
(:12287) defines that result independently of the operand. The ordinary
expression walk still checks operand nodes. Const assertions retain their
operand-dependent path. This is a bridge for the eager Rust representation,
not a claim that native checkAssertion evaluates in this order.

## Experiments, controls and review

The initial full run gained 124 correct assertions but lost 48. Twenty-two
losses came from reinstantiating erased type-parameter constraints; erasing
before substitution recovered them. The other 26 were recursive asserted
returns in conditionalTypeDoesntSpinForever. Disabling the new this/rest walk
recovered them, locating the regression. An experiment moving all argument
checks after candidate instantiation retained those losses and lost four more
Object.freeze assertions; it was reverted. Resolving asserted return types
before operands recovered the remaining losses and added three further matches.

Review found raw syntactic counts in longest-candidate recovery. A native
TS2575 control with one-position and three-position fixed rests recovers the
three-position candidate's return. The port initially chose the first; the
semantic-count fix passes. A related incomplete tuple containing a type
parameter still exposes structural inference refusal and is recorded as a
remaining gap, not hidden by guessing a substitution.

rest_applicability.rs checks 22 native declaration outcomes across three tests:
bind/call/apply, partial application, class constructor binding, array and tuple
rests, wrong receivers, void receivers, recursive asserted returns, const
assertions, non-strict Function fallback and erroneous arity recovery. Native
controls use explicit strict mode. The arity control deliberately emits TS2575;
the other controls emit declarations without diagnostics.

Native sources and logs: /tmp/tsr-95-rest-applicability.ts,
/tmp/tsr-95-rest-applicability-loose.ts, and
/tmp/tsr-95-rest-arity-failure-independent.ts, with matching declaration folders
and -native.log files. The incomplete generic tuple probe is
/tmp/tsr-95-rest-arity-failure.ts. Sequential main-thread simplification and review
cover correctness, standards, tests, maintainability and adversarial cases under
the user's tool mapping. No independent or cross-model review is claimed.

## Checkpoint and remaining work

6c7fe447: 452,420/478,855 correct assertions (94.48%).
Aligned population: 474,243; 452,420 RIGHT, 3,387 GAP, 18,436 WRONG.
Against 5f7bb6e4: 115 WRONG->RIGHT, 12 GAP->RIGHT, zero RIGHT losses,
4 GAP->WRONG, 1 WRONG->GAP and 19 changed wrong answers. The four new wrong
answers are two Object.assign any results and two unreduced OmitThisParameter
aliases. strictBindCallApply1 contributes 90 of the 127 gains.
The user raised the goal to 99% during this checkpoint. The new threshold is
474,067; 21,647 correct assertions remain. The earlier 95% threshold was 454,913.

Source-frozen evidence:
- /tmp/tsr-95-rest-recovery-final-verdict.tsv
- /tmp/tsr-95-rest-recovery-final-verdict.log
- /tmp/tsr-95-rest-recovery-final-transitions.txt
- Checker sources plus trace_case SHA-256: 93b4019df99816313f378c57b20509002c4751943625c698926506521a95f0f0

6,699/9,538 complete cases pass (70.23%), up three. Release workspace tests
(187 passing result blocks, terminal exit 0), clippy with warnings denied, all
3,362 upstream anchors, checker_types snapshot, format and whitespace pass.
The final recovery correction preserves the full-corpus transition counts.

depend walks 4,378 gap lines, with 574 stale C1 roots, 285 cycles, zero depth-cap
hits and balanced C3. C4 still quotes stale 127,736-gap history. tsr-6.29 owns
instrument repair; aligned verdicts and the checker_types snapshot are the
authoritative measurements.

Remaining work includes full non-array-rest spread construction, incomplete
generic tuple inference, empty/synthetic call receiver recovery, CheckMode
propagation and generic mapped/conditional inference (tsr-6.28/tsr-6.30).
Recursive chain operand nodes can still gap even though their independently
asserted outer return is now known. The 99% goal remains active.
