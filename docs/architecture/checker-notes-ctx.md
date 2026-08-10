# Contextual typing

How an unannotated parameter gets a type from where its function is written,
what this port can answer, and — more importantly — what it deliberately cannot.

The code is `crates/tsr-checker/src/contextual.rs`. This document carries the
measurements and the reasoning that decided the slice; the module carries the
rules. Upstream references are to the pinned submodule,
`vendor/typescript-go` @ `5b1047d10`.

## Corrections, 2026-08-05

Two anchors in this document and in `contextual.rs` were wrong at the pinned
commit. Recording the correction rather than editing silently, because a silent
edit destroys trust in every other number here.

| cited | actual at `5b1047d10` | what it is |
|---|---|---|
| `checker.go:29290` | **`checker.go:29343`** | `getContextualType`, the dispatch |
| `checker.go:19806` | **`checker.go:18959`** | `getSignaturesOfType` (cited in `contextual.rs` only) |

Both were off by roughly the same amount in the same direction, which is the
signature of a number copied forward from an earlier submodule pin rather than
of a typo. The remaining citations in this file were re-`grep -n`-verified and
are correct: `29438` (`getContextualTypeForVariableLikeDeclaration`), `29458`
(`getContextuallyTypedParameterType`), `29463`
(`GetImmediatelyInvokedFunctionExpression` inside it), `18264` (`t = c.anyType`),
`10264` (`getContextualSignature`), `10349`
(`assignContextualParameterTypes`), `29785` (the comment introducing the
`resolvingSignature` park; the check itself is `29787`), `11318` (inside
`checkPropertyAccessExpression`'s `isAnyLike` branch, which opens at `11314` and
returns the apparent type at `11320`).

The four anchors in this cycle's brief were verified before use and all four
hold: `getContextualType` `29343`, `getContextualTypeForReturnExpression`
`29621`, `getContextualTypeForObjectLiteralElement` `29920`,
`getContextualTypeForElementExpression` `29972`.

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
(`checker.go:29343` — corrected 2026-08-05 from `29290`, see "Corrections"
above) has twenty arms — variable annotation, object literal
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
  correcting rather than quietly leaving. **Status: open — see below.**

## What actually happened, cycle 9

Recorded here rather than in a message, because the messages do not survive the
session and the numbers need to be checkable against the next run.

Corpus tip after the cycle: assertion lines **60.26% -> 60.60%**, cases
**2,041 -> 2,066**. That is the whole cycle across four agents, not this module
alone; contextual typing's own contribution is not separable from that figure.

**The prediction that mattered held: the gap column did not move, and nothing
regressed.** Both were stated before the measurement — a flat gradient with an
improved right/wrong split was named as the success case in advance, precisely so
a small number could not be re-read afterwards as an excuse. It should be read
that way now.

### One falsifier is still OPEN, and it is the sharpest one

> If the 548 reachable count does not correspond to a visible improvement in the
> property-access wrong population, then the 555 wrong lines in `members.rs` are
> not the shape this document claims.

**This has not been checked.** The cycle-tip run reports passed / assertion lines
/ failed / skipped, and none of those separates *wrong* from *gap*, which is the
only split that can settle it. It is recorded as open, not as passed. Do not cite
the +0.34 as evidence for it either way.

To settle it, the instrument needs something it does not have today: a count of
assertion lines where this port answers a **confident** type that differs from
upstream, bucketed by the syntactic form of the subject — specifically
`>receiver.member : any` against an upstream line that is not `any`. That is a
change to the conformance reporting, not to the checker. Until it exists, the
ownership claim in `members.rs` — that those 555 lines belong to contextual
typing rather than to the property-access arm — rests on the reasoning in this
document and on the probe `const y = x => x.foo`, not on a measurement taken
after the fix.

If the count is taken and the 555 did **not** drop, the correction belongs in
`crates/tsr-checker/src/members.rs`, where the ownership is asserted. Correcting
it there rather than quietly dropping the claim is the whole point of having
written the owner down.

### The snapshot in the tree is older than the tip

`crates/tsr-conformance/snapshots/checker_types.snap` still reads
`288626/478954 (60.26%)` and `2041/9538`. The 60.60% / 2,066 figures above come
from a run that was never committed. Anyone reading the snapshot as current will
under-count the tip by 0.34 points and 25 cases.

---

## Cycle 10: the other nineteen arms, measured

The two arms above were chosen without measuring the alternatives — the argument
arm was the plurality and the annotated-variable arm was nearly free, and that
was enough to justify them but not enough to rank what came next. This section
measures the rest, so the next choice is made against numbers rather than
against which shape is most recognisable.

### The instrument, and its disagreement with the 925

Counted over `vendor/typescript-go/testdata/baselines/reference/submodule`
(12,155 `.types` baselines) at `5b1047d10`. A **contextually typed function
expression** is an assertion line `>SUBJECT : TYPE` where

- `SUBJECT` is a function expression or arrow whose parameters are all **bare
  identifiers** — no annotation, no default, no rest, no destructuring; and
- `TYPE` is `(p: T, …) => R` with the same arity, no optional parameter, and at
  least one parameter type that is not `any`.

The syntactic context is then read off the reconstructed source: the baseline's
non-`>` lines are the source file, so the bracket stack at the function's start
offset says whether it is a call argument, an object-literal member, an array
element, and so on.

**This instrument counts 1,557 where the cycle-9 note records 925, and the
disagreement is not resolved.** The cycle-9 counter was not preserved, so the
two cannot be diffed. The exclusions above are *stricter* than anything the
cycle-9 note describes, so the 925 was probably narrower still in some way it
did not write down. Two consequences, and the second is the one that matters:

1. Every number in this section comes from **one** instrument, described above,
   and is internally comparable. None of them is comparable to the 925/548 split
   in the cycle-9 section.
2. **The 548 "reachable" figure should be treated as instrument-dependent**, not
   as a corpus fact. It is still the best estimate available for that arm, but
   the OPEN falsifier below is stated in terms of it, and a falsifier resting on
   an unreproducible count is weaker than it looked.

The lesson is cheap and general: **a measurement is only re-checkable if the
thing that produced it is described precisely enough to rebuild.** The criteria
above are written out for that reason.

### Population by context

