//! Exact compiler-runner oracle. Native operations execute through a read-only
//! Go overlay; actual output must come from a lossless compiler producer.
//!
//! Unlike the legacy suites, this population includes both native trees, every
//! configuration, native exclusions, divergences, empty output and failures.
use std::{collections::BTreeMap, path::{Path, PathBuf}, process::{Command, Stdio}, time::{Duration, Instant}};
use anyhow::{Context, Result, bail};

/// A fully qualified native case/configuration identity and its input settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Configuration {
    /// Source tree, suite-relative path and native variation description.
    pub id: String,
    /// Native case source, including its extension.
    pub source: PathBuf,
    /// Native variation description (empty for a single configuration).
    pub name: String,
    /// Native merged settings: one `key=hex(value)` per line.
    pub settings: String,
}

/// The exact output, not a projection onto positional diagnostic keys.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifacts {
    /// Ordered diagnostics, recursive chains, related information and flags.
    pub diagnostics: String,
    /// The complete native diagnostic baseline (including pretty printing).
    pub errors: Vec<u8>,
    /// Complete independently generated native type baseline.
    pub types: Vec<u8>,
}

/// Every result is retained; only `Exact` contributes to the exact numerator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Verdict {
    /// All three complete artifacts agree.
    Exact,
    /// At least one complete artifact disagrees.
    Different {
        /// Semantic diagnostic payload differs.
        diagnostics: bool,
        /// Rendered diagnostic baseline differs.
        errors: bool,
        /// Independently generated type baseline differs.
        types: bool,
    },
    /// Native compilation failed, still in the denominator.
    NativeFailure(String),
    /// Actual compilation failed or its producer is unavailable.
    ActualFailure(String),
    /// Native case configuration extraction failed.
    DiscoveryFailure(String),
}

/// Compare complete artifacts without sorting, normalizing or dropping fields.
#[must_use]
pub fn compare(expected: &Artifacts, actual: &Artifacts) -> Verdict {
    if expected == actual {
        Verdict::Exact
    } else {
        Verdict::Different {
            diagnostics: expected.diagnostics != actual.diagnostics,
            errors: expected.errors != actual.errors,
            types: expected.types != actual.types,
        }
    }
}

/// Native record retained independently of the actual verdict.
#[derive(Debug, Clone)]
pub struct NativeResult {
    /// Discovered configuration and merged settings.
    pub configuration: Configuration,
    /// Native skip policy, recorded without excluding the configuration.
    pub eligibility: String,
    /// Complete native artifacts or its recorded failure.
    pub output: Result<Artifacts, String>,
}

/// Full native population, including failed configuration extraction.
#[derive(Debug, Default)]
pub struct Population {
    /// Every successfully discovered native configuration, including crashes.
    pub configurations: BTreeMap<String, NativeResult>,
    /// Cases whose native configuration expansion failed.
    pub discovery_failures: BTreeMap<String, String>,
}

/// Decode the overlay protocol. Duplicate publication and orphan results are
/// errors, not silently overwritten rows or implicit clean expectations.
pub fn read_population(path: &Path) -> Result<Population> {
    let input = std::fs::read_to_string(path)?;
    let mut configurations = BTreeMap::new();
    let mut results = BTreeMap::new();
    let mut failures = BTreeMap::new();
    for (line_number, line) in input.lines().enumerate() {
        let fields: Vec<String> = line.split('\t').enumerate().map(|(index, field)| {
            if line.starts_with("52\t") && index >= 4 { Ok(field.to_string()) } else { unhex(field) }
        }).collect::<Result<_>>()
            .with_context(|| format!("native record {}", line_number + 1))?;
        match fields.first().map(String::as_str) {
            Some("C") if fields.len() == 5 => {
                let config = Configuration {
                    id: fields[1].clone(), source: PathBuf::from(&fields[2]),
                    name: fields[3].clone(), settings: fields[4].clone(),
                };
                if configurations.insert(config.id.clone(), config).is_some() {
                    bail!("duplicate native configuration {}", fields[1]);
                }
            }
            Some("R") if fields.len() == 7 => {
                if results.insert(fields[1].clone(), fields[2..].to_vec()).is_some() {
                    bail!("duplicate native result {}", fields[1]);
                }
            }
            Some("E") if fields.len() == 3 => {
                if failures.insert(fields[1].clone(), fields[2].clone()).is_some() {
                    bail!("duplicate native discovery failure {}", fields[1]);
                }
            }
            _ => bail!("invalid native record {}", line_number + 1),
        }
    }
    let mut population = Population { configurations: BTreeMap::new(), discovery_failures: failures };
    for (id, configuration) in configurations {
        let (eligibility, output) = match results.remove(&id) {
            Some(row) if row[0] == "complete" => (row[1].clone(), Ok(Artifacts {
                diagnostics: unhex(&row[2])?, errors: unhex_bytes(&row[3])?, types: unhex_bytes(&row[4])?,
            })),
            Some(row) => (String::new(), Err(format!("{}: {}", row[0], row[1]))),
            None => (String::new(), Err("native process did not publish a result".into())),
        };
        population.configurations.insert(id, NativeResult { configuration, eligibility, output });
    }
    if let Some(id) = results.keys().next() { bail!("native result without configuration: {id}"); }
    Ok(population)
}

