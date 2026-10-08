//! The cost half of the zero-loss gate (`tsr-2zk.1041`): given a base and a
//! new guarded dump, list every case that got pathologically expensive, went
//! missing, or did not finish, and exit non-zero if there is any.
//!
//! ```text
//! cargo run --release -p tsr-conformance --example slowcases -- BASE.tsv NEW.tsv
//!     [--budget-ms 10000] [--ratio 3] [--floor-ms 1000]
//!     [--budget-mib 1024] [--floor-mib 256]
//! ```
//!
//! Reads either dump: `diagverdictdump` rows (key = case) and `verdictdump`
//! rows (key = `case:file:position`, folded to the case). Per case it takes
//! the largest `ms=` and `mib=` guard column (`tsr_conformance::case_guard`).
//! A base dump without guard columns still works; only the ratio check then
//! has nothing to compare against.
//!
//! A case is reported, and fails the gate, when, in NEW:
//!
//! - `OVER_BUDGET`: wall time is above `--budget-ms` (10 s), or peak memory
//!   above `--budget-mib` (1 GiB), and the base was within that budget;
//! - `SLOWER`: wall time is above `--ratio` (3×) the base's AND above
//!   `--floor-ms` (1 s), so a 4 ms case going to 13 ms is not noise-flagged;
//!   or peak memory is above `--ratio` × the base's AND above
//!   `--floor-mib` (256 MiB);
//! - `FAILED`: its verdict is `PANIC`, `OOM` or `TIMEOUT`;
//! - `MISSING`: the base has rows for it and NEW has none. A dump the kernel
//!   killed, or the watchdog ended, is missing every case it had not printed;
//!   the key/verdict `join` of the loss check drops such cases silently,
//!   which is half of what `tsr-2zk.1041` is about.
//!
//! A case over budget in the base too is listed as `KNOWN_SLOW` and does not
//! fail the gate unless it is also `SLOWER`: at `1252ab9` four cases already
//! take 14–62 s, and a gate that fails on the base itself gates nothing.
//! Each of them has a `bd` issue (`docs/parity/notes/r5-harness.md` §1).
//!
//! Exit status: 0 when nothing fails, 1 otherwise, 2 on bad usage.
//! `docs/parity/notes/r5-harness.md` §4 is the integrator's recipe.

use std::collections::BTreeMap;
use std::process::ExitCode;

#[derive(Default, Clone, Copy)]
struct Cost {
    ms: Option<u64>,
    mib: Option<u64>,
    failed: Option<&'static str>,
}

/// The guard column tagged `tag`, searched from the right: the type-text
/// columns before it may contain anything.
fn tagged(fields: &[&str], tag: &str) -> Option<u64> {
    fields.iter().rev().take(2).find_map(|field| field.strip_prefix(tag)?.parse().ok())
}

/// The case a row belongs to. Type rows are keyed `case:file:position`; a
/// diagnostics key is the case. Case names carry `/` and `(...)`, never `:`
/// followed by a digit-or-star pair at the end, so the two shapes separate.
fn case_of(key: &str) -> &str {
    let mut parts = key.rsplitn(3, ':');
    let (Some(position), Some(file), Some(case)) = (parts.next(), parts.next(), parts.next())
    else {
        return key;
    };
    let positional = |part: &str| part == "*" || part.bytes().all(|b| b.is_ascii_digit());
    if positional(position) && positional(file) { case } else { key }
}

fn load(path: &str) -> Result<BTreeMap<String, Cost>, String> {
    let text = std::fs::read_to_string(path).map_err(|error| format!("{path}: {error}"))?;
    let mut cases: BTreeMap<String, Cost> = BTreeMap::new();
    for line in text.lines().filter(|line| !line.is_empty()) {
        let fields: Vec<&str> = line.split('\t').collect();
        let [key, verdict, ..] = fields.as_slice() else {
            return Err(format!("{path}: not a dump row: {line}"));
        };
        let cost = cases.entry(case_of(key).to_string()).or_default();
        if let Some(ms) = tagged(&fields, "ms=") {
            cost.ms = Some(cost.ms.map_or(ms, |seen| seen.max(ms)));
        }
        if let Some(mib) = tagged(&fields, "mib=") {
            cost.mib = Some(cost.mib.map_or(mib, |seen| seen.max(mib)));
        }
        cost.failed = cost.failed.or(match *verdict {
            "PANIC" => Some("PANIC"),
            "OOM" => Some("OOM"),
            "TIMEOUT" => Some("TIMEOUT"),
            _ => None,
        });
    }
    Ok(cases)
}

