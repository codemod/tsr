//! How many diagnostics of each code this port **emits**, beside how many the
//! baselines record.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example diagemit
//! ```
//!
//! `diagmissing.rs` says which lines a rule is short; this says whether the
//! rule speaks at all. A code with a large `want` and a `have` of zero or
//! near-zero is a rule that is **not running** rather than one that is
//! declining — `checker-notes-diag2.md` §64 found one of those by hand
//! (`Binder::symbol_of` answers `None` for a `Constructor`, which returned
//! `checkFunctionOrConstructorSymbol` on its first line and silenced four codes
//! at once), and this is the instrument that would have found it in one run.

use std::collections::BTreeMap;

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<(BTreeMap<u32, usize>, BTreeMap<u32, usize>)> =
        cases.par_iter().filter_map(measure).collect();

    let (mut want, mut have): (BTreeMap<u32, usize>, BTreeMap<u32, usize>) =
        (BTreeMap::new(), BTreeMap::new());
    for (expected, actual) in &rows {
        for (code, count) in expected {
            *want.entry(*code).or_default() += count;
        }
        for (code, count) in actual {
            *have.entry(*code).or_default() += count;
        }
    }

    println!("{:<10} {:>8} {:>8}   rule", "code", "want", "have");
    let mut ranked: Vec<(&u32, &usize)> = want.iter().collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    // The default keeps the historic view; a first argument widens it. §846
    // needed the whole list to sweep for `**SILENT**` rows — codes this port
    // has ported and never emits — and `.take(60)` was hiding them, which is
    // §829's rule about the width of a grep applied to an instrument.
    let limit = std::env::args().nth(1).and_then(|arg| arg.parse::<usize>().ok()).unwrap_or(60);
    for (code, wanted) in ranked.iter().take(limit) {
        let emitted = have.get(code).copied().unwrap_or(0);
        let note = if !RULE_CODES.contains(code) {
            "unported"
        } else if emitted == 0 {
            "**SILENT**"
        } else if emitted * 4 < **wanted {
            "quiet"
        } else {
            ""
        };
        println!("TS{code:<8} {wanted:>8} {emitted:>8}   {note}");
    }
}

fn measure(case: &CaseEntry) -> Option<(BTreeMap<u32, usize>, BTreeMap<u32, usize>)> {
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
    let mut want = BTreeMap::new();
    let mut have = BTreeMap::new();
    for diagnostic in &expected {
        *want.entry(diagnostic.code).or_default() += 1;
    }
    for diagnostic in &actual {
        *have.entry(diagnostic.code).or_default() += 1;
    }
    Some((want, have))
}

/// The codes this port's rules emit.
///
/// **Derived, not hand-kept.** The hand-kept list had 67 entries and was wrong for
/// codes this port emits hundreds of times — TS1005 (877), TS2300 (495), TS1109
/// (334) — which mattered because a code missing from it is labelled `unported`
/// rather than `**SILENT**`, so §846's silent-rule sweep could only ever see the
/// rows someone had remembered to add. Regenerated at §878 from every
/// `messages::CONSTANT` referenced in `tsr-checker` and `tsr-binder`, mapped to its
/// code in `messages.rs`: 282 constants, 232 codes.
const RULE_CODES: &[u32] = &[
    1014, 1015, 1016, 1021, 1028, 1029, 1030, 1031, 1035, 1036, 1038, 1039, 1040, 1042, 1044, 1046,
    1047, 1048, 1049, 1051, 1053, 1054, 1063, 1070, 1071, 1089, 1090, 1092, 1093, 1100, 1102, 1103,
    1107, 1108, 1114, 1120, 1141, 1155, 1156, 1163, 1169, 1170, 1172, 1173, 1174, 1175, 1176, 1182,
    1183, 1184, 1186, 1187, 1191, 1192, 1194, 1203, 1206, 1210, 1212, 1213, 1214, 1215, 1221, 1222,
    1243, 1244, 1248, 1253, 1254, 1268, 1308, 1317, 1319, 1323, 1344, 1345, 1492, 2300, 2301, 2302,
    2304, 2305, 2306, 2307, 2310, 2313, 2314, 2315, 2320, 2322, 2331, 2335, 2337, 2339, 2341, 2345,
    2347, 2348, 2349, 2351, 2352, 2357, 2358, 2362, 2363, 2364, 2365, 2368, 2374, 2376, 2377, 2378,
    2384, 2389, 2390, 2392, 2393, 2397, 2403, 2408, 2411, 2414, 2415, 2417, 2420, 2427, 2428, 2430,
    2433, 2434, 2437, 2438, 2440, 2448, 2449, 2450, 2451, 2452, 2454, 2457, 2459, 2462, 2465, 2466,
    2474, 2481, 2484, 2503, 2507, 2516, 2524, 2528, 2531, 2532, 2533, 2539, 2540, 2551, 2552, 2554,
    2555, 2558, 2576, 2583, 2584, 2585, 2588, 2591, 2592, 2593, 2610, 2628, 2629, 2630, 2631, 2632,
    2664, 2669, 2678, 2686, 2693, 2694, 2695, 2703, 2707, 2708, 2709, 2724, 2729, 2741, 2790, 2840,
    2863, 2864, 2868, 2872, 2873, 2874, 6133, 6138, 6142, 6192, 6196, 6198, 6199, 6205, 7006, 7008,
    7013, 7019, 7027, 7031, 7041, 8002, 8004, 8005, 8006, 8008, 8009, 8010, 8013, 8016, 18006,
    18014, 18016, 18041, 18047, 18048, 18049, 18050, 18058, 18059,
];
