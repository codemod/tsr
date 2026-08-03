# Performance

**Gate:** [ADR-0009](../adr/0009-performance-gate.md) — the comparison is against
typescript-go at the pinned commit, not against our own history.
**Harness:** `crates/tsr-parser/benches/parse.rs`, mirroring
`internal/parser/parser_test.go`'s `BenchmarkParse`.
**Profiles:** `examples/alloc_profile` (scan/parse split, allocation histogram) and
`perf record` against `examples/parse_loop`, built with `--profile profiling`.

## Where we are — 2026-08-03, at equal work

Single-threaded, pinned to one core (`taskset -c 2`), 3 s per fixture, AMD Ryzen 9
7950X3D. typescript-go at `5b1047d10`, Go 1.26.5. tsr `--release`, parent
assignment on, JSDoc off in the ratio column (see
[ADR-0010](../adr/0010-jsdoc-is-a-parse-option.md)).

| Fixture | Size | tsgo ns/op | tsr `-jsdoc` | ratio | speedup |
|---|---:|---:|---:|---:|---:|
| `empty.ts` | 0 B | 438 | 148 | 0.34× | 3.0× |
| `Herebyfile.mjs` | 37 KB | 551,711 | 208,440 | 0.38× | 2.6× |
| `jsxComplexSignature….tsx` | 19 KB | 148,520 | 76,005 | 0.51× | 2.0× |
| `dom.generated.d.ts` | 2.3 MB | 12,962,213 | 9,206,160 | 0.71× | **1.41×** |
| `checker.ts` | 3.1 MB | 32,721,718 | 23,891,168 | 0.73× | **1.37×** |

The large-file numbers are the ones that matter, and they went from **parity
(1.01×) to 1.37–1.41× faster** in one round of profile-directed work: `checker.ts`
33.2 ms → 23.9 ms, `dom.generated.d.ts` 12.0 ms → 9.2 ms.

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

- **The parent pass, 11.2%.** Comparable to tsgo's 9.6%, so we are not losing
  here, but ours is a second traversal over a tree that was just built and is no
  longer in cache. Fusing it into `finish_node` — where the children were touched
  moments ago — is what upstream does and should be cheaper than either.
- **`NodeTable::push`, 4.9%**, even with capacity reserved: four parallel vectors
  means four length checks and four stores per node.
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
