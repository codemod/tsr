//! Exact full-corpus oracle: frozen native artifacts, TSR runs and the gate.
//!
//! ```text
//! full_oracle_run native NATIVE_DIR [--workers N] [--deadline S] [--filter SUBSTRING]
//! full_oracle_run tsr NATIVE_DIR REPORT_DIR [--workers N] [--deadline S] [--filter SUBSTRING]
//!                 [--per-process] [--allow-dirty]
//! full_oracle_run gate BASE_REPORT CANDIDATE_REPORT
//! full_oracle_run summarize REPORT_DIR
//! ```
//!
//! `native` builds the native producer through a `go test -overlay` of the pinned
//! checkout, discovers the population with the native runner (a failed or
//! timed-out discovery aborts; nothing resumes from a partial plan), runs every
//! configured case once in a bounded process and freezes the artifacts under an
//! identity: native and corpus revisions, producer source and binary SHA-256,
//! plan and results SHA-256. `tsr` refuses a native directory whose identity
//! does not match the current checkout, verifies every native artifact's SHA-256
//! before comparing against it, and runs TSR on each configured case: by default
//! in long-lived `--serve` workers (one case per fresh thread), with
//! `--per-process` one process per case. Every non-excluded row is in the
//! denominator. See `docs/parity/notes/oracle.md`.
use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    io::{BufRead, BufReader, Read, Seek, Write as _},
    path::{Path, PathBuf},
    process::{Child, ChildStdin, Command, Stdio},
    sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
        mpsc,
    },
    time::{Duration, Instant},
};
use tsr_conformance::full_oracle::{
    self as oracle, NATIVE, ProcessOutcome, Stream, hex, readable_key, sha256, sha256_bytes, unhex,
    write_atomic,
};

const USAGE: &str = "usage: full_oracle_run native NATIVE_DIR [--workers N] [--deadline S] [--filter SUBSTRING]
       full_oracle_run tsr NATIVE_DIR REPORT_DIR [--workers N] [--deadline S] [--filter SUBSTRING] [--per-process] [--allow-dirty]
       full_oracle_run gate BASE_REPORT CANDIDATE_REPORT
       full_oracle_run summarize REPORT_DIR";

/// The native producer's sources: the overlay files that define its records.
const NATIVE_SOURCES: [&str; 2] = ["full_oracle_native.go", "full_oracle_native_types.go"];

/// Native identity keys a TSR report inherits and the gate requires equal.
const NATIVE_KEYS: [&str; 8] = [
    "native_revision",
    "corpus_revision",
    "producer_source_sha256:full_oracle_native.go",
    "producer_source_sha256:full_oracle_native_types.go",
    "native_binary_sha256",
    "plan_sha256",
    "native_results_sha256",
    "native_deadline_seconds",
];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("native") => native(&args[1..]),
        Some("tsr") => tsr(&args[1..]),
        Some("summarize") => {
            ensure!(args.len() == 2, "{USAGE}");
            print!("{}", summarize(Path::new(&args[1]))?);
            Ok(())
        }
        Some("gate") => {
            ensure!(args.len() == 3, "{USAGE}");
            let (text, ok) = oracle::gate(Path::new(&args[1]), Path::new(&args[2]))?;
            print!("{text}");
            if !ok {
                std::process::exit(1);
            }
            Ok(())
        }
        _ => bail!("{USAGE}"),
    }
}

fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    ensure!(out.status.success(), "git {args:?} failed in {}", dir.display());
    Ok(String::from_utf8(out.stdout)?.trim().to_string())
}

struct Options {
    positional: Vec<PathBuf>,
    workers: usize,
    deadline: Duration,
    filter: String,
    allow_dirty: bool,
    per_process: bool,
}

