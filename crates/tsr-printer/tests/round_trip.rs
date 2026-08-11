//! Round-trip tests for the traps found while converging on the corpus.
//!
//! The conformance suite is the real gate; these pin one construct each, so a
//! regression names itself instead of moving a percentage.

use tsr_parser::{ParsedFile, ScriptKind};

fn round_trips_as(source: &str, kind: ScriptKind) -> bool {
    let first = ParsedFile::parse_with_script_kind(source.to_string(), kind);
    assert!(first.diagnostics().is_empty(), "the fixture must parse: {source}");
    let printed = first.with_ast(|file| tsr_printer::print(file, first.nodes()));
    assert!(printed.is_complete(), "unsupported: {:?}", printed.unsupported);
    let second = ParsedFile::parse_with_script_kind(printed.text.clone(), kind);
    assert!(
        second.diagnostics().is_empty(),
        "printed text does not parse:\n{}\nerror: {}",
        printed.text,
        second.diagnostics()[0].text()
    );
    true
}

fn round_trips(source: &str) -> bool {
    round_trips_as(source, ScriptKind::TypeScript)
}

fn printed(source: &str) -> String {
    let parsed = ParsedFile::parse(source.to_string());
    parsed.with_ast(|file| tsr_printer::print(file, parsed.nodes())).text
}

#[test]
fn a_private_name_is_not_prefixed_twice() {
    // `PrivateIdentifier.text` already carries its `#`.
    assert!(printed("class C { #a = 1; }").contains("#a"));
    assert!(!printed("class C { #a = 1; }").contains("##"));
    assert!(round_trips("class C { #a = 1; m() { return this.#a; } }"));
    assert!(round_trips(r"class Escaped { #\u0078 = 1; m() { return this.#x; } }"));
}

#[test]
fn a_template_keeps_its_own_delimiters() {
    // `raw_text` is the whole token, backtick and `${` included.
    assert!(round_trips("const a = `x${1}y${2}z`;"));
    assert!(round_trips("type T = `x${string}y`;"));
}

#[test]
fn a_no_substitution_template_prints_in_an_expression() {
    assert!(round_trips("const value = `plain`;"));
}

#[test]
fn a_no_substitution_template_prints_in_a_literal_type() {
    assert!(round_trips("type Plain = `plain`;"));
}

#[test]
fn nested_type_arguments_close_without_inserted_spaces() {
    let output = printed("type T = Outer<Middle<Inner<string>>>;");
    assert!(output.contains("Outer<Middle<Inner<string>>>"), "{output}");
}

#[test]
fn mapped_types_use_the_upstream_multiline_default() {
    let output = printed("type T<U> = { readonly [K in keyof U]?: U[K] };");
    assert_eq!(output.trim_end(), "type T<U> = {\n    readonly [K in keyof U]?: U[K];\n};");
    assert!(round_trips("type T<U> = { readonly [K in keyof U]?: U[K] };"));
}

#[test]
fn mapped_type_modifier_signs_survive() {
    let source = "type R<T> = { -readonly [K in keyof T]-?: T[K] };\n\
                  type A<T> = { +readonly [K in keyof T]+?: T[K] };";
    let output = printed(source);
    assert!(output.contains("-readonly [K in keyof T]-?: T[K]"), "{output}");
    assert!(output.contains("+readonly [K in keyof T]+?: T[K]"), "{output}");
    assert!(round_trips(source));
}

#[test]
fn a_negative_numeric_literal_type_keeps_its_sign() {
    let output = printed("type T = -1e999;");
    assert!(output.contains("-1e999"), "{output}");
}

#[test]
fn a_dotted_namespace_does_not_restate_its_header() {
    // `namespace A.B {}` nests two ModuleDeclarations; writing the inner one as a
    // statement gives `namespace A. export namespace B {}`.
    assert!(round_trips("namespace A.B.C { export const x = 1; }"));
    assert!(!printed("namespace A.B {}").contains("namespace B"));
}

#[test]
fn declare_global_does_not_repeat_its_name() {
    // The keyword *is* the name; the tree carries both.
    assert!(round_trips("declare global { var x: number; }"));
    assert!(!printed("declare global { }").contains("global global"));
}

#[test]
fn jsdoc_function_parameter_with_type_cast() {
    // A JSDoc type cast inside a default value must not disturb the tree.
    let source = "export function fn(p = /** @type {string} */(null)) {}";
    assert!(round_trips(source));
}

#[test]
fn jsdoc_typedef_with_optional_property() {
    // From declarationEmitCastReusesTypeNode4: the `?` lives inside the JSDoc
    // block, which the round-trip print drops by design.
    let source = "/**
 * @typedef {{ } & { name?: string }} P
 */
const something = /** @type {*} */(null);
export function fn(p = /** @type {P} */(something)) {}";
    assert!(round_trips(source));
}

