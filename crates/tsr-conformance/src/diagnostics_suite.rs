//! `diagnostics`: every diagnostic upstream reports, by code and position.
//!
//! # The other half of a checker's conformance
//!
//! [`crate::types_suite`] gates the types a checker computes. This gates the
//! *errors* it reports, and that is the larger half: a `.errors.txt` baseline is
//! what a user sees.
//!
//! # The set is the test runner's, collected the way it collects it
//!
//! `compileFilesWithHost` (`internal/testutil/harnessutil/harnessutil.go:612`)
//! concatenates, per program, `GetConfigFileParsingDiagnostics`,
//! `GetProgramDiagnostics`, `GetSyntacticDiagnostics`, `GetSemanticDiagnostics`,
//! `GetGlobalDiagnostics` and — when `GetEmitDeclarations()` —
//! `GetDeclarationDiagnostics`, then applies `SortAndDeduplicateDiagnostics`.
//! [`reported_for`] follows that shape over the program
//! `types_producer::program_for_case` builds (called rather than copied:
//! `docs/conventions.md`'s *"a probe that re-implements the harness is measuring
//! a different compiler"* applies to suites first of all):
//!
//! - **syntactic** — each file's parse diagnostics, unfiltered;
//! - **semantic** — `getBindAndCheckDiagnosticsWithChecker`
//!   (`compiler/program.go:1352`): nothing for a file `SkipTypeChecking` skips,
//!   binder plus checker diagnostics otherwise, reduced to `plainJSErrors` in a
//!   plain JavaScript file, and passed through the `@ts-ignore` /
//!   `@ts-expect-error` filter; then the file's include-processor (loader)
//!   diagnostics through the same filter (`getSemanticDiagnosticsWithChecker`,
//!   `program.go:1342`);
//! - **declaration** — the `isolatedDeclarations` `TS9xxx` family from
//!   `tsr_dts`, and the accessibility errors for names written in emitted
//!   declarations (`tsr_dts::accessibility` over the checker's
//!   `DeclarationEmitResolver`), for each emitted file, when declarations
//!   are emitted.
//!
//! The global and options halves have no position, and the comparison below is
//! positional: [`errors_baseline::parse`] does not read a line without one.
//!
//! # Only cases that expect at least one diagnostic are judged
//!
//! A case whose baseline records **no** diagnostics is a real expectation, and
//! reproducing it means reporting nothing — but "we report nothing" is already
//! measured, thoroughly, by `parser_typescript` (99.36%) and
//! `scanner_clean_files` (100%). Counting those cases here would add several
//! thousand near-automatic passes and bury the number that matters: **of the
//! diagnostics upstream reports, how many do we?**
//!
//! The false-positive direction is not lost. A judged case that expects three
//! diagnostics and gets four fails, because the comparison is set equality.
//!
//! # What a pass means
//!
//! The **exact multiset of (file, line, column, code)**, compared after sorting.
//! Not a subset, not "the codes we know about" — a diagnostic we invent fails the
//! case as surely as one we miss.
//!
//! Positions are 1-based, as the baselines write them, and the column is a UTF-16
//! code unit offset. Getting that wrong shifts every diagnostic on a line
//! containing an astral character, which is the kind of error that looks like a
//! checker bug for a week.
//!
//! # Exclusions
//!
//! Ordered so each reason means what it says — the trap
//! `docs/architecture/checker-oracle.md` records for the two `.types`/`.symbols`
//! suites:
//!
//! - **Configuration-varied** baselines. A case whose directives vary
//!   (`@target: es2015, esnext`) is compiled by upstream only under each named
//!   configuration, never as itself, so this row skips it and
//!   [`DiagnosticsConfigured`] judges each configuration against its own
//!   `case(target=es2015).errors.txt` (ADR-0047).
//! - **Known divergences** (`.errors.txt.diff`): upstream records that its own
//!   output differs from TypeScript's, so the baseline is not a specification.
//! - **Cases upstream recorded no output for at all.** 617 of them. A missing
//!   `.errors.txt` means "no diagnostics" only when some other baseline proves the
//!   case ran; without that the absence proves nothing, and reading it as a clean
//!   expectation hands out free passes.

use std::collections::{HashMap, HashSet};

use tsr_ast::NodeId;
use tsr_compiler::{Program, ProgramFile, program_diagnostics};
use tsr_diagnostics::Diagnostic;

use crate::{
    CaseEntry,
    errors_baseline::{self, BaselineDiagnostic},
    suite::{Outcome, Suite},
    symbols_baseline::line_and_character,
    types_producer::{program_and_config_for_case, program_for_case},
};

