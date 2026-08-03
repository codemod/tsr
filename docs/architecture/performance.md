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
assignment **on** (see below), JSDoc off in the ratio column.

| Fixture | Size | tsgo ns/op | tsr `-jsdoc` | ratio | tsr `+jsdoc` |
|---|---:|---:|---:|---:|---:|
| `empty.ts` | 0 B | 438 | 144 | **0.33×** | 144 |
| `Herebyfile.mjs` | 37 KB | 551,711 | 281,546 | **0.51×** | 312,754 |
| `jsxComplexSignature….tsx` | 19 KB | 148,520 | 111,403 | **0.75×** | 171,042 |
| `dom.generated.d.ts` | 2.3 MB | 12,962,213 | 12,036,118 | **0.93×** | 21,097,712 |
| `checker.ts` | 3.1 MB | 32,721,718 | 33,158,866 | **1.01×** | 33,475,941 |

**On the largest realistic file we are at parity, marginally slower.** The
double-digit wins are on small files, where the advantage is fixed per-file
overhead — an arena versus Go's parser pool and per-file setup — and that advantage
does not scale with input.

This is a worse picture than the previous entry reported, and the previous entry
was wrong for a reason worth stating: **we were not doing the same work.**

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

## Why are we not faster?

This is the question the numbers now force, and the answer is uncomfortable.

A CPU profile of typescript-go parsing `dom.generated.d.ts`:

| | share of tsgo's parse |
|---|---:|
| `runtime.*` total | **36%** |
| — write barriers (`wbBufFlush`, `gcWriteBarrier`) | 14.6% |
| — `runtime.mallocgc` | 12.4% |
| — `runtime.growslice` | 11.8% |
| parent assignment | 9.6% |
| identifier interning (`mapaccess1_faststr`, `aeshashbody`) | ~5.6% |

**Roughly a third of typescript-go's parse time is Go runtime tax that does not
exist in our build.** We have no GC, no write barriers, and a bump arena instead of
`mallocgc`. That advantage is handed to us before we write a line of parser code.

We are spending all of it and arriving at parity. Netting it out: at equal
algorithmic work, our parser is something like **1.4–1.5× slower than Go's** — and
typescript-go's parser is not exotic, it is a straightforward recursive-descent
port of `tsc`. The gap is ours, not theirs.

Our own profile says where it goes:

| | share of our parse |
|---|---:|
| `Scanner::bump` + `Scanner::peek` | 17% |
| `Scanner::scan` | 12.6% |
| `Scanner::scan_identifier_or_keyword` | 5.9% |
| `is_identifier_part` | 2.8% |
| `NodeTable::push` | 2.1% |

The scanner is ~40% of the parse and is still doing per-character work that a fast
lexer does per-word. That is the headroom, and it is large: closing it should put
us meaningfully ahead rather than at parity.

## Does parse speed justify the port?

Honestly: **not on these numbers, and it was never the strongest argument.**

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

Until those are measured, the honest claim is narrow: *parse throughput is at
parity with typescript-go, with identified headroom, and the port's case rests on
axes we have not yet instrumented.* Anything stronger is unearned.

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
