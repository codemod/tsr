//! Exact full-corpus oracle: pinned native harness records versus real TSR records.
//!
//! See `docs/parity/notes/oracle.md`. The population is the native runner's own
//! enumeration and configuration expansion (`full_oracle_native.go`), never the
//! committed baselines. Each configured case runs in two bounded processes:
//!
//! - native: `runSingleConfigTest`'s `newCompilerTest`, `SkipUnsupportedCompilerOptions`,
//!   `c.result.Diagnostics` and `DoTypeAndSymbolBaseline`'s type walk;
//! - TSR: [`actual`], one configured checker over the case program, the harness
//!   collection of `compileFilesWithHost` and [`crate::types_producer::render_file`].
//!
//! Both publish the same record stream; a case is exact only when the complete
//! streams are equal. Nothing here normalizes either producer's output: file names
//! pass through the baseline writer's `removeTestPathPrefixes` on both sides, and
//! every other field is compared raw.
//!
//! Record format (fields hex-encoded where they are free text):
//!
//! - `D file start len code category message` — one published diagnostic, UTF-16
//!   span, flattened message;
//! - `M path unnecessary deprecated skippedOnNoEmit`, `C`/`R path file start len
//!   code category message` — metadata, chain and related information, recursive;
//! - `S file` — one `.types` section (a loaded unit in `toBeCompiled ++ otherFiles`);
//! - `T file line text type` — one `>text : type` row and the 0-based line it is
//!   placed after;
//! - `NATIVE_SKIPPED` — the native harness skips the configuration;
//! - `COMPLETE` — the producer finished; a stream without it is a failure.

use anyhow::{Context, Result, bail, ensure};
use std::{
    collections::{BTreeMap, HashMap},
    fmt::Write as _,
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
    time::{Duration, Instant},
};

/// The pinned native revision every artifact is bound to.
pub const NATIVE: &str = "5b1047d10d32e7d5b446be4de56b126ff42f82bb";

/// Lowercase hex of the UTF-8 bytes.
#[must_use]
pub fn hex(s: &str) -> String {
    let mut out = String::with_capacity(s.len() * 2);
    for b in s.as_bytes() {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// Inverse of [`hex`].
///
/// # Errors
///
/// Odd length, a non-hex digit, or bytes that are not UTF-8.
pub fn unhex(s: &str) -> Result<String> {
    ensure!(s.len() % 2 == 0, "odd hex length");
    let bytes = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16))
        .collect::<std::result::Result<Vec<_>, _>>()?;
    Ok(String::from_utf8(bytes)?)
}

/// Lowercase hex SHA-256 (FIPS 180-4) of `data`. In-process, so binding every
/// artifact of a run costs no process per file.
#[must_use]
pub fn sha256_bytes(data: &[u8]) -> String {
    const K: [u32; 64] = [
        0x428a_2f98,
        0x7137_4491,
        0xb5c0_fbcf,
        0xe9b5_dba5,
        0x3956_c25b,
        0x59f1_11f1,
        0x923f_82a4,
        0xab1c_5ed5,
        0xd807_aa98,
        0x1283_5b01,
        0x2431_85be,
        0x550c_7dc3,
        0x72be_5d74,
        0x80de_b1fe,
        0x9bdc_06a7,
        0xc19b_f174,
        0xe49b_69c1,
        0xefbe_4786,
        0x0fc1_9dc6,
        0x240c_a1cc,
        0x2de9_2c6f,
        0x4a74_84aa,
        0x5cb0_a9dc,
        0x76f9_88da,
        0x983e_5152,
        0xa831_c66d,
        0xb003_27c8,
        0xbf59_7fc7,
        0xc6e0_0bf3,
        0xd5a7_9147,
        0x06ca_6351,
        0x1429_2967,
        0x27b7_0a85,
        0x2e1b_2138,
        0x4d2c_6dfc,
        0x5338_0d13,
        0x650a_7354,
        0x766a_0abb,
        0x81c2_c92e,
        0x9272_2c85,
        0xa2bf_e8a1,
        0xa81a_664b,
        0xc24b_8b70,
        0xc76c_51a3,
        0xd192_e819,
        0xd699_0624,
        0xf40e_3585,
        0x106a_a070,
        0x19a4_c116,
        0x1e37_6c08,
        0x2748_774c,
        0x34b0_bcb5,
        0x391c_0cb3,
        0x4ed8_aa4a,
        0x5b9c_ca4f,
        0x682e_6ff3,
        0x748f_82ee,
        0x78a5_636f,
        0x84c8_7814,
        0x8cc7_0208,
        0x90be_fffa,
        0xa450_6ceb,
        0xbef9_a3f7,
        0xc671_78f2,
    ];
    let mut h: [u32; 8] = [
        0x6a09_e667,
        0xbb67_ae85,
        0x3c6e_f372,
        0xa54f_f53a,
        0x510e_527f,
        0x9b05_688c,
        0x1f83_d9ab,
        0x5be0_cd19,
    ];
    let mut tail = data[data.len() - data.len() % 64..].to_vec();
    tail.push(0x80);
    while tail.len() % 64 != 56 {
        tail.push(0);
    }
    tail.extend_from_slice(&((data.len() as u64).wrapping_mul(8)).to_be_bytes());
    for block in data[..data.len() - data.len() % 64].chunks_exact(64).chain(tail.chunks_exact(64))
    {
        let mut w = [0u32; 64];
        for (i, word) in block.chunks_exact(4).enumerate() {
            w[i] = u32::from_be_bytes([word[0], word[1], word[2], word[3]]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16].wrapping_add(s0).wrapping_add(w[i - 7]).wrapping_add(s1);
        }
        let mut v = h;
        for i in 0..64 {
            let s1 = v[4].rotate_right(6) ^ v[4].rotate_right(11) ^ v[4].rotate_right(25);
            let ch = (v[4] & v[5]) ^ (!v[4] & v[6]);
            let t1 = v[7].wrapping_add(s1).wrapping_add(ch).wrapping_add(K[i]).wrapping_add(w[i]);
            let s0 = v[0].rotate_right(2) ^ v[0].rotate_right(13) ^ v[0].rotate_right(22);
            let maj = (v[0] & v[1]) ^ (v[0] & v[2]) ^ (v[1] & v[2]);
            let t2 = s0.wrapping_add(maj);
            v = [t1.wrapping_add(t2), v[0], v[1], v[2], v[3].wrapping_add(t1), v[4], v[5], v[6]];
        }
        for (x, y) in h.iter_mut().zip(v) {
            *x = x.wrapping_add(y);
        }
    }
    let mut out = String::with_capacity(64);
    for x in h {
        let _ = write!(out, "{x:08x}");
    }
    out
}

