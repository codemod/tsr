//! Opt-in fixed-plan leased-read admission witness; no production concurrency.
#[path = "read_preparation/budget.rs"]
mod budget;
#[path = "read_budget_pool/policy.rs"]
mod policy;

use budget::{Budget, Error, PreparedText};
use policy::{Failure, Report};

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").unwrap();
    }
    output
}

fn payload(index: usize, path: &str, text: &PreparedText) {
    println!("file\t{index}");
    println!("text\t{}", hex(text.text().as_bytes()));
    let arena = tsr_core::Arena::new();
    let mut nodes = tsr_ast::NodeTable::new();
    let mut map = tsr_ast::NodeMap::new();
    let parsed = tsr_parser::parse_into(
        &arena,
        arena.alloc_str(text.text()),
        tsr_parser::ParseOptions {
            script_kind: tsr_parser::ScriptKind::from_file_name(path),
            ..Default::default()
        },
        &mut nodes,
        &mut map,
    );
    let image = format!(
        "{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}\n{:?}",
        parsed.node_range,
        parsed.source_file,
        parsed.diagnostics,
        parsed.jsdoc,
        parsed.file_references,
        nodes,
        map
    );
    println!("parse\t{}", hex(image.as_bytes()));
}

fn failure(error: Failure) -> String {
    match error {
        Failure::WorkerPanic { index } => format!("worker-panic\t{index}"),
        Failure::Channel => "channel".into(),
        Failure::Read { index, error } => match error {
            Error::Budget { requested, live, limit } => {
                format!("budget\t{index}\t{requested}\t{live}\t{limit}")
            }
            Error::Io(error) => format!("io\t{index}\t{:?}", error.kind()),
            Error::Allocation(error) => format!("allocation\t{index}\t{error}"),
            Error::Overflow => format!("overflow\t{index}"),
            Error::UnsupportedCapacity { requested, actual } => {
                format!("unsupported-capacity\t{index}\t{requested}\t{actual}")
            }
        },
    }
}

fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let [manifest, mode, limit, action] = args.as_slice() else {
        return Err("usage: read_budget_pool /absolute/manifest direct|1|2|4 budget-bytes payload|drop|retain|unwind".into());
    };
    if !std::path::Path::new(manifest).is_absolute() {
        return Err("manifest must be absolute".into());
    }
    let workers = match mode.as_str() {
        "direct" => 0,
        "1" => 1,
        "2" => 2,
        "4" => 4,
        _ => return Err("worker mode must be direct, 1, 2 or 4".into()),
    };
    if !matches!(action.as_str(), "payload" | "drop" | "retain" | "unwind") {
        return Err("invalid consumer action".into());
    }
    if action == "unwind" && cfg!(panic = "abort") {
        return Err("unwind-unsupported\tpanic=abort; use the unwind test binary".into());
    }
    let paths: Vec<_> = std::fs::read_to_string(manifest)
        .map_err(|e| e.to_string())?
        .lines()
        .map(str::to_owned)
        .collect();
    if paths.iter().any(|p| !std::path::Path::new(p).is_absolute()) {
        return Err("input paths must be absolute".into());
    }
    let limit: usize = limit.parse().map_err(|_| "invalid budget")?;
    let budget = Budget::new(limit);
    let mut report = Report::default();
    let mut retained = Vec::new();
    let mut consumed = 0;
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        policy::prepare_paths(&paths, workers, &budget, &mut report, |index, path, text| {
            consumed += 1;
            match action.as_str() {
                "payload" => payload(index, path, &text),
                "retain" => retained.push(text),
                "unwind" => panic!("controlled consumer unwind"),
                _ => drop(text),
            }
        })
    }));
    eprintln!(
        "held\t{}\t{}",
        budget.stats().live,
        retained.iter().map(PreparedText::capacity).sum::<usize>()
    );
    drop(retained);
    let state = budget.stats();
    eprintln!(
        "released\t{}\t{}\t{}\t{}\t{}",
        state.live, state.peak, state.acquired, state.released, limit
    );
    // Unwind exits before Report finalization; never print incomplete counters
    // as if they described completed work.
    if result.is_ok() {
        eprintln!(
            "pool\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            report.attempts,
            report.refusals,
            report.retry_batches,
            report.retry_reads,
            report.decoded_bytes,
            report.startup_ns,
            report.wall_ns,
            report.max_batch_text_bytes,
            consumed
        );
    }
    match result {
        Ok(Ok(())) => Ok(()),
        Ok(Err(error)) => Err(failure(error)),
        Err(_) => Err("consumer-unwind".into()),
    }
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(2);
    }
}
