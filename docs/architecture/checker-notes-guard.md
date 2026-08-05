# The baseline writer's eight guards, sized

ADR-0039 established *that* upstream's `.types` writer decides between `error`
and `any`, and left *what each guard is worth* unmeasured. `bd tsr-d6o` filed the
ceilings — 69,195 `: any` lines in cases carrying an `.errors.txt`, 19,045 in
cases without — with the warning that they are ceilings and not deliverables.

> # CORRECTED 2026-08-05 — every number below the fold was measured lib-less
>
> The first version of `writer_guards.rs` parsed, bound and checked each unit
> itself instead of going through `program_for_case`, so it loaded **no
> `lib.*.d.ts`** and measured a different compiler than the gradient it quoted.
> Same defect `examples/overload_funnel.rs` had before `731b1ee`, and the
> convention added in `dc61c84`. Rewired onto
> `assertions_for_case_with_ids`; corrected figures:
>
> | | lib-less (**wrong**) | with libs (**correct**) |
> |---|---|---|
> | gradient | 58.0441% | **62.1393%** |
> | ceiling | 40,759 (+8.69) | **29,936 (+6.38)** |
> | `hadErrorBaseline` `-> any` | 39,412 | **28,850** |
> | `largeControlFlowGraph` share | 20,000 / 49.1% | **10,000 / 33.4%** |
> | top 10 cases | 72.4% | **65.1%** |
> | control: upstream `error` / `any` | 392 / 5,211 | **391 / 5,146** |
>
> **The verdict does not move, and the load-bearing statistic barely does.** The
> control rate — the whole basis for refusing `hadErrorBaseline` — was 7.0%
> lib-less and is **7.06%** (391 of 5,537) with libs. The ceiling falls 27% and
> `largeControlFlowGraph` remains the single largest case by a factor of three.
> Both arguments for not porting `hadErrorBaseline` survive intact.
>
> The tables further down are left as originally written rather than silently
> edited, per `conventions.md`. Read them for *shape*; take *magnitudes* from
> this block and from "The corrected table" below.

This note is the measurement. The instrument is
`crates/tsr-conformance/examples/writer_guards.rs`; every number below is one run
of it at `bea80b9`, from a single pinned binary.

## The answer in one line

**The ceiling is 40,759 lines (+8.69 gradient points). The honest deliverable is
about 1,500, and one guard — `hadErrorBaseline` — accounts for 39,412 lines that
would be credited for a type this port does not compute.**

> **Corrected after porting.** The per-guard figures below are attribution
> counts, and for the *positional* arms they under-state what porting the arm is
> worth — the label arm measured 209 and converted 597, 2.9× low. See
> "Ported: the label-name guard, and why 209 was the wrong number" at the end.
> The `hadErrorBaseline` verdict is unaffected: it is the arm that absorbs.

## The instrument

One arm per guard, attributed in **upstream's written order**
(`internal/testutil/tsbaseline/type_symbol_baseline.go:380`), because the
condition is a conjunction and the first guard to fire decides the line. Only
lines where *our* producer prints `error` are offered to it: upstream's
`checker.IsTypeAny(t)` precondition, in our terms. A line we already print as
`any` cannot move whichever way the guards fall.

Each arm splits its claims by **upstream's** answer. `-> any` is the conversion;
`-> error` must be zero, because a firing guard means upstream took the node
builder, which cannot print `error`; `-> other` is unchanged and still wrong.

```text
guard (upstream's conjunction order)         claims    -> any  -> error  -> other
hadErrorBaseline (case-scoped)               119166     39412         0     79754
binding element parent                         1154       356         0       798
property access / qualified name parent        8331       452         0       7879
label name                                      209       209         0         0
global scope augmentation                         0         0         0         0
meta property                                     0         0         0         0
import statement name                           899       238         0       661
export statement name                           372        92         0       280
intrinsic JSX tag  [PORTED d6dc9a7]               0         0         0         0
NO GUARD (control bucket)                     44913      5211       392     39310

aligned 468921, right 272181 (58.0441%), gap 174652
```

`CONTRADICTIONS 0` — no firing guard claimed a line upstream prints as `error`.
The file-scoped prediction re-measured on *aligned* lines rather than by grep:
317,893 aligned lines in cases with an errors baseline, **0** of them printed
`: error` upstream. The ported JSX arm reads 0/0/0/0, which is what "already
converted away from `error`" looks like and is the arm's positive control.

