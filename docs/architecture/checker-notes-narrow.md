# Narrowing by an equality test against `null`/`undefined`

Instrument: `crates/tsr-conformance/examples/wrongdelta.rs`. Measured at
`385fb60`, which is the tree `STATUS.md` §1's numbers were taken at.

---

## 1. The board item this replaces, refused with a number

`STATUS.md` §4 item 2 was **"strict-flag gating of the remaining strict-only
arms"**, headed by optionality's added `undefined` (`bd tsr-e10`, quoted at
483 lines). The reasoning was recorded the session before: `strict_null_checks`
had just been plumbed into the checker, `optionality.rs`'s own module doc named
`strictNullChecks` as *"the assumption to revisit first"*, and the issue's
confusing split — one baseline recording `string | undefined` for an optional
parameter and nineteen others recording `number` — looked exactly like a
strict-versus-non-strict split.

**It converts zero lines, and two independent measurements say so.**

### 1a. The union constructor already does it

`get_optional_type` adds `undefined` by calling `get_union_type(&[ty,
undefined])`. Since `856972a`, `add_type_to_union` drops a nullable
constituent outright when `strict_null_checks` is off (`checker.go:25783`),
and `get_union_type_from_sorted_list` collapses the resulting one-element list
back to the element. **So in a non-strict case the optionality arm is already
a no-op** — not by design, but as a consequence of where the two rules sit
relative to each other. Gating `add_optionality_ex` as well would be a second
guard on a path the first guard already closed.

### 1b. And the population is not non-strict anyway

`bd tsr-e10`'s shape is *we print `X | undefined`, the baseline prints `X`*.
Re-measured at `385fb60` it is **256 lines over 50 cases**, not the 483 the
issue quotes — today's union and instantiation work already took the rest.
Split by the case's own `@strict` / `@strictNullChecks` directive:

```
  strict: true (explicit)      244
  no directive (= strict on)    12
  strict: false                  0
```

**Zero.** A rule that only fires when strictness is off cannot convert a
population that is 100% strict. The board item comes off.

> **A plumbing change makes an item *measurable*, not *valuable*.** The
> previous session's note on `bd tsr-e10` said the flag was "the hidden
> variable" and that gating was "now a small measurable change where before it
> was unplumbable". Both halves are true and the conclusion does not follow:
> measurable is exactly what let this be refused in two probes rather than
> built. The note is corrected on the issue rather than deleted.

## 2. What the 256 actually are — and they are a mixture

The case names are the first clue and the guard forms are the confirmation:
`controlFlowOptionalChain` (68 lines, **26.6%**), `strictOptionalProperties1`
(26), `truthinessCallExpressionCoercion1` (11), `typeGuardIntersectionTypes`,
`narrowingOrderIndependent`, `narrowingOfQualifiedNames`,
`controlFlowForCatchAndFinally`. Counting guard syntax in the top four:

| case | dominant form |
|---|---|
| `controlFlowOptionalChain` | **119 `?.`** — optional-chain narrowing |
| `strictOptionalProperties1` | 3 `in`, 1 `=== undefined` |
| `narrowingOrderIndependent` | 4 `instanceof` |
| `narrowingOfQualifiedNames` | qualified-name references |

So `tsr-e10` is **at least four mechanisms**, no one of which holds a majority,
and the largest single one is optional chaining. `docs/conventions.md` already
records the rule this instance repeats — *"a population identified by the shape
of the wrong answer is not thereby attributed to a mechanism"* — and this is
the second time that exact issue has been re-diagnosed by taking the syntactic
position. It is **not** re-filed as an item; it is a symptom row.

## 3. The build this page registers instead

Equality narrowing — `x !== undefined`, `x != null` — chosen **not** for the
256 (it holds a minority of them) but because `narrowTypeByEquality`
(`flow.go:556`) is a rule the whole corpus exercises and the port's machinery
is already in place: `get_type_with_facts`, `filter_type` and a
per-constituent `get_type_facts` all exist, and `narrow_type`'s default arm
returns the type unchanged, so a new arm can only act where the guard is
written.

