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

use std::collections::{BTreeMap, BTreeSet, HashMap};

use rayon::prelude::{IntoParallelRefIterator, ParallelIterator};
use tsr_conformance::{
    CaseEntry, Corpus,
    errors_baseline::{self, BaselineDiagnostic},
    repo_root,
};

/// `(want, have, missing)` per code for one case. §933.
type Tallies = (BTreeMap<u32, usize>, BTreeMap<u32, usize>, BTreeMap<u32, usize>);

fn main() {
    let corpus = Corpus::from_repo_root(&repo_root());
    let cases = corpus.discover().expect("corpus");
    let rows: Vec<Tallies> = cases.par_iter().filter_map(measure).collect();

    let (mut want, mut have, mut gone): Tallies =
        (BTreeMap::new(), BTreeMap::new(), BTreeMap::new());
    for (expected, actual, absent) in &rows {
        for (code, count) in expected {
            *want.entry(*code).or_default() += count;
        }
        for (code, count) in actual {
            *have.entry(*code).or_default() += count;
        }
        for (code, count) in absent {
            *gone.entry(*code).or_default() += count;
        }
    }

    println!("{:<10} {:>8} {:>8} {:>8}   rule", "code", "want", "have", "missing");
    let mut ranked: Vec<(&u32, &usize)> = want.iter().collect();
    ranked.sort_by_key(|(_, count)| std::cmp::Reverse(**count));
    // The default keeps the historic view; a first argument widens it. §846
    // needed the whole list to sweep for `**SILENT**` rows — codes this port
    // has ported and never emits — and `.take(60)` was hiding them, which is
    // §829's rule about the width of a grep applied to an instrument.
    let rule_codes = rule_codes();
    let limit = std::env::args().nth(1).and_then(|arg| arg.parse::<usize>().ok()).unwrap_or(60);
    for (code, wanted) in ranked.iter().take(limit) {
        let emitted = have.get(code).copied().unwrap_or(0);
        let absent = gone.get(code).copied().unwrap_or(0);
        let note = if !rule_codes.contains(code) {
            "unported"
        } else if emitted == 0 {
            "**SILENT**"
        } else if emitted * 4 < **wanted {
            "quiet"
        } else {
            ""
        };
        println!("TS{code:<8} {wanted:>8} {emitted:>8} {absent:>8}   {note}");
    }
}

fn measure(case: &CaseEntry) -> Option<Tallies> {
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
    let mut missing = BTreeMap::new();
    for diagnostic in &expected {
        *want.entry(diagnostic.code).or_default() += 1;
        // **Position-aware.** `want − have` is a difference of totals and a line
        // emitted at the wrong column counts in `have` (§884); this counts the
        // wanted lines this port does not produce, which is the number
        // `diagmissing`'s label promised and did not deliver (§927, §928).
        if !actual.contains(diagnostic) {
            *missing.entry(diagnostic.code).or_default() += 1;
        }
    }
    for diagnostic in &actual {
        *have.entry(diagnostic.code).or_default() += 1;
    }
    Some((want, have, missing))
}

/// The codes this port's rules emit.
///
/// **Derived, not hand-kept.** The hand-kept list had 67 entries and was wrong for
/// The codes this port has a rule for, **derived from the source** rather than
/// listed by hand.
///
/// A code missing from this set is labelled `unported` instead of `**SILENT**`,
/// so a hand-kept list can only ever surface the silent rules someone
/// remembered to add. §878 regenerated it once and it lagged four more times —
/// §954 (2432, 2774), §956 (2303), §958 (2528) — and §958 is the reason this is
/// now computed: TS2528's rule had been ported, commented at length and never
/// fired, and the row read `unported` because nobody had added the number.
///
/// Two passes of plain text, no build step and nothing to remember:
/// `messages.rs` gives `CONSTANT → code`, and every `messages::CONSTANT`
/// mentioned under `tsr-checker` or `tsr-binder` marks that code as ported.
/// `checker-notes-diag2.md` §959.
fn rule_codes() -> BTreeSet<u32> {
    let root = repo_root();
    let source =
        std::fs::read_to_string(root.join("crates/tsr-diagnostics/src/generated/messages.rs"))
            .expect("messages.rs");
    let mut code_of: HashMap<&str, u32> = HashMap::new();
    for (index, line) in source.lines().enumerate() {
        let Some(rest) = line.strip_prefix("pub static ") else { continue };
        let Some(name) = rest.split(':').next() else { continue };
        // **Three layouts, and each one found by a row reading `unported`
        // with its rule committed and firing.** `rustfmt` puts short
        // declarations on one line, wraps the arguments of medium ones, and
        // wraps the *type* of long ones — so `Message::new(` may be on this
        // line or two below it, and the code may follow on the same line or the
        // next. Scanning forward for the call and then for the first integer
        // handles all three and is not a fourth guess about formatting. §961.
        let code = source
            .lines()
            .skip(index)
            .take(4)
            .skip_while(|line| !line.contains("Message::new("))
            .flat_map(|line| line.rsplit("Message::new(").next().unwrap_or(line).split(','))
            .find_map(|piece| piece.trim().parse::<u32>().ok());
        let Some(code) = code else { continue };
        code_of.insert(name, code);
    }
    let mut ported = BTreeSet::new();
    for crate_name in ["tsr-checker", "tsr-binder"] {
        let mut stack = vec![root.join("crates").join(crate_name).join("src")];
        while let Some(directory) = stack.pop() {
            let Ok(entries) = std::fs::read_dir(&directory) else { continue };
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    stack.push(path);
                    continue;
                }
                if path.extension().is_none_or(|extension| extension != "rs") {
                    continue;
                }
                let Ok(text) = std::fs::read_to_string(&path) else { continue };
                for reference in text.split("messages::").skip(1) {
                    // `rustfmt` may wrap after `messages::`, so skip whitespace
                    // before reading the constant. A long name is exactly the
                    // kind this derivation exists to catch. §961.
                    let name: String = reference
                        .trim_start()
                        .chars()
                        .take_while(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || *c == '_')
                        .collect();
                    if let Some(&code) = code_of.get(name.as_str()) {
                        ported.insert(code);
                    }
                }
            }
        }
    }
    ported
}