/// The `diagnostics` suite.
pub struct Diagnostics;

impl Suite for Diagnostics {
    fn name(&self) -> &'static str {
        "diagnostics"
    }

    fn describes(&self) -> &'static str {
        "every diagnostic in upstream's .errors.txt reproduced exactly — same file, \
         line, column and code, no more and no fewer — for cases that expect at \
         least one; syntactic, bind-and-check and isolatedDeclarations diagnostics \
         collected as upstream's test runner collects them"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if case.configuration.is_none() && case.has_varied_errors() {
            return Outcome::Skipped {
                reason: "configuration-varied baseline, judged per configuration by \
                         diagnostics_configured"
                    .into(),
            };
        }
        judge_compilation(case)
    }
}

/// `diagnostics`, over each named configuration of a case whose directives
/// vary ([`crate::Corpus::configured`]), against that configuration's own
/// `case(<configuration>).errors.txt`.
///
/// The judgement is [`Diagnostics`]' exactly; only the population differs,
/// and it is reported as its own row so the plain row's denominator does not
/// move (ADR-0047).
pub struct DiagnosticsConfigured;

impl Suite for DiagnosticsConfigured {
    fn name(&self) -> &'static str {
        "diagnostics_configured"
    }

    fn describes(&self) -> &'static str {
        "the diagnostics suite's judgement for each configuration upstream's runner \
         compiles a configuration-varied case under (`// @target: es2015, esnext`), \
         against that configuration's suffixed .errors.txt"
    }

    fn run(&self, case: &CaseEntry) -> Outcome {
        if case.configuration.is_none() {
            return Outcome::Skipped { reason: "not a named configuration".into() };
        }
        judge_compilation(case)
    }

    fn per_configuration(&self) -> bool {
        true
    }
}

/// Why upstream wrote nothing for a compilation: its runner skipped the
/// options (`SkipUnsupportedCompilerOptions`, which the variants reach most
/// often — `target=es5`, `module=system`), or it recorded no output for a
/// reason this harness cannot see.
///
/// Shared with [`crate::types_suite`], so both per-configuration rows give
/// a skipped configuration the same reason.
#[must_use]
pub fn no_output_reason(case: &CaseEntry) -> String {
    if case.configuration.is_some()
        && let crate::trace_case::CompilationSetup::Skip(reason) =
            crate::trace_case::prepare_compilation(case)
    {
        return reason;
    }
    "upstream recorded no output for this case".into()
}

/// One compilation's judgement: the case as discovered, or one of its named
/// configurations.
fn judge_compilation(case: &CaseEntry) -> Outcome {
    if case.has_known_divergence() {
        return Outcome::Skipped {
            reason: "upstream records a known divergence from TypeScript".into(),
        };
    }
    // Absence is evidence only when something else proves the case ran.
    if !case.has_any_baseline() {
        return Outcome::Skipped { reason: no_output_reason(case) };
    }
    let Ok(baseline) = case.expected_errors() else {
        return Outcome::Failed { reason: "baseline did not load".into() };
    };
    let mut expected: Vec<BaselineDiagnostic> =
        baseline.as_deref().map(errors_baseline::parse).unwrap_or_default();
    if expected.is_empty() {
        return Outcome::Skipped {
            reason: "the case expects no diagnostics (see parser_typescript)".into(),
        };
    }
    expected.sort_unstable();

    let Ok(test) = case.load() else {
        return Outcome::Failed { reason: "case did not load".into() };
    };

    let mut actual = reported_for(&test);
    actual.sort_unstable();

    if actual == expected {
        return Outcome::Passed;
    }
    Outcome::Failed { reason: summarise(&expected, &actual) }
}

/// Every diagnostic this port reports for a case, positioned as the baseline
/// prints it.
///
/// **Public, and the suite calls it rather than inlining it**, because
/// `examples/diaggap.rs` ranks the suite's failures by code and a probe that
/// re-derives this set is ranking a different compiler's failures
/// (`docs/conventions.md`). Unsorted: the caller sorts, because the comparison
/// is a sorted-multiset equality and doing it twice hides which side is which.
#[must_use]
pub fn reported_for(test: &crate::TestCase) -> Vec<BaselineDiagnostic> {
    collect(test).into_iter().map(|(key, _)| key).collect()
}