/// SHA-256 of a file's bytes.
///
/// # Errors
///
/// The file is unreadable.
pub fn sha256(path: &Path) -> Result<String> {
    Ok(sha256_bytes(&fs::read(path).with_context(|| format!("reading {}", path.display()))?))
}

/// Write `content` to `path` through a synced temporary file and a rename.
///
/// # Errors
///
/// Any I/O failure.
pub fn write_atomic(path: &Path, content: &str) -> Result<()> {
    let temp = path.with_extension("tmp");
    let mut f = fs::File::create(&temp)?;
    f.write_all(content.as_bytes())?;
    f.sync_all()?;
    drop(f);
    fs::rename(temp, path)?;
    Ok(())
}

/// `removeTestPathPrefixes(text, false)` (`tsbaseline/util.go:43`): the printed
/// file identity in every native baseline. TSR's conformance program mounts the
/// bundled libraries at `/.ts-lib/` where the native harness uses
/// `bundled:///libs/`; the mount is a harness location, so it is mapped to the
/// native spelling before the writer's replacement, never dropped.
#[must_use]
pub fn printed_path(name: &str, tsr_mount: bool) -> String {
    // strings.NewReplacer: leftmost match, earlier pattern wins at one position.
    const PATTERNS: [(&str, &str); 7] = [
        ("/.ts/", ""),
        ("/.lib/", ""),
        ("/.src/", ""),
        ("bundled:///libs/", ""),
        ("file:///./ts/", "file:///"),
        ("file:///./lib/", "file:///"),
        ("file:///./src/", "file:///"),
    ];
    let name = match name.strip_prefix("/.ts-lib/") {
        Some(rest) if tsr_mount => format!("bundled:///libs/{rest}"),
        _ => name.to_string(),
    };
    let mut out = String::with_capacity(name.len());
    let mut rest = name.as_str();
    'scan: while !rest.is_empty() {
        for (from, to) in PATTERNS {
            if let Some(after) = rest.strip_prefix(from) {
                out.push_str(to);
                rest = after;
                continue 'scan;
            }
        }
        let ch = rest.chars().next().unwrap_or_default();
        out.push(ch);
        rest = &rest[ch.len_utf8()..];
    }
    out
}

/// The published UTF-16 `(start, length)`; `-` for a file-less diagnostic, whose
/// position no baseline or CLI prints (the native producer does the same).
fn utf16_span(text: Option<&str>, start: u32, end: u32) -> Result<(String, String)> {
    let (start, end) = (start as usize, end as usize);
    let Some(text) = text else { return Ok(("-".into(), "-".into())) };
    ensure!(
        start <= end && text.is_char_boundary(start) && text.is_char_boundary(end),
        "diagnostic span {start}..{end} is not a character range"
    );
    Ok((
        text[..start].encode_utf16().count().to_string(),
        text[start..end].encode_utf16().count().to_string(),
    ))
}

/// The request one TSR worker receives: the native configuration, verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Request {
    /// Corpus-relative source, e.g. `compiler/foo.ts`.
    pub identity: String,
    /// Native configuration name; empty for a single configuration.
    pub variant: String,
    /// `NamedTestConfiguration.Config`, lowercased keys, sorted.
    pub options: Vec<(String, String)>,
}

impl Request {
    /// Encode as the plan row's option field (`hex(k)=hex(v),...`).
    #[must_use]
    pub fn encode_options(&self) -> String {
        self.options
            .iter()
            .map(|(k, v)| format!("{}={}", hex(k), hex(v)))
            .collect::<Vec<_>>()
            .join(",")
    }

    /// Decode a plan `CASE identity variant options` row.
    ///
    /// # Errors
    ///
    /// Not a `CASE` row, or malformed fields.
    pub fn from_row(row: &str) -> Result<Self> {
        let f: Vec<_> = row.split('\t').collect();
        ensure!(f.len() == 4 && f[0] == "CASE", "request is not a plan CASE row");
        Ok(Request {
            identity: unhex(f[1])?,
            variant: unhex(f[2])?,
            options: Request::decode_options(f[3]).context("request options")?,
        })
    }

    /// Decode a plan row's option field.
    ///
    /// # Errors
    ///
    /// Malformed pairs.
    pub fn decode_options(field: &str) -> Result<Vec<(String, String)>> {
        field
            .split(',')
            .filter(|p| !p.is_empty())
            .map(|pair| {
                let (k, v) = pair.split_once('=').context("option pair")?;
                Ok((unhex(k)?, unhex(v)?))
            })
            .collect()
    }
}

/// Go's `\s` (ASCII `[\t\n\f\r ]`), for `referencesRegex` (`reference\spath`).
fn has_reference_path(content: &str) -> bool {
    content.match_indices("reference").any(|(i, _)| {
        let rest = &content.as_bytes()[i + "reference".len()..];
        rest.len() > 4
            && matches!(rest[0], b'\t' | b'\n' | b'\x0c' | b'\r' | b' ')
            && rest[1..].starts_with(b"path")
    })
}

/// `newCompilerTest`'s `toBeCompiled ++ otherFiles` (`compiler_runner.go:264`),
/// as absolute unit names: the type walk's section order before the
/// loaded-file filter. The config unit is not a unit (`makeUnitsFromTest`).
fn harness_unit_order(
    case: &crate::TestCase,
    current_directory: &str,
    config_files: Option<&[String]>,
) -> Vec<String> {
    let absolute = |name: &str| tsr_path::get_normalized_absolute_path(name, current_directory);
    let config = case
        .files
        .iter()
        .position(|u| crate::trace_case::config_name_from_file_name(&u.name).is_some());
    let units: Vec<_> =
        case.files.iter().enumerate().filter(|(i, _)| Some(*i) != config).map(|(_, u)| u).collect();
    if let (Some(_), Some(names)) = (config, config_files) {
        let (compiled, other): (Vec<_>, Vec<_>) =
            units.iter().map(|u| absolute(&u.name)).partition(|n| names.contains(n));
        return compiled.into_iter().chain(other).collect();
    }
    let Some(last) = units.last() else { return Vec::new() };
    let only_last = case.options.get("noimplicitreferences").is_some_and(|v| !v.is_empty())
        || last.content.contains("require(")
        || has_reference_path(&last.content);
    if only_last {
        std::iter::once(absolute(&last.name))
            .chain(units[..units.len() - 1].iter().map(|u| absolute(&u.name)))
            .collect()
    } else {
        units.iter().map(|u| absolute(&u.name)).collect()
    }
}

struct Writer<'p, 'a> {
    out: String,
    program: &'p tsr_compiler::Program<'a>,
    config_units: HashMap<String, &'p str>,
}

impl Writer<'_, '_> {
    fn text_of(&self, name: &str) -> Option<&str> {
        self.program
            .source_file(name)
            .map(tsr_compiler::ProgramFile::text)
            .or_else(|| self.config_units.get(name).copied())
    }

