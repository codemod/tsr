# Why a call's callee has no object type

Status: measured at `53588b1` + this commit, over the 9,538-case `.types`
corpus population. Companion to
[`checker-notes-counters.md`](checker-notes-counters.md), which measured the
funnel one level up and left this bucket unsplit.

> **The 12,015 in `bd tsr-efq`'s original text is superseded and must not be
> re-derived from it.** The population is **10,265**, and the difference is not a
> code change: the probe that produced 12,015 bypassed
> `types_producer::assertions_for_case` and so ran without the bundled lib files
> that the gradient is scored with. Every number on this page is a count of
> **call-expression nodes**, never of assertion lines; the board has twice turned
> a node count into a line count by omission, so the unit is restated on every
> table here.

## The correction that came first, and it is the whole story

`checker-notes-counters.md` reported **12,015 of 16,306** call expressions
(73.7%) dying because the callee's type is not an object type, and flagged one
caveat as blocking: *"this harness loads no lib files … so an unknown share of
the 12,015 are calls to `parseInt`, `Math.max` and other lib globals"*. `bd
tsr-efq` carried that caveat forward, and the assignment that produced this
note repeated it as established, citing `grep -c bundled_libs
crates/tsr-conformance/src/binder_suite.rs` returning 0.

**That caveat is false, and the grep that supports it is aimed at the wrong
file.** `crates/tsr-conformance/src/types_producer.rs:49` defines
`bundled_libs()`, and `program_for_case` (`:725`) puts every
`vendor/typescript-go/internal/bundled/libs/lib.*.d.ts` into the program before
the case's own units. `assertions_for_case` (`:642`) — the entry point
`types_suite.rs:211` scores the gradient through — builds one program and one
`Checker` over it. **The types suite has loaded the libs since the rewire in the
`types_producer.rs` history.**

What had no libs was **the probe**. `overload_funnel.rs` at `7fb280b` did not go
through `assertions_for_case`; it re-implemented the *old* per-unit shape —
`tsr_parser::parse_with_options`, `tsr_binder::bind`, `tsr_checker::Checker::new`
per file — which is exactly the lib-less structure `assertions_for_case`'s own
comment says it replaced on 2026-08-05. So the first funnel was measured under a
configuration the gradient has not used, and the 12,015 is not a number about
the corpus.

The fix is the first half of this commit: the example now calls
`types_producer::assertions_for_case`, so the funnel's denominator is the
gradient's denominator by construction rather than by resemblance.

The confound was therefore real, and it was **in the instrument, not the
harness**. Sizing it was worth one corpus pass:

| | no libs (`7fb280b`) | libs (this commit) | delta |
|---|---|---|---|
| call expressions checked | 16,306 | 16,305 | −1 |
| callee type is not an object type | 12,015 | **10,265** | **−1,750** |
| single candidate (no selection needed) | 3,920 | 5,470 | +1,550 |
| overload sets reaching `choose_overload` | 304 | **503** | +199 |

Loading the libs recovers **1,750 callees, 14.6% of the bucket**. Real, worth
having, and **it does not dominate**. The honest reading is the opposite of the
one the caveat set up: the call path is *not* blocked behind `bd tsr-9or.1`,
because the libs are already loaded and 10,265 callees still have no object
type.

The −1 on the denominator is not noise to wave away: the program path drops a
unit the loader will not read where the per-file path parsed it anyway. One call
expression, and it is a difference in *which files exist*, not in how a call is
classified.

## The histogram

```text
cases: 9538

call expressions checked                           16305
  optional chain (unported)                           60
  callee type is not an object type                10265
    identifier: no symbol                            373
    identifier: symbol types as error               2315
    identifier: symbol types as a non-object         665
    property access: receiver is error              3477
    property access: receiver is typed              2562
    element access                                   133
    another expression form                          740
  callee symbol has no signature list                  2
  callee has zero call signatures                      5
  single candidate (no selection needed)            5470
overload sets reaching choose_overload               503
  a generic candidate in the set                     150
  a this or rest parameter                             1
  a parameter type outside SELECTABLE                218
  a spread argument                                    1
  an argument type outside SELECTABLE                 39
  no candidate with matching arity                     8
  arity matched, nothing assignable                    8
  ambiguous: matches with different returns           36
  SELECTED                                            42
    of which the return type is error                  0
