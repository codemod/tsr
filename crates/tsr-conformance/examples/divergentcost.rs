//! The cost of the cases neither dump scores: every case with a native
//! known-divergence baseline (`.types.diff`, `.errors.txt.diff`, `.js.diff`),
//! run under `case_guard` for wall time, peak memory and markers, with **no
//! verdict** (`tsr-2zk.1069`).
//!
//! ```text
//! cargo run --release -p tsr-conformance --example divergentcost > cost.tsv
//! cargo run --release -p tsr-conformance --example slowcases -- BASE/cost.tsv NEW/cost.tsv
//! ```
//!
//! # Why
//!
//! `verdictdump` and `diagverdictdump` skip a known-divergence case
//! (`CaseEntry::has_known_divergence`): native records that it differs from
//! TypeScript there, so its baseline is no specification. The skip also hid
//! its cost. `recursiveConditionalCrash3` does not finish in 400 s and
//! `templateLiteralTypes1` takes 26 s (`docs/parity/notes/r5-harness.md` §4),
//! and a change that made either worse, or crashed one, passed the gate.
//!
//! # What a row is
//!
//! `case<TAB>TIMED<TAB>-<TAB>-<TAB>ms=<wall><TAB>mib=<peak>`, the diagnostics
//! dump's shape, so `slowcases` folds it by case and reads the two guard
//! columns. `TIMED` is not a verdict: nothing is compared. A case that panics,
//! runs out of memory or time prints `PANIC`, `OOM` or `TIMEOUT` instead; one
//! whose process dies with no row (a stack overflow, a kernel kill) prints
//! `CRASH` with the exit status.
//!
//! The work per case is both dumps' work for it: the case's diagnostics
//! (`diagnostics_suite::reported_for`, `diagverdictdump`'s call) and, when it
//! has a `.types` baseline, its rendered type lines
//! (`types_producer::assertions_for_case`, the producer `verdictdump` runs).
//! The population is the dumps' own, with the divergence filter inverted:
//! each named configuration of a varied case, never the varied case as itself.
//!
//! # One process per case
//!
//! `case_guard`'s watchdog ends its process (exit 3) when a case passes its
//! budget, because a thread cannot be stopped. Both dumps accept that: no case
//! in them is known to hang. This population has one that is, so an
//! in-process pass would end at `recursiveConditionalCrash3` and lose every
//! row still in flight or unprinted. Each case therefore runs in a child
//! process (`divergentcost --case <name>`), which costs one lib parse per case
//! (about 15–17 ms, `docs/architecture/program.md`) over a population of
//! about 900 entries, against a gate that runs only once per commit.
//!
//! The parent runs `TSR_JOBS` children at a time (default: the available
//! parallelism). Unless set, it gives each child `TSR_CASE_TIMEOUT_S=120`,
//! twelve times the `slowcases` budget (the dumps' 600 s default would let one
//! hang cost ten minutes), and a `TSR_DUMP_MEM_MIB` share of three quarters of
//! `MemTotal`. `TSR_FILTER` (comma-separated case-name substrings) restricts
//! the run as in the dumps.
//!
//! `docs/parity/notes/r5-align.md` §5 is the integrator's recipe.