/// Every diagnostic this port reports, as `((file, line, column, code), rendered
/// message)`.
///
/// [`reported_for`] drops the message because the suite compares only the four
/// printed fields; the baselines carry the text as well, and auditing it is what
/// `diagtext` does. Built by rendering each message template against its
/// arguments — `Message::format`, the same substitution upstream prints — over
/// the same collection [`reported_for`] reads.
///
/// `docs/architecture/checker-notes-diag2.md` §998.
#[must_use]
pub fn rendered_for(test: &crate::TestCase) -> Vec<((String, u32, u32, u32), String)> {
    collect(test)
        .into_iter()
        .map(|(key, diagnostic)| {
            let args: Vec<&str> = diagnostic.args.iter().map(String::as_str).collect();
            ((key.file, key.line, key.column, key.code), diagnostic.message.format(&args))
        })
        .collect()
}

/// One of the case's units, as the program holds it.
struct Unit<'p, 'a> {
    /// Index into `Program::source_files`.
    index: usize,
    /// The file's `SourceFile` node.
    id: NodeId,
    /// The name the baseline prints: the unit that canonically owns the file.
    name: String,
    file: &'p ProgramFile<'a>,
}

/// The case's units that are program files, once per file.
///
/// `Program.getBindAndCheckDiagnostics` diagnoses canonical source files, not
/// every input spelling: a package redirect may make an input resolve to
/// another unit's AST, and attributing its spans to the redirect's name
/// reports a diagnostic in the wrong file. A unit the program never loaded —
/// JavaScript without `allowJs`, a `.json` nothing imports, a file no root
/// reaches — has no diagnostics at all, as upstream's `p.files` does not
/// contain it.
fn program_units<'p, 'a>(test: &crate::TestCase, program: &'p Program<'a>) -> Vec<Unit<'p, 'a>> {
    let current_directory = tsr_path::get_normalized_absolute_path(
        test.current_directory.as_deref().unwrap_or("/"),
        "/",
    );
    let case_sensitive = test
        .options
        .get("usecasesensitivefilenames")
        .is_none_or(|value| !value.eq_ignore_ascii_case("false"));
    let names: HashMap<_, _> = test
        .files
        .iter()
        .map(|unit| (tsr_path::to_path(&unit.name, &current_directory, case_sensitive), &unit.name))
        .collect();
    let mut seen = HashSet::new();
    let mut units = Vec::new();
    for unit in &test.files {
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        if !seen.insert(id) {
            continue;
        }
        let Some(index) =
            program.source_files().iter().position(|candidate| std::ptr::eq(candidate, file))
        else {
            continue;
        };
        let name = printed_name(names.get(file.path()).copied().unwrap_or(&unit.name), test);
        units.push(Unit { index, id, name, file });
    }
    units
}

/// The file name a baseline header prints for a unit.
///
/// The runner names each unit `GetNormalizedAbsolutePath(unit.name,
/// currentDirectory)` with `currentDirectory` defaulting to `/.src`
/// (`createHarnessTestFile`, `testrunner/compiler_runner.go:521`), and the
/// error baseline prints it through `ConvertToRelativePath` with an empty
/// current directory, which keeps a rooted path whole, then
/// `removeTestPathPrefixes` (`testutil/tsbaseline/util.go:44`). So `./a.js`
/// prints as `a.js`, and `/foo.js` as itself.
fn printed_name(unit_name: &str, test: &crate::TestCase) -> String {
    const SRC_FOLDER: &str = "/.src";
    let current_directory = tsr_path::get_normalized_absolute_path(
        test.current_directory.as_deref().unwrap_or(""),
        SRC_FOLDER,
    );
    let absolute = tsr_path::get_normalized_absolute_path(unit_name, &current_directory);
    // `testPathPrefixReplacer`'s path arms; the URL arms cannot occur in a
    // rooted file name.
    absolute.replace("/.ts/", "").replace("/.lib/", "").replace("/.src/", "")
}

