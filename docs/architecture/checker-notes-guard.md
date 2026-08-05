# The baseline writer's eight guards, sized

ADR-0039 established *that* upstream's `.types` writer decides between `error`
and `any`, and left *what each guard is worth* unmeasured. `bd tsr-d6o` filed the
ceilings — 69,195 `: any` lines in cases carrying an `.errors.txt`, 19,045 in
cases without — with the warning that they are ceilings and not deliverables.

This note is the measurement. The instrument is
`crates/tsr-conformance/examples/writer_guards.rs`; every number below is one run
of it at `bea80b9`, from a single pinned binary.

## The answer in one line

**The ceiling is 40,759 lines (+8.69 gradient points). The honest deliverable is
about 1,500, and one guard — `hadErrorBaseline` — accounts for 39,412 lines that
would be credited for a type this port does not compute.**

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

- **Label name — 209 claims, 209 convert, 0 `-> other`.** A 100% conversion rate
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