    fn diagnostic(&mut self, name: &str, d: &tsr_diagnostics::Diagnostic) -> Result<()> {
        let mut message = String::new();
        tsr_diagnostics::format::write_flattened_diagnostic_message(&mut message, d, "\n");
        let text = if name.is_empty() { None } else { self.text_of(name) };
        ensure!(name.is_empty() || text.is_some(), "diagnostic file {name} has no text");
        let (start, len) = utf16_span(text, d.span.start, d.span.end)?;
        let _ = writeln!(
            self.out,
            "D\t{}\t{start}\t{len}\t{}\t{}\t{}",
            hex(&printed_path(name, true)),
            d.message.code(),
            d.category() as u8,
            hex(&message)
        );
        self.details(d, "head")
    }

    fn details(&mut self, d: &tsr_diagnostics::Diagnostic, path: &str) -> Result<()> {
        let _ = writeln!(
            self.out,
            "M\t{path}\t{}\t{}\t{}",
            d.reports_unnecessary(),
            d.reports_deprecated(),
            d.skipped_on_no_emit()
        );
        for (tag, rows) in [("C", d.message_chain()), ("R", d.related_information())] {
            for (i, child) in rows.iter().enumerate() {
                let child_path = format!("{path}/{tag}{i}");
                let name = child.file().map_or("", |f| f.file_name());
                let text = if name.is_empty() { None } else { self.text_of(name) };
                ensure!(name.is_empty() || text.is_some(), "detail file {name} has no text");
                let (start, len) = utf16_span(text, child.span.start, child.span.end)?;
                let args: Vec<_> = child.args.iter().map(String::as_str).collect();
                let _ = writeln!(
                    self.out,
                    "{tag}\t{child_path}\t{}\t{start}\t{len}\t{}\t{}\t{}",
                    hex(&printed_path(name, true)),
                    child.message.code(),
                    child.category() as u8,
                    hex(&child.message.format(&args))
                );
                self.details(child, &child_path)?;
            }
        }
        Ok(())
    }
}

