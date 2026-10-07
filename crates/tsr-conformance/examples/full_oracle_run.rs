#[path = "../src/full_oracle.rs"]
mod full_oracle;
use anyhow::{Context, Result, ensure};
use full_oracle::{hash, hex, unhex, write_atomic};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::Duration,
};
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    ensure!(
        args.len() == 2 || args.len() == 3,
        "usage: full_oracle_run REPO REPORT_DIRECTORY [PRIOR_REPORT]"
    );
    let prior = args.get(2).map(PathBuf::from);
    let root = Path::new(&args[0]).canonicalize()?;
    let report = Path::new(&args[1]);
    fs::create_dir_all(report)?;
    let report = report.canonicalize()?;
    let native = root.join("vendor/typescript-go");
    let rev = Command::new("git").arg("-C").arg(&native).args(["rev-parse", "HEAD"]).output()?;
    ensure!(String::from_utf8(rev.stdout)?.trim() == full_oracle::NATIVE, "wrong native revision");
    // The native revision binds bundled libraries and harness sources only when clean.
    let clean = Command::new("git")
        .arg("-C")
        .arg(&native)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()?;
    ensure!(clean.status.success() && clean.stdout.is_empty(), "dirty native inputs");
    let corpus_repo = native.join("_submodules/TypeScript");
    let clean = Command::new("git")
        .arg("-C")
        .arg(&corpus_repo)
        .args(["status", "--porcelain", "--untracked-files=no"])
        .output()?;
    ensure!(clean.status.success() && clean.stdout.is_empty(), "dirty corpus support inputs");
    let source = Command::new("git").arg("-C").arg(&root).args(["rev-parse", "HEAD"]).output()?;
    let oracle_revision = String::from_utf8(source.stdout)?.trim().to_string();
    let requested_source = std::env::var("TSR_ORACLE_CHECKER_SOURCE")
        .unwrap_or_else(|_| "c8185606e3b972d59d345b6e45d789586d993af8".into());
    let resolved = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args(["rev-parse", &format!("{requested_source}^{{commit}}")])
        .output()?;
    ensure!(resolved.status.success(), "checker source commit unavailable");
    let source = String::from_utf8(resolved.stdout)?.trim().to_string();
    let changed = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args([
            "diff",
            "--name-only",
            &source,
            "--",
            ".",
            ":(exclude)crates/tsr-conformance/src/full_oracle*",
            ":(exclude)crates/tsr-conformance/examples/full_oracle*",
            ":(exclude)docs/parity/notes/full-oracle.md",
            ":(exclude).beads",
        ])
        .output()?;
    ensure!(
        changed.status.success() && changed.stdout.is_empty(),
        "compiler inputs differ from declared checker source"
    );
    let src = root.join("crates/tsr-conformance/src");
    let overlay = report.join("overlay.json");
    // JSON paths are ASCII on the Box. Reject quotes rather than generating invalid JSON.
    let paths = [
        native.join("internal/testrunner/full_oracle_test.go"),
        src.join("full_oracle_native.go"),
        native.join("internal/testutil/tsbaseline/full_oracle.go"),
        src.join("full_oracle_native_types.go"),
    ];
    for p in &paths {
        ensure!(!p.to_string_lossy().contains(['"', '\\']), "unsupported overlay path");
    }
    write_atomic(
        &overlay,
        &format!(
            "{{\"Replace\":{{\"{}\":\"{}\",\"{}\":\"{}\"}}}}",
            paths[0].display(),
            paths[1].display(),
            paths[2].display(),
            paths[3].display()
        ),
    )?;
    let built_native = report.join("native-oracle-build");
    let status = Command::new("go")
        .current_dir(&native)
        .arg("test")
        .arg(format!("-overlay={}", overlay.display()))
        .args(["-c", "-o"])
        .arg(&built_native)
        .arg("./internal/testrunner")
        .status()?;
    ensure!(status.success(), "native build failed");
    // Build from the verified checkout before binding its executable identity.
    let status = Command::new("cargo")
        .current_dir(&root)
        .args(["build", "--release", "-p", "tsr-conformance", "--example", "full_oracle_actual"])
        .status()?;
    ensure!(status.success(), "source-bound actual producer build failed");
    let built_actual = std::env::current_exe()?.with_file_name("full_oracle_actual");
    ensure!(built_actual.is_file(), "build full_oracle_actual first");
    let native_hash = hash(&built_native)?;
    let actual_hash = hash(&built_actual)?;
    let binary = full_oracle::freeze_worker(&built_native, &report, &native_hash)?;
    let actual = full_oracle::freeze_worker(&built_actual, &report, &actual_hash)?;
    fs::remove_file(&built_native)?;
    let producer_sources =
        ["full_oracle.rs", "full_oracle_native.go", "full_oracle_native_types.go"];
    let mut producer_receipt = format!(
        "checker_source\t{source}\nnative_binary\t{native_hash}\nactual_binary\t{actual_hash}\n"
    );
    for file in producer_sources {
        producer_receipt
            .push_str(&format!("producer_source\t{file}\t{}\n", hash(&src.join(file))?));
    }
    write_atomic(&report.join("producer-sources.tsv"), &producer_receipt)?;
    let corpus = native.join("_submodules/TypeScript/tests/cases");
    let plan = report.join("plan.tsv");
    let mut cmd = Command::new(&binary);
    cmd.current_dir(&native)
        .arg("-test.run=^TestFullOracle$")
        .env("TSR_ORACLE_MODE", "plan")
        .env("TSR_ORACLE_CORPUS", &corpus)
        .env("TSR_ORACLE_OUTPUT", &plan)
        .env("GOMAXPROCS", "1");
    let (ok, _) = full_oracle::run(
        &mut cmd,
        &report.join("plan.stdout"),
        &report.join("plan.stderr"),
        Duration::from_secs(120),
    )?;
    ensure!(ok || full_oracle::complete(&plan), "native enumeration incomplete");
    let plan_text = fs::read_to_string(&plan)?;
    let mut discovery_failures = std::collections::BTreeSet::new();
    let mut discovery_ledger = String::new();
    let tasks: Vec<_> = plan_text
        .lines()
        .filter(|line| *line != "COMPLETE")
        .map(|line| {
            let fields: Vec<_> = line.split('\t').collect();
            ensure!(fields.len() == 3, "invalid discovery record");
            let p = PathBuf::from(unhex(fields[0])?);
            let v = unhex(fields[1])?;
            match fields[2] {
                "COMPLETE" => {}
                "DISCOVERY_FAILED" => {
                    discovery_failures.insert(p.clone());
                }
                _ => anyhow::bail!("unknown discovery publication state"),
            }
            discovery_ledger.push_str(&format!("{}\t{}\t{}\n", fields[0], fields[1], fields[2]));
            Ok((p, v))
        })
        .collect::<Result<_>>()?;
    ensure!(!tasks.is_empty(), "empty corpus");
    write_atomic(&report.join("discovery.tsv"), &discovery_ledger)?;
    let mut source_hashes = std::collections::BTreeMap::new();
    let unique: std::collections::BTreeSet<_> = tasks.iter().map(|(p, _)| p).collect();
    let hashes = Command::new("sha256sum").args(unique).output()?;
    ensure!(hashes.status.success(), "input hashing failed");
    for line in String::from_utf8(hashes.stdout)?.lines() {
        let (h, p) = line.split_once("  ").context("hash record")?;
        source_hashes.insert(PathBuf::from(p), h.to_string());
    }
    let mut inputs = String::new();
    for (p, v) in &tasks {
        inputs.push_str(&format!(
            "{}\t{}\t{}\n",
            hex(&p.to_string_lossy()),
            hex(v),
            source_hashes[p]
        ));
    }
    write_atomic(&report.join("inputs.tsv"), &inputs)?;
    write_atomic(
        &report.join("manifest.tsv"),
        &format!(
            "source\t{source}\nnative\t{}\nnative_binary\t{native_hash}\nactual_binary\t{actual_hash}\ninputs\t{}\nworkers\t10\ndeadline_seconds\t60\nconfigured_cases\t{}\n",
            full_oracle::NATIVE,
            hash(&report.join("inputs.tsv"))?,
            tasks.len()
        ),
    )?;
    let inputs_hash = hash(&report.join("inputs.tsv"))?;
    let mut previous_right = std::collections::BTreeSet::new();
    if let Some(prior) = &prior {
        ensure!(
            hash(&prior.join("inputs.tsv"))? == inputs_hash,
            "prior input/configuration set differs"
        );
        let manifest = fs::read_to_string(prior.join("manifest.tsv"))?;
        let prior_source = manifest
            .lines()
            .find(|l| l.starts_with("source\t"))
            .context("prior source identity missing")?;
        write_atomic(&report.join("prior-source.tsv"), &format!("{prior_source}\n"))?;
        let mut ledger = String::new();
        for i in 0..tasks.len() {
            let dir = prior.join(format!("{i:05}"));
            let result = fs::read_to_string(dir.join("result.tsv"))?;
            if result.starts_with("RIGHT\t") {
                ensure!(
                    full_oracle::complete(&dir.join("native.tsv"))
                        && full_oracle::complete(&dir.join("actual.tsv")),
                    "incomplete prior RIGHT {i}"
                );
                previous_right.insert(i);
                ledger.push_str(&format!(
                    "{i}\t{}\t{}\n",
                    hash(&dir.join("native.tsv"))?,
                    hash(&dir.join("actual.tsv"))?
                ));
            }
        }
        write_atomic(&report.join("prior-right.tsv"), &ledger)?;
    }
    write_atomic(
        &report.join("oracle-revision.tsv"),
        &format!("oracle_revision\t{oracle_revision}\n"),
    )?;
    let tasks = Arc::new(tasks);
    let next = AtomicUsize::new(0);
    let failures = std::sync::Mutex::new(Vec::new());
    std::thread::scope(|scope| {
        for _ in 0..10 {
            let tasks = &tasks;
            let next = &next;
            let failures = &failures;
            let report = &report;
            let binary = &binary;
            let actual = &actual;
            let native = &native;
            let source = &source;
            let native_hash = &native_hash;
            let actual_hash = &actual_hash;
            let inputs_hash = &inputs_hash;
            let source_hashes = &source_hashes;
            let discovery_failures = &discovery_failures;
            scope.spawn(move || {
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    if i >= tasks.len() {
                        break;
                    }
                    let result = (|| -> Result<()> {
                        let (p, v) = &tasks[i];
                        let dir = report.join(format!("{i:05}"));
                        fs::create_dir_all(&dir)?;
                        if discovery_failures.contains(p) {
                            write_atomic(
                                &dir.join("result.tsv"),
                                "FAIL\tdiscovery-failure-native-configuration\n",
                            )?;
                            return Ok(());
                        }
                        let expected = dir.join("native.tsv");
                        let output = dir.join("actual.tsv");
                        let receipt = dir.join("native.receipt");
                        let bound = format!(
                            "{}\n{}\n{}\n{}\n{}\n",
                            full_oracle::NATIVE,
                            native_hash,
                            source_hashes[p],
                            hex(v),
                            inputs_hash
                        );
                        let mut native_ok = full_oracle::complete(&expected)
                            && fs::read_to_string(&receipt).is_ok_and(|r| {
                                hash(&expected).is_ok_and(|h| r == format!("{bound}{h}\n"))
                            });
                        let mut native_ms = 0;
                        if !native_ok {
                            if expected.exists() {
                                fs::remove_file(&expected)?;
                            }
                            let mut cmd = Command::new(binary);
                            cmd.current_dir(native)
                                .arg("-test.run=^TestFullOracle$")
                                .env("TSR_ORACLE_MODE", "actual")
                                .env("TSR_ORACLE_CASE", p)
                                .env("TSR_ORACLE_VARIANT", v)
                                .env("TSR_ORACLE_OUTPUT", &expected)
                                .env("GOMAXPROCS", "1");
                            let (ok, ms) = full_oracle::run(
                                &mut cmd,
                                &dir.join("native.stdout"),
                                &dir.join("native.stderr"),
                                Duration::from_secs(60),
                            )?;
                            native_ok = ok && full_oracle::complete(&expected);
                            native_ms = ms;
                            if native_ok {
                                write_atomic(&receipt, &format!("{bound}{}\n", hash(&expected)?))?;
                            }
                        }
                        if !native_ok {
                            write_atomic(
                                &dir.join("result.tsv"),
                                "FAIL\tnative-failure-or-deadline\n",
                            )?;
                            return Ok(());
                        }
                        if output.exists() {
                            fs::remove_file(&output)?;
                        }
                        let mut cmd = Command::new(actual);
                        cmd.arg(p).arg(&expected).arg(&output).env("RAYON_NUM_THREADS", "1");
                        let (ok, actual_ms) = full_oracle::run(
                            &mut cmd,
                            &dir.join("actual.stdout"),
                            &dir.join("actual.stderr"),
                            Duration::from_secs(60),
                        )?;
                        write_atomic(
                            &dir.join("actual.receipt"),
                            &format!(
                                "{source}\n{actual_hash}\n{bound}request\t{}\noutput\t{}\n",
                                hash(&expected)?,
                                if full_oracle::complete(&output) {
                                    hash(&output)?
                                } else {
                                    "INCOMPLETE".into()
                                }
                            ),
                        )?;
                        let status = if !ok || !full_oracle::complete(&output) {
                            "FAIL\tactual-failure-or-deadline".into()
                        } else {
                            let e = fs::read_to_string(&expected)?;
                            let a = fs::read_to_string(&output)?;
                            if full_oracle::records(&e) == full_oracle::records(&a) {
                                "RIGHT\texact".into()
                            } else {
                                format!("WRONG\t{}", full_oracle::cluster(&e, &a))
                            }
                        };
                        write_atomic(
                            &dir.join("result.tsv"),
                            &format!("{status}\t{native_ms}\t{actual_ms}\n"),
                        )?;
                        if i % 100 == 0 {
                            eprintln!("completed {i}/{}", tasks.len())
                        }
                        Ok(())
                    })();
                    if let Err(e) = result {
                        failures.lock().unwrap().push(format!("{i}\t{e:#}"));
                    }
                }
            });
        }
    });
    let mut counts = full_oracle::Counts::new();
    let mut right = 0;
    let mut prior_right_losses = 0;
    let mut prior_primary_losses = 0;
    let mut missing = 0;
    let mut ledger = String::new();
    for i in 0..tasks.len() {
        let result =
            fs::read_to_string(report.join(format!("{i:05}/result.tsv"))).unwrap_or_else(|_| {
                missing += 1;
                "FAIL\tinfrastructure".into()
            });
        if previous_right.contains(&i) && !result.starts_with("RIGHT\t") {
            prior_right_losses += 1;
        }
        if previous_right.contains(&i) {
            let dir = report.join(format!("{i:05}"));
            let primary = |s: &str| {
                s.lines()
                    .filter(|l| l.starts_with("D\t") || l.starts_with("T\t"))
                    .map(str::to_string)
                    .collect::<Vec<_>>()
            };
            let exact = match (
                fs::read_to_string(dir.join("native.tsv")),
                fs::read_to_string(dir.join("actual.tsv")),
            ) {
                (Ok(e), Ok(a))
                    if full_oracle::complete(&dir.join("native.tsv"))
                        && full_oracle::complete(&dir.join("actual.tsv")) =>
                {
                    primary(&e) == primary(&a)
                }
                _ => false,
            };
            if !exact {
                prior_primary_losses += 1;
            }
        }
        ledger.push_str(&format!(
            "{i}\t{}\t{}\t{}",
            hex(&tasks[i].0.to_string_lossy()),
            hex(&tasks[i].1),
            result
        ));
        if !ledger.ends_with('\n') {
            ledger.push('\n');
        }
        let p: Vec<_> = result.split('\t').collect();
        if p[0] == "RIGHT" {
            right += 1;
        } else {
            *counts.entry(p.get(1).unwrap_or(&"infrastructure").trim().to_string()).or_default() +=
                1;
        }
    }
    let mut summary = format!(
        "source\t{source}\nnative\t{}\nRIGHT\t{right}\nTOTAL\t{}\nPERFORMANCE\tunmet; no equivalent-complete-work median certificate\n",
        full_oracle::NATIVE,
        tasks.len()
    );
    for (key, n) in counts {
        summary.push_str(&format!("cluster\t{key}\t{n}\n"));
    }
    for error in failures.into_inner().unwrap() {
        summary.push_str(&format!("error\t{error}\n"));
    }
    summary.push_str(&format!("MISSING_ROWS\t{missing}\nPRIOR_RIGHT\t{}\nPRIOR_RIGHT_LOSSES\t{prior_right_losses}\nPRIOR_GATE\t{}\n", previous_right.len(), if prior.is_some() { "measured" } else { "unverified: no prior report supplied" }));
    summary.push_str(&format!("PRIOR_PRIMARY_LOSSES\t{prior_primary_losses}\nCONTRACT\tordered-primary-types-chains-related-metadata\nDISCOVERY_FAILED_SOURCES\t{}\nENUMERATED_CONFIGURATION_ROWS\t{}\n", discovery_failures.len(), tasks.len()-discovery_failures.len()));
    write_atomic(&report.join("results.tsv"), &ledger)?;
    write_atomic(&report.join("summary.tsv"), &summary)?;
    print!("{summary}");
    Ok(())
}