#[test]
fn jsdoc_typedef_stateless_component() {
    // From expandoFunctionContextualTypesJs: same shape, `?` inside `@typedef`.
    let source = "/**
 * @template P
 * @typedef {{ (): any; defaultProps?: Partial<P> }} StatelessComponent */

const MyComponent = () => null;";
    assert!(round_trips(source));
}

#[test]
fn import_attributes_survive() {
    // Dropping these still yields valid syntax, just a smaller tree — which is
    // exactly why only a round trip catches it.
    let source = r#"import a from "m" with { type: "json" };"#;
    assert!(round_trips(source));
    assert!(printed(source).contains("with"));
}

#[test]
fn import_type_attributes_survive() {
    let source = r#"type T = import("m", { with: { "resolution-mode": "import" } }).Value;"#;
    assert!(round_trips(source));
    let output = printed(source);
    assert!(output.contains(r#", { with: { "resolution-mode": "import" } })"#));
}

#[test]
fn an_object_binding_pattern_stays_an_object() {
    // `BindingPattern.kind` is the opening bracket, not the pattern's kind.
    assert!(printed("const { a } = o;").contains('{'));
    assert!(round_trips("const { a, b: [c] } = o;"));
}

#[test]
fn binding_pattern_trailing_commas_are_preserved() {
    let output = printed("const { a, } = object; const [b,] = array;");
    assert!(output.contains("{ a, }"), "{output}");
    assert!(output.contains("[b,]"), "{output}");
    assert!(round_trips("const { a, } = object; const [b,] = array;"));
}

#[test]
fn tokens_that_would_merge_are_separated() {
    assert!(round_trips("const a = 1 + +2;"));
    assert!(round_trips("const a = 1 - -2;"));
}

#[test]
fn a_trailing_array_elision_remains_an_element() {
    assert!(round_trips("const a = [,]; const b = [1,,]; const c = [1,,,];"));
    assert!(printed("const a = [1,,];").contains("[1, ,]"));
}

#[test]
fn recovered_constructor_syntax_keeps_its_tokens() {
    assert!(round_trips("class C { constructor<>() {} }"));
    assert!(round_trips("class D { *constructor() {} }"));
}

#[test]
fn recovered_new_type_assertion_does_not_gain_a_second_call() {
    let output = printed("const value = new <any>Factory();");
    assert!(output.contains("new <any>Factory()"));
    assert!(!output.contains("Factory()()"));
}

#[test]
fn a_digit_starting_escape_is_a_scan_error_like_upstream() {
    // `\u0031` decodes to `1`, which cannot start an identifier; upstream
    // reports Invalid_character (`invalidUnicodeEscapeSequance4`), so the
    // recovery tree owes no round trip.
    let parsed = tsr_parser::ParsedFile::parse(r"var \u0031a;".to_string());
    assert!(!parsed.diagnostics().is_empty(), "upstream errors here");
    // §208: this asserted the print reproduced the whole spelling,
    // `var \u0031a;`. **Upstream's own emit does not**, and its baseline says
    // so outright — `invalidUnicodeEscapeSequance4.js` records `var u0031a;`.
    // The backslash is not part of any node: the scanner stops the identifier
    // before an escape that cannot begin one, reports the `\` as an invalid
    // character, and `u0031a` is the identifier. The old assertion was pinning
    // this port's own over-consumption.
    //
    // This port prints `var ;` — neither the old spelling nor upstream's, and
    // the difference is NOT the scanner's. Reaching `var u0031a;` needs
    // `parseDelimitedList`'s third recovery arm: at the `\`,
    // `isListElement(PCVariableDeclarations)` is false and `isListTerminator`
    // is false, so upstream reports `Variable_declaration_expected`, **skips
    // one token and retries**, and only then finds `u0031a`. That arm is
    // §191's named wall and needs the `parsingContexts` bitmask
    // (`docs/architecture/checker-notes-nearmiss.md`). Pinned as what this
    // port does, with the distance to upstream stated, rather than left
    // asserting a spelling the scanner no longer produces.
    let output = printed(r"var \u0031a;");
    assert!(output.contains("var ;"), "{output}");
    // The neighbouring line of the same upstream case is the control, and it
    // is why §208 is a correction rather than a loosening: a VALID escape
    // inside an identifier is still CONSUMED, so the identifier is one token
    // spelling `a1` rather than `a` followed by an invalid character.
    //
    // The print shows the decoded form. That is a separate, pre-existing gap
    // and §208 neither caused nor fixed it: upstream's emit keeps the source
    // spelling (`invalidUnicodeEscapeSequance4.js` records
    // `var a\u0031;`) because its printer reuses the original text for an
    // identifier written with an escape. Asserted as what this port does, so
    // the control cannot pass vacuously and the divergence is on the record.
    let valid = printed(r"var a\u0031;");
    assert!(valid.contains("var a1;"), "{valid}");
}

#[test]
fn jsx_attribute_text_remains_raw() {
    let source = "const view = <div title=\"line one\nline two\\\\raw &quot;\" />;";
    assert!(round_trips_as(source, ScriptKind::Tsx));
    let parsed = ParsedFile::parse_with_script_kind(source.to_string(), ScriptKind::Tsx);
    let output = parsed.with_ast(|file| tsr_printer::print(file, parsed.nodes())).text;
    assert!(output.contains("line one\nline two\\\\raw"));
    assert!(!output.contains("line one\\nline two"));

    let quoted = ParsedFile::parse_with_script_kind(
        "const quoted = <div title='\"' />;".to_string(),
        ScriptKind::Tsx,
    );
    let output = quoted.with_ast(|file| tsr_printer::print(file, quoted.nodes())).text;
    assert!(output.contains("title='\"'"));
}

#[test]
fn a_string_survives_its_own_escapes() {
    assert!(round_trips(r#"const a = "he said \"hi\"";"#));
    assert!(round_trips(r"const a = 'a\\b';"));
    assert!(round_trips("const a = 'line\\nbreak';"));
}

#[test]
fn declaration_keywords_come_from_the_flags() {
    // `const`/`let` are NodeFlags, not tokens: printing `var` for all three would
    // produce valid code with different semantics.
    assert!(printed("const a = 1;").contains("const"));
    assert!(printed("let a = 1;").contains("let"));
    assert!(printed("var a = 1;").contains("var"));
}

#[test]
fn optional_and_rest_markers_survive() {
    // These live in `…_token` fields the AST walk does not traverse, so the
    // conformance suite needs its token histogram to see them at all.
    assert!(round_trips("class C { a?: number; b!: string; }"));
    assert!(round_trips("function f(a?: number, ...rest: string[]): void {}"));
    assert!(round_trips("const x = a?.b?.();"));
    assert!(round_trips("function* g() { yield* other(); }"));
}

#[test]
fn jsx_children_are_not_reformatted() {
    // `JsxText` is a node, so inserted whitespace would change the tree.
    assert!(round_trips_as("const a = <div className=\"x\">hello</div>;", ScriptKind::Tsx));
    assert!(round_trips_as("const a = <><b>{x}</b></>;", ScriptKind::Tsx));
    assert!(round_trips_as("const a = <A.B {...p} k:v=\"1\" />;", ScriptKind::Tsx));
}

#[test]
fn a_dot_after_a_number_keeps_its_separator() {
    // `1 .toString()` is legal, and printing `1.toString()` does not parse. Found
    // by probing rather than by the corpus, which contains no instance of it.
    assert!(round_trips("const a = 1 .toString();"));
    assert!(round_trips("const b = 0x10 .toString();"));
    // The idiomatic spelling must survive untouched too.
    assert!(round_trips("const c = 1..toString();"));
}

// ---- deep nesting (bd tsr-el3.1, ADR-0030) -------------------------------

#[test]
fn a_binary_chain_far_deeper_than_the_stack_prints_exactly() {
    // The shape that overflowed CI's debug run:
    // `compiler/binderBinaryExpressionStress` is 4,958 operands of it. Two things
    // keep this working — the left spine is walked iteratively, and every other
    // expression path grows the stack — and the output must be byte-identical.
    let source = format!("const x = {};", "a + ".repeat(10_000) + "a");
    let parsed = ParsedFile::parse(source.clone());
    assert!(parsed.diagnostics().is_empty(), "the fixture must parse");
    let printed = parsed.with_ast(|file| tsr_printer::print(file, parsed.nodes()));
    assert!(printed.is_complete(), "unsupported: {:?}", printed.unsupported);
    assert_eq!(printed.text.trim_end(), source);
}

#[test]
fn a_deeply_nested_expression_prints_without_overflowing() {
    // Right-leaning nesting is not flattened; stack growth is what covers it.
    // Aborts the process if `ensure_sufficient` is removed.
    let source = format!("const x = {}a{};", "(".repeat(5_000), ")".repeat(5_000));
    let parsed = ParsedFile::parse(source);
    let printed = parsed.with_ast(|file| tsr_printer::print(file, parsed.nodes()));
    assert!(!printed.text.is_empty());
}