/// The real TSR producer for one configured case.
///
/// The case program is the conformance harness's
/// ([`crate::types_producer::program_and_config_for_case`]) under the native
/// configuration and the native harness directory `/.src`. One configured
/// checker checks every file that `SkipTypeChecking` admits, matching the
/// single-threaded test program's one checker (`checkerpool.go:41`). The
/// collection is `compileFilesWithHost`'s (`harnessutil.go:650`): config, syntactic
/// (parse and JS syntax), semantic (`getBindAndCheckDiagnosticsWithChecker` plus the
/// include processor) and declaration diagnostics for every file, then
/// `SortAndDeduplicateDiagnostics`. The type rows render through the same checker
/// with `hadErrorBaseline` taken from that collection, over the harness's unit
/// order filtered to loaded files, JSON included (`verifyTypesAndSymbols`).
///
/// # Errors
///
/// An unreadable source, a structurally invalid case, or a diagnostic whose span
/// is not a character range of its file.
pub fn actual(source: &Path, request: &Request) -> Result<String> {
    let raw = tsr_vfs::decode_bytes(&fs::read(source)?);
    let base = source.file_name().context("basename")?.to_str().context("UTF-8 name")?;
    let mut case = crate::TestCase::parse(&request.identity, base, &raw);
    if let Some(error) = &case.error {
        bail!("case structure: {error}");
    }
    for (k, v) in &request.options {
        case.options.insert(k.clone(), v.clone());
    }
    let current_directory = tsr_path::get_normalized_absolute_path(
        case.current_directory.as_deref().unwrap_or(""),
        "/.src",
    );
    case.current_directory = Some(current_directory.clone());

    let arena = tsr_core::Arena::new();
    let (program, config) = crate::types_producer::program_and_config_for_case(&arena, &case);
    let options = program.compiler_options();
    let files = program.source_files();
    let admitted: Vec<bool> = (0..files.len())
        .map(|i| {
            tsr_compiler::program_diagnostics::skip_type_checking(&program, i, false).is_none()
        })
        .collect();
    let mut checker = crate::types_producer::configured_checker(&program);
    checker.set_checked_files(
        files
            .iter()
            .zip(&admitted)
            .filter(|(_, c)| **c)
            .filter_map(|(f, _)| f.source_file().node_id),
    );
    for (f, _) in files.iter().zip(&admitted).filter(|(_, c)| **c) {
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
    let mut by_file: HashMap<tsr_ast::NodeId, Vec<tsr_diagnostics::Diagnostic>> = HashMap::new();
    for (file, d) in checker.diagnostics() {
        by_file.entry(*file).or_default().push(d.clone());
    }
    let mut found: Vec<(String, tsr_diagnostics::Diagnostic)> = Vec::new();
    if let Some(config) = &config {
        for (d, file) in config.errors.iter().zip(&config.error_files) {
            found.push((file.clone().unwrap_or_default(), d.clone()));
        }
    }
    // `GetProgramDiagnostics`: `verifyCompilerOptions` against the harness
    // `ParsedCommandLine`, whose `ConfigFile` is the case's tsconfig unit.
    let config_unit = config.as_ref().and_then(|_| {
        case.files.iter().find(|u| crate::trace_case::config_name_from_file_name(&u.name).is_some())
    });
    let config_file_name =
        config_unit.map(|u| tsr_path::get_normalized_absolute_path(&u.name, &current_directory));
    let config_syntax = config_unit.map(|u| tsr_tsoptions::syntax::ConfigSyntax::parse(&u.content));
    let suppress_output_path_check = match case.options.get("suppressoutputpathcheck") {
        Some(v) if v.eq_ignore_ascii_case("true") => tsr_core::Tristate::True,
        Some(v) if v.eq_ignore_ascii_case("false") => tsr_core::Tristate::False,
        _ => tsr_core::Tristate::Unknown,
    };
    found.extend(tsr_compiler::program_diagnostics::verify_compiler_options(
        &program,
        tsr_compiler::program_diagnostics::OptionsVerification {
            config_file: config_file_name.as_deref().zip(config_syntax.as_ref()),
            suppress_output_path_check,
        },
    ));
    let emit_declarations = options.declaration.is_true() || options.composite.is_true();
    for (i, f) in files.iter().enumerate() {
        let name = f.file_name();
        found.extend(f.diagnostics().iter().cloned().map(|d| (name.to_string(), d)));
        let Some(id) = f.source_file().node_id else { continue };
        found.extend(
            checker.js_syntax_diagnostics(id).into_iter().map(|(_, d)| (name.to_string(), d)),
        );
        if !admitted[i] {
            continue;
        }
        let checks = by_file.remove(&id).unwrap_or_default();
        found.extend(
            tsr_compiler::program_diagnostics::bind_and_check_diagnostics(
                &program,
                i,
                program.bind_diagnostics_of(i),
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
        // `GetDeclarationDiagnostics` when `GetEmitDeclarations()`: only the
        // isolatedDeclarations family has a TSR producer.
        if emit_declarations
            && options.isolated_declarations.is_true()
            && !tsr_path::is_declaration_file_name(name)
            && tsr_parser::ScriptKind::from_file_name(name) != tsr_parser::ScriptKind::Json
        {
            found.extend(
                tsr_dts::analyze_with_options(
                    f.source_file(),
                    program.nodes(),
                    tsr_dts::AnalysisOptions {
                        strict_null_checks: options.strict_option_value(options.strict_null_checks),
                    },
                )
                .into_iter()
                .map(|d| (name.to_string(), d)),
            );
        }
    }
    let found = tsr_diagnostics::sort_and_deduplicate_located_diagnostics(found);

    let config_units = case
        .files
        .iter()
        .map(|u| {
            (
                tsr_path::get_normalized_absolute_path(&u.name, &current_directory),
                u.content.as_str(),
            )
        })
        .collect();
    let mut w = Writer { out: String::new(), program: &program, config_units };
    for (name, d) in &found {
        w.diagnostic(name, d)?;
    }
    let no_types =
        case.options.get("notypesandsymbols").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    if !no_types {
        let order = harness_unit_order(
            &case,
            &current_directory,
            config.as_ref().map(|c| c.file_names.as_slice()),
        );
        for unit in order {
            let Some(file) = program.source_file(&unit) else { continue };
            let printed = hex(&printed_path(&unit, true));
            let _ = writeln!(w.out, "S\t{printed}");
            let (rows, ids) = crate::types_producer::render_file(
                &mut checker,
                &program,
                file,
                !found.is_empty(),
                false,
            );
            let starts = tsr_core::ecma_line_starts(file.text());
            for (row, id) in rows.iter().zip(ids) {
                let line = crate::types_producer::baseline_line(
                    file.text(),
                    &starts,
                    program.nodes().span(id).start,
                );
                let _ = writeln!(
                    w.out,
                    "T\t{printed}\t{line}\t{}\t{}",
                    hex(&row.text),
                    hex(&row.type_string)
                );
            }
        }
    }
    w.out.push_str("COMPLETE\n");
    Ok(w.out)
}

/// Run `command` with file-backed stdout/stderr and a deadline; kill and reap on
/// expiry. Returns `(outcome, elapsed)`.
///
/// # Errors
///
/// Spawn or wait failures.
pub fn run_bounded(
    command: &mut Command,
    stdout: &Path,
    stderr: &Path,
    deadline: Duration,
) -> Result<(ProcessOutcome, Duration)> {
    let start = Instant::now();
    let mut child = command
        .stdin(Stdio::null())
        .stdout(fs::File::create(stdout)?)
        .stderr(fs::File::create(stderr)?)
        .spawn()?;
    // `Command` keeps its configured handles; release the parent's copies.
    command.stdout(Stdio::null()).stderr(Stdio::null());
    loop {
        if let Some(status) = child.try_wait()? {
            let outcome =
                if status.success() { ProcessOutcome::Exited } else { ProcessOutcome::Failed };
            return Ok((outcome, start.elapsed()));
        }
        if start.elapsed() >= deadline {
            child.kill()?;
            child.wait()?;
            return Ok((ProcessOutcome::Timeout, start.elapsed()));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// How one bounded worker process ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProcessOutcome {
    /// Exit status zero.
    Exited,
    /// Non-zero exit or signal.
    Failed,
    /// Killed at the deadline.
    Timeout,
}

/// One diagnostic with its `M`/`C`/`R` records.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct Group<'s> {
    head: &'s str,
    details: Vec<&'s str>,
}

impl<'s> Group<'s> {
    fn field(&self, i: usize) -> &'s str {
        self.head.split('\t').nth(i).unwrap_or_default()
    }
    /// `(file, start, len, code)`.
    fn location(&self) -> (&'s str, &'s str, &'s str, &'s str) {
        (self.field(1), self.field(2), self.field(3), self.field(4))
    }
    fn code(&self) -> String {
        format!("TS{}", self.field(4))
    }
}

/// A producer stream split into its comparable parts.
#[derive(Debug, Default)]
pub struct Stream<'s> {
    groups: Vec<Group<'s>>,
    sections: Vec<&'s str>,
    rows: Vec<[&'s str; 4]>,
    /// The stream ended with `COMPLETE`.
    pub complete: bool,
    /// The native harness skipped the configuration.
    pub skipped: bool,
}

impl<'s> Stream<'s> {
    /// Parse a producer artifact.
    #[must_use]
    pub fn parse(text: &'s str) -> Self {
        let mut s = Stream { complete: text.ends_with("COMPLETE\n"), ..Stream::default() };
        for line in text.lines() {
            match line.split('\t').next().unwrap_or_default() {
                "D" => s.groups.push(Group { head: line, details: Vec::new() }),
                "M" | "C" | "R" => {
                    if let Some(g) = s.groups.last_mut() {
                        g.details.push(line);
                    }
                }
                "S" => s.sections.push(line),
                "T" => {
                    let f: Vec<_> = line.splitn(5, '\t').collect();
                    if f.len() == 5 {
                        s.rows.push([f[1], f[2], f[3], f[4]]);
                    }
                }
                "NATIVE_SKIPPED" => s.skipped = true,
                _ => {}
            }
        }
        s
    }
}

fn multiset<T: Ord + Clone>(items: impl IntoIterator<Item = T>) -> BTreeMap<T, usize> {
    let mut m = BTreeMap::new();
    for i in items {
        *m.entry(i).or_insert(0) += 1;
    }
    m
}

/// Items of `a` not matched in `b`, by multiplicity.
fn surplus<T: Ord + Clone>(a: &BTreeMap<T, usize>, b: &BTreeMap<T, usize>) -> Vec<T> {
    let mut out = Vec::new();
    for (k, n) in a {
        let m = b.get(k).copied().unwrap_or(0);
        for _ in m..*n {
            out.push(k.clone());
        }
    }
    out
}

/// The diagnostic half's first-difference class, and the code that names it.
fn diagnostic_class(native: &[Group<'_>], tsr: &[Group<'_>]) -> Option<(&'static str, String)> {
    if native == tsr {
        return None;
    }
    let (n_all, t_all) = (multiset(native.iter().cloned()), multiset(tsr.iter().cloned()));
    if n_all == t_all {
        let i = native.iter().zip(tsr).position(|(a, b)| a != b).unwrap_or(0);
        return Some(("diag:order-only", native.get(i).map(Group::code).unwrap_or_default()));
    }
    let (n_loc, t_loc) =
        (multiset(native.iter().map(Group::location)), multiset(tsr.iter().map(Group::location)));
    let missing = surplus(&n_loc, &t_loc);
    let extra = surplus(&t_loc, &n_loc);
    let code = |l: &(&str, &str, &str, &str)| format!("TS{}", l.3);
    match (missing.first(), extra.first()) {
        (Some(m), None) => return Some(("diag:missing", code(m))),
        (None, Some(e)) => return Some(("diag:extra", code(e))),
        (Some(m), Some(_)) => {
            let same_codes = multiset(missing.iter().map(|x| (x.0, x.3)))
                == multiset(extra.iter().map(|x| (x.0, x.3)));
            let class = if same_codes {
                "diag:span-only"
            } else if multiset(missing.iter().map(|x| (x.0, x.1, x.2)))
                == multiset(extra.iter().map(|x| (x.0, x.1, x.2)))
            {
                "diag:code-at-same-span"
            } else {
                "diag:missing+extra"
            };
            return Some((class, code(m)));
        }
        (None, None) => {}
    }
    // Same locations: heads or details differ.
    let (mut ns, mut ts) = (native.to_vec(), tsr.to_vec());
    ns.sort_by(|a, b| a.location().cmp(&b.location()).then(a.cmp(b)));
    ts.sort_by(|a, b| a.location().cmp(&b.location()).then(a.cmp(b)));
    // The flattened head message embeds the chain, so the chain and related
    // lists are compared before the head text.
    let part = |g: &Group<'_>, marker: &str| -> Vec<String> {
        g.details
            .iter()
            .filter(|d| d.split('\t').nth(1).is_some_and(|p| p.contains(marker)))
            .map(|d| (*d).to_string())
            .collect()
    };
    for (a, b) in ns.iter().zip(&ts) {
        if a == b {
            continue;
        }
        let class = if part(a, "/C") != part(b, "/C") {
            "diag:chain"
        } else if part(a, "/R") != part(b, "/R") {
            "diag:related-info"
        } else if a.field(5) != b.field(5) {
            "diag:category"
        } else if a.head != b.head {
            "diag:message-text-only"
        } else {
            "diag:metadata"
        };
        return Some((class, a.code()));
    }
    Some(("diag:order-only", String::new()))
}

/// The type half's first-difference class.
fn type_class(native: &Stream<'_>, tsr: &Stream<'_>) -> Option<(&'static str, String)> {
    if native.sections != tsr.sections {
        let class = if tsr.sections.is_empty() {
            "types:no-sections"
        } else if native.sections.is_empty() {
            "types:unexpected-sections"
        } else {
            "types:section-set"
        };
        return Some((class, String::new()));
    }
    let (want, got) = (&native.rows, &tsr.rows);
    let Some(at) = want.iter().zip(got).position(|(x, y)| x != y) else {
        return match want.len().cmp(&got.len()) {
            std::cmp::Ordering::Equal => None,
            std::cmp::Ordering::Greater => {
                Some(("types:missing-row", decode_text(want[got.len()][2])))
            }
            std::cmp::Ordering::Less => Some(("types:extra-row", decode_text(got[want.len()][2]))),
        };
    };
    let (native_row, tsr_row) = (want[at], got[at]);
    if native_row[0] != tsr_row[0] {
        return Some(("types:section-boundary", String::new()));
    }
    if native_row[2] == tsr_row[2] && native_row[1] == tsr_row[1] {
        return Some((
            "types:type-text",
            format!("{} -> {}", decode_text(native_row[3]), decode_text(tsr_row[3])),
        ));
    }
    // A shifted row: TSR selected an extra node, or skipped one.
    if got.get(at + 1).is_some_and(|row| row == &native_row) {
        return Some(("types:extra-row", decode_text(tsr_row[2])));
    }
    if want.get(at + 1).is_some_and(|row| row == &tsr_row) {
        return Some(("types:missing-row", decode_text(native_row[2])));
    }
    if native_row[2] == tsr_row[2] {
        return Some(("types:line-placement", decode_text(native_row[2])));
    }
    Some(("types:node-selection", decode_text(native_row[2])))
}

fn decode_text(h: &str) -> String {
    let mut s = unhex(h).unwrap_or_default();
    if s.len() > 40 {
        let mut cut = 40;
        while !s.is_char_boundary(cut) {
            cut -= 1;
        }
        s.truncate(cut);
    }
    s.replace(['\t', '\n', '\r'], " ")
}

/// Exact verdict for one case's two complete artifacts.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Verdict {
    /// Diagnostic-half class and its first differing code.
    pub diagnostics: Option<(&'static str, String)>,
    /// Type-half class and its first differing node text.
    pub types: Option<(&'static str, String)>,
}

impl Verdict {
    /// Both halves equal.
    #[must_use]
    pub fn exact(&self) -> bool {
        self.diagnostics.is_none() && self.types.is_none()
    }
}

/// Compare two complete streams.
#[must_use]
pub fn compare(native: &Stream<'_>, tsr: &Stream<'_>) -> Verdict {
    Verdict {
        diagnostics: diagnostic_class(&native.groups, &tsr.groups),
        types: type_class(native, tsr),
    }
}

/// Every diagnostic code involved in a difference, with how: `missing` (a native
/// location TSR lacks), `extra` (a TSR location native lacks), `detail` (the same
/// location published with another message, category, chain or related list).
/// Order-only differences contribute nothing. Sorted, one entry per pair.
#[must_use]
pub fn diagnostic_code_profile(
    native: &Stream<'_>,
    tsr: &Stream<'_>,
) -> Vec<(String, &'static str)> {
    let (n_loc, t_loc) = (
        multiset(native.groups.iter().map(Group::location)),
        multiset(tsr.groups.iter().map(Group::location)),
    );
    let mut out: Vec<(String, &'static str)> = Vec::new();
    out.extend(surplus(&n_loc, &t_loc).into_iter().map(|l| (format!("TS{}", l.3), "missing")));
    out.extend(surplus(&t_loc, &n_loc).into_iter().map(|l| (format!("TS{}", l.3), "extra")));
    let (n_all, t_all) =
        (multiset(native.groups.iter().cloned()), multiset(tsr.groups.iter().cloned()));
    let changed = surplus(&n_all, &t_all);
    let tsr_side = multiset(surplus(&t_all, &n_all).iter().map(Group::location));
    for group in changed {
        // A location present on both sides whose published group differs.
        if tsr_side.contains_key(&group.location()) {
            out.push((group.code(), "detail"));
        }
    }
    out.sort_unstable();
    out.dedup();
    out
}

/// Where a non-exact case first differs: the record a root-cause ranking
/// attributes the case to. Diagnostic half first, as the primary class.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Blocker {
    /// Primary class (`diag:missing`, `types:type-text`, ...).
    pub class: &'static str,
    /// `TSnnnn` of the differing record: the missing/extra/moved diagnostic,
    /// or the first differing chain or related record (else the head).
    pub code: String,
    /// `TSnnnn` of the diagnostic the record belongs to.
    pub head_code: String,
    /// Which producer published the attributed record: `native` or `tsr`.
    pub side: &'static str,
    /// Printed file of the attributed location (empty when file-less).
    pub file: String,
    /// UTF-16 start and length, or `-`.
    pub span: (String, String),
    /// Type half: the row's index within its file's section.
    pub row: Option<usize>,
    /// Type half: `native type -> tsr type` of the first differing row.
    pub types: Option<(String, String)>,
}

fn record_code(record: &str) -> String {
    // `C`/`R` records: tag path file start len code ...
    format!("TS{}", record.split('\t').nth(5).unwrap_or_default())
}

/// The [`Blocker`] of two complete streams that are not exact.
#[must_use]
pub fn blocker(native: &Stream<'_>, tsr: &Stream<'_>) -> Option<Blocker> {
    let located = |class, g: &Group<'_>, code: String, side| Blocker {
        class,
        code,
        head_code: g.code(),
        side,
        file: unhex(g.field(1)).unwrap_or_default(),
        span: (g.field(2).to_string(), g.field(3).to_string()),
        row: None,
        types: None,
    };
    if let Some((class, _)) = diagnostic_class(&native.groups, &tsr.groups) {
        let (n_loc, t_loc) = (
            multiset(native.groups.iter().map(Group::location)),
            multiset(tsr.groups.iter().map(Group::location)),
        );
        let missing = surplus(&n_loc, &t_loc);
        let extra = surplus(&t_loc, &n_loc);
        if let Some(m) = missing.first() {
            let g = native.groups.iter().find(|g| g.location() == *m)?;
            return Some(located(class, g, g.code(), "native"));
        }
        if let Some(e) = extra.first() {
            let g = tsr.groups.iter().find(|g| g.location() == *e)?;
            return Some(located(class, g, g.code(), "tsr"));
        }
        let (mut ns, mut ts) = (native.groups.clone(), tsr.groups.clone());
        ns.sort_by(|a, b| a.location().cmp(&b.location()).then(a.cmp(b)));
        ts.sort_by(|a, b| a.location().cmp(&b.location()).then(a.cmp(b)));
        let Some((a, b)) = ns.iter().zip(&ts).find(|(a, b)| a != b) else {
            let g = native.groups.first()?;
            return Some(located(class, g, g.code(), "native"));
        };
        // The first differing detail record of the kind the class names.
        let marker = match class {
            "diag:chain" => Some("/C"),
            "diag:related-info" => Some("/R"),
            _ => None,
        };
        if let Some(marker) = marker {
            let records = |g: &Group<'_>| -> Vec<String> {
                g.details
                    .iter()
                    .filter(|d| d.split('\t').nth(1).is_some_and(|p| p.contains(marker)))
                    .map(|d| (*d).to_string())
                    .collect()
            };
            let (ra, rb) = (records(a), records(b));
            let i = ra.iter().zip(&rb).position(|(x, y)| x != y).unwrap_or(ra.len().min(rb.len()));
            if let Some(r) = ra.get(i) {
                return Some(located(class, a, record_code(r), "native"));
            }
            if let Some(r) = rb.get(i) {
                return Some(located(class, b, record_code(r), "tsr"));
            }
        }
        return Some(located(class, a, a.code(), "native"));
    }
    let (class, _) = type_class(native, tsr)?;
    let at = native.rows.iter().zip(&tsr.rows).position(|(x, y)| x != y);
    let (row, side) = match at {
        Some(at) => (native.rows[at], "native"),
        None if native.rows.len() > tsr.rows.len() => (native.rows[tsr.rows.len()], "native"),
        None => (*tsr.rows.get(native.rows.len())?, "tsr"),
    };
    let rows = if side == "native" { &native.rows } else { &tsr.rows };
    let index = at.unwrap_or(if side == "native" { tsr.rows.len() } else { native.rows.len() });
    let within = rows[..index].iter().filter(|r| r[0] == row[0]).count();
    Some(Blocker {
        class,
        code: String::new(),
        head_code: String::new(),
        side,
        file: unhex(row[0]).unwrap_or_default(),
        span: ("-".into(), "-".into()),
        row: Some(within),
        types: first_type_text(native, tsr),
    })
}

/// The syntactic context of a [`Blocker`]'s location in its case: the kind of
/// the deepest node whose post-trivia span is the diagnostic's span (prefixed
/// `~` when only an enclosing node exists) or the type walk's node for the row,
/// and its parent's kind. `lib` for a bundled-library location, `-` for a
/// file-less diagnostic. Read off the TSR parse of the native configuration,
/// for ranking only; it never feeds a verdict.
///
/// # Errors
///
/// An unreadable source or a structurally invalid case.
pub fn blocker_context(
    source: &Path,
    request: &Request,
    blocker: &Blocker,
) -> Result<(String, String)> {
    if blocker.file.is_empty() {
        return Ok(("-".into(), "-".into()));
    }
    if blocker.file.starts_with("lib.") && blocker.file.ends_with(".d.ts") {
        return Ok(("lib".into(), "-".into()));
    }
    let raw = tsr_vfs::decode_bytes(&fs::read(source)?);
    let base = source.file_name().context("basename")?.to_str().context("UTF-8 name")?;
    let mut case = crate::TestCase::parse(&request.identity, base, &raw);
    if let Some(error) = &case.error {
        bail!("case structure: {error}");
    }
    for (k, v) in &request.options {
        case.options.insert(k.clone(), v.clone());
    }
    let current_directory = tsr_path::get_normalized_absolute_path(
        case.current_directory.as_deref().unwrap_or(""),
        "/.src",
    );
    case.current_directory = Some(current_directory.clone());
    let arena = tsr_core::Arena::new();
    let (program, _) = crate::types_producer::program_and_config_for_case(&arena, &case);
    let unknown = || Ok(("?".to_string(), "?".to_string()));
    let Some(file) =
        program.source_files().iter().find(|f| printed_path(f.file_name(), true) == blocker.file)
    else {
        return unknown();
    };
    let nodes = program.nodes();
    let name = |id: tsr_ast::NodeId| format!("{:?}", nodes.kind(id));
    let with_parent = |id: tsr_ast::NodeId, prefix: &str| {
        let parent = nodes.parent(id).map_or_else(|| "-".to_string(), name);
        (format!("{prefix}{}", name(id)), parent)
    };
    let text = file.text();
    if let Some(row) = blocker.row {
        let mut ids = Vec::new();
        crate::types_producer::assertions_for_file(
            &tsr_ast::Node::SourceFile(file.source_file()),
            text,
            nodes,
            program.node_map(),
            |id| {
                ids.push(id);
                String::new()
            },
        );
        return ids.get(row).map_or_else(unknown, |&id| Ok(with_parent(id, "")));
    }
    let (Ok(start), Ok(len)) = (blocker.span.0.parse::<usize>(), blocker.span.1.parse::<usize>())
    else {
        return unknown();
    };
    // UTF-16 offsets to byte offsets.
    let byte = |units: usize| {
        let mut seen = 0;
        for (i, ch) in text.char_indices() {
            if seen >= units {
                return i;
            }
            seen += ch.len_utf16();
        }
        text.len()
    };
    let (start, end) = (byte(start), byte(start + len));
    let depth = |mut id: tsr_ast::NodeId| {
        let mut d = 0;
        while let Some(p) = nodes.parent(id) {
            d += 1;
            id = p;
        }
        d
    };
    let mut exact: Option<(usize, tsr_ast::NodeId)> = None;
    let mut enclosing: Option<(usize, tsr_ast::NodeId)> = None;
    for raw_id in file.node_range() {
        let id = tsr_ast::NodeId::new(raw_id);
        let span = nodes.span(id);
        let (s, e) = (span.start as usize, span.end as usize);
        if s > start || e < end {
            continue;
        }
        let d = depth(id);
        let slot = if crate::types_producer::skip_trivia(text, s) == start && e == end {
            &mut exact
        } else {
            &mut enclosing
        };
        if slot.is_none_or(|(best, _)| d > best) {
            *slot = Some((d, id));
        }
    }
    Ok(match (exact, enclosing) {
        (Some((_, id)), _) => with_parent(id, ""),
        (None, Some((_, id))) => with_parent(id, "~"),
        (None, None) => ("?".into(), "?".into()),
    })
}

/// The full `(native, tsr)` type texts of the first row that differs only in its
/// type, when the first type-half difference is of that kind.
#[must_use]
pub fn first_type_text(native: &Stream<'_>, tsr: &Stream<'_>) -> Option<(String, String)> {
    let (a, b) = native.rows.iter().zip(&tsr.rows).find(|(a, b)| a != b)?;
    (a[..3] == b[..3]).then(|| (unhex(a[3]).unwrap_or_default(), unhex(b[3]).unwrap_or_default()))
}

/// One `results.tsv` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    /// `identity` and `variant`, joined by a tab, both hex.
    pub key: String,
    /// `EXACT`, `WRONG`, `NATIVE_SKIPPED`, `LISTED_SKIP`, or a failure outcome.
    pub outcome: String,
    /// Primary class.
    pub class: String,
    /// Detail of the primary class (code or node text).
    pub detail: String,
}