use std::io::Write as _;
use std::process::{Command, ExitCode, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use tsr_conformance::case_guard::{self, RowShape};
use tsr_conformance::corpus::CaseEntry;
use tsr_conformance::{Corpus, diagnostics_suite, repo_root, types_baseline, types_producer};

#[path = "support/counting_alloc.rs"]
mod counting_alloc;

/// Every entry the dumps skip as a known divergence, in name order.
fn population() -> Vec<CaseEntry> {
    let mut cases = Corpus::from_repo_root(&repo_root()).discover().expect("corpus");
    let configured = Corpus::configured(&cases);
    cases.extend(configured);
    cases.retain(|case| {
        case.has_known_divergence()
            && case.has_any_baseline()
            && (case.configuration.is_some() || !case.is_expanded())
    });
    cases.sort_by(|a, b| a.name.cmp(&b.name));
    cases
}

/// The one entry `name` names, without deriving the whole population: a
/// child that did re-ran every case file's configuration expansion, about
/// 7 s of CPU per child.
fn entry(name: &str) -> Option<CaseEntry> {
    let base = name.split_once('(').map_or(name, |(base, _)| base);
    let case = Corpus::from_repo_root(&repo_root())
        .discover()
        .expect("corpus")
        .into_iter()
        .find(|case| case.name == base)?;
    let case = if base == name {
        case
    } else {
        case.configured().into_iter().find(|configured| configured.name == name)?
    };
    case.has_known_divergence().then_some(case)
}

/// One case's work: what both dumps would run for it, with nothing compared.
fn work(case: &CaseEntry) {
    let Ok(test) = case.load() else { return };
    std::hint::black_box(diagnostics_suite::reported_for(&test));
    if let Some(text) = case.expected_types() {
        let expected = types_producer::expected_for_case(&text, &test);
        if types_baseline::assertion_count(&expected) > 0 {
            std::hint::black_box(types_producer::assertions_for_case(&test, &expected, false));
        }
    }
}

/// The child: run one case under the guard and print its row.
fn child(name: &str) -> ExitCode {
    case_guard::install(RowShape::Diagnostics);
    let Some(case) = entry(name) else {
        eprintln!("divergentcost: no known-divergence case {name}");
        return ExitCode::from(2);
    };
    // On the main thread, which has the platform's main stack (8 MiB on
    // Linux), the dumps' `case_guard::WORKER_STACK`.
    let measured = case_guard::run_case(&case.name, || work(&case));
    let columns = measured.columns();
    let row = match measured.value {
        Ok(()) => format!("{}\tTIMED\t-\t-\t{columns}", case.name),
        Err(message) => RowShape::Diagnostics.marker(&case.name, "PANIC", &message, &columns),
    };
    println!("{row}");
    ExitCode::SUCCESS
}

fn env_or(name: &str, default: String) -> String {
    std::env::var(name).unwrap_or(default)
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if let [flag, name] = args.as_slice()
        && flag == "--case"
    {
        return child(name);
    }
    let filter: Vec<String> = std::env::var("TSR_FILTER")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    let names: Vec<String> = population()
        .into_iter()
        .map(|case| case.name)
        .filter(|name| filter.is_empty() || filter.iter().any(|part| name.contains(part)))
        .collect();
    let jobs = std::env::var("TSR_JOBS")
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .or_else(|| std::thread::available_parallelism().ok().map(usize::from))
        .unwrap_or(1)
        .max(1);
    let total_mib = std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|text| {
            let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
            line.split_whitespace().nth(1)?.parse::<u64>().ok()
        })
        .map_or(u64::MAX, |kib| kib / 1024 / 4 * 3);
    let timeout = env_or("TSR_CASE_TIMEOUT_S", "120".to_string());
    let share = env_or("TSR_DUMP_MEM_MIB", (total_mib / jobs as u64).to_string());
    let exe = std::env::current_exe().expect("own executable");

    let next = AtomicUsize::new(0);
    let rows: Mutex<Vec<String>> = Mutex::new(Vec::with_capacity(names.len()));
    std::thread::scope(|scope| {
        for _ in 0..jobs {
            scope.spawn(|| {
                loop {
                    let index = next.fetch_add(1, Ordering::Relaxed);
                    let Some(name) = names.get(index) else { break };
                    let output = Command::new(&exe)
                        .args(["--case", name])
                        .env("TSR_CASE_TIMEOUT_S", &timeout)
                        .env("TSR_DUMP_MEM_MIB", &share)
                        .stdin(Stdio::null())
                        .stderr(Stdio::inherit())
                        .output();
                    let row = match output {
                        Ok(output) => {
                            let stdout = String::from_utf8_lossy(&output.stdout);
                            // The guard's own row, or its marker row before
                            // an exit 3 or an abort.
                            stdout
                                .lines()
                                .rev()
                                .find(|line| line.split('\t').next() == Some(name.as_str()))
                                .map_or_else(
                                    || {
                                        RowShape::Diagnostics.marker(
                                            name,
                                            "CRASH",
                                            &format!("no row; child {}", output.status),
                                            "ms=0\tmib=0",
                                        )
                                    },
                                    str::to_string,
                                )
                        }
                        Err(error) => RowShape::Diagnostics.marker(
                            name,
                            "CRASH",
                            &format!("spawning the child: {error}"),
                            "ms=0\tmib=0",
                        ),
                    };
                    rows.lock().unwrap_or_else(std::sync::PoisonError::into_inner).push(row);
                }
            });
        }
    });
    let mut rows = rows.into_inner().unwrap_or_else(std::sync::PoisonError::into_inner);
    rows.sort();
    let mut stdout = std::io::stdout().lock();
    for row in &rows {
        let _ = writeln!(stdout, "{row}");
    }
    let failed = rows.iter().filter(|row| !matches!(row.split('\t').nth(1), Some("TIMED"))).count();
    eprintln!("divergentcost: {} cases, {failed} with a marker", rows.len());
    ExitCode::SUCCESS
}
