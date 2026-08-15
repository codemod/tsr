# Checker notes — `members` workstream

Rows 5 and 6 of `docs/architecture/checker-notes-rank.md` §3: `member name, the
receiver has no such property` (4,016 lines, 1,060 cases) and `property access,
the receiver has no such property` (4,005 lines, 1,053 cases), measured at
`33e3bd5`. Upstream references are to `vendor/typescript-go` @ `5b1047d10`.

Kept out of `docs/architecture/checker.md` for the reason
`checker-notes-overloads.md` gives: a shared file is swept by whoever commits
second.

---

## 0. Status

**Measured and closed.** Two corpus runs over the 9,538-case `.types`
population, in isolated worktrees: run 1 at `b484ea9` (§5) and run 2 at
`897cc45` (§6). Both pre-registered gates fired **negative**, so **nothing was
built**: `crates/tsr-checker/src/members.rs` and `indexed.rs` are untouched by
this workstream. The three-line disagreement with `rank_board` is **resolved and
was never a disagreement** (§6.1). Issue `bd tsr-gzx`.

The instrument is `crates/tsr-conformance/examples/member_shapes.rs`, six
control buckets and six named mutations (§2). **No behaviour change was made**,
so there is nothing here to falsify by mutation except the instrument itself.
Any number labelled *subset* comes from a capped smoke run and is not an answer:
the caps are alphabetical prefixes, the worst possible sample of a corpus whose
case names cluster by feature.

**The one-line verdict, since it is the thing most likely to be misquoted:** the
two rows are one work item worth **8,024 lines**, of which 19.7% is upstream's
own error and 37.2% is parked, and **both gates over what remains fired
negative** — 774 against a 1,000 bar, and 1,122 against a 1,200 bar with
narrowing over its ceiling. 8,024 is a **ceiling on a population, never a
conversion**.

---

## 1. The two questions, and why the board cannot answer either

The board ranks rows 5 and 6 adjacently at 4,016 and 4,005 lines and it is
tempting to add them: 8,021 would be the largest genuinely-distributed item on
the gradient board. `docs/conventions.md` forbids exactly that step — *"sharing a
downstream function is not the same as being blocked by it"*, the rule written
after 83% of four rows attributed to one function turned out to be turned back
two lines earlier, over-counting a sum by 2.7×.

So two questions have to be answered before the sum is quotable, and they are
different questions:

1. **Are the two rows the same nodes?** Near-identical aggregates are the
   *weakest* available evidence: two independent populations of the same size
   look exactly like one population counted twice. The eleven-line and
   seven-case differences are the part that has to be explained, not waved at.
2. **Whose failure is it?** The row name says *"the receiver has no such
   property"*. `docs/conventions.md` says a row named after a flag or a
   declaration kind is usually not about that flag, and
   `docs/architecture/checker-notes-calls.md` measured this exact phrase on the
   call path: **89% of it was a lookup that never ran**, and only 10.9% was a
   genuine "no such property" that upstream errors on too.

### The mechanism the second question is about, in this file

`Checker::get_property_of_type` (`crates/tsr-checker/src/members.rs`) is anchored
to `getPropertyOfTypeEx` (`checker.go:18899`) and opens:

```rust
let owner = match &self.store.get(id).data {
    TypeData::Named { members: Some(owner), .. } => Owner::Declared(*owner),
    TypeData::Anonymous { symbol, .. } => Owner::Anonymous(*symbol),
    _ => return None,
};
```

That `_` arm answers `None` for every other receiver shape **before it reads the
name**. An intrinsic receiver, a union, an intersection, a literal, and a
`Named` this port built without a member table are all indistinguishable in the
reason string from a receiver whose member table was read and did not contain
the name. One of those is upstream's own answer; the rest are ours.

---

## 2. The instrument, and the order it was built in

`crates/tsr-conformance/examples/member_shapes.rs`.

```
cargo run -p tsr-conformance --example member_shapes --release
```

### The cheap cut was tried first, and it is in the output

`types_producer::access_reason` (`crates/tsr-conformance/src/types_producer.rs`)
ends with

```rust
format!("the receiver has no such property: {}", checker.type_to_string(receiver_type))
```

so the receiver's **printed type is already in the reason string** — it is why
`rank_board`'s `row_key()` has to cut that prefix before the rows are rows. A
receiver-shape histogram is therefore recoverable from the existing output with
no checker change and no counters at all.

**That is recorded here as a result, not as a preamble.** Three probes in this
repository re-implemented the harness and measured a different compiler
(`docs/conventions.md`, "A probe that re-implements the harness"), and one route
into that is reaching for new machinery before checking what the existing output
already contains. The `TSR_MEMBER_COUNTERS` counters sketched in the assignment
were never built, and `members.rs` was never touched, because neither was needed:

- the **cheap cut** decides intrinsic vs union vs named from the printed string;
- the **`TypeData` column** — the one that decides `Named { members: Some }`
  from `Named { members: None }`, which the printed form cannot — needs only
  `Checker::type_of`, which is already `pub` (`crates/tsr-checker/src/checker.rs`).