/// Outcomes that are not part of the measured population: the native harness
/// itself publishes no baseline for them.
#[must_use]
pub fn excluded(outcome: &str) -> bool {
    matches!(outcome, "NATIVE_SKIPPED" | "LISTED_SKIP")
}

/// Load `results.tsv` (`index key... outcome class detail ...`).
///
/// # Errors
///
/// Unreadable or malformed rows.
pub fn load_results(path: &Path) -> Result<Vec<ResultRow>> {
    let mut out = Vec::new();
    for line in fs::read_to_string(path)?.lines().skip(1) {
        let f: Vec<_> = line.split('\t').collect();
        ensure!(f.len() >= 6, "malformed result row: {line}");
        out.push(ResultRow {
            key: format!("{}\t{}", f[1], f[2]),
            outcome: f[3].to_string(),
            class: f[4].to_string(),
            detail: f[5].to_string(),
        });
    }
    Ok(out)
}

/// Identity keys two TSR reports must share to be compared: the frozen native
/// run they were measured against and the population and deadline they used.
const COMPARABLE: [&str; 11] = [
    "native_revision",
    "corpus_revision",
    "producer_source_sha256:full_oracle_native.go",
    "producer_source_sha256:full_oracle_native_types.go",
    "native_binary_sha256",
    "plan_sha256",
    "native_results_sha256",
    "native_deadline_seconds",
    "native_filter",
    "tsr_deadline_seconds",
    "tsr_filter",
];

