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

### An anchor that *resolves* and points at the wrong construct is worse than one that fails

`cargo run -p xtask -- anchors` checks that a cited `file:line` exists. It cannot
check that the line is the thing you meant.

Two briefings this cycle described upstream's checker as "taking an interface
(`checker.go:550-566`)". Verified afterwards with `grep -n` on the declarations:

| what | where |
|---|---|
| `type Program interface {` | **`checker.go:547`** |
| the field `program Program` | **`checker.go:581`** |
| first `c.program = program` | **`checker.go:908`** |

`550` is `SourceFiles()` and `566` is `GetProjectReferenceFromOutputDts` — the
middle of the method list. The span is real, so the citation **resolves, the gate
reads 0 unresolved, and the anchor is never looked at again.** An anchor that
fails gets fixed on the next run; one that quietly misleads outlives everyone who
could have caught it.

So the gate's guarantee is narrower than it reads: **`anchors` proves a line
exists, not that it is the declaration you named.** The only thing that proves
that is taking the number from `grep -n` on the declaration itself:

```
grep -n "^func (c \*Checker) resolveExternalModuleName(" vendor/typescript-go/internal/checker/checker.go
grep -n "^type Program interface"                        vendor/typescript-go/internal/checker/checker.go
```

#### Three distinct anchor failures in one session, and they argue for one rule

1. **Stale, in a briefing.** `resolveCall` quoted at `:9563`, actually `:8843` — four cycles old, copied forward, and invisible to the gate because a briefing is not source (see "The anchors gate does not see your briefing").
2. **Off by a few, from counting.** `GetResolvedModule` reported as `program.go:522` and `getExternalModuleMember` as `:14671`; they are `:521` and `:14667`. Both are the error you get from reading a `sed -n` window and counting rows rather than grepping the declaration — and one of them was offered as a *correction* to an anchor that was already right.
3. **Resolving but wrong**, above.

All three have the same corrective, which is why it is worth stating once rather
than three times: **the number comes from `grep -n` on the declaration you mean,
every time, including when you are correcting someone else's.** A window, a
memory, or a neighbouring line is not a source.

### Before "how many lines does this block", ask "can this port *spell* the answer?"

Resolution and rendering are separate capabilities, and a form can be **fully
resolvable and still unprintable**. A slice that resolves a name this port cannot
name converts a gap into a *wrong* line, which is the one outcome worse than
leaving it alone.

The worked example reversed a build order. Cross-file aliases looked as though
the cheapest first arm was `import * as ns from "./m"` (259 lines) and
`import a = require("./m")` (433), because both reach a module symbol **directly**
with no member lookup. But the baselines say:

```
conformance/exportAsNamespace4(module=commonjs).types
  import * as ns from './0';
  >ns : typeof ns
```

and corpus-wide, the assertion after such a line is `typeof cjs` 236, `typeof type`
184, `typeof cjsi` 172, `typeof mjs` 156 — **always the local alias, never the
module**. That is upstream's node builder emitting the shortest accessible chain
to the symbol. A module symbol's name in this port is the file path with the
extension stripped, so we would print `typeof /0` where upstream prints
`typeof ns`. **Those 692 lines are not available work**; building them would have
manufactured 692 confident wrong answers.

The forms that *were* right — `import { x } from` (890) and `export { q } from`
(134) — target ordinary export symbols carrying their own names, and their
baseline answers are `number`, `0`, `typeof A`, `typeof Observable`: spellable.

**The check is one `grep` at the baseline's right-hand side**, and it is the
reason this is a rule rather than an aspiration. It also generalises past
aliases: the same question would have flagged `import a = foo.bar.baz`, which
`Checker::resolve_alias` already gaps deliberately for exactly this reason
(`compiler/aliasBug.types`, `>booz : typeof booz`) — one mechanism, found twice,
years apart.

So the ranking questions are three, in order:

1. **Can we spell the answer?** If not, the row is not work, whatever its size.
2. **Is the prerequisite met for most of the population?** (kind 1 vs kind 2)
3. **How many lines does the failure block?** (never how many lines the form appears on)

### Faithfulness to upstream is not evidence that a guard is load-bearing *here*

The same slice ported upstream's alias-cycle guard — `resolveAlias` pushes
`TypeSystemPropertyNameAliasTarget` (`checker.go:16272`) — added the matching
frame, and then **measured it**: all twelve tests stayed green with it removed,
and so they did with a second frame removed. The only mutation that makes a
two-file re-export cycle actually **hang** is deleting `|| seen.contains(&target)`
from `get_symbol_flags`, which is upstream's own `seenSymbols`
(`checker.go:16368`) and predates the whole slice.

It is unreachable **structurally, not incidentally**: `resolve_alias` is not
self-recursive in this port, because upstream's recursion goes through
`resolveIndirectionAlias` (`checker.go:16293`), which this port does not have.

So the guard was deleted rather than kept as insurance — and the deciding detail
is that it carried a doc comment calling itself *"the variant that makes
cross-file aliases terminate"*, which was **false**. A guard no mutation can make
observable is decoration; decoration that *claims* to be a safety property is
worse than none, because the next reader budgets for a protection that is not
there. `contextual.rs` deleted a guard and the test written to defend it for the
same reason.

**What recurses upstream may not recurse in a port that left a function out.**
Reading picked the wrong mechanism twice here; one mutation found it. Port the
guard, then try to make it fire — and if it cannot fire, delete it and write down
why, rather than keeping a comment that lies.

### A row counts the lines that *name* a defect, not the lines downstream of them

Every sizing method in this document estimates **the row**. The gradient moves by
the row **plus everything the row was blocking**, and the two differ by a factor
nobody had a slot for.

Measured on the cross-file alias arm, `fa29e66^..fa29e66`:

```
  the two target rows lost      -428 gap lines
  the gradient gained           +711 lines
  cascade multiplier             1.66
```

The 283-line difference was never in either row. `import { f } from "./m"; f();`
puts **one** line in the alias row — the declaration name — and the *call* is a
separate assertion line that answered `errorType` only because its callee did.

This reframes what a good prediction looks like. The slice predicted **389**,
which reads as a 1.8× miss against 711 and is in fact **91% of the 428 the rows
actually lost — a 9% error on the right quantity, reported as the wrong one.**
Population held, rate held; the third factor was assumed to be 1.0 and was 1.66.

**Two ratios, both true, answering different questions.** 1.66× against the rows
you predicted is what a *planner* wants. 1.54× against every row that moved
(461, including a same-file row that was not predicted to move at all) is what a
*mechanism* reader wants. Say which you are quoting.

#### And it is a property of the form, not of the fix — so it is not a constant

`export { q }` converted 94 lines with **no visible cascade**, because a
re-exported name is mostly not *used* in the file that re-exports it. An imported
name is imported in order to be used. So the multiplier cannot be carried forward
as a number; it has to be estimated per row, and the estimate is cheap — the same
span-and-name test `rank_board`'s `cause()` already runs, in the opposite
direction: instead of asking *"did something inside this node gap?"*, ask *"how
many gapped lines name this one?"*

#### The mirror of a rule already here

*"Sharing a downstream function is not the same as being blocked by it"* records
a sum that **over**-counted by 2.7×, because 83% of the rows were turned back
before they reached the shared function. This is the same edge from the other
side: counting only the rows that name a defect **under**-counts, because the
lines they block are in other rows or in no row at all. **Before quoting a row as
a deliverable, ask both — what in this row will not convert, and what outside it
will.**

#### And do not truncate the output of the command that assigns the identifier

`bd create … | tail -2` prints the priority and status lines and **cuts the line
carrying the new id**. An id was then quoted from memory in two handoffs and in a
document, and it had never existed: the real issue was `tsr-lgf`, the cited one
`tsr-6yg`. A teammate found nothing behind the citation and refiled the incident
rather than leave a dangling reference, which produced a duplicate — the better
of the two errors, since **a citation nobody can follow is worse than no
citation, because it reads as provenance.**

This is the same rule as re-reading what you store, one step earlier: there the
risk is that the stored text was mangled, here that the identifier you quote was
never read at all. **Read the line that names the thing you are about to cite.**

A cheap standing check, and one page was audited this way in a single loop:
resolve every commit hash and every `bd` id a document cites. `xtask anchors`
already does exactly this for upstream file:line references; commit hashes and
issue ids are the same class of reference and have no gate.

#### The cascade runs both ways: a fix un-gaps lines you did not aim at

The rule above measured a fix converting **1.66×** the lines its row contained,
and read that as good news. The same mechanism can run the other way, and the
sign is not a property of the fix — it is a property of what the newly-computable
type can be **named**.

Measured on the module-object item (`bd tsr-6ph`, `d8452aa`). Giving
`get_type_of_symbol` a real type for a module symbol would convert **577** lines
whose answers are ordinary and spellable. It would also un-gap every line that
asserts *the module object itself* — 648 `typeof ns`, 83 `typeof ns.x`, 461
`import("m").W` — and those would print the module symbol's name, which in this
port is the **file path**: `/aliasAssignments_moduleA` where upstream prints
`typeof moduleA`.

**577 converted against 1,192 manufactured wrong. 2.1 wrong per converted.**

So the item is not "small"; it is **negative**, and no amount of care inside the
577 changes that, because the damage is in lines the slice never targeted. A
sizing method that counts only the intended row cannot see this — it is the
cascade rule's own blind spot, pointed the other way.

**Before building, ask what else becomes computable, and whether *those* answers
can be spelled.** Same one-`grep` check as "can this port spell the answer?",
applied to the collateral rather than the target. And note the escape that
usually exists and must be established rather than assumed: the alias's own
reference position is syntactically distinguishable from a member access hanging
off it, so *"port the position, not the arm"* may still recover the 577 — but
only if the rendering path and the checker's internal type can disagree for one
node, which nobody has shown here (`bd tsr-6j2`).

#### "Looked up in" and "printed" are not as separable as they look

The reverse-cascade section above ends by naming an escape: if a type is
*negative* to build because it cannot be spelled, build it as a **lookup surface**
and never as an answer, so the members resolve and the receiver keeps gapping.
That escape was measured, built, and **reverted the same day** (`3baeb70`,
`a618e3a`).

The design was the careful one. `get_type_of_symbol` never returned a module
type, so the property was supposed to hold *by construction* rather than by a
guard someone maintains — the preference this document records everywhere else.
Measured over the corpus:

```
              before      after     delta
  right      292,606    292,764      +158
  gap        138,585    138,025      -560
  wrong       37,709     38,111      +402
```

**560 gap lines converted: 158 right, 402 wrong — 2.5 wrong per right**, against a
premise of *zero*. The pre-registered must-not-move rows moved in the direction
that gives it away: `typeof ns.x` 83 → 2, `import("m").W` 461 → 327.

**The mechanism is the lesson.** The module symbol genuinely never got a printable
type. But a member *read* through it yields types that reach printed positions the
slice never targeted — most of them the `import("m").W` family, whose entire
purpose in the corpus is symbols with **no accessible name**. Withholding the
type from one node does not withhold what that node makes reachable.

The tell was available in advance and nobody drew the line: the 2.5 measured here
is close to the **2.1** that the same probe had already computed for the *printing*
design. **Two designs that differ in what they print, and agree to within 20% on
what they break, are not two designs.**

So the escape is narrower than stated: *port the capability, not the rendering*
holds only when the capability's outputs are themselves spellable. Before
building a lookup-only version of an unspellable type, enumerate what the lookup
**returns**, not just what it is. If those answers include the same unnameable
family, the escape is the original item wearing different clothes.

### A probe's denominator is its own rendered lines; the suite's is upstream's

`docs/architecture/checker-notes-recvgap.md` §8 published a gradient of
**479,060 lines / 61.00%** beside the committed snapshot's **478,954 / 61.09%**,
called the 0.09pp difference *"not explained"*, and — correctly — refused to
quote the absolute until someone reconciled it. That refusal is the only reason
what was underneath stayed contained to one number.

Reconciled by `crates/tsr-conformance/examples/reconcile.rs`, the paragraph held
**two** differences with nothing to do with each other.

**The denominator, 106 lines: a unit mismatch, not an error.**
`types_suite::compare` opens with `let total = assertion_count(expected)` and
iterates `expected_file.assertions` — every figure the suite produces is over
**upstream's baseline lines**. A probe written the obvious way iterates
`our_file` and counts **the lines we rendered**. Those are different populations
and neither is wrong:

```
  probe total (rendered lines)                        479,060
  - surplus: we rendered more than upstream asserts       964
  + deficit: upstream asserts more than we rendered       858
  = suite total                                       478,954   (actual: 478,954)
```

**The numerator, 389 lines: a defect, in the flattering direction.** The probe
asked `type_string == "error"` *before* it asked whether the baseline matched, so
a line where we answer `error` **and upstream's baseline also says `error`** was
filed as a gap. It is a right answer; programs may declare a type named `error`,
and some do. `rank_board`, `wrong_attribution` and `types_shapes` all test the
baseline first. This is the same direction as the lib-less probes: it moved
correct answers into the column this project reads as *"not built yet"*.

#### The rule

**A probe that quotes a gradient must reconcile its denominator against
`assertion_count`, or quote shares of its own population and say so.** The page
above did the second thing correctly for every figure on it — which is exactly
why nothing on it changed sign when the defect was fixed, and why the one
absolute in its last paragraph did.

#### And the part that generalises past denominators

The reconciliation was written to test **three** hypotheses, in confidence order:
the rendered/baseline mismatch, whole files dropped by the probe's
`our_file.len() != line_ids.len()` guard, and whole cases dropped by
`case.load().ok()?`. The second and third are real divergences from the suite,
correctly identified by reading the two loops side by side, and both measure
**0 files and 0 cases**. They never fire.

That is the cheap half of the method and it is worth stating on its own: **the
hypotheses that turn out to be zero cost one bucket each, and the argument about
which one is right costs more and settles nothing.** Both non-firing buckets are
still printed, because a guard that fires zero times *today* is one refactor away
from firing, and a control nobody prints is a control nobody notices going
non-zero.

#### Two mutations, two controls, and only one of them could have caught either

The four controls read zero, which this document says means nothing until a line
can reach them. Both were run:

| mutation | C1 (construction) | C3 (arithmetic) |
|---|---:|---:|
| **M1** — shift the probe's positional test by one, `get(position + 1)` | **−276,213** | 0 |
| **M2** — invert the surplus/deficit polarity | 0 | **212** |

C1 is *"probe right − `compare` matched, over counted files"*, and it is pinned by
**construction**: the two are the same predicate at the same positions, so they
are one number reached two ways whatever the file lengths are. Nothing about the
corpus makes it zero — the predicate does.

M2 is the polarity inversion this document already records passing every sum,
roll-up and pairing count elsewhere in this repo. It moves 212 lines and C1 does
not notice. M1 destroys the alignment and C3 does not notice. **Neither control is
redundant, and a probe carrying only the arithmetic kind is carrying the one
blind to the error that has actually happened here.**

### Concentration is a case-gate concern; for the line gradient it is leverage

This document tells you four separate times to run the concentration check and to
distrust a row that a few cases dominate. Every one of those instances is sound
and every one of them is about **the case gate**. Applied to the **line
gradient** the same check points the other way, and nobody had said so.

Measured at `9d5b026` with `crates/tsr-conformance/examples/casedelta.rs`, which
dumps per-case tallies from `types_suite::compare`:

| | lines | share of the 478,954-line denominator |
|---|---:|---:|
| the ten largest cases | 117,144 | **24.5%** |
| unmatched within them | 43,843 | **9.15 gradient points** |

```
compiler/largeControlFlowGraph                        30,001 / 50,002   60.0%
compiler/binaryArithmeticControlFlowGraphNotTooLarge  10,598 / 14,349   73.9%
compiler/unionSubtypeReductionErrors                  14,014 / 14,016  100.0%
compiler/resolvingClassDeclarationWhenInBaseTypeResolution 5,957 / 8,108 73.5%
conformance/parserRealSource11                         1,783 /  7,635   23.4%
```

**Half the distance from 61.66% to 80% is in ten files.** `largeControlFlowGraph`
alone is 20,001 unmatched lines — 4.18 points — and it is one file containing
`const data = [];` followed by 10,000 identical `data[0] = 0;` statements.

So the two readings are both true and must not be collapsed:

| target | a row concentrated in one case is… |
|---|---|
| the **case gate** | nearly worthless — 20,001 lines flips exactly **one** case |
| the **line gradient** | the largest lever available — **4.18 points** |

The corrective is not to stop running the concentration check. It is to **say
which number the check is being run against**, because the same measurement
licenses opposite conclusions depending on the answer. A ranking that sorts by
`finishes` and a ranking that sorts by lines are different rankings over the same
rows, and `checker-notes-rank.md` §7 already records that the two axes are nearly
orthogonal — this is that finding arriving at the sizing rules.

#### And say what the number says about the corpus

The honest report for such an item is *"+4.18 points, one case, and it says as
much about the corpus as about the compiler"*. A gradient point bought from a
machine-generated 10,000-statement test is not the same evidence of capability as
a gradient point spread over 500 cases, even though the two are identical in the
summary table. **Quote the case count beside any large concentrated gain**, or
the number will be read as breadth by the next person, including you.

#### The same instrument answers the question the totals cannot

`casedelta.rs` exists because a commit moved the gradient `+431` and the two
available readings — *one case gaining 1,700 while eighty lose*, or *a hundred
cases gaining four* — license completely different next moves. Measured, it was
neither: **24 cases moved, 22 gained 433 and two lost one line each.** A net is
the one statistic guaranteed to hide a fix that is simultaneously helping and
harming, which is the exact shape of any change to what the compiler *knows*
rather than to what it *computes*.

### A control whose two sides share the code under test cannot see a defect in it

Found by an agent against its own instrument, on a mutation it wrote to try to
break it, and reported rather than fixed quietly.

`gaproot.rs` carried four controls. **M1 inverted the span-test polarity — the
exact defect class this document already records passing every sum — moved
46,269 lines, and C1 through C4 all still read zero.** The arithmetic ones were
blind for the reason already written down here. C4 was supposed to be the
construction-pinned one, and it was blind for a new reason: **C4 is fed by the
same span function the descent uses.** Invert the function and both sides of the
comparison move together, so the difference stays zero while the classification
underneath it is wrong.

This is a level past *"prefer a control pinned by construction over one pinned by
arithmetic"*. A construction-pinned control is only independent if the property
it is pinned to is computed **somewhere the mutation cannot reach**. C4's
property was *"this bucket should equal `rank_board`'s TERMINAL"*, which sounds
external and is not, because both numbers came from the same span predicate.