Both columns are printed side by side, and **where they disagree is printed
too**, because a disagreement is the measurement of what the cheap instrument
cannot see. On a 333-case subset the disagreement is one-sided and instructive:
10 lines that the text calls `printed with | or &` are `Named` types whose
*name* contains a bar — `Promise<string | number>` and friends. A cheap cut
alone would have filed those under unions and pointed at the wrong work.

A second consequence of needing no counters, worth stating because it cost
`overload_funnel` its parallelism: the classification is per-case state, so this
probe runs under `rayon` and its concentration pass needs no single-threaded
run.

### The pairing measurement

Every rows-5/6 line is keyed by the `PropertyAccessExpression` it came from —
row 6 *is* the access, row 5 is its name child, one hop up through
`nodes.parent`. The probe then reports how many access nodes contributed
**both** lines, and, for every node that contributed only one, **why**:

| bucket | what it would mean |
|---|---|
| the access has no identifier name child | `access_reason` routed it to another string |
| no assertion emitted at the other position | the walker or upstream has no line there |
| the other line is unaligned | it never entered either row's population |
| the other line is RIGHT | the two rows are *not* one item at this site |
| the other line is wrong with a real type | ported and defective, a different item |
| the other line gapped for a DIFFERENT reason | **the only bucket that makes the rows separate work** |

The residual is what the pairing claim rests on. "They look paired" is not an
explanation of eleven lines; a named bucket is.

### Six control buckets, printed unconditionally

- rows-5/6 lines that reached no receiver shape
- the line's node is not a property access or its name child
- **the reason says no such property and `get_property_of_type` finds one** —
  the sharpest of the six: `access_reason` reaches this row only when the
  lookup answered `None`, so a hit means the probe and the producer have drifted
  and nothing below it is a partition
- shape roll-up − rows 5/6 total
- pairing − rows 5/6 total
- **row 5, a leaf, counted as `propagated/span`** — added after §5.3, and it is
  the one whose value is fixed by the structure of the thing measured rather
  than by the measurement

There is deliberately **no `other` receiver-shape bucket**. The match producing a
shape is exhaustive over `TypeData`, so a new type variant is a compile error
rather than a silent row of leftovers — the stronger form of the same control. A
runtime bucket that can only ever read zero is decoration, which this project
deletes.

### Six named mutations, each red, none reddening another

Checked in `main` on every run rather than under `#[cfg(test)]`, for
`rank_board`'s reason: Cargo does not run tests inside an example without a
manifest change, and `crates/tsr-conformance/Cargo.toml` is shared.

| mutation | `grep -c` before running | result |
|---|---|---|
| **MU1** — `row_of` drops the `has no such property` guard, admitting `the receiver is a gap` | 1 | red at the gapped-receiver assertion; would have added ~13,500 lines of rows 3/4 to the population |
| **MU2** — the cheap cut matches an intrinsic name *anywhere* in the printed form | 1 | red at `Promise<number>` → `Named`; `Promise<number>` contains `number` |
| **MU3** — both prefixes map to `Row::MemberName` | 1 | red at the access-row assertion; pairing would read 100% by construction |
| **MU4** — the span test's polarity inverted | 1 | the `row 5 as propagated/span` control reads **65** instead of 0 on a 400-case subset. **This is the bug §5.3 records making**, and it is caught by a runtime control rather than an assertion because no fixture-free assertion can see it |
| **MU5** — `agreement` drops the subset test | 1 | red: "upstream holding a strict subset of our constituents is narrowing"; `upstream is NARROWER` would collapse into `differs` and the flow.rs-vs-members.rs question would be unanswerable |
| **MU6** — the union rule becomes *any* constituent instead of *every* | 1 | red: "a union with one of two constituents holding the property is Partial"; this single edit moves the whole composite bucket from "not ours" to "ours" while nothing real changes |

Each panicked on its **own** assertion with the others passing. MU3 and MU6 are
the two that matter most, and for the same reason: each is an edit that would
make a *desirable* answer come out — pairing at 100%, the composite bucket
actionable — with nothing real behind it. MU4 is the one that actually
happened.

### The denominator, stated rather than assumed

The probe applies the suite's own skips (`has_varied_types`,
`has_known_divergence`, empty baselines) and `rank_board`'s alignment test, so
its row totals are directly comparable with the board's 4,016 and 4,005.
**Reconciling them is the cross-check**, and `docs/conventions.md` makes that
the difference between a count and a prediction: the one estimate that landed
in the cycle it describes was the one validated against the instrument before
being quoted.

A capped run (`TSR_MEMBER_SHAPES_LIMIT`) exists for smoke-testing and prints its
case count for exactly this reason. A capped run is not a measurement.

---

## 3. Pre-registered, before the corpus run

Recorded here before the numbers exist, because
`docs/conventions.md` records a method being invented to explain a number
already seen.

