# The file loader

How a set of root file names becomes a program, and why the walk is shaped the
way it is. The decisions behind it are in
[ADR-0019](../adr/0019-the-loader-gate-discharges-the-mode-circularity.md); this
document is how it works.

Upstream: `internal/compiler/fileloader.go` (791 lines) and
`internal/compiler/filesparser.go` (567), plus
`internal/parser/references.go` (71). Pinned at `5b1047d10`.

Code: [`crates/tsr-compiler/src/loader.rs`](../../crates/tsr-compiler/src/loader.rs)
and [`crates/tsr-parser/src/references.rs`](../../crates/tsr-parser/src/references.rs).

## What it is for

[`tsr-module`](module-resolution.md) answers *"given this specifier, from this
file, in this mode, where does it point and by what route"*. Nothing asked it.
The loader asks. It is the only thing in the compiler that knows a program is
more than a list of files.

```
root file names ──▶ parse ──▶ collect specifiers ──▶ resolve ──▶ recurse
                      ▲                                             │
                      └─────────────────────────────────────────────┘
```

## The two walks

There are two, and the separation is the single most important thing about this
module.

**The discovery walk** (`process_task` / upstream `filesParser.start`) claims a
path, loads it, and recurses into whatever its resolutions produced. It may run
in any order — upstream runs it on a work group, several files at a time.

**The replay walk** (`collect_files` / upstream `getProcessedFiles`'s
`collectFiles`) is depth-first, single-threaded, deterministic, and produces the
observable output: the trace, and the file list.

Each task therefore *buffers* what it produced rather than writing it out:

```rust
struct ParseTask {
    type_resolutions_trace:   Vec<Trace>,   // /// <reference types>, and @types
    resolutions_trace:        Vec<Trace>,   // imports and augmentations
    // …plus the requests behind them, for the conformance oracle
}
```

Buffering is not an optimisation. Under upstream's parallel loading, a trace
written as it happened would come out in whatever order the scheduler chose, and
6,700 lines of committed baseline would be unmatchable. It is what makes the
output a property of the *program* rather than of the machine.

This port loads single-threaded today. The buffering is kept anyway, because it
is what lets the two walks differ at all — and because the replay order is not
the discovery order even single-threaded (see below).

### The replay order, exactly

For each task, in depth-first pre-order over the task tree, skipping any path
already emitted:

1. its **type** resolutions, in order;
2. its **module** resolutions, in order;
3. then recurse into its subtasks.

Type-before-module is the one rule that is not "source order", and it is why a
`/// <reference types="node" />` in a file always precedes that file's imports in
a baseline. Swapping the two costs 5 of the 96 conformance cases.

Root tasks are walked in order, and the synthetic `types`/`@types` task is
appended **after** every root file — which is why every automatic type directive
appears at the *end* of a `.trace.json`, however early `// @types:` was declared.

## Where the specifiers come from

Three sources, and they are not interchangeable:

| Source | Produced by | Resolved through | Traces? |
|---|---|---|---|
| `import` / `export from` / `import =` / `import()` / `require()` / `import` types / `declare module "x"` | `tsr_parser::collect_external_module_references` | `Resolver::resolve_module_name` | yes |
| `/// <reference types="…" />`, and the `types` option | `tsr_parser::parse_file_references`, `module::get_automatic_type_directive_names` | `Resolver::resolve_type_reference_directive` | yes |
| `/// <reference path="…" />` | `tsr_parser::parse_file_references` | *not resolved* — it names a file, so it is a file-system probe | no |

Measured across the 146 baselines: of 61 type-reference-directive requests, 40
come from a `/// <reference types=>` in the case and 21 from the `types` option
or `@types` auto-discovery. There is no third source.

### What is *not* a module reference

Five corpus cases are baselined with an **empty** trace, and each is a distinct
way to be wrong here. They are assertions, not skips:

- `compiler/moduleResolutionWithRequire` — `require("x")` in a `.ts` file. It is
  an ordinary call to whatever `declare const require` names. Only JavaScript
  files follow it.
- `compiler/jsdocInTypeScript` — `import("…")` inside a JSDoc comment in a `.ts`
  file (upstream descends into JSDoc only for JS files; here JSDoc is a side
  table the walk never enters), and `import(String())`, whose argument is not a
  literal.
- `conformance/globalAugmentationModuleResolution` — `declare global`, whose
  name is an identifier and not a module specifier.
- `compiler/pathMappingBasedModuleResolution1_node` — a `paths` table over a
  single file that imports nothing. Configuring a mapping is not using one.
- `conformance/typingsLookup2` — an `@types` package whose `package.json` says
  `"typings": null`, which `@types` auto-discovery must not include.

