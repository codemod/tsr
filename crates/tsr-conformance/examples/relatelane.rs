//! Lane `relate-report` (tsr-2zk.1): every missing TS2322 line in the lane's
//! cases, with the `report_assignability_failure` gate that decided it (or
//! `NEVER` when no assignability report was asked at that position) and the
//! innermost node kind there.
//!
//! ```text
//! TSR_ASSIGN_PROBE=1 cargo run --release -p tsr-conformance --example relatelane
//! ```
use std::collections::{HashMap, HashSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_checker::assignreport::{
    PROBE_OBJECT_LITERAL_UNION, PROBE_PAIR_NOT_REPORTABLE, PROBE_RELATION_DECLINED, PROBE_REPORTED,
};
use tsr_conformance::{Corpus, errors_baseline, repo_root};

fn main() {
    let root = repo_root();
    let lane: HashSet<String> =
        std::fs::read_to_string(root.join("docs/parity/lanes/relate-report.txt"))
            .expect("lane file")
            .lines()
            .filter(|line| !line.starts_with('#'))
            .filter_map(|line| line.split('\t').next().map(str::to_string))
            .collect();
    let code: u32 = std::env::var("CODE").ok().and_then(|c| c.parse().ok()).unwrap_or(2322);
    let cases = Corpus::from_repo_root(&root).discover().expect("corpus");
    // `VERDICT=1`: the lane's cases in `diagverdictdump`'s format instead.
    if std::env::var("VERDICT").is_ok() {
        let mut rows: Vec<String> = cases
            .par_iter()
            .filter(|case| lane.contains(&case.name))
            .filter(|case| {
                !case.has_varied_errors() && !case.has_known_divergence() && case.has_any_baseline()
            })
            .filter_map(|case| {
                let baseline = case.expected_errors().ok()?;
                let mut expected =
                    baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
                let test = case.load().ok()?;
                let mut actual = tsr_conformance::diagnostics_suite::reported_for(&test);
                expected.sort_unstable();
                actual.sort_unstable();
                let verdict = match (expected.is_empty(), expected == actual) {
                    (false, true) => "RIGHT",
                    (false, false) => "WRONG",
                    (true, true) => "EMPTY_RIGHT",
                    (true, false) => "EMPTY_WRONG",
                };
                Some(format!("{}\t{verdict}\t{expected:?}\t{actual:?}", case.name))
            })
            .collect();
        rows.sort();
        for row in rows {
            println!("{row}");
        }
        return;
    }
    let mut rows: Vec<String> = cases
        .par_iter()
        .filter(|case| lane.contains(&case.name))
        .flat_map_iter(|case| {
            let mut out = Vec::new();
            let Ok(Some(baseline)) = case.expected_errors() else { return out };
            let expected = errors_baseline::parse(&baseline);
            let Ok(test) = case.load() else { return out };
            eprintln!("CASE {}", case.name);
            let actual = tsr_conformance::diagnostics_suite::reported_for(&test);
            let mut have: HashMap<(String, u32, u32), usize> = HashMap::new();
            for d in actual.iter().filter(|d| d.code == code) {
                *have.entry((d.file.clone(), d.line, d.column)).or_default() += 1;
            }
            let probe = tsr_conformance::diagnostics_suite::assignability_probe_for(&test);
            let kinds = tsr_conformance::diagnostics_suite::node_kinds_by_position_for(&test);
            for d in expected.iter().filter(|d| d.code == code) {
                let key = (d.file.clone(), d.line, d.column);
                if let Some(n) = have.get_mut(&key)
                    && *n > 0
                {
                    *n -= 1;
                    continue;
                }
                if let Some(unit) = test.files.iter().find(|u| u.name == key.0) {
                    let offset: usize = unit
                        .content
                        .split_inclusive('\n')
                        .take(key.1 as usize - 1)
                        .map(str::len)
                        .sum::<usize>()
                        + key.2 as usize
                        - 1;
                    eprintln!("MISS {} {offset}", key.0);
                }
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
                let verdict = match verdict {
                    Some(PROBE_REPORTED) => "REPORTED",
                    Some(PROBE_RELATION_DECLINED) => "DECLINED",
                    Some(PROBE_PAIR_NOT_REPORTABLE) => "NOTREPORTABLE",
                    Some(PROBE_OBJECT_LITERAL_UNION) => "OLUNION",
                    _ => "NEVER",
                };
                let anchor = kinds
                    .iter()
                    .rfind(|k| k.0 == key.0 && k.1 == key.1 && k.2 == key.2)
                    .map_or_else(|| "?".to_string(), |k| format!("{:?} in {:?}", k.3, k.4));
                out.push(format!(
                    "{}\t{}:{}:{}\t{verdict}\t{anchor}",
                    case.name, key.0, key.1, key.2
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