### The rule goes on the bucket that *is* the question

`bd tsr-4qx`'s level-4 probe put its threshold on a proxy — "≥25% of cases at
≤10 remaining" — when the instrument printed the `0` bucket one row above, and
the two disagreed: the proxy said *build*, the direct bucket said the row
finishes **no case at all**. The build was avoided by luck, from a separately
registered override.

So:

- **Gradient question.** Is there a `members.rs`-owned gradient item here? The
  direct bucket is `Intrinsic + Literal` lines — the `getApparentType` item.
  **Build if it is ≥ 1,000 lines *and* its own top-1 case share is ≤ 25%.** Both
  legs on the sub-population, never the parent's: the concentration check
  separated a 557-node row (147 cases, top-10 49.9%) from its 2,113-case parent
  (top-10 24.9%) with opposite verdicts.
- **Case question.** The direct bucket is the **`0` bucket** of the level-4
  histogram for that same sub-population — the cases the item *finishes*, not
  the cases it touches. No proxy.
- **What does not decide anything.** Whatever `Named without members` reads, it
  does **not** unpark `bd tsr-4qx`. That item is parked on a measurement (zero
  of 225 affected cases have nothing else failing; median 18 other failures),
  and its unpark condition is written in the `bd` note: the *other* defects in
  those cases shrinking. A large number in that bucket here is the same fact
  arriving through a second door, not new evidence. (The recursion-limits issue
  is `bd tsr-el3.2`; the instantiated-members item is `bd tsr-4qx`. Three docs
  and eleven source comments had that pair wrong.)

### The prediction

Rows: 5 and 6 of `checker-notes-rank.md` §3. Instrument:
`member_shapes.rs`, added in the commit this file is part of. Scored against the
lead's full-corpus run.

1. **The rows are one work item.** ≥ 99% of the access nodes carrying a rows-5/6
   line carry **both** lines, and the unpaired residual lands in the
   *NotEmitted* / *Unaligned* / *NoNode* buckets rather than in *the other line
   gapped for a DIFFERENT reason*.
2. **The receiver-shape split is dominated by `Named without members`** — the
   largest single bucket, ≥ 40% of the total — with `Intrinsic + Literal`
   second at **10–25%** and `has members, name absent` at **10–20%**.
3. **`Intrinsic + Literal` is between 800 and 2,000 lines.**

**Mechanism.** (1) rests on `types_producer::type_at_location`
(`crates/tsr-conformance/src/types_producer.rs:299-315`), which types the `b` of
`a.b` by *calling* `check_property_access_expression` on the access — the two
lines are literally the same call's answer, so a fix that changes one changes
the other. (2) and (3) rest on the `_` arm quoted in §1 and on the call-path
precedent, where the same `_` arm split 1,985 callee nodes 216 / 474 / 168 /
1,011 / 116.

**What must NOT move.** All five control buckets stay at zero — in particular
`reason says no property, lookup finds one`. The probe's row totals reconcile
with the board's 4,016 and 4,005; if they do not, nothing else on this page is
quotable and the discrepancy is the finding. And the *shape* roll-up equals the
row total exactly, since an exhaustive match cannot lose a line.

**How it could be right for the wrong reason.** Three ways, and each is why a
specific thing was built:

- **Pairing reads ~100% because the probe forces it.** If both rows were keyed
  by the same node id unconditionally, the answer would be 100% whatever the
  corpus contains. MU3 is that bug, applied deliberately, and it goes red.
- **Pairing reads ~100% and the fix is still not one fix.** Same nodes is not
  the same as same answer. The reading of `type_at_location` above is what
  closes that gap, and it is a code reading rather than a measurement — if the
  walker ever types a member name independently of its access, prediction (1)
  becomes true and worthless on the same day.
- **`Intrinsic + Literal` lands in range because the corpus is full of
  `"str".length`, not because `getApparentType` is the blocker.** The probe
  prints the receiver's printed type and the property name that was not found
  for exactly this bucket. If the names are not lib members of `String`,
  `Number`, `Boolean`, the diagnosis is wrong however well the count fits.

**And the trap this brief names explicitly**: a measured population is a
**ceiling, not a predicted conversion**. Two misses this session quoted a
population as a conversion, one with a range whose floor was 100% of the
population. Nothing above claims that closing `getApparentType` converts
`Intrinsic + Literal` lines. It claims those lines are the *most* it could
convert, and the fraction is unknown until something is built — every one of
those receivers still needs the global `String`/`Number`/`Boolean` interface to
resolve and its member to type, and `getApparentType` is the first link of that
chain, not the last.

---

## 4. What is already known about the intrinsic bucket, and its limit