/// Install read-only accessors in Go's overlay, not in the vendor tree. Builds
/// use the exact pinned runner, options tables, VFS, pre/post-emit collector,
/// diagnostic renderer and type walker. All persistent artifacts belong to dir.
pub fn native_population(upstream: &Path, dir: &Path, selection: Option<&str>) -> Result<Population> {
    std::fs::create_dir_all(dir)?;
    let dir = std::fs::canonicalize(dir)?;
    let upstream = std::fs::canonicalize(upstream)?;
    let source = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("src");
    let overlay = format!("{{\"Replace\":{{{}:{},{}:{}}}}}",
        json_path(&upstream.join("internal/testrunner/full_oracle_native_test.go")),
        json_path(&source.join("full_oracle_native_test.go")),
        json_path(&upstream.join("internal/testutil/tsbaseline/full_oracle_type_export.go")),
        json_path(&source.join("full_oracle_type_export.go")));
    let overlay_path = dir.join("overlay.json");
    std::fs::write(&overlay_path, overlay)?;
    let output = dir.join("native.tsv");
    std::fs::write(&output, "")?;
    let run = selection.unwrap_or("^TestFullOracle$");
    let binary = dir.join("native.test");
    let build = Command::new("go").current_dir(&upstream)
        .args(["test", "-c"]).arg("-o").arg(&binary)
        .arg(format!("-overlay={}", overlay_path.display()))
        .arg("./internal/testrunner").output()?;
    std::fs::write(dir.join("build.stderr"), &build.stderr)?;
    if !build.status.success() { bail!("native build failed; see {}", dir.display()); }
    let discovery = run_bounded(Command::new(&binary).current_dir(upstream.join("internal/testrunner"))
        .arg(format!("-test.run={run}"))
        .env("FULL_ORACLE_DISCOVERY", "1")
        .env("FULL_ORACLE_OUTPUT", &output), &dir.join("discovery"), Duration::from_secs(120))?;
    std::fs::write(dir.join("discovery.status"), &discovery)?;
    // Go's discovery failures are records, not a reason to discard successful cases.
    if discovery.starts_with("timeout") { bail!("native discovery deadline exceeded; manifest retained"); }
    if std::fs::metadata(&output)?.len() == 0 {
        bail!("native discovery failed; see {}", dir.display());
    }
    let discovered = read_population(&output)?;
    // Exactly ten leased processes. Every configuration starts a fresh compiler;
    // crashes/nonzero exits override an R row rather than masquerading as success.
    use rayon::prelude::*;
    use std::io::Write;
    let checkpoint = std::sync::Mutex::new(std::fs::OpenOptions::new().append(true).open(&output)?);
    let pool = rayon::ThreadPoolBuilder::new().num_threads(10).build()?;
    let configurations: Vec<_> = discovered.configurations.values().enumerate().collect();
    pool.install(|| configurations.par_iter().for_each(|(index, native)| {
        let config = &native.configuration;
        let run_case = || -> Result<String> {
            let case = config.id.rsplit_once('(').context("configuration identity")?.0;
            let case_output = dir.join(format!("case-{index}.tsv"));
            std::fs::write(&case_output, "")?;
            let expression = format!("^TestFullOracle$/{}", case.split('/').map(regex_literal).collect::<Vec<_>>().join("/"));
            let status = run_bounded(Command::new(&binary).current_dir(upstream.join("internal/testrunner"))
                .arg(format!("-test.run={expression}"))
                .env("GOMEMLIMIT", "256MiB")
                .env("FULL_ORACLE_CONFIGURATION", &config.id)
                .env("FULL_ORACLE_OUTPUT", &case_output), &dir.join(format!("case-{index}")), Duration::from_secs(30))?;
            std::fs::write(dir.join(format!("case-{index}.status")), &status)?;
            if status != "success" { bail!("{status}"); }
            let text = std::fs::read_to_string(case_output)?;
            let records: Vec<_> = text.lines().filter(|line| line.split('\t').count() == 7 && line.starts_with("52\t")).collect();
            if records.len() != 1 { bail!("expected one completed configuration record, got {}", records.len()); }
            Ok(format!("{}\n", records[0]))
        };
        let row = match run_case() {
            Ok(row) => row,
            Err(error) => record(&["R", &config.id, "process failure", &format!("{error:#}"), "", "", ""]),
        };
        // Publish as soon as each isolated process settles, not after a whole Vec.
        let mut file = checkpoint.lock().expect("checkpoint mutex");
        file.write_all(row.as_bytes()).expect("native checkpoint write");
        file.sync_data().expect("native checkpoint durability");
    }));
    read_population(&output)
}

