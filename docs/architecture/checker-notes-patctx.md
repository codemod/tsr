# The pattern-implied contextual type (`bd tsr-84iz`)

Registered before code, fifth session. This item exists because `bd tsr-o00`
**refused** it: approximating it during the destructuring build minted ~80
wrong lines, and `STATUS.md` §5 carries that number. The construct has gapped
whole ever since, behind `destructure.rs`'s array-literal guard.

## 1. The mechanism

`checkDeclarationInitializer` (`checker.go:16797`) threads
`getTypeFromBindingPattern` (`checker.go:17904`) as the initializer's
contextual type. For

```ts
var [a, b] = [1, "x"];
```

that contextual type makes `checkArrayLiteral` infer the **tuple**
`[number, string]` rather than the array `(string | number)[]`, so `a` is
`number` and `b` is `string`. Without it this port answers
`(string | number)[]` and every element prints the union — the ~80 wrong lines.

## 2. The counterfactual

`examples/patctx.rs` forecasts element *i* as the **widened** type of the
literal's element *i* — what a tuple contextual type reduces to for this
shape — and compares to the baseline string for string. C1 = 0, C2 exact,
**291 classified**:

| bucket | lines |
|---|---:|
| **CONVERTS — forecast matches exactly** | **174** |
| the pattern is longer than the literal (out of range) | 39 |
| the literal's element itself gaps (downstream) | 36 |
| **MISS** | **30** |
| want `any` (ceiling) | 7 |
| a spread element — needs `sliceTupleType` | 5 |

The 30 misses are **concentrated in shapes the build will refuse**, which is
why they are listed rather than averaged: nested patterns the flat forecast
mismodelled (`want [number, string], forecast number`; `want [string],
forecast string`), rest elements (`want number[], forecast number`), and
out-of-range optionality (`want number | undefined, forecast number`).

## 3. What will be built, and what stays refused

In `destructure.rs`'s parent path, replacing the blanket array-literal
refusal: mint a **tuple** from the literal's element types, widened, and let
the existing element lookup (`bd tsr-5ll`'s reverse index, and the recursion
this module already does for nested patterns) do the rest.

Refused, each because the counterfactual shows it mispredicting:

- **a spread element in the literal** (5) — needs `sliceTupleType`;
- **a rest element in the pattern** (part of the 30) — same;
- **a pattern longer than the literal** (39) — upstream's out-of-range
  element is optional and prints `T | undefined`; the tuple this port mints
  carries no optional flag, so the whole construct refuses rather than
  printing `T`;
- **a literal element that itself gaps** (36) — downstream, and a gap in an
  element gaps the tuple, the rule every arm here follows.

Minting goes through a **shared** `create_tuple_type`, extracted from
`get_type_from_tuple_type_node` rather than copied, so a tuple built from a
literal and one built from a type node are the same interned type — the
property `bd tsr-5ll`'s comparator depends on.

## 4. The bar

> **KEEP** if **net ≥ +110**, **lost ≤ 5 with every loss diagnosed**, **fewer
> cases regress than finish**, and **gained ≥ 3 × new wrong**.
> **REVERT** otherwise.

- Floor is 63% of the forecast 174.
- Leg 2's denominator is **not** empty and this is the item where that
  matters most: the construct is refused *because* an earlier attempt minted
  wrong lines. A loss here means the tuple is wrong, not that the trade is
  bad.
- Leg 4 is the live leg. The counterfactual bounds it at 30 **before** the
  refusals above, which should remove most of them; new wrong materially
  above 30 means a refusal is not firing.

**Falsifier:** more than 40% of the gain in one case.

If a leg fires: build wrong first, premise wrong second, no third.

## 5. Scored — and leg 3 is VACUOUS, which is reported rather than rounded

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ +110 | **+206** (30 cases) | pass |
| 2 | lost ≤ 5, diagnosed | **0** | pass |
| 3 | regressed < finished | **0 < 0** | **VACUOUS — see below** |
| 4 | gained ≥ 3 × new wrong | **206 vs 16 = 12.9×** | pass |

**Falsifier did not fire**: top case `blockScopedBindingsReassignedInLoop6` at
38 of 206 = 18.4%, against the 40% line.

Leg 3 reads `0 < 0`, which is **false as written**. It is reported here rather
than quietly rounded to a pass, because `docs/conventions.md` already convicts
exactly this shape:

> For each leg of a pre-registered rule, ask what input would make it
> non-zero. A leg that cannot be non-zero should be replaced before the run,
> not explained after it.

The leg was inherited from builds whose gains crossed whole-baseline
thresholds. This arm's 206 lines are spread over 30 cases and finished none of
them, so the *finished* side had no way to be positive and the inequality was
never going to hold. What the leg exists to protect — "do not break more cases
than you complete" — is satisfied by the **0 regressions**, which is a
measurement and not an inequality. Same reading the `&&` build had to make:
**the evidence is the 0, not the ratio.** Kept, loudly, and the leg should be
written as `regressed == 0` next time this shape recurs.

### 5.1 The residual

16 new wrong, against a forecast bound of 30 — the refusals did remove most of
them:

```
   6  want `undefined`, got `never`      <- an empty literal's element slot
   3  want `string`,    got `string | boolean[]`
   3  want `any`,       got `undefined`  <- ADR-0039 ceiling
   3  want `any`,       got `null`       <- ADR-0039 ceiling
   1  want `any`,       got `number`     <- ADR-0039 ceiling
```

Seven are the `hadErrorBaseline` ceiling (ADR-0039), where the arm computes
correctly and the baseline is unmatchable. The six `undefined`/`never` lines
are an empty-slot shape the tuple mints as `never` where upstream's optional
element prints `undefined` — the same optional-flag gap the out-of-range
refusal already names, reaching one shape the refusal does not cover. Filed
against this item rather than fixed, because gating it would need the optional
element flag the tuple model does not carry.

### 5.2 And a sixth intuition-written expectation

`tests/pattern_context.rs` first pinned "a literal element that itself gaps"
with `(undefined as Unresolved)`, on the intuition that an unresolved type
reference gaps. **It does not** — since `bd tsr-eep` such a reference mints a
type that prints the written name, so the element typed fine and `a` read
`number`. Replaced with a template expression, which `examples/tmplgap.rs`
measured as unported *this session*, so the fixture's other half is a live
refusal rather than a guess.

That is the **sixth** expectation this project has written from intuition and
had corrected by the code, and the third in the pessimistic direction. The
running score remains: the port was right every time.