### The mutation that makes the split readable

`WRITER_GUARDS_FLATTEN=1` drops the `hadErrorBaseline` arm from the attribution
order. Its lines must redistribute, not vanish, and `NO GUARD` must absorb the
bulk — if the positional arms swallowed them instead, the arms would not be
disjoint from the case-scoped guard and the per-position split above would not be
readable. Verified: **both runs claim exactly 175,044 lines** (= 174,652 gap +
the 392 `NO GUARD` lines that are already right), `hadErrorBaseline` releases
119,166, and `NO GUARD` takes 94,053 of them (44,913 → 138,966).

> The first attempt at this check appeared to move the gradient, 271,974 →
> 272,181. That was not the mutation: teammates' checker edits landed between the
> two `cargo run`s and the second run rebuilt. **In a shared tree, a probe's
> control and mutation runs must come from one pinned binary**, or the
> measurement is against a moving checker.

## Why the 40,759 must not be shipped as a gain

### The control bucket is the argument

In a case with no errors baseline, where no positional guard fires, the fast path
is live and upstream's writer is showing us its raw checker answer. On lines
where *we* print `error`, upstream prints:

| upstream's answer | lines |
|---|---|
| `error` | 392 |
| `any` | 5,211 |
| a real type | 39,310 |

Restrict to the lines where upstream's own type is an any-flagged intrinsic —
the only lines a rendering guard can act on — and upstream's type is `errorType`
on **392 of 5,603, or 7.0%**. The other 93.0% are a *genuine* `anyType` that
upstream computed and this port did not.

The guard is a rendering rule **for `errorType`**. Applying it where our
`errorType` does not correspond to upstream's produces a string match on a path
that differs. That is the failure ADR-0038 rejected and ADR-0039 explicitly
preserved: *"a blanket substitution would falsely credit ~6,100 lines that our
port merely failed on."*

### `hadErrorBaseline` is not a per-node guard, and that is the finding

ADR-0039 relocated the fix into the producer on a specific ground:

> the guard belongs in the producer, **per-node**, as upstream has it — not in the
> checker, and not as a blanket rendering change. A per-node guard credits only
> the positions upstream credits.

`hadErrorBaseline` does not satisfy that. It is **case-scoped**: one boolean,
`len(result.Diagnostics) > 0` at
`internal/testrunner/compiler_runner.go:501`, true for **56.3% of cases (5,371 of
9,538)**, and it fires at every node in those cases irrespective of position. It
is threaded through `newTypeWriterWalker` (`type_symbol_baseline.go:270`) and read
once at `:380`.

Porting it is precisely the blanket substitution ADR-0038 forbade, wearing the
clothes of a positional guard. **The seven remaining guards are not one item, and
the reason is not their sizes — it is that one of them is a different kind of
thing.**

### The concentration proves it without needing the 7% rate

**20,000 of the 40,759 converted lines — 49.1% — are one case,
`largeControlFlowGraph`.** `docs/conventions.md` already documents that file:
ten thousand accesses hanging off one `const data = []`, where *"upstream
computes `any` there through `autoArrayType` machinery this port does not have"*.
Upstream's type there is a genuine `anyType`. Every one of those 20,000 lines is
false credit, established by evidence already in the repo before this probe ran.

The top 10 cases are 72.4% of the total; six of the next nine are
`parserRealSource*` and `parserindenter` — the TypeScript compiler's own source
used as a fixture, large files that carry errors baselines.

### The decomposition

| | lines |
|---|---|
| headline conversion | 40,759 |
| less `largeControlFlowGraph`, provably false credit | −20,000 |
| remainder | 20,759 |
| of which legitimate at the control's 7.0% rate | **≈1,450** |

So the deliverable is on the order of **1,500 lines, +0.3 gradient points**,
against a headline of 40,759 and +8.69. **The item is small.** That is the
answer, and it is the one the ceilings were filed to prevent being missed.

## What is actually worth porting

Read the arms as *positions*, in the errors-baseline-free cases where the writer's
decision is genuinely positional:

- **Label name — 209 claims, 209 convert, 0 `-> other`. Now ported.** A 100% conversion rate
  with an empty residue is the signature of a position where upstream *always*
  holds `errorType`, and it is the same shape as the already-ported
  `isIntrinsicJsxTag` arm. Small, clean, and free of the false-credit problem
  because there is no competing genuine `any` to confuse it with.
- **Global scope augmentation and meta property — 0 and 0.** Not "small": empty.
  Nothing to port.
