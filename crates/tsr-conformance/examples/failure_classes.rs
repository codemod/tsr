//! Bucket parser failures by message and surrounding source, to pick the next gap.
//!
//! Diagnostic tool, not part of the suite: the committed snapshot caps failure
//! detail at 100 entries so a regression stays reviewable, which is the wrong
//! trade when you want the whole distribution.

use std::collections::BTreeMap;

use tsr_conformance::{
    Corpus, repo_root,
    trace_case::{CompilationSetup, prepare_compilation},
};

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    let mut cases = corpus.discover().expect("discover");
    if let Some(filter) = std::env::args().nth(1) {
        cases.retain(|case| case.name.contains(&filter));
    }

    let mut buckets: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut failed = 0usize;
    let mut passed = 0usize;

    for case in &cases {
        if !case.has_any_baseline() || case.has_known_divergence() || case.has_varied_errors() {
            continue;
        }
        match case.expected_errors() {
            Ok(None) => {}
            _ => continue,
        }
        let CompilationSetup::Ready(prepared) = prepare_compilation(case) else { continue };
        let arena = tsr_core::Arena::new();
        let loaded = tsr_compiler::FileLoader::load(
            &arena,
            &prepared.host,
            tsr_compiler::LoadOptions {
                compiler_options: prepared.options,
                root_file_names: prepared.root_file_names,
                default_library_path: String::new(),
            },
        );

        let mut case_failed = false;
        for file in loaded.files.iter().skip(loaded.lib_file_count) {
            if file.text().contains('\u{FFFD}') {
                continue;
            }
            if let Some(first) = file.diagnostics().first() {
                case_failed = true;
                let start = first.span.start as usize;
                let lo = file.text()[..start.min(file.text().len())]
                    .char_indices()
                    .rev()
                    .nth(40)
                    .map_or(0, |(i, _)| i);
                let hi = (start + 25).min(file.text().len());
                let snippet = file.text()[lo..hi].replace('\n', "\\n");
                let key = format!("TS{} {}", first.message.code(), first.text());
                let entry = buckets.entry(key).or_insert((0, Vec::new()));
                entry.0 += 1;
                if entry.1.len() < 3 {
                    entry.1.push(format!("{}: …{snippet}…", case.name));
                }
                break;
            }
        }
        if case_failed {
            failed += 1;
        } else {
            passed += 1;
        }
    }

    println!("passed {passed}, failed {failed}\n");
    let mut sorted: Vec<_> = buckets.into_iter().collect();
    sorted.sort_by_key(|(_, (n, _))| std::cmp::Reverse(*n));
    for (key, (count, samples)) in sorted.iter().take(18) {
        println!("{count:5}  {key}");
        for s in samples {
            println!("         {s}");
        }
    }
}
