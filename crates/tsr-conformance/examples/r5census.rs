//! Round-5 TS2322/TS2345 census (lane r5-triage2322, `tsr-2zk`): every
//! missing or extra relation diagnostic in the listed cases, with the native
//! message chain, this port's rendered message, the `report_assignability_failure`
//! gate (missing side) and the source line.
//!
//! ```text
//! TSR_ASSIGN_PROBE=1 CASES=/tmp/cases.txt CODES=2322,2345 \
//!   cargo run --release -p tsr-conformance --example r5census > census.tsv
//! ```
//!
//! One row per differing line: `case side file:line:col code gate native tsr
//! source`. `CODES=` (empty) keeps every code. `docs/parity/notes/r5-triage2322.md` is built from it.

use std::collections::{BTreeMap, HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_checker::assignreport::{
    PROBE_OBJECT_LITERAL_UNION, PROBE_PAIR_NOT_REPORTABLE, PROBE_RELATION_DECLINED, PROBE_REPORTED,
};
use tsr_conformance::{Corpus, errors_baseline, repo_root};

type Key = (String, u32, u32, u32);

/// The baseline's message chain per key: the header line and its indented
/// elaboration, joined with ` | `.
fn baseline_texts(baseline: &str) -> BTreeMap<Key, Vec<String>> {
    let mut texts: BTreeMap<Key, Vec<String>> = BTreeMap::new();
    let mut lines = baseline.lines().peekable();
    while let Some(line) = lines.next() {
        let Some((file, rest)) = line.split_once('(') else { continue };
        let Some((position, rest)) = rest.split_once("): error TS") else { continue };
        let Some((l, c)) = position.split_once(',') else { continue };
        let Some((code, message)) = rest.split_once(": ") else { continue };
        let (Ok(l), Ok(c), Ok(code)) = (l.trim().parse(), c.trim().parse(), code.trim().parse())
        else {
            continue;
        };
        let mut chain = message.trim().to_string();
        while let Some(next) = lines.peek() {
            if next.starts_with("  ") && !next.trim().is_empty() {
                chain.push_str(" | ");
                chain.push_str(next.trim());
                lines.next();
            } else {
                break;
            }
        }
        texts.entry((file.to_string(), l, c, code)).or_default().push(chain);
    }
    texts
}

fn main() {
    let root = repo_root();
    let list = std::fs::read_to_string(std::env::var("CASES").expect("CASES")).expect("cases");
    let wanted: HashSet<String> = list.lines().map(str::to_string).collect();
    let codes: Vec<u32> = std::env::var("CODES")
        .unwrap_or_else(|_| "2322,2345".into())
        .split(',')
        .filter_map(|c| c.parse().ok())
        .collect();
    let cases = Corpus::from_repo_root(&root).discover().expect("corpus");
    let mut rows: Vec<String> = cases
        .par_iter()
        .filter(|case| wanted.contains(&case.name))
        .flat_map_iter(|case| {
            let mut out = Vec::new();
            let Ok(baseline) = case.expected_errors() else { return out };
            let baseline = baseline.unwrap_or_default();
            let expected = errors_baseline::parse(&baseline);
            let texts = baseline_texts(&baseline);
            let Ok(test) = case.load() else { return out };
            let rendered = tsr_conformance::diagnostics_suite::rendered_for(&test);
            let probe = tsr_conformance::diagnostics_suite::assignability_probe_for(&test);
            let source = |file: &str, line: u32| -> String {
                test.files
                    .iter()
                    .find(|u| u.name.ends_with(file))
                    .and_then(|u| u.content.lines().nth(line as usize - 1))
                    .unwrap_or("")
                    .trim()
                    .chars()
                    .take(160)
                    .collect()
            };
            let mut have: HashMap<Key, Vec<String>> = HashMap::new();
            for (key, text) in &rendered {
                have.entry(key.clone()).or_default().push(text.clone());
            }
            let mut want: HashMap<Key, usize> = HashMap::new();
            for d in &expected {
                *want.entry((d.file.clone(), d.line, d.column, d.code)).or_default() += 1;
            }
            let mut got: HashMap<Key, usize> = HashMap::new();
            for (key, _) in &rendered {
                *got.entry(key.clone()).or_default() += 1;
            }
            let mut keys: Vec<&Key> = want.keys().chain(got.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                if !codes.is_empty() && !codes.contains(&key.3) {
                    continue;
                }
                let w = want.get(key).copied().unwrap_or(0);
                let g = got.get(key).copied().unwrap_or(0);
                if w == g {
                    continue;
                }
                let side = if w > g { "MISS" } else { "EXTRA" };
                let gate = if w > g {
                    let verdict = probe
                        .iter()
                        .filter(|p| p.file == key.0 && p.line == key.1 && p.column == key.2)
                        .map(|p| p.verdict)
                        .max_by_key(|v| match *v {
                            PROBE_REPORTED => 4u8,
                            PROBE_RELATION_DECLINED => 3,
                            PROBE_PAIR_NOT_REPORTABLE => 2,
                            PROBE_OBJECT_LITERAL_UNION => 1,
                            _ => 0,
                        });
                    match verdict {
                        Some(PROBE_REPORTED) => "REPORTED",
                        Some(PROBE_RELATION_DECLINED) => "DECLINED",
                        Some(PROBE_PAIR_NOT_REPORTABLE) => "NOTREPORTABLE",
                        Some(PROBE_OBJECT_LITERAL_UNION) => "OLUNION",
                        _ => "NEVER",
                    }
                } else {
                    "-"
                };
                let native = texts.get(key).map(|t| t.join(" || ")).unwrap_or_default();
                let tsr = have.get(key).map(|t| t.join(" || ")).unwrap_or_default();
                out.push(format!(
                    "{}\t{side}\t{}:{}:{}\t{}\t{gate}\t{}\t{}\t{}",
                    case.name,
                    key.0,
                    key.1,
                    key.2,
                    key.3,
                    native.replace('\t', " "),
                    tsr.replace(['\t', '\n'], " "),
                    source(&key.0, key.1).replace('\t', " ")
                ));
            }
            out
        })
        .collect();
    rows.sort();
    for row in rows {
        println!("{row}");
    }
}
