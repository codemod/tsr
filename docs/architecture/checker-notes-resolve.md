# Call resolution notes: choosing among overloads

Living notes for `crates/tsr-checker/src/calls.rs`. Anchored to
`vendor/typescript-go` @ `5b1047d10`.

## The forcing constraint

`CallExpression` is the largest single form in the `.types` gap, and the
overload set is its second-largest blocker: 357 of the 1,496 corpus declarations
initialised by a call to a locally declared `function` — 24% — have a callee
with two or more call signatures. Until this change, every one of them answered
`errorType` at the `_` arm of `resolve_call_signature`.

That arm only became live in `6b701cc`. Before an overload set had a printed
type, the callee was already `errorType` and the call never arrived.

## Concentration: this is a corpus-wide shape, not one file's

Measured over `vendor/typescript-go/testdata/baselines/reference/submodule`, by
extracting the source lines from every `.types` baseline, collecting the
`function` names declared more than once in a file, and counting call sites of
those names:

| | |
|---|---|
| Call sites of a locally declared overloaded `function` | 779 |
| Files containing at least one | 189 |
| Largest single file (`compiler/genericFunctionInference1.types`) | 40 |
| Share held by the top ten files | 26% |

Worth stating plainly because the opposite result would have changed the
decision: several rows in this corpus that look like broad wins are one test
file, and porting for them is porting for a fixture. This one is not.

## What separates the candidates, and therefore what had to be ported

Classifying the same call sites by how their candidate set is distinguished:

| Shape | Sites |
|---|---|
| Non-generic, candidates differ by **parameter type** | 523 |
| At least one **generic** candidate | 84 |
| Non-generic, candidates differ only by **arity** | 59 |

Arity filtering alone would have reached 59 sites. The dominant shape needs
assignability, which is why `is_type_assignable_to` — not an arity heuristic —
is what `choose_overload` is built on.

## The decision: selection only where a *negative* answer is trustworthy

`chooseOverload` (`internal/checker/checker.go:9025`) returns the first
candidate, in declaration order, that `hasCorrectArity`
(`internal/checker/checker.go:9107`) admits and every argument is assignable
to. Declaration order is the entire tie-break.

Porting it exposes something no previous caller of the relater depended on:
**overload selection is the first place in this checker where a `false` from
`is_type_assignable_to` changes the answer.** Every earlier caller only acts on
a `true`. The relater's own module docs warn that two distinct object types
answer `false` because structural comparison is narrow, not because they are
unrelated. Under selection, that false negative does not degrade to a gap — it
*promotes the next candidate*, and produces a confidently wrong type.

So selection runs only over the domains where `isSimpleTypeRelatedTo` decides
the relation on flags alone, the `SELECTABLE` constant in `calls.rs`: `string`,
`number`, `bigint`, `boolean` and their literal types, `void`, `undefined`,
`null`, `never`, and unions of those. Anything else — in **any** candidate's
parameters or in any argument — makes the whole call a gap.

This is narrower than upstream's `TypeFlagsPrimitive`. `ENUM_LIKE` and
`ES_SYMBOL_LIKE` are excluded (an enum literal's relation runs through its
declared type, a unique symbol's through its declaration), as are
`TEMPLATE_LITERAL` and `STRING_MAPPING` (unported).

### Alternatives taken seriously

- **Take the first candidate.** Rejected. It answers something for all 779
  sites and is wrong wherever a later candidate wins — which, given that 523 of
  them are separated by parameter type, is most of them. A wrong answer scores
  the same as a gap on the coverage metric and is strictly worse, because a gap
  is an honest "don't know" the instrument can bucket.
- **Filter by arity only, gap when arity is ambiguous.** Rejected: it reaches
  59 sites, 7.6% of the shape, and it would have been indistinguishable from the
  full port on any fixture whose overloads happen to differ in arity — the exact
  test that proves nothing.
- **Run assignability over all types and accept the false negatives.**
  Rejected for the reason above. **This is the option that wins if the relater's
  structural comparison becomes complete**: `SELECTABLE` exists solely to bound
  an incomplete relation, and it should be widened (or deleted) the moment the
  relation is trustworthy over object types. That is the falsifier for this
  decision.

## Consequences accepted

- Object-typed overload sets — a large share of the 523 — remain gaps.
- Upstream's **subtype** and **strict-subtype** passes are not ported. They
  exist to prefer a more specific candidate when several match. Where more than
  one candidate matches with *different* return types, `choose_overload`
  answers `None` rather than guess which pass would have won; where every match
  returns the same type, the passes could not have changed the answer, so it is
  taken.
- Upstream's error-reporting fallback — report against the candidate with the
  fewest problems, and answer that candidate's return type — is not ported. A
  call no candidate accepts is `errorType`, not the nearest miss.
- Generic candidate sets, spread arguments, and rest or `this` parameters are
  all gaps.
- A tagged template with an overloaded tag stays a gap:
  `resolve_call_signature` takes `Option<&[Expression]>` and the tagged-template
  path passes `None`, because a tag's arguments are the template strings array
  and the substitutions, neither of which this port builds.

## A record corrected

`tests/types.rs::a_call_this_slice_cannot_resolve_is_a_gap_and_not_the_first_candidate`
asserted `error` for

```ts
declare function g(): number;
declare function g(a: string): string;
const x = g();
```

That call now resolves to `number` by arity, which is what upstream answers.
The old assertion was pinning the *absence of a mechanism*, not an answer — the
kind of assertion that has to be replaced rather than defended when the
mechanism arrives. It has been replaced with an object-parameter set, which is
still a gap for the reason above, so the test keeps its original job of
catching a first-candidate implementation.
