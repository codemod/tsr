# Conventions

## Anchor ported code to upstream

Every ported item names its typescript-go counterpart in a doc comment:

```rust
/// Ported from typescript-go's `core.PagedLinkStore`
/// (`internal/core/linkstore.go`).
```

**Why:** we chose an idiomatic rewrite ([ADR-0001](adr/0001-idiomatic-rewrite.md)),
which means upstream fixes cannot be diffed and replayed onto our tree — each one
must be re-derived by hand. The anchors are what let a drift tracker map an
upstream commit touching `internal/checker/relations.go` to the Rust items
claiming to port it. Without them, tracking a moving 60k-LOC checker is
archaeology.

This is oxc's own practice in `oxc_type_checker`, and it is the difference between
mechanical drift tracking and manual review of every upstream commit.

**Anchors are checked.** `cargo xtask anchors` verifies that every cited file,
line and Go declaration still resolves against the pinned submodule, and CI fails
if one does not. A cited **file or line is checked wherever it appears**, phrase or
no phrase — `` (`binder.go:214`) `` mid-sentence counts, because a `.go:N` span
cannot be anything but a claim about upstream. A cited **Go declaration** needs one
of the phrases above in front of it, since a bare backticked name is otherwise
indistinguishable from prose. Pointed at a newer checkout with `--upstream`, the same check *is*
the drift report. An anchor nobody verifies is worse than no anchor, because it is
believed — the tool found four broken ones on its first run, three of them written
the previous day. See [architecture/upstream-anchors.md](architecture/upstream-anchors.md).

Coverage is reported per crate but does **not** fail the build: most crates are
near zero, and a gate that always fails gets deleted rather than fixed. Raising
that rate is a ratchet, not a gate.

## Generated code is never hand-edited

`crates/tsr-ast/src/generated/` is produced by `cargo xtask codegen`. It is checked
in so a plain `cargo build` works without the submodule; CI regenerates and diffs
to prove it has not gone stale. Hand-editing it produces a change that silently
reverts on the next regeneration.

## Codegen fails loudly

The generator `bail!`s on any upstream construct it does not recognise, rather
than falling back to a permissive default.

**Why:** an earlier version degraded unmapped types to an opaque `Node`, which
quietly hid 15 token-alias types. A generator that silently copes is a generator
whose output nobody can trust. The cost of failing loudly is one line added to a
match arm; the cost of coping quietly is a conformance gap discovered a year later.

## Tests that depend on the submodule skip, not fail

Conformance tests read `vendor/typescript-go`. When it is absent they print a skip
notice and return, so a checkout without submodules still runs the unit suite. CI
checks out submodules, so the real assertions always run somewhere.

## Prefer measured facts to assertions

In docs, comments, and commit messages: "upstream reads `.Parent` 2,092 times
across `checker`/`ls`/`binder`" is worth more than "parent access is hot". Cite
paths. Anchor to the pinned commit.

## Macros over hand-written repetition

With 351 syntax kinds and 192 node types, anything written per-node is written
wrong eventually. Declarative macros (`define_index!`, `side_tables!`) for
structural repetition within a crate; `xtask` codegen for anything derived from
upstream.

## Never report an unimplemented stage as passing

A conformance suite whose subject does not exist reports **0%**, loudly, with the
reason stated. It does not skip, and it does not omit the row. A missing number
and a zero look identical in a summary table, and only one of them is honest.

## `unsafe` needs a reason and a `SAFETY` comment

The workspace sets `unsafe_code = "deny"`. Using it means an explicit
`#[allow(unsafe_code)]` with a comment saying why no safe design works, and a
`SAFETY` comment on each block stating the invariant it relies on. An `#[allow]`
without that is a review failure. The full argument and the current exception list
are in [ADR-0011](adr/0011-unsafe-is-opt-in.md); there are three exceptions and the
list is meant to stay short enough to read.

## Per-node mutable state goes in a side table, never in the node

A `Cell` or `RefCell` in a node makes the whole tree non-`Sync` and silently
removes the ability to bind or check in parallel. State that varies per node lives
in a side table keyed by `NodeId` ([ADR-0003](adr/0003-tree-plus-side-tables.md)),
which can be locked or sharded independently of the tree. See
[ADR-0012](adr/0012-ast-is-sync.md); there is a compile-time assertion, and it is
the only thing keeping the property true.

## A check that cannot fail is not a check — anchor it uniquely

Two rules, both bought with incidents rather than reasoned out in advance.

**Assert a substring occurs *exactly once* before writing it, not merely that it
is present.** The weaker form was already the rule here after two silent no-op
replaces slipped through. It is not enough. On 2026-08-05 a mutation test aimed
at `from_root_files` matched the *identical* three lines in `Program::new`
first — the mutation applied, tests went red, and the signal read exactly like a
passed verification. It was caught only because the failing test names were the
wrong ones; had the two functions shared a test, a mutation that never touched
the code it claimed to would have been recorded as verified. A one-shot replace
against a non-unique anchor tests whatever it happened to hit.

**Never act on a belief about a file's or a number's state — check it.** Five
incidents in one day, all the same shape: a `git checkout --` that destroyed
another agent's uncommitted work; a `cargo fmt --all` run across three
half-written files by an agent assuming it was the only writer; a
restore-byte-for-byte that would have silently reverted someone else's fix; a
recovery replayed onto a file that had already been recovered; and a superseded
falsifier quoted from memory an hour after the correction was read. The
countermeasures are all one countermeasure: diff against `git show HEAD:<path>`
rather than against a copy whose freshness is an assumption, quote a denominator
with every number, and prefer `git commit -- <paths>` over `git add` then
`git commit`, which can be raced by a concurrent stage.

The general form, which is the same failure as the instrumentation ones in
[checker-oracle.md](architecture/checker-oracle.md): **a check that appears to
pass while measuring something else is worse than no check**, because it also
spends the credibility that a real check would have earned.

## A measurement is attributable to a commit, or it is not a measurement

Two traps, both hit on 2026-08-05 while four agents shared one workspace.

**Never take a number from a working tree that holds other people's unfinished
edits.** A probe run in the shared tree read 22,360 and then 21,485 twenty
minutes later. Neither was wrong; the tree moved. Pin the measurement to a commit
in a `git worktree`, always. The symptom that caught it is the durable part: a
mutation appeared to move lines *into* a bucket that first-match-wins precedence
cannot reach, which is impossible — and noticing an impossible result is a more
reliable detector that the substrate moved than any assertion about freshness.

**`git worktree add` does not populate submodules.** The corpus is then absent
and the run silently judges nothing rather than failing. Symlink
`vendor/typescript-go` into the worktree before measuring.

## A probe needs a control bucket and a positive control, and they are different

An attribution probe that sorts every line into one of N causes produces a tidy
table whether or not its model is right. Two separate guards, both of which
earned their place the day they were written:

