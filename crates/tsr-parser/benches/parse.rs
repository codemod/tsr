//! Parse benchmark, comparable against typescript-go's `BenchmarkParse`.
//!
//! Deliberately mirrors `internal/parser/parser_test.go:23` rather than using a
//! Rust benchmark framework: the point is a ratio against upstream, and the two
//! sides have to measure the same thing the same way. See
//! [ADR-0009](../../../docs/adr/0009-performance-gate.md).
//!
//! Matching decisions, each of which would otherwise quietly bias the ratio:
//!
//! - **The same five fixtures**, resolved from the same submodule paths as
//!   `fixtures.BenchFixtures`.
//! - **Single-threaded.** Both sides measure work, not scheduling.
//! - **Parse only** — no binding, no checking, and the tree is dropped inside the
//!   timed region exactly as Go drops its result to the GC.
//! - **Parent assignment on.** typescript-go does it during `finishNode`; a
//!   profile of its parser attributes 9.6% of `dom.generated.d.ts` to it. Ours is
//!   a separate pass and it runs here, because the alternative is reporting a
//!   ratio against work only one side performs.
//! - **Two arms, and the like-for-like one is `-jsdoc`.** typescript-go does not
//!   build JSDoc nodes for `.ts`/`.tsx` files at all; it sets a flag and defers.
//!   Comparing our JSDoc-building parse against that measures a different amount
//!   of work — on `dom.generated.d.ts` it is 46% more of it. Both arms are
//!   reported so neither number can be quoted without the other.
//! - **Go's adaptive iteration count**: run for a target duration and divide,
//!   rather than a fixed count, so short and long fixtures get comparable
//!   statistical weight.
//!
//! Run with `cargo bench -p tsr-parser`, or `--bench parse -- --json` for CI.

// A counting allocator has to implement `GlobalAlloc`, which is an unsafe trait;
// there is no safe way to observe allocation. Confined to this benchmark, which
// ships in no binary. See docs/adr/0011-unsafe-is-opt-in.md.
#![allow(unsafe_code)]

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use tsr_core::Arena;

/// Counts allocations so the comparison can include Go's `B/op` and `allocs/op`.
///
/// Relaxed ordering throughout: the benchmark is single-threaded, and the
/// counters are read only after the timed region ends.
struct Counting;

static ALLOCATED: AtomicU64 = AtomicU64::new(0);
static ALLOCATIONS: AtomicU64 = AtomicU64::new(0);

// SAFETY: every method forwards to `System`, which is a valid allocator; the
// counters are side effects that do not affect the returned pointers.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        ALLOCATED.fetch_add(layout.size() as u64, Ordering::Relaxed);
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        // Only the growth counts as newly allocated bytes, which is how Go
        // accounts for a slice that grows in place.
        ALLOCATED.fetch_add(new_size.saturating_sub(layout.size()) as u64, Ordering::Relaxed);
        ALLOCATIONS.fetch_add(1, Ordering::Relaxed);
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// The workspace root, found by walking up from this file.
fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ has a grandparent")
        .to_path_buf()
}

/// The same five fixtures as `fixtures.BenchFixtures`, in the same order.
///
/// `empty.ts` is in the set deliberately: it measures fixed per-file overhead,
/// which is what an editor reopening files actually pays.
fn fixtures() -> Vec<(String, String)> {
    let ts = repo_root().join("vendor/typescript-go/_submodules/TypeScript");
    let mut out = vec![("empty.ts".to_string(), String::new())];
    for (name, relative) in [
        ("checker.ts", "src/compiler/checker.ts"),
        ("dom.generated.d.ts", "src/lib/dom.generated.d.ts"),
        ("Herebyfile.mjs", "Herebyfile.mjs"),
        (
            "jsxComplexSignatureHasApplicabilityError.tsx",
            "tests/cases/compiler/jsxComplexSignatureHasApplicabilityError.tsx",
        ),
    ] {
        match std::fs::read_to_string(ts.join(relative)) {
            Ok(text) => out.push((name.to_string(), text)),
            // Upstream's harness skips a missing fixture rather than failing; so
            // does this, but it says so, because a silently short fixture list
            // would make the ratio look like a comparison it is not.
            Err(error) => eprintln!("skipping {name}: {error}"),
        }
    }
    out
}

struct Measurement {
    name: String,
    bytes: usize,
    iterations: u64,
    ns_per_op: f64,
    bytes_per_op: f64,
    allocs_per_op: f64,
}