/// Run an independent, fresh-process actual producer. Required output files
/// must exist even for empty expectations; absence is never a pass. The producer
/// receives source, native settings file, and output directory as positional
/// arguments. It must use compiler diagnostics and an independent type walker.
pub fn actual_artifacts(producer: &Path, config: &Configuration, dir: &Path) -> Result<Artifacts> {
    std::fs::create_dir_all(dir)?;
    let dir = std::fs::canonicalize(dir)?;
    let settings = dir.join("settings.txt");
    std::fs::write(&settings, &config.settings)?;
    let status = run_bounded(Command::new(producer).arg(&config.source).arg(settings).arg(&dir)
        .env("FULL_ORACLE_ID", &config.id), &dir.join("actual"), Duration::from_secs(30))?;
    std::fs::write(dir.join("status"), &status)?;
    if status != "success" { bail!("actual producer {status}"); }
    Ok(Artifacts {
        diagnostics: std::fs::read_to_string(dir.join("diagnostics.semantic"))?,
        errors: std::fs::read(dir.join("errors.txt"))?,
        types: std::fs::read(dir.join("types"))?,
    })
}

/// File-backed output prevents pipe-buffer deadlocks; kill then reap on deadline.
pub fn run_bounded(command: &mut Command, log: &Path, deadline: Duration) -> Result<String> {
    let stdout = std::fs::File::create(log.with_extension("stdout"))?;
    let stderr = std::fs::File::create(log.with_extension("stderr"))?;
    let mut child = command.stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr)).spawn()?;
    let started = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(if status.success() { "success".into() } else { format!("exit {status}") });
        }
        if started.elapsed() >= deadline {
            child.kill()?;
            let status = child.wait()?;
            return Ok(format!("timeout after {} ms; reaped {status}", deadline.as_millis()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

fn record(fields: &[&str]) -> String {
    let mut row = fields.iter().map(|field| hex(field)).collect::<Vec<_>>().join("\t");
    row.push('\n');
    row
}

fn regex_literal(text: &str) -> String {
    let mut result = String::from("^");
    for ch in text.chars() {
        if ".+*?()|[]{}^$\\".contains(ch) { result.push('\\'); }
        result.push(ch);
    }
    result.push('$');
    result
}

fn json_path(path: &Path) -> String {
    format!("\"{}\"", path.to_string_lossy().replace('\\', "\\\\").replace('"', "\\\""))
}

/// Encode protocol bytes without escaping ambiguities.
pub fn hex(text: &str) -> String {
    use std::fmt::Write;
    let mut output = String::with_capacity(text.len() * 2);
    for byte in text.bytes() { write!(&mut output, "{byte:02x}").expect("writing string"); }
    output
}

/// Decode a UTF-8 hex protocol field, rejecting malformed input.
pub fn unhex(text: &str) -> Result<String> {
    Ok(String::from_utf8(unhex_bytes(text)?)?)
}

fn unhex_bytes(text: &str) -> Result<Vec<u8>> {
    if !text.len().is_multiple_of(2) { bail!("odd hex field length"); }
    let mut bytes = Vec::with_capacity(text.len() / 2);
    for pair in text.as_bytes().chunks_exact(2) {
        let digit = |b: u8| -> Result<u8> {
            match b { b'0'..=b'9' => Ok(b-b'0'), b'a'..=b'f' => Ok(b-b'a'+10), _ => bail!("invalid hex digit") }
        };
        bytes.push(digit(pair[0])? * 16 + digit(pair[1])?);
    }
    Ok(bytes)
}
