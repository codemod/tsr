//! Parse + bind + check, comparable against typescript-go.
//!
//! Mirrors `benches/go/check_test.go`. Both sides parse a fresh file, bind it,
//! and force the checker over every symbol on every iteration, and both report
//! the total — the checker's own cost is the difference against the parse+bind
//! benchmark, which `cargo xtask perf` derives.
//!
//! # Why the work has to be forced, and why over *symbols*
//!
//! The checker is lazy on both sides: a `Checker` that is constructed and never
//! asked anything does nothing at all, and asking twice is answered from a memo
//! ([ADR-0013](../../../docs/adr/0013-checker-memoisation.md)). Timing
//! construction alone would report both implementations as infinitely fast, the
//! same trap `BindSourceFile`'s idempotence sets for the binder benchmark.
//!
//! Forcing over **every symbol** rather than every expression is the choice
//! that makes the two sides comparable. `getTypeOfSymbol` is where upstream's
//! declaration-type work lives, it is reachable identically on both sides, and
//! it pulls in annotations, initialisers, signatures and declared types behind
//! it. Walking expressions instead would measure this port's *coverage* — an
//! unported form returns `errorType` immediately and costs nothing — so a
//! partial checker would look fast in proportion to how much it cannot do. That
//! is the failure mode `BIND_IS_GATED` in `xtask/src/perf.rs` was introduced to
//! avoid, and it is the reason this benchmark is reported and not gated.
//!
//! Parsing and binding stay inside the timed region for the same reason they do
//! in the parse+bind benchmark: re-doing them removes the memo instead of
//! working around it, and their cost is already known separately.
//!
//! JSDoc is off, matching the gated arm of the parse benchmark — typescript-go
//! does not build JSDoc nodes for `.ts`/`.tsx`, see
//! [ADR-0010](../../../docs/adr/0010-jsdoc-is-a-parse-option.md).
//!
//! Run with `cargo bench -p tsr-checker`.

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

/// Parse, bind, and force a type for every symbol.
///
/// Returns the type count so nothing can be optimised away, and so a run that
/// silently checked nothing is visible in the output rather than only in the
/// timing.
fn parse_bind_check(name: &str, source: &str) -> usize {
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
    let mut checker = tsr_checker::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let symbols: Vec<_> = bound.symbols().iter().map(|(id, _)| id).collect();
    for symbol in symbols {
        black_box(checker.get_type_of_symbol(symbol));
    }
    checker.type_count()
}

fn measure(name: &str, source: &str, target: Duration) -> (u64, f64, usize) {
    for _ in 0..3 {
        black_box(parse_bind_check(name, source));
    }
    let mut iterations: u64 = 1;
    let mut types = 0;
    loop {
        let start = Instant::now();
        for _ in 0..iterations {
            types = black_box(parse_bind_check(name, source));
        }
        let elapsed = start.elapsed();
        if elapsed >= target || iterations >= 1 << 31 {
            return (iterations, elapsed.as_nanos() as f64 / iterations as f64, types);
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
            let (iterations, ns, types) = measure(name, source, target);
            (name.clone(), iterations, ns, types)
        })
        .collect();

    if json {
        println!("[");
        for (i, (name, iterations, ns, types)) in measured.iter().enumerate() {
            let comma = if i + 1 == measured.len() { "" } else { "," };
            println!(
                "  {{\"name\": {name:?}, \"iterations\": {iterations}, \
                 \"ns_per_op_parse_bind_check\": {ns:.1}, \"types\": {types}}}{comma}"
            );
        }
        println!("]");
        return;
    }

    println!("{:<46} {:>10} {:>20} {:>10}", "fixture", "iters", "parse+bind+check ns", "types");
    for (name, iterations, ns, types) in &measured {
        println!("{name:<46} {iterations:>10} {ns:>20.1} {types:>10}");
    }
    println!(
        "\nThe checker's own cost is this minus the parse+bind benchmark. \
         Reported, not gated: this port answers a fraction of what upstream does, \
         and an unported form costs nothing, so the ratio flatters us by exactly \
         the amount of work not yet written. See `BIND_IS_GATED` in xtask/src/perf.rs."
    );
}