| upstream arm | context | count | files | largest file | top 10 files |
|---|---|---|---|---|---|
| `getContextualTypeForArgument` `29762` | call argument | **760** | 288 | 3% | 24% |
| `getContextualTypeForObjectLiteralElement` `29920` | object-literal member | **297** | 98 | 13% | 47% |
| — | `=` with no enclosing bracket (mostly JSDoc-typed JS) | 124 | 69 | 6% | 40% |
| `getContextualType` `29392` | parenthesised expression | 101 | 28 | 18% | 79% |
| JSX attribute / `yield` / ternary | (several arms) | 87 | 38 | 9% | 56% |
| `getContextualTypeForSubstitutionExpression` `30030` | tagged-template substitution | 85 | 17 | 15% | 89% |
| `getContextualTypeForInitializerExpression` `29423` | class property initialiser | 35 | 16 | 23% | 83% |
| `getContextualTypeForElementExpression` `29972` | array element | 33 | 15 | 18% | 85% |
| `getContextualTypeForReturnExpression` `29621` | `return` operand | **26** | 15 | 19% | 81% |

The brief named three candidates. Two of them are **tiny**: `return` is 26
functions and array element is 33, both in fifteen files with 81–85% of the
population in ten of them. Neither is worth an arm, and the concentration check
is what says so rather than a judgement about how "important" `return` looks.

### Concentration killed the cheapest arm on the board

The parenthesised-expression arm is **one line** upstream — `case
ast.KindParenthesizedExpression: return c.getContextualType(parent,
contextFlags)` (`checker.go:29392`) — and 101 functions is third place. It was
the obvious build until the concentration check ran on the *reachable* subset:

| the parenthesised expression is… | count | files | reachable today |
|---|---|---|---|
| itself parenthesised again | 19 | 4 | yes, transitively |
| a `&&` / `\|\|` / `??=` operand | 15 | 11 | no |
| a call argument | 11 | 5 | yes |
| a decorator or in a block | 9 | 2 | no |
| an annotated declaration's initialiser | 2 | 2 | yes |

**~32 reachable, in 7 files**, and `parenthesizedContexualTyping{1,2,3}.types`
plus `logicalAssignment5` hold most of it. That is the exact shape this project
has been burned by three times — a respectable row that is a handful of test
files exercising one syntax deliberately. Rejected. What would make it win: the
binary-operand arm (`checker.go:29809`) landing first, which converts the 15
`&&`/`||` rows from "not reachable" to "reachable" and moves the population out
of two test families.

### The arm that was built: object-literal member

297 functions in 98 files is second place, but the number that decides it is not
297. This arm answers nothing unless the **enclosing object literal already has
a contextual type**, so the repaired triage question — *is the prerequisite met
for most of the population?* — has to be asked about the object literal, not
about the member. Splitting the 297 by what supplies the object literal's own
context:

| the object literal is… | count | files | met? |
|---|---|---|---|
| an argument to a generic callee | 82 | 26 | no — needs `inferTypeArguments` |
| initialiser of a simply annotated declaration | 47 | 24 | **yes** |
| initialiser of an unannotated declaration | 28 | 18 | no |
| a member of another object literal | 23 | 10 | no — needs the index-signature fallback |
| an array or tuple element | 22 | 8 | no — needs tuple types |
| initialiser of a **union**-annotated declaration | 20 | 3 | no — needs `getApparentTypeOfContextualType` |
| an argument to a one-signature callee | 19 | 10 | **yes** |
| parenthesised / in a block / `return` / other | 56 | 30 | no |

**66 of 297 (22%)** in 33 files. Assertion-line footprint — the function's own
line plus every line whose subject roots at one of its contextually typed
parameters — is **259 lines**; largest file 21%, top 10 files 61%.

So the honest headline is not "297, the second-largest arm". It is **66**, which
is the same order as the annotated-variable arm's 75 and is justified the same
way: it is cheap because everything it needs already exists. That the two
numbers agree so closely is a coincidence, not a cross-check.

The 22% figure is also the strongest single argument that the *remaining*
contextual-typing work is gated on inference and unions rather than on more
arms. 82 + 20 = 102 of the 297 — a third of the arm — are blocked on two
prerequisites, neither of which is another `getContextualType` case.

### A structural deviation, and why upstream's shape had to be adopted

The cycle-9 module had no notion of "the contextual type of an expression". It
asked, of a *function*, "are you a call argument or an annotated initialiser?"
and answered a `Signature`. **That shape cannot express the object-literal arm
at all**, because its subject is the enclosing object literal — an expression
that is not a function and has no signature.

So `contextual.rs` now has `get_contextual_type(node)`, ported from the dispatch
at `checker.go:29343`: a match on the **parent's** kind, answering a `TypeId`.
Three of upstream's twenty arms are implemented. `contextual_signature` is now
`single_call_signature(get_contextual_type(function))`.

This is a strict generalisation, not a rewrite: both cycle-9 arms were already
`single_call_signature(<some type>)` under the hood, so factoring the type out
of them changes no answer. Verified two ways — the seven cycle-9 tests are
untouched and green, and the exclusivity argument the old `or_else` rested on
("a function is either a call argument or an initialiser, never both") is now
structural rather than argued, because a node has one parent.

**The alternative was a fourth special case**: `contextual_signature_from_object_literal_member`,
sitting alongside the other two in the `or_else` chain, reaching into the object
literal and duplicating the annotated-declaration and call-argument logic to
find *its* context. Rejected because the duplication is unbounded — the same
duplication would be needed again for the array-element arm, and again for
parenthesised, since every one of them recurses on the parent's context. What
would make it win: if only ever one more arm were going to be added, three
special cases is less code than a dispatch. Upstream's twenty arms say
otherwise.

### The two "unreachable" buckets, re-checked rather than inherited

Both cycle-9 rejections were re-verified this cycle instead of being carried
forward, because both were rejected on facts about *other files* that have since
moved.

- **Generic callees** — still unreachable, and for a sharper reason than
  "no inference". `crates/tsr-checker/src/inference.rs` has grown
  `check_generic_call` and `instantiate_type` since cycle 9, so the blanket claim
  is out of date. But `check_generic_call` instantiates **only the signature's
  return type** (its own doc comment says so, `inference.rs:114`), never its
  parameter types, and its first act is to `check_expression` every argument —
  which is precisely the cycle contextual typing must not enter. So the bucket is
  unreachable for a *second* reason that did not exist before.
