# Checker notes — nullable receivers and optional chains in access positions

Fourth session, 2026-08-06, at `f436c9d`. Element-access remainder work
(STATUS.md §4.2) led here via `examples/elemgap.rs` → `examples/nnaccess.rs`.

## 1. The population, sized by the mechanism

`nnaccess.rs` (committed with this page): **704 gap lines** whose node is an
access (or a member name) and whose receiver types today as a union carrying a
top-level `null`/`undefined` constituent. `want-any` is **21 (3%)** — the
cleanest want profile of any row this session. Two halves:

```
  356  non-optional access on a nullable receiver   (x.a where x: Thing | undefined)
  348  optional chains                              (x?.a, x?.[i], and inner links)
```

Top receivers: `Thing | undefined` 270, `string | undefined` 49,
`Thing | null` 44. **Concentration flag: 403 of 704 sit in
`conformance/controlFlowOptionalChain`** — the falsifier below names it.

## 2. The upstream mechanism, read before writing

- `checkNonNullExpression` (`checker.go:7405`) = `checkNonNullType(checkExpression(e), e)`:
  `unknown` in strict mode → `errorType`; a type with undefined/null facts →
  report a **diagnostic** (separate channel, ADR-0040) and answer
  `GetNonNullableType(t)` (`checker.go:18663`, the `NEUndefinedOrNull` facts
  filter); if the remainder is still nullable or `never` → `errorType`.
- Chains (`checkPropertyAccessChain`, `checker.go:11253`;
  `checkElementAccessChain`, `:8140`): the receiver goes through
  `getOptionalExpressionType` (`:29064`) — chain **root** strips nullable,
  inner link removes the propagated marker — then the same lookup, then
  `propagateOptionalTypeMarker` (`:29082`): if anything was stripped, the
  outermost chain link unions `undefined` back in (`getOptionalType`,
  `:18640`), an inner link re-adds the marker.

**Divergence, stated:** upstream's marker is `optionalType` — an `undefined`
distinct from the real one, so an inner link removes exactly the marker. This
port has one `undefined`. Removing it at an inner link can also remove a
*real* `undefined`, but on that shape upstream's own `checkNonNullType` strips
it anyway two lines later (with a TS18048 diagnostic this port does not emit),
so the **type answers coincide**; only the diagnostics channel differs, and
this port has none.

## 3. The keep/revert bar — registered before the code

1. **net ≥ 200** (~29% of the 683-line net population; mid-band, conditioned
   on the receiver already typing);
2. **lost == 0**, denominator empty by construction: every access through a
   nullable receiver misses the lookup today, so no right line rides the
   changed path. Any loss is a cascade bug — diagnosed, not priced;
3. **0 case regressions**;
4. **gained ≥ 3 × new wrong** (`wrongdelta`) — the live leg. The known risk:
   a chain result that upstream *narrows* after the access, where this port
   answers the un-narrowed union.

**Falsifier:** `controlFlowOptionalChain` holds 57% of the population and
mixes chains with flow narrowing. If its lines convert to *wrong* rather than
right — the access answers, then the narrow this port lacks flips the line —
leg 4 fires concentrated in that one case, and the item's real owner is
narrowing, not access.

## 4. Scored — all four legs pass, and the falsifier fired as designed

`casedelta`/`wrongdelta` over the pair at the `t[0]` commit:

| leg | rule | measured | verdict |
|---|---|---|---|
| 1 | net ≥ 200 | **+590**, 0 lost | pass |
| 2 | lost == 0 | 0 | pass |
| 3 | 0 case regressions | 0 | pass |
| 4 | gained ≥ 3 × new wrong | **590 vs 129 = 4.6×** | pass |

Conversion ran **86% of the sized 683** — above the band, as §3 predicted for
a population conditioned on the receiver typing.

**The §3 falsifier fired exactly as named**: 106 of the 129 new wrong lines
sit in `controlFlowOptionalChain`, and their shape is the predicted one —
want `42` / `"abc"` (the narrow upstream applies *after* the access), got the
un-narrowed lookup result (`string | number`). The access arm is right; the
owner is the flow matcher, which does not treat an optional-chain access as a
matching reference form. Filed as `bd tsr-97d` with the per-case split, so
the 106 lines are attributed the day they are converted rather than
re-diagnosed.

Two mutations bite disjointly (never-propagate-the-marker; disable-the-strip)
and are restored; the expectations in `tests/types.rs` are
`elementAccessChain.types` and `narrowingOfQualifiedNames.types` verbatim,
including the pair rule for the plain-nullable and non-nullable-chain forms.
