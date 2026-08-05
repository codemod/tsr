//! The performance gate: run both sides, compare, emit artifacts, decide.
//!
//! Implements [ADR-0009](../../docs/adr/0009-performance-gate.md). The comparison
//! is against typescript-go at the pinned commit, never against our own history,
//! for the reason that ADR gives: a self-regression ratchet fires on legitimate
//! correctness work, and a gate whose correct response is routinely "override"
//! trains people to override it.
//!
//! Emits `perf-results.json` (machine-readable, for trend tracking) and
//! `perf-summary.md` (for a PR comment or job summary), then exits non-zero if a
//! gated ratio is out of band.

// Every float here is a ratio or a megabyte figure printed for a human, and the
// inputs are nanosecond and kibibyte counts far below where `f64` loses anything.
#![allow(clippy::cast_precision_loss)]

use std::{collections::BTreeMap, fmt::Write as _, fs, path::Path, process::Command};

use anyhow::{Context, Result, bail};

/// Ceiling on wall-clock ratio (ours ÷ typescript-go's).
///
/// Deliberately loose. GitHub's shared runners are noisy — neighbours on the same
/// host, no CPU pinning, variable clock — and a tight wall-clock gate on that
/// hardware measures the runner, not the code. Local measurement at the time of
/// writing is 0.71–0.73 on the large fixtures, so 1.10 leaves a wide margin and
/// still fails loudly on a real regression.
///
/// The number to watch is the trend in `perf-results.json`, not this threshold.
const MAX_WALL_RATIO: f64 = 1.10;

/// Whether parse+bind ratios gate, or are only reported.
///
/// `true` since 2026-08-04, when `bd tsr-y4u.2` landed the control-flow graph —
/// the condition this constant was introduced waiting for. Until then a
/// parse+bind ratio compared our partial binder against upstream's complete one
/// and flattered us by exactly the amount of work we had not written; gating on
/// it would have locked in a number that must get worse as the binder is
/// finished, which is the failure mode ADR-0009 exists to avoid.
///
/// Some of upstream's binder work is still unported — destructuring symbols,
/// `export` routing, strict-mode diagnostics — so the ratio still has room to
/// worsen legitimately. It is gated anyway because the flow graph was the
/// dominant piece (it was ~95% of upstream's binder cost on `checker.ts`) and the
/// measured headroom is roughly 2×: 0.52 against `MAX_WALL_RATIO`'s 1.10. See
/// `docs/architecture/binder.md` for what remains.
const BIND_IS_GATED: bool = true;

/// Ceiling on peak-RSS ratio (ours ÷ typescript-go's).
///
/// Tight, because peak RSS is close to deterministic: it is a high-water mark over
/// the same allocations on the same input, with none of wall clock's sensitivity
/// to scheduling. Local measurement is 0.57, and using *more* memory than the
/// implementation we are replacing would negate the main argument for the port —
/// see `docs/architecture/performance.md`.
const MAX_RSS_RATIO: f64 = 1.00;

/// Fixtures small enough that timing them on a shared runner is meaningless.
///
/// `empty.ts` measures fixed per-file overhead in hundreds of nanoseconds, which
/// is below the noise floor of a cloud runner. It is still reported; it just does
/// not gate.
const NOT_GATED: &[&str] = &["empty.ts"];

