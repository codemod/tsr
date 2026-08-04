//! Rule-level tests, stated as source in and `(code, line, column)` out.
//!
//! The conformance suite judges whole cases against upstream's baselines, which is
//! the real gate. These exist for the opposite purpose: to pin one rule at a time
//! so that a change in the analysis says *which* rule moved. Positions are 1-based
//! to match the baselines, so a case here can be pasted from an `.errors.txt`.

use tsr_dts::analyze;

/// Run the analysis and reduce it to `(code, line, column)`, 1-based.
fn diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let parsed = tsr_parser::ParsedFile::parse(source.to_string());
    parsed.with_ast(|file| {
        analyze(file, parsed.nodes())
            .into_iter()
            .map(|diagnostic| {
                let (line, column) = line_and_column(source, diagnostic.span.start);
                (diagnostic.message.code(), line, column)
            })
            .collect()
    })
}

fn line_and_column(source: &str, offset: u32) -> (u32, u32) {
    let mut line = 1;
    let mut column = 1;
    for (index, character) in source.char_indices() {
        if u32::try_from(index).unwrap_or(u32::MAX) >= offset {
            break;
        }
        if character == '\n' {
            line += 1;
            column = 1;
        } else {
            column += u32::try_from(character.len_utf16()).unwrap_or(1);
        }
    }
    (line, column)
}

fn codes(source: &str) -> Vec<u32> {
    diagnostics(source).into_iter().map(|(code, _, _)| code).collect()
}

// ----- inferable initialisers ------------------------------------------------

#[test]
fn a_literal_initialiser_needs_no_annotation() {
    assert!(codes("export const a = 1;").is_empty());
    assert!(codes("export const a = \"s\";").is_empty());
    assert!(codes("export const a = 1n;").is_empty());
    assert!(codes("export const a = true;").is_empty());
    assert!(codes("export const a = -1;").is_empty());
}

#[test]
fn arithmetic_is_not_inferable() {
    // TS9010, anchored at the *name* — not at the `1 + 1`. See `rules`' table.
    assert_eq!(diagnostics("export const a = 1 + 1;"), [(9010, 1, 14)]);
}

#[test]
fn an_annotated_declaration_is_never_judged_on_its_initialiser() {
    // The initialiser does not reach the `.d.ts`, so nothing in it can fail.
    assert!(codes("export const a: number = Math.random();").is_empty());
}

#[test]
fn a_template_widens_for_let_but_not_under_a_const_assertion() {
    // `isolatedDeclarationErrorsExpressions` contrasts these directly:
    // `templateLetOk2` is clean, `templateConstNotOk3` is TS9010.
    assert!(codes("export let a = `s${1}`;").is_empty());
    assert_eq!(codes("export const a = `s${1}`;"), [9010]);
    assert_eq!(codes("export let a = `s${1}` as const;"), [9010]);
}

#[test]
fn only_const_arrays_are_inferable() {
    assert_eq!(codes("export let a = [1, 2, 3];"), [9017]);
    assert!(codes("export let a = [1, 2, 3] as const;").is_empty());
    assert_eq!(codes("export let a = [1, ...b] as const;"), [9018]);
}

#[test]
fn a_nested_failure_names_itself_rather_than_the_declaration() {
    // The `Generic` → TS9013 conversion: the declaration's own code is used only
    // when the initialiser fails as a whole.
    assert_eq!(diagnostics("export const a = { b: 1 + 1 };"), [(9013, 1, 23)]);
}

// ----- declarations ----------------------------------------------------------

#[test]
fn a_function_declaration_needs_a_return_type_at_its_name() {
    assert_eq!(diagnostics("export function noReturn() {}"), [(9007, 1, 17)]);
    assert!(codes("export function ok(): void {}").is_empty());
}

#[test]
fn a_function_expression_bound_to_a_variable_does_not() {
    // Narrower than the corpus's own comments claim; see `rules::Checker::arrow`.
    assert!(codes("export const a = function () { return 0; };").is_empty());
    assert!(codes("export const a = () => \"s\";").is_empty());
}

#[test]
fn a_function_used_as_a_parameter_default_does() {
    assert_eq!(codes("export const a = (cb = function () {}): string => \"s\";"), [9007]);
}

#[test]
fn a_parameter_anchors_at_its_initialiser_but_at_its_name_when_bare() {
    assert_eq!(diagnostics("export function f(p): void {}"), [(9011, 1, 19)]);
    assert_eq!(diagnostics("export function f(p = 1 + 1): void {}"), [(9011, 1, 23)]);
    assert!(codes("export function f(p = 1): void {}").is_empty());
}

// ----- visibility -------------------------------------------------------------

#[test]
fn an_unexported_declaration_in_a_module_is_not_judged() {
    // The module has an export, so visibility is a question; `Internal` is not
    // reachable from it and never reaches the `.d.ts`.
    let source = "export const a = 1;\nclass Internal { f = Math.random(); }\n";
    assert!(codes(source).is_empty());
}

#[test]
fn a_declaration_reached_through_an_annotation_is_judged() {
    // `isolatedDeclarationErrorsReturnTypes`'s `IndirectlyExportedClass`, reduced.
    let source =
        "class Indirect { f = Math.random(); }\nexport const a: Indirect = new Indirect();\n";
    assert_eq!(codes(source), [9012]);
}

#[test]
fn a_script_emits_everything_so_everything_is_judged() {
    // `isolatedDeclarationErrors` has no `export` at all and still errors.
    assert_eq!(codes("const a = Math.random();"), [9010]);
}

#[test]
fn private_members_are_not_emitted_and_so_never_fail() {
    let source = "export class C { private f = Math.random(); #g = Math.random(); }";
    assert!(codes(source).is_empty());
}
