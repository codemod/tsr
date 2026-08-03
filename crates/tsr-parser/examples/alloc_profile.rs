//! Where the parser's time and allocations go. Diagnostic tool.
//!
//! Precision lints are off throughout: every number here is a human-readable
//! summary printed to a terminal, not a value anything computes with.
#![allow(clippy::cast_precision_loss, clippy::uninlined_format_args)]
//!
//! Stands in for a sampling profiler on machines where `perf_event_paranoid`
//! blocks `perf`. It answers three questions the wall-clock benchmark cannot:
//!
//! 1. How much of the parse is scanning? (Tokenize the same input and compare.)
//! 2. What size are the allocations? A bump-arena parser should barely allocate;
//!    a histogram dominated by small powers of two is the signature of `Vec`
//!    growth rather than of any one big buffer.
//! 3. How much do the allocations actually cost? (Re-run with the counting
//!    allocator's own overhead subtracted, and against a no-op parse.)

use std::alloc::{GlobalAlloc, Layout, System};
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Instant;

use tsr_core::Arena;

/// Buckets allocations by size, in powers of two.
struct Histogram;

const BUCKETS: usize = 32;
static SIZES: [AtomicU64; BUCKETS] = [const { AtomicU64::new(0) }; BUCKETS];
static TOTAL_BYTES: AtomicU64 = AtomicU64::new(0);
static REALLOCS: AtomicU64 = AtomicU64::new(0);
static ENABLED: AtomicU64 = AtomicU64::new(0);

fn bucket_of(size: usize) -> usize {
    (usize::BITS - size.leading_zeros()) as usize % BUCKETS
}

// SAFETY: every method forwards to `System`; the counters are side effects that
// do not affect the pointers returned.
unsafe impl GlobalAlloc for Histogram {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) != 0 {
            SIZES[bucket_of(layout.size())].fetch_add(1, Ordering::Relaxed);
            TOTAL_BYTES.fetch_add(layout.size() as u64, Ordering::Relaxed);
        }
        unsafe { System.alloc(layout) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        unsafe { System.dealloc(ptr, layout) };
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: Layout, new_size: usize) -> *mut u8 {
        if ENABLED.load(Ordering::Relaxed) != 0 {
            SIZES[bucket_of(new_size)].fetch_add(1, Ordering::Relaxed);
            REALLOCS.fetch_add(1, Ordering::Relaxed);
            TOTAL_BYTES.fetch_add(new_size.saturating_sub(layout.size()) as u64, Ordering::Relaxed);
        }
        unsafe { System.realloc(ptr, layout, new_size) }
    }
}

#[global_allocator]
static GLOBAL: Histogram = Histogram;

/// Median of `runs` timings, in nanoseconds — median rather than mean so one
/// scheduling hiccup does not move the number.
fn time_ns(runs: u32, mut f: impl FnMut()) -> f64 {
    let mut samples: Vec<f64> = (0..runs)
        .map(|_| {
            let start = Instant::now();
            f();
            start.elapsed().as_nanos() as f64
        })
        .collect();
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

fn main() {
    let path = std::env::args().nth(1).expect("usage: alloc_profile <file>");
    let source = std::fs::read_to_string(&path).expect("read");
    let script_kind = tsr_parser::ScriptKind::from_file_name(&path);
    let runs: u32 = std::env::var("RUNS").ok().and_then(|s| s.parse().ok()).unwrap_or(9);

    // Warm up.
    for _ in 0..3 {
        let arena = Arena::new();
        black_box(tsr_parser::parse_with_script_kind(&arena, &source, script_kind));
    }

    let scan_ns = time_ns(runs, || {
        black_box(tsr_scanner::tokenize(&source));
    });
    let parse_ns = time_ns(runs, || {
        let arena = Arena::new();
        black_box(tsr_parser::parse_with_script_kind(&arena, &source, script_kind));
    });
    // The arena alone, to separate "allocating the chunks" from "using them".
    let arena_ns = time_ns(runs, || {
        black_box(Arena::new());
    });

    // One instrumented parse for the histogram.
    ENABLED.store(1, Ordering::Relaxed);
    let (nodes, jsdoc) = {
        let arena = Arena::new();
        let parsed = tsr_parser::parse_with_script_kind(&arena, &source, script_kind);
        (parsed.nodes.len(), parsed.jsdoc.len())
    };
    ENABLED.store(0, Ordering::Relaxed);

    println!("{path}");
    println!("  {} bytes, {nodes} nodes, {jsdoc} documented", source.len());
    println!();
    println!(
        "  scan only      {:>12.0} ns  ({:>5.1}% of parse)",
        scan_ns,
        100.0 * scan_ns / parse_ns
    );
    println!("  full parse     {:>12.0} ns", parse_ns);
    println!("  arena alone    {:>12.0} ns", arena_ns);
    println!(
        "  parse - scan   {:>12.0} ns  ({:>5.1}%)",
        parse_ns - scan_ns,
        100.0 * (parse_ns - scan_ns) / parse_ns
    );
    println!();

    let total: u64 = SIZES.iter().map(|s| s.load(Ordering::Relaxed)).sum();
    let bytes = TOTAL_BYTES.load(Ordering::Relaxed);
    let reallocs = REALLOCS.load(Ordering::Relaxed);
    println!("  {total} allocations ({reallocs} of them reallocs), {bytes} bytes");
    println!("  {:.1} allocations per node", total as f64 / nodes as f64);
    println!();
    println!("  {:>12}  {:>10}  {:>7}", "size", "count", "share");
    let mut cumulative = 0u64;
    for (i, slot) in SIZES.iter().enumerate() {
        let count = slot.load(Ordering::Relaxed);
        if count == 0 {
            continue;
        }
        cumulative += count;
        let upper = if i == 0 { 0 } else { 1u64 << (i - 1) };
        println!(
            "  {:>10}B+  {:>10}  {:>6.1}%   (cum {:>5.1}%)",
            upper,
            count,
            100.0 * count as f64 / total as f64,
            100.0 * cumulative as f64 / total as f64
        );
    }
}
