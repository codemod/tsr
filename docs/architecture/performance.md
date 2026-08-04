# Performance

**Gate:** [ADR-0009](../adr/0009-performance-gate.md) — the comparison is against
typescript-go at the pinned commit, not against our own history.
**Harness:** `crates/tsr-parser/benches/parse.rs`, mirroring
`internal/parser/parser_test.go`'s `BenchmarkParse`.
**Profiles:** `examples/alloc_profile` (scan/parse split, allocation histogram) and
`perf record` against `examples/parse_loop`:

```bash
RUSTFLAGS="-C force-frame-pointers=yes" \
  cargo build --profile profiling -p tsr-parser --example parse_loop
taskset -c 2 perf record -F 2500 --call-graph fp -- \
  target/profiling/examples/parse_loop <file> 120
```

Frame pointers via `RUSTFLAGS`, not the profile: `force-frame-pointers` is a rustc
codegen flag and Cargo ignores it as a profile key (with a warning that is easy to
miss). Without them perf's call graph is mostly `[unknown]`.

## Where we are — 2026-08-03, at equal work

Single-threaded, pinned to one core (`taskset -c 2`), 3 s per fixture, AMD Ryzen 9
7950X3D. typescript-go at `5b1047d10`, Go 1.26.5. tsr `--release`, parent
assignment on, JSDoc off in the ratio column (see
[ADR-0010](../adr/0010-jsdoc-is-a-parse-option.md)).

| Fixture | Size | tsgo ns/op | tsr `-jsdoc` | ratio | speedup |
|---|---:|---:|---:|---:|---:|
| `empty.ts` | 0 B | 438 | 114 | 0.26× | 3.8× |
| `Herebyfile.mjs` | 37 KB | 551,711 | 195,557 | 0.35× | 2.8× |
| `jsxComplexSignature….tsx` | 19 KB | 148,520 | 73,467 | 0.49× | 2.0× |
| `dom.generated.d.ts` | 2.3 MB | 12,962,213 | 8,970,830 | 0.69× | **1.45×** |
| `checker.ts` | 3.1 MB | 32,721,718 | 22,496,149 | 0.69× | **1.45×** |

Peak RSS is **1.74× lower** (32.3 MB against 57.0 MB) — see the memory section.

`checker.ts` started this sequence of work at 49.3 ms.

The large-file numbers are the ones that matter, and they went from **parity
(1.01×) to 1.37–1.41× faster** in one round of profile-directed work: `checker.ts`
33.2 ms → 23.9 ms, `dom.generated.d.ts` 12.0 ms → 9.2 ms.

## Parse + bind

Added 2026-08-03 with the binder; **our column re-measured 2026-08-04** when the
control-flow graph landed. Both sides parse a fresh file and bind it on every
iteration.

| Fixture | tsgo | tsr, no flow graph (2026-08-03) | tsr, with flow graph (2026-08-04) | ratio |
|---|---:|---:|---:|---:|
| `empty.ts` | 0.5 µs | 0.2 µs | 0.21 µs | 0.43× |
| `Herebyfile.mjs` | 777 µs | 288 µs | 247 µs | 0.32× |
| `checker.ts` | 48.5 ms | 23.5 ms | 25.3 ms | 0.52× |
| `jsxComplexSignature….tsx` | 223 µs | 112 µs | 96 µs | 0.43× |
| `dom.generated.d.ts` | 18.3 ms | 10.1 ms | 8.5 ms | 0.47× |

The tsgo column is carried over unchanged — upstream did not move — and only ours
was re-run. Three of the five fixtures got **faster while gaining the entire flow
graph**, because the same change replaced the binder's per-level `Vec` of children
with one shared stack; `dom.generated.d.ts`, whose `.d.ts` interfaces have very
wide child lists, gained the most from that and lost the least to the flow graph.

### Correcting the record

The 2026-08-03 version of this section said, correctly at the time:

> Our binder does not build the control-flow graph and upstream's does, so this
> compares a partial binder against a complete one … Deriving the binder's own
> cost — parse+bind minus parse-only — gives ~0.7 ms for us against ~15.6 ms for
> upstream on `checker.ts`, and that 20× is almost entirely the flow graph.

That prediction held. With the flow graph built, the binder's own cost on
`checker.ts` is **25.3 − 21.3 = 4.0 ms**, against upstream's ~15.6 ms: a **~3.9×**
advantage, not 20×. The 20× was measuring absent work, exactly as the note warned.