- **A gap this opened up that nobody has sized.** A call with **written** type
  arguments — `f<string>(x => …)` — needs no inference at all: substituting the
  written arguments into the parameter type is exactly what
  `instantiate_type` does, and `written_type_arguments` already exists.
  Population unmeasured; the only datum is that exactly 1 of the 297
  object-literal cases has this shape, and `.reduce<T>(…)` appears in the raw
  call-argument bucket. **Filed, not built — it does not exist today:
  `bd tsr-9mx`.** Sizing it is one run of the instrument described above with one
  extra bucket.
- **Overloaded callees** — still rejected, and the condition the cycle-9 note set
  for revisiting is still unmet: `grep -rn "signature_links\|resolving_signature"
  crates/tsr-checker/src/` returns nothing at this tip, and
  `calls.rs:568`'s `resolve_call_signature` still takes the argument list. The
  deliverable therefore remains the one the note already gave: *this needs
  signature links.* Nothing about it changed, and re-checking cost one grep.

### What was built, and what it must not do

Three arms in `get_contextual_type`, four tests, and one deletion.

The **deletion**: `contextual_type_for_object_literal_element` does not check
that the `PropertyAssignment`'s parent is an `ObjectLiteralExpression`. A
`PropertyAssignment` has no other possible parent in this AST, so no mutation
could redden such a guard. Same call, same reasoning, as the initialiser guard
deleted in cycle 9 — a guard no mutation can make observable is decoration, and
decoration is worse than nothing because it reads as evidence.

The **`errorType` rule holds unchanged**: every unported shape in the new arm
answers `None`, which one level up becomes the implicit `any` that upstream also
answers (`checker.go:18264`). A member whose name is absent from the contextual
type — upstream would fall back to an index signature at `checker.go:29946` —
answers `None` rather than guessing, and
`a_member_absent_from_the_contextual_type_stays_the_implicit_any` asserts it
against a sibling member that does resolve.

### Mutations, each confirmed applied before the run

`grep`-counted to exactly one occurrence before each edit, per the rule that a
mutation that does not apply is indistinguishable from one that does not matter.

| mutation | reddens | does **not** redden |
|---|---|---|
| **M0** `Node::PropertyAssignment(…) => None` | all three object-literal tests | the seven cycle-9 tests |
| **M1** `Node::VariableDeclaration(_) => None` | `…member_is_typed_from_the_property_of_the_same_name`, `…absent…`, `an_annotated_variable_types_its_initialiser_s_parameter` | `an_object_literal_argument_types_its_member_through_the_callee` |
| **M2** `Node::CallExpression(_) => None` | `an_object_literal_argument_types_its_member_through_the_callee` + the three cycle-9 call tests | both annotated-object-literal tests |
| **M3** `PropertyName::StringLiteral(_) => return None` | `a_string_literal_property_name_resolves_the_same_as_an_identifier` — and only that | everything else |

M1 and M2 are the required pair: each reddens exactly one of the two new
recursion paths and leaves the other green.

**One claim in the fixtures has no mutation behind it, and that is stated rather
than hidden.** `an_object_literal_member_is_typed_from_the_property_of_the_same_name`
writes the object literal's members in the *opposite* order to the interface's,
so an implementation that matched by position would answer the two types
swapped. No mutation demonstrates this, because the code has no positional path
to mutate — the fixture excludes an alternative *implementation*, not a branch.
That is a different thing from a guard, and it is why it was kept rather than
deleted.

The fixture trap from cycle 9 was respected: `solo`, `duo`, `value`, `yes`,
`nope` each appear exactly once in their fixture, and the interface parameter
names (`declaredA`, `declaredB`, `declaredT`, `declaredK`) appear nowhere else,
so the harness's first-local walk cannot answer from an annotation. No fixture
mentions `Array`, `Promise`, `number[]` or any other lib type, because the unit
harness loads no lib files.

### Prediction

Rows: the **66 reachable object-literal-member functions in 33 files**, footprint
**259 assertion lines**. Commit pair: **`2e00f90^..2e00f90`**, verified with
`git log --oneline 2e00f90^..2e00f90` to resolve to exactly one commit. That is
the only commit carrying a checker change this cycle from this module; the SHA
was filled in by the doc-only commit immediately after it, which touches no
code.

Mechanism: the member's arrow parameter stops answering the implicit `any` and
answers the interface property's parameter type instead, so the parameter's own
line and every line rooting at it move from *wrong* to *right*.

- **Predicted: fewer than 259 lines move, and most likely 120–200.** The
  footprint counts every line rooting at a contextual parameter, including ones
  whose own answer depends on machinery this port lacks for other reasons.
- **The shape is the cycle-9 shape, not the usual one**: wrong → right, with the
  **gap column flat**. A flat gradient here is the success case. This is stated
  before the measurement, exactly as in cycle 9, so a small number cannot be
  re-read afterwards as an excuse. If a future arm is predicted whose shape is
  gap → right, say so up front, because "flat" then means failure.
- **What must NOT move**: the gap column, and the count of passing cases must not
  fall. Any regression falsifies the `None`-means-implicit-`any` reasoning.
  Neither the argument arm's nor the annotated-variable arm's population may
  change — the refactor is meant to be answer-preserving for both, and the seven
  cycle-9 tests staying green is the evidence for that.
- **How this could be right for the wrong reason**: the 66 are concentrated
  enough (top 10 files 61%) that a single baseline moving for an unrelated reason
  — three other agents are editing this checker in the same tree — could account
  for the whole number. A corpus delta taken at the cycle tip cannot attribute it.
  The attributable evidence is the four tests and the mutation table, not the
  gradient.
- **Discount this number if it arrives without a cross-check.** It has one: the
  259 was produced by the same script that produced the 66 and the 297, and the
  297 splits exactly into the eight rows of the reachability table. It has no
  external cross-check, because the cycle-9 instrument that would provide one is
  not reproducible.

### The OPEN falsifier is still OPEN

> If the 548 reachable count does not correspond to a visible improvement in the
> property-access **wrong** population, then the 555 wrong lines in
> `crates/tsr-checker/src/members.rs` are not the shape that document claims.

**Not settled this cycle, and not settleable with the instruments as they
stand.** The cycle-tip run reports passed / assertion lines / failed / skipped;
none of those separates *wrong* from *gap*.

Two things learned about the instrument that would settle it, both worth
recording because they change what "run `wrong_attribution`" costs:

- `crates/tsr-conformance/examples/wrong_attribution.rs` is on the **lib-less
  per-unit path** (`parse_with_options`, ~line 810) rather than going through
  `types_producer::assertions_for_case`. Its numbers are therefore measured on a
  different checker configuration than the gradient — the same defect fixed in
  `examples/overload_funnel.rs` in commit `731b1ee`. Fixing that is a
  prerequisite for the falsifier, not an optional cleanup.
