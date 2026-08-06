# `getApparentType`'s instantiable head (`bd tsr-rppd`)

Registered before code, fifth session.

## 1. The mechanism

`getApparentType` (`checker.go:21729`) opens with a step this port does not
have:

```go
originalType := t
if t.flags&TypeFlagsInstantiable != 0 {
    t = c.getBaseConstraintOfType(t)
    if t == nil { t = c.unknownType }
}
```

`members.rs`'s `apparent_type` ports the five **primitive** arms of the switch
*below* that head and not the head itself, so a member read off a type
parameter or a `this` type finds nothing at all. Both shapes are a `Named`
type carrying `TypeFlags::TYPE_PARAMETER` — `declared.rs:856` mints one for a
type-parameter symbol, `expressions.rs:465` mints the `this` type carrying the
**class** symbol.

## 2. The counterfactual, and what it refuses

`examples/apparent.rs` resolves the constraint, performs the lookup, and
compares the printed answer to the baseline. C1 = 0, C2 exact, **1,065
classified**:

| bucket | lines |
|---|---:|
| `this`: member found, its own type gaps (downstream) | 522 |
| `this`: the constraint has no such member | 305 |
| `T`: the constraint has no such member | 116 |
| **`T`: CONVERTS — forecast matches exactly** | **98** |
| `T`: no constraint — apparent type is `unknown`, stays a gap | 16 |
| `T`: MISS | 4 |
| `T`: member found, its own type gaps | 4 |

**The `this` half converts zero, and the row was misleading.** `depend.rs`
reports ~225 lines as `the receiver has no such property: this`, which reads
like an apparent-type failure and is not one: with the head ported, 522 lines
*find* the member and gap on its own type, and 305 do not find it on the
class's declared type at all. That second bucket is unexplained and is filed
as its own question — a class's declared type missing a member its own body
accesses is a members-table finding, not an apparent-type one.

So the item is **98 lines**, not 445. Third time this session a row has been
much smaller than its `depend.rs` heading once the mechanism was asked
directly.

## 3. The bar

> **KEEP** if **net ≥ +60**, **lost ≤ 5 with every loss diagnosed**, **fewer
> cases regress than finish**, and **gained ≥ 3 × new wrong**.
> **REVERT** otherwise.

- Floor is 61% of the forecast 98.
- Leg 4's forecast bound is **4** — the named miss is
  `want () => T, forecast () => this`, a substitution the arm does not do.
- An unconstrained `T` becomes `unknown`, whose lookup finds nothing, so it
  stays a gap exactly as today: leg 2's denominator is the *cascade* only.

**Falsifier:** more than 50% of the gain in one case (top case in the
counterfactual holds well under that).
