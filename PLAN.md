# Porting `microsoft/typescript-go` to Rust

**Status:** planning
**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`
**Reference corpus:** `vendor/typescript-go/testdata/baselines/reference` (49,354 baseline files)

---

## 1. Decisions

| Decision | Choice |
|---|---|
| **Scope** | Everything: scanner → parser → binder → checker → transformers → emit → compiler driver → tsc CLI → language service → LSP server → project system |
| **Fidelity** | Idiomatic Rust rewrite (not a mechanical transliteration) |
| **Correctness oracle** | Port the Go test harness; run against the full upstream `testdata` conformance corpus and diff against upstream baselines |

### Consequence of the fidelity choice

An idiomatic rewrite permanently forks from upstream. typescript-go is under active
development, and its 60k-LOC checker receives a steady stream of correctness fixes.
Those fixes cannot be diffed and replayed onto our tree; each one must be re-derived
by hand against a structurally different codebase.

We contain this with two mechanisms, both built in Phase 0 and treated as
load-bearing rather than nice-to-have:

1. **Drift tracker** — a scheduled job walks upstream commits since our pin, classifies
   each by the `internal/` package it touches, and files a `bd` issue against the
   corresponding Rust crate. Nothing gets silently dropped.
2. **Baseline ratchet** — the conformance pass-rate is a CI gate that may only go up.
   An upstream fix we failed to port shows up as a baseline diff, not as a silent
   behavioral divergence discovered by a user.

---

## 2. What we are actually porting

`vendor/typescript-go` @ `5b1047d10`:

- **300,987 LOC** non-test Go, 47 `internal/` packages
- **827,846 LOC** of Go tests
- **288 MB / ~50k files** of `testdata`

Non-test LOC by package (top 20):

```
60,269  checker          9,860  fourslash        3,533  binder
39,321  ls               9,701  api              3,131  vfs
24,353  transformers     9,582  execute          2,780  module
21,371  lsp              9,423  diagnostics      2,746  core
20,878  ast              9,040  parser           2,286  modulespecifiers
11,442  printer          6,175  tsoptions        1,435  tspath
11,209  project          5,694  compiler         1,126  pseudochecker
                         5,564  stringutil       1,110  bundled
                         5,446  fswatch
```

Expect **250k–400k LOC of Rust**. This is a multi-engineer, multi-quarter effort;
the phasing below is designed so that each phase produces something independently
verifiable rather than deferring all validation to the end.

### Load-bearing upstream facts

- **386 AST node kinds** (`internal/ast/kind_generated.go`).
- Upstream already uses **arena allocation** (`internal/core/arena.go`, `Arena[T]`),
  which makes the arena-and-index approach below a smaller conceptual jump than it
  looks.
- Generated code (`ast_generated.go`, `kind_generated.go`, diagnostics) is derived
  from the real TypeScript sources in the nested `_submodules/TypeScript` submodule.
  Our codegen must read the same inputs so an upstream bump regenerates cleanly.
- `unsafe` in upstream is confined to `fswatch`, `nativepath`, and one
  `unsafe.String` in `tspath` — i.e. platform syscall glue, not core algorithms.
  Nothing in the compiler core needs `unsafe` in Rust.

---

## 3. Architecture

### 3.1 The central problem

Go's object graph is cyclic and mutably shared:

```go
type Node struct {
    Kind Kind; Flags NodeFlags; Loc core.TextRange
    id atomic.Uint64
    Parent *Node          // back-pointer
    data nodeData         // interface, ~386 implementations
}

type Type struct {
    flags TypeFlags; objectFlags ObjectFlags; id TypeId
    symbol *ast.Symbol
    alias *TypeAlias
    checker *Checker      // back-pointer to the owning checker
    data TypeData
}
```

`Node.Parent`, `Type.checker`, and the `Symbol ↔ Type ↔ Node` cycles make a direct
`Rc<RefCell<…>>` translation both unwritable and slow. It would also forfeit the
single biggest reason to be in Rust here: cheap, sound parallelism.

### 3.2 The answer: arenas + typed index handles

Every cross-referencing entity becomes a newtype index into an arena owned by its
phase:

```rust
#[derive(Copy, Clone, PartialEq, Eq, Hash)] pub struct NodeId(u32);
#[derive(Copy, Clone, PartialEq, Eq, Hash)] pub struct SymbolId(u32);
#[derive(Copy, Clone, PartialEq, Eq, Hash)] pub struct TypeId(u32);
```

- **AST:** one `enum Node` with 386 variants (large payloads boxed to keep the enum
  small), stored in an `IndexVec<NodeId, Node>` owned per `SourceFile`. Children and
  `parent` are `NodeId`. Exhaustive `match` replaces Go's type switches and the
  `nodeData` interface — this is where idiomatic Rust genuinely wins.
- **Symbols/Types:** arenas owned by the `Checker`. `Type.checker` simply
  disappears: methods take `&Checker`.
- **Payoff:** a parsed `SourceFile` becomes plain data — `Send + Sync`, trivially
  shareable across threads, serializable for incremental caching.

**Open design problem (spike required before Phase 4):** the checker memoizes
lazily — it computes and caches types while already holding references into its own
arenas. Rust's borrow checker forbids the naive form. Candidate resolutions:
append-only arenas with interior mutability (`elsa`-style), id-returning `&mut self`
methods that never hand out long-lived `&Type`, or a `RefCell`-guarded memo table
separate from the arena. **This decision constrains 60k LOC and must be settled by a
prototype, not by discussion.**

### 3.3 Crate layout

A Cargo workspace. Crates consolidate upstream packages where the split was a Go
import-cycle artifact rather than a real boundary.

```
tsr-core          core, collections, stringutil, jsnum, semver, glob
tsr-path          tspath, nativepath
tsr-vfs           vfs trait + os/in-memory impls, bundled libs
tsr-diagnostics   generated diagnostic messages (codegen)

