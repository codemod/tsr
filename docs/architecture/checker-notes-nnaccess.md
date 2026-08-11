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

## 5 — an optional chain asks the nullable question of a receiver that has already been stripped

> **Written for `checker-notes-diag2.md` and moved here.** It collided with a
> concurrent session's section number three times in one hour — §890, §901, §910
> were each taken between writing and pushing. This is the right home anyway:
> §2 above owns the one-`undefined` divergence that this section resolves, and
> the two are unreadable apart. The lesson is cheap and general — **a document
> two workstreams append to is a lock, and a section that belongs to a subsystem
> should live in the subsystem's file.**

**Found by the binary, not by the corpus, and then found *again* by the corpus.**
`tsr` on `~/dev/codemod/app/apps/nextjs` reported **276** errors, of which 125
were TS18048 and a further 68 + 26 were its TS18047/TS18049 twins. Two the user
quoted:

```ts
logs: result.logs?.map(truncateString),   // TS18048: 'result.logs' is possibly 'undefined'
const lowerStage = stage?.toLowerCase();  // TS18048: 'stage' is possibly 'undefined'
```

Both are `?.`. Neither is an error in any TypeScript.

### The forcing constraint

`checkPropertyAccessExpression` (`checker.go:11249`) has two roads and this port's
diagnostic took only the first:

```go
if node.Flags&ast.NodeFlagsOptionalChain != 0 {
    return c.checkPropertyAccessChain(node, checkMode)
}
return c.checkPropertyAccessExpressionOrQualifiedName(
    node, node.Expression(), c.checkNonNullExpression(node.Expression()), …)
```

`checkPropertyAccessChain` (`checker.go:11253`) does **not** hand
`checkNonNullType` the receiver's type. It hands it
`getOptionalExpressionType(leftType, expression)` (`checker.go:29064`):

- at a chain **root** — `stage?.x`, where the `?.` is on this very access — that
  is `getNonNullableType`, whose answer carries no nullable facts at all;
- at an inner **link** — the `.c` of `a?.b.c` — it is `removeOptionalTypeMarker`,
  which takes back exactly the `undefined` the chain itself added.

The *type* side of this port already had all three functions
(`members.rs`, `checker-notes-nnaccess.md`). The diagnostic did not: `§849` wired
`reportObjectPossiblyNullOrUndefinedError` to the receiver site and read
`check_expression(receiver)` directly, which is the pre-strip type. So the report
fired on every `?.` in the corpus and in the world.

The corpus *did* see it. `conformance/controlFlowOptionalChain` carried **117**
unexpected TS18048; nobody had looked, because the suite reports a case's first
unexpected code and 3,138 cases fail. A defect can be both invisible in the
summary and enormous in the detail.

### The marker is subtracted by identity, because this port has no marker

Upstream's `optionalType` is an `undefined` **distinct from the real one**, which
is the entire reason `removeOptionalTypeMarker` can put back what the chain took
without touching a genuine `undefined`. This port has a single `undefined` — the
divergence owned at `checker-notes-nnaccess.md` §2 — and the first build
subtracted the marker by filtering the `UNDEFINED` flag.

That is wrong, and one case said so: `conformance/privateIdentifierChain.1` went
from 3 missing to **4**.

```ts
class A { a?: A; #b?: A;
    constructor() { this?.a.#b; }   // TS2532 on `this?.a` — upstream reports
}
```

`this?.a` is `A | undefined` where the `undefined` comes from `a?: A`, not from
the chain — `this` is not nullable, so nothing was stripped and no marker was
ever added. Upstream removes a marker that is not there and reports. A flag
filter removes the genuine one and stays silent.

The fix is to subtract by **identity** instead: `propagate_optional_type_marker_at`
remembers each link's type *before* the union (`Checker::pre_optional_marker`,
keyed by node), and `get_optional_expression_type` returns that remembered type
for an inner link. Same subtraction, without needing a second `undefined`.

