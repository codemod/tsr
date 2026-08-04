//! `cargo run -p tsr-conformance --bin coverage`
//!
//! Runs every suite over the corpus, prints a summary, and writes the committed
//! snapshots under `crates/tsr-conformance/snapshots/`.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tsr_conformance::{
    Corpus,
    binder_suite::BinderSymbols,
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
/// habit hid it. The parser is unaffected: it climbs precedence in a loop.
///
/// Upstream does not need this. Go grows a goroutine's stack on demand up to 1 GiB,
/// so `emitBinaryExpression` recursing 5,000 deep costs it nothing; a Rust thread's
/// stack is fixed at spawn. That is a difference between the languages rather than
/// between the two implementations, and it is the reason a port has to size this
/// explicitly. Making the printer iterative is the real fix and is `bd tsr-el3`.
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