`docs/architecture/checker-notes-overloads.md` records `getApparentType` as
unported and names it as one of two populations blocked *upstream* of global
merging — "`Array.from` and `Object.entries` still answer `error` after merging,
the first through generics, the second because its type is a tuple". That claim
was **re-verified for this page, not copied**: `getApparentType` is
`checker.go:21729` at the pinned commit and there is no `get_apparent_type`
anywhere in `crates/` — `grep -rn apparent crates/ --include='*.rs'` returns
doc comments in `members.rs` (3), `expressions.rs` (1), two `tsr-checker` tests,
and unrelated `tsr-dts`/`tsr-declarations` uses of the word for a different
concept ("syntactically apparent type"). **No implementation.**

The upstream function is a flag switch, and the arms this port's receivers would
reach are four lines of it:

```go
case t.flags&TypeFlagsStringLike != 0:  return c.globalStringType
case t.flags&TypeFlagsNumberLike != 0:  return c.globalNumberType
case t.flags&TypeFlagsBooleanLike != 0: return c.globalBooleanType
case t.flags&TypeFlagsBigIntLike != 0:  return c.getGlobalBigIntType()
```

`StringLike` is what makes the [`Literal`] bucket a sibling of the intrinsic
one rather than a leftover: a string literal type is `StringLike`, so `"abc"`
and `string` take the *same* arm and reach the same wrapper interface.

**The dependency that bounds it, stated before any work:** those arms answer
`globalStringType`, which is the declared type of the global `String` interface
from `lib.d.ts`. Global merging landed (`checker-notes-overloads.md`), so the
interface exists in a corpus program — but the unit harness in
`crates/tsr-checker/tests/` loads **no lib files**, so any fixture naming
`Array`, `Promise`, `number[]` or `String` there measures the missing library
rather than the change. That confound produced two false findings before a
one-line control caught it, and the control is
`var x: number[]; x => error`. Any test written for this item declares its own
`interface String { … }` and carries that control beside it.

---

## 5. The measurement

Full corpus at `b484ea9`, isolated worktree, own target directory: **9,538 cases
judged, 478,954 upstream lines, 291,799 exactly matched (60.92%)**. That last
figure matches the lead's independent tip run, so the probe is on the gradient's
population and not on a resembling one — the check `writer_guards` failed.

**All five control buckets read zero.** A sixth was added afterwards; see §5.3.

```
row 5  member name, the receiver has no such property      4016
row 6  property access, the receiver has no such prop.     4008
sum                                                        8024
```

### 5.1 The prediction, scored: 1 hit, 2 misses

| # | predicted | measured | verdict |
|---|---|---|---|
| 1 | ≥99% of access nodes carry both lines; residual not `different reason` | **4,008 of 4,016 (99.8%)**; residual 2 unaligned + 6 RIGHT, **0 different reason** | **HIT** |
| 2 | `Named without members` largest at ≥40%; `Intrinsic + Literal` second at 10–25%; `has members` 10–20% | largest ✓ but **37.2%**; `Intrinsic + Literal` **fifth at 9.6%**; `has members` **19.7%** ✓ | **MISS** |
| 3 | `Intrinsic + Literal` is 800–2,000 lines | **774** | **MISS** |

Scored honestly and the pattern is the one `docs/conventions.md` predicts. The
hit was the leg with a **mechanism plus a code reading that could have been
wrong** — `type_at_location:299-315` types the `b` of `a.b` by *calling*
`check_property_access_expression`, so the two lines are one call's answer. The
misses were the two legs whose only support was the call-path precedent, i.e. a
mechanism story with no cross-check, which is 12-for-12 the losing method.

**The shape I did not predict at all is the finding.** `union or intersection`
is **2,235 lines, 27.9%**, the second-largest bucket and the largest that is
neither parked nor an upstream error. Nobody had reasoned about it — including
the call-path measurement, where composites were 8.5% and unremarkable.

```
  1581  19.7%  has members, name absent      NOT OURS — upstream errors too
  2988  37.2%  Named without members         PARKED (bd tsr-4qx)
  2235  27.9%  union or intersection         <- largest actionable
   562   7.0%  intrinsic (getApparentType)
   446   5.6%  Anonymous, name absent in exports
   212   2.6%  literal (getApparentType)
```

**Actionable ≈ 3,455 lines, not 8,024 and not 4,016.** The call path's
"89% was a lookup that never ran" reproduces here at one level up, with
different arithmetic: **19.7% is a genuine missing property** against 10.9%
there.

### 5.2 Three lines that `rank_board` and this probe appeared to disagree on

**Resolved in §6.1 — the hypothesis below was confirmed to the line. Kept as
written, before the answer was known.**

The board reads 4,016 and **4,005**; this probe reads 4,016 and **4,008** at a
commit with no checker change in between (the gradient is 291,799 at both). Row
5 agrees exactly; row 6 is off by three.

**Three lines is 0.07% and §6 of this page says an unreconciled numerator gap
makes every share on the page unquotable.** That rule is applied to this page's
own numbers rather than only to other people's: the shares in §5.1 are reported
because the *lead* re-derived the population independently, and the ownership
conclusion in §5.4 does not turn on three lines — but the disagreement is open
and is being measured, not argued.

