# Parallel front end

Source baseline: TSR `6d55ae54c2ca1062efdb16ceb2181b3c6a785591`;
native `5b1047d10d32e7d5b446be4de56b126ff42f82bb`.
Measured runtime: `d5ee4f365a2d320bdf7fa5a083c129303fca364f`;
cost probe: `57b381b3` (production runtime unchanged).
Fixture revision: `4d4f005c8541e0255a9d8791205fdce326e462bc`.
Work is tracked by `tsr-1yb.5`, `tsr-1yb.5.1.1`, `tsr-1yb.5.1.2`,
`tsr-1yb.22.1` and `tsr-1yb.22.2`.

The initial experiment below records root parsing and binding. The dependency
follow-up at the end records the newer loader behavior separately.

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

At the initial runtime `d5ee4f36`, the loader prepares readable, supported,
deduplicated roots already held by its existing read-prefetch stage. Dependency
discovery, path claims, package metadata and resolver replay stay serial;
discovered dependency parsing stays serial in that revision. Each prepared root
is published only when the existing replay
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
## Dynamically discovered dependency parsing

The follow-up in `tsr-1yb.5.2` extends private parsing to dependencies discovered
during the canonical walk. At native commit
`5b1047d10d32e7d5b446be4de56b126ff42f82bb`,
`internal/compiler/filesparser.go`'s `filesParser.start` claims a path and queues
`parseTask.load`; the task resolves references and queues more tasks before the
work group's final wait. TSR retains its existing coordinator claims and
resolver order and overlaps the pure parser part with that walk.

One `FileLoader` owns the pending preparations. The key is the exact normalized
file spelling, together with the loader's immutable dialect options. A cell is
absent, holds owned read text, holds a queued/completed private `ParsedFile`, is
consumed at the original first path claim, or is discarded when the loader
finishes. This is source preparation, not a completed semantic cache entry:
package identity, module format, receiver/alias interpretation, reference
discovery and diagnostics still belong to their existing canonical consumers.
Workers never receive the host, resolver, shared arena or node tables.

The pool leases workers from the existing process-wide front-end limit. It
starts for a broad frontier or an already substantial program, retains at most
twice its actual worker count in dependency cells, and refills while visiting
siblings. A full queue cannot stop the DFS walk: an unprepared current file
parses serially. Sources below 16 KiB or above 1 MiB remain serial, with their
already-read text consumed once at the canonical visit. Missing reads retain
their missing result. The lower bound is a measured scheduling heuristic, not
a tsgo semantic option. `singleThreaded` disables dependency workers.

Each parser owns its source, arena, node table, recovery rows and JSDoc image.
Publication copies into the program arena and rebases the complete AST using
the existing schema-generated contract. The private owner then drops. No new
unsafe implementation is introduced. Workers use the established 8 MiB stack
budget. Their result channels never wait for publication. In unwind builds,
parser panics are carried to the original visit; the release CLI retains its
existing abort-on-panic profile. Loader shutdown disconnects and joins the pool
before binding acquires another worker lease.

The cell count is not a byte or RSS bound. A serial oversized text can be large,
an AST can exceed its source size, and existing root preparations are outside
this dependency count. Read-ahead assumes a stable source snapshot and changes
physical read timing; semantic claims, resolution requests/traces and canonical
publication retain their previous order. The separate leased-read experiment
and native depth/claim fixes remain separate work.

`LoadStatistics` distinguishes submitted dependency jobs, published results,
actual worker count and peak pending cells. `dependency_parse_work` sums parser
durations only for consumed worker results; it overlaps coordinator work and
must not be added to wall-clock phases. Coordinator waits and canonical copies
remain in parse wall time. Root preparation still has its separate barrier.

Focused tests force two/default pool workers to finish out of order, propagate
worker panics and drop unconsumed results without blocking shutdown. A single
root discovers a wide mixed-language graph with cycles, duplicate references,
case variants, augmentation, malformed/missing files, small children and an
oversized source. Serial/default runs compare complete canonical node/parent
and AST images, JSDoc, source diagnostics, file order, package redirects and
ordered resolver requests/traces on both filesystem casing modes. Existing
root publication, binder identity and full-corpus gates remain required.

