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

// ----- rules added while converging on the corpus ------------------------------

#[test]
fn a_readonly_property_is_a_const_context() {
    // `readonly` keeps the template literal type instead of widening to `string`,
    // exactly as `const` does for a variable.
    assert!(codes("export class C { t = `s${1}`; }").is_empty());
    assert_eq!(codes("export class C { readonly t = `s${1}`; }"), [9012]);
}

#[test]
fn only_a_lone_accessor_reports_and_a_setter_anchors_at_its_parameter() {
    assert_eq!(diagnostics("export class C { get a() { return 1; } }"), [(9009, 1, 22)]);
    // The setter anchors at its *parameter* (col 24), not its name (col 22).
    assert_eq!(diagnostics("export class C { set a(v) {} }"), [(9009, 1, 24)]);
    // A pair never reports, even with neither half annotated.
    assert!(codes("export class C { get a() { return 1; } set a(v) {} }").is_empty());
    assert!(codes("export class C { get a(): number { return 1; } }").is_empty());
}

#[test]
fn a_lone_getter_in_an_object_literal_is_silent_unlike_one_in_a_class() {
    // Taken from the baselines rather than derived; see `rules::Checker::accessors`.
    assert!(codes("export const o = { get a() { return 1; } };").is_empty());
    assert_eq!(codes("export const o = { set a(v) {} };"), [9009]);
}

#[test]
fn a_computed_name_must_be_written_as_a_literal() {
    assert!(codes("export const o = { [1]: 1 };").is_empty());
    assert!(codes("export const o = { [-1]: 1 };").is_empty());
    assert!(codes("export const o = { [\"a\"]: 1 };").is_empty());
    assert_eq!(codes("export const o = { [1 - 1]: 1 };"), [9038]);
    // A `const` with a literal type is still not restatable in this position.
    assert_eq!(codes("const k = \"a\";\nexport const o = { [k]: 1 };"), [9038]);
}

#[test]
fn enum_members_are_folded_in_order_and_transitively() {
    // A call is not a constant expression, so the member emits with no value.
    assert!(codes("enum E { A = f(), B = f() }").is_empty());
    assert!(codes("enum Flag { A = 1 >> 1, B = 2, AB = A | B, C = Flag.AB | B }").is_empty());
    // Reaching outside the enum, and then a sibling that did.
    assert_eq!(codes("enum E { A = 1 }\nenum F { A = E.A, B = A }"), [9020, 9020]);
    assert_eq!(codes("const V = 1;\nenum F { A = V }"), [9020]);
}

#[test]
fn an_arrow_with_a_literal_concise_body_needs_no_return_type() {
    assert!(codes("export const f = (cb = () => 1): string => \"s\";").is_empty());
    assert_eq!(codes("export const f = (cb = () => {}): string => \"s\";"), [9007]);
    assert_eq!(codes("export const f = (cb = function () {}): string => \"s\";"), [9007]);
}

#[test]
fn an_extends_clause_must_be_a_name() {
    assert!(codes("export class Base {}\nexport class C extends Base {}").is_empty());
    assert_eq!(
        codes("declare function id(c: unknown): any;\nexport class C extends id(0) {}"),
        [9021]
    );
}

#[test]
fn a_binding_element_default_is_reported_even_when_annotated() {
    assert_eq!(diagnostics("export const [, b = 1]: [number, number] = [0, 1];"), [(9019, 1, 17)]);
    assert!(codes("export const [, b]: [number, number] = [0, 1];").is_empty());
}

#[test]
fn an_interface_method_signature_needs_a_return_type() {
    assert_eq!(codes("export interface I { m(); }"), [9013]);
    assert!(codes("export interface I { m(): void; }").is_empty());
    assert!(codes("export interface I { p: 10; }").is_empty());
}

#[test]
fn a_declaration_reached_only_from_an_initialiser_is_not_emitted() {
    // Reference collection is type-positions-only: `helper` is reached from an
    // initialiser, never from a type, so it does not reach the `.d.ts`.
    let source = "function helper() { return 1; }\nexport const a: number = helper();\n";
    assert!(codes(source).is_empty());
}
