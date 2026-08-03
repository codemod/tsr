//! `cargo run -p tsr-conformance --bin coverage`
//!
//! Runs every suite over the corpus, prints a summary, and writes the committed
//! snapshots under `crates/tsr-conformance/snapshots/`.

use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use tsr_conformance::{
    Corpus, repo_root, run_suite,
    scanner_suite::{ScannerCleanFiles, ScannerTermination},
    snapshot,
    suite::Suite,
    suites::{BaselineResolution, CorpusIngest, Parser, ParserReachable},
    upstream_commit,
};

fn main() -> Result<()> {
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

    let suites: Vec<Box<dyn Suite>> = vec![
        Box::new(CorpusIngest),
        Box::new(BaselineResolution),
        Box::new(ParserReachable),
        Box::new(ScannerTermination),
        Box::new(ScannerCleanFiles),
        Box::new(Parser),
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
