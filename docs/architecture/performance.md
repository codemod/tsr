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

## Does parse speed justify the port?

Better than it was, and still not the strongest argument.

1.37× on a large file is a real result, and it is the kind of margin a rewrite can
be asked to show. But parsing is a small share of a compiler run — the checker
dominates — so this is necessary, not sufficient.

Parsing is a small share of a compiler run — the checker dominates — so a parser at
parity is not a reason to rewrite 300k lines. The arguments that do carry weight
are ones we have not yet measured:

- **Memory and GC behaviour.** 36% of tsgo's parse is runtime overhead, and the
  same tax applies to the checker, where allocation volume is far higher.
  [ADR-0009](../adr/0009-performance-gate.md) gates peak RSS for this reason and
  the harness does not report it yet. This is the most valuable missing number.
- **Latency, not throughput.** GC pauses are what an editor feels. Steady-state
  throughput parity with no pauses is a different product.
- **The checker.** Where the time actually is, and where nobody — including oxc,
  at 0.93% type conformance — has a fast implementation. That is the real prize
  and none of these fixtures touch it.

Until those are measured, the honest claim is: *parse throughput is 1.37–1.41× on
large files and 2–3× on small ones, with identified headroom remaining, and the
port's wider case still rests on axes we have not instrumented.*

### What changed, and a correction to the previous entry

The first measurement (committed in `66e20d5`) had every fixture slower — 1.48× on
`checker.ts` and 2.43× on `dom.generated.d.ts`. That entry attributed the gap to
**allocation counts**, on the evidence that they were 4× and 21.6× upstream's.

**That diagnosis was wrong about the primary cause, and the record is corrected
here.** Profiling showed the allocation counts were real but not what the time was
going into. Two changes to the *scanner* — which the previous entry did not
implicate at all — accounted for nearly all of it:

| | dom.d.ts total | checker.ts total |
|---|---:|---:|
| Before | 32.78 ms | 49.27 ms |
| ASCII fast path in `peek`/`bump` | 25.50 ms | 42.46 ms |
| Generated keyword table | **19.44 ms** | **29.03 ms** |
| Net | **−41%** | **−41%** |

Allocation counts are **unchanged** by both — still 60,182 and 48,584. The whole
41% came from work per byte and per identifier, not from the allocator. Had we
acted on the original diagnosis and built the arena vector first, we would have
done the harder change for the smaller win.

The lesson is narrow and worth keeping: *`allocs/op` was the only per-operation
number the benchmark reported, so it was the only thing the diagnosis could point
at.* A wall-clock benchmark plus an allocation counter is not a profile.

## The two fixes

### 1. The scanner decoded UTF-8 for every character read

`peek`, `peek_at`, and `bump` each built a bounds-checked `&str` slice, constructed
a `Chars` iterator, and decoded a UTF-8 code point — for source that is almost
entirely ASCII:

```rust
fn peek(&self) -> Option<char> {
    self.rest().chars().next()
}
```

Now they read a byte and take the `< 0x80` branch, falling back to the decoder only
for genuinely non-ASCII input. Roughly 30 lines. Scanning got 30–33% faster and
whole-parse 14–22% faster.

### 2. `keyword_kind` was a linear scan with string comparisons

Asked once per identifier — hundreds of thousands of times on `checker.ts` — the
hand-written version walked `FIRST_KEYWORD..=LAST_KEYWORD`, called
`SyntaxKind::name()`, and did `eq_ignore_ascii_case` against each:

```rust
(first..=last).filter_map(SyntaxKind::from_u16).find(|kind| {
    kind.name().strip_suffix("Keyword").is_some_and(|word| word.eq_ignore_ascii_case(text))
})
```

That is ~85 string comparisons to decide that `elementFromPoint` is not a keyword.
It is now a generated `match` on `&str`
(`crates/tsr-scanner/src/generated/keywords.rs`), which rustc dispatches on length
before comparing bytes, so most identifiers cost one integer comparison. This was
the larger of the two fixes.