/// The collection [`reported_for`] and [`rendered_for`] share; see the module
/// documentation for the shape.
fn collect(test: &crate::TestCase) -> Vec<(BaselineDiagnostic, Diagnostic)> {
    let arena = tsr_core::Arena::new();
    let (program, config) = program_and_config_for_case(&arena, test);
    let options = program.compiler_options();
    let units = program_units(test, &program);

    // `SkipTypeChecking(file, false)`, once per unit: it gates both the check
    // walk and the collection.
    let skipped: Vec<bool> = units
        .iter()
        .map(|unit| program_diagnostics::skip_type_checking(&program, unit.index, false).is_some())
        .collect();

    // Match the type producer's module host, JSDoc tables, and compiler options.
    // Without the tables, annotated JavaScript can silently check as `any` or
    // infer only from its initializer instead of its declared type.
    let mut checker = crate::types_producer::configured_checker(&program);
    // `set_checked_files` before the first `check_source_file`, because the set
    // is a property of the program: a rule that asks "is this declaration in a
    // library" would otherwise get an answer that depends on how far the loop
    // below had got.
    let is_json = |unit: &Unit<'_, '_>| {
        tsr_parser::ScriptKind::from_file_name(unit.file.file_name())
            == tsr_parser::ScriptKind::Json
    };
    checker.set_checked_files(units.iter().filter(|unit| !is_json(unit)).map(|unit| unit.id));
    for (unit, skip) in units.iter().zip(&skipped) {
        if *skip {
            continue;
        }
        // Upstream's parser sets `NodeFlagsAmbient` on every node of a
        // declaration file; this port's does not, so the bit is supplied here.
        checker.check_source_file(
            unit.id,
            tsr_checker::check::FileContext {
                ambient: tsr_path::is_declaration_file_name(unit.file.file_name()),
                has_parse_errors: !unit.file.diagnostics().is_empty(),
            },
        );
    }
    // `sourceFile.JSDiagnostics()`, which this port's checker produces on the
    // parser's behalf; syntactic, so asked of every unit.
    let mut from_js_syntax: Vec<(usize, Diagnostic)> = Vec::new();
    for (position, unit) in units.iter().enumerate() {
        for (file, diagnostic) in checker.js_syntax_diagnostics(unit.id) {
            if file == unit.id {
                from_js_syntax.push((position, diagnostic));
            }
        }
    }
    // `checker.GetDiagnostics(ctx, sourceFile)`: the checker's collection,
    // bucketed by the file each diagnostic is in.
    let mut from_checker: HashMap<NodeId, Vec<&Diagnostic>> = HashMap::new();
    for (file, diagnostic) in checker.diagnostics() {
        from_checker.entry(*file).or_default().push(diagnostic);
    }

    let mut reported: Vec<(usize, Diagnostic)> = from_js_syntax;
    for (position, (unit, skip)) in units.iter().zip(&skipped).enumerate() {
        // `GetSyntacticDiagnostics` (`program.go:626`): the parse diagnostics,
        // which no gate below touches.
        reported.extend(unit.file.diagnostics().iter().map(|d| (position, d.clone())));
        if *skip {
            continue;
        }
        // `getSemanticDiagnosticsWithChecker` (`program.go:1342`): bind and
        // check, then the include processor's.
        let from_checker = from_checker.get(&unit.id).into_iter().flatten().map(|d| (*d).clone());
        let bound = file_bind_diagnostics(&arena, &program, unit.file);
        let semantic = program_diagnostics::bind_and_check_diagnostics(
            &program,
            unit.index,
            &bound,
            from_checker,
        );
        reported.extend(semantic.into_iter().map(|d| (position, d)));
        let included = program_diagnostics::include_processor_diagnostics(&program, unit.index);
        reported.extend(included.into_iter().map(|d| (position, d)));
    }

    // `GetDeclarationDiagnostics` (`program.go:1329`) when
    // `GetEmitDeclarations()` (`core/compileroptions.go:349`): the
    // `isolatedDeclarations` family (`tsr_dts::analyze`, ADR-0021) and the
    // accessibility errors for names written in emitted declarations
    // (`tsr_dts::accessibility`, over the checker's `EmitResolver`). Of the
    // errors the node builder's `SymbolTracker` raises for *inferred* types,
    // only the arms in `docs/parity/notes/r5-declemit2.md` §3 and
    // `r5-declemit3.md` §2 have a producer
    // (`r4-declemit.md` §3 for the rest).
    let emit_declarations = options.declaration.is_true() || options.composite.is_true();
    if emit_declarations {
        let isolated = options.isolated_declarations.is_true();
        let analysis = tsr_dts::AnalysisOptions {
            strict_null_checks: options.strict_option_value(options.strict_null_checks),
        };
        let mut resolver = EmitResolverAdapter(
            tsr_checker::symbol_access::DeclarationEmitResolver::new(&mut checker),
        );
        for (position, unit) in units.iter().enumerate() {
            // `getDeclarationDiagnostics` (`compiler/emitter.go:520`) over
            // `sourceFileMayBeEmitted` (`emitter.go:452`): neither declaration
            // files nor JSON. The `IsSourceFileFromExternalLibrary` arm needs a
            // loader fact this port's program does not record.
            if tsr_path::is_declaration_file_name(unit.file.file_name()) || is_json(unit) {
                continue;
            }
            if isolated {
                let found = tsr_dts::analyze_with_options(
                    unit.file.source_file(),
                    program.nodes(),
                    analysis,
                );
                reported.extend(found.into_iter().map(|d| (position, d)));
            }
            // Declined, never invented: JavaScript declarations go through the
            // node builder (`TryJSTypeNodeToTypeNode`), and a file under
            // `node_modules` stands in for `IsSourceFileFromExternalLibrary`
            // — skipping one that upstream emits loses its errors, never adds.
            let javascript =
                program.nodes().flags(unit.id).contains(tsr_ast::NodeFlags::JAVASCRIPT_FILE);
            if javascript || unit.file.file_name().contains("/node_modules/") {
                continue;
            }
            let found = tsr_dts::accessibility::declaration_walk_diagnostics(
                unit.id,
                program.nodes(),
                program.node_map(),
                unit.file.text(),
                tsr_dts::accessibility::WalkOptions { isolated_declarations: isolated },
                &mut resolver,
            );
            reported.extend(found.into_iter().map(|d| (position, d)));
        }
    }

    // `SortAndDeduplicateDiagnostics` (`compiler/program.go:1454`), which the
    // test harness applies to the whole list (`harnessutil.go:645` and `:661`).
    // **The key is upstream's `EqualDiagnosticsNoRelatedInfo`: file, the whole
    // span, the code and the arguments.** Deduplicating on the *printed* form —
    // file, line, column, code — is too coarse: `commaOperator1`'s baseline
    // records three TS2695 at `(1,11)` which differ only in span **length**.
    // §995.
    let mut seen: HashSet<(usize, u32, u32, u32, Vec<String>)> = HashSet::new();
    let mut out = Vec::with_capacity(reported.len());
    for (position, diagnostic) in reported {
        if !seen.insert((
            position,
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.message.code(),
            diagnostic.args.clone(),
        )) {
            continue;
        }
        let unit = &units[position];
        let (line, character) = line_and_character(unit.file.text(), diagnostic.span.start);
        out.push((
            BaselineDiagnostic {
                file: unit.name.clone(),
                line: line + 1,
                column: character + 1,
                code: diagnostic.message.code(),
            },
            diagnostic,
        ));
    }
    if let Some(config) = &config {
        out.extend(config_file_parsing_diagnostics(test, config));
    }
    out
}