/// The transition gate between two complete TSR reports: every key exact in
/// `base` must be present and exact in `candidate`. Lists every exact gain,
/// loss and missing key. Returns the report text and whether it passed.
///
/// # Errors
///
/// Unreadable reports or identities that make the runs incomparable.
pub fn gate(base: &Path, candidate: &Path) -> Result<(String, bool)> {
    let ident = |dir: &Path| -> Result<BTreeMap<String, String>> {
        Ok(fs::read_to_string(dir.join("identity.tsv"))
            .with_context(|| format!("{} has no identity.tsv", dir.display()))?
            .lines()
            .filter_map(|l| l.split_once('\t'))
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect())
    };
    let (bi, ci) = (ident(base)?, ident(candidate)?);
    for (dir, i) in [(base, &bi), (candidate, &ci)] {
        ensure!(
            i.get("kind").map(String::as_str) == Some("tsr")
                && i.get("status").map(String::as_str) == Some("complete"),
            "{} is not a complete TSR report",
            dir.display()
        );
    }
    let mut out = String::new();
    for key in COMPARABLE {
        let (a, b) = (bi.get(key), ci.get(key));
        ensure!(a == b, "incomparable runs: {key} differs ({a:?} vs {b:?})");
    }
    for key in ["source_commit", "source_dirty", "tsr_binary_sha256", "tsr_mode"] {
        let _ = writeln!(
            out,
            "{key}\t{}\t{}",
            bi.get(key).map_or("?", |s| s),
            ci.get(key).map_or("?", |s| s)
        );
    }
    let b = load_results(&base.join("results.tsv"))?;
    let c = load_results(&candidate.join("results.tsv"))?;
    let index = |rows: &[ResultRow]| -> HashMap<String, usize> {
        rows.iter().enumerate().map(|(i, r)| (r.key.clone(), i)).collect()
    };
    let (bk, ck) = (index(&b), index(&c));
    let exact = |rows: &[ResultRow]| rows.iter().filter(|r| r.outcome == "EXACT").count();
    let (mut lost, mut missing, mut gained) = (Vec::new(), Vec::new(), Vec::new());
    for r in &b {
        if r.outcome != "EXACT" {
            continue;
        }
        match ck.get(&r.key).map(|&i| &c[i]) {
            None => missing.push(format!("MISSING\t{}", readable_key(&r.key))),
            Some(now) if now.outcome != "EXACT" => lost.push(format!(
                "LOST\t{}\t{}\t{}\t{}",
                readable_key(&r.key),
                now.outcome,
                now.class,
                now.detail
            )),
            Some(_) => {}
        }
    }
    for r in &c {
        let was = bk.get(&r.key).map(|&i| b[i].outcome.as_str());
        if r.outcome == "EXACT" && was != Some("EXACT") {
            gained.push(format!("GAINED\t{}\t{}", readable_key(&r.key), was.unwrap_or("ABSENT")));
        }
    }
    let _ = writeln!(
        out,
        "exact\t{}\t{}\noracle: exact_gained={} exact_lost={} exact_missing={}",
        exact(&b),
        exact(&c),
        gained.len(),
        lost.len(),
        missing.len()
    );
    for l in lost.iter().chain(&missing).chain(&gained) {
        let _ = writeln!(out, "{l}");
    }
    Ok((out, lost.is_empty() && missing.is_empty()))
}

