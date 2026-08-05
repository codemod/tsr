# ADR-0034: A program needs one identity space

**Status:** accepted
**Date:** 2026-08-05
**Upstream pinned at:** `5b1047d10`

## Context

`bd tsr-9or.1` merged two items — "load lib files" and "check a multi-file case
as one program" — on the finding that they are one missing object. Measured at
`78cfcba` over the 468,921 assertion lines the `.types` walker aligns:

| | lines | share |
|---|---|---|
| a free name that resolves to nothing | 10,535 | 3.73% of gaps |
| type-position names that resolve to nothing, over 306 distinct names | 8,229 | — |
| of those, lib types (`Promise` 640, `Object` 369, `Array` 304, `Record` 212, `Number` 189, `Partial` 142, `Readonly` 104, `Iterable` 89) | 2,054 | — |
| lines in multi-file cases (1,091 of 9,538 judged cases) | 26,479 | 5.65% |

Multi-file cases read 24.88% right against 36.95% overall. The `T[]` bucket —
12,491 lines, still 0.00% — is the same request again, because upstream models
an array type as a reference to the global `Array` interface.

This ADR records what was found when the object was built: **the missing
object was two objects, and only one of them was missing.**

## The forcing constraint

The program was built (see `crates/tsr-compiler/src/lib.rs`,
[ADR-0017](0017-program-before-tsconfig.md)). It now loads the bundled
`lib.*.d.ts`, binds them, and holds every file of a multi-file case together.
`Array`, `Object` and `String` are bound symbols inside it — there is a test
that reads the real shipped `lib.es5.d.ts` and asserts exactly that.

**And no other file can see them.** The reason is not the absence of a globals
table. It is that a symbol has no identity outside the file that produced it:

- `tsr_binder::SymbolId` is a `u32` index into a per-file `SymbolStore`
  (`crates/tsr-binder/src/symbol.rs:22`, `:268`). Two files both produce
  `SymbolId(7)`.
- A `Symbol`'s `declarations` and `value_declaration` are `NodeId`s indexing a
  per-file `NodeTable`. Reading a foreign symbol's declaration indexes the wrong
  table.
- `Checker` holds exactly one of each — `binder: &BindResult`, `nodes:
  &NodeTable`, `node_map: &NodeMap` (`crates/tsr-checker/src/checker.rs:40-90`)
  — and reaches the binder at 13 sites, all through `binder.resolve`,
  `binder.symbol_of`, or `binder.symbols().get(id)`.

So the obvious next step — hand the checker a `name -> SymbolId` map covering the
program's other files — is not incomplete, it is **unsound**. Every consumer of
the returned id would read a different file's symbol and then index this file's
nodes with its declaration. It would print plausible types for the wrong
declarations, which is the one failure mode this port is organised to prevent:
a gap must be distinguishable from an answer.

Upstream has no such problem and therefore no such decision to record.
`ast.Symbol` is a pointer and `Symbol.Declarations` are pointers, so
`initializeChecker` (`internal/checker/checker.go:1296`) can walk every
non-module file's `Locals` and merge them into `c.globals`
(`internal/checker/checker.go:930`) without any of it meaning anything
file-relative. The port turned those pointers into indices
([ADR-0003](0003-tree-plus-side-tables.md)) and inherited an invariant upstream
never had to state: **an index is only meaningful beside the table it indexes.**

## Decision

**Widen the identity space to the program, rather than teach every consumer
about files.**

A program-wide identity means: every file of one program parses into one
`NodeTable`/`NodeMap` and binds into one `SymbolStore`, so a `NodeId` and a
`SymbolId` name one thing across the whole program. `BindResult::resolve` then
needs one further step — when the walk falls off a source file's root, consult a
merged globals table built as `initializeChecker` builds `c.globals`.

This ADR **records the decision, not its implementation.** Nothing in this
commit widens the identity space; the work lands in `tsr-binder` and
`tsr-parser` and is tracked separately. What this commit does is build the
program the widening is for, and add the test that will change its answer when
it happens (`a_lib_files_symbols_are_still_the_lib_files_own`).

## Alternatives, taken seriously

### File-tagged identity: `(FileId, SymbolId)` everywhere

Give the checker a `Vec` of files and make every symbol reference carry the file
it belongs to.

This is the option that requires no change below the checker, and it is what a
reader who knows upstream's `*ast.Symbol` would reach for first, since a pointer
*is* a global identity. It loses because the cost lands in the wrong place:
`Checker`'s three borrowed fields become three indexed collections, and every
one of the 13 binder call sites plus every future one has to name a file. The
checker is the part of this port that will grow from 2,165 lines to something
closer to upstream's 60,269, and this option taxes every line of that growth.

**It would win if** files could not share one arena — if, say, incremental
re-checking needed to replace one file's parse without touching the others'.
That is a real requirement for a language service, and it is the reason this
option is recorded rather than dismissed. Today nothing needs it: a program is
built, checked, and dropped.

### Merge nothing; give the checker a name-keyed table of foreign symbols

Rejected as unsound, above. Recorded because it is cheap, it would raise the
numbers, and the numbers would be wrong in a way no gate here can see.

### Concatenate a program's files into one source text

Would make one `NodeTable` and one `SymbolStore` trivially. Rejected: it answers
a question upstream does not ask. Spans, file names, diagnostics, module-vs-
script classification, and `.types` baseline sections are all per file, and every
one of them is observable in an oracle.

## Consequences accepted

- **The lib files are loaded and inert.** A program built with
  `Program::from_root_files` holds `lib.es5.d.ts`, parsed and bound, and answers
  no more names than it did before. That is worth stating plainly, because a
  changelog entry saying "lib files are now loaded" would otherwise be read as
  "globals now resolve".
- **The 10,535 unresolved-name figure will not move on this commit.** Nor will
  the 12,491-line array bucket. Neither was expected to; both are behind the
  widening, not behind the loading.
- **A second parse of the lib files is now avoided but a shared one is not
  possible.** `ProgramFile` is `Send` and deliberately not `Sync`
  (`crates/tsr-compiler/src/file.rs:88`) — the arena's bump pointer is a `Cell`
  and the bind result's storage is a `OnceCell` — so one bound `lib.dom.d.ts`
  cannot be shared across the programs of a parallel corpus run. At 3.9 MB of
  lib text per program, that is the reason the conformance `.types` producer was
  **not** rewired to build a program in this commit. See
  [architecture/program.md](../architecture/program.md).

## How we would know this was wrong

- If widening the identity space turns out to need a `SymbolId` wider than
  `u32`, or if arena sharing across a program's files measurably worsens
  parse-phase memory in `docs/architecture/performance.md`'s numbers, the
  cost calculus above is wrong and file-tagged identity is the cheaper option
  after all.
- If a language service or incremental build lands before the widening does,
  the "nothing needs per-file replacement" premise is false and this decision
  should be superseded rather than implemented.
- If, after the widening, the unresolved-name count falls by markedly less than
  the 10,535 + 8,229 lines this ADR attributes to it, then those lines were
  never blocked on identity and the ranking that produced `bd tsr-9or.1` was
  measuring something else.
