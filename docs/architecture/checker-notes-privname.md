# Private-name property access — `this.#x` (`bd tsr-0opd`)

Registered before any code, fifth session.

## 1. The row, and why it is exactly one construct

`depend.rs` at HEAD reports **551 gap lines** under
`property access, the name is not an identifier`. That reason is emitted by
`types_producer.rs:958` when `access.name` fails to match
`MemberName::Identifier`, and `MemberName` has exactly two variants —
`Identifier` and `PrivateIdentifier`. So the row is **private names**, whole,
with no mixture to split.

## 2. The mechanism, and why it is small

Upstream reaches a private name through `checkPropertyAccessExpression` →
`getPropertyOfType` with the private identifier's text, after
`lookupSymbolForPrivateIdentifierDeclaration` establishes the name is in
scope. The interesting fact here is that **this port's binder already does the
declaring half**: `binder.rs:3995` maps `PropertyName::PrivateIdentifier` to
its text, and `PrivateIdentifier.text` carries the leading `#`, so a
`#x` field is filed as a member literally named `#x`.

What is missing is the *access* half — `binder.rs:3800` answers `None` for a
`MemberName::PrivateIdentifier`, and `members.rs` has no arm — so the lookup
that would already succeed is never attempted.

## 3. The counterfactual, which is the sizing

`examples/privname.rs` does not count the row. It performs the lookup the arm
would perform and compares the printed result to the baseline, string for
string. C1 = 0, C2 exact. Of **406 classified** lines (those whose receiver
types today):

| | lines |
|---|---:|
| **CONVERTS — forecast matches the baseline exactly** | **295** |
| the receiver has no such private member | 50 |
| want `any` (ceiling) | 29 |
| the member is found but its own type gaps (downstream) | 21 |
| **MISS — forecast differs** | **11** |

The 50 "no such member" are the arm's safe failure: it answers `errorType`
there, so they stay gaps and cannot become wrong lines. Their heads are
`#foo` on `any`, on `this`, and on `typeof Child` — a static private accessed
through the constructor side, which needs the static member table.

The 11 misses are named in advance: `want number, forecast any` (3),
`want { foo?: string; } | undefined, forecast any` (3),
`want number | undefined, forecast any` (2), `want number, forecast string`
(2).

## 4. The bar

> **KEEP** if **net ≥ +200**, **lost ≤ 10 with every loss diagnosed as a
> cascade**, **fewer cases regress than finish**, and **gained ≥ 3 × new
> wrong** by `wrongdelta`.
> **REVERT** otherwise.

- The floor is **68% of the forecast 295**, not of the 551 row: the
  difference between them is receivers that gap today, which this arm does not
  reach, and a floor resting on those would rest on a guess.
- Leg 2's denominator is **not** empty: the arm answers a type where the port
  answered `errorType`, so a consumer of the newly typed access can compute
  confidently and wrongly. That is what the leg watches.
- Leg 4 is the live one, and the counterfactual bounds it at 11 in-family
  misses. New wrong beyond ~11 means the arm reaches lines the forecast did
  not describe.

**Falsifier:** more than 35% of the gain in a single case. The counterfactual
is spread across many cases with no dominant head, so heavy concentration
means the mechanism found something else.

If a leg fires: build wrong first, premise wrong second, no third.

## 5. Scored against §4's bar — all four legs pass

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ +200 | **+590** (71 cases) | pass |
| 2 | lost ≤ 10, diagnosed | **0** | pass |
| 3 | regressions < finished | **0 < 21** | pass |
| 4 | gained ≥ 3 × new wrong | **590 vs 55 = 10.7×** | pass |

**Falsifier did not fire**: top case `privateNameFieldUnaryMutation` at 132 of
590 = **22.4%**, against the 35% line.

**KEEP.** Gradient 70.15% → **70.27%**, cases 2,564 → **2,585**. Conversion is
**200% of the forecast 295**, the cascade again — a typed `this.#x` un-gaps the
expressions built on it.

### 5.1 The 55 new wrong were forecast, and mostly are not defects

The registration said *"new wrong beyond ~11 means the arm reaches lines the
forecast did not describe"*. It read 55, so they were read:

```
  26   want `any`, got a real type   <- the counterfactual's own want-any bucket
  11   the forecast's named misses (want number/got any, and friends)
  18   misc, same families
```

The **26** are ADR-0039's `hadErrorBaseline`, arriving exactly where it must:
using a private name outside its declaring class is an *error* upstream, so
those cases carry an `.errors.txt`, which disables the writer's fast path and
prints every type in the file through the node builder as `any`. The arm
computes the correct type and the baseline is unmatchable — the same ceiling
ADR-0038/0039 record, and **the counterfactual had already counted them**
(29 lines in its `want any (ceiling)` bucket, which necessarily become wrong
lines the moment the arm stops gapping them).

So the honest reading of leg 4 is better than 10.7×: of 55 new wrong, ~26 are
a ceiling nobody can convert and were sized in advance. This is why the
registration named the bound rather than only the ratio.
