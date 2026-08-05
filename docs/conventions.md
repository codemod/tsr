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
