# ADR-0034: A program needs one identity space

**Status:** accepted
**Date:** 2026-08-05
**Upstream pinned at:** `5b1047d10`

## Context

`bd tsr-9or.1` merged two items — "load lib files" and "check a multi-file case
as one program" — on the finding that they are one missing object. Measured over
the 468,921 assertion lines the `.types` walker aligns:

| | lines | share |
|---|---|---|
| unresolved names lib declares, **value** position | 3,126 | — |
| unresolved names lib declares, **type** position | 3,210 | — |
| the array bucket (`T[]`, still 0.00% right) | 12,051 | — |
| **lib's direct effect** | **18,387** | 6.52% of gap lines |
| lines in multi-file cases (1,091 of 9,538 judged cases) | 26,479 | 5.65% |

Multi-file cases read 24.88% right against 36.95% overall. The array bucket is
part of this and not a separate item, because upstream models an array type as a
reference to the global `Array` interface.

> **These numbers were corrected on 2026-08-05, after this ADR was first
> written.** The figures it originally carried — 10,535 lines, 3.73% of gaps —
> came from a roll-up row in `examples/types_shapes.rs` whose
> `contains("annotation")` branch sits four tests before its
> `contains("does not resolve")` branch, so every unresolved name in *type*
> position was filed elsewhere and never reached the row labelled for this
> issue. The row overstated lib 3.4× within itself (7,409 of its 10,535 lines
> are names lib does not declare — `undefined`, `div`, `a`, `b`, `x`, `_`) and
> omitted 15,261 lib lines outside it; net, 1.75× too small. Measured and
> recorded in `2f6f0bf`; see `docs/architecture/checker-oracle.md`, "The second
> instrumentation finding".
>
> **18,387 is an upper bound.** Lib names are matched by "keyword at column 0"
> rather than by parsing, and over-reporting is the direction that flatters the
> argument. Re-taking it against a real parse is an early use for the program
> this ADR is about.
>
> One consequence is load-bearing for the decision below: the observation that
> "the unresolved-name count did not move across two ported layers" was **never
> evidence of a ceiling**. The counter sat where those lines could not arrive.

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

### What that costs, stated before it is built

Two consequences follow from "one `NodeTable`" that are easy to miss when the
decision is argued at the level of ids, and both are larger than the id change
itself.

**One `NodeTable` across files implies one arena across files.** `NodeMap<'a>`
borrows typed nodes from the parse arena
([ADR-0033](0033-the-parser-fills-the-node-map.md)), so program-wide `NodeId`s
mean every file parses into one allocator. That is a `tsr-parser` change, and
its blast radius reaches past the checker: the printer, the declaration
transform, and every parse-only conformance suite consume a parse result today
and would have to consume a program-of-one-file instead. The expectation is that
they degrade to exactly that and change by nothing. **That is a prediction, not
a finding.** ADR-0033 exists because this repository once predicted a fill was
free and measured +16.1%.

*Measured so far:* zero. Every one of those crates builds and tests green
across the parser and binder halves of this widening, because the single-file
entry points (`parse_with_options`, `bind`) kept their signatures and are now
defined as the shared-table ones over fresh tables.

**Discharged 2026-08-05**, when `Program` took the shared arena. The prediction
held: the printer, the declaration transform and every parse-only conformance
suite were not touched and did not move, because none of them goes through
`Program` — they call `parse_with_options`, which is still exactly `parse_into`
over a fresh pair of tables. The blast radius the prediction feared turned out
to stop at `tsr-compiler`'s own two consumers in `tsr-conformance`
(`binder_suite`, `loader_suite`), plus the *fourth* cost below, which the
prediction did not name at all.

### Who owns the arena: the caller

Decided 2026-08-05, after the alternatives met the code.

```rust
let arena   = tsr_core::Arena::new();
let program = Program::load(&arena, host, options);   // Program<'a> borrows
```

Not the loader and not the `Program`. **The arena's extent is the compilation**,
and both of those are borrowers: handing ownership from the loader to the
program would encode a transfer that has no meaning, since the loader does not
stop needing the memory, it stops existing. It is also how the conformance
harness must call it anyway — 12,444 cases each want an arena scoped to the
case and dropped at its end, which is a scope under this design and an ownership
dance under any other.

