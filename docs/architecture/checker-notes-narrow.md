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