//! Which files' bind-and-check diagnostics a program reports, and how a plain
//! JavaScript file's are filtered.
//!
//! The per-file gates of `Program.getBindAndCheckDiagnosticsWithChecker`
//! (`internal/compiler/program.go:1352`) at the pinned commit:
//!
//! 1. `Program.SkipTypeChecking` (`program.go:713`) — `noCheck`,
//!    `skipLibCheck`, `skipDefaultLibCheck`, and
//!    `canIncludeBindAndCheckDiagnostics` (`program.go:721`), which drops a file
//!    carrying `// @ts-nocheck` **whatever its language**, every JSON file, and
//!    JavaScript under an explicit `checkJs: false`.
//! 2. `ast.IsPlainJSFile` (`ast/utilities.go:2889`) — a JavaScript file with
//!    neither a `@ts-check`/`@ts-nocheck` directive nor a written `checkJs` keeps
//!    only the diagnostics in [`is_plain_js_error`].
//!
//! The functions are free and read nothing but the program's files and
//! options; they own no state. The CLI driver (`tsr-execute`) and the
//! conformance harness both call them, so a gate cannot differ between the two.
//! Project-reference redirects (`IsSourceFromProjectReference`) are not loaded
//! by this port's program, so that arm has nothing to test.

use tsr_core::CompilerOptions;
use tsr_diagnostics::Diagnostic;

use crate::{Program, ProgramFile, comment_directives};

mod verify_options;

pub use verify_options::{OptionsVerification, verify_compiler_options};

/// `IsSourceFileJS` (`ast/utilities.go:1290`): the file's script kind is
/// `ScriptKindJS` or `ScriptKindJSX`, which `GetScriptKindFromFileName` derives
/// from these four extensions, case-insensitively.
///
/// Asked of the name rather than `tsr_parser::ScriptKind`, which is a dialect
/// flag and does not separate `.js` from `.ts`.
#[must_use]
pub fn is_source_file_js(file_name: &str) -> bool {
    let extension = file_name.rsplit('.').next().unwrap_or_default();
    ["js", "jsx", "cjs", "mjs"].iter().any(|ext| extension.eq_ignore_ascii_case(ext))
}

/// `ast.IsCheckJSEnabledForFile` (`ast/utilities.go:2882`): the file's own
/// `@ts-check`/`@ts-nocheck` wins; otherwise `checkJs` must be written `true`.
#[must_use]
pub fn is_check_js_enabled_for_file(file: &ProgramFile<'_>, options: &CompilerOptions) -> bool {
    match file.file_references().check_js_directive {
        Some(directive) => directive.enabled,
        None => options.check_js.is_true(),
    }
}

/// `ast.IsPlainJSFile` (`ast/utilities.go:2889`): JavaScript with no directive
/// and `checkJs` unset — not merely not-`true`.
#[must_use]
pub fn is_plain_js_file(file: &ProgramFile<'_>, options: &CompilerOptions) -> bool {
    is_source_file_js(file.file_name())
        && file.file_references().check_js_directive.is_none()
        && options.check_js.is_unknown()
}

/// `Program.canIncludeBindAndCheckDiagnostics` (`program.go:721`).
///
/// `ScriptKindExternal` and `ScriptKindDeferred` have no producer in this port.
#[must_use]
pub fn can_include_bind_and_check_diagnostics(
    file: &ProgramFile<'_>,
    options: &CompilerOptions,
) -> bool {
    if file.file_references().check_js_directive.is_some_and(|directive| !directive.enabled) {
        return false;
    }
    if !is_source_file_js(file.file_name()) {
        // `ScriptKindTS`/`ScriptKindTSX` are included; JSON is not.
        return tsr_parser::ScriptKind::from_file_name(file.file_name())
            != tsr_parser::ScriptKind::Json;
    }
    is_plain_js_file(file, options) || is_check_js_enabled_for_file(file, options)
}

