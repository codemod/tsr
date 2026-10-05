# Threading

**Status:** production loading, binding and checking remain serial. The
conformance harness is parallel. A fully bound `Program` is verified `Send + Sync`,
and independent checker instances can read it concurrently without sharing their
allocator or mutable type state (2026-10-03, `bd tsr-1yb.3`).

Parallel checking is the headline reason typescript-go is fast, and PLAN.md §3.5
makes thread-safety a design requirement rather than a later optimisation —
retrofitting it into an arena design is a rewrite, not a tuning pass. This
document records what is true today and what each next step needs.

## File preparation and parsing have different ownership

The production loader allocates every file into one caller-owned arena and
appends to one `NodeTable`/`NodeMap`. Parsing those files concurrently cannot use
the historical standalone-file `par_iter` model without changing identity and
lifetime contracts. Independent `ParsedFile` values remain useful for standalone
tools and conformance cases; they are not directly mergeable program files.

An earlier independent boundary is an owned file read. A worker can borrow a
shareable backing filesystem and a file-name string, then return `Option<String>`.
The coordinator retains discovery, task claiming, package identity, arena
allocation, parsing and replay order. The opt-in read probe described below
exercises this boundary without changing the production loader.

## What is shareable today

Updated 2026-08-03, when the AST became `Sync`
([ADR-0012](../adr/0012-ast-is-sync.md)).

| Type | `Send` | `Sync` | Note |
|---|:--:|:--:|---|
| every node, `Node`, alias unions, `Token` | ✅ | ✅ | no interior mutability |
| `SourceFile` | ✅ | ✅ | a parsed tree can be read by any number of threads |
| `NodeTable` | ✅ | ✅ | plain vectors |
| `Arena` | ✅ | ❌ | `alloc` bumps a pointer through `&self`; one per thread |
| `ParsedFile` | ✅ | ❌ | owns an `Arena` |

The line to remember: **the tree is shareable, the allocator is not.** That is
enough for parallel binding and checking over an already-parsed file — the case
that motivates parallelism at all — and it is not enough to share one `ParsedFile`
between threads, which would need a `Sync` arena or a frozen view. Neither exists.

Both properties are asserted at compile time in `crates/tsr-parser/tests/parse.rs`
and exercised by a test that walks one tree from four threads. The assertions are
the only thing keeping them true: a `Cell` added to a generated node would break
that test and nothing else.

### The rule this implies for later passes

The binder will want to memoise and the checker will want to cache, and the
shortest path to both is a `Cell` or a `RefCell` on the node. That would silently
undo this. Per-node mutable state goes in a side table keyed by `NodeId`
([ADR-0003](../adr/0003-tree-plus-side-tables.md)), which can be locked, sharded,
or made thread-local independently of the tree.


## `Arena` is `Send`, deliberately not `Sync`

```rust
unsafe impl Send for Arena {}
```

An arena exclusively owns its chunks, and the raw pointers it stores point only
into them. Moving it moves the memory, so values allocated on one thread stay
valid after the arena is sent to another.

It is **not** `Sync`, and must not become so: `alloc` mutates the bump pointer
through `&self`, so two threads allocating at once would race. A test asserts the
absence of `Sync` — if that ever starts failing, someone made concurrent
allocation possible without auditing it.

## `ParsedFile` is the sendable unit

A parsed AST borrows from two things: the arena its nodes live in and the source
text its identifiers point into. `(Arena, String, SourceFile<'a>)` is therefore
self-referential, which Rust will not let you name, let alone move — and moving it
is exactly what handing a parse result back from a worker requires.

`ParsedFile` bundles them with [`self_cell`], which stores the owner alongside the
dependent and hands out the borrow only through a closure. One audited
`unsafe impl Send` covers the whole thing. oxc solves the same problem the same
way in `oxc_type_checker::compiler::source_file`.

The tree is reachable only via `with_ast(|source_file| …)`. Handing out
`&SourceFile` directly would let a caller name a lifetime tied to storage the cell
owns, which is the thing `self_cell` exists to prevent.

