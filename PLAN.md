# Porting `microsoft/typescript-go` to Rust

**Status:** Phase 0 in progress — workspace, foundations, and a conformant AST landed
**Upstream pin:** `vendor/typescript-go` @ `5b1047d10`
**Reference corpus:** 80,521 test inputs (`_submodules/TypeScript/tests`) against 49,354 reference baselines (`testdata/baselines/reference`)
**Prior art studied:** [oxc](https://github.com/oxc-project/oxc) @ `5e5178b`

---

## 1. Decisions

| Decision | Choice |
|---|---|
| **Scope** | Everything: scanner → parser → binder → checker → transformers → emit → compiler driver → tsc CLI → language service → LSP server → project system |
| **Fidelity** | Idiomatic Rust rewrite (not a mechanical transliteration) |
| **Correctness oracle** | Port the Go test harness; run the full upstream conformance corpus and diff against upstream baselines |
| **AST** | Our own, built from scratch to match TypeScript's AST shape. Not `oxc_ast` — the shapes do not match (§8) |
| **Relationship to oxc** | Inspiration, not dependency. We take oxc's *designs* and its *third-party dependencies*, but link no `oxc_*` crate (§3.1) |
| **AST model** | Arena-allocated tree + struct-of-arrays side tables keyed by `NodeId` (§3.3) — **revised from the first draft, see §3.2** |

### Consequence of the fidelity choice

An idiomatic rewrite permanently forks from upstream, and typescript-go's 60k-LOC
checker receives a steady stream of correctness fixes that cannot be diffed and
replayed onto a structurally different tree. Two mechanisms contain this, both built
in Phase 0 and treated as load-bearing:

1. **Upstream-anchored doc comments, lint-enforced.** Every ported item names its
   upstream counterpart and the pinned commit. This is oxc's own practice in
   `oxc_type_checker` — e.g. `//! Corresponds to typescript-go's ast.SourceFile
   (internal/ast/ast.go)`. It is what makes drift tracking mechanical instead of
   archaeological.
2. **Drift tracker.** A scheduled job walks upstream commits since our pin,
   classifies each by the `internal/` package it touches, resolves that to the Rust
   items claiming to port it (via 1), and files a `bd` issue. Nothing is silently
   dropped.
3. **Baseline ratchet.** Conformance pass-rate is a CI gate that may only go up
   (§6). An upstream fix we failed to port surfaces as a baseline diff, not as a
   silent divergence found by a user.

---

## 2. What we are porting

`vendor/typescript-go` @ `5b1047d10`: **300,987 LOC** non-test Go across 47
`internal/` packages, **827,846 LOC** of tests, **288 MB** of `testdata`.

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

Load-bearing facts:

- **351 AST node kinds** (`internal/ast/kind_generated.go`). Note upstream's
  `_scripts/ast.json` lists only 349 — it lags the generated Go by two
  (`DeferKeyword`, `JSDocAllType`), which is why conformance is asserted against
  the Go source (§6).
- Upstream already uses **arena allocation** (`internal/core/arena.go`).
- Upstream already keeps checker state in **side tables, not on nodes** —
  `internal/core/linkstore.go` defines `LinkStore[K, V]` and a paged variant, and
  `checker.go` holds **23+ separate link stores** (`nodeLinks`, `valueSymbolLinks`,
  `typeAliasLinks`, …) keyed by `*ast.Node` / `*ast.Symbol`. This matters a lot;
  see §3.2.
- `.Parent` is read **2,092 times** across `checker`/`ls`/`binder`. Parent access
  must be O(1) and allocation-free.
- Generated code derives from the real TypeScript sources in the nested
  `_submodules/TypeScript` submodule (now initialized).
- `unsafe` upstream is confined to `fswatch`, `nativepath`, and one `unsafe.String`
  in `tspath` — platform syscall glue, not core algorithms.

Expect **250k–400k LOC of Rust**. Multi-engineer, multi-quarter.

---

## 3. Architecture

### 3.1 Inspiration, not dependency

oxc is the most advanced Rust JS/TS toolchain in existence (~960k LOC) and has
solved, in production, most of the infrastructure problems this port would
otherwise hit. We take its designs and its dependency choices; we do not link its
crates. The `oxc_*` workspace crates are pre-1.0 (currently `0.138.0`) and release
weekly with breaking changes — the wrong thing to pin a multi-year, 300k-LOC port's
foundational types to. Their *dependencies*, by contrast, are mature and stable, and
oxc's selections are a well-tested bill of materials we can adopt directly:

| Need | Crate oxc uses | Notes |
|---|---|---|
| Arena | `allocator-api2` + `hashbrown` | **`oxc_allocator` is not bumpalo** — it's a custom allocator over the `Allocator` trait. Their `AllocatorPool` (thread-safe reuse across parallel workloads) and fixed-size allocators are the designs to copy |
| Typed indices | `index_vec` + `nonmax` | `NonMaxU32` ids make `Option<NodeId>` 4 bytes via niche optimization |
| Self-referential AST storage | `self_cell` | §3.4 |
| Hashing / maps | `rustc-hash`, `hashbrown`, `papaya` | `papaya` is a concurrent hash map — relevant to parallel checker link stores |
| Diagnostics | `miette` (oxc maintains the `oxc-miette` fork) | labeled spans, LSP-compatible rendering |
| Parallelism | `rayon` | |
| Small strings / vectors | `compact_str`, `smallvec` | replaces tsgo's string handling |
| Number formatting | `dragonbox_ecma` | directly covers tsgo's `jsnum` (584 LOC) — ECMAScript float-to-string |
| Scanner | `memchr`, `simdutf8`, `unicode-id-start`, `phf` + `phf_codegen` | `phf` for keyword lookup |
| Globs / JSONC | `fast-glob`, `json-strip-comments` | tsconfig `include`/`exclude` and JSONC parsing |
| LSP | `tower-lsp-server`, `ropey` | replaces much of tsgo's 21k-LOC `lsp`; `ropey` for incremental document edits |
| Position lookup | `rust-lapper` | interval tree — tsgo's `astnav` / `positionmap` |
| Graphs | `petgraph` | flow graph, module graph |
| Testing | `insta`, `similar`, `criterion2` | `similar` replaces upstream's `patience` diff for baselines |

So `tsr-core`, `tsr-path`, `tsr-vfs` and `tsr-diagnostics` do get written, but they
are thin TS-specific layers over the above, not from-scratch infrastructure.

**oxc is already porting typescript-go.** `crates/oxc_type_checker` (2,274 LOC,
explicitly "experimental") has ported the tsgo *driver shell* — `vfs`, `tspath`,
`tsoptions/tsconfigparsing`, `compiler/program`, `compiler/fileloader`,
`compiler/references`, `execute/tsc` — with doc comments naming the upstream Go
files. The checker itself is a 144-line no-op scaffold. Our approach is therefore
independently validated by a credible team, and those 2,274 LOC are a worked
reference for the same shell we have to build.

### 3.2 The central problem, and why the first draft was wrong

Go's object graph is cyclic and mutably shared — `Node.Parent`, `Type.checker`, and
`Symbol ↔ Type ↔ Node` cycles. A direct `Rc<RefCell<…>>` translation is unwritable
and forfeits the main reason to be in Rust.

The first draft proposed making the AST itself index-based: a 386-variant enum in
`IndexVec<NodeId, Node>` with children stored as `NodeId`. **Studying oxc says
that's the wrong cut.** Both oxc and typescript-go independently converged on a
different split, and the agreement of two mature implementations is worth more than
my derivation:

> **The tree is a tree. Everything cyclic lives in side tables keyed by id.**

- oxc: the AST is a plain bump-allocated tree of `&'a mut` nodes with no back-edges.
  `oxc_semantic::AstNodes<'a>` then holds `nodes: IndexVec<NodeId, AstNode<'a>>`,
  `parent_ids: IndexVec<NodeId, NodeId>`, `flags: IndexVec<NodeId, NodeFlags>`,
  `cfg_ids: …` — parent is a *side table*, not a field.
- typescript-go: `LinkStore[*ast.Node, NodeLinks]` and 22 sibling stores hold all
  checker state off-node, plus `PagedLinkStore` keyed by dense integer ids — which
  is an `IndexVec` with paging in all but name.

So the port is *more* natural than the first draft assumed. Children stay as direct
arena references (fast to build, no id indirection on the hot traversal path), and
every cyclic or phase-specific relationship — parent, symbol, scope, type,
flow-node, and all 23 checker link stores — becomes an id-keyed side table.

### 3.3 The resulting model

```rust
// Ids: NonMax-backed, so Option<Id> is 4 bytes (oxc_index "nonmax").
pub struct NodeId(NonMaxU32);
pub struct SymbolId(NonMaxU32);
pub struct TypeId(NonMaxU32);

// AST: bump-allocated tree, children are direct references.
// `node_id` is a Cell so the binder can stamp ids without &mut on the tree —
// oxc uses exactly this pattern for `scope_id: Cell<Option<ScopeId>>`.
pub struct CallExpression<'a> {
    pub span: Span,
    pub node_id: Cell<NodeId>,
    pub callee: Expression<'a>,
    pub arguments: Vec<'a, Argument<'a>>,
}
```

- **Parent + flags:** struct-of-arrays side tables keyed by `NodeId`. oxc has a
  `multi_index_vec!` macro (modeled on Zig's `MultiArrayList`) that packs N
  parallel `IndexVec`s into one allocation with a single length, capacity, and
  bounds check. With 2,092 parent reads in the checker alone, this is the right
  shape and we should copy the macro's approach.
- **Symbols/Types:** `IndexVec` arenas owned by the checker. `Type.checker` simply
  disappears — methods take `&Checker`.
- **Checker links:** the 23 `LinkStore`s become id-keyed side tables. Dense ones
  become `IndexVec<NodeId, T>`; sparse ones a paged vec mirroring `PagedLinkStore`.

### 3.4 Long-lived ASTs and the LSP — solved, not open

An arena AST parameterized by `'a` is awkward to *store*: an LSP holds hundreds of
parsed files across edits, and `(Allocator, Program<'a>)` is self-referential.
This was an open risk in the first draft. oxc has shipped the answer, in
`oxc_type_checker::compiler::source_file` and `oxc_linter`:

```rust
self_cell! {
    struct SourceFileCell {
        owner: SourceFileOwner,          // Allocator + owned source text
        #[covariant]
        dependent: SourceFileData,       // Program<'a> + ModuleRecord<'a>
    }
}
// SAFETY: the cell owns the arena together with everything borrowing from it,
// with no outside borrows, so moving the cell moves the arena with its dependents.
unsafe impl Send for SourceFileCell {}
```

`self_cell` + one audited `unsafe impl Send` per self-referential container. Parsed
files then move freely between rayon workers. Adopt directly.

### 3.5 Parallelism

Per-file `Allocator` drawn from an `AllocatorPool`, rayon across files, arenas
reset and returned rather than dropped. This must be a Phase-1 design requirement:
retrofitting thread-safety into an arena design is a rewrite, not an optimization.
The checker is program-wide rather than per-file; upstream runs multiple checker
instances, and we mirror that.

### 3.6 Codegen is a first-class subsystem

oxc spends **19,970 LOC** in `tasks/ast_tools` generating, from `#[ast]`-annotated
definitions: `AstKind`, visitors (`Visit`/`VisitMut`), traversal with ancestor
access, ESTree serialization, `.d.ts` type definitions, and **struct-size
assertions** that fail CI if a node grows. With 351 node kinds and 192 node types, hand-writing and
hand-maintaining this is not viable. Our `xtask` must generate:

- node kinds + diagnostic catalogue from `_submodules/TypeScript`
- visitors and traversal
- the canonical AST dump used for conformance diffing against Go (§6)
- size assertions on every node type

### 3.7 Crate layout

```
tsr-core          arena + AllocatorPool, typed indices, side-table macro, CompactStr
tsr-path          TS-specific path normalization (tspath, nativepath)
tsr-vfs           vfs trait + os/in-memory impls, bundled libs
tsr-diagnostics   TS diagnostic-code catalogue (codegen) over miette

tsr-ast           node definitions matching TypeScript's shape, AstKind, side tables
tsr-scanner       lexer
tsr-parser        parser, JSDoc

tsr-binder        symbols, scopes, flow graph
tsr-module        TS module + type resolution, modulespecifiers, packagejson
tsr-checker       the checker  ← 60k LOC, sharded (§4)

tsr-transformers  ts→js downlevel, jsx, decorators, esm/cjs
tsr-printer       emitter, sourcemaps
tsr-dts           isolatedDeclarations-style .d.ts emit (ships before the checker, §4)
tsr-compiler      Program, driver

tsr-tsoptions     tsconfig parse/validate
tsr-execute       tsc CLI, build mode, incremental
tsr-ls            language service
tsr-lsp           LSP server
tsr-project       project system, ATA
tsr-fswatch       platform watchers
tsr-napi          Node bindings (napi-rs)

tsr               the binary
xtask             codegen, drift tracker
tasks/coverage    conformance runner + committed snapshots
tasks/benchmark   continuous benchmarks
```

---

## 4. Phases

Each phase is gated on a measurable conformance number, not on "the code is
written". **Performance and memory are per-PR CI gates from Phase 0** — oxc runs
`tasks/benchmark`, `tasks/minsize`, and `tasks/track_memory_allocations`
continuously, and that is why they are fast. The first draft's "Phase 9 —
Performance" was a mistake; a final tuning phase remains, but the gate is
continuous.

### Phase 0 — Foundations
Workspace on oxc crates; `xtask` codegen; drift tracker; coverage + benchmark
harnesses; the upstream-anchoring lint.
**Gate:** codegen reproduces the full diagnostic set and all 386 kinds; drift
tracker files issues; CI publishes coverage and benchmark numbers.

**Status (2026-08-03):** both ratchets shipped. `.github/workflows/perf.yml` runs
`cargo xtask perf` on every PR, gates wall clock and peak RSS against
typescript-go at the pin, and uploads `perf-results.json` and `perf-summary.md`.
Currently parse throughput is 1.36-1.38x on large files and peak RSS is 1.74x lower — see
[docs/architecture/performance.md](docs/architecture/performance.md) for the
method and the caveats, and [ADR-0009](docs/adr/0009-performance-gate.md) for why
the comparison is against upstream rather than our own history.

### Phase 1 — Scanner, AST, Parser
The AST model (§3.3) lands here and everything downstream inherits it.
**Gate:** 100% of the corpus parses with byte-identical parse diagnostics vs. Go.

### Phase 2 — Binder
Symbol tables, scopes, declaration merging, flow graph.
**Gate:** symbol-table dumps match Go across the corpus (compare against oxc's
`symbols_typescript` at 21.1% — this is harder than it looks).

### Phase 3 — Module resolution & tsconfig
`node16`/`nodenext`/`bundler`, path mapping, `tsoptions`, on `oxc_resolver`.
**Gate:** upstream module-resolution baselines pass.

### Phase 3.5 — First shippable artifact (no checker required)
`tsr-dts` (isolatedDeclarations-style `.d.ts` emit) and syntax-only transforms.
oxc ships `oxc_isolated_declarations` in 4,066 LOC without any checker. This exists
so the project delivers usable output roughly a year before P4 completes.

### Phase 4 — Checker (the mountain)
Preceded by the memoization spike. Sharded into parallelizable workstreams:
relations/assignability · inference · generics & instantiation ·
unions/intersections/indexed access · control-flow narrowing · contextual typing ·
overload resolution · JSX · decorators · declaration-emit types.
**Gate:** type- and error-baseline pass-rate ratchets toward 100%.

### Phase 5 — Transformers & printer
**Gate:** emit baselines byte-identical.

### Phase 6 — Compiler driver & `tsc`
`Program`, watch, incremental, `--build`.
**Gate:** upstream `tsc` CLI baselines pass; self-hosting on real repos.

### Phase 7 — Language service
Split by feature: completions · quickinfo · goto-definition · find-references ·
rename · organize-imports · auto-import · formatting · code fixes. Requires porting
the **fourslash** DSL (9,860 LOC) first.
**Gate:** fourslash tests pass.

### Phase 8 — LSP, project system, file watching
Evaluate the `notify` crate against porting upstream's hand-rolled
inotify/fanotify/FSEvents/ReadDirectoryChangesW layer. Evaluate `salsa`
(rust-analyzer's query engine) against upstream's explicit incremental model —
these are genuinely different architectures and the choice belongs here, informed
by real editor latency.
**Gate:** editor smoke tests against VS Code.

### Phase 9 — Final tuning
Close the gap against `tsgo` and `tsc` on a fixed repo suite. Distinct from the
continuous per-PR gate above.

---

## 5. Node bindings

`tsr-napi` via napi-rs, following oxc's `napi/` layout. Worth naming as a
first-class deliverable rather than an afterthought given the consumer.

---

## 6. Conformance harness (Phase 0 onward, continuous)

- Port `internal/testrunner` + `internal/testutil/harnessutil`.
- Use upstream `testdata/baselines/reference` (49,354 files) verbatim as the oracle.
- Port the `fourslash` DSL before Phase 7.
- **Copy oxc's snapshot format exactly.** `tasks/coverage/snapshots/*.snap`, each
  pinned to an upstream commit and committed to the repo:

  ```
  commit: 7539c04d

  types_typescript Summary:
  AST Parsed     : 8364/8364 (100.00%)
  Positive Passed: 78/8364 (0.93%)
  ```

  Committing the snapshot makes every regression a reviewable diff in the PR that
  caused it. This is the ratchet, made concrete.

### Reality check from oxc's own numbers

oxc's current pass rates against the TypeScript corpus:

| Suite | Pass rate |
|---|---|
| `parser_typescript` (positive) | **100%** |
| `codegen_typescript` | **100%** |
| `transformer_typescript` | 99.80% |
| `estree_typescript` | 99.81% |
| `semantic_typescript` | 73.94% |
| `symbols_typescript` | 21.10% |
| `types_typescript` | **0.93%** |

Read that bottom row carefully. The strongest team in Rust JS tooling, with a
production parser at 100%, is at **under 1% on type baselines**. Parsing and
emitting TypeScript are solved problems; *checking* it is not, by anyone, in Rust.
This is hard evidence for the §4 phasing — and the reason Phase 3.5 exists.

---

## 7. Risks

| Risk | Mitigation |
|---|---|
| Upstream drift on a checker we rewrote idiomatically | Upstream-anchored doc comments (lint-enforced) + drift tracker + baseline ratchet (§1) |
| Checker memoization vs. the borrow checker | Blocking spike before Phase 4; no direct prior art in oxc, so this is genuinely novel work |
| AST design mistake propagating downstream | Phase 1 gate is corpus-wide diffing; §3.2 model is now validated by two independent implementations rather than derived from first principles |
| Building our own AST and parser costs a phase oxc would have given us free | Accepted deliberately (§8). Bounded: tsgo's scanner+parser is 13.3k LOC and is validated exhaustively against 49,354 baselines |
| Writing our own infrastructure instead of linking `oxc_*` | Bounded by adopting oxc's *dependencies* (§3.1), so what we write is thin TS-specific layers, not allocators and diff algorithms from scratch |
| Effort underestimated | Every phase gated on a number; Phase 3.5 delivers value early |
| Duplicating oxc's effort | Talk to them early (§8) |

---

## 8. Why we build the AST from scratch

**Decided: our own AST, matching TypeScript's shape.**

`oxc_parser` reaches **100%** on the TypeScript corpus and `oxc_ast` covers full TS
syntax, so adopting them would have deleted Phase 1 outright. We are not adopting
them, because `oxc_ast` is *deliberately* not TypeScript's AST. Per their
`ARCHITECTURE.md`, oxc removes what it calls estree's "ambiguous nodes" — splitting
`Identifier` into `BindingIdentifier` / `IdentifierReference` / `IdentifierName`,
and following ESTree conventions throughout. That is a good decision for a linter
and a bundler. It is the wrong substrate for this port.

typescript-go's 60k-LOC checker, its 39k-LOC language service, and all 49,354
baselines are written against TypeScript's AST shape. The checker dominates total
cost and its fidelity *is* the product. Paying a per-function translation tax
across 60k LOC to save a 9k-LOC parser port — one we can validate exhaustively
against those baselines — is a bad trade, and it would put a permanent semantic
seam between us and every upstream fix we need to track.

So: TypeScript's node kinds, TypeScript's node shapes, TypeScript's `SyntaxKind`
numbering. What we take from oxc is *technique*, catalogued in §3.1 (dependencies),
§3.2–3.4 (side tables, `self_cell`), §3.6 (codegen), and §6 (conformance
snapshots).

**Still worth doing: talk to the oxc maintainers.** They have the tsgo driver shell
and no checker; we want the checker. Even with no shared code, that is worth a
conversation rather than discovering in six months that two teams built the same
thing twice.
