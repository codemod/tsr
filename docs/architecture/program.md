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
  at `78cfcba`: 10,535 lines on a free name that resolves to nothing, 8,229 on a
  type-position name, 12,491 in the array bucket. All three are behind the
  identity widening, not behind the loading.
- The conformance `.types` producer was **not** rewired to build a program.
  Doing so would load 3.9 MB of lib text per case across 12,444 cases, and
  `ProgramFile` is deliberately not `Sync`
  ([ADR-0011](../adr/0011-unsafe-is-opt-in.md),
  `crates/tsr-compiler/src/file.rs:88`), so one parsed-and-bound lib file cannot
  be shared between the programs of a parallel run. Rewiring it before the
  program can answer a cross-file name would cost the corpus run minutes and
  change no number. Sharing bound files across programs is the prerequisite, and
  it is a second decision, not a detail of this one.

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