pub fn run(root: &Path) -> Result<()> {
    let ours = rust_parse_ns(root)?;
    let theirs = go_parse_ns(root)?;
    let our_rss = rust_peak_rss(root)?;
    let their_rss = go_peak_rss(root)?;

    let mut rows = Vec::new();
    for (fixture, tsgo_ns) in &theirs {
        let Some(&tsr_ns) = ours.get(fixture) else {
            bail!(
                "fixture {fixture:?} was benchmarked by typescript-go but not by us; the two \
                 fixture lists have drifted apart and the comparison is no longer like for like"
            );
        };
        rows.push(Row {
            fixture: fixture.clone(),
            tsgo_ns: *tsgo_ns,
            tsr_ns,
            gated: !NOT_GATED.contains(&fixture.as_str()),
        });
    }
    if rows.is_empty() {
        bail!("no fixtures were compared; both benchmarks produced nothing");
    }

    // Parse+bind, reported rather than gated; see `BIND_IS_GATED`.
    let bind_rows = match (rust_bind_ns(root), go_bind_ns(root)) {
        (Ok(ours), Ok(theirs)) => theirs
            .iter()
            .filter_map(|(fixture, tsgo_ns)| {
                ours.get(fixture).map(|tsr_ns| Row {
                    fixture: fixture.clone(),
                    tsgo_ns: *tsgo_ns,
                    tsr_ns: *tsr_ns,
                    gated: BIND_IS_GATED && !NOT_GATED.contains(&fixture.as_str()),
                })
            })
            .collect(),
        // A missing Go toolchain or a fixture drift should not fail the parser
        // gate, which is the part that is actually enforced.
        (ours, theirs) => {
            if let Err(error) = ours {
                println!("parse+bind (ours) unavailable: {error:#}");
            }
            if let Err(error) = theirs {
                println!("parse+bind (typescript-go) unavailable: {error:#}");
            }
            Vec::new()
        }
    };

    // Parse+bind+check, ours only — there is no upstream half yet, so this is a
    // trend line rather than a comparison. See `rust_check_ns`. A failure here
    // must not fail the run: it is the one row with nothing to gate on.
    let check_rows = match rust_check_ns(root) {
        Ok(ours) => ours,
        Err(error) => {
            println!("parse+bind+check (ours) unavailable: {error:#}");
            BTreeMap::new()
        }
    };

    let rss_ratio = our_rss as f64 / their_rss as f64;
    write_artifacts(root, &rows, &bind_rows, &check_rows, our_rss, their_rss, rss_ratio)?;

    // Report everything before failing, so one run tells you about every breach
    // rather than only the first.
    let mut failures = Vec::new();
    // Labelled by which measurement it came from. The two collections were
    // chained bare until 2026-08-05, which made a failure unattributable: the
    // fixture names are the same in both, so `checker.ts: wall-clock ratio 1.320`
    // could be the parse gate or the parse+bind gate, and telling them apart
    // meant matching the reported tsgo figure against the table in
    // `docs/architecture/performance.md` by hand.
    let labelled = rows.iter().map(|row| ("parse", row));
    let labelled = labelled.chain(bind_rows.iter().map(|row| ("parse+bind", row)));
    for (measurement, row) in labelled {
        if row.gated && row.ratio() > MAX_WALL_RATIO {
            failures.push(format!(
                "{measurement} {}: wall-clock ratio {:.3} exceeds {MAX_WALL_RATIO:.2} \
                 ({:.3} ms vs {:.3} ms)",
                row.fixture,
                row.ratio(),
                row.tsr_ns / 1e6,
                row.tsgo_ns / 1e6
            ));
        }
    }
    if rss_ratio > MAX_RSS_RATIO {
        failures.push(format!(
            "peak RSS ratio {rss_ratio:.3} exceeds {MAX_RSS_RATIO:.2} ({our_rss} KiB vs \
             {their_rss} KiB)"
        ));
    }

    if failures.is_empty() {
        println!("performance gate passed");
        return Ok(());
    }
    for failure in &failures {
        println!("::error::{failure}");
    }
    bail!("{} performance gate failure(s)", failures.len())
}

struct Row {
    fixture: String,
    tsgo_ns: f64,
    tsr_ns: f64,
    gated: bool,
}

impl Row {
    fn ratio(&self) -> f64 {
        self.tsr_ns / self.tsgo_ns
    }
}