## Determinism is a requirement, not a nicety

The conformance harness collects outcomes in **case order** and tallies afterwards,
so the committed snapshot is byte-identical regardless of how work was scheduled.
Verified at 1, 3, and default thread counts.

A ratchet whose output depends on thread timing would be worse than no ratchet:
every run would produce a diff, and real regressions would hide in the noise.

Measured on the corpus (12,444 cases, release build): **~25 s of CPU in ~4.6 s
wall**, about 5.7× on this machine.

## What each next step needs

**The binder** currently accumulates one program-wide `SymbolStore` in order.
Global merges and shared symbols require an ownership design before parallel
binding, even when per-file syntax is already immutable.

**The checker does not.** It is program-wide: types and symbols are interned in
tables shared across files, and laziness means one file's checking can force
another's. Upstream runs multiple checker instances rather than sharing one, and
that is the model to copy — but it interacts directly with the memoisation
question in `bd tsr-6n3`, which is still unresolved and has no prior art in oxc.
Decide that first; the concurrency shape follows from it.

**Arena reuse.** Every file currently allocates a fresh arena and drops it. oxc
pools them (`AllocatorPool`) so a long-running process reuses chunks instead of
returning them to the allocator. Worth doing when the LSP holds many files, not
before — measure first.

**`NodeTable` and syntax side tables** are program-wide in the production loader.
They remain plain data and can be shared after construction. Mutable checker
link stores belong to private checker instances; sharing completed syntax does
not justify sharing checker-local type IDs or caches.

## The current program and checker ownership boundary

The historical per-file `ParsedFile` description above predates ADR-0034's
program-wide identity. The production `ProgramFile` now borrows its tree and text
from the caller's arena; it does not own a `self_cell`. The program owns the node
tables, bound symbols, file indexes and resolved-module map. None contains an
allocator reference or interior mutation after construction. A test in
`crates/tsr-execute/tests/checker_ownership.rs` asserts `Program: Send + Sync`
without making `Arena` Sync or adding any unsafe implementation.

The ownership decision is:

1. Finish discovery, parsing, binding and global declaration merges before
   publishing a shared `&Program`. Mutating methods retain their exclusive
   `&mut Program` requirement. The arena's owner stays alive through the worker
   scope; workers never receive the allocator.
2. Build a `Checker` inside each worker. Its `TypeStore`, links, alias caches,
   inference contexts, resolution stacks and mapper caches stay private. Workers
   share program-wide `NodeId` and `SymbolId` identities; a `TypeId` belongs to
   exactly one checker and must never enter another worker's cache.
3. Each checker can lazily force a type from any program file through the
   immutable module host. A worker's assigned file group limits diagnostic work,
   not the declarations it can read. Lazy augmentation or linking changes must
   preserve this boundary rather than introduce shared mutation in `Program`.
4. Return owned diagnostics and file identities. Collect in program order and
   normalize duplicate diagnostics deterministically. For declaration emit or
   type printing, use the checker that created the type, or perform an independent
   query; transferring a bare `TypeId` is invalid.

The compiling test seam runs independently constructed checkers at two and three
workers over cross-file generic imports, imported constructors, true assignment
errors and globally merged interfaces. Their normalized diagnostics match a
single checker. This establishes the ownership boundary on those controls; it
does not establish whole-app determinism or ship a parallel CLI. Those belong to
`bd tsr-1yb.6`, with complete-project measurements and broader augmentation,
recursion and emit controls before enabling workers by default.

Start production measurements at 1, 2 and 4 checker instances. Preserve native
checker-count selection separately from execution or memory admission policy;
`singleThreaded` must force one. Native does not clamp instance counts to CPU
availability. Any adaptive TSR default must be an explicit policy decision,
with its effect on full-array affinity verified. Measure per-worker initialization and redundant
cross-file type forcing before choosing the final default. The initial real-app
measurements show 0.926 GB peak RSS with `noCheck` and 1.118 GB with checking. This
suggests immutable program storage dominates, but subtracting two high-water marks
is not an exact measure of private checker memory. Keep this as a planning signal,
not a proven bound. Verify memory and latency under the actual worker pool.