The binder is still not complete — destructuring declares no symbols, `export`
does not route, and the strict-mode diagnostics are unported (see
[binder.md](binder.md)) — so a few percent of upstream's binder work is still
missing from our column. The flow graph was the dominant piece and it is now
present on both sides.

### Now gated

`BIND_IS_GATED` in `xtask/src/perf.rs` is `true` as of 2026-08-04, the condition
its comment named. The measured ratio is 0.52 against a ceiling of 1.10, so the
headroom is roughly 2×.

**This flip has not been exercised locally**: there is no Go toolchain on the
machine it was made on, so the tsgo half of the parse+bind comparison could not be
re-run and the first CI run is the real test. The ratio would have to more than
double before the gate fires.

### The trap in benchmarking a binder

`BindSourceFile` upstream is idempotent: it checks `file.IsBound()` and returns.
A loop that binds the same file repeatedly binds once and then measures a boolean
check, reporting typescript-go as effectively infinitely fast. Both sides
therefore parse a fresh file each iteration, which removes the possibility rather
than working around it, and costs nothing because the parse-only number is already
known.

There is no upstream binder benchmark — `func Benchmark` across `internal/` finds
none for binder or checker — so unlike `BenchmarkParse` this is one we wrote.
ADR-0009's warning applies: `benches/go/bind_test.go` is an artifact we control
both halves of and should be read adversarially.

## Memory — peak RSS

The axis [ADR-0009](../adr/0009-performance-gate.md) gates alongside wall clock,
and the one this port's wider case rests on more heavily than speed.

All four non-empty fixtures parsed and **every tree held live simultaneously**,
which is what a compiler does — measuring one at a time and taking the maximum
would understate a real run. `VmHWM` from `/proc/self/status`, so it is the
kernel's high-water mark rather than whatever is resident at the moment of asking.

| | tsgo | tsr `-jsdoc` | tsr `+jsdoc` | ratio (`-jsdoc`) |
|---|---:|---:|---:|---:|
| Nodes | 420,614 | 419,464 | 419,464 | — |
| Baseline (source read, nothing parsed) | 19.3 MB | 7.3 MB | 7.3 MB | **2.66× lower** |
| **Peak RSS, trees held** | **57.0 MB** | **32.3 MB** | 34.0 MB | **1.77× lower** |
| AST cost (peak − baseline) | 37.8 MB | 25.0 MB | 26.8 MB | **1.51× lower** |
| Bytes of RSS per source byte | 7.12 | 4.72 | 5.05 | 1.51× lower |

Two distinct effects, worth separating because they generalise differently:

- **Fixed overhead is 2.66× lower** (7.3 MB versus 19.3 MB). That is the Go
  runtime — its heap arenas, scheduler, and GC metadata — against a Rust binary
  and one bump arena. It is a constant, so it matters most for short-lived
  processes and for an editor holding many small ones, and it does not grow with
  project size.
- **The AST itself is 1.51× smaller.** This is the one that scales. Same tree, same
  node count, 12.8 MB less to hold it: no per-object GC headers, no interior
  pointers the collector must trace, and a bump arena with no per-allocation
  bookkeeping.

### The binder's own memory — measured, not compared

Added 2026-08-04 with the flow graph. Same four fixtures, same method, three
measurements instead of two: baseline, every AST live, every `BindResult` live
too.

| | KiB |
|---|---:|
| AST | 25,600 |
| Binder, symbols only (2026-08-03) | 12,288 |
| Binder, symbols + flow graph (2026-08-04) | 15,616 |
| Binder, after the symbol-parity work (2026-08-04) | **14,848** |
| — attributable to the flow graph | 3,328 |
| Binder, re-measured at `0e17464` (2026-08-05) | **15,056** |

**The 14,848 KiB figure does not reproduce.** Re-measured on 2026-08-05 with the
binder changes of ADR-0023 stashed — i.e. at `0e17464` exactly, the commit the
number was recorded against — `examples/rss` reports **15,056 KiB**, 208 KiB
(1.4%) above it. This is recorded rather than silently corrected because it was
found the wrong way round: the ADR-0023 change measured 15,052 KiB and looked like
a 1.4% regression against the documented gate, and only measuring the unchanged
baseline showed the change is RSS-neutral (−4 KiB, noise) and the *gate* is stale.
Which of machine state, allocator behaviour, or an untracked change between
2026-08-04 and `0e17464` accounts for the 208 KiB is not established. **Treat
14,848 as unverified: compare against a baseline measured in the same session,
not against this table.**

