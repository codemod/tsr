# The program

The object that holds more than one file. What it is, what it loads, and the
one thing it still cannot do.

Upstream: `internal/compiler/program.go` (2,232 lines) and the lib selection in
`internal/tsoptions/enummaps.go`. Pinned at `5b1047d10`.

Code: [`crates/tsr-compiler/src/lib.rs`](../../crates/tsr-compiler/src/lib.rs),
[`crates/tsr-compiler/src/loader.rs`](../../crates/tsr-compiler/src/loader.rs),
[`crates/tsr-tsoptions/src/libs.rs`](../../crates/tsr-tsoptions/src/libs.rs).

Decisions: [ADR-0017](../adr/0017-program-before-tsconfig.md) built it before
tsconfig; [ADR-0034](../adr/0034-a-program-needs-one-identity-space.md) records
why it still cannot resolve a name across its own files.

## Two ways in

```
                     Program::in_arena(&arena, files)          ← a list, given
root file names ──▶  Program::from_root_files(&arena, host, …) ← a list, discovered
                          │
                          └─▶ FileLoader ──▶ imports, /// <reference />, libs
```

`Program::in_arena` takes the files. `Program::from_root_files` takes the roots
and runs the [file loader](file-loader.md), which is upstream's shape:
`NewProgram` calls `processAllProgramFiles` and takes the list back.

**Both take the arena, and the caller owns it.** `Program<'a>` borrows it, and so
does every file, tree, name, text and symbol in it — see
[ADR-0034](../adr/0034-a-program-needs-one-identity-space.md), "Who owns the
arena". A program's extent is a compilation, which is a scope; the conformance
harness wants exactly that, one arena per case. `Program::in_arena` is named for
the arena rather than called `new` because the argument is the contract, not a
plumbing detail.

**The loader hands over what it already parsed.** `parseTask.load` parses every
file to find its imports, so `LoadedFiles::files` carries the `ProgramFile`s
rather than only their names. Before lib files that was a modest saving; with
them it is not optional — `lib.dom.d.ts` alone is 2.3 MB, and the bundled
directory is 3.9 MB.

It also hands over the **shared node table and node map** the files were parsed
into, because a `NodeId` is only meaningful beside them. The loader is what
fills them: it copies each file's name and text into the arena with
`Arena::alloc_str` and calls `tsr_parser::parse_into`, so every file of a
program numbers its nodes in one sequence.

Binding stays a separate phase, as upstream keeps it (`NewProgram` parses,
`BindSourceFiles` binds) — but it is now **one accumulation, not a `par_iter`**.
`bind_into` fills one `SymbolStore`, in file order, libs first, and "which files
are bound" is the prefix `Program::bound_file_count` rather than a flag per
file. Upstream's `file.IsBound()` guard survives as "bind only past the
prefix"; binding twice adds nothing, which is asserted on the flow-node count,
because re-binding a file leaves the *symbol* count alone.

### A name is canonicalised by the program, not by the caller

`Program::source_file(name)` is `p.toPath(filename)` then `GetSourceFileByPath`
(`internal/compiler/program.go:1834`), and the program stores the two things
`toPath` needs — the current directory and the host's case sensitivity
(`:1830`).

It did not. Until 2026-08-05 it took the current directory as an argument,
stored neither, and recovered the case sensitivity by trying the case-sensitive
conversion and then the case-insensitive one, reasoning that a path which
round-trips unchanged was built that way. **Probed rather than argued
(`bd tsr-q89`):** a case-*sensitive* program holding `a.ts`, asked for `A.ts`,
misses on `/A.ts` and then hits on the fallback's `/a.ts`. It answered a name it
did not hold, for every file already spelled in lower case.

The bug predates the widening and the widening is what made it dangerous. With
one node table spanning the program, a caller that resolves a unit by name and
then reads spans against a *different* unit's text gets plausible positions from
the wrong file rather than anything that fails — see the `Span` section above.
Both conformance suites do exactly that lookup, once per baseline section.

The test holds **both** directions on two programs, because a fix that merely
deleted the fallback would pass the first and break the second: a
case-insensitive program genuinely must match `A.ts` to `a.ts`.

### A file owns nothing, and a `Span` needs a file

`ProgramFile` used to own an arena, a `NodeTable`, its text and its
`BindResult`, held together by `self_cell`. All four moved to the `Program`, so
it is a plain borrowing struct and the `self_cell` — and the `unsafe impl Send`
under it, an unlisted exception to
[ADR-0011](../adr/0011-unsafe-is-opt-in.md) — are gone.