/// `Program.SkipTypeChecking` (`program.go:713`), naming the arm that applied.
///
/// `file_index` indexes [`Program::source_files`]; the default-library arm is
/// membership in [`Program::lib_files`], which is that list's prefix.
///
/// # Panics
///
/// Panics if `file_index` is outside `program.source_files()`.
#[must_use]
pub fn skip_type_checking(
    program: &Program<'_>,
    file_index: usize,
    ignore_no_check: bool,
) -> Option<&'static str> {
    let options = program.compiler_options();
    let file = &program.source_files()[file_index];
    if !ignore_no_check && options.no_check.is_true() {
        return Some("no_check");
    }
    if options.skip_lib_check.is_true() && tsr_path::is_declaration_file_name(file.file_name()) {
        return Some("skip_lib_check");
    }
    if options.skip_default_lib_check.is_true() && file_index < program.lib_files().len() {
        return Some("skip_default_lib_check");
    }
    if file.file_references().check_js_directive.is_some_and(|directive| !directive.enabled) {
        return Some("file_no_check");
    }
    if !can_include_bind_and_check_diagnostics(file, options) {
        return Some(
            if tsr_parser::ScriptKind::from_file_name(file.file_name())
                == tsr_parser::ScriptKind::Json
            {
                "json_source"
            } else {
                "check_js_false"
            },
        );
    }
    None
}

