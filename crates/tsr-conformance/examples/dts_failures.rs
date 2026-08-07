//! Classify every declaration-emit failure without changing suite semantics.
//!
//! The committed snapshots intentionally cap failure details at 100 entries. That
//! keeps reviews readable, but it makes them unsuitable for choosing declaration
//! work by frequency. This diagnostic runs the real `dts_emit` and `dts_shape`
//! suites over the whole corpus and groups every non-pass into stable buckets.

use std::collections::BTreeMap;

use tsr_conformance::{
    Corpus, Outcome, Suite, dts_emit_suite::DtsEmit, dts_shape_suite::DtsShape,
    dts_target_suite::DtsReachableTarget, repo_root,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Bucket {
    WrongDeclarationShape,
    MissingDeclaration,
    ExtraDeclarationOrOutput,
    ExactTextOnlyDifference,
    SourceParseSkip,
    BaselineReparseSkip,
    UnsupportedPrinterKind,
    EmittedDeclarationDoesNotReparse,
    OtherSkip,
    OtherFailure,
}

impl Bucket {
    const fn label(self) -> &'static str {
        match self {
            Self::WrongDeclarationShape => "wrong declaration kind/name/order",
            Self::MissingDeclaration => "missing declaration",
            Self::ExtraDeclarationOrOutput => "extra declaration or output file",
            Self::ExactTextOnlyDifference => "exact-text-only difference",
            Self::SourceParseSkip => "source parse skip",
            Self::BaselineReparseSkip => "emitted-baseline reparse skip",
            Self::UnsupportedPrinterKind => "unsupported printer kind",
            Self::EmittedDeclarationDoesNotReparse => "emitted declaration does not reparse",
            Self::OtherSkip => "other skip",
            Self::OtherFailure => "other failure",
        }
    }
}

#[derive(Default)]
struct Count {
    total: usize,
    samples: Vec<String>,
}

fn shape_bucket(outcome: &Outcome) -> Option<Bucket> {
    match outcome {
        Outcome::Passed => None,
        Outcome::Unsupported { reason } if reason.starts_with("printer: ") => {
            Some(Bucket::UnsupportedPrinterKind)
        }
        Outcome::Skipped { reason } if reason == "a unit of this case does not parse cleanly" => {
            Some(Bucket::SourceParseSkip)
        }
        Outcome::Skipped { reason } if reason == "upstream's .d.ts baseline does not reparse" => {
            Some(Bucket::BaselineReparseSkip)
        }
        Outcome::Skipped { reason } if is_population_exclusion(reason) => None,
        Outcome::Skipped { .. } => Some(Bucket::OtherSkip),
        Outcome::Failed { reason } if reason.contains(": missing `") => {
            Some(Bucket::MissingDeclaration)
        }
        Outcome::Failed { reason }
            if reason.contains(": extra `")
                || (reason.starts_with("emitted ")
                    && reason.ends_with("upstream emitted no declaration file")) =>
        {
            Some(Bucket::ExtraDeclarationOrOutput)
        }
        Outcome::Failed { reason } if reason.contains(": want `") => {
            Some(Bucket::WrongDeclarationShape)
        }
        Outcome::Failed { reason } if reason == "the emitted .d.ts does not reparse" => {
            Some(Bucket::EmittedDeclarationDoesNotReparse)
        }
        Outcome::Unsupported { .. } | Outcome::Failed { .. } => Some(Bucket::OtherFailure),
    }
}

