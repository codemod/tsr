# Generic construct-signature argument contexts

Pinned tsgo: 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Unit: tsr-6.21
(generic constructor signature inference). Baseline: 6b2203cc plus 4ff9d349.

## The gap

`getContextualTypeForArgument` (checker.go) handles `CallExpression` and
`NewExpression` with the same code: it reads the resolving or resolved
signature of the call-like node and answers the parameter type at the argument
position. The port's call road (`contextual_type_for_argument`) follows that,
including the "iteration 4 arm (a)" read for a single generic candidate: the
parameter type, fixed to `unknown` where no inference reaches it, or written
type arguments.

The `NewExpression` arm had only the completed-memo reads and the §155 sole
non-generic constructor annotation. A non-context-sensitive argument of a
generic constructor is checked in `check_generic_call_worker`'s first loop, so
neither memo exists yet and the array literal in `new C([1, "a"])` against
`constructor(x: [T, string])` received no tuple context. It printed
`(string | number)[]` and inferred `C<string | number>`; native infers
`C<number>`. The same shape breaks `new DMap([["1", 2]])` and
`destructuringParameterProperties3`'s `new C1(undefined, [0, true, ""])`.

## The change

Arm (a)'s body moved into `single_generic_candidate_argument_type`, shared by
both roads. The `NewExpression` arm now mirrors the call arm's precedence:
an inferential active inference context first, then the memo and resolved
signature, then — parked on `resolving_signature_calls` exactly like the call
road's §469 section — a single generic construct signature of the callee type
through the shared helper. Non-generic and overloaded construct sets keep the
existing §155 annotation road.

## Alternatives

Duplicating the fixing-mapper logic in the `NewExpression` arm would split one
upstream function across two copies. Treating every construct set (including
overloads) like calls would also need the call road's §70 agreement and
§114 arity disambiguation for construct signatures; `new Map([[k, v]])` (three
construct overloads) is therefore still wrong and is left for the overloaded
constructor half of tsr-6.21.

## Verification

Full scorepair against 6b2203cc + the agreeing-overloads unit: +45 assertions
(41 WRONG-to-RIGHT, 4 GAP-to-RIGHT: 34 destructuringParameterProperties3, 8
inferenceContextualReturnTypeUnion3, 3 contextualTypeIterableUnions), zero
RIGHT losses, zero GAP-to-WRONG. Two already-WRONG rows change but remain
wrong: destructuringParameterProperties3's array literal prints
`[number, boolean, string]` where native's final check against the
instantiated signature keeps `true` (`[number, true, string]`). Native types
the literal node during the applicability check with the instantiated
contextual type; the port caches the first, unknown-fixed check. Controls in
`crates/tsr-conformance/tests/generic_constructors.rs` were checked against
pinned tsgo.