**The hypothesis, and it is not yet confirmed.** `rank_board` keys every gap by
`(row_key, cause)` and prints **one ranking section per cause**
(`GRADIENT RANKING A — TERMINAL`, `B — PROPAGATED`, `C — DEPENDENT-UNKNOWN`).
Its 4,016 appears under A and its 4,005 under B, so **4,005 is row 6's
*propagated/span* count and not row 6's total**. If three row-6 lines are
TERMINAL, they are a 3-line row in ranking A — and ranking A is truncated at
261 lines in the saved output, so it would not print. That is consistent with
the evidence and is exactly the kind of "consistent" this page refuses to call
confirmation.

Two independent facts support it and neither settles it: two earlier
`rank_board` runs at another commit read **4,014 / 4,006**, a difference of
**8** — the same difference this probe measures at 4,016 / 4,008 — while the
board's published pair differs by 11.

The probe now buckets rows 5/6 by `rank_board`'s own span test, copied rather
than re-derived, and **names** the row-6 TERMINAL lines with their case and
source text. One run settles it. If the count is not exactly 3, the hypothesis
is wrong and something else is being counted differently.

### 5.3 A bug I made, and the control that now catches it

The span test was copied correctly and **its polarity was not**. `rank_board`
binds `below[i] = true` to mean *nothing gapped below* and then passes
`!gapped_below` into `cause()`. Reading the expression without its use, I
mapped `true` to `propagated/span`, and the capped run printed row 5 as 100%
propagated and row 6 as 100% terminal — a table that is perfectly plausible if
you do not already know that **row 5 is a leaf**.

That is the fix and it is also the control. Nothing can be nested strictly
inside the span of the `b` in `a.b`, so `propagated/span` is impossible for row
5 *by construction*, and a sixth control bucket now prints it:

```
CONTROL row 5 (a LEAF) counted as propagated/span    = 0
```

Under the inverted polarity, applied deliberately as mutation **MU4**, it reads
**65** on a 400-case subset instead of 0. This is a *structure-pinned* control
rather than an arithmetic one, and §6.4 works out why that distinction is a
class — it is the repair for `docs/conventions.md`'s
*"A control bucket over a classifier whose last arm is a default cannot fire"*. This is the general lesson restated:
copying an expression is not copying its meaning, and the cheapest guard against
that is a bucket whose value is fixed by the *structure* of what is measured
rather than by the measurement.

### 5.4 Ownership: the union bucket is not obviously ours, and the names say so

The lead's challenge, which is `docs/conventions.md`'s "a row named after a
mechanism is usually not about that mechanism" arriving a fourth time. The
property names in the composite bucket are `foo` 340, **`kind` 222**,
`length` 184, `type` 72, and the top cases are `controlFlowOptionalChain` (372,
16.6%), `controlFlowAliasing`, `discriminatedUnionTypes1`,
`discriminantPropertyCheck`. **`kind` and `type` on a union, in files named
after discriminant narrowing, is a narrowing signature.** If upstream never had
a union at that position, the owner is `crates/tsr-checker/src/flow.rs` and not
this file.

So the probe now answers it two ways, and the second is the decisive one:

1. **Does upstream even have our type there?** Upstream's assertion for the
   *receiver's own position* against our printed receiver type, classified as
   `same` / `same constituents, different print order` / **`upstream is
   NARROWER`** / `differs another way`. On a 1,500-case subset the raw pairs
   were dominated by `DIFFERS`, and classifying them showed that reading to be
   worthless as it stood: `upstream Set<number>` vs `ours Set<number> |
   Set<string>` is narrowing, but `upstream string[] | number[]` vs
   `ours number[] | string[]` is **our union sort order**, and
   `upstream { type: 'string'; … }` vs `ours Nested` is **alias printing**.
   Three different findings under one `DIFFERS`.
2. **Probe past the blocker, not at it.** `docs/conventions.md`: a probe at the
   blocker only shows it is live; ask what the form answers once it is removed.
   The probe strips the nullish constituents upstream's `checkNonNullExpression`
   removes before the lookup, then asks `get_property_of_type` of **each
   remaining constituent**, and applies upstream's own rule
   (`getPropertyOfUnionOrIntersectionType`): a union answers only if *every*
   constituent has the property, an intersection if *any* does. The three
   outcomes are `a distribution arm WOULD find it` (ours, `members.rs`),
   `some constituents have it` (upstream errors too), and **`NO constituent has
   it`** — in which case the union is *not* the blocker and the work is
   somewhere else entirely.

On the 1,500-case subset that split 46.4% / 3.6% / **50.0%**, which is why no
number from it is quoted here and why the corpus run is worth its cost: half the
bucket may not be a union problem at all.

### 5.5 Pre-registered for run 2, before it is run

Registered now so it cannot be fitted afterwards. The composite bucket was
**not** pre-registered before run 1 — it was not a bucket anyone expected to
matter — and inventing a rule for it after seeing 2,235 is exactly the failure
`docs/conventions.md` records as "the first version of this method became fitted
by being invented to explain a number already seen".

