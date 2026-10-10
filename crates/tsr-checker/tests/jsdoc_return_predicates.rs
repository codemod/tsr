//! `@return {x is T}` / `{asserts x}` predicates in JS, pinned to 5b1047d1:
//! `reparseHosted`'s `KindJSDocReturnTag` arm (`parser/reparser.go:514`)
//! makes `tag.TypeExpression().Type()` the function's return type, so a
//! predicate node is the signature's predicate; and a variable's reparsed
//! `@type` is `node.Type()` for `isDeclarationWithExplicitTypeAnnotation`
//! (`flow.go:2197`), so an `asserts` call through it narrows.
//! `docs/parity/notes/r6-jsdoc.md` §3.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// Every diagnostic of a checked JS file, as `(code, text at its span)`.
fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.js"));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    // As the loader stamps a JS file: the root and every comment's root.
    parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    for (_, docs) in parsed.jsdoc.iter() {
        for doc in docs {
            if let Some(id) = doc.node_id {
                parsed.nodes.add_flags(id, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
            }
        }
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.js", text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            (d.message.code(), source[d.span.start as usize..d.span.end as usize].to_string())
        })
        .collect()
}

/// `returnTagTypeGuard`: `@return {value is A}` narrows the argument.
#[test]
fn a_return_tag_predicate_narrows_the_argument() {
    let source = "/** @typedef {{ a: number }} A */
/** @typedef {{ b: string }} B */
/**
 * @param {any} value
 * @return {value is A}
 */
function isA(value) {
    return true;
}
/** @param {A | B} val */
function foo(val) {
    if (isA(val)) {
        val.b;
    }
}
";
    let d = diagnostics(source);
    assert!(d.contains(&(2339, "b".to_string())), "{d:?}");
}

/// `assertionsAndNonReturningFunctions`: a `const` typed by `@type` with an
/// `asserts` signature is an explicitly typed callee, so its call narrows.
#[test]
fn an_asserts_call_through_a_jsdoc_typed_const_narrows() {
    let source = "/** @typedef {(x: unknown) => asserts x is { a: number }} AssertA */
/** @type {AssertA} */
const assertA = x => {};
/** @param {*} x */
function f1(x) {
    assertA(x);
    x.b;
}
";
    let d = diagnostics(source);
    assert!(d.contains(&(2339, "b".to_string())), "{d:?}");
}