tsr-ast           node arena, 386 kinds, flags, symbols, astnav, positionmap
tsr-scanner       scanner
tsr-parser        parser, JSDoc

tsr-binder        binder, flow graph
tsr-module        module + type resolution, modulespecifiers, packagejson
tsr-checker       checker  ← 60k LOC, internally sharded (§4, Phase 4)

tsr-transformers  ts→js downlevel, jsx, decorators, esm/cjs
tsr-printer       emitter, sourcemap
tsr-compiler      Program, driver, outputpaths

tsr-tsoptions     tsconfig parse/validate
tsr-execute       tsc CLI, build mode, incremental
tsr-ls            language service
tsr-lsp           LSP server, jsonrpc
tsr-project       project system, ATA
tsr-fswatch       platform watchers
tsr-api           programmatic API surface

tsr               the binary (tsc / lsp / api entry points)
xtask             codegen from _submodules/TypeScript; drift tracker
tsr-testharness   baseline runner, fourslash DSL, compiler-test harness
```

### 3.4 Parallelism as a design constraint

typescript-go's headline advantage is parallel checking. `Send + Sync` arenas must
be a Phase-1 design requirement, not a Phase-9 retrofit — retrofitting thread-safety
into an arena design is a rewrite, not an optimization.

---

## 4. Phases

Each phase is gated on a measurable conformance number, not on "the code is written".

### Phase 0 — Foundations
Workspace skeleton, CI, `xtask` codegen (diagnostics + node kinds from
`_submodules/TypeScript`), upstream-drift tracker, `tsr-core` / `tsr-path` /
`tsr-vfs` / `tsr-diagnostics`.
**Gate:** codegen reproduces the full diagnostic set; drift tracker files issues.

### Phase 1 — Scanner, AST, Parser
The AST arena design lands here and everything downstream inherits it. Verify by
parsing the entire corpus and diffing a canonical AST + parse-diagnostic dump
against the Go implementation.
**Gate:** 100% of corpus parses with byte-identical parse diagnostics.

### Phase 2 — Binder
Symbol tables, declaration merging, control-flow graph construction.
**Gate:** symbol-table dumps match Go across the corpus.

### Phase 3 — Module resolution & tsconfig
`node16`/`nodenext`/`bundler` resolution modes, path mapping, `tsoptions`.
**Gate:** upstream module-resolution baselines pass.

### Phase 4 — Checker (the mountain)
Preceded by the memoization spike (§3.2). Sharded into parallelizable workstreams:
relations/assignability · inference · generics & instantiation ·
unions/intersections/indexed access · control-flow narrowing · contextual typing ·
overload resolution · JSX · decorators · declaration emit types.
**Gate:** type-baseline and error-baseline pass-rate ratchet toward 100%.

### Phase 5 — Transformers & printer
Downlevel emit, JSX, decorators, module transforms, sourcemaps.
**Gate:** emit baselines byte-identical.

### Phase 6 — Compiler driver & `tsc`
`Program`, watch mode, incremental, `--build`.
**Gate:** upstream `tsc` CLI baselines pass; self-hosting check on real repos.

### Phase 7 — Language service
Split by feature: completions · quickinfo · goto-definition · find-references ·
rename · organize-imports · auto-import · formatting · code fixes.
Requires porting the **fourslash** DSL (9.8k LOC) first.
**Gate:** fourslash tests pass.

### Phase 8 — LSP, project system, file watching
`fswatch` is the one place to seriously evaluate an existing crate (`notify`) rather
than porting upstream's hand-rolled inotify/fanotify/FSEvents/ReadDirectoryChangesW
layer.
**Gate:** editor smoke tests against VS Code.

### Phase 9 — Performance & parallelism
Benchmark against `tsgo` and `tsc`. Tune parallel checking, arena layout,
string interning.
**Gate:** at or below typescript-go wall-clock on a fixed repo suite.

---

## 5. Conformance harness (starts in Phase 0, runs continuously)

- Port `internal/testrunner` + `internal/testutil/harnessutil` baseline machinery.
- Use upstream `testdata/baselines/reference` (49,354 files) verbatim as the oracle.
- Port the `fourslash` DSL before Phase 7.
- CI publishes `passing/total` per baseline category; the number is a ratchet.

**Note:** the conformance *inputs* live in the nested `_submodules/TypeScript`
submodule, which must be initialized (`git submodule update --init` inside
`vendor/typescript-go`). Only 341 test files live directly in
`vendor/typescript-go/testdata/tests`.

---

## 6. Risks

| Risk | Mitigation |
|---|---|
| Upstream drift on a 60k-LOC checker we rewrote idiomatically | Drift tracker + baseline ratchet (§1) |
| Checker memoization vs. the borrow checker | Blocking spike before Phase 4 |
| AST design mistake propagating into every downstream crate | Phase 1 gate is corpus-wide diffing, not unit tests |
| Effort underestimated | Every phase gated on a conformance number, so progress is measured rather than asserted |