- **Reconciliation, and it gates everything else.** Row 6 must read **4,005
  propagated/span + 3 TERMINAL**, and the three named lines must be real. Any
  other split leaves the two instruments unreconciled and this page's shares
  stay unquotable.
- **Build the union distribution arm in `members.rs` only if** `a distribution
  arm WOULD find it` is **≥ 1,200 lines** (over half the bucket) **and**
  `upstream is NARROWER` is **≤ 33%** of the bucket. Both legs on the
  sub-population, never the parent's.
- **If `NO constituent has it` is the largest outcome, this is not a
  `members.rs` item** and the finding is that the composite bucket chains into
  the constituents' own member tables — `bd tsr-4qx` and the apparent type — and
  should be reported as such rather than claimed.
- **The case axis is already decided and it is negative.** The composite
  bucket's level-4 `0` bucket is **1.1%** (2 of 189 cases) with a median of 19
  other failing lines — the **worst profile on the page**, and worse than the
  parent row's 6.8%. Whatever run 2 says, **this is a gradient item or it is
  nothing**; it must not be sold as a case item. The healthiest `0` bucket on
  the page, 14.4%, belongs to `has members, name absent`, which is the one shape
  that is *not* our defect.

### 5.6 The pre-registered gradient rule fired, and it said no

§3 registered: build `getApparentType` if `Intrinsic + Literal` is ≥ 1,000 lines
**and** its top-1 case share is ≤ 25%. Measured: **774 lines** (562 + 212),
top-1 4.3% and 17.9%. The concentration leg passes and **the size leg fails**,
so the rule says **do not build it**, and it is not being built.

Recording that plainly matters more than the decision does. The rule was written
before the number existed, the number came in 23% under the floor, and the
temptation to read "774 ≈ 1,000, and the concentration is excellent" is exactly
what a pre-registration is for. The property names confirm the *diagnosis* was
right — `toString` 112, `length` 100, `charAt` 38, `toUpperCase` 28 on receivers
printing `string` 248, `number` 150 — so this is a correctly identified item
that is simply smaller than the bar. `getApparentType` stays unbuilt and
correctly diagnosed, which is a better outcome than a fitted rule that licensed
it.

---

## 6. Run 2, and the verdict

Full corpus at `897cc45`, isolated worktree. **All six control buckets read
zero**, including the new structure-pinned one.

### 6.1 The reconciliation: pre-registered as 4,005 + 3, measured 4,005 + 3

```
  4016  row 5 (member name)      TERMINAL
     0  row 5 (member name)      propagated/span
     3  row 6 (property access)  TERMINAL
  4005  row 6 (property access)  propagated/span
```

**There was never a disagreement.** `rank_board` prints one ranking section per
cause — `rank_board.rs:547` filters `Cause::Terminal`, `:559` filters
`PropagatedSpan | PropagatedNamed` — so its 4,005 is row 6's *propagated* count
and the three TERMINAL lines sat below Ranking A's truncation. The two
instruments agreed completely and always had.

The three lines are named rather than counted, which is the part that turns an
explanation into a check:

```
compiler/parse1:                      bar.
conformance/classAbstractCrashedOnce: this.
conformance/parser509667:             this.
```

All three are **`a.` with no name at all** — property accesses in files whose
names say they are parser error-recovery cases. The access node exists, its name
child does not, so nothing gapped strictly inside the span and the line is
TERMINAL. That is a satisfying end: the residual is a parse artefact, not a
checker behaviour, and it could not have been guessed from the counts.

**Holding this open was worth more than the three lines.** Three lines in eight
thousand is 0.07% and is exactly the size of thing that gets waved through as
noise. What came back instead was a *mechanism* that predicted the split before
it was seen and now documents, for the next reader of the board, that its rows
are **per cause** — so a row's headline is not its total.

### 6.2 Both legs of the union gate fail

§5.5 registered: build only if `WOULD find` ≥ 1,200 **and**
`upstream NARROWER` ≤ 33%.

```
  1122   50.2%  a distribution arm WOULD find it (ours, members.rs)   <- 1,122 < 1,200  FAIL
   512   22.9%  some constituents have it (upstream errors too)
   601   26.9%  NO constituent has it (the union is not the blocker)

   908   40.6%  upstream's receiver type is ours, exactly
    34    1.5%  same constituents, different print order (ours)
   784   35.1%  upstream is NARROWER (narrowing — flow.rs, not ours)  <- 35.1% > 33%   FAIL
   507   22.7%  differs another way (alias printing, or a real difference)
```

**Both legs, not marginally one.** Under the bar on the count *and* over it on
the narrowing share.

**And none of the three branches I registered describes the outcome.** I named a
build case, a fail case, and a "`NO constituent` dominates, so it is not ours"
case. What happened is a fourth: **the largest single outcome is ours — 1,122
lines a distribution arm would find today — and it is still under the bar.**
Writing that plainly rather than rounding it to the nearest registered branch is
the point of registering them; a pre-registration that gets reinterpreted to fit
is not one.

