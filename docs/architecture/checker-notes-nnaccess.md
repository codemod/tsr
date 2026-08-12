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

### Postscript — the chain-root skip was a shortcut, and it is now the mechanism

The first build answered the chain root with an early `return` before any arm
ran. That is outcome-equivalent for **every code this function emits today** —
`getNonNullableType`'s answer carries no nullable facts, so no arm could fire —
and the section above said so. Asked whether that was a fix or a suppression,
the honest answer was *"a shortcut with one hole"*, and the hole is worth naming
because it is invisible until someone else falls in it:

**TS18046** (`_0_is_of_type_unknown`) exists in `tsr-diagnostics` and **nothing
in the checker emits it**. It is the `unknown` arm of the same
`checkNonNullType` (`checker.go:7409`), and `u?.x` with `u: unknown` *does*
report upstream. A skip keyed on `?.` would have swallowed it silently on the
day that arm was ported — the rule would have read "optional chains are exempt"
when the truth is "a stripped receiver has no facts to report".

So the skip is gone. The receiver's type now goes through
`get_optional_expression_type` on **both** roads — chain root and inner link —
and every arm reads its facts from that one type, which is the type
`checkNonNullType` is handed. Measured against the same base: `diagnostics`
2,396 → 2,396, **zero cases changed**, `checker_types` 415,703 → 415,703. A
refactor that moves nothing is exactly what a behaviour-preserving one should
measure.

**One thing it did move, and the fix is instructive.** The first attempt tested
the facts *before* the syntactic arms and lost `compiler/classExtendsNull3`.
§910's `extends null` stand-in sets `maybe_null` **inside** the reporter — the
fact is syntactic because `check_expression(super)` cannot supply it yet
(`bd tsr-gjze`) — so an earlier gate is one its arm can never reach. The test
now guards the two syntactic arms only; the type-based arms hand their type to
the reporter and let it run its own test, after the stand-in has had its say.

Generalised: **a shared reporter that enriches its own inputs cannot have its
precondition hoisted into its callers.** Hoisting reads as deduplication and is
a silent behaviour change.

## 6 — a gap must stay a gap: `typeof x === "object"` was manufacturing `null`

Three user reports across one session, three unrelated libraries, one line of
code.

```go
// narrowTypeByTypeName, flow.go:670
case "object":
    if t.flags&TypeFlagsAny != 0 { return t }
    return c.getUnionType([]*Type{ …nonPrimitive…, …null… })
```

Upstream guards this arm with a **flags** test. This port wrote
`t == self.intrinsics.any` — an **identity** test. Upstream's `errorType` is
`newIntrinsicType(TypeFlagsAny, "error")`, and so are `wildcardType` and
`blockedStringType` (`crate::intrinsics` documents all four). Every one is
admitted by the flag and excluded by the comparison.

### Why one transliteration slip produced three bug reports

`errorType` is this port's answer for *anything it cannot resolve yet*. Upstream
reaches this arm with `errorType` almost never; this port reaches it constantly,
because it has thousands of gaps upstream does not. So a rule that treats
`errorType` as an ordinary type is quiet in the corpus and loud in the world.

Falling through built `object | null` out of a gap, and the next property access
reported `'x' is possibly 'null'` — a `null` that exists in no program, invented
by the narrowing itself. The three reports:

| reported as | the unresolved producer |
|---|---|
| `'rule' is possibly 'null'` after `!rule \|\| typeof rule !== "object"` | a `Record`-defaulted generic from a wasm loader |
| `'session' is possibly 'null'` after `session && typeof session === "object"` | a Playwright `page.evaluate` chain |
| `'error' is possibly 'null'` through an `&&` chain | (same arm, same shape) |

**I had attributed two of these to their producers** (`checker-notes-printseam.md`
§9) and called them blocked on the mapped-type subsystem. That was right about
the gap and **wrong about the diagnostic**: the gap is upstream-shaped and
silent, and only this arm turned it into an error message. Fixing the producer
would have hidden the arm; fixing the arm fixes every producer at once.

The general rule this is an instance of: **no rule may convert `errorType` into
a concrete type.** Any place that does is the same bug waiting for a different
library to find it. This one was found by being reported three times.

### Measured

| | base | after |
|---|---:|---:|
| `checker_types` | 422,620 | **422,620** — per-case, **zero rows changed** |
| `diagnostics` | 2,519 | **2,519** — per-case, **zero rows changed** |
| `apps/nextjs` | 69 errors | **63** — six gone, **none new** |

The corpus resolves what it writes, so nothing in it reaches this arm with an
`errorType`. That is the same profile as every other defect in
`real_repo_regressions.rs`, and it is why that file exists.

**A near-miss on the numbers, recorded because it nearly went into a commit
message.** The first after-run read +6,917 lines and +123 cases against a
baseline worktree cut before a rebase. Re-cutting the baseline at the true `HEAD`
read *exactly* the after-numbers: the entire delta was other sessions' landings.
A baseline is only a baseline at the commit you are actually sitting on.

### How you would know this was wrong

Four diagnostic tests in `real_repo_regressions.rs` and one type test in
`narrowing.rs`. The mutation table records that **no diagnostic test can see the
mutation that returns `t` for every type** — that mutation makes the checker
narrow *less*, and a suite of "did it report?" assertions is structurally blind
to under-narrowing. Two predictions about which test would catch it were written
and both lost to the actual run. The type assertion is what catches it.
