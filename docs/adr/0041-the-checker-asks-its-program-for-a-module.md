# ADR-0041: the checker asks its program for a module, and the program answers with a file

Status: accepted
Date: 2026-08-05
Upstream pinned at `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.

## The forcing constraint

`Checker::new(binder, nodes, node_map)` had no module awareness, so every
cross-file alias answered `errorType`. Two ranking-board rows sit behind that:
`declaration name … SymbolFlags(ALIAS) / no value declaration` and its
`reference …` sibling. **No line count is claimed here.** A third agent is
measuring the true population, and `docs/conventions.md` records five rows that
collapsed on contact with a measurement; a ceiling on a population is not a
prediction.

What is *not* a judgement call is where the missing edge is, and it is one line:

```rust
let (resolved, traces) = self.resolver.resolve_module_name(&specifier.text, &file_name, mode);
// ... ResolutionRequest { kind, name, containing_file, mode }   <- the KEY, kept
// ... self.add_sub_task(index, &ResolvedRef { file_name: resolved.resolved_file_name, .. });
//                                                       ^^^^^^^^ the VALUE, consumed and dropped
```

`crates/tsr-compiler/src/loader.rs`. `ResolutionRequest` was already exactly
upstream's cache key — `module.ModeAwareCacheKey{Name, Mode}` under
`p.resolvedModules[file.Path()]` (`internal/compiler/program.go:521`-`:527`) —
missing only the value. And `LoadedFiles.requests` was then dropped in full by
`Program::from_root_files`.

Upstream's counterpart map, `p.resolvedModules`, is read by
`Program.GetResolvedModule` (`program.go:521`), which the checker reaches through
an **interface** it declares itself (`internal/checker/checker.go:547`, held in
the field at `:581`, assigned at `:908`), from `resolveExternalModule`
(`checker.go:15207`).

That interface direction is forced here too. `tsr-compiler` depends on
`tsr-binder`; `tsr-checker` does not depend on `tsr-compiler` and must not
start, because ADR-0040 has the compiler driving a `check_source_file` traversal
for diagnostics — the moment it does, a checker that names its driver is a
cycle.

## Decision

1. The loader records the resolved file name beside the request it already kept
   (`loader::ResolutionRequest::resolved`).
2. `Program` gains `resolved_modules`, the port of `p.resolvedModules`, built
   from those requests, plus `Program::resolved_module`, the port of
   `GetResolvedModule` composed with `GetSourceFileForResolvedModule`
   (`program.go:1839`).
3. The seam's trait, `ModuleHost`, is declared in `tsr-checker`
   (`crates/tsr-checker/src/resolution.rs`) and implemented on
   `tsr_compiler::Program`.
4. `Checker` holds `Option<&dyn ModuleHost>`, supplied by a **second
   constructor**, `Checker::with_module_host`. `Checker::new` keeps its
   signature and delegates with `None`.

The currency across the seam is a **`SourceFile` node id**, both directions.

## The alternatives, taken seriously

### Keying the cache on the mode, as upstream does

Upstream keys on `(path, {name, mode})`. Dropping the mode is a real loss, so
the first design kept it and had the checker supply one.

It cannot. Upstream computes a usage location's mode with
`getModeForUsageLocation` (`internal/compiler/fileloader.go:726`), which reads
the specifier's parent syntax *and* the file's `SourceFileMetaData`; this port
keeps the metadata inside the loader walk and discards it, and the checker has
neither. The available substitute is the other branch of `resolveExternalModule`
(`checker.go:15205`), `GetDefaultResolutionModeForFile` (`program.go:1562` →
`fileloader.go:718`), which needs only the file.

**Measured, it disagrees with the mode the loader actually used, including on
the corpus default:**

| options | file's default mode | the mode `import "./b"` used |
|---|---|---|
| corpus default | `None` | **`CommonJS`** |
| `module: node16` | `CommonJS` | `CommonJS` (`import("./b")` → `ESNext`) |
| `module: esnext` | `None` | **`ESNext`** |

A lookup keyed that way would have missed on every import in the corpus. The two
are different functions upstream as well (`GetImpliedNodeFormatForEmitWorker`
versus `getEmitSyntaxForUsageLocationWorker`), so the disagreement is faithful,
not a porting error.

So the key collapses to `(importing file, specifier text)`. **What would make
the rejected option win:** a `NodeId` on the parser's `ModuleSpecifier`, or any
other way to identify the checker's string-literal node with the loader's
specifier. Then `mode_for_usage_location` — which the loader already has and
already runs — can be recalled by node id, the key regains its mode, and the
change is local to `Program`.

The mode is **not silently dropped**: it stays on `ResolutionRequest`, it is used
to detect the collapse, and where two modes disagree the entry becomes
`ModuleResolution::Conflicting` and answers **nothing** rather than the first or
the majority. That case is real — under `node16` a file's static `import "./b"`
resolves and its dynamic `import("./b")` does not.

### A fourth parameter on `Checker::new`

`Checker::new` has **84** call sites (`grep -rn "Checker::new" crates
--include='*.rs' | wc -l`), across `tsr-checker/tests/`, `tsr-conformance/`
(including three examples owned by other agents), `tsr-checker-spike/`, and a
bench. Almost all parse one file and have no program.

The change is judged on one property: *a call site with no host behaves
bit-for-bit as today.* Under a fourth parameter that is a claim about ~81
hand-edits. Under a second constructor it is true by construction — `new` is
`with_module_host(.., None)` and there is one body. That is the same reasoning
`contextual.rs` used when it returned `None` rather than an answer: returning
nothing invents nothing, so the only lines that can move are ones someone moved
deliberately.

**What would make the fourth parameter win:** a second optional dependency. Two
of them means four constructors under this shape and one signature under that
one — and at that point a builder beats both.

### A builder

Rejected as a second way to construct the central type for one optional field.
It becomes right at two optional fields, which is the same trigger as above.

### Returning a `SymbolId` (the module symbol) instead of a `NodeId`

The checker's next step after resolution *is* the module symbol, so handing it
back directly looks like it saves a hop. It does not, and it costs vocabulary:
the host would have to know about symbols. The binder creates a file's own symbol only inside an
`if self.is_module` branch (`crates/tsr-binder/src/binder.rs:2595`, calling
`bind_source_file_as_external_module` at `:2573`), so
`BindResult::symbol_of(<SourceFile NodeId>)` is `Some` **exactly** for external
modules — which *is* upstream's `if sourceFile.Symbol != nil` gate
(`checker.go:15321`), reachable from the id the checker already holds. Returning
the id provably loses nothing, and it matches upstream, whose
`GetSourceFileForResolvedModule` returns an `*ast.SourceFile`.

## Consequences accepted, including the bad ones

- **Two modes that disagree answer nothing.** Callers see a gap where upstream
  has an answer. Bounded to files that import one specifier text twice under
  different syntax, under `node16`/`nodenext`/`preserve`.
- **Unresolved and never-asked are indistinguishable** at the seam. Both must
  answer `errorType` today. This stops being safe the moment TS2307
  ("Cannot find module") is ported, because that diagnostic needs exactly the
  distinction. Recorded on `ResolutionRequest::resolved`, where a future porter
  will be standing.
- **`Program::new` (the file-list constructor) resolves no module at all.** It
  ran no resolver, so it has nothing to recall, and it does *not* try to
  re-derive resolutions by matching specifier text against the file list. Every
  such program answers `None`. `tsr-conformance` builds through
  `Program::from_root_files` (`types_producer.rs`, `program_for_case`), so the
  seam is live where it is measured.
- **`tsr-compiler` now depends on `tsr-checker`.** That is upstream's direction
  (`internal/compiler` imports `internal/checker`) and it is what the interface
  exists to keep one-way.
- **Memory.** Measured on a corpus-shaped program (108 bundled libs offered, 19
  loaded, one case file with no imports): **0** module requests, so
  `resolved_modules` is a default `FxHashMap` and allocates nothing. Where a case
  does import, the cost is one entry per distinct `(file, specifier)` pair
  holding two `Path`s. The companion index `files_by_source_file` is one
  `(NodeId, usize)` per file — 19 entries for that program.
- **This converts nothing on its own**, and reporting otherwise would be
  measuring noise. Everything here is the map, the trait impl, and a field that
  is `None` at every existing call site — including `types_producer.rs`'s, which
  is what the gradient is scored through. The alias arms convert the lines, and a
  further commit switching the harness to `with_module_host` is what lets the
  gradient see them.

  That said, the seam is **demonstrably live**, which is stronger than
  plausible. With the consuming arm present,
  `crates/tsr-compiler/tests/module_host.rs`'s fixture
  (`/a.ts: import { b } from "./b"; export const a = b;`) computes `a` and `b`
  as `error` without a host and as `1` with one, and `/b.ts`'s own `b` is `1`
  either way. That is *probing past the blocker* rather than at it, which
  `docs/conventions.md` asks for: it shows what the form answers once the
  blocker is removed, instead of only confirming the blocker exists.

## How I would know I was wrong

- **The mode collapse is not benign.** If a corpus run shows cases where
  `ModuleResolution::Conflicting` is reached at all — the design assumes it is
  rare and node16-shaped — the key must regain its mode, and the specifier-node
  identity above becomes required work rather than an option.
- **The default-mode measurement is wrong.** It was taken on synthetic fixtures
  through `FileLoader::load`, not over the corpus. If `GetDefaultResolutionModeForFile`
  in fact agrees with the request mode for the corpus, the faithful key was
  available all along and the collapse was unnecessary. The falsifier is one
  histogram of `(request.mode, default mode for containing file)` over a corpus
  run.
- **`module_resolution` (95/95) or `file_loader` (96/96) moves.** Either means
  the loader's data flow changed observably and the change comes out.
- **`checker_types` moves on this alone.** It must not. If it does, either
  `Checker::new`'s delegation is not equivalent, or something else landed in the
  same range — and the commit pair, not the reasoning, is what settles which.
- **The second constructor is a fork nobody uses.** If, a cycle from now, the
  conformance harness is the only caller of `with_module_host` and every test
  still passes `None`, the seam is live in one place and the `Option` is
  carrying a harness limitation rather than a real distinction. The fix then is
  to give the unit harness a program, not to add a third constructor.

## See also

- [ADR-0034](0034-a-program-needs-one-identity-space.md) — why a `SourceFile`
  node id names a file, and why cross-file *symbol* access was already done.
- [ADR-0040](0040-diagnostics-come-from-a-check-traversal-and-assignability-gets-a-reporting-twin.md)
  — why the compiler will drive the checker, which is what forbids the reverse
  dependency.
- `docs/architecture/checker-notes-modules.md` — the field notes, the
  measurements, and the corrections made along the way.