UNATTRIBUTED (before selection)                        0
UNCLASSIFIED (callee has no object type)               0
UNATTRIBUTED (within selection)                        0
```

**All three control buckets read zero**, including the new one. That is the only
evidence that the seven sub-rows are a partition of 10,265 rather than a list of
seven things that happen to be true. The unit is a call expression node;
`check_expression` memoises on `NodeId` and there is now one `Checker` per case,
so each node is counted once.

## What the split says

Ordered by size, with the share of the 10,265:

| bucket | count | share | what it is |
|---|---:|---:|---|
| property access, receiver is `error` | 3,477 | 33.9% | `a.b()` where `a` itself is already a gap |
| property access, receiver is typed | 2,562 | 25.0% | `a.b()` where `a` has a real type and the member does not resolve to an object type |
| identifier, symbol types as `error` | 2,315 | 22.6% | the name resolves; this port cannot type the declaration |
| another expression form | 740 | 7.2% | a call, a parenthesis, `this`, `new`, a non-null assertion |
| identifier, symbol types as a non-object | 665 | 6.5% | resolves to `string`/`number`/a union — upstream reports "not callable" and so would we |
| identifier, no symbol | 373 | 3.6% | genuinely undeclared, **or** a global no lib declares |
| element access | 133 | 1.3% | `a[b]()` |

### 1. The lib-global bucket does not dominate. It is 3.6%, and it is a ceiling

`identifier: no symbol` is **373 of 10,265**, and that row is an *upper* bound on
lib globals: with the libs in the program, `parseInt` and `Math` resolve, so what
remains is names no bundled lib declares plus names the case genuinely does not
declare. The claim that "an unknown share of the 12,015 are `parseInt`/`Math.max`
shaped" is now measured, and the share is at most 3.6% of the remaining bucket
after the 1,750 the libs already recovered.

**So the call path is not blocked behind lib loading.** That was the answer the
assignment considered most likely and it is wrong, in the direction that means
more work exists rather than less.

### 2. It is a property-access problem, not a call problem

**5,792 — 56.4% — are `a.b()`**, and only 4,086 (39.8%) have any identifier
callee at all. The row is named `CallExpression` and the majority of it is a
member lookup that did not produce something callable. Two thirds of the way down
`docs/conventions.md`: *a row named after a symbol flag or a declaration kind is
usually not about that flag.* Same here for a syntactic form.

Within the property accesses the split is roughly even and the two halves have
**different owners**:

- **3,477 with an `error` receiver** are downstream of a gap that is not on the
  call path at all. Whatever makes `a` a gap is the item; the call is a
  passenger. Nothing written in `calls.rs` moves these.
- **2,562 with a typed receiver** are the ones where the call path could
  plausibly act: the receiver has a real type and `get_property_of_type` either
  found nothing or found a member whose type is not an anonymous object. That
  includes every method on an interface (call-signature members are unported)
  and every method whose type comes from a function *type node*.

### 3. `identifier: symbol types as error` is `get_type_of_symbol`, not calls

2,315 callees are names that **resolve** — the binder found the symbol — and
`get_type_of_symbol` answered `error`. That is the dispatch
`docs/conventions.md` already names as the home of three histogram rows that
looked like they were about something else. It is the third-largest bucket here
and it is not an item in `calls.rs`.

### 4. `checker-notes-counters.md`'s selection numbers are superseded

Re-running with the libs changes the selection funnel too, and the numbers `bd
tsr-6v7` was re-sized against move:

| row | no libs | libs |
|---|---:|---:|
| overload sets reaching selection | 304 | 503 |
| a parameter type outside `SELECTABLE` | 99 | **218** |
| a generic candidate in the set | 73 | **150** |
| arity matched, nothing assignable | 6 | **8** |
| SELECTED | 42 | 42 |
| ambiguous, different returns | 36 | 36 |
| an argument type outside `SELECTABLE` | 39 | 39 |

`SELECTED` did not move: the 199 extra sets the libs admitted all landed in a
gate, none of them in the outcome. The `SELECTABLE` ceiling rises from ≤138 to
**≤257 sites**, so `bd tsr-6v7` is worth roughly twice what the previous note
said — still fifth in size on this page, still not first.

**The row that "must not move" moved, from 6 to 8, and that is not a
regression.** It is stated in `bd tsr-6v7` as a falsifier *for a widening of
`SELECTABLE`*, and no widening happened here; two more calls reached selection
because their callees now resolve through a lib. The falsifier is conditional on
the change, and quoting it against a different change would have been the
"a guard rail that cannot observe a change" error in reverse.

## The typed-receiver row splits 1,985 / 557 / 20, and the actionable half is 557

`property access: receiver is typed` was reported above as an **upper** bound on
the actionable half, because it mixed our gaps with calls upstream rejects too —
the same mixing the identifier side already separates. Running the same
three-way split on it, with its own control bucket:

```text
    property access: receiver is typed              2562
      of which: no such member                      1985   77.5%
      of which: member types as error                557   21.7%
      of which: member types as a non-object          20    0.8%