The last two became visible only when [`tsr-tsoptions`](tsconfig.md) landed:
both configure themselves through a `tsconfig.json`, so both were previously
skipped rather than asserted.

Also not module references: `import M = N` (a namespace alias, not
`require`), and a *relative* import written inside an ambient external module —
TypeScript 1.0 spec §12.1.6, still enforced.

### An ambient module is one of two different things

```ts
declare module "m" { }
```

is an **augmentation** of an existing module `"m"` — which the loader resolves —
when the containing file is itself an external module. In a script it *declares*
`"m"`, which resolves nothing and instead satisfies other files' imports of that
name. The `is_external_module` input decides which, and it is computed by the
loader rather than the parser because it depends on compiler options
(`ast.GetExternalModuleIndicatorOptions`).

An augmentation is resolved but never adds a file. Upstream gets there by index
arithmetic — an augmentation's index is past the end of `file.Imports()`, so
`shouldAddFile` fails — which does not survive the port and so is spelled out.

## The mode

Every request carries a resolution mode, and computing it is the loader's real
work. Three layers:

1. **The file's format** (`ast.GetImpliedNodeFormatForFile`). `.mts`/`.mjs`/
   `.d.mts` are ESM, `.cts`/`.cjs`/`.d.cts` are CommonJS, and everything else
   asks the nearest enclosing `package.json` for its `type` field. That lookup
   goes through `Resolver::get_package_scope_for_path`, which upstream runs with
   tracing *off* — so it emits no trace lines but does warm the `package.json`
   cache that later resolutions observe. The baseline sanitiser exists to make
   that unobservable.
2. **The syntax at the usage site** (`getModeForUsageLocation`). `require()` and
   `import x = require()` are CommonJS whatever the file is; `import()` depends
   on whether the emitter would down-level it; a plain `import` follows the file.
3. **An explicit override** — `with { "resolution-mode": "require" }` on a
   type-only import or an `import` type — which beats both.

For (2) the loader needs to know what syntax produced a specifier.
`getModeForUsageLocation` reads `usage.Parent`; this tree has no back-edges
([ADR-0003](../adr/0003-tree-plus-side-tables.md)), so
`tsr_parser::ModuleSpecifier` records a `SpecifierContext` at the moment the
collecting walk finds it instead. That is strictly more information than the
parent gives, since the `resolution-mode` override lives two nodes above it.

One counter-intuitive consequence, and it is upstream's:
`importSyntaxAffectsModuleResolution` is true under **every default**, because
`GetResolvePackageJsonExports()` returns true when the option is unset. So the
syntax decides the mode even under `bundler`, where there is no format split.

## The depth budget

A JavaScript file found inside `node_modules` is loaded but its own imports are
not followed, unless `maxNodeModuleJsDepth` says otherwise. Two flags carry it:
`increase` (this file was reached through `node_modules`) and `elide` (drop it
once the budget is spent). They travel together and so live in one `Depth`.

## What it does not do

Named here rather than discovered later; the rationale and falsifiers for each
are in [ADR-0019](../adr/0019-the-loader-gate-discharges-the-mode-circularity.md).

- Lib files are not loaded (they resolve nothing) — except under
  `libReplacement`, which does, and is not implemented (bd tsr-9or.5).
- Root file names arrive from the caller. Turning a `tsconfig.json` into that
  list is [`tsr-tsoptions`](tsconfig.md)'s job, as it is upstream's; the loader
  takes `ParsedCommandLine.FileNames` either way.
- No `importHelpers`/`tslib` synthetic import: no baseline has one.
- No project references, redirects, or package deduplication — as with
  [`Program`](../adr/0017-program-before-tsconfig.md).
- Single-threaded, and each path is claimed once. Upstream additionally keys
  tasks by file-name casing and reprocesses a task reached at a lower depth.
- `moduleDetection` is assumed `auto`. It is not a ported option.
- The include-reason bookkeeping behind `--explainFiles`, and the diagnostics for
  a missing or unsupported-extension file, are dropped: each place upstream
  records one, this returns nothing. None is a resolution, so none is visible in
  the oracle.

## The gate

`file_loader`, in
[`crates/tsr-conformance/src/loader_suite.rs`](../../crates/tsr-conformance/src/loader_suite.rs).
It runs this loader over each `@traceResolution` case and compares the lines that
describe a *request* — the opening `======== Resolving … ========` header and the
`Resolving in … mode with conditions …` line — against the committed baseline.

**96/96, 100.00%.** Not a rate that moved when the loader landed: it was a new
number covering the half of the `.trace.json` oracle `module_resolution` was
explicitly not judging. It went from 76 to 96 when
[`tsr-tsoptions`](tsconfig.md) let both suites stop skipping tsconfig-configured
cases. See [module-resolution.md](module-resolution.md) for the other half and
how the two denominators relate.
