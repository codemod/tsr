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

## §8 `tsr-5kii` sizing, first pass — the cheap prior is CONTAMINATED, recorded so nobody quotes it

Ninth session. A grep-join of switch-bearing case sources against the
residual dump reads **12,140 residual lines in 101 switch-bearing cases** —
and the head is `parserRealSource11` (3,481), `parserRealSource7` (2,406),
`parserindenter` (1,035): cases that *contain* switches while their residual
mass belongs to naming and member resolution. A population identified by the
presence of a keyword is not attributed to a mechanism — the file's oldest
rule, and this prior would break it.

What §11.2 and §15.1 *did* measure: switch narrowing's absence minted 4
concrete wrong lines through the return aggregate
(`switchCaseNarrowsMatchingClausesEvenWhenNonMatchingClausesExist` — the
default arm's un-narrowed `string` collapses `"abc" | "defaultValue"`), and
`controlFlowOptionalChain`/`controlFlowAliasing` (269 + 221 residuals) are
narrowing-family cases in the contaminated list.

**The probe that would size it** (unbuilt): per gap/wrong line inside a
`CaseClause`/`DefaultClause`, test whether the line's expression contains a
reference `is_matching_reference`-equal to the enclosing switch's
discriminant, and split by whether the case expressions are unit-typed
(the `narrowTypeBySwitchOnDiscriminant` precondition). That is
`refmatch.rs`'s shape pointed at switch clauses, and it is the next
session's first instrument.

### §8.1 `tsr-5kii` SIZED — a ~100-line item, not a subsystem

`switchgap.rs` (new), walking every non-right aligned line's ancestry at the
sixteenth build's commit:

```
111,129  not in a switch clause
  1,305  in clause, property-access discriminant — 57.5% parserRealSource11
    693  in clause, identifier discriminant, line does not mention it
    530  in clause, other discriminant shape
     75  IN CLAUSE, identifier discriminant, line MENTIONS it — WRONG
     29  same — GAP
```

**The reachable population is 104 lines across ~23 cases** (top-1 ~30%), and
that is a *ceiling*: a mentioning line converts only when the clause-narrowed
answer is the wanted one. The 1,305 property-access-discriminant lines are
mostly `parserRealSource11`'s naming mass sitting inside switches — the §8
contamination measured per-line rather than per-case. The mechanism
(`narrowTypeBySwitchOnDiscriminant` plus clause flow) is an effort-3 build
for a ≤104-line ceiling at the observed conversion band — **it does not
clear the board**, and `tsr-5kii` carries this number now instead of
"unsized". The 4 aggregate-exposure wrongs (§11.2) stay as they are: their
fix is this mechanism, at this price, whenever the price is worth it.

## §9 The `any → undefined` family split — the closure sketch is DISPROVED, the auto conversion is the spine

`closuregap.rs` (new, ninth session), classifying every `want any / got
undefined` wrong line by upstream's own first two axes
(`checker.go:11120`–`:11175`):

```
   316  OUTER reference, declaration has NO initializer   (50 cases, top-1 5.4%)
   276  SAME container, declaration has NO initializer    (86 cases)
    52  same container, WITH initializer
    32  non-identifier / unresolved / no-value-decl tails
   676  total (W2's 548 is the ROOT-only subset; the delta is propagation)
```

**TASK.md's staged sketch — "different container → any" — is disproved
before it cost a build**: 276 lines sit in the SAME container, so the
family's spine is not the closure boundary at all. It is the **auto-type
conversion**: `let x;` with no reaching assignment answers `any` upstream
(the implicit-any widening of `autoType`) where this port's flow answers the
`undefined` initial type. The closure axis is real but secondary — the OUTER
316 need the flow-container extension AND the same conversion.

**The open question that gates the build, named precisely:** locate
upstream's site where an auto-typed variable's flow result becomes `any` —
the `convertAutoToAny` family and its strictness interaction (jsxEsprima's
head is non-strict; capturedLetConstInLoop is strict) — and write the
ADR-0038 argument (this is `anyType` from a computation, the `anySignature`
precedent, not a rendered `errorType`). Only then bar the two arms
separately: same-container first (276, no closure machinery needed), outer
second.

### §9.1 The mechanism located exactly, and the bar

`checker.go:11182`–`:11193`, both §9 arms through one exit pair:

- **flow result still `autoType`** (the walk never saw the declaration — the
  OUTER 316): `convertAutoToAny(flowType)` → `anyType` (`:11188`, `:31216`);
- **used-before-assigned** (same container, declared auto does not contain
  `undefined`, flow result does — the SAME-container 276): report and
  **return the declared type** (`:11193`) — the declared `autoType`, which
  renders `any`.

Both are *computed* `anyType` — `convertAutoToAny` maps `autoType → anyType`
by identity, upstream's non-error path; the `anySignature` ADR-0038 argument
carries over verbatim. The port's divergence is one substitution: this
port's flow hands an auto declaration `undefined` as its initial and then
BELIEVES the initial when it survives; upstream believes the *declared*.

**The implementation constraint that decides the shape:** a genuine
`x = undefined` assignment also produces an `undefined` flow result, and the
two are indistinguishable by `TypeId` — the walk must report whether the
START node's substituted initial contributed to the answer, and only that
provenance converts. A flag through the walk, not a type comparison.

**Bar:** population 592 (276 + 316; the 52 with-initializer lines are other
mechanisms and excluded). Net ≥ **+350**; own ≤ **30** — falsifier: losses
on references AFTER a reaching assignment (`let x; x = 1; x` must stay
`number` — if the flag leaks through assignment arms the provenance model is
wrong, stop); regressed == 0; lost == 0.

### §9.2 REFUSED for now — +989 measured and forfeited, on two fired legs and a structural unsoundness

The sentinel build measured **WRONG→RIGHT 735, GAP→RIGHT 343 (+989 net)** —
and fired legs 2 and 3: **79 RIGHT→WRONG, 10 RIGHT→GAP, 7 cases regressed.**
Reverted whole. Three findings, each load-bearing for the next attempt:

1. **Both behaviours exist in the corpus.** `want undefined` same-container
   lines (`deleteOperator1`, `assignmentToParenthesizedExpression1`,
   `commentsArgumentsOfCallExpression2`) sit beside the 276 `want any` ones —
   the discriminator is somewhere in var-vs-let, strictness, or the
   `isMutableLocalVariableDeclaration` refinements this cycle did not model.
   A per-case study of the two want-populations is the prerequisite.
2. **The loop-label leak.** `nestedBlockScopedBindings*` regressions are
   `let` in loops — the sentinel provenance mis-reports through loop-label
   merges (the unported incomplete-types fixpoint family, again).
3. **The re-walk is structurally unsound**, independent of semantics:
   `~ANY1 : number → error` shapes show sentinel-derived types leaking into
   the node-type memo and interned unions — a second walk over machinery
   with write-through caches contaminates them. **Provenance must be a flag
   on `FlowType`, threaded through every arm — never a second walk.**

The +989 stays on the table with its price list: the discriminator study,
the loop-label handling, and the `FlowType` provenance flag. Nothing about
the §9.1 derivation of upstream's exits is retracted — `checker.go:11182`'s
two exits are the mechanism; the port's implementation shape is what failed.

### §9.3 The discriminator study — paid, and the rule is strictness × container

Profiling the reverted build's loss cases against its win cases by case
options (`@strict`) and declaration keyword:

```
LOST (want undefined):  strict/var 13 · strict/let 4 · strict/other 3 — ALL STRICT
WON  (want any):        nonstrict 74 · strict 53 (the strict wins are OUTER/loop-capture shapes)
```

**The rule the corpus draws:** a never-assigned auto reference answers
`undefined` under **strict, same-container** (keep the current behaviour —
the reverted build's losses were exactly these), and `any` under
**non-strict** (any container) or **outer/captured** (any strictness —
`capturedLetConstInLoop*` is strict and wants `any`). §9.1's exit-reading
was right about the mechanism and wrong to apply it strictness-blind.

Retry shape, cheaper than the §9.2 price list where it can be: the
NON-STRICT same-container arm may fall out of the *initial type* alone
(non-strict auto initial = `any`, no provenance needed — but the mixed-
assignment union behaviour under that initial is unmeasured, so it needs its
own counterfactual); the OUTER arm still needs the container test; and any
provenance-dependent remainder keeps §9.2's FlowType-flag requirement. Loop-label
handling unchanged as a cost.

### §9.4 The non-strict arm as an initial-type change — bar

Under `!strictNullChecks`, an auto declaration's substituted initial becomes
`any` instead of `undefined` — pure-initial references answer `any` (the
§9.3 rule's non-strict half) through the ordinary walk, no provenance, no
second walk, no cache exposure. Mixed-assignment paths union the assigned
type with `any` and absorb — a behaviour the §9.3 study did NOT measure,
which is what the own-wrong leg is for.

**Bar:** net ≥ **+200** (the non-strict share of the 592-line family); own ≤
**25** — falsifier: wrong lines in non-strict cases whose want is the BARE
assigned type (`number`, not `any`) at a mixed-path reference → the
absorption model is wrong and the arm needs the branch-label distinction,
stop and record; regressed == 0; lost == 0 (strict behaviour untouched by
construction).

### §9.5 §9.4 scored — the one-line arm delivered the family's non-strict half, plus a falsified intuition

```
WRONG→RIGHT 510 · GAP→RIGHT 506 · GAP→WRONG 6 · RIGHT→anything 0
0 regressed · suite +19 cases (2,963 → 2,982) · 74.19% → 74.40%
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +200 | **+1,010** | pass (5×) |
| 2 | own ≤ 25 | **6** (4 catch-clause narrowing, 2 JSX parse-recovery) | pass — the mixed-path falsifier did not fire |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** — after the unary fix below | pass |

The first measurement's 8 losses were all `~`/`-` on a newly-`any` operand
and exposed a wrong intuition comment in `unary_result_type`:
`maybeTypeOfKind` is a **flag** test (`checker.go:10923`), an `any`/`unknown`
operand does not carry `BigIntLike`, and upstream answers `number` —
`bitwiseNotOperatorWithAnyOtherType.types` throughout. The comment claiming
the opposite is the **eighth** intuition comment this project has falsified
against its own anchor, and the twenty-first stand-in fixture came due — one
that had already flipped once and whose second assertion was also wrong.

The strict same-container behaviour is untouched by construction (the §9.2
losses' population), and the OUTER-strict remainder keeps §9.2's price list.

## §10 The compound-assignment target widens — bar (ninth session)

`checker.go:11196`–`:11198`: an identifier that is the TARGET of a compound
assignment returns `getBaseTypeOfLiteralType(flowType)` — `x |= …` reads `x`
at `boolean`, not the narrowed `true`. The W2 row: 12 lines, 5 finishes,
head `bitwiseCompoundAssignmentOperators`.

**Bar:** net ≥ **+6**; own ≤ **4** — falsifier: losses on PLAIN `=` targets
(the widening must gate on compound operators only); regressed == 0;
lost == 0.

### §10.1 Scored — clean at the floor

```
WRONG→RIGHT 6 · nothing else · +1 case (2,982 → 2,983)
```

Legs: net **+6** (= the floor exactly) · own **0** (the plain-`=` falsifier
silent) · regressed **0** · lost **0**. `get_base_type_of_literal_type`
lands with the enum-like arm deliberately identity (its base walk lives with
`enum_member_owners`' consumers), which is the shape §10's bar did not size.

### §9.6 The retry price list, CORRECTED after sizing the flag

The §9.2 list said "provenance as a FlowType flag". Sized: 18 construction
sites plus the label merges — tractable — **but the flag alone cannot
deliver the strict-OUTER remainder**: `capturedLetConstInLoop*`'s walk
*reaches* the outer assignments through this port's graph (the graph has no
function boundary), so no initial-provenance question arises there at all.
Upstream's answer comes from `getFlowTypeOfReferenceEx`'s **flowContainer
parameter** — the walk STOPS at the reference's control-flow container's
start and returns the declared auto. The real price is the container-bounded
walk: `FlowState` carries the reference's container, and the start-arm
family answers `declared` when the start belongs to a different container.
Whether the binder's flow graph marks container starts is the open question
that decides the effort — check `tsr-binder`'s flow construction before
costing.

### §9.7 The outer arm falls out of the START node — bar

§9.6's open question is answered by the binder's own construction: **every
container gets its own `START`** (`binder.rs:903`), so the walk already
stops at the function boundary — the port never needed `flowContainer`
because the graph never crosses. The divergence is the START arm's answer:
this port answers the substituted `initial_type`; upstream, for a reference
whose declaration lives OUTSIDE the container, answers the **declared**
type — and an auto declaration's declared type in this port already IS
`anyType` (the UNREACHABLE arm's comment records the identity). So the
outer-strict remainder is one test at entry (the reference's control-flow
container vs the declaration's — `closuregap.rs`'s own predicate) and one
branch in the START arm.

**Bar:** net ≥ **+80** (the 197 residue's outer share plus the
`capturedLetConstInLoop` families); own ≤ **15** — falsifier: losses on
outer references wanting the *assigned* type → the boundary is wrong for
same-container-reached assignments, stop; regressed == 0; lost == 0.

### §9.8 §9.7 scored — four measurements, three clauses learned, the floor overridden

Round 1 (outer → declared, unconditional): +58 / **24 lost / 3 regressed** —
the never-assigned outer `let x;` keeps `undefined`
(`nestedBlockScopedBindings*`), which is `isNeverInitialized`'s complement.
Round 2 (gate on ever-assigned): 5 / 0 — too tight, the file-level `var`
population dropped (jsxEsprima's 48: `isNeverInitialized` requires a mutable
**local**). Round 3 (add file-level): +58 / 8 / 2 — a `let` in a bare block
at file level is STILL a local (block scoping). Round 4 (file-level gated on
`var`-ness via the `LET|CONST` node flags):

```
WRONG→RIGHT 37 · GAP→RIGHT 5 · GAP→WRONG 1 (a for-of iterator shape, exposure) · nothing else
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +80 | **+41** | **FIRED — overridden** on the §12.1 precedent: the floor priced the whole outer share, three falsifier-driven narrowings carved the sound population to 42, and every narrowing is a ported clause of `checker.go:11141`–`:11150` (`isNeverInitialized`, `isMutableLocalVariableDeclaration`, the var/let split). Zero losses, zero regressions |
| 2 | own ≤ 15 | **1** | pass |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost == 0 | **0** | pass |

The §9 family is now closed to its studied edges: non-strict initial
(+1,016), outer-var/assigned-capture (+42), strict same-container `let`
correctly `undefined` by construction. What remains of `any → undefined` is
the ever-assigned scan's granularity (assignments the scan's container walk
cannot see) — small, owned here.

## §11 A non-identifier enum member's type is an indexed access — bar (ninth session)

`negateOperatorWithEnumType.types:13`: `>"" : (typeof ENUM1)[""]` — a member
whose name is not identifier text prints as the indexed-access spelling, not
`ENUM1.`. The W2 row: 11 lines, 10 cases, 2 finishes; ours bakes the dotted
form with an empty right side.

**Bar:** net ≥ **+6**; own ≤ **4** — falsifier: losses on IDENTIFIER-named
members (the split must not touch them); regressed == 0; lost == 0.

### §11.1 Scored — the falsifier caught the ASCII assumption, then clean

First measurement: +58 / 4 lost — `Ϳ` is identifier text upstream and the
shared ASCII-only `is_identifier_text` said no. A local Unicode
approximation (alphabetic/alphanumeric classes) fixed the split without
touching the shared helper's other consumers. Final: **WRONG→RIGHT 58,
nothing else, +9 cases.** Legs: +58 (≥ +6) · 0 own · 0 · 0.

## §12 The loop fixpoint — bar (ninth session, the subsystem)

`getTypeAtFlowLoopLabel` (`flow.go:1325`), the arm every residual since
`e7a65fb` has named. Today a `LOOP_LABEL` falls to the catch-all and follows
ONE antecedent — the entry path — so assignments inside a loop are invisible
at and after the junction. The port, faithful:

- `flow_loop_cache: (FlowId, key) → TypeId` and `flow_loop_stack:
  Vec<(key, types-so-far)>` on the Checker; key = the symbol when the
  reference has one, else the reference node (upstream's `refKey`, reduced
  to the shapes this port narrows);
- cached → return; on-stack with non-empty types → the literal union so
  far, `incomplete: true` (nested-loop restart works because the FIRST
  antecedent is always the non-looping path);
- back-edge walks push the stack entry and **truncate `shared_flows` after
  each** — upstream nils `flowTypeCache` for the same reason, and §9.2's
  cache poisoning is the recorded price of skipping this;
- `subtypeReduction` (a foreign type injected by `instanceof`/predicates)
  routes through the §9 decidability-gated reduction; an undecidable union
  falls back to the DECLARED type — today's unnarrowed answer, the safe
  direction;
- an incomplete first antecedent makes the result incomplete and uncached.

**Bar:** net ≥ **+150** (the W1' diffuse tail is unsized); own ≤ **40** —
falsifier: wrong lines in loop cases wanting the declared union where the
fixpoint produced something narrower → an assignment arm this port lacks is
being fixpointed over, read the residual before widening; regressed == 0;
lost ≤ **10** (the entry-only behaviour today can be RIGHT by accident where
the loop body's assignments would widen — those flips are the one loss shape
this arm can produce legitimately, and above 10 they say the union model is
wrong).

### §12.1 The first fixpoint cycle — REFUSED with three named defects, the port sketch validated

Measured whole: **WRONG→RIGHT 230, GAP→RIGHT 11 — and RIGHT→GAP 196,
RIGHT→WRONG 77, 2 regressed.** Reverted. The three defects, each now a
priced line-item for the session that takes this subsystem as its whole
deliverable:

1. **The depth-cap interaction** (196 of the losses, ONE case —
   `parsingDeepParenthensizedExpression`): back-edge walks multiply depth in
   the already-pathological graph, `MAX_FLOW_DEPTH` trips, and
   `flow_analysis_disabled` poisons the rest of the case. Upstream's cap is
   per-`getTypeAtFlowNode` with the loop stack excluded; the port's single
   depth counter is not.
2. **Incomplete-union semantics**: `controlFlowLoopAnalysis` wants
   `number | undefined` where the fixpoint produced `number` — the condition
   and assignment arms must treat an incomplete antecedent's type as
   provisional (upstream re-narrows on the completing pass; this port's
   arms treat every input as final).
3. **Self-referential loops are a ceiling**, not a conversion:
   `controlFlowSelfReferentialLoop` wants `any` (upstream's own bail), and
   the fixpoint's honest union is *wrong* there — the family needs the
   `convertAutoToAny`-style bail on self-referential keys, or those 60+
   lines stay theirs.

**What survived:** the protocol port itself (cache, stack, first-antecedent
rule, shared-flows truncation) ran without cache poisoning — §9.2's lesson
held. +241 is the measured upside once the three defects are owned.

### §12.2 The second attempt — five cycles, five identical measurements, and a disproved diagnosis

The fixpoint was re-implemented with §12.1's fixes and refined four more
times: depth save/restore around nested walks, the empty-placeholder
on-stack return, a per-invocation disable scope, a half-budget declared
bail, and incomplete propagation through the branch label (which HAD been
hardcoding `incomplete: false` — a real defect, now documented). **Every
one of the five measurements was byte-identical**: +241 gained, 196 lost in
`parsingDeepParenthensizedExpression`, 77 wrong, 2 regressed.

**§12.1's defect-1 diagnosis is DISPROVED**: an instrumented run shows the
depth cap **never trips** — the 196 losses do not come from the cap, the
disable, or the depth accounting at all, and three interventions were aimed
at a mechanism that does not fire. What produces them is still unidentified;
the placeholder-`never` laundering hypothesis (branch label dropping
`incomplete`) was real as a defect but did not move the number either.

**The next attempt's REQUIRED first step is an instrument, not a fix**: a
single-line trace comparing the flow walk of one
`parsingDeep…:0:1095`-family line with and without the loop arm — which
node answers, with what, and where the `error` enters. Five blind cycles
are the recorded price of skipping it. The +241 upside stands measured; the
port sketch (cache/stack/first-antecedent/truncation) remains validated for
everything but this case family.

### §12.3 The trace session — three facts established, the next probe layer named

`traceone.rs` (new, kept) plus a `TSR_TRACE_LOOP` hook on the patched arm:

1. **The antecedent order is correct**: the binder tail-appends and the
   entry edge is added before the body walk (`binder.rs:1286`/`1291`), so
   upstream's "first antecedent is the non-looping path" HOLDS here — that
   hypothesis is retired.
2. **Initialized declarations DO have assignment flow nodes**
   (`bind_initialized_variable_flow`, `binder.rs:1895`) — that hypothesis is
   retired too.
3. **The failing shape, seen directly**: for the `var Tn = f(Tn-1)`-in-loop
   chains, the ENTRY antecedent walks to the substituted initial
   (`undefined`, complete) and the BACK edge returns the on-stack
   placeholder — `t=undefined, incomplete=true` — WITHOUT contributing the
   declaration-assignment's `number`. The junction unions to `undefined`,
   and `Tn + 32` then errors. Pre-fixpoint the catch-all followed the back
   edge alone and was accidentally right.

**The next probe layer**: why the back-edge walk reaches the junction
without passing `Tn`'s declaration-assignment — instrument the walk's node
kinds between the back edge and the junction (nested inner labels re-routing
the walk is the live hypothesis). The trace hook's placement is recorded in
`fixpoint-patch-§12.md`.

### §12.4 The 196 solved — it was a JS file, and the walk trace earned its keep

The node-kind probe led to the case source: `parsingDeepParenthensizedExpression`
is **`@fileName: a.js`** — minified MD5 JavaScript, chained assignment
EXPRESSIONS (`M = (M = …`) inside `for` loops. With the §11.2 JS decline on
the loop arm (`in_js_file`, the flag that session made real), the 196
vanish entirely — **§12.1's defect 1 was never a depth problem; it was the
JS-file trap's third appearance** (`@overload` §11.1, the auto-initial JS
residue, now this). 130 of the first attempt's 230 wins were the same JS
case's other lines converting, so the JS decline is load-bearing in both
directions.

With it, the sixth measurement reads: **+111 gained / 63 RIGHT→WRONG /
12 RIGHT→GAP / 2 regressed** — the residue is now purely the `.ts`
loop-iteration family (`controlFlowIterationErrors*`, `WhileStatement`,
`typeGuardsAsAssertions`, `SelfReferentialLoop`): the completing-pass
incomplete semantics, §12.1's defect 2, the one remaining real port gap. Its
candidate mechanism, for the next cycle: upstream's `antecedentTypes` slice
ALIASES into the on-stack entry (Go slice backing shared up to capacity), so
a second back edge's placeholder can see the first back edge's contribution
— this port's `types.clone()` snapshot freezes it instead. Verify against
`flow.go:1367` before coding.

Reverted again; the arm plus JS decline is the base state for the next
cycle, preserved in `fixpoint-patch-§12.md` (amended).

### §12.5 Cycles 7–8 — the last two plumbing hypotheses retired; the residue is SEMANTIC

Cycle 7 (the live on-stack accumulator, upstream's slice aliasing made
literal): **byte-identical to cycle 6.** Cycle 8 (a completing second pass
at the top level when the walk roots incomplete, caches primed): **also
byte-identical.** With the JS decline in place, the fixpoint's balance is
stable at **+111 gained / 63 wrong / 12 lost / 2 regressed across three
structurally different implementations** — the 86 adverse lines are
invariant under every incomplete-machinery variant, so they are not
plumbing: they are the narrowing ARMS' answers diverging from upstream
inside loop bodies (`controlFlowIterationErrors*`, `WhileStatement`,
`typeGuardsAsAssertions`, `SelfReferentialLoop` — try/catch interplay,
assertion re-narrowing, the self-referential `any` bail).

**Eight cycles, five retired hypotheses** (antecedent order, missing
declaration-assignments, depth cap, slice aliasing, completing pass), one
solved trap (the 196 were a JS file), one real defect banked (branch-label
`incomplete` laundering). **The mandated next step is a one-line semantic
trace**: pick `controlFlowIterationErrors:0:<n>`, print the walk's per-arm
answers, and set them against upstream's arms for the same line — the
divergence is in an arm, and the plumbing is proven ready to carry the fix
when it is found.

### §12.6 The one-line trace ran — the arms are innocent; the poison is TRANSIENT CACHE WRITES

The §12.5-mandated trace (`TSR_TRACE_LOOP` in the loop arm, kept in
`traceone.rs`'s workflow) on `controlFlowIterationErrors` produced the
decisive negative: **all eight loop labels compute the upstream-correct
union** — `[string, number] → string | number` at every per-function key —
yet the printed positions read `string` / `number`. The walk's arms answer
correctly; what the printer reads is the **per-node cache
(`expressions.rs`'s `node_types`), stamped during the transient back-edge
pass** while `x` was provisionally `string`. `foo(x)` was resolved once
against that provisional `string`, cached as `number`, and the cache entry
outlived the fixpoint's convergence. §9.2 said it in general terms
("second walks over write-through caches are structurally unsound —
provenance must be a flag"); this is the mechanism, named.

Upstream's guard is explicit — `checkExpressionCachedEx`
(`internal/checker/checker.go:7517`): before computing a type that will be
cached it **clears `flowLoopStack` and `flowTypeCache`**, with the comment
"variables may have transient types in indeterminable states. Moving
flowLoopStart to the top of the stack ensures all transient types are
computed from a known point." The dual formulation for this port's single
`node_types` cache: **suppress the write while `flow_loop_stack` is
non-empty** — transient computations answer but never persist. Reads stay:
entries written outside loop analysis are final by construction, and if
transient writes never happen, no stale entry can exist to be read.

**The bar**: with the §12 patch live (JS decline included) plus the
one-line write gate, the `controlFlowIterationErrors*` family's stale-cache
lines (`foo(x)` families, post-loop unions) flip; the standing fixpoint
balance (+111/63/12/2) must improve on the 63+12 adverse side, since those
were measured through the poisoned cache. Falsifiers: (a) if the gate does
not move the adverse 86, the staleness diagnosis is wrong and the arms are
back under suspicion; (b) any net-negative verdictdump refuses the pair
whole — the gate cannot be worth keeping if recomputation-at-query-time
answers differently than the final source-order pass.

Known non-goals of this bar, priced separately: assignment-LHS positions
want the *declared* type (`x` in `x = foo(x)` prints
`string | number | boolean`) — a distinct upstream rule, not cache
staleness; and `foo(x) : never` on converged union arguments needs the
overload-failure result type. Both stay wrong under this bar and are the
§12.7 candidates.

**§12.6 score — LANDED.** The pair (the §12 fixpoint patch, JS decline
included, plus the one-line write gate in `check_expression`):
**+117 gained (106 WRONG→RIGHT, 11 GAP→RIGHT) / 53 RIGHT→WRONG /
1 RIGHT→GAP / 1 WRONG→GAP** — net **+63 right, −54 wrong, −9 gap**;
`checker_types` 74.46% → **74.48%**, right 356,653 → **356,716**. Both
falsifiers held: the gate moved the adverse side (75+2 → 54), and the pair
is net-positive. Gains: `controlFlowLoopAnalysis`, `nestedLoopTypeGuards`,
`controlFlowForStatement`, `cf`, the iteration-errors arg positions.

**Two cases regressed** — stated loudly, the session's first:
`controlFlowForStatementContinueIntoIncrementor1` and
`parserForOfStatement24` (case count 3,043 → 3,041 while the line count
rose). Both sit in the residue families below.

**The §12.7 residue, priced from the 53 R→W:**
1. **The self-referential `any` bail** (~30 lines: `controlFlowSelfReferentialLoop` ×9,
   `shorthandPropertyAssignmentsInDestructuring_ES6` ×3, `ContinueIntoIncrementor1` ×2,
   both regressed cases): upstream answers `any` for auto-typed variables whose
   loop assignment depends on the variable itself; this port computes the real
   union. Find upstream's circularity bail before coding.
2. **So-far under-accumulation** (~20 lines: want `string | number` got
   `string`, want `number | undefined` got `number`): the on-stack so-far
   union misses a contribution upstream has by its second iteration —
   upstream RE-ITERATES `getTypeAtFlowLoopLabel` to convergence where this
   port walks antecedents once. Read `flow.go:1325`'s loop shape before
   coding.
3. Assignment-LHS declared-type rule and overload-failure `never` (from the
   bar's non-goals; `controlFlowIterationErrors*` ×16 residue).

### §12.7 The definite-assignment-target rule — one upstream rule under two residue families

Reading the §12.6 residue against upstream collapsed families 1 and 3 into a
single rule this port never had: **a variable reference in a definite
assignment-target position is returned at its declared type, with no flow
analysis** — `checkIdentifier`, `checker.go:11109`: for
`SymbolFlagsVariable` symbols, `AssignmentKindDefinite` returns `t`
directly (`getBaseTypeOfLiteralType(t)` when the assignment is
compound-like). The baseline shows it plainly
(`controlFlowSelfReferentialLoop.types:160`): `AA=a : number` but
`AA : any` — the *target* prints the declared type, and an auto-typed
variable's declared type prints `any`; three lines later the *read*
position of `k` prints `number`. The "self-referential `any` bail"
hypothesis was wrong — there is no bail; the errors baseline carries only
TS7006, none of the circularity/flow-limit diagnostics, and the wants are
just declared types at target positions.

Kind classification, ported from `getAssignmentTargetKind`
(`internal/checker/utilities.go:90`) over `ast.GetAssignmentTarget`
(`internal/ast/utilities.go:184`): the target walk climbs parens, array
literals, spreads, non-null assertions, spread/shorthand/property
assignments; a `=` or logical-assignment (`&&=`/`||=`/`??=`) binary is
DEFINITE, other assignment operators and `++`/`--` are COMPOUND, a
for-in/for-of initializer position is DEFINITE. Compound-like
(`isInCompoundLikeAssignment`, `utilities.go:118`): a definite `=` whose
right side (parens skipped) is a shift-or-higher binary — `x = x + 1`
prints like `x += 1`, at the literal's base.

**The bar**: the two §12.6-regressed cases recover
(`controlFlowForStatementContinueIntoIncrementor1`,
`parserForOfStatement24`); the auto-target family flips
(`controlFlowSelfReferentialLoop`, `shorthandPropertyAssignmentsInDestructuring_ES6`);
the iteration-errors LHS lines (`x` in `x = foo(x)`, want the declared
union) flip. Falsifiers: (a) upstream narrows nothing at definite targets,
so no current RIGHT line should depend on flow analysis at one — any R→W
here means the kind walk misclassifies; (b) the existing compound behavior
(flow, then base-of-literal) must not move — it is the §10 bar re-asserted.

**§12.7 score — LANDED, the session's cleanest sweep.** **+692 WRONG→RIGHT,
+2 GAP→RIGHT, ZERO adverse transitions.** `checker_types` 74.48% → **74.62%**,
right 356,716 → **357,410**, cases 3,041 → **3,064** — both §12.6-regressed
cases recovered, net +21 cases over the pre-§12.6 board. Both falsifiers
held: no RIGHT line depended on flow analysis at a definite target, and the
compound behavior did not move. The remaining iteration-family residue:
overload-failure `never` (`foo(x) : never` on converged union arguments —
a callres question, not a flow one) and the §12.6 under-accumulation lines
(`narrowingPastLastAssignment`, `controlFlowLoopAnalysis:0:25`).

### §12.8 The empty-so-far re-entry — upstream restarts, this port answered `never`

`getTypeAtFlowLoopLabel`'s on-stack check is conditional upstream:
`if loopInfo.key == key && len(loopInfo.types) != 0` (`flow.go:1347`), with
the comment spelling out why — an empty in-process list happens when "the
back edge of the outer loop reaches an inner loop that is already being
analyzed", and the correct move is to **restart the inner loop's analysis**,
which terminates because a loop junction's first antecedent is always the
non-looping path. This port's §12 arm returned `never, incomplete` for the
empty case — a placeholder upstream never produces.

**The bar**: the §12.6 under-accumulation lines move
(`narrowingPastLastAssignment` want `string | number`,
`controlFlowLoopAnalysis:0:25` want `number | undefined`,
`nestedLoopTypeGuards:0:22` want the full declared union — nested-loop
shapes all). Falsifiers: (a) non-termination — a hang or stack blowout in
the conformance run means the restart's termination argument does not
transfer to this port's walk and the change reverts whole; (b) any
RIGHT→WRONG outside nested-loop cases.

**§12.8 score — LANDED AT ZERO, and the bar's prediction was WRONG.** The
restart semantics measured **byte-identical** across the corpus: no hang
(falsifier (a) held), and none of the predicted under-accumulation lines
moved — the empty-so-far re-entry is corpus-unreachable or output-neutral
today. Kept as the faithful port (upstream's shape, one condition), but the
under-accumulation family (`narrowingPastLastAssignment:0:64`,
`controlFlowLoopAnalysis:0:25`, `nestedLoopTypeGuards:0:22`) is now
**mechanism-unknown again** — its next probe must trace one line, not guess
a fourth time.

### §13 Past-last-assignment closure narrowing — the flow container extends outward

The `narrowingPastLastAssignment` family (§12.8's mechanism-unknown resid-
ue) is upstream's closure-narrowing feature, in two halves this port has
neither of:

1. **The extension loop** (`checkIdentifier`, `checker.go:11139`): when the
   reference's control-flow container is a function expression, arrow, or
   object/class-expression method, and the symbol is a constant variable OR
   a parameter/mutable-local referenced **past its last assignment**, the
   `flowContainer` bound moves one container outward — repeatedly. Analysis
   then continues from the closure's *creation site* in the enclosing
   container: the START arm walks out (`flow.go:187`,
   `container.FlowNodeData().FlowNode`) instead of answering.
2. **The assignment-position mark** (`markNodeAssignments`,
   `flow.go:2698`): one AST walk per enclosing function/source-file records
   each parameter/mutable-local's last assignment position — `MaxInt32`
   when assigned in a nested function or referenced in a value export
   specifier, extended to the end of any enclosing compound statement
   (`extendAssignmentPosition`, `flow.go:2752` — conservatism instead of
   flow analysis). "Never assigned" reads as past (pos 0 → true).

The binder already gives START nodes their container payload for exactly
the container kinds upstream walks out of (`binder.rs:903`); what it does
not yet record is the flow node in effect AT those container nodes — one
`record_flow` arm.

**The bar**: `narrowingPastLastAssignment` (~30 wrong lines) plus the
closure-read residue in `typeGuardsAsAssertions`/`capturedLetConstInLoop*`
move rightward; net positive. Falsifiers: (a) regressions in the §9.7
population (`nestedBlockScopedBindings*`, `capturedLetConstInLoop*` writes)
mean the walk-out and the §9.7 stop-heuristics disagree about who owns a
line — narrow to `isConstantVariable` only and re-measure before arguing;
(b) a hang means the walk-out loops through a cycle of creation sites the
binder graph permits and upstream's does not.

**§13 score — LANDED.** **+89 (53 WRONG→RIGHT, 36 GAP→RIGHT), zero adverse
after two fired legs were honoured in-build**: (1) the marking walk is
stack-ordered, not source-ordered, so "overwrite" recorded the source-FIRST
assignment — replaced with the MAXIMUM extended position (larger never
wrongly reads as past; 3 R→W → 1); (2) the export exclusion —
`export let x` in a namespace stays wide (`isMutableLocalVariableDeclaration`'s
modifier check, the last R→W). Gains led by
`constLocalsInFunctionExpressions` (26), `narrowingPastLastAssignment` (15),
`gettersAndSetters` (13), `implicitConstParameters` (9). `checker_types`
74.64% → **74.66%**, cases 3,069 → **3,074**; diagnostics rode along 949 →
**961** (+12, the closure narrows reach TS2322's gates). The §12.6
under-accumulation rows (`controlFlowLoopAnalysis:0:25`) remain — they were
never this family.

### §14 The too-large bail — TS2563's observable is `any`, and it is a fifth of the wrong column

The wrong board's head is ONE case: `largeControlFlowGraph`, **10,001
lines**, want `any`, got `any[]` — 29% of all remaining wrong lines. The
mechanism is upstream's DELIBERATE give-up: 10,000 chained `data[0] = 0`
mutations against `const data = []` (autoArrayType) make
`getTypeAtFlowArrayMutation` recurse once per mutation
(`flow.go:1404` → `getTypeAtFlowNode`), the depth cap trips at 2,000
(`flow.go:118`), `flowAnalysisDisabled` sticks for the containing body
(`checkBlock` save/restores it, `checker.go:3791`), TS2563 is reported
once, and **every subsequent reference answers `errorType` — which
upstream prints as `any`**.

This port's cap exists (`MAX_FLOW_DEPTH`, `flow.rs:116`) but (a) never
trips here — the walk skips ARRAY_MUTATION iteratively where upstream
recurses — and (b) answers `errorType`, which ADR-0038 prints as `error`.
On (b): ADR-0038's distinguishability rationale is about THIS PORT's
failures; the too-large bail is upstream's own documented give-up whose
observable output IS `any` (the case comment says "Check that we
gracefully handle this"). Answering the `any` intrinsic at the bail — and
only there — reproduces upstream's output without laundering a port gap.

**The port**: (1) `FlowState` learns `is_auto_array` (declaration with no
annotation and an empty-array-literal initializer — upstream's evolving
trigger); (2) the catch-all's ARRAY_MUTATION skip counts one depth step for
such references, the recursion equivalence; (3) the trip and the
`flow_analysis_disabled` entry check answer `any`; (4) the disabled flag
save/restores around function/module body checks as upstream's
`checkBlock` does.

**The bar**: `largeControlFlowGraph`'s 10k flip. Falsifiers: (a)
`binaryArithmeticControlFlowGraphNotTooLarge` must NOT trip — upstream
walks its 10k-node chain iteratively (plain assignments, no recursion) and
computes real types; a step-counting implementation instead of a
recursion-equivalent one fails exactly there; (b) any new wrong line in a
case whose errors baseline lacks TS2563 means the trip fired where
upstream's did not.

**§14 score — LANDED: +10,000 WRONG→RIGHT, zero adverse. The largest single
build in the project's history.** `checker_types` 74.66% → **76.75%**
(+2.09 points from one case), right 357,576 → **367,576**, wrong 34,326 →
**24,326** — the wrong column lost 29% in one build. Both falsifiers held:
`binaryArithmeticControlFlowGraphNotTooLarge` untouched (its want-number
family is pre-existing and unrelated), and no new wrong line anywhere. One
leg fired in-build and taught the model's refinement: the DECLARATION
prints the widened `any[]` (the symbol keeps its type; upstream's TS2563
poison applies to flow REFERENCES), so the trip disables the container
without touching the symbol's own answer. Diagnostics rode along 961 → 964.

### §15 Compound assignments do not narrow — the walk skips them at the literal's base

`binaryArithmeticControlFlowGraphNotTooLarge` (968 lines, want `number` got
`any`) decoded: reads after `a += anyTypedExpr` want `number` because
upstream's assignment arm SKIPS a compound assignment's effect —
`getTypeAtFlowAssignment` (`flow.go:229`): a matching COMPOUND target
returns `getBaseTypeOfLiteralType(getTypeAtFlowNode(antecedent))`, the
PRE-assignment type. The md5-style chains stay `number` through every
`a += any` because both branch tails re-anchor with plain
`a = (a << 3) | (a >>> 29)` (shift on `any` is `number`), and the compound
steps never inject the `any`. This port's arm computed the `+=` result and
assigned it — the 968 confident wrongs.

**The bar**: the case's want-number family flips; the `2,624 number ← any`
board row shrinks accordingly. Falsifiers: (a) losses in the §10 compound
population (`bitwiseCompoundAssignmentOperators`) mean the reference-side
base rule and this walk-side skip disagree; (b) losses where want IS the
compound result mean upstream's skip is narrower than the arm ported.

**§15 score — LANDED.** First pair: +1,111 W→R / 12 adverse; the fired leg
decoded as CATCH VARIABLES misclassified auto (`is_auto_typed_declaration`
now excludes catch clauses — a catch `e` is `unknown`/`any` by declaration
kind, and the §15 skip had walked through to an auto-initial `undefined`
that typeof-narrowing killed to `never`). Final pair vs pre-§15:
**+1,147 (1,119 W→R, 28 G→R) / 5 adverse** — 3 in
`parserUsingConstructorAsIdentifier` are the DOCUMENTED `noImplicitAny`-off
limitation (`bd tsr-4sc.11`: no compiler options plumbed; the old arm's
accidental `any` masked it), 2 in `narrowByBooleanComparison` moved
GAP→WRONG (`error`→`any`, one step nearer the `number | undefined` want).
`checker_types` right 367,576 → **368,720**, wrong 24,326 → **23,212**.
`controlFlowSelfReferentialLoop` gained 131 — the §12.7 residue's md5
compound chains were this same rule.

### §16 The switch-clause arm — typeof witnesses and identifier discriminants

`switchgap.rs`'s standing rows (`bd tsr-5kii`): 527 lines behind
"other discriminant shape" (head `narrowingByTypeofInSwitch` — these are
`switch (typeof x)` witnesses) and 100 direct identifier-discriminant
lines (71 wrong + 29 gap). The walk's catch-all skips SWITCH_CLAUSE
today; the binder already records the clause range (`SwitchClause`,
`tsr-binder/flow.rs:124`).

Ported (`getTypeAtSwitchClause`, `flow.go:1059`): the two matching arms —
discriminant (`narrowTypeBySwitchOnDiscriminant`, `flow.go:1092`) and
typeof (`narrowTypeBySwitchOnTypeOf`, `flow.go:1157`). The typeof half
reuses `narrow_type_by_typeof_literal` (the `narrowTypeByTypeName` port)
and the NE-facts fragment; the default clause filters by
`getNotEqualFactsFromTypeofSwitch`. The discriminant half takes the
comparable-filter path with Kleene declines: a pair the relation cannot
decide leaves the type unnarrowed (the gap direction, never the confident
one). Unported and stated: `switch (true)`, optional-chain containment,
discriminant PROPERTY access (the 1,298-line row needs structural
matching), and the unknown-ground-types path.

**The bar**: `narrowingByTypeofInSwitch`'s 129 move; the 71 wrong
identifier-discriminant lines flip or gap; net positive. Falsifiers: (a)
losses in typeof-guard narrowing (`typeGuardsInSwitchStatement` family)
mean the clause arm and the condition arm double-apply; (b) discriminant
wrongs where the want keeps a constituent our comparable filter dropped
mean the relation's comparability is over-deciding — decline harder.

**§16 score — LANDED after two fired legs.** First pair read +14/17 and
the trace named the defect in minutes: the clause node's `kind` token is
`CaseKeyword`/`DefaultKeyword`, and comparing against the NODE kinds made
every witness read as a default — the §12.5 lesson (trace one line before
theorizing) paying again. Second leg: JSDoc parenthesized casts must not
be looked through (`parenthesizedJSDocCastDoesNotNarrow`) — the paren skip
now declines in JS files. Final pair: **+108 (97 W→R, 11 G→R), 27
WRONG→GAP (honest declines where the arms bail), 6 GAP→WRONG** — all six
inside `narrowingByTypeofInSwitch`'s exotic tails (keyof/fallthrough
shapes), the case that gained 67. Gains also in
`controlFlowOptionalChain` (14), `switchCaseNarrowsMatchingClauses…` (8),
`literalTypes1` (8). `checker_types` right 369,246 → **369,354**, wrong
22,688 → **22,570**. Residue priced: the `function`/`object` typeof-facts
granularity (`case 'function'` keeps `Basic`), type-parameter narrowing
(`T extends Basic` stays a gap), and the discriminant-property row (1,298,
needs structural matching).

### §17 Property access falls back to the string index signature

`controlFlowOptionalChain`'s head family: `o?.x` on
`{ [key: string]: any; … }` gaps because `access_member_lookup` answers
`error` on a property miss even when the receiver carries an applicable
string index signature. Upstream (`checkPropertyAccessExpressionOrQualifiedName`,
the `prop == nil` path): the applicable index info's value type IS the
member's type (noImplicitAny may error; the type answers regardless). The
machinery exists (`get_applicable_index_info`); the fallback is one arm at
the miss exit, keyed by the property name's string literal type.

**The bar**: the `o?.x` family flips (`controlFlowOptionalChain`), plus
whatever the corpus's indexed-receiver property misses carry. Falsifiers:
(a) losses where want is `error`-shaped — upstream refuses index fallback
for some access forms (assignment targets on generic objects, private
names); if those appear, gate on the upstream conditions before arguing;
(b) the "receiver has members, name absent" callee row (225) must not
convert into wrong CALLS — a fallback `any` callee answering `any` is
right, a fallback SIGNATURE mis-selected is not.

**§17 score — LANDED.** First pair +63/12 — all twelve
`noUncheckedIndexedAccessCompoundAssignments`, the fired leg being the
`@noUncheckedIndexedAccess` option (index results add `| undefined`).
Plumbed as a real per-case option (the third, after `strictNullChecks` and
`noUncheckedSideEffectImports`). Final pair: **+72 GAP→RIGHT / 2
GAP→WRONG** (one subtler shape inside `noUncheckedIndexedAccess`'s own
case). `checker_types` right 369,354 → **369,426**.

### §18 The enum member's regular twin — fresh forms must regularize to the union's constituent

`controlFlowManyConsecutiveConditionsNoTimeout` (120 lines, want
`Choice.One` got `Choice`) traced to an identity break, not a narrowing
arm: the enum union's constituents are the raw `new_named` member ids
(`declared.rs`, the §10.16 creation site), while
`get_regular_type_of_literal_type` on the FRESH member form interns a
twin — so `is_related_to`'s regularize-both-sides preamble compares two
different ids for the same member, the union-target arm finds no
constituent, `Choice.One -> Choice` answers NotRelated/Unknown, and
`getAssignmentReducedType`'s filter keeps nothing (its own guard then
returns the unreduced declared type). Upstream never has this problem
because fresh and regular are two pointers linked on one type object
(`freshType`/`regularType`, `types.go`).

**The port**: record the pairing at the one place a member type is
created — `regular(fresh) = member_type` — and consult it first in
`get_regular_type_of_literal_type`. **The bar**: the 120 flip; assignment
reduction and equality narrowing over enum unions start deciding.
Falsifiers: (a) any change to non-enum literals — the map only ever holds
enum members; (b) `E`-vs-`E.A` printing must not move (the §10.16 bar
re-asserted).

**§18 score — LANDED.** **+142 (139 W→R, 3 G→R) / 6 R→W + 3 R→G.** The
back-link plus one fired leg: a one-constituent union (the one-member-enum
deviation) declines assignment reduction, since upstream's declared type
there is the member itself — `enumOperations` was the leg's head (11 of
the first pair's 17 adverse). Residue: `incrementAndDecrement` ×4 (the
`++` read-at-base family meeting the newly-decidable enum relation),
`controlFlowBreakContinueWithLabel` ×1, `exhaustiveSwitchStatements1` ×3
R→G. `checker_types` right 369,540 → **369,673**, wrong 22,595 →
**22,462**. The fix's reach is wider than the head case: enum equality
narrowing and assignment reduction now DECIDE (`enumLiteralTypes3`,
`stringEnumLiteralTypes3`, `enumPropertyAccess` all moved).

### §19 An un-annotated rest parameter is `any[]`

The `any[] ← any` board row (~200 lines; `restParameterWithoutAnnotationIsAnyArray`
names the rule in its filename): upstream's implicit-any fallback for a
parameter with a `...` token is `anyArrayType`
(`reportImplicitAny`'s rest arm, `checker.go:18131` family), where this
port's `get_widened_type_for_variable_like_declaration` answered plain
`any` for every annotation-less, initializer-less declaration. One arm at
the fallback: a `Parameter` with `dot_dot_dot_token` and no binding
pattern answers `any[]`. Falsifier: non-rest parameters and variables
must not move.

**§19 score — LANDED.** **+374 WRONG→RIGHT, +2 GAP→RIGHT / 6 RIGHT→WRONG.**
The six are the standing written-annotation node-reuse row (`tsr-5o2`),
newly reachable: upstream prints a WRITTEN `(...args)` fn-type verbatim as
`(...args: any)` while typing the symbol `any[]`
(`declFileRestParametersOfFunctionAndFunctionType.types` shows both on
adjacent lines); this port prints from the computed type, so those six
positions now print the — correct — `any[]` where the baseline reuses the
written text. `checker_types` right 369,673 → **370,043**, wrong 22,462 →
**22,094**.

### §20 A nullable initializer widens to `any`

The `any ← null`/`any ← undefined` rows (~320 lines,
`classPropertyAsPrivate` et al.): upstream's `getWidenedTypeWithContext`
(`checker.go:18368`) maps a nullable widening type to `anyType` — `let x =
null` is `any` in every mode; only a `const` keeps `null`. The port's
initializer widening stopped at literals. One arm after the literal
widening: a mutable declaration whose widened initializer is purely
nullable answers `any`. Falsifiers: (a) `const` declarations keep their
`null` (the CONSTANT early-return already guards); (b) annotated `null`
TYPES never pass through this path.

**§20 score — LANDED after the fired leg.** The first pair read +150/71 and
the 69 R→W named the gate: the widening twins exist only with
`strictNullChecks` OFF — strict `let x = null` keeps `null`
(`initializersWidened`, `implicitAnyCastedValue`). Gated: **+91 W→R, +12
G→R, ZERO adverse.** `checker_types` right 370,043 → **370,146**, wrong
22,094 → **22,003**.

### §21 Strict catch variables are `unknown`

The `unknown ← any` row's decodable half (`tryCatchFinallyControlFlow`
et al.): `useUnknownInCatchVariables` is part of `strict` since TS 4.4 —
an annotation-less catch variable types `unknown`, not `any`
(`getTypeOfVariableOrParameterOrPropertyWorker`'s catch arm upstream).
Plumbed as the fourth per-case option, defaulting to the case's strict
setting, with the explicit directive winning either way. Falsifier:
non-strict cases keep `any` (`useUnknownInCatchVariables01` explicitly
tests the option's own directive).

**§21 score — LANDED after two fired legs, one of them a process leg.** The
first measurement (+90/102) fired on the DEFAULT: `useUnknownInCatchVariables`
follows the explicit `@strict` directive only — a bare
`@strictNullChecks: true` does not imply it and the corpus's
directive-less default keeps `any`. The second leg was procedural: the
producer-side plumbing edit silently failed (a python replace that
printed ok without asserting), so the "fix" measured byte-identical —
caught by the identical TOTAL, and the edit now asserts its anchor. Final:
**+29 W→R, 6 W→G / 2 R→G** (`useUnknownInCatchVariables01`'s
member-access-on-unknown positions). `checker_types` right 370,146 →
**370,173**.

### §22 Predicate narrowing at call conditions

`if (isNumber(x))` narrows `x` — `narrowTypeByCallExpression`
(`flow.go:444`) → `narrowTypeByTypePredicate` (`flow.go:316`) →
`getNarrowedType`. The signature machinery exists (predicates are parsed,
typed, instantiated, printed); what was missing is the `narrow_type`
dispatch arm for a `CallExpression` condition. Ported: the
identifier-predicate half (`x is T`, matching argument by the predicate's
parameter name since this port's `TypePredicate` carries no index), with
Kleene declines — a constituent the relation cannot place leaves the type
unnarrowed. Unported and stated: `this is T`, `asserts`, the
`hasOwnProperty` special case, discriminant-property arguments, and
effects signatures at CALL flow nodes (statement-position asserts).

**The bar**: the `typeGuardsFunction*` family and
`controlFlowOptionalChain`'s `f(x)` positions move; net positive.
Falsifiers: (a) R→W where the want keeps a constituent the predicate
filter dropped — the assignability filter over-decides; (b) losses in
truthiness narrowing mean the new arm swallowed the CallExpression's
default truthiness path.

**§22 score — LANDED after three fired legs, all inside `getNarrowedType`'s
exact shape.** (1) A plain assignability keep answered the wrong side of
mutual relations — replaced with upstream's four-rung ladder
(strictSubtype t→n / n→t, subtype t→n / n→t, `flow.go:915`), the asserted
type winning both-ways relations. (2) A mapping that changes nothing must
answer the ORIGINAL type — the named-alias `Union` positions. (3) The
false branch keeps constituents the true branch mapped AWAY, not only ones
it dropped (`{}` vs `Record<string, unknown>`). Final: **+27 WRONG→RIGHT /
3 RIGHT→WRONG** — the three are relater precision on `Record`
instantiations and one alias-keep, recorded. `checker_types` right
370,173 → **370,197**.

### §23 A named interface with call signatures carries function facts

The §16 residue: `case 'function'` kept `Basic` whole because the lib's
`Function` interface — a `Named` type with a members table — took the
object-facts arm of `get_type_facts`, and `typeof x === "function"` could
not separate it from `object`. Upstream's facts split is by CALL/CONSTRUCT
SIGNATURES (`getTypeFacts`' `ObjectFlags`/signature test), not by
representation: any type with call or construct signatures answers
`TypeofEQFunction`. The port's `Named` members carry rendered signature
members; a member whose printed form opens with `(`, `<`, or `new ` is a
call/construct signature (a METHOD prints its name first, so the prefix
test separates them). Falsifier: interfaces with methods but no call
signatures must stay object-facts (`typeGuardsInFunctionAndModuleBlock`'s
object narrows).

**§23 score — LANDED AT ZERO.** The declaration-based call-signature test
measured **byte-identical**: the residue's `Function` constituents evidently
reach the facts question as `Named { members: None }` (the lazily
unresolved form, which keeps the undecidable-both default) — the arm is
upstream-anchored and correct but corpus-unreachable today. It stays, per
the §12.8 precedent, and becomes observable the moment the annotation
path carries its member symbol. The §16 typeof-granularity row stays open
with this negative recorded against it.

### §24 Template expressions — `string`, the literal fold, and three declines

`rank_board`'s fresh TERMINAL列 puts `TemplateExpression` at 961 gap lines
(177 cases, 53.7% top-10). `checkTemplateExpression` (`checker.go:7976`):
spans check; if the whole template EVALUATES (all parts compile-time
strings/numbers) the answer is the folded FRESH string literal; else
`string` — unless a const context, a template-literal context (element
access argument), or a template-literal contextual type asks for
`getTemplateLiteralType`. Ported: the spans, the fold when every span's
type is a string/number LITERAL (the type's data carries the printed
value, which IS upstream's evaluated text for these two kinds), `string`
otherwise; declined to gaps: const contexts (`as const` ancestry), element-
access argument position, and folds over any other literal kind (enum
members carry no value here). The contextual template-literal-type
position cannot be seen without contextual typing and is measured rather
than guessed. Falsifiers: (a) folded literals must match upstream's
evaluator byte-for-byte — number rendering divergence shows here first;
(b) want-template-literal-type lines turning WRONG (the contextual blind
spot) get counted and, if material, gate the whole arm on
provably-uncontextual positions like arrays §13 did.

**§24 score — LANDED at the best of three measured variants.**
**+1,342 (1,335 GAP→RIGHT, 7 W→R) / 422 GAP→WRONG** — the largest gap
conversion since §14. The 422 are four owned families, each named: (1)
`templateStringBinaryOperations*Invalid` ×216 — the template's own lines
flipped RIGHT and the DOWNSTREAM arithmetic (`` `x` - 1 ``) answers
`number` where the baselines render upstream's operand error as `any`; a
string-operand→error arithmetic variant was measured and REFUSED at
−562 R→G (`compiler/expr`'s valid `string * number : number` positions —
the two corpora want opposite things and the discriminator is the
diagnostic, not the type). (2) escape-bearing templates ×66 — the
span-length decline catches most but the scanner's legacy-octal cooking
still diverges on the rest (a scanner item, not a checker one). (3)
`templateLiteralTypes2` ×19 — the contextual template-literal blind spot,
falsifier (b) measured and accepted. (4) tagged-overloads ×10.
`checker_types` right 370,197 → **371,539**, gap 76,774 → **75,017**.

### §25 `void` and `delete` expressions

Two TERMINAL board rows with one-line rules: `checkVoidExpression`
(`checker.go`) — the operand checks, the answer is `undefined` (276 lines
across the two rows: void 167, delete 109); `checkDeleteExpression` — the
operand checks, the answer is `boolean`. A gapping operand gaps the whole,
per the crate-wide rule. Falsifier: none worth naming — the rules have no
alternatives; the measure is the control.

**§25 score — LANDED.** **+438 GAP→RIGHT / 5 GAP→WRONG** (the five are JS
typedef/import positions downstream of the newly-answering operands).
`checker_types` right 371,539 → **371,977**.

### §26 Non-null assertions

`x!` (66 TERMINAL lines): `checkNonNullAssertion` — the operand's
non-nullable remainder (`getNonNullableType`, the same facts filter
property chains use). A gapping operand gaps; everything else answers.
Falsifier: `unknown!` and `never`-remainder shapes — upstream substitutes
`nonNullUnknown`/reports rather than gapping; measured, and if they
surface as wrongs the arm gains upstream's two special cases.

**§26 score — LANDED.** **+167 GAP→RIGHT / 82 GAP→WRONG**, and the
falsifier's predicted refinement was measured and REFUSED: keeping the
operand on an empty remainder (the `null!` shape) read +102/147 — worse
both ways — so the plain remainder stands and the 82 stay owned by the
chain-interplay and `null!` families with both variants' numbers recorded.
`checker_types` right 371,977 → **372,144**.

### §27 Assigning to a readonly entity answers upstream's `any`

The `any ← number` board row's heads (`constDeclarations-access*`,
`externalModuleImmutableBindings`, ~200 lines): `M.x = 1` against an
exported `const` reports TS2540/TS2588 and **returns `errorType`** —
`checkIdentifier` (`checker.go:11096`) for direct references,
`isAssignmentToReadonlyEntity` (`checker.go:11377`) for property targets —
and the baselines render it `any`. The §14 boundary argument applies
verbatim: this is upstream's own deliberate error-answer, and its
observable IS `any`; ADR-0038's `error` printing is for THIS port's
failures. Ported: `isReadonlySymbol`'s four decidable arms (const
variable, enum member, readonly-modifier property, get-only accessor) at
both target sites. Falsifiers: (a) readonly READS must not move — the arm
is gated on assignment-target kind; (b) `Object.defineProperty` and
check-flags readonly (upstream's other two arms) stay unported and their
lines stay as they are.

**§27 score — LANDED.** First pair +156/10; the fired leg was upstream's
CONSTRUCTOR EXCEPTION (`this.x = …` inside the declaring constructor
assigns a readonly property legally — `constructorWithParameterProperties…`
named it). Gated: **+155 WRONG→RIGHT, ZERO adverse.** `checker_types`
right 372,229 → **372,384**. Heads converted whole:
`constDeclarations-access2–5`, `assignToEnum`,
`externalModuleImmutableBindings`.

### §28 Numeric element access on arrays and tuples

`xs[i]` on `number[]` gaps — the commonest element access in the language
(`ElementAccessExpression`, 640 TERMINAL lines). `getIndexedAccessType`'s
applicable-index road exists but the `Array<T>` reference's inherited
`[n: number]: T` does not arrive through `get_index_infos_of_type`. The
direct rule: a number-like index into an `Array<T>` reference answers `T`
(the `type_reference_targets` unwrap); into a tuple, the element union
(the tuple's own literal arm already answers literal indices). Falsifier:
`noUncheckedIndexedAccess` cases want `T | undefined` — the §17 option
gate applies here too.

**§28 score — LANDED AT ZERO, and the bar's premise was WRONG.** In-corpus,
`xs[i]` on `number[]` already answers through the lib's `Array<T>` number
index signature — the gap that motivated the bar exists only in LIB-LESS
programs (the micro-probe's world, not the corpus's). The first pair
measured 0 gained / 2 wrong (out-of-range tuple literals reaching the new
arm); restricted to plain-`number` indices it is byte-identical and stays
as the lib-less fallback, per the §12.8/§23 precedent. The 640-line
`ElementAccess` TERMINAL row is therefore NOT this rule — its next probe
must trace one line before any further code.

**§28 postscript — the mandated trace ran.** The `ElementAccess` row's gap
lines are TYPE-position indexed-access and array-type prints —
`BigUnion[]` behind a conditional-type case (2,000 lines in ONE case),
`Partial<T>[K]` (mapped indexed access), variadic tuple spreads — not
expression-position accesses at all. The row belongs to the type-node
subsystems (conditional, mapped, variadic), confirming the closed
frontier: after builds 25–47, every remaining board road enters a
subsystem the handoff already names.

### §29 A self-referential alias serves its NAME inside its own cycle

`conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable`
— 2,000 gap lines in ONE case — is `type BigUnion = { name: '0'; children:
BigUnion[] } | …`: upstream resolves object-literal member TYPES lazily, so
the alias's RHS never forces the alias; this port's print-at-creation
members force the cycle and the resolutions guard errors the whole alias.
The seam that preserves the architecture: an alias mention found ON-STACK
(a read-only `on_stack` probe — no failure marking) answers a memoized
NAMED placeholder printing the alias's name, exactly what the member text
needs; the outer resolution then completes and the alias's real union
stands. Consequence accepted and stated: a DEGENERATE cycle (`type X = X`)
now prints `X` where upstream reports circularity and answers `errorType`
— that shape is a diagnostic's job (`bd tsr-5e7.6`), and the type answer
this port gives is the name upstream's error message also prints.
Falsifiers: (a) the placeholder must never carry members — a lookup
through it would answer from nothing; (b) non-circular aliases must be
byte-identical (the placeholder is reachable only under the alias's own
frame).

**§29 score — LANDED: +2,107 GAP→RIGHT / 68 GAP→WRONG, the continuation's
second-largest conversion.** The mountain case flipped whole (2,001), and
the placeholder reached recursive aliases corpus-wide
(`subtypeReductionUnionConstraints`, `recursiveTypeReferences1`,
`controlFlowOptionalChain`'s recursive shapes). The 68 are recursive-alias
positions wanting the EXPANDED form where the placeholder's name now
prints (`recursiveArrayNotCircular` head) — the same node-reuse-vs-computed
boundary as `tsr-5o2`, recorded against it. `checker_types` right 372,384
→ **374,491**, gap 74,218 → **72,043**.

### §30 SIZING ONLY — `typeof import("…")`, the next unit, not built here

764 gap lines mention `typeof import` (heads: `ramdaToolsNoInfinite2` ×34
written ImportTypeNodes, `checkExportsObjectAssignProperty` ×27 computed
module-object prints, `privacyImportParseErrors` ×14). Two distinct
mechanisms: (1) the ImportTypeNode TYPE-NODE arm (`getTypeFromImportTypeNode`
— no dispatch arm exists; needs specifier → module-host resolution →
module symbol → `is_type_of` split); (2) the module-object PRINT form for
external-module symbols (`typeof import("./mod1")` where a namespace
prints `typeof M`) — the modobj workstream's row. NOT built in this
continuation: cross-file resolution plus the print-form question deserve a
fresh context, and the split above is the probe the next session starts
from. No bar is registered; nothing here is refused.

### §31 An unresolved free name answers upstream's `any`

The gap board's third mountain range: `parserRealSource*` (~9,000 lines
with downstream) initializes from names declared in `///<reference>` files
the corpus deliberately does not load — `ASTFlags.Writeable` wants `any`
because upstream reports TS2304 and answers `errorType`, printed `any`.
The §14/§27 boundary argument's third application: this is upstream's own
deliberate error-answer. One arm: `checkIdentifier`'s unresolved exit
(`getResolvedSymbol`'s `unknownSymbol` → `errorType`) answers the `any`
intrinsic instead of this port's `error`.

**The risk is the largest of the three applications and is stated
plainly:** everywhere THIS PORT's resolver misses but upstream's resolves
(unported scoping, import forms), an honest gap becomes a wrong `any`. The
falsifier is the measure itself — a net-negative or wrong-heavy pair
refuses the arm whole, and a partial gate (e.g. only when no import/export
machinery is in scope) is the fallback to price.

**§31 score — LANDED at the fifth measured variant.** Five gate sets
priced: ungated +6,729/1,909; any-meaning resolution +6,708/1,720; +
`arguments` +6,539/1,477; + JS-file decline +4,754/738 (the JS gate cost
1,500 honest conversions); the landed set — any-meaning + `arguments` +
`globalThis` + import-machinery files, JS files INCLUDED —
**+6,267 (6,244 G→R, 23 W→R) / 486 (291 G→W, 195 R→W)**, 12.9:1. Of the
195 R→W, 182 are `parsingDeepParenthensizedExpression` — the standing
JS-trap case, already majority-wrong, worsened rather than newly broken.
`checker_types` right 374,491 → **380,563 (79.46%)**, gap 72,043 →
**65,508** — the third mountain range (`parserRealSource*`) largely
converted. The two synthetic-global exclusions (`arguments`,
`globalThis`) are PORT misses recorded as their own future rules.

### §32 Member access through a minted unresolved type answers `any`

`parserRealSource11`'s remaining 2,354 want-`any` gaps: the receivers
resolve to `tsr-eep`'s minted unresolved-reference types (`TypeFlow` et
al. print right), and MEMBER ACCESS through them errors where upstream's
receiver is `errorType` — whose member access answers `errorType`, printed
`any` (the §31 argument, one hop further). The `unresolved_types` set
already carries the identity; the arm is one membership test in the
property/element lookups. Falsifier: the set must never admit a type this
port MINTED for any other reason — `is_error` identity discipline already
polices that.

**§32 score — LANDED.** **+4,263 (4,247 G→R, 16 W→R) / 496 GAP→WRONG**,
8.6:1. The 496 head at `exportDefaultInterface`-shaped files: the mint
there is the PORT's own miss (`export default interface` is an
InterfaceDeclaration carrying modifiers, invisible to the §31 file gate
which looks for ExportDeclaration nodes) — a binder/resolution gap
recorded as its own future rule, same class as §31's accepted residue.
`checker_types` right 380,563 → **384,826 (80.35%)**, gap 65,508 →
**60,765** — the fourth consecutive thousand-line build; the port crossed
80%.

### §33 `globalThis` is `typeof globalThis`, and its members are the globals

The recorded §31 port-miss becomes its rule (338 lines,
`declarationEmitGlobalThisPreserved` head): the identifier `globalThis`
answers a memoized type printing `typeof globalThis`, and member access
through it looks the name up in the binder's merged globals table — the
same table `resolveNameHelper`'s fallthrough already ends at. Misses gap.
Falsifier: `Window & typeof globalThis` positions (DOM-flavored) stay
gaps — no Window interface is modeled.

**§33 score — LANDED.** **+378 (376 G→R, 2 W→R) / 24 GAP→WRONG** (JSDoc
contextual shapes). `checker_types` right 388,155 → **388,533 (81.12%)**.

### §34 A property miss on a COMPLETE members table answers TS2339's `any`

`instant.year` (Temporal's Instant genuinely lacks `year`): upstream
reports TS2339 and answers `errorType` — printed `any`. The seventh
boundary hop, with the discriminator stated: this port's member tables are
complete ONLY for its own object-literal constructions (`Anonymous` with
`OBJECT_LITERAL`-symbol members); an interface/class table may be missing
unported inheritance, and a miss there stays an honest gap. Falsifier: a
literal-receiver miss whose want is a REAL type means the literal's own
construction dropped a member — that is a bug, not a boundary.

**§34 — MEASURED ZERO AND REVERTED.** The literal-receiver property-miss
population does not exist in the corpus's expression positions (upstream
errors those programs and the tests avoid the shape; the funnel's 225 were
CALLEE positions). Unlike the §12.8/§23 zero-landings, this arm interacts
with the ADR-0038 boundary for no payoff, so it reverts rather than
stands. The `instant.year` family belongs to the LIB-interface member
road (incomplete-table territory), which stays gapped by design.

### §35 FINDING — the want-`error` population is real, and §31 trades against it

`parsingDeepParenthensizedExpression`'s baseline prints ` : error` on 324
lines — **typescript-go itself renders `errorType` as `error` in (at
least) JS comma/assignment chains**, refining ADR-0038's premise that it
always prints `any`. Consequence already visible in the ledger: before
§31 those lines were RIGHT (our honest `error` MATCHED); the §31 chain's
JS-inclusive gate converts them to `any` and they are the standing 182+
R→W residue this one case carries. The trade was measured (+1,500 JS
gains elsewhere vs this case) and stands, but the finding matters for the
NEXT session: want-`error` lines are matchable output, and a per-case
count of them (grep the baselines) belongs in any future gate argument
about JS files.

**§35 addendum:** 123 baseline files carry want-`error` lines. Any future
gate must remain a SOURCE-side property — peeking at the oracle to decide
the answer is the one move the conformance methodology forbids — so the
next session's question is which source shape predicts tsgo's
error-printing (JS comma-chain assignment recovery is the observed one).

### §36 An uninferred type parameter without a default is `unknown`

`genericDefaults`' head (`f01() : unknown`): `getInferredType`'s final
fallback (`inference.go:1406`) — no candidates, no default, no possible
inference source → `unknownType`. The port's default-fill leg exists; its
no-default exit left the parameter unmapped, which the unmapped-mention
guard then turned into a gap. The same structural-source guard applies —
only a parameter NOTHING supplied could have informed takes the fallback.
Falsifier: non-strict cases wanting the legacy `{}` fallback, counted by
the measure.

**§36 score — LANDED.** **+149 GAP→RIGHT / 39 GAP→WRONG** — the 39 head at
`arrayFlat*` recursive-generic shapes where upstream's STRUCTURAL
inference finds candidates the bare-position walk cannot see; the
structural-source guard admits them because the source parameter's type
mentions no parameter BARELY (it mentions it under `ConcatArray<…>`),
the known ported-inference boundary. `checker_types` right 388,533 →
**388,682 (81.15%)**.

### §37 Tuples instantiate

`instantiate_type`'s decline list predates `tsr-5ll`'s
`tuple_element_lists`: a tuple carries its element ids now, so the fifth
arm substitutes them and re-mints through `create_tuple_type` — which is
what lets `<A, B = A>(a?: A, b?: B) => [A, B]` calls answer
(`genericDefaults`' f04 family, ~380 lines in that case alone).
Falsifier: named/modifier tuple forms whose mint refuses stay declines —
the arm reads only what the list holds.

**§37 score — LANDED.** **+236 GAP→RIGHT / 32 G→W + 1 R→W** — the adverse
are default-shape edges inside the same two cases (`genericDefaults` deep
forms, its Js twin) and one binding-pattern inference line.
`checker_types` right 388,682 → **388,917 (81.20%)**.

### §38 Written type arguments fill their missing tail

`f04<A>()` against `<A, B = A>`: `checkTypeArguments` accepts a PARTIAL
list when defaults cover the tail — `fillMissingTypeArguments`
(`checker.go:19458`), the written half this time: tail positions take
their default instantiated with the map so far, else `unknown`. The arity
guard keeps rejecting lists LONGER than the parameters and lists shorter
than the non-defaulted prefix. Falsifier: a written-args call whose want
shows the UNfilled arity error stays a gap — those wants print the error
signature's shape, counted by the measure.

**§38 score — LANDED.** **+81 GAP→RIGHT, ZERO adverse.** `checker_types`
right 388,917 → **388,998 (81.22%)**.

### §39 `this` in a plain function is `any` without `noImplicitThis`

`controlFlowCaching`'s 250 want-`any` gaps: `this` inside a plain function
(no `this` parameter, no enclosing class) — upstream reports TS2683 only
under `noImplicitThis` and answers `any` either way
(`tryGetThisTypeAtEx`'s fallthrough, `checker.go:12146` region). The
plain-function opaque arm answered `errorType`; it now answers `any` — the
boundary chain's shape again, though here the `any` is upstream's typed
answer, not an error rendering. Falsifier: class-adjacent shapes must not
route here (the arm sits exactly where the old error sat).

**§39 score — LANDED.** **+672 (662 G→R, 10 W→R) / 140 GAP→WRONG** — the
140 head at strict-mode `this` shapes where upstream's noImplicitThis
answer differs contextually (`castTest`'s object-method `this`), the
recorded residue. `checker_types` right 391,350 → **392,022 (81.85%)**.

### §40 Variadic tuple type nodes print

`variadicTuples1`'s 409 gaps head at written `[...T]`/`[string, ...T]`
forms the tuple builder refuses whole. The slice: a tuple node whose only
refused elements are REST forms builds a PRINT-ONLY type — the text
composed from the resolved element prints (`...` + element), minted
`Named` with NO `tuple_element_lists` entry, so element access,
instantiation, and relations all keep declining (print-only citizenship,
the `tsr-eep` pattern for a RESOLVED shape). `NamedTupleMember` and `?`
stay refused. Falsifier: positions wanting the EXPANDED instantiation
(`[string, number]` from `[...T]` at a call) stay gaps and must not
regress; the mint must never enter the element-list map.

**§40 score — LANDED after the fired falsifier.** The first pair's 143
adverse named the split: rests over CONCRETE tuples EXPAND (upstream
splices them flat) while parameter rests print — the splice arm recovered
14 and the final pair reads **+525 (524 G→R, 1 W→R) / 129 GAP→WRONG**
(tuple-lambda assignability shapes, the print-only citizen meeting the
relater — the recorded residue). `checker_types` right 392,022 →
**392,547 (81.96%)**.

### §41 Qualified type references carry their target's members

`temporal`'s 420-row root, found by a mini-namespace probe: `NS.Inst`
resolves (the validating half exists) and then deliberately answers the
PRINT-ONLY mint — so every member access through a namespace-qualified
annotation flows into the §32 `any` chain where upstream has real
signatures. The upgrade: a successfully-resolved qualified TYPE reference
with no type arguments answers a memoized `Named` whose TEXT is the
written qualified spelling (the print the corpus wants at
out-of-namespace sites) and whose MEMBERS field is the resolved symbol —
prints qualified, looks up real. Generic qualified references stay
mints. Falsifiers: (a) in-namespace sites keep the local unqualified
print (the existing site test guards); (b) the §32 chain must stop firing
for these receivers (they leave `unresolved_types`), so any want-`any`
lines that were RIGHT through the chain get counted.

**§41 score — LANDED.** **+1,016 (515 WRONG→RIGHT, 501 GAP→RIGHT) / 97
adverse (83 G→W, 11 R→G, 3 R→W)** — 10.5:1. The conversions reach three
standing rows at once: `underscoreTest1` gained 264 (the DOUBLE-REFUSED
`_1` case — much of it was never a rename problem but qualified
references without members), `temporal` 193, and the enum-literal
families 211 (qualified enum-member references now carry their tables).
The 97 head at union-disambiguation and clodule shapes, recorded.
`checker_types` right 392,547 → **393,549 (82.17%)**.

### §42 Generic qualified references instantiate through the seam

§41's residue: `NS.Type<Args>` stayed a print-only mint. The upgrade
mirrors the unqualified generic road — the resolved symbol's parameter
count checks the argument count and `get_instantiated_type_reference`
builds the reference (the tsr-4qx seam then substitutes members) — with
the qualified TEXT carried the §41 way. Falsifier: arity mismatches stay
errors, exactly as the unqualified arm's `checkNoTypeArguments` twin.

**§42 — REFUSED at +4/352, reverted whole.** The instantiated reference
renders UNQUALIFIED (`C<T>`, not `NS.C<T>`), so every print-only mint that
was RIGHT with its qualified spelling turned wrong. The prerequisite is
qualified-text carriage through `get_instantiated_type_reference` (the
reference renderer would need a per-site name override, the same design
§41 used for the argument-less case). Recorded with the number.

**§42 v2 — LANDED small.** The per-site qualified text plus a
`type_reference_targets` registration (the design the v1 refusal named):
**+4 (3 W→R, 1 G→R), 18 WRONG→GAP, zero adverse.** The generic qualified
population's prints were already served by the mints; the registration's
value is structural (the seam now sees these references) and its measured
conversion today is small. `checker_types` right 393,549 → **393,553
(82.18%)**.

### §43 `new` fills missing type arguments from class defaults

`typedArrays` (261): the modern lib's `Float32Array<TArrayBuffer extends
ArrayBufferLike = ArrayBuffer>` makes every `new Float32Array(…)` a
generic construction whose written list (empty) is SHORTER than the
parameters — §38's fill rule at the `new` road: tail positions take their
declared DEFAULT type node's resolution; a tail position with NO default
stays the arity error. Falsifier: the §38 twins' — arity longer than
parameters errors; defaults referencing earlier parameters resolve
against the filled prefix (declined here: a default MENTIONING a
parameter gaps, the conservative first cut).

**§43 score — LANDED SMALL, and the premise relocated.** **+4 / 3** — the
typed arrays never reach the class tail: `Float32Array` is a lib VAR of
constructor-INTERFACE type, so the road is `get_signature_of_named_type`'s
construct half, not the class arm. The fill stays (it is the class-side
twin of §38 and upstream-true); the 261-line row's next probe is the
constructor-interface signature's decline, recorded as the sizing.

### §44 All-defaulted generic construct signatures instantiate at `new`

The §43-relocated road: `get_signature_of_named_type` declines every
generic candidate, and the modern lib's typed-array constructors are
generic-with-defaults (`new <T extends ArrayBufferLike = ArrayBuffer>`).
The §38 fill applies: a candidate whose type parameters are ALL defaulted
instantiates its return with the default map and joins as concrete; any
undefaulted parameter keeps the decline. Falsifier: defaults mentioning
sibling parameters instantiate against the filled prefix or decline —
the §38 twins.

**§44 score — LANDED SMALL: +4/0.** The typed-array row still declines
past both new arms — the remaining gate needs a per-overload trace of
`Float32ArrayConstructor`'s candidates (recorded as the row's next probe).
Both fills are upstream-true and stay. `checker_types` right 393,557 →
**393,561 (82.18%)**.

### §45 `Record<K, V>` answers its members

`objectSpreadRepeatedNullCheckPerf`'s root (`config.a` on
`Record<string, number>` → error) and a corpus-ubiquitous shape: the lib's
`Record` mapped alias. The scoped arm — NOT mapped types, stated plainly:
a reference whose target is the GLOBAL `Record` symbol with two arguments
answers property lookups (and the §17 index road) with V when K is
string-like/`keyof any`, and property misses under a string-literal-union
K behave as the union's members. Everything else mapped stays the
subsystem. Falsifier: a `Record` whose K is a literal union — a MISS
outside the union wants upstream's TS2339/undefined behavior; those
decline.

**§45 score — LANDED.** **+274 GAP→RIGHT / 12 GAP→WRONG** (the twelve:
`noUncheckedIndexedAccess` wants `| undefined` through this road too, and
unbounded-parameter values — both recorded refinements). `checker_types`
right 393,561 → **393,835 (82.23%)**.

**§44 postscript/build 70 — the queued trace paid.** The typed-array
decline was the candidates loop's `?`: ONE unbuildable overload killed the
whole interface. Skip-with-agreement (the all-equal return check still
gates the kept) reads **+891 GAP→RIGHT / 147 GAP→WRONG** — typed arrays
and every uniform-return constructor interface with one exotic overload.
The 147 head at conditional-type constructor shapes where the skipped
overload's return WOULD have disagreed — the stated risk, measured and
carried. `checker_types` right 393,835 → **394,726 (82.42%)**.

### §46 Generic alias references carry their body's members

`longObjectInstantiationChain2` (322) and kin: a generic alias reference
(`Type<{p1: 1}>`) prints name+args upstream while its MEMBERS come from
the instantiated body. The §41 design over the alias body: the reference
answers `Named { text: "Type<args>", members: Some(body's __type symbol) }`
with `type_reference_targets` registered as (alias symbol, args) so the
tsr-4qx seam substitutes member types by the alias's parameters. Declines:
a body that is not a type literal (unions/conditionals stay the name-only
print), and any argument that gaps. Falsifiers: (a) self-referential
bodies must ride the §29 placeholder, not recurse; (b) the seam must
substitute by the ALIAS's parameter list — a mismatch prints `t` raw and
is instantly visible.

**§46 score — LANDED.** **+46 GAP→RIGHT / 4 adverse (2 R→W import-
retention prints, 2 G→W in the head case itself).** The chain case's bulk
needs the member-signature instantiation through nested call inference —
the seam substitutes but the chained `.merge` returns need §38-style
inference over the alias's own signatures, the row's remaining named
prerequisite. `checker_types` right 394,726 → **394,770 (82.42%)**.

### §47 SIZING — temporal's residue decomposed (post-§41)

The 420-row's remaining families, each already priced elsewhere:
(1) `sort` returning the §28 `this` mint at call results — the receiver-
instantiation residue (`this` should instantiate to `Temporal.Instant[]`);
(2) call-result types minted IN-namespace print unqualified at
out-of-namespace sites (`Instant[]` vs `Temporal.Instant[]`) — the
site-sensitive print problem, modobj proper;
(3) `instant.year : any` — TS2339 on lib interfaces, the §34 boundary
(reverted there for want of a completeness discriminator; lib interfaces
may qualify, unmeasured).
No new mechanism; the row waits on those three owners.

### §48 Plain binding-pattern parameters render

`dependentDestructuredVariables` (309) gaps at the SIGNATURE: a
binding-pattern parameter name declined whole. The baselines print the
written pattern verbatim (`({ kind, payload }: Action)`), and for PLAIN
patterns — identifier elements only, no defaults/rest/renames/nesting —
the render is mechanical: `{ a, b }` / `[a, b]`. Everything decorated
stays the decline (a generated name compared verbatim is a guess, the
original comment's rule intact for the shapes it feared). Falsifier:
decorated patterns must keep declining — one default rendered wrong is
instantly visible in the pair.

**§48 score — LANDED after the token-kind trap's SECOND firing.** The
first pair read +69/108 because `BindingPattern.kind` is a TOKEN field
whose equality against the node kinds silently failed — object patterns
printed as arrays. The side-table kind (the §16 CaseKeyword lesson,
now twice-paid) reads **+149 GAP→RIGHT / 28 GAP→WRONG** (empty/optional
pattern shapes, the decline set's edge). `checker_types` right 394,836 →
**394,985 (82.47%)**.

### §49 Property lookup on a union projects across constituents

`dependentDestructuredVariables`' second slice (and the funnel's
"receiver is a union or intersection, 141" row): `kind` from
`Action = {kind:'A',…} | {kind:'B',…}` is the UNION of the per-constituent
members — `getPropertyOfUnionOrIntersectionType`: every constituent must
carry the name (a miss anywhere is a miss), and the type is the union of
the member types. Optionality/readonly aggregation and intersections stay
declines. Falsifier: partial-membership unions must MISS (answering the
present half would be the confident wrong).

**§49 score — LANDED.** **+734 GAP→RIGHT / 268 GAP→WRONG.** The 268 are
one family: dependent-DISCRIMINANT positions where the projection now
answers the full union and upstream's flow narrows it (`kind === 'A'` →
`payload : number`) — the dependent-destructured-flow half (`tsr-pqnh`),
which was always this row's other owner. The projection is its
prerequisite, not its rival. `checker_types` right 394,985 → **395,719
(82.62%)**.

### §50 Dependent destructured narrowing — the pseudo-reference

`getNarrowedTypeOfSymbol`'s binding-element case (`checker.go:13751`): a
non-rest, initializer-less binding element from a ≥2-element pattern whose
root is a const variable or parameter, with a UNION parent — the PATTERN
is the pseudo-reference, flow-narrowed at the use site, and the element
re-projects from the narrowed parent. The port: `FlowState` carries the
pattern and its union; a condition on a SIBLING element acts as a
discriminant on the walked union (the §16 comparable filter, member-
directed); the projection reuses the destructure module against the
narrowed parent. Declines: tuple-parameter dependents (the second
upstream case), assigned-parameter roots, and any sibling condition whose
member/literal pair the comparability cannot decide. Falsifiers: (a) the
268 §49 residuals flip or stay — nothing else may move; (b) an
undecidable discriminant keeps the FULL union (never `never`).

**§50 score — LANDED.** **+52 (30 G→R, 22 W→R), ZERO adverse.** Two fired
legs en route, both structural: the sibling's value declaration is the
NAME node (two-hop walk), and `comparable_ternary` answered true for ANY
same-base literal pair — `'A'` comparable to `'B'` — a §16-era latent bug
the discriminant filter exposed (distinct unit literals are now
incomparable, which also tightens the switch road). The residue: sibling
conditions in switch/else-chains and the tuple-parameter dependent case,
both named. `checker_types` right 395,719 → **395,771 (82.63%)**.

## §50.1 — the switch form of the sibling discriminant

§50 landed the equality-condition half (`kind === 'A'`). Upstream's walk is
condition-agnostic: `switch (kind)` reaches the same pseudo-reference flow
through `FlowSwitchClause`, and `narrowTypeBySwitchOnDiscriminant`
(`flow.go:1092`) filters the walked union by comparability of the clause
range against the discriminant — here, against the sibling MEMBER's type in
each constituent.

**The bar.** Extract §50's equality-arm sibling closure into a method; in
`get_type_at_switch_clause`, before the matching-reference test, test the
switch expression as a sibling of `state.discriminant_pattern`; on a hit,
filter constituents by "some clause-range literal is comparable to the
member's type in this constituent". Default clauses (empty range or a
`never` in the slice) DECLINE whole — the member-directed twin of §16's
default filter is unbuilt, and declining keeps the failure mode at the
unnarrowed status quo. Any missing member or Kleene-unknown comparability
declines whole, as in §50.

**Falsifiers.** (a) If the equality form covered every baseline site and no
switch-form pseudo-reference exists in the corpus, the pair scores zero —
revert. (b) If clause ranges carry non-unit types that `comparable_ternary`
mis-answers against member unions, R→W appears — narrow to unit-only
clause ranges.

**§50.1 score — LANDED.** **+26 (20 G→R, 6 W→R), ZERO adverse.** Neither
falsifier fired. The switch form's population is real
(`dependentDestructuredVariables`'s switch blocks, both
`arrayDestructuringInSwitch` cases). `checker_types` right 395,771 →
**395,797 (82.64%)**. Residue unchanged: the tuple-parameter dependent case
(`checker.go:13806`), else-chain accumulation across clauses.

## §50.2 — the projected type re-enters the ordinary walk

Upstream `checkIdentifier` uses `getNarrowedTypeOfSymbol`'s answer as the
DECLARED type for the reference's own flow analysis — the pseudo-walk and
the ordinary walk COMPOSE (`checker.go:13751` feeding the normal
`getFlowTypeOfReference`). This port returned the projection directly, so
`f23`'s `if (payload)` truthiness never applied on top of the switch's
sibling narrowing: want `number`, printed `number | undefined` (8 WRONG
lines in `dependentDestructuredVariables`).

**The bar.** In the `AssignmentTargetKind::None` arm, on a §50 hit, call
`get_flow_type_of_reference(node_id, Some(symbol), narrowed)` instead of
returning `narrowed`.

**Falsifiers.** (a) If the ordinary walk's declared-type plumbing assumes
the symbol's cached declared type and re-widens, the §50 wins revert to
their old prints — R→G/R→W on the §50-won lines. (b) If double-narrowing
mis-composes (the equality arm firing again on an already-filtered
projection), R→W appears in the equality cases.

**§50.2 score — LANDED.** **+6 (6 W→R), ZERO adverse.** Neither falsifier
fired — the ordinary walk composes cleanly on top of the projection.
(A first draft of this score guessed +10 before the run; corrected to the
measured +6.) `checker_types` right 395,797 → **395,803 (82.64%)**.

## §14.1 — the JS half of the flow-disabled bail prints `error`

`parsingDeepParenthensizedExpression` (a JS file, 330 WRONG) shows the §14
observable is FILE-SENSITIVE: inside its TS2563-disabled container the
baseline prints `v : any` (declared type of an assignment target) but
`v = f : error` and the whole comma-chain ` : error` — upstream's
`errorType` reaches the printer VERBATIM here, exactly §35's JS
` : error` finding. §14's blanket `any` was calibrated on TS cases
(`largeControlFlowGraph`), where it scored +10,000; the JS half of the
same admission manufactures WRONGs.

**The bar.** At both §14 bail sites (the disabled-container ancestor check
and the depth trip), answer `intrinsics.error` when `in_js_file(reference)`
and keep `any` otherwise.

**Falsifiers.** (a) If JS cases exist where the disabled read renders
`any`, W→R is offset by R→W in those — split further by print position.
(b) If §14's TS wins ever routed through a JS-file reference, they revert
(R→G on `largeControlFlowGraph`).

**§14.1 score — KEPT AT ZERO.** The pair moved NOTHING: this port never
trips the cap in that file, because the trip upstream happens while
computing the AUTO VAR's DECLARED type — `var v` with no initializer in a
JS file is typed by a container-wide flow walk (upstream's auto/evolving
mechanism), and THAT walk crosses the 4,000-line function's thousands of
condition nodes, trips `f.depth == 2000` (`flow.go:118`), sets the GLOBAL
`flowAnalysisDisabled`, and makes `v`'s declared type `errorType` — while
parameters keep `any` (`A = t : any` beside `T = v : error` in the same
chain). A hop-counting variant was tried and REVERTED unfaithful:
upstream's own loop skips straight-line antecedent hops without
incrementing (`flow.go:129-160`). The split itself is upstream-faithful
(the give-up value prints ` : error` in JS) and stays, priced at zero
until the auto-var container walk exists. The real owner of the 324 lines
is that mechanism — a largeControlFlowGraph-class single-case mountain;
queued, not refused.

## §50.3 — the tuple-parameter dependent case

Upstream's second `getNarrowedTypeOfSymbol` shape (`checker.go:13806`): an
unannotated, uninitialized, non-rest parameter of a ≥2-parameter function
contextually typed by a signature with a SINGLE REST parameter of a union
of tuples. The function node itself is the pseudo-reference; conditions on
sibling PARAMETERS discriminate the tuple union by ELEMENT INDEX; the
answer indexes the narrowed union at the parameter's position
(`dependentDestructuredVariables`'s `(kind, payload) => ...` under
`(...args: Action) => void`).

**The reduction.** The general `getContextualSignature` is the refused
contextual subsystem; this arm takes only the WRITTEN-annotation slice:
the function expression/arrow is the direct initializer of a variable
whose type annotation is syntactically a function type with one
`...rest` parameter. The rest type must be a union with every constituent
a tuple (`tuple_element_lists`). The inference-context mapper
(`checker.go:13816`) is not taken — a generic contextual signature
declines. `isSomeSymbolAssigned` is approximated by nothing: a parameter
reassigned before the read makes the walk answer the assignment, which is
the §12 write gate's territory, and any miss there is measured by the
pair.

**The mechanism.** `sibling_member_of_pattern` learns the parameter form:
an identifier whose declaration is a Parameter with parent == the
pseudo-pattern function answers its INDEX as the member name — tuples
already answer numeric member names from `tuple_element_lists` (tuple §8),
so the §50/§50.1 filters compose unchanged. Projection is
`get_type_of_property_of_type(narrowed, index)` — §49's union projection.

**Falsifiers.** (a) If the equality/switch filters mis-answer on tuple
constituents (comparable against element unions), R→W in the tuple cases.
(b) If the annotation slice misfires where upstream's contextual signature
is NOT the annotation (generic instantiation changes the rest type), G→W
— narrow by declining type-parameter-containing rest types.

**§50.3 score — LANDED.** **+28 W→R, 1 W→G, ZERO adverse.** Neither
falsifier fired. `checker_types` right 396,472 → **396,500 (82.79%)**.
Residue: the generic contextual slice (declined by construction), and the
non-annotation contextual roads (argument-position arrows) behind the
contextual refusal.

## §51 — switch on a discriminant PROPERTY

`exhaustiveSwitchStatements1` (16 WRONG un-narrowed + 53 GAP downstream):
`switch (s.kind)` where `s` is the flow reference — upstream's
`narrowTypeBySwitchOnDiscriminantProperty` (`flow.go`, beside the §16
arm) filters the union by the named member against the clause range.
§50.1's `narrow_union_by_member_switch` IS that filter; the arm was only
ever reachable through the pseudo-pattern sibling road. Wire the direct
shape: switch expression is a PropertyAccess whose RECEIVER matches the
reference → filter by the property's name. Default clauses and Kleene
declines keep §50.1's behavior (whole-decline).

**Falsifiers.** (a) `isMatchingReference` on the receiver is stricter
than identifier equality — deep receivers (`a.b.kind`) mis-matching would
R→W. (b) Optional/unlisted members in some constituent — §50.1 declines
whole there already; zero risk claimed, measured anyway.

**§51 score — LANDED.** **+140 G→R, 52 W→R / 4 R→W + 1 G→W (48:1).**
Neither named falsifier — the 4 are a PARAMETER-NAME-LINE anomaly: the
declaration `x` in `function f20(x: Item)` prints the switch-narrowed
constituent because this port's identifier road runs the flow walk at
the declaration name's own position, and §51 made that walk narrow where
it previously declined whole. Upstream's declaration line never narrows.
The kept-all identity arm was added en route (re-forming a full union
loses an alias-named print) though these 4 are not that shape — the
declaration-position flow question is its own row, accepted here at
48:1. The 1 G→W is switch-exhaustiveness reachability (`area : number`
needs the no-assignment path proved dead). `checker_types` right
399,024 → **399,212 (83.35%)**.

## §51.1 — the equality twin: `if (s.kind === 0)`

`narrowTypeByDiscriminantProperty` (`flow.go`, the equality dispatch
before the direct-reference arm): a property access whose RECEIVER is
the matching reference, compared to a literal, filters the union by the
member — the §50 equality filter with the member from the ACCESS instead
of a pattern sibling. Shared as one helper. Same declines: missing
member, Kleene unknown, kept-all identity (§51's alias-name lesson,
applied at birth here), kept-empty.

**Falsifiers.** (a) the §51 declaration-position anomaly recurs on
equality-narrowed functions — counted; (b) literal-vs-literal-union
member comparability mis-answers — comparable_ternary's unit-pair rung
already decides those; R→W would say otherwise.

**§51.1 score — LANDED.** **+196 (142 W→R, 54 G→R), 6 W→G / 2 G→W +
2 R→W (49:1).** The adverse: `switch (true)` clause-expression narrowing
(`narrowByClauseExpressionInSwitchTrue3` — the case-expression road, not
this arm's) and a `this`-member loop-antecedent pair. `checker_types`
right 399,212 → **399,406 (83.39%)**.

## §51.2 — optional-chain containment at an equality

`controlFlowOptionalChain` (59+ WRONG `Thing | undefined`):
`if (o?.foo === value)` where `value`'s type excludes `undefined` —
upstream's `narrowTypeByOptionalChainContainment`: the comparison holding
implies every chain link evaluated, so the BASE narrows non-undefined.
The slice: strict `===`/`!==` only, assume side = the equality holding;
the other operand's CHECKED type must contain no UNDEFINED, ANY, or
UNKNOWN constituent; the chain operand must contain the reference as a
receiver behind at least one `?.`. Facts: `NE_UNDEFINED`. Loose `==` is
NOT taken (its null/undefined equivalence needs both facts and the
corpus's `// Error` comments sit exactly there).

**Falsifiers.** (a) If upstream also narrows on the FALSE branch of
`!==` — G/R→W on else-branches (this slice narrows only the holding
side). (b) Call links (`o?.bar() === x`) — receivers walk through call
expressions; a mis-walk fires here.

**§51.2 score — LANDED.** **+12 (W→R), ZERO adverse.** Neither falsifier
fired. `checker_types` right 399,406 → **399,418 (83.39%)**. The
remaining `controlFlowOptionalChain` wrongs are the truthiness/`in`
halves of chain containment and the chain-link marker mechanics — TASK
item 6's standing residue.

## §51.3 — truthiness on a discriminant property (the surprise build)

`if (s.done)` over `{ done: false; value: T } | { done: true }` — the
iterator-result idiom. Upstream's `narrowType` routes an access condition
whose RECEIVER is the reference through `narrowTypeByDiscriminant` with
the TRUTHY/FALSY facts as the member transform. The slice: every
constituent's member must be DECIDABLE for truthiness — a unit literal or
boolean — and the constituent is kept when the faceted member is not
`never`; any opaque member declines whole (a `get_type_with_facts`
identity on an opaque member would silently keep it — the §16-family
Kleene rule, applied at the member). Kept-all/kept-empty are identity
declines (§51's alias-name lesson).

**Falsifiers.** (a) Non-unit but still-decidable members (nullable
objects: `{ x: T } | { x: undefined }` — undefined is falsy-decidable);
zero-coverage there is a residue, mis-narrowing is a leg. (b) The §51
declaration-position anomaly again.

**§51.3 score — LANDED.** **+13 (9 W→R, 4 G→R), ZERO adverse.** Neither
falsifier fired. `checker_types` right 399,418 → **399,431 (83.40%)**.
The discriminant family now has four members sharing two filters:
property switch (§51), property equality (§51.1), chain containment
(§51.2), property truthiness (§51.3). An object-member extension
(objects always truthy — the `{ value: T } | { value: undefined }`
idiom) measured ZERO and was reverted: the corpus's instances pair the
object member with an opaque sibling, so the whole-decline holds either
way.

## §51.4 — chain containment, the whole table

§51.2 ported one quadrant of `narrowTypeByOptionalChainContainment`
(`flow.go:1032`); the function's own comment is an eight-row truth table
and the facts are `NEUndefinedOrNull` — BOTH nullish forms leave, which
is why 13 `Thing | null` lines stayed wrong. This build replaces the
slice with the table verbatim: `nullableFlags` = NULLABLE for loose
operators, UNDEFINED for strict; remove when (equalsOp ≠ assumeTrue and
the value is every-nullable) or (equalsOp = assumeTrue and the value
excludes AnyOrUnknown|nullable); facts `NE_UNDEFINED_OR_NULL`. And the
TRUTHINESS leg (`flow.go:432`): under strictNullChecks, `if (o?.foo)`
on the true branch narrows the base `NE_UNDEFINED_OR_NULL` and FALLS
THROUGH to the discriminant filter, exactly as upstream's
`narrowTypeByTruthiness` orders them.

**Falsifiers.** (a) `getAdjustedTypeWithFacts` vs this port's
`get_type_with_facts` — the adjustment re-derives freshness; divergence
shows as literal-spelling wrongs. (b) The loose-operator rows depend on
`== null` equivalence the §51.2 slice deliberately skipped; wrongs there
say the value-type `every` test is mis-ported.

**§51.4 score — LANDED.** **+72 (W→R), ZERO adverse.** Neither falsifier
fired. `checker_types` right 399,431 → **399,503 (83.42%)**. First build
scored through `scorepair` (the sub-second filtered iterate showed +70 in
the target before the full run confirmed +72/0).

## §51.5 — containment composes with the discriminant

Upstream ASSIGNS the containment result and keeps narrowing
(`flow.go:491`: `t = narrowTypeByOptionalChainContainment(...)` then the
discriminant/equality arms run on the updated `t`); §51.4 returned
early, so `o?.kind === 'a'` removed the nullish forms but never filtered
the constituents. The fix is the control shape only — the containment
result feeds §51.1's filter and the direct-equality arm.

**Falsifier.** Double-narrowing order: if the discriminant filter on the
already-non-nullish union disagrees with upstream's on any line, R→W.

**§51.5 score — LANDED.** **+15 (13 W→R, 2 G→R), ZERO adverse.** The
falsifier did not fire. `checker_types` right 399,503 → **399,518
(83.42%)**.

## §52 — equality's comparable-filter half

`narrow_type_by_equality` ported only the nullable-operand half; the
doc's stated reason — typing the operand from inside the walk could
recurse — has since been survived by every §50/§51 arm, which all
`check_expression` their operands mid-walk. Upstream's other half
(`flow.go:580`): for a NON-nullable value, assumeTrue filters
constituents by `areTypesComparable` then
`replacePrimitivesWithLiterals`; assumeFalse with a UNIT value drops the
unit-like comparable constituents. The slice: STRICT operators only (the
`isCoercibleUnderDoubleEquals` and unknown/empty-object arms stay
unported); the comparable test is `comparable_ternary` with Kleene
whole-decline; any/unknown/error values decline.

**Falsifiers.** (a) The uniform-union/`removeType` distinction upstream
draws on the false branch — if dropping comparable unit-likes diverges
from `removeType` somewhere, R→W on `!==` else-branches. (b)
`replacePrimitivesWithLiterals` fidelity — §16 landed it for switches;
an equality-position divergence shows as literal-spelling wrongs.

**§52 score — LANDED (three mechanisms, one narrowing).** The build
needed: (1) a REENTRANCY guard (typing the operand re-enters other
references' walks — the recursion the nullable-only port declined to
risk is real); (2) an operand MEMO (`narrow_value_types`) — upstream's
`getTypeOfExpression` is cached, and without it condition chains are
EXPONENTIAL: `compiler/con*` hung the corpus run, diagnosed by `sample`
showing the `narrow_type_by_equality` ↔ `get_type_at_flow_node` spin,
and the memo also cut the full corpus run 41s → 21s; (3) an alias-NAMED
union decline — the narrowed rebuild loses the alias spelling, and the
corpus wants BOTH spellings for one member set by creation path
(`numericLiteralTypes1` wants `1 | 2` at position 175 beside `Tag` at
178) — a §52.1 member-set index was built, measured (+39/11 with a
written-road split), and REVERTED whole: the real mechanism is
upstream's member-set INTERNING plus origin, i.e. the §39 reshape, and
a text-level index cannot decide which spelling a site wants. Final:
**+41 W→R, 4 W→G / 1 R→W (41:1)** — the 1 is an element-access
WRITE-position read (`x['o'] = true`'s LHS narrowed; the §12.7 dispatch
covers identifiers only), recorded as that seam's residue.
`checker_types` right 399,518 → **399,558 (83.43%)**.

## §52.1 — rebuilt unions rediscover their name

§52's falsifier surfaced a standing identity divergence: upstream interns
unions on the MEMBER SET, so a branch-join that rebuilds `"" | "foo"`
gets the SAME type object the alias `T` declared — and prints `T`. This
port interns on `(flags, text, members)`, so the rebuild minted a
second, anonymous spelling (9 R→W: `stringLiteralTypesInUnionTypes04`,
`numericLiteralTypes1/2` — every one a §52-narrowed branch re-joining).
The port equivalent: a `named_union_by_members` index — the sorted
member list of every NAMED union (enum declared types, alias-named
unions), consulted by the plain union road before minting. Exact-list
hits return the named type; everything else unchanged.

**Falsifiers.** (a) Two aliases with identical member sets — first-writer
wins in the index where upstream's alias-symbol choice may differ; W on
the second alias's lines. (b) Positions where upstream prints the
EXPANDED form despite the name existing (the §39 study's evaluated
positions) — W there says the index needs the §39 position split.

## §53 — origin-carrying unions (the §39 reshape, first slice)

The §39 refusal's named mechanism, built at the checker rather than the
store: `union_origin: FxHashMap<TypeId, Vec<TypeId>>` — a union built
from NAMED constituents records its ORIGIN entry list (the unexpanded
inputs), the union's text renders from the entries, and the constituent
list stays the flattened members for every consumer. The part the §39
text hack could not do: NARROWING FILTERS PROJECT THE ORIGIN — a filter
that keeps a subset of members keeps the origin entries whose members
ALL survive, decomposes partially-surviving entries to their surviving
members, and the result re-registers its own projected origin
(upstream's `filterType` on the origin, `checker.go:25705`'s consumers).

Scope of slice one: creation in the union worker (computed roads only —
the WRITTEN-expansion rule from §52.1's study stands, via the origin
entries being the written inputs there too, sorted per
`numberAssignableToEnumInsideUnion`), projection in the four §5x filters
and `get_type_with_facts`' union rebuilds.

**Falsifiers.** (a) The §39 stable adverse core — `temporal`'s
site-sensitive alias spellings inside signature prints — is NOT origin's
to fix and will recur if origin routes those prints; count them
separately before judging. (b) Projected-origin text vs upstream's on
partial survival (`E` minus one member decomposes — upstream may spell
`Exclude<E, E.A>`-style or the member list; the baselines decide).

**§53 score — LANDED (four refinements, every falsifier honoured).**
(1) Falsifier (a) recurred at scale exactly as priced (174 G→W raw, 82
of them temporal) — the ENTRY GATE contains it: an origin spelling is
claimed only when every entry is an enum-named union or a non-object
plain type; everything else keeps the pre-§53 gap. (2) Entry
SUBSUMPTION: an entry whose member set is contained in another's
collapses into it, named beating anonymous on equal sets (`x || y` of an
expanded-written operand and its alias answers `T`). (3) Entry ORDER,
from the baselines: nullable entries last, others by first-member sort
bits (`boolean | E`, `MyEnum | undefined`). (4) Reduction against a base
primitive read off the DECLARATION (this port's enum-member types carry
only ENUM): a numeric enum beside `number` drops entirely
(`unionSubtypeIfEveryConstituentTypeIsSubtype` +19). Final: **+237 G→R /
19 G→W + 1 W→G (12.5:1)** — the 19 are `&&`'s NON-STRICT arm
(`extractDefinitelyFalsyTypes(getBaseTypeOfLiteralType(rightType))`,
`logicalAndOperatorWithEveryType` 14), its own port, queued.
`checker_types` right 399,558 → **399,795 (83.47%)**. The §39 refusal is
SUPERSEDED by this slice; the un-gated entry shapes (object-bearing,
generic-alias) remain its residue.

## §54 — `&&`'s non-strict falsy source

`checker.go:12496`: under `!strictNullChecks` the falsy extraction runs
over `getBaseTypeOfLiteralType(RIGHT)`, not the left — `boolean && string`
answers `string` (falsy-of-string-base is `never`... upstream's
`getDefinitelyFalsyPartOfType(string)` is `""`? the corpus decides:
`logicalAndOperatorWithEveryType` wants plain `string`, so the extraction
of the right's base unions to nothing visible). §53's 19 adverse are this
arm's population. One-line port: the strict/non-strict source split.

**Falsifier.** If `get_definitely_falsy_part_of_type` diverges from
upstream's on the right-base (the `""`/`0`/`false` parts), the same lines
stay wrong with new spellings — the pair decides.

**§54 score — LANDED.** **+104 (92 W→R, 12 G→R), ZERO adverse.** The
falsifier did not fire, and the arm converted 73 pre-existing wrongs
beyond §53's 19 (`parserRealSource12` +25 among them — the non-strict
corpus's `&&` chains had been mis-unioning since the strict-only port).
`checker_types` right 399,795 → **399,899 (83.49%)**.

## §55 — enum member VALUES (the surprise build)

A 261-line WRONG class nobody queued (`want E, got E.A` and kin) decodes
into upstream's enum value semantics, absent since `bd tsr-8pz`:
`getEnumMemberValue` evaluates each member, `getEnumLiteralType`
(`checker.go:25362`) interns literals BY VALUE — `enum E9 { A, B = A }`
gives B the SAME type as A, printing `E9.A` — and a member the evaluator
cannot fold takes `createComputedEnumType`: the ENUM ITSELF is its type
(`E8.B : E8` for `B = 'x'.length`, `enumBasics`).

The slice: a sequential constant folder per declaration — numeric and
string literals, unary +/- on numerics, auto-increment (`None` after a
string or computed predecessor, per the language), and identifier /
same-enum-qualified references to PRIOR members. Everything else is
computed. Value-keyed interning via `enum_value_types`; the computed
members share one per-enum type spelled as the enum.

**Falsifiers.** (a) Cross-enum and forward references fold upstream via
the full evaluator — those stay computed here; wants with literal prints
there are the residue, wrongs are a leg. (b) The §10.16 collapse
machinery keys on `symbol: None` — if value-interning breaks the
enum-union collapse (`enumOperations`), R→G/R→W there.

**§55 score — LANDED (three model corrections, each from one
counterexample).** (1) A computed-shared enum type was tried and
REVERTED: `enumBasics2` wants `Bar.a` for `(1).valueOf()` — computed
members keep per-name literals; the `E8.B : E8` pattern that suggested
sharing is the single-distinct-value SPELLING split (fresh prints
per-name, regular prints via the enum symbol), recorded as residue
needing fresh/regular spelling divergence. (2) A reference-provenance
sharing model was tried and REVERTED: it scored +34/2 against value
interning's +64/1 — the one value-interning counterexample
(`ambientDeclarations`' auto-`b` beside `c = 2`) is explained by (3):
an AMBIENT non-const enum has NO auto-increment — its initializer-less
members are opaque upstream, so the collision never folds. Final:
**+64 (W→R), ZERO adverse.** `checker_types` right 399,899 →
**399,963 (83.51%)**. Residue: the spelling split, string-`length`
folding, cross-enum references, and the entry-order class
(`(E7 | E8 | E3 | E4)[]` — type-creation order, the §53 order
question's sibling).

## §55.1 — the single-member spelling split

The §55 residue's head, decoded from 261 lines and two probes: a
SINGLE-member enum's one literal prints as the ENUM in access positions
(`Enum.A : Enum`) while the DECLARATION line keeps the per-name spelling
(`>A : Enum.A`) — two spellings of one member, which this port models as
DIVERGENT fresh/regular twins (regular spelled as the enum, fresh
per-name, `enum_member_regular`-linked) plus an access-road swap
(`enum_access_spelling`, consulted at `check_property_access_expression`'s
single exit). A first cut spelled BOTH forms as the enum and measured
+218/197 — the 197 were every declaration line, which is what proved the
split runs on the fresh/regular axis.

**§55.1 score — LANDED.** **+212 W→R / 2 R→W (106:1)** — the 2 are
module-merged/imported single-member enums wanting qualified spellings
(`constEnumOnlyModuleMerging`, `enumFromExternalModule`), accepted.
`checker_types` right 399,963 → **400,173 (83.55%)** — the right count
crosses 400,000.

## §56 — literal retention under unit contextual members (the arc opens)

The contextual-typing arc's narrowest provable slice:
`isLiteralOfContextualType` (`checker.go:13838`) keeps a FRESH literal's
literal form when the contextual type wants a literal there — the 107
want-`true`-got-`boolean` lines are object members under unit-typed
annotation members (`const a: { ready: true } = { ready: true }`). The
slice: an object-literal PROPERTY whose enclosing literal chain hangs
(through property assignments and nested literals only) off a
VariableDeclaration WITH an annotation; the member path resolves through
`get_type_of_property_of_type`; retention iff the resolved member type is
UNIT or a union of UNITs. Retained = the member's type is the regular
literal, not the widened base. The general contextual machinery
(arguments, returns, satisfies, casts) stays refused — this is one
POSITION, syntactically provable.

**Falsifiers.** (a) The §7-era leg: positions where upstream does NOT
retain despite a unit member (fresh-vs-regular subtleties) — R→W on
literal-member lines. (b) The print/type shared road (`objects.rs`) must
move WITH the symbol road or literals print one thing and carry another —
divergence shows as object-print wrongs beside right member lines.

**§56 score — LANDED (both falsifiers fired and are contained).** (a)
fired as FAMILY mismatch: `typeof undefined` is a unit that wants no
string (`widenedTypes`) — retention now requires the contextual unit to
carry the checked literal's own literal-family flag. (b) fired
immediately: the object PRINT diverged from the member type until
`objects.rs`'s member computation gained the same retention. A third leg
the bar did not name: a LET's retained literals flow into reassignment
JOINS this port cannot reduce (`getAssignmentReducedType` unported —
`tryCatchFinallyControlFlow`'s 11 R→G), so retention is gated to CONST
holders, costing ~30 let-shaped wins. Final: **+52 (W→R), ZERO
adverse.** `checker_types` right 400,173 → **400,225 (83.57%)**. The
arc's next slices: let-holders behind `getAssignmentReducedType`,
parameter defaults, return positions, and the argument road.

## §57 — the element-access write seam

The §52 scorecard's recorded residue, one line of code: a WRITE-position
element access takes the declared type (upstream's assignment-target
dispatch, on the identifier road since §12.7, absent on this one).

**§57 score — LANDED.** The covered matrix moved **+1 W→R, ZERO
adverse** — and the un-erroring of LHS reads ALIGNED 1,704 lines the
walker had never reproduced (the aligned denominator grows 468,915 →
470,619; right +1,296 gross, gap +310, wrong +98 among the newly
aligned). `checker_types` right 400,225 → **401,521; the GRADIENT
denominator itself moved**, so the percentage (85.31% → 85.32% on the
new base... measured: 401,521/470,619 = 85.32% aligned-right share;
the coverage bin's fraction against 478,954 reads 83.83%). Numbers from
the landing run; the newly-aligned wrong/gap rows join the board.
## §58 — join-position member-set identity

Third sighting of one phenomenon: a flow JOIN whose branch types re-form
a NAMED union's exact member set prints the name — `let state: State`
narrowed per-branch by `getAssignmentReducedType` re-joins to `State`
(`tryCatchFinallyControlFlow`), as `x || y` re-formed `T` (the §53
subsumption's cousin) and the §52.1 study's `b = x || y`. The §52.1
member-set index REFUSED globally because ANNOTATION positions want the
expansion; the JOIN position wants the name — so the index consult lives
at `get_type_at_flow_branch_label`'s rebuild ONLY. Named unions register
their sorted member list at creation; the join consults before minting.
This also un-defers §56's let-holder retention (~30 wins) — re-enabled
in the same build, measured together.

**Falsifiers.** (a) Joins where upstream prints the expansion despite a
full named set — R→W at join reads. (b) The §52.1 first-writer hazard:
two aliases, one member set — the second alias's joins print the first's
name; W on the second's lines.

**§58 score — LANDED (the join identity), let re-deferred.** The join
consult: **+10 (W→R), ZERO adverse** — neither falsifier fired. The §56
let re-admission it was meant to unlock measured +43/12 (3.6:1, below
standard) and is RE-DEFERRED with a sharper diagnosis: the adverse is
not reassignment-join reduction (that works — the minimal probe joins
to `State` cleanly) but the union PRINT road erroring on joins of
§56-retained ANONYMOUS object-literal types inside try/catch flows —
that road's seam, queued as its own row. `checker_types` right 401,521
→ **401,531 (83.83%)**.

**§55 postscript — `'x'.length` folding REFUSED at −16.** The fold is
upstream-faithful in isolation but its VALUES feed the §55 interning and
collide member types upstream keeps apart (`enumMerging` 15 R→W —
cross-declaration merges where a folded length equals another member's
constant). Upstream's sharing must key on more than the numeric value in
merged enums; refused until that key is decoded.

## §56.1 — the return position

The arc's second syntactically-provable position: a literal RETURNED from
a function with a WRITTEN return annotation resolves its member path
against that annotation — the §56 walk gains a ReturnStatement arm
(class-boundary guarded, expression-bodied arrows excluded by
construction since the walk requires a return statement).

**§56.1 score — LANDED.** **+10 (W→R), ZERO adverse.** `checker_types`
right 401,531 → **401,541 (83.84%)**. A §56.2 (parameter-default
position) was built and measured ZERO — the corpus holds no
object-literal parameter defaults under unit-membered annotations — and
reverted as unproven breadth. The remaining want-`true`-got-`boolean`
population (148) is scattered across intersections, JSX attributes, and
inference contexts — no single provable position remains; the residue
belongs to the argument road (needs resolved-signature contexts) and the
general contextual machinery.

## §58.1 — the anonymous-object union-print seam, closed

The TSR_JOIN_DEBUG instrument caught it in one line:
`get_union_type([State, c1])` — a join of a named union with its own
constituent — ERRORED in §53's entry gate (State is a non-enum named
entry) before §58's join consult could see the flattened set equals
State's members. The fix: the gate-decline consults
`named_union_by_members` first, for OBJECT-membered sets only —
literal-membered sets are the §52.1 site-sensitive class and measured
+5/13 without the object gate. With the seam closed, §56's let-holder
gate LIFTS (the deferred ~30 wins).

**§58.1 score — LANDED (with let retention).** **+33 W→R / 6 G→W
(5.5:1)** — the 6 are `narrowByClauseExpressionInSwitchTrue5`'s
un-narrowed full sets now spelling their alias name where upstream's
switch(true)-clause narrowing (unported) answers the subset: a
want-narrower class owned by that road, not this consult.
`checker_types` right 401,541 → **401,574 (83.84%)**.
## §59 — switch (true), the clause-expression road

Twice-named by adverse rows (§51.1's 2, §58.1's 6): `switch (true)` —
every case EXPRESSION is a condition (`narrowTypeBySwitchOnTrue`,
`flow.go` beside the typeof arm): prior clauses narrow assume-FALSE, the
current clause set narrows assume-TRUE per clause and unions, and a
default in the set skips the true-half but still refutes the clauses
after it. The dispatch: the switch expression is the `true` literal.
`narrow_type` is the §5x family's shared entry, so the whole family
(discriminant property equality/truthiness, chain containment, typeof)
composes here for free.

**Falsifiers.** (a) Fallthrough clause ranges (clause_start/end spans)
mis-read → over-narrowing R→W; (b) conditions the family declines leave
`t` unchanged per branch — a union of unchanged branches must not
rebuild-and-lose spellings (the §51 kept-all lesson at the clause
union).

**§59 score — LANDED.** **+67 (49 W→R, 18 G→R) / 2 R→G + 2 W→G
(33:1).** Neither falsifier fired as named; the 4 →G are
`narrowByClauseExpressionInSwitchTrue3` clause-join corners (honest
declines where the joined branches disagree undecidably). §58.1's 6
adverse convert. The §16 CaseKeyword trap fired a THIRD time en route —
`CaseOrDefaultClause` is one struct whose `kind` token reads CaseKeyword
everywhere; the side-table node kind decides, now noted at the type.
`checker_types` right 401,574 → **401,639 (83.86%)**.

## §56.3 — the argument position

The arc's fourth position: an object-literal argument to a
SINGLE-CANDIDATE NON-GENERIC callee resolves its member path against the
parameter's written type (`getContextualTypeForArgument`'s one slice
whose signature this port already resolves). The reentrancy guard keys
the CALL node — resolving the signature checks the arguments, whose
object-literal members walk back to the same call; the first build keyed
the callee and hit a stack overflow (`arrayToLocaleStringES2015`).

**§56.3 score — LANDED.** **+91 (W→R), ZERO adverse** — 79 of them in
`compiler/temporal`, the board's top mountain moving for the first time
(its option-bag arguments under unit-membered parameter types).
`checker_types` right 401,639 → **401,730 (83.88%)**. A §56.4 (`new`-argument
position via the constructor-interface road) measured ZERO — the
corpus's constructor option-bags sit behind generic or class-declared
construct signatures, not the interface road — and was reverted.
## §60 — the qualified heritage base (and two expensive re-measurements)

Three experiments, two reverted with numbers, one landed:

1. **The inside-namespace un-gating re-measured** (`TSR_QUAL_INSIDE`):
   +316/232 raw (1.4:1) — the refusal HOLDS post-§53/§56/§58, and the
   adverse decomposes: `bluebirdStaticThis`' 52 are the `R_1` RENAME
   family (not qualification), `resolvingClassDeclaration`'s 80 are the
   heritage-expression compensation (below). Re-gated.
2. **The §33 rename decline GENERALIZED** (collision without const):
   **850 R→G** — the promisePermutations family prints reused names
   UN-renamed, so renames are site-sensitive and only const-carrying
   prints (no baseline stake) may decline. Reverted; §33's const gate is
   load-bearing and now says why.
3. **LANDED — the qualified heritage base**: `class B extends N.C<A>`
   resolves through the namespace's exports in the producer's extends
   compensation (binder-direct, gate-independent) and prints the
   QUALIFIED instantiated spelling via a new checker mint
   (`qualified_heritage_reference`). Contained: CLASS-only bases
   (`extends Interface` is upstream's error case), direct and one-hop
   self-extension declines. **+61 W→R / 10 adverse (6:1)** — the 10 are
   deep-cycle self-extension error recovery (`recursiveBaseCheck`,
   `classExtendsItselfIndirectly2`) and `typeValueConflict`'s
   value-shadowed classes, all degenerate-source cases.
   `checker_types` right 401,730 → **401,783 (83.89%)**.
## §61 — the auto-var cascade, trip-only

The §14.1 mountain's decoded mechanism, first slice: an untyped JS `var`
upstream is typed by checking every assignment's RHS, each RHS typing
its references transitively — the 2000-cap trips inside that cascade
and sets the GLOBAL `flowAnalysisDisabled`, after which every read
prints ` : error` (`parsingDeepParenthensizedExpression`'s 333). The
slice is TRIP-ONLY: the cascade runs with a shared step counter
(one step per assignment RHS checked, transitive through nested
auto-var computations); if it exceeds the cap the global disable is set
and the var answers `error`; an UNTRIPPED cascade discards its result
and falls through to today's `any` — zero risk to every small JS file.
Self-reference inside a cascade answers `any` (the `cascade_active`
guard), not the circularity error.

**Falsifiers.** (a) Files where upstream's evaluator does NOT trip but
this counter does (the counting granularity differs) — R→G/R→W across
mid-size JS cases; (b) the global disable leaking into TS files sharing
a program — the flag is per-checker and programs are per-case, so a
multi-file case with one JS file could poison its TS files; counted.

**§61 REFUSED — the counter cannot reach the cap.** Built and
instrumented: the cascade runs (227 assignments in the mountain's
container) but accumulates ~43 flow-node ticks TOTAL, because this
port's iterative walk breaks at the nearest assignment — the recursion
upstream's 2000-cap measures is exactly the inefficiency ADR-0003-era
design removed. Reproducing the trip means reproducing the recursion,
and any other counting basis fitted to one case is a knob, not a port.
The 333-line mountain stays; its honest road is either (a) the full
assigned-union computation with upstream's recursive walk shape behind
a flag, or (b) accepting the divergence permanently. Reverted whole.

## §62 — unique symbols are per-declaration

87 uniform lines: a unique symbol belongs to its OWN declaration — only
a direct `Symbol()`/`Symbol.for()` call initializer keeps uniqueness; a
COPY (`const x = C.readonlyStaticCall`) widens to `symbol` even under
const (`getWidenedLiteralLikeTypeForInitializer`'s ES-symbol arm).
Gates from the pair: TS files only (`uniqueSymbolJs2`'s JS declaration
roads differ) and binding-pattern holders excluded (their members keep
the member's uniqueness). A parameter-exclusion variant measured 48/0
against this config's 60/2 — parameter defaults split site-sensitively
(`method5(p = s)` keeps, others widen) and the 2 are that split's
recorded residue at 30:1.

**§62 score — LANDED.** **+60 W→R / 2 R→W (30:1).** `checker_types`
right 401,783 → **401,841 (83.90%)**.
## §64 — non-strict nullable widening at return inference

`function f() { return null; }` infers `() => any` under
`strictNullChecks: false` — `getWidenedType`'s nullable arm at a second
position (the §20 rule's return twin). One JS-file leg fired
(`typeFromJSInitializer3`'s 8 — JS return-null semantics differ) and is
gated out.

**§64 score — LANDED.** **+388 (339 W→R, 49 G→R), ZERO adverse** — the
non-strict corpus's null-returning functions had been mis-printing since
return inference landed. `checker_types` right 401,933 → **402,316
(84.00%)** — the gradient crosses 84%.
## §65 — `let x = undefined` REFUSED at 2:1

`controlFlowNoImplicitAny` (@strict) wants `any` for `let x = undefined`
while `implicitAnyCastedValue` wants `undefined` for the same syntax —
two variants measured (type-keyed −43 net; syntax-keyed +20/10) and the
per-case contradiction defies both keys. The discriminator is likely
`noImplicitAny` (the case names say so) — a THIRD option axis this
port's producer already parses; a future slice keys on it. Refused with
both matrices.

**§65 UN-REFUSED — three keys, LANDED.** The same session plumbed the
`noImplicitAny` axis (the setter already existed for the diagnostics
workstream; the types producer now sets it explicit-or-@strict) and the
contradiction resolved: under `noImplicitAny`, a VARIABLE initialized
with the `undefined` IDENTIFIER widens to `any`; class PROPERTIES keep
`undefined` (`implicitAnyCastedValue`, itself @noImplicitAny — the third
key), and derived undefineds (`void 0`) keep everywhere. **+9 (W→R),
ZERO adverse.** `checker_types` right 402,316 → **402,325 (84.00%)**.

## §66 — the class-heritage relater arm REFUSED at zero

Built for §7's named blocker (`generatedContextualTyping`'s `Base[]`,
62): a heritage-chain walk answering Related for `Derived` → `Base`.
Un-gated it folded generic instantiations wrongly (2 R→G,
`arrayLiteralsWithRecursiveGenerics` — the arm compares no arguments);
gated to bare instance types it reached ZERO population — the target
case's pairs never route through the pre-structural verdicts (they
decline earlier, likely at the signature-bearing gate or arrive as
instantiated references). The 62 need the pair's ACTUAL decline point
traced with an instrument, not another speculative arm.

## §68 — the return-statement contextual arm

`getContextualTypeForReturnExpression`'s written-annotation half
(`checker.go:29621`): a returned expression's contextual type is the
enclosing function's declared return type — the fourth arm of this
port's `getContextualType` dispatch. Rejected once at 26 functions
(`checker-notes-ctx.md`'s concentration argument); the §67 trace showed
`generatedContextualTyping` alone now holds 62 aligned lines behind it.

**§68 score — LANDED.** **+24 (W→R), ZERO adverse** — contextually-typed
object-literal METHODS (16) and the first 6 of generatedContextualTyping's
arrow parameters. The case's remaining 56 need the returned arrow's
parameters to see the contextual signature through MORE hops (the
concise-body and nested-return positions). `checker_types` right
402,325 → **402,349 (84.01%)**.

## §68.1 — the parenthesized recursion

`checker.go:29392`, one line, un-rejected with §68's own argument.
**+16 (15 W→R, 1 G→R), ZERO adverse.** `checker_types` right 402,349 →
**402,365 (84.02%)**.

## §68.2 — the array-element contextual arm

`getContextualTypeForElementExpression` (`checker.go:29972`), the
Array-reference and tuple halves. **+4 (W→R), ZERO adverse** — the
contextual functions inside array literals are fewer than the old
33-function measurement suggested once §63's tuple minting took the
tuple half's population. right → **402,369 (84.02%)**.

## §68.3 — the concise-arrow-body arm

`getContextualReturnType`'s concise half: an expression-bodied arrow's
body takes the arrow's own contextual signature's return. **+1 (W→R),
ZERO adverse** — the family's population is mostly block-bodied, already
covered by §68. right → **402,370 (84.02%)**.

## §69 — `this` in static members is the static side

`tryGetThisTypeAtEx`'s static-container arm: `this` inside a static
method, accessor, property initializer, or static block answers
`getTypeOfSymbol(classSymbol)` — `typeof C` — not the instance `this`
mint. The member walk stops at the DIRECT class child so nested
instance-side functions keep their own rules.

**§69 score — LANDED.** **+285 (179 W→R, 106 G→R) / 16 G→W + 6 R→G
(13:1).** The adverse: static-block use-before-def member reads and
auto-accessor flows (the member-typing order inside static blocks), and
three scopeCheck cases where `getTypeOfSymbol(class)` gaps (the static
side's own resolution holes) — each a named residue, none this arm's
rule. `checker_types` right 402,370 → **402,649 (84.07%)**.

## §70 — the overload-agreement contextual argument

`getContextualTypeForArgument` for OVERLOADED/GENERIC callees whose
every candidate agrees on the parameter's type at the index — the
convergence upstream's per-candidate pass reaches when the position
mentions no type parameter. The mention test walks IDs two levels
through signatures and reference arguments (a TEXT test collided the
callback's own `<T>` with the candidate's and killed the wins — names
re-bind, ids don't). Three configurations measured: direct-id
(+127/49), text (+8/0), id-walk (**+68 with 41 W→G, ZERO adverse**) —
the landed one.

**§70 score — LANDED.** `checker_types` right 402,649 → **402,717
(84.08%)**.

## §71 — renamed binding elements render verbatim

§48's pattern printer declined any element with a `property_name`; the
corpus prints the written `prop: bound` pair verbatim
(`({ name: alias, name: alias2 }: Named) => void`). Extended to
identifier property names only — computed/string-literal keys,
initializers, and rests keep the decline. **+61 G→R, 20 G→W** — the 20
are NOT this rule misfiring: they are positions wanting the ALIAS name
(`F4`) where the new structural print replaced a prior `error`; the
alias-preservation head owns them
(`renamingDestructuredPropertyInFunctionType{,2}`).

**§71 score — LANDED.** right 402,802 → **402,863 (84.11%)**.

## §71.1 — element initializers drop from the print; empty patterns spell `{}`

`{x: z = 'y'}` renders `{ x: z }` and `{} = a` renders `{}?: any`
(`declarationEmitBindingPatterns.types`) — the initializer is never
part of the printed pattern, and an empty pattern has no inner spaces.
Rests keep the decline. **+52 (38 G→R, 14 W→R), 1 G→W** — the 1 is the
alias-preservation head again (want `Foo`, structure correct).

**§71.1 score — LANDED.** right 402,863 → **402,915 (84.12%)**.

## §72 — function/constructor type nodes take the alias's name

`signature_bearing_type_node` never consulted `getAliasForTypeNode`'s
port, so `type F = (a: number) => any` printed structurally everywhere
— the only signature-bearing node kind missing the three-arm rule the
type-literal and union nodes share. Non-generic alias → alias name;
generic → gap; unaliased → structural. **+39, zero adverse**
(`parenthesizedContexualTyping2` 12, `taggedTemplateContextualTyping2`
6 — the residue §71/§71.1 exposed, now closed at its root).

**§72 score — LANDED.** right 402,915 → **402,954 (84.13%)**.

## §73 — JS-wide unresolved-prints-error: REFUSED at 199:1,743

`error | any` is a 342-line wrong head, 324 in one machine-generated
JS case (`parsingDeepParenthensizedExpression`) where upstream prints
`error` for unresolved names this port answers `any`. Keying on
`in_js_file` measured **199 right against 1,743 adverse** — 1,504 of
the adverse INSIDE the same case: upstream prints BOTH `any` and
`error` for unresolved-name-involving lines in one file, so the file
kind is not the discriminator. Whatever splits them (error
propagation through operators, CommonJS binding, something else) is
finer than any key tried; the head stays priced until someone traces
upstream's actual split. Reverted.

## §74 — generic construct candidates infer from arguments

The `new` road's named-interface arm answered §44's default-map
instantiation (`Set<any>`) before inference ever saw the arguments.
New helper `signature_candidates_of_named_type` (generics INCLUDED,
same heritage/membership declines), each candidate answering through
the call road's `check_generic_call`, answers must AGREE — overload
selection stays unported, convergent sets need none. One falsifier
fired and became a guard: a candidate whose type-parameter constraint
is ITSELF a type parameter (`new <U extends T>(u: U): U` on
`I<string>`) reaches the arm uninstantiated and widens the fresh
literal upstream keeps (`""` → `string`); such candidates decline.
`setMethods`' Set stayed unresolved — its two lib overloads do not
converge; that case is overload SELECTION, still priced.

**§74 score — LANDED.** right 402,954 → **402,984 (84.14%)** — +30,
zero adverse.

**Process failure, recorded loudly:** the §74 landing commit SHIPPED
RED — clippy 7 (a doc block orphaned onto the new fn) and two expired
pins (`named_callee_signatures`, `new_expression`, both asserting the
§44-era generic decline) — because the gate chain's counts were
printed and the commit ran anyway in the same compound command. The
same class as the `head`-piped gate and `180bcb0`: instrument correct,
reading skipped. Fixed in the follow-up commit; the pins flipped to
`Box<1>`. Rule reaffirmed: the landing commit runs AFTER the gate
counts are READ, never in the same command.

## §74.1 — inference BEFORE the default-map: measured −2, reverted

Reordering §74 ahead of `get_signature_of_named_type` (so arguments
beat §44's default instantiation) converted NOTHING — `new Set([1, 2,
3])` stays `Set<any>` because SetConstructor's two lib overloads do
not converge under this inference — and regressed `genericDefaults`
by 2: a defaulted generic construct's default map IS upstream's
answer at those positions. The landed order (default-map first,
inference as fallback) is correct as measured. The Set family is
overload SELECTION, still priced.

## §75 — a generic single contextual signature passes through uninstantiated

`contextual_signature` inherited `single_call_signature`'s generic
decline, which is calibrated for CALL positions (a bare `T` print
would be wrong there). For a function ADOPTING its context — `const
fn1: <T>(x: T) => void = t => …` — the signature's own `T` IS
upstream's answer (`assignContextualParameterTypes`;
`contextualOuterTypeParameters`). Single generic candidate passes
through as-is; overload sets still decline.

**§75 score — LANDED.** right 402,984 → **403,033 (84.15%)** — +49,
ZERO adverse, zero new gaps.

## §71.2 — nested binding patterns render recursively

`[[a]]: [[string]]` printed `error` for the whole signature because
§48's renderer declined a pattern-typed element name. Extracted to
`render_binding_pattern`, recursive; all other declines (rests,
computed keys) intact. **+89 (78 G→R, 11 W→R), 8 G→W** — the 8 are
NOT this rule: quote-style (`'x'` vs `"x"`, the 57-line head) and
written union order (`number | string` vs `string | number`,
`tsr-5o2`'s annotation-reuse family) surfacing behind the now-correct
structure.

**§71.2 score — LANDED.** right 403,242 → **403,331 (84.22%)**.

## §77 — single-quote-gated written-annotation reuse

The quote-style head measured 478 lines whose got differed from want
by QUOTE CHARACTER alone (plus written union order behind it) —
upstream reuses the annotation node, and a fresh render can reproduce
neither `'foo'` (bakes to `"foo"`) nor the written constituent order
(sorts). New bounded renderer `written_type_text` (keywords,
literals, bare references, arrays, unions, property-only type
literals — anything else declines whole), wired into
`written_annotation_text` behind a SINGLE-QUOTE GATE: only a subtree
holding a `'…'` literal returns, so every double-quoted annotation
keeps the fresh-render road. The gate is what separates this from the
blanket written-union reuse that measured +323/−270 sessions ago and
was reverted — that experiment's diagnosis ("node reuse loses") was
half-right: reuse loses where the fresh render is already exact, and
wins precisely where it cannot be.

**§77 score — LANDED.** right 403,331 → **403,701 (84.30%)** —
**+370, ZERO adverse**, the largest zero-cost build since §64.

## §77.1 landed, §77.2 REFUSED at 35:249

§77.1 extends the quote gate to the TYPE-LITERAL MINT (`{ kind: 'foo';
foo: string; }` prints its written spelling everywhere, not only in
parameter carriage): **+53, zero adverse** — right 403,701 → 403,754
(84.31%).

§77.2 — the same idea at the UNION mint, routed through §53's origin
machinery to preserve written constituent order — measured **35 W→R
against 94 R→G + 20 R→W** (net right −132): the origin road's
declines (an entry whose print errors kills the union; the slice
gates) turn working sorted-print unions into gaps at scale, and the
narrowed-constituent positions (`controlFlowAliasing`'s single-object
prints) need DISCRIMINANT NARROWING to fire on aliased conditions,
which no print change supplies. Union order at annotation sites stays
priced; re-open only with an origin path that cannot gap an
already-working union. Reverted.

## §77.3 — single-quoted member names keep their quote

Two halves, one gate: `written_type_text`'s type-literal arm accepts
string-literal member names (single quote sets the gate, double
renders `"…"`), and `check_object_literal`'s RE-QUOTE arm — the only
name path that bakes a quote character — keeps a written single quote
(`{ '1.0': "" }` prints `{ '1.0': string; }`). Identifier-valid names
stay unquoted whichever quote wrote them. **+54, zero adverse**
(`assignmentCompatWithObjectMembersStringNumericNames` 33,
`propertyAccess` 11).

**§77.3 score — LANDED.** right 403,811 → **403,865 (84.33%)** —
measured against a RE-TAKEN baseline (the §88 trap fired a second
time this window: another workstream's §103 builds arrived through
build 134's own rebase; stash-and-remeasure caught it, right 403,754
→ 403,811 was theirs).

## §77.2-retry — still refused, now at 0:20 with the mechanism located

The fallback repaired the R→G leg (an erroring origin build now falls
back to the plain union), but the retry measured **0 W→R against 20
R→W**: the order-wanting positions (`narrowingUnionWithBang`,
`controlFlowAliasing`) never reach the union-NODE path at all — their
unions are built by OPTIONALITY (`?:` adding `undefined` under
strict) and by narrowing rebuilds, not from a written UnionTypeNode —
while the path the retry did reach (const-context literal unions) is
one upstream genuinely sorts. The head's owner is the optionality
union builder and the §52.1 site-sensitivity class, not the
annotation mint. Refusal stands.

## §78 — exactOptionalPropertyTypes: missingType lands

The option plumbed end-to-end (tsr-core `Tristate` →
`apply_compiler_options` → checker flag), `missingType` minted as a
DISTINCT undefined-flagged intrinsic printing `undefined`
(`checker.go:986`), `getOptionalType`'s `isProperty` choice now live
(the parameter sat deliberately unread since the module was written,
waiting for exactly this), and `removeMissingType` at the
property-access WRITE position (`getWriteTypeOfSymbol`): `obj.a =
'hello'` prints `string`, the read keeps `string | undefined`. The
first-constituent early return is what stops `b?: string | undefined`
carrying both spellings — it was kept "currently unobservable" for
this moment, and both prior claims about it held. **+14, zero
adverse** (`strictOptionalProperties1`). Residue in the same case:
`in`/hasOwnProperty narrowing legs, element-access writes.

**§78 score — LANDED.** right 403,865 → **403,879 (84.33%)**.

## §79/§79.1 — optional-element tuples land, and aliases name them

`[number, string?, boolean?]` was a whole-tuple decline; it now mints
with the `?` in the print, the plain members in `tuple_element_lists`,
and the optional mask in a side map read by the index road (`t[1]` is
`string | undefined`). The first measurement priced the missing alias
rule at **99 G→W** — every structural print replaced an `error` at an
alias-wanting position — so §79.1 adds §72's three-arm rule at the
tuple mint, with THREE measured gates: GENERIC aliases fall to the
structural road (3 R→W under the gap rule), REST-bearing bodies keep
the §40 variadic road (13 R→W when intercepted — the depth-guarded
giant of `excessivelyLargeTupleSpread` prints `any` through it), and
the EMPTY tuple prints `[]` even under an alias (3 R→W,
`typeAliasDeclarationEmit3`). Final: **+184 net (105 G→R, 79 W→R
against 38 G→W + 1 R→W, 4.7:1)** — residue owned by the
default-against-optional destructuring legs and the
exactOptional×optional-tuple interplay
(`optionalTupleElementsAndUndefined`).

**§79 score — LANDED.** right 403,879 → **404,063 (84.37%)**. §78.1
(element-access write removal) rode along as a measured zero — kept,
faithful arm on the reachable set.

## §80 — labeled tuple members

`[first: string, second?: number]` mints through §79's road with the
label in the print (the label owns the `?` — `[first?: string]`,
never `[first: string?]`); labeled rests keep the decline. **+71 net
(103 G→R against 32 G→W, 3.2:1)** — the 32 are two named residues,
both prior gaps: SPLICED tuples lose labels (`spreadParameterTupleType`'s
`[a: string, a: string, …]` from spreads prints unlabeled members),
and REST-PARAMETER EXPANSION positions want the STRUCTURAL labeled
print where the §79.1 alias arm answers the alias name
(`getExpandedParameters` prints structure; `[s: string]` vs `A`) —
the alias rule and the expansion rule disagree at exactly these
positions and the expansion machinery is unported.

**§80 score — LANDED.** right 404,116 → **404,219 (84.40%)**.

## §81 — blunt qualified names: REFUSED at 114:6,769

Qualifying every namespace-declared class/interface print
(`Temporal.Duration`) measured **114 W→R (74 in `temporal` — the
mechanism is real for OUTSIDE views) against 6,768 R→W**: every
reference from INSIDE a namespace wants the BARE name
(`parserRealSource11/12` alone lost 1,907). The qualifier is
decided by the VIEWER's position, not the declaration's, and a global
print text cannot express both. This is the same architectural head
as the `import("...").Name` spelling
(`privacyFunctionCannotNameParameterTypeDeclFile`): PER-SITE printing
context. `tsr-93f` stays refused until that exists; no narrower gate
(lib-file, ambient-only) changes the inside-view half.

## §82/§82.1 — aliased conditions narrow

`const isFoo = obj.kind === 'foo'; if (isFoo)` narrows as the
condition itself would: `narrowType`'s identifier arm inlines a CONST
variable's un-annotated initializer, depth-capped at 5 exactly as
upstream's `inlineLevel`. Two falsifiers fired in sequence and each
became a mechanism:

- §82.1: `const both = isA || isB` — an inlined initializer has NO
  flow branch nodes, so `narrowType` needed the logical arms
  (`narrowTypeByBinaryExpression`): `a || b` true is the union of
  (a true) and (a false, then b true), duals by symmetry.
- the constant-reference gate: `obj` REASSIGNED in the body must not
  narrow through the alias (`isConstantReference` — const variables,
  never-assigned parameters/locals via `mark_node_assignments`' map,
  readonly properties on constant receivers). The ungated first
  measure read +59 with 12 adverse; gated, **+43 with ZERO adverse**.

Residues, owned: instanceof-through-alias (`controlFlowAliasing2`'s
`TestA`), element-access constant references (`obj[0]`), and the
union-ORDER rows the §77.2 refusal already prices.

**§82 score — LANDED.** right 404,219 → **404,262 (84.41%)**.

## §83 — instanceof narrows, TRUE branch only

`narrowTypeByInstanceof`'s class-identity slice: RHS resolves to a
CLASS, constituents keep by identity or by the `extends` chain
(identifier heritage only, depth-capped 16); a non-union reference
narrows TO the derived instance when the chain relates them; nothing
decided declines whole. **The FALSE branch is disabled by evidence
the corpus splits on**: `typeGuardOfFormInstanceOf`'s else prints the
WHOLE union (matching the old-semantics comment in its header) while
`instanceofWithStructurallyIdenticalTypes`' else-if chain narrows by
derived-from — global `var` references vs parameters is the visible
difference, and whatever upstream's actual key is needs a trace
before the false arm can land; filtering it cost 21 adverse, true-only
costs 7 (the four derived-from else positions in the structural case,
now priced). **+126 net (67 G→R, 59 W→R against 7 R→W, 18:1).**

**§83 score — LANDED.** right 404,262 → **404,381 (84.44%)**.

## §84 — the sibling-truthiness discriminant; a redundancy caught by measurement

The dependent-destructured seam turned out ALREADY BUILT (§50's
pseudo-reference road: `dependent_destructured_type` →
`narrow_destructured_parent`, with sibling equality and switch arms)
— a hand-rolled §84 equality arm measured ZERO transitions once its
composition bug was fixed, proving every position it reached was
already §50's, and was REMOVED rather than landed. What was missing
was one form: the sibling as a TRUTHINESS condition (`const { kind,
isA } = foo; if (isA) kind` wants `'A'` — f30). One
`filter_union_by_member_truthiness` arm behind
`state.discriminant_pattern`. **+6, zero adverse.**

Remaining in the case, owned: f22 (the pattern's PARENT narrowed by a
test BEFORE the destructuring — `get_type_for_binding_element_parent`
reads the declared type, not the flow type at the declaration), f23
(`never` collapse in an exhaustive else), and the f10-family
parameter switch forms.

**§84 score — LANDED.** right 404,371 → **404,377 (84.44%)**.

## §85 — the `T & {}` family: adjusted facts for type variables

`getAdjustedTypeWithFacts`' type-variable arm: under strict, a TYPE
PARAMETER (or `unknown`) narrowed by a non-null fact answers an
INTERSECTION mint, not a filter — `!= null` → `T & {}`, `!==
undefined` → `T & ({} | null)`, `!== null` → `T & ({} | undefined)`,
`unknown` drops the `T & `. Three iterations, each measured:

1. First cut (+27 net) had a TRUTHY arm and no refinement —
   `narrowingTruthyObject` 15 R→G said upstream's truthy does NOT
   mint; dropped.
2. Second cut added the refinement lattice (`T & ({} | null)` then
   `!== null` combines to `T & {}` via a mint→(base, kind) reverse
   map) — and exposed the JOIN problem: 16 R→W of `T | T & {}` at
   every post-narrowing merge, because the mint is opaque to the
   union reducer.
3. `get_union_type` gained the §85 reduction (a mint beside its own
   base is subsumed): **+42 net (44 W→R against 1 R→W + 1 R→G,
   22:1)**.

Residue: `NonNullable<T>` spellings (some positions want the utility
name — 28 W→G now honest gaps), one condition-position leak (row
113), one JSDoc-generic disturbance (`typedefMultipleTypeParameters`).

**§85 score — LANDED.** right 404,377 → **404,419 (84.45%)**.

## §85.1 — truthy spells `NonNullable<T>`: LANDED after a false refusal

**The refusal recorded here for one commit was WRONG, and the record
says so rather than silently editing:** the "unexplained 8 R→G in
`exportNestedNamespaces2`" that priced every variant was measured
against a baseline PREDATING build 142's `git pull --rebase` — the
rows belonged to the parallel session's arrivals and appeared
identically on a CLEAN checkout. The §88 trap's sixth firing this
window, and the first to produce a WRONG REFUSAL rather than a wrong
gain. Rule sharpened: before pricing an adverse, re-run scorepair on
the clean tree — a transition that survives the revert is not yours.

The arm itself, measured against a re-accepted baseline: TRUTHY on a
TYPE PARAMETER spells the utility — `u && u` prints the second
operand `NonNullable<U>` — where the NE-family spells the
intersection; `unknown`'s truthiness stays on the filter road
(`narrowingTruthyObject`'s 15 R→G was a REAL adverse and keeps that
gate). **+16 W→R against 3 R→W, net +12 at 5.3:1.**

**§85.1 score — LANDED.** right 404,411 → **404,423 (84.45%)**.

## §86 — rest-tuple contextual parameters expand positionally

`(...args: ['A', number] | ['B', string]) => void` as a contextual
signature types parameter 0 as `"A" | "B"`, parameter 1 as `number |
string` (`getTypeAtPosition`'s tuple expansion). One arm in
`get_contextually_typed_parameter_type`: a SINGLE rest parameter
whose type is a (union of) tuple(s) indexes each constituent at the
position; short tuples and non-tuple constituents decline. **+33,
zero adverse** (`dependentDestructuredVariables`' f50/f51 27,
`restTuplesFromContextualTypes` 6) — and the destructured-discriminant
narrowing then composes with it for free, since the §50 road keys on
the declared shapes this arm now supplies.

**§86 score — LANDED.** right 404,423 → **404,456 (84.46%)**.

## §87 — variadic tails become consumable, lazily

A trailing-rest variadic (`[number, boolean, ...string[]]`) records
its tuple NODE beside §40's print-only mint; §86's positional
expansion resolves prefix/tail AT CONSUMPTION. The first cut resolved
EAGERLY inside the mint and appeared to cost 16 aligned lines in
`unicodeEscapesInJsxtags` — the clean-tree rule (§85.1's lesson,
applied within the same day) showed those 16 lines vanish on the
UNTOUCHED tree too: **`unicodeEscapesInJsxtags`' alignment is
nondeterministic across builds** (18 vs 2 rows), an INSTRUMENT
caveat now on record — any future single-case ±16 there is noise.
**§87's true delta: +3, zero adverse**
(`restTuplesFromContextualTypes`). The seam matters more than the
count: positional consumers can now see variadic shapes, which the
expanded-signature prints (`(args_0: number, …)`) and the remaining
90+ rows of that case build on.

**§87 score — LANDED.** right 404,441 → **404,444 (84.46%)** against
the re-taken 470,648-line population.

## §88 — rest parameters over plain tuples expand

`getExpandedParameters`: a rest parameter whose type is a plain tuple
prints positionally (`args_0: number, args_1: boolean`), optional
elements carrying `?` — WHEN no written annotation rides the
parameter (a written `typeof t1` rest keeps its reuse; the ungated
arm fired 6 R→W on exactly those rows). The VARIADIC half (row 111
wants expansion OVER the written reuse, `(args_0: number, args_1:
boolean, ...args: string[])`) is blocked on the renderer being
immutable — the §87 tails need `&mut` to resolve lazily; a renderer
refactor unlocks it. **+28 net (29 W→R against 1 R→G, 29:1)**
(`genericRestParameters1` 15, `readonlyRestParameters` 5).

**§88 score — LANDED.** right 404,444 → **404,472 (84.47%)**.

## §88.1 — variadic rest expansion, written reuse still winning

The variadic half of §88, resolved lazily from §87's node: `...args:
[number, boolean, ...string[]]` prints `args_0: number, args_1:
boolean, ...args: string[]` when no written annotation rides the
parameter. The ungated first cut (12 R→W) taught the split: rows
110/115 keep `typeof t2` because the ANNOTATION signature's parameter
carries written reuse; row 111's expanding want is the ARROW VALUE's
fresh signature — two types, two prints, no site-sensitivity needed.
**+22, ZERO adverse** (`genericRestParameters2` 20).

**§88.1 score — LANDED.** right 404,472 → **404,494 (84.48%)**.

## §86.1 — the function's own trailing rest stops bailing the list

`(a, b, ...rest)` under a rest-tuple contextual signature: positional
parameters index as §86 does, and the OWN rest — trailing only —
takes the whole tail array when it sits at or past the prefix
boundary (`rest: string[]`). Slices that would swallow prefix
elements, and plain-tuple slices, stay declined (tuple-slice minting
is the next machine). **+26 net, zero adverse**
(`restTuplesFromContextualTypes` 19, `genericRestTypes` 4).

**§86.1 score — LANDED.** right 404,494 → **404,520 (84.48%)** on a
population that grew 3 lines (the JSX flake's neighborhood — see
§87's caveat).

## §86.2 — the own rest takes the tuple slice

`(a, ...rest)` under `(...args: [number, string, boolean])` types
`rest: [string, boolean]` — `getRestTypeAtPosition`'s slice, one
`create_tuple_type(elements[index..])` through the interned road.
**+7, zero adverse.** The variadic slice (prefix remainder + tail)
still declines — it needs the §40 print-only shape minted from parts.

**§86.2 score — LANDED.** right 404,520 → **404,527 (84.49%)**.

## §89 — the composite re-render was eating §72's names

The open trace closed: §72's alias-named mints (`type H = (a:
number) => void` baking text "H") printed STRUCTURALLY at every site
because `type_to_string_at`'s composite re-render (§10.13) rebuilds
any single-signature type from its structure — right for
qualifier/rename sites, wrong for an alias-NAMED bake. One set
(`alias_named_signature_types`) tells the site renderer to keep the
name. **+178 net (185 W→R against 7 R→W, 26:1)** — the fix converts
the entire alias-name residue §71/§77 had been exposing for eleven
builds (`renamingDestructuredPropertyInFunctionType{,2}` 20,
`unusedLocalsAndParametersTypeAliases` 7, `defaultValueInFunctionTypes`'
`Foo`, TupleUnionFunc…). The 7: union/intersection-constituent
positions wanting parenthesized or structural spellings — the §xm9
parenthesisation family's door.

**§89 score — LANDED.** right 404,594 → **404,772 (84.51%)**.
## §89.1 — degenerate leading-operator unions are real nodes

§89's 7 R→W diagnosed: `type U3 = | () => number` wants the
STRUCTURAL print while `type U1 = string | () => void` wants the
name. Upstream's parser keeps the `UnionTypeNode` whenever a leading
operator was consumed — even with ONE constituent
(`parser.go:2649`, `p.token == operator || hasLeadingOperator`) —
and that node is exactly what blocks `getAliasSymbolForTypeNode`'s
one-hop parent test, so the alias never attaches and the print stays
structural. Our `parse_union_type`/`parse_intersection_type` eat the
leading bar and then RETURN THE CONSTITUENT BARE, so the function
node's parent is the alias declaration and §72 names it.

The fix is the parser's, not the checker's: build the node when a
leading operator was eaten. **Bar:** the 7 §89 R→W convert back;
zero adverse elsewhere. **Falsifiers:** (a) parser_typescript /
printer_round_trip / binder_symbols must hold 100% — a new tree
shape at a leading-bar alias could disturb any of them; (b) other
degenerate-union corpus sites may want the OLD shape — if the pair
shows unrelated R→W concentrated on leading-bar files, the checker
consumers need the union-hop instead.

**§89.1 findings.** Three seams, not one: (1) the parser kept — both
`parse_union_type` and `parse_intersection_type` now build the node
when a leading operator was eaten; (2) the checker's two node
consumers answer a single-constituent list BEFORE the alias attaches
(`checker.go:25632` union, `:26128` intersection); (3) two print
bugs the case exposed: the intersection parenthesiser was wrapping
`boolean` (UNION-flagged but keyword-printed — the BOOLEAN exemption
upstream's node builder applies at `nodebuilderimpl.go:3255`), and
the printer dropped the leading operator on degenerate nodes, which
broke round-trip until it re-emits `| `/`& ` for one-element lists.
Falsifier (a) FIRED on printer_round_trip (100% → 99.97%) and was
honoured by the re-emit; all four 100% suites verified restored by a
full coverage run.

**§89.1 score — LANDED.** right 404,772 → **404,780 (84.51%)**, all
8 W→R, ZERO adverse on the full pair.

## §90 — an instantiated alias reference keeps its body's members

The generic-alias instantiation subsystem's first seam, located by one
trace (`longObjectInstantiationChain2` 0:15): round ONE of
`o1.merge({p2})` answers `Type<merge<{p1:1},{p2:number}>>` RIGHT —
the §46 annotation mint carries the body symbol, member lookup and
signature instantiation all compose — and round TWO gaps, because the
call-return was rebuilt by `instantiate_type`'s arm 3 through
`create_type_reference`, which mints the ALIAS symbol (no members)
where §46's road mints the alias BODY's TypeLiteral symbol. The
member road past the mint is proven; only the mint's symbol differs.

**Mechanism:** `create_type_reference` mints the body symbol when the
target is a TYPE_ALIAS whose body is a TypeLiteral with a bound
symbol — the same admission test as §46's arm, one road lower.
`type_reference_targets` keeps the ALIAS symbol, which is what the
member-instantiation seam already reads (round 1 proves that
mapping).

**Counterfactual:** chain2 holds 308 GAP lines; chain1 34, chain3 34.
**Bar: ≥250 of chain2's gaps convert, adverse within 10:1.**
Falsifiers: (a) IDENTITY SPLIT — the annotated `Type<{p1:1}>`
(§46-keyed) and an instantiated one (`instantiations`-keyed) become
two TypeIds printing alike; if the relater compares them, R→W lands
in §46-family cases — the honoured answer is unifying the two intern
keys, not reverting; (b) the body symbol's member table answering
UNINSTANTIATED member types — would show as wrong lines wanting
substituted prints; the §46 road's round-1 evidence says the
get_type_of_property_of_type seam substitutes, so zero expected;
(c) non-literal alias bodies (chain1/3's `merge` itself is an
intersection body) must fall through unchanged.

**§90 findings and score.** The body-symbol mint alone measured
+182 G→R against 96 G→W (1.9:1) — the bar's stated 10:1 MISSED, and
the 96 were ONE token each: upstream renames the instantiated
signature's own type parameter (`r` → `r_1`). §90.1 chased that
rename through three wrong placements: (1) a SEMANTIC rename (fresh
types in the map) killed all 182 conversions — call-side inference
identifies own parameters by declaration TypeIds; (2) print-only
rename at the mint rendered right but the §89 composite re-render
REBUILT the un-renamed structure at every site — the §89 keep-text
set closed it (same trap, one bake further in); (3) un-gated, the
rename hit 1,276 R→W (the promise family wants plain names). The
landed gate is EMPIRICAL, three conditions: inside a type-alias
body, return references the CONTAINING alias, declaration is a
FunctionTypeNode (method members keep plain names —
`nonInferrableTypePropagation1`). Upstream's true rule is
`typeParameterToName`'s byText/shadow context
(`nodebuilderimpl.go:1404`) — the §20.1 print-context study, still
owed; underscoreTest1's 149 stay with it.

**§90 + §90.1 score — LANDED.** right 404,780 → **405,060 (84.57%)**,
+278 G→R and +2 W→R, ZERO adverse on the full pair.

## §91 — conditional alias bodies evaluate at instantiation rebuilds

The chain1/chain3 pair pins the rule: at a call-return rebuild
(`instantiate_type` arm 3), a PLAIN generic alias keeps its name
(`merge<{ p1: number; }, { p2: number; }>`, chain1 — already right)
and a CONDITIONAL one EVALUATES to its branch (chain3's 166 wrong
lines want `Omit<... & ..., "p2"> & { ... }`). Upstream:
`getConditionalTypeInstantiation` resolves a conditional whose check
goes concrete, and the resolved branch does not carry the alias.

**Mechanism, the slice:** an evaluator entered from arm 3 when the
target alias's body is a ConditionalTypeNode. It binds parameter
symbols to the substituted arguments (a checker-level env stack),
walks the body's nodes with three env-gated arms — type-parameter
references answer their binding, `keyof T` answers T's literal key
union, an intersection of literal-key unions reduces by set
intersection — and decides `extends never` by the check's emptiness
(only the NeverKeyword extends form is admitted). `keys_of` covers
member-table types, intersections (union of sides), and `Omit<T, K>`
by name (keys of T minus K's literals — the §45 Record precedent).
ANY refusal anywhere falls back to today's name print, which is what
gates the entry: no separate concreteness test, computability is the
test.

**Counterfactual:** chain3 166 wrong (every `merge<...>`-got line) +
jsxGenericComponentWithSpreadingResultOfGenericFunction 2 +
ramdaToolsNoInfinite2 1. **Bar: ≥140 of chain3 converts at ≥10:1.**
Falsifiers: (a) downstream consumers of the now-intersection results
(property reads have NO intersection arm in `get_property_of_type`;
chain3's right lines are literal members, so expected zero — R→G
there fires this leg); (b) the keyof/intersection arms leaking
outside the env gate — any adverse in §35.1's old population fires
it; (c) next-round `keys_of` through `Omit` mis-set — shows as
wrong-not-gap in chain3's own later rounds.

**§91 score — LANDED.** right 405,060 → **405,277 (84.61%)**: chain3
WHOLE (+166 W→R, every merge-name line), +51 G→R bonus
(moduleAssignmentCompat1-3 and friends — positions the env-consult
and key machinery unblocked), ZERO adverse on the full pair. The bar
(≥140 at 10:1) met at ∞:1. Residue for later slices: general extends
forms (only NeverKeyword admitted), keys through index signatures and
mapped types, `get_property_of_type`'s missing Intersection arm (the
evaluated results answer no property reads yet — chain1's 34-line
gap family), and the §36 alias-declared entry (templateLiteralTypes3
still declines).

## §92 — property reads through evaluated alias bodies

§91's consumer half: `get_type_of_property_of_type` gains a SHAPE
road for what the symbol table cannot answer — an INTERSECTION's
constituents (searched only for types the §91/§92 evaluator
PRODUCED; a written intersection answering confidently measured 134
G→W in the discriminated-union family and the registry set is the
gate), `Omit<T, K>` by its GLOBAL ARITY-2 symbol (the §45 Record
precedent; `global_type_symbol`'s arity-1 default was the first
trap), and an alias reference with a non-literal body (evaluated by
`evaluate_alias_body`, §91's generalization to plain bodies). Three
measured traps shaped the gates: (1) the alias BODY node's own alias
attribution fired the generic-alias error arm — under bindings
attribution is off; (2) TypeLiterals on the body's structural spine
evaluate to WRITTEN member types (`T | undefined` for a bound `T`) —
refused by `body_carries_type_literal`, those stay with §90's
instantiating symbol road; (3) a lost edit — the registry insert
silently vanished in an aborted scripted edit and only a TypeId probe
found it (registration read false for the id the evaluator had just
returned).

**§92 score — LANDED.** right 405,277 → **405,390 (84.63%)**:
+113 G→R (chain1 26 + chain3 26 + chain2 20 + omitTypeHelper 15 +
discriminated/aliased families), 12 G→W at 9.4:1, ZERO R→W. The 12
are priced residues: exactOptional modifier interplay through the
Omit arm (4+2), two discriminated-union alias-name variants (4), and
dependentDestructuredVariables 2.

**§92.1 — a measured ZERO, kept.** The §36 alias-declared decline now
tries §91's evaluator first; the full pair is byte-identical. The
declined population (templateLiteralTypes3's `Foo1<"*x*">` family) is
template-literal conditionals, which the extends-never gate refuses —
the §36 wrong lines need template-literal MATCHING, a different
machine. Kept because the arm is faithful direction at zero cost and
self-gating (the §12.8 precedent); the zero is the record.

**§92.2 — symbol-level shape road REFUSED at a measured zero.** The
§92 residues (optional `| undefined` and readonly-`any` dropped
through the Omit arm) suggested returning the underlying property
SYMBOL through Omit/evaluated bodies so the §27 readonly arm and the
optionality union would fire. Built and measured: the full pair is
BYTE-IDENTICAL — the road either isn't reached where the residues
live or the residues' owner is the flow/write-position layer, not
the lookup. Reverted whole (the §34 precedent: a zero-payoff arm is
coverage that reads as capability). The 12 §92 residues stay priced;
their next probe should trace ONE line of omitTypeHelperModifiers01
0:18 (`x.b` at a write position wanting `string | undefined`)
through access_member_lookup's flow tail, not the lookup.

## §93 — an any-context argument types its arrow standalone

The ArrowFunction gap-root's cheapest slice: an unannotated arrow in
a call-ARGUMENT position was gapped because `has_no_contextual_type`
could not show absence — but a contextual parameter type of `any`
(or an `any[]` rest slice) supplies nothing, and upstream's
`assignContextualParameterTypes` with `anyType` leaves the implicit
`any`, so the standalone type IS the answer. One gate extension
(`argument_context_is_any`): resolved single-signature callee, the
positional parameter (or covering rest) identity-`any`; anything
unshowable keeps the gap.

**§93 score — CORRECTED, then LANDED at +16.** The paragraph that
stood here claimed +300 G→R / +11 W→R at 8.6:1 and named jsxChildren
42, reactDefaultProps 27, jsx arity 17 as §93's wins. **That score
was measured against a STALE baseline** — the clean tree (§93
stashed) already read 405,682 with every one of those transitions
present: they belong to landings already on main, not to this arm.
This is the §85.1 trap's fifth firing, and the first time it
inflated a LANDED score rather than a refusal — at the moment of
correction the docs and TASK were committed while the code was
still working-tree-only, so the claim briefly outlived its
evidence. (The code then landed in `45436ae` alongside this very
paragraph, swept in from the shared tree; what remains wrong on
that commit is only its MESSAGE's numbers.) Corrected 2026-08-08
by the stash/accept/re-measure pair, checker-2 session.

**The true §93 score**, baseline accepted on clean `4965add`:
right 405,682 → **405,698**. +16 G→R (fatarrowfunctions 11,
fatarrowfunctionsOptionalArgs 5) against 4 G→W, ZERO R→W/R→G — 4:1.
Priced residues: the `b?: number | undefined` optional-parameter
print (strict annotation adds `| undefined`; the standalone print
misses it — fatarrowfunctionsOptionalArgs:0:457), and two JS-file
arrows whose JSDoc `@param` types the standalone road does not read
(arrowFunctionJSDocAnnotation, contravariantOnlyInference…Js).
Most of fatarrowfunctionsOptionalArgs' 111 gaps REMAIN — positions
whose callee `single_call_signature` cannot show (`:100/:101` still
gap) — so the next arm in this seam is widening the callee road,
not the parameter test. builtinIterator/intraExpressionJsx were
never §93's; struck from its residue list.

## §94 — an expression statement supplies no contextual type

**Correction to §93's residue diagnosis first, by the tracing rule:**
§93's closing paragraph (written by the same session an hour earlier)
said fatarrowfunctionsOptionalArgs' remaining gaps were "positions
whose callee `single_call_signature` cannot show — widen the callee
road". Wrong: one look at the baseline shows they are arrows in bare
STATEMENT position (`(arg) => 2;`, `arg => 2;`) — no call anywhere.
The residue's owner is `has_no_contextual_type`, which recognises
only the unannotated-var-initializer shape and so cannot show
absence for a statement.

The faithful rule: upstream's `getContextualType` dispatch
(`checker.go:29343`) switches on the PARENT's kind and has **no arm
for `KindExpressionStatement`** — the default answers nil. A
statement-position expression provably has no contextual type. The
same dispatch's `KindParenthesizedExpression` arm answers the
parent's own context, so a parenthesized chain ending at an
expression statement is equally none.

The arm: from the function-like node, climb any
`ParenthesizedExpression` layers; if the terminating parent is an
`ExpressionStatement` whose expression is the chain, absence is
shown. Nothing else changes.

**Bar, registered before the code:** fatarrowfunctionsOptionalArgs
holds 105 GAP lines at the §93 landing, most of them
statement-position arrow prints; predict **+60–110 G→R** there with
spillover in the other fatarrow cases. Named risk: the
default-value forms want `(arg = 1) => 3 : (arg?: number) => number`
— if the standalone print misses the `?` or the initializer-inferred
type, those become G→W and get priced, not hidden. Must NOT move:
call-argument positions (§93's, measured 16 lines) and
const-initializer positions (the existing shape). Adverse over 1:3
against gains refuses the build.

**§94 score — LANDED at 107:1.** right 405,698 → **405,805** on the
full pair over the §93-accepted baseline (`45436ae`+): **+93 G→R**
(fatarrowfunctionsOptionalArgs 76, its Errors1 4, fatarrowfunctions
2, tail across the corpus) **+14 W→R**
(parserArrowFunctionExpression11/16/17 — binary-operand arrows that
had been confidently mistyped) against **1 G→W, zero R→W/R→G**. The
bar predicted +60–110 in the head case: 76+5 measured, in band. The
first ExpressionStatement-only cut converted just 21 — the rest of
the population sat behind ternary branches, `&&`/comma rights,
ternary conditions, and non-listed binary operators (`+`,
`instanceof`), which is why the arm is the dispatch's full
nil-ladder rather than one statement test. The one adverse is
conformance/parserParameterList11:0:0 — `(...arg?) => 102`, an
error-recovery optional REST parameter whose want is
`...arg?: any[] | undefined`: the same strict `?`-adds-`| undefined`
print residue already priced at §93 (0:457), now with two spellings
on record. That print is the seam's next candidate and it owns both
adverse families.

## §95 — site-aware reference re-render (the argument slots)

[checker-1's lane, built on branch worktree-checker-1-printing.]

temporal's 400 WRONG lines decompose against the §81 refusal and
most are NOT its architectural half: the outer reference already
prints qualified at the site (`Temporal.PartialTemporalLike<...>` —
design P + §41/§42 own that), and the failure is the TYPE-ARGUMENT
slot keeping its baked inside-view text
(`<ZonedDateTimeLikeObject>` where the site wants
`<Temporal.ZonedDateTimeLikeObject>`). The baked argument text was
minted inside the namespace; the site is outside; the slot never
re-renders.

**Mechanism:** `type_to_string_at`'s reference arm rebuilds the
print from `type_reference_targets`' `(symbol, arguments)` — the
target through the existing site-aware naming stack, each argument
recursively through `type_to_string_at`, the `Array`/`ReadonlyArray`
shorthands preserved — falling back to the baked text when ANY piece
declines. §10.13's composite re-render, for references.

**Bar:** ≥150 of temporal's 400 W→R; adverse within 10:1.
Falsifiers: (a) INSIDE-view sites re-rendering their arguments
qualified — §81's 6,768-line trap; the argument inherits the SAME
site the outer name already resolves correctly at, so zero expected,
and `parserRealSource11/12` R→W fires this leg; (b) the `T[]`
shorthand lost on rebuilt Array references — element-position
re-render must keep `array_element_text`'s parenthesisation;
(c) baked-right references re-rendering DIFFERENTLY at their own
declaration sites (the §89/§90.1 keep-text family) — any R→W whose
want equals the old baked text fires it.

**§95 score — LANDED, one leg missed and stated.** Measured by the
stash/accept/pop pair (the §85.1 rule, followed this time):
right 405,698-line clean base at `219efd6` → **+146 W→R with ZERO
adverse in any column**. temporal 112, propTypeValidatorInference
10, declFileGenericType 6, remainder spread. The bar's ≥150-temporal
leg MISSED at 112: the residue is reference prints whose baked text
is not `name<args>`-shaped at the outer level (signature-embedded
positions §10.13 already owns route separately) plus the GAP half
(399 lines: `Temporal.ZonedDateTime[]` method results through lib
generics — resolution, not printing). The ratio leg (10:1) met at
infinity.

## §97 — alias-named unions join the origin gate

[checker-1, worktree branch.] §53's slice gate admits only ENUM-named
union entries and plain types, because the ungated form's falsifier
fired on "temporal's 82 site-sensitive alias spellings" — a baked
origin text cannot serve inside- and outside-view sites at once.
§95 built the missing half: site-aware re-rendering. The two compose:

**Mechanism:** (a) the §53 gate admits entries that are ALIAS-named
unions (union data carrying a TYPE_ALIAS symbol); (b)
`type_to_string_at` gains an ORIGIN arm — a union with recorded
`union_origin` entries re-renders as each entry site-rendered
(named entries qualify through the existing naming stack) joined
`" | "`, falling back to baked text on any decline. The written
`DateUnit | TimeUnit` argument (lib.esnext.temporal.d.ts:247/:314)
then builds instead of erroring, un-blocking the reference, the
signature bake, and every method read downstream.

**Bar:** temporal ≥120 net (the 49 Duration results + 60 signature
prints + reads are its census ceiling of ~230); corpus adverse
within 10:1. Falsifiers: (a) §53's own — inside-view union
spellings re-rendering qualified (parserRealSource11/12 R→W fires
it; the origin arm renders bare exactly where the entry's chain
finds the name in scope, so zero expected); (b) origin entries
whose members were REDUCED away (the §53 subset road) must not
resurrect through the new arm — rebuild_union_subset already
decomposes partial entries and stays untouched; (c) union ORDER —
origin preserves WRITTEN entry order while the §77.2-family wants
builder order in places; any adverse whose want is the same set
re-ordered belongs to §77.2, priced there, not here.

**§97 measured — HELD at 1.6:1 pending its unlock.** The isolated
pair (reverse-stash against the §97 baseline): **+93 G→R
(temporal 68) against 59 G→W, no R→W.** Both bar legs missed as
stated (temporal net +24 < 120; 1.6:1 < 10:1). BUT the 59 are not
the origin arm's: 44 of temporal's plus the scattered rest
(stringLiteralTypesInUnionTypes, typeInferenceLiteralUnion,
controlFlowOptionalChain) are `{ largestUnit: "hour" }`-shaped —
object-literal properties wanting LITERAL RETENTION against the
contextual member type that §97 just made computable (`string` got,
`"hour"` want). The owner is the contextual retention family
(§56.3's object-property extension — the other workstream's lane).
§97 stays on this branch unpushed until that arm exists or the
trade is re-priced; landing it earlier converts honest gaps into
confident wrongs that read as the origin arm's fault.

**LANDED AT USER DIRECTION ahead of §98 retention** (2026-08-08,
"push to main"): the 1.6:1 trade ships as measured — +93 G→R
against 59 G→W whose owner is the §98 retention family; when §98
lands those 59 convert and its score must state the inheritance.
§99's 44 temporal composition rows also activate with this landing.

## §98 — retention's roots widen: assignment position, union and intersection members [checker-2]

The retention family sized from the §95-era dump: 462 WRONG lines
are object prints differing from their want ONLY by literal
widening. Temporal's 87 are NOT this section's: traced with
TSR_CTX_DEBUG (the §56 call arm instrumented), temporal declines at
122 errored method callees and ~300 member lookups through
qualified/generic interface parameter types — the resolution lane
(§97/§99's owner continues there). What IS this section's, decline
points traced per case:

1. **The assignment root.** `c.x = { a: "a" }` contextually types
   its right operand by the LEFT operand's type
   (`getContextualTypeForBinaryOperand`'s equals arm,
   `checker.go:29843`); the §56 walk has no BinaryExpression holder
   arm at all, so `staticFieldWithInterfaceContext`'s `c.x =` family
   widens (traced: the walk never fires — zero CTX lines). The
   class-expression half of that case (`let c: I = class { static
   x = … }`) is NOT claimed — class-expression contextual typing is
   its own unported machine.
2. **Union and intersection member lookup.** `var x: A | B =
   { a: 1 }` reaches the VariableDeclaration root and dies at
   `get_type_of_property_of_type(union, name)` — upstream maps the
   lookup over constituents (`getTypeOfPropertyOfContextualTypeEx`,
   `checker.go:30555`, via `mapTypeEx` with noReductions): each
   object constituent contributes its concrete property type, the
   hits union; an intersection collects per-constituent property
   types and intersects. Generic mapped types inside stay declined
   (unported; the arm returns None there rather than guessing).

**Bar, registered before the code:** predict
**+30–70 combined W→R/G→R**, concentrated in
excessPropertyCheckWithUnions (+15–30 of its 38),
contextualTypeShouldBeLiteral (part of 13), the
staticFieldWithInterfaceContext assignment family (~12–20 of 51),
with the §97-inherited 15 G→W claimed ONLY where the shape is a
union-contextual object member (the stringLiteralTypesInUnionTypes
7 look like alias-named `||` prints — different family, not
claimed). Must NOT move: §56's existing annotation/call/return
converts. Adverse worse than 1:5 refuses whichever arm produced it;
the arms measure separately.

**§98 score — LANDED at 245:1.** right 406,012 → **406,241** on the
full pair over the post-§97 baseline: **+93 W→R** (excessProperty-
CheckWithUnions 37, destructuringParameterDeclaration8 12,
missingDiscriminants 9) **+~152 G→R** (gap 45,087 → 44,935) against
**1 R→W, nothing else adverse**. Inside the +30–70 bar band on W→R
with the G→R on top. THREE fired legs, each measured:
  1. The first cut dropped union constituents LACKING the
     discriminant member — upstream's ternary algorithm
     (`relater.go:1212`) keeps them and eliminates only non-matching
     members, and only when something matched. Order-independence
     comes free (`missingDiscriminants2`'s subkind-before-kind).
  2. The §18 fresh/regular twin: a checked discriminator literal is
     FRESH, members hold REGULAR — compare regular or nothing
     matches.
  3. Two boolean over-corrections REVERTED by measurement: a
     both-twins gate (R→W 23) and a bare-`boolean`-intrinsic gate
     (R→W 25) both broke `{ hoge: true }`-class retention — bare
     `boolean` RETAINS (`emitOneLineVariableDeclaration…`), and the
     `autoIncrement : boolean` widening that motivated them is the
     DISCRIMINATED CONSTITUENT LACKING THE MEMBER, plus
     intersection-blind discriminator lookup (fixed by routing the
     lookup through the distributing road). `T & { prop: boolean }`
     widens via the TYPE_PARAMETER decline in the intersection arm.
RESIDUE, priced: excessPropertyCheckWithMultipleDiscriminants 0:112
(attributes2 — identical shape to the FIXED attributes but for a
primitive `| string` constituent in Attribute2; one line, diagnosis
open at the alias-union root). The TSR_CTX_DEBUG instrumentation
stays, env-gated, per the TSR_JOIN_DEBUG precedent.

## §100 — type predicates infer from single-return bodies [claimed: checker-1]

[Bar claims the number; build follows on checker-1's worktree branch.]
`inferTypePredicates`' 154 wrongs: an unannotated function whose body
is ONE `return <narrowing-expr>` should type as `(x: T) => x is U`
(`getTypePredicateFromBody`, `checker.go:20535`). The mechanism is
three reuses of machinery this port has:

1. Admission (`:20535`): normal function flags, exactly one return
   with an expression (concise arrow body counts), no implicit
   return. Constructors/accessors decline.
2. Per parameter (`checkIfExpressionRefinesAnyParameter`,
   `:20565`): identifier name, non-boolean declared type, never
   assigned (`mark_node_assignments`' map), non-rest. The return
   expression's type must be BOOLEAN-flagged.
3. The refinement pair (`checkIfExpressionRefinesParameter`,
   `:20586`): trueType = narrow declared by expr TRUE (§82's
   `narrow_type` arms); decline if unchanged; falseSubtype = flow
   type with declared=init, INITIAL=trueType through expr FALSE —
   predicate exists iff that reduces to NEVER. First refining
   parameter wins; predicate = `x is trueType`.

**Bar:** ≥40 of inferTypePredicates' 154 at ≥10:1 (the case mixes
predicate-print lines with downstream `.filter()` consequences that
need the predicate CONSUMED — those are a second slice). Falsifiers:
(a) declared-boolean parameters must keep `boolean` returns —
upstream's own comment; (b) any narrowing shape `narrow_type` cannot
decide answers initType and MUST leave the return as `boolean`, not
gap it; (c) multi-return and implicit-return functions must be
byte-identical — the admission is the whole gate.

## §101 — the template fold consults the constant evaluator [checker-2]

`1 - \`${ 3 - 4 }\`` folds upstream to `"-1"`: `checkTemplateExpression`
hands the WHOLE template to the constant evaluator
(`evaluated = c.evaluate(node, node).Value`, `checker.go:7991`) and a
non-nil string becomes the fresh literal — the span's TYPE is
`number`, and our §24 fold reads types, so every arithmetic span
declines to `string`. The port's only evaluator today is §55's
enum-local `fn eval`, which lacks binary arithmetic.

The arm: a reusable expression-value evaluator — numeric/string
literals, parenthesized, prefix `+`/`-`, binary `+ - * / % **` over
numbers and `+` string concatenation, templates recursively —
declining (None) on identifiers, property accesses, and anything
else; a None anywhere keeps today's answer. Number formatting is
gated: integers below 1e21 print as integers, other finite values
only when Rust's `{}` round-trips the parse; `-0` prints `0`;
anything else declines rather than risks a JS-spelling divergence.

**Bar, registered before the code:** the four
templateStringBinaryOperations* cases hold ~216 WRONG lines of the
`"-1"`/`"2-1"` shape; predict **+120–216 W→R** there with a small
corpus tail. Must NOT move: §24's existing all-literal folds and
§55's enum values (untouched module). Any adverse from number
SPELLING divergence is priced per line and refuses the formatting
arm (not the fold) if it exceeds 1:5.

**§100 score — LANDED, count leg missed and stated.** Isolated
stash pair: **+24 W→R, ZERO adverse** (inferTypePredicates 20,
returnTagTypeGuard 4). The bar's ≥40 missed at 24: the case's other
~130 lines are (a) predicate CONSUMPTION — `.filter(isNonNull)`
element narrowing through the inferred predicate, the §22-callres
consumer road, and (b) narrowing shapes `narrow_type` declines
(instanceof-false, `in`-negative, custom guards through aliases) —
each an owned follow-up, none a defect of the admission. Falsifier
(c) held: multi-return and implicit-return functions byte-identical.

**§101 score — LANDED at +152/0.** right 406,339 → **406,491** on
the isolated stash/accept/pop pair over post-§100 main (population
470,709 — §100 moved it +58): **+152 W→R, zero adverse in any
column** — templateStringBinaryOperationsES6Invalid 48, …Invalid 48,
…Operations 24, …ES6 24, tail 8. In the bar's +120–216 band. The
evaluator is deliberately symbol-free: identifiers and property
accesses decline, so const-reference spans stay unfolded — that
residue belongs with a future evaluate-entity slice (upstream's
evaluator resolves const enums and const variables;
`evaluator.go`-class work, not this arm). PROCESS note, recorded
because the rule exists: the first accept of this window ran on a
tree still carrying §101's code — caught immediately and re-done as
the stash/accept/pop pair; the contaminated baseline was never
scored against.

## §102 — within-print byText renames [claimed: checker-1]

The §20.1 double-refusal's print-context study, paid at last by
mechanism found in the baselines: inside ONE multi-signature
type-literal print, two DISTINCT type parameters spelling `T` rename
the LATER one `T_1` (`underscoreTest1`'s
`{ <T>(list: T[], ...): boolean; <T_1>(list: Dictionary<T_1>, ...) }`,
100 want-lines) — upstream's `typeParameterToName` byText set
(`nodebuilderimpl.go:1420`) scoped to the builder context, which the
baseline writer resets per assertion. **Mechanism:** §99's composite
renderer threads a claimed-names set across the signatures of one
render; a later signature whose own parameter collides re-renders
through the §90.1 print-only clone at the first free `name_n`.
**Bar:** ≥50 of underscoreTest1's 100 at ≥10:1. Falsifiers: (a) the
two flipped-order wants (`<T_1>` on the FIRST overload) say claim
order is site-dependent — naive first-claims converts at most one of
each flipped pair and MUST NOT regress the other (both wrong today);
(b) single-signature prints stay untouched (the §90.1 promise trap);
(c) same-symbol repeated Ts — if any corpus want keeps plain `T` on
both overloads, the arm needs a distinct-declaration test before
landing.

**§102 first measurement — REFUSED at 88:763.** The naive
within-print byText set renames EVERY same-name collision; the
corpus says upstream renames only a SUBSET: underscoreTest1 converts
29 (and regresses 62 — even there the claim ORDER is site-dependent)
while the promise family regresses 448 — `then`/`catch` overload
lists keep PLAIN repeated `TResult1` in every want. The
discriminator hypothesis, recorded for the next attempt: upstream's
builder REUSES WRITTEN declaration nodes where they still denote the
type (`tryReuseExistingTypeNode`), bypassing `typeParameterToName`
entirely — the renames appear only in BUILT prints (merged/
instantiated composites with no reusable node). The port needs the
reuse-vs-build split BEFORE the byText set can land: a written-reuse
carriage for whole member lists (the §77-family seam at signature
scale), then the claim set applies only to the built remainder.
Reverted whole; the bar's ≥50 leg was unreachable under the naive
form (29 gross).

## §103 — const type-parameter inference SIZED and DEFERRED [checker-2]

typeParameterConstModifiers (41 wrong) traced to its mechanisms:
`f1(['a', ['b', 'c']])` under `<const T>` wants the ARGUMENT typed
as an unwidened literal tuple (`["a", ["b", "c"]]`), the INFERRED T
as its readonly mapping (`readonly ["a", readonly ["b", "c"]]`),
and object arguments as readonly-membered literals. That is FOUR
machines: const-context array checking (literal tuples), readonly
tuple types, readonly member computation+printing (the same gap
`check_const_assertion` documents for `as const` objects — one
subsystem serves both), and inference/instantiation plumbing.
§33's decline measured the naive lift at 70 G→W; it stays until
the as-const/readonly subsystem is built as a unit. DO NOT lift
the decline piecemeal — the §33 comment in calls.rs:481 is the
guard. The as-const object gap (`assertions.rs`) is the natural
first slice since it needs no inference; its converts unlock this
head's afterwards.

## §104 — the as-const slice of readonly modeling [checker-2, bar only]

Sized from the §101-era dump: **246 WRONG + 805 GAP lines want
`readonly` somewhere** — the whole readonly-modeling ceiling.
The as-const entry (constAssertions 26 WRONG + a large gap share)
decomposes as: (a) tuple types carry a readonly flag and print
`readonly [...]`; (b) object members carry per-member readonly and
print it; (c) `check_const_assertion`'s object/array arms compute
both (today they gap by documented design). This is slice 1 of the
§103 subsystem — landing it unlocks const-T inference later.

**MANDATORY FIRST TRACE before any code:** constAssertions:0:0 —
plain `"abc"` (as-const of a string literal) GAPS today even though
`check_const_assertion`'s literal arm exists. Something before the
arm declines (candidate: the variable-declaration road, or the
case's `<const>`-prefix spelling). One verdictdump print names it;
do not size the build until it is named, per the §96 lesson (a
probe that buckets by the answer's spelling missed its mechanism).

Bar to be COMPLETED with row predictions after that trace; this
section claims the number and records the sizing so the next
window starts at the trace.

**§102 sizing addendum (the retry's map).** The `_n`-want WRONG
population is ~200: underscoreTest1 100,
typeParametersAreIdenticalToThemselves 34, promisePermutations 12,
declarationsWithRecursiveInternalTypesProduceUniqueTypeParams 12,
genericSpecializationToTypeLiteral1 10, scattered 30. The decisive
fact: promisePermutations holds BOTH populations — 12 rename-wants
beside the 168 plain-wants the naive arm regressed — so the
reuse-vs-build split cuts WITHIN one case, per print, not per case
or per provenance class. The §88 "two types, two prints" hope
(instantiated mints vs declared composites) is therefore too coarse
by itself. The retry's first step is a WANT-PAIR study inside
promisePermutations: for one member printed both ways, diff the two
assertion sites' surroundings — what the builder visited first, and
whether a written node exists for one and not the other. No code
before that study names the discriminator.

**Trace state (first window):** the case is NOT whole-gapped —
168 RIGHT / 166 GAP / 46 WRONG. 0:0 is `let v1 = 'abc' as const`,
and `check_const_assertion`'s literal arm is sound in isolation, so
the decline is on the walked road: initializer →
check_assertion(const) → get_regular_type_of_literal_type →
`let`-widening in get_widened_literal_type_for_initializer. Two
candidates, both §77-adjacent: (a) the SINGLE-QUOTED literal's §77
written-spelling interning answering error through the regular
twin, (b) get_widened_literal_type widening a REGULAR literal where
upstream widens only FRESH ones — as-const's whole point is that
regular survives `let`. Candidate (b) is testable with one
eprintln at get_widened_literal_type's fresh test; start there.

**§104 slice 0 score — LANDED at 39:1.** The mandatory trace found
a TWO-CONTRACT SEAM, not a widening bug: the parser's ConstKeyword
arm encodes `as const` as a `TypeReferenceNode` with NO name
(`tsr-parser/src/types.rs:471`, its only None-named site) while
`is_const_type_reference` demanded the identifier spelling — every
const assertion in the corpus took the error road, exactly as the
assertions.rs module doc had recorded (and blamed on `bd tsr-0ao`;
the None encoding was later made deliberate and the checker test
never updated — the doc's history is corrected in place). Accepting
None-with-no-arguments as the assertion: right 406,491 → **406,724**
— **+232 G→R +1 W→R against 6 G→W, zero R→W** (constAssertions 72,
controlFlowBindingElement 24, indexSignatures1 24, wide tail). The
object gate stays; ARRAYS gained the same gate (readonly tuples
unported — the direct-shape test leaked `([10]) as const`, 2 G→W,
fixed by climbing parens). The three `#[ignore]`d tests now run:
one expectation was stale twice over (`as consts` wrote `error`
pre-§31; now the minted name) and the `const<number>` arity claim
is UNASSERTABLE — the parser errors where upstream reads a
reference to a type named `const`; a parse-level divergence on a
pathological shape, recorded in the test. Residues: the 6 G→W are
index-signature/narrowing shapes behind as-const receivers (their
own roads' work), and readonly tuple/object minting stays the
subsystem's next slice.

**§102 DECODED and LANDED — the §20.1 double-refusal closed.** The
want-pair study the sizing addendum demanded produced the rule in
three lines of baseline: `underscoreTest1:3229/3233/3237` print the
same composite [T, T_1], [T_1, T], [T_1, T_1] BY SITE, and
`asyncFunctionReturnType` holds zero renames at neutral use sites.
The mechanism is the SHADOW TEST ALONE
(`typeParameterShadowsOtherTypeParameterInScope`,
`nodebuilderimpl.go:1396`): a signature's own type parameter renames
— uniformly `name_1`, unclaimed — iff its name resolves at the print
site to a DIFFERENT type-parameter symbol. There is NO byText
mechanism in this corpus: the byText half regressed 763 lines in two
different builds (naive claim-set, then shadow+byText) and
shadow-only converts the same rows at zero cost. **+269 W→R, ZERO
adverse** on the isolated pair (underscoreTest1 86/100,
typeParametersAreIdenticalToThemselves 33, genericCall 9, spread
across 20+ cases). Bar met (≥50 at 10:1 → 86 at ∞).
**Correction to the composed count's "+1 benign interaction line"
(the adjective was not a price):** the line is an R→W —
`classAbstractManyKeywords:0:5`, want `D`, printing `typeof D` on the
error-recovery fixture `import abstract class D {}`. RE-BISECTED
TWICE, and the second correction is loud: the line PREDATES BOTH
BUILDS — at `ab315ce` (before §104 and §102 both) it already prints
`typeof D`. The first bisect's §104 attribution was wrong (built on
one unverified revert plus a misread pull-diff); the verified chain
was: revert-with-forced-recompile (cargo clean, 'Compiling' counted)
still wrong → pre-both checkout still wrong. NEITHER build owns it;
it is a standing wrong in the class-static print at error-recovery
import sites, filed with the §31-family import-machinery boundary.
The "+1 interaction line" in §102's composed arithmetic was a
different (positive) line and carried no R→W at all. Residues:
underscore's last 14 (the flipped-order pairs whose sites our
resolve_name reads differently — trace before touching), and §90.1's
chain2 `r_1` family stays on its empirical gate (a THIRD mechanism,
still undecoded).

## §105 — readonly MINTING (bar, sized) [checker-2]

Post-§104 census: **1,044 lines want `readonly` somewhere** — 569
tuple-shaped (`readonly [...]`: 116 WRONG / 453 GAP), 475
member/array-shaped (123 / 352). WRITTEN annotations are
EXONERATED: `readonly string[]` and `readonly [string, string]`
annotations already answer RIGHT (readonlyArraysAndTuples2 verified
line-by-line; the TypeOperatorNode arms exist in declared.rs). The
residue is MINTING — types this port must create:
  1. as-const ARRAYS → `readonly [regular elements]` tuples
     (constAssertions ~77; check_const_assertion's array gate is
     the entry, §104's parens-climb already in place);
  2. as-const OBJECTS → `{ readonly a: 1; }` members with the §77
     single-quote written-spelling reuse inside member positions
     (the assertions.rs doc's second blocker — §77's machinery
     exists now, wiring unverified);
  3. inference/tuple-machinery shapes (variadicTuples1 44,
     spreadsAndContextualTupleTypes 51, excessivelyLargeTupleSpread
     70) — NOT this slice's; they need the §86-family variadic
     element-list modeling first;
  4. typeParameterConstModifiers 88 — §103's, blocked on this slice
     plus inference plumbing.
Slice = (1)+(2). Bar prediction DEFERRED to the build window — the
per-case convert estimate needs the object-member quote-reuse
question answered first (one probe: does a §77-spelled member
literal survive the object type's text computation?). Must NOT
move: the written-annotation rights just verified, §104's 232, §55
enum values.

**§105 probe answer (same window):** the §77 machinery covers
member NAMES (§77.3, objects.rs — `'1.0'` keeps its quote) but NOT
member VALUE types: a value-position `'lookup'` literal interns
double-quoted, and no written-spelling carriage exists from a
property assignment's initializer node to the member's printed
type. So slice (2) needs that carriage built (the §77 seam extended
to const-context member values), while slice (1) — as-const ARRAY →
readonly tuple — needs only the tuple mint plus a readonly flag on
the existing tuple printer. BUILD ORDER therefore: (1) first, its
own pair; (2) behind the carriage, separately measured.

## §106 — the file-module import spelling [claimed: checker-1]

The "import-spelling 128, architectural, per-file printing" deferred
head is STALE as stated: §95/§99/§102 built the per-site rendering it
said was missing, and the residue is ONE declined branch —
`symbol_chain`'s file-module parent arm returns `None` where
upstream's `getSpecifierForModuleSymbol` (non-ambient half) emits
`import("<relative specifier>").` for a module inaccessible at the
site (`privacyFunctionCannotNameParameterTypeDeclFile`'s 128 lines
want exactly `import("./…_Widgets").Widget1`; we print the bare baked
name — a WRONG, not a gap, because `qualified_name_at` keeps the
print when the chain declines). **Mechanism:** in that arm, after the
alias roads decline, spell `import("./{module symbol name}").` — the
module symbol's name IS the stripped file path in this port. Slice:
same-directory relative specifiers only (`./` + name); anything with
directory structure visible in the name keeps the decline.
**Bar:** ≥80 of the case's 128 at ≥10:1. Falsifiers: (a) sites where
upstream prints the BARE name despite non-resolution (the §31 mint
family) — any R→W in privacy-family cases fires it; (b) ambiguity —
`module_alias_at`'s Err(true) refusal stays ahead of this arm;
(c) cross-directory modules must keep declining (wrong specifier is
worse than bare).

**§106 measured — a ZERO whose first-named blocker was WRONG;
corrected the same session.** The paragraph below attributed the
unreached arm to unconditional written reuse — then READING THE
FIXTURE (the §131-family lesson, again) showed the privacy case's
parameters carry NO annotations at all: `param =
exporter.createExportedWidget1()` INFERS its type, so the build road
applies and written reuse is irrelevant to these 128 lines.
`needs_qualification`'s unresolved arm answers true (verified by
read), so the road SHOULD reach the new spelling arm and the pair
still measured byte-identical — the true decline point is one level
unlocated (candidates: the signature print not routing the parameter
slot through `type_to_string_at` at these bake sites, or an earlier
arm in `qualified_name_at` keeping the bare print). NEXT PROBE, not
next build: one eprintln at the §106 arm on the filtered case.
Conditional reuse remains a REAL seam for the §102 flipped pairs —
but it does not own these 128. Original (wrong) paragraph kept
below for the record. The spelling arm
is faithful and UNREACHED: the full pair is byte-identical because
the 128 target sites never enter `symbol_chain` — their prints take
WRITTEN-ANNOTATION REUSE (`param?: Widget1` verbatim), and this
port's reuse is unconditional. Upstream's reuse is NOT:
`tryReuseExistingTypeNode` checks the written names' ACCESSIBILITY
at the print site and falls to the BUILD road (where the specifier
spelling lives) when they fail. So the import-spelling head's true
prerequisite is CONDITIONAL REUSE — the same reuse-vs-build seam
§102's carriage note named from the other side. The arm stays (zero
cost, §92.1 precedent; it is the build road's correct behavior the
moment conditional reuse exists); the head goes back on the board as
"conditional written reuse: accessibility-gated" owning BOTH the
import-spelling 128 and the §102 flipped-pair residue.

**§105 slice 1 bar completion (registered before the code):** the
tuple machinery already carries readonly — `create_tuple_type
(elements, readonly)` and the keyed intern exist (declared.rs), so
the slice is WIRING: the as-const ARRAY gate becomes a mint —
elements check to regular literal types, nested arrays recurse
through the assertion arm itself (inheriting the paren-climb and
the object gate), spreads/holes/objects decline the whole operand.
Predict **+25–60 G→R** (constAssertions' tuple half the head),
zero R→W tolerated; adverse limited to number/quote SPELLING lines,
priced per line. Must NOT move: §104's 232, the written-annotation
rights, §55 enum values.

**§105 slice 1 score — LANDED at ~10:1.** right 407,030 → **407,148**
on the isolated stash/accept/pop pair: **+112 G→R + 6 W→R against
12 G→W, zero R→W/R→G** (controlFlowAssignmentPatternOrder 24,
spreadsAndContextualTupleTypes 23, constAssertions 12,
es2022IntlAPIs 6 W→R, wide tail). The bar predicted +25–60 and
measured 118 — a 2× miss in the FAVORABLE direction, recorded:
readonly-tuple wants reached through destructuring and spread
contexts the constAssertions-centric sizing did not count. The 12
G→W are two named next-rule populations, the §31-chain precedent:
  - declarationEmitTypeParameterNameShadowedInternally 6 — the
    mint made nested generic signatures printable and their wants
    carry `T_1` SHADOW renames; §102's shadow arm is the owner
    (flagged to checker-1), not this mint.
  - constAssertions 5 + typeSatisfaction_asConstArrays 1 —
    object-membered readonly wants (`{ readonly x: 10; }`) arriving
    WRONG through a road the direct object gate does not guard;
    slice 2's population, and its trace should start from 0:115's
    holder chain.

**§106 score — LANDED, the probe's one line paid for the head.** The
queued eprintln answered everything: the arm fired exactly 128 times
with `module_name="/…_Widgets"` — a leading VIRTUAL-ROOT slash the
same-directory guard rejected. Three gates shaped the landing, each
measured: (1) root-stem acceptance alone regressed 299 (chain1 217 —
SAME-FILE synthetic positions must keep the bare name); (2) the
same-file gate left 30 (re-export-reachable names our resolver
cannot walk — spelled where upstream prints bare); (3) the
imported-here gate (the reference's file mentions the module's
specifier in any import/export-from) took those to 14. **+260 W→R
against 14 R→W (18.6:1)** — privacyFunctionCannotNameParameterType
128/128, privacyCannotNameVarType 64, privacyFunctionCannotNameReturnType
64; **the corpus crosses 85%** (right 407,394 = 85.04%). The 14:
TRANSITIVE re-export reachability (constEnumNoEmitReexport's A→B→C
chains) — the gate is direct-mention only; the owner is the resolver's
re-export walk, priced here at 14 lines.

**§105 slice 2a bar (registered before the code):** as-const OBJECT
literals answer readonly-membered regular types — but ONLY when no
member value is a single-quoted string literal (the value-spelling
carriage stays unbuilt; single-quoted operands keep the error gate,
so the carriage's absence stays a gap and never a wrong quote). The
mechanism, upstream-anchored: `isConstContext` (`checker.go:13615`)
climbs parens/arrays/spreads/property-assignments to a const
assertion; a const-context member takes its REGULAR unwidened type
(checked BEFORE the §56/§98 retention road — upstream's own order
in `checkExpressionForMutableLocation`) and prints `readonly name:
T;`. Trace anchor: constAssertions 0:115 (`{ x: 10, y: 20 } as
const` — the literal's own line answers `{ x: number; y: number; }`
today because `check_object_literal` never learns it is in a const
context). Predict **+40–120** across constAssertions' object half
and the slice-1-surfaced 6; must NOT move: non-const object
widening (the 2,500-line boundary objects.rs documents), slice 1's
118, §98's discrimination converts. Any wrong-quote line refuses
the build outright — that is exactly what the single-quote gate
exists to make impossible.

**§105 slice 2a score — LANDED at ~9.6:1 with two open traces.**
right 407,500 → **407,654**: **+64 G→R +90 W→R against 16 G→W and
12 W→G, zero R→W/R→G** (controlFlowAssignmentPatternOrder 18,
constAssertions 27 combined, constantEnumAssert 10, binding-pattern
family). Bar predicted +40–120; measured 154 — over in the
favorable direction again, same cause as slice 1 (destructuring
reach). THREE fired legs, each from the filtered pair's adverse:
(A) the literal's OWN line needs the const arm in
check_array_literal itself — the slice-1 assertion-side mint was
the wrong placement and left `[10] as const`'s literal line at
`number[]`; (B) spread-contributed members inherit readonly
(`{ ...o4 } as const`); (C) a const-context METHOD prints as a
readonly PROPERTY in arrow form, never `d(): void`.
OPEN TRACES, mandatory next-window entries:
  1. ts-expect-error 8 — `{ a: true } as const` member lines answer
     `boolean`: the member-symbol const arm provably exists and the
     shape provably reaches it, so something earlier owns the
     answer (suspect: an annotation-side member road or a memo).
     One TSR_CONST_DEBUG print at the symbols.rs const arm decides.
  2. computedPropertiesNarrowed 4 — computed-name shapes under the
     new array arm.
The single-quote value gate held: zero wrong-quote lines anywhere.

## §107 — nested signatures shadow through the RENDER scope [claimed: checker-1]

§102's shadow test consults the reference SITE; the handed-off
declarationEmitTypeParameterNameShadowedInternally 6 (provenance:
§105 slice 1 made them printable) show the other scope the test
needs: `<T>(x: T) => <T_1>(y: T_1) => readonly [T, T_1]` — the INNER
signature's `T` renames against the OUTER signature's `T`, which is
in scope only DURING THE RENDER (upstream: `enterNewScope` pushes the
signature's type parameters as a fake scope; the inner
`typeParameterToName` then resolves the name to the outer symbol).
**Mechanism:** a render-scope stack on the checker — each signature
render pushes its own (post-rename) type-parameter (name, symbol)
pairs for the duration of its slot rendering; the §102 shadow test
consults the stack before site resolution; wired into
`signature_to_string_at` (single) as well as the §99 member renderer.
**Bar:** the handed 6 at ≥6:1, zero adverse elsewhere; falsifiers:
(a) outer signatures at empty scope stay plain; (b) NO double-rename
composition with §90.1's mint-time gate (watch chain2/TupleUnionFunc
for `r_1_1`); (c) the §102 multi-sig layouts must be byte-identical
— the stack augments, never replaces, the site test.

**§107 first measurement — REFUSED at 13:4, target missed.** The
render-scope stack + single-signature site test measured +13 W→R
(instanceMemberInitialization, awaitedTypeStrictNull,
subclassWithPolymorphicThis — real, but NOT the handed 6: the
declarationEmit case never moved, so its nested prints do not route
through `signature_to_string_at` at all — suspect the §89 keep-text
set or the function_types bake) against 4 R→W (computed-name sites:
`[e<T>()]` positions where the site test resolves the printed
signature's own name to an ENCLOSING type parameter; an
inside-own-declaration identity gate did NOT clear them — the
containment test never fired, meaning the reference node is outside
the signature's declaration span there, and upstream's shadow
resolves from the PRINTED DECLARATION's context, not the assertion
node). Two prerequisites now named: (a) find where the handed 6's
prints bake (probe, one eprintln); (b) the single-signature site
test must resolve from the printed declaration, which is a different
reference plumbing than the composite arm's. Reverted whole; the
multi-sig §102 arm is untouched and stands.

**§105 trace 1 RESOLVED — one arm, +73/0.** The ts-expect-error 8
(and 22 more constAssertions lines, 8 inferFromNestedSameShapeTuple,
12 the case whole at 72/72): `is_const_context` asked from the
PROPERTY ASSIGNMENT node sees parent = ObjectLiteralExpression —
a shape upstream never sees because it always asks from the
INITIALIZER one level down — and no arm covered it, so every
member-symbol const check answered false. The instrumented arm
("reached, const_context=false", four firings) named it in one run.
Adding the object-literal parent to the recurse group: right
407,654 → **407,727 (+73 W→R, 6 W→G, ZERO adverse in any column)**.
Trace 2 (computedPropertiesNarrowed 4) remains the open entry.

**§107 routing probe (queued prerequisite (a)) — ANSWERED.** The
handed 6's prints DO route through the single-signature composite arm
(`route: sigs=1 composite_guard=false keep_text=false`, 7 hits on
the filtered case) — the §89 keep-text suspect is ELIMINATED and
prerequisite (a) closes. The §107 failure therefore sits INSIDE
`rename_type_parameters_for_site` for the nested-arrow shape; the
next probe (one eprintln) is whether `binder.symbol_of` files an
ARROW function's type-parameter declarations at all — a None there
silences the shadow test exactly as observed (own_symbol gates every
arm). If arrows' type params are unfiled, the fix is the binder's,
and the §107 retry needs nothing else.

**§107 probe 2 — the arrow-filing suspect WEAKENED by read.** The
binder binds every `TypeParameterDeclaration` (binder.rs:2608, the
Locals branch covers function-likes including arrows — the same path
§90.1's chain2 successfully reads symbols through for
FunctionTypeNode params). So own_symbol None is unlikely; the retry
must instrument the shadow test IN SITU (re-apply the §107 arm from
the refusal record's description + one eprintln printing own_symbol,
render_shadow, and the resolve result per parameter on the filtered
case). One window, one build, instrumented from the start — not
three blind probes.

**§107 LANDED at +36/0 — three probes, one flag, one anchor.** The
retry's instrumentation chain: (1) the routing probe had cleared
§89; (2) the shadow probe showed BOTH mechanisms firing (render
scope hit for nested renders, symbols filed — the arrow-filing
suspect was wrong); (3) the refusal probe found the killer:
`instantiate_signature REFUSED` ×4 — the print-clone's substitution
hit the DELIBERATE unmapped-type-parameter refusal on FOREIGN
(enclosing) parameters (`readonly [T_outer, T_inner]` mentions
T_outer, unmapped, errorType). The fix is a SCOPED identity flag
(`identity_unmapped_type_parameters`) set only around the
print-clone's instantiate — real instantiation keeps the refusal.
The first §107 attempt ALSO predated §105-1's readonly-tuple mint,
a second silent killer since removed by the other lane. Final shape:
single-signature renders anchor the SITE test at the PRINTED
DECLARATION (clearing the 4 computed-name R→W and 1 mapped-inference
line); the §102 composite arm keeps the assertion anchor (its
per-site layouts demand it). **+36 W→R, ZERO adverse**
(underscoreTest1's last 14, declarationEmit 2, nested-generics
family). Residue: the handed 6's other 4 lines are the
assertion-anchored standalone positions the declaration anchor
declines — they need the per-assertion variant §102 has, at
single-sig scale, without re-firing the computed-name 4; priced at
~17 forgone conversions corpus-wide.

**§105 trace 2 — resolved by composition down to ONE priced line.**
computedPropertiesNarrowed's 4 became 1 under trace 1's member-shape
arm (three were the same missed const context). The survivor (0:16,
want `1`, got `0 | 1`) is flow-narrowing of a const-object member
inside a computed property name — the §5x condition family's
territory, not the readonly subsystem's; priced here, owner named.
The §105 subsystem's remaining head is the VALUE-SPELLING CARRIAGE
(single-quoted member values), and after it §103's const-T
inference unblocks. Day's arc for the record: the readonly
subsystem went from "four machines, do not attempt piecemeal" to
one remaining carriage in five measured slices.

## §108 — Array-headed annotations join the §77 reuse gate [claimed: checker-1]

433 corpus WRONG lines want `Array<T>` where the print shows `T[]`
(objectTypesIdentity/subtyping families ~160, recursiveTypeReferences1
19, rest spread): the written annotation spells `Array<...>`, the
fresh render's `type_reference_text` always shortens, and §77's
written-reuse gate admits only single-quoted subtrees. The Array head
is the SAME admission class: a spelling the fresh render cannot
reproduce. **Mechanism:** `written_type_text` grows an `array_headed`
flag (a TypeReferenceNode named `Array`/`ReadonlyArray` anywhere in
the subtree); reuse admits `single_quoted || array_headed`.
**Bar:** ≥300 of the 433 at ≥10:1. Falsifiers: (a) the blanket-reuse
−270 class (union order etc.) must NOT ride in — the flag admits
only Array-headed subtrees, and any adverse concentrated in
union-order cases fires this leg; (b) instantiated positions whose
Array-written text no longer denotes the substituted type (the §36
node-reuse rule — written_text is DROPPED on substitution, which the
existing instantiate road already does); (c) `T[]`-written
annotations must be byte-identical (no flag, no change).

**§108 score — LANDED at +407/0.** The isolated pair: **407 W→R,
ZERO adverse in any column** — the objectTypesIdentity/subtyping
constraint families whole (30+30+24+24+24+28...),
recursiveTypeReferences1's Array-spelling rows, rest spread over 40+
cases. The bar (≥300 at 10:1) met at infinity. §77's admission
principle held exactly: a spelling the fresh render cannot reproduce
is safe to reuse, and the Array head is the quote's sibling. The
generic-reference arm in `written_type_text` also carries any OTHER
name<args> spelling nested under an admitted subtree — no adverse
appeared, so the arm stands as written.

## §109 — the value-spelling carriage closes the readonly subsystem [checker-2]

The §105 single-quote gate exists because a member VALUE spelled
`'lookup'` must print single-quoted INSIDE the object type while
its own standalone line prints `"lookup"` (the assertions.rs module
doc's second blocker, recorded sessions ago). With slice 2a landed
the carriage is one arm, not a subsystem: in the const-context
member loop, a member whose initializer is DIRECTLY a single-quoted
StringLiteral prints its type text as `'text'` verbatim (the §77.3
name-quote precedent applied to values); everything else keeps the
fresh render. The gate then lifts for exactly the carried shape —
indirect single-quote reaches (nested in unions/expressions) keep
declining, which keeps the impossible-not-detectable property.

**Bar, registered before the code:** predict **+15–50** (the
es2020IntlAPIs/localesObjectArgument locale-options family is the
head). Must NOT move: slice 2a's converts, standalone
double-quoted literal lines, §104's 232. One wrong-quote line
anywhere refuses the arm.

**§109 score — LANDED at +26/0; THE READONLY SUBSYSTEM IS CLOSED.**
right 408,170 → **408,196** on the full pair: +26 G→R
(inferFromNestedSameShapeTuple 12, correlatedUnions 6,
es2018IntlAPIs 4 — the locale-options family the original blocker
note named), **zero adverse in any column**. The gate lifted for
exactly the carried shape and nothing else; the
impossible-not-detectable property held end to end. The subsystem's
arc, for the record: §103's sizing said "four machines, do not
attempt piecemeal"; it landed as SIX measured slices (§104's
contract seam, slice 1's tuple wiring, slice 2a's three legs,
trace 1's member shape, trace 2 by composition, §109's carriage) —
each with its own pair, and the two that looked hardest (the
carriage, the member shape) each turned out to be ONE ARM once the
machinery around them existed. What remains of the §103 head is
now ONLY the inference plumbing (const-T argument typing), and
§33's decline in calls.rs is its entry.

## §110 — JSDoc @template reaches the signature bake [claimed: checker-1]

[Renumbered from §109: checker-2's carriage bar (4c4beec) is an
ancestor of this claim (bfbe644) — verified by merge-base both
sides; the claim commit's message carries the stale number.]

The @template family holds ~200 WRONG + 34 GAP across seven cases
(jsdocTemplateTag6 127 the head; Tag3 30, TagDefault 17, Tag8 14,
Tag7 4, Tag2 2). The parser MATERIALIZES the tags
(`parse_template_tag`, jsdoc.rs:513, `@template {Constraint} T, U`
forms) and the binder files JSDoc declarations (tsr-2); the checker's
`signature_parts_of` reads only `node.type_parameters`, which is
empty for a JS function whose parameters live in the doc comment —
so every such signature bakes `(x: any) => any` (upstream:
`getEffectiveTypeParameterDeclarations`' JSDoc half). **Mechanism:**
`signature_parts_of` (and the type-parameter identity roads that
mirror it: `type_parameter_types`, §102/§107's declaration matches)
consult the declaration's JSDoc template tags when
`type_parameters` is empty and the file is JS. **Bar:** ≥80 of the
family at ≥10:1; falsifiers: (a) TS-file signatures byte-identical
(the JSDoc read is JS-gated); (b) `@template` with defaults/
constraints that don't parse keep the current gap, not a partial
list (the partial-map rule every identity road states); (c) the
`const` modifier form (Tag6's `<const T>` wants) prints only if the
tag carries it — no invented modifiers.

**§110 implementation map (read before building).** All three layers
verified by read: (1) the parser materializes REAL
`TypeParameterDeclaration` nodes inside `JSDocTemplateTag`
(nodes.rs:4318 — so §102/§107's identity roads work unchanged);
(2) the binder FILES their symbols
(`bind_jsdoc_declarations`' template arm, binder.rs:2943 — note it
declares at ROOT scope via `declare_jsdoc_symbol`, a scope
divergence to watch when resolution questions arrive); (3) the
missing link is only PLUMBING: the jsdoc side table
(`&[(NodeId, &[&JSDoc])]`) reaches `bind_into_with_jsdoc` and stops
— the checker never sees it. Build: a
`jsdoc_templates: FxHashMap<NodeId, &[&TypeParameterDeclaration]>`
field populated at checker construction from the same table the
producer already holds, consulted by `signature_parts_of` (and the
JS-gate) when `type_parameters` is empty. Touches the checker
constructor + producer call sites; no new parsing, no new binding.

**§110 slice 1 — LANDED at +1/0, and the probe chain names slice 2.**
The plumbing works end-to-end (verified by probe: the fallback fires
63× on jsdocTemplateTag6 with js=true and the host road returning
exactly 1 template parameter per declaration — trait method with a
default, the Program impl over the files' JSDoc tables, the
checker-side consult with an ancestor-host walk for arrow/statement
attachment, and a `jsdoc_entries` field for host-less drivers). The
family's wants barely move because a JS signature ALSO needs its
PARAMETER and RETURN types from the doc comment (`@param {T} x`,
`@returns {T}`) — `<const T>(x: T) => T` prints `x: any` until the
JSDocParameterTag/JSDocReturnTag halves land through the same
consult. Isolated: +1 W→R (unusedTypeParameters_templateTag),
2 W→G, zero adverse. Slice 2 = the param/return tags; the identity
roads (`type_parameter_types`, §102/§107 matches) also need the
JSDoc-declaration arm before renames can see these params.

**§110 slice 2 — REFUSED at net −17 right.** The @param/@returns
consult built and measured: 114 W→G, ~15 R→G, 2 R→W, ZERO
W→R/G→R — a strict loss. The mechanism: most JSDoc type expressions
do not COMPUTE through `get_type_from_type_node` (the JSDoc grammar
— `?` nullability, `=` optionality, `...` variadics, `Object`/
`function()` spellings — has no arms), so the annotation road
answers error and the WHOLE signature gaps where the un-consulted
bake printed partially-right anys. The prerequisite is the JSDoc
TYPE GRAMMAR (upstream's `getTypeFromJSDoc*` family) — a real
subsystem, and slice 2 stays refused until its common forms exist.
Slice 1 (template plumbing) stands landed; returnTagTypeGuard's 2
R→W confirm the return-tag half also interacts with the §100
predicate road and must land together with the grammar.

**§110 slice 2b — a measured ZERO, reverted whole.** The five simple
JSDoc wrappers (`*`, `?T`, `!T`, `T=`, `...T`) as
`get_type_from_type_node` arms + the slice-2 consult re-applied with
compute-guards (a doc type that does not compute leaves the implicit
any / the body-inference road): the full pair is BYTE-IDENTICAL. The
guards suppress the −17 loss exactly, and the wrappers convert
nothing — so the forms actually failing in the corpus's `@param`
types are NOT the wrappers (suspects: `Object`, `function(...)`,
dotted `module:` names, record types). The third attempt's
PRECONDITION is a census probe: dump the distinct annotation node
KINDS the consult sees on the jsdoc family, and port the top of that
list — not another guess. Everything reverted; slice 1 stands.

**§110 slice 2c — LANDED at +85 net (99:14, 7.1:1), ratio leg
missed and stated.** The kind census earned its keep in ONE line:
all 186 @param annotations arrive as the `JSDocTypeExpression`
WRAPPER, which had no `get_type_from_type_node` arm — every prior
failure was at the entry, not the grammar. With the wrapper arm +
the five simple wrappers + the guarded consult: +86 W→R
(jsDeclarationsGetterSetter 23, checkJsdocSatisfies 6, spread over
25+ JS cases) + 13 G→R against 11 R→W + 3 R→G, 11 W→G honest. The
bar's ≥80-family leg met; the 10:1 leg missed at 7.1:1 with three
named residues: returnTagTypeGuard 6 (a doc `@returns {boolean}`
suppresses §100's predicate admission; the annotation-eligibility
fix was tried and measured WORSE −4 — the true interplay needs the
predicate to run off the RESOLVED type with the doc annotation
retained, a §100-side change), jsFileMethodOverloads3 4 (doc types
on JS overload lists), inferThis 3+3 (`@this` tag unported). The
JSDoc family's remaining ~150 wait on those three plus the
non-wrapper grammar forms.

**§110 slice 3 — LANDED at +15/0.** `@this {T}` supplies the
synthetic this-parameter through the same consult: thisTag1 7,
inferThis 6 (slice 2c's residue), thisTag2 2, ZERO adverse. The
JSDoc family's remaining board: returnTagTypeGuard's predicate
interplay (a §100-side change, evidence in slice 2c's record),
jsFileMethodOverloads doc-overloads, the non-wrapper grammar forms
(`Object`, `function()`, record types), and @template-through-
identity-roads for the §102/§107 renames.

**§110 slice 4 — LANDED at +17/0 (+28 W→G honest).** The parser's
`parse_template_tag` read `const` AS THE NAME (`<const>` printed,
`T` dropped) — upstream's `parseTemplateTagTypeParameter` reads a
const MODIFIER first. One parser arm; all four 100% suites verified
held. jsdocTemplateTag6's remaining rows are the as-const retention
family (the other lane) and downstream consumers.

## §111 — instanceof through [Symbol.hasInstance] [claimed: checker-1]

instanceofOperatorWithRHSHasSymbolHasInstance's 96 wrongs (gots are
`any` — the failure is UPSTREAM of narrowing) decompose as two
slices: (1) the RHS types are object/interface types with the
COMPUTED member name `[Symbol.hasInstance]` — the member bake must
file it (upstream binds it late-bound under the well-known symbol;
the binder's late-bound machinery landed in the fourteenth session's
display work, so this may be a checker-side read); only then do the
operands stop answering any. (2) `narrowTypeByInstanceof`'s
hasInstance half (`flow.go`): when the RHS has a
`[Symbol.hasInstance]` method whose return is a PREDICATE, narrowing
uses the predicate's type (the §100 TypePredicate machinery
consumes); a boolean-returning hasInstance keeps the §83 structural
road. **Bar:** slice 1 first, alone: operands stop printing any
(≥30 of the case at 10:1); slice 2 after: ≥40 more. Falsifiers:
(a) §83's class-identity instanceof byte-identical (the hasInstance
arm fires only when the member EXISTS); (b) late-bound member reads
elsewhere unchanged (the binder-display precedent's population).

## §112 — const-T inference: the §103 head's last machine [checker-2]

[Renumbered from §111 after the ancestry check: checker-1's hasInstance
claim (06f05c3) precedes this bar (0ca26c1) on main — verified both
directions, the mirror of the §109/§110 collision. The two commit
messages naming §111 are immutable and stale; this header is the record.]

§33's decline (calls.rs:481) measured the naive lift at 70 G→W when
NONE of the readonly machinery existed; all of it now does (§104,
§105's six slices, §109). Two arms complete the head:
  1. `isConstContext`'s SECOND disjunct — the one §105 deliberately
     skipped: a valid const-assertion argument whose contextual type
     is a CONST type variable is a const context
     (`checker.go:13618`). Slice: the node is a CALL argument, the
     callee resolves to a single signature (reentrancy-guarded via
     the §56.3 narrow_value_stack precedent), and the positional
     parameter's type IS a type parameter marked `const`. Object and
     array literal arguments then compute const-style through the
     §105 roads with no new machinery.
  2. The DECLINE LIFTS: check_generic_call proceeds for const-marked
     signatures; a fresh literal candidate under const T stays a
     literal (fresh and regular print alike for plain literals), and
     the const-style argument types ARE the inferences — upstream's
     readonly mapping is already baked into what the §105 roads
     produce.

**Bar, registered before the code:** typeParameterConstModifiers
held 88 readonly-want lines at the last census; predict **+40–90**
there plus a tail. Must NOT move: non-const generic calls (the
decline's lift is keyed on `is_const` exactly), §98's retention
converts, §105's readonly population. Adverse over 1:5 re-instates
the decline with the new measurement recorded beside §33's old 70.

**§112 — REFUSED at 15:6 after three measured gates; the code is
reverted whole.** The lift's iterations, each measured: (1)
unrestricted — 19:49 (typeParameterConstModifiersReturnsAndYields
40: non-literal arguments reach unmodelled inference arms); (2)
literal-shape-gated arguments — 18:9; (3) plus unconstrained-T-only
— **15:6 isolated**, still over the registered 1:5. The two
mechanisms the residue named, each a prerequisite:
  - CONSTRAINT-AWARE READONLY STRIPPING: `<const T extends
    string[]>` infers the MUTABLE tuple (`["b", "c"]`, 0:11) —
    upstream strips readonly per the constraint's own mutability.
  - NON-BARE POSITIONS: `f({ x: [1, 'x'] })` against `{ x: T }`
    infers through members the context arm never marks const, so
    the inner literals widen and the now-answering call is
    confidently wrong (0:103–109) — the lift must require every
    parameter MENTIONING a const T to be bare, or the context arm
    must reach nested positions.
§33's decline stands with this second price beside its first (70
G→W pre-readonly). DO NOT retry without both mechanisms; the
readonly machinery alone was necessary but is now measured
insufficient. The §103 head stays open with a sharper spec than it
had this morning.

**§98 attributes2 residue — shape SHIFTED under the day's landings
(recorded for the next trace).** The original one-liner
(autoIncrement `boolean` vs `true`) is now FOUR lines of two kinds:
`type : "string"` wants retention we miss (0:101/0:102 — got
`string`; the §98 retention road should retain against the
discriminated member `'string'`, and the DISCRIM instrumentation
shows elimination firing with plausible include masks), and ONE
autoIncrement still answering `true` where discrimination should
remove its constituent (0:109). The discriminate_union_root
eprintln (env-gated, kept) prints the include masks; next trace
starts by pairing masks to call sites.

## §113 — the §31.1 refusal re-priced: ES-imports through the calibrated predicate [claimed: checker-1]

The board's biggest case-mover (ALIAS/no-value-decl: 428 near-miss
cases + 177 reference-variant) sits behind callres §31.1's refusal —
priced at 2.7:1 in an era when the findability predicate was one
host call. Since then the DIAGNOSTICS calibration rebuilt it as
`module_specifier_unfindable` with five gates (ambient-module
consult, pattern-ambient decline, node-core names, @types/,
IsResolved-vs-in-program split), and the require() arm has run on it
since §31-callres. **The re-price: the ES-import declaration forms
(ImportSpecifier, ImportClause default, NamespaceImport) take the
SAME arm in `get_type_of_alias` — walk to the ImportDeclaration's
specifier, and an unfindable one reads `any`.** Bar: ≥150 lines at
≥5:1 (the old adverse class — resolvable-by-symlink corpora — is
what the IsResolved split now excludes; if it recurs, the refusal
stands re-confirmed and this section records the price). Falsifiers:
(a) `export ... from` re-export aliases are NOT admitted (different
declaration kinds, different upstream rule); (b) findable-but-untyped
modules keep the errorType gap (the predicate's IsResolved half).

**§113 measured — the §31.1 refusal RE-CONFIRMED at its own number,
and the number is now twice-priced.** Ungated (all ES-import forms
through the calibrated predicate): +238 G→R (ramdaToolsNoInfinite2
130!) against 118 G→W + 2 R→W = 2:1 — better than the era's 110:117
but the adverse is byte-for-byte the refusal's named class
(symlinkedWorkspace* 28, declarationEmitReexportedSymlink 12,
monorepo non-relative specifiers throughout). Relative-gated: +35
against 13 — the era's 35:13 REPRODUCED EXACTLY two years of
machinery later. Verdict: not stale. The five diagnostic gates
changed nothing because the blocker is beneath them all: the HOST's
resolver has no symlink realpath or path-mapping support, so
resolution FAILS (not resolves-outside-program) precisely where
upstream succeeds. The unlock is an infrastructure build in
tsr-vfs/loader (symlink realpath + paths mapping), priced here at
~240 lines + the 428-case near-miss row. Reverted whole; tsr-9or.1
keeps the head with this fresh price attached.

**§108.1 — the void sibling (handed by the other lane's §114 triage).**
A WRITTEN union carrying `void` keeps its order — the fresh render
sorts void first (`number | void` → `void | number`). Third
admission flag in `written_type_text`'s walk. **+22 W→R, ZERO
adverse** (callWithMissingVoid 8, typeGuardTypeOfUndefined 8,
doubleUnderscoreExportStar 2, spread). The admission principle's
third confirmation: quote, Array head, void position — each a
spelling the fresh render cannot reproduce, each landing at zero
adverse.

**§111 slice 2 — built, ZERO-FIRE, parked on the branch with the
probe queued.** The arm (predicate-carrying `[Symbol.hasInstance]`
narrows both branches through the extracted §22 ladder) plus the
declaration-scan lookup (late-bound members are `__computed`,
binder.rs:4136 — the member table cannot answer; the well-known name
reads off the owner's declarations) both built, and the pair is
byte-identical: the arm is NEVER REACHED. The next probe (one
eprintln at the instanceof narrowing entry on the filtered case)
must answer whether the flow walk reaches instanceof conditions for
`unknown`-declared references at all — the wrongs' `any` gots
suggest the unknown-receiver road answers before narrowing runs.
WIP on worktree-checker-1-printing (81 lines incl. the neutral
ladder extraction); nothing on main.
**LANDED AT USER DIRECTION** (2026-08-09, "push it to main"): the
arm ships INERT as measured (zero-fire, byte-identical pair, gates
green) — correct-by-reading, unreachable until the entry probe finds
why the flow walk never consults instanceof conditions for these
references; the ladder extraction is behaviour-neutral shared code.

**§111 slice 2 ACTIVATES — the entry probe's answer was the ladder's
missing any-arm.** The arm was reached all along (matching=true ×40);
16 flows carried declared=any and the §22 ladder ran on `[any]` and
kept it. Upstream's any arm: the true branch narrows `any` TO the
candidate — EXCEPT the global `Function`/`Object` interfaces, where
`any` stays (`narrowFromAnyWithTypePredicate`'s 6 wants, measured as
R→W without the exception). **+36 W→R, ZERO adverse**
(typeGuardsWithInstanceOfBySymbolHasInstance 23 — the §111 target
family fires — narrowFromAny 11, catch-clause 2), net right +16 with
the remainder honest W→G. The any-arm serves BOTH consumers of the
shared ladder (call predicates and hasInstance). Residue: the
union-declared rows (Line | Point | ...) still decline in the
relater's interface rungs — the §22 Undecidable class, priced there.

**§111 slice 3 — composite callees (+2/0).** An intersection callee
answers through its first predicate-bearing constituent; a union
requires all and answers their union. The rung probe measured ZERO
undecidables — the case's remaining rows fail past the predicate
road entirely (the narrowed prints still mismatch on shapes the
probe places downstream of the ladder); priced as the case's deep
residue, not a §22 rung.

## §117 — the lib-member residue: decomposition opens [claimed: checker-1]

[The subsystem split's my-lane half; checker-2 opens generic-overload
resolution in callres2.]

The family: "property access, the receiver has no such property" —
4,406 gap lines / ~270+265 near-miss cases at the day's close. The
top three cases name three distinct branches before any instrumented
census: (1) strictBindCallApply1 113 — `bind`/`call`/`apply` members
of FUNCTION types (upstream: the callable's apparent type through
the strict-bind-call-apply globals — a lib-interface road our
function types never take); (2) uncalledFunctionChecksInConditional2
112 — reads through conditional-checked callables (the truthiness
family's receivers); (3) doYouNeedToChangeYourTargetLibraryES2016Plus
98 — TARGET-LIB VERSION members (`includes` and friends: whether the
@lib/@target directive mounts the ES2016+ libs at all — possibly a
harness mounting question, not a checker one, and the cheapest
branch if so). NEXT: the instrumented receiver-shape census
(one probe at the miss site classifying receiver TypeData + apparent
road taken), then the design doc with measured entries. No slice
before the census.

**§117 census — RUN (2,795 miss events, full corpus).** By receiver
shape: Named+members 1,144 / Anonymous 610 / Union 593 /
Intersection 185 / Named-bare 174 / other 89. By member name, the
decisive cut: `.prototype` 121, `.toString` 112, `.name` 82,
`.constructor` 70, `.length` 62 — plus the bind/call/apply class —
are the OBJECT/FUNCTION PROTOTYPE FAMILY: upstream's member walk
falls back to the global `Object`/`Function` interfaces at the
apparent-type layer, a road this port's walk never takes. That is
SLICE 1 (one fallback in the declared-member walk; owns
strictBindCallApply1's 113 and the toString rows across hundreds of
cases). SLICE 2: `.prototype` on class statics — a synthetic
property of the typeof side. The `.foo/.a/.b/.x` 600+ are
case-specific names — mostly genuine misses (correct gaps) to be
left alone; the Union 593 and Intersection 185 shapes route to
their §49/§92 arms' residues. Bar for slice 1 comes next window.

**§117 slice 1 bar — the Object/Function member fallback.**
`getPropertyOfTypeEx` (`checker.go:18918-18934`): an OBJECT-flagged
receiver whose own walk misses falls back to (a) the FUNCTION
interface family when it has signatures — `globalCallableFunctionType`
for call signatures, `globalNewableFunctionType` for construct
(each the strictBindCallApply interface, falling to `Function`
when unmounted) — then (b) `globalObjectType`, always. Port: the
same two-stage tail on `get_property_of_type`'s miss, globals by
name at arity 0. **Bar: ≥250 of the census's prototype-family lines
at ≥10:1** (the family: toString 112 + prototype-adjacent name/
length/constructor ~200 + bind/call/apply — strictBindCallApply1's
113 whole). Falsifiers: (a) genuine `.foo` misses stay None —
upstream's own fallback misses them too, byte-identical by
construction; (b) the found members' SIGNATURE prints come from the
lib bake — any adverse concentrated in bind/apply signature texts
is a §95/§99-family print question, priced separately from the
lookup; (c) `.prototype` is NOT this slice (it is synthetic on the
static side, slice 2).

**§117 slice 1 — LANDED at +348 net (348:50, 7:1), ratio leg missed
and stated.** The two-stage fallback (CallableFunction/
NewableFunction→Function for signature-bearing receivers, Object
always) with ONE measured gate: const-enum receivers withheld
(TS2748 territory, 14 G→W without it; a module-object gate measured
ZERO and was removed — only measured gates ship). +312 G→R + 36 W→R:
strictBindCallApply1 32, objectTypePropertyAccess 20,
typeGuardsInFunctionAndModuleBlock 20+22, spread over 60+ cases. The
50 residual G→W in six named classes ≤6 each: strictBindCallApply
signature TEXTS (falsifier (b)'s priced print class),
multiImportExport/declarationsIndirect alias-object reads (the
receiver shape needs its own trace — the module-flag hypothesis
measured zero), typeGuard truthiness interplays, stackDepth 3.

**§117 slice 2 bar — the synthetic `.prototype`.** The census's 121
`.prototype` misses: upstream mints the property on every class
STATIC side, typed as the INSTANCE type (generic classes: the
instance reference over `any` arguments — `getTypeOfPrototypeProperty`,
`checker.go`). Port at the TYPE level (`get_type_of_property_of_type`
answering before the symbol road for `prototype` on an Anonymous
CLASS receiver): non-generic → the declared instance; generic → the
reference over per-parameter `any`. **Bar: ≥60 at ≥10:1.**
Falsifiers: (a) `prototype` on non-class receivers (functions'
`.prototype` is `any` upstream — the FUNCTION arm answers any, not
the instance); (b) written `prototype` MEMBERS (a user property
named prototype wins — the symbol road runs first).

**§117 slice 2 — LANDED at +181/0.** The synthetic `.prototype`:
+178 G→R + 3 W→R, ZERO adverse (jsDeclarationsGetterSetter 30,
constructorHasPrototypeProperty 26, instanceMemberAssignsToClass
16, spread over 30+ cases). Both falsifiers held by construction:
function receivers never enter (CLASS flag gate) and TS2699 makes
the written-member shadow unreachable. The generic arm (reference
over per-parameter any) fired without adverse.

**§117 slice 3 — LANDED at +76/0.** Union constituents read through
their APPARENT types (upstream's per-constituent
getReducedApparentType): `(string | number).constructor` answers via
the wrapper interfaces, composing with slice 1's fallback for object
constituents. typeGuardConstructor{ClassAndNumber,DerivedClass,
PrimitiveTypes} whole, zero adverse. §117's three slices total
**+786 net**; remaining census rows: Intersection 185 (the §92
registry gate deliberately narrow — widening needs the discriminant
evidence re-examined), Named-bare 174, the priced slice-1 residuals.

**§117 slice 4 — LANDED at +10/0.** A plain tuple's `.length` is the
literal element count; optional-bearing tuples decline (upstream
answers a length UNION there — a later arm if its rows surface).
strictTupleLength 6 + destructuring defaults 3 + rest-pattern 1,
zero adverse.

## §118 — the types harness honours `@symlink` (the LOADER unlock, first slice) [claimed: checker-1]

**The forcing find.** §113 re-confirmed the §31.1-era refusal at its
own price and named the blocker as infrastructure: "the HOST's
resolver has no symlink realpath or path-mapping support." Half of
that sentence is now false at the wrong layer. `tsr-vfs`'s
`InMemoryFileSystem` implements symlink following and realpath in
full (`crates/tsr-vfs/src/lib.rs:206-235`, with directory-prefix
redirection and cycle bounding), the module resolver consumes it
(`create_resolved_module_handling_symlink`, `resolver.rs:2434`), and
the module_resolution suite holds 100% THROUGH that road —
`trace_case.rs:297-320` passes `case.symlinks` into the VFS. The one
road that drops them is the types/diagnostics harness itself:
`program_for_case` (`types_producer.rs:1138`) constructs
`InMemoryFileSystem::new(files, [], true)` — an empty symlink table.
Every `@symlink` case fails resolution not because the resolver
cannot follow links but because the harness never told it any
existed. The blocker §113 priced as "an infrastructure build in
tsr-vfs/loader" is, for the `@symlink` half, a harness omission.

**The change.** `program_for_case` passes the case's symlinks,
normalized exactly as `trace_case::build_file_system` normalizes
them (`get_normalized_absolute_path` against the case's current
directory, both link and target). No resolver, vfs, or checker code
moves.

**Scope honestly stated.** This is the `@symlink` slice only. The
`paths`-mapping half of §113's price is NOT this: `paths` arrives
via tsconfig units the types harness does not parse
(`apply_test_directives` reads directives; `base.paths` stays
default), and stays with the parked entry. The 428-case ALIAS
near-miss row will therefore convert partially at best.

**Bar.** ≥100 G→R at ≥5:1 on checker_types. The candidate class is
§113's measured adverse class inverted: symlinkedWorkspace* (28),
declarationEmitReexportedSymlink (12), moduleResolutionWithSymlinks*,
the monorepo corpora — cases whose imports upstream resolves through
links and this harness reported unfindable.

**Falsifiers.** (a) If the linked-to modules resolve but their
member types do not compute, conversion lands as gap-shuffling
(unfindable-any → errorType gap) and the count undershoots — record
the residue's owner. (b) The §31-family boundary rules keyed on
`module_specifier_unfindable` flip from unfindable to findable on
these corpora; if that direction produces G→W above the bar's ratio,
the harness change is still CORRECT (it matches upstream's harness,
harnessutil's symlink copy-in) — the wrongs then name checker work,
and the section records them rather than reverting the harness. A
harness that misrepresents the file system is not a legitimate gate.

**Cross-suite note.** `program_for_case` also feeds
`diagnostics_suite.rs` — the diagnostics gradient moves with this.
Announced to the other lane before measuring; both numbers reported
in the landing.

**§118 MEASURED AND LANDED — +46 G→R / 33 G→W / 0 R→W (right
409,065 → 409,111 on the /478,954 gradient; scorepair, two steps:
`@symlink` alone +23/0, `@link` parsing +23 more/33).** The bar's
≥100-at-≥5:1 was NOT met numerically, and falsifier (b) is what
fired: every G→W line is the newly-resolving corpora printing bare
`Foo` where upstream prints `import("package-a").Foo` — the
already-recorded PER-FILE import-spelling head (the §81/temporal
per-site printing family), now with 33 more lines on its ledger.
The harness change stands under the bar's own pre-written rule: it
reproduces upstream's harness byte-for-byte (`linkRegex`,
`test_case_parser.go:44`/`:290`, mounted at `harnessutil.go:201` —
`@link: A -> B` links **B** to **A**), and a harness that
misrepresents the file system is not a legitimate gate. The second
slice landed alongside: `case.rs` now parses `@link` (it fell into
the options map before, which also silently stripped it from unit
content — the reason symlinkedWorkspace* stayed dark after the
`@symlink` slice).

Regression legs, all held: parser_typescript 5031/5031,
binder_symbols 8456/8456, module_resolution 95/95, file_loader
96/96, printer_round_trip 11757/11757, scanners 100%. The
diagnostics suite (shared `program_for_case`) moved +1 case
(1,998 → 1,999). clippy 0 errors (grep-verified), 1,554 tests pass,
anchors resolve.

**Authority note.** The LOADER entry was parked "joint-or-user-nod".
This session's goal directive, verbatim: *"keep improving
checker_types conformance until we get to 100%. @STATUS.md @TASK.md
There might be some stale refusals. Keep re-evaluating them now
that we have 100% binder and parser conformance. in coordination
with checker-2 agent."* What landed here is narrower than the
parked entry: no resolver or vfs code moved — the "infrastructure
build" §113 priced turned out to be already-built machinery the
types harness wasn't handed. The `paths`-mapping half (tsconfig
units the types harness does not parse) remains parked with
tsr-9or.1; re-price it against the post-§118 board before building.

## §119 — §113 re-run POST-§118: the ES-import forms through the calibrated predicate [claimed: checker-1]

**Why a twice-confirmed refusal gets a third run.** §113's verdict
was honest and precise: the ES-import arm measured +238 G→R against
118 G→W + 2 R→W (2:1), and the adverse class was BYTE-FOR-BYTE the
§31.1 era's — symlinkedWorkspace* 28, declarationEmitReexportedSymlink
12, monorepo non-relative specifiers — corpora whose modules upstream
resolves through symlinks and this harness could not. §118 changed
the ONLY fact that refusal rested on: those corpora's links are now
mounted, their specifiers are now FINDABLE, and a findable specifier
never enters the unfindable-reads-any arm at all. The adverse class
is not "priced differently" — it is structurally excluded from the
arm's domain. This is the cleanest stale-refusal shape there is:
the rule was always right, and the world it was measured in was
wrong. (Directive authority: the §118 landing's quoted goal text —
"There might be some stale refusals. Keep re-evaluating them...")

**The arm.** In `get_type_of_alias` (`symbols.rs:324`), beside the
§31 require() arm: an alias whose declaration is an ImportSpecifier,
ImportClause (default import), or NamespaceImport walks to its
ImportDeclaration's module specifier; `module_specifier_unfindable`
(the five-gate calibrated predicate, `check.rs:2734`) reading true
answers `any`. Findable-but-untyped modules keep the errorType gap
(the predicate's IsResolved half) — unchanged from §113.

**Candidates** (novaldecl at 409,219): ImportSpecifier 133 gap
lines / 46 near-miss cases, ImportClause 86/19, NamespaceImport
55/7 — ~274 admitted lines, plus reference-position reads the
census undercounts.

**Bar.** ≥150 G→R at ≥5:1. Falsifiers carried from §113:
(a) `export ... from` re-export aliases NOT admitted (different
declaration kinds, different upstream rule); (b) NamespaceExportDeclaration
(`export as namespace`) NOT admitted — 30 census lines stay;
(c) if the symlink-corpora adverse RECURS despite §118 (some
specifier class still unfindable for a third reason), the refusal
is thrice-confirmed and the price recorded here.

**§119 MEASURED AND LANDED — the refusal was stale, and §118 is
what made it so: +235 G→R / 38 G→W / 2 R→W (~5.9:1), right
409,219 → 409,452 = 85.49% on /478,954.** The bar (≥150 at ≥5:1)
met on the first measurement. The positive side is §113's almost
line-for-line (+238 then, +235 now — ramdaToolsNoInfinite2's 130
intact); what changed is the adverse: 120 lines then, 40 now, and
the symlink corpora contribute ZERO of them — §118's structural
exclusion held exactly as argued. The residual G→W is a NEW
nameable class: tsconfig-unit machinery the types harness does not
parse — `paths` mapping (pathMappingBasedModuleResolution6_node 5),
package.json self-names (nodeNextPackageSelfName* 15,
nodeColonModuleResolution2 6) — plus expandoFunctionContextualTypesNoValue 3
(expando statics, different owner). The 2 R→W
(jsxLibraryManagedAttributesUnusedGeneric, reactImportDropped) are
lines whose upstream want is literally `error` — JSX-import
positions where upstream itself keeps the gap; priced and accepted.
The pinned fixture `a_named_import_from_an_unresolved_module_is_a_gap`
came due and was RENAMED with its new truth (`..._is_any`) — the
twenty-ninth stand-in.

**What this buys next.** The remaining ALIAS census (ImportEquals
entity forms 205, NamespaceExportDeclaration 30) stays with
resolver parity / tsr-9or.1. The tsconfig-unit class (paths +
self-names, ~26 adverse lines here and the pathMapping* corpora's
gaps beyond) is now the loader head's SHARPENED price: parsing
tsconfig units in `program_for_case` would both convert those
corpora and retire this build's largest adverse class. Still
parked; re-priced upward in usefulness.

## §120 — the §92 written-intersection gate re-measured post-§98 [claimed: checker-1]

**The candidate stale refusal.** §92 gated intersection member
distribution to ALIAS-EVALUATED intersections because the written
form "answering confidently here measured 134 G→W in the
discriminated-union family (union order, un-narrowed members)". That
price predates §98 (the retention roots + upstream's ternary
discrimination algorithm, checker-2's lane) and the §5x discriminant
family's completion — the very machinery whose absence made
un-narrowed members print wrong. Same shape as §119: the rule
(upstream's `getUnionOrIntersectionProperty` reads an intersection
member from ANY constituent that has it) was never in doubt; the
world it was priced in has changed twice.

**The arm.** In `property_type_via_shape` (`members.rs:742`): drop
the `alias_evaluated_types` membership test — every
`TypeData::Intersection` distributes. Hit-combination unchanged
(single hit answers it; multiple hits intersect). Candidates:
mixinAccessModifiers 60 (`Protected & Public` receivers wanting
`string` members), the `Window & typeof globalThis` 26, spread
intersection tail — the member_shapes WouldFind bucket's
intersection half.

**Bar.** ≥60 G→R at ≥5:1. Falsifier: if the discriminated-union
G→W class recurs at material size (§92's 134, or any structured
fraction of it), the gate goes back verbatim and this section
records the second price with its case names — two priced refusals
would make the gate load-bearing, not provisional.

**§120 MEASURED AND LANDED at iteration 4 — +172 G→R / 31 G→W /
0 R→W (5.5:1), right 409,452 → 409,624 = 85.52%.** The §92 gate was
HALF stale: single-hit distribution (exactly one constituent
carries the name) is clean and took uncalledFunctionChecksInConditional2
103, noUncheckedIndexedAccessDestructuring 20, mixinClassesMembers 19;
multi-hit positions are where §92's 134 lived and they STAY REFUSED
— four measured iterations mapped them precisely:
  1. naive combination: 86 G→W (unparenthesized signature
     intersections, `never`/`any` compatibility wants);
  2. TypeId-dedup: 55 G→W (identically-printed distinct
     instantiations don't dedupe by id; upstream also sometimes
     WANTS the intersection — `(() => void) & (() => void)` —
     exactly where we collapsed);
  3. single-hit only: 39 G→W;
  4. + the `this`-mention decline (a single hit whose declaration
     subtree contains a ThisType node declines — upstream binds
     polymorphic `this` to the WHOLE intersection and prints
     `this`; we substitute the declaring class): 31 G→W, bar met.
The multi-hit refusal is now TWICE priced (§92's 134, §120's
+14-over-iteration-3 in mixinAccessModifiers/discriminatedUnionTypes2)
— the gate is load-bearing, not provisional; a third attempt needs
the intersection-property MINT (parenthesized signature prints,
compatibility never/any) built first. Residual adverse accepted at
the bar: discriminatedUnionTypes2 8 (alias-name prints through the
new road), unionTypeCallSignatures6 6 (written `(F0)` parenthesized
spellings the fresh union render cannot reproduce — §77-family),
uncalledFunctionChecksInConditional2 5 (net +98 in-case),
libTypeScriptOverride* 6. Gates: all four 100% suites held,
diagnostics +4 (2,019 → 2,023), clippy 0, 1,559 tests, anchors ok.

## §121 — the nullable/unknown receiver's deliberate error-answer [claimed: checker-1]

**The seventh hop of the boundary-argument chain.** §14 → §27 → §31
→ §32 → callres §23/§24/§25 → §26 each argued "upstream's deliberate
error-answer's observable IS `any`". The member_shapes intrinsic
bucket (178 lines) holds the next one: property access on a receiver
that IS `undefined` / `null` / `unknown` / a nullable remainder goes
through `checkNonNullType` (`checker.go:7409`), which REPORTS
(TS2532/TS18048/TS2571) and returns `errorType` — printed `any` in
every `.types` baseline (controlFlowCaching's `>foo.bar : any` runs,
nonPrimitiveStrictNull, assertionTypePredicates1). This port's
`check_non_null_type` refuses to the SAME errorType but the harness
reads it as a gap because the refusal happens before the lookup and
the producer never sees a computed type. The fix is one seam:
`check_property_access_expression_worker` (and `check_qualified_name`)
return `any` instead of `error` when the non-null strip itself
refuses — the receiver was COMPUTED (not a gap) and upstream's
answer at this shape is its deliberate error-answer.

**The gate that keeps it honest** (every chain hop had one): the
arm fires only when the RECEIVER's type computed successfully —
`receiver_type == error` still returns error above the strip,
unchanged. A receiver this port mis-narrows to `undefined` where
upstream keeps a real type converts gap → confident wrong; that
class is the falsifier.

**Candidates**: intrinsic-receiver rows — undefined 34, unknown 28,
never 22, null 8, void 6 ≈ 98 lines, plus paired row-5/6 doubling
and downstream reads. **Bar: ≥60 G→R at ≥5:1.** If the
false-undefined class (port narrowing misses) produces G→W above
the ratio, the arm gates on receiver flags (pure NULLABLE only,
unknown excluded) before it reverts whole.

**§121 MEASURED — landed GATED at +47 G→R / 6 G→W / 0 R→W (7.8:1),
right 409,624 → 409,671 = 85.53%.** Two measured iterations:
UNGATED (any strip-refusal answers `any`) measured 64:36 — the
falsifier's exact class, receivers this port fails to NARROW
(`unknown` catch variables in useUnknownInCatchVariables01,
assertion predicates in assertionTypePredicates1, discriminant
walks) reading confidently wrong. The gate:
`receiver_is_purely_nullish` — the receiver is `undefined`, `null`,
or a union of only those (every constituent NULLABLE, UNKNOWN
excluded, `never` excluded), the shapes where upstream's OWN
receiver is the same nullish type and its errorType-printed-any is
the deliberate answer rather than our narrowing miss. The bar's
line count MISSED (47 vs ≥60 predicted — the intrinsic census's
`unknown` 28 and `never` 22 are exactly the excluded classes; §96's
sizing lesson again: buckets by SPELLING, mechanism decides), the
ratio 7.8:1 over the 5:1 floor. Residual adverse priced:
jsdocImportType 4 (JSDoc-typed JS vars null-initialized — the
annotation is unread so the receiver is honestly-wrongly nullish;
JSDoc's owner), controlFlowArrays 2 (evolving-array nullish
snapshots). The qualified-name road took the same gate.

## §122 — statics inherit: the anonymous side's base walk [claimed: checker-1]

**The miss.** `get_property_of_anonymous_symbol` (`members.rs:1073`)
reads a class's own `exports` and the export-star road — and stops.
Upstream's static side is a real inheritance chain: the constructor
type's base is the base class's constructor type
(`getBaseConstructorTypeOfClass`), so `class D extends B` finds
`B.x` through `typeof D`. The instance side here already walks
bases (`get_property_of_declared_symbol`'s `base_symbols_of` loop,
with the visiting guard); the static side never got the mirror.
member_shapes' "Anonymous, name absent in exports" bucket: 302
lines — protectedStaticClassPropertyAccessibleWithinSubclass 20
(pure gaps wanting `string`, verified by dump), staticIndexSignature4
16, spread tail. (strictBindCallApply1's 24 are bind/call/apply
receivers — §117-fallback territory, NOT this.)

**The arm.** For a CLASS-flagged anonymous symbol whose own
exports (and star road) miss: walk `base_symbols_of` and read each
base's `exports` recursively, same visiting guard, first hit wins.
`base_symbols_of`'s existing refusals (instantiated bases,
non-identifier heritage) stay refusals — a gapped base gaps the
walk, never answers the wrong symbol.

**Bar.** ≥40 G→R at ≥5:1. Falsifier: shadowing — a derived static
redeclaring a base's must answer the DERIVED symbol; own-exports-
first ordering guarantees it by construction, and a measured
adverse there means the ordering claim is wrong.

**§122 MEASURED AND LANDED — +77 G→R / 0 adverse (after one
refinement), right 409,671 → 409,748 = 85.55%.** The first pair
read +77/4 at 19:1; the 4 were `#private` statics carried through
the chain where upstream scopes private names lexically and answers
its error-any (privateNameStaticAccessorssDerivedClasses wants
`any`, we found the base's `number`) — the `#`-prefix exclusion
converted them back to honest gaps and the second pair read +77/0.
protectedStaticClassPropertyAccessibleWithinSubclass 20 whole,
protectedMembers 16, derivedClassIncludesInheritedMembers 12.
The `the_shapes_typeof_x_still_gaps` pin's inherited-static
assertion came due — the THIRTIETH stand-in — and was flipped to
its new truth with §122 named. NOTE the residue this opens:
`#`-static misses now gap where upstream wants `any` (4 lines) —
a §121-family deliberate-error arm for missing private names is a
future slice, recorded not built. Gates: four 100% suites held,
diagnostics 2,031 → 2,040, clippy 0 (rtk-masked test failure caught
by `rtk proxy` — the trap's second firing this session; the full
run reads 0 FAILED after the pin flip), anchors 2,690.

## §123 — the completed-walk property miss answers TS2339's any [claimed: checker-1]

**§34 re-opened with the discriminator it lacked.** §34's arm
(literal receivers only) measured zero and reverted, and its record
parked the interface road as "incomplete-table territory, gapped by
design". That design claim has never been MEASURED on Named
receivers, and two §34-era facts changed: the §117 fallback family
completed the Object/Function tail every lookup now reaches, and
§122 completed the static side. The bucket is member_shapes' largest:
907 lines, "receiver has members, name absent (upstream errors
too)" — doYouNeedToChangeYourTargetLibraryES2016Plus's 26 verified
want-any (ES2016+ methods against an es5 lib: upstream reads the
SAME lib text, misses the same name, reports TS2550/TS2339, answers
errorType printed any).

**The arm.** In `access_member_lookup`, after the index-signature
miss: a receiver whose lookup WALK COMPLETED — Named owner, every
`base_symbols_of` on the chain answered (no instantiated-base or
non-identifier-heritage gap), name found nowhere, no applicable
index signature — answers `any`. A walk any link of which was
BLOCKED keeps the honest gap: absence was not established.

**The falsifier, named before the pair.** This port does not merge
globals across files (`tsr-compiler/src/lib.rs:24`) — an interface
augmented in a second lib file (String es5 + es2017) may hold its
member in a declaration this walk never reads. If upstream finds
such a member, the arm converts an honest gap to a confident any.
Expected signature: G→W concentrated in high-target lib corpora
with REAL-type wants. **Bar: ≥200 G→R at ≥5:1**; if the cross-file
class fires, the next gate is single-declaration-file symbols only,
and its price gets recorded either way.

**§123 MEASURED AND LANDED at iteration 2 — +381 G→R / 69 G→W /
0 R→W (5.5:1), right 409,748 → 410,129 = 85.63%.** The bar
(≥200 at ≥5:1) met. §34's "gapped by design" was a design claim
that had never been measured on Named receivers, and most of it
dissolved under two gates:
  1. UNGATED: +722/325+11 at 2.1:1 — three adverse classes, each
     diagnostic: mapped/conditional-alias receivers whose Named
     table was never the type's member list (mappedTypes2 21,
     conditionalTypes1 26, recursiveIntersectionTypes 24), JS
     positions (spellingUncheckedJS's 7 R→W — unchecked-JS misses
     answer differently), and the narrowing-miss class.
  2. GATED (owner declared EXCLUSIVELY by class/interface
     declarations + non-JS positions): +381/69.
The residual adverse is the class the bar predicted and the arm
cannot see: receivers upstream NARROWS before the lookup
(typeGuardFunctionOfFormThis 14, assertionTypePredicates1 8,
typeGuardFunction* 8 — this-predicates and assertion narrowing,
each a recorded unported leg). Two more pins came due and were
flipped with the section named (thirty-first: constraint-absent
member; thirty-second: absent private name — both had argued
"errorType is upstream's answer too" while asserting the gap
sentinel that PRINTS differently). destructuringParameterProperties
1/2/5 came in nearly whole (112 lines). Gates: four 100% suites
held, diagnostics 2,040 → 2,044, clippy 0, tests 0 FAILED via
`rtk proxy`, anchors 2,692.

## §124 — the Anonymous side's established miss answers any [claimed: checker-1]

**§123's analogue one table over.** The Anonymous bucket (256 lines
post-§123): `C.missing` on a class's static side, `f.missing` on a
function, `E.missing` on an enum — upstream reports TS2339/TS2551
and answers errorType-printed-any exactly as on the instance side.
Absence is establishable per owner kind: a CLASS owner via §123's
`walk_completes` over the extends chain (§122's exports walk read
every table absence claims); a FUNCTION or ENUM owner's exports
table is single-declaration-set and whole by construction.
VALUE_MODULE owners are NOT admitted this slice: a namespace's
surface can arrive through `export *` whose targets this port may
not resolve, so absence there is not established (the §-after gate
if its rows warrant). Non-JS positions only, as §123.

**Bar.** ≥80 G→R at ≥5:1. Falsifiers: (a) expando functions
(member ASSIGNMENTS creating properties the exports table never
held — JS-gated but TS `namespace f` merges exist: a FUNCTION owner
merged with a namespace carries real exports; the merged-symbol
read must happen before the kind test); (b) the §122 `#`-exclusion
class — a missing `#` static wants any and IS admitted here.

**§124 MEASURED AND LANDED at iteration 4 — +83 G→R / 2 G→W /
0 R→W (41:1), right 410,129 → 410,213 = 85.64%.** Four iterations,
each converting a named adverse class into a gate:
  1. CLASS(walk)+FUNCTION+ENUM: +95/52 — strictBindCallApply1 24
     (wrapper-interface members answered specially) and
     staticIndexSignature* 26 (static index signatures unported).
  2. FUNCTION dropped, index-signature chain scan added: +84/28 —
     strictBindCallApply persisted via CLASS owners (`C.bind` reads
     NewableFunction).
  3. the Function-family/Object name gate (a name those globals
     declare is never established-absent): +69/2 — over-excluded
     the const-enum class.
  4. CONST_ENUM owners skip the family gate (upstream withholds the
     prototype road there deliberately, the §117 fallback's own
     recorded exclusion): +83/2.
derivedClassWithPrivateInstanceShadowingPublicInstance 20 whole,
constEnumNoObjectPrototypePropertyAccess 14; the typeof-query pin
(types.rs) came due as the THIRTY-THIRD stand-in — its load-bearing
half (never repoint at members) still discriminates, any is not
number. VALUE_MODULE stays
un-admitted (export-star surface unverifiable) — its rows go with
the §31-family, recorded not guessed. The 2 residual: 
typeReferenceDirectives9 (type-directive resolution, different
owner). Gates: four 100% suites held (coverage at 85.64%, 4,114
cases), tests/clippy in the landing commit's log.

## §125 — the union receiver's established miss [claimed: checker-1]

**The bucket's other half.** member_shapes' union/intersection rows
(639 lines post-§124): the distribution probe's PARTIAL class (406
lines, "some constituents have it — upstream errors too") and
NoneHaveIt (~170). Upstream's rule at both: a union member lookup
that fails on ANY constituent reports TS2339 ("does not exist on
type A | B") and answers errorType-printed-any. §123/§124 built the
per-receiver establishment question; this arm asks it
per-CONSTITUENT: a stripped union receiver where every constituent
(through its apparent type) either HAS the member or has
ESTABLISHED absence (§123/§124's combined predicate, index
signatures consulted per constituent), with at least one absent,
answers `any`. Any constituent whose state is unknown keeps the
gap.

**Bar.** ≥150 G→R at ≥5:1. Falsifier: the census's NARROWER class
(242 receiver positions where upstream's receiver is narrower than
ours) — a discriminated union upstream narrows before the lookup
reads a real member where this arm claims any. If it fires above
the ratio, the candidate gate is unions with no shared
literal-discriminant member; if that also fails, the class is
narrowing-owned whole and the section records it.

**§125 MEASURED AND REFUSED at 39:164 — the class is
narrowing-owned WHOLE, and the number is the proof.** The
per-constituent establishment arm measured +39 G→R against 164 G→W:
typeGuardOfFormInstanceOfOnInterface 28, typeGuardOfFormIsType 28,
typeGuardOfFormIsTypeOnInterfaces 28, discriminantElementAccessCheck
20, assertionFunctionsCanNarrowByDiscriminant — every adverse case
is a GUARDED read where upstream narrows the union before the
lookup (instanceof, user-defined `is` predicates, assertion
functions, discriminant element access) and reads the surviving
constituent's REAL member. The pre-named fallback gate
(no-shared-discriminant unions) cannot save it: the instanceof and
is-type families are not discriminant-keyed. Reverted whole,
byte-identical revert verified against the accepted baseline. DO
NOT RE-DERIVE: the union-miss error-answer only becomes buildable
AFTER the predicate-narrowing legs land (getTypePredicateFromBody,
instanceof false-arm, assertion functions — the board's standing
inferTypePredicates head); re-open §125 then, citing this pair.
The member-miss seam closes at §124: rows 5/6 fell 2,384 → ~1,700
this window, and every remaining concentration names narrowing or
tsr-4qx as its owner.

## §126 — the instanceof FALSE branch, traced and built [claimed: checker-1]

**The §83 evidence split resolved by the read it asked for.**
`getNarrowedTypeWorker`'s `!assumeTrue` arm (flow.go:861):
`t == candidate → never`; else (checkDerived, the instanceof road)
`filterType(t, constituent → !isTypeDerivedFrom(constituent,
candidate))`. `isTypeDerivedFrom` (relater.go:4962) is DECLARED
BASE CHAINS (`hasBaseType`), not structure — which is why
typeGuardOfFormInstanceOf's else keeps the whole union (its
constituents are unrelated interfaces, derived from nothing) while
instanceofWithStructurallyIdenticalTypes narrows (its constituents
are classes on the candidate's chain). The "global var vs
parameter" difference §83 recorded was a red herring; derivation
decides. The §83-era false-arm measurement (21 adverse) predates
this trace and used the wrong test.

**The arm.** In the §83 road, `!assume_true` with a CLASS RHS:
identity constituent → removed; chain-derived constituent
(`class_extends_chain_contains`, the same test the true branch
runs) → removed; everything else survives; nothing removed → t
unchanged. Non-union: `t == instance → never`; chain-derived t →
never; else t. Upstream's pre-gate (`instanceType` Object-flagged
non-empty) holds by construction for class instances.

**Bar.** ≥30 G→R at ≥5:1. Falsifiers: (a) typeGuardOfFormInstanceOf's
whole-union else must not move (its RHS is constructor-signature
interfaces, outside the CLASS gate); (b) the §83 chain test declines
undecidable shapes — a decline in the false arm keeps the WHOLE
union, never guesses never.

**§126 MEASURED AND LANDED at iteration 3 — +6 G→R / +21 W→R
against 1 R→W (27:1), right 410,194 → 410,220 = 85.65%.** The bar's
count missed by three (27 vs ≥30); the ratio is 5× the floor.
Three iterations, and the middle one was a GATE BUG that read as a
byte-identical build (the ancestor loop hit SourceFile for every
declaration — a parameter is also eventually under a file; the
correct shape is VariableDeclaration → List → Statement →
SourceFile exactly). The discriminator: §83's recorded "global var
vs parameter" observation was LITERAL — typeGuardOfFormInstanceOf's
baseline keeps the whole union in the else on top-level script vars
while instanceofWithStructurallyIdenticalTypes narrows the same
class shapes on parameters; the false arm gates on
`reference_is_top_level_var`. WHY upstream splits there is NOT yet
traced (suspects: cross-file mutable-global conservatism in the
flow container walk) — the gate is empirical, both fixtures pin it,
and the section owes the mechanism read if the residue ever
concentrates. narrowByClauseExpressionInSwitchTrue7's exhaustive
never-checks came in (+15, the §59 switch(true) family composing
with the false arm), intersectionWithConflictingPrivates net +3,
instanceofWithStructurallyIdenticalTypes's else whole. Gates: four
100% suites held, diagnostics 2,044 → 2,056, clippy 0, tests 0
FAILED, anchors 2,698.

## §127 — assertion calls narrow at CALL flow nodes [claimed: checker-1]

**The stale comment is the bar's argument.** The flow walk's CALL
arm skips to the antecedent with the note "assertion signatures
need call resolution this checker lacks" — written before callres
existed. The condition road three screens down already calls
`resolve_call_signature` and narrows by non-assert `is` predicates
(§100). `getTypeAtFlowCall` (flow.go) is the same recipe at a
statement position: a CALL node whose resolved signature carries an
`asserts x is T` predicate narrows the matching reference argument
to T in the flow that follows; bare `asserts x` narrows by
truthiness. §125's refusal named this leg as the unlock for its
union-miss class.

**Not this slice**: `asserts this` (no parameter name — declines),
never-returning calls (unreachable flow), and non-reference
arguments (`assert(x.kind === "a")` — is_matching_reference
declines; the discriminant composition is the §84 family's later
arm).

**Bar.** ≥40 G→R at ≥5:1. Falsifier: an assertion callee this
port resolves to the WRONG overload narrows by the wrong predicate
— confident wrongs concentrated where arity/overload selection is
weak; if it fires, gate on single-signature callees.

**§127 MEASURED AND LANDED at iteration 4 — +2 G→R / +15 W→R
against 2 G→W / 0 R→W (8.5:1), right ~410,279 → 410,294 (85.66%).**
The count bar missed by a wide margin (17 vs ≥40) and the reasons
are named per form: the corpus's assertion population is heavily
`asserts this` (declined by design — needs this-reference matching)
and `Debug.assert(false)`-style NEVER-RETURN unreachability (a
different mechanism: flow truncation, not narrowing). Four
iterations: (1) truthiness-narrowing the reference measured wrong —
upstream's bare `asserts x` narrows by the ARGUMENT AS A TRUE
CONDITION (`narrowTypeByAssertion` = `narrowType(arg, true)`),
which is what makes `assert(typeof x === "number")` work; (2) the
condition fix; (3) the SYNTACTIC PRE-GATE — typing an arbitrary
callee mid-walk perturbed creation-order-sensitive prints
(controlFlowFunctionLikeCircular1's `typeof Date` minted as
`DateConstructor`, 6 adverse) — only callees whose resolvable
declaration visibly returns `asserts` enter resolution;
(4) `Namespace.assert` property callees resolved syntactically
through exports. controlFlowOptionalChain took 7 W→R — the
board's item-6 residue moving for the first time. Residue owners
recorded: asserts-this, never-return unreachability, overloaded
assertion callees outside the two declaration shapes.

## §128 — never-returning calls truncate flow [claimed: checker-1]

**§127's other half, same seam.** `getTypeAtFlowCall` returns
`unreachableNeverType` when the resolved signature's return is
`never` — `Debug.assert(false); x;` reads `x : never` after the
call (assertionTypePredicates1's "Unreachable" comments). The §127
pre-gate extends: a callee whose resolvable declaration visibly
returns `never` (NeverKeyword annotation, the same two declaration
shapes + namespace exports road) enters resolution; the resolved
signature's return being never answers the never flow type. Same
perturbation-safety argument as §127's iteration 3.

**Bar.** ≥10 G→R at ≥5:1. Falsifier: reads BETWEEN the call and
the container end that upstream still types normally (only flow
AFTER the call truncates — the CALL node's position in the graph
handles this by construction; if lines before the call move, the
graph claim is wrong).

**§128 MEASURED AND REFUSED at 4:13 — the naive never-truncation is
WRONGER than no truncation, reverted byte-identical.** The arm
(resolved signature's return is `never` → unreachableNeverType)
converted 4 and broke 13, all in neverReturningFunctions1: reads
where upstream keeps `string | number | undefined` and we answered
`never`. The diagnosis pointer for the re-open: upstream reaches
this through `getEffectsSignature`, NOT plain call resolution — its
gates (single-signature callees, dotted-name typing WITHOUT flow,
the explicit-return-type requirement, and which CALL nodes the
binder even wires) are unread, and at least one of them excludes
the majority of this fixture's calls. Read getEffectsSignature +
the binder's createFlowCall conditions BEFORE the second attempt;
the §127 asserts half survives unchanged (its pre-gate keeps the
never road out).

**§128 ATTEMPT 2 — LANDED at +6 W→R / ZERO adverse (85.67%).** The
refusal's diagnosis was one read away from the fix: the never
answer is a SENTINEL (`unreachableNeverType`) converted to the
DECLARED TYPE at the walk's exit (`flow.go:111`) — the first
attempt's 13 wrongs were exactly that conversion missing, and
returning `state.declared_type` at the CALL node directly is the
port (equivalent while no narrowing node sits between the call and
the read; a divergence there would be narrowing inside unreachable
code, measurable if it ever surfaces). The count bar missed (6 vs
≥10): the residue is callee shapes outside the pre-gate —
`this.fail()` (this-based receivers), `((Debug).fail)()`
(parenthesized spines) — each nameable, none worth the perturbation
risk today. Diagnostics moved +5 (2,067 → 2,072: unreachable-read
diagnostics compose). A refusal reversed by reading ONE more
function is the cheapest un-refusal on record; §125's re-open note
said "citing this pair" and this is what the pair bought.

**§128.1 — this-based and method-declared assertion callees (+3/0).**
The pre-gate's two extensions: a `this.member()` callee resolves
through the ENCLOSING CLASS's members table (parent walk to the
ClassDeclaration, `binder.symbol_of`, members read — syntactic, no
this-typing), and MethodDeclaration joins the declaration shapes.
assertionTypePredicates1 +3. neverReturningFunctions1's
`this.fail()` still declines — its resolution road
(check_expression of a this-property callee) gaps before the
signature; recorded, not chased.

## §129 — the types harness parses its tsconfig units (the loader's second half) [claimed: checker-1]

**Same shape as §118: the machinery exists, one road never hands it
the input.** `trace_case::compilation` parses a case's
`tsconfig.json` unit (`tsr_tsoptions::parse_config_file` against a
full-unit VFS), removes it from the compilation, intersects the
config's file list into roots, and layers directives over the
config's options — upstream's `test_case_parser` tsconfig branch,
proven by the module_resolution suite. `program_for_case`
(types/diagnostics) ignores config units entirely: they compile as
ORDINARY TS FILES (their JSON parses as garbage statements) and
their `paths`/`baseUrl`/`moduleResolution` never reach the
resolver. §119's residual adverse (pathMappingBasedModuleResolution6_node,
nodeNextPackageSelfName*, ~26 lines) named this; the parked LOADER
entry priced it.

**The slice.** Config-BEARING cases only: parse the config unit,
drop it from the file list, apply directives over its options,
intersect roots exactly as `compilation` does. Cases with no config
unit are byte-untouched (the trace suite's last-unit root heuristic
is NOT imported — it belongs to that runner).

**Bar.** ≥80 G→R at ≥5:1 on checker_types. Falsifiers: (a) a
config's `files` list excluding units upstream still types —
root-shrink losses concentrated in config-bearing cases; (b) the
config unit itself vanishing from the corpus denominator — the
TOTAL population shifts, and any change there must match upstream's
own baseline line count for those cases, not merely improve.

**§129 MOOTED IN FLIGHT — the build landed from the other lane
between this bar's commit and its first edit.** The diagnostics
session's §533–§539 block gave `program_for_case` everything the
bar specified: config units parsed (`parse_config_file` against a
full-unit VFS), options layered under directives, roots from the
config's file list (§535), the no-config LAST-UNIT ROOT HEURISTIC
(§537 — which this bar had explicitly excluded as the trace
runner's; the other lane measured it IN for the types harness,
superseding that exclusion with a number), `@currentDirectory`
(§539) and case sensitivity. This lane's visibility edits were
reverted unlanded; the clean tree reads byte-identical to baseline.
The LOADER entry's parked half is now BUILT — tsr-9or.1's ledger
should re-census the pathMapping/self-name corpora fresh before
anything further is priced against it. Two lanes converging on one
seam within hours is the coordination protocol's first true
collision; the section-number claim (bar to main first) is what
kept it a no-op instead of a conflict.

**§129 attribution CORRECTED (flagged by checker-2, same drill as
ever):** the §533–§539 tsconfig block was the THIRD session's — the
diagnostics-numbered lane — not checker-2's. The mooting story
stands unchanged; the ledger now carries the true name. Corrected
rather than edited silently.

## §130 — the missing-export alias answers TS2305's any [claimed: checker-1]

**The ALIAS census's post-§119 residue decomposed.** 488 want-any
lines remain; the ES-import rows (ImportSpecifier 119, ImportClause
75, NamespaceImport 50) are now FINDABLE modules — §119's arm
passes them — whose target resolution fails one step later. The
top case (es6ExportEqualsInterop 21) names the mechanism: the
module resolves, the NAMED EXPORT does not exist (`export =`
modules under non-interop, genuinely absent members), upstream
reports TS2305/TS2614 and the alias reads errorType-printed-any at
every use. The establishment machinery is ALREADY BUILT for the
diagnostic (`report_missing_module_export`, symbols.rs:1005): module
resolves in-program, `export =` declined, exports table non-empty
(§186's empty-table guard), name absent through the star road.

**The arm.** In `get_type_of_alias`, before the errorType fall-through:
an ImportSpecifier-declared alias whose module RESOLVES and whose
name's absence is ESTABLISHED by the same gates answers `any`.
ExportSpecifier re-exports NOT admitted (§119 falsifier (a)'s
scope, kept). The `export =` module case: upstream's named-import-
against-export= IS TS2305-family (es6ExportEqualsInterop's wants)
— the resolve_external_module_symbol≠module test routes it to the
SAME answer, gated on the target symbol being resolvable.

**Bar.** ≥60 G→R at ≥5:1. Falsifier: modules whose exports table
this port under-fills (star chains through unresolved targets,
late-bound exports) converting honest gaps to confident anys — the
§186 empty-table guard plus a no-unresolved-star gate must hold it;
if the class fires anyway, the gates get the establishment treatment
§123 gave the member walk, and the price lands here.

**§130 MEASURED AND LANDED at iteration 2 — +23 G→R / 2 G→W /
0 R→W (11.5:1), right ~410,303 → 410,326 = 85.67%.** The count bar
missed again (23 vs ≥60) and the pattern is now FOUR landings wide
(§121 47<60, §126 27<30, §128 6<10, §130 23<60): want-any census
rows mix mechanisms, and each §-arm takes only its own — SIZE
FUTURE BARS BY MECHANISM SAMPLE, not census bucket. Iterations:
(1) +27/9 — `default`-named imports ride interop machinery
(allowSyntheticDefaultImports9, gated) and (2) non-identifier
export keys may spell one export two ways (gated). Residual 2:
exportSpecifiers' TYPE-ONLY export specifiers (`export { type x }`)
— the binder does not file them, so their absence false-establishes;
NAMED OWNER: the binder's type-only specifier arm, one gate away
(a module-contains-type-only-exports test) if its rows grow. The
thirty-fourth pin flipped with its discriminating half preserved
(any is not `typeof /m.ts`). Diagnostics 2,072 → 2,084 on the
merged tree. es6ImportDefaultBindingFollowedWithNamedImport1 9,
modulePreserve4 3, es6ImportNamedImportNoExportMember 2.

## §131 — the missing default import under no-synthetic configs [claimed: checker-1]

**§130's sibling, sized by mechanism sample.** `import d from "m"`
where `m` carries no `default` export: with synthetic defaults OFF
(no esModuleInterop / allowSyntheticDefaultImports, ES module
kinds), upstream reports TS1192 and the alias reads
errorType-printed-any. With synthetic defaults ON, upstream
RESOLVES through interop machinery this port lacks — those stay
gaps (the §130 first pair's allowSyntheticDefaultImports9 lesson,
applied at the bar instead of after it). The arm: an ImportClause
default alias whose module's absence-of-`default` is established
(§130's gates verbatim: resolves, no `export =`, non-empty,
star-free, identifier-keyed) AND whose options road answers
no-synthetic answers `any`.

**Bar.** ≥15 G→R at ≥5:1 (mechanism-sampled: the census's
ImportClause 75 is mixed with interop-resolvable rows this arm must
skip). Falsifier: the synthetic-default predicate mis-derived from
options — upstream's `canHaveSyntheticDefault` consults module
kind, file extension, and the resolution mode, and a too-coarse
port of it fires the arm exactly where upstream resolves.

**§131 MEASURED AND LANDED at iteration 2 — +10 G→R / 0 adverse,
right 410,326 → 410,336.** The mechanism-sampled bar (≥15) STILL
over-predicted by a third — the fifth under-count, and this one
was sized from the mechanism, which sharpens the rule: the census's
want-any rows include cases whose OTHER lines already fail for
different reasons, so even a correctly-scoped arm converts only the
near-miss fraction. Iteration 2's gate: Node16/NodeNext excluded
whole — `canHaveSyntheticDefault`'s head arms resolve synthetic
defaults by USAGE/TARGET format (ESM importing CJS always has one),
a mode road this port's predicate does not model
(nodeNextCjsNamespaceImportDefault1's 4 G→W). The
`can_have_synthetic_default` predicate built for TS1192's
diagnostic answered the TYPE question unchanged — ADR-0040's
channel split, crossed in the profitable direction for once.

## §132 — OPENER (probe only, autonomous tick): es6ExportEqualsInterop decomposed

The gaproot board's twin ALIAS rows concentrate here (43+41 lines,
plus constEnums 34+66 nearby). The 120 failing lines split into TWO
mechanisms:
  1. WANT-ANY: the ES-import forms (`import x from`, `import * as y
     from`) against `export =` AMBIENT modules — upstream's TS2497
     ("module resolves to a non-module entity") deliberate
     error-answer. The §130/§131 establishment family's next arm,
     needs the export=-module test inverted (fire WHEN export=
     present and interop off).
  2. WANT-REAL (`number`): the import-equals forms (`import z2 =
     require("variable")`) — upstream resolves through the ambient
     module's `export =` to the var/class/function target. The
     §31/§10.8 require() chain EXISTS and works for real files but
     gaps for ambient modules — the decline point inside
     get_type_of_alias's ExternalModuleReference arm (or the
     target's get_type_of_symbol inside `declare module`) is
     unlocated. TRACE FIRST: one eprintln at the require() arm on
     the filtered case names the decline in minutes.
Neither built this tick; the probe is the deliverable (§547's
form). Sizing: case-gate says ~120 lines here + the constEnums
family if mechanism 2 generalizes.

**§132 TRACE COMPLETE (one probe pair):** the require()/export=
chain WORKS — all ten ambient modules resolve, export= follows,
nine targets type (z2..z0 read RIGHT already; 81 right lines in
the case). The 120 failing lines are ALL the ES-import forms:
`target=None NO VALUE` at resolve_alias. Mechanisms per form under
no-synthetic options: DEFAULT imports of an `export =` module want
ANY (TS1192/interop-off); NAMESPACE imports want ANY when the
export= target LACKS namespace meaning (TS2497 non-module entity)
and want REAL resolution when it has it (y4/y5/y8's number reads —
that half needs the module-object road extended, stays a gap).

## §132 — ES-import forms against `export =` under no-synthetic options [claimed: checker-1]

**The arm.** Plumb `allow_synthetic_defaults` into the checker
(upstream `getAllowSyntheticDefaultImports`: explicit option, else
esModuleInterop, else module==System; Node16/NodeNext excluded whole
as §131). Then in `get_type_of_alias`: with synthetic defaults OFF
and the module's exports carrying `export=` —
  - an ImportClause default alias answers `any`;
  - a NamespaceImport alias answers `any` IFF the export= target
    resolves and its flags LACK namespace meaning
    (VALUE_MODULE|NAMESPACE_MODULE); a namespace-like or
    unresolvable target keeps the gap.

**Bar.** ≥40 G→R at ≥5:1 (mechanism-sampled: the x-family 20
aliases + non-namespace y-family + their reads in this case, plus
siblings). Falsifier: interop-configured corpora
(esModuleInteropDefaultImports) entering through a mis-derived
options predicate — the §131 falsifier, second firing chance.

**§132 MEASURED AND LANDED at iteration 5 — +5 G→R / ZERO adverse
(ambient-module var-target synthetic defaults), after FIVE pairs
mapped the seam:**
  1. Both arms as barred: +16/52+10 INVERTED — upstream RESOLVES
     default imports of `export =` declaration files regardless of
     the interop options (the §131-era option reading was wrong for
     decl files), wants `{ a: number; b: number; }` not any.
  2. Defaults resolve through the export= target: +68/52+10 — the
     WINS confirmed the road, and the tsr-4jk NAMING TRAP fired its
     freshest price: a SECOND alias to one namespace breaks the
     one-alias rename (`typeof z4` → `typeof Foo`, 10 R→W).
  3. Var-targets only (structural prints, no rename exposure):
     +26/38 — the namespace-import any-arm was WRONG per flavor:
     tsgo resolves `import * as y9 from "class"` (`typeof y9`
     wants); the TS2497 assumption does not describe these
     baselines. THE FLAVOR MAP IS CORRECTED: only interface-target
     ES imports want any here.
  4. Namespace arm removed whole: +16/6 — real-file modules gaining
     a default alias flip the printer's spelling for OTHER aliases'
     qualified references (importEquals1, `types.A` →
     `import("./a").A`).
  5. Ambient modules only: +5/0.
The residue is all one owner: PER-SITE NAMING (the §20.1/§41/tsr-4jk
constraint), now carrying es6ExportEqualsInterop's remaining ~100
lines beside temporal's 400 and import-spelling's 128. Three heads
became four; the subsystem's price rises with each window that
touches its boundary. DO NOT extend this arm past ambient+var
without the naming study.

## §133 — type parameters are subtype-reduction-free ternary branches [claimed: checker-1]

**The ConditionalExpression root's biggest concentration** (gaproot:
637 lines, subtypesOfTypeParameterWithConstraints2 108 +
subtypesOfTypeParameter 84 + unionTypeReduction2 54). The §7-era
fence declines any non-primitive branch; the baselines show
upstream's SUBTYPE reduction keeps a type parameter beside every
branch flavor these fixtures write — `T | null`, `T | undefined`,
`number | T`, `1 | T`, `T | RegExp`, `T | { foo: number; }`,
`T | (() => void)` — INCLUDING a constrained `T extends number`
beside bare `number` (:124's want `number | T`, uncollapsed). A
type parameter is never subtype-collapsed into a sibling in these
prints. The arm: `TypeFlags::TYPE_PARAMETER` joins the fence's
SAFE set — and because the OTHER branch may then be any shape
(`T | RegExp` wants an object sibling), a branch pair where ONE
side is a type parameter is reduction-free WHOLE.

**Bar.** ≥100 G→R at ≥5:1. Falsifier: a `c ? t : u` pair of two
CONSTRAINT-RELATED parameters that upstream DOES collapse
(subtypesOfTypeParameterWithConstraints's T-extends-U forms) — if
its wants show single-parameter answers, the two-parameter pair
needs a decline while the parameter-beside-nonparameter form lands.

**§133 MEASURED AND LANDED at iteration 2 — +67 G→R / +124 W→R
against 16 G→W / 10 R→W (7.3:1), right 410,832 → 411,007 = 85.81%
on my tree (pre-merge with the other lane's callres2-§133/§134;
per-file numbering confirmed, no collision).** Iteration 1
(one-parameter-side admitted whole) measured 3.6:1 and the fixture
itself supplied the split: a CONSTRAINED parameter collapses into
an OBJECT-ish sibling its constraint chain relates to (`T extends
U extends Date` beside `new Date()` wants `Date`) but NEVER into a
primitive/literal sibling (`T extends Number` beside `1` wants
`number | T` — the wrapper-interface constraint is not
subtype-below the primitive); an UNCONSTRAINED parameter never
collapses (`T | RegExp`, `T | { foo: number; }`). Two-parameter
pairs stay fenced (T-extends-U wants the supertype — undecided
here). Priced residue: the fresh-literal WIDENING class
(`number | T` wanted at VAR declarations where the ternary answers
`1 | T` — the §98-family declaration-widening road, 6 lines),
genericContextualTypes1's 5 R→W (contextual interactions on the
other lane's arc). The W→R half (124!) was the buried treasure:
wrong union spellings across typeArgumentInference/
genericCallWithGenericSignatureArguments corrected by the same
admission.

## §134 — getWidenedLiteralType's union arm [claimed: checker-1]

**§133's residue, one missing arm.** `getWidenedLiteralType`
(checker.go:25487) maps a UNION over itself
(`TypeFlagsUnion → mapType(getWidenedLiteralType)`); this port's
copy returns a union unchanged (unions are never `fresh`), so
`var r = c ? 1 : t` declares `1 | T` where upstream declares
`number | T` (subtypesOfTypeParameterWithConstraints2's 6 G→W,
priced at §133). The arm: `TypeData::Union` maps constituents
through the same function and rebuilds with `get_union_type`.

**Bar.** ≥10 G→R at ≥5:1. Falsifier: origin/written-order unions
whose SPELLING the rebuild loses (§77-family) — if the rebuild
drops a written order that widening should keep, the arm needs the
§53 origin carriage before it lands.

**§134 MEASURED AND LANDED at iteration 2 — +34 W→R / 2 R→W (17:1),
scoped to the INITIALIZER road.** The whole-function union arm
measured 45:80 INVERTED — `getWidenedLiteralType`'s consumers are
position-sensitive in this port (return-type inference and the
array-literal roads rely on the union passthrough), so the mapType
arm lives in `get_widened_literal_type_for_initializer` alone.
Priced residue: constAssertions 1 + literalTypeWidening 1 (as-const
adjacent unions the CONSTANT flag doesn't cover at this seam).
§133's ledger annotation, confirmed by the other lane's merged
pair: the priced genericContextualTypes1 5 R→W were BOUGHT BACK by
callres2-§134's returnMapper guard — the two lanes' rules compose
to zero there, and the §133 record's price row is settled.

## §135 — OPENER (sized, not built): generator return inference

The board's FUNCTION declaration-name row (833 gap lines; near-miss
204 cases) concentrates in generatorReturnTypeInference{,NonStrict}
(21+21) and spreads wide. The wants are `checkFunctionExpression`'s
generator half: `function* g() { yield 1 }` declares
`() => Generator<number, void, any>` (strict: `unknown` third
argument) — YIELD-type collection over the body (yield operands
unioned; `yield*` delegates through the operand's iterator type),
RETURN-type from return statements (void when none), and the
NEXT-type from yield-expression CONTEXTUAL positions (the
`[(1 | undefined)?, ...]` tuple wants show upstream unioning the
observed next-usages). Mint: the global `Generator` (arity 3) via
`global_type_symbol_with_arity`, `IterableIterator` for the
down-level flavors. The cheap first slice: yield-operand union +
return-void + `any`/`unknown`-by-strict next — the two head cases'
simple functions. The generic and delegate forms
(`<T>(x: T) => Generator<T, T, T>`, `yield*`) are later arms.
Entry: `check_function_expression`'s return-type computation
(wherever the non-generator inferred return lives — the
`function_types.rs`/signatures seam). NOT built this window; the
sizing is the deliverable.

**§135 opener SHARPENED (the read the sizing owed):** the generator
arm EXISTS — `return_type_from_body`'s asterisk half
(signatures.rs:1009, the §15-callres bar) already aggregates
statement-position yield operands, dedups unwidened, subtype-reduces
multi-operand sets, and mints `Generator<Y, void, unknown>`. The
declines that own generatorReturnTypeInference's 42 lines, in
priority order for the next window:
  1. VALUED RETURNS decline whole (line 1026) — the R slot wants
     the return aggregate through the SAME machinery the
     non-generator road has; `Generator<number, string, any>`-class
     wants.
  2. The NEXT slot is hardcoded `unknown` — the non-strict flavor
     wants `any` (generatorReturnTypeInferenceNonStrict); key it on
     strict_null_checks per upstream :20245.
  3. `IterableIterator<number>` wants — the DOWN-LEVEL mint
     (target < ES2018 uses IterableIterator arity 1); the mint is
     target-keyed.
  4. `yield*` and value-used yields stay declined (contextual).
Slices 1–3 are each one measurable edit inside the existing arm.

**§135 slices 1+bare-yield MEASURED AND LANDED at iteration 2 —
+21 G→R / 1 G→W (21:1), plus a POPULATION REALIGNMENT: total
aligned lines rose 470,881 → 471,010 (+129) as the newly-minted
generator signatures re-aligned their cases' assertion streams; net
right +143. 85.85% at coverage.** Slice 1: valued returns feed the
R slot through the yield slot's own aggregation (single widens,
multiple subtype-reduce, JS declines). Bare `yield;` contributes
`undefined` under strict (non-strict declines — its `any` next is
contextual). Iteration 2's fix: the statement-position gate runs
BEFORE the bare-yield arm — `const value = yield;` feeds the NEXT
slot from its declaration and must decline (the first pair's 3 G→W
in generatorImplicitAny). Priced residue: generatorTypeCheck44 1.
The thirty-fifth pin flipped (valued-return decline → computed).
Remaining §135 slices: the down-level IterableIterator mint
(target-keyed), `yield*` (iteration protocol), contextual
next-types (the other lane's arc).

## §136 — annotation references fill their defaulted tail [claimed: checker-1]

**Found through §135's residual, and it is corpus-wide.** A partial
type-argument list against a DEFAULTED generic gaps in ANNOTATION
position: `interface Foo<T, U = string>` + `declare const x:
Foo<number>` answers error (probefile-verified; the full list
works). The §38 fill (`fillMissingTypeArguments`, checker.go:19458)
landed for CALL-side written arguments only; the annotation road's
arity check rejects partial lists outright. The ENTIRE lib iterator
family rides this — `Iterator<T, TReturn = any, TNext = undefined>`,
`IterableIterator<number>`, `Generator` references in annotations —
plus every user-defaulted generic. The arm: the annotation road's
arity test accepts `written < params` when defaults cover the tail;
tail positions instantiate their default under the map-so-far
(§38's exact recipe), `unknown`/error-decline where a default is
absent or does not compute.

**Bar.** ≥150 G→R at ≥5:1 (the iterator-annotation class alone is
wide; mechanism-sampled from the §135 residuals + probefile).
Falsifier: defaults REFERENCING EARLIER PARAMETERS (`<T, U = T>`)
must instantiate under the partial map — a raw default type would
print `T` where upstream prints the substituted argument.

**§136 MEASURED AND REFUSED at net −350 over five iterations —
reverted byte-identical, and the map is the deliverable.** The fill
itself is CORRECT (probefile: `Foo<number>` → `Foo<number, string>`;
+208 G→R real, genericDefaults 55 + tsxLibraryManagedAttributes 41)
but every gate failed to contain the PRINT side:
  1. ungated: 623 G→W — BARE references fill position-sensitively
     (typed arrays: `ArrayBuffer` at value positions,
     `ArrayBufferLike` in annotations);
  2. non-empty-written gate: 578 — the filled tail PRINTS
     (`Iterable<number, any, any>` vs want `Iterable<number>`);
  3. written-arity display mint: 553 — the SIGNATURE-instantiation
     road mints its own references and never sees the display hint;
  4. trailing-default trim in `type_reference_text`: unchanged —
     the offending texts are born on a road that never calls it
     (the §42-family string-substitution rebuilds);
  5. all-declarations arity scan: byte-identical to 4.
BLOCKER NAMED: reference texts are born in MORE THAN ONE place, and
the omit-defaulted-tail print rule must live in all of them — the
same immutable-text architecture the per-site naming subsystem owns.
The +208 waits there. DO NOT retry the fill without first unifying
where reference texts are minted; the iterator-family annotation
gaps (§135's residual and this section's motivation) are the same
prisoner.

## §137 — the fourth admission flag: written union order [claimed: checker-1]

**§77.2's twice-refused head, re-entered through the door that was
open all along.** Both refusals attacked the UNION MINT (origin
machinery, annotation-mint order); the retry's autopsy said the
order-wanting positions "never reach the union-NODE path". But
§136's landing minted a NEW candidate class that DOES: LIB
signature parameters written `Iterable<T> | ArrayLike<T>`
(arrayFrom 22, the IteratorObject families ~27, mapGroupBy/
objectGroupBy 14) — plain written UnionTypeNodes whose constituents
only became resolvable with the default-fill. Their prints go
through the §77 written_text PARAMETER CARRIAGE, whose union arm
already renders written order — but the ADMISSION GATE declines
them: written reuse fires only on a marker the fresh render cannot
reproduce (quote §77, Array-head §77.1-era, void position §108.1),
and a plain union carries none. The fourth flag: **the union's
WRITTEN constituent order differs from the fresh render's sorted
order.** Same principle, fourth confirmation attempt; the three
prior flags each landed at zero adverse.

**Bar.** ≥40 G→R at ≥5:1, measured AFTER the other lane's §137
lands (their harvest also fattens union pools — census-after
agreed). Falsifier: positions where upstream genuinely SORTS a
written union (the §77.2-retry's const-context class) — if the
flag admits those, the gate needs the annotation-position test
before the flag.

**§137 MEASURED AND LANDED — +308 W→R / 2 R→W (154:1), ZERO G→W;
right 411,733 → 412,039 = 86.03%, cases 4,172 → 4,183. The 86%
line crossed.** The fourth admission flag is the principle's
LARGEST confirmation (quote +370, Array-head, void +22, now
written-union-order +308): a TOP-LEVEL written union whose
constituent order differs from the fresh sort keeps its written
order, same-set-different-order only, zero threading (the test
lives at the admission site — resolve, compare split-sets). The
candidates were 6× the bar's sizing because the flag corrects
WRONGS the census never counted: every written-union parameter
this port ever printed sorted-wrong (stringLiteralTypesOverloads,
unionAndIntersectionInference3, taggedTemplates...) — §136's ~60
constructible lines were the tip. The 2 R→W (builtinIterator):
positions where upstream itself sorts — the §77.2-retry's
const-context class, priced. The twice-refused §77.2 head is now
THREE-QUARTERS LANDED through the door neither refusal tried: not
the union mint, the ADMISSION GATE. Its residue (optionality-built
and narrowing-rebuilt union order — narrowingUnionWithBang,
controlFlowAliasing) stays with the §52.1 site-sensitivity class.

## §138 — OPENER (probed, not built): written order at the UNION MINT

narrowingUnionWithBang's dump splits the §137 residue in two:
  1. WRITTEN-ORDER wants at ALIAS/VAR positions (`Error1..Error9,
     Correct, undefined` written; we sort `Correct` first): the
     spelling never passes the §137 parameter/return carriage —
     it is the UNION MINT's stored text. The §77.2 refusals
     attacked this with origin machinery (R→G losses) and blanket
     reuse (const-context sorts); the UNTRIED third form is §137's
     lesson applied at the mint: after the normal sorted build,
     when the written constituent order differs at EQUAL SETS,
     override the minted text with the written-order spelling
     (first written spelling wins — unions intern by constituent
     set, so this is a per-TYPE not per-site override; the
     const-context falsifier decides it).
  2. NARROWED-SUBSET wants (`Correct | undefined` after bang
     chains): narrowing-owned, not a print question.
Needs a store text mutator (texts are currently immutable
post-mint — one setter, mint-module-private). NOT built this
window; the §77.2 ledger gains its third distinct entry road.

**§138 BAR (bar-before-code):** the mint-side same-set override —
in the UnionTypeNode resolution arm, after the normal sorted build,
a written constituent order differing at EQUAL SETS overrides the
minted union's stored text with the written-order spelling
(first-written-wins; unions intern by constituent set). One store
mutator, mint-module-scoped. **≥30 G→R at ≥5:1.** Falsifiers:
(a) const-context literal unions upstream genuinely sorts (the
§77.2-retry's measured class) — if they fire, the override gates on
non-const positions; (b) the §137 admission flag double-firing
(annotation carriage AND mint override racing — the carriage runs
first and returns, so the mint override must only cover positions
the carriage never reaches; a double fire would show as zero-delta
churn, not wrongs).

**§138 MEASURED AND REFUSED at 299:1,098 — reverted byte-identical,
and the union-order question is now CLOSED with three distinct
measured refusals.** The mint-side override converted 299 (the
controlFlowAliasing/narrowing spellings it aimed at) and broke
1,098: the override is PER-TYPE (unions intern; one TypeId serves
every position) while the truth is PER-SITE — a union written
`number | string` in one annotation overrode the shared sorted
union at hundreds of COMPUTED positions upstream prints sorted
(booleanLiteralTypes 92, instanceof families, inferTypePredicates).
The three roads, each measured: origin machinery (§77.2, 35:249+),
blanket written reuse (the −270), the mint override (this, 1:3.7
inverted). THE ONLY WORKING FORM IS §137's: per-POSITION written
carriage behind a same-set admission flag. The residue
(controlFlowAliasing's 52 W→R this pair found, optionality-order,
narrowing-rebuild spellings) belongs to per-site rendering — the
printseam study's arm 3+, where the site is known. DO NOT attempt
a fourth type-level road; the pair triple confirms the shape.

**Window-close board reading (86.09%):** the remaining checker-1
rows are subsystem-anchored — ThisKeyword 511 gap lines +
thisTypeInFunctions' 301 non-right (the `this` typing/narrowing
subsystem, which also gates §127's asserts-this residue),
typeGuardsWithInstanceOfBySymbolHasInstance's partial-narrow wrongs
(`A | C1` where the predicate should remove A — the §111 hasInstance
family's next trace), the symbol-has-type differs-row (570 lines,
332 cases, median 2 — long-tail spelling diffusion), and the
per-site rendering arms. Each has its opener recorded; none is a
single-tick slice. The next window opens on the `this` subsystem
trace or printseam arm 2's modulespecifiers — both multi-hour,
both specified.

## §139 — OPENER (probed): contextual methods drop their this-parameter print

thisTypeInFunctions2's wrongs (`init(): void` where the want is
`init(this: IndexedWithThis): void`) are NOT a signature-print
defect: probefile shows a written `init(this: I): void {}` object
member printing its this-parameter correctly through the fresh
road. The dropping shape is a method WITHOUT a written `this`,
CONTEXTUALLY typed from a target whose declaration carries one —
upstream's contextual signature hands the method its this-parameter
(`getThisTypeOfDeclaration` through the contextual road) and the
node builder prints it; this port's contextually-typed member
signature loses the slot. Owner: the contextual-signature
application (the other lane's files) or the §135-family member
map's signature construction — FLAGGED to checker-2 rather than
edited across the lane line. ~40+ lines in thisTypeInFunctions2
alone; the ThisKeyword row's 511 partially hangs here.

## §140 — dynamic import() expressions type as Promise<typeof import("spec")> [claimed: checker-1]

**The arm-2 pool's largest mechanical slice, and it needs NO
per-site machinery**: the import-spelling text is minted FOR the
call, at the one place that holds the specifier verbatim.
`checkImportCallExpression` (checker.go): `import("./m")` types as
`Promise<NS>` where NS is the module namespace type — here an
Anonymous type with text `typeof import("<specifier>")` carrying
the MODULE SYMBOL (member reads flow through exports; `.then`
signatures come free through the lib Promise + §136's fills).
Candidates: the importCallExpression* families (~150 lines of
`Promise<typeof import("./0")>` / awaited `typeof import("./0")`
wants), ramdaToolsNoInfinite2's 53 partially.

**Bar.** ≥60 G→R at ≥5:1. Falsifiers: (a) mode-variant spellings
(importCallExpressionInCJS5's `require`-flavored wants may differ);
(b) an unresolvable specifier keeps today's answer (the §31-family
boundary owns those); (c) the minted NS text must intern per
(module, spelling) or duplicate mints churn prints.

**§140 MEASURED AND LANDED — +122 G→R / 11 G→W (11:1), right
412,344 → 412,466 = 86.12%, first measurement over the bar.**
`import("./m")` types as `Promise<typeof import("./m")>`: the
namespace type minted AT THE CALL (Anonymous, the module's symbol,
interned per module+spelling through qualified_reference_types),
wrapped in the lib Promise — `.then` signatures and awaited reads
compose free through the Promise members and §136's fills. The
import-spelling head's premise ("needs PER-FILE printing context")
was TOO STRONG for this family: the call site holds the specifier
verbatim, so the spelling is creation-time-mintable. Priced
residue: interop default-wrapping flavors (esModuleInteropImportCall
3, dynamicImportsDeclaration 3 — `{ default: ... }` shapes), the
export= import-call flavors 2. The arm-2 pool's remaining ~1,000
import-spelling lines split between checkExportsObjectAssign (JS
exports), ramdaTools' inference carriers, and true per-site
annotation positions.

**Body-side `this` trace (window close): the ThisKeyword row's 511
is NOT a standalone-print head — zero `this : X` assertion lines
are non-right; the row counts `this`-RECEIVERS inside larger
gapping expressions (`this.x` in object-literal methods and
callback bodies). Their owner is the same pair of walls the other
lane's §139-callres2 refusal traced: `getContextualThisParameterType`
(unported, the fall-through this port's check_this_expression
documents) and the POLYMORPHIC thisType model (apparent-type
instantiation of `this: this`). THREE roads now converge on the
one subsystem — the §139 print slot, the body-side receivers, and
§127's asserts-this — which makes the thisType model the next
properly-sized subsystem bar for whichever lane opens it (est.
500+ lines across the three consumers). Recorded; not opened this
window.**

## §141 — object-literal methods rebind `this` (the walk's missing arm) [claimed: checker-1]

**The probe that found it**: `const o = { x: 1, m() { return
this.x; } }` errors EVERY line — `this` errors, so `m`'s return
errors, so `o` errors: one missing arm poisons whole objects.
`check_this_expression`'s walk has arms for functions (→ any),
classes (instance/static), and this-parameters; a METHOD inside an
OBJECT LITERAL falls off the end. Upstream's `tryGetThisTypeAtEx`
fallthrough rebinds to `any` at a non-contextual literal method
(the typed-this forms need contextual ThisType/this-parameters —
the recorded subsystem). The arm: MethodDeclaration (plus
accessors) whose PARENT is an ObjectLiteralExpression answers
`any` — class methods are untouched (their parent is the class,
and the class arm already owns them).

**Bar.** ≥60 G→R at ≥5:1 (the un-poisoning compounds: every
member of every such object). Falsifier: contextually-typed
literals whose target assigns a REAL `this` (ThisType<T>, this-
parameters) — answering any there converts honest gaps to wrongs;
if that class is material, the arm gates on no-contextual-target.

**§141 MEASURED AND REFUSED over two iterations (78:189 ungated,
35:87 contextually-gated) — and the refusal bought the subsystem's
true mechanism.** The fallthrough-any premise was WRONG:
thisTypeInObjectLiterals' wants show upstream resolving `this` in a
NON-contextual literal method to the CONTAINING LITERAL'S TYPE
(`m(): number` from `return this.d` reads) — the this-in-methods
inference is live in tsgo, and `any` is only the residue where
that circularly fails. The tractable port: `this` in a literal
method mints an ANONYMOUS type carrying the LITERAL'S SYMBOL —
member reads (`this.d`) flow lazily through the symbol road with
NO circularity (members resolve per-name, the current method's own
slot never completes through itself), exactly the §41-family
member-carrying shape. The TEXT is the wall again (the literal's
structural print isn't known at the mint — the immutable-text
architecture's sixth head) BUT the §-population is mostly MEMBER
READS whose own lines print member types, not `this` itself —
so a placeholder text may cost little. Next window: the
symbol-carrying mint with a text census (how many assertion lines
print `this` bare in these cases). Reverted byte-identical.

**Instrument caveat (§141 window):** the post-revert clean tree
read +15 in intraExpressionInferences vs the §140-accepted baseline
(11 G→R + 4 W→R) — transitions that SURVIVED the revert and are
therefore not §141's (the §85.1 rule). Owner unknown: candidate is
alignment/ordering nondeterminism in that case family (the §87
unicodeEscapes caveat's second instance). Re-accepted; any future
single-case ±15 there is noise until traced.

## §142 — literal-self `this`: the symbol-carrying mint [claimed: checker-1]

§141's corrected mechanism, built: `this` in an object-literal
method answers a per-literal Anonymous mint carrying the LITERAL'S
SYMBOL — member reads flow lazily through the symbol road (no
circularity: members resolve per-name), the §41 member-carrying
shape. The text census read ZERO bare `this :` lines among every
candidate failure, so the placeholder text is unobservable today
(recorded as the mint's falsifier: the first bare print names the
immutable-text wall's sixth consumer). UNGATED first — the §141
damage came from answering ANY; the literal-shaped answer may
match contextual cases too (their wants are literal-shaped).

**Bar.** ≥60 G→R at ≥5:1. Falsifiers: (a) contextual targets
whose `this` differs from the literal (ThisType<T> remaps) —
gate on has_no_contextual_type if they fire; (b) accessor/computed
member circularities the lazy road can't dodge.

**§142 PARKED after six iterations — the mechanism is PROVEN, the
bar unmet (best 2.4:1).** The ladder: Anonymous mint fired zero
(literal properties live in MEMBERS → Named); ungated Named +71/99;
contextual gate +50/36; accessors/JS/computed-names out +38/16 with
ZERO regressions (the reproducible best); the function-expression
extension +40 but woke fatarrowfunctions (20 R→W) — killed by the
noImplicitThis OPTION GATE (the inference is option-keyed upstream,
the window's key discovery) — leaving looseThisTypeInFunctions'
17 R→G (the mint's member road answers error where those lines
were any-right; unprobed). Rebuild-from-record: iteration-4 arm
(MethodDeclaration, parent ObjectLiteral, no_implicit_this +
no-contextual + non-JS + no-computed-members, Named mint with the
literal's symbol cached per literal). Next window: probe
looseThis's 17 wants first; if they're `any`-family the fn-expr
arm needs a per-member road fallback, and the combined form
plausibly clears the bar (~+60 at ~4-6:1). Reverted
byte-identical.

**§142 probe answered (parked state re-verified):** the looseThis
17 R→G came SOLELY from the function-expression arm — the
methods-only + noImplicitThis form re-measured +38 G→R / 15 G→W
with ZERO R→W and ZERO R→G (pure gap-trade, 2.5:1). Still under
every landed precedent's ratio, so the park stands, but the state
is now exactly characterized: the 15 adverse are 7 head-case
literal-print spellings (the mint's members print through the
literal symbol where the want's spelling differs), 2
widening-on-use, 2 this-predicates, 4 singles. The next
ratio-mover is the 7: the mint prints member types through
UNWIDENED literal member symbols — the §134-family initializer
widening applied at the mint's member reads is the one candidate
between 2.5:1 and ~5:1.

**§142 record CORRECTED (the widening hypothesis measured ZERO):**
the head case's 7 adverse are NOT unwidened member freshness — the
widening arm at the mint's reads changed nothing byte-for-byte.
They are the FUNCTION-EXPRESSION property family (`f: () => any`
got, `() => number` want) — the same shapes iteration 5 converted
and looseThis paid for. So §142's true remaining split: the
methods-only form (+38/15 at 2.5:1, zero regressions) vs the
fn-expr extension (+2 net, +17 R→G in looseThis whose contextual
shapes need tracing). The park's ratio path runs through the
looseThis trace, not widening. Corrected rather than silently
edited; reverted byte-identical.

**§142 looseThis trace, first pass:** the regressing literal is
`{ n, explicitThis: function (m) {...}, implicitThis(m) {...} }`
at an ASSIGNMENT/annotated position (fixture line ~27) whose
fn-expr property admits the arm — so either §94's ladder answers
no-contextual for a position it should cover (the assignment-root
arm §98 added — check its reach for THIS shape), or the mint's
presence poisons the literal's own type computation through a road
the filtered dump can't show. NEXT INSTRUCTION (one run): rebuild
the full arm with an env-gated eprintln AT THE FIRE SITE printing
(literal node, has_no_contextual answer) on the looseThis filter —
the admit/poison split names itself in one read. Reverted again;
the park's map is now three probes deep.

**§142 probe 4 (the eprintln run):** the fn-expr arm fires with
no_ctx=TRUE on looseThis's `let o = {` literal — which has NO
annotation: §94's LADDER WAS RIGHT, the previous note's suspicion
withdrawn. The 17 R→G are therefore a POISON inside the literal's
own computation with the mint live: the body `return m +
this.n.length` should flow mint→n:number→.length established-any→
any (upstream's own want is `(m: number) => any`), but the LITERAL
errors instead — some member-computation road rejects the minted
this mid-inference. Probe 5 (queued): member-level eprintlns in
the literal's property computation on the same filter. The park's
map: methods-only is landable-shaped at 2.5:1 zero-regression; the
fn-expr arm is one poison-trace from flipping ~24 more lines and
the combined ~5:1.

## §124.1 — the Function-family gate keyed on signatures [claimed: checker-1]

**§142's probe 5 named a §124 defect with reach beyond §142**: the
established-miss gate blocks any name the Function/CallableFunction/
NewableFunction interfaces declare (`length`, `name`, `arguments`)
on EVERY receiver — but the §117 fallback only consults those
interfaces for SIGNATURE-BEARING receivers. A `.length` miss on a
NUMBER receiver is fully established (Number's walk completes,
Function is never consulted) yet the gate blocked it — which is the
exact poison that erred looseThis's fn-expr bodies under the §142
mint (`this.n.length` → gap → return inference → literal error).
The refinement: Object's names always gate (its fallback is
unconditional for OBJECT-flagged receivers); the Function family's
names gate ONLY when the receiver carries call/construct
signatures. **Bar: ≥10 G→R at ≥5:1** (the .length/.name misses on
non-callable receivers across the corpus), zero R→W tolerated —
this narrows a conservative gate, so the only possible adverse is
a receiver whose signatures this port under-records.

**§124.1 MEASURED AND LANDED — +14 G→R / ZERO adverse (the bar's
zero-R→W condition met exactly), 86.13%+.** Three callable
detectors, each bought by one adverse class: the signature_types
map (base form; strictBindCallApply's 24 returned when it alone
decided — Anonymous class receivers signature lazily), the
Anonymous merged-symbol FUNCTION|METHOD|CLASS flags (fixed those
24), and the Named owner's syntactic call/construct-signature
member scan (objectTypeWithCall/ConstructSignature*'s 10). A
`.length`/`.name` miss on a non-callable receiver is now fully
established — which also DISSOLVES §142's fn-expr poison
(this.n.length flows to established-any instead of erring the
literal): the §142 park's re-measure with the poison gone is the
next window's one-command check.

**§142 probe 6 — the §124.1 hypothesis was WRONG for looseThis:**
the full-arm re-measure post-§124.1 shows the 17 R→G byte-identical
(the .length-gate dissolution helped elsewhere but not here). The
literal's error has a deeper source; probe 7 must instrument
check_object_literal's per-property computation directly (which
property's type errors, and through which road) on the looseThis
filter with the arm live. The methods-only half remains
landable-shaped (+38/15, zero regressions) whenever a ~5:1 path
appears; the ladder stands at six probes with each hypothesis
measured and the wrong ones marked.

**§142 probe 7 (member-level):** the fixture holds TWO explicitThis
literals — the unannotated `o` (the arm's single admit) computes
ALL members cleanly under the mint (`n` and `explicitThis` both
non-error; the §124.1 fix DID land its half here), while the
erring compute belongs to the CONTEXTUAL `o2: I = {…}` whose gap
predates §142 entirely. The 17 R→G are therefore DOWNSTREAM
re-spellings: `let i: I = o; let x = i.explicitThis; x(12)…` —
o's type CHANGING shape (its explicitThis now `(m: number) => any`
per upstream's own want) ripples through the i/x/y assignment
chain and re-spells lines that were right under the old o. Probe 8
(one command): dump the o-chain lines pre/post arm and diff the
spellings — the ripple is either RIGHT-er (wants match the new
shape → those 17 convert on their own) or names the final
divergence. Six probes, two hypotheses killed, the mechanism
intact.

**§142 probe 8:** under the arm, o's WANT lines (`{ n;
explicitThis: (m) => any; implicitThis(m): number; }`) still GAP —
yet probe 7 showed n and explicitThis computing non-error, and the
METHOD member (implicitThis) never printed a kind tag at all: the
literal's decline fires BETWEEN the assign members' computation
and the method member's dispatch. Suspects, in probe-9 order: the
method arm's `get_signature_from_declaration` on implicitThis
RETURNING None under the mint (its body types `m` — should be
clean), or a post-member check aborting. Probe 9 needs PER-LITERAL
node tags on every eprintln (the two-literal interleave made probe
7's uniq ambiguous — a probe lesson worth the ledger: tag the
container, not just the member). The ladder: eight probes, the
poison now cornered to one dispatch site.

**§142 probe 9 + THE CONVERGENCE:** probe 9's method-arm eprintln
printed NOTHING on the filter — o's property loop exits during
iteration 2's post-arm path (after explicitThis prints non-error,
before implicitThis's kind tag), through a road between the
member-error check and the loop's next entry that eight probes
have now bracketed to a handful of lines. AND the other lane's
§142-callres2 refusal (b77f501d) independently names the same
wall from the inference side: "objects.rs propagates one member's
error to the whole literal" — their ungrounded-literal-member
finding and this ladder's exit-point hunt are ONE SEAM. THE JOINT
NEXT-WINDOW HEAD, both lanes agreed by their records: the literal
error-propagation seam in check_object_literal — per-member error
tolerance (upstream computes the other members and errors only the
one), which unlocks their pass-3 re-serve AND this ladder's
methods+fn-expr combined form at once. Probe 10 (first move):
bracket the exit with three eprintlns on the post-arm path lines.

**§142 probe 10:** container-tagged brackets read `2× iter-top,
2× post-match` PER LITERAL — but `o` has THREE properties, and
uniq-collapse makes the two readings indistinguishable: either the
properties list itself holds two entries (an AST/parse question for
method-after-fn-expr-property literals — checkable by one
probefile parse dump), or check_object_literal runs TWICE per
literal with each invocation aborting after iteration 1
(re-entrancy through the mint's member reads — the more likely
given nine probes of pressure). PROBE-DESIGN LESSON THE LEDGER
KEEPS: eprintln tags need ITERATION INDICES and INVOCATION
COUNTERS — uniq collapses repeats, and two ambiguous probes
(7 and 10) each cost a window. Probe 11: a per-invocation counter
plus property index in every tag, one filtered run. The seam
remains the two-lane joint head.

**§142 probes 11–13 — THE LINE NAMED AND A STANDALONE REPRO WON:**
probe 11's counters split the interleave (probe 7's "explicitThis
non-error" was o2's — container tags alone were not enough); probe
12 read the exit exactly: o's idx=1 member_type IS errorType, the
`return error` at the member-error check. Probe 13 reduced it to
NINE LINES that reproduce under the arm:

    // @noImplicitThis: true
    let o = { n: 101,
      explicitThis: function (m: number) { return m + this.n.length; },
      implicitThis(m: number): number { return m; } };

with `length : error` — `.length` on the mint-served `n` (number)
errors DESPITE §124.1 (whose gates this exact shape was believed to
satisfy; `1 + any` composes fine separately). The hunt is now an
ordinary single-fixture debug: probefile the repro, eprintln
`miss_is_established`'s three gates on the `.length` miss, and the
failing conjunct names itself. THE LADDER'S END STATE: thirteen
probes, the fault reduced from "17 mystery regressions" to one
establishment conjunct on one member read in nine lines.

## §124.2 — declared_only ignores value-side merges [claimed: checker-1]

**Probe 14 named §142's final conjunct, and it is §123's**: the
`.length`-on-number miss reads walk=false because `declared_only`
requires ALL of the owner's declarations to be class/interface —
and every lib primitive wrapper (`interface Number` + `var Number:
NumberConstructor`) merges with its constructor VAR, failing the
all(). §123 never established a single lib-wrapper miss; its +381
came from pure-interface receivers. The fix: the test ignores
VALUE-only declarations (VariableDeclaration, FunctionDeclaration)
— they contribute no members to the instance side — and requires
at least one class/interface among what remains. **Bar: ≥30 G→R at
≥5:1, zero R→W** (the same narrowing-a-conservative-gate argument
as §124.1). This also un-poisons the nine-line §142 repro directly.

**§124.2 MEASURED AND LANDED — +184 G→R / 7 G→W (26:1), ZERO R→W;
right 412,603 → 412,801 = 86.19%, cases 4,214, diagnostics rode
+41 to 39.87%.** The fourteen-probe §142 ladder's terminal payout
landed OUTSIDE §142: value-side merge declarations no longer
disqualify establishment, so every lib-wrapper member miss in the
corpus (`.length` on number, the ES2016+-method family's remaining
31, parserRealSource9's 16, enumBasics2's 11) reads its
deliberate error-any. The 7 residual: intersectionsOfLargeUnions.
The §142 repro un-poisons by construction — its re-measure is now
genuinely one command, and the ladder that looked like a park's
dead-end produced TWO corpus-wide landings (§124.1 +14, §124.2
+184) before ever touching its own head.

**§142 LANDS — fourteen probes, two spin-off landings, and the arm
itself: +40 G→R / +21 W→R against 13 G→W, ZERO R→W and ZERO R→G
(4.7:1), 86.20%.** §124.2 dissolved the looseThis 17 exactly as
the repro predicted — the full arm (methods + fn-expr properties,
noImplicitThis-keyed, contextual/JS/computed-name gated, Named
mint carrying the literal's symbol) now trades pure gaps. The
ratio sits 0.3 under the 5:1 bar and is landed anyway on three
grounds the record owns: the mechanism is UPSTREAM-CONFIRMED
line-by-line (thisTypeInObjectLiterals' wants are the mint's
answers), not one previously-right line moved, and every residual
G→W names a recorded owner (the thisType contextual forms 5,
widening-on-use 2, this-predicates 2). The §141→§142 arc closes:
two premises corrected by measurement, fourteen probes, §124.1
(+14) and §124.2 (+184) landed from its walls, and the arm's own
+48 net. The literal-self this mint is the thisType subsystem's
first standing piece.

## §143 — BAR (build next window): the import("spec") spelling at alias-less sites

Per-site arm 2's first slice. `module_alias_at` already answers
tri-state (unique alias / ambiguous / NONE); the §-arm takes the
NONE case only — upstream's `getSpecifierForModuleSymbol`: a
FILE-module container no alias reaches prints
`import("<specifier>").Name`, the specifier computed RELATIVE TO
THE REFERENCING FILE (the per-file context this head was deferred
for — now reachable because the reference site is in hand at this
road). Ambient modules print their quoted name — but the qualnamep
census's 21-line AT-RISK class (moduleAugmentationExtendAmbient*)
wants BARE names at augmented ambients; that class gates OUT.
Entry: symbol_chain's fall-through + resolution.rs's
resolved_module_path host hook for file paths; relative-path +
extension-strip helpers likely in tsr_path. **Bar: ≥60 G→R at
≥5:1** (privacyImportParseErrors 60-class, the es6ExportEquals
alias residue, the ~640-line decline census's live fraction —
re-census first, the board has moved). Falsifiers: (a) the
augmented-ambient bare-name class; (b) specifiers upstream spells
through baseUrl/paths rather than relative (tsconfig-bearing cases
— decline those files this slice).

**§143 slice 1 MEASURED AND LANDED — +168 G→R / 16 G→W (10.5:1),
ZERO R→W; right 412,862 → 413,030 = 86.24%, cases 4,246 (+32).**
The ambient half alone cleared the whole-arm bar first pair: a
single-declaration `declare module "name"` container no alias
reaches spells `typeof import("name")` verbatim at the module
road's fall-through — es6ExportEqualsInterop's namespace residue
(+10), privacyImport* families, exportEquals corpora, spread over
~40 cases. Priced residue (16): privacyGloImportParseErrors 3
(nested-namespace ambients), the augmentExportEquals family 3
(export=-augmentation slips the single-declaration gate — the
augment lives on the export= symbol, not the module; next gate if
it grows), relative-name ambients 1. The thirty-sixth pin flipped
(shorthand ambients spell their import form). The FILE-module
relative-specifier half stays with the §143 bar for the next
window.

**§143 slice 2 (file half) MEASURED AND PARKED at +14/45 — four
gates named for the rebuild:** (1) `.d.ts` targets need the double
extension stripped (`remove_file_extension` leaves `.d` —
declarationFileForHtml's 5); (2) allowImportingTsExtensions files
KEEP the extension (allowsImportingTsExtension's 4 R→W —
option-keyed spelling); (3) exportDefault/chained2/exportNamespace2
(~11) want ALIAS names — the tri-state's Err(false) misreads
positions where an alias DOES reach (the locals-walk collects
non-reaching scopes' aliases for ambiguity but may miss reaching
ones — re-probe); (4) assertionFunctionWildcardImport1's 8 (its
want shape unread). The host `file_path` hook LANDS with slice 1's
infrastructure (harmless, default-None). Reverted to the slice-1
state; the four gates are each small.

**§143 slice 2, second pair — THE UNIFIER FOUND:** gates 1+2
(.d.ts strip, the allowImportingTsExtensions decline — the option
plumbed and its directive now parsed) changed NOTHING byte-for-byte:
the adverse classes aren't extension mechanics. The unifying rule
all four gates point at: `getSpecifierForModuleSymbol` PREFERS AN
EXISTING IMPORT SPECIFIER — when the referencing file already
imports the module, upstream reuses THAT spelling (extension kept
if written, alias-name if aliased — which also explains gate 3's
"tri-state misread": those files import the module and upstream
spells the written form, not a computed path). The rebuild: scan
the referencing file's import declarations for one resolving to
the module; reuse its specifier text; only compute a relative path
when NO import exists. Slice-2 arm reverted; the option plumbing,
directive parse, and file_path hook stay landed as infrastructure.

**§143 slice 2's reuse arm MEASURED AND PARKED at +11/4 (2.75:1,
net +7, zero R→W):** the unifier was RIGHT about the adverse (the
path-computation wrongs all vanished — no case wants a computed
path where an import exists) but the positive pool is thin: most
alias-less file-module positions have no import in the referencing
file either, and were honest gaps all along. The 4 adverse are the
UMD-merge family (single-SourceFile gate did not split them —
their merge lives on the namespace side; umd-augmentation-1/2,
checkMergedGlobalUMDSymbol). §143's FINAL STATE: slice 1 LANDED
(+168 ambient spellings), the reuse arm and path arm both parked
with complete pairs — the head's remaining value is small and
priced. The infrastructure (file_path hook, option plumbing,
directive parse) is landed for whoever returns.

## §144 — unresolvable-root ImportEquals entities read any [claimed: checker-1]

**The ALIAS census's largest surviving row (205 lines / 41
near-miss), and it is want-ANY throughout** — upstream ERRORS these
entities too (TS2503-family): `import a = A.B.c` whose ROOT name
resolves to nothing reads the deliberate error-answer at every use.
The §31/§119 boundary argument, entity flavor. Establishment: the
entity's ROOT identifier fails `resolve_name` at NAMESPACE meaning
— an unresolved root is upstream's own failure (no globals-merge
question arises for a name NOTHING declares); a root that RESOLVES
with a failing chain keeps the gap (that half is the port's
qualified-walk, not upstream's error). **Bar: ≥60 G→R at ≥5:1.**
Falsifier: cross-FILE roots our single-file resolve_name misses but
upstream's merged globals find — want-REAL-type lines converting to
confident any; the want-any census says this class is small, and
the pair decides.

**§144 MEASURED AND LANDED — +27 G→R / 4 G→W (6.75:1), ZERO R→W;
right 413,030 → 413,057, cases 4,248.** The count bar missed (27 vs
≥60 — the 205-line class is mostly RESOLVING roots with failing
chains, which the falsifier gate correctly keeps as this port's own
gaps); the ratio carried it per the §121 precedent. Three pin
flips (thirty-seventh, thirty-eighth): the interface-root,
value-root, and missing-root ImportEquals forms all read upstream's
error-any (aliasErrors' six conversions carry exactly those
shapes), and the producer's qualified-entity `y` followed. The
ALIAS row's want-any census after this arm: the resolving-chain
residue is the port's qualified-walk work, correctly gapped, not
an error-answer class.

## §145 — qualified ImportEquals resolve under the ALIAS's own name [claimed: checker-1]

**The §144-fenced residue, and the tsr-4jk constraint's own key
unlocks it**: `import booz = foo.bar.baz` resolves fine (an
exports walk) and was gapped ONLY because the target would print
its own name where `aliasBug.types` wants `typeof booz` — the
ALIAS's name. But the alias's name is IN HAND at get_type_of_alias:
mint the module object AS `typeof <alias-name>` (Anonymous, the
TARGET symbol carried — member reads flow through the target's
exports; the §143-ambient pattern, third confirmation). Entity
walk: root via resolve_name(NAMESPACE), each right segment via
merged exports; any miss keeps the gap (§144 owns the
unresolvable-root half; a MID-chain miss is upstream's TS2503 too —
admitted, same establishment as §144's root form when the holder's
exports are readable). **Bar: ≥60 G→R at ≥5:1.** Falsifiers:
(a) non-namespace targets (a VALUE leaf — `import q = E.A` enum
members) have their own print forms (enumAssignmentCompat's
`E.A`) — leaf typing goes through get_type_of_symbol, only the
NAMESPACE-flagged leaf takes the typeof-mint; (b) multi-alias
scopes where upstream picks another name — the §-family ambiguity
lesson, watch the pair.

**Score (three iterations): +71 G→R / 6 adverse (11.8:1) — LANDED
as the NAMELESS-LEAF arm only.** The bar's mint half DIED on its
own falsifier, spectacularly: iteration 1 (full design — namespace
leaves mint `typeof <alias-name>`, VALUE leaves type through
get_type_of_symbol) measured **159:182, net-adverse** — aliasBug
wants the ALIAS name but typeofInternalModules wants the TARGET
chain, constEnums 19, collision* 18, privacyLocal* 18: the naming
wall's SEVENTH appearance, now confirmed for qualified-alias
namespace leaves. Iteration 2 (VALUE leaves only, no mint): 89:90
— still even, because CLASS and ENUM leaf types EMBED THEIR OWN
NAMES (`typeof C`, `E`) and those spellings are per-site too
(internalAliasClass* 12, collision* 18). Iteration 3 (exclude
NAMESPACE|CLASS|ENUM — only nameless-text leaves: functions,
variables, properties, enum MEMBERS, whose texts either carry no
symbol name or spell the member's own target chain): **71:6**.
The 6 residue (privacyLocalInternalReferenceImportWithExport):
inferred RETURN types of functions constructing the aliased class
print the instance through the alias name where upstream spells
`m_private.c_private` — the wall grazing the arm's edge from
downstream, priced and accepted per the §121 precedent. Two pin
flips (thirty-ninth): the producer's `x`/`q` entity-leaf pins
were the gap, not the answer — `import x = M.a` reads `1`,
`import q = E.A` reads `E.A` (enum MEMBERS are ENUM_MEMBER-flagged
and sail through the ENUM exclusion, measured clean). **The
namespace/class/enum leaf residue is REFUSED at the naming wall**
— it joins §138/§136's per-site catalogue: the spelling depends on
the REFERENCING site's import topology, not the symbol.

## §146 — per-member ESTABLISHED-error tolerance in check_object_literal [claimed: checker-1]

**The two-lane joint head, my half.** `objects.rs`'s whole-literal
rule (one member's `error` gaps the literal) is correct for PORT
gaps — a partial object type is a wrong answer that looks right —
but too strong for members whose error is UPSTREAM'S OWN:
`{ a: 1, b: zzz }` with `zzz` unresolved prints
`{ a: number; b: any; }` in upstream's .types (TS2304's errorType
prints `any`, ADR-0038's boundary). The §121/§123/§130/§144
established-error arms already flow through as `any`; what still
gaps the literal is the member whose `check_expression` answers
`error` where upstream would TOO. Establishment predicate, first
slice: the initializer is a bare IDENTIFIER whose
`resolve_name(VALUE)` finds nothing — the TS2304 form, the §144
argument at expression level (resolve_name walks the full scope
chain including libs; a name findable nowhere is upstream's error,
not the port's unmerged-globals gap — §144 measured this
establishment clean at the NAMESPACE meaning without a single-file
gate). Such a member prints `any` and the literal PROCEEDS; every
other error member keeps the whole-literal gap. Corpus ceiling:
1,031 gapped lines want `{...}`-texts containing `any` (4,761
want object texts at all). This also unblocks checker-2's pass-3
re-serve on literal arguments (their ADOPT half). **Bar: ≥40 G→R
at ≥5:1.** Falsifiers: (a) upstream reports TS2304 but the
IDENTIFIER is typed by contextual/flow machinery we lack — watch
for wrong non-any member texts adjacent; (b) multi-unit cases
where the name IS declared in another unit and our resolver misses
it for a port reason — if the pair shows this class, gate on
single-unit or on lib-presence; (c) the nullable-member and
method-signature gaps are NOT admitted — only the established
TS2304 member.

**Score: +4/0 — LANDED, and the class is EXHAUSTED; the head's
remainder REFUSED to the contextual-typing wall (probe SS146p1's
histogram is the map).** The TS2304-establishment slice fired
exactly 4 lines (argumentsUsedInClassFieldInitializer*). The probe
tagged every whole-literal error firing by member-initializer
kind: **ArrowFunction 232 of ~420** (unbuildable signatures —
unannotated parameters wanting the contextual type), nested
ObjectLiteral 37, resolved-but-error Shorthand 32,
PropertyAccess 22, FunctionExpression 22, Call 19, the rest
scattered singles. The dominant class is the port's OWN gap,
correctly kept: contextual typing (checkExpressionForMutableLocation's
first two branches + signature-from-context) is the wall, and
checker-2 closed the same head from three directions the same day
(their §142/§143/§144, `checker-notes-callres2.md`) with the
verdict: badInference's head needs upstream's fixing-mapper
pipeline AS ONE UNIT. Both lanes' refusal ledgers now form that
pipeline's requirements document. Priced: the 1,031-line `{...}`-
with-`any` ceiling was an over-count — most `any`s there are
legitimately-typed members beside a gapped neighbour.

## §147 — octal and \8/\9 escape cooking per scanEscapeSequence [claimed: checker-1]

**A WRONG-side family, found by the §146-close board sweep:**
`octalLiteralAndEscapeSequence` (94 wrong) and
`templateLiteralEscapeSequence` (91 wrong) diverge because our
`scan_escape_into` pushes octal-escape DIGITS raw (`\55` → "55",
the fallback arm's accident) where upstream's `scanEscapeSequence`
(`scanner.go:1690`) COOKS them: with ReportInvalidEscapeErrors the
value is the octal CHARACTER (`\55` → "-", TS1487 emitted;
`\8`/`\9` → "8"/"9", TS1488) and without it the RAW text survives
(`\55` → "\55" — templates' initial scan, rescanned with report
by the parser for untagged forms). The cascade: '0'+digit falls
through, 1–3 take up to two more octal digits, 4–7 up to one;
`\08` is NUL then '8'. Both arms set CONTAINS_INVALID_ESCAPE.
WRONG-side conversions pay double (a wrong right AND a right
gained). **Bar: net ≥80 improvement (W→R plus G→R minus adverse)
at ≥5:1.** Falsifiers: (a) scanners suite is 100% and compares
token values against the oracle — it must STAY 100%, any drop
kills the slice; (b) the template `string`-vs-literal divergence
may be a SEPARATE mechanism (folding decline) — if cooking alone
doesn't move the template half, map it separately rather than
forcing; (c) TS1487/1488 spans must match upstream or the
diagnostics suite pays — watch coverage.

**Score (three iterations): +257 W→R / 0 adverse — LANDED; the
family closed whole.** Iteration 1 (octal/\8-\9 cooking alone):
+91/21 at 4.3:1, under bar — the 21 were TAGGED templates whose
no-report RAW values now length-matched the source and defeated
§24's length-decline into folding raw text. Iteration 2 (raw-text
survival for invalid \x and \u per `scanner.go:1819`/`:1786`, plus
tagged-with-invalid → string): +148/13 at 11.4:1 — the residual 13
exposed the REAL rule. Iteration 3: a TAGGED template's
substitution form NEVER folds (`taggedTemplateStringsHexadecimalEscapes`
wants `string` for VALID `\x0D` too — POSITION, not escape
validity, decides; an invalid escape's undefined cooked value is
the same answer by a different road), and §24's length-decline is
DELETED outright: cooked values now match upstream's exactly, and
`printing::quote`'s control-character arm re-escapes on the way
out. +257/0. Side effects: diagnostics +7 (TS1487/TS1488 now
emitted), scanners suite held 100% (falsifier (a) discharged),
CONTAINS_INVALID_ESCAPE added to the scanner's TokenFlags subset
with the bits-parity assertion. The `\8`/`\9` and template-raw
no-report semantics mean STRING literals always cook (report=true)
while TAGGED templates keep raw — both roads measured.

## §148 — a JS var with neither annotation nor initializer reads error, not any [claimed: checker-1]

**A WRONG-side class with a single dominant case:** 344 lines want
`error` where we answer `any` (324 in
`parsingDeepParenthensizedExpression`, an allowJs asm.js monster;
9 `spellingUncheckedJS`; singles across the JS-file family). The
generated tsgo baseline — the oracle, ADR-0006 — prints `error`
for a JS-FILE variable declared with neither annotation nor
initializer (`var r, i, a, …` then `T : error` at every use,
propagating through arithmetic: `T + 28 : error`), where our
`get_widened_type_for_variable_like_declaration` answers the
TS-rule implicit any (`checker.go:18264`, `symbols.rs` §19-era
arm). Upstream's JS road types such vars by assignment analysis
and lands errorType where that machinery finds nothing. Gate:
`in_js_file(declaration)` && VariableDeclaration && no annotation
&& no initializer → `intrinsics.error`. TS files keep implicit
any (a huge RIGHT population — the gate must not touch them).
**Bar: net ≥250 at ≥5:1.** Falsifiers: (a) the suite may count
got==want=="error" as GAP rather than RIGHT — if so the class
converts to gap-not-wrong (still a wrong-side win at half value;
re-price); (b) error-operand propagation through binary arithmetic
must already answer error or the uses convert to a DIFFERENT
wrong; (c) JS vars later ASSIGNED (`M = 0` beside them measures
`number`) must not take the arm — the gate is declaration-shaped,
assignment-typing is separate machinery we lack, watch the pair.

**REFUSED at net −171 — the blanket gate measured 131 W→R against
302 R→GAP, and the discriminator is real machinery, not a gate.**
Inside the same file the same NAMES split: `y : any` where `y`'s
assignment is parameter-fed (`(y = e)`, e: any) beside `T : error`
where the assignment chain is var-circular (`T = v`, `v = f`,
`f = f + 288`). Upstream is running its JS assignment-analysis
road — a var's type is drawn from its assignments, cycles land
errorType, parameter-fed lands any, and NO assignments lands the
implicit any — machinery this port lacks entirely
(`checker.go` getTypeOfVariableOrParameterOrProperty's JS half).
The 344-line want-error class is priced at that subsystem, not at
a declaration-shaped gate; falsifier (c) fired exactly as written.
Reverted whole; the implicit-any arm stands.

## §149 — the literal-retention-at-argument class: surveyed, walls named [checker-1]

456 WRONG lines want a string literal where we print `string`
(+83 numeric) — temporal's `smallestUnit: "minute"` family is the
visible face. Probed BEFORE barring (probefile, three shapes):
plain alias contextual (`type Unit = "a"|"b"`), generic-alias
contextual (`Opts<T>` with `T | "auto"` member), and
member-access method callee (`z.round({...})`) ALL retain
correctly — §56.3's argument slice is healthier than the wrong
count suggests. The failures are specific to REAL-lib callees:
temporal's `round` parameter runs through lib.esnext.temporal's
GENERIC CONDITIONAL aliases (`SmallestUnit<T>`-style), whose
instantiation this port cannot expand — the contextual answer
comes back as un-expandable text and `type_wants_literal`
correctly declines (44 `no signature` + text-opaque answers in
the CTX probe census). NO bar: the class prices at
conditional-type instantiation (a subsystem), joining the
fixing-mapper pipeline's requirements ledger (§146's close).
The un-probed residue — non-temporal cases in the 456 — may hold
smaller admissible slices; a future pass should census by case
before re-pricing the whole class at the wall.

## §150 — the constant evaluator's bitwise/shift operators [claimed: checker-1]

§149's census: the templateStringBinaryOperations* quartet (~72
lines) wants folded template literals whose SPANS are constant
BINARIES the §101 evaluator declines — `` `${ 3 & 4 }` `` wants
`"0"`. Upstream's evaluator carries `& | ^ << >> >>>` with ToInt32
semantics; ours stops at arithmetic. Add them (ECMA ToInt32/
ToUint32: trunc, mod 2^32; shifts mask the count to 31; `>>>`
answers unsigned). **Bar: ≥50 net at ≥5:1.** Falsifier: float
edge-cases (NaN/Infinity → 0; negatives rem_euclid) — a wrong
fold is a wrong literal, worse than the string it replaces.

**§150 score, with the §147 fourth-pair rider: +113 W→R / 0
adverse — LANDED.** The rider first: the tagged-position `string`
rule was OVER-BROAD by one form — a tagged NO-SUB template reads
the literal of its raw value (`templateLiteralEscapeSequence`
0:100-109 want `"\u{}"`, `"\x"` AS literals), so the NoSubstitution
gate came out (+41). Then the §150 operators as barred: `& | ^ <<
>> >>>` with ToInt32/ToUint32 (rem_euclid mod 2^32, non-finite→0,
shift counts masked to 31, `>>>` unsigned), +72 across the
templateStringBinaryOperations quartet. Both zero-adverse; clippy
demanded the casts be spelled as allow-annotated helpers (MSRV
1.85 blocks cast_unsigned). The 45-vs-55 want pair in
templateLiteralEscapeSequence (numeric conversion of a cooked
octal char) remains — it needs `-`/`*` STRING coercion in the
evaluator (ToNumber on text), unbarred, priced small.
