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

## The blocker measurement was stale, and the element arm's bar (seventh session)

This page's "46% cannot resolve the `JSX` namespace" was measured **before
the `/.lib` mount** (`93b540a`) — react.d.ts declares a *plain global*
`namespace JSX` and simply never loaded. Re-run at `94cc971`:

```
  759  want JSX.Element  | declared type prints `Element`      <- the item
  410  `JSX` does not resolve (256 want any + 132 + 22)        <- the real residual
   28  want any          | JSX resolves, no `Element` export
    2  want any          | declared type prints `Element`
```

The dominant bucket is now the whole mechanism: **the element type resolves
and computes; no expression arm asks for it.** The build is one arm —
`checkJsxElement` (`jsx.go:72`) → `getJsxElementTypeAt` (`:1275`): a
`JsxElement` / `JsxSelfClosingElement` / `JsxFragment` expression answers the
declared type of the in-scope `JSX` namespace's `Element` export. The
qualifier (`JSX.Element`, never bare `Element`) is design P's existing
`symbol_chain` on a plain-namespace parent — no new naming machinery.

**Bar, registered before the arm:** net ≥ +500 (66% of the 759, discount
owned by needs-qualification behaviour at tsx sites being unmeasured); own
new wrong ≤ 30 (2× the measured 2 want-any lines plus margin for the
`no-Element-export` and unresolved buckets, which must stay gaps); cases
regressed == 0; lost ≤ 5 (the arm is additive on kinds with no arm today).
Falsifier: if new wrong lands in `want any` JSX lines, the 256-line
unresolved bucket is being answered instead of refused — the arm must gap
when `JSX` or `Element` is absent, not answer `error`-adjacent text.

## The element arm scored — every leg passed, and the residual names the composite-print boundary

`verdictdump` pair at the arm's commit:

```
right  348,389 -> 349,542   +1,153     GAP→RIGHT 1,157, RIGHT→WRONG 4
gap     80,875 ->  79,429   −1,446
wrong   39,651 ->  39,944   +293       GAP→WRONG 289
cases    2,767 ->   2,786   +19, 0 regressed
```

