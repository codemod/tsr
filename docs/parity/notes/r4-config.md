# r4-config — inherited config specs and the composite file list (`tsr-2zk.936`)

Round-4 lane for two program/config-level causes that
r4-realworld (`docs/parity/notes/r4-realworld.md`, on its lane branch
`claude/beautiful-shannon-ar5gh0-r4-realworld` until integrated) found on TypeScript's own `src/jsTyping` and
`src/typingsInstallerCore` (`vendor/typescript-go/_submodules/TypeScript` @
`4d4f005c`): cause 14 (`tsr-2zk.932`) and cause 13 (`tsr-2zk.931`). Native
source is `vendor/typescript-go` @ `5b1047d`.

## Method

- **Native.** `tsgo` built by `scripts/offline-cargo/build-tsgo.sh` (Go 1.26.8
  bootstrapped from source; the toolchain download was not needed).
- **TSR.** Release `tsr` at this branch, against the frozen baseline binary
  built from `bc1e54e` (the branch point).
- **Scratch configs**, untracked, next to each project's `tsconfig.json`:
  `tsconfig.scratch.json` = `{ "extends": "./tsconfig.json", "compilerOptions":
  { "types": [], "outDir": "<scratch>", "pretty": false } }`, and
  `tsconfig.scratch-notypes.json` without the `types` override. A third copy
  of the first lives in an unrelated scratch directory and extends the
  project's config by absolute path (the cause-14 shape). The native outDir is
  deleted before every native run (composite projects replay `.tsbuildinfo`).
- **Keys** as r4-realworld: `(file, line, column, code)` of each `error TS`
  line. For TS6307 the whole diagnostic, message chain included, was diffed.

## Cause 14 — inherited `include` resolved against the extending directory

`parseConfig` (`internal/tsoptions/tsconfigparsing.go:1101-1170`) rebases a
base config's `include`/`exclude`/`files` by the relative path from the
extending config's directory to the base's directory, per hop, and passes only
those three keys and `compileOnSave` to the extending config. This port copied
every non-`compilerOptions` key verbatim. Two consequences, both measured:

| jsTyping via a config in another directory | files | errors | TS18003 |
|---|---|---|---|
| native | 83 | 163 | 0 |
| TSR before | **0** | **0** (exit 0) | 0 |
| TSR after | 83 | 951 | 0 |

TSR before found no files **and reported nothing**: the inherited
`references` key (copied by the old merge, not inherited natively) made
`canJsonReportNoInputFiles` false, so the empty program was silent. Same
directory (`tsconfig.scratch.json`) is unchanged by the fix: 83 files both.