fn parse(args: &[String], positional: usize) -> Result<Options> {
    let mut it = args.iter();
    let mut o = Options {
        positional: Vec::new(),
        workers: std::thread::available_parallelism().map_or(4, std::num::NonZeroUsize::get),
        deadline: Duration::from_secs(60),
        filter: String::new(),
        allow_dirty: false,
        per_process: false,
    };
    while let Some(a) = it.next() {
        match a.as_str() {
            "--workers" => o.workers = it.next().context("--workers N")?.parse()?,
            "--deadline" => {
                o.deadline = Duration::from_secs(it.next().context("--deadline S")?.parse()?);
            }
            "--filter" => o.filter.clone_from(it.next().context("--filter SUBSTRING")?),
            "--allow-dirty" => o.allow_dirty = true,
            "--per-process" => o.per_process = true,
            other if !other.starts_with("--") => o.positional.push(PathBuf::from(other)),
            other => bail!("unknown argument {other}\n{USAGE}"),
        }
    }
    ensure!(o.positional.len() == positional, "{USAGE}");
    ensure!(o.workers > 0, "--workers must be positive");
    Ok(o)
}

/// The identity every artifact of a directory is bound to.
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

fn read_identity(dir: &Path) -> Result<BTreeMap<String, String>> {
    Ok(fs::read_to_string(dir.join("identity.tsv"))
        .with_context(|| format!("{} has no identity.tsv", dir.display()))?
        .lines()
        .filter_map(|l| l.split_once('\t'))
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect())
}

/// Copy `source` to `dest` read-only and return its SHA-256.
fn freeze(source: &Path, dest: &Path) -> Result<String> {
    fs::copy(source, dest)?;
    let h = sha256(dest)?;
    let mut p = fs::metadata(dest)?.permissions();
    p.set_readonly(true);
    fs::set_permissions(dest, p)?;
    Ok(h)
}

/// An empty (or new) directory with `bin/` and `cases/`, canonicalized.
fn fresh(dir: &Path) -> Result<PathBuf> {
    ensure!(
        !dir.exists() || fs::read_dir(dir)?.next().is_none(),
        "directory {} is not empty: every run starts fresh",
        dir.display()
    );
    fs::create_dir_all(dir.join("bin"))?;
    fs::create_dir_all(dir.join("cases"))?;
    Ok(dir.canonicalize()?)
}

/// The repository root, the pinned native checkout and its corpus, verified
/// clean: `(root, native, corpus, corpus_revision)`.
fn checkouts() -> Result<(PathBuf, PathBuf, PathBuf, String)> {
    let root = PathBuf::from(git(Path::new("."), &["rev-parse", "--show-toplevel"])?);
    let native = root.join("vendor/typescript-go");
    let corpus = native.join("_submodules/TypeScript");
    let native_rev = git(&native, &["rev-parse", "HEAD"])?;
    ensure!(native_rev == NATIVE, "native checkout is {native_rev}, not the pinned {NATIVE}");
    ensure!(
        git(&native, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "native checkout is dirty"
    );
    ensure!(
        git(&corpus, &["status", "--porcelain", "--untracked-files=no"])?.is_empty(),
        "corpus checkout is dirty"
    );
    let corpus_rev = git(&corpus, &["rev-parse", "HEAD"])?;
    Ok((root, native, corpus, corpus_rev))
}

/// Run `body(i)` for every index on `workers` threads, each with its own state.
fn parallel<S>(
    n: usize,
    workers: usize,
    state: impl Fn() -> S + Sync,
    body: impl Fn(&mut S, usize) -> Result<String> + Sync,
) -> Result<Vec<String>> {
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let results: Mutex<Vec<Option<String>>> = Mutex::new(vec![None; n]);
    let errors = Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..workers.min(n) {
            scope.spawn(|| {
                let mut s = state();
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= n {
                        break;
                    }
                    match body(&mut s, i) {
                        Ok(line) => results.lock().unwrap()[i] = Some(line),
                        Err(e) => errors.lock().unwrap().push(format!("{i}\t{e:#}")),
                    }
                    let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                    if d % 1000 == 0 {
                        eprintln!("oracle: {d}/{n}");
                    }
                }
            });
        }
    });
    let errors = errors.into_inner().unwrap();
    ensure!(errors.is_empty(), "runner infrastructure failed:\n{}", errors.join("\n"));
    results
        .into_inner()
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(i, r)| r.with_context(|| format!("row {i} has no result")))
        .collect()
}

