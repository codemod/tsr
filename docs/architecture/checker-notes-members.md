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

**This page is a pre-registration and an instrument, not yet a measurement.**
The open issue is `bd tsr-gzx`.
`crates/tsr-conformance/examples/member_shapes.rs` exists, its classifier is
mutation-checked, and it has been exercised on capped subsets. The corpus run is
the lead's, serialised, in an isolated worktree. **Every number below that is
labelled *subset* is from a capped run and is not the answer** — the caps are
alphabetical prefixes of the corpus, which is the worst possible sampling for a
corpus whose case names cluster by feature.

Nothing in `crates/tsr-checker/src/members.rs` or `indexed.rs` has changed. No
behaviour change has been made, so there is nothing here to falsify by mutation
except the instrument, which is mutation-checked in §2.

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

### Five control buckets, printed unconditionally

- rows-5/6 lines that reached no receiver shape
- the line's node is not a property access or its name child
- **the reason says no such property and `get_property_of_type` finds one** —
  the sharpest of the five: `access_reason` reaches this row only when the
  lookup answered `None`, so a hit means the probe and the producer have drifted
  and nothing below it is a partition
- shape roll-up − rows 5/6 total
- pairing − rows 5/6 total

There is deliberately **no `other` receiver-shape bucket**. The match producing a
shape is exhaustive over `TypeData`, so a new type variant is a compile error
rather than a silent row of leftovers — the stronger form of the same control. A
runtime bucket that can only ever read zero is decoration, which this project
deletes.

### Three named mutations, each red, none reddening another

Checked in `main` on every run rather than under `#[cfg(test)]`, for
`rank_board`'s reason: Cargo does not run tests inside an example without a
manifest change, and `crates/tsr-conformance/Cargo.toml` is shared.

| mutation | `grep -c` before running | result |
|---|---|---|
| **MU1** — `row_of` drops the `has no such property` guard, admitting `the receiver is a gap` | 1 | red at the gapped-receiver assertion; would have added ~13,500 lines of rows 3/4 to the population |
| **MU2** — the cheap cut matches an intrinsic name *anywhere* in the printed form | 1 | red at `Promise<number>` → `Named`; `Promise<number>` contains `number` |
| **MU3** — both prefixes map to `Row::MemberName` | 1 | red at the access-row assertion; pairing would read 100% by construction |

Each panicked on its **own** assertion with the others passing. MU3 is the one
that matters most: it is precisely the bug that would make the pairing result
true by construction, which is the way this measurement could be right for the
wrong reason.

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

## 5. Subset readings — NOT the measurement

Recorded so the full run can be compared against something, and labelled
because an alphabetical prefix of the corpus is the worst possible sample of a
corpus whose case names cluster by feature.

| capped run | cases judged | rows 5+6 | paired | unpaired |
|---|---:|---:|---:|---:|
| `TSR_MEMBER_SHAPES_LIMIT=150` | 112 | 10 | 100% | 0 |
| `TSR_MEMBER_SHAPES_LIMIT=400` | 333 | 130 | 100% | 0 |
| `TSR_MEMBER_SHAPES_LIMIT=1200` | 1,004 | 372 | 100% | 0 |

Every control bucket read zero in all three. The gradient over the 1,004-case
prefix reads 66.31%, against the board's corpus-wide 60.83% — the prefix is
easier than the corpus, which is the sampling bias stated above showing itself
in the one number that can be compared. **That difference is the reason none of
the shape shares below should be quoted.**

Shape split on the 1,004-case prefix, for orientation only: `Named without
members` 54.3%, `has members, name absent` 17.2%, `intrinsic` 12.4%,
`Anonymous` 7.5%, `union or intersection` 5.4%, `literal` 3.2%.

---

## 6. How you would know this page is wrong

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
