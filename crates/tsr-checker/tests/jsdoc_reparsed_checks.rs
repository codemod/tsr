//! The reparsed JSDoc nodes typescript-go's checker visits, pinned to
//! 5b1047d1: `reparseHosted` / `reparseUnhosted` (`parser/reparser.go`) put
//! casts, `@satisfies`, `@this`, full-signature `@type`, `@callback` and
//! `@overload` types into the tree, and `checkSourceElement` checks them like
//! written ones. This port answers them from the JSDoc side table
//! (`jsdoc_checks.rs`); `docs/parity/notes/r5-jsdoc4.md` §1.
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// Every diagnostic of a checked file, as `(code, text at its span)`.
fn diagnostics(source: &str, javascript: bool) -> Vec<(u32, String)> {
    let name = if javascript { "t.js" } else { "t.ts" };
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    if javascript {
        parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            let text = &source[d.span.start as usize..d.span.end as usize];
            (d.message.code(), text.to_string())
        })
        .collect()
}

fn reports(source: &str, code: u32, text: &str) -> bool {
    diagnostics(source, true).iter().any(|(c, t)| *c == code && t == text)
}

/// `thisTag3`: `@this` on a class property's arrow initializer prefixes a
/// `this` parameter (`getFunctionLikeHost` → the arrow), and `checkParameter`
/// reports TS2730 at it — its location is the tag's name.
#[test]
fn a_reparsed_this_parameter_on_an_arrow_is_ts2730_at_the_tag_name() {
    let source = "class C {\n  /**\n   * @this {object}\n   */\n  p = () => 1;\n}\n";
    assert!(reports(source, 2730, "this"));
}

/// A plain function may take `@this`; nothing to report.
#[test]
fn a_reparsed_this_parameter_on_a_function_is_allowed() {
    let source = "/** @this {object} */\nfunction f() {}\n";
    assert!(!diagnostics(source, true).iter().any(|(c, _)| *c == 2730));
}

/// `checkAssertion` checks a JSDoc cast's type (`checker.go:12302`).
#[test]
fn a_cast_type_is_checked() {
    assert!(reports("var x = /** @type {MissingCast} */ (1);\n", 2304, "MissingCast"));
}

/// `checkSatisfiesExpression` checks the `@satisfies` type (`:10743`).
#[test]
fn a_satisfies_type_is_checked() {
    assert!(reports(
        "/** @satisfies {MissingSatisfies} */\nvar y = 1;\n",
        2304,
        "MissingSatisfies"
    ));
}

/// A `@callback` alias's function type is checked like a typedef's.
#[test]
fn a_callback_signature_is_checked() {
    let source = "/**\n * @callback Cb\n * @param {MissingParam} x\n * @returns {MissingReturn}\n */\nvar z = 1;\n";
    assert!(reports(source, 2304, "MissingParam"));
    assert!(reports(source, 2304, "MissingReturn"));
}

/// Each `@overload` of a function declaration is a reparsed signature whose
/// parameter and return types `checkSignatureDeclaration` checks.
#[test]
fn an_overload_signature_is_checked() {
    let source = "/**\n * @overload\n * @param {MissingOverload} a\n * @returns {void}\n */\nfunction g(a) {}\n";
    assert!(reports(source, 2304, "MissingOverload"));
}

/// Inside an object literal no overload signature is made
/// (`PCObjectLiteralMembers`), so nothing is checked.
#[test]
fn an_overload_in_an_object_literal_is_not_reparsed() {
    let source = "var o = {\n  /**\n   * @overload\n   * @param {MissingInLiteral} a\n   */\n  m(a) {}\n};\n";
    assert!(!reports(source, 2304, "MissingInLiteral"));
}

/// A full-signature `@type` is resolved by `checkFunctionOrMethodDeclaration`.
#[test]
fn a_full_signature_type_is_checked() {
    let source = "/** @type {(a: MissingSignature) => void} */\nfunction h(a) {}\n";
    assert!(reports(source, 2304, "MissingSignature"));
}

/// None of this runs for a TypeScript file, whose comments are not reparsed.
#[test]
fn typescript_comments_are_not_checked() {
    let source = "var x = /** @type {MissingCast} */ (1);\n/** @satisfies {MissingSatisfies} */\nvar y = 1;\n";
    assert!(diagnostics(source, false).is_empty());
}