#[allow(clippy::too_many_lines)]
fn native(args: &[String]) -> Result<()> {
    let o = parse(args, 1)?;
    let (root, native, corpus, corpus_rev) = checkouts()?;
    let dir = fresh(&o.positional[0])?;
    let mut id = Identity { rows: Vec::new() };
    id.set("status", "running");
    id.set("kind", "native");
    id.set("native_revision", NATIVE);
    id.set("corpus_revision", &corpus_rev);
    let src = root.join("crates/tsr-conformance/src");
    for f in NATIVE_SOURCES {
        id.set(&format!("producer_source_sha256:{f}"), sha256(&src.join(f))?);
    }
    id.set("workers", o.workers.to_string());
    id.set("native_deadline_seconds", o.deadline.as_secs().to_string());
    id.set("native_filter", &o.filter);
    id.set("native_env", "TS_TEST_PROGRAM_SINGLE_THREADED=true GOMAXPROCS=1");

    // Native producer: the pinned runner plus two overlay files.
    let overlay = dir.join("bin/overlay.json");
    write_atomic(
        &overlay,
        &format!(
            "{{\"Replace\":{{\"{}\":\"{}\",\"{}\":\"{}\"}}}}",
            native.join("internal/testrunner/full_oracle_test.go").display(),
            src.join(NATIVE_SOURCES[0]).display(),
            native.join("internal/testutil/tsbaseline/full_oracle.go").display(),
            src.join(NATIVE_SOURCES[1]).display()
        ),
    )?;
    let built = dir.join("bin/native-build");
    let status = Command::new("go")
        .current_dir(&native)
        .arg("test")
        .arg(format!("-overlay={}", overlay.display()))
        .args(["-c", "-o"])
        .arg(&built)
        .arg("./internal/testrunner")
        .status()?;
    ensure!(status.success(), "native producer build failed");
    let native_bin = dir.join("bin/native");
    id.set("native_binary_sha256", freeze(&built, &native_bin)?);
    fs::remove_file(&built)?;
    id.write(&dir)?;

    // Discovery: complete or abort.
    let plan = dir.join("plan.tsv");
    let mut cmd = Command::new(&native_bin);
    cmd.current_dir(native.join("internal/testrunner"))
        .arg("-test.run=^TestFullOracle$")
        .env("TSR_ORACLE_MODE", "plan")
        .env("TSR_ORACLE_OUTPUT", &plan)
        .env("TS_TEST_PROGRAM_SINGLE_THREADED", "true");
    let (outcome, _) = oracle::run_bounded(
        &mut cmd,
        &dir.join("plan.stdout"),
        &dir.join("plan.stderr"),
        Duration::from_secs(900),
    )?;
    let plan_text = fs::read_to_string(&plan).unwrap_or_default();
    if outcome != ProcessOutcome::Exited || !plan_text.ends_with("COMPLETE\n") {
        id.set("status", format!("discovery-{outcome:?}"));
        id.write(&dir)?;
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
    id.write(&dir)?;

    let cases_root = corpus.join("tests/cases");
    let started = Instant::now();
    let lines = parallel(
        rows.len(),
        o.workers,
        || (),
        |(), i| native_one(i, &rows[i], &dir, &cases_root, &native, &native_bin, o.deadline),
    )?;
    let mut table = String::from(
        "index\tidentity\tvariant\toutcome\tclass\tdetail\tnative_ms\tnative_sha256\n",
    );
    for l in lines {
        table.push_str(&l);
    }
    write_atomic(&dir.join("native.tsv"), &table)?;
    id.set("native_results_sha256", sha256(&dir.join("native.tsv"))?);
    id.set("wall_seconds", started.elapsed().as_secs().to_string());
    id.set("status", "complete");
    id.write(&dir)?;
    let mut counts: BTreeMap<&str, usize> = BTreeMap::new();
    for l in table.lines().skip(1) {
        *counts.entry(l.split('\t').nth(3).unwrap_or_default()).or_default() += 1;
    }
    println!("native {}: {counts:?}", dir.display());
    Ok(())
}

fn clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ").chars().take(120).collect()
}

