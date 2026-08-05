//! Where the `checker_types` gradient is lost, bucketed by **upstream's answer
//! shape** — `bd tsr-4sc.6`.
//!
//! `cargo run -p tsr-conformance --example types_shapes --release`
//!
//! # The question
//!
//! We nominally cover the two largest buckets — intrinsic (35.30%) and literal
//! (18.86%) — and the gradient reads 22.39%. Under half of what we supposedly
//! support, and the case rate cannot say why. This is the histogram that says
//! why, and it decides whether the next investment is lib files, object types or
//! narrowing (`docs/architecture/checker.md`).
//!
//! # Only aligned lines are attributed
//!
//! A `.types` line is `>{expression} : {type}` and **cannot be split**, because
//! the expression may contain `" : "` too. But where our walker produced the
//! same expression text at the same position, upstream's line starts with
//! `{our text} : ` and the remainder is *exactly* upstream's type — no split
//! needed, no ambiguity. So every type comparison here is over lines the walker
//! aligned (97.85% of them, `docs/architecture/checker-oracle.md`), and the rest
//! are reported as their own bucket rather than guessed at. A line the walker
//! lost is not evidence about the checker.

use std::collections::HashMap;

use rayon::prelude::*;
use tsr_conformance::{
    Corpus, repo_root,
    type_shape::{self, Shape},
    types_baseline, types_producer,
};

/// How many exact `expected -> ours` pairs to keep. Bounded because the "ours"
/// side is unbounded — a literal type is any string in the corpus.
const PAIR_LIMIT: usize = 4000;
/// How many worked examples to print per bucket.
const SAMPLES: usize = 6;

#[derive(Default)]
struct Tally {
    cases: usize,
    /// Lines upstream wrote.
    expected: usize,
    /// Lines where our expression text matched, so the type is comparable.
    aligned: usize,
    /// Aligned lines, by upstream's answer shape.
    lines: HashMap<Shape, usize>,
    /// Of those, the ones whose type matched exactly.
    matched: HashMap<Shape, usize>,
    /// Mismatches, by (upstream's shape, the shape *we* answered with).
    confusion: HashMap<(Shape, Shape), usize>,
    /// Mismatches where we answered `error` — an unported form, not a claim.
    gaps: HashMap<Shape, usize>,
    /// `error` answers by (upstream's shape, the node kind we failed on). This
    /// is the dimension the shape table cannot supply: a bucket names the answer
    /// and this names the construct, which is what ranks the work.
    gap_kinds: HashMap<(Shape, String), usize>,
    /// `error` answers by (upstream's shape, why the checker stopped).
    gap_reasons: HashMap<(Shape, String), usize>,
    /// Cases whose baseline has more than one file section, and what they
    /// carry. Each file is parsed, bound and checked **on its own** — there is
    /// no program (`bd tsr-9or.1`) — so a name declared in one file of a case
    /// cannot resolve from another. This measures how much of the corpus that
    /// costs, which no other row can say.
    multi_file_cases: usize,
    multi_file_aligned: usize,
    multi_file_right: usize,
    multi_file_gaps: usize,
    /// Names in *type* position that did not resolve, by the name itself. The
    /// same question as [`Tally::unresolved`], asked about annotations, and the
    /// one that ranks lib files: `Array` and `Promise` are lib types, `Foo` is
    /// someone's own.
    unresolved_types: HashMap<String, usize>,
    /// Names that did not resolve at all, by the name itself. This is the direct
    /// test of the lib-file hypothesis (`bd tsr-9or.1`): if the misses are
    /// `Array`, `console`, `Math`, they are globals we have no declarations for.
    unresolved: HashMap<String, usize>,
    /// Mismatches where we answered something other than `error` — a wrong
    /// claim, and therefore a defect in what is already ported.
    wrong_kinds: HashMap<(Shape, String), usize>,
    /// Mismatches as exact strings, for the buckets we claim to cover.
    pairs: HashMap<(String, String), usize>,
    /// `(case, expression, upstream type, our type)`, per upstream shape.
    samples: HashMap<Shape, Vec<(String, String, String, String)>>,
}

