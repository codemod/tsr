# Contextual typing

How an unannotated parameter gets a type from where its function is written,
what this port can answer, and — more importantly — what it deliberately cannot.

The code is `crates/tsr-checker/src/contextual.rs`. This document carries the
measurements and the reasoning that decided the slice; the module carries the
rules. Upstream references are to the pinned submodule,
`vendor/typescript-go` @ `5b1047d10`.

## The forcing constraint

Contextual typing did not exist in this port at all before `contextual.rs`. It
was already the *named owner* of the largest known-wrong population in the
checker.

`crates/tsr-checker/src/members.rs` has an arm that answers `any` for a property
access on an `any` receiver. That arm is upstream-faithful — `checker.go:11318`
does exactly this — and it landed with a **measured +555 wrong property-access
lines** against 894 gaps closed. Every one of those 555 is the same shape: an
unannotated parameter that answered the implicit `any`, whose member then
answered `any` again with confidence.

Those lines cost no gradient. The parameter's own line was already wrong, so
every case containing one was already failing. What they cost is **diagnostic
separability**: someone reading a property-access histogram sees 555 wrong lines
and cannot tell from the instrument that they belong to a missing subsystem
rather than to a defect in `members.rs`.

So the expected shape of the win here is unusual, and it is worth stating before
the measurement rather than after:

> This turns **wrong lines into right ones without moving the gap column.**

A flat gradient with an improved right/wrong split is the success case. Predicted
any other way, a flat number reads as a failed slice.

## Concentration, measured first

This project has been burned three times by a large row that turned out to be one
file — a 12,376-line row that was 10,000 lines in a single baseline, a 585-line
row that was 96% one file. So the check runs before the code.

Counted over `vendor/typescript-go/testdata/baselines/reference/submodule` by the
signature a contextually typed function leaves in a `.types` baseline: an
assertion whose subject is a function expression with **unannotated** parameters
and whose printed type has **typed** parameters, at least one of them not `any`.

| quantity | value |
|---|---|
| contextually typed function expressions | **925** |
| files containing at least one | **349** |
| largest single file | 42 (4.5%) |
| top 25 files | 36% |
| assertion-line footprint (lines whose subject roots at such a parameter) | **5,230** |
| files in that footprint | **325** — top 10 hold 30% |
| corpus assertion lines counted | 594,122 → footprint is **0.88%** |

The answer is **diffuse**, which is the opposite of the trap. That is what makes
the item worth porting, and it is also what makes any single test fixture a poor
proxy for it.

(594,122 is every `>` line in the baselines. The `checker_types` instrument's
denominator, 478,954, excludes skipped cases, so 0.88% is a lower bound on the
share of the *measured* population.)

## The obvious slice is not the reachable one

The shape everyone pictures is `arr.map(x => x.foo)`. It is **not** reachable
here. `Array.prototype.map` is generic and `every` is an overload set, and this
port has neither `inferTypeArguments` nor a full `resolveCall`.

Classifying the 925 by the printed type of the callee they sit under:

| callee | count | share | reachable |
|---|---|---|---|
| exactly one non-generic signature | 548 | 59% | **yes** |
| generic | 183 | 20% | no — needs inference |
| overload set | 102 | 11% | no — needs the full `resolveCall` |
| not classified by the counter | 92 | 10% | — |

The reachable bucket is itself spread over **206 files**, largest 37 (6.7%).
That is the arm `contextual.rs` ports: a function expression or arrow in the
argument list of a call whose callee has one non-generic call signature, taking
the corresponding parameter's own signature as the contextual one.

## Alternatives, taken seriously

**Port `getContextualType` broadly.** Upstream's `getContextualType`
(`checker.go:29290`) has dozens of arms — variable annotation, object literal
member, return position, JSX attribute, binary operand, array element. Rejected
for this slice because each arm is independent work and the argument arm is the
plurality; a broad port would have shipped several arms untested. The
annotated-variable arm (`const f: (x: T) => U = x => …`) is the strongest
candidate to follow: it needs no call machinery whatsoever, only the annotation's
signature.

**Reuse `calls.rs`'s `resolve_call_signature`.** Rejected on a real hazard, not
on file ownership. That function now takes the argument list and passes it to
`choose_overload`, which checks **every argument** — including the very arrow
whose parameter is being typed. Upstream has the same cycle and breaks it with
signature links, parking `anySignature`/`resolvingSignature` for the duration
(`checker.go:29785`). This port has no such cache. Requiring a single non-generic
callee signature means no argument has to be checked to select it, so the cycle
is never entered rather than broken. The price is the 102 overloaded callees
above. **This decision should be revisited the moment signature links exist**;
with them, the overload bucket and the IIFE arm both open up.