/// The checker's `EmitResolver` accessibility half as the declaration
/// walk's resolver; the two result types mirror one another.
struct EmitResolverAdapter<'c, 'a, 'n>(
    tsr_checker::symbol_access::DeclarationEmitResolver<'c, 'a, 'n>,
);

impl tsr_dts::accessibility::AccessibilityResolver for EmitResolverAdapter<'_, '_, '_> {
    fn precalculate_declaration_emit_visibility(&mut self, file: NodeId) {
        self.0.precalculate_declaration_emit_visibility(file);
    }

    fn is_declaration_visible(&mut self, node: NodeId) -> bool {
        self.0.is_declaration_visible(node)
    }

    fn is_entity_name_visible(
        &mut self,
        entity_name: NodeId,
        enclosing: NodeId,
    ) -> tsr_dts::accessibility::EntityNameVisibility {
        use tsr_checker::symbol_access::EmitAccessibility as A;
        use tsr_dts::accessibility::EntityNameVisibility as V;
        match self.0.is_entity_name_visible(entity_name, enclosing) {
            A::Accessible { aliases_to_make_visible } => V::Accessible(aliases_to_make_visible),
            A::NotAccessible { error_symbol_name, error_node } => {
                V::NotAccessible { symbol_name: error_symbol_name, error_node }
            }
            A::NotResolved => V::NotResolved,
        }
    }

    fn is_implementation_of_overload(&mut self, node: NodeId) -> bool {
        self.0.is_implementation_of_overload(node)
    }

    fn is_import_required_by_augmentation(&mut self, import: NodeId) -> bool {
        self.0.is_import_required_by_augmentation(import)
    }

    fn inferred_type_reports(
        &mut self,
        node: NodeId,
    ) -> Vec<tsr_dts::accessibility::TrackerReport> {
        use tsr_checker::symbol_access::TrackerReport as Checker;
        use tsr_dts::accessibility::TrackerReport as Walk;
        self.0
            .inferred_type_reports(node)
            .into_iter()
            .map(|report| match report {
                Checker::PrivateInBaseOfClassExpression(name) => {
                    Walk::PrivateInBaseOfClassExpression(name)
                }
                Checker::LikelyUnsafeImportRequired { specifier, symbol_name } => {
                    Walk::LikelyUnsafeImportRequired { specifier, symbol_name }
                }
                Checker::InaccessibleUniqueSymbol => Walk::InaccessibleUniqueSymbol,
                Checker::TrackSymbol(result) => {
                    use tsr_checker::symbol_accessibility::SymbolAccessibility as From;
                    use tsr_dts::accessibility::SymbolAccessibility as To;
                    Walk::TrackSymbol(tsr_dts::accessibility::SymbolAccessibilityResult {
                        accessibility: match result.accessibility {
                            From::Accessible => To::Accessible,
                            From::NotAccessible => To::NotAccessible,
                            From::CannotBeNamed => To::CannotBeNamed,
                            From::NotResolved => To::NotResolved,
                        },
                        aliases_to_make_visible: result.aliases_to_make_visible,
                        error_symbol_name: result.error_symbol_name,
                        error_module_name: result.error_module_name,
                        error_node: result.error_node,
                    })
                }
            })
            .collect()
    }
}