3.25 MiB buys 81,713 flow nodes and 419,464 `node -> flow` entries — one flow
node per 5.1 AST nodes. (Both counts are as measured on 2026-08-04 before the
parser stopped turning contextual keywords into `KeywordExpression`s; the graph
is now 83,690 nodes over 419,571 AST nodes, and peak RSS did not move.)

The binder then *shrank* while gaining work. Closing the symbol-parity gaps took
it from 39,306 symbols to 41,529 (index signatures, function expressions, JSX
attributes, destructuring, anonymous containers), which cost ~6% of bind time and
0.5 MiB — until `Symbol::declarations` became a `SmallVec<[NodeId; 1]>`. Almost
every symbol has exactly one declaration, so a `Vec` there meant ~41,000 heap
allocations to hold one four-byte id apiece. Removing them paid for all of the
new work and more:

| | parse+bind `checker.ts` | binder RSS |
|---|---:|---:|
| flow graph only, 2026-08-04 | 25.2 ms | 15,616 KiB |
| + symbol parity, `Vec` declarations | 26.8 ms | 16,128 KiB |
| + `SmallVec<[NodeId; 1]>` | **25.6 ms** | **14,848 KiB** | The flow graph is 13% of the AST's footprint and 21% of
the binder's; [ADR-0014](../adr/0014-flow-graph-representation.md) accounts for it
byte by byte and explains why a record is 16 bytes rather than upstream's 32.

**There is no typescript-go number beside these.** `benches/go/rss_test.go`
measures parsing only, so unlike the AST row above, the binder's memory is
absolute rather than comparative and cannot be gated. Writing the Go counterpart
is `bd tsr-y4u.9`. Reproduce ours with:

```
cargo run -p tsr-binder --example rss --release
```

### What this measurement does *not* show

At this heap size **Go's collector never runs** — `GOGC=off` and `GOGC=400`
produce exactly the same figure as the default. So this compares the size of the
two data structures and nothing else.

It therefore omits the effect people actually hit: a collector needs headroom, and
Go's default `GOGC=100` lets the heap reach roughly twice the live set before
collecting. That is the mechanism behind `tsc` exhausting memory on large
monorepos, and it is the reason to expect the real-world gap to be **wider** than
1.77×, not narrower. Measuring it needs a whole-project workload and a driver,
which do not exist yet; it is filed under `bd tsr-oqn` rather than claimed here.

Read the 1.77× as a floor established under conditions favourable to Go.

### Reproducing

```bash
taskset -c 2 cargo run --release -p tsr-parser --example rss          # add --jsdoc
cp benches/go/rss_test.go vendor/typescript-go/internal/parser/
cd vendor/typescript-go && taskset -c 2 go test -run TestTsrPeakRSS -v ./internal/parser/
rm vendor/typescript-go/internal/parser/rss_test.go
```

The Go half is ours, not upstream's — there is no upstream RSS benchmark — so
ADR-0009's warning applies: it is an artifact we control both halves of and should
be read adversarially. `benches/go/rss_test.go` says what to check.

### Recording parents without materialising children

`record_parent_of_children` collected a node's children into a `Vec<Node>` and
then read each id back out through `Node::node_id`, a match over all 192 variants
that showed up at 1.8% on its own. Both were avoidable: the caller only ever wants
the id, and at each field the concrete or alias type is statically known.

`for_each_child_id` is generated to dispatch at that static type — a direct `Cell`
read for a concretely-typed field, a handful of arms for an alias — and passes ids
to a closure with no intermediate collection. Alias enums gained their own
`node_id` for the same reason. Worth 5–6% of the parse.

### Parent assignment moved into `finish_node`

It was a separate pass over the finished tree, costing 11.2% of a `checker.ts`
parse (`assign_parents` + `push_children` + `Node::node_id`). It now happens in
`finish_node_with_end`, where the children were created moments earlier and are
still in cache — which is where typescript-go does it, presumably for the same
reason. The cost fell to ~6.4%.

Wall clock barely moved, which is worth stating plainly rather than dressing up:
the profile share halved but the total did not, so something else absorbed it. The
change stands because it removes an entire traversal and a worklist, and because
it makes the structure match upstream's — not because it produced a headline
number.

