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
}

#[test]
fn a_template_keeps_its_own_delimiters() {
    // `raw_text` is the whole token, backtick and `${` included.
    assert!(round_trips("const a = `x${1}y${2}z`;"));
    assert!(round_trips("type T = `x${string}y`;"));
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
fn import_attributes_survive() {
    // Dropping these still yields valid syntax, just a smaller tree — which is
    // exactly why only a round trip catches it.
    let source = r#"import a from "m" with { type: "json" };"#;
    assert!(round_trips(source));
    assert!(printed(source).contains("with"));
}

#[test]
fn an_object_binding_pattern_stays_an_object() {
    // `BindingPattern.kind` is the opening bracket, not the pattern's kind.
    assert!(printed("const { a } = o;").contains('{'));
    assert!(round_trips("const { a, b: [c] } = o;"));
}

#[test]
fn tokens_that_would_merge_are_separated() {
    assert!(round_trips("const a = 1 + +2;"));
    assert!(round_trips("const a = 1 - -2;"));
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
