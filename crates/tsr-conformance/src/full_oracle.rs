//! Source-bound configured oracle. Artifacts are ordered records, not baseline verdicts.
#![allow(dead_code, missing_docs)]
use anyhow::{Context, Result, bail};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

pub const NATIVE: &str = "5b1047d10d32e7d5b446be4de56b126ff42f82bb";
pub fn hex(s: &str) -> String {
    s.as_bytes().iter().map(|b| format!("{b:02x}")).collect()
}
pub fn unhex(s: &str) -> Result<String> {
    if s.len() % 2 != 0 {
        bail!("odd hex")
    };
    Ok(String::from_utf8(
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
            .collect::<std::result::Result<Vec<_>, _>>()?,
    )?)
}
pub fn hash(path: &Path) -> Result<String> {
    let out = Command::new("sha256sum").arg(path).output()?;
    if !out.status.success() {
        bail!("hash failed")
    };
    Ok(String::from_utf8(out.stdout)?.split_whitespace().next().context("missing hash")?.into())
}
fn path_name(s: &str) -> String {
    s.replace("/.src/", "").replace("/.lib/", "").replace("/.ts/", "")
}
fn diagnostic(out: &mut String, name: &str, d: &tsr_diagnostics::Diagnostic, text: Option<&str>) {
    let mut message = String::new();
    tsr_diagnostics::format::write_flattened_diagnostic_message(&mut message, d, "\n");
    let (start, len) =
        text.map_or((d.span.start as usize, (d.span.end - d.span.start) as usize), |s| {
            let start = d.span.start as usize;
            let end = d.span.end as usize;
            (s[..start].encode_utf16().count(), s[start..end].encode_utf16().count())
        });
    out.push_str(&format!(
        "D\t{}\t{start}\t{len}\t{}\t{}\t{}\n",
        hex(&path_name(name)),
        d.message.code(),
        d.category() as u8,
        hex(&message)
    ));
}

fn details(
    out: &mut String,
    d: &tsr_diagnostics::Diagnostic,
    path: &str,
    program: &tsr_compiler::Program<'_>,
    case: &tsr_conformance::TestCase,
) -> Result<()> {
    out.push_str(&format!(
        "M\t{path}\t{}\t{}\t{}\n",
        d.reports_unnecessary(),
        d.reports_deprecated(),
        d.skipped_on_no_emit()
    ));
    for (tag, rows) in [("C", d.message_chain()), ("R", d.related_information())] {
        for (i, child) in rows.iter().enumerate() {
            let child_path = format!("{path}/{tag}{i}");
            let name = child.file().map_or("", |f| f.file_name());
            let text = program.source_file(name).map(|f| f.text()).or_else(|| {
                case.files
                    .iter()
                    .find(|f| {
                        tsr_path::get_normalized_absolute_path(
                            &f.name,
                            case.current_directory.as_deref().unwrap_or("/.src"),
                        ) == name
                    })
                    .map(|f| f.content.as_str())
            });
            let (start, len) = if name.is_empty() {
                (child.span.start as usize, (child.span.end - child.span.start) as usize)
            } else {
                let text = text.context("related/chain diagnostic source unavailable")?;
                let start = child.span.start as usize;
                let end = child.span.end as usize;
                anyhow::ensure!(
                    text.is_char_boundary(start) && text.is_char_boundary(end),
                    "invalid diagnostic detail span"
                );
                (text[..start].encode_utf16().count(), text[start..end].encode_utf16().count())
            };
            let args: Vec<_> = child.args.iter().map(String::as_str).collect();
            out.push_str(&format!(
                "{tag}\t{child_path}\t{}\t{start}\t{len}\t{}\t{}\t{}\n",
                hex(&path_name(name)),
                child.message.code(),
                child.category() as u8,
                hex(&child.message.format(&args))
            ));
            details(out, child, &child_path, program, case)?;
        }
    }
    Ok(())
}