UNCLASSIFIED (typed receiver)                          0
```

The control bucket is zero, so these three are a partition of 2,562.

- **557 — the member exists and types as `error`.** `get_property_of_type` found
  the symbol on the receiver's type and this port could not type it. These are
  unambiguously *our* gap. **This is the actionable number, and it is 4.6× smaller
  than the 2,562 it was extracted from.**
- **20 — the member types as a real non-object type.** Upstream reports "not
  callable" here as well. Not a gap, and crediting them would be false credit of
  the kind `docs/adr/0038` refuses. Tiny, and worth having measured precisely
  because "tiny" was not knowable in advance.
- **1,985 — no such member.** This row is *still mixed*, and it is the largest
  of the three. `get_property_of_type` (`members.rs:163`) answers only for
  `TypeData::Named { members: Some(..) }` and `TypeData::Anonymous`; every other
  receiver shape returns `None` before looking at the name. So this row contains
  both genuine "property does not exist" — which upstream errors on too — and
  every receiver whose type this port models without a member table, where
  upstream *does* have the member. Splitting it needs a counter on the
  **receiver's type shape**, not on the member, and that is one more bucket in
  the same probe (`bd` filed).

**So the actionable-inside-calls share of the 10,265 is 557 confirmed, plus an
unmeasured share of 1,985.** It is not 2,562, and it was never 12,015.

### On `bd tsr-qk9`, and why this is not it

`tsr-qk9` is `signature_parts_of` having no arm for `CallSignatureDeclaration`
or `ConstructSignatureDeclaration`, so `{ (): number }` and `{ new (): C }` gap.
**It is a different defect from the 557**, and the briefing's guess that these
are interface methods does not survive the code:
`get_property_of_type` resolves members of a `Named` type, and an interface
method's type comes from a `MethodSignatureDeclaration`, which
`signature_parts_of` *does* handle. `tsr-qk9` would move members declared as
bare call signatures, which is a narrower shape than "a method on an interface".

The 557 are members that resolve and do not type, and this probe does not say
*which* declaration kinds they are. Attributing them is the next counter, not an
inference from this one — writing it down as `tsr-qk9` would be the "a row named
after a declaration kind is usually not about that kind" error again, one
paragraph after recording it.

## `bd tsr-s2k`: 89% of "no such member" is a lookup that never ran

The 1,985 was the one row still conflating "the lookup ran and said no" —
upstream's answer too — with "this port never looked", because
`get_property_of_type` (`members.rs:163`) returns `None` for every receiver
shape but `Named`-with-members and `Anonymous` **before it reads the name**.
Splitting by the receiver's `TypeData` variant, with its own control bucket:

```text
      of which: no such member                      1985
        receiver has members, name absent            216   10.9%
        receiver is an intrinsic                     474   23.9%
        receiver is a union or intersection          168    8.5%
        receiver is Named without members           1011   50.9%
        receiver is another shape                    116    5.8%
