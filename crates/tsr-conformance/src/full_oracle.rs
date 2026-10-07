//! Exact compiler-runner oracle. Native operations execute through a read-only
//! Go overlay; actual output must come from a lossless compiler producer.
//!
//! Unlike the legacy suites, this population includes both native trees, every
//! configuration, native exclusions, divergences, empty output and failures.
use std::{collections::BTreeMap, path::{Path, PathBuf}, process::Command};
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
    let result = Command::new(&binary).current_dir(upstream.join("internal/testrunner"))
        .arg(format!("-test.run={run}"))
        .env("FULL_ORACLE_DISCOVERY", "1")
        .env("FULL_ORACLE_OUTPUT", &output).output()?;
    std::fs::write(dir.join("discovery.stdout"), &result.stdout)?;
    std::fs::write(dir.join("discovery.stderr"), &result.stderr)?;
    if std::fs::metadata(&output)?.len() == 0 {
        bail!("native discovery failed; see {}", dir.display());
    }
    let discovered = read_population(&output)?;
    let mut cases = BTreeMap::new();
    for config in discovered.configurations.values() {
        let case = config.configuration.id.rsplit_once('(').context("configuration identity")?.0;
        cases.insert(case.to_string(), ());
    }
    // One fresh native process per source case bounds checker lifetime. A process
    // crash cannot truncate the population, which was published before any checks.
    use rayon::prelude::*;
    let pool = rayon::ThreadPoolBuilder::new().num_threads(2).build()?;
    let rows: Vec<Result<String>> = pool.install(|| cases.keys().enumerate().collect::<Vec<_>>()
        .par_iter().map(|(index, case)| {
            let case_output = dir.join(format!("case-{index}.tsv"));
            std::fs::write(&case_output, "")?;
            let expression = format!("^TestFullOracle$/{}", case.split('/').map(regex_literal).collect::<Vec<_>>().join("/"));
            let result = Command::new(&binary).current_dir(upstream.join("internal/testrunner"))
                .arg(format!("-test.run={expression}"))
                .env("GOMEMLIMIT", "256MiB")
                .env("FULL_ORACLE_OUTPUT", &case_output).output()?;
            std::fs::write(dir.join(format!("case-{index}.log")), &result.stdout)?;
            std::fs::write(dir.join(format!("case-{index}.stderr")), &result.stderr)?;
            let text = std::fs::read_to_string(case_output)?;
            // Keep only atomically completed records; incomplete publication after
            // a signal leaves the discovered configuration as NativeFailure.
            let mut complete = String::new();
            for line in text.lines() {
                if line.split('\t').count() == 7 && line.starts_with("52\t") {
                    complete.push_str(line); complete.push('\n');
                }
            }
            Ok(complete)
        }).collect());
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new().append(true).open(&output)?;
    for row in rows { file.write_all(row?.as_bytes())?; }
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
    let output = Command::new(producer).arg(&config.source).arg(settings).arg(&dir)
        .env("FULL_ORACLE_ID", &config.id).output()?;
    std::fs::write(dir.join("stdout"), &output.stdout)?;
    std::fs::write(dir.join("stderr"), &output.stderr)?;
    if !output.status.success() { bail!("actual producer exited {}", output.status); }
    Ok(Artifacts {
        diagnostics: std::fs::read_to_string(dir.join("diagnostics.semantic"))?,
        errors: std::fs::read(dir.join("errors.txt"))?,
        types: std::fs::read(dir.join("types"))?,
    })
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
