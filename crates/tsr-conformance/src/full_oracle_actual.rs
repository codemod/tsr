//! Independent TSR compiler artifacts. No expected output enters this module.
use anyhow::{Context, Result, bail};
use tsr_diagnostics::Diagnostic;
use tsr_diagnostics::format::{DiagnosticFile, FormattingOptions, LocatedDiagnostic};
use crate::{TestCase, TestFile, full_oracle::{Artifacts, Configuration, hex, unhex}, types_producer};

/// Use existing option declarations rather than a second directive whitelist.
pub(crate) fn apply_compiler_settings(mut options: tsr_core::CompilerOptions, case: &TestCase, directory: &str) -> tsr_core::CompilerOptions {
    use tsr_tsoptions::{declarations::{COMPILER_OPTIONS, OptionKind}, value::ConfigValue};
    if options.new_line == tsr_core::NewLineKind::None { options.new_line = tsr_core::NewLineKind::CarriageReturnLineFeed; }
    if options.skip_default_lib_check.is_unknown() { options.skip_default_lib_check = tsr_core::Tristate::True; }
    options.no_error_truncation = tsr_core::Tristate::True;
    for (key, raw) in &case.options {
        let Some(declaration) = COMPILER_OPTIONS.iter().find(|d| d.name.eq_ignore_ascii_case(key)) else { continue };
        let convert = |raw: &str, kind: OptionKind| match kind {
            OptionKind::Boolean => ConfigValue::Bool(raw.eq_ignore_ascii_case("true")),
            OptionKind::Number => raw.parse::<f64>().map_or(ConfigValue::Null, ConfigValue::Number),
            _ => ConfigValue::String(if declaration.is_file_path { tsr_path::get_normalized_absolute_path(raw, directory) } else { raw.to_string() }),
        };
        let value = match declaration.kind {
            OptionKind::List(kind) => ConfigValue::List(raw.split(',').map(str::trim).filter(|s| !s.is_empty()).map(|s| convert(s, *kind)).collect()),
            OptionKind::PathMap => continue,
            kind => convert(raw, kind),
        };
        (declaration.apply)(&mut options, &value);
    }
    options
}

