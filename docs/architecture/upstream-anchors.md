# Upstream anchors, and the tool that checks them

**Status:** `bd tsr-5e7.3` (the lint) is done and gating CI. `bd tsr-l68` (the
drift tracker) has its mechanism — the same tool, pointed at a newer checkout —
and not yet its schedule.

**Command:** `cargo xtask anchors [--upstream <path>]`.
**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`.

## What an anchor is for

[ADR-0001](../adr/0001-idiomatic-rewrite.md) chose an idiomatic rewrite, so an
upstream fix cannot be diffed and replayed onto this tree — each one has to be
re-derived by hand. [conventions.md](../conventions.md) therefore requires every
ported item to name its typescript-go counterpart, because that is what lets a
commit touching `internal/checker/relations.go` resolve to the Rust items claiming
to port it.

**An anchor nobody checks is worse than no anchor, because it is believed.** The
slice that produced `tsr-declarations` also produced three unverified claims about
upstream — a resolver-method count wrong by 3×, a line number pointing at a blank
line, and a crate-doc claim that "every child list goes through `emit_list`" that
two call sites contradicted. All three were caught by a person reading, which does
not scale to 516 anchors.

## One tool, two modes

The check is one question — *does this anchor resolve?* — asked against two trees:

| | upstream tree | an unresolvable anchor means |
|---|---|---|
| **lint** | the pinned submodule | the anchor was wrong when it was written |
| **drift** | a newer checkout, via `--upstream` | upstream moved, and *this item* is affected |

That is why they are one program rather than two. `bd tsr-l68` originally sketched
a drift tracker that classified upstream commits by the `internal/` package they
touch and filed an issue per package. That is coarse: a commit touching
`internal/checker/relations.go` would file one issue against a whole crate. This
resolves to the item, because the anchor already says which item claims what.

## What it checks

Three kinds of claim, extracted from anchor comments:

| Claim | Example | Verified by |
|---|---|---|
| A file | `` `internal/printer/printer.go` `` | the file exists |
| A file and line | `` `internal/printer/printer.go:4745` `` | the file is at least that long |
| A Go declaration | `` `ast.SourceFile` ``, `` `Printer.emitList` `` | an index of every `func`, `type`, `const`, `var` and method under `internal/` |

Struct **fields** are deliberately not indexed. A field named `Name` or `Kind`
exists in dozens of structs, so indexing them would make almost any symbol resolve
and the check would pass vacuously. The cost is that an anchor naming a field is
invisible to the tool; the alternative is a check that cannot fail.

### The claims it will not make

Only what the anchor *phrase* introduces — the first backticked span after
`Ported from` / `Corresponds to` / `Stands in for` — plus any `.go` path in the
same anchor. Everything else in the comment is ignored.

That rule was arrived at the hard way. An earlier version classified every
backticked span in an anchor comment and reported 42 failures: `TS9017`,
`package.json`, `None`, `.errors.txt`, and half a dozen conformance case names,
none of them a claim about Go. This module's own documentation says a gate that
always fails is turned off within a day — and a check that doubts prose is that
gate.

A related trap in the same pass: `PHRASINGS` briefly included `"ports "`, which is
a substring of *reports*, *supports*, *imports* and *exports*. It turned ordinary
sentences into anchors and then disbelieved their backticks.

## What it found on its first run

696 references checked, **4 broken** — every one of them real:

| | |
|---|---|
| `tsr-printer/src/lib.rs:586` | `Printer.emitClassBody` does not exist upstream. Both call sites inline the braces; the anchor named a function someone expected to be there |
| `tsr-core/src/span.rs:3` | `internal/core/textrange.go` — the type is in `text.go` |
| `tsr-declarations/src/modifiers.rs:3` | `.../util.go`, an elided path that resolves to nothing |
| `tsr-declarations/src/factory.rs:3` | `internal/ast/factory.go` — `NodeFactory` is in `ast_generated.go` |

Three of the four were written in the preceding session, by someone who had the
upstream tree open at the time. That is the argument for the tool in one line.

## Coverage is a rate, not a gate

`bd tsr-5e7.3` asks for a lint that "fails CI on unanchored public items".
Measured by the tool, `tsr-parser` carries 6 anchors against 125 public items and
`tsr-binder` 5 against 102. A gate failing on every unanchored item would fail
hundreds of times on its first run, and a gate that always fails gets deleted.

(A first draft of this paragraph said "8 anchors across 8,648 lines" and "13
across 5,407", from a `grep` run before the tool existed. That grep counted
`"ports "` as an anchor phrase — see below — and so counted *reports*, *supports*
and *imports*. The numbers above are the tool's.)

So the two halves are separated on purpose:

- **Broken anchors fail the build.** There are few, they are always bugs, and the
  number is zero as of this commit.
- **Missing anchors are reported per crate**, the way the conformance suites report
  a rate — a ratchet to raise, not a gate to trip.

```
crate                    anchors   public items anchored
tsr-ast                      272              3/21 (14%)
tsr-printer                  151              32/90 (36%)
tsr-declarations              43              17/39 (44%)
tsr-scanner                    9              6/48 (12%)
tsr-parser                     6             0/125 (0%)
tsr-binder                     5             0/102 (0%)
tsr-module                     5             0/104 (0%)
tsr-dts                        0                0/7 (0%)
```

(Abridged; `cargo xtask anchors` prints all sixteen crates. These figures are the
tool's own output — an earlier draft of this table carried numbers written from
memory of a previous run, which is the exact failure the tool exists to catch and
was caught by re-running it before committing.)

`tsr-ast`'s 272 anchors are generated, which is why its count is high and its rate
is not: only non-generated files contribute to the coverage column, because
regeneration is what keeps generated anchors honest.

`tsr-dts` shows zero anchors and that is correct rather than a gap —
[ADR-0021](../adr/0021-isolated-declarations-is-not-a-port.md) removed the usual
rule for that crate, because it has no upstream counterpart to name. It anchors to
`TS9xxx` diagnostic codes instead, which this tool deliberately does not treat as
Go declarations.

The two columns disagree for a reason worth knowing: an anchor on a module (`//!`)
or on an inner function is counted in the first column and cannot be counted in
the second, which only looks at the doc comment directly above a `pub`/`pub(crate)`
item. `tsr-printer` at 151 anchors and 90 public items is the clearest case — most
of its anchors sit on `match` arms, which is where its dispatch lives.