/// Fresh real Program and Checker execution. Requests carry options only; no expected nodes.
pub fn actual(source: &Path, request: &Path) -> Result<String> {
    let raw = fs::read_to_string(source)?;
    let mut case = tsr_conformance::TestCase::parse(
        "oracle",
        source.file_name().context("basename")?.to_str().context("UTF8")?,
        &raw,
    );
    for line in fs::read_to_string(request)?.lines() {
        let p: Vec<_> = line.split('\t').collect();
        if p[0] == "O" {
            case.options.insert(unhex(p[1])?, unhex(p[2])?);
        }
    }
    if let Some(error) = &case.error {
        bail!("{error}")
    }
    // Native harness defaults to /.src, not the positional baseline producer's /.
    case.current_directory = Some(tsr_path::get_normalized_absolute_path(
        case.current_directory.as_deref().unwrap_or(""),
        "/.src",
    ));
    let arena = tsr_core::Arena::new();
    let (program, config) =
        tsr_conformance::types_producer::program_and_config_for_case(&arena, &case);
    let mut checker = tsr_conformance::types_producer::configured_checker(&program);
    let files = program.source_files();
    checker.set_checked_files(
        files
            .iter()
            .enumerate()
            .filter(|(i, _)| {
                tsr_compiler::program_diagnostics::skip_type_checking(&program, *i, false).is_none()
            })
            .filter_map(|(_, f)| f.source_file().node_id),
    );
    for (i, f) in files.iter().enumerate() {
        if tsr_compiler::program_diagnostics::skip_type_checking(&program, i, false).is_none() {
            if let Some(id) = f.source_file().node_id {
                checker.check_source_file(
                    id,
                    tsr_checker::check::FileContext {
                        ambient: tsr_path::is_declaration_file_name(f.file_name()),
                        has_parse_errors: !f.diagnostics().is_empty(),
                    },
                );
            }
        }
    }
    let mut found = Vec::new();
    for (i, f) in files.iter().enumerate() {
        let name = f.file_name();
        found.extend(f.diagnostics().iter().cloned().map(|d| (name.to_string(), d)));
        if let Some(id) = f.source_file().node_id {
            found.extend(
                checker.js_syntax_diagnostics(id).into_iter().map(|(_, d)| (name.to_string(), d)),
            );
            let bound = tsr_binder::bind_into_with_jsdoc(
                tsr_binder::BindResult::empty(),
                &arena,
                f.source_file(),
                program.nodes(),
                tsr_binder::FileInfo { name, text: f.text() },
                &f.jsdoc().iter().collect::<Vec<_>>(),
            );
            let checks = checker
                .diagnostics()
                .iter()
                .filter(|(file, _)| *file == id)
                .map(|(_, d)| d.clone());
            found.extend(
                tsr_compiler::program_diagnostics::bind_and_check_diagnostics(
                    &program,
                    i,
                    bound.diagnostics(),
                    checks,
                )
                .into_iter()
                .map(|d| (name.to_string(), d)),
            );
            found.extend(
                tsr_compiler::program_diagnostics::include_processor_diagnostics(&program, i)
                    .into_iter()
                    .map(|d| (name.to_string(), d)),
            );
        }
        if (program.compiler_options().declaration.is_true()
            || program.compiler_options().composite.is_true())
            && program.compiler_options().isolated_declarations.is_true()
            && !tsr_path::is_declaration_file_name(name)
            && tsr_parser::ScriptKind::from_file_name(name) != tsr_parser::ScriptKind::Json
        {
            found.extend(
                tsr_dts::analyze_with_options(
                    f.source_file(),
                    program.nodes(),
                    tsr_dts::AnalysisOptions {
                        strict_null_checks: program
                            .compiler_options()
                            .strict_option_value(program.compiler_options().strict_null_checks),
                    },
                )
                .into_iter()
                .map(|d| (name.to_string(), d)),
            );
        }
    }
    if let Some(config) = config {
        for (d, file) in config.errors.into_iter().zip(config.error_files) {
            found.push((file.unwrap_or_default(), d));
        }
    }
    let found = tsr_diagnostics::sort_and_deduplicate_located_diagnostics(found);
    case.had_error_baseline = !found.is_empty();
    let mut out = String::new();
    for (name, d) in &found {
        diagnostic(&mut out, name, d, program.source_file(name).map(|f| f.text()));
        details(&mut out, d, "head", &program, &case)?;
    }
    // Sections derive only from actual input units. Native expected rows never choose TSR work.
    let sections: Vec<_> = case
        .files
        .iter()
        .map(|f| tsr_conformance::types_baseline::FileTypes {
            file: tsr_path::get_normalized_absolute_path(
                &f.name,
                case.current_directory.as_deref().unwrap_or("/.src"),
            ),
            assertions: Vec::new(),
        })
        .collect();
    let mut render_case = case.clone();
    for unit in &mut render_case.files {
        unit.name = tsr_path::get_normalized_absolute_path(
            &unit.name,
            case.current_directory.as_deref().unwrap_or("/.src"),
        );
    }
    let rows = tsr_conformance::types_producer::assertions_for_case(&render_case, &sections, false);
    for (section, rows) in sections.iter().zip(rows) {
        for row in rows {
            out.push_str(&format!(
                "T\t{}\t{}\t{}\n",
                hex(&path_name(&section.file)),
                hex(&row.text),
                hex(&row.type_string.replace("\r\n", "\n"))
            ));
        }
    }
    out.push_str("COMPLETE\n");
    Ok(out)
}

