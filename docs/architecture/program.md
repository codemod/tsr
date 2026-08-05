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
                     Program::new(files)                 ← a list, given
root file names ──▶  Program::from_root_files(host, …)   ← a list, discovered
                          │
                          └─▶ FileLoader ──▶ imports, /// <reference />, libs
```

`Program::new` takes the files. `Program::from_root_files` takes the roots and
runs the [file loader](file-loader.md), which is upstream's shape:
`NewProgram` calls `processAllProgramFiles` and takes the list back.

**The loader hands over what it already parsed.** `parseTask.load` parses every
file to find its imports, so `LoadedFiles::files` carries the `ProgramFile`s
rather than only their names. Before lib files that was a modest saving; with
them it is not optional — `lib.dom.d.ts` alone is 2.3 MB, and the bundled
directory is 3.9 MB.

Binding stays a separate phase, as upstream keeps it (`NewProgram` parses,
`BindSourceFiles` binds).

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

## What the program still cannot do

**Resolve a name across its own files.** A program holding `lib.es5.d.ts` has
`Array` as a bound symbol and no other file can see it; a two-file case has both
files and neither can name the other's declarations.

This is not a missing globals table. It is that `SymbolId` and `NodeId` are
indices into per-file tables, so a symbol from another file cannot be handed to
the checker without reading the wrong file's declarations. The full argument,
the alternatives, and the falsifiers are in
[ADR-0034](../adr/0034-a-program-needs-one-identity-space.md).

Two consequences worth stating so they are not mistaken for capabilities:

- Loading the lib files does **not** move the unresolved-name numbers. Measured
  3,126 lines on a lib name in value position, 3,210 on one in type position,
  12,051 in the array bucket — 18,387 in all, an upper bound taken in `2f6f0bf`.
  All three are behind the
  identity widening, not behind the loading.
- The conformance `.types` producer was **not** rewired to build a program, so
  none of the lib work is visible to `checker_types` — the producer gives each
  unit its own arena, binds it alone, and gives it its own `Checker`
  (`types_producer.rs:273-299`). That is a property of the *measurement path*,
  not of the lib work, and it means no further lib work can move that suite by a
  line until the producer changes.

  **Corrected 2026-08-05.** This paragraph previously gave the reason as cost
  plus `ProgramFile` not being `Sync`, and called sharing a bound lib program a
  prerequisite. The measurement below shows it is neither: the cost is about
  three minutes on a corpus run, and the `Sync` problem does not arise because
  nothing needs to be shared.

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
- **Project references, redirects, package deduplication, and `--explainFiles`
  include reasons**, as with the [file loader](file-loader.md).