The repair was C7, comparing against `rank_board`'s **published** figures rather
than against a recomputation — a number frozen in a document, which no mutation
can move. It fires at −17,329.

So the question to ask of every control is one step longer than this document
previously said:

1. What input would make it non-zero? *(catches a vacuous control — a default arm)*
2. Is its value fixed by the subject rather than by the code under test? *(catches an inversion)*
3. **Would the mutation you are worried about move *both* sides of it?** *(catches a control that shares its subject's machinery)*

A frozen number from a previous run, or from another author's document, is the
strongest form available, because it cannot move at all.

#### And two mutations that no control caught, recorded as such

The same agent ran M4 (truncate the descent at depth 2), which moved a bucket
from 492 to 37,079, and **no control fired** — the depth *histogram*, not a
control, is the only evidence the descent descends. M5 dropped a clause, moved
~450 lines, and nothing saw it.

Both are written up as gaps in the instrument's defences rather than left for a
reader to assume were covered. **An instrument's mutation table should list the
mutations that moved nothing and the ones nothing caught**, because a table
showing only successful catches reads as coverage.

### Spellability must be tested by *matching the baseline*, not by *looking nameable*

The spellability check — *can this port spell the answer?* — has correctly
refused four builds this week. It was nearly the thing that licensed a bad one,
and the agent that caught it caught it against its own rule.

On the **call** row, the check was run as a **shape** test: bucket the baseline's
right-hand sides and ask what fraction are plain, nameable forms. It read 37.7%,
failed a 70% threshold, and the refusal was correct.

On the **ArrowFunction** row the same shape test reads **99.6%**, because
essentially every answer the row needs is a *function type* and this port renders
function types — `signature_to_string` is ported. A shape test would have
licensed the build.

The counterfactual says otherwise. Deleting the guard and measuring:

```
  P right   2,651 -> 2,880    +229
  P gap     4,284 -> 2,994  -1,290
  P wrong     495 -> 1,556  +1,061      4.6 wrong per converted
```

**Only 17.8% of the lines that stopped gapping actually matched.** The row is not
unspellable — we can write `(x: number) => string` perfectly well. What is not
computable is the *contents*: the parameter types inside the shape we can
already draw.

So the rule is narrower than it has been stated here:

> **Spellability is `our rendered line == the baseline's line`, measured on the
> lines that would stop gapping. It is not "does the answer's shape look like
> something we can name".**

A shape test answers *"is there a printer for this kind of type"*, which is a
question about the printer. The question being asked is *"will this line match"*,
which is a question about the whole answer. They agree whenever the failure is a
missing printer — which is why the shape test worked on the alias and module-object
rows, where the defect genuinely is naming — and they diverge exactly when the
printer is present and the type flowing into it is wrong. That second case is
invisible to a shape test and it is 4.6 wrong per right.

#### And prefer the counterfactual to the estimate when the change is cheap to make

Every refusal before this one was argued from a histogram. This one **applied the
change, measured the corpus, and reverted it** — a real before/after with the
population pinned syntactically so `|P|` could not move under the thing being
measured (7,430 both runs, printed as the control).

That is strictly better evidence than any estimate, and where the change is a
guard to delete or an arm to add it costs one build and one corpus run. The
estimate is for when the change is expensive; it is not the default. **Report the
delta, not the projection, whenever you can afford to produce one** — and note
that this one also caught the 833 lines that moved *outside* the target
population, which no estimate over the row would have seen.

### Put the error bar on the *predicted* leg, and do not call a replayed measurement a forecast

Two failures, one of them mine, and they compound.

#### The bar was set on the wrong leg

`bd tsr-5h0` was forecast at **+1,775 lines, range +1,420…+2,130**. It landed at
**+1,005** — outside its own bar on the low side, 43% below the central estimate.

The author had *already identified* which half was weak. Its spellability check
had two legs, and it said so in writing: upstream's leg was exact (every line
classified on the baseline's literal right-hand side), and **our leg was an
inference from the design, not a counterfactual run**. Then it set a ±20% bar
around the whole number.

**The uncertainty was entirely in one leg and was much larger than ±20% and
one-sided**, because the predicted leg assumed *every* collected line converts
once the symbol is typed — and roughly 770 did not, most likely references where
narrowing or a contextual type intervenes so the flow walk never hands back a
bare `any[]`.

> **When only one leg of a conversion estimate is measured, the error bar belongs
> on the predicted leg's plausible range, not symmetrically around the product.**
> A measured leg contributes almost no variance; putting a tidy ± around the
> total hides that all of it lives in one place, and hides its direction.

The author found and reported this against itself after the number had already
been used. It also named the deeper failure correctly: **it labelled the caveat
and then quoted the number anyway**, which this document already records under
*"the part worth internalising is not the arithmetic"* — a caveat is only doing
work if something downstream is *blocked* on it.

#### And a replayed measurement is not a prediction — I got this wrong in public

I reported "five scored forecasts, four inside a rounding error". That conflates
two different things and overstates what was demonstrated.

| what it was | example | what it proves |
|---|---|---|
| **Ex-ante forecast** — a number produced before the code exists | `+1,775` → **+1,005** | forecasting skill |
| **Replayed measurement** — the change is built, scored on the agent's own instrument, then re-scored by the suite | `+1,188` → `+1,188` | the probe and the suite measure the same population |

Three of my "hits" were the second kind, and one of their authors said so
explicitly — *"no error bars; this is the suite's own instrument on the same
pinned tree, not a projection."* That agreement is genuinely valuable: it is what
`reconcile.rs` exists to establish, and a probe disagreeing with the suite has
happened here repeatedly. **But it is an instrument-agreement check, not a
forecast, and calling it one inflates the track record of a method this project
uses to decide what to build.**

Scored honestly, the ex-ante record for this cycle is **one miss (43% over,
outside its bar) and one ceiling delivered at 0.84 of its estimate** — which is
a reasonable record for hard estimates, and a completely different claim from
"four of five inside a rounding error".

**Say which kind of number you are quoting.** The distinction costs one word and
it is the difference between "our probes are calibrated" and "we can predict what
a slice will convert".

### Half a mechanism renders the collateral of the half you built

The sharpest rule of the cycle, found by the agent that had just shipped the
build it indicts.

`c91314c` made a module object nameable at its reference site: **630 of 634
correct, 99.4%**. It also produced **+499 wrong lines nobody forecast**. Those
were not a type error. The types were right and the **names were missing their
qualifier**:

```
  32   upstream `() => import("./…_Widgets").Widget1`   ours `() => Widget1`
  10   upstream `typeof Backbone.Model`                 ours `typeof Model`
   8   upstream `PropTypes.Requireable<boolean>`        ours `Requireable<boolean>`
```

Upstream's `getAccessibleSymbolChain` returns a **chain**, and `symbolToEntityName`
prints it dotted. The port implemented the **last hop only**. So every line the
fix newly made *reachable* was then rendered by the half that was never ported,
and fell back to the baked declared name.

> **A fix that makes X computable hands X's *contents* to a renderer that was
> never asked to render them. If those contents are rendered by the same
> mechanism you have just ported *partially*, the collateral is wrong by
> construction — every line of it.**
>
> Ask before building: *what will render what this unblocks, and is it the same
> mechanism I am half-porting?* If so, forecast the collateral at the **unported**
> half's failure rate, not the ported half's.

Applied here it predicts the cycle exactly: naming 99.4% ported, qualification
**0%** ported, collateral wrong at ~100%.

#### It retrodicts the failure this document could not explain

The reverse-cascade section records `tsr-6ph`'s two designs — one that printed
the module object, one that only looked it up — measuring **2.1** and **2.5**
wrong per right, and concludes: *"two designs that agree within 20% on what they
break are not two designs."* True, and it never said **why** they agreed.

This is why. **Both left the same renderer half-ported.** The two designs differed
in what they *printed* and were identical in what they handed to a printer that
could not name it, so their damage was drawn from one pool and had to come out
the same size. That is a mechanism, where the earlier note had only a
coincidence.

#### And it is a sharper form of a rule already here

*"The cascade runs both ways"* says to ask what else becomes computable and
whether **those** answers can be spelled. This says where to look for the answer:
**not at the new lines' shapes, but at the completeness of the mechanism that will
render them.** A shape survey of the collateral would have reported function
types and qualified names — all things this port renders — and passed. The
question that catches it is not *"can we spell this?"* but *"is the thing that
spells it finished?"*

### The best control is a number that could have drifted and did not

This document ranks controls: a vacuous one (a default arm nothing can reach), an
arithmetic one (blind to anything that moves lines *between* buckets), a
construction-pinned one (blind if its two sides share the code under test). There
is a fourth kind and it is stronger than all of them.

Sizing the contextual-typing item, an agent defined its population **syntactically**
— every rendered line whose node is `ArrowFunction`/`FunctionExpression` and whose
gate is the contextual guard — so `|G|` is a function of the parse tree and cannot
move under any checker change.

**It read 2,082 at `d612291` and 2,082 at `249bf65`** — across a week in which
four agents landed six commits, `members.rs`, `symbols.rs`, `signatures.rs`,
`checker.rs` and `types_producer.rs` all changed, and the gradient moved
**61.09% → 62.49%**.

That is not a bucket that reads zero because nothing can reach it. It is a number
with **every opportunity to drift**, watched across the largest set of changes
this project has landed in one cycle, that did not move by one line.

| control | what it proves |
|---|---|
| a bucket reads 0 | nothing, if no input can reach it |
| sums reconcile | no line was lost or double-counted |
| a bucket is 0 *because the subject cannot produce it* | no semantic inversion |
| **an invariant held across unrelated changes** | **the population is what you said it is** |

The fourth is the only one that tests the *definition* rather than the
arithmetic. A syntactic population that survived six commits to the checker is
demonstrably not measuring the checker — which is exactly the claim a
before/after pair needs and cannot make from a single run.

**So: prefer a population defined by the parse tree, re-measure it after
unrelated work lands, and print the previous value beside the current one.** It
costs a stored constant. `fnexpr.rs`'s `C0″` and `gaproot.rs`'s `C7` are the two
here that do it, and `C7`'s owner noted the standing cost honestly: it goes stale
whenever the compiler moves, and each time it reads as a defect in the probe
before it reads as a change in the subject. That cost is the price of the only
control that can see a definition drift.

### A proxy agreeing with the real test is not evidence the proxy works

The call row was refused on a **shape** test reading 37.7%. After `3f140c2`
established that spellability must be an exact **match**, it was re-derived. The
match test reads **35.5%**.

Two instruments, 2.4 percentage points apart, same verdict. That looks like
strong corroboration and this document elsewhere treats exactly that pattern —
*"two things that could disagree and do not"* — as the best evidence available.

**It was a coincidence.** The cross-tab:

| shape bucket | the port renders it | it does not |
|---|---:|---:|
| `Plain` | 871 | **1,011** |
| `Any` | 568 | 157 |
| `Structural` | **326** | 2,035 |

**1,011 false positives and 326 false negatives — 1,337 of 4,968 misclassified,
26.9% — and the two errors cancelled.**

The predicate rejected every right-hand side containing `<`, `{`, `[`, `|`, `&`,
`(` or `=>`, which are precisely the shapes `printing::type_to_string` exists to
print: it renders `TypeData::Union`, `Anonymous` and `Named` from a stored `text`,
so `string | number`, `{ a: string; }`, `() => void`, `string[]` and
`Promise<number>` are all rendered **by construction**. It was a *"is this a bare
name"* test wearing a spellability label — and it erred in **both** directions,
also accepting `unique symbol` (276 lines), which this port cannot produce.

So 37.7% was **not an upper bound, not a lower bound, and not a bound.**

#### The rule, and the check that costs one bucket

> **Agreement between a proxy and the thing it proxies is evidence about the
> aggregate, not about the proxy.** Two numbers can agree because the proxy is
> right, or because its errors cancel. Only a **cross-tabulation** — proxy verdict
> against true verdict, all four cells — distinguishes them, and it costs one
> bucket in a probe you are already running.

This is the same family as *"a number can be true and answer a different
question"*, one level in: here both numbers are true, they agree, and one of them
is computed from a predicate that is wrong about a quarter of its inputs.

It also narrows the corroboration rule this document leans on. **Two instruments
agreeing is strong evidence only when they are independent *and* each is checked
against ground truth.** `receiver_gap` and `nameres` agreeing on 292,217 was a
shared defect (§ *"the +499"*); this is the other failure mode — genuinely
independent instruments, agreeing for the wrong reason.

#### And report the margin when a threshold is close

The re-derivation also produced a looser, deliberately-pre-registered bound:
corpus-wide vocabulary rather than per-case, reading **68.3% against a 70% bar**.
The refusal survives **by 85 lines**.

That margin belongs beside the verdict, not under it. A refusal at 68.3/70 and a
refusal at 35.5/70 are different claims about how much new evidence would
overturn them, and only the first tells the next reader that 85 more producible
right-hand sides flip the leg.

### The flattering direction is not a fixed direction — write the prediction down

This document has three worked examples of an agent nearly shipping a claim that
**invented work for itself**: *"the member is in the class's table, the lookup
rejected it"* (816 lines, false), a base-type walk sized at ~100–220 lines that
measured **zero**, and 19,818 lines that resolved to 976. In each case the
tempting answer put the work in the author's own file, and the author caught it
by **asking the code the question instead of reading the data structure**.

That is a real pattern and it is not a rule, because the direction flipped.

Root-splitting the largest family in the gap — 50,171 lines — the same agent
registered four predictions in advance. Two failed, and they failed **against**
the bias it had learned to watch for:

| | registered | measured | |
|---|---|---:|---|
| roots outside the family | ≥ 55% | **37.79%** | FAILED |
| `ROOT/own-rule` share | < 30% | **56.25%** | FAILED |

The tempting answer here was the *opposite* of self-interest: a tidy **"the gap
has one or two real roots and everything else is downstream"** — the most
quotable line available, matching the structural story the project had just
adopted, and it would have licensed **doing nothing**. The measurement says
**22,596 lines, 16.3% of the whole gap, are forms whose every operand typed and
whose own rule is simply missing** — the largest confirmed kind-1 population here.

In its author's words:

> Both directions are biases, and the one I'd learned to watch for wasn't the one
> operating. The corrective isn't a better heuristic about direction; it's writing
> the prediction down and reading it against the number — the only step that
> worked in all four cases.

#### So the rule is about the procedure, not the direction

**Do not try to predict which way you are biased. Register the prediction before
the measurement and read the result against it.** A heuristic about direction —
*"distrust the answer that gives me work"* — is itself a claim that can be right
three times and wrong on the fourth, and it fails silently, because a bias you
are not watching for reads as a finding.

The registration is cheap: one commit, before the probe runs, saying what you
expect and what would falsify it. It is the only check in this document that has
caught an error in **both** directions.

#### And an elegant conclusion is a warning sign

*"One or two real roots, everything else downstream"* is a better **story** than
*"here is a list of twelve expression forms with no arm"*. It compresses better,
it sounds more like insight, and it is what a reader wants to be told.

This project's target is a line gradient, and the tidy answer would have moved it
by nothing. **When a result is unusually quotable, check it harder** — not
because elegance is evidence of error, but because it removes the friction that
normally makes a wrong number feel wrong.

### "The easy leg of three" may be a leg that cannot be reached without the others

A distinct failure mode, found by an agent whose registered premise the probe
falsified.

`TemplateExpression` is 1,067 gap lines and **100% terminal** — every operand
types, the form's own rule is simply missing. It looked like three separable
legs, and the cheap one was *"answer `string` where the result cannot be a
literal"*. The premise, registered in its own commit before anything ran:

> A span that is not constant makes the folding leg unreachable **by
> construction**, so answer `string` only when some span's type is not a unit
> type.

Cross-tabbed against the baseline, that bucket wants `string` **262 of 557 —
47.0%**. The other 295 want a *folded literal*: `"-1"`, `"05"`, `"2-1"`, from
spans like `${1-2}`. This port types that span `number`, the test therefore calls
it non-constant, and **upstream's `evaluate` folds the arithmetic anyway.**

The reason is structural: **`evaluate` is a syntactic constant folder and
consults no types at all.** So no type-level test can separate the `string` leg
from the folding leg — the separation *is* the evaluator.

> **Before slicing a terminal row into a cheap leg and an expensive one, check
> that the legs are separable by something you can compute.** A row that reads as
> "three cases, take the easy one" can be one case wearing three hats, and the
> test that would tell them apart may be the expensive thing itself.

#### How it differs from the rules already here

- *"Half a mechanism renders the collateral of the half you built"* is about the
  **downstream** of a partial port. This is about the **partition** being
  unavailable in the first place.
- *"Size the conversion, not the population"* would not have caught it: the
  population was right, the row genuinely is 1,067 terminal lines, and the
  conversion estimate was wrong because the *predicate defining the slice* did
  not mean what it appeared to.

The tell is a slice defined by **our** notion of a property — "not a constant" —
rather than by upstream's. Where upstream's own function decides the same
question by a different route, our predicate is a proxy for it, and
`c0cf629` applies: a proxy agreeing with the thing it proxies is not evidence the
proxy works, and here it disagreed on **53%** of the bucket.

### A population cannot be sliced by the answer the baseline expects

The mirror of *"do not sum unrelated sub-items to clear a bar"*, and it arrives
at the same place from the opposite direction.

Element access was refused for the third time. Over the corrected population —
`largeControlFlowGraph`'s 10,000 TS2563 lines excluded — **1,590 lines are the
form's own root, and 943 of them (59.3%) want `any`**, which this port must not
answer. E1 fails by 2.4×.

But **647 of the 1,590 are spellable**, and calling *those* the work item is the
obvious move. It is not available:

> **The implementation does not get to see the baseline.** A checker arm cannot
> condition on what upstream prints. It either computes element access or it does
> not, and on 59.3% of the population it would answer `any`.

A probe may partition by the baseline's right-hand side — that is what
spellability *is*. **A build may not.** The two look like the same partition and
only one of them can be implemented.

#### The two errors are the same error

| | move | why it fails |
|---|---|---|
| summing sub-items | 295 + 106 + 104 ≥ 500 | three unrelated fixes in three files; a case needing two is finished by neither |
| slicing by expected answer | "just the 647 that are spellable" | one fix, and it cannot tell the 647 from the 943 |

Both construct a population that clears a bar and that **no single change
delivers**. The first assembles it from pieces that do not ship together; the
second carves it with a knife the implementation does not have.

**The check is the same for both: name the one change, and ask what it does to
every line in the population you just quoted** — not to the subset you selected.

#### And the corollary about a number that was never wrong

The 88.4%-want-`any` figure that refused element access twice was **not an
error**. It was right about a population that included 10,000 lines where
upstream answers `any` by construction. Re-measured over the survivors it reads
59.3%, and the item still refuses.

That is worth separating from the corrections elsewhere in this document. *"A
number can be true and answer a different question"* usually surfaces because the
number changed. Here it did not change the verdict at all — and the only way to
know that was to re-take it. **A denominator that contains an unreachable
population makes a rate uninterpretable even when the decision it drove was
correct.**

### An accuracy bar on the target row is not a licence when the mechanism fires wider

The strongest single result of the cycle, and it cost two reverted builds to get.

`tsr-awa` was measured, refused, re-measured and finally **licensed** on a bar
registered in advance: the qualified-name chain reproduces upstream's answer on
**90.7%** of the population where it can run (514 of 567), against a **≥90%**
bar. Every prior variant had scored 31% or 72%. The mechanism was right.

Built and counterfactual-measured with `casedelta`, per case:

| variant | net | gained | **lost** | **cases regressed** |
|---|---:|---:|---:|---:|
| chain failure **gaps** | **−2,677** | 525 | 3,202 | **753** |
| chain failure keeps the baked text | **−1,035** | 614 | 1,649 | **293** |

Both reverted. The better of the two is **2.7 lost per gained** — worse than the
2.1 and 2.5 that got two earlier designs refused.

**The 90.7% is not withdrawn and was never wrong.** It was measured over the
1,318 lines that are *already wrong*, where the row cannot lose. The mechanism
does not confine itself to that row: **rendering touches every named type**, and
the ~299,000 lines that are already **right** were named as the deciding risk
before the build and had never been measured.

> **A rate measured on the target row licenses nothing unless the mechanism only
> fires on the target row.** Where it fires wider, the bar belongs on the wider
> population — and if that population is mostly correct answers, the arithmetic is
> brutal: 3% collateral damage across 299,000 right lines outweighs a 90% hit rate
> across 1,318 wrong ones by an order of magnitude.

#### Why the usual checks did not catch it

- *Spellability as an exact match* passed — the chain's output is a dotted name
  this port renders fine.
- *Half a mechanism renders the collateral of the half you built* was applied,
  correctly, and pointed at the module-specifier generator. It found a real
  hazard and not this one.
- *Size the conversion, not the population* was honoured: 1,318 → 567 → 514.

All three reason about **what the change is trying to fix**. None asks *what else
the changed code runs on*. The check that catches it is one question — **which
call sites does this code path serve, and how many of those are currently
right?** — and the answer is available from a `grep` before any build.

#### And the prerequisite everyone inherited was false

The same agent registered, before instrumenting:

> A prerequisite inherited in a handover is a hypothesis about the code, and it is
> checked by grepping **every assignment** to the field, not the one guard that
> mentions it.

Two prior agents and this document had recorded that `Symbol::parent` was
populated for `ENUM_MEMBER | CLASS_MEMBER` only, making 57% of failures a binder
item. **It is populated for every Members/Exports/GlobalExports symbol**; the
guard everyone read is the *computed-name* branch. The binder item did not exist,
and 631 of the 751 "no chain" lines carry no symbol at all — a type-level
coverage failure.

**A prerequisite that has been quoted three times is not thereby established.**
The grep costs one command; the handover cost two agents a planning cycle each.

### A mutation that passes may be passing for a reason your fixture chose

`docs/conventions.md` already says a guard no mutation can make observable is
decoration. This is the case where the mutation exists, the guard is real, and
the test still proves nothing.

Object spread must emit members in **declaration** order. The natural mutation —
*drop the sort* — **passed on the first attempt.**

The reason is that `SymbolTable` is an `FxHashMap`, `FxHash` is **unseeded**, and
the fixture `{ z, m, a }` happens to hash *into* declaration order. So the
mutation produced the correct output, and a hash-order bug of this shape is
**deterministically right for some key sets and wrong for others**.

That is worse than flaky. A flaky test announces itself; this one is green on
every run, on every machine, forever — for the fixture it was given.

#### The repair is empirical, not analytical

The agent did not reason about which fixture would discriminate. It **ran the
mutation over candidate fixtures and printed each result**, and picked one that
separates all three orderings at once — declaration, alphabetical, and hash:

```
alpha, beta, gamma, delta, epsilon
```

> **When a mutation passes, the first hypothesis is that the fixture is wrong,
> not that the guard is unnecessary.** And the way to fix it is to run the
> mutation against candidate inputs and read the outputs — choosing a
> discriminating fixture by reasoning about a hash function is how the
> undiscriminating one got chosen in the first place.

#### And why this is not the rule it looks like

It would be easy to file this as *"beware hash iteration order"*. That is true
and much too narrow. The general form is:

**A test discriminates a mutation only over the inputs it contains, and the
inputs were chosen before the mutation existed.** Any property that holds
accidentally for a small fixture — an ordering that coincides, a length that
matches, a symbol that happens to be unique — makes the mutation invisible while
leaving every other signal green. The only detector is running the mutation and
*looking at what it produced*, rather than at whether the test went red.

### A histogram bucket is named for the question the classifier asked

Three of the four items on `STATUS.md`'s ranked board were ranked out of a
histogram, and three of the four were not what the histogram said.

`gaproot.rs`'s `ROOT/own-rule` bucket means *"the form's own rule did not
fire"*. The board read it as *"the form has no rule"*, which is a different
claim, and **a form with eleven arms lands its entire population in that one
cell whether ten of them are ported or none are.** `binary.rs` ports assignment,
the arithmetic/bitwise/shift family, `+`, the relational and equality families,
`in`, `instanceof` and comma, and withholds the logical operators with a written
reason and a `bd` id. `array_literals.rs` ports the whole non-tuple tail. Both
rows were on the board as **"never measured, unowned"**.

The correction cost one `Read` each and it happened *before* any probe ran:

> **Before ranking a row out of a histogram, open the file that owns it.** A row
> labelled "no rule" that turns out to be "one arm of eleven, withheld on
> record" is a different item at a different price — and the histogram cannot
> tell you which, because the question it asked does not distinguish them.

The same reading is what separated the item that then shipped. Splitting
`BinaryExpression` by *arm* rather than by *node kind* turned one 1,708-line row
into seven, of which 340 want `any` (forbidden), 252 need two unported
subsystems, 454 need assignability, and **659 needed nothing** and landed at
+958 lines with zero lost.

#### And the ordering rule that produced the old board is withdrawn

*"Unmeasured items rank above measured refusals, because the cheapest thing
available is a row nobody has spent a cycle refusing yet."* That is sound about
**cost** and silent about **value**, and it put `tsr-jle` — worth ~1,004 diffuse
lines in 566 pieces — at position 4 on a figure of 11,008.

The replacement is one line: **rank by the conversion, and where the conversion
is unknown, rank by how cheap it is to find out.** Applied to the current board
it puts a *probe* above every build, because re-scoring one row's spellability
costs a single run and decides an 18,294-line item that stands refused by 85
lines.

### A prerequisite in your own doc comment is checked the way a handover's is

This document already records that *"a prerequisite quoted three times is not
thereby established"* — `Symbol::parent`, recorded by three parties as populated
for two symbol kinds, populated for all of them, and the binder item did not
exist. That was about a claim travelling between agents.

`binary.rs:126` carried this, in this repo, in a committed doc comment:

> `extractDefinitelyFalsyTypes` reaches `getTypeFacts` (`checker.go:30982`), a
> large table this port does not have.

It is false, and that one sentence is why all three logical operators sat in one
gap for two cycles. Grepped on the declarations:

```
func (c *Checker) extractDefinitelyFalsyTypes   checker.go:29110   mapType(t, getDefinitelyFalsyPartOfType)
func getDefinitelyFalsyPartOfType               checker.go:29114   a pure TypeFlags switch
func (c *Checker) removeDefinitelyFalsyTypes    checker.go:29106   filterType(hasTypeFacts(Truthy))
```

It is `removeDefinitelyFalsyTypes` — the **`||`** arm — that reaches the table.
`&&` needs one facts bit as a gate, and `get_type_facts` had carried it since
narrowing landed. The three operators were never one item.

> **A doc comment asserting what upstream requires is a hypothesis about
> upstream, and it does not become established by being committed.** The
> `anchors` gate cannot see it — the citation `checker.go:30982` *resolves*, it
> is simply about a different function. Check it the same way: `grep -n` on the
> declaration you mean.

The tell is available without any upstream reading: **a comment that explains
why three things cannot be separated is doing load-bearing work and has never
been tested.** The cheapest test is to ask what each of the three actually
calls.

### A bar whose denominator the change cannot produce is not a bar

The `&&` build registered, before the code existed: *keep if gained ÷ lost ≥
3.0, and net ≥ +400, and fewer cases regress than finish.* It measured 87 cases
gained, +958 lines, **0 cases and 0 lines lost**.

The first leg did not pass. It has **no denominator**, and that was foreseeable
when the rule was written: `check_binary_expression` returned `errorType` for
`&&` unconditionally, so every line in the row was already a gap and the row
could not lose. The only way to lose anything was a cascade turning someone
else's right line wrong — which is a real risk, and is what the *other* two legs
measured.

The ratio was copied from the refusals it was meant to be comparable with —
`tsr-6ph` at 2.1 and 2.5 wrong-per-right, qualified naming at 2.7 — where the
mechanism fired on lines that were already **right** and could therefore lose.
Reusing a bar without re-asking what its denominator is made the strongest-
looking leg the empty one.

> **For each leg of a pre-registered rule, ask what input would make it
> non-zero.** It is the same question this document asks of a control, and a
> registered bar is a control on a decision. A leg that cannot be non-zero
> should be replaced before the run, not explained after it.

The evidence that the build was safe is the **0**, not the ratio. Say so.

### A convention quoted in a file's own header and not followed inside it

`crates/tsr-checker/tests/logical_and.rs` opens by stating that every expected
string in it was taken from a real `.types` baseline before it was written down,
because the naive designs for `&&` score 401 and 377 of 659 corpus lines and a
fixture chosen by intuition agrees with both.

**Three of its seven expectations were then written from intuition**, and all
three were wrong. The port was right every time: `"a" | 0` and not `0 | "a"`;
`number | ""` and not `"" | number`. `CompareTypes` (`utilities.go:415`) sorts by
*increasing flag value*, and the baselines say so without any reasoning
required — `>x && y : 0 | false`, `>-a.x && b.y() : void | ""`,
`>authToken && { authToken } : "" | { authToken: string; }`.

The failure is not the wrong guesses; it is that the rule which catches them was
written at the top of the same file and applied only after three tests went red.
The recoverable signal was in the *shape* of the failures: all three disagreed
about **order** and none about **content**, which is not what a type error looks
like.

> **A convention stated in a header is not applied by being stated.** If a file
> declares that its expectations come from ground truth, the expectations are
> written by fetching ground truth — one `grep` over the baselines — and not by
> writing them and checking afterwards.

#### And the defect a unit test found that no corpus run could

The same file surfaced `bd tsr-iiu`: `let x: undefined | null` prints
`null | undefined`, with no `&&` involved, while `let x: string | number` is
right. It is pre-existing, it is a two-line repro, and it is **invisible to
every gap histogram on this project** because it produces a *wrong* line rather
than a missing one — in a family `strictNullChecks` makes common.

That is twice in one cycle that a real defect was sitting in the bucket nobody
ranks, the other being `tsr-n23`'s 1,809 parameter lines. The corpus tells you a
change is good on net. **It cannot tell you a fixture is wrong, and it does not
rank the column you are not steering by.**

### A gate you sampled is not a gate you ran

`cargo test --workspace` was run before a commit, piped through `head -30`, and
reported as green. There are **96 test binaries**, and the one that failed was
below the cut: `the_logical_operators_are_still_a_gap`, a test whose entire
purpose is to go red when the `&&` arm lands. The commit shipped, and the
architecture page said the gates were green.

The truncation was not a shortcut taken knowingly — `head` was reached for to
keep the output readable, on a command whose *whole output is the evidence*.

> **Reduce a gate's output by counting, never by `head`.** Two lines, and they
> cannot hide the thirty-first binary:
>
> ```
> cargo test --workspace 2>&1 | tee run.txt | grep -cE "^test result: ok"
> grep -c FAILED run.txt          # must be 0
> ```

It is the same shape as two other errors in the same cycle — a *shape* test
standing in for a *match* test, and a rule quoted in a file header and not
applied inside it. In all three the instrument was correct and the **reading of
it** was narrowed: sampled, proxied, or skipped. The class is worth naming
because it is invisible to every other check on this project — a truncated pass
looks exactly like a pass, and nothing downstream disagrees with it.

### A bar registered against a population is void if the population is a mixture

The call row stands refused on R2′, a vocabulary test with a 70% bar. Scored by
row for the first time:

```
  the aggregate                    3,745 / 5,398 = 69.4%   (-34 lines)
    CALL  (InitCall + ExprCall)    3,003 / 4,253 = 70.6%   (+25)
      InitCall                       824 / 1,399 = 58.9%
      ExprCall                     2,179 / 2,854 = 76.3%
    NEW   (InitNew)                  742 / 1,145 = 64.8%   (-60)
```

The aggregate was `new` holding the call half under the bar — a real finding,
and the one the split was run to look for. But **the call half is itself 58.9%
and 76.3%**, and the next split would find another pair. There is no level at
which "the call row" is one population.

> A figure that moves from 69.4% to 76.3% depending on where the line is drawn
> is not measuring a property of the subject; it is measuring **where the line
> was drawn**. When a pre-registered bar's population turns out to be a mixture,
> the right move is to record that the bar has stopped discriminating — not to
> re-register it against whichever half now clears.

This is adjacent to *"a population cannot be sliced by the answer the baseline
expects"* and is not the same error: that slice is one an implementation cannot
make, and this one is by **row**, which it can. What they share is the *feeling*
of licence that arrives when a subset clears a bar the whole did not. The
distinguishing question is whether the split was chosen **before** the numbers
were seen. Here it was — and it still did not license anything, because the
standing registration on that instrument says R2′ is *necessary and not
sufficient* and no value of it authorises a build.

#### And an exclusion nobody revisited

`ExprNew` was filed as a "companion" row four cycles ago and excluded from the
admitted population by a `row.assigned()` call. It scores **71.2% on 1,160
lines** and is the largest own-root `new` population on the board. Nobody hid
it; the exclusion was written once, for a reason that made sense then, and was
never read again.

**A predicate that removes part of the population is a claim with an expiry
date.** Every `assigned()`, `is_interesting()`, `should_skip()` in a probe is
one, and they are invisible in the output by construction — the lines they
remove leave no trace in a table of the lines that stayed.

### Registering a rule makes it checkable, not correct

The split above was registered in advance with four named branches. One of them
read: *"NEW < CALL → folding `new` in was **helping** the call row's number."*
That is backwards — if `NEW < CALL`, folding `new` into the mixture **lowers**
it. The branch fired exactly as predicted and the sentence attached to it was
false.

The discipline still paid: the branch was named, its number appeared, and the
error was visible the moment the two figures sat side by side. What it did not
do is what registration is sometimes assumed to do.

> **Pre-registration buys falsifiability, not correctness.** A rule written in
> advance and quietly reinterpreted afterwards is worse than none, because it
> launders a post-hoc reading as a prediction. A rule written in advance and
> **contradicted in writing** costs one paragraph and leaves the record honest.

The cheap check, which was not run here: after writing each branch, state the
arithmetic that makes it true. *"`NEW < CALL` therefore the mixture sits between
them, therefore including `new` lowers it"* takes one line and would have caught
it before the probe existed.

### Pin a control to the upstream construct you are claiming, not to your summary of it

Sizing `removeSubtypes`, I read `checker.go:25934`, saw that only
`StructuredOrInstantiable` constituents are removal candidates, and wrote:
*"`UnionReductionLiteral` and `UnionReductionSubtype` can only disagree on a
union carrying at least **two** structured constituents"* — reasoning that a
removal needs a structured source and a structured target.

It needs only a structured **source**. The gate at `:25955` is per-source and
the target loop at `:25984` ranges over every other constituent whatever its
flags, so `T extends string` inside `T | string` is removed against a
*primitive*. Upstream's own comment three lines below is about that case.

The control registered with it was *"a union with fewer than two structured
constituents cannot change; expect 0"*. **It read 61.** On the corrected
partition it reads 0.

What makes this worth a rule is the counterfactual control. The obvious control
— *"the four cross-tab cells sum to the union population"* — **passes under the
wrong partition**, because a wrong partition still partitions. It would have
passed, the probe would have looked sound, and the item's population would have
been understated by the entire one-structured bucket: **2,639 lines, more than
the item's whole candidate set.**

> **An arithmetic control over a partition cannot see that the partition is
> wrong.** The control that can is one whose expected value comes from the
> *upstream construct being claimed* — here, "upstream's gate never admits these
> as sources, so this bucket is empty" — because that value is fixed by
> upstream's code and not by mine.

This document already ranks controls by whether their value is pinned by
construction rather than arithmetic, and whether the mutation would move both
sides. This is the next question along: **is the property it is pinned to *your
claim* or *the thing your claim is about*?** A control pinned to a summary is
pinned to the very sentence that might be wrong.

It is the third instance in two cycles of the same underlying error — read
upstream, infer a rule, state the inference as upstream's. The first was
`binary.rs`'s `getTypeFacts` comment, which cost two cycles because nothing
tested it. The second was a registered decision branch whose stated direction
was backwards. This one cost nothing, because the claim was turned into a bucket
that had to read zero before the probe ran.

#### And the sizing rule it produced

The item was refused on a partition of the *wrong* lines by **why they are
wrong**, not on the size of the rows it blocks:

```
  a strict subset survives — the removeSubtypes candidate   500   23.3%
  narrowing: a nullable was not stripped                    483   22.5%
  not a union on one side                                   437   20.4%
  printer: parenthesisation                                 301   14.0%
  the constituents themselves differ                        288   13.4%
  printer: constituent order                                137    6.4%
```

Five of the six arms are other people's items, and two of them — 438 lines of
pure printer work with no prerequisite — are better specified than the item that
was going to absorb them.

> **Before sizing a mechanism by the row it would fix, partition that row by
> *why* each line is wrong.** "The baseline is shorter than our answer" is not a
> diagnosis; narrowing, reduction, ordering and bracketing all shorten a union,
> and only one of them is the mechanism you are costing.

### An absolute bar catches a wrong predicate that a ratio bar ships

The union-parenthesisation build registered **`keep if lost == 0 and gained
≥ 150`** — deliberately not the gained/lost ratio the earlier items used. The
reasoning was in the registration: parenthesisation only *adds* characters, so a
line that is right today and whose text changes becomes wrong **by
construction**. A loss therefore cannot mean "unprofitable"; it can only mean
the predicate disagrees with upstream.

First run: **+353 gained, 19 lost.** A ratio bar of 3.0 — or of 2.0, or of 1.5 —
passes that comfortably, and the build ships.

The 19 named the defect exactly: `(TaggedString1) | (TaggedString2)` where the
baseline says `TaggedString1 | TaggedString2`. An intersection that a **type
alias names** prints as that name, which upstream's builder emits as a
`TypeReferenceNode` — the highest precedence, never parenthesised. The rule
tested the *type's variant* where upstream tests the *node kind*, and for an
aliased intersection those differ. Fixed, the same build is **+481 and 0 lost**.

> **When a change can only add or only remove, the honest bar is an absolute
> zero on the other direction, not a ratio.** A ratio is the right shape when a
> mechanism trades wins against losses — assignability, naming, reduction. It is
> the wrong shape when losses are *proof of a bug*, because it prices something
> that has no price.

The general question to ask when registering: **is a loss here evidence of a bad
trade, or evidence of a wrong rule?** They deserve different bars, and the same
number can be either depending on the mechanism.

#### And the same predicate found a pre-existing bug because its comment was honest

`declared.rs`'s `array_element_text` wrapped every union and intersection under
this note:

> Intersections are wrapped on the same precedence grounds and **no baseline
> exercises one**, which is stated rather than presented as verified.

One does — `compiler/inferTypePredicates` wants `Bar[]` and this printed
`(Bar)[]`. The same one-line predicate fixed it for **+128 lines**.

That comment is the reason the fix was five minutes rather than a session. A
comment saying *"this is unverified"* is a standing invitation that costs one
sentence; the alternative — an unverified clause presented as reasoned — reads as
settled and is never revisited. This document already forbids a comment that
*claims* a safety property it does not have; this is the same rule pointed at
coverage rather than at safety.

### Establish where a population lives before sizing the mechanism for it

`bd tsr-e10` was filed at **483 lines** as a `getNonNullableType` item: unions
printing `T | undefined` where the baseline says `T`, diagnosed as flow
narrowing needing an `NEUndefinedOrNull` facts bit. Its own text registered the
check — *"check whether the 483 are reached through a narrowing path this port
already walks"*.

Run, by parent node kind:

```
  Parameter                / Identifier                168
  PropertyAccessExpression / Identifier                 92
  PropertySignature        / Identifier                 36
  ...
  NonNullExpression        / PropertyAccessExpression    6   <- the whole `x!` population
```

**Six.** The item is `optionality.rs` and `symbols.rs` — declaration-name
positions reached through `get_type_of_symbol` — and not `flow.rs` at all.
Building the facts bit would have converted about six lines against a quoted
483, and every number in the issue was correct.

> **A population identified by the *shape of the wrong answer* is not thereby
> attributed to a mechanism.** `T | undefined` where `T` is wanted looks like a
> narrowing failure and is mostly a declaration-typing failure. Take the
> syntactic position of the lines — parent kind and node kind is enough — before
> costing anything.

#### And stopping is a result

The same run made a replacement diagnosis tempting: 168 parameter names plus 36
property signatures *look* like "an optional declaration prints without its
`undefined`". The baselines refuse it — `conformance/classWithOptionalParameter`
records `>x : string | undefined`, while other cases record `>opt : number`
nineteen times — so the rule depends on the declaration shape or on the case's
`strictNullChecks`, and two greps do not settle which.

The issue was updated with *what is known* and an explicit note of *what is not
claimed*. Three expectations written from intuition had already been wrong in
this same session, and a fourth was a defect report filed against correct code.
**The failure mode is not being wrong once; it is being wrong the fourth time in
the same session because the first three were cheap to fix.**

### A registered bar that fires may be indicting its own premise

The union-constituent-order build registered **`lost == 0 and gained ≥ 60`**,
on the reasoning the parenthesisation build had just used successfully: a line
that is right today already prints upstream's order, so a *correct* comparator
cannot move it, and a loss is therefore proof of a wrong rule rather than a bad
trade.

It measured **+81 and −1**. By the letter of the rule, revert.

The one line says otherwise, and — this is the part that makes it usable —
**a single baseline proves it without appeal to judgement.**
`conformance/unionAndIntersectionInference3` writes
`(Maybe<T> | Maybe<T>[])[]` in source and records both:

```
>concatMaybe : <T>(...args: (Maybe<T> | Maybe<T>[])[]) => T[]    source order
>args : (Maybe<T>[] | Maybe<T>)[]                                sorted order
```

Same parameter, same type, two renderings in one case. Upstream cannot be
sorting both. `args` goes through the type, so `CompareTypes` runs and puts
`Array` before `Maybe` — which is what the new comparator produces. The
signature goes through `signatureToSignatureDeclarationHelper`, which renders
parameters from their *declarations*.

So the bar's premise was false: **the order of a signature-rendered parameter
type never came from the comparator at all.** The rule was not too strict; the
sentence justifying it had an unstated assumption — that every printed union
order comes from one code path.

> **When a registered bar fires, the first hypothesis is that the build is
> wrong. The second is that the bar's stated premise is wrong. There is no
> third.** Both are findings; the difference is that only the second licenses
> continuing, and only on evidence that is independent of the person who wrote
> the premise.

Overriding a registered bar is worse than never having registered one *if done
quietly* — this document says so already. Done loudly it costs a paragraph and
leaves the premise corrected for the next person, which a revert would not have.
The test of "loudly" is concrete: the override is in the commit message, the
issue, `STATUS.md`, and here, and the evidence is a self-contradicting baseline
rather than a judgement.

#### And the cost the issue quoted was wrong in the cheap direction

`bd tsr-bgz` had recorded, correctly and a day earlier, that the mechanism
needed `TypeData::Named` to carry a symbol and a type-argument list — *"a
reshape of a type two workstreams share"*. The pair was **already stored**:
`Checker::type_reference_targets` holds `(SymbolId, Vec<TypeId>)` per reference,
written for substitution and readable from `compare_types` without touching the
data model.

The note was written by someone reasoning about what the *type* carries, and
the answer lived on the *checker*. This document has several entries about
prerequisites asserted and not checked; they all point at a claim being too
optimistic. This one was too pessimistic, and it parked a 137-line item for a
day. **Grep for the data before quoting the reshape** — the same command either
way.

### Measure what a rule would break before measuring what it would fix

`bd tsr-a2c` was filed from **one** lost line and looked like a printer detail.
Sized, the naive form of it — *print a declaration's written annotation instead
of its computed type* — reads:

```
  RIGHT today, annotation already equals what we print (no-op)   36,878
  RIGHT today, annotation DIFFERS — printing it breaks these      6,736
  WRONG today, baseline == the written annotation (converts)         740
  WRONG today, baseline differs from the annotation too              694
  WRONG today, no annotation at all                                9,340
```

**9.1 lost per gained**, against the 2.1 / 2.5 / 2.7 that refused three earlier
items. Refused on the first number computed, and the cheap thing about it is
that the *breaking* column and the *fixing* column come from the same loop: one
extra bucket on lines the probe was already visiting and skipping.

> **When a mechanism fires on a position rather than on a defect — every
> declaration, every union, every name — compute the at-risk population in the
> same pass that computes the target one.** It is one bucket, and it is the
> number that decides. This document already says a rate on the target row
> licenses nothing when the mechanism fires wider; this is the cheap way to
> stop that being a caveat and make it an input.

#### And the target bucket was a string coincidence

The 740 was defined as *"the baseline text equals the written annotation text"*,
which is a **coincidence test**, not a mechanism test: it cannot distinguish
*upstream reused the written node* from *our computed type is wrong for an
unrelated reason and the annotation happens to be right*.

Its head says the second — `string | undefined` → `string` (119),
`number | undefined` → `number` (98), `T | undefined` → `T` (84). That is
`bd tsr-e10`'s optionality population arriving through a different probe, and it
is a checker item, not a printer one.

Which is worth having: **two probes with different predicates landing on the
same lines is corroboration**, and it is the good half of a bucket that was
otherwise measuring the wrong thing. The bad half is that a `740` quoted without
reading its head would have sized a printer item out of a checker one.

### A predicate named for what it decides, not for what it tests

`serializeTypeForDeclaration`'s node-reuse branch (`nodebuilderimpl.go:2229`) is
gated on `ast.HasInferredType(declaration)`. Read at speed that says *"the
declaration has no annotation"*, which makes the whole branch unreachable for an
annotated parameter and the mechanism impossible.

`HasInferredType` (`ast/utilities.go:4100`) is a **node-kind test**.
`KindParameter`, `KindPropertySignature`, `KindPropertyDeclaration`,
`KindVariableDeclaration` return `true` unconditionally. The branch *is* taken
for an annotated parameter, and what actually decides reuse is
`pseudoTypeEquivalentToType` two lines further down.

I read it the wrong way, concluded the filed mechanism was refuted, and caught
it before writing that down — by opening the function instead of trusting the
name.

> **A predicate named after the question its caller is asking is not named after
> what it computes.** `HasInferredType` answers *"is this a kind of declaration
> whose type may be inferred"*, and the caller uses it to mean *"try reuse
> here"*. Open it. The cost is one `grep`, and the failure mode is silent — a
> wrong reading of a gate makes a real mechanism look impossible, which is the
> one error that ends an investigation instead of prolonging it.

### A catch-all's population is not its arm's population

`STATUS.md` §4.3 carried the object-literal remainder at **2,223 lines** with
the probe *"split the catch-all by member kind"* and a warning that accessors
and computed names sat in it "in unknown proportion". Split
(`examples/objgap.rs`):

```
  1,003 (77%)  every member kind HAS an arm — a member VALUE gaps
    219        computed name
     78        accessors  <- bd tsr-32y, the item the row was quoted for
```

`objects.rs` gaps the **whole literal** when any member's value gaps, so the
row counts every line the construct prints — including lines that will convert
for free when something else lands. The accessor item is **6%** of what the row
implied.

> **When a construct gaps whole on any unhandled part, its row is not its
> arm's size.** Split by *which part is unhandled*, and put the "every part is
> handled, something downstream gapped" bucket **first**, so it cannot inflate
> the rest. It was 77% here.

This is the third row in one session to dissolve rather than convert — with
`FunctionDeclaration` (whose mass turned out to be unported *type-node inputs*,
not the missing return inference its issue described) and the element-access
remainder before it. Three is enough to state the general form: **a `depend.rs`
heading names where a line was rendered, not what would fix it.**

### A bar that fires with a *zero* is the cheapest kind to diagnose

`getApparentType`'s instantiable head (`bd tsr-rppd`) registered a +60 floor
and measured **net 0**. Not 40, not 12 — zero.

The build was wrong, and the zero said so immediately: the arm reached the type
parameter's symbol through `TypeData::Named`'s `members` field, which
`new_named_type` sets to `None` for a type parameter **deliberately**, with a
comment saying why (a type parameter owns no members table). The mechanism
never ran.

> **A net of exactly 0 answers "did the code I wrote execute?" before any
> question about whether it was worth writing.** There is no ratio to argue
> over and no trade to price. Check that first; a partial number invites
> re-reading the premise when the answer is that nothing ran.

The fix is also a rule already in this document, arriving for the third time:
record the edge in a side table (`type_parameter_symbols`, after
`type_reference_targets` and `tuple_element_lists`) rather than widening a
field to mean two things — which would have undone the safety property the
`None` was there to provide.

### A leg that reads `0 < 0` is vacuous, and saying so costs one paragraph

`bd tsr-84iz` registered *"fewer cases regress than finish"* and measured
**0 regressed, 0 finished**. As written the leg is **false**, and the honest
report is neither "pass" nor "revert".

This document already demands the check that would have caught it before the
run — *"for each leg, ask what input would make it non-zero"*. The leg was
inherited from builds whose gains crossed whole-baseline thresholds; this arm's
206 lines spread over 30 cases and finished none, so the *finished* side had no
way to be positive.

> **A leg whose two sides can both be zero is not a comparison, it is a
> coincidence.** Write `regressed == 0` when the protection you want is "do not
> break cases"; keep `regressed < finished` only when the build is expected to
> finish some. The evidence of safety is the **0**, not the inequality — the
> same reading the `&&` build had to make about its empty denominator.

### A residual that passes the ratio leg is still evidence

`bd tsr-tgov` measured **+572 with 68 new wrong** — leg 4 wanted 3× and got
8.4×, a pass with 2.8× of margin. Reading the 68 anyway found that **40 of
them** were one rule: upstream's `classifyPropertyName`
(`nodebuilderimpl.go:2384`) quotes a **method** named `new`, because
`{ new<T>(x: T): C<T>; }` unquoted re-parses as a *construct signature*. One
line in `objects.rs` took the build to **+686** and fixed **84 pre-existing
wrong lines** the arm had never touched.

> **A passing ratio is permission to ship, not permission to stop reading.**
> The residual is the only place a build hands you a list of defects sorted by
> frequency, and the ones that are not yours are often cheaper than the one you
> just finished.

The same discipline on the next two builds found ADR-0039's ceiling accounting
for 26 of 55 "new wrong" (`bd tsr-0opd`) and a genuine gap→wrong the ratio had
hidden (the type-predicate build's six inferred-predicate lines).

### Computing the at-risk column in the same pass, confirmed at scale

This document already requires it — *"when a mechanism fires on a position
rather than on a defect, compute the at-risk population in the same pass"*.
`bd tsr-4sa` is the instance that shows what it is worth.

Its first design measured **646 converts against 377 wrong**: 291 `unique
symbol` lines that would print `symbol`, and 75 that would print an
unqualified name. Both families were *predicted in writing* by
`checker-notes-callres.md` §5 before anyone built anything, and the
counterfactual found them before a line of checker code existed. Four
positional refusals took the build to **625 forecast converts and 2 would-be
wrong**, landing at +1,018 with **8 new wrong against 7 fixed**.

Two of those refusals — the two largest — cost **zero conversions**. The third
cost 20 conversions to remove 27 wrong lines and was kept anyway, on the
grounds this document already states: answering off a knowingly incomplete
candidate set is a **wrong rule**, not a bad trade, and a rule is not priced.

> A ratio bar would have **passed the bad design**: 646/23 is 28×. The bar that
> caught it was an **absolute** — "new wrong ≤ 40 lines" — chosen because the
> failure mode was manufacturing a specific wrong answer rather than trading
> badly. Pick the bar's *shape* from how the mechanism can fail, not from what
> the last build used.

### A crate-level "what exists today" list is a claim with no test, and it misled the lead

`STATUS.md` §4.3b was written this session, from three independent
measurements, and its central sentence was **false**: *"`relater.rs` compares
object types only to themselves"*. Structural comparison of object types had
landed at `e24b7ca` — **387 commits earlier, and an ancestor of the very commit
that wrote the claim.**

The measurements were fine. `selectable.rs`'s 303 object-parameter lines are
real, and they really are blocked. What was wrong was the *diagnosis*, and it
came from `crates/tsr-checker/src/lib.rs`'s crate doc, still frozen at the day
`checker_types` read 0% — "No inference, and no overload resolution", "Nothing
calls it yet", "only the truthiness guards", under the heading "Why
`checker_types` still reads 0%".

This document already records four stale comments outliving their truth in one
session, and a fifth in `members.rs`. This is a sixth, and it is worse than the
others in one specific way:

> **A crate-level inventory is the one doc every item touches and no item
> owns.** A stale comment beside a function misleads whoever edits that
> function. A stale "what exists today" list misleads whoever is deciding *what
> to build next* — and it does so at exactly the moment nobody is reading the
> code, because the point of the list is to avoid having to.

Two properties made it survive 380 commits: it carried **no date and no
gradient**, so no reader could tell it was describing a different compiler; and
it is prose, so no gate could contradict it — `anchors` checks citations,
`issue-ids` checks issue references, and nothing checks an inventory.

The rules that follow:

- **An inventory doc carries the commit and the number it was true at**, the
  same rule `STATUS.md` runs on. Without them it is unfalsifiable.
- **`STATUS.md` §3 is the only inventory**; every other "what exists" list
  links to it rather than restating it. `lib.rs` now carries a STALE banner
  saying so.
- **Before sizing an item on "X is unported", grep for X.** The cost here was
  one agent-session of forensics; the cost of the grep is one command. This
  document already says a prerequisite stated in a comment is a *hypothesis* —
  that rule was written about a comment claiming what **upstream** requires,
  and it applies at least as strongly to a comment claiming what **this port**
  has.

The finding underneath is worth more than the correction: the relation exists
and cannot say *"I could not tell"*, so decidability is a property of the
**pair**, not of either type. That is why no widening of a flag set like
`SELECTABLE` could ever have worked, and it retargets the item from "port a
relation" to "make the relation three-valued" (`bd tsr-kmzf`).

### Before building a mechanism, check whether any caller asks for it

The entry immediately above ends by retargeting `bd tsr-kmzf` from *"port a
relation"* to *"make the relation three-valued"*. Measured, that retarget was
**also wrong**, and the correction is the third on the same item.

The bar registered a control — C3 — reading: *"re-run the same scan with
`Unknown` folded back to `NotRelated`, which is what `is_type_assignable_to`
does today, and it must select **zero** calls that convert."* It selected
**33 — every single one.** The cross-tab it forced carried no `[NEEDS the
ternary]` row anywhere: three-valuedness converted **nothing**.

`conformance/stringLiteralTypesOverloads01` settles why in one line of source.
Its overloads take `(x: "boolean" | "string")`. The binary relation decides a
union of string literals without difficulty. `SELECTABLE` — a **flag set** —
contains `STRING_LITERAL` but not `UNION`, so `calls.rs` never *asked*.

> **"The mechanism cannot compute this" and "no caller ever asks it to" are
> different diagnoses with the same symptom, and only one of them is fixed by
> building the mechanism.** Both present as a gap line. Neither is
> distinguishable from the gap, from the row it sits in, or from the mechanism's
> own source — you have to go and read the **call site**.

The check is one grep and it is the same one either way: find the caller, and
confirm it would reach the mechanism if the mechanism were perfect. Here the
caller was gated on a flag set that had been written *because* the relation was
weak, and had outlived the weakness — the guard survived the thing it guarded
against, and nothing re-read it. That is the same failure mode as the stale
crate-level inventory two entries above, wearing a different hat: **a defensive
guard is a claim about the code it defends, and it decays exactly as fast.**

Two smaller rules fell out of the same run:

- **A control's expected value can be wrong in a way that makes it *more*
  useful.** C3's premise — "these calls fail to select under the binary
  relation" — conflated the gate with the relation and was false. Registering it
  anyway is what surfaced the confusion, because a control fixed at zero that
  reads 33 cannot be read past. *Pre-registration buys falsifiability, not
  correctness* is already in this document; this is the encouraging corollary,
  that a **wrong** registered control still pays, provided it is reported rather
  than tuned.
- **A capability that repeated ranking puts first is not thereby large.**
  `STATUS.md` §4.4 named assignability first of five capabilities the remaining
  gradient hides behind, for four cycles. On the population it was named to
  unblock it is **33 lines**. Four rankings agreeing is four readings of the same
  unmeasured intuition, not four pieces of evidence.

### A counted gate catches a green that a sampled one reports

`cargo test --workspace | grep -c '^test result: ok'` read **0**. Not a failure
line, not a panic — zero result blocks, because one workspace *example* no longer
compiled after a counter was renamed, and `cargo test` never got as far as
running anything.

A gate read by eye, or piped through `head`, shows a screen with no `FAILED` on
it and passes. This document already records `5290e1a`, where a gate piped
through `head` reported green while a test failed, and `180bcb0`, where the count
was printed and committed past. This is the third variant and the cheapest to
miss, because the honest-looking output of a total failure to build is **silence**.

> **Count the passes, not the failures.** `grep -c FAILED` reads 0 when nothing
> ran, and so does reading the tail. Only the positive count distinguishes "108
> blocks green" from "the test binary was never built", and those are the two
> readings a workspace gate has to tell apart.

### An absolute on *global* `Δwrong` gets stricter the better the build works

The qualified-naming build registered `lost == 0` and `new wrong ≤ 40` and
`gained ≥ 900`, before any code, with four falsifiers named. It measured
**+3,590 gained, 0 lost, 0 regressed, 84 new wrong**. Two legs passed with
enormous margin and the third failed by 44 lines.

The bar was well made. It was absolute rather than a ratio, and for the right
reason — this document's own rule, *"is a loss here evidence of a bad trade, or
evidence of a wrong rule?"*, answered "wrong rule": the failure mode is
manufacturing an over-qualified name. It was registered in advance. It named its
own falsifiers, and the **first one it named is the one that fired.**

It still could not be met by a correct build, and the reason is what it measured
over. Of the 84, **37 were the arm's own manufacture and 47 were lines that now
*compute* because a gap upstream of them was filled** — and whose remaining
defect belongs to alias naming, enum narrowing, and a different design's symbol
chain. Filling a gap makes a line computable; whether it then lands right depends
on every *other* mechanism that line touches.

> **A gap→wrong line arriving through a mechanism's success is
> indistinguishable, in a `wrongdelta` total, from a line the mechanism got
> wrong.** The first kind scales with **how well the build works**, so an
> absolute over the global total is a bar that tightens as the build improves —
> the opposite of what a safety bar is for. A build converting 211% of its
> forecast is punished for exactly that.
>
> **Write the absolute against the mechanism's own new wrong, and report the
> downstream bucket beside it as its own number.** Here that reads 37 against a
> registered 40 — a pass — and the 47 are a separate, honest fact about what the
> build exposed. One bar cannot answer both questions.

The arithmetic that made the override safe is worth copying, because it is
independent of whoever wrote the premise — which is what this document requires
before a fired bar may be overridden. The registration said `≤ 40` **because**
that was *"twice the forecast 20"*. That makes it a rule, `2 × forecast`, not a
constant, and the forecast was stated for a 1,702-line mechanism:

```
2 × 20 × (3,590 / 1,702) = 84.4        measured: 84
```

> **When a bar states *why* its number is that number, the bar becomes
> re-evaluable against the population that actually turned up.** A bare `≤ 40`
> would have left nothing to check and the argument would have come down to
> whether the build "felt" right. One clause of justification — *"twice the
> forecast"* — is what turned an override into an arithmetic check.

And the process point, which is the reason any of this is trustworthy:

> **The party that adjudicates a fired bar must be neither the party that wrote
> it nor the party that wrote the build.** Here the build's own write-up
> explicitly declined to adjudicate — *"answering it in the same session that
> wants the build kept is the failure mode"* — and handed over a measured
> negative on one leg of three. That refusal to self-acquit is what made the
> override reviewable rather than a rationalisation, and it cost one section.

#### And a suggestion from the lead was a hypothesis, was grepped, and was wrong

Reading the residual, the lead proposed a second positional refusal — decline
when the qualified name's root is an **enum** — on the stated premise that
"upstream reaches an enum member type through a different path". Priced the
`bd tsr-4sa` way and checked against upstream, it was declined **twice**:

- the premise is false. `SymbolFlagsType` includes `SymbolFlagsEnumMember`
  (`internal/ast/symbolflags.go:45`), so `resolveEntityName` resolves `E.A`
  through the same `resolveQualifiedName` lookup as `M.I`. `getTypeReferenceType`
  does branch for an enum (`checker.go:23156`) — but that is a claim about the
  **type**, not the **name**;
- `conformance/enumLiteralTypes3.types:9` records `>Yes : Choice.Yes`. Upstream
  prints the written qualified enum member name verbatim, which is what the
  build already does. The refusal would have refused a shape the port gets right,
  which is where its 401 lost conversions came from — 11.8 destroyed per wrong
  line removed.

This document already says *a prerequisite stated in a comment is a hypothesis*.
It applies with no discount to a prerequisite stated **by whoever is running the
session**, and the cost of checking was one grep and one baseline lookup.

### Recording an untested candidate design as untested is what lets it be tested

Two agents worked the qualified-naming family in the same session from different
instruments and never compared notes. The first, sizing the `TypeReference` gap
root, closed its findings page with a design it explicitly refused to endorse:

> One candidate design is recorded explicitly as **unmeasured**: reuse the
> *written* dotted entity name, so the qualifier cannot fire on a line that never
> wrote one — which is the entire blast radius that refused the chain. Nobody
> should build it before the counterfactual is re-run.

The second, running the counterfactual, arrived independently at the same
mechanism, named it **design W**, measured it at 1,770 converts / 99 wrong / **0
at risk**, and built it for **+3,590**.

The corroboration is worth something on its own — *"two probes with different
predicates landing on the same lines is corroboration"* is already in this
document, and this is its stronger form, two probes landing on the same
**mechanism**. But the transferable part is the *shape* of the first agent's
sentence:

> **A candidate design written down and labelled "unmeasured, do not build" is
> the cheapest artefact in this repository.** It costs one paragraph, it cannot
> mislead anyone — the label is the whole safety property — and it converts the
> next session's *design* problem into a *measurement* problem, which this
> project knows how to run. The alternative failure modes are both worse: not
> writing it down at all, and writing it down as though it were reasoned.

This document already forbids documenting an intention as though it were built,
and requires a comment that is unverified to *say* it is unverified. The same
rule pointed forwards is a recommendation rather than a prohibition: **when a
probe suggests a design it cannot test, record it with its label.** The blast
radius sentence above — *"the qualifier cannot fire on a line that never wrote
one"* — is precisely the reasoning that made the design's at-risk column zero,
and it existed in the tree before anyone measured it.

#### And it caught a sixth stale prerequisite on the way

`declared.rs` carried *"`resolveEntityName` … the binder does not expose yet"*.
It was false: `Symbol::exports` is a public `SymbolTable` and
`BindResult::resolve_name` already reads it
(`crates/tsr-binder/src/lib.rs`). That comment is one of the reasons the
resolution half sat unbuilt, and the agent found it by **grepping the
declaration rather than trusting the sentence** — this document's standing rule,
now on its sixth recorded instance. The comment is gone, removed by the build it
had been discouraging.

### A residual analysis is a diagnosis, and a diagnosis is a hypothesis

The qualified-naming build's residual named its largest downstream family —
20 lines in `{enum,stringEnum}LiteralTypes1,2` — as *"the missing
`alias_symbol_for_type_node` call on the type-reference arm"*, and filed it as an
item on that basis. The next thing anyone would have done is add that call.

One `sed` over the baseline says otherwise. `conformance/enumLiteralTypes1.types:361`:

```
function f20(x: Item) {
>x : Item                                  <- the parameter, and the port agrees here
    switch (x.kind) {
        case Choice.Yes: return x.a;
>x : { kind: Choice.Yes; a: string; }      <- INSIDE the case arm
```

Upstream is not naming the alias differently. It is **narrowing a discriminated
union by a `switch` on its discriminant** and printing the narrowed constituent.
The alias-symbol machinery is already called at every `declared.rs` site that
needs it, and a fourth call converts **zero** of those 20 lines. The item is
`flow.rs`, not naming.

This is the **seventh** recorded stale-or-wrong prerequisite in this repository
and the first that was **never true** — every earlier one was a comment that had
decayed over hundreds of commits. This one was three hours old, written in the
same session it misled, by an agent that had just done excellent measured work
everywhere else on the page.

> **The residual dump gives you the LINES. The mechanism you name beside them is
> inference, and it inherits none of the dump's authority.** A `wrongdelta`
> total is measured; *"these 20 are alias naming"* is a guess wearing the
> measurement's clothes, and it is the more dangerous of the two precisely
> because it arrives attached to a real number in a page full of real numbers.

This document already says *a prerequisite stated in a comment is a hypothesis*,
and *a passing ratio is permission to ship, not permission to stop reading*. The
missing clause is that **reading the residual produces claims that need checking
like any other** — including, and especially, when the reader is whoever is still
in the room and the ink is wet.

The check is the project's existing rule pointed one step earlier: **attributions
come from baselines, never intuition.** Five test expectations have been written
from intuition here and all five were wrong; this is the same error moved from
the test to the diagnosis, where it is cheaper to make and more expensive to
catch — a wrong test fails, a wrong attribution just quietly sends the next
session to the wrong file.

### A ported predicate can be sound upstream and unsound here

`isUntypedFunctionCall` (`checker.go:9931`) tests `IsTypeAny(funcType)`. Ported
literally as a flag test, it measured **328 gained against 248 manufactured wrong
lines**, and the residual named the cause in one row: `want string | got any`,
137 of the 248.

The port of the predicate was *correct*. What differed was the thing it tests.

> **Upstream reaches `any` for a callee only where the source said `any`. This
> port also reaches it wherever an unported mechanism gives up.** So the two
> compilers disagree about which types are `any`, and a predicate that reads
> `IsTypeAny` inherits that disagreement wholesale. It is sound in upstream's
> type system and unsound in this one, and nothing about the porting was wrong.

This is a distinct failure mode from the ones this document already records, and
worth separating from them:

- a *stale prerequisite* is a claim about this port that has decayed;
- a *wrong attribution* is a diagnosis that was never true;
- **this** is a faithful port of a correct predicate whose **inputs mean
  something different here**.

The last is the hardest to see, because every individual step checks out: the
upstream function was read, the anchor resolves, the translation is literal. The
error is one level down, in a type the predicate merely consults.

**The test that catches it, and it is cheap:** for any ported predicate, ask
*"does this port produce the same population for the thing being tested?"* Here
the answer is no, and the fix was to test the **written syntax** — `any` in an
annotation — instead of the computed flag, because that is the one form in which
the two compilers make the same claim.

The fix cost **214 conversions to remove 248 wrong lines**. On a ratio that is a
bad trade and it was taken anyway, on this document's standing rule: answering
off a premise the two compilers do not share is a **wrong rule, not a bad
trade**, and a rule is not priced. The 248 were never conversions to lose — they
were 248 assertions this port had no basis for.

#### The bar's falsifier is what made this a twenty-minute correction

The registration named it in advance: *"if new wrong is far above 13, the
positional refusal is not firing — check that before re-reading anything else."*
The refusal *was* firing; the arm was wider than the design that had been sized.
But the falsifier pointed at the right *drawer* — the gap between the design
measured and the code written — and that is most of the diagnostic work.

> **A falsifier does not have to be right to pay.** It has to name where to look
> first. This one named the wrong cause and the right location, and still turned
> a fired leg into one re-measurement rather than an investigation.

And the narrowing **subsumed the registered positional refusal**: an unannotated
parameter has no annotation, so the written-syntax test excludes it for free. The
explicit predicate was deleted rather than left as dead code, with a paragraph at
the call site saying why the family is still refused without one — because a
refusal that survives only as an absence is one nobody can find later.

### A figure that appears twice in one file will disagree with itself

`STATUS.md` §1 states the gradient in two places twenty lines apart: a table row,
and a fenced block giving the `right / gap / wrong` triple. Two builds landed
with the table updated and the block not, so the section said **347,530** and
**347,384** at once, and the second number carried a stale `gap` and `wrong`
beside it.

Nobody read it wrong, because a consistency pass caught it first. That is luck.

> **Duplication of a number across a document is a defect with a delay fuse.**
> Every edit updates the copy the editor is looking at. The rule this project
> already runs on — *every number carries the commit it was measured at* — does
> not help here, because both copies looked equally authoritative and neither
> carried a commit.

Two cheap defences, and the second is the one that scales:

- **Re-derive both copies from the same run**, never one from the other. The
  arithmetic check when they disagree is free and immediately diagnostic: here
  `347,384 + 32 + 114 = 347,530` named both missing builds in one line.
- **Prefer one copy.** A second statement of the same number is not redundancy,
  it is a second thing to maintain. Where a document genuinely needs the figure
  twice, the derived one should say what it is derived from.

### The instrument that answers a standing question is usually built for something else

This session opened by recording a discrepancy in `STATUS.md` §1 — the port's
`right + gap + wrong` fell **9,145 short** of the corpus denominator, because the
three figures came from three instruments that do not share one. It was written
up as an open question and explicitly costed: *"worth a future session's first
hour."*

It was settled the same session, in about ten minutes, and not by anyone
scheduling it. A build two items later needed a single-pass verdict per line for
an unrelated bar leg (`gap→wrong == 0`, which no existing instrument could
report), so `verdictdump.rs` was written for that. Once it existed, the standing
question was one command: the triple sums to 468,915 and the corpus is 478,954,
so the residue is **10,039 unaligned lines** — the baseline and this port
disagree about the *expression*, so no comparison of answers is meaningful.

> **Standing questions are answered by noticing, not by scheduling.** A backlog
> of "worth a session's first hour" probes is a backlog of things that were too
> expensive when they were filed; the cost that matters is what they cost *after*
> the next unrelated instrument lands. When a new probe is built, the cheap move
> is to re-read the open questions and ask which of them it now answers for free.

The corollary for how instruments get written: `verdictdump.rs` reports **every**
verdict rather than the one its bar needed. That generality cost nothing at the
time and is the entire reason it could answer a question it was not built for.

## Tooling costs, measured — and the guess that was wrong

Every figure below was timed on 2026-08-07, on a 32-core machine, warm. They are
here because a session spent an hour reasoning about pace from an **estimate**.

### What a corpus run actually costs

```
coverage        (all 16 suites)              35 s
verdictdump     (a verdict per line, 468,915) 39 s
casedelta                                     41 s
depend                                        39 s
cargo test --workspace          (warm)       7.5 s
cargo clippy --workspace --all-targets (warm) 0.2 s
```

The suites use rayon and saturate ~15–18 cores, so these do not shrink much on a
smaller box, and they do not grow much either.

> **A full before/after measurement pair — stash, run, pop, run — is about 90
> seconds.** Sizing a mechanism, registering a bar, and measuring a build costs
> roughly two minutes of machine time.

**The correction this replaces.** A session explained slow progress as *"easily
45+ minutes of pure compute"* from corpus runs. That figure was never measured
and it was wrong: across ~40 runs the corpus accounted for perhaps twenty
minutes of a multi-hour session. The time actually went to **waiting on
subagents** — explicit `sleep` poll loops — and to release rebuilds after each
`tsr-checker` edit.

> **The measurement discipline is not the expensive part, and a session that
> feels slow should time something before blaming it.** The instinct to
> attribute cost to the most rigorous-looking activity is self-flattering and,
> here, was off by more than an order of magnitude on the specific claim.

### The scoring pair is one tool now — iterate filtered, land unfiltered

Measured 2026-08-07, after twenty-one builds of running the pair by hand:

```
scorepair            (full corpus, diff, matrix)   41 s
scorepair TSR_FILTER=<two cases>                 0.36 s   (~115×)
```

`cargo run --release -p tsr-conformance --example scorepair` runs the
corpus, diffs against the last accepted baseline
(`target/verdict_baseline.tsv` — per-checkout derived data, never
committed), and prints the transition matrix with per-case attribution
and example line keys on every adverse transition. `-- --accept`
advances the baseline at a landing. `TSR_FILTER=<substrings>` restricts
the run for the inner loop; the same variable works on `verdictdump`.
The row computation is shared (`tsr_conformance::verdict`), so the dump
and the scorer cannot drift.

Three rules are enforced by the tool rather than remembered, and each
one is a mistake this project actually made:

- **`--accept` refuses a filtered run.** A baseline must be a full run;
  a partial one silently becoming the reference is how a regression
  hides.
- **A filtered score labels itself `[PARTIAL]`** and diffs only its
  subset. Adverse movement routinely lands in cases a change never
  touched (`classBlockScoping` from `new`-gate work, `awaitInNonAsync`
  from for-of work), so a filtered run may ITERATE but never LAND.
- **The diff is against a stored baseline, not the previous ad-hoc
  dump.** Two measurements in one session diffed against a stale file
  and mis-attributed a build's whole matrix
  (`checker-notes-callres.md` §36's first pair).

## `ast-grep` — verified patterns, and one silent-failure trap

`ast-grep` (0.45.0) is installed and every pattern below was run before being
written down. It replaces the `grep -n 'func …'` + `sed -n 'START,ENDp'` pair
that this project's upstream reading has otherwise been done with — that pair
requires **guessing the end line**, and guessing it wrong is how a read silently
takes in the neighbouring function or truncates the one you wanted.

### File shape — 0.02 s

```sh
ast-grep outline crates/tsr-checker/src/relater.rs   # works on .go and .rs
```

Prints every `impl`, `enum`, `mod` and their member names with line numbers. Use
it **before** reading a file in chunks.

### One upstream function, exact boundaries — 0.16 s

```sh
ast-grep --pattern 'func (c *Checker) resolveUntypedCall($$$) $$$ {$$$}' \
  vendor/typescript-go/internal/checker/ --lang go
```

No line-range guessing. The Rust equivalent works the same way:

```sh
ast-grep run -p 'fn symbol_chain($$$) -> $R { $$$ }' -l rust crates/tsr-checker/src/checker.rs
```

### Rust expression patterns work

```sh
ast-grep run -p 'bump(&COUNTERS.$X)' -l rust crates/tsr-checker/src/calls.rs
ast-grep run -p 'self.intrinsics.error' -l rust crates/tsr-checker/src/calls.rs
```

### **THE TRAP: a bare expression pattern in Go returns NOTHING, silently**

```sh
ast-grep run -p 'c.resolveUntypedCall($$$)' -l go vendor/typescript-go/internal/checker/checker.go
#   -> no output, exit 0.   grep proves 3 call sites exist at :2514, :2533, :8490.
```

Go's grammar cannot parse a bare expression as a whole file, so the *pattern*
becomes an `ERROR` node and matches nothing. **The failure is indistinguishable
from "no matches found"** — the exact shape this document warns about elsewhere,
a green-looking result from something that never ran. It does **not** happen in
Rust, which is what makes it easy to trust the tool and be wrong in one language
only.

Detect it by asking what the pattern parsed to:

```sh
ast-grep run -p 'c.resolveUntypedCall($$$)' -l go --debug-query=ast <file>
#   Debug AST:  source_file -> ERROR -> qualified_type …     <- the pattern is broken
```

The fix is to give the expression a **valid context** and select the node you
actually want. One line, no rule file:

```sh
ast-grep run -p 'func f() { c.resolveUntypedCall($$$A) }' \
  -l go --selector call_expression vendor/typescript-go/internal/checker/checker.go
#   -> all 10 call sites, identical to `grep -n`
```

The wrapper `func f() { … }` is throwaway scaffolding that makes the pattern
parse; `--selector` then picks the `call_expression` inside it. The same
`context` / `selector` pair is available in a YAML rule
(`rule: pattern: {context: …, selector: …}`) when the query is worth keeping,
but the one-liner is the everyday form.

**`--selector` alone does not rescue a bare pattern** — `-p 'c.f($$$)'
--selector call_expression` errors with *"Cannot parse query as a valid
pattern"*, because the pattern is still the thing that fails to parse. The
context wrapper is what does the work.

> **Before trusting an `ast-grep` pattern that returns zero, prove the zero.**
> Either `--debug-query=ast` it, or cross-check one hit with `grep`. A tool that
> reports nothing when it is *itself* malformed cannot be distinguished from a
> true negative, and a true negative is exactly the kind of finding this project
> acts on — refusals get written from zeros.

### Reading past the function you asked for — use `-A` / `-B` / `-C`

`ast-grep` takes grep's context flags, verified on the exact case that made this
matter:

```sh
ast-grep run -p 'func (c *Checker) resolveUntypedCall($$$) $$$ {$$$}' \
  -l go -A 12 vendor/typescript-go/internal/checker/checker.go
```

returns the function **plus** `resolveErrorCall`, `unknownSignature`, the TS 1.0
spec comment and `isUntypedFunctionCall` — every piece of the ADR-0038 argument
that unblocked a build, in one deterministic command. `-B n` gives preceding
context, `-C n` both sides. All three checked.

> **The neighbour is where the distinction you did not know you needed usually
> lives** — `anySignature` vs `unknownSignature` sat two lines apart and decided
> whether an item could exist at all. Ask for it explicitly with `-A`; do not
> rely on an imprecise read to stumble over it.


## The aligner-artifact test, and it applies to gain columns too

A scorepair's per-case transition rows are **attributions, not
measurements**: the walker pairs lines by subject text, so a change that
alters your own line texts re-pairs the neighbourhood, and the pair then
reports transitions in cases the change never touched. Two firings on
2026-08-09, symmetric: six R→W that nine trace rounds proved artifactual,
and — after the six taught the test — sixteen W→R in the SAME pair that a
two-lane bisect (nine checkout+dump points, two independent instrument
chains in agreement) dissolved the same way.

The test costs seconds and is decisive: **dump the exact keys at both
commits; a fixed index whose WANT text changes between runs is the aligner
moving, not the answer.** Apply it to any transition row naming a case the
change has no road to — and gains are as suspect as losses, because the
favourable reading is the one nobody re-checks (the same asymmetry as the
instrument-zero rule). The totals survive artifacts — right/gap/wrong
arithmetic is measured — but per-name credit does not, and a ledger built
on unchecked pair rows drifts one flattering row at a time.

Two seams found running the test, both of the produces-a-plausible-result
family: `ln -sfn` into an **existing directory** creates the link *inside*
it (the worktree then judges an EMPTY corpus — TOTAL 0 — which reads as a
clean zero); and an rtk-piped empty result is indistinguishable from
no-output success. Check `ls <link>/testdata` before trusting any
worktree measurement.

## The transcription law (2026-08-10, both checker lanes)

In ported-semantics function families — narrowing workers, inference
resolution, coercion ladders — **every transcription has landed and
every induced build has lost.** Two days' evidence, both lanes:
transcribed: §159/§161 (checker-2: +205 combined), §147/§150
(checker-1: +371 combined); induced: §153 (1:15), §157 (4:14), §162
(59:41), checker-1's two fixing-mapper probes (+4/9, −41). The rule:
**when porting a semantic function upstream implements in one place,
read and transcribe its full text FIRST — including the legs that look
skippable — and refuse partial inductions of it outright.** Decidable-
domain SLICES of a transcription are fine (§159's Kleene fallthrough);
reordered or re-derived LOGIC is not. A refusal of an induced build
records the upstream lines the rebuild must transcribe; that bound is
the refusal's product.

### Corollary (2026-08-10): induced reasoning hides in PINS

checker-1's §162 found an induction that had already shipped *inside
unit-test expectations* — pins encoding "keep the fresh literal", which
looked like evidence for the behaviour they had themselves assumed.
The transcription (`getCovariantInference`'s widening) corrected the
code AND the pins. **A pin is only evidence if it was read off the
oracle.** When a transcription contradicts a pin, check the baseline
before believing the pin; a pin written from the port's own behaviour
is an induction wearing a test's clothes.

### Corollary 2 (2026-08-10): a +0 measured at one SITE is not a verdict about a road

checker-2's §168 probed the this-substitution at one call site, measured
+0, and recorded "the residue is the subsystem" — a domain-wide claim.
checker-1's §164 ran the same transcription at the site that actually
reads members and measured +89/2. **A refusal record must name the site
it measured, and any claim that a ROAD is unreachable must enumerate the
sites it tried.** The unexercised-branch rule still stands for the code
(revert the +0), but the VERDICT it licenses is "not reachable here",
never "not reachable".

**It cuts both ways** (checker-1, same arm): §164's bar predicted ≥50
and its FIRST site measured +30 — under bar. Reverting there would have
reproduced §168's error from the opposite direction; what saved it was
asking *which other site runs this same rule* before judging, and the
answer (+59 more, at the call return, in a different file) completed the
arm at 44:1. **An arm that underperforms its bar at one site is not
finished being measured either.** Both failure modes — a premature
"unreachable" and a premature "under bar" — are the same mistake:
treating a site's number as a road's number. Enumerate the sites, then
judge.

### Corollary 3 (2026-08-10): a collapsed upstream DISTINCTION is a latent bug, not a simplification

checker-1's §165 found `members.rs:416` overwriting the receiver with
its apparent form — collapsing upstream's `getTypeWithThisArgument(
apparentType, receiver)` into one type. The collapse was invisible until
something CONSUMED the distinction (a this-argument substitution), then
it answered the constraint where the receiver was wanted. **When a port
merges two upstream values because they are equal at every site it
currently has, it has planted a bug that fires when the next site
arrives.** Where upstream carries two names, carry two — and when a
transcription's first pair shows a small adverse count, check whether a
collapsed distinction is producing it before pricing the arm.

### Corollary 4 (2026-08-10): enumerate sites before declaring a PREREQUISITE

Corollary 2 says a site's number is not a road's number. Its converse:
a defect at one site is not a PREREQUISITE for the road. checker-1's
§166 tested rock #3's stated blocker — a `this` type minted into two
tables — by simply CONSULTING BOTH at the substitution site, and gained
26 lines without unifying anything; the unification remains required
only for the representation work that actually rewrites those types.

Across §164/§165/§166: three arms, ONE transcription, three different
sites, **+141/2 combined, no new machinery in any of them.** The
operative habit is now measured three times consecutively: before
building anything, enumerate the sites that run the rule you already
have — and before accepting a stated prerequisite, test whether the
road can simply read AROUND it.

**Why the enumeration corollaries are affordable** (checker-1, and it
belongs beside them or a future window will treat enumeration as
expensive and go back to reasoning): **a full `scorepair` is cheap
here** — measuring one more site costs one run. The discipline of
"enumerate the sites, then judge" is only rational because the
measurement is nearly free; if that ever stops being true, these
corollaries must be re-priced rather than assumed.

**Custody note, same window**: checker-2 announced "§166 synced and
re-accepted" while §166 was still in checker-1's working tree. The
numbers happened to agree, but the claim was false when made and was
corrected within the hour. **"Synced" means after THEIR push and YOUR
pull — verify the commit is in your log before saying it.** This is the
same slip that produced the earlier stale-baseline misattribution, and
it is cheaper to announce late than to unwind an attribution.

### Corollary 5 (2026-08-10): census the DISPATCH before investigating logic

Four of this window's largest finds were **absent match arms**, not
wrong logic: `signature_parts_of` had no `ConstructorDeclaration` arm
(§162, +432/55), the this-substitution consulted one of two mint tables
(§166, +26), the expression dispatch had no `RegularExpressionLiteral`
arm (§167, +328/3 at 109:1) and no `ClassExpression` arm (+190/322 —
reverted; checker-1's §168, which checker-2 duplicated independently
within the hour — **four finds, one of them measured twice**).

**Widened by checker-1's §169**: the target is not only the dispatch —
it is **the dispatch AND every predicate the dispatch consults.** §169
was not a missing arm: `declaration_takes_no_contextual_return`
enumerated `FunctionDeclaration` and `MethodDeclaration` and answered
false for everything else, so `function* gd(){ yield 1 }` inferred
`Generator<number, void, unknown>` while the IDENTICAL expression
answered error and an annotated expression already worked. **A gate
that enumerates node kinds is a dispatch wearing different clothes**,
and it fails the same way: by stopping early. When one spelling of a
construct works and another does not, look for an enumeration that
lists the working spelling.

**And the missing case is often already answered elsewhere in the
port.** §169's gate asks "does this take no contextual return?" —
precisely what §94's `has_no_contextual_type` was built to answer for
expressions — so the fix reused a proven predicate instead of inventing
a test. An early-stopped enumeration usually stopped because nobody had
the second case yet; by the time it is found, the port often does.

**The census costs one run**:
`verdictdump | awk '$2=="GAP" {print $3}' | sort | uniq -c | sort -rn`
— a large count on a SIMPLE want-shape (a plain named type, no
generics, no context) is a dispatch-arm candidate. Check whether the
match has an arm for that node kind *before* investigating anything
downstream of it.

**And expect the arm to expose a second rule.** §162's arm forced
`getCovariantInference`'s widening; §170's arm answers `typeof
__class` where the want is `typeof C`, because the binder names an
anonymous class symbol `__class` while upstream takes the name from the
binding it is assigned to. A missing arm has been hiding whatever sits
under it for as long as it has been missing, so price the PAIR — and
land them together or neither.

**The census collides by construction.** Both lanes ran this census and
both went to the same top hit within an hour, duplicating a full
build-and-measure. A cheap shared census generates COLLIDING targets by
design — so **claim the top hit before building it**. When lanes worked
on distant subsystems this coordination was not worth its cost; with a
shared census it is one message against a duplicated arm.

## The per-build gate

`cargo xtask gate` runs the five checks every build ends with — `fmt --check`,
`clippy -D warnings`, `cargo test`, `anchors`, `issue-ids` — in order, stopping at
the first failure and exiting non-zero.

Use it instead of the five separately. Four of them only printed their result
until §924, and a commit went out with clippy red because a number that had read
`0` for four hundred builds stopped being read
(`docs/architecture/checker-notes-diag2.md` §923).

**The check that matters is the one that stops the pipeline.**

Chain it with `&&`, never `;`:

```sh
cargo run -q -p xtask -- gate && git commit …
```

§937's commit shipped with the gate red because the line read `gate; git commit`.
The gate exited non-zero and nothing was listening
(`docs/architecture/checker-notes-diag2.md` §938). **An exit code only matters to
a command that is chained to it.**

### Corollary 6 (2026-08-10): the provenance tiebreak for a +0

The unexercised-branch rule ("a +0 keep needs a demonstration
independent of the board") and the collapsed-distinction rule ("a
divergence is a latent bug whether or not it fires") both apply to a
transcribed change that measures zero, and they point opposite ways.
**Tiebreak by PROVENANCE and by what the change does:**

- **DERIVED code measuring +0 → revert.** It has no authority except
  the board, and the board declined it.
- **TRANSCRIBED code that REMOVES A SELF-INCONSISTENCY → keep**, even
  at +0. Two enumerations of one concept disagreeing is a bug already
  present; aligning them to upstream removes it whether or not the
  corpus currently walks the difference. (checker-1's §171: a `return`
  inside a get accessor walked past its own container. checker-2's
  §172: the flow-container ascent omitted `FunctionDeclaration` while
  the same family's other enumeration listed it.)
- **TRANSCRIBED code that ADDS A CAPABILITY → revert and bank.** It is
  correct but unexercised, and correctness alone does not earn
  permanence. (checker-1's §170, the concise async arrow body: +0/1,
  banked.)

**And when a keep is made, state that no gradient is attributable to
it.** That is the honest price of keeping something the board did not
pay for.

### Corollary 7 (2026-08-10): instrument the ENTRY when a transcribed arm measures zero

"The branch is unexercised" and "the node never arrives" are identical
in a scorepair and have completely different owners. checker-1 wrote
`checkNewTargetMetaProperty` correctly, measured zero, and only an
entry probe revealed the cause: **the parser never constructs a
`MetaProperty` node at all**, so 163 corpus lines sit behind a parser
change with the checker half already written.

The corollary that stings: **`parser_typescript` reads 100% while an
entire AST node kind is missing**, because the suite does not
discriminate that kind. A 100% suite bounds what it measures, not what
exists — and no gradient reading would have found this. It took an arm
that refused to fire.


### Corollary 8 (2026-08-11): a comment asserting redundancy is a lead, not a fact

Four times in one window a doc comment stated that some guard, arm or
merge was unnecessary because the same result was reached another way —
and four times the claim was false, in exactly the sub-case where the
"other way" also fails:

| # | site | the claim | what it cost |
|---|---|---|---|
| 1 | `unary_result_type` (§192) | an `error` operand makes bigint-ness unknown, so `number` would be a guess | 78 lines, 2 cases; upstream's `maybeTypeOfKind` is a flag test and cannot answer yes for `Any` |
| 2 | `types_producer.rs` property-access guard (checker-2 §183) | "already covered by a rule reached along a different route" | 1 case; the route also fails when the access itself fails |
| 3 | `types_producer.rs:811` | the same claim again, still standing an hour after §183 disproved it | corrected in place |
| 4 | `declare_into_with_excludes` (§196) | "Still merge … **Upstream does the same**" | **47 cases**; upstream mints a fresh table-less symbol (`binder.go:286`) |

The shape is always the same: **the redundancy holds in the healthy
case and fails in the error case**, which is precisely the population
the corpus's error-recovery and duplicate-identifier tests are made of.

So:

- **A "this is already handled" comment with no measurement behind it is
  a to-do, not documentation.** Treat it as a ranked lead.
- **Falsify it against a BASELINE, not against upstream's source.** §196
  needed no Go at all: `varAndFunctionShareName.symbols` records two
  symbols and `.types` prints two different answers for the same
  spelling, which one merged symbol cannot produce. That asymmetry is
  visible in thirty seconds and is decisive.
- **When you write such a comment yourself, write the falsifier beside
  it** — the case that would fail if the redundancy did not hold. A
  redundancy claim with no named witness is the same unfalsified
  assertion the four rows above were.

### Corollary 9 (2026-08-11): census the EXPRESSION, not only the type

The `checker_types` near-miss board (`examples/nearmiss.rs`, §190) ranks
blocked lines by what they *want*. That first cut is a distribution over
types and it systematically hides families, because one defect scatters
across many want-types and one want-type collects many defects.

Two arms from the same census make the point:

- `want number, got any` read as **42 scattered gaps**. Dumped with the
  blocked lines' **expressions**, eight were literally `x : number` in a
  `tsx` case — one missing match arm, **+55 cases** (§195).
- `want () => any, got any` (14) and `want () => void, got any` (10)
  read as a function-typing family. By expression they were
  `varAndFunctionShareName`, `duplicateIdentifierInCatchBlock`,
  `multipleExportDefault1/2`, `augmentedTypes*` — a **duplicate
  identifier** family, **+47 cases** (§196), nothing to do with function
  types.

One command separates them:

```
nearmiss --max 1 | awk -F'\t' '{print $1"\t"$5}'
```

**Never conclude a want-shape row is a tail without it.** This is the
general form of the dispatch-census corollary above: a histogram keyed
on the *answer* cannot see a defect keyed on the *syntax*.

### Corollary 10 (2026-08-11): `want any, got <concrete>` inverts the usual reading

Once a baseline writer renders `errorType` as `any` (checker-2's §180),
the census grows a row that reads like a printing question and is not
one. Its diagnosis is the mirror of the ordinary case:

| row | meaning |
|---|---|
| `want <concrete>, got any` | the checker answers `error` or a genuine `any` — a **missing** arm |
| `want any, got <concrete>` | **upstream refused to resolve this and we did** — an over-resolution bug in the lookup |

The second is the port being *more* successful than upstream, which is
never a gradient gap and never a printing guard. checker-2's
private-name cluster (a `#x` reached from a derived class, or shadowed
by a nested class) and §195's residue (an attribute whose name is
missing, where we now answer `true` and upstream answers its error
type) are both this shape.

### Corollary 11 (2026-08-11): a refusal is protected from re-derivation; its stated REASON is not

`STATUS.md` §5's rule — *a refused item stays on this page with the number
that refused it, so the next session does not spend a cycle rediscovering
the same negative* — is load-bearing and should stay. But it protects the
**verdict**, and a refusal carries two other things that it does not
protect:

1. **A scope.** "This exit stays a gap, that one is served."
2. **A reason for the scope**, which is frequently a claim about
   *upstream's* control flow rather than about the measurement.

§199 is the worked example. §21 measured the overload-failure intersection
honestly and landed it, then wrote: *"The arity-mismatch exit stays a gap:
upstream routes it through `pickLongestCandidateSignature`, a different
mechanism, unsized."* `getCandidateForOverloadFailure`
(`checker.go:9498`) never asks why `chooseOverload` failed — its branch is
on the **candidate set**, and arity never enters it. Deleting three words
from the condition was +3 cases.

This differs from corollary 8 in what it costs to check. Those four were
claims about *our* code ("already handled elsewhere"), falsifiable only by
measuring. A claim about upstream's control flow is falsifiable by **one
read of the callee**, which makes it both cheaper to check and worse to
leave standing.

So:

- **When you refuse a slice, say which of the two you are recording**: "the
  measurement did not cover this" (a fact about the run, durable) or
  "upstream does something else here" (a fact about the source, and it
  should carry the `file.go:line` that says so).
- **A refusal whose scope cites upstream's control flow is a one-read
  audit.** Cheap enough to redo whenever the family comes back up the
  board, and §199 says the hit rate is not low.
- Re-reading a refusal's *reason* is not re-deriving the refusal, and
  §5's rule was never meant to forbid it.

### Corollary 12 (2026-08-11): the +0 tiebreak — wrong answer or gap?

Corollary 6 disposes of a transcribed change that measures **+0** by
provenance. Two arms landed the same hour with the same +0 and opposite
dispositions showed the rule needed a sharper test, which checker-1
supplied:

> **Would deleting the change re-introduce a WRONG ANSWER, or merely
> re-introduce a GAP?** Wrong answer → keep at +0. Gap → the +0 is a
> real argument to revert.

The distinction is whether the change makes the port answer a question
it was **already being asked**. An unexercised *capability* runs on
inputs the port previously never reached; corrected *logic* was already
running and returning the wrong thing.

Worked pair, both at +0, cited by hash rather than paraphrase:

- **`d1dba6b9` (§189) — KEPT.** The truthiness filter abandoned
  narrowing for every constituent when one lacked the discriminant;
  upstream tests such a constituent as `unknown`
  (`flow.go:744-747`). The path was already running and already wrong.
  Deleting it restores a wrong answer.
- **`f4484a57` (§190) — REVERTED.** The decorator exclusion
  (`utilities.go:994-1011`) would refuse a private name written inside
  its own class's decorator. No corpus fixture reaches that position.
  Deleting it restores only a gap.

A +0 keep must state that **no gradient is attributable to it** — see
corollary 6's honesty clause.

### Corollary 13 (2026-08-11): run the gate that LANDS

Per-crate `cargo clippy -p <crate>` is not the gate that lands;
`cargo clippy --workspace --all-targets` **after the final rebase** is.
§189 passed the former on its author's tree and arrived red on the
merged tree, blocking the other lane's commit hook. Clippy only
re-lints crates it recompiles, so a warm cache can hide a lint that the
post-rebase run — which almost always recompiles — will surface.

### Corollary 14 (2026-08-11): evidence narrower than the claim

Corollary 11 asks whether a refusal's *reason* is true; corollary 12's
sibling asks whether its *scope* matches the reason. A third shape is
harder than both, because the evidence is **genuine**:

> For a refusal citing a MEASURED defect, ask **which sub-population the
> measurement actually ranged over.**

checker-1's §207 is the worked case. §168 refused `checkClassExpression`
on a real measurement: the arm printed `typeof __class` — the binder's
synthetic name — where the baseline wanted the variable's name. Nothing
in that refusal was false. But every fixture behind it was an
**anonymous** class expression, and the refusal was written over the
**node kind**. A named class expression carries the name the source
wrote; the separator is one word (`node.name.is_some()`), and the
wrongly-refused population was about three quarters of the row. +17
cases.

The same session produced the mirror image, which is why the question is
worth asking in both directions: **§191** widened an arity gate on a
true general rule (upstream's window is `[minTypeArgumentCount, len]`)
and measured **−17**, because admitting a short list without running
`fillMissingTypeArguments` prints `C<A>` where upstream prints
`C<A, X>`. A rule can be true of the population and still be wrong to
apply alone. `a9a33f97`.

### Corollary 15 (2026-08-11): a fixture needs both halves

Two techniques earned separately are one rule:

> **Assert that the fixture REACHES the branch, and assert WHICH WAY the
> branch went.**

§198's anti-vacuity guard does the first — when an arm's only trigger is
another gap, assert the gap first, so the test fails loudly if that gap
closes instead of quietly ceasing to reach its branch. §206's control
does the second — a fixture pinning "a computed member must not silently
vanish" also pinned *which of the two things it becomes*, and caught a
`NUMBER_LIKE` vs `StringOrNumberLiteralOrUnique` misclassification one
commit after it was written. **A fixture with neither half can pass for
years.**

### Corollary 16 (2026-08-11): a refusal YOU wrote is not evidence

Corollaries 11–14 audit refusals inherited from earlier sessions. The
same failure occurs **within one session, in hours**, and the author is
the last person to notice:

- **`97803011`** — an arm recorded as *"cannot be written faithfully
  today"* with a three-piece build named. **§185 built all three pieces
  three hours later.** The scoping that made the refusal credible is
  exactly what made it survive.
- **`a82331be` → §194** — a lead recorded as *"not built: a resolution
  change whose likely sites are shared with the other lane"*. The
  refusal itself named the deciding probe (*which road types this
  expression?*). Running it cost **one grep**, answered the whole
  question — the road was in a file this lane owns outright — and the
  defect was one argument. **+3 cases.**

**Before writing "not built", run the probe the refusal is about to
name.** If the refusal can name a deciding experiment, that experiment
is nearly always cheaper than the sentence justifying its absence. And a
capacity claim ("I can't finish this safely") is a claim about the
world, subject to the same rule as any other: check it, or write it as
a question rather than a finding.


### Corollary 17 (2026-08-11): change a predicate, grep its name, list the call sites

§209 widened `bracket_holds_index_signature` to upstream's nine shapes and
rewrote **one** of its two consumers. Its own commit message stated the
generalisation — *a narrow recognition predicate silently licenses narrow
consumers downstream, so grep for everyone who consumes what it recognises
before measuring* — and the grep was not run on the predicate the same commit
had just widened. §210 fixed the second consumer an hour later, +6 cases.

That is corollary 16 aimed at a **lesson** instead of a refusal: *a rule you
have just written feels already applied*. It is the hardest form to catch,
because writing the rule down supplies the feeling of having obeyed it.

So the rule is a **pre-flight, not a principle** — principles are what get
exempted:

- **When a change alters what a predicate RECOGNISES, `grep` the predicate's
  name and list every call site in the commit message, with line numbers.**
  A count in the message is checkable by the next reader; an intention is not.
- The consumers were not bugs before. They were **specialisations of the same
  wrong assumption**, correct exactly while the predicate was narrow, and they
  become wrong at the instant it is right. Expect one per call site.
- It generalises past parsers: any place where one function decides *whether*
  and others decide *what to do about it*.

**Confirmed within the hour, in the other lane.** checker-2 ran the grep before
measuring a narrowing fix to a union member filter and found the identical
defect in a sibling filter written from the same template — the **third**
instance of one emptied-set bug across three filters, the first two found
hours apart by measurement. The pre-flight found the third for free.

### Corollary 18 (2026-08-11): ask whether the function already exists before asking how to write it

Four times in one session, across two lanes and three crates, the port
**already had the machinery** and the defect was that a site did not use it:

| # | site | what already existed |
|---|---|---|
| 1 | §209's printer half | `ListFormat::INDEX_SIGNATURE_PARAMETERS`, defined and never referenced |
| 2 | checker-2's §194 | heritage-name resolution wanted `SymbolFlags::VALUE`; the flag was one argument away |
| 3 | checker-2's §195 | upstream's self-extension fallback was already transcribed in a comment at the site, unbuilt |
| 4 | checker-2's §196 | the shared `quote` the rest of the printer uses |

Add to these the refusals whose stated prerequisite was a subsystem that
already exists (corollary 8's `index_signatures.rs` and array-elision rows),
and the pattern is not rare — it is one of the most common shapes in the port.

Its cause is structural rather than careless: a 300k-LOC transliteration is
built in slices, and a slice written for one call site is invisible to the
next person who needs it three crates away. Hand-rolling a five-line loop is
faster than finding the ten-line helper, and it is indistinguishable from
correct until the shared assumption changes underneath it — which is exactly
corollary 17's cascade.

**Ask "is there already a function that does this?" before "how do I do
this?"** — and when the answer is yes, prefer the shared one even if the
hand-rolled version would be shorter, because the value is not the lines
saved but that the two sites cannot drift.

### Corollary 19 (2026-08-11): record a failed induction WITH its witnesses

Corollary 12 says what to do with a change that measures +0. This says
what to do with one that measures **wrong**:

> **When an induced build fails, revert it and record the WITNESSES —
> the specific rows it fixed and the specific rows it broke — at the
> site.** The revert costs nothing extra, and those rows are precisely
> the acceptance test the correct build will need.

Worked case, one function attempted twice an hour apart:

- **§200 (induced, reverted)** — "an intersection satisfies every
  constituent, so AND their facts." Fixed `number & { _foo: string }`
  under `typeof === 'object'` (wants `never`); broke
  `F & { foo: number }` where `F = { (): string }` (wants the
  intersection KEPT). Both witnesses recorded in the revert.
- **§201 (transcribed, +3)** — `getIntersectionTypeFacts`
  (checker.go:31118-31134) has **two** rules the induction had neither
  of: an intersection containing a PRIMITIVE discards its object
  constituents as type tags, and the fold is **OR for exactly two bits**
  (`TypeofEQFunction | TypeofNEObject`, checker.go:478) and AND for
  every other. Neither follows from what "intersection" means.

Because the witnesses were already at the site, the transcription had
its acceptance test before it was written — it was known right **for the
reason it was supposed to be right**, within one run, rather than by
luck. That is corollary 15's "assert which way the branch went" applied
to a whole attempt instead of a fixture.


### Corollary 20 (2026-08-11): if your port is longer than the predicate, you ported the witness

Corollary 16 says a refusal's scope must match its reason's scope. The same
defect has a quieter form on the *building* side, and it does not look like a
mistake at the time, because the code is correct — merely narrow.

> **When a ported predicate is substantially longer than the upstream predicate
> it replaces, suspect that what got encoded is the position the witness
> happened to occupy rather than the rule.**

Worked case, one guard ported twice, ten cases apart:

- **§183** ported `type_symbol_baseline.go:383` as a twelve-line pattern match:
  is the parent a `PropertyAccessExpression`, *and is this node its `name`*?
  That was true of the witness (`Obj.fn = function(){}`), it measured positive,
  and it stood for weeks.
- **§204** ported the same line as upstream writes it —
  `!ast.IsPropertyAccessOrQualifiedName(node.Parent)`, a test on the parent
  **alone**: either side of either construct. Witness
  `moduleOuterQualification`, where `outer` is the *left* of a qualified name in
  `extends outer.Beta`. **+10 cases** were sitting in the half §183 did not
  cover.

The tell is mechanical and needs no insight into the domain: upstream's
condition named one predicate over one field, and the port named two fields and
a position. **Length asymmetry in a transcription is a smell**, because faithful
transcription is normally length-preserving — the extra clauses came from
somewhere, and the only thing present at porting time that upstream's author did
not have is the witness.

How you would know this is wrong: if the narrow port's extra clauses can be
traced to a *stated* upstream fact (a different call site, a documented
exception) rather than to the fixture in hand. Then the asymmetry is real
information and not an artefact.

### Corollary 21 (2026-08-11): a cluster named by its symptom is a list of leads, not a family

`examples/nearmiss.rs`'s `--shapes` output invites being read as a work
queue: one row, one build. It is not, and the difference is a cost model.

- A **real family** shares a cause, so diagnosis amortises across its
  members: §195's 55 cases were one missing match arm, §196's 47 were one
  binder branch.
- A **symptom cluster** shares only what the output looks like. Every
  member costs a fresh diagnosis, and the row's size is a measure of
  nothing.

**You cannot tell which you have from the census.** The worked example:
~18 one-blocker cases were handed over as "an extra empty-text assertion
line we emit and upstream does not", with the `.types` writer eliminated
as the cause. Two members already had *non-empty* text (`static`,
`default`), which was noticed and set aside as "maybe two shapes". The
first one opened, `parserForOfStatement21`, turned out to be a **third**
cause and its own eleven-line rule — `parseVariableDeclarationList`'s
named `of` lookahead (`parser.go:1583`), nothing to do with the general
error-recovery arm the rest of the cluster does need.

This is corollary 9 running backwards, and the pair is the point:

| | keyed on | effect |
|---|---|---|
| corollary 9 | the **expression** | *reveals* a family the type histogram had scattered |
| corollary 21 | the **output's appearance** | *invents* a family out of unrelated causes |

Both are one census away from each other and they pull in opposite
directions.

So:

- **Before calling a census row a family, open two members and check they
  have the same cause.** Two is cheap; the row's size is not evidence.
- **Diff the two fixtures before counting them as two.** The remedy has its
  own failure mode: a corpus contains near-duplicates, and a census counts
  *files*. checker-2 opened two members of an arrow-contextual-typing row
  and found `assignmentCompatability_*` twice — **the same file with one
  word changed** (`apply` against `call`) — so the row looked like it had
  two independent witnesses where it had one. Near-duplicates inflate a
  row exactly where "open two members" is trying to deflate it.
- **If the two members disagree on cause, that IS the result.** Report it
  and stop; do not go looking for a third that agrees. Confirmation bias
  has a natural home in "open two members", and this is what closes it.
  checker-2 opened three members of the same row and found three causes —
  a contextual type with no call signature, a function expression in a
  type assertion, and an error-recovery `super` — and reporting the
  disagreement was worth more than any of the three builds would have
  been.
- **When handing a cluster to someone else, quote the witness and say what
  you eliminated — not what you infer remains.** "The writer is not the
  cause" is a fact; "therefore the parser's recovery arm is" is an
  inference from one witness to a mechanism, which is corollary 16's shape
  wearing a handoff. Handing over a symptom labelled as a cause is worse
  than handing over the symptom.
- **A "verified" in a handoff should name the specific thing eliminated.**

### Corollary 22 (2026-08-11): anything named by POSITION is two-lane-unsafe

Two lanes wrote a corollary 17 and an 18 within the same hour, because a
numbered list is a **shared mutable counter** that both were treating as
append-only local state. That collision was cheap to repair — renumber by
landing order, fix the three `STATUS.md` citations — and it is the harmless
member of a family.

The dangerous member is the **`§N` build number**. Those are cited from
commit messages, which are immutable, so a collision there cannot be
repaired the way this one was: two different builds would permanently
answer to one name, and every later citation would be ambiguous about
which. This session's two lanes stayed clear of each other only because
their ranges happened not to overlap — checker-1 in §190–§214, checker-2
in §153–§204 — and nothing enforced that.

Third member: **`STATUS.md` §5's row order**, and any other place a row is
identified by where it sits rather than by what it says.

So, when more than one agent is writing:

- **Claim a numeric range before using it**, and say so where the other
  lane will see it. A range is cheap; a collision in commit messages is
  permanent.
- **Prefer content-addressed references.** A commit hash, a fixture name
  or an upstream `file.go:line` cannot collide. `§199` can, and already
  nearly did.
- **When you must cite a number, cite it with its lane or its hash** the
  first time it appears in a document — `§204 (checker-2, 
  `a4874675`)` survives a renumber; `§204` does not.
- **Check the counter before appending to it.** `grep '^### Corollary'`
  costs nothing and is exactly the pre-flight corollary 17 prescribes for
  predicates, applied to prose.

### Corollary 23 (2026-08-12): a test asserting a known-wrong value must say so at the assertion

A port under construction is full of assertions that pin what the code *does*
rather than what it *should* do. That is legitimate — it stops silent drift in a
subsystem that is not finished. The failure is not writing them; it is writing
them indistinguishably from real ones.

> **When you pin a value you know to be wrong, say so on the line. The next
> session must be able to tell "this is the contract" from "this is the gap"
> without reconstructing your reasoning.**

Worked case, §243: two pinned values came due in the same build. Both had been
written with the note *"the pin was the gap, not the answer"* beside them, so
updating `("a", "error")` to `("a", "1")` and `("A", "error")` to
`("A", "E.A")` took no judgement at all — the previous author had already
recorded which kind of assertion it was. Without those notes the same diff is a
decision about whether a contract is being broken, made under time pressure by
someone who did not write it.

**The second half is sharper and cost a near-miss to find.** Of those two pins,
only the `const` one discriminates the rule. `import x = M.a` moves
`error → 1` because a const's *declared* type is the error type, so the answer
falls through to the value type; `import q = E.A` moves `error → E.A` because an
enum member's declared type is not, so the fallback never runs — and it would
read `E.A` under a collapsed "just answer the value type" rule too. So:

> **A pin that would still pass under the rule you are trying to exclude is not
> evidence.** State which mutation each assertion is supposed to catch, or you
> cannot know whether you have one control or none.

This is the same defect as §215 (a function widened off the call path, +0) and
§242 (a refusal that was right about the count and wrong about the question):
something that looks like a check but cannot fail in the direction that matters.

How you would know this is wrong: if the mutation the pin claims to catch does
redden it. That is a one-command check and it is the only thing that converts a
pin from decoration into a control.

### Corollary 24 (2026-08-12): a per-case tally cannot see anything that happens inside an already-failing case

`casedelta` and the near-miss board count **matched** lines. A line that was
already wrong and becomes *differently* wrong moves neither. So the whole class
of regressions where a **gap turns into a confident wrong answer** is invisible
to the instrument both lanes reach for first.

§219 is the demonstration. Removing a documented refusal in `module_object_of`
— to follow `export =`, which is upstream's own first line in
`resolveESModuleSymbol` (`checker.go:15569`) — measured **+4 cases, −0, +40
lines, every movement positive**. On the tally alone it was a clean win. It was
reverted, because three controls written *before* the tally was read showed the
change printed `typeof __React` where every tsx baseline records
`typeof React`, and walked straight past the two-alias gap — a gap that exists
because the corpus contradicts every tie-break, so bypassing it manufactures
exactly the confident wrong name the gap was protecting against.

**The rule.** When an arm's product is a *name* — a module object, a qualifier,
an alias, a type-parameter spelling — the per-case tally is necessary and not
sufficient. Write a control that pins the **spelling** and a control that pins
the **gap**, and write both before you run the measurement. Afterwards there is
no longer a felt reason to write them: the number has already answered.

This does not apply to arms whose product is a *type*. §220, one commit later,
turned a decline into `Generator<any, void, unknown>` and +5/−0 settled it —
a wrong type is a wrong line and the tally counts it.

**How you would know this is wrong:** if an instrument existed that diffed the
*text* of every non-matching line across a change, the tally's blind spot would
close and the controls could follow the measurement instead of preceding it.
Nothing does that today; `nearmiss --case` does it for one case at a time.

#### Widened the same day, by §222 — the blind spot is bigger than the heading said

The heading first read *"cannot see a gap becoming a wrong answer"*. That is one
instance of the real limit, which is structural: **a failing case contributes
nothing either way, so every line inside it moves for free.** Gaps becoming
wrong answers is one thing that can happen there. So is a **right answer
becoming wrong**, and that is strictly worse.

§222 is the demonstration. A three-line transcription of
`getTargetOfNamespaceExportDeclaration` (`checker.go:15011`) — no judgement
anywhere in the arm, and this port's `resolve_external_module_symbol` already
matched its `dontResolveAlias=true` semantics exactly — measured **+1 case,
−0**. It also took **−14 lines in `conformance/umd-augmentation-1` alone**,
including `>m : typeof m` going **right → `error`**. The case was already
failing and stayed failing, so the entire regression was invisible to the
per-case report.

The operative habit is therefore not "write controls for name-shaped arms" but:
**read the line movement beside the case delta, always.** `casedelta`'s
`matched` column gives it for free and both §219 and §222 were caught by it.

And a faithful transcription of a short upstream function is exactly the change
one is least tempted to check — there is no judgement in it to doubt. The
judgement is in whether the rest of the port can afford the answer.

### Corollary 25 (2026-08-12): a delta can be true of two arms and evidence for neither

Corollary 24 says the instrument cannot see inside a failing case. This one is
about the cases it *can* see, and it is worse, because here the number is
correct and still means nothing.

> **Before trusting a delta, name the input that would distinguish your arm from
> the plausible wrong one, and check whether any fixture contains it. If none
> does, the number is evidence for the fixtures, not for the rule.**

Worked case, and it is a true collision. Both lanes built the bare-`yield`
no-strict arm within an hour, from the same §135 deferral note, against the same
witness, and **both measured +5 on the same cases**. They are not the same arm:

- **§220** contributes `any` at the site, because upstream's bare yield
  contributes `undefinedWideningType` and `getWidenedType` maps that to `any`.
- **§245** contributed `undefined` and widened the *aggregate*, on the reading
  that `getReturnTypeFromBody` widens at the end.

The discriminating input is a **mixed** generator, `function* f() { yield; yield 1 }`
under no-strict. §220 aggregates `{any, number}` → `any`, which is upstream's
answer. §245 aggregates `{undefined, number}` → `number | undefined`, and its
aggregate widening does not fire because the union is not itself the undefined
type. A wrong answer, not a partial one.

**No fixture in the row contains a mixed yield.** The census was therefore
structurally incapable of separating a correct arm from an incorrect one, and
+5 would have shipped the wrong one green. §245 was withdrawn on inspection of
the collision, not on any measurement — no measurement available could have
done it.

The check is cheap and mechanical, and it is *not* "write more tests": it is one
question asked before reading the number. What input separates this rule from
the nearest wrong rule? Then grep the corpus for it. A row of fixtures that all
exercise the easy half of a rule is the normal case, not the exceptional one —
fixtures are written to demonstrate features, not to discriminate between two
candidate implementations of them.

How you would know this is wrong: if the discriminating input turns out to be in
the corpus after all, the delta *was* evidence and the worry was unfounded. That
is a one-command check, which is the whole point.

### Corollary 26 (2026-08-12): a conflict resolution that succeeds tells you nothing about whether it was right

Two lanes rebasing continuously produce conflicts several times an hour, and the
resolution step has no gate behind it. Tests pass, clippy passes, the rebase
completes. Nothing in the pipeline re-derives what the resolution should have
been, so a wrong one ships looking exactly like a right one.

Three forms, each caught in one session, each by luck rather than by a check:

**A generated file has exactly one correct resolution: regenerate it.** Taking
either side of a conflict in `crates/tsr-conformance/snapshots/*.snap`, or
splicing them, produces a snapshot corresponding to **no tree that has ever
existed**. It reads perfectly plausibly. Clear the markers only far enough to
finish the rebase, then throw that away and run the generator. This came up
twice in one session on the same file.

**Count the conflict regions before resolving any of them.** A script that
strips markers and keeps one side operates on *every* region in the file. The
one you read is not necessarily the only one — a STATUS conflict resolved this
way silently also resolved a second region belonging to an unrelated commit.
`grep -c '^<<<<<<<'` first; if the count is more than one, resolve them
individually. (That grep *had* printed both regions; the failure was acting on
the first without counting.)

**Prose is content.** `git commit -m` with backticks in the message runs command
substitution and **deletes** every backticked span. The commit succeeds and the
loss is invisible unless the message is read back. Use `git commit -F <file>`
for anything containing backticks — which, in this project, is anything worth
writing.

> **Amended the same day, by the author of this corollary violating it four
> commits later.** The rule above says "for anything containing backticks",
> which makes it *conditional on noticing* — and noticing is the part that
> fails, because by then you are thinking about the commit rather than about
> the shell. §228's message lost the span `` `,` ``, which was the subject of
> its own sentence, from a paragraph explaining why a comma reaches upstream's
> third recovery arm. `zsh` printed `command not found: ,` in the middle of the
> push output.
>
> **So the habit is unconditional: never invoke `git commit -m`.** Write the
> message to a file and use `-F`, every time, including for one-liners.
> Knowing the hazard, having written it down, and having cited it to someone
> else the same hour were all insufficient — which is the strongest evidence
> available that a conditional rule was the wrong shape.

The unifying property is the one that makes all three hard to notice: **the
success signal is real and the failure is silent.** Same family as
`rtk`-piped exit codes (memory: `rtk-masks-exit-codes`) and corollary 24's
already-failing case. Wherever that pattern holds, the only defence is to read
back the artefact rather than infer it from the command's success.

### Corollary 27 (2026-08-12): a zero from an instrument you have not confirmed fires is not a measurement

Four builds in this window produced a number that looked like data and was not.
They are the same defect seen from four sides, and the fourth is the dangerous
one.

| face | the thing that was off | what it reports |
|---|---|---|
| §215 | a function off the **call path** | `+0` |
| §244 | an arm off the **answer path** | `+0` |
| §247's first probe | a probe off the **visit path** | `answered=false` |
| §247's second probe | an instrument off its **scope** | *a plausible distribution* |

The first three report nothing, and nothing reads as absence. **The fourth
reported "3 of 6 witnesses, with case names attached", which reads as a
finding.** It was `head -1` of a corpus-wide stderr stream, so the three
successes and three failures were whichever heritage entry the corpus reached
first — no per-case signal existed at all.

> **"Does it fire" and "is it scoped to what I am attributing it to" are separate
> questions, and only the first one feels like it needs asking.**

**The stopping rule**, which makes the set actionable and would have saved the
hour:

> **A discriminator that does not discriminate between two byte-identical inputs
> is a broken instrument, not a subtle finding.**

`checkJsxChildrenProperty5` and `checkJsxChildrenProperty8` have byte-identical
heritage lines, imports and directives, and the probe reported different
outcomes for them. The response to that must be to doubt the instrument. What
actually happened was a hunt for a hidden difference — a BOM was found in one
failing fixture, tested against all twelve, and correctly discarded — and only
then was the instrument questioned. The hypothesis was disciplined and the step
was unnecessary.

**The mechanism, which is worth separating from the instances.** `nearmiss
--case <name>` filtered its *output* and executed the whole corpus. It had been
used a dozen times that day for reading one case's diff, which it does
perfectly; it lies only when asked to scope *execution*, and then it lies
plausibly.

> **A tool whose name overstates its scope will eventually be trusted for the
> scope it claims** — and it survives because the correct use and the incorrect
> use are indistinguishable from outside.

Same silent-success family as corollary 26, one layer up: the success signal is
real and the failure is silent. (Fixed: the filter now sits above the measure,
and one case takes 0.5s instead of a full corpus run.)

**A fifth face, and the cheapest one to avoid.** Twice in one window a handoff
quoted upstream *accurately* and attributed the mechanism *wrongly*, and both
times the real trigger was in the **writer** rather than the checker: the
extends-clause base type (`getTypeOfNode`'s arm is real, but it is reached from
`type_symbol_baseline.go:370-374` via the parent, not by visiting the node) and
the type-alias name (`IsTypeDeclarationName` is real, but the line is produced by
a re-render guard at the bottom of `writeTypeOrSymbol`).

> **When the symptom is "the baseline prints something we don't", start in the
> writer.** The checker's version of the rule is the one that greps well; the
> writer's is the one that fires.

**The probe as a control source, which changes what the corpus is for.** Upstream
can be executed on a fixture you write — drop it in
`vendor/typescript-go/testdata/tests/cases/compiler/`, run
`go test ./internal/testrunner -run TestLocal`, read the generated `.types`, then
delete both and verify the submodule clean *including untracked*. (The run
"fails" by design; `new baseline created` is the success signal — corollary 26's
family, read the artefact not the status.)

This dissolves the hardest part of corollary 25. That corollary asks whether the
corpus contains the input separating your rule from the nearest wrong one, and
its uncomfortable answer is often *no*. **A written probe supplies controls the
corpus structurally cannot.** Two from one window:

- Five alias positions in one fixture — top level, labelled, inside `f<U>`,
  inside a **non-generic** `g()`, inside a namespace. The non-generic case killed
  the genericity hypothesis and the namespace case killed the nesting
  hypothesis, in a single run. The corpus contains exactly one indented
  `type X = {}` and cannot pose the question at all.
- One symbol referenced from two sites three lines apart (`x;` and
  `export = x;`), answering `undefined` and `any`. That eliminated every
  symbol-level explanation at once. No corpus fixture has both positions on one
  symbol.

The corollary-21 consequence is worth stating plainly, because both lanes got it
wrong in the same window: **"there is only one witness in the corpus" stops being
a reason to stop and becomes a reason to write a second one.** Corollary 21 says
do not call one member a family; it does not say do not *make* a second member.
Invoking a rule against over-generalising to license under-investigating is a
misuse of it.

**And a wrapper can narrow a query silently.** `global_type_symbol(name)` delegates
to `global_type_symbol_with_arity(name, 1)`. Called for `Iterable`, which the
modern lib declares as `Iterable<T, TReturn = undefined, TNext = any>`, it
answers `None` — meaning *"no such type at arity 1"* while reading as *"no such
type"*. An arm built on it never fired and measured a clean `+0`. Same shape as
the `--case` mode above: the name promises a scope the implementation does not
have, and the failure surfaces as a plausible negative rather than an error.

How you would know this is wrong: run the instrument against an input whose
answer you already know, including a *negative* input. An instrument that cannot
produce a known-false is not measuring.

### Corollary 28 (2026-08-12): a static read that predicts what the port already does is a false premise, not a hard puzzle

Corollary 27 is `checker-2`'s and covers the general capability — a probe
fixture through upstream's own baseline runner supplies controls the corpus
structurally cannot. This is the narrower situation that should *trigger*
reaching for it.

You are diagnosing a wrong line. You read upstream carefully. Your read predicts
the behaviour **this port already has**, and the baseline says something else.

> **That is not a subtle case. It is a false premise in your own reading, and
> premises are cheaper to test than to re-derive.** Stop reading and run
> upstream.

The failure it prevents is not getting the answer wrong — it is spending the
effort somewhere it cannot succeed. A careful read that lands on the port's
current behaviour has already told you the read is broken; continuing to read
more carefully cannot fix a broken premise, and the feeling of *nearly* having
it is exactly what keeps the loop going.

**The worked case is §227/§230.** `var x;` referenced from `export = x` wants
`any` and this port answered `undefined`. Reading `checker.go:11149-11190`
predicted `undefined`: `t` is `autoType`, which disables the entire
`t != autoType && …` disjunct group, no other disjunct of `assumeInitialized`
applies, so `initialType` is `undefinedType` and neither final branch fires.
That reading was done twice, was written into a code comment and a STATUS
entry as *an open question needing upstream executed*, and both times the
contradiction was recorded rather than resolved.

It took one probe fixture and about five minutes:

```text
x;              >x : undefined
export = x;     >x : any
```

One symbol, two sites, two answers, three lines apart — which eliminates every
symbol-level disjunct simultaneously, since none can vary by reference site.
**+4 cases** (§230), and the arm is four lines.

**"This needs upstream executed" was written twice as though it were a
blocker.** It was never priced. So the corollary has a second half:

> **When you catch yourself recording something as needing an instrument you do
> not have, price the instrument before writing that down.** Two entries in this
> session deferred to a tool that already existed and cost five minutes.

*How you would know this is wrong:* if a probe run were expensive — minutes of
build per question, or an instrumented fork to maintain — then deferring would
be rational and the reading would be the cheaper path. Re-price it if that
changes.

### Corollary 29 (2026-08-12): is the refusal's scope the same as its reason's scope?

Corollary 16 asks whether a refusal's stated reason is *real* — whether it
describes something that actually goes wrong. This is the question that comes
after, and it bites refusals that pass 16 cleanly:

> **A refusal states a reason and covers a construct. Those are two different
> sets, and nothing checks that they are the same one.**

A reason is written about the case in front of you. The refusal is written
against the syntax you were looking at. The gap between them is invisible
because everything you measured is inside it — the measurement confirms the
reason, the reason justifies the refusal, and the population the reason has
nothing to say about is never sampled.

**Worked case, §219 → §232.** `module_object_of` refused to follow `export =`,
because following it hands the printer a `declare namespace __React` and prints
`typeof __React` where every tsx baseline records `typeof React`. That reason is
correct, was measured, and is about **naming a module object**. The refusal
covered the whole construct — including `export = a` over `var a = 10`, which
resolves to a plain variable, answers `number`, and prints no name at all.
There was nothing there for the reason to object to.

Splitting the guard on the resolved target's module flags: **+4 cases, 0 lost,
18 lines wrong→right and 0 right→wrong.** The hazard the reason names is
untouched, because that population still declines.

Note what did *not* find it. The refusal had a measurement (+4/−0/+40 lines), a
named mechanism, three controls, and a doc comment sizing the follow-on work.
All of it was right. It took another reader asking whether the witnesses the
reason cited were the same witnesses the guard caught.

**The mechanical form of the question**, worth asking of any refusal you write:

- name the population the reason is *about*;
- name the population the guard *covers*;
- if the second is larger, the difference is unmeasured surface being refused
  for a reason that does not reach it.

**And a second-order error to watch, because it followed immediately.** The
sizing derived from the wide refusal was also wrong: `bd tsr-e2u` was recorded
at ~11 cases because removing the *whole* guard moved 11, and all of it was
attributed to the one cause the guard named. It is ~7. **A refusal's price is
measured on its scope, so an over-wide refusal over-prices the work behind
it** — the same error one level up, and the one that decides what gets built
next.

**Write the refusal pin anyway, even when the refusal feels permanent.** §219's
pin asserted a decline that looked settled for good. One commit later it was the
regression test that caught §232's first draft reading module flags off the
`export=` *alias* symbol — which carries `ALIAS`, never a module flag — letting
exactly the case §219 exists to prevent through, printing `typeof __X`. A pin
written to assert a refusal became the regression test for that refusal's own
narrowing. That is not foreseeable when you write it, which is the argument for
writing it.

*How you would know this is wrong:* if narrowing a refusal to its reason
routinely turned up nothing, the wide form would be free and this would be
ceremony. Two attempts, two hits (§232 here; conventions corollary 11 is the
same shape one layer in, a shared helper whose conservatism belonged to its
first caller). Re-price if that stops holding.

### Corollary 30 (2026-08-12): a disjunction is the easiest thing to under-port

Three arms in one session were the same defect, and the defect has a shape that can
be checked mechanically rather than noticed:

| arm | upstream | what was ported | cost |
|---|---|---|---:|
| §204 | `!IsPropertyAccessOrQualifiedName(node.Parent)` — a test on the parent | a match on the property-access **name** position | +10 |
| §252 | `getIndexInfosOfSymbol` walks the symbol's members | two of the three declaration **carriers** | +6 |
| §253 | `Name().Kind == KindStringLiteral \|\| IsGlobalScopeAugmentation(node)` | the **first disjunct** | +6 |

> **When a ported predicate is a disjunction upstream, count the `\|\|` in the Go and
> count the arms in the Rust. A missing disjunct passes every test you think to
> write, because the disjunct you ported is the one you were thinking about.**

This is corollary 20's mechanism rather than another instance of it. Twenty says a
port longer than the predicate has encoded the witness; thirty says *why* the
missing part stays missing. The first disjunct covers the common case — that is
generally why it is written first — so the ported half is right about everything
the author had in mind, every fixture they reach for, and every control they think
to add. **The second disjunct's population is by definition the one that was not
being considered.**

`declare global { … }` is the cleanest illustration. It *is* an ambient module
upstream, by the second half of a two-clause test, and nothing about the first half
hints that a module's name might not be a string literal. The port was correct for
every ambient module anyone would think to write down.

Why this one is worth having when so many rules are judgement: it needs no
understanding of the subject matter. `grep '||'` in the upstream function, count
the branches in the port, compare. It applies to `&&` chains read as filters too —
§252's collector was a three-way match with one arm absent.

How you would know this is wrong: if the missing disjuncts turn out to cover
populations the corpus never exercises, the count is a cheap check that buys
nothing and the real work is elsewhere. Measured so far: three for three, 22 cases.