UNCLASSIFIED (no such member)                          0
```

Control bucket zero, so this is a partition of 1,985. Nodes, not lines.

- **216 (10.9%) are not ours.** The receiver had a member table and the name was
  not in it. Upstream reports "property does not exist" here too. Subtract them.
- **1,769 (89.1%) are a lookup that never ran.** The mechanism is a single line
  of `get_property_of_type`, and the hypothesis behind `tsr-s2k` is confirmed.
- **The largest single shape is `Named` without a member table: 1,011, 50.9%.**

### And the 1,011 is the most concentrated thing on this page

```text
concentration by case: 1011 over 253 cases
    122   12.1%  compiler/promiseType
    122   12.1%  compiler/promiseTypeStrictNull
     65    6.4%  compiler/promisePermutations
     65    6.4%  compiler/promisePermutations2
     65    6.4%  compiler/promisePermutations3
     36    3.6%  compiler/controlFlowArrays
     20    2.0%  compiler/mapUpsert
top 10 hold 53.0%
```

**The top five baselines are all `Promise`, and they are 439 of 1,011 — 43.4%.**
Ten baselines hold over half. This is the `largeControlFlowGraph` shape arriving
one level down: a row that looks like a mechanism and is substantially five
files about one lib-declared generic interface whose members this port does not
build.

That does not make it worthless — those are 439 real nodes and five cases — but
it settles the ranking question. **This is a case-gate item, not a gradient
item**, and it should be labelled that way even though it is the largest thing on
this page. 4.0 nodes per affected case over 253 cases, with half in ten files, is
the narrow-and-deep signature `docs/conventions.md` says to rank *for cases* and
against the gradient. The stated goal is a gradient, so a large number here must
not recruit anyone into calling it a gradient item.

### It confirms the mechanism and it does not restore the large-row strategy

The falsifier as stated was *"if most of the 1,985 are one receiver-shape
mechanism, that is a single ~2,000-node item and the large-row strategy comes
back."* Measured, both halves of that need correcting:

- "Most" holds, barely: 1,011 is 50.9% of 1,985 — but of the **10,265** it is
  **9.8%**, and of the 16,305 calls checked it is 6.2%.
- "~2,000" does not hold. The largest single mechanism is **1,011 nodes**, half
  the figure the falsifier was set at. The 1,769 total spans **four** different
  receiver shapes, and only if all four are fixed by one change does it read as
  one item — which is a claim about `get_property_of_type`'s structure that this
  probe does not make.

So the verdict stands, with the number moved: the largest confirmed mechanism on
the call path is ~1,011 nodes, against 12,289 lines as the row was originally
ranked. That is still an order of magnitude down, and it is **not a `calls.rs`
item** — it is member-table construction for `Named` types, and any line yield
depends on how many of those 1,011 receivers this port could build members for
at all, which is the same kind-2 chain as everything else on this page.

## Concentration: the row is distributed, and it does not evaporate

`docs/conventions.md` makes this the *first* command, because on an earlier row
9,999 of 11,363 lines sat in a single baseline and the whole workstream
evaporated. It was run here (single-threaded, since the counters are
process-wide and a delta across a parallel region attributes other cases' calls
to this one):

```text
callee-has-no-object-type by case: 10265 over 2113 cases
    696    6.8%  compiler/temporal
    621    6.0%  conformance/parserRealSource11
    239    2.3%  compiler/genericDefaults
    177    1.7%  compiler/promiseType
    172    1.7%  compiler/promiseTypeStrictNull
    146    1.4%  compiler/underscoreTest1
    143    1.4%  conformance/parserRealSource7
    141    1.4%  conformance/parserindenter
    121    1.2%  conformance/parserRealSource10
    101    1.0%  compiler/promisePermutations
