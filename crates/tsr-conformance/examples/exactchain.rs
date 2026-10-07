//! LOCAL MEASUREMENT ONLY (never committed): exact header-block parity incl. chains.
use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{CaseEntry, Corpus, repo_root};

fn baseline_entries(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut in_global = false;
    for line in text.lines() {
        if line.starts_with("==== ") {
            break;
        }
        if line.starts_with(' ') {
            if !in_global {
                if let Some(last) = out.last_mut() {
                    last.push('\n');
                    last.push_str(line);
                }
            }
            continue;
        }
        if line.is_empty() {
            continue;
        }
        if line.contains("): error TS") || line.contains("): warning TS") {
            in_global = false;
            out.push(line.to_string());
        } else {
            in_global = true;
        }
    }
    out
}

fn measure(case: &CaseEntry) -> Option<(String, bool, String)> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?.unwrap_or_default();
    if baseline.contains('\u{1b}') {
        return None;
    }
    let mut want = baseline_entries(&baseline);
    let test = case.load().ok()?;
    let mut got: Vec<String> = tsr_conformance::diagnostics_suite::flattened_for(&test)
        .into_iter()
        .map(|((file, line, column, code), text)| {
            format!("{file}({line},{column}): error TS{code}: {text}")
        })
        .collect();
    want.sort();
    got.sort();
    if want == got {
        return Some((case.name.clone(), true, String::new()));
    }
    if std::env::var_os("EXACT_FULL").is_some() {
        let mut out = String::new();
        for w in want.iter().filter(|w| !got.contains(w)) {
            out.push_str(&format!("- {}\n", w.replace('\n', "\n  | ")));
        }
        for g in got.iter().filter(|g| !want.contains(g)) {
            out.push_str(&format!("+ {}\n", g.replace('\n', "\n  | ")));
        }
        return Some((case.name.clone(), false, out));
    }
    let first = want
        .iter()
        .find(|w| !got.contains(w))
        .map(|w| format!("WANT {w}"))
        .or_else(|| got.iter().find(|g| !want.contains(g)).map(|g| format!("GOT {g}")))
        .unwrap_or_default();
    Some((case.name.clone(), false, first.replace('\n', "\\n").replace('\t', " ")))
}

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let mut rows: Vec<(String, bool, String)> = cases.par_iter().filter_map(measure).collect();
    rows.sort();
    let exact = rows.iter().filter(|r| r.1).count();
    if std::env::var_os("EXACT_FULL").is_some() {
        for (name, ok, detail) in &rows {
            if !*ok {
                println!("#### {name}\n{detail}");
            }
        }
        return;
    }
    for (name, ok, detail) in &rows {
        println!("{name}\t{}\t{detail}", if *ok { "EXACT" } else { "DIFF" });
    }
    eprintln!("exact {exact}/{}", rows.len());
}