**Answer `errorType` for unreachable forms**, as the rest of the checker does.
Rejected, and this is the one place in the checker where that rule inverts. For a
parameter, `anyType` *is* upstream's answer when no contextual type is available
(`checker.go:18264`, reached from a nil `getContextuallyTypedParameterType`) —
`function f(x) {}` types `x` as `any` in TypeScript too. Returning `errorType`
would convert currently-**right** lines into wrong ones. The rule the module is
actually bound by is the stronger one the `errorType` discipline is derived from:
*never invent an answer*. Returning `None` invents nothing — behaviour is bit-for
-bit what it was before the module existed, so the only lines it can move are
ones it types deliberately.

## The second arm: an annotated variable

`const f: (item: string) => void = item => item`. Ported from
`getContextualTypeForVariableLikeDeclaration` (`checker.go:29438`), whose first
three lines are the whole of it — if the declaration has a type node, the
contextual type *is* that type. No call to resolve, no inference to avoid.

Measured the same way: **75 contextually typed function expressions in 32 files**,
largest file 8 (10.7%). An eighth of the argument arm's 548. It earns its ~30
lines only because it reuses `single_call_signature` wholesale.

Restricted to a `VariableDeclaration`. A `PropertyDeclaration`, a
`PropertySignature`, and a parameter with a function-typed annotation and a
function initialiser all reach the same upstream function, but each is a separate
corpus shape and none is measured, so each is a gap rather than an untested
generalisation.

### A guard that was written, then deleted for being unfalsifiable

The arm originally asserted that the function was the declaration's *initialiser*
and not something else beneath it. The mutation written to prove that assertion
load-bearing **did not go red**, and the reason is structural: a
`VariableDeclaration` has three children — name, type annotation, initialiser —
and only the initialiser can hold an arrow or function expression. The case that
looked most dangerous, typing the annotation's own parameter from the signature it
belongs to, is turned away one level up, because such a parameter's parent is the
`FunctionType` node rather than a function expression.

So the guard was removed and the reasoning put in its place, and the test written
to defend it was deleted rather than left passing. `members.rs` records making the
same call for the same reason. A guard no mutation can make observable is
decoration, and decoration is worse than nothing because it reads as evidence.

## Consequences accepted

- Generic and overloaded callees stay gaps, which means the array-method
  callbacks — the most *recognisable* shape in the corpus — do not move.
- Contexts beyond the two arms — object-literal member, `return`, JSX attribute,
  binary operand, array element — are gaps.
- The IIFE arm (`(x => x)(1)`) is not ported. Upstream types it from the argument
  *expressions*, not from a signature (`checker.go:29463`); it shares no code
  with what is here. 28 of the 925 are IIFE cases.
- A rest or `this` parameter on the contextually typed function is a gap rather
  than an index computed with a correction that nothing tests.
- A union-typed contextual parameter is a gap: upstream builds a union signature
  (`getContextualSignature`, `checker.go:10264`), and picking one member would be
  a guess.

## How this could be right for the wrong reason

The failure mode this slice is most exposed to is a fixture whose contextual type
and inferred type coincide — it proves nothing, because a checker with no
contextual typing prints the same string.

A sharper version of that bit during development and is worth recording. The test
harness resolves a name by walking every node and taking the **first** local of
that name. A contextual-typing fixture names the same thing twice by nature: once
on the callee's function-type annotation, once on the arrow. With the name reused,
the harness answers from the *annotation*, which prints the expected string
whether or not contextual typing exists. Two tests were written that way and one
of them **passed against a checker that could not have known the answer**. Every
arrow parameter in `crates/tsr-checker/tests/contextual.rs` now has a name that
appears nowhere else in its fixture, and the harness's doc comment says why.

## How we would know this was wrong

- If the `checker_types` **gap** column moves materially, the module is answering
  where it should be declining — the only intended movement is wrong → right.
- If any currently-passing case regresses, the `None`-means-implicit-`any`
  reasoning above is wrong somewhere.
- If the 548 reachable count does not correspond to a visible improvement in the
  property-access wrong population, then the 555 wrong lines in `members.rs` are
  not the shape this document claims and the ownership note there needs
  correcting rather than quietly leaving.
