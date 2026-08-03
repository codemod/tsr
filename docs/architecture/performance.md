# Performance

**Gate:** [ADR-0009](../adr/0009-performance-gate.md) — the comparison is against
typescript-go at the pinned commit, not against our own history.
**Harness:** `crates/tsr-parser/benches/parse.rs`, mirroring
`internal/parser/parser_test.go`'s `BenchmarkParse`.
**Profiles:** `examples/alloc_profile` (scan/parse split, allocation histogram) and
`perf record` against `examples/parse_loop`, built with `--profile profiling`.

## Where we are — 2026-08-03, after profiling

**Faster than typescript-go on all five fixtures**, like for like.

Single-threaded, pinned to one core (`taskset -c 2`), 3 s per fixture, AMD Ryzen 9
7950X3D. typescript-go at `5b1047d10`, Go 1.26.5. tsr built `--release`.

| Fixture | Size | tsgo ns/op | tsr `-jsdoc` | ratio | tsr `+jsdoc` |
|---|---:|---:|---:|---:|---:|
| `empty.ts` | 0 B | 438 | 112 | **0.26×** | 121 |
| `Herebyfile.mjs` | 37 KB | 551,711 | 244,413 | **0.44×** | 271,064 |
| `jsxComplexSignature….tsx` | 19 KB | 148,520 | 95,127 | **0.64×** | 154,419 |
| `dom.generated.d.ts` | 2.3 MB | 12,962,213 | 10,908,342 | **0.84×** | 19,825,587 |
| `checker.ts` | 3.1 MB | 32,721,718 | 28,838,476 | **0.88×** | 29,612,201 |

`-jsdoc` is the like-for-like column: typescript-go does not build JSDoc nodes for
`.ts`/`.tsx` — `withJSDoc` sets a flag and returns. Both arms are printed by the
benchmark so neither can be quoted alone; see
[ADR-0010](../adr/0010-jsdoc-is-a-parse-option.md).

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

Every fixture is now ahead of tsgo, so what follows is opportunity rather than
deficit. In the order the profile supports:

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
