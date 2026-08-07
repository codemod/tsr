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
