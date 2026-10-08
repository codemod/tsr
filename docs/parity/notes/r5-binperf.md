# Parity lane `r5-binperf` — binder and non-JSDoc front-end hot paths (round 5)

Lane: `tsr-2zk.1039`, binder hot paths with no change to binding semantics,
after [`r5-bind.md`](r5-bind.md) §7 sized the binder at 16% of
generic-imports' Ir. Box protocol: `docs/parity/box-protocol.md`. Pinned
upstream: `vendor/typescript-go` @ `5b1047d`. JSDoc parsing and scanning
(`tsr-2zk.17.1`, main's) and the checker are not touched. Release target
(CLAUDE.md): TSR/tsgo median wall ratio <= 0.50 on equivalent complete work.

Sections are numbered so code can cite them (`r5-binperf.md` §N). Every
number names the source state and the measurement it came from.

## §1 Method

- Source base `4960842`: this branch's dispatch head with r5-bind's branch
  (`8fd5b4c`, the byte-wise block-comment skip) merged in, so the base
  already carries r5-bind §3. 4-vCPU cloud container, Linux 6.18, glibc
  `malloc`, `RUSTUP_TOOLCHAIN=stable`. Native tsgo built from the pinned
  submodule (`scripts/offline-cargo/build-tsgo.sh`).
- **Ir**: `valgrind --tool=callgrind` on a release `tsr` built with
  `CARGO_PROFILE_RELEASE_DEBUG=line-tables-only`, run as
  `tsr -p benches/projects/<p>/tsconfig.json --singleThreaded --pretty false`.
  Binder-only numbers add `--toggle-collect='*Program*bind_source_files*'`.
  Every Ir run's CLI output was `cmp`-identical to the base binary's.
- **Wall/CPU**: fresh processes, base / new / tsgo interleaved with the
  order rotated each round, one warmup round dropped, medians of wall and
  `wait4` user+sys over 31 rounds, with the default (multi-threaded) mode
  and `whole_project_perf.py`'s flags. Base and new stdout are compared on
  every run.

Base profile, generic-imports (373.06 M Ir): the binder
(`bind_source_files`) is 60.9 M; inside it `bind_inner` 18.2 M self
(`declare`'s per-kind matches inlined), `<&str, SymbolId>` table inserts
7.6 M of which 4.7 M is `reserve_rehash` (table growth), and
`name_nodes` (`FxHashMap<NodeId, NodeId>`) inserts 5.1 M. Outside the
binder the profile turned up a larger non-JSDoc item first: two whole-tree
walks per file in the loader (§2).

## §2 The loader's dynamic-import and `import.meta` walks are flag-gated

**Forcing measurement.** `push_children` was 8.5 M Ir on generic-imports
but only 3.5 M of it came from the binder. The rest was two whole-tree
walks the loader runs on every file, 133,221 and 132,906 visits:
`collect_dynamic_imports` (via `collect_external_module_references`) and
`is_file_probably_external_module`'s `import.meta` search — about 14 M Ir
with their own loop costs, almost all of it over `lib.dom.d.ts`, which
contains neither an `import(` nor an `import` type nor `import.meta`.

**Upstream's shape.** Both walks are gated on parser source flags:
`collectExternalModuleReferences` walks only when
`file.Flags&NodeFlagsPossiblyContainsDynamicImport != 0 || IsInJSFile`
(`parser/references.go:16`), and `getImportMetaIfNecessary` only when
`NodeFlagsPossiblyContainsImportMeta` is set (`ast/parseoptions.go:101`).
The parser sets the first in `parseImportType` (`parser.go:3027`) and on an
`import(`/`import<` call (`parser.go:5183`), the second on `import.<name>`
other than `defer` (`parser.go:5195`). `references.rs` had dropped the
dynamic-import gate as "only an optimisation"; r5-modules had already
ported `PossiblyContainsImportMeta` and the binder's `is_external_module_in`
reads it, but the loader's copy of the indicator walked unconditionally.

**Change.**
- The parser sets `POSSIBLY_CONTAINS_DYNAMIC_IMPORT` in the primary-type
  `import` arm (`parseImportType`, which `typeof import(…)` also goes
  through) and on every `import` keyword expression. The second is a
  superset of upstream's `(`/`<` lookahead; the flag only gates a walk, so a
  superset changes nothing it finds, and every `import(…)` callee the walk
  can match is a `KeywordExpression` built in that one arm.
- `collect_external_module_references` walks only when the flag is set or
  the file is JavaScript.
- `is_file_probably_external_module` takes the `NodeTable` and searches for
  `import.meta` only when `POSSIBLY_CONTAINS_IMPORT_META` is set; its one
  caller (`FileLoader::is_external_module`, `loader.rs`) passes the loader's
  table, where the file's rows already are.

No module-indicator rule changed: the same statements decide, and the
`import.meta` arm answers the same for every file (a file whose walk could
find `import.meta` has the flag by construction).

**Measured** (callgrind, `--singleThreaded`): generic-imports
373,064,096 → **352,539,427 Ir (−5.50%)**; domain-model 1,232,378,910 →
1,207,537,425 (−2.02%). CLI output `cmp`-identical on both.

**How we would know it is wrong.** A file whose dynamic import or
`import.meta` is reached by the walk but not by the flag: an `import` node
built outside the two parser arms (a reparse, a JSDoc-hosted expression
moved into the tree), which would drop a module reference — visible as a
missing resolution in a trace or a TS2307 that disappears.

## §3 Registered tokens skip `bind_inner`

**Forcing measurement.** Base `bind_inner` was 18.2 M Ir self on
generic-imports with `declare` and its per-kind tests inlined; roughly half
the nodes bound are tokens (identifiers, keywords, literals), and every one
ran the full sequence: the `bind_inner` prologue (a very large inlined
frame), the expando check, `record_flow`, `declare` with its seven per-kind
`match`es (`is_default_export`→`modifiers_of`, `name_node_of`,
`assignment_declaration_kind`, `anonymous_declaration`, the TypeParameter
arm, `classify` twice), and only then the `kind <= LAST_TOKEN` early return.

**Upstream's shape.** `bind` (`binder.go:572`) runs the flow half of its
switch and `bindWorker` for every node, then returns before
`GetContainerFlags` and the child walk when `node.Kind > ast.KindLastToken`
is false (`binder.go:726`). `bindWorker` has no arm that declares a token
kind.

**Change** (`Binder::bind`, `Binder::bind_token`). `bind` reads the kind of a
registered node first; a token goes to `bind_token`, which does exactly
what `bind_inner` did for it — `record_flow`, then what `declare` leaves
behind for a non-declaring node (`default_export_declaration = None`) — and
records the depth the leaf would have reached (`max_depth`). A
`debug_assert!` checks that no declaring arm (`name_node_of`,
`modifiers_of`, `anonymous_declaration`, the kind-only `classify`, and the
kinds `Binder::classify`/`assignment_declaration_kind` special-case)
matches the token, so a future declaring arm for a token kind fails the
debug test suite rather than silently binding nothing.

**Measured:** generic-imports 352,539,427 → **349,539,491 Ir (−0.85%)**;
domain-model 1,207,537,425 → 1,203,757,273. Smaller than the inlined
costs suggested, because `record_flow` is now an out-of-line call
(1.66 M) and `bind` itself grew (4.66 M self); `bind_inner` fell from
18.2 M to 10.5 M.

## §4 `name_nodes` is an append-only log

**Forcing measurement.** `name_nodes: FxHashMap<NodeId, NodeId>` (declaration
→ name node) took one insert per named declaration — 56,381 on
generic-imports, 5.1 M Ir inclusive with 1.5 M of growth rehash — and is
read only by `declaration_name_span`, i.e. only when a redeclaration
diagnostic is reported. It is private to one file's `Binder` (reset in
`resuming_with_names`) and never iterated.

**Change.** Writes push `(declaration, name)` onto a `Vec`; the first read
hashes every row not yet indexed into `name_index` (in write order, so a
later row for the same declaration replaces an earlier one, as the map's
`insert` did) and answers from it. Reads stay O(1) amortised, so a file
with many redeclarations costs one hash per declaration, as before; a file
with none hashes nothing.

**Measured:** generic-imports 349,539,491 → **345,084,338 Ir (−1.27%)**;
domain-model 1,203,757,273 → 1,198,132,683.
