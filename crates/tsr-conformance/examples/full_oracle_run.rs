//! Exact full-corpus oracle runner and transition gate.
//!
//! ```text
//! full_oracle_run run REPORT_DIR [--workers N] [--deadline SECONDS] [--filter SUBSTRING] [--allow-dirty]
//! full_oracle_run gate BASE_REPORT CANDIDATE_REPORT
//! ```
//!
//! `run` binds the source commit, the pinned native and corpus revisions and both
//! worker binaries' SHA-256 into `identity.tsv`, builds the native producer through
//! a `go test -overlay` of the pinned checkout, discovers the population with the
//! native runner (a failed or timed-out discovery aborts the run; nothing resumes
//! from a partial plan), then runs every configured case in two bounded processes.
//! Every non-excluded row is in the denominator: failures, timeouts and discovery
//! failures count as non-exact. See `docs/parity/notes/oracle.md`.
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
use tsr_conformance::full_oracle::{
    self as oracle, NATIVE, ProcessOutcome, Stream, readable_key, sha256, unhex, write_atomic,
};

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("run") => run(&args[1..]),
        Some("summarize") => {
            ensure!(args.len() == 2, "usage: full_oracle_run summarize REPORT");
            print!("{}", summarize(Path::new(&args[1]))?);
            Ok(())
        }
        Some("gate") => {
            ensure!(args.len() == 3, "usage: full_oracle_run gate BASE_REPORT CANDIDATE_REPORT");
            let (text, ok) = oracle::gate(Path::new(&args[1]), Path::new(&args[2]))?;
            print!("{text}");
            if !ok {
                std::process::exit(1);
            }
            Ok(())
        }
        _ => bail!(
            "usage: full_oracle_run run REPORT_DIR [--workers N] [--deadline SECONDS] [--filter SUBSTRING] [--allow-dirty]\n       full_oracle_run gate BASE_REPORT CANDIDATE_REPORT"
        ),
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    ensure!(out.status.success(), "git {args:?} failed in {}", dir.display());
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

struct Options {
    report: PathBuf,
    workers: usize,
    deadline: Duration,
    filter: String,
    allow_dirty: bool,
}

fn parse(args: &[String]) -> Result<Options> {
    let mut it = args.iter();
    let report = PathBuf::from(it.next().context("REPORT_DIR")?);
    let mut o = Options {
        report,
        workers: std::thread::available_parallelism()
            .map_or(4, |n| n.get().saturating_sub(2).max(1)),
        deadline: Duration::from_secs(60),
        filter: String::new(),
        allow_dirty: false,
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--workers" => o.workers = it.next().context("--workers N")?.parse()?,
            "--deadline" => {
                o.deadline = Duration::from_secs(it.next().context("--deadline S")?.parse()?);
            }
            "--filter" => o.filter.clone_from(it.next().context("--filter SUBSTRING")?),
            "--allow-dirty" => o.allow_dirty = true,
            other => bail!("unknown argument {other}"),
        }
    }
    Ok(o)
}

/// The identity every artifact in the report is bound to.
struct Identity {
    rows: Vec<(String, String)>,
}

impl Identity {
    fn set(&mut self, k: &str, v: impl Into<String>) {
        let v = v.into();
        match self.rows.iter_mut().find(|(key, _)| key == k) {
            Some(row) => row.1 = v,
            None => self.rows.push((k.to_string(), v)),
        }
    }
    fn write(&self, dir: &Path) -> Result<()> {
        let mut s = String::new();
        for (k, v) in &self.rows {
            writeln!(s, "{k}\t{v}")?;
        }
        write_atomic(&dir.join("identity.tsv"), &s)
    }
}

fn freeze(source: &Path, dest: &Path) -> Result<String> {
    fs::copy(source, dest)?;
    let h = sha256(dest)?;
    let mut p = fs::metadata(dest)?.permissions();
    p.set_readonly(true);
    fs::set_permissions(dest, p)?;
    Ok(h)
}

