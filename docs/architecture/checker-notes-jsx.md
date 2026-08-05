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

## Size

JSX is 13,078 assertion lines, 2.2% of the 594,122-line corpus, spread across 397
baselines — the largest single file is 2–4% of the JSX population, so unlike some
rows measured this cycle the line count does not overstate the value. It also
does not make JSX large.