- **Binding element (356 of 1,154), property access / qualified name (452 of
  8,331), import name (238 of 899), export name (92 of 372).** Mixed. These are
  positions where upstream mostly holds a *real* type and we are simply gapping,
  so the same false-credit discount applies to whatever they convert. 1,138 lines
  combined, before discounting.

## Level 4: this predicts lines, not cases

Of the 2,093 cases the full conversion touches, **542 (25.9%) would be left with
no other defect**; 55,551 defects remain across the rest. Nineteen converted lines
per case flipped — the signature `docs/conventions.md` describes for a broad,
shallow change. If the goal is cases rather than gradient, this item is the wrong
shape regardless of the false-credit argument.

## How we would know this is wrong

- **If the 7.0% control rate is not transferable.** It is measured on
  fast-path-live cases and applied to cases with an errors baseline. Those
  populations differ by construction — a case with diagnostics may well produce
  `errorType` at a higher rate, which would raise the legitimate share. The
  falsifier: port `hadErrorBaseline` behind a flag and check whether the converted
  lines outside `largeControlFlowGraph` concentrate in positions where upstream
  plausibly holds `errorType`. The 49.1% single-file share would have to be
  explained away first.
- **If `largeControlFlowGraph` is excluded from the corpus metric**, the headline
  drops to 20,759 and the argument rests entirely on the 7.0% rate rather than on
  two independent lines of evidence.
- **The measurement is against a moving checker.** Three agents held
  `crates/tsr-checker` edits in the tree at `bea80b9`. The arm *shape* is robust —
  the same run before their edits gave 39,412 / 356 / 452 / 209 / 238 / 92
  identically — but the totals will drift.

## Ported: the label-name guard, and why 209 was the wrong number

The label arm is the one this measurement recommended porting, and it is now in
`type_at_location` with `is_label_name` beside `jsx_tag_name_of`. Re-measured on
the same instrument, the arm reads **0 / 0 / 0 / 0** — the ported-JSX signature,
meaning its lines have been converted away from `error` and the arm can no longer
claim them.

Isolated commit pair: **`53588b1^..53588b1`** (one commit; verified with
`git log --oneline 53588b1^..53588b1` rather than quoted as "my last commit to
this one", which in a four-agent checkout resolves to whatever teammates landed
in between).

**This slice is measured, not unmeasured.** The corpus run below was done with the
guard in place, on the same instrument that sized it. It is the arm's own numbers
that make it checkable without re-running anything: the `label name` row went
209 → 0/0/0/0.

**It converted 597 lines, not 209.** The accounting closes exactly:

| | before | after | delta |
|---|---|---|---|
| gradient | 272,181 (58.0441%) | 272,778 (58.1714%) | **+597** |
| lines we print as `error` | 175,044 | 174,444 | −600 |
| `label name` arm claims | 209 | 0 | −209 |
| `hadErrorBaseline` arm claims | 119,166 | 118,775 | −391 |
| `hadErrorBaseline` arm `-> any` | 39,412 | 39,024 | −388 |
| wrong | 22,088 | 22,091 | +3 |

600 label-name lines left the `error` population: 209 from fast-path-live cases
and **391 that were sitting inside cases with an errors baseline**. 597 of the 600
convert to right (209 + 388); the other 3 are lines upstream answers with a real
type, so they stay wrong — differently wrong, at no cost, since they were wrong
before.

**The 209 was a true number answering a different question.** Attribution gives
`hadErrorBaseline` precedence, because the guard that fires first is the one that
decides upstream's line — so every label name in the 56.3% of cases carrying an
errors baseline was counted under the dominant arm, not under `label name`. The
209 answered *"how many label-name lines sit in fast-path-live cases"*. The
question being asked was *"how many lines does porting the label guard convert"*,
and the conjunction means either guard alone sends the line to the node builder.

This is the failure `docs/conventions.md` describes under *"A number can be true
and answer a different question"*, arriving from a direction that section does not
yet cover: **an attribution order chosen to model upstream's control flow hides,
under its dominant arm, the very lines a subordinate arm would convert.** The
per-guard split reads correctly as *"which guard does upstream use here"* and
incorrectly as *"what is this guard worth to port"*. Every remaining positional
arm in the table above is therefore an **under**-estimate by roughly the same
factor, and none of that changes the `hadErrorBaseline` verdict — it is the arm
that absorbs, not one that is absorbed.