### Dependency confirmation

Baseline is the already root/bind-parallel TSR
`e222e6b2475f509a9bdf5d5a50745fef7403ca59`; the measured dependency runtime is
`d0ad694fc81563c85367a19a746ee18101a69953`. Final delivery also contains comments,
reproduction scripts and this evidence, without another production behavior
change. The JSON's `dependency_followup` retains source/binary hashes, every
paired row and warmup, input fingerprints, resources and qualified rejected
rounds. Builds, fixture setup and corpus runs were outside confirmed timings.

| Workload / metric | Root-parallel baseline | Dependency candidate |
|---|---:|---:|
| 768 dependency declarations, normal CLI (5 pairs) | 313.68 ms | 241.63 ms (-22.97%) |
| Same fixture user / system CPU | 351.94 / 89.84 ms | 430.57 / 100.46 ms |
| Same fixture median peak RSS | 636.83 MB | 658.80 MB |
| 192 declarations, instrumented CLI (5 pairs) | 113.17 ms | 93.16 ms |
| 192 declarations, normal CLI (3 pairs) | 104.27 ms | 104.61 ms |
| API instrumented CLI (5 pairs) | 8.456 s | 8.012 s |
| API coordinator parse wall | 552 ms | 541 ms |
| API normal CLI (3 pairs) | 7.289 s | 6.794 s |
| API normal CLI median peak RSS | 1,663.39 MB | 1,730.13 MB |

The broad fixture confirms whole-command frontend benefit with more CPU and
memory; the smaller normal fixture was flat. API aggregate medians improve,
but its instrumented median paired wall ratio is 0.991, and check-time variation
exceeds the parse change. Do not attribute the whole API drop to parser overlap
or claim a verified native 2x win. The first unrefilled frontier and unfiltered
refill rounds remain in the record; the latter submitted 10,040 API jobs and
raised parser/copy overhead. The final size admission submits/publishes 477 API
jobs on eight leased workers, with 16 maximum pending cells. The 192-file
fixture uses 16 workers, 32 pending cells and 199 consumed jobs.

Every comparison uses fresh processes, alternating order, identical effective
configurations, complete loaded-file order and diagnostic fingerprints, and
unchanged hashed inputs/binaries before and after each run. The instrumented
API counts remain 9,861 loaded, 615 checked and 10,555 coordinator parse or
publication calls, with the same four existing ioredis TS1005 errors. Normal
runs do not invent checked-count telemetry. Full checked-file identities,
resolver-query coverage and the native depth/trace gates remain broader work.

Generate the exact larger public input with
`python3 scripts/dependency_parse_fixture.py /tmp/dependency-fixture --files 768`.
Build the baseline/candidate in separate target directories, then use
`scripts/dependency_parse_perf.py --baseline <binary> --candidate <binary>
--baseline-source <sha> --candidate-source <sha> --project /tmp/dependency-fixture
--output /tmp/dependency-evidence --samples 5`, adding `--extended-diagnostics`
for phase attribution. Raw output stays local. The generator reproduces all
770 public input files byte-for-byte; the harness has an executed smoke check.

The final dependency runtime preserves all 477,970 positioned assertions:
469,765 RIGHT, 993 GAP and 7,212 WRONG, with zero RIGHT losses and byte-identical
TSVs (`acdc6a1df173b7b598b8627c3ad27dbd1758b94301a992722a02477a7f586b50`).
Binder, loader and diagnostic snapshots are also byte-identical to the
root-parallel baseline. All 116 targeted tests, Rust formatting and changed
crate Clippy with warnings denied pass. The scoped pool is retained for its
confirmed frontend benefit; broader native loader fidelity and the <=0.50
release ratio remain open in the parent tasks.
