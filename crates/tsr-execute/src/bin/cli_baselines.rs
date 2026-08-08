//! Replay every upstream `tsc` baseline and report the tally.
//!
//! `STATUS-cli.md` phase 0. Prints a number rather than asserting one: this is
//! an instrument, and a number moving is the signal every later phase is
//! steered by. Run it with
//!
//! ```text
//! cargo run --release -p tsr-execute --bin cli_baselines
//! ```

use std::path::{Path, PathBuf};

use tsr_execute::baseline::{Verdict, parse_baseline, run_baseline};

fn main() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|root| root.join("vendor/typescript-go/testdata/baselines/reference/tsc"))
        .expect("the workspace root is two directories up");

    if !root.is_dir() {
        eprintln!(
            "no baselines at {}; run `git submodule update --init --recursive`",
            root.display()
        );
        std::process::exit(1);
    }

    let mut files = Vec::new();
    collect(&root, &mut files);
    files.sort();

    let (mut passed, mut failed, mut needs_emit, mut unparsed) = (0_i32, 0_i32, 0_i32, 0_i32);
    let mut failures: Vec<(String, String)> = Vec::new();

    for path in &files {
        let name = path.strip_prefix(&root).unwrap_or(path).display().to_string();
        let Ok(text) = std::fs::read_to_string(path) else { continue };
        let Some(baseline) = parse_baseline(&name, &text) else {
            unparsed += 1;
            continue;
        };
        match run_baseline(&baseline) {
            Verdict::Passed => passed += 1,
            Verdict::NeedsEmit => needs_emit += 1,
            Verdict::Failed { reason } => {
                failed += 1;
                failures.push((name, reason));
            }
        }
    }

    let judged = passed + failed;
    let rate = if judged == 0 { 0.0 } else { f64::from(passed) * 100.0 / f64::from(judged) };
    println!("cli_baselines: {passed}/{judged} judged ({rate:.2}%)");
    println!("  needs emit (excluded): {needs_emit}");
    println!("  unparsed (excluded):   {unparsed}");
    println!("  total files:           {}", files.len());

    if !failures.is_empty() {
        println!("\nfirst {} failures:", failures.len().min(25));
        for (name, reason) in failures.iter().take(25) {
            println!("  {name}\n    {reason}");
        }
    }
}

fn collect(directory: &Path, into: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else { return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect(&path, into);
        } else if path.extension().is_some_and(|extension| extension == "js") {
            into.push(path);
        }
    }
}