The error ran 2.9× low. Recorded as a miss.

### Controls that held

- No line that was right could break: the guard fires only where our type is the
  error type, upstream prints `: error` on 0 of 317,893 aligned lines in
  errors-baseline cases, and the label arm's `-> error` column was 0. The measured
  `wrong` delta of +3 with a `right` delta of +597 and no offsetting loss confirms
  it.
- The JSX element lines that print `error` — the named control — cannot move: the
  guard tests `LabeledStatement`/`BreakStatement`/`ContinueStatement` parents
  only, and the intrinsic JSX arm still reads 0/0/0/0.
- `aligned` unchanged at 468,921; `CONTRADICTIONS` still 0.

### The two tests, and the mutation that reddens each

| test | mutation | result |
|---|---|---|
| `a_label_name_prints_any` | drop the jump-target half of `IsLabelName` | `["any", "error"]` — converts the declaration, leaves the `break` target |
| `a_label_shadowing_a_value_keeps_the_type_we_computed` | drop the `IsTypeAny` precondition | `["1", "any", "1"]` — converts a label our checker resolved to a value |

Neither mutation reddens the other test, so they discriminate different things.
The second is not hypothetical: given `const outer = 1;`, this checker resolves
the label `outer` to the variable and answers `1`, because it does not keep labels
in a separate namespace. Upstream never faces that, which is exactly why the
precondition has to be carried across rather than reasoned away.

## The corrected table, and a second clean sub-position

Re-measured through `assertions_for_case_with_ids`, with libs, at `edb37c3`:

```text
guard (upstream's conjunction order)         claims    -> any  -> error  -> other
hadErrorBaseline (case-scoped)                94801     28850         0     65951
binding element parent                         1105       309         0       796
property access / qualified name parent        6066       447         0      5619
label name                                        0         0         0         0   <- ported, 53588b1
global scope augmentation                         0         0         0         0
meta property                                     0         0         0         0
import statement name                           897       238         0       659
export statement name                           364        92         0       272
intrinsic JSX tag  [PORTED d6dc9a7]               0         0         0         0
NO GUARD (control bucket)                     37198      5146       391     31661

aligned 468900, right 291371 (62.1393%), gap 140040
```

The `label name` arm reading 0/0/0/0 is the ported-arm signature, and is now the
second entry with it. Its own conversion is inside the 62.1393%; this run cannot
re-derive the +597 because the lines it converted no longer print `error`. **That
figure is the one number here still carrying the lib-less defect** — it was
measured on the same broken probe, and re-deriving it would need the guard
reverted and another corpus run. Treat +597 as unverified; the arm's *shape*
(209 claims, 0 residue) is what justified porting it, and that is confirmed by
the corrected run for the arm's sibling below.

### `binding element: the property name` — 192 lines, 0 residue

The sub-position splitter another agent added to this probe (`role_of`) found
something the guard-level table cannot show. The `binding element` arm as a whole
is unattractive — 1,105 claims, 309 converting, 796 residue, the mixed shape that
carries the full false-credit discount. **One position inside it is clean:**

```text
position                                           claims    -> any  -> error  -> other
binding element: the property name                    192       192         0         0
```

192 claims, 192 convert, empty `-> other` — the label-arm signature exactly. It
is spread over **48 cases with the top 10 holding 57.8%**, so it is not one file
pretending to be a rule. This is the recommended next slice, and it is only
visible because the arm was split by position inside its parent rather than by
the parent kind the guard names — the `conventions.md` rule "a row named after a
position is usually not about that position", arriving one level deeper than the
guard table could reach.

**Not built.** It is measured and recommended, not ported; no commit implements
it.

### What the correction cost, and what it did not

The gradient I quoted for the label slice (58.0441% → 58.1714%) was against a
lib-less denominator and should not be cited. What survived the correction
unchanged: the structural argument that `hadErrorBaseline` is case-scoped rather
than per-node (read from upstream source, not measured), the 7% control rate, the
`largeControlFlowGraph` concentration, and the emptiness of the global-scope and
meta-property arms. What moved: every absolute magnitude, by roughly a quarter.

**The general lesson is `dc61c84`'s and I re-learned it the expensive way: a probe
that re-implements the harness measures a different compiler.** I modelled the new
probe on `examples/qualified_name_left.rs`, which has the same per-unit shape, so
copying a working example propagated the defect. Following a local precedent is
not a substitute for checking which entry point the gradient itself comes from.