The lead's narrowing challenge lands at **35.1%** and that is worth stating
precisely rather than as a win for either side: it was enough to fail the gate
and it is **not a majority**. About a third of the composite bucket is
`flow.rs`, and the rest splits three further ways, including **34 lines of our
own union sort order** and 507 of alias printing or a real difference. The raw
`DIFFERS` column conflated all of that under one label, and only the second cut
showed it — by cutting, not by arguing.

Two numbers nobody asked for and both are diagnostic: **324** of the composite
lines are an optional chain `a?.b`, and **676** had `undefined` or `null` in the
union. Upstream removes the nullish constituents in `checkNonNullExpression`
*before* the lookup, so a meaningful slice of this bucket is a non-null step
rather than a distribution step, and the two are different work.

### 6.3 The verdict: nothing to build, by rules written before the numbers

| item | gate, registered before the number existed | measured | outcome |
|---|---|---:|---|
| `getApparentType` | ≥ 1,000 lines and top-1 ≤ 25% | **774** | **declined** |
| union distribution arm | ≥ 1,200 lines and narrowing ≤ 33% | **1,122** / **35.1%** | **declined** |

So the largest apparently-distributed item on the gradient board — 8,024 lines,
1,060 cases, top-1 4.7% — yields **no `members.rs` item above the bar**, and
this workstream ends with its own files untouched.

**A rule that only ever licenses is not a rule.** That was written in §5.6 about
`getApparentType` and it now applies twice over, the second time at the cost of
the workstream that wrote it. Both numbers were close enough to their bars —
774 against 1,000, 1,122 against 1,200 — that a sentence of reinterpretation
would have carried either one over, and the only thing preventing that is that
the bars existed before the numbers did. Neither was tuned afterwards.

What survives is not nothing:

- **rows 5 and 6 are one work item**, measured node by node (99.8%, zero gapped
  for a different reason), so the board can stop listing them as two candidates;
- **the board's rows are per cause**, so a row's headline is not its total;
- **19.7% of the row is not our defect at all**, and 37.2% is parked;
- and the composite bucket, which nobody had reasoned about, is now measured
  three ways instead of being available for a future estimate to guess at.

### 6.4 The control that caught the polarity bug, and why it is a class

`docs/conventions.md`, *"A control bucket over a classifier whose last arm is a
default cannot fire"*, records the complementary defect found this cycle:
`rank_board`'s `UNATTRIBUTED` control is structurally incapable of reading
anything but zero, because its classifier is total — it read zero on every run
and proved nothing on any of them.

The control added in §5.3 is the repair for that failure mode and it is worth
naming as a class. **A control pinned by arithmetic** — "these buckets must sum
to that total" — can only catch a bookkeeping slip, and cannot fire at all if
the classifier is total. **A control pinned by construction** — row 5 is a
*leaf*, so nothing can be nested strictly inside its span, so `propagated/span`
is impossible for it whatever the corpus contains — is fixed by the structure of
the thing being measured rather than by the measurement, and it fires on a
*semantic* inversion that leaves every sum intact.

That is exactly what happened: the inverted polarity produced a table where
every total still reconciled and every arithmetic control still read zero. Only
the structure-pinned bucket moved, from 0 to 65.

So, as a rule to carry: **for each control, ask what input would make it
non-zero, and prefer the one whose answer is fixed by a property of the subject
rather than by the classifier.** A leaf has nothing inside it; that is true
before any code runs.

---

## 7. How you would know this page is wrong

- **A control bucket reads non-zero.** Then the buckets are a list of true facts
  rather than a partition, and §3's rule has nothing to stand on.
- **The row totals do not reconcile with 4,016 / 4,005.** Either the probe's
  denominator is not the gradient's, or the board moved; both make every share
  here a share of an unnamed population. `docs/conventions.md` records
  `writer_guards` quoting a rate over its own population as the gradient's, 2.7
  points off, for four documents.
- **Pairing is high and the unpaired residual is `the other line gapped for a
  DIFFERENT reason`.** Then the rows share nodes and not work, and the sum is
  refused for the same reason the 2.7× over-count was.
- **`Intrinsic + Literal` clears the gradient bar and the property names in that
  bucket are not `String`/`Number`/`Boolean` members.** The count would be right
  and the diagnosis wrong, which is the failure this project keeps recording.
  *Measured: it did not clear the bar (774 against 1,000) and the names are
  `toString`, `length`, `charAt`, `toUpperCase`. Right diagnosis, too small.*
- **The row-6 TERMINAL count is not 3.** §5.2's hypothesis is that `rank_board`
  prints per cause and its 4,005 is a cause bucket. If run 2 reads any other
  number, that explanation is wrong, and the two instruments disagree about
  something not yet identified — in which case §5.1's shares are the thing to
  stop quoting first. *Measured: exactly 3, and the three lines are `a.` with no
  name child, in parser error-recovery cases. §6.1.*