- The falsifier's *left-hand side* is now known to be instrument-dependent (see
  the 1,557-vs-925 discussion above). If the count is taken and the numbers
  nearly agree, that is weaker evidence than it reads, because the 548 cannot be
  reproduced.

If it is ever taken and the 555 did **not** drop, the correction belongs in
`crates/tsr-checker/src/members.rs`, where the ownership is asserted — not
quietly here.

## Cycle 10, measured: the object-literal arm moved +22, against 120–200 predicted

Measured by the lead, not by this workstream. Isolated pair
**`2e00f90^..2e00f90`** (verified one commit with `git log --oneline`), each side
run in its own `git worktree add --detach` with its own submodule checkout and
its own target dir, so no teammate's edits are inside either measurement. Both
snapshots confirmed freshly written with `git status --porcelain` before being
read — see the trap recorded in `checker-notes-arrays.md`, where a checked-in
snapshot was nearly read as a measurement.

| | before | after | delta |
|---|---:|---:|---:|
| assertion lines | 291,349/478,954 | 291,371/478,954 | **+22** |
| cases | 2,128/9,538 | 2,128/9,538 | **0** |

**This is a miss, and a large one: predicted 120–200, delivered 22.** Between 5.5×
and 9× high. Recorded as a miss on the project scoreboard, which now stands at
4 hits, 8 misses.

### What held, and it is not nothing

The prediction's *"must not move"* clause was **passing-case count**, and it did
not move — 2,128 both sides. The stated success shape was "wrong → right with the
gap column flat", and because both wrong→right and gap→right add to `right`, the
+22 is the whole conversion however it splits. Nothing regressed.

### What the miss does and does not tell us

The arm's own sizing was the careful part of this cycle's work and should not be
discarded with the prediction: the headline 297 was correctly cut to **66
reachable in 33 files, footprint 259 assertion lines**, by splitting on what
supplies the enclosing object literal's own contextual type. That cut was the
right move and it was measured.

22 of a 259-line footprint is an **8.5% conversion rate**, and the two candidate
explanations are not separable from this number:

1. the 66 reachable sites are reachable in principle and something later in the
   chain still gaps them — the kind-2 chain exposing its next link, exactly as
   `docs/conventions.md` warns happens when a named blocker is removed; or
2. the 259-line footprint over-counts what the 66 sites actually block.

**Distinguishing them needs the wrong-versus-gap split, which is the same
instrument this page's other falsifier is already blocked on** —
`crates/tsr-conformance/examples/wrong_attribution.rs`, still on the lib-less
per-unit path (`bd tsr-qj4`). That is now the second open question on this page
waiting on one instrument fix, which is a stronger argument for doing it than
either question made alone.

### The estimate's shape, named so it is not repeated

This is the same failure as the export-markers miss scored the same day
(`checker-notes-arrays.md`): **a measured population was quoted as a predicted
conversion.** The 259-line footprint is a *ceiling* — it is what the 66 sites
touch, not what fixing them converts — and 120–200 is 46–77% of it. Neither miss
stated an expected conversion *rate* separately from the population, and neither
said what would make the rate low. The correction is one sentence in the
prediction and it is now recorded in `docs/conventions.md`.

Worth stating plainly against the temptation to read a small number kindly: this
page pre-registered "a flat gradient with an improved right/wrong split is the
success case", which is a *correct and honest* thing to have written before
measuring — and it is not what this prediction said. This prediction named
120–200 lines. It got 22.

## §114 — the contextual parameter road's next context: PARENTHESIZED function annotations [checker-2, bar]

