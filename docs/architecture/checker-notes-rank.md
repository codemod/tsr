# The ranked board for `checker_types`

> **FIXED 2026-08-06 (cycle 12). The correction below diagnosed the defect and
> asked for a positive terminal arm; `cause()` now has one, and the `KIND` column
> has been re-measured. `TERMINAL` is 16.41%, not 34.92%.**
>
> The 2026-08-05 correction (kept in full below) said `TERMINAL` was an *upper
> bound* on kind 1 rather than a measurement of it. Measured, **more than half of
> it was not kind 1**:
>
> | | published (`33e3bd5`) | old classifier at `9b10272` | **fixed** |
> |---|---:|---:|---:|
> | `TERMINAL` | 34.92% | 47,933 (34.59%) | **22,739 (16.41%)** |
> | `propagated/span` | — | 34,332 (24.77%) | 34,332 (24.77%) |
> | `propagated/named` | — | 19,112 (13.79%) | 19,112 (13.79%) |
> | `DEPENDENT-UNKNOWN` | — | 37,208 (26.85%) | **45,814 (33.06%)** |
> | `UNMATCHED` (the control) | — | 0, and could not be non-zero | **16,588 (11.97%)** |
>
> **Of the 47,933 lines the old classifier called `TERMINAL`, 25,194 — 52.6% —
> were not.** 8,606 name a dependency the span test cannot see; 16,588 carried no
> evidence either way and had been absorbed by the `else`.
>
> **Two changes, both of them positive tests:**
>
> 1. `Terminal` now requires **`has_inner`** — that the node has at least one
>    rendered line strictly inside its span. The span test returns two facts
>    instead of one, because *"nothing inside this node gapped"* and *"this node
>    has nothing inside it"* are the same `false` and mean opposite things.
>    Collapsing them is precisely what let a **leaf** claim the strongest label on
>    the page. A line no arm claims now falls to `UNMATCHED`, which is the
>    control, and the control can finally be reached.
> 2. `no value declaration` and `the name does not resolve` join `initialiser` and
>    `annotation` as `DEPENDENT-UNKNOWN`. Both name a dependency outside the
>    node's span; neither is evidence that the work is local to the row.
>
> **The control that makes this trustworthy is M0: revert `cause()` to its
> published form and re-run.** `propagated/span` (34,332) and `propagated/named`
> (19,112) are then **identical to the digit** — the change moved lines only
> between the three buckets it touches, and 47,933 − 22,739 = 8,606 + 16,588
> exactly. Nothing leaked.
>
> **Row 9 reclassifies itself.** The 2,618-line declaration-name row that an agent
> measured by hand at **68.6% propagated** now reads `UNMATCHED` by construction,
> from a classifier that knows nothing about that investigation. A hand result and
> a structural fix agreeing is the strongest evidence form available here.
>
> Two more pairs fall out of the same mechanism and are worth carrying: `member
> name, the receiver has no such property` (4,016) reads `UNMATCHED` while its twin
> `property access, the receiver has no such property` (4,005) reads
> `propagated/span`, and `member name, the property has no type` (1,861) pairs the
> same way with 1,860. `a.b` renders two lines, the member name is the leaf, and
> the access is not — so **where a member-name row reads `UNMATCHED`, its access
> twin's label is the better estimate of what it is really waiting on.** That is a
> next-reader's inference, not a measurement, and it is not folded into any
> number here.
>
> Figures at `9b10272`, which is `058b4a9` plus two documentation commits — the
> compiler is unchanged, and `right` reads 292,606 either way. They differ from
> the published `33e3bd5` figures because three checker commits landed in between
> (`fa29e66`, `1e4bddb`, `979f18c`); gap is 138,585 here against 139,612 there.
>
> **Same caution as `checker-notes-recvgap.md` §4 carries**, and it applies to
> every `the name does not resolve` figure on this page: the row is a population
> and a majority of it is not a defect — upstream's own baseline answers `any` or
> `error` for those names too, and this port already computes the same answer with
> a different renderer (ADR-0038). Its largest single contributor was a harness
> limitation (`@lib` was dropped; fixed in `9d5b026`, converted 362 lines against
> a 1,784-line estimate). The corrected split is being re-measured and is not
> quoted until it is. `bd tsr-cug`.
>
> See `bd tsr-v1j`.

> **CORRECTED 2026-08-05 (cycle 11). `TERMINAL` does not mean what this page
> reads it as, and the control bucket that appears to guarantee it cannot fire.**
>
> `cause()` (`crates/tsr-conformance/examples/rank_board.rs:127`) is four arms with
> a **default**:
>
> ```rust
> if reason.contains("the receiver is a gap") { PropagatedNamed }
> else if gapped_below                        { PropagatedSpan }
> else if reason.contains("/ initialiser ") || reason.contains("/ annotation ") { Unknown }
> else                                        { Terminal }   // <-- the default
> ```
>
> So `TERMINAL` means **"none of three evidence patterns matched"**, not "the
> prerequisite is met". §4 below reads it as the second. For a *declaration-name*
> row the difference is total: `gapped_below` is a span test and a declaration
> name spans only itself, so no line can ever be below it, and a reason ending
> `/ neither` contains neither `initialiser` nor `annotation`. **Such a row falls
> to `Terminal` by construction, whatever it actually depends on.**
>
> Row 9 is the demonstration. Ranked here as `TERMINAL / kind 1`, it measures
> **68.6% propagation, 17.6% lib globals, 13.1% local — and none of the local part
> is in the module this page names.** Building it would have converted zero lines.
> See [`checker-notes-symbols.md`](checker-notes-symbols.md) and `bd tsr-eyn`.
>
> **`CONTROL UNATTRIBUTED by the TERMINAL/PROPAGATED split = 0` is vacuous.**
> `cause()` is total — every line gets one of four labels — so nothing can ever be
> unattributed and that control is structurally incapable of reading non-zero. It
> reads zero on every run and proves nothing. The page's other controls are sound:
> `family()` has a genuine `UNCLASSIFIED` arm, and when it *was* non-zero it found
> two real work items (5,743 + 1,145 lines).
>
> **What survives.** Every *count* on this page is correct; the concentration
> columns, the family split, the level-4 statistic and the case/gradient
> separation all stand. What must be requoted is the **34.92% TERMINAL** figure and
> every per-row `KIND` label derived from it: read them as an **upper bound on
> kind 1**, not a measurement of it. A row labelled `TERMINAL` here has not been
> shown to be terminal; it has been shown that this classifier found no evidence
> against it.
>
> Found by the agent assigned rows 9 and 10, after a briefing (mine) that repeated
> this page's `KIND` column as established. Recorded here rather than only in a
> successor file, because two agents were briefed off this page.