Loading and parsing need their own design. They currently fill shared node tables
and allocate through one arena; the ready-to-share finished program does not make
those construction phases parallel. Per-file/worker allocation, ID assignment and
deterministic global merges remain work in `bd tsr-1yb.5`.

## File-read preparation seam

`cargo build --release -p tsr-compiler --example loader_reads` builds an opt-in
read-plan probe. Its input is a predetermined manifest of absolute physical paths,
one per line, followed by `direct`, `1`, `2` or `4`. It does not discover a project,
parse configuration, provide a production worker policy or enable CLI workers.
The direct mode uses the calling thread; worker modes use persistent scoped
threads with bounded per-worker channels. A batch contains at most one read per
worker and is consumed completely before another is submitted.

Workers receive a path borrow and `&F` where `F: FileSystem + Sync`. They return
owned `Option<String>` and primitive counters. They do not receive the arena,
node tables, resolver, task graph, package identities, bound symbols or checker
types. `OsFileSystem` and `InMemoryFileSystem` pass the shareability checks. The
current `CachedFileSystem` and `Arena` fail actual compiler `Sync` checks;
`Cell`/`RefCell` caches must stay on the coordinator. Merely adding `Sync` to a
trait object or wrapping the arena in an unsafe implementation cannot satisfy
this boundary.

The consumer runs on the coordinator and can borrow the arena and exclusive
node tables without being `Send` or `Sync`. A test parses while consuming reads
and compares complete ASTs, parents/flags/spans, node ranges, typed map recovery,
JSDoc, directives and parse diagnostics against direct serial construction.
No node remapping is needed: every node is allocated and numbered by the same
serial parser. Missing reads consume a slot and allocate no syntax nodes.
Separate frozen-host controls compare complete loader requests/results, trace
order, package redirects, syntax and checker diagnostics at 1/2/4 workers.
They exercise cycles, duplicate roots, symlinks, both filesystem case modes,
duplicate package identities, merged globals and TS/TSX/JS/JSON. A controlled
out-of-order completion test proves overlap and ordered consumption; another
proves consumer unwinding closes the workers. These are six passing controls,
with no ignored tests.

The frozen-host adapter is test-only and rejects reads outside its finite plan.
It retains prepared strings to replay the loader, so its memory use is not the
streaming queue bound. Production reads remain live and uncached. Parallel reads
from a mutable or non-shareable custom host need an explicit contract; the probe
does not cast the erased `&dyn FileSystem` from `ResolutionHost` into a shareable
host. An eventual serial fallback must preserve custom-host behavior.

The native task graph remains a separate requirement. Pinned
`filesparser.go:start` claims canonical paths while tracking file-name casing,
lowest reached depth, package identity and redirected work. Its
`getProcessedFiles` walks the graph deterministically, replays per-task type
traces before module traces, chooses the first package instance in replay order,
then sorts library files. Rust `loader.rs:process_task` still claims a path once
in depth-first order and does not reproduce native casing/depth reprocessing.
`collect_task` separately performs postorder collection and package redirects.
A completed `--listFiles` list is therefore not the production parse-order plan.
Workers must not assign node IDs or select package winners in completion order.
Read-plan equivalence alone does not prove dynamic discovery, augmentation,
global merge or native task-graph fidelity.

The source-specific cost controls in
[loader-read-preparation.json](loader-read-preparation.json) use 13,097 physical
paths from the previous loaded-file observation, including pinned physical libs,
and read 73,506,200 decoded bytes. This omits package metadata and discarded
tasks read during real discovery. The external harness
[loader-read-costs.py](loader-read-costs.py) runs serial fresh processes in two
rounds, each with one warmup and five rotated samples per mode. All 48 read
summaries are byte-identical, and original-byte input fingerprints stay fixed.
Warm OS filesystem pages are allowed; this is not a cold-disk benchmark.

