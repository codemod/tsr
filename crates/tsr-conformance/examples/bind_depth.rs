//! How deep `bind()` actually recurses on real corpus files.
//!
//! Diagnostic tool, not a suite. `examples/stack_depth.rs` measures how much
//! stack a *generated* shape costs; this measures the other half of the same
//! question — how deep real input goes — because a depth guard needs both. A
//! limit chosen without this fires on valid TypeScript.
//!
//! Reports the distribution and names the deepest cases, so the guard constant
//! in `tsr-binder` can be justified by a number rather than picked round.

use std::collections::BTreeMap;

use tsr_conformance::{Corpus, repo_root};

fn main() {
    // A generous stack: this measures how deep the walk goes, so it must not be
    // the thing that stops it. The whole point is that the default is too small.
    std::thread::Builder::new()
        .stack_size(512 * 1024 * 1024)
        .spawn(measure)
        .expect("spawn")
        .join()
        .expect("measure");
}

fn measure() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    let cases = corpus.discover().expect("discover");

    // depth -> how many files reached it, and the deepest few by name.
    let mut histogram: BTreeMap<u32, usize> = BTreeMap::new();
    let mut deepest: Vec<(u32, String)> = Vec::new();

    for case in &cases {
        let Ok(test_case) = case.load() else { continue };
        for file in &test_case.files {
            let lower = file.name.to_ascii_lowercase();
            if !["ts", "tsx", "js", "jsx", "mts", "cts", "mjs", "cjs"]
                .iter()
                .any(|ext| lower.ends_with(&format!(".{ext}")))
            {
                continue;
            }
            if file.content.contains('\u{FFFD}') {
                continue;
            }

            let parsed = tsr_parser::ParsedFile::parse_with_script_kind(
                file.content.clone(),
                tsr_parser::ScriptKind::TypeScript,
            );
            let depth = parsed.with_ast_and_arena(|ast, arena| {
                let bound = tsr_binder::bind(
                    arena,
                    ast,
                    parsed.nodes(),
                    tsr_binder::FileInfo { name: &file.name, text: parsed.source() },
                );
                bound.max_depth()
            });

            // Bucket by power of two so the tail is readable.
            let bucket = depth.next_power_of_two();
            *histogram.entry(bucket).or_default() += 1;
            deepest.push((depth, format!("{}::{}", case.name, file.name)));
        }
    }

    deepest.sort_unstable_by_key(|(depth, _)| std::cmp::Reverse(*depth));

    println!("bind() recursion depth over the corpus\n");
    println!("{:>12}  {:>8}", "depth <=", "files");
    for (bucket, count) in &histogram {
        println!("{bucket:>12}  {count:>8}");
    }

    println!("\ndeepest 25 files:\n");
    for (depth, name) in deepest.iter().take(25) {
        println!("{depth:>8}  {name}");
    }

    let total = deepest.len();
    for percentile in [50.0_f64, 90.0, 99.0, 99.9] {
        #[allow(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::cast_precision_loss
        )]
        let index = ((total as f64) * (1.0 - percentile / 100.0)) as usize;
        println!("\np{percentile}: {}", deepest[index.min(total - 1)].0);
    }
    println!("\n{total} files measured");
}