/// Compile one native configuration, retaining compiler file identity and byte spans.
pub fn produce(config: &Configuration) -> Result<Artifacts> {
    let source = std::fs::read_to_string(&config.source)?;
    let basename = config.source.file_name().context("source basename")?.to_string_lossy();
    let mut case = TestCase::parse(&config.id, &basename, &source);
    if let Some(error) = &case.error { bail!("{error}"); }
    // Settings are extracted/expanded by the pinned native configuration parser.
    // The source parser's global map must not restore unexpanded variations.
    case.options.clear();
    for line in config.settings.lines() {
        let (key, value) = line.split_once('=').context("native setting")?;
        case.options.insert(key.to_string(), unhex(value)?);
    }
    case.current_directory = Some(tsr_path::get_normalized_absolute_path(
        case.options.get("currentdirectory").map_or("", String::as_str), "/.src",
    ));
    let arena = tsr_core::Arena::new();
    let (program, parsed_config) = types_producer::program_and_config_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    checker.set_checked_files(program.source_files().iter().filter_map(|file| file.source_file().node_id));
    for (index, file) in program.source_files().iter().enumerate() {
        if tsr_compiler::program_diagnostics::skip_type_checking(&program, index, false).is_some() { continue; }
        if let Some(id) = file.source_file().node_id {
            checker.check_source_file(id, tsr_checker::check::FileContext {
                ambient: tsr_path::is_declaration_file_name(file.file_name()),
                has_parse_errors: !file.diagnostics().is_empty(),
            });
        }
    }
    let mut diagnostics = Vec::<(Option<String>, Diagnostic)>::new();
    for (index, file) in program.source_files().iter().enumerate() {
        diagnostics.extend(file.diagnostics().iter().cloned().map(|d| (Some(file.file_name().to_string()), d)));
        let Some(id) = file.source_file().node_id else { continue };
        diagnostics.extend(checker.js_syntax_diagnostics(id).into_iter().filter(|(owner, _)| *owner == id)
            .map(|(_, d)| (Some(file.file_name().to_string()), d)));
        let jsdoc: Vec<_> = file.jsdoc().iter().collect();
        let bound = tsr_binder::bind_into_with_jsdoc(tsr_binder::BindResult::empty(), &arena,
            file.source_file(), program.nodes(), tsr_binder::FileInfo { name: file.file_name(), text: file.text() }, &jsdoc);
        let checked = checker.diagnostics().iter().filter(|(owner, _)| *owner == id).map(|(_, d)| d.clone());
        diagnostics.extend(tsr_compiler::program_diagnostics::bind_and_check_diagnostics(&program, index, bound.diagnostics(), checked)
            .into_iter().map(|d| (Some(file.file_name().to_string()), d)));
        diagnostics.extend(tsr_compiler::program_diagnostics::include_processor_diagnostics(&program, index)
            .into_iter().map(|d| (Some(file.file_name().to_string()), d)));
        let options = program.compiler_options();
        if (options.declaration.is_true() || options.composite.is_true()) && options.isolated_declarations.is_true()
            && !tsr_path::is_declaration_file_name(file.file_name()) && !file.file_name().ends_with(".json") {
            diagnostics.extend(tsr_dts::analyze_with_options(file.source_file(), program.nodes(), tsr_dts::AnalysisOptions {
                strict_null_checks: options.strict_option_value(options.strict_null_checks),
            }).into_iter().map(|d| (Some(file.file_name().to_string()), d)));
        }
    }
    if let Some(config) = &parsed_config {
        diagnostics.extend(config.error_files.iter().cloned().zip(config.errors.iter().cloned()));
    }
    // Config diagnostics have a source image even when the config is not a
    // Program source file. Canonical Program text replaces matching input images.
    let mut images: std::collections::BTreeMap<_, _> = case.files.iter().map(|file|
        (absolute(&case, &file.name), std::sync::Arc::new(DiagnosticFile::new(absolute(&case, &file.name), &file.content)))).collect();
    images.extend(program.source_files().iter().map(|file|
        (file.file_name().to_string(), std::sync::Arc::new(DiagnosticFile::new(file.file_name(), file.text())))));
    for (file, diagnostic) in &mut diagnostics {
        if let Some(image) = file.as_ref().and_then(|name| images.get(name)) { diagnostic.set_file(image.clone()); }
    }
    diagnostics.sort_by(|(_, a), (_, b)| LocatedDiagnostic {
        file: a.file(), diagnostic: a,
    }.compare(&LocatedDiagnostic { file: b.file(), diagnostic: b }));
    diagnostics.dedup_by(|(_, a), (_, b)| tsr_diagnostics::equal_diagnostics_no_related_info(a, b));
    let mut texts: std::collections::BTreeMap<_, _> = case.files.iter().map(|f| (absolute(&case, &f.name), f.content.as_str())).collect();
    texts.extend(program.source_files().iter().map(|f| (f.file_name().to_string(), f.text())));
    let mut semantic = String::new();
    for (file, d) in &diagnostics {
        semantic.push_str(&semantic_diagnostic(file.as_deref(), &texts, d)?);
        semantic.push('\n');
    }
    // The native writer's first guard uses actual collected errors, never baseline presence.
    let ordered = baseline_files(&case, parsed_config.as_ref());
    let type_files: Vec<_> = ordered.iter().filter(|f| program.source_file(&f.name).is_some()).cloned().collect();
    let types = if case.options.get("notypesandsymbols").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        b"<no content>".to_vec()
    } else {
        let assertions = types_producer::render_with_checker(&program, &type_files, false, None, &mut checker, !diagnostics.is_empty());
        type_baseline(&config.source, &type_files, &assertions).into_bytes()
    };
    let errors = error_baseline(&case, &ordered, &diagnostics)?.into_bytes();
    Ok(Artifacts { diagnostics: semantic, errors, types })
}

fn absolute(case: &TestCase, name: &str) -> String {
    tsr_path::get_normalized_absolute_path(name, case.current_directory.as_deref().unwrap_or("/.src"))
}

fn baseline_files(case: &TestCase, config: Option<&tsr_tsoptions::ParsedCommandLine>) -> Vec<TestFile> {
    // makeUnitsFromTest removes only the first config unit. Further JSON/config
    // inputs remain filesystem/baseline units, even when not Program sources.
    let config_index = case.files.iter().position(|f| crate::trace_case::config_name_from_file_name(&f.name).is_some());
    let mut files: Vec<_> = config_index.into_iter().map(|index| case.files[index].clone()).collect();
    let units: Vec<_> = case.files.iter().enumerate().filter(|(index, _)| Some(*index) != config_index).map(|(_, file)| file).collect();
    let roots = config.map(|c| c.file_names.clone()).unwrap_or_else(||
        crate::trace_case::root_files_without_a_config(case, &case.files, case.current_directory.as_deref().unwrap_or("/.src")));
    files.extend(units.iter().filter(|f| roots.contains(&absolute(case, &f.name))).map(|f| (*f).clone()));
    files.extend(units.iter().filter(|f| !roots.contains(&absolute(case, &f.name))).map(|f| (*f).clone()));
    for file in &mut files { file.name = absolute(case, &file.name); }
    // Input aliases keep their own source echo; canonical ownership applies only to diagnostics.
    files
}

