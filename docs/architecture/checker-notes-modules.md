# Checker notes: reaching another file

Field notes for the seam between `tsr-compiler`'s `Program` and `tsr-checker`.
Living document; the decision itself is
[ADR-0041](../adr/0041-the-checker-asks-its-program-for-a-module.md).

Upstream is pinned at `5b1047d10d32e7d5b446be4de56b126ff42f82bb`. Every anchor
below was taken with `grep -n` on the declaration, not by counting inside a
window — three anchors reached this workstream stale in one cycle, one of them
(`checker.go:550-566` for the `Program` interface) pointing at a *fragment* of
the right thing, which resolves and therefore passes `xtask anchors` forever.

## What the seam is

One method:

```rust
fn resolved_module(&self, importing_file: NodeId, specifier: &str) -> Option<NodeId>;
```

Declared as `ModuleHost` in `crates/tsr-checker/src/resolution.rs`, implemented
on `tsr_compiler::Program`. Both ids are **`SourceFile` node ids**.

It is upstream's `Program.GetResolvedModule` (`internal/compiler/program.go:521`)
composed with `Program.GetSourceFileForResolvedModule` (`:1839`) — the two calls
`resolveExternalModule` makes at `internal/checker/checker.go:15207` and `:15216`
with a diagnostic filter in between that this port has no diagnostics for.

The direction is forced, not stylistic. `tsr-compiler` depends on `tsr-binder`;
`tsr-checker` does not depend on `tsr-compiler` and must not start, because
ADR-0040 has the compiler driving a `check_source_file` traversal for
diagnostics, which would close the cycle. Upstream has exactly this shape for
exactly this reason: its checker declares a `Program` **interface** inside the
checker package (`checker.go:547`), holds it in the field `program`
(`checker.go:581`), and has it assigned at `checker.go:908`.

## How much of this ADR-0034 had already done, and nobody had noticed

This was briefed as a workstream. Most of it was already built.

A `Program` holds **one** `NodeTable`, **one** `NodeMap` and **one**
`BindResult` spanning every file (`crates/tsr-compiler/src/lib.rs`,
`bind_source_files`). So a `Checker` built over a `Program` already sees every
file's symbols and nodes. Cross-file *symbol* access is not part of this job and
has not been since ADR-0034.

the binder creates the file's own symbol only inside an `if self.is_module`
branch (`crates/tsr-binder/src/binder.rs:2595`, calling
`bind_source_file_as_external_module` at `:2573`), so
`BindResult::symbol_of(<SourceFile NodeId>)` is `Some` **exactly** for external
modules. That is upstream's `if sourceFile.Symbol != nil` gate
(`checker.go:15321`), already available from the binder. Which means returning a
`NodeId` provably loses nothing: everything downstream — module symbol, its
`exports` table, the alias target — is a local walk from the id, and the existing
export-marker arm (`crates/tsr-checker/src/symbols.rs`, `export_symbol_of`)
already performs precisely that walk for a same-file marker.

**The only thing missing was the specifier-to-file map.** It did not exist
anywhere: the loader resolved a specifier, handed
`resolved.resolved_file_name` to `add_sub_task`, and dropped it on the line that
computed it.

## The measurement that shaped the design: two mode functions, two answers

The obvious way to key the cache is upstream's way, on `(path, {name, mode})`.
The checker cannot supply the mode — upstream gets it from
`getModeForUsageLocation` (`internal/compiler/fileloader.go:726`), which reads
the specifier's *parent syntax* and the file's `SourceFileMetaData`, and this
port keeps neither anywhere the checker can see.

The tempting repair is the other branch of `resolveExternalModule`
(`checker.go:15205`), `GetDefaultResolutionModeForFile`
(`program.go:1562` → `fileloader.go:718`), which needs only the file. **Measured,
that does not work**, and the measurement is the finding:

| options | each file's default mode | the mode each `import "./b"` actually used |
|---|---|---|
| corpus default | `None` | **`CommonJS`** |
| `module: node16` | `CommonJS` | `CommonJS`, except `import("./b")` → `ESNext` |
| `module: esnext` | `None` | **`ESNext`**, except `import = require` → `CommonJS` |

Two of the three disagree, and the corpus default is one of them — so a lookup
keyed on the default mode would have missed on **every** import in the
conformance corpus while looking entirely reasonable. The two functions are
genuinely different upstream as well (`GetImpliedNodeFormatForEmitWorker` versus
`getEmitSyntaxForUsageLocationWorker`), so this is a faithful port of a real
distinction, not a porting slip.