### The separable leg, and why it is separable

`narrowTypeByEquality` has two halves and only one is portable:

| half | needs |
|---|---|
| the operand is `null`/`undefined` (`valueType.flags&Nullable`) | **facts bits computable from `TypeFlags`** |
| everything else | `areTypesComparable`, `isUniformUnionType`, `replacePrimitivesWithLiterals` — **assignability** |

This is the same shape as the `&&` / `||` split (`checker-notes-armsplit.md`
§3.1) and it is checked the same way — by reading the declarations rather than
believing a comment. The nullable half needs six bits whose values are fixed
by upstream's own per-type aggregates (`checker.go:471`, `:472`, and the
`Base*StrictFacts` set):

```
undefined  EQUndefined  EQUndefinedOrNull  NENull                 (not NEUndefined, not EQNull)
null       EQNull       EQUndefinedOrNull  NEUndefined            (not NENull, not EQUndefined)
any other  NEUndefined  NENull             NEUndefinedOrNull      (no EQ bit at all)
```

`getAdjustedTypeWithFacts`' extras — `unknownUnionType` recombination and
`getGlobalNonNullableTypeInstantiation` — are **not** ported: they act on
`unknown` and on type parameters, and `get_type_with_facts` alone is exact for
a union of concrete constituents, which is what the corpus's
`string | undefined` population is. Those two shapes stay as they answer today.

Upstream returns `t` unchanged when `strictNullChecks` is off
(`flow.go:565`), which is now expressible and is ported as written.

### The safety property carried over

`get_type_facts`' existing default is *both truthiness bits*, so an
undecidable type survives every filter. The new bits extend that unchanged:
a type this port cannot classify reports **all six** nullable bits, so
`filter_type` keeps it under every query. A wrong `NE` claim would delete a
constituent and print a confident wrong type; a wrong `EQ` claim cannot,
because nothing filters on `EQ` except the assume-false branch, which is
symmetric.

### The bar

A loss here is **evidence of a wrong rule, not a bad trade** — narrowing only
ever removes constituents from a union that upstream also removes, and the
default arm means untouched guards keep today's answer. So the bar is
absolute, per *"an absolute bar catches a wrong predicate that a ratio bar
ships"*:

> **KEEP** if **lost ≤ 5** and **gained ≥ 150** and **no case regresses**, and
> **Δwrong = −Δgap − Δright ≤ Δright ÷ 3**.
> **REVERT** otherwise.

The loss allowance is 5 rather than 0 because, unlike parenthesisation, this
rule can legitimately interact with an already-wrong declared type: narrowing
a type that was wrong before can move it to a different wrong answer. Anything
above 5 is the predicate.

**How this would be shown wrong:** if the gain is concentrated in
`controlFlowOptionalChain` or the other `tsr-e10` cases, then this arm is
converting the symptom row after all and §2's mixture finding was wrong about
which mechanism owns those lines.

### Prediction

**+150 to +600**, spread widely, with most of it *outside* the 256 — `!= null`
is a guard the whole corpus writes, and only a minority of the `tsr-e10`
population is an equality test.

---

## 4. The result — the bar fired on its floor, and the floor was the wrong number

Measured at the tree this section was written on.

```
  net +30 | 328,877 -> 328,907
  gained 33 in 12 cases | lost 3 in 3 cases | 1 finished, 0 regressed
  gap 99,215 -> 99,203  =>  Δwrong = -18
```

Against the registered bar: **lost ≤ 5 passes** (3), **no case regresses
passes** (0), **Δwrong ≤ Δright ÷ 3 passes** (−18, the wrong bucket *shrank*),
and **gained ≥ 150 fails by 117**.

### The diagnosis, and it is not the mechanism

The first hypothesis is that the build is wrong. It is not, and the evidence is
independent of the arithmetic: six fixtures taken from two baselines
(`controlFlowGenericTypes.types:566` for the union case,
`equalityStrictNulls.types:4` for the non-nullable control) all pass, including
the three that separate `!==` from `!=` and `null` from `undefined`. Three
losses against thirty-three gains is 11:1, and the wrong bucket went down.