**The file texts and names live in the arena too, and this is what makes the
design work.** A `Symbol`'s name borrows the source text
(`FileInfo { name: &'a str, text: &'a str }`) — which is *why*
`crates/tsr-compiler/src/file.rs` owns each file's text inside its `self_cell`
today. The arena was never the only thing that cell was holding for. Under
caller-ownership the loader reads a file from the host and copies its text and
name into the arena with `Arena::alloc_str` (`crates/tsr-core/src/arena.rs:159`),
after which every borrow points at the arena alone. The cost is one extra copy
of text we already hold in memory.

With that, **`self_cell` leaves `ProgramFile` entirely rather than moving up a
level.** Nothing about it is load-bearing: it exists solely because parse and
bind results borrow from an arena that had nowhere else to live.

*Consequence accepted:* `Program` becomes lifetime-parameterised, so it cannot
be returned from a function that created its own arena, and anything storing one
grows a lifetime. That is a real ergonomic tax and it is the reason to choose
otherwise. **How we would know this was wrong:** a caller that genuinely needs
an owning `Program` — a language service holding one across requests is the
plausible case. The answer then is `self_cell` around the arena at the *top*
level, which is one self-referential type instead of one per file, and strictly
better than what exists today.

**One shared `SymbolStore` serialises binding.** Added 2026-08-05, on contact
with the code. `Program::bind_source_files` binds with rayon `par_iter_mut`
today, and `bind_into` accumulates into one store, so binding a *program*
becomes sequential. Upstream binds files in parallel (`BindSourceFiles` on a
work group) and can, because its symbols are pointers with no shared allocator
between them.

For the conformance corpus this does not bite — parallelism moves to the case
level, where the harness already parallelises, and the 15–17 ms per-case figure
in [architecture/program.md](../architecture/program.md) is already the
sequential number. For a real `tsc` over a large project it would, and nobody
has measured it.

The escape hatch is mechanical, which is why it is recorded rather than built:
bind each file into its own store in parallel, then merge with a `SymbolId`
offset. Ids are indices, so offsetting is arithmetic, and by then the node table
is already shared. **How we would know it was needed:** a wall-clock regression
on a multi-file build that does not appear on the corpus, whose profile is
dominated by `bind` on one thread.

**A `Span` was never widened, and nothing made that a compile error.** Added
2026-08-05, on building the thing — the fourth cost, found where the ADR's own
"check one level down" note predicted a fourth would be.

The argument above is that an *index* is only meaningful beside the table it
indexes, so widen the table. A `Span` is an index too — into the file's **text**
— and the text is the one thing a program does not have one of. Widening the
node table therefore desynchronised the two: before, holding a file meant
holding the only node table whose spans could be read against the only text you
had, and the pairing was structural. Now `nodes.span(id)` returns a `Span` that
is meaningful against exactly one of the program's files, and *which* one is not
recoverable from the id's type.

The failure mode is the bad kind. A span from `b.ts` read against `a.ts`'s text
does not panic and does not produce an error value; it produces a **plausible
line number from the wrong file**. `crates/tsr-conformance/src/binder_suite.rs`
hit this on the first run of the widened program: it iterates the symbol store
and converts each declaration's span to a line in the unit it is comparing, and
the store now holds every unit's symbols.

The countermeasure is `tsr_parser::ParsedInto::node_range`, carried on each
`ProgramFile` and asked through `ProgramFile::contains`. It works because files
are parsed one at a time, so a file's ids are one unbroken run — which is a
property of the loader's sequencing, not of the type system. *Consequence
accepted:* nothing enforces the pairing. A consumer that reads a span against a
text without asking `contains` first compiles and answers wrongly.

**How we would know this was the wrong countermeasure:** a second consumer
making the same mistake. Two would say the discipline does not hold at review
time, and the answer then is to make it structural — a `FileSpan`, or a
`text_of(NodeId)` on `Program` that does the lookup, so the wrong thing is
unspellable rather than merely discouraged.

**Parsing serialises too, not only binding.** A footnote to the third cost
rather than a fourth: `Program::parse` was a rayon `into_par_iter` over files
with one arena each. One arena and one node table means one thread and a defined
order, so it is now a `for` loop. The same escape hatch does not apply — node
ids have to continue the numbering — and the same mitigation does: parallelism
moves to the case level, which is where the conformance harness already has it.

**So it is measured, not argued.** ADR-0033 put the current shape at −17.0% for
parse+bind and +7.4% for parse-only. Whatever program-wide identity is built has
to be reported beside those two numbers, in
[architecture/performance.md](../architecture/performance.md), before it is
called done. A shared arena plausibly *improves* parse+bind — one allocator
instead of one per file, and no per-file `NodeTable` growth — and plausibly
worsens peak memory, since nothing can be dropped until the program is. Neither
guess is worth anything without the measurement.

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
*is* a global identity.