A second measured fact from the same probe: under `node16`, one file's two
requests for the *same specifier text* answered differently — the static import
found `/b.ts`, the dynamic `import("./b")` at `ESNext` found nothing. So the mode
is not decorative; collapsing it away loses real information in a real
configuration.

Hence the shape in `crates/tsr-compiler/src/lib.rs`: the cache is keyed by
`(importing file path, specifier text)`, the mode stays on
`loader::ResolutionRequest` where it was computed, and a key that two modes
disagree about is stored as `ModuleResolution::Conflicting` and **answers
nothing**. A gap, never a guess.

## The seam is live end to end, and that was measured by accident

`crates/tsr-compiler/tests/module_host.rs` originally asserted the wrong
property — that a checker holding a host computes what a checker without one
computes. It failed, and the failure is the useful part. On the fixture
`/a.ts: import { b } from "./b"; export const a = b;`:

| symbol | no host | with host |
|---|---|---|
| `a` | `error` | **`1`** |
| `b` (in `/a.ts`) | `error` | **`1`** |
| `b` (in `/b.ts`) | `1` | `1` |
| `u` (`var u = undefined`) | `undefined` | `undefined` |

So the map, the trait, the field and the consuming alias arm compose, and a
cross-file alias reaches its target's type. The test now asserts the property
that is actually claimed — **`Checker::new` equals
`Checker::with_module_host(.., None)`** — because "a host changes nothing" is
false and is supposed to be.

## What must not move, and what did

- `module_resolution` 95/95 and `file_loader` 96/96 are the revert conditions.
  The loader's data flow changed (`ResolutionRequest` gained a field); neither
  suite constructs a `ResolutionRequest`, and `loader_suite.rs:148` reads them
  only in a failure-describe path.
- **`checker_types` must not move on the plumbing alone**, and if it does, the
  movement is not attributable to it. The `Checker` field is `None` at all 84
  existing call sites, including `types_producer.rs`'s, which is what the
  gradient is scored through. `Checker::new` delegates to
  `Checker::with_module_host(.., None)` and there is one body, so "unchanged for
  a call site with no host" is true by construction rather than by 84
  hand-edits. **The alias arms convert the lines, and the commit that switches
  `types_producer.rs` to `with_module_host` is what lets the gradient see
  them — neither is in this workstream's commits.**
- `binder_symbols` is **not** a usable rail here. `crates/tsr-conformance/src/binder_suite.rs:248`
  skips every symbol whose declarations span more than one file, which is close
  to the population this enables. See `docs/conventions.md`, "A guard rail that
  cannot observe a change reports safe either way".

## What it costs

Measured on a corpus-shaped program — 108 bundled `lib.*.d.ts` offered, 19
loaded, one case file `var x: number[] = [];`:

```
FILES=19  MODULE-REQUESTS=0  TYPE-REQUESTS=0
```

Zero module requests, so `resolved_modules` is a default `FxHashMap`, which
allocates nothing. For the conformance cases that never import, the cost is the
struct field and no heap. Where a case does import, the cost is one entry per
distinct `(importing file, specifier text)` pair, each holding two `Path`s
(`Path(String)`) — proportional to the imports a program actually has.

The other new field, `files_by_source_file`, is one `(NodeId, usize)` per file —
19 entries for the program above. It exists because the cache is keyed by `Path`
and the checker's currency is a `NodeId`; building the translation once beats
scanning the file list per import.

## Corrections made while doing this

Recorded because `docs/conventions.md` asks for it, and because two of these
were asserted to me as current facts and were not.

1. **`Checker::new` has 84 call sites, not ~12.**
   `grep -rn "Checker::new" crates --include='*.rs' | wc -l` → 84, across
   `tsr-checker/tests/` (many), `tsr-conformance/` (examples and
   `types_producer.rs`), `tsr-checker-spike/`, and a bench. This is the whole
   reason the constructor decision went to a second constructor rather than a
   fourth parameter.
2. **`Program::source_file` no longer round-trips a path through both case
   conversions.** That behaviour was removed under `bd tsr-q89`; it now stores
   `current_directory` and `use_case_sensitive_file_names` and converts once. So
   `resolved_module` mapping a resolved *name* to a member file by
   `source_file_by_path` is safe on a case-differing pair by construction — both
   sides went through the same `to_path` under the same program settings. Worth
   restating because a warning about the old behaviour was still in circulation.
3. **The `Program` interface is `checker.go:547`, not `:550-566`.** `:550` is
   `SourceFiles()` and `:566` is `GetProjectReferenceFromOutputDts` — both real
   lines, so the stale span *resolves* and would pass `xtask anchors` while
   naming a fragment. A passing wrong anchor is worse than a failing one.
