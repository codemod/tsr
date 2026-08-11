//! §195. The type of a JSX attribute's NAME.
//!
//! `checkJsxAttribute` (`internal/checker/jsx.go:871`) is three lines — the
//! initialiser's type at a mutable location, or `true` when the attribute is
//! bare, because `<Elem attr />` is sugar for `<Elem attr={true} />`. The
//! `.types` walker reaches it through the attribute's name, which is a
//! declaration name, so `x` in `<div x={10} />` prints `number` and not the
//! literal `10`: the widening is `checkExpressionForMutableLocation`'s.
//!
//! `getTypeOfVariableOrParameterOrPropertyWorker` (`checker.go:16578`) had no
//! `KindJsxAttribute` arm at all and answered the error type, which after
//! §180's `hadErrorBaseline` guard prints as `any` — a wrong answer wearing a
//! plausible spelling.
//!
//! # Why these are integration tests and not `tsr-checker` unit tests
//!
//! The rule is only observable through the walk: it is reached from a
//! *declaration name*, not from an expression, so a test that calls
//! `check_expression` on the initialiser cannot see it. `type_at_location` is
//! the entry the suite itself uses.

use tsr_ast::Node;
use tsr_conformance::types_producer::{assertions_for_file, type_at_location};

/// Every `{text} : {type}` pair for a `.tsx` source, through the real checker
/// and the real walk.
fn typed_tsx(source: &str) -> Vec<(String, String)> {
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse_with_script_kind(&arena, source, tsr_parser::ScriptKind::Tsx);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.tsx", text: source },
    );
    let mut checker = tsr_checker::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    assertions_for_file(
        &Node::SourceFile(parsed.source_file),
        source,
        &parsed.nodes,
        &parsed.node_map,
        |id| type_at_location(&mut checker, &bound, &parsed.nodes, &parsed.node_map, id),
    )
    .into_iter()
    .map(|a| (a.text, a.type_string))
    .collect()
}

/// The declarations every fixture needs: the walk only reaches an attribute
/// name inside a JSX element, and an element only types if `JSX.Element` is in
/// scope.
const PRELUDE: &str = "declare namespace JSX {\n    interface Element { }\n    interface IntrinsicElements { [name: string]: any }\n}\n";

fn attribute_type(source: &str, name: &str) -> String {
    let rendered = typed_tsx(&format!("{PRELUDE}{source}"));
    rendered
        .iter()
        .find(|(text, _)| text == name)
        .map_or_else(|| panic!("no assertion for {name:?} in {rendered:?}"), |(_, ty)| ty.clone())
}

#[test]
fn an_attribute_name_takes_the_initialisers_widened_type() {
    assert_eq!(attribute_type("<div x={10} />;", "x"), "number");
    assert_eq!(attribute_type("<div x={\"s\"} />;", "x"), "string");
    assert_eq!(attribute_type("<div x={true} />;", "x"), "boolean");
}

/// The widening is the point. Without `checkExpressionForMutableLocation` the
/// answer is the fresh literal type `10`, which is what the initialiser itself
/// prints one line below — the two lines are supposed to differ.
#[test]
fn the_attribute_widens_but_its_initialiser_does_not() {
    let rendered = typed_tsx(&format!("{PRELUDE}<div x={{10}} />;"));
    assert!(rendered.contains(&("x".to_string(), "number".to_string())), "{rendered:?}");
    assert!(rendered.contains(&("10".to_string(), "10".to_string())), "{rendered:?}");
}

/// A string-literal initialiser is written without braces and is a different
/// arm of `JsxAttributeValue`.
#[test]
fn a_string_attribute_value_is_a_string() {
    assert_eq!(attribute_type("<div x=\"s\" />;", "x"), "string");
}

/// `<Elem attr />` is sugar for `<Elem attr={true} />` — upstream's own comment
/// at `jsx.go:875`.
#[test]
fn a_bare_attribute_is_true() {
    assert_eq!(attribute_type("<div x />;", "x"), "true");
}