/// `sourceFile.BindDiagnostics()`, from binding the file on its own.
///
/// Not [`Program::bind_diagnostics_of`], which the CLI reads: the program binds
/// every file into **one** store, and that store reports collisions *across*
/// files that upstream's per-file binder never sees: two
/// `export as namespace Alpha` files (`umdGlobalConflict`) earn a TS2300 that
/// upstream's checker merges away. A fresh store over the program's tree and
/// JSDoc table is upstream's per-file binding; the binder reads both and
/// mutates neither. Costs a program-sized side-table allocation per file,
/// which is why the CLI does not do it.
fn file_bind_diagnostics<'a>(
    arena: &'a tsr_core::Arena,
    program: &Program<'a>,
    file: &ProgramFile<'a>,
) -> Vec<Diagnostic> {
    let jsdoc: Vec<_> = file.jsdoc().iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        tsr_binder::BindResult::empty(),
        arena,
        file.source_file(),
        program.nodes(),
        tsr_binder::FileInfo { name: file.file_name(), text: file.text() },
        &jsdoc,
    );
    bound.diagnostics().to_vec()
}

/// `GetConfigFileParsingDiagnostics`: the errors of the case's `tsconfig.json`
/// parse, each in the config file it is positioned in. An error without a
/// position prints no `(line,column)` and is not part of the compared set.
fn config_file_parsing_diagnostics(
    test: &crate::TestCase,
    config: &tsr_tsoptions::ParsedCommandLine,
) -> Vec<(BaselineDiagnostic, Diagnostic)> {
    // The directory `program_and_config_for_case` named the config against.
    let current_directory = tsr_path::get_normalized_absolute_path(
        test.current_directory.as_deref().unwrap_or("/"),
        "/",
    );
    let mut seen: HashSet<(&str, u32, u32, u32, &[String])> = HashSet::new();
    let mut out = Vec::new();
    for (diagnostic, file) in config.errors.iter().zip(&config.error_files) {
        let Some(file) = file else { continue };
        let Some(unit) = test.files.iter().find(|unit| {
            tsr_path::get_normalized_absolute_path(&unit.name, &current_directory) == *file
        }) else {
            continue;
        };
        let key = (
            file.as_str(),
            diagnostic.span.start,
            diagnostic.span.end,
            diagnostic.message.code(),
            diagnostic.args.as_slice(),
        );
        if !seen.insert(key) {
            continue;
        }
        let (line, character) = line_and_character(&unit.content, diagnostic.span.start);
        out.push((
            BaselineDiagnostic {
                file: printed_name(&unit.name, test),
                line: line + 1,
                column: character + 1,
                code: diagnostic.message.code(),
            },
            diagnostic.clone(),
        ));
    }
    out
}

/// One position `report_assignability_failure` was asked about, and the gate
/// that decided it.
///
/// See [`assignability_probe_for`].
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct ProbedPosition {
    /// The case-relative file name, as `BaselineDiagnostic` spells it.
    pub file: String,
    /// 1-based, as the baselines are.
    pub line: u32,
    /// 1-based, as the baselines are.
    pub column: u32,
    /// One of `tsr_checker::assignreport::PROBE_*`.
    pub verdict: u8,
}