| Mode | Round 1 median wall | Round 2 median wall | Round 1 median peak RSS | Round 2 median peak RSS |
| --- | ---: | ---: | ---: | ---: |
| Direct serial | 184.9 ms | 186.8 ms | 21.8 MB | 21.8 MB |
| 1 read worker | 247.7 ms | 248.1 ms | 34.0 MB | 33.9 MB |
| 2 read workers | 171.1 ms | 174.6 ms | 31.9 MB | 39.2 MB |
| 4 read workers | 121.3 ms | 122.8 ms | 38.3 MB | 38.7 MB |

Four workers save about 64 ms on this fixed read plan, with greater CPU and RSS.
The one-worker queue is slower than direct reads. Observed maximum completed
batch text is 4,795,155 bytes at four workers, versus a largest individual string
of 4,549,667 bytes. These are logical decoded string lengths, not allocator
capacities or a memory ceiling: decoding may temporarily hold both raw bytes and
text, the allocator may retain blocks, and one oversized file can exceed any
chosen byte budget. The maximum outstanding read count does not prove bounded
process RSS. Process wall includes thread startup/join, read/decode, hashing,
output and queue overhead; individual read intervals can overlap and must not be
added to wall. Separate startup attribution and constrained-memory behavior
remain open.

Eight existing physical encoding inputs, plus a missing file and a directory,
produce identical direct/1/2/4 read summaries. Prior native diagnostic controls
characterize the eight encodings; this read probe does not re-establish all
malformed-byte native semantics. The measurement manifest and private file names
remain local; the committed evidence records fingerprints and counters.

This proves a compiling ownership seam and a small isolated read opportunity,
not a whole-project improvement. `tsr-1yb.19` retains dynamic-plan/native replay,
augmentation and constrained-resource acceptance. Production scheduling stays in
`.5`, behind its existing `.3`/`.2` gates and `.19`; the full comparable native
median ratio of 0.50 remains unverified. Prioritize the larger unique metadata and
discovery costs before turning this prototype into a production executor.

## Not yet built in production

- Parallel parsing in any real driver — only the harness uses it.
- Arena pooling.
- Parallel binding; it runs serially.

The CLI checks with the native pool (`crates/tsr-execute/src/checker_pool.rs`,
`checkerpool.go`): the count and file affinity below, one scoped worker per
checker, and each checker publishing only diagnostics located in files it
owns, as native `GetSemanticDiagnostics` asks the file's associated checker.
The controls in `crates/tsr-execute/tests/checker_ownership.rs` now compare
the probe seam against that pooled CLI. Root-file reads run ahead of the
loader walk through `FileSystem::read_files`: `OsFileSystem` reads them
concurrently, every other host reads serially, and parsing and node
numbering stay on the walk. The memory and real-app controls of `tsr-1yb.3.1`
and `tsr-1yb.3.2` were not rerun for this change.

## Opt-in worker measurement probe

`cargo build --release -p tsr-execute --example checker_workers` builds a
standalone ownership/cost probe. Run the resulting absolute executable from the
project directory with `/absolute/tsconfig.json` and an optional positive
32-bit count such as `1`, `2`, `4`, or `8`. The probe shares a fully bound Program and gives each scoped
worker a private checker; file affinity follows the complete Program array
index modulo checker count, before filtering eligible files. With no positional
override it uses parsed `checkers`, or native's default of four. A positional
count replaces parsed `checkers`; `singleThreaded` then forces one. The final
count is clamped to 1 through the smaller of the full Program array length and
256, including libraries and skipped declarations. It no longer silently caps
explicit counts at four or available CPUs. CPU availability is telemetry, not a
count input. This selection matches pinned `checkerpool.go`; it does not approve
a production default or establish a memory bound.

Stdout contains checker diagnostics after comment directives. Stderr records
loaded/checked identities, phase times and each worker's checked count, initial
and final type counts, expression computations and initialization/check time.
Type counts are not allocation bytes, and expression computations are not all
generic instantiations. Measure external process wall/CPU/peak RSS separately.
The probe is not a replacement for CLI config, parse/bind diagnostics or emit.

