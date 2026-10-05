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

use tsr_core::{Arena, CompilerOptions};
use tsr_diagnostics::Diagnostic;

use crate::{Program, ProgramFile, comment_directives};

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

/// `sourceFile.BindDiagnostics()` for one program file.
///
/// The program binds every file into **one** store whose diagnostic list has
/// no per-file attribution, so the file is bound again on its own, over the
/// program's own tree and JSDoc table, and the binder reads both and mutates
/// neither, and the fresh store is dropped on return. Cross-file merges are the
/// checker's, upstream and here.
#[must_use]
pub fn bind_diagnostics<'a>(
    arena: &'a Arena,
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

/// `Program.getBindAndCheckDiagnosticsWithChecker` (`program.go:1352`) for
/// program file `file_index`, given what the checker reported in it.
///
/// Nothing for a file [`skip_type_checking`] skips. Otherwise the file's
/// binder diagnostics and `checker_diagnostics`; in a plain JavaScript file
/// only the [`is_plain_js_error`] codes, returned before any comment
/// directive is consulted; elsewhere the survivors of
/// `getDiagnosticsWithPrecedingDirectives` plus a TS2578 for each
/// `@ts-expect-error` nothing used.
///
/// A checked JavaScript file's `JSDocDiagnostics()` are not added: this
/// port's parser discards JSDoc parse errors (`parse_jsdoc_comment`).
///
/// # Panics
///
/// Panics if `file_index` is outside `program.source_files()`.
#[must_use]
pub fn bind_and_check_diagnostics<'a>(
    arena: &'a Arena,
    program: &Program<'a>,
    file_index: usize,
    checker_diagnostics: impl IntoIterator<Item = Diagnostic>,
) -> Vec<Diagnostic> {
    if skip_type_checking(program, file_index, false).is_some() {
        return Vec::new();
    }
    let file = &program.source_files()[file_index];
    let mut diagnostics = bind_diagnostics(arena, program, file);
    diagnostics.extend(checker_diagnostics);
    if is_plain_js_file(file, program.compiler_options()) {
        diagnostics.retain(|d| is_plain_js_error(d.message.code()));
        return diagnostics;
    }
    let (mut kept, unused) = with_preceding_directives(file.text(), diagnostics);
    kept.extend(unused);
    kept
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
    let found: Vec<Diagnostic> = program
        .loader_diagnostics()
        .iter()
        .filter(|d| program.to_path(&d.file_name) == *file.path())
        .map(|d| Diagnostic::with_args(d.message, d.span, d.args.iter().cloned()))
        .collect();
    if found.is_empty() {
        return found;
    }
    with_preceding_directives(file.text(), found).0
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
}