The port is described in
[`docs/architecture/tsconfig.md`](../../architecture/tsconfig.md#inherited-files--include--exclude).
Every expectation in `crates/tsr-tsoptions/tests/extends_rebasing.rs` was
checked against native `tsgo --showConfig` / `--listFilesOnly` on the same
trees; one of mine was wrong and native corrected it: the two-hop spec is
`../configs/mid/../base/../../src/main.ts`, un-normalised, not
`../configs/base/../../src/main.ts`.

`${configDir}` in `files`/`include`/`exclude` was not substituted at all before
(`getSubstitutedStringArrayWithConfigDirTemplate`); it is now, against the
invoked config's directory.

## Cause 13 — TS6307, composite file-list check

`Program.verifyCompilerOptions` (`internal/compiler/program.go:938-956`): under
`composite`, each program file that `sourceFileMayBeEmitted` and whose path is
not among `Config.FileNames()` gets an include-processor diagnostic explained
by `createDiagnosticExplainingFile` (`processingDiagnostic.go:69`). That
positions it at the file's first written reference in loader replay order,
chains `The file is in the program because:` with every reason when there is
more than one, adds related information for the other reasons, and appends the
implied-format explanation (`explainRedirectAndImpliedFormat`).

None of the include-reason machinery existed. Ported:

- `crates/tsr-compiler/src/file_include.rs` — `FileIncludeReason`,
  `getReferencedLocation`, `toDiagnostic`/`toRelatedInfo` for referenced
  reasons, `explainRedirectAndImpliedFormat`'s format arm,
  `createDiagnosticExplainingFile`.
- `loader.rs` — each task carries its reason (`parseTask.includeReason`); the
  replay walk records it for the loading task before the seen check, as
  `filesParser.collectFiles` does (`filesparser.go:357-366`).
  `SourceFileMetaData` gains `package_json_directory`. Recording allocates
  nothing: a reason names its containing file by task index (mapped to a file
  index once files are numbered) and is moved, not cloned, because each task is
  one edge of the tree and the walk meets each edge once. The first version
  keyed reasons by `Path` and cloned path and specifier per edge; its median
  child CPU on generic-imports read 1.033 / 1.028 against the baseline binary
  (identical binaries: 0.982), which prompted the change.
- `program_diagnostics.rs` — `composite_file_list_diagnostics`, cached once per
  program (`includeProcessor.getDiagnostics`' `computedDiagnosticsOnce`), the
  located ones reported through `include_processor_diagnostics` of their file
  (`GetIncludeProcessorDiagnostics`), the global ones through
  `global_program_diagnostics`.

| | native TS6307 | TSR before | TSR after | diff of the TS6307 text |
|---|---|---|---|---|
| jsTyping | 77 | 0 | 77 | empty (248 lines, chains included) |
| typingsInstallerCore | 83 | 0 | 83 | empty |
| jsTyping via another directory | 77 | 0 | 77 | empty |

All other diagnostics are unchanged on both projects (jsTyping: common keys
56 → 133, native-only 107 → 30, TSR-only 818 → 818). Wall time is unchanged
within noise (≈70 s under parallel load for both binaries).

### Judgment calls

1. **Locating an import's literal.** Upstream's reason stores an index into
   `file.Imports()`, a node list. This port's
   `tsr_parser::ModuleSpecifier` has no node, only where its enclosing syntax
   starts and what syntax it is. `file_include::specifier_literal` reads the
   literal from the typed node at that position (the declaration's
   `module_specifier`, the import type's argument, or the literal itself for a
   call argument or JSDoc `@import`); one literal starts at a position and a
   declaration has one specifier slot, so no text is needed. It is evaluated
   only when an explaining diagnostic is built. **Rejected:** adding
   the literal's `NodeId` to `ModuleSpecifier` — the cleaner fix, but
   `tsr-parser` is not this lane's; it is offered to the integrator as a
   follow-up (the lookup then becomes an index). **Wrong if:** a TS6307 or
   related location lands on a different literal than native's; the
   jsTyping/typingsInstallerCore diffs above would show it.
2. **Sorting the file's include-processor list.** Upstream's collection sorts
   per file (`ast/diagnostic.go:229`). This port's loader diagnostics were
   returned unsorted and no case needed otherwise; the sort is applied only
   when an explaining diagnostic joins them, so the existing order is
   untouched. **Wrong if:** a file carries both a loader diagnostic and a
   TS6307 out of native order.
3. **Global TS6307 is produced but not reported.** A file reached by no
   written reference (only an automatic `types` entry resolving to a `.ts`)
   gets a file-less TS6307. Natively it is part of `GetProgramDiagnostics`,
   reported before the semantic pass and gating it.
   `diagnostics_of_any_program` returns file-located pairs only, and the driver
   (`tsr-execute/src/compile.rs`, not owned here) would have to call
   `global_program_diagnostics` and apply the gate. Recorded, not built.
4. **Reasons that point into the config file.** The root-file reason's
   `Part of 'files' list` / `Matched by include pattern` text, and the
   related locations of root, lib and type-library reasons, need the config's
   syntax tree and `GetMatchedFileSpec`/`GetMatchedIncludeSpec`. A root file
   cannot be the subject of TS6307, and lib/type entry points are declaration
   files, so these only appear as secondary reasons; they print upstream's
   no-config text and no related location. Recorded in the module docs.

## Also measured (not this lane's to fix)

- **`types` without `@types/node` (the scratch config without `types: []`).**
  Native reports one global diagnostic, `TS2688 Cannot find type definition
  file for 'node'`, and nothing else: it is an include-processor global
  diagnostic, so `GetDiagnosticsOfAnyProgram` skips the semantic pass. TSR
  reports 951 (the same set as with `types: []`): it has no TS2688 producer
  for an unresolved automatic type directive and no program-diagnostics gate.
  Both are the global half of the include processor (judgment call 3).

## Gates (at the push of both commits; baseline binary from `bc1e54e`)

- **§5 loss checks:** both empty; `diagverdictdump` and `verdictdump` are
  byte-identical to the frozen baseline. No corpus case exercises TS6307 or a
  rebased `extends` (the TS6307 baselines are all under `tsc/`/`tsbuild/`,
  which the corpus does not run).
- **Coverage:** every suite row unchanged — `checker_types` 8090/9538
  (98.16% lines), `diagnostics` 4264/5502, `module_resolution` 95/95,
  `file_loader` 96/96.
- **Tests:** workspace passes except the two known pre-existing failures
  (`iteration::optional_tuple_check_types_preserve_named_enum_identity_and_reads`,
  `mapped_tuple_inference::tuple_slice_optional_arguments_follow_null_and_exact_optional_options`).
- **Perf** (median child CPU, 41 samples, new/old): domain-model 1.012,
  generic-imports 1.028. Callgrind, `--singleThreaded`: generic-imports
  400,329,027 → 400,404,884 Ir (+0.02%), domain-model 1,733,939,629 →
  1,734,559,534 Ir (+0.04%).