What arrived in their place is a question that did not exist before: **a `Span`
is an offset into one file's text, and a `NodeId` no longer says which.** Each
file carries the contiguous run of ids its parse claimed
(`ProgramFile::node_range`, asked through `ProgramFile::contains`), and anything
converting a span to a position has to consult it first. Getting this wrong is
silent — a plausible line number from the wrong file — which is why it is
recorded in ADR-0034 as an accepted consequence with a falsifier rather than
left as a convention.

## Which lib files, and in what order

Three questions, all answered in
[`tsr_tsoptions::libs`](../../crates/tsr-tsoptions/src/libs.rs) against the two
generated tables `LIB_NAMES` (108 shipped files) and `LIB_MAP` (107 `--lib`
values, in upstream's declaration order).

**Which:** `noLib` selects none. An unset `lib` selects one file, the target's
default — `lib.d.ts` for ES5, `lib.es6.d.ts` for ES2015, `lib.esNNNN.full.d.ts`
above that. An explicit `lib` selects one file per entry, and an entry naming
neither an option nor a lib file name is skipped, as upstream skips it with its
`// !!! error on unknown name` still outstanding.

**By what spelling:** `--lib` accepts the option (`es2015`) or the file
(`lib.es2015.d.ts`), and `GetLibFileName` tries the file spelling first against
a set built from `LibMap`'s *values*. That set does not contain `lib.d.ts`, so
`--lib lib.d.ts` is an error even though the file ships. Checking against the
shipped-file list instead would quietly accept it.

**In what order:** the order they are loaded is not the order they were named.
`fileLoader.sortLibs` sorts them by `getDefaultLibFilePriority` — `lib.d.ts` and
`lib.es6.d.ts` first, then a file's position in `LibMap`'s key order, then
everything else — and the sorted libs are concatenated *in front* of the rest of
the program. This matters because a global interface declared in several libs
merges in file order, so `lib.es5.d.ts`'s `Array` must be seen before
`lib.es2015.iterable.d.ts` adds to it. A sort that looked tidier by going
alphabetical would silently change what the program means.

`/// <reference lib="es2015.symbol" />` goes through the same table. It is a
lookup, not a module resolution: it adds no `======== Resolving … ========`
header and no trace, and `noLib` silences it too.

### A lib file's `package.json` is never read

`parseTask.load` gives a lib file a fixed `CommonJS` metadata rather than calling
`loadSourceFileMetaData`. Upstream's stated reason is file-watcher noise. Here
the consequence is sharper: `load_source_file_meta_data` warms the resolver's
`package.json` cache, and later resolutions observe that cache — so probing for a
lib file's package would change the resolution *trace* of a program that has one.
The skip is load-bearing, and
`a_lib_files_format_is_fixed_and_its_package_json_is_never_read` is the test that
holds it in place.

### The default library path travels beside the host, not on it

Upstream reads it from `CompilerHost.DefaultLibraryPath()`
(`internal/compiler/host.go:16`). This port's loader takes a
`module.ResolutionHost`, which is a different and smaller interface; adding the
method there would merge two identities upstream keeps apart, and a
`CompilerHost: ResolutionHost` supertrait would need trait upcasting to reach
the resolver — stable since Rust 1.86, against this workspace's
`rust-version = "1.85"`. So it is a field of `LoadOptions`. If the MSRV moves,
the supertrait is the better shape.

An empty `default_library_path` is **not** `noLib`: the lib tasks are still
created and simply find nothing. That is what the conformance `file_loader`
suite runs with, since its host is an in-memory file system holding only the
case's own units.

### A file that could not be read is not in the program

Upstream collects it into `missingFiles` and reports a diagnostic
(`filesparser.go:483`). This port has no loader diagnostics, so the file is
simply absent. Before lib files this was invisible — every path the walk reached
had come from a file-system probe that had already found it. A lib file is the
first task built from a *name* rather than from a probe, so it is the first that
can be absent, and `collect_task` now drops an unread task rather than listing
it.

## What the program can now do, and what it still cannot

**It resolves a name across its own files.** Added 2026-08-05 by the identity
widening ([ADR-0034](../adr/0034-a-program-needs-one-identity-space.md)). A
program holding `lib.es5.d.ts` answers `Array` from a user file, and the symbol
it answers with has its declarations inside the lib file's own range of the
shared node table — which is the half that matters, because a name-keyed table
of foreign symbols would have answered the first half while reading the wrong
file's declarations.

> **This section used to say the opposite**, and the sentence it replaces is
> worth keeping in view: "*a program holding `lib.es5.d.ts` has `Array` as a
> bound symbol and no other file can see it*". That was true from `2a0dc19`
> until the widening landed, and the test that asserted it —
> `a_lib_files_symbols_are_still_the_lib_files_own` — was **inverted rather than
> deleted**, under the name `a_lib_files_symbols_are_the_whole_programs`. A
> limit that becomes a capability leaves a better record as one test that
> changed its answer than as one test removed and another added.

What is still missing is declaration merging, below. Two numbers are still open,
and both belong to the `.types` producer rather than to this object:

- Whether loading the lib files moves the unresolved-name numbers — 3,126 lines
  on a lib name in value position, 3,210 in type position, 12,051 in the array
  bucket, 18,387 in all, an upper bound taken in `2f6f0bf`.
- The conformance `.types` producer used to be the reason none of it was
  visible to `checker_types`: it gave each unit its own arena, bound it alone,
  and gave it its own `Checker`. **Rewired 2026-08-05**, as its own commit — see
  below.

## The global scope, and the merge that is not ported

Added 2026-08-05 with the identity widening
([ADR-0034](../adr/0034-a-program-needs-one-identity-space.md)).
`tsr_binder::bind_into` binds every file of a program into one `SymbolStore`,
and `merge_globals` ports the first loop of `initializeChecker`
(`internal/checker/checker.go:1296`): a non-module file's top-level names become
globals, and a UMD module's `export as namespace` names merge first-in-wins.
`BindResult::resolve_name` consults that table after the lexical walk runs off
the top of the file, which is where upstream's `resolveNameHelper` ends.

That is how `Array` resolves at all. Measured against the real shipped
`lib.es5.d.ts`: `Array`, `Object`, `String`, `Number`, `Boolean` and `Function`
resolve in type position and `parseInt`, `NaN`, `Infinity`, `JSON` and `Math` in
value position, each to a declaration inside the lib file's own node range.

**Declaration merging is not ported, and this is the sentence that matters:**
upstream's `mergeGlobalSymbol` unions two declarations of a name into one
symbol; this keeps the first and drops the second. `lib.es5.d.ts` and
`lib.es2015.iterable.d.ts` both declare `interface Array`, so `Array` resolves
to the ES5 one and the ES2015 members are simply absent.

**It turns "does not resolve" into "resolves, member missing".** That is
progress on the ranked measurement and it is *not* the same as being finished.
It is safe in the one way that decides whether a shortcut is acceptable here: an
unmerged member is **absent rather than wrong**, so a lookup for it still
answers `errorType` and a gap stays distinguishable from an answer. That is the
`errorType`-never-`anyType` discipline
([conventions](../conventions.md)) holding at a new layer — and it is exactly
the property that would be lost by the tempting shortcut of merging symbol
*tables* by name, which would produce one symbol whose members came from two
declarations that upstream might not have merged at all.

### `Promise` is declared in `lib.es5.d.ts`

Recorded because it changes the order in which the lib work pays. `lib.es5.d.ts`
declares `interface Promise<T>`; it is `PromiseConstructor` and
`declare var Promise` that live in `lib.es2015.promise.d.ts`. So the 640
`Promise` lines in the type-position bucket — the largest single name in it —
are reachable from ES5 alone, with no ES2015 lib loaded. It does not change the
18,387 total, only when the total arrives.

Found because a negative control asserted the opposite and the implementation
was right. The control now names `Map`, `Set`, `WeakMap` and `Iterable`,
verified against the shipped files rather than assumed.

## What a program of libs costs, and why that decides an ordering

Added 2026-08-05, measured by
[`examples/lib_program_cost.rs`](../../crates/tsr-compiler/examples/lib_program_cost.rs)
in release, second pass of two so the first warms the file cache.

| default lib | files | text | nodes | globals | per case | over 12,444 cases |
|---|---|---|---|---|---|---|
| `lib.es5.d.ts` alone | 3 | 0.22 MB | 10,933 | 132 | 1.4 ms | 18 s |
| `lib.d.ts` (the ES5 default, pulls in DOM) | 18 | 2.55 MB | 126,815 | 2,202 | 15.6–17.1 ms | **194–213 s** |
| `lib.es2015.d.ts` | 13 | 0.30 MB | 15,469 | 163 | 1.5–1.6 ms | 19 s |
| `lib.esnext.full.d.ts` | 93 | 2.85 MB | 140,767 | 2,241 | 14.3–15.4 ms | 178–192 s |

**The timings are a range because the machine was not quiet** — three other
agents were working during both runs, and the two passes differ by about 10%.
The file counts, byte counts, node counts and global counts are exact and
stable. The conclusion below turns on an order of magnitude, not on a
percentage, so the noise does not reach it; a number quoted to one decimal place
from this table would be false precision. Re-take it on a quiet machine if it is
ever load-bearing.

### The ordering decision

The `.types` producer parses, binds and checks **each unit standalone** — a
fresh arena per unit, `bind` alone, one `Checker` per unit. So it cannot see a
program, and no amount of lib work moves `checker_types` until it does. That
made the question: is sharing one bound lib program across cases
(`bd tsr-6av`) a *precondition* for the producer building a program, or an
optimisation?

**It is an optimisation, worth about three to four minutes on a corpus run.** Each case
building its own program costs 15–17 ms, and the cases are independent, so the
cost parallelises exactly as the corpus run already does.

Two things follow, and the second corrects an earlier claim in this document:

- **The `Sync` problem does not arise.** It was filed as the obstacle: one
  parsed-and-bound lib file cannot be shared between the programs of a parallel
  run because `ProgramFile` is `Send` and not `Sync`. True, and irrelevant —
  nothing has to be *shared*. Each worker builds its own program and `Send` is
  all a worker needs.
- **The widening makes sharing harder, not easier.** Under program-wide
  identity a case's files bind into the same `SymbolStore` as the libs, so a
  read-only lib prefix cannot be extended without copying it. `bd tsr-6av`'s
  original shape — "bind the libs once, share the result" — is no longer
  available; what remains is "copy a pre-parsed prefix per case", which trades
  parse time for a memory copy and is worth doing only if three minutes proves
  intolerable.

So `bd tsr-0e9` goes first and finishes with the producer rewire **as its own
commit**. That keeps the measurement attributable: the widening lands invisibly
(the suites are flat across it, checked), and the rewire commit changes nothing
else, so its delta *is* the widening's effect. Measuring the two together would
reproduce exactly the problem this project hit when four slices landed in one
integration run and the +5.10 could not be attributed among them.

### What the rewire actually does, and what it does not

`types_producer::assertions_for_case` builds **one program per case and one
`Checker` over it**: every unit is a root file, the bundled `lib.*.d.ts` are
mounted on the case's in-memory file system at `/.ts-lib`, and the case's own
`@target` directives choose the default lib through
`trace_case::apply_test_directives`. One checker rather than one per unit is
upstream's shape and also means a lib type resolved for the first unit is
memoised for the rest.

Three consequences, stated because each of them is a way to misread the number
it produces:

- **`@lib` is not applied.** `apply_test_directives` passes `lib` straight
  through (`trace_case.rs:417`), so a case asking for `es2015.iterable` gets the
  *target's* default lib instead. Cases that depend on a non-default lib will
  read as gaps for a reason that is not the checker's.
- **The libs are parsed with JSDoc on.** The loader uses a real compiler's parse
  options; the old producer used `jsdoc: false`. The lib text is JSDoc-dense, so
  a corpus run costs more than the 15–17 ms/case measured above, which was taken
  with JSDoc off.
- **`gap_reason`'s explanations move without `gap_reason` changing.** A receiver
  that used to be a gap can now resolve into a lib type, so lines migrate from
  "the receiver is a gap" to "the receiver has no such property". That is the
  instrument reading a changed world, not the instrument drifting.

The alternative considered was rewiring the producer first, against today's
per-file identity. Rejected: with per-file identity `Checker::new` takes one
file's `BindResult`, so a "program" in the producer would still check each unit
separately — it would pay the three minutes and read the same number, and then need a
second rewire when the identity model changed underneath it.

## What is not here

- **`libReplacement`.** Resolving `@typescript/lib-dom` to `@typescript/lib-*`
  through the module resolver — the one lib task that *does* trace. Off by
  default; `bd tsr-9or.5`.
- **An explicit `"lib": []`.** Upstream distinguishes an unset `Lib` from an
  empty one; `CompilerOptions::lib` is a `Vec<String>` with no such
  distinction, so an explicit empty list loads the target default here and
  loads nothing upstream. No corpus case writes it — checked.
- **Project references, redirects, and `--explainFiles`**, as with the
  [file loader](file-loader.md). The include reasons `--explainFiles` prints
  are recorded (`Program::file_include_reasons`) and explain TS6307, the
  composite file-list check of `verifyCompilerOptions`
  (`program_diagnostics::composite_file_list_diagnostics`). The rest of
  `verifyCompilerOptions` (`programDiagnostics`: option conflicts, removed
  options) has no producer, and the global half of `GetProgramDiagnostics`
  (`program_diagnostics::global_program_diagnostics`) is not yet reported by
  `tsr-execute`, which also means its gate on the semantic pass is absent.