### Affinity, diagnostic merging and query ownership

Pinned `internal/compiler/checkerpool.go:createCheckers` associates every source
file with `checkers[index % checkerCount]` before filtering checked files.
The probe and ownership controls preserve that full-array index, including the
bundled-lib prefix and
skipped declaration files. Filtering first changes private cache affinity.
Within a group, files retain Program order.

Native `GetChecker` grants exclusive access to one mutable checker; a nil file
hint selects the first checker. Its nonexclusive emit access is valid only when
the caller guarantees no concurrent access. TSR's scoped seam constructs,
checks and queries each checker in its worker, then returns owned strings and
diagnostics. It does not require `Checker: Send`, share mutable type caches, or
transfer `TypeId` values. A future retained pool must preserve exclusive access
for queries after checking, even if checking itself has finished.

Every private checker calls `report_merge_conflicts` once. Disjoint file groups
therefore still produce duplicate program-wide conflicts. The probe coordinator
now sorts and deduplicates raw diagnostics before applying comment directives,
using the supported portion of native `ast/diagnostic.go:CompareDiagnostics`
and `compiler/program.go:SortAndDeduplicateDiagnostics`: file name, full span,
code and message arguments. Node identity or rendered text alone is insufficient.
TSR's current Diagnostic has no message-chain or related-information fields;
their comparison and native related-information union must extend this seam
when those fields are ported. This is not full diagnostic-model parity.

The expanded controls compare complete formatted output with the existing
serial CLI at 1/2/3/4 workers. Passing controls cover cross-file generic imports,
constructors and merged globals; actual duplicate merge conflicts; a skewed
96-error file; directives and queries after checking; a nonempty lib prefix
with skipped declarations; and `noCheck`. A standalone merge fixture also
produces byte-identical output from the CLI, probe at 1/2/4 workers and pinned
tsgo. Local evidence is `/tmp/tsr-1yb-worker-controls/results.json`.

A separate full-app correctness replay retains all 123 complete diagnostics,
13,097 loaded files and 1,341 checks at 1/2/4 workers, matching the fresh serial
CLI. Input contents are unchanged before and after the runs. This replay is
not a throughput benchmark; evidence is
`/tmp/tsr-1yb-worker-merge-nextjs.json`.

The recursive generic import control now matches pinned native output:
the numeric assignment is accepted, the string assignment reports TS2322
against `number`, and query-after-check retains `A<number>` at 1/2/3/4 workers.
`tsr-6.48` was a diagnostic elaboration read that bypassed the concrete receiver's
mapper, rather than a worker identity defect. The enabled controls also cover
inherited/defaulted and mapped members, mutable literals and const assertions;
see [receiver diagnostics](checker-receiver-diagnostics.md).

The module augmentation control still exposes `tsr-6.49`: augmented interface
members are invisible to imports. Its native-expectation test remains explicitly
ignored and does not count as passing worker readiness. That fidelity gap still
blocks `tsr-1yb.3.2`, which precedes production workers.

Checker-backed declaration emit cannot yet be exercised: the CLI behaves as
`noEmit`, and `tsr-declarations` uses `SyntacticResolver` while mutating its
syntax node table. `tsr-fe9` tracks the missing CheckerResolver. Future semantic
emit must query its exclusive owning checker and keep mutable emit storage
separate from the frozen Program; the existing syntax emitter does not prove
worker/emit equivalence.

After fixing diagnostic normalization (`tsr-1yb.1.4`), the exploratory probe's
complete 123 diagnostics match the saved serial CLI, including multiline
messages. The normalizer now strips ANSI decoration and ends message
continuations at summary/phase headings; it does not attach summary table rows
to the last diagnostic. Negative controls retain changes to messages, related
information, locations and codes.

Five fresh-process samples per mode on source `23563207`, after one warmup per
mode and with rotated order, measured the same Next.js workload:

| Workers | Median wall | Wall range | Median CPU | Median peak RSS | Program | Check/join |
|---|---:|---:|---:|---:|---:|---:|
| 1 | 4.733 s | 4.668–4.948 s | 4.728 s | 1.040 GB | 2.327 s | 2.214 s |
| 2 | 3.823 s | 3.698–3.926 s | 5.052 s | 1.073 GB | 2.355 s | 1.275 s |
| 4 | 3.363 s | 3.306–3.426 s | 5.311 s | 1.046 GB | 2.325 s | 0.833 s |

Every run checked the same 1,341 files from 13,097 loaded files, with stable
content fingerprints and the same complete diagnostics as a fresh serial CLI
control. Four workers reduced probe wall time by **28.9%** against one worker,
while CPU rose 12.3%. RSS ranges overlap, so this establishes no memory
reduction or exact private-worker memory bound. Median worker initialization
totals were below 1.1 ms; lazy checking, not construction, creates most private types.

Summed final type counts were 187,759/230,491/280,423 at 1/2/4 workers, and actual
expression computations were 247,852/264,017/278,122. These counts were stable
across samples. Four workers create 49.4% more final type entries and perform
12.2% more expression computations than one. Balanced file counts still yield
skew: in one four-worker sample, checking ranged from 0.492 to 0.761 s and one
worker retained 104,752 types versus 50,693 in another. These are checker-local
counts, not independent memory estimates or native instantiation counters.

The historical evidence supports evaluating four checker instances. The native
selection audit below supersedes the probe's former CPU/four cap; available CPUs
can limit execution without changing instance count or file affinity. The
historical measurements do not approve a production default: broader augmentation/recursion/emit/query controls and
constrained-memory behavior remain in `tsr-1yb.3.1` and `tsr-1yb.3.2`. The explicit
native count boundary now has direct controls, while the remaining memory and
fidelity requirements still precede production scheduling.

Loading remains about 2.3 s, already above half the previously observed native
total of 3.305 s. Checker parallelism alone cannot reach the overall 2x target.
This is a TSR probe comparison; the CLI remains serial and native scope/config
alignment remains incomplete. Local evidence is
`/tmp/tsr-1yb-worker-scaling-repeated.json` and
`/tmp/tsr-1yb-worker-diagnostic-normalization.json`.


### Native count selection and memory admission

`checker_workers/selection.rs` follows pinned native
`internal/compiler/checkerpool.go:40-48`: default four, `singleThreaded` before
explicit `checkers`, then `max(min(count, full-file-count, 256), 1)`. An empty
Program still selects one checker. Defensive zero/negative internal values
clamp to one; the standalone probe rejects nonpositive, fractional, malformed
or out-of-i32-range positional counts before reading the project. This probe
argument parser is not proof of compiler CLI numeric parsing parity.

Two hundred observations taken by calling the pinned native `newCheckerPool`
are committed as a Rust regression fixture. Native results are byte-identical
at default, `GOMAXPROCS=1` and `GOMAXPROCS=16`. GOMAXPROCS controls native Go
execution capacity, not the number of checker instances; it is not a simulated
Rust host CPU limit or a memory constraint. Public probe controls compare full
loaded-file order and complete diagnostics with normal TSR and pinned native,
including parsed/positional eight, skipped declarations, all-skipped checking
and `singleThreaded`. See [selection evidence](worker-selection-policy.json)
and [the portable controls](worker-selection-controls.py).

No measured per-checker byte upper bound exists yet. Type-store counts and
expression computations expose duplication and skew, but cannot certify
concurrent admission under a declared memory budget. Unknown or constrained
memory therefore remains a production gate in `tsr-1yb.3.1.1`: retain the serial
CLI and do not present this opt-in probe as memory-safe admission. Serial
checking itself also has no hard RSS ceiling. Do not silently rewrite an
explicit native count to satisfy an unproved estimate. A future resource policy
must either prove its bound, reject an unenforceable requested limit explicitly,
or document an intentional selection/affinity policy change. A queue, CPU limit
or subtraction of independently sampled RSS maxima proves none of these.