- **A control bucket** — "attributed to none of the causes" — printed
  unconditionally, including at zero. On its first run it was the *majority* at
  51.44%, which is what revealed that most of the population predated the changes
  being attributed.
- **A positive control per cause**, counting how often the arm fires at all.
  Without it, an arm reading zero is unreadable: it cannot be distinguished from
  an arm asking the wrong question. One arm read 0 wrong lines and its positive
  control read 52 firings over 261,042 candidate lines — absurdly low, which
  exposed a defect in the *model*: a `.types` line is emitted for the declaration,
  not for the type reference inside its annotation, so `class C<T> { p: T }`
  produces a line for `p`, a `PROPERTY` symbol, and an arm asking what the
  identifier resolves to can never see it.

Report the arms **non-exclusively** as well, with a "matched more than one arm"
count, so first-match-wins precedence cannot hide a doubly-explained line — the
same failure as the roll-up in
[checker-oracle.md](architecture/checker-oracle.md).

**A zero from an instrument that cannot see the thing is not a zero.** Report it
as untested. This matters most when the instrument is measuring its author's own
work, where the favourable reading is the one nobody will question.

## Build before committing a file another agent is also editing

`git commit -- <paths>` instead of `git add` then `git commit` protects against a
concurrent stage sweeping *whole files* into your commit — a race that happened
on 2026-08-05 and is visible in the commit's file list.

It does not protect against the subtler form, which happened the same day: **one
file that two agents are editing**, where committing it takes *half* of a change
whose other half lives in a different file. `TypeData::Anonymous` was committed
into `types.rs` without the matching arm in `printing.rs`, and `origin/main` did
not compile for four commits. The file list looked exactly right.

Only building catches it. Run `cargo build -p <crate>` immediately before any
`git commit -- <paths>` that touches a file outside your sole ownership. Seconds,
not a full gate.

## Attribute by the answer, not by the name

A `.types` line is emitted for a **declaration**, not for the type reference
inside its annotation. `class C<T> { p: T }` produces a line for `p` — a
`PROPERTY` symbol — so a probe that asks "what does the identifier on this line
resolve to" can never see a change that altered what `T` resolves to.

That is not a quirk of one probe. Given what the walker emits, **any slice whose
effect lands on a declaration is invisible to a name-side arm**, and that is most
slices. Attribute by whether the *answer* has the property you changed, not by
what the name resolves to, whenever the change alters what a declaration's type
is.

Measured cost of getting this wrong: an arm read 0 wrong lines and its positive
control read 52 firings; asking the same question of the answer instead read
9,735 — a blind spot of **187×**, on the author's own work, in the direction that
would have flattered it.

### Two seams in the rules above, both found by the rules catching something

**Gating your own crate does not gate your own examples.** `cargo clippy -p <crate>
--all-targets` lints examples under a weaker set than `--workspace` does, so a
local gate can read clean and hand a broken commit to the integrator. Verify at
the commit in a worktree before pushing anything with an `examples/` file in it.

**Build the commit, not the working tree.** Building the shared tree proves *the
tree* compiles, which is a different claim: the tree holds other agents'
uncommitted halves, so it can compile while the commit does not. That is exactly
what left `origin/main` broken for four commits. Check out the commit in a
detached worktree and build there.

Both were found on the first use of the rule they refine, which is the argument
for writing rules down while the incident is fresh rather than after.

### A third seam: `git` resolves against the *inherited* working directory

`git worktree add`, run from a shell whose cwd had been left inside
`vendor/typescript-go` by an earlier command, created a worktree **in the
vendored submodule** and reported `HEAD is now at 5b1047d10` — upstream's commit,
not ours. It succeeded, printed a plausible line, and produced a real worktree of
the wrong repository.

**Use `git -C <repo>` or an absolute path for every git command in a repository
with submodules.** Never a bare `git` on an inherited cwd.

This is the same shape as the other two seams and as both instrumentation
findings: **the failure produced a plausible result rather than an error.** A
wrong-repo worktree is indistinguishable from a right one until you read the
commit hash. Measuring in it would have failed loudly — there are no Rust crates
there — but a command that merely *wrote* would not have.

The general rule the three seams share: **be explicit about which object you are
acting on, because the implicit one is plausible and wrong.** The commit rather
than the tree; the workspace rather than the crate; the repository rather than
whatever directory you were last in.

### The sweep hazard has three forms, and only the third is preventable by the sweeper alone

`origin/main` was left not building **twice** on 2026-08-05, both times by a
commit that swept part of another agent's in-progress change out of the shared
working tree. The three forms, in increasing subtlety:

1. **Whole files swept in.** A concurrent `git add` puts another agent's files
   into your commit. Visible in the commit's file list. Fixed by
   `git commit -- <paths>`.
2. **One shared file's two halves split.** `TypeData::Anonymous` was committed
   into `types.rs` without the matching arm in `printing.rs`. The file list looks
   *correct*, because the file genuinely belongs in the commit.
3. **A shared file declaring an absent module.** `pub mod optionality;` reached
   `main` in a commit whose author had never seen `optionality.rs` — `lib.rs` was
   modified in the shared tree and legitimately belonged in their commit, while
   the module file was untracked and did not. `E0583: file not found for module`.

**`git commit -- <paths>` does not help with (2) or (3)**, because the shared file
is genuinely yours to commit. Only building the commit does.

Two rules, and the second is the cheap structural one:

- **Build the commit, not the tree** — the sweeper's obligation, not just the
  author's. Check the commit out in a detached worktree and build there.
- **Add `pub mod foo;` to `lib.rs` in the same act as committing `foo.rs`**, never
  earlier. A `lib.rs` left modified in the shared tree while its module is
  untracked is a trap armed for whoever commits next, and they cannot see it.

Both breakages were found by a *measurement attempt failing to build* rather than
by a gate — the same property as every other finding today: **the failure
announced itself as an impossible result, not as a wrong one.**

## Make the failure impossible, not merely detectable

Six findings on 2026-08-05 divide cleanly, and the division predicts what each
one cost.

**Five announced themselves as *impossible* results** — a mutation moving lines
into a bucket first-match-wins cannot reach; a conformance run failing to build
instead of returning a number, twice; a `git worktree` reporting the vendored
submodule's commit hash; an attribution arm firing 52 times where 9,735 was the
truth. Each cost minutes. **Impossibility is self-announcing**: you cannot look
at a worktree at upstream's hash and rationalise it.

**One announced itself as a *plausible* result** and cost four cycles: the parser
produced a `TypeReferenceNode` with `type_name: None` and **no diagnostic**,
silently gapping 257 `as const` assertions and every type reference with a
keyword segment (`bd tsr-0ao`). A plausible result recruits you into explaining
it.

So the design rule is not "detect the failure" but **make it impossible to
produce silently**:

- A required field that can be absent without complaint is a defect, whatever
  the absence later causes.
- An attribution arm that can read zero without complaint is the same defect at
  a different scale — which is why every arm needs a **positive control** as well
  as a control bucket.
- A control bucket that must be reported means a model cannot silently absorb
  what it does not explain.

The two instrument guards in
[checker-oracle.md](architecture/checker-oracle.md) are this rule applied twice,
and the parser defect is what it looks like when nobody applied it.

### Run the mutation. Do not read it.

On 2026-08-05 two people shipped a decoration test within an hour of each other,
independently, and both caught it only by *running* the mutation rather than
reasoning about it:

- a constructor-type test whose helper searched for `FunctionType` only, so on a
  constructor fixture it asked a vacuously false question — the fold-in mutation
  left it green;
- a type-query guard whose assertion could not distinguish the fix from its
  absence through the helper it used, so forcing the guard false left it green.

Both tests looked obviously correct. **The mutation step is not optional even
then** — especially then, because a test that looks obviously correct is the one
nobody re-reads.

And the mutation itself needs verifying: two attempts that day silently failed to
apply because the anchor did not match, and would have been recorded as
"verified" had the substitution not been counted with `grep -c` afterwards.
**Confirm the mutation applied before believing it did not bite.**

### Two more rules for reading an instrument

Both learned twice the same afternoon, by different people, from opposite sides:

- **A bucket you cannot reproduce is not a false positive.** Three hand-built
  fixtures all came back clean for one defect because all three were the wrong
  shape — a lone accessor already gaps, and only a *merge* reached the arm. From
  the other direction, a probe arm read zero because it asked about the name when
  the change altered the answer. **A null from an instrument is a fact about the
  instrument until shown otherwise.**
- **Aggregates rank; per-file dumps diagnose.** The histogram says what to work
  on. It never says why a line is wrong, and no amount of re-aggregating it will.

### A fourth sweep form: an approved-but-uncommitted edit is indistinguishable from the owner's own work

The three forms above are about what a commit *takes*. This one is about what you
*leave*.

On 2026-08-05 an agent asked permission for a one-line arm in a contended file,
was told to hold, and **left the edit sitting in the shared tree while waiting**.
Another agent committed that file. Their commit called a method whose module was
still untracked, and `origin/main` did not build until the next commit.

Asking first is not enough, and the agent did ask. **An approved-but-uncommitted
edit in someone else's active file is indistinguishable from their own work**, so
it gets swept in good faith — the committer cannot tell it is not theirs, and an
untracked module is invisible in their diff.

**Hold the edit outside the tree and apply it in the minute before committing.**
A worktree is the mechanism: verify there, keep the shared tree clean of your
pending edit, and touch the contended file only when you are ready to commit it
in the same act.

### Predict which histogram row you move, and by how much, before measuring

A slice that lands green, passes its own tests, and moves **nothing** is the
worst outcome available in this project, because every gate reports success and
the miss is invisible until someone re-measures — by which time the cycle is
spent and the item looks finished.

It nearly happened on 2026-08-05. Return-type inference was built to close a
4,898-line row. It reused `has_no_contextual_type`, a conservative helper
written when its only caller was the concise-arrow-body arm, which answers
*false* for a plain `function f() { return 1; }` because that function's parent
is a source file. Every such function would have gapped. The slice would have
compiled, passed its own new tests, satisfied all four gates, and moved zero of
the 4,898 lines. It was caught by the author, not by any gate — no gate in this
repository can see it.

**So state the prediction up front: which row, and roughly how many lines.**
Then a zero-mover shows up as a failed prediction rather than as a success. This
is cheap — the row is already named in the assignment — and it is the only
instrument that catches this failure mode.

It generalises past the one bug. In the same cycle, four items were assigned
from a board that had gone stale, and all four were already closed. A stated
prediction would have been contradicted by the very next measurement instead of
by four wasted agent-runs.

Attribute **by row, not by total**. A cycle's delta covers every commit in it,
and several commits usually touch the same rows; quoting a cycle number as one
slice's number is the same error as quoting a roll-up as a sum. The instrument
buckets by cause, so the movement in a specific row is attributable to the
commit that addressed that cause, from a single corpus run.

### `cargo fmt --all` rewrites other agents' uncommitted files

The mandatory gate list says `cargo fmt --all`. In a shared checkout that
formats **every** agent's in-flight work, not just yours. It cannot corrupt
anything — formatting is semantics-preserving and their diffs survive — but it
manufactures exactly the condition the staging rules exist to prevent: a working
tree full of files you did not author but did modify, one bare `git commit` away
from sweeping them.

Use `cargo fmt -p <your crate>`, and make the gate `cargo fmt --all -- --check`.

### A gate result describes the tree you ran it on, not the commit you push

`a1ca5b8` claimed "it compiles and clippy is clean". That was true of the tree it
was run against and false of what landed, because another agent's staged file
entered the same commit after the gate ran. The tip was left red, which blocks
every other agent's pre-commit check — the real cost, and worse than the lint.

