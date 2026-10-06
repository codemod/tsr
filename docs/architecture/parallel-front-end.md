# Parallel front end

Source baseline: TSR `6d55ae54c2ca1062efdb16ceb2181b3c6a785591`;
native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Measured runtime: `d5ee4f365a2d320bdf7fa5a083c129303fca364f`;
cost probe: `57b381b3` (production runtime unchanged).
Fixture revision: `4d4f005c8541e0255a9d8791205fdce326e462bc`.
Work is tracked by `tsr-1yb.5`, `tsr-1yb.5.1.1`, `tsr-1yb.5.1.2`,
`tsr-1yb.22.1` and `tsr-1yb.22.2`.

## Experiment contract (CP0)

Target the actual CLI's parsing and binding, keeping loaded files, checked files,
source order, diagnostics and checker results unchanged. Compare serial, two
workers and default workers on the same source and input. Measure fresh-process
wall time, CPU, RSS and extended diagnostic phase times. Keep first-launch
security delays separate, retain their rows, and run warmed comparisons in
alternating order. Accept a production default only after whole-command benefit;
the independent release target remains TSR/native median wall ratio <=0.50 on
equivalent complete work, without previously RIGHT losses.

## Ownership inventory

Native `compiler/filesparser.go:242-267` queues independent file parse work;
native `compiler/program.go:472-486` queues independent file binds.
The serial Rust baseline parses into a caller-owned arena and binds into a
program-wide symbol/flow store.
`Arena` is Send and not Sync; worker allocation cannot use that shared arena.

AST node and token `node_id` fields are immutable `Option<NodeId>` after
registration. Generated alias `node_id()` and `HasNodeId` expose those fields;
direct reads remain legitimate. IDs also live in NodeTable parent rows, NodeMap
indices, JSDoc host entries, binder declarations/locals/facts/computed-name links
and diagnostic consumers. Speculation truncates registration tables, so only
finished reachable registration rows may be published. JSDoc nodes are included
even when outside the SourceFile syntax walk. Spans and diagnostic offsets are
file-relative and must not be shifted.
Unregistered hand-built nodes keep their absent IDs; copying assigns no new
registration identity to them.

The bounded parser candidate retains the existing field representation. Workers
own ParsedFile cells, private arenas and zero-based tables. Publication copies
syntax into the caller arena with an ordered node base, remaps every child and
JSDoc edge, and appends all table rows. Source slices borrow the canonical copy;
decoded strings are copied separately. States are private, finalizing, then
published immutable. No worker node or borrowed worker string escapes. Compared
with an accessor/atomic slot migration, this leaves all direct readers unchanged
and requires no new unsafe lifetime or allocator sharing. Its extra allocation,
copy time and transient double AST storage must be measured before adoption.

Binding must defer global merging until ordered publication. Symbol records
carry declaration/value NodeIds and parent/export/member SymbolIds. BindResult
also carries node-symbol columns, locals, globals/UMD exports, redirects,
conflicts, module and pattern augmentations, undefined, computed names, node-flow
columns, facts and end/return/fallthrough flow maps. FlowStore carries record
antecedents, label-list cells, switch-clause nodes and reduced label targets/list
heads; its overloaded auxiliary field must be relocated by flow kind. The
unreachable FlowId zero remains canonical. Symbol-store identity stays owned by
the coordinator; private stores cannot manufacture published checker handles.
Nil versus allocated-empty member/export tables must survive publication.

File-local binding does not read earlier globals. The coordinator must append
symbols in file order, replay script/UMD/global-augmentation merges, and synthesize
undefined at the same first-file boundary. Non-global module augmentations remain
the existing final program pass. Receiver and alias context remain the original
AST and published symbol links; no checker-owned type/member cache is shared.

Controls must exercise reversed completion, empty/skewed/malformed files, JSDoc,
TS/TSX/JS/JSON, duplicate globals, augmentations, aliases/cycles and branching flow.
Wrong-base and wrong-merge-order mutations must be rejected. Counts, identity
edges and complete diagnostics are correctness prerequisites, not speed wins.

## Concrete publication owners