fn measure(name: &str, source: &str, target: Duration, jsdoc: bool) -> Measurement {
    let options = tsr_parser::ParseOptions {
        script_kind: tsr_parser::ScriptKind::from_file_name(name),
        // Parents are always on: typescript-go assigns them inside `finishNode`
        // and cannot opt out, so leaving them off would compare our parse against
        // 9.6% of work upstream is doing and we are not.
        parents: true,
        jsdoc,
        defer_ts_jsdoc: false,
        module_indicator: tsr_parser::ModuleIndicatorOptions::default(),
    };
    // Warm the allocator and the instruction cache before anything is recorded;
    // otherwise the first fixture pays for both and looks slower than it is.
    for _ in 0..3 {
        let arena = Arena::new();
        black_box(tsr_parser::parse_with_options(&arena, source, options));
    }

    let mut iterations: u64 = 1;
    loop {
        ALLOCATED.store(0, Ordering::Relaxed);
        ALLOCATIONS.store(0, Ordering::Relaxed);

        let start = Instant::now();
        for _ in 0..iterations {
            let arena = Arena::new();
            let parsed = tsr_parser::parse_with_options(&arena, source, options);
            black_box(&parsed);
        }
        let elapsed = start.elapsed();

        // Go's approach: grow the iteration count until the run is long enough to
        // dominate timer resolution, then report the last run.
        if elapsed >= target || iterations >= 1 << 31 {
            let allocated = ALLOCATED.load(Ordering::Relaxed);
            let allocations = ALLOCATIONS.load(Ordering::Relaxed);
            #[allow(clippy::cast_precision_loss)]
            return Measurement {
                name: name.to_string(),
                bytes: source.len(),
                iterations,
                ns_per_op: elapsed.as_nanos() as f64 / iterations as f64,
                bytes_per_op: allocated as f64 / iterations as f64,
                allocs_per_op: allocations as f64 / iterations as f64,
            };
        }
        iterations = (iterations * 2).max(1);
    }
}

fn main() {
    let json = std::env::args().any(|a| a == "--json");
    let target = Duration::from_secs(
        std::env::var("TSR_BENCH_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3),
    );

    let loaded = fixtures();
    let run = |jsdoc: bool| -> Vec<Measurement> {
        loaded.iter().map(|(name, source)| measure(name, source, target, jsdoc)).collect()
    };
    // The like-for-like arm first: it is the one ADR-0009's ratio is against.
    let without = run(false);
    let measurements = run(true);

    if json {
        println!("[");
        for (i, (m, w)) in measurements.iter().zip(&without).enumerate() {
            let comma = if i + 1 == measurements.len() { "" } else { "," };
            // Both arms, named so a consumer cannot pick the flattering one by
            // accident: `ns_per_op_without_jsdoc` is the like-for-like figure.
            println!(
                "  {{\"name\": {:?}, \"bytes\": {}, \"iterations\": {}, \
                 \"ns_per_op_without_jsdoc\": {:.1}, \"ns_per_op_with_jsdoc\": {:.1}, \
                 \"bytes_per_op\": {:.0}, \"allocs_per_op\": {:.1}}}{comma}",
                m.name,
                m.bytes,
                m.iterations,
                w.ns_per_op,
                m.ns_per_op,
                m.bytes_per_op,
                m.allocs_per_op
            );
        }
        println!("]");
        return;
    }

    println!(
        "{:<46} {:>14} {:>14} {:>13} {:>11}",
        "fixture", "ns/op -jsdoc", "ns/op +jsdoc", "B/op +jsdoc", "allocs +jsdoc"
    );
    for (m, w) in measurements.iter().zip(&without) {
        println!(
            "{:<46} {:>14.1} {:>14.1} {:>13.0} {:>11.1}",
            m.name, w.ns_per_op, m.ns_per_op, m.bytes_per_op, m.allocs_per_op
        );
    }
    println!(
        "\n(-jsdoc is the like-for-like comparison against typescript-go, which does\n \
         not build JSDoc nodes for .ts/.tsx. See docs/adr/0010-jsdoc-is-a-parse-option.md.)"
    );
    println!("\n(source sizes: {})", {
        let mut parts: Vec<String> =
            measurements.iter().map(|m| format!("{} {}B", m.name, m.bytes)).collect();
        parts.sort();
        parts.join(", ")
    });
}