/// The first informative stderr line of a failed worker.
fn failure_detail(text: &str) -> String {
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

fn native_one(
    i: usize,
    row: &[&str],
    dir: &Path,
    cases_root: &Path,
    native: &Path,
    native_bin: &Path,
    deadline: Duration,
) -> Result<String> {
    let line = |outcome: &str, class: &str, detail: &str, ms: u128, sha: &str| {
        format!(
            "{i}\t{}\t{}\t{outcome}\t{class}\t{}\t{ms}\t{sha}\n",
            row.get(1).copied().unwrap_or_default(),
            row.get(2).copied().unwrap_or_default(),
            clean(detail),
        )
    };
    match row[0] {
        "LISTED_SKIP" => return Ok(line("LISTED_SKIP", "native:skippedTests", "", 0, "")),
        "DISCOVERY_FAILED" => {
            return Ok(line("DISCOVERY_FAILED", "native:discovery-failed", "", 0, ""));
        }
        "CASE" => ensure!(row.len() == 4, "malformed plan row {i}"),
        other => bail!("unknown plan row kind {other}"),
    }
    let case_dir = dir.join(format!("cases/{i:05}"));
    fs::create_dir_all(&case_dir)?;
    write_atomic(&case_dir.join("request.tsv"), &format!("{}\n", row.join("\t")))?;
    let out = case_dir.join("native.tsv");
    let (stdout, stderr) = (case_dir.join("native.stdout"), case_dir.join("native.stderr"));
    let mut cmd = Command::new(native_bin);
    cmd.current_dir(native.join("internal/testrunner"))
        .arg("-test.run=^TestFullOracle$")
        .env("TSR_ORACLE_MODE", "case")
        .env("TSR_ORACLE_CASE", cases_root.join(unhex(row[1])?))
        .env("TSR_ORACLE_VARIANT", unhex(row[2])?)
        .env("TSR_ORACLE_OUTPUT", &out)
        .env("TS_TEST_PROGRAM_SINGLE_THREADED", "true")
        .env("GOMAXPROCS", "1");
    let (outcome, time) = oracle::run_bounded(&mut cmd, &stdout, &stderr, deadline)?;
    let ms = time.as_millis();
    let text = fs::read_to_string(&out).unwrap_or_default();
    let stream = Stream::parse(&text);
    match outcome {
        ProcessOutcome::Timeout => return Ok(line("NATIVE_TIMEOUT", "native:timeout", "", ms, "")),
        ProcessOutcome::Failed => {
            let detail = failure_detail(&fs::read_to_string(&stdout).unwrap_or_default());
            return Ok(line("NATIVE_FAILED", "native:failed", &detail, ms, ""));
        }
        ProcessOutcome::Exited if !stream.complete => {
            return Ok(line("NATIVE_FAILED", "native:incomplete", "", ms, ""));
        }
        ProcessOutcome::Exited => {}
    }
    // A passing test binary's console output carries nothing.
    fs::remove_file(&stdout)?;
    fs::remove_file(&stderr)?;
    let sha = sha256_bytes(text.as_bytes());
    if stream.skipped {
        return Ok(line("NATIVE_SKIPPED", "native:SkipUnsupportedCompilerOptions", "", ms, &sha));
    }
    Ok(line("CASE", "", "", ms, &sha))
}

/// A long-lived `full_oracle_actual --serve` process.
struct Server {
    child: Child,
    stdin: ChildStdin,
    replies: mpsc::Receiver<String>,
}

/// One TSR run's end.
enum TsrEnd {
    /// Artifact written.
    Exited,
    /// Error, panic or abnormal exit, with the worker's stderr for the case.
    Failed(String),
    /// Killed at the deadline.
    Timeout,
}

/// A worker slot: its server (started on demand, replaced after a failure)
/// and the stderr file every server of the slot appends to.
struct Slot {
    server: Option<Server>,
    stderr: PathBuf,
}

impl Slot {
    fn start(&mut self, bin: &Path) -> Result<&mut Server> {
        if self.server.is_none() {
            let err = fs::OpenOptions::new().create(true).append(true).open(&self.stderr)?;
            let mut child = Command::new(bin)
                .arg("--serve")
                .env("RAYON_NUM_THREADS", "1")
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(err)
                .spawn()?;
            let stdin = child.stdin.take().context("worker stdin")?;
            let stdout = child.stdout.take().context("worker stdout")?;
            let (tx, replies) = mpsc::channel();
            std::thread::spawn(move || {
                for line in BufReader::new(stdout).lines() {
                    let Ok(line) = line else { break };
                    if tx.send(line).is_err() {
                        break;
                    }
                }
            });
            self.server = Some(Server { child, stdin, replies });
        }
        Ok(self.server.as_mut().expect("started above"))
    }

    fn stop(&mut self, kill: bool) -> Result<()> {
        if let Some(mut s) = self.server.take() {
            if kill {
                let _ = s.child.kill();
            }
            s.child.wait()?;
        }
        Ok(())
    }

    /// The stderr this slot wrote since byte `from`.
    fn stderr_since(&self, from: u64) -> Result<String> {
        let mut f = fs::File::open(&self.stderr)?;
        f.seek(std::io::SeekFrom::Start(from))?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        Ok(s)
    }

    fn run(
        &mut self,
        bin: &Path,
        source: &Path,
        request: &str,
        output: &Path,
        deadline: Duration,
    ) -> Result<(TsrEnd, Duration)> {
        let from = fs::metadata(&self.stderr).map_or(0, |m| m.len());
        let line = format!(
            "{}\t{}\t{request}\n",
            hex(source.to_str().context("UTF-8 source path")?),
            hex(output.to_str().context("UTF-8 output path")?)
        );
        let start = Instant::now();
        let server = self.start(bin)?;
        let sent = server.stdin.write_all(line.as_bytes()).and_then(|()| server.stdin.flush());
        let reply = if sent.is_ok() {
            server.replies.recv_timeout(deadline.saturating_sub(start.elapsed()))
        } else {
            Err(mpsc::RecvTimeoutError::Disconnected)
        };
        let elapsed = start.elapsed();
        Ok(match reply {
            Ok(r) if r == "ok" => (TsrEnd::Exited, elapsed),
            Ok(r) => {
                let msg = r.strip_prefix("error\t").unwrap_or(&r);
                (TsrEnd::Failed(format!("Error: {msg}\n")), elapsed)
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {
                self.stop(true)?;
                (TsrEnd::Timeout, elapsed)
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                self.stop(false)?;
                (TsrEnd::Failed(self.stderr_since(from)?), elapsed)
            }
        })
    }
}

/// A verified native directory: its identity and `native.tsv` rows.
struct NativeRun {
    dir: PathBuf,
    identity: BTreeMap<String, String>,
    table: String,
}

impl NativeRun {
    /// Refuse anything but a complete native run of this checkout's pinned
    /// native, corpus and native producer sources, with intact artifacts.
    fn open(dir: &Path, root: &Path, corpus_rev: &str) -> Result<Self> {
        let dir = dir.canonicalize()?;
        let identity = read_identity(&dir)?;
        let get = |k: &str| identity.get(k).map_or("", String::as_str);
        ensure!(get("kind") == "native", "{} is not a native run", dir.display());
        ensure!(get("status") == "complete", "native run {} is not complete", dir.display());
        ensure!(get("native_revision") == NATIVE, "native run is not at the pinned revision");
        ensure!(get("corpus_revision") == corpus_rev, "native run corpus revision differs");
        let src = root.join("crates/tsr-conformance/src");
        for f in NATIVE_SOURCES {
            ensure!(
                get(&format!("producer_source_sha256:{f}")) == sha256(&src.join(f))?,
                "native producer source {f} changed since the native run: re-run `native`"
            );
        }
        ensure!(get("native_binary_sha256") == sha256(&dir.join("bin/native"))?, "native binary");
        ensure!(get("plan_sha256") == sha256(&dir.join("plan.tsv"))?, "native plan");
        let table = fs::read_to_string(dir.join("native.tsv"))?;
        ensure!(
            get("native_results_sha256") == sha256_bytes(table.as_bytes()),
            "native results table"
        );
        Ok(NativeRun { dir, identity, table })
    }
}

#[allow(clippy::too_many_lines)]
fn tsr(args: &[String]) -> Result<()> {
    let o = parse(args, 2)?;
    let (root, _, corpus, corpus_rev) = checkouts()?;
    let native_run = NativeRun::open(&o.positional[0], &root, &corpus_rev)?;
    let report = fresh(&o.positional[1])?;
    let mut id = Identity { rows: Vec::new() };
    id.set("status", "running");
    id.set("kind", "tsr");
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
    id.set("native_dir", native_run.dir.display().to_string());
    id.set("native_identity_sha256", sha256(&native_run.dir.join("identity.tsv"))?);
    for k in NATIVE_KEYS.iter().chain(&["native_filter", "population_rows"]) {
        id.set(
            &k.replace("population_rows", "native_population_rows"),
            native_run.identity.get(*k).cloned().unwrap_or_default(),
        );
    }
    id.set("tsr_mode", if o.per_process { "per-process" } else { "serve" });
    id.set("workers", o.workers.to_string());
    id.set("tsr_deadline_seconds", o.deadline.as_secs().to_string());
    id.set("tsr_filter", &o.filter);
    id.set("tsr_env", "RAYON_NUM_THREADS=1");
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
    id.set(
        "producer_source_sha256:full_oracle.rs",
        sha256(&root.join("crates/tsr-conformance/src/full_oracle.rs"))?,
    );
    let mut rows: Vec<Vec<&str>> =
        native_run.table.lines().skip(1).map(|l| l.split('\t').collect()).collect();
    ensure!(rows.iter().all(|r| r.len() == 8), "malformed native.tsv");
    if !o.filter.is_empty() {
        rows.retain(|r| unhex(r[1]).is_ok_and(|s| s.contains(&o.filter)));
    }
    ensure!(!rows.is_empty(), "empty population");
    id.set("population_rows", rows.len().to_string());
    id.write(&report)?;
    fs::create_dir_all(report.join("workers"))?;

    let cases_root = corpus.join("tests/cases");
    let slot_id = AtomicUsize::new(0);
    let started = Instant::now();
    let lines = parallel(
        rows.len(),
        o.workers,
        || Slot {
            server: None,
            stderr: report
                .join(format!("workers/{}.stderr", slot_id.fetch_add(1, Ordering::Relaxed))),
        },
        |slot, i| tsr_one(&rows[i], &native_run.dir, &report, &cases_root, &tsr_bin, slot, &o),
    )?;
    let mut table = String::from(
        "index\tidentity\tvariant\toutcome\tclass\tdetail\tdiag_class\tdiag_detail\ttypes_class\ttypes_detail\tnative_ms\ttsr_ms\tnative_sha256\ttsr_sha256\n",
    );
    for l in lines {
        table.push_str(&l);
    }
    write_atomic(&report.join("results.tsv"), &table)?;
    id.set("wall_seconds", started.elapsed().as_secs().to_string());
    let summary = summarize(&report)?;
    write_atomic(&report.join("summary.md"), &summary)?;
    id.set("results_sha256", sha256(&report.join("results.tsv"))?);
    id.set("status", "complete");
    id.write(&report)?;
    print!("{summary}");
    Ok(())
}

fn tsr_one(
    native_row: &[&str],
    native_dir: &Path,
    report: &Path,
    cases_root: &Path,
    tsr_bin: &Path,
    slot: &mut Slot,
    o: &Options,
) -> Result<String> {
    let (index, outcome, native_ms, native_sha) =
        (native_row[0], native_row[3], native_row[6], native_row[7]);
    let line = |outcome: &str,
                class: &str,
                detail: &str,
                v: Option<&oracle::Verdict>,
                tsr_ms: u128,
                tsr_sha: &str| {
        let (dc, dd) = v
            .and_then(|v| v.diagnostics.clone())
            .map_or((String::new(), String::new()), |(c, d)| (c.to_string(), clean(&d)));
        let (tc, td) = v
            .and_then(|v| v.types.clone())
            .map_or((String::new(), String::new()), |(c, d)| (c.to_string(), clean(&d)));
        format!(
            "{index}\t{}\t{}\t{outcome}\t{class}\t{}\t{dc}\t{dd}\t{tc}\t{td}\t{native_ms}\t{tsr_ms}\t{native_sha}\t{tsr_sha}\n",
            native_row[1],
            native_row[2],
            clean(detail),
        )
    };
    if outcome != "CASE" {
        // Native-side outcomes pass through: skips are excluded, failures count.
        return Ok(line(outcome, native_row[4], native_row[5], None, 0, ""));
    }
    let i: usize = index.parse()?;
    let native_case = native_dir.join(format!("cases/{i:05}"));
    let native_bytes = fs::read(native_case.join("native.tsv"))?;
    ensure!(
        sha256_bytes(&native_bytes) == native_sha,
        "native artifact {i} does not match its SHA-256"
    );
    let native_text = String::from_utf8(native_bytes)?;
    let request = fs::read_to_string(native_case.join("request.tsv"))?;
    let request = request.trim_end_matches('\n');
    let source = cases_root.join(unhex(native_row[1])?);
    let dir = report.join(format!("cases/{i:05}"));
    fs::create_dir_all(&dir)?;
    let out = dir.join("actual.tsv");
    let (end, time) = if o.per_process {
        let request_file = dir.join("request.tsv");
        write_atomic(&request_file, &format!("{request}\n"))?;
        let mut cmd = Command::new(tsr_bin);
        cmd.arg(&source).arg(&request_file).arg(&out).env("RAYON_NUM_THREADS", "1");
        let stderr = dir.join("actual.stderr");
        let (outcome, time) =
            oracle::run_bounded(&mut cmd, &dir.join("actual.stdout"), &stderr, o.deadline)?;
        fs::remove_file(&request_file)?;
        let end = match outcome {
            ProcessOutcome::Exited => TsrEnd::Exited,
            ProcessOutcome::Failed => TsrEnd::Failed(fs::read_to_string(&stderr)?),
            ProcessOutcome::Timeout => TsrEnd::Timeout,
        };
        (end, time)
    } else {
        slot.run(tsr_bin, &source, request, &out, o.deadline)?
    };
    let tsr_ms = time.as_millis();
    let tsr_text = match end {
        TsrEnd::Timeout => return Ok(line("TSR_TIMEOUT", "tsr:timeout", "", None, tsr_ms, "")),
        TsrEnd::Failed(stderr) => {
            write_atomic(&dir.join("actual.stderr"), &stderr)?;
            let detail = failure_detail(&stderr);
            let class = if detail.starts_with("panic") { "tsr:panic" } else { "tsr:error" };
            return Ok(line("TSR_FAILED", class, &detail, None, tsr_ms, ""));
        }
        TsrEnd::Exited => fs::read_to_string(&out).unwrap_or_default(),
    };
    for f in ["actual.stdout", "actual.stderr"] {
        if dir.join(f).exists() {
            fs::remove_file(dir.join(f))?;
        }
    }
    let tsr_stream = Stream::parse(&tsr_text);
    if !tsr_stream.complete {
        return Ok(line("TSR_FAILED", "tsr:incomplete", "", None, tsr_ms, ""));
    }
    let tsr_sha = sha256_bytes(tsr_text.as_bytes());
    let verdict = oracle::compare(&Stream::parse(&native_text), &tsr_stream);
    if verdict.exact() {
        // Exact artifacts are identical bytes; nothing is kept.
        ensure!(native_sha == tsr_sha, "exact verdict over differing artifacts in row {i}");
        fs::remove_file(&out)?;
        fs::remove_dir(&dir)?;
        return Ok(line("EXACT", "", "", None, tsr_ms, &tsr_sha));
    }
    // The report is self-contained for every difference.
    write_atomic(&dir.join("native.tsv"), &native_text)?;
    let (class, detail) =
        verdict.diagnostics.clone().or_else(|| verdict.types.clone()).unwrap_or_default();
    Ok(line("WRONG", class, &detail, Some(&verdict), tsr_ms, &tsr_sha))
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
