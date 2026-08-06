# Members on an instantiated generic type: why the slice is not one edit

> **Correction, 2026-08-05, and it moves the item.** The section "Coverage,
> honestly" below asked for the 557 `member types as error` nodes to be split by
> whether the receiver carries type arguments. **That split is zero by
> construction and asks the wrong row.** A receiver carrying type arguments has
> no member table, so `get_property_of_type` returns `None` before it reads the
> name and the node lands in `no such member` — never in `member types as
> error`. The population instantiated members acts on is inside the **1,985**,
> and almost certainly inside the **1,011** `Named`-without-members sub-row that
> `checker-notes-calls.md` already measured as 43.4% `Promise`. See
> [the counter section](#the-counter-bd-tsr-fua-and-why-it-had-to-move-rows).
> The same wrong claim is in `checker-notes-recv.md` ("97 of the 557, 17.4%");
> that page is another agent's and has not been edited here.

> **Second correction, same day: the `bd` id was wrong too.** This page, its two
> siblings and several source comments cited **`bd tsr-el3.2`** for instantiated
> members. `tsr-el3.2` is *"Checker: port upstream's algorithmic recursion
> limits"*; the instantiated-members item is **`bd tsr-4qx`**. The wrong id had
> been copied forward through three documents and into commit messages, and it
> was caught only by running `bd show` instead of trusting the citation — which
> is the same failure `docs/conventions.md` records under *"The anchors gate does
> not see your briefing"*, one identifier class over. `cargo run -p xtask --
> anchors` validates upstream line numbers and does **not** validate `bd` ids, so
> nothing would have caught this. The citations in this workstream's files are
> corrected; `checker-notes-recv.md` still carries the wrong one. Note that the
> two items are genuinely related — `tsr-4qx` step 3 pulls in `tsr-el3.2`'s
> `instantiationDepth` guard — which is exactly why the confusion survived.

Status: living, 2026-08-05. **Steps 1 and 2 are built** and change no answer;
**steps 3 and 4 are not built** and `bd tsr-4qx` is parked on a measurement —
level 4 says closing all 929 nodes finishes zero cases at today's tip. The
original status line read *"designed, no production code changed"*, which was
true when this page was written and is kept here as the record of that.

Written for the item `bd tsr-4qx` names and that
[`checker-notes-recv.md`](checker-notes-recv.md) demonstrated is the blocker for
the typed-receiver calls — **not** for the 557, as the first correction above
records. The finding below is why the first agent did not build it, stated so the
next one starts from the seam rather than from the symptom.

## The one-line finding

`create_type_reference` passing `members: None` is **not** the blocker; it is the
only *safe* answer given where members currently live. Making `C<number>` carry
members is a two-line change that produces confidently wrong types at three call
sites unless a substitution seam is introduced first — and there is no such seam,
because **member lookup in this port returns a `SymbolId`, and a symbol has
nowhere to hold an instantiation.**

That is not a new discovery. It is the falsifier `crates/tsr-checker/src/members.rs`
wrote down for its own design, verbatim:

> **The consequence accepted:** the moment `getDeclaredTypeOfClassOrInterface`
> grows real member resolution — the instantiated members of `class C extends
> B<number>` — this must move to the type level, because a symbol has no place
> to hold an instantiated table. That is the falsifier for this shape.

**The falsifier has fired.** This page is the record of that, which is the thing
`docs/conventions.md` asks for and which no later diff will recover.

## What upstream does, verified against the pinned submodule

`grep -n`, `vendor/typescript-go/internal/checker/checker.go`:

| function | line |
|---|---|
| `getTypeOfPropertyOfType` | `18951` |
| `resolveTypeReferenceMembers` | `19095` |
| `instantiateType` | `22100` |
| `createTypeReference` / `createTypeReferenceEx` | `25103` / `25107` |

`resolveTypeReferenceMembers` (`:19095`) reads the target's `allTypeParameters`
and the reference's `typeArguments` and hands both to `resolveObjectTypeMembers`,
which builds a `TypeMapper` and **eagerly instantiates a whole new
`ast.SymbolTable`** onto the reference type. Note the fast path at `:19113`: when
`slices.Equal(typeParameters, typeArguments)` it reuses `declaredMembers`
untouched — upstream itself treats "no substitution needed" as a distinct,
cheaper case.

So upstream's answer is *a member table hanging off the type*. Ours has no such
thing: `TypeData::Named { members: Some(owner) }` holds a `SymbolId`, and
`get_property_of_type` walks symbol → base symbol → symbol
(`crates/tsr-checker/src/members.rs:163`). Two references to the same generic
share one symbol, so any instantiated table written there would be wrong for the
other. This is the ADR-0003 shape argument arriving at members: the instantiated
table is id-keyed by `TypeId`, not by `SymbolId`.

## Why the obvious two-line version is unsafe

The obvious change is `declared.rs`'s `create_type_reference` passing
`Some(symbol)` instead of `None` to `new_named`, then substituting at the
property-access site with the existing `Checker::instantiate_type`
(`crates/tsr-checker/src/inference.rs:262`) and the reverse index
`Checker::type_reference_targets` (`crates/tsr-checker/src/checker.rs:108`). The
substitution half genuinely is already built: arm 1 maps `T := number`, arm 2 is
the identity on a member that mentions no type parameter, and everything else
answers `errorType`. That is exactly fixtures `A`/`B` of `checker-notes-recv.md`.

The problem is that **`get_property_of_type` has four production callers, and
only one of them is the property-access site**:

```
crates/tsr-checker/src/members.rs:108     property access  a.b
crates/tsr-checker/src/indexed.rs:117     element access   a["b"]
crates/tsr-checker/src/relater.rs:389-390 structural relate, both sides
crates/tsr-checker/src/calls.rs:390       classification counter only
```

Each of the first three turns the returned `SymbolId` into a type on its own. A
member typed `string` on `C<number>` is fine everywhere; a member typed `T` would
answer **`T`** at `indexed.rs:117` and at both `relater.rs` sites — a confident
wrong type, and at the relater it is the dangerous direction described under
"A conservative `false` is safe for one kind of consumer and unsafe for the
other" in `docs/conventions.md`: the relater acts on a negative. Instantiating
only at `members.rs:108` leaves the other two wrong, which is strictly worse than
today's uniform `error`.

## The slice, stated so it can be built

1. **Introduce the seam.** `Checker::get_type_of_property_of_type(receiver: TypeId,
   name: &str) -> Option<TypeId>`, anchored to `getTypeOfPropertyOfType`
   (`checker.go:18951`) — upstream has exactly this function and it is the one
   that returns a *type*. Body: `get_property_of_type`, then `get_type_of_symbol`,
   then instantiate against the receiver.
2. **Route all three type-taking callers through it** (`members.rs:108`,
   `indexed.rs:117`, `relater.rs:389-390`). `calls.rs:390` only asks existence and
   stays. This step changes no answer on a non-generic receiver and is the one
   that makes step 3 safe; it is worth committing separately for that reason.
3. **Instantiate inside the seam.** Look the receiver up in
   `type_reference_targets`. On a miss, the type is not a reference and the
   member's type is returned unchanged — today's behaviour exactly. On a hit,
   build the map from the *target's* type parameters to the arguments and call
   `instantiate_type`; `errorType` out means `errorType` for the member.
4. **Only then** flip `create_type_reference` to `Some(symbol)`.

Step 3 needs one thing that does not exist: the type parameters of a **class or
interface** declaration as `TypeId`s with names.
`Checker::type_parameter_types` (`inference.rs:314`) does this for function-like
declarations only — its `match` covers eight function-like node kinds and returns
`None` otherwise. A sibling over `InterfaceDeclaration` / `ClassDeclaration` /
`TypeAliasDeclaration` is the missing piece, and going through each type
parameter's *symbol* (as that function already does) rather than matching by
printed name is what keeps two `T`s apart.

Deliberately **out** of the slice: `fillMissingTypeArguments` and defaults
(`get_instantiated_type_reference` already answers `errorType` on an arity
mismatch and should keep doing so), the `len(typeArguments) == len(typeParameters)-1`
self-padding at `checker.go:19099`, instantiated call/construct signatures and
index infos, and instantiated *base* types (`class C extends B<number>`) — the
symbol walk in `get_property_of_declared_symbol` reaches the base symbol, not a
base reference, so an inherited member of a generic base stays `errorType`.

## The counter, `bd tsr-fua`, and why it had to move rows

**Superseded by measurement below: the paragraph in "Coverage, honestly" that
asked for a split of the 557.** It is left in place because the wrong turn is the
useful part of the record.

### The forcing fact, pinned by a test rather than asserted

`create_type_reference` (`crates/tsr-checker/src/declared.rs`) builds every
instantiated reference with `members: None`. `get_property_of_type`
(`crates/tsr-checker/src/members.rs`) matches only
`TypeData::Named { members: Some(..) }` and `TypeData::Anonymous`, and returns
`None` for everything else **before it reads the name**. Compose the two:

> A call whose receiver carries type arguments *cannot* reach
> `of which: member types as error`. It lands in `of which: no such member`,
> sub-row `receiver is Named without members`.

That is a two-hop code reading, which is exactly the kind this project has been
wrong about eight times, so it is a test:
`calls::tests::a_generic_receiver_has_no_member_table_so_the_lookup_never_runs`
asserts `get_property_of_type(P<number>, "get") == None` and that the same
interface *without* the type parameter answers `Some` — the A/A' discrimination
from `checker-notes-recv.md`, one token apart. Its named mutation is step 4
itself (`new_named(.., Some(symbol))` in `create_type_reference`), so the test is
simultaneously the falsifier for this finding and the tripwire for the change
that ends it.

The consequence for the two pages that sized this item: **`checker-notes-recv.md`
fixture A — `interface P<T> { get(): string; } declare var p: P<number>; p.get()`
— is not a member of the 557.** The demonstration is still sound; the population
it was attached to was the wrong one. The "97 of the 557" figure was arrived at
by summing `promiseType` (51) and `promiseTypeStrictNull` (46) out of the *case*
concentration of the 557, which says which files those nodes are in and nothing
about their mechanism. The Promise nodes that *are* this shape are the 122 + 122
of the **1,011**, not the 51 + 46 of the 557. This is the
"a row named after one case is about that case" error one level down, and it is
recorded here rather than quietly fixed.

### What was built instead

A second, **orthogonal** partition of `property access: receiver is typed`
(2,562), keyed on the receiver's *provenance* rather than on the member lookup's
outcome. `Checker::receiver_carries_type_arguments` asks the reverse index
`type_reference_targets` (`docs/architecture/checker-notes-subst.md`) — upstream
asks `objectFlags & ObjectFlagsReference` and reads `target` off the type
(`createTypeReference`, `checker.go:25103`, verified at `5b1047d10`), which this
port cannot, because the `(symbol, arguments)` pair is the intern map's key.

```text
      by receiver: type arguments, no member found          ?   <- the population
      by receiver: type arguments, member found (CONTROL: 0) ?  <- must read 0
      by receiver: no type arguments                        ?
```

The three sum to 2,562, which the reader can check off the printed rows; that is
the partition control. The **zero control** is the middle row, and it is not a
residual — it is reachable in principle and empty in fact for the reason above.
A zero there over 2,562 samples re-derives the invariant on every run instead of
trusting the two-hop reading, exactly as `classify_unresolved_callee`'s existing
control buckets do. It stops being a control the moment step 4 lands, at which
point it becomes the measurement of how many generic receivers newly resolve a
member — the same counter answering the before and after question.

`receiver_generic_member_absent` is also predicted to be **≤ 1,011**, since
carrying type arguments implies `Named`-without-members. If it exceeds that,
`create_type_reference` is not the only producer of `type_reference_targets`
entries and this page's model of the type store is wrong.

### How to run it

```bash
TSR_OVERLOAD_COUNTERS=1 cargo run -p tsr-conformance --example overload_funnel --release
```

No change to `crates/tsr-conformance/` was needed: the `define_counters!` macro
generates `Snapshot::rows()` in declaration order and the example prints every
row, so new counters appear without editing a file this workstream does not own.

### Measured, 2026-08-05, at `d59bee2^..d59bee2`

```text
    property access: receiver is typed              2562
      of which: no such member                      1985
        receiver is Named without members           1011
      of which: member types as error                557
      of which: member types as a non-object          20
      by receiver: type arguments, no member found   929
      by receiver: type arguments, member found (CONTROL: 0)    0
      by receiver: no type arguments                1633
UNCLASSIFIED (typed receiver)                          0
UNCLASSIFIED (no such member)                          0
```

All three checks this page asked for hold: 929 + 0 + 1,633 = 2,562 exactly, the
control reads **0**, and 929 ≤ 1,011. **Call-expression nodes, not assertion
lines** — this board has twice turned one into the other by omission.

**929 is 92% of the 1,011.** Instantiation is not merely present in that sub-row,
it is nearly the whole of it, which makes it the largest confirmed single
mechanism anywhere on the call path — and a different defect from the 557, which
this page spent its first section establishing.

The zero control is the load-bearing half of that reading. It says the 929 are
nodes where the lookup **never ran**, not nodes where it ran and failed, so the
mechanism is the one named here and not a member table that exists and is
incomplete.

### The prediction for step 4, pre-registered before it is built

Stated now so it cannot be fitted afterwards. The commit pair will be quoted as
`X^..X` when it exists.

**Mechanism.** Flipping `create_type_reference` to `Some(symbol)` gives every
instantiated reference a member table, so `get_property_of_type` runs where it
previously returned `None` before reading the name. Step 3 then substitutes the
found member's type through the seam.

**What must move:**

| row | now | direction |
|---|---|---|
| `by receiver: type arguments, no member found` | 929 | **down** |
| `by receiver: type arguments, member found` (today's control) | 0 | **up by the same amount** |
| `receiver is Named without members` | 1,011 | down by the same amount |
| `of which: no such member` | 1,985 | down by the same amount |

Those four are one arithmetic identity, not four observations.

**What must NOT move:**

- `receiver has members, name absent` — 216. Upstream reports "property does not
  exist" here too. **If this rises, step 4 is inventing members**, and that is a
  revert condition rather than a result.
- `receiver is an intrinsic` — 474 — and `receiver is a union or intersection` —
  168. Neither shape is a type reference; if either moves, `create_type_reference`
  is reaching types this page's model says it cannot.
- `arity matched, nothing assignable` — 8 — and `by receiver: no type arguments`
  — 1,633. Step 4 touches only receivers that carry type arguments.

**How it could be right for the wrong reason, and this is the important half.**
The 929 falling is *mechanically guaranteed* by step 4 and confirms **nothing**:
giving a type a member table makes the lookup run whether or not the type it
then produces is correct. Three quarters of this page exists because a number
moved for a reason nobody checked. So the 929 is not the scoreable row. The
scoreable question is **where those nodes land**:

- into `of which: member types as a non-object` or out of the funnel entirely —
  the member typed, which is the claim;
- into `of which: member types as error` — the lookup was flipped and
  **instantiation bought nothing**. The 557 would grow by roughly 929 and the
  gradient would not move. That outcome is a *failure* of this item even though
  every row above moved exactly as predicted.

**No line prediction is offered.** `docs/conventions.md` scores five of six
predictions as missing, and the sole hit was the one whose author counted
assertion lines directly and cross-checked them against the instrument before
quoting. Nothing here has counted a single assertion line — 929 is nodes — and a
number derived from it by a mechanism story is exactly the shape that missed by
6.6×, 3.6×, 2.5× twice and an order of magnitude. The cross-check that would
license one: count `.types` assertion lines in the affected baselines whose
subject is a property access or call on a receiver printing as `X<Y>`, and
validate that count against the funnel's own per-case deltas before quoting it.

### The decision this gates

- If `by receiver: type arguments, no member found` comes back **near the 439
  Promise nodes**, this is the same case-gate item `checker-notes-calls.md`
  already ranked: ~4 nodes per case, half in ten files, and worth building for
  cases rather than for the gradient.
- If it comes back **materially above 1,000**, it is the largest single
  confirmed mechanism on the call path and step 4 is the highest-value build item
  on this board.
- If it comes back **near zero**, the receivers in the 1,011 are `Named` types
  with no member table for reasons unrelated to instantiation — a different item
  — and steps 1–4 should not be built at all. The demonstration in
  `checker-notes-recv.md` would still be correct and still be about a shape the
  corpus does not contain in quantity, which is the outcome this counter exists
  to be able to report.

**Measured at 929, which sits between two of those thresholds and so does not
decide it.** The rule was pre-registered and it is being reported as
under-determined rather than rounded to whichever side is convenient — this page
would rather say "my decision rule did not decide this" than record a threshold
retrofitted to a number already seen, which is how the first version of the
sizing method in `docs/conventions.md` became fitted.

The open question is **ranking, not correctness**: 929 nodes at 92% of the row
with a zero control is strong evidence the mechanism is real, and
`checker-notes-recv.md` already demonstrated sufficiency on the shape. What is
not settled is whether it is a gradient item or a case-gate item, and that is one
command:

```bash
TSR_OVERLOAD_COUNTERS=1 TSR_FUNNEL_CONCENTRATION=receiver-generic \
  RAYON_NUM_THREADS=1 cargo run -p tsr-conformance --example overload_funnel --release
```

which needs one arm added to `concentrated()` in
`crates/tsr-conformance/examples/overload_funnel.rs` — a file this workstream
does not own. The parent 1,011 was measured at 253 cases, 4.0 nodes per case,
top ten 53.0%, 43.4% `Promise`: the narrow-and-deep signature
`checker-notes-calls.md` explicitly labelled *a case-gate item, not a gradient
item*. The default hypothesis is that the 929 inherits it, and the reason to run
it anyway is that the same check separated the 557 from *its* parent by a factor
of four and reversed the verdict. Inheriting the parent's concentration is an
assumption, and this page has already been wrong once this session by inheriting
a population instead of measuring it.

**Status of the build.** Steps 1 and 2 are landed and change no answer. Steps 3
and 4 are **not built**, and this page does not pretend otherwise — `bd
tsr-4qx` remains open. They are held on the concentration run above, because
the honest scope statement differs between "a 929-node gradient item" and "five
`Promise` baselines and a long tail", and the second is a workstream rather than
a slice.

### The concentration ran, the branch fired, and the verdict is case-gate

Measured at `d59bee2` plus the `concentrated()` arm at `9e34f3c`, single-threaded
in an isolated worktree:

```text
concentration by case: 929 over 225 cases
    122   13.1%  compiler/promiseType
    122   13.1%  compiler/promiseTypeStrictNull
     65    7.0%  compiler/promisePermutations
     65    7.0%  compiler/promisePermutations2
     65    7.0%  compiler/promisePermutations3
     36    3.9%  compiler/controlFlowArrays
     20    2.2%  compiler/mapUpsert
     17    1.8%  compiler/staticAnonymousTypeNotReferencingTypeParameter
     12    1.3%  compiler/inferFromGenericFunctionReturnTypes2
     11    1.2%  compiler/controlFlowArrayErrors
top ten hold 57.6%
```

Against the parent 1,011: **225 cases against 253, top ten 57.6% against 53.0%,
4.13 nodes per affected case against 4.0.** The `Promise` five hold 439 of 929 —
**47.3%**, up from 43.4% in the parent. The 929 does not merely inherit the
parent's shape, it is *marginally more concentrated* than it.

The pre-registered branch was: *"if the 929 lands at roughly 250 cases and
top-ten ~53%, it is the parent's shape and I will call it a case-gate item and
say so plainly, gradient goal or not."* It landed there and closer than the
threshold required, so:

> **`bd tsr-4qx` is a case-gate item, not a gradient item — and the stated goal
> is a gradient.** 4.13 nodes per affected case across 225 files is the
> narrow-and-deep signature `docs/conventions.md` says to rank *for cases* and
> *against* the gradient. It is the same verdict `checker-notes-calls.md` reached
> for the parent 1,011, reached independently one level down, and it does not
> change because the number underneath it is large.

Recording it that way is the point. A 929-node row with a zero control and a
sufficiency demonstration behind it is the most persuasive-sounding thing on this
board, and the concentration is the only instrument that says persuasive is not
the same as well-aimed. This is the second time this cycle a large-sounding row
has been refused on this page; the first was the 557 itself.

**What this does not say.** It is a statement about *ranking*, not about
correctness. The mechanism is real, the control bucket is zero, and
`checker-notes-recv.md` demonstrated sufficiency on the shape. Nothing here
argues step 4 is wrong — only that "929 nodes" is not a reason to build it this
cycle when the goal is the gradient.

### And the concentration still does not settle it, because it is level 3

The three levels the concentration speaks to predict **lines**. Level 4 predicts
**cases**, and it is the level that actually ranks a case-gate item:
929 nodes in 225 files is a good case-gate target *only if those files are
otherwise nearly clean*. If `promiseType` carries four hundred other failing
assertion lines, flipping its 122 nodes finishes nothing, and a case-gate item
that flips no cases is worth less than the gradient item it was refused in favour
of.

That is `bd tsr-g27`, filed and never run, and it is one extra bucket in the
probe already built: for each affected case, the **remaining failing assertion
lines**, scored by `types_suite::compare` — the same function the gate uses, so
the denominator is the gradient's by construction. The instrument is described in
[the level-4 section](#level-4-the-instrument-and-what-each-outcome-means).

**`bd tsr-4qx` is parked with the measurement attached.** Not rejected — parked,
which is a different claim and the honest one: the mechanism is confirmed, the
ranking is against it for the current goal, and — since level 4 ran — the case
goal is measured against it too. **It ran; see
[Level 4 ran, and the answer is zero](#level-4-ran-and-the-answer-is-zero).**

### Level 4: the instrument, and what each outcome means

Built as a patch to `crates/tsr-conformance/examples/overload_funnel.rs` — a file
this workstream does not own, so it is handed over rather than committed here.
It was written, formatted, `clippy -D warnings`-clean and **smoke-run** in an
isolated worktree with its own target dir before being handed over, because a
patch that has only been read is a patch that has not been checked.

The example already produced the assertions and threw them away. It now keeps
them and scores each affected case through **`types_suite::compare`** — the same
function the gate uses, so the denominator is the gradient's *by construction*
rather than by agreement. Re-implementing the comparison in the probe is the
exact defect `checker-notes-guard.md` records this example already having had
once, when it measured a lib-less corpus against a gradient that had every lib.

It prints, for the affected cases only: a histogram of **remaining failing
assertion lines** (0 / 1–5 / 6–10 / 11–25 / 26–100 / >100) with an
`UNBUCKETED` control that must read zero, the median and mean, and — the most
informative rows in the output — the ten largest cases with `N nodes, R of T
lines still fail`.

#### Two properties of the number, stated before it exists

- **`remaining` is an over-estimate of the damage, and deliberately so.**
  `compare` is positional, so one missing assertion in the middle of a file costs
  every line after it. A case with a single early gap reads as far dirtier than
  it is. That biases the statistic *against* the case gate, which is the
  direction to be wrong in: it can under-sell this item, never over-sell it.
- **"Few remaining failures" is necessary, not sufficient.** The instrument
  counts *how many* lines still fail, not *which*. A case with eight remaining
  failures might have all eight blocked by something instantiation does not
  touch. So a favourable reading licenses "worth building for cases", not "will
  flip these cases".

#### The decision rule, pre-registered

A node blocks at least one assertion line and usually two — the callee and the
call — so a case at the mean 4.13 nodes needs roughly ten or fewer remaining
failures for this fix to plausibly be its *last* defect. The bucket that decides
it is therefore **≤10 remaining**:

- **≥25% of the 225 cases at ≤10 remaining** → step 4 is a strong case item,
  build it next cycle framed as a case-gate item, and say in the same breath that
  it is not a gradient item.
- **<10%** → park `bd tsr-4qx` with the measurement attached. Steps 1–2 stand on
  their own as the safe refactor they are, and the 929 becomes a documented
  blocker rather than a queued build.
- **Between 10% and 25%** → under-determined, and it gets reported as
  under-determined rather than rounded, exactly as the 929 was.

Separately and regardless: **the five `Promise` baselines are 47.3% of the
population**, so their four rows in the "ten largest" table are worth more than
the aggregate. If `promiseType` shows 122 nodes against several hundred remaining
failures, half this item finishes nothing whatever the histogram says, and that
single line should override the percentage above.

#### Level 4 ran, and the answer is zero

Measured at `0cd9e22` with the patch applied, single-threaded, isolated
worktree, one pinned binary. `UNBUCKETED` reads **0**, and the six buckets sum
to 225 exactly.

```text
remaining failing assertion lines per affected case:
      0    0.0%  0 (nothing else fails)
     24   10.7%  1-5
     43   19.1%  6-10
     68   30.2%  11-25
     62   27.6%  26-100
     28   12.4%  more than 100
median remaining 18, mean 78.8

the ten largest, and what else is broken in them:
    122 nodes   787 of 1217 lines still fail (35.3% right)  compiler/promiseType
    122 nodes   734 of 1187 lines still fail (38.2% right)  compiler/promiseTypeStrictNull
     65 nodes   460 of  970 lines still fail (52.6% right)  compiler/promisePermutations
     65 nodes   448 of  949 lines still fail (52.8% right)  compiler/promisePermutations2
     65 nodes   447 of  949 lines still fail (52.9% right)  compiler/promisePermutations3
     36 nodes   235 of  363 lines still fail (35.3% right)  compiler/controlFlowArrays
     20 nodes    69 of  130 lines still fail (46.9% right)  compiler/mapUpsert
     17 nodes   204 of  493 lines still fail (58.6% right)  compiler/staticAnonymousTypeNotReferencingTypeParameter
     12 nodes   210 of  361 lines still fail (41.8% right)  compiler/inferFromGenericFunctionReturnTypes2
     11 nodes    69 of  116 lines still fail (40.5% right)  compiler/controlFlowArrayErrors
```

**The number that settles it is the first row, and neither threshold below
names it: `0 cases, 0.0%, nothing else fails`.** Not "few" — zero. Closing all
929 nodes at today's tip finishes **no case whatsoever**, and the median
affected case carries 18 other failing lines. That is the case question
answered as flatly as it can be, and it does not depend on where anyone drew a
line, which is what makes it worth more than either clause of the rule.

So **`bd tsr-4qx` stays parked, and the grounds have changed.** It is no longer
"the wrong shape for the current goal" — a ranking claim, contingent on the goal
— but **"it finishes zero cases at today's tip"**, which is a measurement. The
second is a stronger and narrower statement and it is the one that should be
quoted.

#### The rule fired both ways, and that is the part worth keeping

Applied as written, without adjustment:

| clause | measured | verdict |
|---|---|---|
| **primary**: ≥25% of the 225 at ≤10 remaining → build | 24 + 43 = **67 of 225 = 29.8%** | **met** |
| **override**: `promiseType` at "several hundred" remaining → half the item finishes nothing whatever the histogram says | **787**, and 734 / 460 / 448 / 447 for the other four | **fired** |

**The override wins, and the primary threshold was met and overridden.** That is
recorded deliberately: a rule that can only ever fire one way is not a rule, it
is a conclusion with a threshold drawn around it. The primary clause on its own
would have licensed a build that finishes nothing, and the only thing standing
between this page and that outcome is a sentence written before the number
existed. Every one of the `Promise` five is "several hundred", the smallest by a
factor of four.

#### Two caveats, one load-bearing and one untested

- **`remaining` over-estimates**, as this page said before the run: `compare` is
  positional, so one early gap costs every line after it, and the true reading is
  somewhat kinder than 0.0%. **It cannot be kind enough to matter.**
  `promiseType` needs 787 lines and holds 122 nodes; 122 nodes cannot be most of
  787 however the positional bias falls. A caveat that would change the sign is
  worth chasing; one that cannot is worth stating and setting down.
- **"Few remaining failures is necessary, not sufficient" was never tested**,
  because the necessary condition failed first. It is kept rather than dropped —
  the next item that *passes* the histogram will need it, and a caveat retired
  because it went unused is a caveat that has to be rediscovered.

#### What this page got wrong about its own instrument

The bucket that answered the question — `0 (nothing else fails)` — was in the
histogram this page specified, and the decision rule this page pre-registered
**did not look at it**. The rule reasoned from "≤10 remaining is roughly what
4.13 nodes could finish" and set a percentage threshold, when the direct
question "how many of these cases would this fix *complete*" had a bucket of its
own sitting one row above. Building the right instrument and then reading the
wrong row off it is a distinct failure from building the wrong instrument, and it
is only visible because the override happened to catch the same thing by a
different route. **Pre-register on the most direct bucket the instrument
produces, not on a proxy derived from it.**

#### The smoke run, and why its numbers are not the answer

Run over the first 600 discovered cases to prove the instrument, not the
population: 53 nodes over 25 cases, control `UNBUCKETED` at 0, median remaining
11, no case at 0 remaining. **Those numbers say nothing about the 225** — it is
an alphabetical prefix, it contains none of the `Promise` five, and 25 cases is
not a sample of anything. It is recorded only because "the instrument ran and its
control read zero" is a different claim from "the instrument compiles", and this
page has been burnt once already by a two-hop reading nobody executed.

## Steps 1 and 2, built: the seam and the routing

**Built, and it changes no answer.** `Checker::get_type_of_property_of_type`
(`crates/tsr-checker/src/members.rs`), anchored to `getTypeOfPropertyOfType`
(`checker.go:18951`, verified at `5b1047d10`), and the three type-taking callers
routed through it: property access (`members.rs`), element access
(`crate::indexed`) and both sides of `properties_related_to`
(`crate::relater`). `calls.rs`'s classification counter asks only *existence* and
deliberately still calls `get_property_of_type`.

This is the whole of the safety argument for step 3. Substituting at the
property-access site alone would leave element access and the relater answering
`T` where upstream answers `number` — a confident wrong type, and at the relater
the dangerous direction, since it is the one consumer that acts on a `false`.

Why it was committed before the counter came back: it is a pure consolidation
that is correct whatever the counter says, and its cost if the counter kills
step 4 is one two-line function with an upstream anchor.

### It is verified by mutation, not by a new test

No new test was written, because nothing new is observable — a test asserting
today's answers would pass identically before and after and would be
decoration. What *is* checkable is that all three routes are **live**, and an
arm that is never entered has tests that pass without running it. So the seam
was mutated to `None` unconditionally and the suite re-run:

| suite | failing under the mutation | which route |
|---|---|---|
| `tests/members.rs` | 7 | property access |
| `tests/types.rs`, `tests/index_signature_members.rs` | `a_literal_index_is_a_property_lookup_by_name`, index-signature fallbacks | element access |
| `tests/relater.rs` | `two_structurally_identical_interfaces_relate`, `an_inherited_property_is_a_requirement`, `a_missing_or_mistyped_property_does_not_relate` | relater, both sides |

Seven test binaries redden in total. Restored, all green. That is the evidence
that step 3 has exactly one place to go.

### The consequence accepted

`get_type_of_property_of_type` widens the public surface by one function that
today does nothing its callers could not do inline. If the counter kills step 4,
this is dead weight with an upstream anchor rather than a wrong answer — the
cheapest of the available failure modes, and the reason it was safe to build
ahead of the number.

## Coverage, honestly

*Superseded by the section above; retained per "never delete a decision record".*

Measured share is unchanged from `checker-notes-recv.md` and this page adds none:
`promiseType` + `promiseTypeStrictNull` are **97 of the 557 (17.4%)** and are this
shape. `typedArrays` (54, 9.7%) is not — its receiver `Int8ArrayConstructor` is
non-generic. **The split of the 557 by "does the receiver carry type arguments"
is still not measured**; it is one counter at `crates/tsr-checker/src/calls.rs:416`
and it should be run before step 4, because it is what says whether this is worth
17% of the row or most of it.

Note that steps 1–3 move **nothing** on their own — they are refactoring plus
dead machinery. Only step 4 moves a line. An agent that builds 1–3 and reports
movement has measured noise.

## How you would know this page is wrong

- If `get_type_of_symbol` on a member of a generic already answers `errorType`
  for a `T`-typed member and the *declared* type for a `string`-typed one, then
  routing is unnecessary for the relater and `indexed.rs`, and the two-line
  version is safe after all. This page asserts it does not; that assertion is one
  fixture away from being checked and was **not** checked here.
- If the 557's receiver-carries-type-arguments counter comes back near 97, the
  slice is worth 17% of a 557-node row and the ranking that put it first should
  be revisited before step 4 is built.

## The registration for steps 3+4, written before the build — `bd tsr-4qx`

Registered at `40970d7`, which is the commit that landed the prerequisite
(`instantiate_type`'s depth/count guard, `checker.go:22111`). Nothing below had
been measured when this section was written.

### What will be built, as one change

`create_type_reference` flips `members: None` → `Some(symbol)` (`declared.rs`),
and `get_type_of_property_of_type` (`members.rs`) instantiates the found
property's type through `type_reference_targets` + `instantiate_type` whenever
the receiver is an instantiated reference. One change, because the two steps
cannot be separated: the flip without the seam converts 5,161 gap lines into
**wrong** lines (`C<number>.a` answers `T`), which is the recorded reason
`members: None` exists.

The `calls.rs:416` counter the issue's first plan asked for is superseded by
`examples/depend.rs`'s sizing at `d356450`: the receivers *are* instantiated
generics — 2,357 lines in the top ten alone — which is what the counter existed
to establish.

### Predictions

1. The conversion concentrates in members whose declared type is
   **parameter-free** (`length: number`) or **rebuildable** (a reference or a
   union of references). A member whose type is baked signature text —
   `Promise.then`, `Array.map` — mentions a type parameter, has no intern key,
   and stays an honest gap through `instantiate_type`'s fallback. So the
   conversion is a fraction of the 12,742 reachable, predicted low thousands.
2. The dangerous edge is not the target row (all gap today, can only gain
   there); it is (a) **gap→wrong** — an instantiated member whose printed form
   differs from upstream's, the `tsr-awa` qualification mechanism among others —
   and (b) **right→wrong through the relater**, which now sees members on
   instantiated references it previously found opaque, changing assignability
   answers and therefore overload selection.

### The bar

**KEEP** only if all four legs hold on the corpus counterfactual:

1. **Net matched ≥ +1,500 lines.** Against a 12,742-line reachable pool, a
   floor of 1,500 says the mechanism converted, not one case.
2. **gained ÷ lost ≥ 3.0** on per-case matched lines (`casedelta`) — the bar
   the `tsr-6ph` designs (2.1, 2.5) and qualified naming (2.7) each failed. If
   lost is zero this leg is vacuous, not passed; the net floor and leg 4 then
   carry the decision (the `&&` build's lesson).
3. **Fewer cases regress than finish.**
4. **Gap→wrong control, the leg `casedelta` cannot see:** with Δright from
   `casedelta` and Δgap from `depend.rs` run before/after,
   Δwrong = −Δgap − Δright must satisfy **Δwrong ≤ Δright ÷ 3**. A dissolved
   gap that did not become right became wrong, and a mechanism that mostly
   mints wrong answers fails here whatever the gradient says.

**REVERT otherwise.** If a leg fires, the first hypothesis is the build is
wrong and the second is the leg's premise is wrong; there is no third. A
partial port of a resolver loses more than it gains on its first run — if the
first counterfactual is negative, the move is to name the missing arm and
iterate, not to revert on sight; the bar judges the *final* state.

### How this registration would be shown wrong

If the gain concentrates in one or two cases (`casedelta` top-1 ≥ 50% of the
gain), the population was never the ~2,400 diffuse cases `depend.rs` measured,
and prediction 1's mechanism is not what converted it.

## The result, scored against that registration at `0fe102a`

```
  net +12,357 lines | 313,849 -> 326,206 | 65.53% -> 68.11%
  lost 0 lines, 0 cases regressed, 26 finished
  gap 113,765 -> 101,303 (depend.rs)  =>  Δwrong = +105, against a bound of 4,119
```

Every leg of the bar passed (leg 2 vacuous at lost = 0, as the registration
said it would be treated). **KEEP.**

### The falsifier fired, and what it means is not what it predicted

`compiler/largeControlFlowGraph` gained 10,000 lines — 81% of the gain — and
the registration said concentration ≥ 50% means the diffuse population was not
what converted. Both readings are true at once and they decompose cleanly:

- **The windfall.** The case's `data: any[]` element accesses now compute a
  genuine `any` through `Array<any>`'s instantiated index signature — and that
  string coincides with the `any` upstream prints because `TS2563` disabled its
  flow analysis. These lines were **ADR-0038's ceiling**: counted unmatchable
  on the premise that upstream's answer there is `errorType`, which this port
  refuses to render as `any`. The premise had an unstated assumption — that
  this port could never *honestly compute* `any` on those lines. It can, and
  did. `ceiling.rs` re-run at `0fe102a` reads **2,202 firmly unreachable**
  (was 35,508): the reachable denominator is 476,752, today is 68.42% of
  reachable, and 80% of the full denominator is now only 80.37% of the
  reachable one.
- **The predicted mechanism.** Ex-largeControlFlowGraph the gain is **+2,357
  over 147 cases**, diffuse (next largest 1,216, then 110), and above the
  1,500 floor on its own. Prediction 1 held: `typedArrays` +99,
  `parserRealSource11` +110, arrays and `Record` receivers throughout.

### What did NOT convert, and why — the next item's sizing starts here

The property-access root rows barely moved: 13,315 → 12,921 and 6,272 → 6,047
(`depend.rs`). The `Promise<boolean>`-receiver population that headlines
`bd tsr-4qx` is still gapped, because those members — `then`, `catch`, `map`,
`push` — have declared types that are **baked signature text**
(`TypeData::Named`/`Anonymous` with no intern key), so `instantiate_type`
falls through to `errorType` on them. Honest gaps, exactly as designed. The
lever behind them is a structured, rebuildable representation for
function/method member types — a data-model change, not another seam.

## The registration for signature instantiation (`bd tsr-0hc`), before the build

Registered at `HEAD` after the `tsr-4qx` scoring above; nothing below measured.

### The mechanism

The structure `tsr-0hc` asks for already exists: every baked signature text is
rendered from a [`Signature`] at exactly two sites (`function_types.rs:98`,
`symbols.rs:1207`), and then the struct is dropped. The build records
`TypeId -> Vec<Signature>` at those two sites (the ADR-0003 side-table move,
same as `type_reference_targets`), and `instantiate_type_worker` gains a
signature arm porting `instantiateSignature` (`checker.go:19640`): substitute
every `TypeId` the signature carries — parameter types, `this`, return,
constraints, defaults — refusing the whole signature if any part refuses, then
re-render through the same `signature_to_string` / type-literal branch that
produced the original text.

Two hazards named before the build, each with its guard:

1. **Calls must not resolve through the minted type.** `resolve_call_signature`
   reads signatures off an `Anonymous`'s symbol, which are the
   *uninstantiated* declarations — `p.then(f)` would answer
   `Promise<TResult1 | TResult2>`, a wrong line. The minted type is
   registered in the instantiation intern table and the resolver gaps on it.
2. **Union parenthesisation is keyed on `Anonymous::signature`**, so the minted
   type must stay `Anonymous` with that bit, or
   `(() => boolean) | undefined` loses its parens — the 19-line defect class
   the union-paren build already paid for.

A signature's own type parameters (`then<TResult1 = T>`) are distinct
`TypeId`s from the receiver's, so the map misses them by identity and they
survive substitution unrenamed, which is upstream's behaviour. A shadowed name
(`m<T>` inside `C<T>`) misses the map, trips the mentions scan, and gaps —
the bias this module already chose.

### The bar

**KEEP** only if all of: **net ≥ +800** matched lines; **gained ÷ lost ≥ 3.0**
(vacuous-if-zero handled as in the previous registration); **fewer cases
regress than finish**; **Δwrong = −Δgap − Δright ≤ Δright ÷ 3** with Δgap from
`depend.rs` at the same commit pair. REVERT otherwise, after naming which
premise failed.

### Predictions

The conversion lands in the two property-access rows (12,921 + 6,047 at
`0fe102a`) on members whose signatures mention the receiver's parameters —
`push`/`map`/`then`/`catch` — plus whatever the member-name row shares. The
overload-set (`Many`) form converts little: its members are also behind call
resolution. Net predicted +1,500 to +4,000.
