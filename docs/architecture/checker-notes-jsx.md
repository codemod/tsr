# JSX in the checker — and why the first JSX win was not checker work

Pinned to `vendor/typescript-go` @ `5b1047d10`.

## What is ported

Nothing in `tsr-checker`. There is no `jsx.rs`, and as of this note that is the
correct state.

One rule is ported in `crates/tsr-conformance/src/types_producer.rs`: an
intrinsic JSX tag name renders as `any`. That is a **baseline-writer** rule, not
a checker rule, and the distinction is the whole content of this note.

## The forcing constraint

`>div : any` appears 1,025 times in the corpus `.types` baselines and never once
as anything else — including in cases carrying a complete `JSX` namespace, where
the paired `.symbols` line resolves the same node to
`Symbol(JSX.IntrinsicElements.div, Decl(react.d.ts, 2402, 45))`. Across all
intrinsic tag names the population is ~1,779 lines in 288 baselines.

The obvious reading is that upstream's checker computes `anyType` for these, and
that this is a third instance of the accepted "answer `any` because upstream
does" exception (alongside `yield` and an unannotated accessor). **That reading
is wrong**, and it is wrong in a way that only shows up if you look for what
would exclude it.

## What upstream actually does

Upstream's checker answers **`errorType`** for an intrinsic tag name:

- `getIntrinsicTagSymbol` (`internal/checker/jsx.go:1216`) caches its resolved
  symbol on the *opening element's* links, not the tag name's.
- `checkJsxOpeningLikeElementOrOpeningFragment` (`internal/checker/jsx.go:130`)
  takes `getStringLiteralType(tagName.Text())` for an intrinsic and deliberately
  never calls `checkExpression(tagName)`.
- So the tag name reaches `getTypeOfNode` (`internal/checker/checker.go:31927`)
  unvisited. It *is* an expression node (`internal/ast/utilities.go:1971`, the
  `IsJsxTagName` clause), `resolveName("div", Value)` misses, and
  `checkIdentifier` returns the error type.

The `any` is produced by the writer. `internal/testutil/tsbaseline/type_symbol_baseline.go:378`
prints `t.AsIntrinsicType().IntrinsicName()` — the literal string `"error"` —
unless one of **eight** guards excludes the node, in which case it falls through
to the node builder, which renders any `TypeFlagsAny` type as the `any` keyword.
`isIntrinsicJsxTag` (`type_symbol_baseline.go:481`) is one of the eight.

`conformance/inlineJsxFactoryOverridesCompilerOption.types` shows both spellings
of one type on adjacent lines:

```text
><h></h> : error      <- errorType, fast path, prints the intrinsic name
>h : any              <- the SAME errorType, guard fires, node builder
```

That single pair is the falsifier. Any account claiming upstream renders
`errorType` uniformly — as `any` or as `error` — has to explain those two lines.

## Consequence, and the correction to ADR-0038

ADR-0038 concluded that upstream renders `errorType` as `any` everywhere, and
therefore that the ~6,000 lines where we print `error` were unreachable by
construction, capping the metric near 98.7%. They are not unreachable. They are
**unported**: `types_producer.rs` renders unconditionally through
`type_to_string` and has no counterpart to any of the eight guards. See ADR-0039.

## Sizing the seven remaining guards — `hadErrorBaseline` is not one of seven equals

Measured read-only over all 12,155 `.types` baselines, no corpus run.

`hadErrorBaseline` is **file-scoped**: when a case has an `.errors.txt`, the fast
path at `type_symbol_baseline.go:378` is disabled for *every* node in the case,
so the intrinsic name is never printed there. That yields a prediction sharp
enough to be worth stating before testing it:

> the string `: error` must never appear in a `.types` baseline whose case also
> has an `.errors.txt`.

**It holds, 0 violations in 12,155 baselines.** The partition:

| | `: error` lines | `: any` lines |
|---|---|---|
| cases **with** an `.errors.txt` | **0** (predicted 0) | 69,195 |
| cases **without** | 701 | 19,045 |

Three things follow, and they change the shape of the row ADR-0039 reopened.

1. **Upstream prints `error` only 701 times in the entire corpus.** Our producer
   prints it wherever the checker gaps — tens of thousands of lines. So the
   `error`-printing population is overwhelmingly *ours*, not upstream's, which is
   the ADR-0038 correction restated as a number.
2. **`hadErrorBaseline` dominates and is a single flag.** Its addressable pool is
   the 69,195 `any` lines in cases carrying an errors baseline — 78.4% of all
   `any` lines in the corpus. It is not a positional predicate at all; porting it
   is threading one file-scoped boolean into the producer.
3. **The six remaining positional guards share the 19,045-line complement**, and
   only that. They can only fire where the fast path is live, i.e. in cases
   *without* an errors baseline. That is a ceiling on all six together, before
   any per-position split.

