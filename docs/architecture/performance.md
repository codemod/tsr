# Performance

**Gate:** [ADR-0009](../adr/0009-performance-gate.md) — the comparison is against
typescript-go at the pinned commit, not against our own history.
**Harness:** `crates/tsr-parser/benches/parse.rs`, mirroring
`internal/parser/parser_test.go`'s `BenchmarkParse`.
**Profile:** `cargo run --release -p tsr-parser --example alloc_profile -- <file>`.

## Where we are — 2026-08-03, after the first round of profiling

Single-threaded, pinned to one core (`taskset -c 2`), 3 s per fixture, AMD Ryzen 9
7950X3D. typescript-go at `5b1047d10`, Go 1.26.5. tsr built `--release`.

| Fixture | Size | tsgo ns/op | tsr ns/op | tsr / tsgo |
|---|---:|---:|---:|---:|
| `empty.ts` | 0 B | 438 | 111 | **0.25×** |
| `Herebyfile.mjs` | 37 KB | 551,711 | 270,721 | **0.49×** |
| `checker.ts` | 3.1 MB | 32,721,718 | 29,744,505 | **0.91×** |
| `jsxComplexSignature….tsx` | 19 KB | 148,520 | 158,598 | 1.07× |
| `dom.generated.d.ts` | 2.3 MB | 12,962,213 | 20,176,507 | 1.56× |

Faster on three, at parity on one, and 1.56× slower on the large `.d.ts`.

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

## Where the remaining time goes

`dom.generated.d.ts`, the one fixture still behind, at 19.44 ms against tsgo's
12.96 ms:

```
  2,348,669 bytes, 124,103 nodes
  scan only        7.39 ms   (38%)
  parse - scan    12.05 ms   (62%)
  60,182 allocations (12,955 reallocs), 12.0 MB
```

Two candidates, in the order the evidence supports:

**The parser proper: 12.05 ms for 124,103 nodes, ~97 ns per node.** This is now the
biggest single block of time on the fixture and it has not been profiled — `perf`
is unavailable on this machine (`perf_event_paranoid=4`), so `alloc_profile` only
resolves to the scan/parse boundary. Getting inside it needs either relaxed perf
permissions or finer manual instrumentation.

**The arena vector, still worth doing but smaller than first claimed.** Every list
site collects into a `Vec` and copies into the arena, giving ~47k mallocs and ~13k
reallocs on this fixture. At a plausible 20–30 ns each that is roughly 1.0–1.4 ms of
19.4 ms — about **6%**, not the 2.4× the first entry implied. It remains the right
change (it also removes a second copy of every list, which the malloc estimate does
not capture) but it should not be expected to close the remaining gap alone.

`.d.ts` files are the worst case for both: dense in declarations, and each
declaration carries several lists — modifiers, type parameters, parameters,
members. Per node, `dom.d.ts` builds three times as many lists as `checker.ts`
(37,913 / 124,103 versus 43,245 / 304,884).

## Honest limits of these numbers

- **`Herebyfile.mjs` is a dialect mismatch that flatters us.** tsgo parses `.mjs`
  with the JSX language variant (`getLanguageVariant` maps `ScriptKindJS` to JSX);
  `ScriptKind::from_file_name` gives us plain TypeScript. We are doing less work.
  Its 0.49× should not be quoted without this caveat.
- **We parse JSDoc eagerly** ([ADR-0008](../adr/0008-jsdoc-parsed-eagerly.md)),
  which tsgo defers for `.ts`. That is included in every tsr number here and makes
  the comparison, if anything, unfavourable to us.
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