It needed `From<&'a T> for Node<'a>` for all 192 node types, which is generated:
`finish_node` is generic over `T` and had no way to reach the `Node` union. Those
impls were missing generally, and their absence is what made the first version of
`push_children` awkward.

### What did it

All four came from the same observation: the scanner was written in terms of
`char`, and TypeScript source is bytes that are almost all ASCII.

| Change | What it removed |
|---|---|
| Byte-driven trivia loop | A `peek()` call, a UTF-8 decode and two branches *per character* of whitespace, between every pair of tokens |
| ASCII identifier fast path | The same, per character of every identifier, plus the `char` classification |
| `ASCII_ID_START`/`ASCII_ID_PART` tables | Three comparisons per character, replaced by one indexed load |
| `capture_value` asking the scanner directly | Two slice constructions and a pointer comparison per token, to answer a question the scanner already knew |
| `NodeTable::with_capacity(len / 10)` | Four capacity checks and four growth reallocations per doubling, across 300k nodes |

None of it is clever. It is the difference between writing a lexer in terms of the
abstraction the language offers (`chars()`) and writing it in terms of what the
hardware does. `Scanner::scan` went from 19% of the profile to well under that,
and the aggregate is a 28% cut on `checker.ts`.

### The regression this introduced, and how it was caught

The byte-driven trivia loop broke out of the byte path on the first non-ASCII
byte — including *inside a line comment*, which then resumed scanning the
comment's contents as code. `// héllo` is enough to trigger it.

Every unit test passed. The corpus caught it: `scanner_clean_files` 100% → 99.66%,
`parser_typescript` 99.36% → 99.03%, 17 files each. Non-ASCII characters in
comments are common in real source and absent from hand-written tests, which is
the whole argument for the corpus gate. There are now five trivia regression tests
covering it (`crates/tsr-scanner/tests/scan.rs`).

## Is the comparison fair? — audited both directions

typescript-go's `BenchmarkParse` calls `parser.ParseSourceFile` and nothing else:
no binder, no checker, no program construction. That much is straightforwardly
comparable. But `ParseSourceFile` does three things around the parse that we did
not, and they are not all negligible.

Measured by adding benchmarks inside `internal/parser` (temporary, not committed —
these functions are package-private):

| tsgo does | cost on `dom.generated.d.ts` | share of its 13.0 ms |
|---|---:|---:|
| `getCommentPragmas` — rescans the source from offset 0 | 570 ns | 0.004% |
| `collectExternalModuleReferences` — walks the finished tree | 12.6 µs | 0.1% |
| **`overrideParentInImmediateChildren` — sets every node's parent** | **~1.25 ms** | **9.6%** |

The first two are noise. The third is not, and **we were not doing it at all.**

`finishNodeWithEnd` calls `overrideParentInImmediateChildren`, which re-walks each
node's children through `ForEachChild` with a closure, assigning `Parent`. A CPU
profile of the Go parser attributes 9.55% to that path. Our `NodeTable` had a
`parent` column that nothing ever wrote — the tree could not answer "who is my
parent" at all.

So the earlier 0.84× on `dom.generated.d.ts` was comparing our parse against a
tsgo parse doing ~10% more work *and* producing something we were not producing.
`tsr_ast::assign_parents` now fills the column, costing us 10–11% — close enough to
upstream's 9.6% that the axis is fair — and the ratio moved from 0.84× to 0.93×.

## Where the remaining time goes

A profile of `checker.ts` after this round:

| | share |
|---|---:|
| `Scanner::scan` | 18.9% |
| parent assignment (`assign_parents` + `push_children` + `Node::node_id`) | 11.2% |
| `Scanner::scan_identifier_or_keyword` | 5.8% |
| `NodeTable::push` | 4.9% |
| `Scanner::peek` + `Scanner::bump` | 5.9% |
| `keyword_kind` | 1.9% |

For context, the same profile of **typescript-go**:

| | share of tsgo's parse |
|---|---:|
| `runtime.*` total | **36%** |
| — write barriers (`wbBufFlush`, `gcWriteBarrier`) | 14.6% |
| — `runtime.mallocgc` | 12.4% |
| — `runtime.growslice` | 11.8% |
| parent assignment | 9.6% |
| identifier interning (`mapaccess1_faststr`, `aeshashbody`) | ~5.6% |