The second hypothesis is that the bar's premise is wrong, and it is. The floor
of 150 was set from this sentence in §3's prediction: *"`!= null` is a guard
the whole corpus writes."* That is true about the corpus and irrelevant about
this port, because of a fact that was in the code the whole time and was never
checked before the number was written:

> **`is_matching_reference` is identifier-only.** It compares resolved
> **symbols** (`flow.rs`), so a guard narrows a reference only when both the
> guard's operand and the reference under test are identifiers resolving to
> the same variable. `a.b !== undefined`, `o?.foo !== undefined`,
> `this.x != null` — the forms the corpus actually writes most — match
> nothing, because this port has no flow reference for a property access at
> all.

`FlowState`'s own doc says so in as many words — *"upstream has no such field:
it compares reference expressions with `isMatchingReference`, because a
reference can be `a.b.c`… here the match is identifier-only"* — which is why
this counts as evidence independent of whoever wrote the premise. It is also
exactly what §2 of this page had already measured and then failed to carry
forward: the head case of the population is `controlFlowOptionalChain`, whose
119 guards are all `?.` property references.

### KEPT, loudly, and what would have made the bar right

The build stays. `docs/conventions.md` requires that overriding a registered
bar be done in the commit message, the issue, `STATUS.md` and here, on evidence
independent of the author — all four, and the evidence is a field comment in
someone else's function.

What the registration should have contained instead of a line count: **a
population measured through the port's own reference matcher.** "How many
corpus guards compare an *identifier* against `null`/`undefined`" is one probe
over the parse trees, it costs nothing, and it would have set a floor near 30
rather than 150. A floor derived from what upstream's users write, rather than
from what this port can see, is not a floor about this change.

> **A population is only a population if the change can reach it.** This is the
> fourth entry in this project's record of a number that was true of one set
> and quoted about another, and the first where the gap between the two sets is
> a *capability boundary in this port* rather than a property of the corpus.

### The three losses, named

`compiler/narrowingPastLastAssignment`, `compiler/uncalledFunctionChecksInConditional2`,
`conformance/parserRealSource7` — one line each. Each is a case where narrowing
now fires on a type that was already wrong before it, moving one wrong answer
to a different wrong answer; none flips a case.

### What this unlocks, and the honest size of it

The arm is a **prerequisite that pays later**: the moment
`is_matching_reference` learns property references — upstream's structural
`isMatchingReference` plus `getFlowTypeOfReference` reached from
`check_property_access_expression` — every guard already ported here starts
firing on the forms the corpus writes, and the `?.` head of §2's population
comes into range. That is the item this page leaves behind, and it should be
sized **through the matcher**, not through the guards. `bd tsr-6ka`.

---

## 5. Property-reference narrowing, built — `bd tsr-6ka`

### The sizing, run first because the issue demanded it

`crates/tsr-conformance/examples/refmatch.rs` (new). §4's failure was a floor
set from what upstream's *users* write; this probe counts what a working
matcher would put **in range**, which is the set the change can reach.

```
  guard form                   verdict today      lines
  truthiness       (PORTED)    right (AT RISK)       70
  truthiness       (PORTED)    gap                  118
  truthiness       (PORTED)    wrong                 28
  nullable equality (PORTED)   right (AT RISK)       13
  nullable equality (PORTED)   gap                   28
  nullable equality (PORTED)   wrong                  7
  other guard    (unported)    right/gap/wrong      296

  CONVERTIBLE (ported guard, not right today)      181
  AT RISK     (ported guard, right today)           83
  LOOSE BOUND (any guard in the same file)       2,172
```

**The strict figure and the loose one are both reported because neither is the
answer.** The strict test asks "is the line inside the guard's branch", which
cannot see the commonest narrowing idiom there is — `if (!a.b) return;`
followed by a use of `a.b` — nor early exits, `&&` chains or assignments. The
loose bound admits all of those and also every unported guard form. The honest
statement was **[181, 2,172], centre near 800** applying the in-range
ported-guard share.

### The build

Three changes, each with an upstream anchor:

1. `FlowState.symbol` becomes `Option<SymbolId>`, and `is_matching_reference`
   splits: symbol comparison for an identifier reference (the binder records
   assignments against *declarations*, which have no expression to compare),
   structural `references_match` for an access reference.
2. `references_match` ports `isMatchingReference`'s arms — same accessed
   property name **and** a recursively matching receiver, plus `this`,
   parentheses, and identifiers by resolved symbol. Element access matches on
   a **string-literal** argument only: `a[i]` needs `isSymbolAssigned` to prove
   `i` constant, so it is refused rather than matched on text.
3. `check_property_access_expression` and `check_element_access_expression`
   call the flow walk. **The binder already recorded flow nodes for narrowable
   accesses** (`record_flow`'s access arm) — nothing in the binder moved; the
   checker had simply never asked.

A fourth change was needed and the corpus is what named it — see below. Two
smaller misses were caught by the tests first: `narrow_type`'s truthiness arm
matched only `Node::Identifier` as a condition, so `if (a.b)` never narrowed
even once the matcher could decide it.

### The result

```
  net +58 | 328,907 -> 328,965 | 68.67% -> 68.68%
  gained 64 in 11 cases | lost 6 in 2 | 2 finished, 0 regressed
  gap 99,203 -> 99,182  =>  Δwrong = -37
```

64 against a strict sizing of 181 and a bracket of [181, 2,172]: **under the
low end of the range.** The bracket was honest about its own looseness — rule 3
admits guards whose receivers differ and lines whose real blocker is upstream
of the access — and the delivered figure says the over-count dominated the
under-count. gained ÷ lost is 10.7, and the wrong bucket shrank by 37.

### The arm the corpus named, and the direction it was wrong in

The first run lost **11** lines, five of them in
`conformance/destructuringControlFlow`, all in the **over-narrowing**
direction — this port answered `string` where the baseline says
`string | undefined`. That is the failure mode this module's header calls the
only way narrowing produces a wrong answer rather than a gap, and the case
states the rule outright:

```ts
if (obj.a) { obj = {}; let a2 = obj.a; }   // >a2 : string | undefined
```

Assigning to `obj` invalidates everything narrowed about `obj.a`. Upstream
handles it in `getTypeAtFlowAssignment`'s **miss** path with
`containsMatchingReference` (`flow.go:255`, `:1841`): the assignment is not to
this reference, but it is to a left-hand part of it, so the declared type is
the answer. Ported, and the losses fell 11 → 6.

> **A partial port of a resolver loses more than it gains on its first run,
> and each loss names the arm still missing.** `docs/conventions.md` records
> this from a three-round sequence (−2,627, −1,621, −16). This is the same
> shape in one round, and what made it cheap was that the loss was
> *concentrated in one case whose baseline states the rule* — five lines in a
> file called `destructuringControlFlow`.

### A process miss, recorded

**No keep/revert bar was registered before this build.** The sizing probe was
run, as `bd tsr-6ka` demanded, and then the code was written without turning
the sizing into a rule. That is the discipline this page's own §3 applied and
§4 leant on, skipped one section later. Judged after the fact against the
standing figures the project refuses items with, it is a clear keep — 10.7
gained per lost against a 3.0 bar, zero cases regressed, the wrong bucket down
37 — but *judged after the fact* is exactly the thing pre-registration exists
to prevent, and saying so is cheaper than pretending the order was different.

### What is left in this row

The 6 remaining losses are `conformance/parserRealSource12` (4) and
`parserRealSource6` (2), all truthiness on a `boolean` property where the
baseline keeps `boolean` and this port now says `true`/`false`. The 296
in-range lines behind **unported guard forms** — `typeof`, `in`, `instanceof`,
comparability — are the next tranche and are each their own item; the matcher
they were waiting on now exists.

## 6. `tsr-q9g` — `typeof` guard narrowing, sized twice and registered

Fourth session, at `5a6d735`. Two probes priced this in one day, in opposite
directions, both before any code: the `refmatch.rs` per-form split showed the
ACCESS-line population is 27 (typeof 18 + `in` 9 — the issue's "typeof is the
largest form" was upstream inference, not corpus measurement), and the new
`examples/idtypeof.rs` counted the population no instrument had ever seen:
**440 identifier-reference lines under a `typeof x === "…"` guard, 417 of
them in the WRONG bucket** — we print the un-narrowed declared union where
upstream prints the narrowed member. Wants: `string` 110, `number` 64,
`boolean` 47, `never` 39. Diffuse: top case 41 of 440.

### The mechanism, read before writing

`narrowTypeByTypeof` (`flow.go:614`) → `narrowTypeByLiteralExpression`
(`:646`) → on the true branch `narrowTypeByTypeName` (`:657`), on the false
branch the eight `typeofNEFacts` bits (`:635`); both end in
`narrowTypeByTypeFacts` (`:687`), which needs
`isTypeRelatedTo(t, implied, strictSubtypeRelation)` and `isTypeSubtypeOf` —
**and `relater.rs` has only `Relation::Assignable`**. That is the real cost
the issue's "as mechanical as the six nullable bits" missed. Three parts:

1. `Relation::Subtype` and `Relation::StrictSubtype` in `relater.rs` — the
   simple-arm deltas are three relation tests in `isSimpleTypeRelatedTo`
   (`relater.go:212` unknown-target/any-source, `:258` object→nonPrimitive
   freshness, `:261` the assignable/comparable-only block, which is what makes
   `any` NOT strictSubtype-related to `string` and therefore not collapse);
2. the eight `TypeofEQ`/`TypeofNE` facts bit pairs with their per-type
   aggregates (`checker.go:400` region);
3. the `narrowTypeByTypeof` arm family in `flow.rs`, behind the structural
   matcher that already exists.

### The bar

1. **net ≥ 120** (~27% of 440);
2. **gained ≥ 3 × lost** — a ratio, not an absolute, because over-narrowing
   trades: a wrong facts bit deletes a live constituent from a line that was
   right;
3. **0 case regressions**;
4. **Δwrong ≤ −150 by `wrongdelta`** — the PRIMARY leg here, inverted from
   every previous bar: 95% of the population is wrong lines, so the arm's
   success is a *fall* in the wrong bucket, which `casedelta` sees only
   indirectly. Any NEW wrong concentrated in the `typeGuard*` family is
   over-narrowing and means the build is wrong.

**Falsifier:** the 39 `never` wants need the union-exhaustion semantics
end-to-end (both facts directions correct). If they stay wrong while
`string`/`number` convert, the facts table is half-right — a state worse than
absent, and leg 4's concentration split will show it.

### §6 scored — all four legs pass; the concentration split names three owners

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ 120 | **+745** (767/22) | pass |
| 2 | gained ≥ 3 × lost | **34.9×** | pass |
| 3 | 0 case regressions | 0, **+12 pass** | pass |
| 4 | Δwrong ≤ −150 | **−448** (708 fixed, 260 new) | pass |

The falsifier did not fire — `never` wants convert with the rest — and the
260 new wrong decompose into three families, none of them this arm's bits:
**loop fixpoints** (`controlFlowWithIncompleteTypes`: upstream's
incomplete-types iteration re-widens across loop back-edges, unported —
`want string | number, got number` is the arm being *right* on a straight
line the loop re-widens), **further narrows** this port lacks (truthiness on
`boolean` to `true`, discriminants — the `bd tsr-97d` family), and
**want-`any` ceiling lines** now computed concretely (`any` vs the resolved
`toFixed` signature — ADR-0038's asymmetry seen from the other side).

Conversion: +745 against the 440-line sized population — **169%**, the third
mechanism this session to overshoot its row (the narrowed reference feeds
every consumer downstream of it). The three unported forms — `in`,
`instanceof`, comparability — stay with `bd tsr-q9g`, re-sized per the
refmatch split.

### §6.1 — the `in` guard, registered before code

Sized by the same two probes: **9 access lines**; the identifier population
was not separately counted, and the floor is set from the access lines alone.
Mechanism: `narrowTypeByInKeyword` (`flow.go:1001`), **known-property half
only** — `filterType` by `isTypePresencePossible` (`:1024`: a non-optional
property is present iff the guard holds, an optional one is possible either
way, an index signature makes both possible, absence is possible only on the
false branch). The unknown-property half intersects with `Record<X, unknown>`
via the global `Record` alias — alias instantiation is unported and upstream
itself returns `t` unchanged when the symbol is missing, so that half is a
no-op here by upstream's own fallback.

Bar: **net ≥ 5**; gained ≥ 3 × lost; 0 case regressions; Δwrong ≤ 0. Small
population, same legs, same instruments, one pair.

### §6.1 scored — leg 2 caught a real bug before it shipped

First run: +45/−13, ratio 3.46 — numerically passing every leg, with all 13
losses in one case. Diagnosed per the rule (build wrong first): the port read
`SymbolFlags::OPTIONAL`, **which this binder never writes**, so every optional
property tested as required and the else branch of `'a' in obj` collapsed to
`never` (`strictOptionalProperties1`). The fix reads optionality where this
port keeps it — the declaration's question token, via
`is_optional_declaration` — and the pair is pinned by a unit test. Final:
**+43, 0 lost, 0 regressions, Δwrong −19** (21 fixed, 2 new). A leg that
passes numerically can still be naming a bug; 13 lines in one case was the
tell, and pricing it as a trade would have shipped it.

## 7. Assignment narrowing keeps a fresh boolean literal fresh (`bd tsr-xs0`)

Registered at `563849e`, before code. Found by `bd tsr-o00`'s wrongdelta:
`function f15() { var c4 = true; return { c4 }; }` prints the shorthand
member `c4: true` where upstream prints `boolean`. Upstream's
`getAssignmentReducedTypeWorker` (`flow.go:2415`) carries the exact line —
*"Ensure that we narrow to fresh types if the assignment is a fresh boolean
literal type"*: after filtering the declared union, a fresh boolean-literal
assignment maps the kept constituents through `getFreshTypeOfLiteralType`,
so the narrowed literal **widens back to `boolean` at mutable-location and
initializer boundaries**. The port's `get_assignment_reduced_type` skipped
the step behind a comment whose reason — "freshness is not modelled here" —
was true when written and is stale since the literals module landed.

Sized from the live wrong dump at `563849e`: **73 lines** are exactly
`want boolean, got true/false`, and **47 more** are compound texts fixed by
that substitution — a ~120-line ceiling of which this mechanism owns an
unknown share (other freshness paths exist). Head case
`conformance/parserRealSource7` at 18; the mechanism's own named case,
`compiler/literalFreshnessPropagationOnNarrowing`, is in the head at 5.

**The bar:**

1. **net ≥ +15**;
2. **lost ≤ 5, each diagnosed** — the mapped-to-fresh condition mirrors
   upstream's exactly (fresh boolean-literal assigned type), so a loss
   means the *assigned type's* freshness diverges somewhere upstream's
   does not;
3. **0 case regressions**;
4. **gained ≥ 3 × new wrong** by `wrongdelta`.

**Falsifier:** >40% of the gain in `parserRealSource7` (its share of the
sized family is 25%; concentration well above it means the family was
mis-sized).

Bar fires → build wrong first, premise wrong second, no third.

### §7 scored — all legs pass, Δwrong −37

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ +15 | **+37** (6 cases, 3 finished) | pass |
| 2 | lost ≤ 5 diagnosed | **0** | pass |
| 3 | 0 regressions | **0** | pass |
| 4 | gained ≥ 3 × new wrong | **37 fixed, 0 new** | pass |

**Falsifier:** `parserRealSource7` took none of the gain — its 18 lines
are a *different* freshness path (they remain in the wrong bucket and are
the unowned remainder of the ~120 ceiling). Top gainer is the
destructuring-pattern pair at 9/37 = 24%, and the mechanism's named case
`literalFreshnessPropagationOnNarrowing` converted 7. **KEEP.**

This is the build that carried the gradient over the exact 70% threshold:
**335,293 / 478,954 = 70.003%**, cases 2,540.