top 10 hold 24.9%
```

**The largest single case is 6.8% and the top ten are a quarter.** This is the
opposite of the `largeControlFlowGraph.types` shape: the row is genuinely
corpus-wide, spread over **2,113 of the 9,538 cases** at a mean of 4.9 callees
per affected case. Nothing here evaporates on inspection, and the check's usual
verdict — prefer distributed rows when targeting the gradient — applies.

The same fact reads the other way for the **case** gate, which is level 4 of the
bucketing rule and the one that predicts cases rather than lines. 4.9 unresolved
callees per affected case, over 22% of the corpus, is the signature of a broad
shallow row: it reaches many files and finishes almost none of them. If the goal
is flipping cases rather than moving the gradient, this row is close to the worst
available shape, and an item concentrated in few files with few other defects
beats it.

### The actionable sub-bucket has the opposite shape, and it is one command

The 557 is a different row from its parent and the concentration check says so:

```text
concentration by case: 557 over 147 cases
     54    9.7%  compiler/typedArrays
     51    9.2%  compiler/promiseType
     46    8.3%  compiler/promiseTypeStrictNull
     35    6.3%  compiler/duplicateLocalVariable1
     20    3.6%  conformance/parserindenter
     16    2.9%  conformance/assertionTypePredicates1
     16    2.9%  conformance/types.asyncGenerators.es2018.1