/// Our parse benchmark, JSON mode.
///
/// The `-jsdoc` arm is the one compared: typescript-go does not build JSDoc nodes
/// for `.ts`/`.tsx`, so including ours would compare different amounts of work.
/// See [ADR-0010](../../docs/adr/0010-jsdoc-is-a-parse-option.md).
fn rust_parse_ns(root: &Path) -> Result<BTreeMap<String, f64>> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["bench", "-q", "-p", "tsr-parser", "--bench", "parse", "--", "--json"])
        .output()
        .context("running the Rust parse benchmark")?;
    if !output.status.success() {
        bail!("the Rust parse benchmark failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find('[').context("no JSON array in the benchmark output")?;
    let end = text.rfind(']').context("unterminated JSON array in the benchmark output")? + 1;
    let parsed: serde_json::Value =
        serde_json::from_str(&text[start..end]).context("parsing the benchmark's JSON")?;

    let mut out = BTreeMap::new();
    for entry in parsed.as_array().context("benchmark JSON is not an array")? {
        let name = entry["name"].as_str().context("benchmark entry has no name")?;
        let ns = entry["ns_per_op_without_jsdoc"]
            .as_f64()
            .or_else(|| entry["ns_per_op"].as_f64())
            .context("benchmark entry has no ns_per_op")?;
        out.insert(name.to_string(), ns);
    }
    Ok(out)
}

/// typescript-go's own `BenchmarkParse`, reused rather than reimplemented.
fn go_parse_ns(root: &Path) -> Result<BTreeMap<String, f64>> {
    let output = Command::new("go")
        .current_dir(root.join("vendor/typescript-go"))
        .args([
            "test",
            "-run",
            "^$",
            "-bench",
            "BenchmarkParse",
            "-benchmem",
            "-cpu",
            "1",
            "-benchtime",
            "2s",
            "./internal/parser/",
        ])
        .output()
        .context("running typescript-go's BenchmarkParse (is the Go toolchain installed?)")?;
    if !output.status.success() {
        bail!("typescript-go's benchmark failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }

    let out = parse_go_bench(&String::from_utf8_lossy(&output.stdout), "BenchmarkParse/");
    if out.is_empty() {
        bail!("parsed no benchmark results from Go's output");
    }
    Ok(out)
}

/// Pull `name -> ns/op` out of `go test -bench` output.
fn parse_go_bench(text: &str, prefix: &str) -> BTreeMap<String, f64> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        // `BenchmarkParse/checker.ts    109   32721718 ns/op   ...`
        let Some(rest) = line.strip_prefix(prefix) else { continue };
        let mut fields = rest.split_whitespace();
        let Some(name) = fields.next() else { continue };
        // Go appends `-1` for `-cpu 1`.
        let name = name.rsplit_once('-').map_or(name, |(head, _)| head);
        let fields: Vec<&str> = fields.collect();
        let Some(position) = fields.iter().position(|f| *f == "ns/op") else { continue };
        let Some(value) = position.checked_sub(1).and_then(|i| fields.get(i)) else { continue };
        if let Ok(ns) = value.parse::<f64>() {
            out.insert(name.to_string(), ns);
        }
    }
    out
}

/// Our parse+bind benchmark, JSON mode.
fn rust_bind_ns(root: &Path) -> Result<BTreeMap<String, f64>> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["bench", "-q", "-p", "tsr-binder", "--bench", "bind", "--", "--json"])
        .output()
        .context("running the Rust parse+bind benchmark")?;
    if !output.status.success() {
        bail!("the Rust parse+bind benchmark failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find('[').context("no JSON array in the benchmark output")?;
    let end = text.rfind(']').context("unterminated JSON array in the benchmark output")? + 1;
    let parsed: serde_json::Value =
        serde_json::from_str(&text[start..end]).context("parsing the benchmark's JSON")?;
    let mut out = BTreeMap::new();
    for entry in parsed.as_array().context("benchmark JSON is not an array")? {
        let name = entry["name"].as_str().context("benchmark entry has no name")?;
        let ns = entry["ns_per_op_parse_and_bind"]
            .as_f64()
            .context("benchmark entry has no ns_per_op_parse_and_bind")?;
        out.insert(name.to_string(), ns);
    }
    Ok(out)
}

/// typescript-go's parse+bind, from the harness we keep outside the submodule.
fn go_bind_ns(root: &Path) -> Result<BTreeMap<String, f64>> {
    let source = root.join("benches/go/bind_test.go");
    let destination = root.join("vendor/typescript-go/internal/binder/zz_tsr_bind_test.go");
    fs::copy(&source, &destination)
        .with_context(|| format!("copying {} into the submodule", source.display()))?;
    let result = Command::new("go")
        .current_dir(root.join("vendor/typescript-go"))
        .args([
            "test",
            "-run",
            "^$",
            "-bench",
            "BenchmarkTsrParseBind",
            "-cpu",
            "1",
            "-benchtime",
            "2s",
            "./internal/binder/",
        ])
        .output()
        .context("running the Go parse+bind benchmark");
    // Removed whatever happened, so a failure does not leave the pin dirty.
    let _ = fs::remove_file(&destination);
    let output = result?;
    if !output.status.success() {
        bail!("the Go parse+bind benchmark failed:\n{}", String::from_utf8_lossy(&output.stdout));
    }
    Ok(parse_go_bench(&String::from_utf8_lossy(&output.stdout), "BenchmarkTsrParseBind/"))
}

/// Our parse+bind+check benchmark, JSON mode.
///
/// **Tracked, not compared.** There is no `tsgo_ns` for this row and no ratio,
/// because the upstream half does not exist yet: `func Benchmark` across
/// `internal/` finds no checker benchmark, and writing one needs a full
/// `Program` rather than the single-file setup `bind_test.go` gets away with.
/// See `bd` for that follow-up.
///
/// It is recorded anyway because the trend is the point — the number that
/// matters is this row moving in `perf-results.json` between runs, which is
/// exactly what a checker under active construction needs and what nothing
/// currently watches.
///
/// **It must not become a gate by default when the Go side lands.** This port
/// answers a fraction of what upstream's checker does, and an unported form
/// returns `errorType` immediately and costs nothing — so the ratio flatters us
/// by precisely the amount of work not yet written, and gets *worse* as the
/// checker gets *better*. That is the trap [`BIND_IS_GATED`] documents, and the
/// checker is deeper into it than the binder ever was.
fn rust_check_ns(root: &Path) -> Result<BTreeMap<String, f64>> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["bench", "-q", "-p", "tsr-checker", "--bench", "check", "--", "--json"])
        .output()
        .context("running the Rust parse+bind+check benchmark")?;
    if !output.status.success() {
        bail!("the Rust check benchmark failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let start = text.find('[').context("no JSON array in the benchmark output")?;
    let end = text.rfind(']').context("unterminated JSON array in the benchmark output")? + 1;
    let parsed: serde_json::Value =
        serde_json::from_str(&text[start..end]).context("parsing the benchmark's JSON")?;
    let mut out = BTreeMap::new();
    for entry in parsed.as_array().context("benchmark JSON is not an array")? {
        let name = entry["name"].as_str().context("benchmark entry has no name")?;
        let ns = entry["ns_per_op_parse_bind_check"]
            .as_f64()
            .context("benchmark entry has no ns_per_op_parse_bind_check")?;
        out.insert(name.to_string(), ns);
    }
    Ok(out)
}

/// Peak RSS in KiB, ours.
fn rust_peak_rss(root: &Path) -> Result<u64> {
    let output = Command::new("cargo")
        .current_dir(root)
        .args(["run", "-q", "--release", "-p", "tsr-parser", "--example", "rss"])
        .output()
        .context("running the Rust RSS harness")?;
    if !output.status.success() {
        bail!("the Rust RSS harness failed:\n{}", String::from_utf8_lossy(&output.stderr));
    }
    parse_labelled(&String::from_utf8_lossy(&output.stdout), "loaded KiB:")
}

/// Peak RSS in KiB, typescript-go's.
///
/// The harness is copied into the submodule for the run and removed afterwards, so
/// the pin stays a clean checkout of upstream. `internal/parser` cannot be
/// imported from outside its module, so there is no way to do this from a file
/// that lives permanently outside it.
fn go_peak_rss(root: &Path) -> Result<u64> {
    let source = root.join("benches/go/rss_test.go");
    let destination = root.join("vendor/typescript-go/internal/parser/zz_tsr_rss_test.go");
    fs::copy(&source, &destination)
        .with_context(|| format!("copying {} into the submodule", source.display()))?;

    let result = Command::new("go")
        .current_dir(root.join("vendor/typescript-go"))
        .args(["test", "-run", "TestTsrPeakRSS", "-v", "./internal/parser/"])
        .output()
        .context("running the Go RSS harness");

    // Remove it whatever happened, so a failure does not leave the submodule dirty
    // and turn the next `git status` into a puzzle.
    let _ = fs::remove_file(&destination);

    let output = result?;
    if !output.status.success() {
        bail!("the Go RSS harness failed:\n{}", String::from_utf8_lossy(&output.stdout));
    }
    parse_labelled(&String::from_utf8_lossy(&output.stdout), "loaded KiB:")
}

fn parse_labelled(text: &str, label: &str) -> Result<u64> {
    text.lines()
        .find_map(|line| line.trim().strip_prefix(label))
        .and_then(|rest| rest.trim().parse().ok())
        .with_context(|| format!("no {label:?} line in:\n{text}"))
}

fn write_artifacts(
    root: &Path,
    rows: &[Row],
    bind_rows: &[Row],
    check_rows: &BTreeMap<String, f64>,
    our_rss: u64,
    their_rss: u64,
    rss_ratio: f64,
) -> Result<()> {
    let fixtures: Vec<serde_json::Value> = rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "fixture": row.fixture,
                "tsgo_ns_per_op": row.tsgo_ns,
                "tsr_ns_per_op": row.tsr_ns,
                "ratio": row.ratio(),
                "gated": row.gated,
            })
        })
        .collect();

    let bind_fixtures: Vec<serde_json::Value> = bind_rows
        .iter()
        .map(|row| {
            serde_json::json!({
                "fixture": row.fixture,
                "tsgo_ns_per_op": row.tsgo_ns,
                "tsr_ns_per_op": row.tsr_ns,
                "ratio": row.ratio(),
                "gated": row.gated,
            })
        })
        .collect();

    let check_fixtures: Vec<serde_json::Value> = check_rows
        .iter()
        .map(|(fixture, ns)| serde_json::json!({ "fixture": fixture, "tsr_ns_per_op": ns }))
        .collect();

    let json = serde_json::json!({
        "pinned_commit": pinned_commit(root),
        "wall_clock": { "max_ratio": MAX_WALL_RATIO, "fixtures": fixtures },
        "parse_and_bind": {
            "gated": BIND_IS_GATED,
            "note": "our binder has no control-flow graph and upstream's does, so \
                     this compares a partial binder against a complete one",
            "fixtures": bind_fixtures,
        },
        "parse_bind_check": {
            "gated": false,
            "note": "ours only — upstream has no checker benchmark, so this is a \
                     trend line and not a comparison. It must not become a gate \
                     by default: an unported form answers errorType and costs \
                     nothing, so a ratio would flatter us by the amount of work \
                     not yet written and would worsen as the checker improves.",
            "fixtures": check_fixtures,
        },
        "peak_rss_kib": {
            "max_ratio": MAX_RSS_RATIO,
            "tsgo": their_rss,
            "tsr": our_rss,
            "ratio": rss_ratio,
        },
    });
    fs::write(root.join("perf-results.json"), serde_json::to_string_pretty(&json)?)
        .context("writing perf-results.json")?;

    let mut md = String::from("## Performance vs typescript-go\n\n");
    md.push_str("Ratios are ours ÷ upstream's, so **lower is better** and 1.00 is parity.\n\n");
    md.push_str("| Fixture | tsgo | tsr | ratio | |\n|---|---:|---:|---:|:--|\n");
    for row in rows {
        let verdict = if !row.gated {
            "not gated"
        } else if row.ratio() > MAX_WALL_RATIO {
            "❌"
        } else {
            "✅"
        };
        writeln!(
            md,
            "| `{}` | {:.3} ms | {:.3} ms | {:.3}× | {verdict} |",
            row.fixture,
            row.tsgo_ns / 1e6,
            row.tsr_ns / 1e6,
            row.ratio()
        )?;
    }
    write!(
        md,
        "\n| Peak RSS | tsgo | tsr | ratio | |\n|---|---:|---:|---:|:--|\n\
         | holding every parsed tree | {:.1} MB | {:.1} MB | {rss_ratio:.3}× | {} |\n",
        their_rss as f64 / 1024.0,
        our_rss as f64 / 1024.0,
        if rss_ratio > MAX_RSS_RATIO { "❌" } else { "✅" }
    )?;
    write!(
        md,
        "\nGates: wall clock ≤ {MAX_WALL_RATIO:.2}× (loose — shared runners are noisy), \
         peak RSS ≤ {MAX_RSS_RATIO:.2}× (tight — RSS is near-deterministic). \
         See [ADR-0009](docs/adr/0009-performance-gate.md).\n"
    )?;
    if !bind_rows.is_empty() {
        md.push_str("\n| Parse + bind | tsgo | tsr | ratio |\n|---|---:|---:|---:|\n");
        for row in bind_rows {
            writeln!(
                md,
                "| `{}` | {:.3} ms | {:.3} ms | {:.3}× |",
                row.fixture,
                row.tsgo_ns / 1e6,
                row.tsr_ns / 1e6,
                row.ratio()
            )?;
        }
        md.push_str(
            "\n**Parse+bind is reported, not gated.** Our binder does not build the \
             control-flow graph and upstream's does, so this compares a partial binder \
             against a complete one and flatters us by the amount of work we have not \
             written. It becomes a gate when the flow graph lands.\n",
        );
    }
    if !check_rows.is_empty() {
        md.push_str("\n| Parse + bind + check | tsr |\n|---|---:|\n");
        for (fixture, ns) in check_rows {
            writeln!(md, "| `{fixture}` | {:.3} ms |", ns / 1e6)?;
        }
        md.push_str(
            "\n**Ours only, and tracked rather than compared.** typescript-go has no \
             checker benchmark to compare against, so there is no ratio here — the \
             number that matters is this row moving between runs. It must not become \
             a gate when an upstream half lands: an unported form answers `errorType` \
             immediately and costs nothing, so the ratio would flatter us by exactly \
             the work not yet written, and would get *worse* as the checker gets \
             *better*.\n",
        );
    }
    fs::write(root.join("perf-summary.md"), md).context("writing perf-summary.md")?;
    Ok(())
}

/// The submodule commit the comparison is against, so a result file is
/// interpretable months later — the ratios move when the pin moves.
///
/// The full SHA, not `--short`: git scales the abbreviation with the object count,
/// so a shallow CI clone and a full local one disagree about the same commit.
fn pinned_commit(root: &Path) -> String {
    Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(root.join("vendor/typescript-go"))
        .output()
        .ok()
        .filter(|out| out.status.success())
        .map_or_else(
            || "unknown".to_string(),
            |out| String::from_utf8_lossy(&out.stdout).trim().to_string(),
        )
}