fn emit_bucket(emit: &Outcome, shape: &Outcome) -> Option<Bucket> {
    match emit {
        Outcome::Passed => None,
        Outcome::Unsupported { reason } if reason.starts_with("printer: ") => {
            Some(Bucket::UnsupportedPrinterKind)
        }
        Outcome::Unsupported { .. } => Some(Bucket::OtherFailure),
        Outcome::Skipped { reason } if reason == "a unit of this case does not parse cleanly" => {
            Some(Bucket::SourceParseSkip)
        }
        Outcome::Skipped { reason } if reason.contains("needs inference") => None,
        Outcome::Skipped { reason } if is_population_exclusion(reason) => None,
        Outcome::Skipped { .. } => Some(Bucket::OtherSkip),
        Outcome::Failed { reason }
            if reason.starts_with("emitted ")
                && reason.ends_with("upstream emitted no declaration file") =>
        {
            Some(Bucket::ExtraDeclarationOrOutput)
        }
        Outcome::Failed { .. } if matches!(shape, Outcome::Passed) => {
            Some(Bucket::ExactTextOnlyDifference)
        }
        Outcome::Failed { .. } => match shape_bucket(shape) {
            Some(Bucket::BaselineReparseSkip | Bucket::SourceParseSkip | Bucket::OtherSkip)
            | None => Some(Bucket::OtherFailure),
            bucket => bucket,
        },
    }
}

fn is_population_exclusion(reason: &str) -> bool {
    matches!(
        reason,
        "upstream recorded no output for this case"
            | "upstream recorded no .js emit baseline"
            | "the emit baseline has no emitted .d.ts section"
    )
}

fn record(
    counts: &mut BTreeMap<(&'static str, Bucket), Count>,
    suite: &'static str,
    bucket: Bucket,
    case: &str,
    outcome: &Outcome,
) {
    let count = counts.entry((suite, bucket)).or_default();
    count.total += 1;
    if count.samples.len() < 5 {
        count.samples.push(format!("{case}: {}", reason(outcome)));
    }
}

fn reason(outcome: &Outcome) -> &str {
    match outcome {
        Outcome::Passed => "passed",
        Outcome::Failed { reason }
        | Outcome::Unsupported { reason }
        | Outcome::Skipped { reason } => reason,
    }
}

fn main() {
    let root = repo_root();
    let corpus = Corpus::from_repo_root(&root);
    let mut cases = corpus.discover().expect("discover corpus");
    let args: Vec<String> = std::env::args().skip(1).collect();
    let show_cases = args.iter().any(|arg| arg == "--cases");
    let show_target = args.iter().any(|arg| arg == "--target");
    if let Some(filter) = args.iter().find(|arg| *arg != "--cases" && *arg != "--target") {
        cases.retain(|case| case.name.contains(filter.as_str()));
    }

    if show_target {
        let target = DtsReachableTarget;
        for case in &cases {
            let outcome = target.run(case);
            if !matches!(outcome, Outcome::Skipped { .. }) {
                println!("{}\t{}", case.name, reason(&outcome));
            }
        }
        return;
    }

    let emit = DtsEmit;
    let shape = DtsShape;
    let mut counts = BTreeMap::new();
    let mut rows = Vec::new();

    for case in &cases {
        let shape_outcome = shape.run(case);
        let emit_outcome = emit.run(case);
        if let Some(bucket) = shape_bucket(&shape_outcome) {
            record(&mut counts, "dts_shape", bucket, &case.name, &shape_outcome);
            rows.push(("dts_shape", bucket, case.name.clone(), reason(&shape_outcome).to_string()));
        }
        if let Some(bucket) = emit_bucket(&emit_outcome, &shape_outcome) {
            record(&mut counts, "dts_emit", bucket, &case.name, &emit_outcome);
            rows.push(("dts_emit", bucket, case.name.clone(), reason(&emit_outcome).to_string()));
        }
    }

    if show_cases {
        for (suite, bucket, case, outcome_reason) in rows {
            println!("{suite}\t{}\t{case}\t{outcome_reason}", bucket.label());
        }
        return;
    }

    for suite in ["dts_shape", "dts_emit"] {
        println!("{suite}");
        let mut suite_counts: Vec<_> =
            counts.iter().filter(|((name, _), _)| *name == suite).collect();
        suite_counts.sort_by_key(|((_, bucket), count)| (std::cmp::Reverse(count.total), *bucket));
        for ((_, bucket), count) in suite_counts {
            println!("{:5}  {}", count.total, bucket.label());
            for sample in &count.samples {
                println!("       {sample}");
            }
        }
        println!();
    }
}
