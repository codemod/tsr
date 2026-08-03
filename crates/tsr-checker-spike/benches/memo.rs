//! Comparing the three memoisation styles.
//!
//! The question is not which is fastest in the abstract — all three are doing the
//! same asymptotic work — but whether any of them is *disqualifyingly* slow, and
//! how much the safe-but-indirect style costs against the one that hands out
//! references. A style that is 10% slower and cannot be got wrong is a better
//! basis for 60k lines than one that is 10% faster and panics on a bad edit.
//!
//! Run with `cargo bench -p tsr-checker-spike`.

// Nanosecond counts printed for a human; `f64` loses nothing at these magnitudes.
#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
use std::time::{Duration, Instant};

use tsr_checker_spike::{Program, arena, cells, ids};

/// Median of `runs`, in nanoseconds. Median rather than mean so one scheduling
/// hiccup does not move the number.
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

/// Repeat until the run is long enough to dominate timer resolution.
fn measure(target: Duration, mut f: impl FnMut()) -> f64 {
    let mut iterations = 1u32;
    loop {
        let elapsed = time_ns(3, || {
            for _ in 0..iterations {
                f();
            }
        });
        if elapsed >= target.as_nanos() as f64 || iterations >= 1 << 20 {
            return elapsed / f64::from(iterations);
        }
        iterations *= 2;
    }
}

fn main() {
    let target = Duration::from_millis(200);
    let programs = vec![
        Program::wide(2_000),
        Program::wide(20_000),
        Program::diamond(20),
        Program::chain(2_000),
        Program::cycle_in_the_middle(20_000),
    ];

    println!(
        "{:<26} {:>8} {:>13} {:>13} {:>13} {:>9}",
        "program", "symbols", "ids ns", "arena ns", "cells ns", "best"
    );
    for program in &programs {
        // Warm up: the first run of each pays for allocator growth.
        black_box(ids::resolve_only(program));
        black_box(arena::resolve_only(program));
        black_box(cells::resolve_only(program));

        let by_ids = measure(target, || {
            black_box(ids::resolve_only(program));
        });
        let by_arena = measure(target, || {
            black_box(arena::resolve_only(program));
        });
        let by_cells = measure(target, || {
            black_box(cells::resolve_only(program));
        });

        let best = [("ids", by_ids), ("arena", by_arena), ("cells", by_cells)]
            .into_iter()
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map_or("-", |(name, _)| name);

        println!(
            "{:<26} {:>8} {:>13.0} {:>13.0} {:>13.0} {:>9}",
            program.name,
            program.len(),
            by_ids,
            by_arena,
            by_cells,
            best
        );
    }

    println!(
        "\nResolution only. An earlier version also timed the structural\n\
         description, which is identical in all three and was large enough to\n\
         flatten every difference to under 4%."
    );
}