fn semantic_diagnostic(file: Option<&str>, texts: &std::collections::BTreeMap<String, &str>, d: &Diagnostic) -> Result<String> {
    let (start, length) = if let Some(file) = file {
        let text = texts.get(file).context("diagnostic source image unavailable")?;
        let before = text.get(..d.span.start as usize).context("diagnostic start outside source")?;
        let marked = text.get(d.span.start as usize..d.span.end as usize).context("diagnostic end outside source")?;
        (before.encode_utf16().count(), marked.encode_utf16().count())
    } else { (0, 0) };
    // The model lacks per-diagnostic getters. Message defaults are not evidence
    // of dynamic flags; do not infer any of the three native fields.
    let mut out = format!("{},{},{},{},{},{},{},{},{},unavailable:reportsUnnecessary,unavailable:reportsDeprecated,unavailable:skippedOnNoEmit,[", hex(file.unwrap_or("")),
        d.span.start, d.span.len(), start, length, d.message.code(), d.message.category() as u8,
        hex(d.message.key()), hex(&d.text()));
    for arg in &d.args { out.push_str(&hex(arg)); out.push(';'); }
    out.push_str("],[");
    for child in d.message_chain() {
        let child_file = child.file().map(DiagnosticFile::file_name);
        // Native chains inherit locations only when explicitly attached.
        out.push_str(&hex(&semantic_diagnostic(child_file, texts, child)?)); out.push(';');
    }
    out.push_str("],[");
    for related in d.related_information() {
        let related_file = related.file().map(DiagnosticFile::file_name);
        out.push_str(&hex(&semantic_diagnostic(related_file, texts, related)?)); out.push(';');
    }
    out.push(']');
    Ok(out)
}

/// Exact pinned prefix replacements, confined to rendered baselines.
pub(crate) fn remove_prefixes(text: &str) -> String {
    let pairs = [("/.ts/", ""), ("/.lib/", ""), ("/.src/", ""), ("bundled:///libs/", ""),
        ("file:///./ts/", "file:///"), ("file:///./lib/", "file:///"), ("file:///./src/", "file:///")];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while !rest.is_empty() {
        if let Some((from, to)) = pairs.iter().find(|(from, _)| rest.starts_with(from)) {
            out.push_str(to); rest = &rest[from.len()..];
        } else { let ch = rest.chars().next().expect("nonempty"); out.push(ch); rest = &rest[ch.len_utf8()..]; }
    }
    out
}

fn type_baseline(source: &std::path::Path, files: &[TestFile], assertions: &[Vec<types_producer::Assertion>]) -> String {
    if files.is_empty() { return "<no content>".into(); }
    let path = source.to_string_lossy().replace('\\', "/");
    let header = format!("tests/{}", path.rsplit_once("/tests/").map_or(path.as_str(), |(_, rest)| rest));
    let mut out = format!("//// [{header}] ////\r\n\r\n");
    for (file, results) in files.iter().zip(assertions) {
        // Go regexp alternatives put standalone CR first, including CRLF.
        let lines: Vec<_> = file.content.split(['\r', '\n', '\u{2028}', '\u{2029}']).collect();
        let mut section = format!("=== {} ===\r\n", file.name);
        let mut last = None;
        let blank_or_bracket = |line: &str| matches!(line.trim(), "" | "{" | "}" | "|");
        for result in results {
            let line = result.source_line as usize;
            if last != Some(line) {
                let first = last.map_or(0, |last| last + 1);
                if last.is_some() && !lines.get(first).is_some_and(|s| blank_or_bracket(s)) { section.push_str("\r\n"); }
                section.push_str(&lines[first..=line].join("\r\n")); section.push_str("\r\n");
            }
            last = Some(line);
            section.push('>'); section.push_str(&result.line()); section.push_str("\r\n");
        }
        let first = last.map_or(0, |last| last + 1);
        if first < lines.len() {
            if !blank_or_bracket(lines[first]) { section.push_str("\r\n"); }
            section.push_str(&lines[first..].join("\r\n"));
        }
        section.push_str("\r\n");
        out.push_str(&remove_prefixes(&section));
    }
    out
}