impl Tally {
    fn merge(&mut self, other: Tally) {
        self.cases += other.cases;
        self.expected += other.expected;
        self.aligned += other.aligned;
        for (key, count) in other.lines {
            *self.lines.entry(key).or_default() += count;
        }
        for (key, count) in other.matched {
            *self.matched.entry(key).or_default() += count;
        }
        for (key, count) in other.confusion {
            *self.confusion.entry(key).or_default() += count;
        }
        for (key, count) in other.gaps {
            *self.gaps.entry(key).or_default() += count;
        }
        for (key, count) in other.gap_kinds {
            *self.gap_kinds.entry(key).or_default() += count;
        }
        for (key, count) in other.gap_reasons {
            *self.gap_reasons.entry(key).or_default() += count;
        }
        for (key, count) in other.unresolved {
            *self.unresolved.entry(key).or_default() += count;
        }
        self.multi_file_cases += other.multi_file_cases;
        self.multi_file_aligned += other.multi_file_aligned;
        self.multi_file_right += other.multi_file_right;
        self.multi_file_gaps += other.multi_file_gaps;
        for (key, count) in other.unresolved_types {
            *self.unresolved_types.entry(key).or_default() += count;
        }
        for (key, count) in other.wrong_kinds {
            *self.wrong_kinds.entry(key).or_default() += count;
        }
        for (key, count) in other.pairs {
            // Beyond the limit, established keys still accumulate; new ones are
            // dropped. The report prints the limit so a truncated tail is
            // visible rather than implied.
            if self.pairs.len() < PAIR_LIMIT || self.pairs.contains_key(&key) {
                *self.pairs.entry(key).or_default() += count;
            }
        }
        for (shape, examples) in other.samples {
            let slot = self.samples.entry(shape).or_default();
            for example in examples {
                if slot.len() < SAMPLES {
                    slot.push(example);
                }
            }
        }
    }
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    assert!(corpus.is_available(), "corpus missing; run git submodule update --init --recursive");
    let cases = corpus.discover().expect("discovering cases");

    let tallies: Vec<Tally> = cases
        .par_iter()
        .map(|case| {
            let mut tally = Tally::default();
            // The same population the suite judges, skip for skip, so the shares
            // here are shares of the gradient's own denominator.
            if case.has_varied_types() || case.has_known_divergence() {
                return tally;
            }
            let Some(text) = case.expected_types() else { return tally };
            let expected = types_baseline::parse(&text);
            if types_baseline::assertion_count(&expected) == 0 {
                return tally;
            }
            let Ok(parsed) = case.load() else { return tally };
            let ours = types_producer::assertions_for_case(&parsed, &expected, true);
            tally.cases = 1;
            let multi_file = expected.len() > 1;
            tally.multi_file_cases = usize::from(multi_file);

            for (index, expected_file) in expected.iter().enumerate() {
                let our_file = ours.get(index);
                for (position, want) in expected_file.assertions.iter().enumerate() {
                    tally.expected += 1;
                    let Some(got) = our_file.and_then(|file| file.get(position)) else { continue };
                    // The alignment test *is* the split: upstream's line starts
                    // with our expression text, so what follows is its type.
                    let Some(want_type) = want.text.strip_prefix(&format!("{} : ", got.text))
                    else {
                        continue;
                    };
                    tally.aligned += 1;
                    if multi_file {
                        tally.multi_file_aligned += 1;
                        if want_type == got.type_string {
                            tally.multi_file_right += 1;
                        } else if got.type_string == "error" {
                            tally.multi_file_gaps += 1;
                        }
                    }
                    let shape = type_shape::classify(want_type);
                    *tally.lines.entry(shape).or_default() += 1;
                    if want_type == got.type_string {
                        *tally.matched.entry(shape).or_default() += 1;
                        continue;
                    }
                    *tally
                        .confusion
                        .entry((shape, type_shape::classify(&got.type_string)))
                        .or_default() += 1;
                    // `error` is this port's marker for an unported form and
                    // never a claim that the answer *is* `error`
                    // (docs/architecture/checker.md). Separating it from a wrong
                    // answer is the difference between missing work and a bug.
                    let kind = format!("{:?}", got.kind);
                    if got.type_string == "error" {
                        *tally.gaps.entry(shape).or_default() += 1;
                        *tally.gap_kinds.entry((shape, kind)).or_default() += 1;
                        let mut reason = got.reason.clone().unwrap_or_else(|| "?".to_string());
                        // The unresolved *type* name rides along in the reason
                        // so that it can be counted separately without a second
                        // pass over the corpus; the reason itself is normalised
                        // back so the roll-up stays readable.
                        if let Some(cut) = reason.find("TypeReference unresolved") {
                            let tail = &reason[cut..];
                            if let Some((head, name)) = tail.split_once(": ") {
                                *tally.unresolved_types.entry(name.to_string()).or_default() += 1;
                                reason = format!("{}{head}", &reason[..cut]);
                            }
                        }
                        if reason == "reference, the name does not resolve" {
                            *tally.unresolved.entry(got.text.clone()).or_default() += 1;
                        }
                        *tally.gap_reasons.entry((shape, reason)).or_default() += 1;
                    } else {
                        *tally.wrong_kinds.entry((shape, kind)).or_default() += 1;
                    }
                    if matches!(shape, Shape::Intrinsic | Shape::Literal)
                        && tally.pairs.len() < PAIR_LIMIT
                    {
                        *tally
                            .pairs
                            .entry((want_type.to_string(), got.type_string.clone()))
                            .or_default() += 1;
                    }
                    let slot = tally.samples.entry(shape).or_default();
                    if slot.len() < SAMPLES {
                        slot.push((
                            case.name.clone(),
                            got.text.clone(),
                            want_type.to_string(),
                            got.type_string.clone(),
                        ));
                    }
                }
            }
            tally
        })
        .collect();

