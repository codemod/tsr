# Performance

**Gate:** [ADR-0009](../adr/0009-performance-gate.md) — the comparison is against
typescript-go at the pinned commit, not against our own history.
**Harness:** `crates/tsr-parser/benches/parse.rs`, deliberately mirroring
`internal/parser/parser_test.go`'s `BenchmarkParse`.

## First measurement — 2026-08-03

The gate was specified in ADR-0009 before anything had been measured. This is the
first run of it. **We are slower than typescript-go on every non-trivial fixture.**

Both sides: single-threaded, pinned to one core (`taskset -c 2`), 3 s per fixture,
AMD Ryzen 9 7950X3D, typescript-go at `5b1047d10` built with Go 1.26.5, tsr built
`--release`.

### Wall clock

| Fixture | Size | tsgo ns/op | tsr ns/op | tsr / tsgo |
|---|---:|---:|---:|---:|
| `empty.ts` | 0 B | 438 | 110 | **0.25×** ✅ |
| `Herebyfile.mjs` | 37 KB | 551,711 | 593,857 | 1.08× |
| `jsxComplexSignature….tsx` | 19 KB | 148,520 | 282,170 | 1.90× |
| `dom.generated.d.ts` | 2.3 MB | 12,962,213 | 31,540,421 | **2.43×** |
| `checker.ts` | 3.1 MB | 32,721,718 | 48,429,856 | 1.48× |

Fixed per-file overhead is 4× *better* than upstream's — an arena and no VFS
lookup beat Go's per-file setup. Everything that actually parses is worse.

### Allocation

This is where the answer is.

| Fixture | tsgo B/op | tsr B/op | tsgo allocs/op | tsr allocs/op | alloc ratio |
|---|---:|---:|---:|---:|---:|
| `empty.ts` | 1,072 | 4,264 | 4 | 6 | 1.5× |
| `Herebyfile.mjs` | 433,223 | 388,565 | 1,104 | 1,386 | 1.3× |
| `jsxComplexSignature….tsx` | 153,434 | 210,602 | 183 | 868 | 4.7× |
| `checker.ts` | 26,132,951 | 25,773,917 | 11,930 | 48,584 | 4.1× |
| `dom.generated.d.ts` | 9,840,043 | 12,035,922 | 2,784 | 60,182 | **21.6×** |

Bytes are comparable. **Allocation *counts* are not**, and the two fixtures where
the count is worst are exactly the two where wall-clock is worst.

## Diagnosis: the arena is bypassed for every list

`dom.generated.d.ts` produces 124,103 nodes and 60,182 heap allocations — one
malloc per two nodes, in a parser whose entire premise is a bump arena.

The cause is the shape of every list-building site. `Arena::alloc_slice` takes a
`&[T]` and copies, so each of the 108 call sites in the parser looks like:

```rust
let mut members = Vec::new();
while … { members.push(self.parse_type_member()); }
let members = self.arena.alloc_slice(&members);   // copy into the arena
```

The `Vec` is a heap allocation that reallocs as it grows, and then the contents are
copied a second time into the arena. Instrumenting `alloc_slice` confirms the
scale:

| Fixture | Nodes | Non-empty `alloc_slice` calls | Total allocations |
|---|---:|---:|---:|
| `dom.generated.d.ts` | 124,103 | 37,913 | 60,182 |
| `checker.ts` | 304,884 | 43,245 | 48,584 |

Non-empty lists account for the bulk of allocations on both, and the growth
reallocs make up most of the rest.

It also explains why `dom.generated.d.ts` is the worst case rather than the larger
`checker.ts`. A `.d.ts` is dense in *declarations*, and each declaration carries
several lists — modifiers, type parameters, parameters, members. A function body is
dense in *expressions*, which mostly are not lists. Per node, dom.d.ts builds three
times as many lists (37,913 / 124,103 versus 43,245 / 304,884), and it is 2.43×
slower where checker.ts is 1.48× slower.

typescript-go avoids this with pooled slice arenas (`p.nodeSliceArena.NewSlice(n)`),
which is why it manages 2,784 allocations for a 2.3 MB file.

## What to do about it

The fix is an arena-backed growable vector — allocate into the bump arena and grow
in place, so a list costs no malloc and no second copy. This is what oxc does with
`ArenaVec`, and it is the single change the numbers point at. Filed as `bd`
issue; it should be done before any other performance work, because the allocation
profile is currently distorted enough that any other measurement is measuring this.

Nothing here suggests a design problem with
[ADR-0003](../adr/0003-tree-plus-side-tables.md): the tree-plus-side-tables shape
is fine, and byte counts are already at parity. It is one missing container type.

## Honest limits of this measurement

- **Two fixtures out of five are dialect mismatches.** `Herebyfile.mjs` is parsed
  by tsgo with the JSX language variant (its `getLanguageVariant` maps `ScriptKindJS`
  to JSX); `ScriptKind::from_file_name` gives us plain TypeScript for `.mjs`. We are
  doing slightly *less* work than upstream on that fixture, so its 1.08× flatters us.
- **The parse is not the same parse.** tsgo defers JSDoc for `.ts`; per
  [ADR-0008](../adr/0008-jsdoc-parsed-eagerly.md) we parse it eagerly. That is
  measured at +7.5% on ordinary TypeScript and is included in every tsr number here.
  It does not account for a 2.4× gap.
- **This is parse only.** The binder and checker do not exist, so ADR-0009's
  "faster in every aspect" is not yet a claim this covers — as that ADR says
  explicitly.
- **Peak RSS is not measured.** ADR-0009 gates it; the harness does not report it
  yet.
- **One machine, one run.** No variance estimate. Treat the 1.08× as noise-adjacent
  and the 2.43× as real.

## Reproducing

```bash
# tsr — pinned to one core, 3 s per fixture
taskset -c 2 cargo bench -p tsr-parser --bench parse

# typescript-go, same fixtures, same core
cd vendor/typescript-go
taskset -c 2 go test -run '^$' -bench BenchmarkParse -benchmem -cpu 1 -benchtime 3s ./internal/parser/
```

A Go toolchain is not installed in CI yet — ADR-0009 lists that, and recursive
submodules, as the prerequisites the gate still needs.