/// Every position the TS2322 site was reached at, for one case.
///
/// **Runs the shipped checker through the same program `from_check_traversal`
/// builds**, rather than re-deriving one: `docs/conventions.md`'s rule that a
/// probe re-implementing the harness measures a different compiler, and the
/// defect `probefile.rs` exists to prevent (a hand-rolled program silently
/// loses the `/.lib` mount).
///
/// Requires `TSR_ASSIGN_PROBE` in the environment; the checker records nothing
/// otherwise. See `docs/architecture/checker-notes-diag2.md` §172.
#[must_use]
pub fn assignability_probe_for(test: &crate::TestCase) -> Vec<ProbedPosition> {
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, test);
    let mut checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    checker.apply_compiler_options(program.compiler_options());

    let mut own_files = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        if let Some(file) = program.source_file(&unit.name)
            && let Some(id) = file.source_file().node_id
        {
            own_files.push(id);
        }
    }
    checker.set_checked_files(own_files);

    let mut units = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        let declaration_file = unit.name.ends_with(".d.ts")
            || unit.name.ends_with(".d.mts")
            || unit.name.ends_with(".d.cts");
        checker.check_source_file(
            id,
            tsr_checker::check::FileContext {
                ambient: declaration_file,
                has_parse_errors: !file.diagnostics().is_empty(),
            },
        );
        units.push((id, unit.name.clone(), file.text()));
    }

    let mut out = Vec::new();
    for &(file, span, verdict) in &checker.assignability_probe {
        let Some((_, unit_name, source)) = units.iter().find(|(id, _, _)| *id == file) else {
            continue;
        };
        let (line, character) = line_and_character(source, span.start);
        out.push(ProbedPosition {
            file: unit_name.clone(),
            line: line + 1,
            column: character + 1,
            verdict,
        });
    }
    out
}

/// The node kind at every position in the case's own files, keyed the way a
/// baseline line is keyed.
///
/// A diagnostic sits at `error_span(anchor)`, so a baseline line and the node
/// that would carry it join on `(file, line, column)`. Several nodes share one
/// position — `error_span` narrows a declaration to its name, and a name is
/// inside its declaration — so every kind at a position is returned and the
/// caller decides which to believe. §173.
#[must_use]
pub fn node_kinds_by_position_for(
    test: &crate::TestCase,
) -> Vec<(String, u32, u32, tsr_ast::SyntaxKind, Option<tsr_ast::SyntaxKind>)> {
    let arena = tsr_core::Arena::new();
    let program = program_for_case(&arena, test);
    let checker = tsr_checker::Checker::with_module_host(
        program.binder(),
        program.nodes(),
        program.node_map(),
        Some(&program),
    );
    let mut units = Vec::new();
    for unit in &test.files {
        if tsr_parser::ScriptKind::from_file_name(&unit.name) == tsr_parser::ScriptKind::Json {
            continue;
        }
        let Some(file) = program.source_file(&unit.name) else { continue };
        let Some(id) = file.source_file().node_id else { continue };
        units.push((id, unit.name.clone(), file.text()));
    }
    let nodes = program.nodes();
    let mut out = Vec::new();
    for index in 0..nodes.len() {
        let id = <tsr_ast::NodeId as tsr_core::Idx>::from_usize(index);
        let Some(file) = checker.source_file_of_diagnostics(id) else { continue };
        let Some((_, unit_name, source)) = units.iter().find(|(unit, _, _)| *unit == file) else {
            continue;
        };
        let span = checker.error_span_of(id);
        let (line, character) = line_and_character(source, span.start);
        out.push((
            unit_name.clone(),
            line + 1,
            character + 1,
            nodes.kind(id),
            nodes.parent(id).map(|parent| nodes.kind(parent)),
        ));
    }
    out
}

/// A one-line summary of how the two sets differ.
///
/// **Multiset, not set.** The comparison this explains is `Vec == Vec` over sorted
/// diagnostics, so a case expecting the *same* diagnostic twice and getting it
/// once is a difference. An earlier version filtered with `Vec::contains`, which
/// answers "is it present at all": both differences came back empty and the
/// function panicked on its own `expect`. It stayed hidden until the binder began
/// reporting a redeclaration on every declaration involved, which is exactly when
/// duplicate positions started to occur.
fn summarise(expected: &[BaselineDiagnostic], actual: &[BaselineDiagnostic]) -> String {
    let missing = surplus(expected, actual);
    let extra = surplus(actual, expected);

    // An unexpected diagnostic is reported in preference to a missing one: with no
    // checker, missing is the expected state and extra is a defect we own today.
    if let Some(first) = extra.first() {
        return format!(
            "{} unexpected (first TS{} at {}({},{})), {} missing",
            extra.len(),
            first.code,
            first.file,
            first.line,
            first.column,
            missing.len()
        );
    }
    match missing.first() {
        Some(first) => format!(
            "{} missing (first TS{} at {}({},{}))",
            missing.len(),
            first.code,
            first.file,
            first.line,
            first.column
        ),
        // Unreachable while the caller compares sorted vectors, but returning a
        // sentence beats an `expect` in a harness that runs over 12,444 cases.
        None => "sets differ but no diagnostic does".to_string(),
    }
}