| Data | Private owner and first permitted read | Canonical publication |
| --- | --- | --- |
| AST fields, typed children, tokens and lists | Completed ParsedFile cell; parser-local IDs | Schema-generated copy into the caller arena; every registered NodeMap row copied before any consumer runs |
| NodeTable kind/span/flags/parents | Private table; spans remain file-relative | Append in source order; shift only parent IDs by the file base |
| Publication memo | One copying operation, keyed by its private NodeId | Empty then copied immutable Node; memo and worker owner drop after publication |
| Source and decoded strings | ParsedFile source/arena until copying ends | Source slices borrow the equal canonical source; other strings are copied into its arena |
| JSDoc hosts/docs and recovery rows | Private cell and registration domain | Relocate hosts and all generated child links, including rows outside the root visitor |
| Constructed binder names | Coordinator-owned PreparedNames, keyed by exact lexical spelling | Canonical arena strings prepared once; immutable reads during file binds |
| Symbols, locals, exports, redirects and conflicts | One opaque FileBindResult; symbol IDs private | Append symbols and relocate every SymbolId before original ordered global merging |
| Flow records, label cells, clauses and reductions | Same private bind; graph indices private | Relocate kind-specific edges; preserve canonical zero including REFERENCED/SHARED flags |
| Node-symbol/flow columns and facts | File-bounded columns; NodeIds already canonical | Place columns at the file base; preserve NodeIds in declarations/facts/diagnostics |
| UMD aliases and global/undefined merging | Coordinator and accumulated canonical store | UMD files bind serially; replay globals and synthesize undefined at the original boundary |

PreparedNames changes storage for the binder's three allocating lexical operations:
numeric normalization, signed computed numeric names and quoted ambient modules
(`binder/binder.go:getDeclarationName`, pinned above). It stores no resolved alias,
receiver, type or member result. Ordinary source names still borrow their source.
FileBindResult retains its NodeTable owner; publication rejects a foreign table
before mutation. The existing checker symbol-store identity remains canonical.
The exhaustive consumer search, per-path counts and inventory hash are in the
[evidence record](parallel-front-end.json); generated copying covers every schema
field, so direct ID readers require no accessor migration.

## Scheduling and measured costs

The actual loader prepares readable, supported, deduplicated roots already held
by its existing read-prefetch stage. Dependency discovery, path claims, package
metadata and resolver replay stay serial; discovered dependency parsing also
stays serial. Each prepared root is published only when the existing replay
first claims it. A prepared result whose source changed is discarded. Unclaimed
roots may have incurred private parse work; that CPU and storage are included in
whole-process measurements, not represented as a new published file.

Direct Program file lists use the same private parse/publication seam. Parse
workers require at least eight files and 128 KiB total source. Binding workers
require eight unbound files and 10,000 nodes. `singleThreaded` disables frontend
workers; `checkers` continues to control only checker instances. Worker capacity
is capped process-wide at available parallelism, and competing Programs fall
back without waiting. There are at most two completed results per worker in the
channel/send pipeline. The loader's pending root ASTs have separate ownership
and can retain all prepared roots until their claims are replayed.

One-, two- and default-worker controls compare all AST rows/typed fields, JSDoc,
symbol/flow graphs, table presence and diagnostics. They force file zero to finish
after file one. The real loader integration also checks import cycles, duplicate
roots, globals, augmentation, JS/JSDoc, JSX, JSON, malformed and empty sources.
Executed wrong-node-base, wrong-symbol-offset and reversed-merge controls failed
and were restored. The foreign-owner and consumer-panic controls pass.

The ownership probe retains all private results to expose an upper envelope,
rather than modeling the production queue. On 192 declaration files with
876,480 nodes, five-sample median costs were:

| Phase | One worker | Two workers | Default (16) |
| --- | ---: | ---: | ---: |
| Private parsing | 43.61 ms | 22.26 ms | 5.92 ms |
| AST publication | 22.74 ms | 23.21 ms | 23.99 ms |
| Lexical-name preparation | 1.09 ms | 1.09 ms | 1.12 ms |
| Private binding | 30.53 ms | 15.88 ms | 4.22 ms |
| Symbol/flow publication and global merge | 10.77 ms | 10.94 ms | 9.09 ms |

Private AST arena requests total 38,736,384 bytes; canonical AST/source requests
total 42,015,346 bytes. These overlap during copying. The final binder reports
15,387,136 heap bytes. Arena requested bytes exclude allocator padding, table
capacity and temporary vectors; process RSS captures the combined allocation
cost. The probe records process CPU separately from per-phase wall timing.
Canonical copying and merging are now material serial costs in this probe.