This is worth stating plainly: **a one-`undefined` port can still model
`removeOptionalTypeMarker` exactly**, because the marker is identified by *where
it was added*, and that is recorded, rather than by *what it looks like*, which
is not distinguishable. Where the divergence in §2 does still bite is anywhere
the marker must survive an operation that rebuilds the union — this construction
does not.

### Measured, before and after on one checkout

Baseline is a `git worktree --detach` at HEAD, both runs on the same corpus.

| | before | after |
|---|---:|---:|
| `diagnostics` cases | 2,359 | **2,360** (+1) |
| `diagnostics` cases changed | — | 8 improved, 1 to PASS, **0 lost** |
| `checker_types` cases | 4,312 | 4,312 |
| `checker_types` lines | 414,927 | **414,927** (totals; the per-case run that proved *identical*, not merely equal-summing, was taken on the first base) |
| every other suite | — | identical |
| `apps/nextjs` (real repo) | 276 errors | **66** |

**Measured twice, on two different bases, for the same delta.** The first pair
was taken at `e5469b7` (2,349 → 2,350); a concurrent session landed underneath
and the rebased pair reads 2,359 → 2,360 with the *same eight cases* moving and
the same one reaching PASS. Two workstreams composing rather than overlapping is
the claim, and this is its evidence.

The snapshot committed alongside this section reads **2,369**, higher than either
pair: main moved twice more underneath during the rebase. The delta this work
owns is the +1 measured on both bases, with the same eight cases moving; the
absolute is whatever the tree read when the snapshot was written.

**A stale snapshot file was nearly claimed as a gain.** On the first base the
committed `checker_types.snap` read 414,473 while `STATUS.md`'s newest row (§163)
correctly read 414,490 — the *file* lagged its own commit by one landing, so this
change's diff appeared to carry +17 lines it had not earned. Re-measuring the
base in a worktree is what separated them, and it is the only reason the +17 is
not in this section's table. A number in a diff is not a number you measured.

Per-case, the eight that improved: `neverNullishThroughParentheses`,
`useUnknownInCatchVariables01`, `callChain.3`, `controlFlowOptionalChain`
(117 unexpected → 9), `controlFlowOptionalChain3` (**→ PASS**), `deleteChain`
(25 → 0), `exhaustiveSwitchStatements1`, `noPropertyAccessFromIndexSignature1`.

### What is still wrong, named rather than buried

The 9 that remain in `controlFlowOptionalChain` are a **different** defect: an
optional chain used as a *guard* does not narrow its root.

```ts
if (o?.foo != null) { o.foo; }   // still TS18048 here
```

That is `narrowTypeByOptionalChainContainment` (`flow.go`), unported, and it is
the same family as the two shapes left on the real repo — `error && … &&
error.body` (truthiness through `&&`) and `while ((m = re.exec(s)) !== null)`
(assignment-in-condition). All three are flow narrowing, none is this rule.

`apps/nextjs`'s residue after the fix: 23 TS18047, 17 TS2709, 6 TS18048,
5 TS2322, 4 TS7006, 3 TS7016, 3 TS18049, 2 TS2306, 2 TS2305, 1 TS1361.

### How you would know this was wrong

Two falsifiers, both wired as tests
(`crates/tsr-checker/tests/real_repo_regressions.rs`):

1. If subtracting the marker by identity were merely a spelling of the flag
   filter, `a_genuine_undefined_behind_a_chain_still_reports` would pass under
   both. It passes under one — mutation 3 in that file's table.
2. If the chain-root skip were suppression rather than upstream's own branch,
   deleting the receiver rule outright would redden nothing extra. It reddens
   exactly four true positives and no silence test — mutation 4.

**Two fixtures in that file were vacuous on their first run** and are recorded
there: written with `string[]` to mirror the real repository, they answered
`errorType` in a harness that loads no lib, so the rule declined for a reason
unrelated to chains and both stayed green under the mutation that should have
reddened them. Third occurrence this session of a green test that should have
been red; the pattern is always a fixture that does not reach the rule.