/// File-backed stdout/stderr, bounded lifetime, kill + wait on deadline. No pipe/Fd leak.
pub fn run(
    command: &mut Command,
    output: &Path,
    log: &Path,
    deadline: Duration,
) -> Result<(bool, u128)> {
    let stdout = fs::File::create(output)?;
    let stderr = fs::File::create(log)?;
    let start = Instant::now();
    let mut child = command.stdin(Stdio::null()).stdout(stdout).stderr(stderr).spawn()?;
    // Command retains configured Stdio handles after spawn. Release the parent's copies now.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status.success(), start.elapsed().as_millis()));
        }
        if start.elapsed() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok((false, start.elapsed().as_millis()));
        }
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// Copy, never hard-link, an executable into a content-addressed read-only worker image.
/// Rebuilding the original path cannot change the bytes subsequently spawned.
pub fn freeze_worker(
    source: &Path,
    directory: &Path,
    expected_hash: &str,
) -> Result<std::path::PathBuf> {
    let destination = directory.join(format!("worker-{expected_hash}"));
    if destination.exists() {
        anyhow::ensure!(hash(&destination)? == expected_hash, "worker image hash mismatch");
        return Ok(destination);
    }
    let temporary = destination.with_extension("copying");
    fs::copy(source, &temporary)?;
    anyhow::ensure!(hash(&temporary)? == expected_hash, "worker changed during snapshot");
    let mut permissions = fs::metadata(&temporary)?.permissions();
    permissions.set_readonly(true);
    fs::set_permissions(&temporary, permissions)?;
    fs::rename(&temporary, &destination)?;
    Ok(destination)
}

pub fn records(s: &str) -> Vec<&str> {
    s.lines()
        .filter(|l| ["D\t", "T\t", "M\t", "C\t", "R\t"].iter().any(|prefix| l.starts_with(prefix)))
        .collect()
}
pub fn complete(path: &Path) -> bool {
    fs::read_to_string(path).is_ok_and(|s| s.ends_with("COMPLETE\n"))
}
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let temp = path.with_extension("tmp");
    let mut f = fs::File::create(&temp)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()?;
    drop(f);
    fs::rename(temp, path)?;
    Ok(())
}
pub fn cluster(expected: &str, actual: &str) -> String {
    let a = records(expected);
    let b = records(actual);
    match a.iter().zip(&b).find(|(x, y)| x != y) {
        Some((x, y)) if x.starts_with("D\t") && y.starts_with("D\t") => {
            format!("diagnostic:{}", x.split('\t').nth(4).unwrap_or("unknown"))
        }
        Some((x, _)) if x.starts_with("R\t") => "related-information".into(),
        Some((x, _)) if x.starts_with("C\t") => "diagnostic-chain-structure".into(),
        Some((x, _)) if x.starts_with("M\t") => "diagnostic-metadata".into(),
        Some((x, _)) if x.starts_with("T\t") => "type-print-or-selection".into(),
        _ => "missing-or-extra-records".into(),
    }
}
/// Reconcile recorded input manifests without executing or inferring compiler work.
pub fn compare_populations(left: &Path, right: &Path, output: &Path) -> Result<()> {
    let load = |path: &Path| -> Result<BTreeMap<(String, String), String>> {
        let mut rows = BTreeMap::new();
        for line in fs::read_to_string(path)?.lines() {
            let fields: Vec<_> = line.split('\t').collect();
            anyhow::ensure!(fields.len() == 3, "invalid input manifest row");
            let source = unhex(fields[0])?;
            // Workspace prefixes differ; compiler/conformance identity is suite-relative.
            let relative = source.split_once("/tests/cases/").map_or(source.as_str(), |(_, r)| r);
            anyhow::ensure!(
                relative.starts_with("compiler/") || relative.starts_with("conformance/"),
                "unknown suite identity"
            );
            let key = (relative.to_string(), unhex(fields[1])?);
            anyhow::ensure!(
                rows.insert(key, fields[2].to_string()).is_none(),
                "duplicate configured identity"
            );
        }
        Ok(rows)
    };
    let a = load(left)?;
    let b = load(right)?;
    let mut out = format!(
        "LEFT_SHA256\t{}\nRIGHT_SHA256\t{}\nLEFT_ROWS\t{}\nRIGHT_ROWS\t{}\n",
        hash(left)?,
        hash(right)?,
        a.len(),
        b.len()
    );
    let mut missing = 0;
    let mut extra = 0;
    let mut changed = 0;
    for ((source, variant), h) in &a {
        match b.get(&(source.clone(), variant.clone())) {
            None => {
                missing += 1;
                out.push_str(&format!("MISSING_RIGHT\t{}\t{}\t{h}\n", hex(source), hex(variant)));
            }
            Some(other) if other != h => {
                changed += 1;
                out.push_str(&format!(
                    "CHANGED_INPUT\t{}\t{}\t{h}\t{other}\n",
                    hex(source),
                    hex(variant)
                ));
            }
            _ => {}
        }
    }
    for ((source, variant), h) in &b {
        if !a.contains_key(&(source.clone(), variant.clone())) {
            extra += 1;
            out.push_str(&format!("EXTRA_RIGHT\t{}\t{}\t{h}\n", hex(source), hex(variant)));
        }
    }
    out.push_str(&format!("MISSING_RIGHT_COUNT\t{missing}\nEXTRA_RIGHT_COUNT\t{extra}\nCHANGED_INPUT_COUNT\t{changed}\n"));
    write_atomic(output, &out)
}

pub type Counts = BTreeMap<String, usize>;
