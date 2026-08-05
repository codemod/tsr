//! `cargo run -p tsr-conformance --bin coverage`
//!
//! Runs every suite over the corpus, prints a summary, and writes the committed
//! snapshots under `crates/tsr-conformance/snapshots/`.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tsr_conformance::{
    Corpus,
    binder_suite::BinderSymbols,
    diagnostics_suite::Diagnostics,
    dts_emit_suite::DtsEmit,
    dts_shape_suite::DtsShape,
    dts_suite::IsolatedDeclarations,
    dts_target_suite::DtsReachableTarget,
    loader_suite::FileLoaderRequests,
    module_suite::ModuleResolution,
    printer_suite::PrinterRoundTrip,
    repo_root, run_suite,
    scanner_suite::{ScannerCleanFiles, ScannerTermination},
    snapshot,
    suite::Suite,
    suites::{BaselineResolution, CorpusIngest, Parser, ParserReachable},
    types_suite::CheckerTypes,
    upstream_commit,
};

/// Stack for each corpus worker.
///
/// The corpus is a *compiler* test suite and contains inputs written to break
/// compilers. `compiler/binderBinaryExpressionStress` is 4,971 lines of one
/// binary-operator chain, and printing it recurses once per operand:
///
/// | | parse | parse + print |
/// |---|---|---|
/// | debug | under 1 MiB | **between 6 and 8 MiB** |
/// | release | under 1 MiB | under 1 MiB |
///
/// Rayon gives a worker 2 MiB by default, so the debug run aborted with
/// `has overflowed its stack` — and only the debug run, which is why a release-only
/// habit hid it.
///
/// **Two claims above were too narrow, corrected 2026-08-05 by
/// `examples/stack_depth.rs`.** "The parser is unaffected: it climbs precedence in a
/// loop" is true only of the *binary* shape. And the binder, recorded as passing at
/// 2 MiB, does not in general. Minimum surviving stack at nesting depth 5,000, by
/// generated shape:
///
/// | shape | parse (debug/release) | parse+bind (debug/release) |
/// |---|---|---|
/// | `a + a + a …` | 256 KiB / 256 KiB | 4 MiB / 4 MiB |
/// | `((((…))))` | 1 MiB / 256 KiB | **8 MiB** / 4 MiB |
/// | `Array<Array<…>>` | 1 MiB / 256 KiB | 4 MiB / 4 MiB |
/// | nested conditional types | 4 MiB / 1 MiB | 4 MiB / 2 MiB |
///
/// So **the binder alone exceeds rayon's 2 MiB default at depth 5,000 on three of
/// four shapes, in release as well as debug.** The original measurement tested one
/// corpus case, whose shape happens to be the one the parser loops over.
///
/// Upstream does not need any of this, and has nothing to port: Go's runtime grows a
/// goroutine's stack on demand to 1 GiB, so recursing 5,000 deep costs it nothing.
/// A Rust thread's stack is fixed at spawn, and wasm cannot grow one at all. That is
/// a difference between the languages rather than between the two implementations.
///
/// **Now 8 MiB, the budget
/// [ADR-0029](../../../docs/adr/0029-stack-discipline-is-guards-plus-a-budget.md)
/// adopts** (`bd tsr-el3.1`, 2026-08-05). It was 32 MiB while the walks were
/// unguarded; that number is what let the suites pass over trees no library
/// consumer could have walked, which is the failure this whole issue exists to
/// surface.
///
/// What made the reduction safe is that the bounding moved into the walks
/// themselves: `tsr-binder`'s `MAX_DEPTH` and `tsr-printer`'s
/// `MAX_EXPRESSION_DEPTH`, both 1,000, plus an iterative left-spine emit for the
/// one shape that genuinely goes deeper. Measured over 16,207 corpus files by
/// `examples/bind_depth.rs`, peak bind depth is 7 at p50, 25 at p99.9, and 285 for
/// the deepest file that is not a left-leaning binary chain.
///
/// Keep this at the budget. Raising it to make a suite pass would restore exactly
/// the blind spot described above: a green run proving only that *this harness* has
/// a stack no one else does.
const WORKER_STACK: usize = 8 * 1024 * 1024;

fn main() -> Result<()> {
    // Before any suite runs: `build_global` may only be called once, and rayon
    // builds a default pool on first use.
    rayon::ThreadPoolBuilder::new()
        .stack_size(WORKER_STACK)
        .build_global()
        .context("sizing the corpus thread pool")?;

    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);

    if !corpus.is_available() {
        bail!(
            "corpus not found at {}\n\n\
             The nested TypeScript submodule holds the test cases. Run:\n  \
             git submodule update --init --recursive",
            corpus.cases_root().display()
        );
    }

    let cases = corpus.discover().context("discovering cases")?;
    let commit = upstream_commit(&root);
    println!("corpus: {} cases, upstream {commit}\n", cases.len());

    let snapshot_dir = root.join("crates/tsr-conformance/snapshots");
    std::fs::create_dir_all(&snapshot_dir).context("creating snapshot directory")?;

    let suites: Vec<Box<dyn Suite + Sync>> = vec![
        Box::new(CorpusIngest),
        Box::new(BaselineResolution),
        Box::new(ParserReachable),
        Box::new(ScannerTermination),
        Box::new(ScannerCleanFiles),
        Box::new(Parser),
        Box::new(BinderSymbols),
        Box::new(ModuleResolution),
        Box::new(FileLoaderRequests),
        Box::new(IsolatedDeclarations),
        Box::new(DtsReachableTarget),
        Box::new(DtsEmit),
        Box::new(DtsShape),
        Box::new(PrinterRoundTrip),
        Box::new(CheckerTypes),
        Box::new(Diagnostics),
    ];

    let mut rows = Vec::new();
    for suite in &suites {
        let result = run_suite(suite.as_ref(), &cases);
        let rendered = snapshot::render(&result, &commit);

        let path: PathBuf = snapshot_dir.join(format!("{}.snap", result.name));
        std::fs::write(&path, &rendered).with_context(|| format!("writing {}", path.display()))?;

        rows.push((
            result.name.clone(),
            result.passed,
            result.total(),
            result.percentage(),
            result.skipped,
            result.line_percentage(),
        ));
    }

    // `lines` is the gradient column: the share of individual assertion lines
    // matched, for the suites whose baselines are lists of positioned assertions.
    // It is always at least the pass rate — a case passes only when every one of
    // its lines matches — so it prints beside the gate and never in place of it.
    println!("{:<28} {:>18}  {:>8}  {:>8}  {:>8}", "suite", "passed", "rate", "skipped", "lines");
    for (name, passed, total, pct, skipped, line_pct) in rows {
        let lines = line_pct.map_or_else(|| "—".to_string(), |rate| format!("{rate:.2}%"));
        println!(
            "{name:<28} {:>18}  {pct:>7.2}%  {skipped:>8}  {lines:>8}",
            format!("{passed}/{total}")
        );
    }
    println!("\nsnapshots written to {}", snapshot_dir.display());

    Ok(())
}