On source `064d40db`, one warmup per mode and five rotated fresh-process samples
refresh the measurement on **new input bytes**: 14,015 loaded files, 1,364 checks
and 122 complete diagnostics. Every probe process preserves these identities,
physical inputs and diagnostics against the fresh normal CLI controls before and
after the samples. The normal CLI exposes loaded identities but no direct
checked marker; actual checking identities and completed group counts come from
the probe. Serialized effective options also stay fixed at both boundaries;
`checkers` is omitted from `showConfig` by both compilers, so its value is proved
by `POLICY` and native constructor/parser controls instead.

| Checkers | Median wall | Wall range | Median CPU | Median peak RSS | Median Program | Median check/join |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| 1 | 4.233 s | 4.072–4.262 s | 4.168 s | 1.081 GB | 1.610 s | 2.308 s |
| 2 | 3.139 s | 3.036–3.408 s | 4.320 s | 1.146 GB | 1.579 s | 1.270 s |
| 4 | 2.596 s | 2.531–2.680 s | 4.616 s | 1.135 GB | 1.596 s | 0.754 s |

Four versus one reduces probe wall 38.7% while increasing CPU 10.7%. Each mode's
summed final types and expression computations are stable across the five
samples: 184,223 / 223,219 / 269,246 types and 250,905 / 266,244 / 279,995
computations at 1 / 2 / 4. Four therefore retains 46.2% more type entries and
performs 11.6% more expression computations. Initialization totals remain
small (about 1.2 ms in a four-checker sample), while that sample's private check
intervals range 0.569–0.706 s. RSS ranges overlap. None of these counts is a
per-checker byte bound, and builds/tests were serial outside measurements;
other agents' host activity was not controlled.

At the measured source above, the config transport control was
**not native-equivalent**: a base config with `checkers: 8, singleThreaded: true`
and an own config with `2, false` yielded native `2, false`, while TSR retained
`8, true` and selected one. This historical failure remains in the measurement
artifact.

The follow-up `tsr-1yb.3.1.1.2` corrects config/CLI transport: own and CLI worker
values replace inherited values, and explicit null clears them. Compiler CLI
counts use native machine-width integer parsing and the minimum of one, without
rounding large integers through JSON's floating representation. The standalone
probe still accepts only positive-i32 positional overrides; wider parsed project
counts use the native full-file/256 pool clamp. See the
[transport contract and reproduction](worker-option-transport.md) and its
[public controls](worker-option-transport-controls.py). This prerequisite does
not establish memory admission or enable production checker scheduling.

Reproduce the count matrix by overlaying
[the native Go helper](worker-selection-native_test.go) as a compiler-package
`_test.go` in the exact pinned source, then run its two named tests with
`TSR_WORKER_ORACLE` and `TSR_WORKER_OPTIONS_ORACLE` output paths. The local run
restored two earlier instrumentation files through the overlay and excluded two
unrelated compiler tests to avoid adding their uncached test dependency; this is
not the native compiler suite. The native constructor observations count slots
without binding or constructing checkers. Valid JSON controls use the native
JSON text parser with absolute file names. The aggregate retains the corrected
parser results; early malformed harness setup attempts are excluded.

Build the release CLI and `checker_workers` before running the portable Python
control with `--normal`, `--probe`, `--tsgo`, `--project` and a fresh `--output`
directory. `--public-only` runs its six public cases and five invalid standalone
arguments. Raw private paths, config contents and diagnostic text stay in
`/tmp/tsr-checker-worker-selection`; the committed aggregate retains fingerprints
and numerical evidence. Only the probe's selection and documentation change;
production CLI/checker/loader code and full-corpus snapshots remain unchanged.
This completes the count slice `.3.1.1.1`; memory admission, config transport and
query/augmentation/emit fidelity remain open. The comparable native wall target
of 0.50 remains unverified.
