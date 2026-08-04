# ADR-0015: the file name is an input to binding, and the binder decides module-vs-script itself

- **Status:** accepted
- **Date:** 2026-08-04
- **Related:** [ADR-0002](0002-own-ast.md) and
  [ADR-0005](0005-codegen-from-ast-json.md) (why `SourceFile` has only the fields
  `ast.json` names), [ADR-0003](0003-tree-plus-side-tables.md) (why derived facts
  live beside the tree)

## The forcing constraint

A source file binds one of two ways. A **script**'s top-level declarations are
globals and go in the file's `locals`. An external **module** has a symbol of its
own, and an exported declaration is filed twice — a local, and an export on that
symbol (`declareModuleMember`, `internal/binder/binder.go:373`). Until now every
file bound as a script, so nothing ever reached a module symbol.

Reproducing upstream's choice needs two things it reads off the file and we did
not have:

1. **`ast.IsExternalModule(b.file)`** — the parser sets
   `SourceFile.ExternalModuleIndicator` from `isFileProbablyExternalModule`
   (`internal/parser/parser.go`), a scan for a top-level `import`/`export`.
2. **`b.file.FileName()`** — `bindSourceFileAsExternalModule`
   (`internal/binder/binder.go:766`) *names* the module symbol
   `"\"" + RemoveFileExtension(fileName) + "\""`, and `setExportContextFlag`
   (`:880`) treats a declaration file as an ambient context in which every
   declaration is implicitly exported.

Our `SourceFile` node carries `statements` and `end_of_file_token` and nothing
else, because it is generated from `ast.json` and checked in
([ADR-0005](0005-codegen-from-ast-json.md)); upstream's extra per-file fields —
file name, `IsDeclarationFile`, `ExternalModuleIndicator`, `GlobalExports` — are
not part of that schema and cannot be hand-added to generated code
([ADR-0007](0007-generated-code-policy.md)).

## The decision

**`bind` takes the file name.**

```rust
pub fn bind<'a>(file: &'a SourceFile<'a>, nodes: &NodeTable, file_name: &'a str)
    -> BindResult<'a>
```

**The binder computes the module indicator itself**, in `is_external_module`, by
scanning the top-level statement list for an `import`/`export` — the same test
upstream's parser runs, moved to the only place that currently asks the question.

Two consequences follow from having no field to write to and no owned strings:

- The module symbol's name is the path with the extension removed, **unquoted**.
  Upstream's quotes are a *spelling* of the same value, and the codebase already
  stores the value rather than the spelling for ambient module names
  (`declare module "fs"` gives the symbol `fs`, see `module_name`). The
  conformance harness strips the quotes off the baseline for exactly this reason
  (`normalise_symbol_name`).
- `export as namespace N` lands in a new `BindResult::global_exports`, mirroring
  upstream's `SourceFile.GlobalExports`, rather than in the module's exports.

## Alternatives

**Add the fields to `SourceFile`.** This is what upstream does and it is closed
to us while the node is generated: the file is regenerated and diffed in CI, so a
hand-added field is deleted by the next `cargo xtask codegen`. Reopening it means
either teaching the generator about non-`ast.json` fields — which makes the
generated tree diverge from the schema it claims to implement — or forking the
node out of codegen. Neither is worth one string and three booleans. *This wins
if* the number of per-file facts grows past a handful, at which point a
`FileInfo` struct passed alongside the tree is the natural shape and `file_name`
becomes its first field.

**Have the parser return the indicator in `ParsedSourceFile`.** Closer to
upstream's division of labour, and it would let the parser use the information it
already has (it knows where the top-level statements are, and it must decide
`import.meta`'s meaning under some module settings anyway). Rejected *for now*
because the parser would compute it for every caller while only the binder reads
it, and because the binder's scan is over a list it walks immediately afterwards
— it costs a few dozen kind comparisons. *This wins if* `import.meta` support is
implemented, since detecting it needs a full-tree walk that the parser gets free
and the binder would have to pay for.

**Derive a name from the tree instead of taking one.** There is nothing in the
tree to derive it from. A synthetic name (`__module`) would work for every
current consumer, because the conformance harness indexes a symbol under every
dotted suffix of its qualified name and so never depends on the module's own
name. It was rejected because module resolution compares these names: a checker
that resolves `import x from "./a"` looks for the module symbol named after
`./a`, and a synthetic name would have to be replaced exactly when the cost of
being wrong is highest.

## Consequences accepted

- **Every caller of `bind` must supply a name**, including tests and benchmarks
  that do not care. They pass `"test.ts"`.
- **`import.meta` is not a module indicator.** A file whose only module-ness is
  `import.meta` binds as a script. Named in `lib.rs` under "what is not built".
- **The module symbol's name diverges from upstream's by two quote characters.**
  Anything that compares symbol names against upstream's output must strip them.
  The harness already did.
- **The binder now depends on a filename convention** (`.d.ts`) for ambience.
  That convention lives in `is_declaration_file`, which is the one place to
  change if the compiler ever learns about it from configuration instead.

## How we would know this was wrong

- **A second per-file fact is needed** — `IsDeclarationFile` from configuration
  rather than the suffix, a package.json `type` field, a module kind. Two
  parameters is one too many; that is the signal to introduce the `FileInfo`
  struct named above.
- **`is_external_module` and the parser disagree** about what a module is, once
  the parser has its own reason to care (`import.meta`, or `export {}` in a JS
  file). Two implementations of one predicate is exactly the drift this project
  exists to avoid; at that point the parser owns it and passes it along.
- **The unquoted name causes a resolution miss** once module resolution exists.
  Then the quoting is not a spelling after all and the symbol name must carry it.