struct Limits {
    budget_ms: u64,
    budget_mib: u64,
    ratio: u64,
    floor_ms: u64,
    floor_mib: u64,
}

fn parse_args() -> Result<(String, String, Limits), String> {
    let mut paths = Vec::new();
    let mut limits =
        Limits { budget_ms: 10_000, budget_mib: 1024, ratio: 3, floor_ms: 1000, floor_mib: 256 };
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        let slot = match arg.as_str() {
            "--budget-ms" => &mut limits.budget_ms,
            "--budget-mib" => &mut limits.budget_mib,
            "--ratio" => &mut limits.ratio,
            "--floor-ms" => &mut limits.floor_ms,
            "--floor-mib" => &mut limits.floor_mib,
            _ if arg.starts_with("--") => return Err(format!("unknown flag {arg}")),
            _ => {
                paths.push(arg);
                continue;
            }
        };
        *slot = args
            .next()
            .and_then(|value| value.parse().ok())
            .ok_or_else(|| format!("{arg} takes a whole number"))?;
    }
    let [base, new] = <[String; 2]>::try_from(paths)
        .map_err(|_| "usage: slowcases BASE.tsv NEW.tsv [--budget-ms N] [--ratio N] [--floor-ms N] [--budget-mib N]".to_string())?;
    Ok((base, new, limits))
}

fn main() -> ExitCode {
    let (base_path, new_path, limits) = match parse_args() {
        Ok(parsed) => parsed,
        Err(message) => {
            eprintln!("slowcases: {message}");
            return ExitCode::from(2);
        }
    };
    let (base, new) = match (load(&base_path), load(&new_path)) {
        (Ok(base), Ok(new)) => (base, new),
        (Err(message), _) | (_, Err(message)) => {
            eprintln!("slowcases: {message}");
            return ExitCode::from(2);
        }
    };

    let mut reports: Vec<String> = Vec::new();
    let mut known: Vec<String> = Vec::new();
    for (case, cost) in &new {
        let was = base.get(case).copied().unwrap_or_default();
        let fmt = |value: Option<u64>| value.map_or_else(|| "-".to_string(), |v| v.to_string());
        let detail = format!(
            "{case}\tms {} -> {}\tmib {} -> {}",
            fmt(was.ms),
            fmt(cost.ms),
            fmt(was.mib),
            fmt(cost.mib)
        );
        if let Some(failed) = cost.failed {
            reports.push(format!("FAILED {failed}\t{detail}"));
            continue;
        }
        let over = |now: Option<u64>, before: Option<u64>, budget: u64| {
            let now = now.unwrap_or(0) > budget;
            (now && before.is_none_or(|before| before <= budget), now)
        };
        let grew = |now: Option<u64>, before: Option<u64>, floor: u64| {
            let now = now.unwrap_or(0);
            before.is_some_and(|before| now > floor && now > before.saturating_mul(limits.ratio))
        };
        let (new_ms, over_ms) = over(cost.ms, was.ms, limits.budget_ms);
        let (new_mib, over_mib) = over(cost.mib, was.mib, limits.budget_mib);
        if grew(cost.ms, was.ms, limits.floor_ms) || grew(cost.mib, was.mib, limits.floor_mib) {
            reports.push(format!("SLOWER\t{detail}"));
        } else if new_ms || new_mib {
            reports.push(format!("OVER_BUDGET\t{detail}"));
        } else if over_ms || over_mib {
            known.push(format!("KNOWN_SLOW\t{detail}"));
        }
    }
    let missing: Vec<&String> = base.keys().filter(|case| !new.contains_key(*case)).collect();
    for case in missing.iter().take(25) {
        reports.push(format!("MISSING\t{case}"));
    }
    if missing.len() > 25 {
        reports.push(format!("MISSING\t... and {} more", missing.len() - 25));
    }

    let guarded = new.values().filter(|cost| cost.ms.is_some()).count();
    eprintln!(
        "slowcases: base {} cases, new {} cases ({guarded} with guard columns), {} missing, \
         budget {} ms / {} MiB, ratio {}x over {} ms / {} MiB",
        base.len(),
        new.len(),
        missing.len(),
        limits.budget_ms,
        limits.budget_mib,
        limits.ratio,
        limits.floor_ms,
        limits.floor_mib
    );
    if guarded == 0 {
        eprintln!("slowcases: {new_path} has no guard columns — not a guarded dump");
        return ExitCode::from(2);
    }
    for report in known.iter().chain(&reports) {
        println!("{report}");
    }
    if reports.is_empty() { ExitCode::SUCCESS } else { ExitCode::FAILURE }
}
