# ADR-0056: Rebasable node ids and per-file bind for a zero-copy parallel front end

- **Status:** **proposed**, not built. It is the design record for
  `tsr-2zk.17.4` (rebasable node ids) and `tsr-2zk.17.5` (per-file binding
  with id-offset merge), which `tsr-2zk.17.3` (parse and bind source files
  in parallel) is blocked on. The round-7 integrator decides whether to fund
  it from the measured gain below.
- **Date:** 2026-10-10
- **Would supersede:** [ADR-0012](0012-ast-is-sync.md) in part: the
  `node_id` field becomes write-once-after-construction instead of a plain
  field. It would also amend [ADR-0034](0034-a-program-needs-one-identity-space.md)'s
  "every file parses into one allocator".
- **Related:** [`docs/architecture/parallel-front-end.md`](../architecture/parallel-front-end.md),
  [`docs/parity/notes/r5-loader.md`](../parity/notes/r5-loader.md) §3/§6,
  [`docs/parity/notes/r7-perf.md`](../parity/notes/r7-perf.md) §7/§12

## The forcing constraint

Native tsgo parses and binds each file on its own goroutine, straight into
the `SourceFile` that *is* the program's file (`filesparser.go:245-301`,
`program.go:472-486`). There is no publication step.

TSR's parallel front end parses and binds into **private** tables, and the
coordinator **publishes** each file into the canonical ones in file order
(`parallel-front-end.md`):

- the AST is copied into the canonical arena, with every node id and child
  edge remapped (`ParsedFile::publish`);
- the binder's symbols and flow nodes are relocated and the globals merged
  (`BindResult::publish_file`).

That publication is serial, and the parallel work only wins when it saves
more than publication adds. On a project whose largest file is
`lib.dom.d.ts` it does not. r5-loader §3 therefore gates parallel bind off
when the largest file is more than half the nodes, and r5-loader §6.2
refused a side-pool parse of `lib.dom.d.ts`. generic-imports and
domain-model run their whole front end serially today.

### Measured (`front_end_ceiling`, release, minimum of 7 rounds per file, 4 workers)

`cargo run --release -p tsr-compiler --example front_end_ceiling -- <tsconfig> 4 7`
times each program file alone. It loads through the real loader with the
CLI's parse options, then models critical paths. It ignores thread
start-up and memory bandwidth, so the result is an upper envelope.