| leg | registered | measured | |
|---|---|---:|---|
| 1 | net ≥ +500 | **+1,153** (152% — the cascade is arrow-function *return* lines) | pass |
| 2 | own ≤ 30, global beside | **4 own · +293 global** | pass |
| 3 | regressed == 0 | **0** | pass |
| 4 | lost ≤ 5 | **4** — all `compiler/tsxUnionSpread`, where the *baseline itself* records `error` (upstream's union-component spread resolution fails and says so); this port now computes `JSX.Element` where upstream refused. ADR-0038's boundary from the other side | pass |

The own/downstream split of the 289 is one predicate over the dump: **4**
lines print the arm's own answer where upstream wanted something else; **285**
are composite *signature* prints — `() => Element` where `() => JSX.Element`
is wanted — the baked-text-inside-composites boundary design P has always
had, now with its largest single population. That family is the next naming
shard and it is NOT this arm's defect: the return type it embeds is correct.

## The heritage-expression shard (`bd tsr-fpti`, first shard) — bar

`class B extends React.Component<P, any>` records the **instantiated
reference** on the heritage expression (`checkJsxChildrenProperty12.types:27`,
`>React.Component : React.Component<ButtonProp, any>`); a bare `extends Base`
records `typeof Base`, which the value path already answers. Gate: the
expression's parent is `ExpressionWithTypeArguments` with non-empty
arguments. Sized at **108 wrong lines / at-risk 0** (upstream never records
`typeof` for a with-arguments heritage). Bar: net ≥ +55 (~50%; the entity
walk and arity check will decline a share); own new wrong ≤ 10; regressed
== 0; lost ≤ 3. Falsifier: losses → the gate is firing outside heritage
positions (`ExpressionWithTypeArguments` appears in `implements` too, where
the same rule holds — but check that first).

### The heritage shard's bar FIRED on both sides — REFUSED with its numbers

Built to the bar above and measured: **+10 converts against a ≥ 55 floor, 5
lost, 2 gap→wrong — reverted.** Two findings, each a correction to the
sizing:

1. **The gate over-fires**: `ExpressionWithTypeArguments` is also TS 4.7's
   *instantiation expression* (`Box<number>` in value position,
   `instanceofOnInstantiationExpression`), where upstream keeps
   `typeof Box`. A correct gate needs the `HeritageClause` ancestor.
2. **The 108-line population does not convert from resolution**: most of its
   lines decline in the arm (the written type *arguments* themselves gap —
   the tsx `P`/`S` argument types are interfaces whose members gap), so the
   line stays `typeof …` wrong either way. The row's blocking cause is the
   argument types, not the reference shape — a population identified by the
   shape of the answer, misattributed to a mechanism, this project's oldest
   trap, hit again and caught by the bar in one measurement.

`bd tsr-fpti` keeps the corrected diagnosis; nothing further is claimed.

## `bd tsr-fpti` re-probed at HEAD (ninth session) — the family decomposes to owners, none of them this item

Sized fresh from the `verdictdump` at `594973e`: gap lines whose wanted type
mentions `Component`/`JSX.`/`Element` are **960 in 135 cases** (was "1,955 +
96 wrong over 220" when filed — the element arm and the naming family
harvested the head), top case 9%. `jsxfeas.rs` re-run classifies the
element/fragment half at **438**, and the buckets have flipped since the
seventh session: the "declared type prints bare `Element`" bucket — the 759
that built the element arm — is **gone**. What remains, each with its owner:

| lines | shard | owner, and why it is not buildable here |
|---:|---|---|
| 256 | element gaps wanting `any`, `JSX` does not resolve | **ceiling.** Upstream's `checkJsxElement` returns `getJsxElementTypeAt` = `errorType` when there is no namespace (`jsx.go:74`, `:1303`), rendered `any` by ADR-0038. The one honest-`any` escape is **fragments only** — `checkJsxFragment` converts `errorType` → `anyType` explicitly (`jsx.go:123`) — and the corpus has **zero** fragment gap lines wanting `any` (measured, this session). Computing `any` for the element lines would reproduce a rendered `errorType`, which ADR-0038 forbids |
| 132 + 22 | want `JSX.Element`/other, `JSX` does not resolve | `declare global` augmentation in the **binder** — the fifth session's named blocker, still standing |
| 28 | `JSX` resolves, no `Element` export | upstream `errorType` again — ceiling |
| ~100+ | custom-pragma factories (`dom.JSX.Element`, `predom.JSX.Element`, `JSXInternal.*`; head case `inlineJsxFactoryDeclarationsLocalTypes` 86) | per-file `@jsx` pragma namespace resolution (`getJsxNamespace(location)`) plus `import("…")`-form naming — `bd tsr-xpb8`'s modulespecifiers family |
| ~46+ | composite prints embedding `JSX.Element` (`() => JSX.Element`, props signatures) | the composite-print seam's remaining shards |
| rest | class instance typing through heritage (`this.props`, `React.StatelessComponent<T>`) | refused at `7299a14` with its numbers (+10 vs ≥55, reverted) — the blocking cause is the written type **argument** types (tsx `P`/`S` interfaces whose members gap), i.e. downstream of the members workstream |

**Conclusion: `bd tsr-fpti` holds no buildable item.** The probe cost two
instrument re-runs and settles what the heritage-shard refusal left open —
the family's residue is ceiling + two named subsystem blockers, and it should
not return to a board until global augmentation or the pragma machinery
exists.

## The heritage instantiation, the `ts` slice — bar (ninth session)

The `tsr-fpti` heritage shard refused at `7299a14` (+10 vs ≥55) for two
reasons: the gate over-fired on TS 4.7 instantiation expressions, and the
tsx population's written argument types gap. Neither applies to the
producer's own heritage branch (`types_producer.rs:414`): it already demands
the `HeritageClause` ancestor, `extends`, and a class owner — and it then
answers `get_declared_type_of_symbol`, the UNINSTANTIATED `A<T>`, ignoring
the written arguments. The fresh W2 row `A<Base> → A<T>` (11 lines, **6
finishes**) is the convertible `ts` slice: simple argument types that
resolve.

**Bar:** net ≥ **+8**; own ≤ **6** — falsifier: wrong lines where the
baseline records `typeof X` for an argument-bearing heritage → the refused
shard's population is leaking in despite the ancestor gate, stop; regressed
== 0; lost ≤ **2** (an argument that gaps must fall through to the declared
answer exactly as before, never to a gap).

### The ts-slice scored — the 11-line row was the family's tip

```
WRONG→RIGHT 122 · nothing else at all · +27 suite cases (2,983 → 3,010) — past 3,000
```

Legs: net **+122** (15× the floor — the W2 rename-shape heuristic sees only
single-token diffs, and most of the family's wants are multi-token) · own
**0** (the `typeof`-leak falsifier silent) · regressed **0** · lost **0**.
The `tsr-fpti` heritage refusal stands untouched for the tsx population —
this branch's ancestor gate is what the refused checker-side gate lacked,
and the argument-gap fall-through is what keeps the tsx lines exactly where
they were.

## The export-assignment name — the heritage compensation's sibling — bar (ninth session)

`export = C1` / `export default Foo` record the class name's **declared**
type — `>C1 : C1`, `>Foo : Foo<T>` for a generic (its own parameters) — not
`typeof C1` (`exportNonVisibleType`, `exportAssignmentGenericType`,
`es6ExportEqualsInterop`; ~22 wrong lines, 4 finishes). The same
producer-side compensation shape as the heritage branch, keyed on an
`ExportAssignment` parent and a successful TYPE-meaning resolution — a `var`
or function on the right resolves only as a VALUE and keeps its value type,
which is the bar's falsifier.

**Bar:** net ≥ **+14**; own ≤ **8** — falsifier: losses on `export =` of
values → the TYPE-meaning gate leaks; regressed == 0; lost == 0.

### The export-assignment name scored — two gate refinements from two adverse lines

Ungated: +99 / 1 lost (`export = Math` resolving the GLOBAL `Math`
interface). Same-symbol gate: 70 — dropped `importNonExportedMember*`'s 27
interface-only lines (no value meaning is fine). Final — decline only a
DIFFERENT-symbol value resolution:

```
WRONG→RIGHT 70 · GAP→RIGHT 26 · GAP→WRONG 1 (a generic interface's declared
print lacking its own parameters — a declared-print divergence, not this
gate) · zero losses · +24 suite cases (3,019 → 3,043)
```

Legs: **+95** (≥ +14) · own **1** (≤ 8) · **0** regressed · **0** lost.
