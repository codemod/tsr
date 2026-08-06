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