/// `compiler/foo.ts [variant]` from a hex result key.
#[must_use]
pub fn readable_key(key: &str) -> String {
    let (id, variant) = key.split_once('\t').unwrap_or((key, ""));
    let id = unhex(id).unwrap_or_default();
    let variant = unhex(variant).unwrap_or_default();
    if variant.is_empty() { id } else { format!("{id} ({variant})") }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn d(file: &str, start: u32, code: u32, msg: &str) -> String {
        format!(
            "D\t{}\t{start}\t1\t{code}\t1\t{}\nM\thead\tfalse\tfalse\tfalse\n",
            hex(file),
            hex(msg)
        )
    }

    fn class(native: &str, tsr: &str) -> Option<&'static str> {
        let (n, t) = (Stream::parse(native), Stream::parse(tsr));
        let v = compare(&n, &t);
        v.diagnostics.map(|x| x.0).or(v.types.map(|x| x.0))
    }

    #[test]
    fn diagnostic_differences_are_classified_by_first_kind() {
        let a = d("a.ts", 1, 2322, "x");
        let b = d("a.ts", 5, 2345, "y");
        assert_eq!(class(&(a.clone() + &b), &(a.clone() + &b)), None);
        assert_eq!(class(&(a.clone() + &b), &(b.clone() + &a)), Some("diag:order-only"));
        assert_eq!(class(&(a.clone() + &b), &a), Some("diag:missing"));
        assert_eq!(class(&a, &(a.clone() + &b)), Some("diag:extra"));
        assert_eq!(class(&a, &d("a.ts", 2, 2322, "x")), Some("diag:span-only"));
        assert_eq!(class(&a, &d("a.ts", 1, 2322, "z")), Some("diag:message-text-only"));
        assert_eq!(class(&a, &d("a.ts", 1, 2345, "x")), Some("diag:code-at-same-span"));
        let related = a.clone()
            + &format!(
                "R\thead/R0\t{}\t0\t1\t2728\t3\t{}\nM\thead/R0\tfalse\tfalse\tfalse\n",
                hex("a.ts"),
                hex("r")
            );
        assert_eq!(class(&related, &a), Some("diag:related-info"));
        // The flattened head embeds the chain: a missing chain is a chain
        // difference, not a message-text one.
        let chained = d("a.ts", 1, 2322, "x\n  c")
            + &format!(
                "C\thead/C0\t{}\t1\t1\t2322\t1\t{}\nM\thead/C0\tfalse\tfalse\tfalse\n",
                hex("a.ts"),
                hex("c")
            );
        assert_eq!(class(&chained, &a), Some("diag:chain"));
    }

    #[test]
    fn type_rows_are_classified_by_first_difference() {
        let s = format!("S\t{}\n", hex("a.ts"));
        let row = |line: u32, text: &str, ty: &str| {
            format!("T\t{}\t{line}\t{}\t{}\n", hex("a.ts"), hex(text), hex(ty))
        };
        let base = s.clone() + &row(0, "x", "number") + &row(1, "y", "string");
        assert_eq!(class(&base, &base), None);
        assert_eq!(
            class(&base, &(s.clone() + &row(0, "x", "any") + &row(1, "y", "string"))),
            Some("types:type-text")
        );
        assert_eq!(class(&base, &(s.clone() + &row(0, "x", "number"))), Some("types:missing-row"));
        assert_eq!(
            class(
                &base,
                &(s.clone() + &row(0, "q", "q") + &row(0, "x", "number") + &row(1, "y", "string"))
            ),
            Some("types:extra-row")
        );
        assert_eq!(
            class(&base, &(s.clone() + &row(1, "x", "number") + &row(1, "y", "string"))),
            Some("types:line-placement")
        );
        assert_eq!(class(&base, ""), Some("types:no-sections"));
    }

    #[test]
    fn blocker_attributes_the_first_differing_record() {
        let a = d("a.ts", 1, 2322, "x");
        let b = d("a.ts", 5, 2345, "y");
        let attributed = |native: &str, tsr: &str| {
            let (n, t) = (Stream::parse(native), Stream::parse(tsr));
            blocker(&n, &t).map(|b| (b.class, b.code, b.side, b.span.0))
        };
        // Missing: the native diagnostic; extra: the TSR one.
        assert_eq!(
            attributed(&(a.clone() + &b), &a),
            Some(("diag:missing", "TS2345".into(), "native", "5".into()))
        );
        assert_eq!(
            attributed(&a, &(a.clone() + &b)),
            Some(("diag:extra", "TS2345".into(), "tsr", "5".into()))
        );
        // Chain: the first differing chain record's code, not the head's.
        let chain = |code: u32| {
            format!(
                "C\thead/C0\t{}\t1\t1\t{code}\t1\t{}\nM\thead/C0\tfalse\tfalse\tfalse\n",
                hex("a.ts"),
                hex("c")
            )
        };
        assert_eq!(
            attributed(&(a.clone() + &chain(2326)), &(a.clone() + &chain(2328))),
            Some(("diag:chain", "TS2326".into(), "native", "1".into()))
        );
        assert_eq!(attributed(&a, &a), None);
    }

    #[test]
    fn sha256_matches_fips_vectors_across_padding_boundaries() {
        assert_eq!(
            sha256_bytes(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
        assert_eq!(
            sha256_bytes(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
        // 56 bytes: the length no longer fits the first block.
        assert_eq!(
            sha256_bytes(b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq"),
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1"
        );
        assert_eq!(
            sha256_bytes(&vec![b'a'; 1_000_000]),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }

    #[test]
    fn printed_paths_follow_the_baseline_replacer() {
        assert_eq!(printed_path("/.src/a.ts", true), "a.ts");
        assert_eq!(printed_path("/.ts-lib/lib.es5.d.ts", true), "lib.es5.d.ts");
        assert_eq!(printed_path("bundled:///libs/lib.es5.d.ts", false), "lib.es5.d.ts");
        assert_eq!(printed_path("/x/.src/a.ts", false), "/xa.ts");
    }
}