Status: measured 2026-08-05 at **`33e3bd5`**, over the 9,538-case `.types`
population, from one pinned binary in an isolated worktree. The instrument is
`crates/tsr-conformance/examples/rank_board.rs`, added in the same commit as
this file. Upstream references are to `vendor/typescript-go` @ `5b1047d10`.

Every number here is an **assertion line** in a `.types` baseline unless the row
says otherwise. `docs/architecture/checker-notes-calls.md` counts **call
expression nodes**; the board has twice turned a node count into a line count by
omission, so the unit is restated on every table.

> The board was first taken at `0e8e902` (the pin in the assignment). It was
> re-taken at `33e3bd5` once `types_producer::assertions_for_case_with_ids`
> landed (`edb37c3^..edb37c3`), because the prerequisite split in §4 needs it
> and mixing binaries is what `checker-notes-guard.md` records losing a
> measurement to. **Every number on this page is from the `33e3bd5` binary.**
> The two runs differ by +450 right lines and +18 cases; nothing in the ranking
> changed sign.

---

## 0. The findings, in the order they change decisions

1. **The board has been ranked on the wrong axis.** The line gradient and the
   case gate are nearly orthogonal, and the case gate — the harder-sounding
   target — is the *cheaper* one. §7.
2. **Only ~~34.92%~~ 16.41% of the gap is confirmed kind 1.** The repaired triage
   question has now been asked of every gap line rather than row by row, and the
   majority of the gap is either propagation or unmeasured. §4.
   **Requoted 2026-08-06:** the 34.92% was the default arm's total. With a
   positive terminal test it is **16.41%**, and 11.97% of the gap is `UNMATCHED` —
   no evidence either way. See the header.
3. **20.03% of the residual is not a gap at all.** 37,489 lines are already
   typed and *wrong*, they appear in no histogram in `docs/architecture/`, and
   the largest case-flipping row on the whole board is among them. §6.
4. **The `errorType, never anyType` rule is not held.** 7,288 lines answer `any`
   and lose; 21,685 lines of the gradient are *banked* on `any` and nobody has
   separated the computed from the defaulted. §6.
5. **Three of the top gradient rows evaporate on the concentration check**, one
   of them by 86.6%. §3.

---

## 1. The instrument

### It is on the correct harness path — verified, not assumed

```
$ grep -n "assertions_for_case" crates/tsr-conformance/examples/types_shapes.rs
164:  let ours = types_producer::assertions_for_case(&parsed, &expected, true);

$ grep -n "assertions_for_case_with_ids" crates/tsr-conformance/examples/rank_board.rs
      let (program, ours, ids) =
          types_producer::assertions_for_case_with_ids(&arena, &parsed, &expected);
```

Both entry points go through `render_case` and `program_for_case`
(`crates/tsr-conformance/src/types_producer.rs`), which build **one program per
case with every `vendor/typescript-go/internal/bundled/libs/lib.*.d.ts`**
loaded. That is the check `checker-notes-counters.md` failed and
`overload_funnel.rs` was rewired for in `731b1ee`.

### Why `types_shapes` was not enough

`examples/types_shapes.rs` buckets by upstream's answer shape and by the node
kind we failed on. That is level 3 and it is the right level, but it cannot
answer three questions a ranking cannot skip:

1. **Concentration, per row.** A row size is a sum; a sum cannot say whether it
   is 500 cases or one. `rank_board` prints case count, top-1 share, top-10
   share and the three largest cases *beside every row*, so the check cannot be
   skipped by a reader in a hurry.
2. **Is the prerequisite met?** 37.15% of the gap sits under `expression
   answered error: X`, a catch-all over two opposite work items. §4.
3. **The level-4 statistic** (`bd tsr-g27`), never run before: for each row,
   over the cases it touches, how many would have **nothing left** if it were
   closed. And the case gate is whole-baseline and positional
   (`crates/tsr-conformance/src/types_suite.rs:150`), so an **unaligned** line
   fails its case exactly as a wrong type does — 10,054 lines and 166 whole
   cases that no aligned-only histogram can see. §7.

### Five control buckets, all zero

```
CONTROL right+gap+wrong-aligned                          = 0
CONTROL family roll-up - gap total                       = 0
CONTROL UNCLASSIFIED                                     = 0
CONTROL UNATTRIBUTED by the TERMINAL/PROPAGATED split    = 0
CONTROL residual sum - (upstream - right)                = 0
CONTROL gap+wrong+unaligned - residual                   = 0
```