**It loses first on fidelity, and only then on cost.** There is no file tag
anywhere in `internal/checker`, because with pointer identity the question never
arises. Threading a `FileId` through every id comparison would introduce a
distinction upstream does not have and then maintain it forever — the same
failure as answering a question upstream does not ask, one crate lower down.
[ADR-0003](0003-tree-plus-side-tables.md) points the same way: when state is
side tables keyed by id, the way to cover more things is to widen the id space,
not to teach every consumer which table its id came from.

The cost argument is the second one, and it is that the cost lands in the wrong
place:
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

### Every cost of this decision was one level below the argument

Added 2026-08-05, after the third one. This ADR argued at the level of
**identifiers**: `SymbolId` and `NodeId` are indices, indices are only meaningful
beside their table, so widen the table. That argument is correct and it predicted
none of what the decision actually cost:

| cost | level it lives at |
|---|---|
| one arena implies the file **texts** too, because `Symbol.name` borrows source | ownership and lifetimes |
| sharing a bound lib prefix gets **harder**, not easier — it cannot be extended without copying | allocation |
| binding **serialises**, because one `SymbolStore` cannot be filled from several threads | concurrency |
| a `Span` indexes the **text**, which was not widened, so a node no longer knows which text reads it | positions |

The fourth was added on building it, and it is the one this table predicted:
the argument is about ids and tables, and a `Span` is an id into the one table a
program deliberately has many of.

All three were invisible from the id-level argument and all three were visible on
first contact with the code. Two of them reversed a plan that had been reasoned
about carefully at the wrong altitude: `bd tsr-6av` was filed as a precondition
for this work and is an optimisation, and its stated obstacle (`ProgramFile` not
being `Sync`) never applied at all.

The transferable instruction is **check one level down from wherever you are
arguing**. It is the same failure this project has now recorded three times in
different clothes: a histogram that ranked gaps while the defects producing them
went unranked; a bucket named for the answer rather than for the feature; and an
argument about identity that missed ownership, allocation and concurrency. In
each case the reasoning at its own level was sound.

## Consequences accepted

> **The three bullets below describe the state of the tree when this ADR was
> written, before the widening was built.** They are kept as written because
> they are what the decision was taken against; what each one reads as now is
> stated under it. Nothing here was silently edited.

- **The lib files are loaded and inert.** A program built with
  `Program::from_root_files` holds `lib.es5.d.ts`, parsed and bound, and answers
  no more names than it did before. That is worth stating plainly, because a
  changelog entry saying "lib files are now loaded" would otherwise be read as
  "globals now resolve".
- **None of the 18,387 lines moves on this commit.** Not the 3,126 in value
  position, not the 3,210 in type position, not the 12,051-line array bucket.
  None was expected to; all three are behind the widening, not behind the
  loading.
- **A second parse of the lib files is now avoided but a shared one is not
  possible.** `ProgramFile` is `Send` and deliberately not `Sync`
  (`crates/tsr-compiler/src/file.rs:88`) — the arena's bump pointer is a `Cell`
  and the bind result's storage is a `OnceCell` — so one bound `lib.dom.d.ts`
  cannot be shared across the programs of a parallel corpus run. At 3.9 MB of
  lib text per program, that is the reason the conformance `.types` producer was
  **not** rewired to build a program in this commit. See
  [architecture/program.md](../architecture/program.md).

**As of the widening (2026-08-05):**

- The lib files are **no longer inert**. `crates/tsr-compiler/src/lib.rs`'s
  `a_lib_files_symbols_are_the_whole_programs` — the same test, inverted rather
  than deleted — now asserts that `/a.ts` resolves a name declared in
  `lib.es5.d.ts`, *and* that the symbol it gets back has its declarations inside
  the lib file's own range of the shared node table. The second half is the part
  per-file identity could not have delivered: a name-keyed table of foreign
  symbols would have satisfied the first half and read the wrong file's
  declarations, which is the unsoundness this ADR rejected.
- **`ProgramFile` is `Send` by derivation, not by assertion.** The
  `unsafe impl Send` and the `self_cell` under it are both gone; see
  [ADR-0011](0011-unsafe-is-opt-in.md), whose exception list this shortens (and
  which turned out to have been one short). A `Program<'a>` is still `Send`,
  because it *borrows* the arena rather than owning it and the AST is `Sync`
  ([ADR-0012](0012-ast-is-sync.md)) — so a corpus worker creates the arena and
  the program is free to move. An owning `Program` would not be, since the
  arena's bump pointer is a `Cell`; that is the concrete price of the
  caller-owns-the-arena choice.
