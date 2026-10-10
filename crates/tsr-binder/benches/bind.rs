//! Parse+bind, comparable against typescript-go.
//!
//! Mirrors `benches/go/bind_test.go`. Both sides parse a fresh file and bind it
//! on every iteration, and both report the total — the binder's own cost is the
//! difference against the parse-only benchmark, which `cargo xtask perf` derives.
//!
//! Parsing inside the timed region is not laziness. `BindSourceFile` upstream is
//! idempotent: it checks `file.IsBound()` and returns, so binding the same file
//! repeatedly measures a boolean check and reports typescript-go as effectively
//! infinitely fast. Re-parsing removes the possibility instead of working around
//! it, and it costs nothing here because the parse-only number is already known.
//!
//! JSDoc is off, matching the parse benchmark's gated arm: typescript-go does not
//! build JSDoc nodes for `.ts`/`.tsx` (see ADR-0010), so leaving it on would
//! compare different amounts of work.
//!
//! Run with `cargo bench -p tsr-binder`.

// Nanosecond counts printed for a human; `f64` loses nothing at these magnitudes.
#![allow(clippy::cast_precision_loss)]

use std::hint::black_box;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tsr_core::Arena;

/// The same fixtures as `fixtures.BenchFixtures`, in the same order.
fn fixtures() -> Vec<(String, String)> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(std::path::Path::parent)
        .expect("crates/<crate>/ has a grandparent")
        .join("vendor/typescript-go/_submodules/TypeScript");
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
        match std::fs::read_to_string(root.join(relative)) {
            Ok(text) => out.push((name.to_string(), text)),
            // Skipping is upstream's behaviour for a missing fixture; saying so
            // matters because a silently short list looks like a comparison.
            Err(error) => eprintln!("skipping {name}: {error}"),
        }
    }
    out
}

/// Parse and bind once, returning something that cannot be optimised away.
fn parse_and_bind(name: &str, source: &str) -> usize {
    let arena = Arena::new();
    let options = tsr_parser::ParseOptions {
        script_kind: tsr_parser::ScriptKind::from_file_name(name),
        parents: true,
        jsdoc: false,
        defer_ts_jsdoc: false,
        module_indicator: tsr_parser::ModuleIndicatorOptions::default(),
    };
    let parsed = tsr_parser::parse_with_options(&arena, source, options);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
    );
    bound.symbols().len()
}

fn measure(name: &str, source: &str, target: Duration) -> (u64, f64, usize) {
    for _ in 0..3 {
        black_box(parse_and_bind(name, source));
    }
    let mut iterations: u64 = 1;
    let mut symbols = 0;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            symbols = black_box(parse_and_bind(name, source));
        }
        let elapsed = start.elapsed();
        if elapsed >= target || iterations >= 1 << 31 {
            return (iterations, elapsed.as_nanos() as f64 / iterations as f64, symbols);
        }
        iterations *= 2;
    }
}

fn main() {
    let json = std::env::args().any(|a| a == "--json");
    let target = Duration::from_secs(
        std::env::var("TSR_BENCH_SECONDS").ok().and_then(|s| s.parse().ok()).unwrap_or(3),
    );
    let measured: Vec<(String, u64, f64, usize)> = fixtures()
        .iter()
        .map(|(name, source)| {
            let (iterations, ns, symbols) = measure(name, source, target);
            (name.clone(), iterations, ns, symbols)
        })
        .collect();

    if json {
        println!("[");
        for (i, (name, iterations, ns, symbols)) in measured.iter().enumerate() {
            let comma = if i + 1 == measured.len() { "" } else { "," };
            println!(
                "  {{\"name\": {name:?}, \"iterations\": {iterations}, \
                 \"ns_per_op_parse_and_bind\": {ns:.1}, \"symbols\": {symbols}}}{comma}"
            );
        }
        println!("]");
        return;
    }

    println!("{:<46} {:>10} {:>16} {:>10}", "fixture", "iters", "parse+bind ns", "symbols");
    for (name, iterations, ns, symbols) in &measured {
        println!("{name:<46} {iterations:>10} {ns:>16.1} {symbols:>10}");
    }
    println!(
        "\nThe binder's own cost is this minus the parse-only benchmark; \
         `cargo xtask perf` does the subtraction."
    );
}