`UNCLASSIFIED` reached zero only after two rows were added that
`types_shapes`'s roll-up hides inside its `other` bucket (6,943 lines, 4.96% at
`0e8e902`): **5,743 lines** where the symbol has *no value declaration at all*,
and 1,145 where a type alias's right-hand side gaps. Neither is "a symbol whose
kind `getTypeOfSymbol` does not handle" — there is nothing to dispatch *on*
rather than no arm to dispatch *to*, and the work is the binder's. **A non-zero
control bucket was not a rounding error; it was two whole work items filed under
a label that named neither.**

The bucket that is deliberately non-zero is `DEPENDENT-UNKNOWN` in §4. It is
reported as its own answer rather than folded into either of the other two.

### Four named mutations, each red, none reddening another

`rank_board`'s classifier is checked before anything is measured, on **every
run** (`fn check_classifier`). Assertions in `main` rather than `#[cfg(test)]`,
because Cargo does not run tests inside an example without a manifest change and
`crates/tsr-conformance/Cargo.toml` is a shared manifest three other agents are
editing this cycle (`docs/conventions.md`: "on a shared manifest, commit the
index you constructed").

| mutation | `grep -c` before running | result |
|---|---|---|
| **M1** — `family()` tests the `property access` arm *before* the `receiver is a gap` arm | 1 | red: "the receiver arm must be tested before the property arm". 19,166 lines change family silently |
| **M2** — `row_key()` stops cutting `"has no such property: "` | 1 | red: "the receiver type must be cut from the key" |
| **M3** — `cause()` drops the named-dependency arm and trusts the span test alone | 1 | red: "a named gapped dependency outranks the span test". 6,749 lines flip to TERMINAL |
| **M4** — `cause()` calls a sibling initialiser TERMINAL | 1 | red: "a sibling initialiser is not evidence of TERMINAL" |

Independence was demonstrated rather than argued: each mutation panics on *its
own* assertion with the earlier ones passing, and under M1 with the order
assertions disabled the collapse assertions stay green and the corpus numbers
are unchanged. All restored, green, re-run.

---

## 2. The level-3 histogram at `33e3bd5`

```
cases judged             9,538
assertion lines upstream 478,954
  aligned                468,900 (97.90%)
  exactly right          291,799 (60.92% of upstream, 62.23% of aligned)
  gap  (we said `error`) 139,612
  wrong (ported, defect)  37,489
  unaligned               10,054
```

### What changed against the last recorded numbers

- The **committed snapshot in the tree is stale**: 290,258 lines / 2,066 cases
  (60.60%). Do not quote it.
- **"58.17%" is not the `checker_types` gradient** and never was. It is
  `examples/writer_guards.rs`'s figure over its own **lib-less** population. It
  is quoted as the gradient in `bd tsr-4sc`, in ADR-0039's resolution, and in
  `checker-notes-calls.md:427` ("The current number is 58.17%"). That probe has
  since been corrected (`91ad1b3`) and now reads 62.14% over *its* aligned
  population; that is consistent with the 60.92% here over *all* upstream lines,
  because aligned lines are the subset where our walker agrees with upstream.
  **Quote either number with the instrument named in the same sentence.**
- The +597 attributed to the label slice came from the broken probe and cannot
  be re-derived. It is **unverified, not corrected**. Not carried here.

### The gap, partitioned by why the checker stopped (139,612 lines)

| lines | share | family |
|---|---|---|
| 51,859 | 37.15% | an expression we do not compute (unported form, **or an error operand**) |
| 20,904 | 14.97% | a type node we cannot resolve (`getTypeFromTypeNode` / declared types) |
| 19,166 | 13.73% | a property access whose receiver we cannot type |
| 16,454 | 11.79% | an initialiser expression we do not compute |
| 12,145 | 8.70% | a property access whose property we cannot find or type |
| 8,224 | 5.89% | a symbol whose kind `getTypeOfSymbol` does not handle |
| **5,743** | **4.11%** | **a symbol with no value declaration at all (alias, export marker)** — new row |
| 3,621 | 2.59% | a free name that does not resolve, in value position |
| **1,145** | **0.82%** | **a type alias whose right-hand side gaps** — new row |
| 212 | 0.15% | a node that is neither a declaration name nor an expression |
| 74 | 0.05% | a member name resolved as if it were free (`bd tsr-tl8`) |
| 55 | 0.04% | the symbol has a type; the line differs for another reason |
| **0** | | **UNCLASSIFIED** |

The top family is a **catch-all, not a work item**. §4 splits it.

Upstream's own answer shape (level 2, for continuity only — it ranks nothing):
intrinsic 164,406 lines at 53.75% right; literal 91,675 at **95.87%**; named
reference 63,017 at 66.09%; function/signature 55,418 at 45.86%; array 12,491 at
**37.86%**; intersection 1,216 at **28.87%**.

---

## 3. RANKING A — the gradient (unit: assertion lines)

Rank this list if the target is the **line gradient**. Breadth wins; the
concentration column is the veto; the cause column is the discount.

| # | row | lines | cases | top-1 | top-10 | cause / KIND | feature exists? | module |
|---|---|---|---|---|---|---|---|---|
| 1 | `expression answered error: ElementAccessExpression` | 11,553 | 248 | **86.6%** | 94.2% | TERMINAL / 1 | **yes** — `indexed.rs:59`, anchored `checker.go:8146` | `tsr-checker/src/indexed.rs` |
| 2 | `expression answered error: CallExpression` | 9,560 | 1,928 | 7.2% | 26.5% | propagated/span / 2 | **yes** — `calls.rs` | `tsr-checker/src/calls.rs` |
| 3 | `property access, the receiver is a gap: Identifier` | 6,751 | 886 | 14.1% | 59.4% | propagated/named / 2 | n/a | — |
| 4 | `member name, the receiver is a gap: Identifier` | 6,749 | 885 | 14.2% | 59.4% | propagated/named / 2 | n/a | — |
| 5 | `member name, the receiver has no such property` | 4,016 | 1,060 | 4.7% | 20.7% | **TERMINAL / 1** | **yes** — `members.rs` | `tsr-checker/src/members.rs` |
| 6 | `property access, the receiver has no such property` | 4,005 | 1,053 | 4.7% | 20.7% | propagated/span / 2 | as #5 | — |
| 7 | `reference, the name does not resolve` | 3,621 | 869 | 9.7% | 44.8% | **TERMINAL / 1** | partial — 367 are lib names | `tsr-binder` |
| 8 | `expression answered error: BinaryExpression EqualsToken` | 3,014 | 455 | 10.1% | 42.3% | propagated/span / 2 | **yes** — `binary.rs:73` | `tsr-checker/src/binary.rs` |
| 9 | `declaration name … SymbolFlags(FUNCTION)/FunctionDeclaration/neither` | 2,618 | 769 | 1.5% | 9.2% | **TERMINAL / 1** | dispatch arm missing | `tsr-checker/src/symbols.rs` |
| 10 | `declaration name … SymbolFlags(ALIAS)/no value declaration` | 2,535 | 1,027 | 3.4% | 16.5% | **TERMINAL / 1** | binder marker carries no value decl | `tsr-binder` |
| 11 | `expression answered error: ParenthesizedExpression` | 2,178 | 433 | 20.8% | 40.3% | propagated/span / **2, and 100% of the row** | **yes** — pure propagation | — |
| 12 | `declaration name … SymbolFlags(PROPERTY)/PropertySignature/annotation ArrayType…` | 2,003 | **3** | **99.9%** | 100.0% | DEPENDENT-UNKNOWN | — | — |
| 13 | `expression answered error: BinaryExpression PlusToken` | 1,975 | 152 | **42.9%** | 82.7% | propagated/span / 2 | **yes** — `binary.rs` | `tsr-checker/src/binary.rs` |
| 14 | `expression answered error: ArrowFunction` | 1,893 | 526 | 5.1% | 19.5% | **TERMINAL / 1** | **yes** — `expressions.rs:160` | `tsr-checker/src/expressions.rs` |
| 15 | `expression answered error: CallExpression` | 1,854 | 683 | 2.4% | 12.5% | **TERMINAL / 1** | **yes** | `tsr-checker/src/calls.rs` |

### The rows that evaporate on the concentration check

Three, and the check found them without anyone having to suspect them — which
is why it is printed beside every row rather than run on request:

| row | lines | where they actually are |
|---|---|---|
| #1 `ElementAccessExpression` | 11,553 | **10,000 in `compiler/largeControlFlowGraph`** (86.6%). The same file, the same `const data = []` evolving array, and the same verdict as the 9,999 in `docs/conventions.md`. Distributed size: **~1,553 over 247 cases.** |
| #12 `SymbolFlags(PROPERTY)/PropertySignature` | 2,003 | **2,000 in one case**, `compiler/conditionalTypeDiscriminatingLargeUnionRegularTypeFetchingSpeedReasonable`. Three cases in total. |
| #13 `BinaryExpression PlusToken` | 1,975 | top-1 42.9%, top-10 82.7% over 152 cases. |

**Rank #1 as ~1,553, not 13,143 and not 11,553.** Anyone quoting the headline
number as the value of element-access work is repeating a mistake
`docs/conventions.md` already records twice.

### The rows that survive it

`CallExpression` (1,928 + 683 cases, top-1 ≤7.2%), the two `has no such
property` rows (1,053–1,060 cases, top-1 4.7%), `ALIAS / no value declaration`
(1,027 cases, top-1 3.4%) and `SymbolFlags(FUNCTION)/FunctionDeclaration/neither`
(769 cases, top-1 1.5%) are genuinely distributed. Those are the
gradient-shaped items.

---

## 4. Is the prerequisite met? The repaired triage question, measured

`docs/conventions.md` repairs "does movement depend on something else first?"
into "**is the prerequisite met for most of the population?**", which is
measurable rather than a judgement. Until now it has been answered row by row by
argument. It is now answered line by line, for all 139,612:

| lines | share | cause |
|---|---|---|
| **48,757** | **34.92%** | **TERMINAL** — nothing this line depends on is known to have gapped. Kind 1; the size is the worth. |
| 34,411 | 24.65% | propagated/span — a sub-expression inside this node's span already gapped. Kind 2. |
| 19,166 | 13.73% | propagated/named — the reason itself says `the receiver is a gap`. Kind 2. |
| **37,278** | **26.70%** | **DEPENDENT-UNKNOWN** — the reason names an initialiser or an annotation *beside* the node, which this probe does not follow. Neither answer. |

**Only about a third of the gap is confirmed kind 1.** 38.38% is confirmed
propagation whose row size is an upper bound of unknown depth, and 26.70% is
honestly unmeasured. Any ranking that treats a gap row's size as its worth is
wrong for two-thirds of the board.

### How the split works, and the trap it had to be repaired around

The span test asks whether a gapped line sits **inside this node's span**. That
is exactly right for an expression — `a[b]`, `f(x)`, `x = y` all contain their
operands. It is wrong for a node whose dependency is its *sibling*:

- the `b` of `a.b` is a **leaf**, so no line is inside it, and a span-only
  classifier calls all 6,749 `member name, the receiver is a gap` lines TERMINAL
  — while the reason string says in words that the receiver gapped;
- a declaration name `x` in `var x = f()` spans only `x`; the gap it waits on is
  `f()` **beside** it.

A first version of this probe did exactly that and reported **68.49% TERMINAL**.
That number is wrong and is recorded here so it is not quoted: the correct
figure is **34.92%**, and the difference is entirely the two classes above. The
repair is to use the span test where it is valid, the reason string where it is
not, and to report what neither can decide as its own bucket. M3 and M4 in §1
are the mutations that hold the repair in place.

### What the split does to the rows

| row | headline | TERMINAL | propagated | collapse |
|---|---|---|---|---|
| `expression answered error: CallExpression` | 11,414 | **1,854** | 9,560 | **6.2×** |
| `expression answered error: BinaryExpression EqualsToken` | 3,123 | **109** | 3,014 | **29×** |
| `expression answered error: ParenthesizedExpression` | 2,178 | **0** | 2,178 | **∞ — zero work** |
| `expression answered error: BinaryExpression PlusToken` | 2,225 | **250** | 1,975 | 8.9× |
| `expression answered error: ArrowFunction` | 3,174 | 1,893 | 1,281 | 1.7× |
| `expression answered error: ArrayLiteralExpression` | 1,149 | 620 | 529 | 1.9× |

The `CallExpression` result is an **independent confirmation of
`checker-notes-calls.md`** by a method that shares nothing with it: that page
took a row from 12,289 read → 12,015 → 10,265 → 2,562 → 557 confirmed
actionable, over four measurements. The span test, in one pass, says 1,854 of
11,414 lines are terminal. Two unrelated instruments agree that the row is
roughly an order of magnitude smaller than its headline.

`ParenthesizedExpression` is the clean case: **2,178 lines, zero of them
terminal.** `(x)` has no rule of its own to port. A board that ranked it at
2,178 would have assigned work worth nothing, and nothing short of this
measurement says so.

### A prediction of mine that this falsified

Before the split was measurable, §4 of the first draft argued from §5 — every
top gradient row names an already-ported feature — that the mass would be
"mostly PROPAGATED". At 34.92% TERMINAL and 26.70% unmeasured, that was
**directionally right and quantitatively unsupported**: I would have quoted a
number I could not have justified. Recorded as a miss.

---

## 5. Every top gradient row names a feature that already exists

`docs/conventions.md` records that the repaired criterion was itself falsified
once: it classified `+=` as kind 1 worth its whole row, and `+=` **already had a
working arm** — nobody had grepped `binary.rs`. That check was run on every row
in §3. The result is not one row; it is nearly all of them.

```
$ grep -rn "ElementAccessExpression" crates/tsr-checker/src/
  expressions.rs:133:  Expression::ElementAccessExpression(node) => self.check_element_access_expression(node),
  indexed.rs:59:       /// Ported from `Checker.checkElementAccessExpression` (`checker.go:8146`).

$ grep -rn "EqualsToken" crates/tsr-checker/src/binary.rs
  binary.rs:73:  SyntaxKind::EqualsToken | SyntaxKind::CommaToken => right_type,

$ grep -rn "ArrowFunction" crates/tsr-checker/src/expressions.rs
  expressions.rs:160:  Expression::ArrowFunction(node) => node …
```

Element access is ported and anchored. Assignment is ported and returns
`right_type`. Arrow functions are ported. Calls are ported (`calls.rs`, 36 KB).
Property lookup is ported (`members.rs`).

This is "a row named after a symbol flag is usually not about that flag", one
level out: **a row named after a node kind is usually not about that node kind
either.** The row name says where the answer was demanded, not where it was
lost. §4 is what tells the two apart, and it is now a column rather than an
argument.

---

## 6. The 37,489 lines nobody has been ranking

20.03% of the case-gate residual is **not a gap**. These are lines we already
answer, wrongly. They appear in no gap histogram in `docs/architecture/`, and
they contain the largest case-flipping row on the board.

| row (node kind, whole bucket) | lines | share | cases | top-1 | **finishes** |
|---|---|---|---|---|---|
| `Identifier` | 31,067 | 82.87% | 2,943 | 32.2% | **521** |
| `ArrayLiteralExpression` | 1,773 | 4.73% | 361 | 28.9% | 4 |
| `PropertyAccessExpression` | 1,549 | 4.13% | 600 | 5.4% | 3 |
| `ObjectLiteralExpression` | 725 | 1.93% | 216 | 17.1% | 1 |
| `BinaryExpression` | 693 | 1.85% | 252 | 10.0% | 0 |

Split by the **shape substitution**, because "31,067 wrong Identifiers" is not a
work item:

| substitution | lines | cases | top-1 | finishes |
|---|---|---|---|---|
| `Identifier intrinsic -> array` | 10,003 | **4** | **100.0%** | 0 |
| `Identifier intrinsic -> intrinsic` | 5,453 | 823 | 22.5% | 82 |
| `Identifier function/signature -> function/signature` | 1,573 | 503 | 1.9% | 36 |
| `Identifier function/signature -> object literal` | 1,402 | 110 | 21.8% | 18 |
| `Identifier array -> array` | 1,215 | 116 | 50.3% | 6 |
| `Identifier object literal -> object literal` | 1,044 | 212 | 9.6% | 31 |

**The whole-bucket 521 is not the sum of the sub-rows' finishes.** A case that
needs two substitutions is finished by neither alone. This is
`docs/conventions.md`'s "before summing, state which question the sum answers",
and it is why both roll-ups are re-derived by the probe rather than one being
inferred from the other.

`intrinsic -> array` evaporates on the concentration check: **10,000 of its
10,003 lines are `any -> never[]` in `compiler/largeControlFlowGraph`.** That is
the *same file* as gradient row #1, and it has **changed sides**. We used to gap
there; we now answer `never[]` where upstream answers `any`. It is worth zero
cases.

Commonest exact wrong answers over the corpus:

```
        any -> never[]   10,000        any -> undefined    635
     number -> any        2,642          E[] -> error[]    512
     string -> any        1,485          any -> number     451
      any[] -> never[]    1,047  undefined[] -> never[]    232
```

### The `any` audit — a rule nobody had measured

`docs/conventions.md` and ADR-0039 both forbid answering `any` where the checker
cannot compute a type. Whether the rule is *held* had never been measured.

```
 21,685  lines we answered `any` and upstream did too — banked, 7.43% of the gradient's right column
  7,288  lines we answered `any` and upstream did NOT — a confident wrong answer
    672  lines where `error` reached a printed type (`error[]`, `E[] -> error[]`)
```

- **7,288 lines are already the failure the rule exists to prevent.** `number ->
  any` (2,642), `string -> any` (1,485), `unknown -> any` (191), `error -> any`
  (139), `boolean -> any` (94). Worse for ranking than for correctness: they are
  filed under *wrong*, so they are invisible in every gap histogram and
  attributable to no work item.
- **21,685 banked lines rest on answering `any`** — 7.43% of the gradient. That
  is the exposure if any of it is defaulted rather than computed. It is **not**
  concentrated (2,747 cases, top-10 share 27.5%, largest single case 3,686 in
  `compiler/resolvingClassDeclarationWhenInBaseTypeResolution`), which makes it
  harder to audit rather than easier. **This is the false credit and its size is
  21,685 lines. Nobody has separated computed `any` from defaulted `any`, and
  this document does not either.** Filed as the largest open measurement.
- 672 lines print `error` *inside* a type. `error[]` is neither a gap nor an
  answer: it is a gap that escaped and was then scored as a wrong claim.

**No row on this board should be closed by widening an `any` answer.** Every
such line would score as converted and be indistinguishable in the aggregate
from real work.

---

## 7. The level-4 statistic (`bd tsr-g27`)

Run for the first time. Per-case residual is `upstream lines - exactly matched
lines`, over **all** lines, because that is what the gate reads.

### What the residual is made of (187,155 lines)

| lines | share | what |
|---|---|---|
| 139,612 | 74.60% | gap — we answered `error` |
| 37,489 | 20.03% | wrong — ported and defective |
| 10,054 | 5.37% | **unaligned** — the walker did not reproduce upstream's *text* |

**166 cases have a residual made entirely of unaligned text.** No checker change
of any kind can flip them; they are a walker work item, and they are invisible
in every aligned-only histogram in `docs/architecture/`.

### The case curve — and the orthogonality result

| residual ≤ | cases | rate | lines in them | share of residual |
|---|---|---|---|---|
| 0 | 2,146 | 22.50% | 0 | 0% |
| 1 | 3,002 | 31.47% | 856 | 0.46% |
| 3 | 4,547 | 47.67% | 4,561 | 2.44% |
| 5 | 5,513 | 57.80% | 8,829 | 4.72% |
| 10 | 6,963 | 73.00% | 20,038 | 10.71% |
| **25** | **8,375** | **87.81%** | **42,620** | **22.77%** |
| 50 | 9,007 | 94.43% | 65,029 | 34.75% |
| 100 | 9,303 | 97.54% | 85,606 | 45.74% |
| 250 | 9,471 | 99.30% | 112,017 | 59.85% |

**40.15% of the whole residual — 75,138 lines — lives in 67 cases**, 0.70% of
the corpus. However much of it is closed, the case rate moves by 0.70 points.
That is the orthogonality result, and it is why gradient-shaped work has
produced case deltas of +0.34.

> The 2,146 here against the gate's 2,128 is a **known +18 discrepancy**: this
> residual metric cannot see lines *we* emit that upstream does not, and the
> gate's positional comparison fails on those. 0.19% of the corpus; stated
> rather than smoothed.

### Near-misses need more than one thing

Of the **4,817** cases sitting 1–10 lines from passing:

| distinct rows needed | cases | share |
|---|---|---|
| 0 (walker text only) | 143 | 2.97% |
| **1** | **1,289** | **26.76%** |
| 2 | 1,159 | 24.06% |
| 3 | 687 | 14.26% |
| 4 | 545 | 11.31% |
| 5+ | 994 | 20.64% |

Only 27% of near-misses are one work item away. 70% need two or more, from a
tail of roughly 400 distinct rows.

---

## 8. RANKING B — cases (unit: cases the row would finish)

A different list from §3, and deliberately not merged with it. `finishes` =
cases where this row accounts for **the entire remaining residual**.

> **`finishes` is an upper bound and the size of the error is known.** It assumes
> the row is closed *completely* in every case it touches. The one measured
> analogue in this project — 6,610 lines converted, 109 of 1,571 touched cases
> flipped, 6.9% against a predicted 10–25% — says real yields sit well under the
> optimistic bound. Treat every number in this column as a ceiling and score
> against it.

| # | row | lines | **finishes** | cases | median left | cause | module |
|---|---|---|---|---|---|---|---|
| **1** | **`WRONG: Identifier`, whole bucket** | 31,067 | **521** | 2,943 | 6 | already typed, wrongly | across the checker |
| 2 | `declaration name … SymbolFlags(ALIAS)/no value declaration` | 2,535 | 117 | 1,027 | 6 | TERMINAL | `tsr-binder` |
| 3 | `reference, the name does not resolve` | 3,621 | 114 | 869 | 4 | TERMINAL | `tsr-binder` |
| 4 | `declaration name … SymbolFlags(FUNCTION)/FunctionDeclaration/neither` | 2,618 | 67 | 769 | 12 | TERMINAL | `tsr-checker/src/symbols.rs` |
| 5 | `expression answered error: CallExpression` (terminal half) | 1,854 | 41 | 683 | 11 | TERMINAL | `tsr-checker/src/calls.rs` |
| 6 | `expression answered error: TemplateExpression` | 948 | 38 | 172 | **2** | TERMINAL | `tsr-checker/src/expressions.rs` |
| 7 | `property access, the name is not an identifier` | 400 | 26 | 85 | 4 | TERMINAL | `tsr-checker/src/members.rs` |
| 8 | `expression answered error: NewExpression` (terminal half) | 1,027 | 24 | 433 | 10 | TERMINAL | `tsr-checker/src/calls.rs` |
| 9 | `declaration name … SymbolFlags(VALUE_MODULE)` | 291 | 20 | 168 | 6 | TERMINAL | `tsr-binder` |
| 10 | `expression answered error: ArrowFunction` (terminal half) | 1,893 | 18 | 526 | 15 | TERMINAL | `tsr-checker/src/expressions.rs` |
| 10= | `declaration name … SymbolFlags(FUNCTION_SCOPED_VARIABLE)/BindingElement/neither` | 1,547 | 18 | 222 | 10 | TERMINAL | `tsr-checker/src/symbols.rs` |
| 12 | `expression answered error: ObjectLiteralExpression` (terminal half) | 526 | 17 | 290 | 8 | TERMINAL | `tsr-checker/src/objects.rs` |

Every propagated row finishes **zero** cases. That is not a coincidence and it
is the sharpest single argument for the cause column: a row that cannot finish a
case on its own is, by construction, waiting on something else.

### The two rankings disagree, and that is the point

`ElementAccessExpression` is **#1 on the gradient and 14th on cases** — 11,553
lines, 11 cases. `WRONG: Identifier` does not appear on the gradient board at
all (it is not a gap) and is **#1 on cases by a factor of 4.5**.
`TemplateExpression` is 948 lines — outside the gradient top 20 — and 6th on
cases, because its cases carry a **median of 2** lines after it.

The lines-per-case-flipped signature from `docs/conventions.md` (61:1 for a
broad shallow fix) reproduces and extends: `TemplateExpression` is 25:1,
`ALIAS` 22:1, `WRONG: Identifier` 60:1, `CallExpression` 45:1.

### The kind-2 rows: what demonstration would settle each

No number is quoted for any of these. Per `docs/conventions.md`, "probe past the
blocker, not at it" — ask what the form answers **once the blocker is removed**.

| row | lines | the demonstration |
|---|---|---|
| `expression answered error: CallExpression` (propagated half) | 9,560 | Already done and it is the model: `checker-notes-recv.md` mocked past the blocker and got 557 nodes. **Do not re-derive; read that page.** |
| `property access / member name, the receiver is a gap` | 13,500 | Hand the receiver a real object type on the top three cases (`parserRealSource11`, `temporal`, `parserRealSource10` — 955/818/796 lines) and see how far up the chain the answer travels. If the access still gaps, the receiver was not the only blocker. |
| `property access, the receiver has no such property` (propagated half) | 4,005 | The receiver types but the member is absent. Print the receiver types actually seen; `Promise<boolean>`, `SymbolConstructor` and `this` are already in the tail. Give one of them the missing member by hand and see whether the line converts. |
| `expression answered error: BinaryExpression EqualsToken` | 3,014 | `binary.rs:73` already returns `right_type`. Type the RHS by hand on one case; if the line converts, the row belongs entirely to whatever gaps the RHS and **not** to `binary.rs`. |
| `expression answered error: ParenthesizedExpression` | 2,178 | **None needed. Zero terminal lines. There is no work item here.** |
| `DEPENDENT-UNKNOWN` rows | 37,278 | The measurement in §9, not a demonstration. |

---

## 9. What is still not measured

- **The 37,278 DEPENDENT-UNKNOWN lines (26.70% of the gap).** The reason names
  an initialiser or an annotation *beside* the node. Following them needs the
  initialiser's own `NodeId`, which `gap_reason` formats away into a string. The
  fix is small — have `gap_reason` carry the dependency's id — but it is in
  `types_producer.rs`, which another agent owns this cycle. **Filed. Until it is
  taken, no row in RANKING C is either ranked or dismissed.**
- **The computed/defaulted split of the 21,685 banked `any` lines** (§6). 7.43%
  of the gradient and nobody has looked.
- **`examples/wrong_attribution.rs` is on the lib-less path**
  (`parse_with_options` at ~line 810), so it cannot answer "what are we
  confidently wrong about". §6 answers a coarse version from the correct path;
  the fine version needs it rewired, as `overload_funnel.rs` was in `731b1ee`.
- **`examples/qualified_name_left.rs` has the same defect** (`bd tsr-qj4`,
  unowned). Its 174-wrong / 4,455-zero-wrong figures are measured on the wrong
  compiler. Not used here.

---

## 10. Is 90% on `checker_types` reachable?

The previous session answered **no**, from cycle deltas of +4.94, +3.36, +0.34
against four workstream-shaped cliffs (instantiation, generic inference,
signature links, contextual typing). **The conclusion survives. The mechanism
does not, and the target was ambiguous.**

### First, say which 90%

The suite reports two numbers and the previous verdict mixed them: its deltas
are **gradient** points, but "90% on `checker_types`" in the board's usual sense
is the **case** rate.

| | today | to 90% | residual lines to close | share of 187,155 |
|---|---|---|---|---|
| case rate | 22.31% (2,128/9,538, `coverage`) | +6,457 cases | ~48,000 | **~25.6%** |
| line gradient | 60.92% (291,799/478,954, `rank_board`) | +29.08 pts | 139,300 | **74.4%** |

From §7: 90% of 9,538 is 8,585 cases; the curve passes 8,375 at residual ≤25 and
9,007 at ≤50, so 90% means clearing **every case with about 30 lines or fewer
left** — roughly 48,000 lines. Reaching 90% on cases would leave the gradient at
about **70.9%**. **The case gate is the cheaper of the two targets by 2.9×,**
which inverts the intuition the board has been ranking on.

### The mechanism is not four cliffs. It is a zero-tolerance gate over a long tail

The cliffs are real; §7 shows they are not what binds:

- **73.0% of the corpus is already within 10 lines of passing.** No cliff stands
  between those 4,817 cases and a pass; a handful of lines each does.
- **40.15% of the residual is in 67 cases** (0.70%) — the deep tail, worth 0.70
  points however much of it closes. The cliffs largely live *there*.
- What binds is that the gate has **no partial credit**: 70% of near-misses need
  2+ distinct rows from a ~400-row tail, and 166 cases need the *walker* rather
  than the checker.

### The arithmetic

The best single row on the board finishes **521** cases — the optimistic ceiling
of §8, on a statistic whose one measured analogue came in at 6.9% of prediction.
The best gap rows finish 117 and 114. To add 6,457 cases you need roughly a
dozen `WRONG: Identifier`-sized wins or fifty `ALIAS`-sized ones, each landed
completely. Observed deltas of +4.94, +3.36, +0.34 gradient points are
consistent with that and give no reason to expect a step change.

**Verdict: 90% is not reachable on the current trajectory. But the previous
session's reasoning was wrong about why, and its implied strategy — attack the
cliffs — is the more expensive of the two available.** ADR-0038's ~98.7% ceiling
was withdrawn by ADR-0039 and no ceiling is established; nothing here
establishes one either. This is a statement about the *rate*, not about a wall.

### What I would do instead

Stop ranking on gradient rows. The three items with the largest measured case
yield are:

1. **`WRONG: Identifier`** — 31,067 lines, 2,943 cases, ceiling 521. Not a gap;
   already-typed lines that are wrong. Start with `intrinsic -> intrinsic`
   (5,453 lines, 823 cases, ceiling 82) and specifically the `number -> any` /
   `string -> any` substitutions inside it, which are the §6 rule violation.
2. **The two binder rows** — `ALIAS / no value declaration` (117) and `the name
   does not resolve` (114). Both TERMINAL, both distributed, both cheap, and
   both name the **binder** rather than the checker.
3. **`SymbolFlags(FUNCTION) / FunctionDeclaration / neither`** — 2,618 lines,
   769 cases, ceiling 67, a missing `get_type_of_symbol` arm.

Two items already measured elsewhere rank as follows against this board:

- **Instantiated generic members** (`bd tsr-4qx`: 929 nodes, 225 cases, top-10
  57.6%, 47.3% `Promise`). Its owning agent's pre-registered threshold fired for
  "case-gate item, not a gradient item" and this board agrees: 929 nodes is
  outside the gradient top 20, and 225 cases with a 57.6% top-10 share is
  concentrated enough that its case yield will land well below 225.
- **The binding-element position** (`bd tsr-4qa`: 192 claims, 192 convert,
  **empty residue**, 48 cases, top-10 57.8%). Small, but the empty residue is
  the strongest signal on the board — a position where upstream always holds
  `errorType` converts fully or not at all. Carry its caveat: **192 is a floor,
  not a value**, because upstream's conjunction order hides subordinate arms.
  Port the position, not the arm.

### Falsifiers, stated before the work

- **If `WRONG: Identifier` work flips far fewer than ~35 cases per 1,000 lines
  converted, the level-4 statistic over-predicts and every `finishes` column
  here must be discounted by the measured factor.** I expect it to come in low;
  the one prior analogue came in at 6.9% of prediction.
- **If measuring the 37,278 DEPENDENT-UNKNOWN lines shows most of them
  TERMINAL**, §4's "only a third is kind 1" understates the board and RANKING A
  grows by up to 26.70 points of the gap. I predict the opposite — mostly
  dependent — because the rows are `/ initialiser CallExpression` and
  `/ annotation TypeReference`, and both of those named dependencies have large
  gap rows of their own.
- **If a cycle converts >10,000 residual lines and moves the case rate by more
  than 2 points**, the "40% of the residual in 67 cases" model is wrong and the
  two axes are not as orthogonal as §0 claims.
- **What must NOT move**: `binder_symbols` (8,292/8,459), `printer_round_trip`
  (11,681/11,737), `parser_typescript` (5,000/5,031). And the 10,000
  `any -> never[]` lines in `largeControlFlowGraph` must stay put under every
  item above. If they move, the item that moved them touched evolving arrays and
  was not the work it was ranked as.

---

## 11. Reproducing this

```bash
git worktree add --detach /tmp/hist 33e3bd5
cd /tmp/hist && git submodule update --init --recursive
CARGO_TARGET_DIR=/tmp/hist/target cargo run --release -p tsr-conformance --example rank_board
CARGO_TARGET_DIR=/tmp/hist/target cargo run --release -p tsr-conformance --example types_shapes
```

`rank_board` takes about 70 seconds after the build, `types_shapes` about 25.
**Take them in a worktree, not in the shared checkout**:
`docs/architecture/checker-notes-guard.md` records a probe whose control and
mutation arms appeared to move by 207 lines purely because teammates' edits
landed between two `cargo run`s. One pinned binary per comparison.