The calleegap sizing's named entry, claimed for the next window.
parenthesizedContexualTyping2's shapes: a function-typed parameter
annotation wrapped in PARENS (`x: (<T>(p: T) => T)`) fails to
supply the contextual parameter type — the §56-family road reads
the annotation node and a ParenthesizedTypeNode layer defeats it
(GAP `(x: <T>(p: T) => T) => …` whole, WRONG `any` for the inner
callback's own print). The suspected one-arm fix: unwrap
ParenthesizedTypeNode layers wherever the road reads a written
function-type annotation — upstream's getTypeFromTypeNode does it
implicitly because parens are transparent in type resolution.
Sizing input recorded in TASK (the 87-line bucket, spread); bar
prediction to be REGISTERED after one trace confirms the paren
layer is the decline (TSR_CTX_DEBUG on the head case). Entry:
crates/tsr-checker/src/contextual.rs's parameter road.

[Renumbered §113→§114: checker-1's ALIAS re-price bar (2885500) precedes
the ctx bar (6151e57), ancestry verified both directions. The commit
messages naming ctx-§113 are immutable and stale.]

**§114 bar CORRECTED before any code (the trace contradicted the
suspicion).** The head case read, not skimmed: FuncType is
`(x: <T>(p: T) => T) => typeof x` — a typeof-own-parameter RETURN —
and `fun` is an OVERLOADED GENERIC taking FuncType callbacks; the
failing positions are arrows contextually typed through overload
selection over generic candidates, which is the §33/§35 refused
territory, not a ParenthesizedTypeNode unwrap. The one-arm
suspicion was written from the annotation TEXT without reading the
case — exactly the sizing sin the conventions catalogue (level 1
bucketing), caught here by the mandatory trace before code. The
87-line bucket therefore needs PER-CASE TRIAGE before any mechanism
is claimed: callWithMissingVoid, taggedTemplateContextualTyping1,
dependentDestructuredVariables, and contextuallyTypeAsyncFunction-
ReturnType each carry their own shape. §113 stays a bar-in-triage;
no prediction registered, no code owed against it yet.

**§114 triage, second case (callWithMissingVoid, 5+ lines):** NOT
the contextual-parameter machine either — two families:
  1. WRITTEN UNION ORDER WITH VOID (the 4 WRONGs): want
     `x: number | void` as written, we print `void | number` — the
     fresh sort puts void first. This is §108's admission principle
     verbatim ("a spelling the fresh render cannot reproduce") on
     the printing lane; flagged to checker-1 as its §108 family's
     next candidate rather than claimed here.
  2. GENERIC INSTANTIATION AT void (the GAPs): `f<void>(...)`-class
     explicit-argument instantiation producing `(t: void) =>
     { a: void; }` — the §35-family deferred-instantiation road.
The 87-line bucket is now measured HETEROGENEOUS across its top
three cases (overload+generic dispatch / union order / void
instantiation) — the "one road" reading of the calleegap bucket was
level-1 bucketing and the bar stays in triage; remaining cases
(dependentDestructuredVariables 8, taggedTemplate 5) owe their
looks before any §113 mechanism is claimed.

**§114 triage COMPLETE — the 87-line bucket is FOUR families:**
  1. OVERLOAD+GENERIC CONTEXTUAL DISPATCH (the largest): both
     parenthesizedContexualTyping2 AND taggedTemplateContextualTyping1
     carry the identical FuncType shape (`(x: <T>(p: T) => T) =>
     typeof x` through overloaded generic callees) — one family, two
     syntactic entries (call and tagged template).
  2. DESTRUCTURED-DISCRIMINANT OPTIONALITY
     (dependentDestructuredVariables 8): contextual types leak
     `| undefined` into destructured members (`string | number`
     wants, `| undefined` got) plus one exhaustive-`never` want —
     the §50 family's optionality half, its own machine.
  3. WRITTEN UNION ORDER WITH VOID — routed to and CLAIMED by the
     printing lane's §108 umbrella.
  4. VOID INSTANTIATION (`f<void>()`) — the §35 deferred road.
Family 1 is the claimable head: two cases, one shape, and the
FuncType fixture is deliberately built so that any partial
contextual typing shows up as `any`-invoked-with-type-arguments
errors — upstream's own test design isolates the mechanism. The
next window's bar targets family 1 with the §68-family method.

**§114 family-1 bar (registered; the build is the next window's):**
the mechanism is a GENERALIZATION OF LANDED MACHINERY, not new: §70
(overload-agreement contextual argument, +68, with the id-walk
mention test that survived its own falsifier) already decides when
an overloaded callee supplies a position's context — the family-1
arm extends `get_contextual_type`'s argument case from
single-signature callees to §70-AGREEING overload sets whose agreed
parameter type mentions no callee type parameter (FuncType mentions
no T of `fun`, so the id-walk admits it), which then flows through
`contextual_signature` → the §75 uninstantiated-generic pass into
the arrow's parameters. Free falsifier, upstream's own design: the
fixtures invoke contextually typed values WITH TYPE ARGUMENTS, so
`any` (contextual typing absent) errors and `<T>(p: T) => T`
(present) resolves — partial credit is impossible. Predict the two
FuncType cases' family (~40–70 lines across
parenthesizedContexualTyping2 + taggedTemplateContextualTyping1 +
the fun-family tail); must NOT move: §70's own converts, §75's, the
§93 nil-ladder. Tagged templates need the template-tag argument
mapping (span 0 is the strings array) — if that mapping is absent
the tagged half stays gapped and says so.

**§114 family-1, first probe: a MEASURED ZERO, reverted per the §34
precedent.** The proposed arm turned out to ALREADY EXIST — §70's
agreement generalization sits in `contextual_type_for_argument`
citing this very family, and the mention walk correctly skips
re-bound inner generics (a generic nested signature contributes no
mention). The suspected remaining decline — `contextual_signature`'s
Anonymous-only fallback missing alias-named function types — was
built and measured ZERO transitions on the head case: correct in
principle, unreached in fact, reverted. THE NEXT TRACE therefore
starts with instrumentation at THREE points on
parenthesizedContexualTyping2: does `contextual_type_for_argument`
fire and answer FuncType; does `contextual_signature` receive it;
which road consumes the arrow if neither. The §93 nil-ladder's
has_no_contextual_type may be answering true (standalone-typing the
arrow as `any`) BEFORE the contextual road is consulted — check the
consultation ORDER first; the §68-family dispatch decides who asks
whom.

**§114 family-1, trace round 2 (all probes reverted, findings
banked):** (a) the dispatch's ParenthesizedExpression arm EXISTS at
contextual.rs:435 — the module doc's "seventeen arms not here" list
still names it as rejected-on-concentration and is STALE; correct
it when the family lands. (b) The alias-signatures fallback is
genuinely UNREACHED even with parens flowing (two independent zero
measurements) — FuncType's contextual TypeId is consumed by the
Anonymous road, so the alias theory is dead. (c) The instrumented
run's shape: ARG-ARM answers FuncType 8×, CTX-SIG receives it 10×
and still the arrow prints `any` — THE THIRD POINT is the live one:
`get_contextually_typed_parameter_type` / the §75 pass between a
received contextual signature and the arrow's parameter types.
NEXT: one instrumented run at contextual.rs:161's road on the head
case; the consumer between CTX-SIG Some and the `any` print is the
family's whole remaining question.

**§114 family-1, trace round 3 (banked; instrumentation reverted):**
point 3 is EXONERATED end to end — the P3 zero was my own
instrumentation asymmetry (the print covered only the None side; a
zero from an instrument that cannot see success is not a zero — the
conventions' own rule, self-applied), and reading the extraction
shows the road answers: contextualisable_parameters passes,
contextual_signature receives FuncType (10× measured), and the
positional extraction (`parameters.get(index)`, non-optional
non-rest) returns the callback type. EVERY stage of the contextual
chain now measures or reads as working — so the `any` wrongs must
sit in a CONSUMER this trace has not yet identified. Round 4's task
is position identification, not mechanism theory: dump the case
WITH walker position indices, name which node each want-`<T>(p: T)
=> T`/got-`any` line belongs to (the arrow? its parameter's
REFERENCE inside the body? the call result?), and instrument THAT
node's road. The §116 lesson candidate if it lands: four rounds of
mechanism theory, and the family's blocker was never once in the
roads the sizing named.

**§114 family-1, round 4 — THE MECHANISM, found by position:**
PARAM-SYM instrumentation (28 firings on the head case) shows the
contextual road answering for every agreeing position and None for
THIRTEEN parameters that cluster precisely where §70's agreement
test declines: the MIXED-ARITY overload pairs (`fun(f, x)` /
`fun(f, g, x)`) disagree at index 1+ — candidate 1's position 1 is
its own `T` (correctly declined by the mention walk), candidate 2's
is FuncType — so agreement fails and the WHOLE position gets no
context. Upstream never asks for agreement: it contextually types
through the RESOLVED signature (`getResolvedSignature`, memoized).
THE BUILDABLE SLICE — ARITY SELECTION: in
`contextual_type_for_argument`'s §70 arm, when candidates disagree
at the index, select the SINGLE candidate whose parameter count
equals the call's argument count (upstream's own first overload
discriminator); its parameter — still guarded by the mention walk —
is the context. `fun(f, x)` → 2 args → candidate `(f, x)`;
`fun(f, g, x)` → 3 args → candidate `(f, g, x)`. Exactly-one-match
required; ties decline as today. Four rounds, and the family's spec
is now one arm with a measured population: the thirteen None
positions plus their downstream prints. Next window BUILDS this.

**§114 family-1, round 5 — two arm shapes measured, both reverted,
the selection semantics now pinned by data:**
  - ARITY-FIRST (override): +16 (4 G→R, 12 W→R) against 6 R→W in
    the head case — the wins are real and the mechanism close, but
    six positions relied on the §70 loop's strict declines
    (standalone-typing was RIGHT there) and the override consumed
    them.
  - DISAGREEMENT-GATED: measured ZERO — unreachable, because the
    §70 loop's Nones on this family are MENTION-driven (candidate
    1's index-1 is its own `T`, declined before any disagreement is
    seen), never `Some(_) != Some(_)`.
  The next design must therefore be PER-CANDIDATE: arity-select the
  candidate FIRST (upstream's resolution order), apply the guards to
  THE CHOSEN CANDIDATE ALONE — and the six R→W positions need
  naming (which walker lines, which calls) before the chosen-only
  guards can be trusted; if the six are all positions where the
  chosen candidate's parameter passes the guards but the answer is
  still wrong, the discriminator needs applicability, not arity.
  Population confirmed at +16-vs-6 scale on the head case alone;
  the family remains the board's best-instrumented open head.

**§114 family-1, round 6 (banked; reverted):** the six R→W
positions are NAMED — all six are arrows inside TERNARY arguments
(`fun((cond ? arrow1 : arrow2), arrow3)`, positions 0:201-206 and
0:235-237), where upstream's inference-phase context-sensitivity
skip answers `any` while direct parenthesized arrows convert. A
paren-unwrap-to-conditional guard on the arity arm was built and
MEASURED NOT FIRING (6 R→W unchanged) — the branch arrows receive
their context through a dispatch path the guard never sees (not
via the argument arm's `argument` node). Round 7's single question:
one eprintln in `get_contextual_type` printing the PARENT KIND for
the ternary-branch arrows names the transmitting arm; the guard
then moves to that arm's conditional case. The +16 win population
and the six-line adverse are both stable across three arm shapes —
the family lands the day the transmitting arm is named.

**§114 family-1, round 7 (banked; reverted):** the transmitting arm
is NAMED — `get_contextual_type`'s dispatch HAS a
ConditionalExpression arm and it fires for the ternary-branch
arrows (2× on the head case; GCT parent-kind census: 17
paren-parents, 4 call, 4 property-assignment, 2 conditional). The
guard belongs in that arm, gated to call-argument-derived context
only (annotation-derived ternary context must keep flowing — §68
wins depend on it). ONE PUZZLE remains, round 8's single command:
pre-arity-arm these branches printed `any` (RIGHT) even though
index-0 agreement SUCCEEDS for the 2-arg call — so the conditional
arm's RESULT was None for a reason the arity arm then changed;
print that arm's result with and without the arity arm before
placing the guard, or the guard may mask a different mechanism.
Seven rounds: every arm in the chain is now individually measured;
the family is one result-print plus one guard placement from its
pair.

**§114 family-1, rounds 8-9 (banked):** round 8 INVERTS round 7 —
there is NO ConditionalExpression arm in the dispatch (zero grep
matches; the two GCT firings fell to the default None), and round
9 finds NO consumers of get_contextual_type outside the module. So
no road exists by which the arity arm could retype the
ternary-branch arrows — every chain to them dies at the
conditional-parent None, pre-arm and post-arm alike. The six
"R→W" are therefore SUSPECTED ALIGNMENT ARTIFACTS of the twelve
adjacent wins: the walker aligns by subject text, our texts at the
converted positions changed, and the §87 caveat (recorded at
unicodeEscapesInJsxtags: "any single-case ±16 there is noise")
describes exactly this. VERIFICATION, one command before landing:
dump the case pre- and post-arm and diff the SUBJECT texts at
0:202/204/206/236 — if the subjects differ between runs, the six
are artifacts, the arity arm's real score is +16/0, and it LANDS
(gates + isolated pair as usual). If the subjects match, the six
are real and the transmitting road is still unfound — refuse per
the bar. Nine rounds; the family is one diff from its verdict.

**§114 family-1 — LANDED after nine trace rounds and the deciding
diff.** The arity arm (contextual_type_for_argument: when §70's
agreement declines on disagreement, the single candidate whose
parameter count equals the call's argument count decides, same
guards): isolated full pair right 408,314 → **408,346** — +4 G→R
+34 W→R (parenthesizedContexualTyping2 12, callWithMissingVoid 8,
typeGuardTypeOfUndefined 8, tail) against 8 adverse, ALL in the
head case and ALL of the CONFIRMED artifact class: the deciding
diff showed the want-texts at the fixed indices changing between
runs (`x : any` → bare `any`) — the aligner re-pairing around the
twelve in-case wins, the §87 unicodeEscapes precedent exactly, now
with the artifact test written down: A FIXED INDEX WHOSE WANT TEXT
CHANGES BETWEEN RUNS IS THE ALIGNER MOVING, NOT THE ANSWER.
Nine rounds' lessons, banked where they fired: an instrument that
prints only one side reads zero (round 3, self-caught); a stale
module doc claimed an existing arm absent (round 7-8 inversion);
and the transmitting-road hunt ended with NO road — the adverse
was never semantic. The §114 record is the project's most complete
trace archive; read it before any future contextual-arc build.

**§114 attribution addendum (open, flagged by checker-1):** the
+34 W→R names callWithMissingVoid 8 + typeGuardTypeOfUndefined 8 —
the same 16 lines §108.1 (adad333) converted in ITS isolated pair
hours earlier. Both measurements are attested: RIGHT at adad333
(checker-1's +22/0 pair) and WRONG at e6a7b2a7^ (this build's
baseline, accepted clean on post-adad333 main — the pair could not
otherwise have shown them W→R). If both hold, an UNSEEN R→W
REGRESSION landed in the intervening range and §114 re-converted
its lines; the intervening checker-code landings are the bisect
range (checker-2 landed only docs there — §112 was refused whole).
The total right 408,346 is measured and unaffected; only the
per-build names move. Bisect owner: whoever's landing falls in the
range once enumerated — enumerate first, then the artifact-test
dump at each candidate. Until resolved, §114's per-case list
carries this note and §108.1's does too.

**Destructured-optionality trace (one look, banked):** Action2's
members DECLARE `payload: number | undefined` / `string |
undefined`, and the want at the destructure (`payload : string |
number`) strips the undefined — this is not an optionality leak in
our roads but upstream's FLOW NARROWING AT THE DECLARATION of
destructured members (the f22 "parent-flow-at-declaration" head
TASK has carried since the §82 era, confirmed now with its exact
shape). The family is flow-machine work, not a contextual arm;
it stays with the §50-family board entry, and the "our roads add
undefined" reading from the §114 triage is CORRECTED — our answer
matches the declared type; upstream's narrowing is what we lack.

## §115 — the dispatch's ConditionalExpression arm [checker-2, bar]

The §114 rounds PROVED the arm absent (round 8's zero grep). The
arm is upstream's one-line recursion: a ternary BRANCH answers the
conditional's own context, the CONDITION answers nil
(`getContextualTypeForConditionalOperand`, `checker.go:30022`) —
the §94 nil-ladder already models the condition side in
has_no_contextual_type; this is the positive twin. Risk, named from
§114's fixture: OVERLOAD-ambiguous callee ternaries want `any`
upstream (the inference-phase skip) — if the arm re-creates the six
artifact positions as REAL adverse, the §114 artifact test
distinguishes them in one dump, and an overloaded-callee gate (the
arm declines when the transmitted context arrives from a
multi-candidate callee) is the priced fallback. Predict +10–40
(annotated-ternary spillover: `var x: F = cond ? arrow : arrow`
family); must NOT move: §114's converts, §68's return-position
family. Adverse over 1:5 refuses per standard.

**§115 score — LANDED at +98/0.** right 408,346 → **408,444** on
the full pair: **+94 W→R +4 G→R, ZERO adverse in any column** —
conditionalOperatorWithoutIdenticalBCT 32, WithIdenticalBCT 20,
contextualTypingOfConditionalExpression 18, wide tail. The bar's
named risk (re-creating §114's artifact positions as real adverse)
did not materialize: the callee arm's own guards decline the
overload-ambiguous transmissions before this arm can relay them.
Above the +10–40 prediction — the conditional-operator BCT families
were not in the sizing, the §96-lesson shape again (favorable
direction). One dispatch arm, ten minutes, +98: the §114 nine-round
archive is what made it ten minutes.

**§114 attribution addendum, round 2 — e42c32d9 EXONERATED by the
worktree dump.** The four §108.1-family keys read RIGHT at
e42c32d9 (`number | void` written order intact), WRONG at
e6a7b2a7^ (my attested baseline): the regression window narrows to
e42c32d9..e6a7b2a7^, whose checker-code commits are the four
remaining third-session diagnostics landings (856e3a09 TS2313,
7e9236c3 TS7031, ac112aae TS2661, d53b8a03 TS1042) plus any
checker-1 commits in the span. TWO INSTRUMENT SEAMS caught en
route, both conventions-grade: the worktree dump first judged
NOTHING (TOTAL 0) because `ln -sfn` into an EXISTING directory
creates the link INSIDE it — the `git worktree`/submodule seam's
`ln` sibling, now recorded; and rtk swallowed the empty result's
distinction from a no-output success twice. Bisect continues at the
narrowed range's midpoint; the conventions entry (re-pair ledger
disputes against the last known-good attestation) is confirmed
earned and should land with the bisect's conclusion.

**§114 SCORE CORRECTED — the artifact test applied to my own gain
column.** The bisect concluded with TWO independent instrument
chains in full agreement (checker-1's four checkout+dump points and
my own worktree probes at e42c32d9, 7e9236c3, d53b8a03, 0a56e089,
and decisively e6a7b2a7^ itself): the §108.1 keys were RIGHT at
EVERY point — there was NO regression, and §114's claimed
"callWithMissingVoid 8 + typeGuardTypeOfUndefined 8 W→R" were MY
OWN §87-class aligner artifacts on the GAIN side, exactly
symmetric with the six R→W the same pair produced. §114's true
per-name delta is ~+22 (the head-case family and G→R stand); the
measured total right 408,346→408,450 arithmetic is unaffected —
only the names move, §108.1's +22 stands whole, and no regression
entry is owed. THE SHARPENED RULE, earned by this correction: **the
artifact test applies to GAIN columns too — a W→R in a case your
change didn't touch is as suspect as an R→W**, and a pair's
per-case rows are attributions, not measurements, until the
untouched-case rows pass the fixed-index dump. My §114 landing
message and STATUS row carry the stale +38; corrected here and in
STATUS, messages immutable.

## §116 — the dispatch's ArrayLiteralExpression arm [checker-2, bar]

The "seventeen arms not here" doc rejected this arm because "every
one of them needs tuple types this port does not have" — written
before §37/§79/§80/§105 built exactly those. The arm
(`getContextualTypeForElementExpression`, `checker.go:29380/29972`):
an array-literal ELEMENT answers the array's own contextual type's
element type — positional through a TUPLE context
(tuple_element_lists), the plain element type through an ARRAY
context (type_reference_targets on the Array target), nil past a
tuple's length; spreads decline the whole literal (the index
becomes meaningless — the §114-family precedent). Predict **+15–60**
(the old census: 33 contextual functions over 15 files, top 10
files 85% — plus everything tuple contexts gained since); must NOT
move: §63's tuple-context literals (the array's own print road —
this arm feeds ELEMENTS, not the literal), §115's converts, §68's
family. Adverse over 1:5 refuses; artifact test on any
untouched-case row per the new conventions rule.

**§116 — MEASURED ZERO, reverted per the §34 precedent.** The arm
was built complete (tuple-positional + array-element + spread
decline) and the full pair moved NOTHING. The lesson is in the
population, not the arm: `get_contextual_type`'s consumers are
FUNCTION-contextualization only (contextual_signature + the
parameter road), so the arm could only ever serve arrows/function
expressions sitting directly inside array literals under
tuple/array contexts — and that population, whatever the old
33-function census counted, is drained or trivial at today's
baseline (§63 took the literal side long ago). The old census
counted CONTEXTUALLY TYPED FUNCTIONS, not convertible lines — the
kind-1/kind-2 conflation, again. A future array-element context
consumer (object literals in arrays, the §56-family's walk) would
route through symbols.rs's walk, not this dispatch — that walk
already has its own array handling. DO NOT rebuild this arm without
first naming a consumer that reaches it.

## §152 — the assignment arm: `g = (a) => a` takes the left's type [claimed: checker-1]

The probe that opened this (SS152 repro): variable-annotation
arrows type their parameters; the SAME arrow under plain `=`
declines — `get_contextual_type` has no BinaryExpression arm.
Upstream: `getContextualTypeForBinaryOperand` (`checker.go:29809`)
— the RIGHT operand of `=` answers the LEFT operand's TYPE. The
§98 walk (symbols.rs) already built this road for object-literal
members under assignment, with its reentrancy guard
(narrow_value_stack on the holder) and the JS-file decline
(module.exports carve unmodelled); this arm is the same rule at
the contextual-dispatch site, serving arrows/function expressions.
Boundary per checker-2's custody agreement: dispatch ARM only, no
gate-condition changes. **Bar: ≥30 net at ≥5:1.** Falsifiers:
(a) compound assignments (`&&=`, `||=`, `??=`) are NOT the plain
arm and decline; (b) self-referential lefts (`f = () => f()`)
must not cycle — the guard's job, watch for stack growth;
(c) JS files decline whole (§98's carve).

**Score: +167 / 12 adverse (13.9:1) — LANDED.** 78 G→R + 89 W→R:
assignmentCompatBug2 32, contextualTyping 40 across both columns,
targetTypeTest1 12, generatedContextualTyping 6, the rest spread
thin — the §116 lesson inverted: this arm HAS a consumer
population because assignment-positioned arrows are everywhere.
The 12 adverse (thisTypeInFunctions 10, looseThis 2): lefts whose
signature carries an explicit `this` parameter — the contextual
signature types the arrow but the arrow's PRINT drops the
inherited `this` (`(this: void, x: number) => number` wanted,
`(x: number) => number` printed). Priced within ratio; the
this-carriage into contextual arrow prints is a named residue,
NOT a falsifier firing — falsifiers (a) compound ops and (c) JS
were exercised by the corpus and held.

## §153 — the PropertyDeclaration initializer context [claimed: checker-1]

The VariableDeclaration arm's own comment named this gap: a CLASS
property's annotation is the same upstream road
(`getContextualTypeForVariableLikeDeclaration`, `checker.go:29438`)
and was left unmeasured. Probed (SS153 repro): `f: (x: number) =>
string = (x) => "a"` answers error/any while the variable and
object-literal-member shapes both type. Arm: PropertyDeclaration
with a type node → the annotation's type, initializer position
only. ParameterDeclaration (annotation + default) rides if free.
**Bar: ≥25 net at ≥5:1.** Falsifiers: (a) computed-name
properties decline; (b) static/instance makes no difference to
the rule — if the pair says otherwise, split; (c) accessors are
NOT this arm.

**Score: +58 / 1 adverse (58:1) — LANDED.** 27 G→R + 31 W→R,
generatedContextualTyping carrying 42 of them plus a 4-line
spillover into hasInstance narrowing (a contextually typed
property arrow now types its parameter, and a downstream guard
sharpened). The 1 adverse (classPropertyErrorOnNameOnly): a
property whose ANNOTATION errors now hands the error to its
initializer's context where the gap previously hid it — the same
file gained 2 W→R, net positive inside itself. Falsifiers unfired.

## §154 — assertion-family contexts: `as T` and `satisfies T` [claimed: checker-1]

Probed (SS154): an arrow or object literal under `x as T` /
`<T>x` / `x satisfies T` gets NO context — all three decline into
error/any while the annotation road types the same shapes.
Upstream: `getContextualType`'s AssertionExpression arm answers
the asserted TYPE (`checker.go` assertion arm; a CONST assertion
answers nil — `as const` is not a type context), and
SatisfiesExpression answers its type node the same way. Two
dispatch arms, both `get_type_from_type_node`. **Bar: ≥25 net at
≥5:1.** Falsifiers: (a) `as const` must keep §105's road
untouched (it is isConstContext's business, not a contextual
type); (b) assertion-to-any (`x as any`) must not manufacture
member types the gap correctly withheld.

**Score: +70 / 0 adverse — LANDED.** 37 G→R + 33 W→R:
contextualTyping 35, castTest 12, typeSatisfaction 5,
objectLitGetterSetter 4, the tail spread. `is_const_type_reference`
went pub(crate) to share the §105 detector rather than duplicate
it. Falsifiers unfired — `as const` declines through the shared
detector, and no assertion-to-any manufacture appeared in the
pair. The arc's running total (§152+§153+§154): +295 net across
three dispatch arms, each a one-read landing — the §116 lesson's
complement: arms with NAMED consumer populations land at ratio.

## §155 — NewExpression argument context through the arity road [claimed: checker-1]

Probed (SS155): call arguments type through
contextual_type_for_argument; NEW arguments decline (`new K((n) =>
"x")` answers error/any beside the identical call shape typing).
Upstream reaches both through resolveCall; this port's §90 arity
road already extracts THE SOLE NON-GENERIC CONSTRUCTOR's written
annotations positionally (`sole_constructor_parameters`, base-hop
and overload rules measured in call_arity.rs). The arm: reuse it —
argument index → annotation → type_from_annotation_id, declining
wherever check_argument_types is false (generic/overloaded), no
type_arguments, spreads decline positionally by the same rule the
arity check uses. **Bar: ≥20 net at ≥5:1.** Falsifiers: (a) the
recursion the module doc guards (resolve-while-checking) is never
entered — the arity road reads declarations, not signatures;
(b) derived-class constructors through base hops carry the §343
two-endings rule — a fired falsifier there shows as wrong
parameter types in derived `new`s.

**Score: +5 / 0 own-lane — the ≥20 VOLUME BAR MISSED, landed
with the miss stated.** The pair's raw read (+43) carried
checker-2's §149a (+38 hasInstance) arriving through the rebase —
attributed and excluded. The own-lane 5 (contextualTyping 2,
targetTypeBaseCalls 3): new-positioned function arguments are
simply RARE — the module doc's original census never listed
NewExpression among the ranked arms, and the census was right.
Kept rather than reverted per the zero-adverse/shared-machinery
argument (20 lines, no new surface: the §90 arity road already
owns every rule the arm uses); the §116 revert precedent is about
ZERO movers, and this moved. The lesson for the arc: the ranked
census (module doc's table) remains the bar-setter — arms outside
it should be priced small BEFORE barring, not after.
