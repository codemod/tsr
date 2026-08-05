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
/// **32 MiB is oversized under the policy now adopted**
/// ([ADR-0029](../../../docs/adr/0029-stack-discipline-is-guards-plus-a-budget.md)):
/// depth guards on recursive tree walks do the bounding, and the stack budget is
/// headroom behind them at 8 MiB. This constant should come down to that once the
/// binder and printer guards land, so the suites exercise the guards rather than
/// hiding behind a stack no consumer has. Reducing it before then would re-break the
/// debug run. `bd tsr-el3`.
const WORKER_STACK: usize = 32 * 1024 * 1024;

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
        ));
    }

    println!("{:<28} {:>18}  {:>8}  {:>8}", "suite", "passed", "rate", "skipped");
    for (name, passed, total, pct, skipped) in rows {
        println!("{name:<28} {:>18}  {pct:>7.2}%  {skipped:>8}", format!("{passed}/{total}"));
    }
    println!("\nsnapshots written to {}", snapshot_dir.display());

    Ok(())
}