top 10 hold 49.9%
```

**147 cases, top ten hold half.** The parent row was 2,113 cases with the top ten
at a quarter; the actionable half inside it is *four times* more concentrated by
that measure. This is the reverse verdict and it matters for ranking:

- **For the gradient**, 557 nodes in 147 files is small and narrow — a worse
  target than the distributed parent, and the parent is mostly not ours.
- **For flipping cases**, 3.8 per affected case in 147 files is close to the best
  shape on the board, the opposite of the parent's broad-and-shallow signature.

The three largest are `typedArrays`, `promiseType` and `promiseTypeStrictNull` —
151 of 557, 27%, in three baselines that are all about **lib-declared generic
types**. That is a suggestive shape and it is *not* measured here: this run says
which cases, not which declarations. Reading it as "the 557 are Promise members"
would be exactly the inference this page keeps refusing.

**What was not measured**, and it is the level-4 statistic proper: the
distribution of *remaining* failures in the 2,113 affected cases. Knowing that
these calls are 4.9-per-case says nothing about whether those cases are otherwise
clean. It costs one more bucket in this same probe and it is not in this run.

## KIND, and what the deliverable for the follow-on is

**Kind 2, dependency-gated, by construction** — under the repaired triage
question in `docs/conventions.md`, the prerequisite is *the callee has an object
type* and the row name says it is unmet. Applying the criterion per bucket:

| bucket | prerequisite | met? | kind |
|---|---|---|---|
| property access, receiver `error` | the receiver has a type | unmet for all 3,477 | kind 2, and **not owned by calls** |
| identifier, symbol types `error` | `get_type_of_symbol` answers | unmet for all 2,315 | kind 2, owned by `get_type_of_symbol` |
| property access, receiver typed | member resolves to something callable | unmet, but the *receiver* half is met | kind 2, the shallowest chain here |
| identifier, no symbol | the name is declared | unmet | kind 2, and 373 is the whole ceiling |
| identifier, non-object type | — | **met**; upstream errors too | not a gap to close |

So **10,265 is a ceiling and not a deliverable**, and no number on this page
should be quoted as lines. The deliverable for any follow-on is a
*demonstration*: pick the shape, hardcode past the blocker, and see whether the
call then answers. `docs/conventions.md` is explicit that probing the blocker
alone is too weak — a kind-2 form is a chain, and removing the named blocker
usually exposes the next.

The one bucket where that demonstration is cheap is **property access with a
typed receiver** — and the split above narrows it from 2,562 to the **557**
whose member resolves and does not type. The receiver already types and the
symbol is already found, so the probe is "give the member an anonymous object
type with a call signature and see whether the call line goes from `error` to a
return type". If it does not, the call path was never the blocker for them
either, and that is worth learning for the same cost.

## Is 90% on `checker_types` reachable? Not from this row, and probably not at all

Asked as a lead question, answered from this page's numbers only.

The cycle deltas are **+4.94, +3.36, +0.34**. The current number is 58.17%
(`53588b1`). Reaching 90% needs **+31.8 points**, which at the last cycle's rate
is 94 cycles and at the best of the three is 6.5 — and the trend across the three
is decelerating by roughly an order of magnitude per cycle, not holding.

The largest row on the board has now been measured as a partition, and what it
contains is the argument against a large-row strategy:

- **10,265** call expressions whose callee has no object type.
- Of those, **557** are confirmed as this port's own gap in a place the call path
  owns, plus an unmeasured share of 1,985.
- **665 + 20 = 685 are not gaps at all** — upstream reports "not callable" too.
- The two largest sub-rows, **3,477 and 2,315**, belong to whatever gaps the
  receiver and to `get_type_of_symbol`, and are chains whose next link is
  unmeasured.

So the biggest single item anyone could name resolved into one confirmed
workstream of 557 **nodes** — not lines — and a set of pointers to other
people's rows. Every one of those pointers is kind 2: the prerequisite is unmet
by construction, so each is a chain of unknown depth, and `docs/conventions.md`
records that removing a named blocker in a kind-2 chain usually just exposes the
next one.

**My reading is that the remaining work is many small workstreams, not a few
large ones, and that 90% is not a target anyone can plan against today.** The
evidence is that the one row large enough to matter was ranked at 12,289 lines,
survived one probe at 12,015 nodes, and has now been measured at 557 confirmed —
a 22× reduction across two cycles of measurement with no code written. That is
the third time on this page that a large row shrank on contact.

**How I would know I am wrong.** If `bd tsr-s2k` finds that most of the 1,985
"no such member" callees are receivers this port models without a member table,
that is a *single* mechanism worth ~2,000 nodes and the large-row strategy comes
back. That is one 25-second corpus pass, it is filed, and it is the measurement
that would overturn this paragraph. I would rather it did.

**What I am not claiming.** I have measured one row. The gradient has other rows
this page never looked at, and a lead ranking the board should weight this as
evidence about *the largest row*, not about the corpus. What generalises is the
method, not the verdict: three cycles of ranking by reading produced 12,289,
12,015 and 2,562, and each was corrected downward by one command.

## How to know this is wrong

- **The three control buckets.** All zero. A non-zero `UNCLASSIFIED` means a
  callee form stopped matching `classify_unresolved_callee` and every sub-row is
  a lower bound of unknown depth.
- **The lib delta is measured against a probe rewrite, not a lib toggle.** The
  1,750 is `7fb280b`'s number minus this commit's, and the two runs differ in
  *both* the lib files and the one-program-per-case identity space (ADR-0034).
  Attributing all 1,750 to lib files alone would over-claim; the clean
  attribution needs a run with `bundled_libs()` stubbed empty on this commit,
  which costs one corpus pass and is filed rather than done.
- **`identifier: no symbol` is a ceiling on lib globals, not a count of them.**
  If someone wants the count, the discriminating question is whether the name
  resolves in a program that *does* have libs — which it does here — so the 373
  are by definition names no lib declares. A shrinking 373 under `bd tsr-9or.1`
  (declaration merging) would prove some of them were merge failures rather than
  undeclared names.
- **The conclusion could be right for the wrong reason** in one specific way. "It
  is a property-access problem" rests on `classify_unresolved_callee` reading the
  *syntax* of the callee. A method call whose receiver types fine and whose
  member is missing lands in `property access: receiver is typed` — but so does
  `a.b()` where `b` resolves to a `number`, which upstream also rejects. That
  sub-row therefore mixes our gaps with genuine errors, exactly as
  `identifier: symbol types as a non-object` separates out for identifiers. It
  is one more counter and it was not run; until it is, treat 2,562 as an upper
  bound on the actionable half.

## Why there is no unit test for this commit

The checker change is counting only: `classify_unresolved_callee` is behind
`counters::counting()`, which is `false` unless `TSR_OVERLOAD_COUNTERS` is set or
`enable()` is called, and it bumps atomics without returning anything. A unit
test over a process-wide `AtomicU64` under a parallel test harness would be
flaky, and it would discriminate less than what is already here: the
`UNCLASSIFIED` control bucket reading exactly zero over 10,265 samples is a
stronger statement than any fixture, and it is re-derived on every run of the
example rather than asserted once.

The mutation that would show it red is real and was checked by construction:
delete any one arm of the `match` in `classify_unresolved_callee` and
`UNCLASSIFIED` prints that arm's count. It is a control bucket, so it fails
loudly rather than silently — which is the property a fixture would have been
bought for.
