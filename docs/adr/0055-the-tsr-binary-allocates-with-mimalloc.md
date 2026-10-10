# ADR-0055: The `tsr` binary allocates with mimalloc, with purging off

- **Status:** accepted (round 7, `r7-perf`, `tsr-2zk.1277`; approved by the
  round-7 integrator on 2026-10-10, on the condition that the 41-sample
  interleaved A/B is ≤ 1.00 child CPU on all four projects)
- **Date:** 2026-10-10
- **Supersedes:** [ADR-0011](0011-unsafe-is-opt-in.md) in part. Its list of
  `unsafe` exceptions gains one entry, `crates/tsr/src/main.rs`'s
  `configure_allocator` (below). ADR-0011 is not edited; this record is
  where the entry lives.
- **Related:** [ADR-0054](0054-the-release-ratio-is-measured-on-the-dist-profile.md),
  [`docs/parity/notes/r7-perf.md`](../parity/notes/r7-perf.md) §8 and §10

## The forcing constraint

Under glibc `malloc`, the allocator is one of the largest single costs left
in a whole-project check (r7-perf §7, `perf` on the release binary):

- domain-model, single-threaded: about **11% self** (`malloc`, `_int_free`,
  `cfree`, `_int_malloc`, `malloc_consolidate`, `realloc`);
- jsTyping, single-threaded: about **18% self** (`malloc` 6.1%, `_int_free`
  5.5%, `_int_malloc` 3.4%, `cfree` 1.7%, `malloc_consolidate` 1.1%).

The checker allocates many small, short-lived values. `tsr-2zk.1092`
(Signature copies) is the long-term fix: fewer allocations. The allocator is
the near-term fix: each allocation costs less. glibc also gives each checker
thread its own arena with a global consolidation path. mimalloc's
thread-local heaps fit the checker pool's one-checker-per-thread shape
(`checker_pool.rs`).

## The decision

1. The `tsr` binary sets `#[global_allocator]` to `mimalloc::MiMalloc` with
   the crate's **v2** feature. Only `crates/tsr` links it. Libraries,
   tests, examples, the conformance harness and benches keep the system
   allocator, so the gates (box-protocol §5) and the corpus run are
   unaffected.
2. At the start of `main`, before any thread exists, the binary sets
   mimalloc's `purge_delay` option to −1 (never return freed pages to the
   OS) through `libmimalloc_sys::mi_option_set`. That needs the
   `extended` feature and one `unsafe` call, recorded below as an ADR-0011
   exception. The option is fixed in the binary. It does not depend on the
   environment, so `MIMALLOC_PURGE_DELAY` cannot change the measured
   configuration except by overriding it on purpose.

### The ADR-0011 exception

| Where | Why it is unavoidable |
|---|---|
| `crates/tsr/src/main.rs`, `configure_allocator` — one `mi_option_set` call | The option exists only as a C function. `libmimalloc-sys` exposes it as an `extern "C"` declaration with no safe wrapper, and `mimalloc` does not wrap it. It takes two integers, has no pointer arguments, and runs once before any other work. |

## Why v2 and why purging off

Measured on the release binary (`/tmp/box/ab.py`: interleaved fresh
processes, one warmup, medians of wall and `wait4` child CPU; ratios are
mimalloc/glibc on the same source). The decision row is the committed
binary against its glibc parent `c36a7706`. Its wall ratios are gi 0.933,
dm 0.820, dml 0.819 and jsTyping 0.831.

| variant | generic-imports CPU | domain-model CPU | domain-model-large CPU | jsTyping CPU |
|---|---:|---:|---:|---:|
| v3 (crate default), 21 samples | 1.082 | 0.900 | 0.875 | 0.854 (5 samples) |
| v3 + `no_thp`, 21 | 1.078 | — | — | — |
| v2, 41 | 1.026; 1.008 and 0.981 in two reruns | 0.884 | 0.855 | — |
| v2 + `MIMALLOC_PURGE_DELAY=-1` (env), 41 | **0.970** | **0.843** | **0.828** | **0.770** |
| **v2 + purge off set in `main` (this decision), 41** | **0.949** | **0.833** | **0.826** | **0.775** |

generic-imports is lib.dom-bound and short (about 42 ms). In it, v3 costs
more than it saves, and v2 sits within a few percent of glibc, with run to
run swinging across 1.00. Turning purging off is what puts it reliably
below glibc. Under the default delay, mimalloc returns freed pages to the
OS and the process faults the same pages back in a few milliseconds later.
A `tsr` process exits when the command ends (r7-perf §8), so there is
nothing to give the memory back for.

## Alternatives

- **Keep glibc.** It costs 11–18% CPU, as measured above.
- **glibc tunables** (`GLIBC_TUNABLES`, measured on generic-imports, 21
  samples): `glibc.malloc.hugetlb=1` gave 0.993 wall / 0.994 CPU, and a
  256 MiB `top_pad` + `mmap_threshold` gave 0.991 / 0.988. Both are noise,
  and both need the environment set by whoever launches the binary.
- **mimalloc configured only through the environment**
  (`MIMALLOC_PURGE_DELAY=-1`). The numbers are those of the decision. It
  was refused because the shipped binary's behaviour would depend on the
  user's environment, and the integrator's condition was that the
  configuration be fixed in the binary.
- **Build-time defines** (`CFLAGS=-DMI_...` through `cc`). This depends on
  the builder's environment, and the purge delay is not one of mimalloc's
  compile-time defaults in v2 anyway.
- **jemalloc** (`tikv-jemallocator`). Not measured. It is the alternative
  to try if mimalloc's C build or behaviour becomes a problem on another
  target.

## Consequences accepted

- Two new dependencies (`mimalloc`, `libmimalloc-sys` with its vendored C
  sources and `cc` build), and a C compiler at build time for the binary.
- One `unsafe` block, listed above.
- With purging off, the binary does not return freed memory to the OS
  before it exits. For a one-shot CLI that is the intended trade. A
  long-running host (a future `--watch` or language server) must
  reconsider it. Such a host would not link this binary's `main`, and
  `System::exits_after_command` already separates the two cases.
- Peak RSS may differ from glibc's. The perf harness records it per sample.

## How we would know this was wrong

- A 41-sample interleaved A/B on any of the four projects above 1.00 child
  CPU for mimalloc against glibc, on the same source.
- A difference in CLI output between the two allocators, which would mean
  undefined behaviour somewhere. The check is a `cmp` on the four projects,
  which matched at this decision.