The same shape appears twice more in that day's record: an agent reported `main`
red when it had actually measured a neighbour's mid-edit state, and a `cargo
clean` was needed because a stale binary in the shared `target/` had baked in a
`CARGO_MANIFEST_DIR` from a different checkout and was failing a test that
passed under `cargo test -p`.

**Verify against a commit, not against the working tree**: `git worktree add
--detach <dir> <sha>` and run the gates there. That is how the red commit above
was identified, and it is the only check that answers "is *this commit* green".

## Estimating what a slice is worth

One cycle produced six predictions, scored honestly. Five missed, one hit, and
the misses were not close: 6.6× high, 3.6× low, 2.5× high twice, an order of
magnitude, and one that named the wrong row entirely. The pattern in how each
number was *made* is sharper than the pattern in who made it.

| how the number was made | result |
|---|---|
| reasoned from code structure | 10,200 → 3,804; "40–50%" → 2.6%; "long tail" → 98.5% one shape |
| counted source occurrences | 871 → 231; 716 → 386 |
| counted a population | 6,000–15,000 → 950 |
| counted lines where the form *appears* | 432 → 1,537 |
| **counted lines + cross-checked against the instrument** | **1,500 → 1,537** |

### Reading tells you what is blocked. Only measurement tells you how much.

These are different questions and conflating them is what produced most of the
misses. An agent read `getBaseTypeOfEnumLikeType` correctly, identified the
missing back-edge exactly, built the right fix — and was still wrong about the
population by more than an order of magnitude, because the lines were somewhere
else. Correct mechanism, wrong magnitude.

The inverse also happens: four items in one cycle were assigned as open when
they were already built, because the board's line counts had been derived by
reading rather than measured.

### Count the lines a failure *blocks*, not the lines where the form *appears*

The unit that matters is the assertion line in a `.types` baseline, and the
mapping from source construct to blocked lines is neither 1 nor constant:

- `var d = Object.assign` costs **three** wrong lines — `d`, `Object.assign`,
  `assign` — with only `Object` right.
- An annotation blocks its own assertion line *and* every reference to the thing
  it annotates: a count of 432 annotation sites corresponded to 1,537 blocked
  lines.
- A source-text count of `get x()` occurrences over-counted its row by 2.5×.

### A count without an independent cross-check is not a prediction

The one prediction that landed (1,500 predicted, 1,537 measured) was the one
where the author counted assertion lines directly **and validated the count
against the instrument before quoting it** — 2,031 against the histogram's
2,003, 1.4% apart. Every number quoted without such a check missed.

A corollary the same agent drew, and the right way to weight incoming estimates:
**trust a number that arrives with a cross-check; discount one that arrives with
a mechanism story.**

### Bucket by the shape of the *answer*, not the shape of the question

A 12,376-line row read as an index-shape histogram says "79% numeric", which
means arrays and tuples, which points at a real blocker — instantiated generic
members. Counting the *answers* instead showed 10,737 of those lines answer
`any`, so the receivers were `any`, not arrays. The syntax histogram and the
answer histogram disagreed and only the second was the cause. One extra command
separated a correct-sounding workstream from a landed fix.

Same disease, stated three ways by three people in one cycle: *sharing a
downstream function is not the same as being blocked by it* — 83% of the rows
attributed to one function were turned back two lines earlier.

### State the prediction, its rows, its commit pair — and how it could be right for the wrong reason

The discrimination check that applies to tests applies to predictions too. A
prediction that names only a direction can be confirmed by accident. Naming the
mechanism makes it scoreable. Naming *how it could be right for the wrong
reason* closes the last gap:

> binder_symbols going up is not automatically a confirmation. The newly-passing
> cases must be ones where a global has more than one declaration. If the
> improvement is spread across single-declaration symbols, the mechanism is not
> the one I named and I want that recorded as a miss even though the number went
> the right way.

Two further requirements, each learned by a prediction failing without them:

- **If a prediction excludes a large group, name the row that group occupies and
  predict for it separately** — otherwise the exclusion is invisible in the
  result. A stated ~871 landed at 231 partly because the ~716 lines explicitly
  *not* claimed shared a row with the ones that were.
- **Name what must NOT move.** One prediction said a 2,003-line row must stay
  put, and that if it moved the analysis was wrong. It stayed at exactly 2,003.
  Predicting what will not change, and being right, is what distinguishes a
  model from a guess.

### Probing is not a substitute for reading

The cheapest diagnosis of the cycle was five lines run against a real program:

```
Math.random   => () => number                  right today
Object.assign => error   (declared only in es2015.core)
```

That demonstrated the receiver path was live and merging was the sole blocker —
proof rather than inference. But it was cheap *only because* the path happened
to be live. Had it not been, five `error`s would have said nothing about which
of several candidate blockers was at fault, and the code reading would still
have been required.

So the rule is not "probe first". It is **probe to find which of several
candidate blockers is the live one, once reading has narrowed it to a few.**

### Three levels of bucketing, and the one that matters

An earlier version of this section said "bucket by the shape of the answer, not
the shape of the question". That is right and still insufficient. There are
three levels, and they diverge:

1. **The syntax of the question** — "79% of these accesses have a numeric
   index", which reads as *arrays and tuples*.
2. **Upstream's answer** — "88% of these lines answer `any`", which reads as
   *the receivers are `any`*.
3. **Our failure** — the join of *our* gap set against upstream's answers.

Level 2 beats level 1: the syntax histogram pointed at instantiated generic
members, a real blocker and the wrong one. But level 2 is not level 3, and on
that same row they diverged maximally. Of the 11,363 lines answering `any`,
**9,999 were in a single file** — `largeControlFlowGraph.types`, ten thousand
accesses hanging off one `const data = []` evolving array. Upstream computes
`any` there through `autoArrayType` machinery this port does not have, so the
receiver is `any` upstream and is *not* `any` here. A fix aimed at level 2 could
not touch them.

**A baseline records what upstream computes. It never records what this port can
compute for a dependency.** That is the ceiling on what any baseline-derived
estimate can know.

### Two kinds of estimate, and you cannot tell them apart in advance

The blind test that falsified the counting method above found *two* failure
modes, which is why its errors ran in opposite directions:

- **Kind 1 — correctable.** The count was of sites where the construct appears
  rather than lines its failure blocks. Correcting for that recovered the answer
  to within 17%, from an original error of 260%. The multiplier is real and
  measurable after the fact.
- **Kind 2 — not correctable.** The quantity depends on whether *our port* can
  produce the precondition, which no baseline records. Two defensible proxies
  bracketed the true answer by **31×** — 7,068 against 225, measured 950.

**Nothing in the baselines distinguishes a kind-1 form from a kind-2 form before
you measure.** A method that cannot tell you which case you are in is not usable
even on the cases where it would have worked.

This also demotes the method's one apparent success. A 744-against-673 agreement
on a different form now reads as luck rather than validation: that form happened
to be kind 1 with a multiplier near 1. It would have gone on being cited as
evidence if the blind test had not been run.

### If you self-test a method, let someone else pick the best case

The agent who proposed the blind test also framed the two forms and nominated
which one should land tightly. It nominated wrong — the form it called the clean
case missed by 3.6×. Choosing your own best case is the last piece of freedom a
self-test leaves you; hand it to someone else.

### A guard rail that cannot observe a change reports "safe" either way

`binder_symbols` was used as the standing revert condition for two binder
changes in one cycle — "if the rail moves down, the change comes out". It could
only have caught one of them.

The suite skips every symbol whose declarations span more than one file
(`crates/tsr-conformance/src/binder_suite.rs:242`), and deliberately so: it
loads no lib files, so requiring cross-file declarations would score the absence
of a standard library as a binder failure. `grep -c bundled_libs
crates/tsr-conformance/src/binder_suite.rs` returns 0.

Global declaration merging is *by definition* about symbols whose declarations
span files. The rail filters out exactly the population the change acts on. Flat
was the only possible outcome — for a correct merge and for one that destroyed
every symbol table alike.

That rail was valid for the enum remap in the same cycle, because an enum member
and its enum are in one file. **Before quoting a rail as the safety condition,
read what it measures and confirm the change is inside it.** Otherwise "the rail
held" is not evidence, and the more confident it sounds the worse it is.

The same applies to predictions: an agent predicted movement on this rail and
could have known it was blind by reading eight lines of the suite first. Getting
the right answer — flat — from a model that was wrong about *why* is the failure
this document keeps describing, and it survives being written down.

### Name a commit pair as `X^..X`, never as "my last commit to this one"

An agent quoted a pair as `<its previous commit>..<its new commit>` and the
range held eleven commits, because teammates had landed in between. It cost a
full corpus measurement. The agent corrected the instance, then made the *same*
error on its next pair — because the correction had fixed the instance and not
the method.

`X^..X` is mechanically correct whatever anyone else landed. The other form
depends on remembering the state of a shared branch, which in a four-agent
checkout is precisely the thing you cannot know. Checked afterwards, every one
of that agent's commits resolved to exactly one commit under the parent form,
and would have been quotable that way from the start.

Verifying the pair is the *measurer's* job too — run `git log --oneline <range>`
before spending a measurement on it, whoever proposed it.

### For a dependency-gated form, probe past the blocker, not at it

The right deliverable for a kind-2 item is a demonstration, not an estimate. But
the obvious demonstration is too weak: **a probe can confirm a blocker is live
without showing it is the only one.** Kind-2 forms are chains by definition, so
removing the named blocker often just exposes the next, and the probe passes
either way.

Ask instead: **what does this form answer once the blocker is removed?** Mocked,
hardcoded, however cheaply. If the answer is still wrong, the blocker was real
and not sufficient — learned for the cost of the same probe.

The accessor work is the worked example. The missing `getReturnTypeFromBody`
seam was genuinely the blocker for case 4. Probing only that would still have
missed that the *ordering* around it was load-bearing: case 4 must gap before
the implicit-`any` arm, or every inferable accessor becomes a plausible wrong
`any`. No probe of the blocker alone surfaces that.

#### The three steps that produce one

1. **Find where the volume actually is.** Rank the *files*, not the rows. One
   command showed 9,999 of 11,363 lines sitting in a single baseline rather than
   spread across the corpus, which is what broke that row open.
2. **Read the source shape, not the aggregate.** The aggregate said "an `any`
   receiver". The source said `const data = []` — an evolving array, a different
   problem with a different owner.
3. **Probe our port on that exact shape.** The receiver is not `any` here, so
   the access propagates our own gap correctly and no work in the access code
   could ever have touched those lines.

Step 3 is what makes it a demonstration rather than a story: it shows the
blocker biting on a fixture instead of merely being plausible. The agent that
found this had a *real* blocker in hand for that row twice — instantiated
generic members, `bd tsr-el3.2` — and it was the wrong one both times. No amount
of reasoning would have revealed that; only the probe did.

### The triage question, repaired: is the prerequisite *met*?

The first form of this question — "does movement depend on the port computing
something else first?" — is **true of every expression form**. `1 + 2` depends
on typing `1`. Read strictly it classifies everything as dependency-gated and
separates nothing.

The repair is to ask not whether a prerequisite exists but whether it is **met
for most of the population**, which is measurable rather than a judgement:

| row | prerequisite | met? | outcome |
|---|---|---|---|
| `a += b` | LHS types | number 73%, string, any — all typed today | **kind 1** |
| `...x` | spread type | string, number, small unions — typed today | **kind 1** |
| `any` receiver | port produces `any` for the receiver | flow/evolving-array, unported | **kind 2**: 7,068 candidates, 950 moved |
| `symbol has no type: …` | the symbol has a type | unmet *by construction* — the row name says so | **kind 2** |

The `any`-receiver row and the `+=` row have the same *shape* of dependency and
opposite outcomes, which is exactly why "has a dependency" cannot be the
criterion.

**The named test of this repaired criterion is `+=` and spread themselves.** If
either converts far less than its row size when built, the repair is wrong too.
Recorded before the work, not after — the first version of this method became
fitted by being invented to explain a number already seen.

### A number can be true and answer a different question

Two sizing errors this cycle had the same shape, and neither was a
miscalculation — both numbers were correct:

- **Summing rows that share a downstream function.** Four histogram rows all
  reached `resolve_call_signature`, which was verified. 83% of them were turned
  back two lines earlier and never got there. The rows were real; "these rows
  share a path" answered a different question than "this fix unblocks these
  rows".
- **Summing the sub-rows you can see.** A bucket was sized at 4,300 by adding
  its `NumericLiteral` and `StringLiteral` rows — the two visible in the
  histogram's top entries. The full bucket across all initialiser kinds was
  **8,549**. Nobody asked what else was in it.

The first over-counted by 2.7×, the second under-counted by 2×, and both were
arithmetic on accurate inputs. **Before summing, state which question the sum
answers and check that it is the question being asked.** For a bucket, that
means enumerating what is in it rather than adding the visible rows; for a
dependency, it means walking from each row to the fix and confirming nothing
earlier gates it.

This is the same family as the three-level bucketing rule: a count over the
syntax, a count over upstream's answers, and a count over *our* failure are all
true, and only the last predicts movement.

### A row named after a symbol flag is usually not about that flag

Three separate histogram rows this cycle turned out to live in the same place —
`get_type_of_symbol`'s dispatch — and each was a **missing `match` arm rather
than missing machinery**:

| row | what it looked like | what it was |
|---|---|---|
| `SymbolFlags(EXPORT_VALUE) / no value declaration` | exports need module plumbing | the binder's local marker carries no value flag, so no arm matches |
| `SymbolFlags(PROPERTY) / PropertyAssignment / initialiser …` | object literals are unported | the literal's type is already right; the *member symbol* has no arm |
| `SymbolFlags(ALIAS) / no value declaration` | aliases need module resolution | true cross-file, false for the same-file half |

**Each time the row's name pointed at the wrong step, because the lookup already
worked.** `get_property_of_type` found the member; the literal printed
correctly; the symbol resolved. What failed was the step after, and the row name
named the step before it.

So: **before believing a row named after a symbol flag or a declaration kind is
about what it says, read that dispatch.** Three diagnoses this cycle would have
been shorter, and one wrong recommendation avoided entirely.

The general form is the three-level rule again — the row name describes the
*question*, and only reading the code says which step answers it.

### A fourth level, and it is the one that predicts *cases*

The three levels above predict **lines**. They say nothing about **cases**, and
the two goals want different items.

  1. the syntax of the question
  2. what upstream answers
  3. what our port fails on — **predicts lines**
  4. **what else in the same case still fails — predicts cases**

Measured: one slice converted **6,610 lines** and flipped **109 cases** of the
1,571 it touched — 6.9%, against a predicted 10–25%. The author's reasoning
("whole-file gating needs the rest to match") was the right mechanism and the
wrong estimate, because it counted the cases the fix *touches* rather than the
cases the fix *finishes*.

**61 lines per case flipped.** That ratio is the signature of a broad, shallow
fix: it reaches many files and completes few. A narrow deep fix does the
reverse.

So the ranking consequence is concrete, and nothing on the board currently
distinguishes it:

- **Targeting the gradient?** Rank as described above — breadth wins, and the
  concentration check's usual verdict (prefer distributed rows) is right.
- **Targeting cases?** Rank items concentrated in *few files with few other
  defects* — the opposite verdict. A distributed row is the worst possible shape
  for the case gate.

The statistic for level 4 is the distribution of *remaining* failures per
affected case. It costs one extra bucket in a probe that is already being run.

### A conservative `false` is safe for one kind of consumer and unsafe for the other

`is_type_assignable_to` answers `false` for two distinct object types because
structural comparison is *narrow*, not because they are unrelated. That was
already documented as an incompleteness. What was not documented is that its
**safety depends entirely on who is asking**:

- A consumer that acts on `true` — "is this assignable, therefore allowed?" —
  degrades gracefully. A false negative becomes a gap, which is honest.
- A consumer that acts on `false` — "this candidate does not accept the
  argument, therefore **try the next one**" — does not. A false negative
  promotes the next candidate and yields a **confident wrong type**.

Overload selection was the first consumer of the second kind in this checker,
and nothing in the relater's own documentation could have warned it. The
containment was to restrict selection to the domains
`isSimpleTypeRelatedTo` decides on flags alone, and gap the whole call when
anything falls outside.

**Anything that ranks, filters, or selects is in the second category.**
`getBestMatch`, discriminant narrowing on a union, `filterType`, and
excess-property checking are all shaped this way and will each hit it when
ported. Ask which kind of consumer you are writing before relying on a negative.

### The anchors gate does not see your briefing

`cargo run -p xtask -- anchors` validates anchors **in the source tree**. It
cannot see a `checker.go:NNNN` pasted into an assignment, a `bd` note, or a
message. Seven unverified upstream facts reached agents in one session through
exactly that route — the gate exists and the briefing path routes around it.

Checking costs less than pasting:

```
grep -n "^func (c \*Checker) resolveCall(" vendor/typescript-go/internal/checker/checker.go
```

One number in a brief this session was four cycles stale (`resolveCall` at
`:9563`, actually `:8843` on the pinned commit) and had been copied forward from
a note nobody re-checked.

#### And one file where `git commit -- <path>` is the *unsafe* option

`git commit -- <paths>` takes the **working-tree** state of those paths. That is
usually what you want. It is exactly wrong when the path is a shared *manifest*
whose entries depend on files that are not yet tracked.

`crates/tsr-checker/src/lib.rs` is the case. Two agents each needed one `pub mod`
line in it. Committing it by pathspec would have swept the neighbour's
`pub mod inference;` into the commit **while `inference.rs` was still
untracked** — a commit that does not build, from an operation that looks
careful.

The agent that hit it built the blob by hand instead — HEAD's `lib.rs` plus its
own line, via `git hash-object -w` and `update-index --cacheinfo` — so its commit
carried one `pub mod` and left the neighbour's line untouched in the tree.

The rule is not "always use pathspec". It is: **on a shared manifest, commit the
index you constructed, not the working tree you happen to be standing in.**

### A probe that re-implements the harness is measuring a different compiler

This has now happened three times, which makes it a rule rather than an
incident.

`crates/tsr-conformance/src/types_producer.rs` exposes `assertions_for_case`
(`:642`), which builds **one program per case** with every
`vendor/typescript-go/internal/bundled/libs/lib.*.d.ts` in it (`bundled_libs()`
at `:49`, loaded by `program_for_case` at `:725`). That is the entry point
`types_suite.rs:211` scores the gradient through. A probe that instead calls
`tsr_parser::parse_with_options` → `tsr_binder::bind` → `tsr_checker::Checker::new`
per **file** gets no libs and no program, and therefore measures a checker that
cannot resolve `Math.trunc`, `Object.assign`, any `Promise` member, or any array
method.

The three instances, and what each cost:

| probe | status | cost |
|---|---|---|
| `examples/overload_funnel.rs` | fixed in `731b1ee` | 12,015 callees → **10,265**; a "this harness loads no lib files" caveat, carried forward through three documents and one `bd` issue, turned out to be **false** and aimed at the wrong file |
| `examples/writer_guards.rs` | found 2026-08-05, `bd tsr-qj4` | ADR-0039's resolved ceiling of 40,759 lines, the 39,412-line `hadErrorBaseline` arm, and the 392-of-5,603 control rate that is the whole basis for refusing to port it |
| `examples/wrong_attribution.rs`, `qualified_name_left.rs`, `types_walker.rs` | found 2026-08-05, `bd tsr-qj4` | unquantified; `wrong_attribution` is the instrument that would settle `checker-notes-ctx.md`'s open falsifier |

**The tell is arithmetic on the denominators, and it is cheap.** At `0e8e902`
the gradient is 291,349/478,954 = 60.83%; `writer_guards` reported
272,181/468,921 = 58.04%. The 10,033-line gap in the *denominators* is
legitimate — that probe scores only lines whose subject text aligns, and the
walker agrees with upstream on 97.85% of assertion text, which lands almost
exactly on 468,921. The ~18,600-line gap in the **numerators** is not explained
by that, and it is the whole finding. Before trusting any probe's number,
reconcile its denominator against the gradient's and account for the difference;
an unexplained numerator gap means the two are not measuring the same compiler.

So: **a probe's denominator must be the gradient's by construction, not by
resemblance.** Route through `assertions_for_case`. If a probe genuinely needs a
different population, it must say so in its module doc and print both counts, so
that nobody quotes its rate as the metric.

#### And the same defect at one remove: quoting a probe's rate as the gradient

`bd tsr-4sc` and ADR-0039's resolution both quote **58.17%** as the
`checker_types` gradient. It is `writer_guards`'s rate over `writer_guards`'s
population. The real figure at the same tip is **60.83%**, and every target
quoted against 58.17% is quoted against a number that is 2.7 points low and
measured without a standard library.

This is *"a number can be true and answer a different question"* arriving a
fourth time, and the fourth time is worth a stronger rule than the first three
produced: **when you quote a rate, name the instrument that produced it in the
same sentence.** "58.17%" is unfalsifiable; "58.17% on `writer_guards`'s aligned
population" invites exactly the check that found this.

### Size a positional arm from the flattened run, never from the attributed one

The label-name guard predicted **209** and converted **597** — 2.9× low — and
`docs/architecture/checker-notes-guard.md` diagnosed it correctly at the time:
upstream's writer condition is a **conjunction**, attribution gives the dominant
arm precedence, so every line a subordinate arm would convert is already counted
under the dominant one. It then drew the weaker conclusion available — *"every
positional figure in that table is an under-estimate"* — and left the multiplier
unknown.

The repair is mechanical and it is now tested rather than asserted. The
instrument already has a mutation that answers it: `WRITER_GUARDS_FLATTEN=1`
drops the dominant arm from the attribution order and lets the subordinate arms
claim what they would actually convert. **That column is the answer to "what is
this arm worth to port". The attributed column answers "which guard does upstream
use here".** Two true numbers, two different questions.

Scored on two arms in one commit, both predicted before building:

| arm | attributed | **flattened** | measured |
|---|---:|---:|---:|
| label name (re-derived with libs) | 209 | 594 | **594** |
| binding element: the property name | 192 | 428 | **428** |

Exact, twice. The generalisation is not about the writer guards: **whenever an
instrument attributes a line to the first of several conditions that could each
have claimed it, its per-condition counts are answers about upstream's control
flow and not about your work.** Ask whether your instrument's attribution order
is modelling something — here, upstream's own conjunction — because if it is,
that model is silently subtracting from every subordinate row.

#### The corollary that saved a build

A clean signature is a hypothesis, not a licence. Two positions in that table
have near-identical syntax and opposite dispatch: the property name of a binding
element (`const { a: b } = x`) is `errorType` by construction, because
`getTypeOfNode` (`checker.go:31927`) falls off its end — `IsInExpressionContext`
tests `parent.Initializer() == node` and `IsDeclarationName` tests
`parent.Name() == name`, and a binding element's `Name()` is the *bound* name.
The property name of an **import specifier** looks the same and is not, because
`IsDeclarationNameOrImportPropertyName` has an explicit `KindImportSpecifier`
case, so upstream reaches `getTypeOfSymbol` and holds a real type — 37 of 123.

**Port the position, not the arm, and trace the dispatch before trusting the
signature.** The same check found that the qualified-name-left position was
already covered and holds 141 of its arm's 25,681 claims — a true claim, and far
narrower than its row name reads.

### A ratio is robust to an instrument defect only if the defect lands outside its denominator

Worth separating from the rule above because the reasoning nearly went wrong in
the confident direction.

When `writer_guards` was found to be lib-less (see the previous section), the
prediction stated in the briefing was that the **control rate** — 392 of 5,603,
7.0%, the entire empirical basis for refusing to port `hadErrorBaseline` — was
the leg at risk, and that the **concentration** leg would survive untouched.

Measured, it was backwards. The control rate went **7.0% → 7.06%**, the most
stable number in the table; the concentration leg fell from **49.1% to 33.4%**.
Both survived, so the conclusion held, but the risk assessment was wrong and it
was asserted in a briefing.

The tempting explanation — "the defect inflates both columns of the ratio, so
ratios are robust" — is also wrong, and predicts too much. What actually
happened: the lib-less lines were real and numerous (the control bucket's
`-> other` column fell 39,310 → 31,661, ~7,650 lines a loaded lib resolves) but
they land where upstream holds a **real type**, which is *outside* the
any-flagged denominator the ratio is taken over. They never entered either side
of it.

So the usable rule is narrower than "ratios are robust": **a ratio is robust to a
defect whose lines land outside its denominator, and is not robust to one whose
lines land inside.** An absolute count, or a share of a total, has no such
protection — which is why the concentration leg moved and the rate did not.
Deciding which case you are in costs one bucket in the probe and is not
available from the argument.

### Pre-register on the most direct bucket your instrument produces, not on a proxy

The level-4 probe for `bd tsr-4qx` printed a histogram of remaining failures per
affected case, with buckets `0 / 1–5 / 6–10 / 11–25 / 26–100 / >100`. The
question being decided was *"would closing this row finish any case?"*. The
decision rule pre-registered against it was **"≥25% of cases at ≤10 remaining"**
— a proxy, reasoned as "≤10 is roughly what 4.13 nodes per case could finish".

The direct answer was the **`0` bucket, one row above the proxy**, and it read
**0.0%**. The rule did not look at it.

Measured, the proxy said *build* (29.8% ≥ 25%) and the direct bucket said the row
finishes **no case at all**. They disagreed, and the build was avoided only
because a separately-registered override — about the concentration of the top
cases — happened to reach the same conclusion by another route. **That is luck,
not method.**

This is a distinct failure from the ones already catalogued here. Building the
wrong instrument is caught by a control bucket. Building the *right* instrument
and reading a derived row off it is caught by nothing, because every number
involved is correct and the partition is sound. It is the same family as *"a
number can be true and answer a different question"*, arriving one level in:
**the instrument answered the question directly, and the rule was written against
a quantity computed from the answer instead.**

So, when pre-registering a threshold: look down the list of buckets the
instrument will actually print and ask which one *is* the question. If one of
them is, the rule goes on that bucket. A proxy is only legitimate when no bucket
answers directly — and then it should be labelled a proxy, so the next reader
knows the rule is one inference away from the evidence.

The agent that wrote the rule found and recorded this against itself.

### A count over baseline *files* is not a count over judged *cases*

The corpus on disk and the population a suite scores are different sets, and the
gap is large enough to invert a decision.

Sizing the TS2322 diagnostics item, I counted `.errors.txt` files under
`vendor/typescript-go/testdata/baselines/reference/submodule` and reported the
item as **+536 cases**. **1,180 of those files are configuration variants the
`diagnostics` suite excludes by construction** (`crates/tsr-conformance/src/corpus.rs:163`).
Over the suite's own 5,488 judged cases the same measurement is:

| | quoted | actual (judged) |
|---|---:|---:|
| reachable today | 1,063 | **765** |
| + TS2322 | 1,599 | **1,254** (gain **+489**, not +536) |
| cases whose only code is TS2322 | 516 | **478** |

The **shares** survived almost exactly — 22.76% against 22.85%. The **counts**
did not. A rate computed over the corpus can be quoted; an absolute count cannot,
until it is reconciled against the suite's own denominator.

This is the third time configuration-varied baselines have distorted a count
here. `bd tsr-bb4.1` tracks them, and the stored `bd` memory about the 793
config-varied baselines and the 617 cases with no output of any kind records the
same family: **the corpus directory is not the population, and file presence is
not evidence of judgement.**

#### The part worth internalising is not the arithmetic

The `bd` issue that carried the wrong number **also carried the caveat that would
have caught it**, written by the same author in the same sitting:

> *applying a rate measured over 7,025 baseline cases to the 5,488 judged assumes
> the skips are neutral on code mix. Unverified; that is the first thing to check
> before quoting ~750 as a target.*

The caveat was written, and then the absolute number was quoted anyway — in an
ADR and to the user. **Flagging a risk and then propagating the number is worse
than not flagging it**, because the caveat reads as diligence and licenses the
very number it warns about. A caveat is only doing work if something downstream
is *blocked* on it. If you write one and then quote past it, delete the number,
not the caveat.

### Read the values a `switch` assigns, not the shape of the `switch`

ADR-0040 asserted that upstream's TS2322 message is "an output of the relation
walk", on the strength of `reportRelationError` (`relater.go:4780-4797`) being a
`switch` that assigns to one `message` variable through five branches. That
reading made a call-site formatter look unfaithful and produced decision (3), a
reporting twin taking an error node.

**It is wrong, and one `grep` says so.** Each branch assigns a *different*
`Message`, and in this codebase a `Message` carries its own code
(`internal/diagnostics/diagnostics_generated.go`):

| branch | message | code |
|---|---|---:|
| `relation == comparableRelation` | `Type_0_is_not_comparable_to_type_1` | **2678** |
| `sourceType == targetType` | `…Two_different_types_with_this_name_exist…` | **2719** |
| `exactOptionalPropertyTypes` | `…with_exactOptionalPropertyTypes…` | **2375** |
| string-literal suggestion | `…Did_you_mean_2` | **2820** |
| fallthrough | `Type_0_is_not_assignable_to_type_1` | **2322** |

**The switch is five diagnostics, not five renderings of one.** So filtering on
2322 *already* selects the fallthrough branch, every alternative arm is
separately unreachable for it, and 0 of 2,888 TS2322 header texts in the corpus
are anything else. A call-site formatter is faithful, and decision (3) was
over-engineering — struck through in place rather than deleted.

The general form: **a control-flow shape is not evidence about what the code
produces.** "Five branches assigning one variable" reads as *five ways to say the
same thing* and is equally consistent with *five different things*. Only the
assigned values distinguish them, and they were one `grep` away.

This is the same family as "a row named after a symbol flag is usually not about
that flag", one level down: there the *name* pointed at the wrong step, here the
*syntax* did. The corrective is identical — resolve to the concrete thing (the
dispatch arm, the message code) before building on the reading.

### A control bucket over a classifier whose last arm is a default cannot fire

This project leans on control buckets harder than on any other check. *"All three
control buckets read zero, and that is the only evidence the sub-rows are a
partition rather than a list of seven things that happen to be true."* The check
is sound — and it has a failure mode that reads exactly like success.

`rank_board`'s `cause()` classifier is four arms with a **default**:

```rust
if reason.contains("the receiver is a gap") { PropagatedNamed }
else if gapped_below                        { PropagatedSpan }
else if reason.contains("/ initialiser ") || reason.contains("/ annotation ") { Unknown }
else                                        { Terminal }
```

Beside it: `CONTROL UNATTRIBUTED by the TERMINAL/PROPAGATED split = 0 (must be 0)`.
**That control is structurally incapable of reading anything but zero**, because
`cause()` is total — every line receives one of four labels, so nothing is ever
unattributed. It has read zero on every run and proved nothing on any of them.

The damage was not the vacuous control; it was what the default *absorbed*.
`Terminal` was read across the board as **"the prerequisite is met, so the row
size is its worth"**, when what it means is **"none of three evidence patterns
matched"**. For a declaration-name row those cannot match by construction —
`gapped_below` is a span test and a declaration name spans only itself — so such
rows fall to `Terminal` whatever they actually depend on. One row so labelled,
2,618 lines, measured at **68.6% propagation** when someone finally looked;
building it would have converted zero.

So, two rules:

1. **A control bucket only proves a partition if a line can actually reach it.**
   Before quoting one, ask what input would make it non-zero. If the classifier's
   last arm is `else`, `_ =>`, or `default:`, the answer is *nothing*, and the
   control is decoration. Give the classifier an explicit terminal arm and let
   the genuinely unmatched fall through to the control.
2. **Never let the semantically loaded label be the default arm.** `Terminal`
   here is the strongest claim the classifier makes — kind 1, size equals worth —
   and it was the arm that required no evidence. Name the default `UNKNOWN` and
   make every load-bearing label earn a positive test.

The mutation discipline does not catch this and cannot. Mutations were written
for three of the four arms and each went red; **a mutation to a default branch is
invisible, because everything that stops matching it still lands there.** The
tell is not a failing test, it is reading the classifier and asking which arm
would notice being wrong.

#### And the direction of that defect: it launders wrong answers into gaps

The rule above says a probe that re-implements the harness measures a different
compiler. Fixing the last two such probes measured *which way* the error runs,
and it is the unflattering direction:

| | lib-less | corrected |
|---|---:|---:|
| gap (we answered `error`) | 173,537 (37.01%) | **139,612 (29.77%)** |
| wrong (ported, defective) | 22,067 (4.71%) | **37,489 (8.00%)** |

**33,925 lines moved out of `gap` and into `wrong`.** Without a standard library
this port answers `error` for a great many nodes where, with the libs loaded, it
answers something *confidently incorrect*. So every lib-less probe made this port
look **more honest than it is** — it converted confident wrong answers into
honest-looking "don't know"s, in exactly the column this project uses to decide
that a row is safe to leave alone.

That is worse than a wrong magnitude, because the gap/wrong split is not a
statistic here — it is the thing that separates *"we have not built this yet"*
from *"we built it and it is broken"*, and every ranking leans on it.

The corrected figure, **37,489 wrong**, is now produced by two independent
instruments — `rank_board.rs` and the repaired `wrong_attribution.rs` — written
by different authors along different code paths. **That agreement is the
evidence**, and it is the strongest form available here: not a control bucket
reading zero, but two things that could disagree and do not.

#### Prefer a control pinned by *construction* over one pinned by *arithmetic*

The rule above says a control only proves a partition if some input can reach it.
There is a second axis, and it is the one that catches the errors arithmetic
cannot.

An agent copying `rank_board`'s span test into another probe reproduced the
expression correctly and its **polarity** backwards — that probe binds
`below[i] = true` to mean *nothing* gapped below, and passes `!gapped_below`. The
inverted reading printed one row as 100% propagated and the other as 100%
terminal, which is entirely plausible unless you already know the first row is a
**leaf**.

**Every sum still reconciled. Every arithmetic control still read zero.** Totals,
roll-ups, pairing counts — all intact, because the bug moved lines *between*
buckets rather than losing them.

What caught it was a bucket whose value is fixed by a property of the subject
rather than by the classifier: *row 5's node is the `b` of `a.b`, a leaf, so
nothing can be nested inside it, so its propagated-by-span count is **0** — and
that was true before any code was written.* Under the mutation it read **65**.

So, two classes, and they catch different things:

| control | pinned by | catches | blind to |
|---|---|---|---|
| `sum of parts == whole` | arithmetic | a lost or double-counted line | any error that moves lines *between* buckets |
| `this bucket is 0 because the subject cannot produce it` | construction | a semantic inversion | a bookkeeping slip |

The usable form: **for each control, ask what input would make it non-zero, and
prefer the one whose answer is fixed by the subject rather than by the code under
test.** A partition of *n* items into *k* buckets has exactly one arithmetic
control and usually several structural ones going unused — a leaf that cannot
contain, a receiver that cannot be generic, a position upstream never visits.
Write those down; they are free, and they fail loudly in the one direction the
sums cannot see.
