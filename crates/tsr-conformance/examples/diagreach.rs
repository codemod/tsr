//! Which cases are reachable by **deepening the rules that already exist**?
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagreach
//! ```
//!
//! `diaggap.rs` ranks codes by "cases blocked on exactly one missing code", and
//! eleven builds have now shown that column to be an *ordering* rather than a
//! forecast: a case usually wants the code it is blocked on **and something
//! else** (`checker-notes-diag2.md` §53 is 71 right lines for one conversion).
//!
//! This asks the complementary question. A case is counted here when
//!
//! - it reports nothing the baseline does not, **and**
//! - every diagnostic it is missing carries a code some rule in this port
//!   already emits.
//!
//! Such a case needs **no new rule at all** — only the rules it already
//! triggers, reporting more completely. The output ranks those cases by the set
//! of codes involved, which is what says *which* rule to deepen.

use std::collections::{BTreeMap, BTreeSet};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(String, BTreeSet<u32>)> = cases.par_iter().filter_map(measure).collect();

    println!("cases reachable by deepening existing rules: {}", rows.len());

    let mut by_set: BTreeMap<Vec<u32>, Vec<&str>> = BTreeMap::new();
    for (name, codes) in &rows {
        by_set.entry(codes.iter().copied().collect()).or_default().push(name.as_str());
    }
    let mut sets: Vec<(&Vec<u32>, &Vec<&str>)> = by_set.iter().collect();
    sets.sort_by_key(|(_, names)| std::cmp::Reverse(names.len()));

    println!("\n== by the SET of codes the case is missing ==");
    for (codes, names) in sets.iter().take(30) {
        let printed: Vec<String> = codes.iter().map(|code| format!("TS{code}")).collect();
        println!("{:4}  {}", names.len(), printed.join(" + "));
        for name in names.iter().take(3) {
            println!("        {name}");
        }
    }

    // The split the eleventh session's closing question needs: of the cases
    // reachable by deepening existing rules, how many want **only** codes whose
    // rule is bound by the structural relation or the members table — the
    // subsystem `STATUS.md` §5 refuses — and how many want none of them?
    let (mut relation_only, mut mixed, mut relation_free) = (0usize, 0usize, 0usize);
    for (_, codes) in &rows {
        let bound = codes.iter().filter(|code| RELATION_BOUND.contains(code)).count();
        if bound == 0 {
            relation_free += 1;
        } else if bound == codes.len() {
            relation_only += 1;
        } else {
            mixed += 1;
        }
    }
    println!("\n== reachable cases, split by whether the RELATION/MEMBERS subsystem is needed ==");
    println!("wants only relation-bound codes : {relation_only}");
    println!("wants a mix                     : {mixed}");
    println!("wants NO relation-bound code    : {relation_free}");

    let mut per_code: BTreeMap<u32, usize> = BTreeMap::new();
    for (_, codes) in &rows {
        for code in codes {
            *per_code.entry(*code).or_default() += 1;
        }
    }
    let mut counted: Vec<(u32, usize)> = per_code.into_iter().collect();
    counted.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    println!("\n== the same cases, per code: which RULE to deepen ==");
    for (code, count) in counted.iter().take(30) {
        println!("TS{code:<8} {count}");
    }
}

/// `Some` when every missing diagnostic is a code this port already emits and
/// nothing extra is reported.
fn measure(case: &CaseEntry) -> Option<(String, BTreeSet<u32>)> {
    if case.has_varied_errors() || case.has_known_divergence() || !case.has_any_baseline() {
        return None;
    }
    let baseline = case.expected_errors().ok()?;
    let expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return None;
    }
    let test = case.load().ok()?;
    let actual = tsr_conformance::diagnostics_suite::reported_for(&test);
    if actual.iter().any(|diagnostic| !expected.contains(diagnostic)) {
        return None;
    }
    let missing: BTreeSet<u32> = expected
        .iter()
        .filter(|diagnostic| !actual.contains(diagnostic))
        .map(|diagnostic| diagnostic.code)
        .collect();
    if missing.is_empty() || !missing.iter().all(|code| RULE_CODES.contains(code)) {
        return None;
    }
    Some((case.name.clone(), missing))
}

/// The codes this port's rules emit — kept in step with `diag2307.rs`.
const RULE_CODES: &[u32] = &[
    2307, 2882, 2564, 2304, 2454, 2369, 2695, 1104, 1105, 1107, 1115, 1116, 1036, 1183, 2384, 2389,
    2390, 2391, 2392, 2393, 6133, 6138, 6192, 6196, 6198, 6199, 6205, 2322, 7006, 7019, 2314, 2707,
    2554, 2555, 2339, 2741, 2353, 2345, 2411, 2415, 2416, 2420, 2430, 2583, 2301, 2352, 18050,
    2552, 2367, 2872, 2873, 1345, 2365, 18047, 18048, 18049, 2531, 2532, 2533, 2464, 2540, 2362,
    2363, 2356, 2341, 2445, 2374,
];

/// The codes whose rule is gated on the structural relation or on a resolved
/// members table — the subsystem `STATUS.md` §5 refuses. Everything else in
/// [`RULE_CODES`] answers from scope, flow, flags or syntax.
const RELATION_BOUND: &[u32] =
    &[2322, 2345, 2339, 2741, 2353, 2352, 2416, 2430, 2420, 2415, 2403, 2411];