#[allow(clippy::too_many_lines)]
fn run(args: &[String]) -> Result<()> {
    let o = parse(args)?;
    let root = PathBuf::from(git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    ensure!(
        !o.report.exists() || fs::read_dir(&o.report)?.next().is_none(),
        "report directory {} is not empty: every run starts fresh",
        o.report.display()
    );
    fs::create_dir_all(o.report.join("bin"))?;
    fs::create_dir_all(o.report.join("cases"))?;
    let report = o.report.canonicalize()?;
    let native = root.join("vendor/typescript-go");
    let corpus = native.join("_submodules/TypeScript");

    let mut id = Identity { rows: Vec::new() };
    id.set("status", "running");
    let source = git(&root, &["rev-parse", "HEAD"])?;
    id.set("source_commit", &source);
    let dirty = git(
        &root,
        &[
            "status",
            "--porcelain",
            "--untracked-files=no",
            "--",
            "crates",
            "xtask",
            "Cargo.toml",
            "Cargo.lock",
        ],
    )?;
    ensure!(
        dirty.is_empty() || o.allow_dirty,
        "compiler sources differ from {source}; commit them or pass --allow-dirty"
    );
    id.set(
        "source_dirty",
        if dirty.is_empty() {
            "clean".to_string()
        } else {
            format!("DIRTY {}", dirty.lines().count())
        },
    );
    let native_rev = git(&native, &["rev-parse", "HEAD"])?;
    ensure!(native_rev == NATIVE, "native checkout is {native_rev}, not the pinned {NATIVE}");
    ensure!(
        git(&native, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "native checkout is dirty"
    );
    id.set("native_revision", &native_rev);
    id.set("corpus_revision", git(&corpus, &["rev-parse", "HEAD"])?);
    ensure!(
        git(&corpus, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "corpus checkout is dirty"
    );
    id.set("workers", o.workers.to_string());
    id.set("deadline_seconds", o.deadline.as_secs().to_string());
    id.set("filter", &o.filter);
    id.set("native_env", "TS_TEST_PROGRAM_SINGLE_THREADED=true GOMAXPROCS=1");
    id.set("tsr_env", "RAYON_NUM_THREADS=1");

    // Native producer: the pinned runner plus two overlay files.
    let src = root.join("crates/tsr-conformance/src");
    let overlay = report.join("bin/overlay.json");
    write_atomic(
        &overlay,
        &format!(
            "{{\"Replace\":{{\"{}\":\"{}\",\"{}\":\"{}\"}}}}",
            native.join("internal/testrunner/full_oracle_test.go").display(),
            src.join("full_oracle_native.go").display(),
            native.join("internal/testutil/tsbaseline/full_oracle.go").display(),
            src.join("full_oracle_native_types.go").display()
        ),
    )?;
    let built = report.join("bin/native-build");
    let status = Command::new("go")
        .current_dir(&native)
        .arg("test")
        .arg(format!("-overlay={}", overlay.display()))
        .args(["-c", "-o"])
        .arg(&built)
        .arg("./internal/testrunner")
        .status()?;
    ensure!(status.success(), "native producer build failed");
    let native_bin = report.join("bin/native");
    id.set("native_binary_sha256", freeze(&built, &native_bin)?);
    fs::remove_file(&built)?;
    let status = Command::new("cargo")
        .current_dir(&root)
        .args(["build", "--release", "-p", "tsr-conformance", "--example", "full_oracle_actual"])
        .status()?;
    ensure!(status.success(), "TSR producer build failed");
    let target =
        std::env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    let tsr_bin = report.join("bin/tsr-actual");
    id.set(
        "tsr_binary_sha256",
        freeze(&target.join("release/examples/full_oracle_actual"), &tsr_bin)?,
    );
    for f in ["full_oracle.rs", "full_oracle_native.go", "full_oracle_native_types.go"] {
        id.set(&format!("producer_source_sha256:{f}"), sha256(&src.join(f))?);
    }
    id.write(&report)?;

    // Discovery: complete or abort.
    let plan = report.join("plan.tsv");
    let mut cmd = Command::new(&native_bin);
    cmd.current_dir(native.join("internal/testrunner"))
        .arg("-test.run=^TestFullOracle$")
        .env("TSR_ORACLE_MODE", "plan")
        .env("TSR_ORACLE_OUTPUT", &plan)
        .env("TS_TEST_PROGRAM_SINGLE_THREADED", "true");
    let (outcome, _) = oracle::run_bounded(
        &mut cmd,
        &report.join("plan.stdout"),
        &report.join("plan.stderr"),
        Duration::from_secs(900),
    )?;
    let plan_text = fs::read_to_string(&plan).unwrap_or_default();
    if outcome != ProcessOutcome::Exited || !plan_text.ends_with("COMPLETE\n") {
        id.set("status", format!("discovery-{outcome:?}"));
        id.write(&report)?;
        bail!("native discovery did not complete ({outcome:?}); no population is published");
    }
    id.set("plan_sha256", sha256(&plan)?);

    let mut rows: Vec<Vec<&str>> =
        plan_text.lines().filter(|l| *l != "COMPLETE").map(|l| l.split('\t').collect()).collect();
    if !o.filter.is_empty() {
        rows.retain(|r| unhex(r[1]).is_ok_and(|s| s.contains(&o.filter)));
    }
    ensure!(!rows.is_empty(), "empty population");
    id.set("population_rows", rows.len().to_string());
    id.write(&report)?;

    let cases_root = corpus.join("tests/cases");
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<String>>> = Mutex::new(vec![None; rows.len()]);
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..o.workers {
            scope.spawn(|| {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= rows.len() {
                        break;
                    }
                    match one(
                        i,
                        &rows[i],
                        &report,
                        &cases_root,
                        &native,
                        &native_bin,
                        &tsr_bin,
                        o.deadline,
                    ) {
                        Ok(line) => results.lock().unwrap()[i] = Some(line),
                        Err(e) => errors.lock().unwrap().push(format!("{i}\t{e:#}")),
                    }
                    let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if n % 500 == 0 {
                        eprintln!("oracle: {n}/{}", rows.len());
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    ensure!(errors.is_empty(), "runner infrastructure failed:\n{}", errors.join("\n"));
    let mut table = String::from(
        "index\tidentity\tvariant\toutcome\tclass\tdetail\tdiag_class\tdiag_detail\ttypes_class\ttypes_detail\tnative_ms\ttsr_ms\tnative_sha256\ttsr_sha256\n",
    );
    for (i, r) in results.into_inner().unwrap().into_iter().enumerate() {
        table.push_str(&r.with_context(|| format!("row {i} has no result"))?);
    }
    write_atomic(&report.join("results.tsv"), &table)?;
    let summary = summarize(&report)?;
    write_atomic(&report.join("summary.md"), &summary)?;
    id.set("results_sha256", sha256(&report.join("results.tsv"))?);
    id.set("status", "complete");
    id.write(&report)?;
    print!("{summary}");
    Ok(())
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ").chars().take(120).collect()
}

/// The first informative stderr line of a failed worker.
fn failure_detail(stderr: &Path) -> String {
    let text = fs::read_to_string(stderr).unwrap_or_default();
    let mut lines = text.lines();
    while let Some(l) = lines.next() {
        if let Some(rest) = l.strip_prefix("thread '") {
            let at = rest.split(" panicked at ").nth(1).unwrap_or(rest);
            let msg = lines.next().unwrap_or_default();
            return clean(&format!("panic {} {msg}", at.trim_end_matches(':')));
        }
        if let Some(rest) = l.strip_prefix("Error: ") {
            return clean(rest);
        }
        if l.contains("panic:")
            || l.contains("fatal error:")
            || l.trim_start().starts_with("--- FAIL")
            || l.contains(".go:") && l.contains("Fatal")
        {
            return clean(l.trim());
        }
    }
    clean(text.lines().last().unwrap_or("no output"))
}

#[allow(clippy::too_many_arguments)]
fn one(
    i: usize,
    row: &[&str],
    report: &Path,
    cases_root: &Path,
    native: &Path,
    native_bin: &Path,
    tsr_bin: &Path,
    deadline: Duration,
) -> Result<String> {
    let line = |outcome: &str,
                class: &str,
                detail: &str,
                v: Option<&oracle::Verdict>,
                ms: (u128, u128),
                shas: (&str, &str)| {
        let (dc, dd) = v
            .and_then(|v| v.diagnostics.clone())
            .map_or((String::new(), String::new()), |(c, d)| (c.to_string(), clean(&d)));
        let (tc, td) = v
            .and_then(|v| v.types.clone())
            .map_or((String::new(), String::new()), |(c, d)| (c.to_string(), clean(&d)));
        format!(
            "{i}\t{}\t{}\t{outcome}\t{class}\t{}\t{dc}\t{dd}\t{tc}\t{td}\t{}\t{}\t{}\t{}\n",
            row.get(1).copied().unwrap_or_default(),
            row.get(2).copied().unwrap_or_default(),
            clean(detail),
            ms.0,
            ms.1,
            shas.0,
            shas.1
        )
    };
    match row[0] {
        "LISTED_SKIP" => {
            return Ok(line("LISTED_SKIP", "native:skippedTests", "", None, (0, 0), ("", "")));
        }
        "DISCOVERY_FAILED" => {
            return Ok(line(
                "DISCOVERY_FAILED",
                "native:discovery-failed",
                "",
                None,
                (0, 0),
                ("", ""),
            ));
        }
        "CASE" => ensure!(row.len() == 4, "malformed plan row {i}"),
        other => bail!("unknown plan row kind {other}"),
    }
    let source = cases_root.join(unhex(row[1])?);
    let dir = report.join(format!("cases/{i:05}"));
    fs::create_dir_all(&dir)?;
    write_atomic(&dir.join("request.tsv"), &format!("{}\n", row.join("\t")))?;
    let native_out = dir.join("native.tsv");
    let mut cmd = Command::new(native_bin);
    cmd.current_dir(native.join("internal/testrunner"))
        .arg("-test.run=^TestFullOracle$")
        .env("TSR_ORACLE_MODE", "case")
        .env("TSR_ORACLE_CASE", &source)
        .env("TSR_ORACLE_VARIANT", unhex(row[2])?)
        .env("TSR_ORACLE_OUTPUT", &native_out)
        .env("TS_TEST_PROGRAM_SINGLE_THREADED", "true")
        .env("GOMAXPROCS", "1");
    let (n_outcome, n_time) = oracle::run_bounded(
        &mut cmd,
        &dir.join("native.stdout"),
        &dir.join("native.stderr"),
        deadline,
    )?;
    let n_ms = n_time.as_millis();
    let native_text = fs::read_to_string(&native_out).unwrap_or_default();
    let native_stream = Stream::parse(&native_text);
    match n_outcome {
        ProcessOutcome::Timeout => {
            return Ok(line("NATIVE_TIMEOUT", "native:timeout", "", None, (n_ms, 0), ("", "")));
        }
        ProcessOutcome::Failed => {
            return Ok(line(
                "NATIVE_FAILED",
                "native:failed",
                &failure_detail(&dir.join("native.stdout")),
                None,
                (n_ms, 0),
                ("", ""),
            ));
        }
        ProcessOutcome::Exited if !native_stream.complete => {
            return Ok(line("NATIVE_FAILED", "native:incomplete", "", None, (n_ms, 0), ("", "")));
        }
        ProcessOutcome::Exited => {}
    }
    let native_sha = sha256(&native_out)?;
    if native_stream.skipped {
        return Ok(line(
            "NATIVE_SKIPPED",
            "native:SkipUnsupportedCompilerOptions",
            "",
            None,
            (n_ms, 0),
            (&native_sha, ""),
        ));
    }
    let tsr_out = dir.join("actual.tsv");
    let mut cmd = Command::new(tsr_bin);
    cmd.arg(&source).arg(dir.join("request.tsv")).arg(&tsr_out).env("RAYON_NUM_THREADS", "1");
    let (t_outcome, t_time) = oracle::run_bounded(
        &mut cmd,
        &dir.join("actual.stdout"),
        &dir.join("actual.stderr"),
        deadline,
    )?;
    let t_ms = t_time.as_millis();
    let tsr_text = fs::read_to_string(&tsr_out).unwrap_or_default();
    let tsr_stream = Stream::parse(&tsr_text);
    match t_outcome {
        ProcessOutcome::Timeout => {
            return Ok(line(
                "TSR_TIMEOUT",
                "tsr:timeout",
                "",
                None,
                (n_ms, t_ms),
                (&native_sha, ""),
            ));
        }
        ProcessOutcome::Failed => {
            let detail = failure_detail(&dir.join("actual.stderr"));
            let class = if detail.starts_with("panic") { "tsr:panic" } else { "tsr:error" };
            return Ok(line("TSR_FAILED", class, &detail, None, (n_ms, t_ms), (&native_sha, "")));
        }
        ProcessOutcome::Exited if !tsr_stream.complete => {
            return Ok(line(
                "TSR_FAILED",
                "tsr:incomplete",
                "",
                None,
                (n_ms, t_ms),
                (&native_sha, ""),
            ));
        }
        ProcessOutcome::Exited => {}
    }
    let tsr_sha = sha256(&tsr_out)?;
    let verdict = oracle::compare(&native_stream, &tsr_stream);
    if verdict.exact() {
        // Exact artifacts are identical bytes; drop the duplicate copy.
        ensure!(native_sha == tsr_sha, "exact verdict over differing artifacts in row {i}");
        fs::remove_file(&tsr_out)?;
        return Ok(line("EXACT", "", "", None, (n_ms, t_ms), (&native_sha, &tsr_sha)));
    }
    let (class, detail) =
        verdict.diagnostics.clone().or_else(|| verdict.types.clone()).unwrap_or_default();
    Ok(line("WRONG", class, &detail, Some(&verdict), (n_ms, t_ms), (&native_sha, &tsr_sha)))
}

/// Numerator, denominator and the failure-class histogram of a complete report.
fn summarize(report: &Path) -> Result<String> {
    let text = fs::read_to_string(report.join("results.tsv"))?;
    let rows: Vec<Vec<&str>> = text.lines().skip(1).map(|l| l.split('\t').collect()).collect();
    let mut outcomes: BTreeMap<&str, usize> = BTreeMap::new();
    let mut primary: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut diag: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut types: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut codes: BTreeMap<String, BTreeMap<String, usize>> = BTreeMap::new();
    let mut halves: BTreeMap<&str, usize> = BTreeMap::new();
    let (mut exact, mut denominator, mut excluded) = (0usize, 0usize, 0usize);
    for r in &rows {
        let key = readable_key(&format!("{}\t{}", r[1], r[2]));
        let outcome = r[3];
        *outcomes.entry(outcome).or_default() += 1;
        if oracle::excluded(outcome) {
            excluded += 1;
            continue;
        }
        denominator += 1;
        if outcome == "EXACT" {
            exact += 1;
            continue;
        }
        let class = if r[4].is_empty() { outcome.to_string() } else { r[4].to_string() };
        primary.entry(class.clone()).or_default().push(key.clone());
        if !r[6].is_empty() {
            diag.entry(r[6].to_string()).or_default().push(key.clone());
            *codes.entry(r[6].to_string()).or_default().entry(r[7].to_string()).or_default() += 1;
        }
        if !r[8].is_empty() {
            types.entry(r[8].to_string()).or_default().push(key.clone());
            *codes.entry(r[8].to_string()).or_default().entry(r[9].to_string()).or_default() += 1;
        }
        if outcome == "WRONG" {
            let half = match (r[6].is_empty(), r[8].is_empty()) {
                (false, false) => "diagnostics and types",
                (false, true) => "diagnostics only",
                (true, false) => "types only",
                (true, true) => "neither (inconsistent)",
            };
            *halves.entry(half).or_default() += 1;
        }
        if r[4].starts_with("tsr:") || r[4].starts_with("native:") {
            *codes.entry(class).or_default().entry(r[5].chars().take(60).collect()).or_default() +=
                1;
        }
    }
    let identity = fs::read_to_string(report.join("identity.tsv"))?;
    let mut s = String::new();
    writeln!(s, "# Oracle summary\n\n```\n{identity}```\n")?;
    #[allow(clippy::cast_precision_loss)]
    let pct = 100.0 * exact as f64 / denominator.max(1) as f64;
    writeln!(s, "exact {exact} / {denominator} = {pct:.2}% (excluded native skips: {excluded})\n")?;
    writeln!(s, "| outcome | cases |\n|---|---|")?;
    for (k, v) in &outcomes {
        writeln!(s, "| {k} | {v} |")?;
    }
    writeln!(s, "\n| WRONG half | cases |\n|---|---|")?;
    for (k, v) in &halves {
        writeln!(s, "| {k} | {v} |")?;
    }
    let table = |s: &mut String, title: &str, m: &BTreeMap<String, Vec<String>>| -> Result<()> {
        let mut v: Vec<_> = m.iter().collect();
        v.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then(a.0.cmp(b.0)));
        writeln!(
            s,
            "\n### {title}\n\n| class | cases | top details | examples |\n|---|---|---|---|"
        )?;
        for (k, ids) in v {
            let mut top: Vec<_> =
                codes.get(k).map(|c| c.iter().collect::<Vec<_>>()).unwrap_or_default();
            top.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
            let top: Vec<_> = top.iter().take(4).map(|(c, n)| format!("{c} {n}")).collect();
            let ex: Vec<_> = ids.iter().take(3).map(|e| format!("`{e}`")).collect();
            writeln!(s, "| {k} | {} | {} | {} |", ids.len(), top.join(", "), ex.join(", "))?;
        }
        Ok(())
    };
    table(&mut s, "Primary class (diagnostic half first, else type half)", &primary)?;
    table(&mut s, "Diagnostic half, all WRONG cases with a diagnostic difference", &diag)?;
    table(&mut s, "Type half, all WRONG cases with a type difference", &types)?;

    // Per code, from the stored artifacts of every WRONG case.
    let mut by_code: BTreeMap<String, [Vec<String>; 3]> = BTreeMap::new();
    let mut answers: BTreeMap<&str, Vec<String>> = BTreeMap::new();
    for r in rows.iter().filter(|r| r[3] == "WRONG") {
        let key = readable_key(&format!("{}\t{}", r[1], r[2]));
        let dir = report.join(format!("cases/{:05}", r[0].parse::<usize>()?));
        let native_text = fs::read_to_string(dir.join("native.tsv"))?;
        let tsr_text = fs::read_to_string(dir.join("actual.tsv"))?;
        let (native_stream, tsr_stream) = (Stream::parse(&native_text), Stream::parse(&tsr_text));
        if !r[6].is_empty() {
            for (code, how) in oracle::diagnostic_code_profile(&native_stream, &tsr_stream) {
                let slot = match how {
                    "missing" => 0,
                    "extra" => 1,
                    _ => 2,
                };
                by_code.entry(code).or_default()[slot].push(key.clone());
            }
        }
        if let Some((native_type, tsr_type)) = oracle::first_type_text(&native_stream, &tsr_stream)
        {
            // Top-level `|` members in another order: a report-only split, the
            // verdict stays WRONG.
            let members = |t: &str| {
                let mut m: Vec<String> = t.split(" | ").map(str::to_string).collect();
                m.sort();
                m
            };
            let answer = match (native_type.as_str(), tsr_type.as_str()) {
                (_, "error") => "TSR prints `error` (no answer)",
                (_, "any") => "TSR prints `any`, native a type",
                ("any" | "error", _) => "native prints `any`/`error`, TSR a type",
                (n, t) if n.contains(" | ") && members(n) == members(t) => {
                    "same union members, other order"
                }
                _ => "both print a type; printing or inference differs",
            };
            answers.entry(answer).or_default().push(key);
        }
    }
    let mut v: Vec<_> = by_code.iter().collect();
    let size = |c: &[Vec<String>; 3]| c[0].len() + c[1].len() + c[2].len();
    v.sort_by(|a, b| size(b.1).cmp(&size(a.1)).then(a.0.cmp(b.0)));
    writeln!(
        s,
        "\n### Diagnostic codes in differences (cases; a case counts once per code and kind)\n\n| code | missing | extra | same location, other text/chain/related | examples |\n|---|---|---|---|---|"
    )?;
    for (code, c) in v.iter().take(40) {
        let ex: Vec<_> = c.iter().flatten().take(3).map(|e| format!("`{e}`")).collect();
        writeln!(
            s,
            "| {code} | {} | {} | {} | {} |",
            c[0].len(),
            c[1].len(),
            c[2].len(),
            ex.join(", ")
        )?;
    }
    writeln!(
        s,
        "\n### First `types:type-text` difference by answer\n\n| answer | cases | examples |\n|---|---|---|"
    )?;
    let mut v: Vec<_> = answers.iter().collect();
    v.sort_by_key(|entry| std::cmp::Reverse(entry.1.len()));
    for (k, ids) in v {
        let ex: Vec<_> = ids.iter().take(3).map(|e| format!("`{e}`")).collect();
        writeln!(s, "| {k} | {} | {} |", ids.len(), ex.join(", "))?;
    }
    Ok(s)
}