- **The composite bucket's `NO constituent has it` outcome dominates.** Then the
  2,235 lines are not a union-lookup item, the largest actionable bucket on this
  page dissolves into `bd tsr-4qx` and the apparent type, and the honest
  statement is that rows 5/6 contain **no `members.rs` item above the bar at
  all**. That is a real possible outcome of run 2 and it is written here before
  it is known. *Measured: it did not dominate — `WOULD find` did, at 50.2% — and
  the conclusion is the same anyway because 1,122 is under the 1,200 bar. The
  outcome arrived by a route none of the three registered branches named, which
  is recorded in §6.2 rather than rounded to the nearest one.*
- **Someone quotes 8,024 as a `members.rs` opportunity.** It is the size of the
  two rows and it is not the size of any item: 19.7% is upstream's own error,
  37.2% is parked on `bd tsr-4qx`, and both gates over the remainder fired
  negative. The number is a **ceiling on a population**, never a conversion —
  the trap `docs/conventions.md` records two agents falling into in one session.

## §523 — the late-bound accessor pair reconstructed (+4 cases, 5,862 → 5,866, 61.50%; +10 W→R, 0 adverse)

The binder gives every computed name its own `__computed` symbol (the §383
method precedent), so a `get [Symbol.toPrimitive]` / `set` pair splits and
the setter's symbol never saw the getter upstream's late-bound merge reads —
`set [Symbol.toPrimitive](x)` printed `any` where the getter's inferred
`string` is the answer, and §441's unannotated-setter-parameter road starved
the same way. `get_type_of_accessors_worker` now reconstructs the pair
through §383's sibling walk (same spelled name, accessor kinds) before the
getter/setter split. Wins: symbolProperty47, symbolDeclarationEmit4/10/11.

---

## §584 — a `TypeLiteral` property name that is not an identifier (+161 lines, 0 adverse)

`get_type_from_type_literal` (`declared.rs`) resolved a `PropertySignature`
name with a one-arm match — `Identifier` — and a `_ => return error` catching
everything else. A returned `error` is not a local decline: it is the type of
the WHOLE literal, printed `any`. So

```ts
var a: { 1: number; 1: number; }        // numericNamedPropertyDuplicates
var x: { "data-foo"?: string; }         // tsxAttributeResolution7
```

printed `any` where upstream prints `{ 1: number; }` and
`{ "data-foo"?: string; }`.

### The find

This did not come from reading `declared.rs`. It came from **joining the `any`
audit against the single-transition population** — the 547 cases whose only
non-right line is one line, so that converting it flips a case. 180 of those
547 print `any`, and `any_audit`'s ranked rows could not say which arm to build
because its rows are diffuse (top-1 under 4%, each row spread over hundreds of
cases that fail for other reasons too). Ranking those same rows **by their
intersection with the single-transition set** is a case forecast rather than a
line count, and it is what put a one-arm match at the top of a 9,538-case
board. `TSR_ANY_DUMP=1 cargo run --release -p tsr-conformance --example
any_audit` writes `target/any_lost_lines.tsv` for that join; the key is
`verdict.rs:70`'s, byte-for-byte.

**The attributed REASON on those rows was wrong and did not matter.** 25 of the
42 function-typed wants carried "declaration name -> shorthand ambient module",
which is nonsense for a plain `function*` — they are `UNCLASSIFIED/DISAGREEMENT`
rows, and that label means precisely *the classifier's reason does not explain
this line*. The rows were still the right ranking, because what was being
ranked was the **population**, not the explanation. A probe can be worth using
with a broken column in it, provided the broken column is the one you are not
reading.

### The answer

The object-literal road (`objects.rs:1132`) had already solved this exact
question — unquoted when the name is identifier-valid, `printing::quote`
otherwise, the §77.3 single-quote rule, `printing::normalise_number` for a
numeric name. `objects::written_property_name` is that spelling extracted;
`declared.rs` calls it for the `StringLiteral` and `NumericLiteral` arms.

```
TOTAL 474196  right 433956  gap 8450  wrong 31790
GAP->RIGHT: 31   WRONG->RIGHT: 130   (no adverse transition of any kind)
  assignmentCompatWithObjectMembersStringNumericNames 31, numericIndexingResults 28,
  assignmentCompatWithObjectMembersNumericNames 15, unionTypeWithIndexSignature 11,
  objectTypeWithStringNamedPropertyOfIllegalCharacters 8
```

161 lines from an arm that is four lines of dispatch, because a whole-literal
decline is leveraged: one unspellable member costs every line the literal
appears on.

### The duplication, stated rather than hidden

The object-literal road still carries its own inline copy of the four arms.
Wiring it through the extracted function means restructuring a match whose
other arms `continue` and contribute index signatures, on a road with **no
measured defect** — churn for no conversion, so it was not done. Both copies
call the same `printing::quote`/`printing::normalise_number`, so the escape
table stays single-sourced; what is duplicated is the dispatch. If they ever
disagree, the extracted copy is the one to delete.