Reproduce with `cargo run --release -p tsr-compiler --example front_end_costs --
DIR 2`, where DIR contains independent .d.ts files. This is an ownership-cost
probe; it omits loader/resolver/checker work and proves no release speed target.

## Whole-command confirmation and decision

Five alternating pairs per workload used fresh processes, incremental/composite
disabled, one retained warmup pair, unchanged hashed source inputs and identical
loaded-file order, published counts and full diagnostics. No builds or corpus
processes ran in this session during timing; other agents could use the host.
Raw wall/CPU/RSS rows, diagnostic outputs, input hashes and source/binary pins
are retained in [parallel-front-end.json](parallel-front-end.json).

| Median objective | Serial TSR | Parallel TSR | Change |
| --- | ---: | ---: | ---: |
| Frontend fixture CLI wall | 120.04 ms | 88.14 ms | -31.90 ms, -26.57% |
| Frontend fixture parse | 61 ms | 48 ms | -13 ms, -21.31% |
| Frontend fixture bind | 34 ms | 15 ms | -19 ms, -55.88% |
| Frontend fixture user / system CPU | 98.45 / 33.05 ms | 132.11 / 51.07 ms | More CPU work |
| Frontend fixture peak RSS | 182.03 MB | 281.43 MB | +99.40 MB, +54.61% |
| Real API CLI wall | 7.265 s | 7.143 s | -0.122 s, -1.68%; uncertain |
| Real API parse | 515 ms | 510 ms | -5 ms, -0.97% |
| Real API bind | 160 ms | 111 ms | -49 ms, -30.63% |
| Real API user / system CPU | 16.297 / 0.847 s | 16.259 / 0.891 s | Similar user CPU, more system CPU |
| Real API peak RSS | 1,593.62 MB | 1,669.15 MB | +75.53 MB, +4.74% |

The frontend fixture has 274 loaded/published files (192 generated declarations
plus libs), and checks zero files under skipLibCheck. It is a complete frontend
workload, not evidence of faster semantic checking. The API preserves 9,861
loaded files, 10,555 published parse calls, 615 checked files and four preexisting
TS1005 errors in ioredis's reserved `function` parameter declarations. Their
faithfulness bug is tracked separately in `tsr-1yb.35`.

The first API round was 6.792 s -> 6.869 s, with bind 146 ms -> 103 ms.
The final round's overlapping wall ranges were 7.158-8.004 s versus
6.710-7.711 s. Together they support faster binding, but do not establish a
reliable API wall improvement. The initial candidate's exact CLI/source hash
was not retained; only the final committed build is source-bound confirmation.
The prospectively recorded confirmation forecast was about 40 ms less binding
time and uncertain API wall benefit. The 49 ms reduction supports that estimate;
the frontend whole-wall forecast also held. No initial numeric forecast is
reconstructed after the fact, and the combined result is not split into invented
standalone contributions for parsing and binding.

Retain the integrated bounded frontend: its full CLI frontend workload benefits,
and real API binding improves without demonstrated whole-project regression.
CPU and memory overhead remain explicit costs. This run does not establish the
TSR/pinned-native <=0.50 release ratio: native equivalent complete work, resolver
query coverage and dynamic loader scheduling prerequisites remain unresolved.

The final runtime preserves all 477,970 positioned assertions: 469,765 RIGHT,
993 GAP and 7,212 WRONG. The full verdict TSV is byte-identical, with zero RIGHT
losses. Full binder_symbols (8,508/8,508), file_loader (96/96) and diagnostics
(4,221/5,502) snapshots are also byte-identical to the serial baseline. The 114
targeted tests, generator/crate Clippy with warnings denied, and the probe checks
pass. These are fidelity prerequisites, not improvements to diagnostic coverage.

This scoped implementation stops after one integrated candidate and two API
measurement rounds. Three intentional negative controls were rejected; no
measurement timed out. A shared Cargo target caused a baseline/candidate build
artifact collision during setup; separate targets fixed it, and final binaries
were rebuilt and source-bound. Whole-run cost is unknown; no paid judge calls
were made. The local experiment log remains under
`.context/compound-engineering/ce-optimize/parallel-front-end/`, with aggregate
evidence exported above. The worktree is retained for follow-up.