/// The elements of `left` that `right` does not have *as many of*.
///
/// Both inputs are sorted, so this is a merge rather than a scan.
fn surplus<'a>(
    left: &'a [BaselineDiagnostic],
    right: &[BaselineDiagnostic],
) -> Vec<&'a BaselineDiagnostic> {
    let mut out = Vec::new();
    let mut index = 0usize;
    for item in left {
        while index < right.len() && right[index] < *item {
            index += 1;
        }
        if index < right.len() && right[index] == *item {
            index += 1;
        } else {
            out.push(item);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn diagnostic(code: u32, line: u32) -> BaselineDiagnostic {
        BaselineDiagnostic { file: "a.ts".into(), line, column: 1, code }
    }

    #[test]
    fn an_unexpected_diagnostic_is_reported_before_a_missing_one() {
        // With no checker almost every case is "missing"; a false positive is a
        // defect this port owns today, so it must not be buried behind the count.
        let expected = vec![diagnostic(2304, 1), diagnostic(2322, 2)];
        let actual = vec![diagnostic(1005, 9)];
        let summary = summarise(&expected, &actual);
        assert!(summary.starts_with("1 unexpected (first TS1005 at a.ts(9,1))"), "{summary}");
        assert!(summary.ends_with("2 missing"), "{summary}");
    }

    #[test]
    fn the_same_diagnostic_expected_twice_and_seen_once_is_a_difference() {
        // Multiset, not set. This is the case that made the previous `contains`
        // based version panic on its own `expect`.
        let expected = vec![diagnostic(2300, 3), diagnostic(2300, 3)];
        let actual = vec![diagnostic(2300, 3)];
        assert_eq!(summarise(&expected, &actual), "1 missing (first TS2300 at a.ts(3,1))");
    }

    #[test]
    fn a_purely_missing_set_names_the_first_code() {
        let expected = vec![diagnostic(2304, 3), diagnostic(2322, 7)];
        assert_eq!(summarise(&expected, &[]), "2 missing (first TS2304 at a.ts(3,1))".to_string());
    }

    fn codes_of(test: &crate::TestCase) -> Vec<(u32, u32)> {
        let mut codes: Vec<_> = reported_for(test).into_iter().map(|d| (d.line, d.code)).collect();
        codes.sort_unstable();
        codes
    }

    #[test]
    fn a_nocheck_file_keeps_only_its_syntactic_diagnostics() {
        // `SkipTypeChecking` drops a `// @ts-nocheck` file's bind-and-check set
        // in TypeScript too, unused `@ts-expect-error` included; the parse
        // error is syntactic and survives.
        let test = crate::TestCase::parse(
            "probe/nocheck",
            "a.ts",
            "// @ts-nocheck\n// @ts-expect-error\nlet x: number = 'a';\nlet y = ;\n",
        );
        assert_eq!(codes_of(&test), [(4, 1109)]);
    }

    #[test]
    fn a_plain_javascript_file_keeps_plain_js_errors_and_its_js_syntax() {
        // TS2451 is in `plainJSErrors`; TS2322 is not, and an unused
        // `@ts-expect-error` is never reported in plain JavaScript. TS8010 is
        // the file's `JSDiagnostics()`, which no gate touches.
        let mut test = crate::TestCase::parse(
            "probe/plain-js",
            "a.js",
            "let a = 1;\nlet a = 2;\n/** @type {number} */\nvar n = 'no';\n\
             // @ts-expect-error\nfunction f(p: number) {}\n",
        );
        test.options.insert("allowjs".into(), "true".into());
        assert_eq!(codes_of(&test), [(1, 2451), (2, 2451), (6, 8010)]);
    }

    #[test]
    fn redirected_package_diagnostics_keep_the_canonical_file_name() {
        let root = crate::repo_root();
        if !root.join("vendor/typescript-go/_submodules/TypeScript/tests/cases").is_dir() {
            eprintln!("skipping: TypeScript submodule absent");
            return;
        }
        let cases = crate::Corpus::from_repo_root(&root).discover().expect("corpus");
        let case = cases
            .iter()
            .find(|case| case.name == "compiler/duplicatePackage_globalMerge")
            .expect("duplicate-package control");
        let baseline = case.expected_errors().expect("baseline").expect("errors baseline");
        let test = case.load().expect("case");
        let mut expected = errors_baseline::parse(&baseline);
        let mut actual = reported_for(&test);
        expected.sort_unstable();
        actual.sort_unstable();
        assert_eq!(actual, expected);
    }
}