Roughly a third of typescript-go's parse is Go runtime tax that does not exist in
our build: no GC, no write barriers, a bump arena instead of `mallocgc`. Before
this round we were spending that entire advantage and arriving at parity, which
meant our parser was ~1.4× slower than Go's at equal algorithmic work. We are now
ahead, but the 36% is still the size of the structural head start, and 1.37× is
less than that — **the algorithmic gap has narrowed, not closed.**

The next items, in the order the profile supports:

- **`NodeTable::push`, 5.0%**, even with capacity reserved: four parallel vectors
  means four length checks and four stores per node. The largest remaining item
  that is not the scanner.
- **`Scanner::scan`, 18.5%**, after the trivia loop and token dispatch were both
  put on bytes. What is left is the token-kind dispatch itself; further gains here
  look like a jump table over the punctuation set rather than another easy win.
- **The arena vector.** Every list site still collects into a `Vec` and copies into
  the arena. With JSDoc off, `dom.d.ts` does 26,044 allocations; this is the
  smallest of the three and has been re-estimated downward twice.

## Does this justify the port?

Better than it did an hour ago, and the memory number is the stronger half.

**Speed:** 1.37–1.41× on large files, 2–3× on small ones. Real, but parsing is a
small share of a compiler run — the checker dominates — so on its own this is
necessary rather than sufficient.

**Memory:** 1.77× lower peak RSS holding the same trees, measured under conditions
that favour Go (its collector never ran). This is the better argument, for three
reasons:

1. **It scales with the thing that hurts.** Parse time is linear in source size and
   already fast in absolute terms; memory is what decides whether a large monorepo
   can be checked *at all*. Halving the footprint is a capability difference, not a
   speed one.
2. **It should widen, not narrow.** The 1.77× excludes GC headroom entirely. It
   also excludes the checker, where allocation volume is far higher than the
   parser's and where Go's 36%-of-runtime tax applies again.
3. **It is the axis the rewrite structurally wins.** The speed margin has to be
   earned back token by token against a well-written Go parser — this session went
   from 1.01× to 1.37× by hand. The memory margin falls out of arena allocation and
   no GC headers, and does not have to be re-won as the codebase grows.

**Still unmeasured:** latency under an editor workload (GC pause distribution,
which is what a person actually feels), and anything at all about the checker —
where both the time and the memory really are, and where nobody has a fast
implementation.

The defensible claim: *parse throughput is 1.37–1.41× on large files with headroom
remaining, peak RSS is 1.77× lower with the measurement biased against us, and the
checker — the part that decides the project — is unbuilt and unmeasured.*

## Honest limits of these numbers

- **`Herebyfile.mjs` is a dialect mismatch that flatters us.** tsgo parses `.mjs`
  with the JSX language variant (`getLanguageVariant` maps `ScriptKindJS` to JSX);
  `ScriptKind::from_file_name` gives us plain TypeScript. We are doing less work.
  Its 0.44× should not be quoted without this caveat.
- **Parent assignment is on**, matching tsgo, but ours is one pass afterwards
  while tsgo's is interleaved into `finishNode`. Same work, different cache
  behaviour; the totals are close (10–11% versus 9.6%) but they are not identical
  operations.
- **The ratio is the `-jsdoc` column**, because that is what tsgo does for these
  fixtures. The `+jsdoc` column is our default. Turning off work is not the same as
  making work faster, and both columns stay in the table so that stays visible.
- **Parse only.** No binder, no checker. ADR-0009's "faster in every aspect" is not
  a claim these numbers support.
- **Peak RSS is not measured**, though ADR-0009 gates it.
- **One machine, one run, no variance estimate.**
- **The RSS figures are a single sample each** and, unlike wall clock, come from a
  process that ran once. They are stable across runs (peak RSS is far less noisy
  than timing) but no distribution was collected.

## Reproducing

```bash
taskset -c 2 cargo bench -p tsr-parser --bench parse
taskset -c 2 cargo run --release -p tsr-parser --example alloc_profile -- <file>

cd vendor/typescript-go
taskset -c 2 go test -run '^$' -bench BenchmarkParse -benchmem -cpu 1 -benchtime 3s ./internal/parser/
```

CI has neither a Go toolchain nor recursive submodules; ADR-0009 lists both as
prerequisites the gate still needs.