| project | files | nodes | `lib.dom` parse + bind | **serial** (today, gi/dm) | copy-parallel (today's design, all fanned out) | **zero-copy** (this record), merge 0 … full |
|---|---:|---:|---:|---:|---:|---:|
| generic-imports | 66 | 133,292 | 15.4 + 6.2 ms | **27.1 ms** | 31.6 ms | **22.0 – 24.3 ms** |
| domain-model | 105 | 180,088 | 16.2 + 6.5 ms | **36.9 ms** | 34.3 ms | **21.8 – 24.6 ms** |
| domain-model-large | 265 | 365,528 | 15.2 + 6.4 ms | 69.6 ms | **53.5 ms** (today) | **23.0 – 31.2 ms** |

Sums across files: publication is 7.3 / 9.7 / 22.2 ms of AST copy and
2.3 / 2.8 / 8.2 ms of bind relocation plus merge, against 19.0 / 25.3 /
44.8 ms of parsing.

**Expected gain per process**, from today's path to zero-copy:

- generic-imports: **2.8–5.1 ms** (7–12% of its ~42 ms wall)
- domain-model: **12.3–15.1 ms** (11–14% of ~106 ms)
- domain-model-large: **22–30 ms** (5–7% of ~458 ms)

The floor on all three is `lib.dom.d.ts`'s own parse and bind, about 22 ms
on one thread. No file-level parallelism goes below it. tsgo pays the same
floor on one goroutine: its parse phase on generic-imports is 36 ms of wall.

## The proposal

### 1. Rebasable node ids (`tsr-2zk.17.4`)

Today the generated `node_id: Option<NodeId>` is a plain field, assigned
once at registration with an absolute id because `parse_into` appends to
the shared tables. A private parse numbers from zero, so publication must
rewrite every node, which is why it copies.

- The generated field becomes write-once-after-construction: an
  `AtomicU32`-backed `NodeIdCell` (relaxed loads; `Sync`, so ADR-0012's
  shareable tree survives), read through the generated `node_id()`
  accessor. A private parse stores local ids. Publication is one pass over
  the file's registered nodes that adds the file's base: one store per
  node, and no allocation or copy. `NodeTable::append_published` (rows
  appended with parent ids shifted, as today) and `NodeMap` (pointers to
  the existing nodes) complete it.
- The worker's arena is **kept**, not copied out of. A program owns an
  append-only pool of per-file arenas with stable addresses: a `tsr-core`
  `ArenaPool` handing out `&'a mut Arena` once per slot. The coordinator
  takes a slot per job and sends the worker the `&'a mut Arena`. `Arena:
  Send` makes that sendable, and the worker's AST borrows `'a`, the
  program's lifetime. This needs one audited `unsafe` in `ArenaPool`
  (append-only, each slot handed out once), alongside `arena.rs`'s existing
  ADR-0011 exception.
- Read sites: **2,994** `.node_id` reads outside generated code, 2,146 of
  them in `tsr-checker`. The field-to-accessor change is mechanical (the
  generated alias `node_id()` already exists), done by codegen plus one
  rewrite.

### 2. Per-file bind with id-offset merge (`tsr-2zk.17.5`)

`bind_file` + `publish_file` already exist and are verified equal to
serial binding (`parallel_tests.rs`). With rebasable node ids, a worker can
bind its file right after parsing it, against its local node ids, which
are rebased with the AST. Symbol and flow ids stay private and are
relocated at merge, as today. That relocation is the "merge" column above,
and the range brackets it: rebasing does not remove it. Making symbol ids
file-relative as well is out of scope. It would be the next record if the
merge column turned out to be the remaining floor.

### 3. Scheduling

One job per file (parse, then bind) on the existing bounded workers. The
coordinator publishes in file order as results arrive, so identities stay
in program order. The r5-loader §3 gate and the static-lib exclusion in
`prepare_dependencies` (r5-loader §2) go away, because publication no
longer costs a copy.

## Alternatives

- **Keep the copy and tune the gates** (today). Measured above:
  copy-parallel is worse than serial on generic-imports (31.6 against 27.1
  ms) and only slightly better on domain-model (34.3 against 36.9 ms).
- **Two-level node ids** (file index + local id). This removes publication
  entirely but changes every `NodeId`-keyed dense side table (node table,
  node map, checker links) into a two-level index. It wins only if the
  `AtomicU32` patch pass ever shows up as material, which at one store per
  node is not expected.
- **Reserved, sparse id ranges per file** (base = previous base + an upper
  bound from the text length). This needs no patch pass, but the dense
  tables become 10–20× sparse (lib.dom: 2.35 MB of text for 124 K nodes).
  Refused on memory.
- **Splitting `lib.dom.d.ts` at top-level declarations** to parse it in
  parallel. This attacks the floor itself, but it is a heuristic with no
  upstream counterpart: native parses each file on one goroutine. Refused.

## Consequences if adopted

- An AST field type changes across generated code and about 3,000 read
  sites.
- Per-file arenas live as long as the program does. That is the same
  memory the canonical copy holds today, minus the copy, so peak RSS
  should fall by the private-copy overlap that `parallel-front-end.md`
  measured.
- One new audited `unsafe` (`ArenaPool`).
- The front end's floor remains `lib.dom.d.ts` at about 22 ms. This record
  buys at most the gains above. It does not reach the 0.50 target on its
  own; dml's remainder is in the checker.

## How we would know this was wrong

- The built version's critical path on generic-imports/domain-model is not
  within the modelled zero-copy range (22–25 ms) when measured with
  `--extendedDiagnostics` parse + bind. Either the patch pass or the merge
  is larger than modelled.
- Peak RSS rises against today's serial path on the same projects.