/// `Program.getBindAndCheckDiagnosticsWithChecker` (`program.go:1352`) for
/// program file `file_index`, given the file's `BindDiagnostics()` and what
/// the checker reported in it.
///
/// Nothing for a file [`skip_type_checking`] skips. Otherwise
/// `bind_diagnostics` and `checker_diagnostics`; in a plain JavaScript file
/// only the [`is_plain_js_error`] codes, returned before any comment
/// directive is consulted; elsewhere the survivors of
/// `getDiagnosticsWithPrecedingDirectives` plus a TS2578 for each
/// `@ts-expect-error` nothing used.
///
/// A checked JavaScript file also reports its `JSDocDiagnostics()`, the parse
/// errors inside its JSDoc comments ([`tsr_parser::JSDocTable::diagnostics`]),
/// before the directives are applied.
///
/// # Panics
///
/// Panics if `file_index` is outside `program.source_files()`.
#[must_use]
pub fn bind_and_check_diagnostics(
    program: &Program<'_>,
    file_index: usize,
    bind_diagnostics: &[Diagnostic],
    checker_diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> Vec<Diagnostic> {
    if skip_type_checking(program, file_index, false).is_some() {
        return Vec::new();
    }
    let file = &program.source_files()[file_index];
    let mut diagnostics = bind_diagnostics.to_vec();
    diagnostics.extend(checker_diagnostics);
    if is_plain_js_file(file, program.compiler_options()) {
        diagnostics.retain(|d| is_plain_js_error(d.message.code()));
        return diagnostics;
    }
    if is_source_file_js(file.file_name())
        && is_check_js_enabled_for_file(file, program.compiler_options())
    {
        diagnostics.extend_from_slice(file.jsdoc().diagnostics());
    }
    let (mut kept, unused) = with_preceding_directives(file.text(), diagnostics);
    kept.extend(unused);
    kept
}

/// `GetDiagnosticsOfAnyProgram` (`program.go:1782`) once the checkers have
/// run, as `(file name, diagnostic)`. The caller prepends
/// `GetConfigFileParsingDiagnostics`, which gate nothing.
///
/// Every file's `GetSyntacticDiagnostics` (`program.go:626`): its parse
/// diagnostics and its `js_syntax` (`SourceFile.JSDiagnostics()`, which this
/// port's checker produces). Only when there are none, `program_level`
/// (`GetProgramDiagnostics`, [`program_level_diagnostics`]); and only when
/// that is empty too and the run is not `listFilesOnly`, each file's semantic
/// diagnostics: [`bind_and_check_diagnostics`] over
/// [`Program::bind_diagnostics_of`] and the `checker` diagnostics located in
/// it, then [`include_processor_diagnostics`]. `GetGlobalDiagnostics` has no
/// producer in this port. A caller that knows the semantic set will be
/// skipped need not check at all ([`semantic_diagnostics_are_asked`]), as
/// upstream never asks its checkers then.
#[must_use]
pub fn diagnostics_of_any_program(
    program: &Program<'_>,
    program_level: Vec<(String, Diagnostic)>,
    js_syntax: Vec<(usize, Diagnostic)>,
    checker: Vec<(usize, Diagnostic)>,
) -> Vec<(String, Diagnostic)> {
    let files = program.source_files();
    let named = |located: Vec<(usize, Diagnostic)>| -> Vec<(String, Diagnostic)> {
        located.into_iter().map(|(index, d)| (files[index].file_name().to_string(), d)).collect()
    };
    let mut out: Vec<(usize, Diagnostic)> = Vec::new();
    for (index, file) in files.iter().enumerate() {
        out.extend(file.diagnostics().iter().map(|d| (index, d.clone())));
    }
    out.extend(js_syntax);
    if !out.is_empty() {
        return named(out);
    }
    if !program_level.is_empty() || program.compiler_options().list_files_only.is_true() {
        return program_level;
    }
    let mut by_file: Vec<Vec<Diagnostic>> = vec![Vec::new(); files.len()];
    for (index, diagnostic) in checker {
        by_file[index].push(diagnostic);
    }
    for (index, checked) in by_file.into_iter().enumerate() {
        let semantic =
            bind_and_check_diagnostics(program, index, program.bind_diagnostics_of(index), checked);
        out.extend(semantic.into_iter().map(|d| (index, d)));
        out.extend(include_processor_diagnostics(program, index).into_iter().map(|d| (index, d)));
    }
    named(out)
}

/// Whether [`diagnostics_of_any_program`] can reach the semantic set, judged
/// before checking: no file has a parse diagnostic, `program_level` is empty
/// and the run is not `listFilesOnly`. A JavaScript file's `js_syntax` can
/// still close the gate after checking; that is the one case where checking
/// runs and its result is dropped.
#[must_use]
pub fn semantic_diagnostics_are_asked(
    program: &Program<'_>,
    program_level: &[(String, Diagnostic)],
) -> bool {
    program_level.is_empty()
        && !program.compiler_options().list_files_only.is_true()
        && program.source_files().iter().all(|file| file.diagnostics().is_empty())
}

/// `Program.GetProgramDiagnostics` (`program.go:698`): `programDiagnostics`
/// ([`verify_compiler_options`], which upstream runs when the program is
/// created) and the include processor's global diagnostics
/// ([`global_program_diagnostics`]), as `(file name, diagnostic)`. A
/// compiler diagnostic has an empty file name. The caller's final
/// `SortAndDeduplicateDiagnostics` orders them.
#[must_use]
pub fn program_level_diagnostics(
    program: &Program<'_>,
    input: OptionsVerification<'_>,
) -> Vec<(String, Diagnostic)> {
    let mut found = verify_compiler_options(program, input);
    found.extend(global_program_diagnostics(program).into_iter().map(|d| (String::new(), d)));
    found
}

/// `Program.GetIncludeProcessorDiagnostics` (`program.go:705`): the loader's
/// diagnostics located in program file `file_index`, through a directive
/// filter of their own whose unused directives are not reported again.
///
/// # Panics
///
/// Panics if `file_index` is outside `program.source_files()`.
#[must_use]
pub fn include_processor_diagnostics(program: &Program<'_>, file_index: usize) -> Vec<Diagnostic> {
    if skip_type_checking(program, file_index, false).is_some() {
        return Vec::new();
    }
    let file = &program.source_files()[file_index];
    let mut found: Vec<Diagnostic> = program
        .loader_diagnostics()
        .iter()
        .filter(|d| program.to_path(&d.file_name) == *file.path())
        .map(|d| Diagnostic::with_args(d.message, d.span, d.args.iter().cloned()))
        .collect();
    let explaining = program.explaining_diagnostics();
    let explained_here = explaining.iter().filter(|(located, _)| *located == Some(file_index));
    let before = found.len();
    found.extend(explained_here.map(|(_, d)| d.clone()));
    if found.is_empty() {
        return found;
    }
    // `DiagnosticsCollection.GetDiagnosticsForFile` sorts its file's list
    // (`ast/diagnostic.go:229`). Applied only when an explaining diagnostic
    // joined the loader's, whose own order this port already reproduces.
    if found.len() != before {
        found.sort_by(tsr_diagnostics::compare_diagnostics);
    }
    with_preceding_directives(file.text(), found).0
}

/// The composite file-list check of `Program.verifyCompilerOptions`
/// (`internal/compiler/program.go:938-956`): under `composite`, every program
/// file that may be emitted and is not one of the config's root files is
/// TS6307, explained as `includeProcessor` explains any file
/// ([`crate::file_include::diagnostic_explaining_file`]).
///
/// Returns `(file index the diagnostic is positioned in, diagnostic)`, in
/// program-file order, as upstream appends them; `None` is a global
/// diagnostic, for a file no written reference reached. The located ones are
/// reported through [`include_processor_diagnostics`] of their file, the
/// global ones through [`global_program_diagnostics`].
///
/// `rootPaths` is built from `opts.Config.FileNames()` with `toPath`; the
/// project name is the config file's name as the program was given it
/// (`configFilePath()`, the config source file's `FileName()`), which is
/// `options.config_file_path` here and empty without a config.
#[must_use]
pub fn composite_file_list_diagnostics(program: &Program<'_>) -> Vec<(Option<usize>, Diagnostic)> {
    let options = program.compiler_options();
    if !options.composite.is_true() {
        return Vec::new();
    }
    let root_paths: rustc_hash::FxHashSet<tsr_path::Path> =
        program.root_file_names().iter().map(|name| program.to_path(name)).collect();
    let mut out = Vec::new();
    for (index, file) in program.source_files().iter().enumerate() {
        if program.source_file_may_be_emitted(index) && !root_paths.contains(file.path()) {
            out.push(crate::file_include::diagnostic_explaining_file(
                program,
                index,
                &tsr_diagnostics::messages::FILE_0_IS_NOT_LISTED_WITHIN_THE_FILE_LIST_OF_PROJECT_1_PROJECTS_MUST_LIST_ALL_FILES_OR_USE_AN_INCLUDE_PATTERN,
                vec![file.file_name().to_string(), options.config_file_path.clone()],
            ));
        }
    }
    out
}

/// The global half of `Program.GetProgramDiagnostics` (`program.go:698`):
/// the include processor's diagnostics that name no file, sorted and
/// deduplicated. Today only a TS6307 whose subject no written reference
/// reached ([`composite_file_list_diagnostics`]). `programDiagnostics` itself
/// (the option checks of `verifyCompilerOptions`) joins it in
/// [`program_level_diagnostics`].
#[must_use]
pub fn global_program_diagnostics(program: &Program<'_>) -> Vec<Diagnostic> {
    tsr_diagnostics::sort_and_deduplicate_diagnostics(
        program
            .explaining_diagnostics()
            .iter()
            .filter(|(located, _)| located.is_none())
            .map(|(_, d)| d.clone())
            .collect(),
    )
}

/// `Program.getDiagnosticsWithPrecedingDirectives` (`program.go:1386`) over
/// one file: the survivors, and the TS2578 each unused `@ts-expect-error`
/// earns (`program.go:1377`).
fn with_preceding_directives(
    text: &str,
    diagnostics: Vec<Diagnostic>,
) -> (Vec<Diagnostic>, Vec<Diagnostic>) {
    let directives = comment_directives::directives_in(text);
    if directives.is_empty() {
        return (diagnostics, Vec::new());
    }
    // `ComputeLineOfPosition` over the ECMA line map, 0-based.
    let starts = tsr_core::ecma_line_starts(text);
    let entries: Vec<(u32, Diagnostic)> = diagnostics
        .into_iter()
        .map(|d| (tsr_core::compute_line_of_position(&starts, d.span.start), d))
        .collect();
    comment_directives::filter(text, &entries, &directives)
}

/// `plainJSErrors` (`program.go:2137`): the only codes a plain JavaScript
/// file's bind-and-check diagnostics keep.
#[must_use]
pub fn is_plain_js_error(code: u32) -> bool {
    PLAIN_JS_ERRORS.binary_search(&code).is_ok()
}

/// `plainJSErrors`, by code, ascending so [`is_plain_js_error`] can bisect.
/// Each entry names the upstream message it is the code of.
const PLAIN_JS_ERRORS: &[u32] = &[
    1005,  // X_0_expected
    1009,  // Trailing_comma_not_allowed
    1013,  // A_rest_parameter_or_binding_pattern_may_not_have_a_trailing_comma
    1014,  // A_rest_parameter_must_be_last_in_a_parameter_list
    1029,  // X_0_modifier_must_precede_1_modifier
    1030,  // X_0_modifier_already_seen
    1031,  // X_0_modifier_cannot_appear_on_class_elements_of_this_kind
    1042,  // X_0_modifier_cannot_be_used_here
    1044,  // X_0_modifier_cannot_appear_on_a_module_or_namespace_element
    1048,  // A_rest_parameter_cannot_have_an_initializer
    1049,  // A_set_accessor_must_have_exactly_one_parameter
    1053,  // A_set_accessor_cannot_have_rest_parameter
    1054,  // A_get_accessor_cannot_have_parameters
    1089,  // X_0_modifier_cannot_appear_on_a_constructor_declaration
    1090,  // X_0_modifier_cannot_appear_on_a_parameter
    1091,  // Only_a_single_variable_declaration_is_allowed_in_a_for_in_statement
    1097,  // X_0_list_cannot_be_empty
    1100,  // Invalid_use_of_0_in_strict_mode
    1101,  // X_with_statements_are_not_allowed_in_strict_mode
    1102,  // X_delete_cannot_be_called_on_an_identifier_in_strict_mode
    1104,  // A_continue_statement_can_only_be_used_within_an_enclosing_iteration_statement
    1105,  // A_break_statement_can_only_be_used_within_an_enclosing_iteration_or_switch_statement
    1106,  // The_left_hand_side_of_a_for_of_statement_may_not_be_async
    1107,  // Jump_target_cannot_cross_function_boundary
    1111,  // Private_field_0_must_be_declared_in_an_enclosing_class
    1113,  // A_default_clause_cannot_appear_more_than_once_in_a_switch_statement
    1114,  // Duplicate_label_0
    1115,  // A_continue_statement_can_only_jump_to_a_label_of_an_enclosing_iteration_statement
    1116,  // A_break_statement_can_only_jump_to_a_label_of_an_enclosing_statement
    1123,  // Variable_declaration_list_cannot_be_empty
    1155,  // X_0_declarations_must_be_initialized
    1156,  // X_0_declarations_can_only_be_declared_inside_a_block
    1162,  // An_object_member_cannot_be_declared_optional
    1171,  // A_comma_expression_is_not_allowed_in_a_computed_property_name
    1172,  // X_extends_clause_already_seen
    1174,  // Classes_can_only_extend_a_single_class
    1182,  // A_destructuring_declaration_must_have_an_initializer
    1184,  // Modifiers_cannot_appear_here
    1186,  // A_rest_element_cannot_have_an_initializer
    1188,  // Only_a_single_variable_declaration_is_allowed_in_a_for_of_statement
    1189,  // The_variable_declaration_of_a_for_in_statement_cannot_have_an_initializer
    1190,  // The_variable_declaration_of_a_for_of_statement_cannot_have_an_initializer
    1191,  // An_import_declaration_cannot_have_modifiers
    1193,  // An_export_declaration_cannot_have_modifiers
    1197,  // Catch_clause_variable_cannot_have_an_initializer
    1200,  // Line_terminator_not_permitted_before_arrow
    1210, // Code_contained_in_a_class_is_evaluated_in_JavaScript_s_strict_mode_which_does_not_allow_this_use_of_0_For_more_information_see_https_Colon_Slash_Slashdeveloper_mozilla_org_Slashen_US_Slashdocs_SlashWeb_SlashJavaScript_SlashReference_SlashStrict_mode
    1211, // A_class_declaration_without_the_default_modifier_must_have_a_name
    1214, // Identifier_expected_0_is_a_reserved_word_in_strict_mode_Modules_are_automatically_in_strict_mode
    1215, // Invalid_use_of_0_Modules_are_automatically_in_strict_mode
    1248, // A_class_member_cannot_have_the_0_keyword
    1255, // A_definite_assignment_assertion_is_not_permitted_in_this_context
    1258, // A_default_export_must_be_at_the_top_level_of_a_file_or_module_declaration
    1262, // Identifier_expected_0_is_a_reserved_word_at_the_top_level_of_a_module
    1308, // X_await_expressions_are_only_allowed_within_async_functions_and_at_the_top_levels_of_modules
    1312, // Did_you_mean_to_use_a_Colon_An_can_only_follow_a_property_name_when_the_containing_object_literal_is_part_of_a_destructuring_pattern
    1325, // Argument_of_dynamic_import_cannot_be_spread_element
    1341, // Class_constructor_may_not_be_an_accessor
    1344, // A_label_is_not_allowed_here
    1358, // Tagged_template_expressions_are_not_permitted_in_an_optional_chain
    1359, // Identifier_expected_0_is_a_reserved_word_that_cannot_be_used_here
    1368, // Class_constructor_may_not_be_a_generator
    1450, // Dynamic_imports_can_only_accept_a_module_specifier_and_an_optional_set_of_attributes_as_arguments
    1451, // Private_identifiers_are_only_allowed_in_class_bodies_and_may_only_be_used_as_part_of_a_class_member_declaration_property_access_or_on_the_left_hand_side_of_an_in_expression
    1473, // An_import_declaration_can_only_be_used_at_the_top_level_of_a_module
    1474, // An_export_declaration_can_only_be_used_at_the_top_level_of_a_module
    2451, // Cannot_redeclare_block_scoped_variable_0
    2462, // A_rest_element_must_be_last_in_a_destructuring_pattern
    2480, // X_let_is_not_allowed_to_be_used_as_a_name_in_let_or_const_declarations
    2492, // Cannot_redeclare_identifier_0_in_catch_clause
    2501, // A_rest_element_cannot_contain_a_binding_pattern
    2528, // A_module_cannot_have_multiple_default_exports
    2566, // A_rest_element_cannot_have_a_property_name
    2633, // JSX_property_access_expressions_cannot_include_JSX_namespace_names
    2752, // The_first_export_default_is_here
    2753, // Another_export_default_is_here
    2803, // Cannot_assign_to_private_method_0_Private_methods_are_not_writable
    2839, // This_condition_will_always_return_0_since_JavaScript_compares_objects_by_reference_not_value
    2852, // X_await_using_statements_are_only_allowed_within_async_functions_and_at_the_top_levels_of_modules
    5076, // X_0_and_1_operations_cannot_be_mixed_without_parentheses
    17000, // JSX_attributes_must_only_be_assigned_a_non_empty_expression
    17001, // JSX_elements_cannot_have_multiple_attributes_with_the_same_name
    17012, // X_0_is_not_a_valid_meta_property_for_keyword_1_Did_you_mean_2
    18006, // Classes_may_not_have_a_field_named_constructor
    18007, // JSX_expressions_may_not_use_the_comma_operator_Did_you_mean_to_write_an_array
    18012, // X_constructor_is_a_reserved_word
    18013, // Property_0_is_not_accessible_outside_class_1_because_it_has_a_private_identifier
    18016, // Private_identifiers_are_not_allowed_outside_class_bodies
    18036, // Class_decorators_can_t_be_used_with_static_private_identifier_Consider_removing_the_experimental_decorator
    18038, // X_for_await_loops_cannot_be_used_inside_a_class_static_block
    18041, // A_return_statement_cannot_be_used_inside_a_class_static_block
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_js_errors_are_sorted_and_unique() {
        assert!(PLAIN_JS_ERRORS.windows(2).all(|pair| pair[0] < pair[1]));
    }

    #[test]
    fn a_plain_js_file_keeps_binder_errors_and_drops_type_errors() {
        // TS2451 `Cannot redeclare block-scoped variable` is a binder error the
        // runtime would also raise; TS2322 is a type error plain JS never sees.
        assert!(is_plain_js_error(2451));
        assert!(!is_plain_js_error(2322));
    }

    // ---- TS6307: the composite file-list check -------------------------------

    struct TestHost {
        fs: tsr_vfs::InMemoryFileSystem,
    }

    impl tsr_module::types::ResolutionHost for TestHost {
        fn fs(&self) -> &dyn tsr_vfs::FileSystem {
            &self.fs
        }

        fn current_directory(&self) -> &'static str {
            "/"
        }
    }

    fn composite_program<'a>(
        arena: &'a tsr_core::Arena,
        files: &[(&str, &str)],
        roots: &[&str],
        composite: bool,
    ) -> Program<'a> {
        let host = TestHost {
            fs: tsr_vfs::InMemoryFileSystem::new(
                files.iter().map(|(name, text)| ((*name).to_string(), (*text).to_string())),
                [],
                true,
            ),
        };
        Program::from_root_files(
            arena,
            &host,
            crate::LoadOptions {
                compiler_options: CompilerOptions {
                    no_lib: tsr_core::Tristate::True,
                    composite: tsr_core::Tristate::from_bool(composite),
                    config_file_path: "/p/tsconfig.json".to_string(),
                    ..Default::default()
                },
                root_file_names: roots.iter().map(|root| (*root).to_string()).collect(),
                ..Default::default()
            },
        )
    }

    fn rendered(program: &Program<'_>, index: usize) -> Vec<(String, u32, u32, Vec<String>)> {
        include_processor_diagnostics(program, index)
            .iter()
            .map(|d| (d.message.key().to_string(), d.span.start, d.span.end, d.args.clone()))
            .collect()
    }

    #[test]
    fn an_imported_file_outside_the_root_list_is_reported_at_its_import() {
        let arena = tsr_core::Arena::new();
        let main = "import { b } from \"../lib/b\";\nexport const a = b;\n";
        let program = composite_program(
            &arena,
            &[("/p/a.ts", main), ("/lib/b.ts", "export const b = 1;"), ("/lib/c.d.ts", "")],
            &["/p/a.ts"],
            true,
        );
        let a = program.source_files().iter().position(|f| f.file_name() == "/p/a.ts").unwrap();
        let found = rendered(&program, a);
        assert_eq!(found.len(), 1, "{found:?}");
        let (key, start, end, args) = &found[0];
        assert!(key.ends_with("_6307"), "{key}");
        // `CreateDiagnosticForNodeInSourceFile` over the specifier literal.
        assert_eq!(&main[*start as usize..*end as usize], "\"../lib/b\"");
        assert_eq!(args, &["/lib/b.ts".to_string(), "/p/tsconfig.json".to_string()]);
        // One written reason: no "The file is in the program because" chain.
        let diagnostic = include_processor_diagnostics(&program, a).remove(0);
        assert!(
            diagnostic.message_chain().iter().all(|d| d.message.code() != 1430),
            "{:?}",
            diagnostic.message_chain()
        );
        assert!(diagnostic.related_information().is_empty());
        assert!(global_program_diagnostics(&program).is_empty());
    }

    #[test]
    fn several_reasons_chain_every_reason_and_relate_the_others() {
        let arena = tsr_core::Arena::new();
        let program = composite_program(
            &arena,
            &[
                ("/p/a.ts", "import \"../lib/b\";\nimport \"./c\";\n"),
                ("/p/c.ts", "/// <reference path=\"../lib/b.ts\" />\nexport {};\n"),
                ("/lib/b.ts", "export const b = 1;"),
            ],
            &["/p/a.ts", "/p/c.ts"],
            true,
        );
        let a = program.source_files().iter().position(|f| f.file_name() == "/p/a.ts").unwrap();
        let c = program.source_files().iter().position(|f| f.file_name() == "/p/c.ts").unwrap();
        // Positioned at the first reason in replay order, the import in a.ts.
        assert_eq!(rendered(&program, a).len(), 1);
        assert!(rendered(&program, c).is_empty());
        let diagnostic = include_processor_diagnostics(&program, a).remove(0);
        let chain = diagnostic.message_chain();
        assert_eq!(chain[0].message.code(), 1430, "{chain:?}");
        let reasons: Vec<String> = chain[0].message_chain().iter().map(Diagnostic::text).collect();
        assert_eq!(
            reasons,
            [
                "Imported via \"../lib/b\" from file '/p/a.ts'",
                "Referenced via '../lib/b.ts' from file '/p/c.ts'",
            ]
        );
        let related: Vec<(u32, String)> = diagnostic
            .related_information()
            .iter()
            .map(|d| (d.message.code(), d.file().unwrap().file_name().to_string()))
            .collect();
        assert_eq!(related, [(1401, "/p/c.ts".to_string())]);
    }

    #[test]
    fn root_files_declarations_and_non_composite_programs_are_not_reported() {
        let arena = tsr_core::Arena::new();
        let files = [
            ("/p/a.ts", "import \"../lib/b\";\nimport \"../lib/d\";\n"),
            ("/lib/b.ts", "export {};"),
            ("/lib/d.d.ts", "export {};"),
        ];
        let program = composite_program(&arena, &files, &["/p/a.ts", "/lib/b.ts"], true);
        assert!(composite_file_list_diagnostics(&program).is_empty());
        let program = composite_program(&arena, &files, &["/p/a.ts"], false);
        assert!(composite_file_list_diagnostics(&program).is_empty());
        let program = composite_program(&arena, &files, &["/p/a.ts"], true);
        let found = composite_file_list_diagnostics(&program);
        assert_eq!(found.len(), 1, "only b.ts may be emitted: {found:?}");
    }
}
