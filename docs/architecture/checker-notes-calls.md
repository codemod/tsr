# Why a call's callee has no object type

Status: measured at `53588b1` + this commit, over the 9,538-case `.types`
corpus population. Companion to
[`checker-notes-counters.md`](checker-notes-counters.md), which measured the
funnel one level up and left this bucket unsplit.

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
typed receiver, 2,562**: the receiver already types, so the probe is "give the
member an anonymous object type with a call signature and see whether the call
line goes from `error` to a return type". If it does not, the call path was never
the blocker for them either.

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