**These are ceilings, not deliverables.** Most of the 69,195 are genuinely
`anyType`, which the producer already prints as `any`; the gain is only the
subset where our checker computes `errorType` and upstream's guard converts it.
Sizing that subset needs a corpus run bucketed per guard — the shape of
`examples/qualified_name_left.rs`, with an unattributed bucket printed
unconditionally. That measurement is **not done**; `bd` it rather than assume
these ceilings.

Practical consequence for whoever scopes it: `hadErrorBaseline` was flagged as
the hard one because the producer carries no file-scoped state, and that is still
true — but it is now also clearly the *valuable* one, and the six positional
guards are a much smaller pool than the eight-way split implied. Do not order
this work by guard count.

## What was ported, and what was deliberately left

Only `isIntrinsicJsxTag`. The other seven guards — `hadErrorBaseline`, binding
element, label name, global scope augmentation, meta property, and the
import/export statement names — move populations nobody has measured. (The
property-access/qualified-name guard is already covered in the same function by
a rule reached along a different route, added for `bd tsr-tl8`.) Porting them
blind would present an unmeasured net as a gain, which is the failure mode
ADR-0039 exists to record.

Upstream's `IsTypeAny` precondition is ported with the guard and is load-bearing,
not defensive. A lowercase tag name that *does* resolve — `const foo = () => 1`
used as `<foo/>` — is an intrinsic *name* by `IsIntrinsicJsxName`
(`internal/scanner/utilities.go:98`) but has a real type, never reaches the fast
path, and must keep printing it. The corpus discriminates this at 32 lines
printing `() => any` and 24 printing `typeof foo`.

## What is still gapped, and correctly

The 1,809 JSX **element** lines. Their types are `JSX.Element` (1194), `any`
(477), `error` (85) and a long tail, and which one you get depends on whether a
`JSX` namespace is in scope. That needs `getJsxElementTypeAt` / namespace
resolution — genuinely unported checker work, tracked separately. Answering `any`
for them would be a guess for the 1194, so they stay `error`.

The 85 element lines that already print `error` **must keep printing `error`**;
they are the control on this change.

## The `isIntrinsicJsxTag` slice has no clean number

Cycle 9 moved 60.26% -> 60.60%, 2,041 -> 2,066 cases, with this guard, an
inference change and a contextual-typing change landing together and ~1,580 lines
shared between them after `resolve`'s isolated +48. The prediction here was ~1,779
lines across 288 baselines, which is larger than that whole remainder — so this
either under-delivered or the other two were near zero, and no arithmetic over
three simultaneous changes separates them.

**Recorded as unresolved: neither a hit nor a miss.** Do not read the cycle total
as this slice's number. An isolated run of `d6dc9a7^..d6dc9a7` would settle it.
What *is* confirmed is the control: nothing fell, so the 85 `error` element lines
did not flip.

## Size

JSX is 13,078 assertion lines, 2.2% of the 594,122-line corpus, spread across 397
baselines — the largest single file is 2–4% of the JSX population, so unlike some
rows measured this cycle the line count does not overstate the value. It also
does not make JSX large.

## The element row's feasibility, measured (fifth session)

`examples/jsxfeas.rs`, over every JSX element/fragment gap line (1,199
classified), walks `getJsxElementTypeAt`'s path one hop at a time:

| | lines |
|---|---:|
| want `JSX.Element`, **`JSX` does not resolve as a namespace** | 548 |
| want `JSX.Element`, resolves — but the declared type **prints bare `Element`** | 343 |
| want `any`, `JSX` does not resolve | 256 |
| want `any`, resolves, no `Element` export | 28 |
| other | 24 |

Two findings, both unfavourable to the board's 0.70 feasibility:

1. **46% of the row cannot see the namespace.** The corpus's `JSX`
   namespaces overwhelmingly live inside `declare global` blocks in module
   files — global augmentation, which this binder does not merge. That is
   binder work with its own hazard rail (`binder_symbols` 98.03%).
2. **The resolvable 29% lands in a refused family.** Upstream prints
   `JSX.Element`; the declared type here prints `Element`, so every
   converted line needs the enclosing-namespace qualification — the
   qualified-naming build STATUS.md §5 refused at **2.7 wrong per right**.
   Building the JSX arm without it converts nothing (`Element` ≠
   `JSX.Element` on every line); building it *with* it re-opens a refusal.

**Re-scored: feasibility ~0.25, not 0.70** — the item is blocked on global
augmentation (binder) and namespace-qualified naming, in that order, and
should not be picked until one of those moves. The want-`any` 284 are a
trap besides: upstream's `any` is the *no-namespace-in-scope* fallback, and
this port cannot distinguish "absent upstream" from "invisible to us"
until the same augmentation machinery exists.