    let mut total = Tally::default();
    for tally in tallies {
        total.merge(tally);
    }
    report(&total);
}

#[allow(clippy::cast_precision_loss)]
fn pct(n: usize, d: usize) -> f64 {
    if d == 0 { 0.0 } else { n as f64 / d as f64 * 100.0 }
}

/// The commonest node kinds in one of the kind maps, optionally for one shape.
fn print_kinds(kinds: &HashMap<(Shape, String), usize>, shape: Option<Shape>, take: usize) {
    let mut rolled: HashMap<&str, usize> = HashMap::new();
    for ((bucket, kind), count) in kinds {
        if shape.is_none_or(|wanted| wanted == *bucket) {
            *rolled.entry(kind.as_str()).or_default() += count;
        }
    }
    let total: usize = rolled.values().sum();
    let mut rows: Vec<_> = rolled.into_iter().map(|(kind, count)| (count, kind)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, kind) in rows.iter().take(take) {
        println!("  {kind:<32} {count:>9}  {:>6.2}%", pct(*count, total));
    }
}

fn report(total: &Tally) {
    println!("cases judged:             {}", total.cases);
    println!("assertion lines upstream: {}", total.expected);
    println!(
        "  the walker aligned:     {} ({:.2}%) — the only lines a type can be compared on",
        total.aligned,
        pct(total.aligned, total.expected)
    );
    println!(
        "  unaligned:              {} ({:.2}%) — not evidence about the checker",
        total.expected - total.aligned,
        pct(total.expected - total.aligned, total.expected)
    );

    let matched: usize = total.matched.values().sum();
    println!(
        "\ntypes right, over aligned lines: {}/{} ({:.2}%)",
        matched,
        total.aligned,
        pct(matched, total.aligned)
    );

    println!("\nupstream's answer shape, over aligned lines:");
    println!(
        "{:<20} {:>9} {:>8} {:>9} {:>8} {:>9} {:>8}",
        "shape", "lines", "share", "right", "rate", "gap", "wrong"
    );
    for shape in Shape::all() {
        let lines = total.lines.get(&shape).copied().unwrap_or_default();
        if lines == 0 {
            continue;
        }
        let right = total.matched.get(&shape).copied().unwrap_or_default();
        let gap = total.gaps.get(&shape).copied().unwrap_or_default();
        println!(
            "{:<20} {:>9} {:>7.2}% {:>9} {:>7.2}% {:>9} {:>8}",
            shape.label(),
            lines,
            pct(lines, total.aligned),
            right,
            pct(right, lines),
            gap,
            lines - right - gap
        );
    }
    println!(
        "  gap   = we answered `error`: the form is not ported, and it is not a claim\n  wrong = we answered something else: a defect in what is ported"
    );

    println!("\nnode kinds we answered `error` on, where upstream answered intrinsic:");
    print_kinds(&total.gap_kinds, Some(Shape::Intrinsic), 15);
    println!("\nnode kinds we answered `error` on, all shapes:");
    print_kinds(&total.gap_kinds, None, 15);
    println!("\nnode kinds we answered *wrongly* on — ported and defective, all shapes:");
    print_kinds(&total.wrong_kinds, None, 15);
    println!("\nwhere the checker stopped, rolled up:");
    let mut rolled: HashMap<&str, usize> = HashMap::new();
    for ((_, reason), count) in &total.gap_reasons {
        // Categories, in the order they are tested. Each names the *work* that
        // would close it, which the raw table below cannot do on its own.
        let category = if reason.contains("annotation") {
            "a type node we cannot resolve (getTypeFromTypeNode / declared types)"
        } else if reason.contains("initialiser") {
            "an initialiser expression we do not compute"
        } else if reason.starts_with("expression answered error") {
            "an expression we do not compute (unported form, or an error operand)"
        } else if reason.contains("the name of a") {
            "a member name, resolved as if it were free (bd tsr-tl8)"
        } else if reason.contains("does not resolve") {
            "a free name that does not resolve (lib files, bd tsr-9or.1)"
        } else if reason.contains("neither") {
            "a symbol whose kind getTypeOfSymbol does not handle"
        } else {
            "other"
        };
        *rolled.entry(category).or_default() += count;
    }
    let gap_total: usize = total.gaps.values().sum();
    let mut rows: Vec<_> = rolled.into_iter().map(|(what, count)| (count, what)).collect();
    rows.sort_unstable_by(|a, b| b.cmp(a));
    for (count, what) in rows {
        println!("  {count:>9}  {:>6.2}%  {what}", pct(count, gap_total));
    }

    println!(
        "\nmulti-file cases: {} of {} ({:.2}%), carrying {} aligned lines ({:.2}%)",
        total.multi_file_cases,
        total.cases,
        pct(total.multi_file_cases, total.cases),
        total.multi_file_aligned,
        pct(total.multi_file_aligned, total.aligned)
    );
    println!(
        "  right in them: {}/{} ({:.2}%) against {:.2}% over all aligned lines; gaps {}",
        total.multi_file_right,
        total.multi_file_aligned,
        pct(total.multi_file_right, total.multi_file_aligned),
        pct(total.matched.values().sum::<usize>(), total.aligned),
        total.multi_file_gaps
    );

    println!("\nthe names in TYPE position that do not resolve, commonest first:");
    let mut type_names: Vec<_> =
        total.unresolved_types.iter().map(|(name, count)| (*count, name.clone())).collect();
    type_names.sort_unstable_by(|a, b| b.cmp(a));
    let unresolved_type_lines: usize = type_names.iter().map(|(count, _)| count).sum();
    println!("  {unresolved_type_lines} lines over {} distinct names", type_names.len());
    for (count, name) in type_names.iter().take(20) {
        println!("  {name:<24} {count:>8}");
    }

    println!("\nthe names in VALUE position that do not resolve, commonest first:");
    let mut names: Vec<_> =
        total.unresolved.iter().map(|(name, count)| (*count, name.clone())).collect();
    names.sort_unstable_by(|a, b| b.cmp(a));
    for (count, name) in names.iter().take(20) {
        println!("  {name:<24} {count:>8}");
    }

    println!("\nwhere the checker stopped, on every `error` line:");
    print_kinds(&total.gap_reasons, None, 400);
    println!("\nwhere it stopped, where upstream answered intrinsic:");
    print_kinds(&total.gap_reasons, Some(Shape::Intrinsic), 12);

    for shape in [Shape::Intrinsic, Shape::Literal] {
        let lines = total.lines.get(&shape).copied().unwrap_or_default();
        let wrong = lines - total.matched.get(&shape).copied().unwrap_or_default();
        println!(
            "\nwhere upstream answered {} and we did not ({wrong} lines), what we said:",
            shape.label()
        );
        let mut by_shape: Vec<_> = total
            .confusion
            .iter()
            .filter(|((upstream, _), _)| *upstream == shape)
            .map(|((_, ours), count)| (*count, *ours))
            .collect();
        by_shape.sort_unstable_by(|a, b| b.cmp(a));
        for (count, ours) in by_shape {
            println!(
                "  {:<20} {:>8}  ({:.2}% of the bucket's misses)",
                ours.label(),
                count,
                pct(count, wrong)
            );
        }
        let mut pairs: Vec<_> = total
            .pairs
            .iter()
            .filter(|((want, _), _)| type_shape::classify(want) == shape)
            .map(|((want, got), count)| (*count, want.clone(), got.clone()))
            .collect();
        pairs.sort_unstable_by(|a, b| b.cmp(a));
        println!("  commonest exact substitutions (of {} distinct pairs kept):", pairs.len());
        for (count, want, got) in pairs.iter().take(12) {
            println!("    {want:>28}  ->  {got:<28} {count:>8}");
        }
    }

    println!("\nfirst mismatch sampled per shape:");
    for shape in Shape::all() {
        let Some(examples) = total.samples.get(&shape) else { continue };
        println!("  [{}]", shape.label());
        for (case, expression, want, got) in examples {
            println!("    {case}: >{expression}\n      upstream: {want}\n      ours:     {got}");
        }
    }
}