## The drift mode, demonstrated

Not claimed — run. Against a copy of the pinned tree with two files renamed, to
simulate upstream moving them:

```
$ mv internal/core/text.go                          internal/core/textrange_renamed.go
$ mv internal/transformers/declarations/util.go     internal/transformers/declarations/helpers.go
$ cargo xtask anchors --upstream <that tree>

696 upstream references checked, 5 unresolved
  crates/tsr-core/src/span.rs:3:            no such file upstream: `internal/core/text.go`
  crates/tsr-declarations/src/modifiers.rs:3:    …/declarations/util.go
  crates/tsr-declarations/src/modifiers.rs:81:   …/declarations/util.go
  crates/tsr-declarations/src/transform.rs:1129: …/declarations/util.go
  crates/tsr-declarations/src/transform.rs:1134: …/declarations/util.go
```

Two upstream renames, five precisely-located Rust sites across three files. The
package-classifying design `bd tsr-l68` originally sketched would have produced
"something in `internal/core` and `internal/transformers` changed" and left the
rest as archaeology.

## What is left for the drift tracker

The mechanism works today. What `bd tsr-l68` still
needs is the part around it — a scheduled job that fetches a current
typescript-go, runs this, and files a `bd` issue per broken anchor, idempotently.
That is CI plumbing (`bd tsr-cmh.1` is the neighbouring piece) rather than
analysis, and it is filed rather than claimed here.

One limitation to close first: a *renamed* symbol resolves to nothing, which the
tool reports, but a symbol that upstream *deleted and re-added elsewhere* resolves
fine and reports nothing. Catching that needs the file to be part of the claim,
which is why the `path` half of an anchor earns its keep and why anchors that name
only a symbol are the weaker kind.