fn error_baseline(case: &TestCase, files: &[TestFile], diagnostics: &[(Option<String>, Diagnostic)]) -> Result<String> {
    if diagnostics.is_empty() { return Ok("<no content>".into()); }
    let indexed: Vec<_> = files.iter().map(|f| DiagnosticFile::new(&f.name, &f.content)).collect();
    let located: Vec<_> = diagnostics.iter().map(|(name, diagnostic)| LocatedDiagnostic {
        file: diagnostic.file().or_else(|| name.as_ref().and_then(|name| indexed.iter().find(|f| f.file_name() == name))), diagnostic,
    }).collect();
    let mut options = FormattingOptions::new(String::new(), true);
    options.newline = "\r\n".into();
    let pretty = case.options.get("pretty").is_some_and(|v| v.eq_ignore_ascii_case("true"));
    let mut top = String::new();
    if pretty { tsr_diagnostics::format::write_format_diagnostics_with_color_and_context(&mut top, &located, &options); }
    else { tsr_diagnostics::format::write_format_diagnostics(&mut top, &located, &options); }
    let mut out = remove_prefixes(&top); out.push_str("\r\n\r\n");
    let mut first = true;
    let newline = |out: &mut String, first: &mut bool| { if *first { *first = false; } else { out.push_str("\r\n"); } };
    let message = |out: &mut String, first: &mut bool, d: &Diagnostic| {
        let mut flattened = String::new();
        tsr_diagnostics::format::write_flattened_diagnostic_message(&mut flattened, d, "\r\n");
        for line in remove_prefixes(&flattened).lines().filter(|line| !line.is_empty()) {
            newline(out, first);
            out.push_str(&format!("!!! {} TS{}: {}", d.message.category().name(), d.message.code(), line));
        }
        for related in d.related_information() {
            newline(out, first);
            let mut location = String::new();
            if let Some(file) = related.file() {
                let (line, column) = file.line_and_character(related.span.start);
                let basename = tsr_path::get_base_file_name(file.file_name());
                location = if basename.starts_with("lib.") && basename.ends_with(".d.ts") {
                    format!(" {}:--:--", remove_prefixes(file.file_name()))
                } else { format!(" {}:{}:{}", remove_prefixes(file.file_name()), line + 1, column + 1) };
            }
            let mut text = String::new();
            tsr_diagnostics::format::write_flattened_diagnostic_message(&mut text, related, "\r\n");
            out.push_str(&format!("!!! related TS{}{location}: {text}", related.message.code()));
        }
    };
    for (file, diagnostic) in diagnostics { if file.is_none() { message(&mut out, &mut first, diagnostic); } }
    for file in files {
        let errors: Vec<_> = diagnostics.iter().filter(|(name, _)| name.as_deref() == Some(file.name.as_str())).collect();
        newline(&mut out, &mut first);
        out.push_str(&format!("==== {} ({} errors) ====", remove_prefixes(&file.name), errors.len()));
        let starts = tsr_core::ecma_line_starts(&file.content);
        let lines: Vec<_> = file.content.split('\n').map(|line| line.strip_suffix('\r').unwrap_or(line)).collect();
        let mut marked = 0;
        for (index, line) in lines.iter().enumerate() {
            newline(&mut out, &mut first); out.push_str("    "); out.push_str(line);
            let start = starts[index] as usize;
            let next = starts.get(index + 1).map_or(file.content.len(), |n| *n as usize);
            for (_, d) in &errors {
                let ds = d.span.start as usize; let end = d.span.end as usize;
                if end >= start && (ds < next || index == lines.len() - 1) {
                    let offset = ds.saturating_sub(start);
                    let length = end.saturating_sub(ds).saturating_sub(start.saturating_sub(ds));
                    let prefix = line.get(..offset).context("squiggle start outside line")?;
                    let suffix = line.get(offset..(offset + length).min(line.len())).context("squiggle end outside line")?;
                    newline(&mut out, &mut first); out.push_str("    ");
                    out.extend(prefix.chars().map(|ch| if ch.is_whitespace() { ch } else { ' ' }));
                    out.extend(std::iter::repeat_n('~', suffix.chars().count()));
                    if index == lines.len() - 1 || next > end { message(&mut out, &mut first, d); marked += 1; }
                }
            }
        }
        if marked != errors.len() { bail!("unmarked diagnostics for {}: {marked}/{}", file.name, errors.len()); }
    }
    if pretty { let mut summary = String::new(); tsr_diagnostics::format::write_error_summary_text(&mut summary, &located, &options); out.push_str(&remove_prefixes(&summary)); }
    Ok(out)
}
