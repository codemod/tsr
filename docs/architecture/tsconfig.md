# Reading tsconfig.json

How a config file becomes compiler options and a root file list. The judgment
calls are in [ADR-0020](../adr/0020-the-parser-reads-tsconfig.md); this document
is how it works.

Upstream: `internal/tsoptions` (8,706 lines) and `internal/vfs/vfsmatch` (717).
Pinned at `5b1047d10`.

Code: [`crates/tsr-tsoptions`](../../crates/tsr-tsoptions),
[`crates/tsr-vfs/src/glob.rs`](../../crates/tsr-vfs/src/glob.rs), and
[`crates/tsr-parser/src/json.rs`](../../crates/tsr-parser/src/json.rs).

## The pipeline

```
tsconfig.json ──▶ tsr-parser (ScriptKind::Json) ──▶ AST
                                                    │
                                     value::root_properties
                                                    │
                        ┌───────────────────────────┴───────────┐
                   compilerOptions                   files / include / exclude
                        │                                       │
                  declarations::apply                  vfs::glob::read_directory
                        │                                       │
                 CompilerOptions                    file_names::expand
                                                                │
                                                        root file names
```

Both outputs go to [`FileLoader`](file-loader.md), which is where the program
actually starts.

## The parser reads it, not a JSON library

`tsconfig.json` is not JSON. It permits comments and trailing commas, and the
corpus contains configs with both — one of the cases the resolution baselines
judge opens its `compilerOptions` with a `//` comment. A conforming JSON parser
rejects real configs.

`tsr-parser` gained a `ScriptKind::Json` entry point (`parseJSONText`, ~70 lines)
that reuses the object- and array-literal parsers already there. Comments become
trivia and a trailing comma is ordinary object-literal syntax, so both work
without a special case. Two other things follow for free: every option error can
point at a span, and the language service will be able to offer completions
inside a config.

`tsr_module::json` is a *different* reader and stays: `package.json` is
machine-written, strictly JSON, and read for values rather than spans.

## One option table, not two

Upstream keeps the declaration (`optionsForCompiler` says `"target"` is an enum
over `ScriptTarget`) apart from the assignment (`ParseCompilerOptions`, a switch
on the option name that writes into the struct). Two lists keyed the same way,
kept in step by hand — upstream has a test whose only job is to check they are.

Here each `OptionDeclaration` carries its own setter:

```rust
OptionDeclaration {
    name: "typeRoots",
    kind: OptionKind::List(&OptionKind::String),
    is_file_path: true,
    apply: |options, value| { /* … */ },
}
```

Name, type, path-ness and destination are one entry, so there is nothing to keep
in step and no second switch to forget.

The table declares the options `CompilerOptions` has a field for — the same rule
that governs the struct. An option declared here with no field would parse into
nothing; anything undeclared is reported as unknown, which is what upstream does
for a genuinely unknown option.

### `is_file_path` is not decoration

A path option is made absolute against the config's directory *before* it is
stored, so nothing downstream needs to know where the config was. Miss it and
`"outDir": "bin"` resolves against the current directory instead — which the
conformance suites catch: turning it off costs both of them a case.

## Expanding `files` / `include` / `exclude`

Three lists with three different rules, so three maps:

| Source | Rule |
|---|---|
| `files` | Verbatim. An `exclude` cannot remove one, and a wildcard cannot suppress one. Always first in the result. |
| `include` | Globbed against the file system. Subject to extension priority. |
| `include`, `.json` files | Only when a pattern names `.json` explicitly, so `**/*` does not sweep up every `package.json`. |

Defaults, in the order they are applied:

- No `exclude` and an `outDir`/`declarationDir` set → the output directories
  exclude themselves. Otherwise the second build compiles what the first emitted.
- Neither `files` nor `include` → `include: ["**/*"]`.

### Extension priority

A wildcard must not pull in a file's own compiler output. Given `a.ts` and `a.js`
side by side with `allowJs`, `include: ["**/*"]` takes only `a.ts`. Two functions
enforce it in both directions, because the walk finds files in directory order
rather than extension order: one skips a file when something higher-priority is
already in, the other removes something lower-priority that already got in.

One deliberate exception, which upstream labels LEGACY BEHAVIOUR and this port
keeps: a `.d.ts` *does* accompany its `.js(x)` counterpart. It came from an
off-by-one in the original priority table, and projects now depend on it.

## The glob engine

`include`/`exclude` patterns are not POSIX globs and not `.gitignore` patterns.
Three rules make them their own thing, and all three change which files a program
contains:

- **`**` in an include refuses to descend into `node_modules`,
  `bower_components`, `jspm_packages`, or any dot-directory** — while the same
  `**` in an exclude descends happily. A pattern's meaning depends on which list
  it was written in. This is why a default config does not compile every
  dependency it has.
- **A leading wildcard does not match a dotfile.** `*.ts` does not match `.a.ts`.
- **`*.js` does not match `foo.min.js`** unless the pattern itself mentions
  `.min.`.

It lives in `tsr-vfs` because that is where upstream puts it
(`internal/vfs/vfsmatch`) and because it is about the file system, not about
options. Matching is iterative with a single backtrack point — O(n·m) rather than
exponential, so a pattern like `*a*a*a*b` is not a denial of service.

## What is not here

Each is a named gap, not an oversight:

- **`extends`** — 6 of the corpus's 130 `tsconfig.json` units use it and none of
  the 20 the resolution baselines judge. It needs a resolution stack, circularity
  detection, and in its package form a module resolution of its own, which would
  make config parsing depend on `tsr-module`. bd tsr-9or.6.
- **Project references** — with the rest of the project-reference machinery the
  file loader also lacks.
- **The command line** — `tsc` is Phase 6.
- **`tsc --init` / `--showConfig`** — nothing prints a config back yet.
- **Config diagnostics as an oracle.** Errors are collected and returned, and
  their messages are upstream's, but no suite judges them and the spans are the
  offending value's rather than upstream's exact
  `CreateDiagnosticForNodeInSourceFile` range.

## The gate

Both resolution suites. `tsr-tsoptions` is what let them stop skipping
tsconfig-configured cases:

```text
    75  module_resolution, before
   +20  cases configured by a tsconfig.json unit
 =  95  after   →   95 passed, 100.00%
```

`file_loader` gained the same 20, reaching 96/96. See
[module-resolution.md](module-resolution.md) for how the two denominators relate.

It also made the corpus's own skip predicate honest: `SkipUnsupportedCompilerOptions`
runs on the *merged* options, so a `baseUrl` or an `outFile` written in a config
is now seen. That moved 18 cases into the "upstream skips this" bucket where they
had previously been mis-bucketed as tsconfig-configured, and it is asserted by
`the_skip_rule_explains_every_missing_baseline` rather than left to drift.
