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
if one does not. Pointed at a newer checkout with `--upstream`, the same check *is*
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
