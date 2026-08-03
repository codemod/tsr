//! Bucket parser failures by message and surrounding source, to pick the next gap.
//!
//! Diagnostic tool, not part of the suite: the committed snapshot caps failure
//! detail at 100 entries so a regression stays reviewable, which is the wrong
//! trade when you want the whole distribution.

use std::collections::BTreeMap;

use tsr_conformance::{Corpus, repo_root};

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    let cases = corpus.discover().expect("discover");

    let mut buckets: BTreeMap<String, (usize, Vec<String>)> = BTreeMap::new();
    let mut failed = 0usize;
    let mut passed = 0usize;

    for case in &cases {
        if case.has_known_divergence() || case.has_varied_errors() {
            continue;
        }
        match case.expected_errors() {
            Ok(None) => {}
            _ => continue,
        }
        let Ok(test_case) = case.load() else { continue };

        let mut case_failed = false;
        for file in &test_case.files {
            if !file.name.to_ascii_lowercase().ends_with(".ts")
                && !file.name.to_ascii_lowercase().ends_with(".tsx")
                && !file.name.to_ascii_lowercase().ends_with(".js")
                && !file.name.to_ascii_lowercase().ends_with(".jsx")
                && !file.name.to_ascii_lowercase().ends_with(".mts")
                && !file.name.to_ascii_lowercase().ends_with(".cts")
                && !file.name.to_ascii_lowercase().ends_with(".mjs")
                && !file.name.to_ascii_lowercase().ends_with(".cjs")
            {
                continue;
            }
            if file.content.contains('\u{FFFD}') {
                continue;
            }
            let arena = tsr_core::Arena::new();
            let script_kind = tsr_parser::ScriptKind::from_file_name(&file.name);
            let result = tsr_parser::parse_with_script_kind(&arena, &file.content, script_kind);
            if let Some(first) = result.diagnostics.first() {
                case_failed = true;
                let start = first.span.start as usize;
                let lo = file.content[..start.min(file.content.len())]
                    .char_indices()
                    .rev()
                    .nth(40)
                    .map_or(0, |(i, _)| i);
                let hi = (start + 25).min(file.content.len());
                let snippet = file.content[lo..hi].replace('\n', "\\n");
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