Generated rather than hand-written because the keyword set is defined by
`SyntaxKind`: a hand-maintained table would drift silently the next time upstream
adds a keyword, and the failure mode is the scanner quietly emitting it as an
identifier. A generated test asserts the table and the `SyntaxKind` range agree.

## Round two: the profile

With `perf` available (`perf_event_paranoid=1`), `dom.generated.d.ts` at 150
iterations, frame pointers on:

```
  12.6%  Scanner::scan
  12.5%  Scanner::scan_jsdoc_comment_text_token
   8.9%  Scanner::bump
   8.2%  Scanner::peek
   5.9%  Scanner::scan_identifier_or_keyword
   4.7%  jsdoc_ranges_in
   4.0%  Parser::parse_leading_jsdoc
   2.9%  Parser::next_token
   2.9%  Scanner::scan_jsdoc_token
   2.8%  is_identifier_part
   2.1%  NodeTable::push
   1.5%  mentions_tag
   1.3%  keyword_kind
```

**JSDoc is 25% of the samples**, in five separate functions none of which the
previous two rounds of guessing had implicated. Measured directly by turning it
off, it is more than that:

| | with JSDoc | without | share |
|---|---:|---:|---:|
| `dom.generated.d.ts` | 19.83 ms | 10.91 ms | **45%** |
| `dom.generated.d.ts` allocations | 60,182 | 26,044 | **57%** |
| `jsxComplexSignature….tsx` | 154.4 µs | 95.1 µs | 38% |
| `checker.ts` | 29.61 ms | 28.84 ms | 3% |

This fired [ADR-0008](../adr/0008-jsdoc-parsed-eagerly.md)'s stated falsifier
verbatim — "the 7.5% figure becomes, say, 25% on a large project's `.ts` files" —
and [ADR-0010](../adr/0010-jsdoc-is-a-parse-option.md) makes JSDoc a parse option
in response. ADR-0008's 7.5% was not wrong for the corpus it used; it was the wrong
corpus, because a `.d.ts` is exactly the documentation-dense shape a real project
parses most of.

It also revises the allocation story a second time. 57% of `dom.d.ts`'s
allocations were JSDoc's own — so the "parser builds a `Vec` per list" diagnosis
from round one was, in part, measuring JSDoc.

## Where the remaining time goes

In the order the profile supports:

**The character cursor is still 17% of samples** (`bump` 8.9%, `peek` 8.2%) even
after the ASCII fast path. Both are called per character and still re-derive the
window bounds on every call. A cursor holding a raw pointer pair rather than
`(source, pos, limit)` would remove that; it is the standard shape for a fast
lexer and the profile says it is worth roughly what the keyword table was.

**JSDoc scanning is expensive when it runs at all.**
`scan_jsdoc_comment_text_token` alone is 12.5% of the `+jsdoc` profile. It advances
one `peek()` at a time and calls `at_tag_start()` per `@`. A `memchr`-style scan
for the next interesting byte would cut most of it. This matters for the language
service, which is the consumer that will actually want JSDoc.

**The arena vector, smaller again than the revised estimate.** With JSDoc off,
`dom.d.ts` does 26,044 allocations rather than 60,182, so the ceiling on this fix
is now roughly 0.5–0.8 ms of 10.9 ms. Still worth doing for the second-copy
elimination, but it has been re-estimated downward twice and should be scheduled
accordingly.

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
- **One machine, one run, no variance estimate.** Treat 1.07× as parity and the
  1.56× as real.

## Reproducing

```bash
taskset -c 2 cargo bench -p tsr-parser --bench parse
taskset -c 2 cargo run --release -p tsr-parser --example alloc_profile -- <file>

cd vendor/typescript-go
taskset -c 2 go test -run '^$' -bench BenchmarkParse -benchmem -cpu 1 -benchtime 3s ./internal/parser/
```

CI has neither a Go toolchain nor recursive submodules; ADR-0009 lists both as
prerequisites the gate still needs.