- Whether the 18,387 lines move is the `.types` producer rewire's question, and
  it is deliberately a **separate commit** so its delta is attributable.

## How we would know this was wrong

- If widening the identity space turns out to need a `SymbolId` wider than
  `u32`, or if arena sharing across a program's files measurably worsens
  parse-phase memory in `docs/architecture/performance.md`'s numbers, the
  cost calculus above is wrong and file-tagged identity is the cheaper option
  after all.
- If a language service or incremental build lands before the widening does,
  the "nothing needs per-file replacement" premise is false and this decision
  should be superseded rather than implemented.
- If, after the widening, the **gap-line** count falls by markedly less than the
  18,387 lines this ADR attributes to it, then those lines were never blocked on
  identity and the ranking that produced `bd tsr-9or.1` was measuring something
  else. Read this one with care: 18,387 is an upper bound taken by matching
  names at column 0, so a shortfall of a few thousand falsifies the *bound*
  rather than the decision. A shortfall approaching the 12,051-line array
  bucket would falsify the decision.

  **The denominator is gap lines, and substituting the unresolved-*name* count
  for it inverts the test.** Recorded because it was reached twice in one
  session, once by the person who had taken the measurement:

  |  | lines |
  |---|---|
  | unresolved-name count (10,535 value + 8,229 type) | 18,764 |
  | of which lib names, the only part a widening can move | **6,336** |
  | of which not lib — `undefined` (intrinsic), `div` (JSX), `T`/`U`/`V`/`K` (`bd tsr-y4u.21`) | 12,428 |
  | the 12,051-line array bucket, which is *not in the name count at all* | — |

  The two totals look interchangeable — 18,764 against 18,387, within 2% — and
  they are different pools. A **flawless** widening moves 6,336 of the name
  count, roughly a third of what a name-count test stated against 18,764 would
  demand, and that test would report a complete success as a two-thirds
  failure. The array bucket does not make up the difference, because array
  lines are gap lines and not name lines. Whenever these numbers are quoted,
  quote the denominator with them.

## Outcome, measured 2026-08-05 at `dddf425`

The widening landed in `454478a` (invisible, every suite flat) and the producer
rewire in `fa16ae4`, exactly as the ordering intended.

```
checker_types   1,163 → 1,336 cases     gradient 43.40% → 47.16%   (+3.76)
```

Every guard rail flat to the case: `binder_symbols` 8,292/8,459,
`printer_round_trip` 11,681/11,737, `parser_typescript` 5,000/5,031,
`file_loader` 96/96.

**The falsifier fired, and what it found is that this ADR's 18,387 was right in
total and wrong in composition.** Against the two-tier reading — a shortfall of
a few thousand falsifies the *bound*, a shortfall approaching the 12,051-line
array bucket falsifies the *decision*:

| | before | after |
|---|---:|---:|
| lib names unresolved, **type** position | 3,209 | **3** |
| lib names unresolved, **value** position | 3,126 | **397** |
| the array bucket | 12,051 | **11,784** |

**The lib-name half is 94% eliminated — 6,335 lines to 400 — and those lines
*were* blocked on identity exactly as claimed.** The array bucket did not move,
and the shortfall is therefore almost exactly the array bucket, which by the
letter of the falsifier condemns the decision.

It should not, and the reason is a mis-attribution rather than a wrong decision.
**The array bucket was never blocked on identity.** `T[]` is a reference to the
global `Array`, and reaching it needs *two* things: the global to exist, and the
`ArrayType` **type node** to be ported. This ADR counted the first and silently
assumed the second. The global now exists; the type node is still unported, so
the bucket sits at 0.00% and will until `getTypeFromTypeNode` grows an array arm.

So the honest reading is: **12,184 lines of the 18,387 were correctly
attributed, and 11,784 of those still require a separate, named piece of work
that this ADR did not name.** The falsifier was well constructed — it caught a
real defect in the reasoning — and the defect was in the ADR's arithmetic rather
than in its decision. Corrected here rather than quietly restated.

The whole-corpus effect is larger than the lib lines alone, because a program
also resolves *across a case's own units*: the literal bucket went 87.27% →
93.87%, named references 40.15% → 46.80%, and generic references 38.18% →
48.82%, none of which is lib.
