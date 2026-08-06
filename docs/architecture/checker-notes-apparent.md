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

## 4. Scored — and leg 1 FIRED first, on a real defect

**First run: net 0.** Leg 1's floor was +60 and the arm converted nothing.
Diagnosed by the rule (build wrong first): the arm tested
`TypeData::Named { members: Some(symbol) }` to get back to the type
parameter's symbol, and `new_named_type` (`declared.rs:1044`) sets
`members: None` for a type parameter **deliberately** —

> A class or interface owns its members; a type parameter and an enum do not,
> and pointing them at a members table they do not have would be a lookup that
> silently succeeds against the wrong symbol.

That comment is right, and it also means there is no route from the `TypeId`
back to the symbol at all. The fix is the `type_reference_targets` precedent
for the third time: a `type_parameter_symbols` side table written where the
type is minted (`declared.rs:856`), read by the head. **Not** a widening of
`TypeData::Named`'s `members`, which would make one field mean two things and
undo the safety property the comment describes.

> **A bar that fires with a *zero* is the cheapest kind to diagnose.** There is
> no ratio to argue about and no trade to price: the mechanism did not run.
> The first question is always "did the code I wrote execute", and a net of
> exactly 0 answers it.

### Final measurement

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ +60 | **+156** (22 cases) | pass |
| 2 | lost ≤ 5, diagnosed | **0** | pass |
| 3 | regressions < finished | **0 < 6** | pass |
| 4 | gained ≥ 3 × new wrong | **156 vs 14 = 11.1×** | pass |

**Falsifier did not fire**: top case `protectedMembersThisParameter` at 21 of
156 = 13.5%, against the 50% line.

**KEEP.** Gradient 70.27% → **70.30%**. Conversion **159% of the forecast 98**
— and the head of the gain is a shape the counterfactual under-counted: a
`this` **parameter** annotated with a type parameter (`function f(this: T)`),
which is the `T` rule reached through a receiver that prints as `this`.
