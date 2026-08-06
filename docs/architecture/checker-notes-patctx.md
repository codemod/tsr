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
