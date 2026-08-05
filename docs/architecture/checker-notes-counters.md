# Where a call stops: the overload attribution histogram

Status: measured at `78cfcba` + this commit, over the 9,538-case `.types`
corpus population.

## Why this was measured

Overload selection (`6c55f29`, `crates/tsr-checker/src/calls.rs`) was predicted
to move "low hundreds" of assertion lines. It moved **48 lines across 8 cases**.

Its author then classified the 779 overloaded call sites it could find in the
baseline sources and reported that `SELECTABLE` — the parameter-side guard
`bd tsr-6v7` proposes widening — *admits* about 198 of them. 198 admitted
against 8 landed leaves at least three quarters of the attenuation happening at
a step nobody could name, and the author flagged its own method as a regex over
single-line signatures whose two counts of generic candidates disagreed 84
against 162.

The naming problem is the point. `bd tsr-6v7` says widening `SELECTABLE` will
make the `CallExpression` row move. Whether that is true depends entirely on
*where* calls actually stop, and no count of the source can answer it: the same
guard also runs over the **argument** types, the winning candidate's return
annotation may be unported, and a callee whose type never resolves never reaches
selection at all. Those three are invisible to a regex and visible to a counter.

So: five counters plus the unattributed bucket, no behaviour change.
`crates/tsr-conformance/examples/overload_funnel.rs` prints them.

## The histogram

```text
cases: 9538

call expressions checked                           16306
  optional chain (unported)                           60
  callee type is not an object type                12015
  callee symbol has no signature list                  2
  callee has zero call signatures                      5
  single candidate (no selection needed)            3920
overload sets reaching choose_overload               304
  a generic candidate in the set                      73
  a this or rest parameter                             0
  a parameter type outside SELECTABLE                 99
  a spread argument                                    1
  an argument type outside SELECTABLE                 39
  no candidate with matching arity                     8
  arity matched, nothing assignable                    6
  ambiguous: matches with different returns           36
  SELECTED                                            42
    of which the return type is error                  0
UNATTRIBUTED (before selection)                        0
UNATTRIBUTED (within selection)                        0
```

Both control buckets read zero, which is the only evidence that the rows above
them are a partition rather than a list.

The unit is a **call expression node**, not an assertion line.
`Checker::check_expression` memoises on `NodeId` and the probe builds one
`Checker` per file, so each call node is counted exactly once.

## What it says

### 1. The 779 was 2.6× high, and the parameter gate is a third of what remains

**304** overload sets reach `choose_overload`, against a source-side estimate of
779. Of those 304, the gate `bd tsr-6v7` is about rejects **99 — 32.6%**.

Widening `SELECTABLE` therefore has a hard ceiling of 99 call sites plus
whatever share of the 39 argument-side rejections it also recovers (the same
constant gates both), so **at most ~138 of 304 sites**. At the 48-lines-per-42-
selected-sites ratio this slice actually produced, that is **roughly 110–160
assertion lines** — a real item, and one that requires trusting the relater's
`false` over object types, which is the thing `bd tsr-6v7` exists to warn about.

It is not "the low hundreds" and it is not the reason selection under-delivered.

### 2. Two of the author's three guesses were wrong, and the third was right

| guessed | measured |
|---|---|
| 1. the argument types are "suspected to be the bulk" | **39 of 304, 12.8%** — real, a third the size of the parameter gate |
| 2. unported return types make selection buy nothing | **0** — every one of the 42 selected calls returned a real type |
| 3. callees that never resolve | **12,015 of 16,306 — 73.7% of every call in the corpus** |

Guess 2 is dead outright and should not be pursued: selection never succeeds and
then prints `error`. Guess 3 is not merely the largest of the three, it is
larger than everything else on this page combined.

### 3. The binding constraint on calls is not selection at all

**73.7% of call expressions in the corpus die because the callee's type is not
an object type** — before any signature list is consulted, before arity, before
assignability. Only 4,224 of 16,306 calls reach a signature at all, and 3,920 of
those had a single candidate and needed no selection.

Overload selection operates on **1.9%** of the corpus's calls. No amount of work
inside `choose_overload` can change that ratio, and the ceiling on *all* future
overload work — widening `SELECTABLE`, porting the subtype pass, porting
inference for generic candidates — is the 304 sites, together about a fifth the
size of the pre-selection loss.

A caveat that must be measured before the 12,015 is treated as a checker item:
**this harness loads no lib files** (nothing in `types_suite.rs` or `case.rs`
references bundled libs), and the gradient is scored under the same condition.
So an unknown share of the 12,015 are calls to `parseInt`, `Math.max` and other
lib globals that cannot resolve here for a reason that has nothing to do with
the call path. Splitting that bucket by *why* the callee has no object type —
unresolved symbol, resolved-to-error, property access on a gapped receiver — is
the obvious next probe and is not filed yet.

### 4. Two rows a widening would not touch

- **`ambiguous: matches with different returns` — 36 sites, 11.8%.** These are
  calls where selection ran, arity and assignability both matched more than one
  candidate, and this port declines to guess because upstream's subtype pass
  (`resolveCall`'s first pass, `checker.go:8843`) is unported. A third the size
  of the parameter gate, entirely independent of it, and it needs specificity
  ordering rather than a wider relation.
- **`arity matched, nothing assignable` — 6 sites.** Inside `SELECTABLE` a
  `false` from the relater is trustworthy, so these are genuine "no candidate
  accepts these arguments" — upstream reports an error and this port gaps. They
  are the row that *must not move* on a widening; if it does, the widening broke
  the relater's negatives, which is exactly the failure mode `bd tsr-6v7`
  describes.

## Verdict on `bd tsr-6v7`

Worth doing, at about a fifth of what the issue implies, and **not first**.
Ranked by measured size, the call path's items are:

1. the 12,015 unresolved callees (needs the split probe above before it is an
   item at all)
2. generic candidates, 73 sites — `bd tsr-4sc.8`, already owned
3. widening `SELECTABLE`, ≤138 sites, and it trades a gap for a risk
4. the subtype pass, 36 sites

## How to know this is wrong

- **The control buckets.** Both read zero. If a future reading is non-zero the
  counters have stopped partitioning `check_call_expression` and
  `choose_overload`, and every number here is a lower bound of unknown depth.
- **The shadowing is real and is not an artefact.** Arguments are checked
  *after* the parameter gate, so the 39 is "among the 132 sites the parameter
  gate admitted", not "among 304". Counting both independently would mean
  running `check_expression` on expressions the checker does not otherwise
  visit, perturbing the caches that feed the assertion lines — a behaviour
  change. The consequence is that widening `SELECTABLE` cannot be sized to
  better than the 99–138 band from this run alone; re-running the probe with the
  wider constant is the measurement that narrows it, and it costs one corpus
  pass.
- **The 48-lines-per-42-sites ratio is this slice's, not a constant.** If the 99
  object-typed sites sit in denser files, the line yield is higher than 160.

## The switch

Counting is off unless `TSR_OVERLOAD_COUNTERS` is set in the environment or
`tsr_checker::calls::counters::enable()` is called before anything is checked.
When off, the counters cost one `OnceLock` read per branch; when on, one relaxed
atomic increment. No call classifies differently either way — the only code
change in `choose_overload` is that its single short-circuiting `any` over the
candidates became three, tested in the order its doc comment already listed, so
a set that is both generic and object-typed is attributed to `generic` alone.
