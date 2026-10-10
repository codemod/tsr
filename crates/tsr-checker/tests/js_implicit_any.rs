//! `reportImplicitAny` (`checker/checker.go:18275`) in a checked JS file, at
//! the pinned 5b1047d: a JS file reports TS7006 and TS7009 as a TypeScript
//! one does, and a parameter typed by its reparsed `@param`, by its
//! function's `@type` full signature, or contextually by a reparsed `@type`
//! or `@satisfies` wrapper (`parser/reparser.go:344`, `:396`) does not
//! report. Every expectation was checked against native `tsgo`
//! (`docs/parity/notes/r6-jsdoc2.md` §2).
use tsr_ast::Node;
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

/// Every diagnostic of a checked JS file under `noImplicitAny`, as
/// `(code, text at its span)`.
fn diagnostics(source: &str) -> Vec<(u32, String)> {
    let name = "t.js";
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
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
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_no_implicit_any(true);
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

fn implicit_any_parameters(source: &str) -> Vec<String> {
    diagnostics(source).into_iter().filter(|(code, _)| *code == 7006).map(|(_, t)| t).collect()
}

/// An untyped JS parameter is TS7006, as in TypeScript.
#[test]
fn an_untyped_js_parameter_is_ts7006() {
    assert_eq!(implicit_any_parameters("function f(x) { return x; }\n"), ["x"]);
}

/// `@param` and the `@type` full signature type the parameter.
#[test]
fn reparsed_parameter_types_are_annotations() {
    let source = "/** @param {number} y */\nfunction g(y) { return y; }\n\
                  /** @type {(z: number) => number} */\nfunction h(z) { return z; }\n";
    assert!(implicit_any_parameters(source).is_empty());
}

/// A reparsed `@type` on the variable, a `@satisfies` cast and a `return`'s
/// `@type` cast each give the function a contextual signature.
#[test]
fn reparsed_wrappers_are_contextual_types() {
    let source = "/** @type {(w: number) => number} */\nconst k = (w) => w;\n\
                  const m = /** @satisfies {(v: number) => number} */ ((v) => v);\n\
                  function n() {\n    /** @type {(u: number) => number} */\n    return (u) => u;\n}\n";
    assert!(implicit_any_parameters(source).is_empty());
}

/// `@param` names are read with their unicode escapes decoded
/// (`ScanJSDocToken`, `scanner/scanner.go:1490`): `a\u0061` documents `aa`.
#[test]
fn an_escaped_param_name_documents_its_parameter() {
    let source = "/**\n * @param {number} \\u0061\n * @param {number} a\\u0061\n */\n\
                  function foo(a, aa) {}\n";
    assert!(implicit_any_parameters(source).is_empty());
}

/// tsgo gives a JS function no construct signature, so `new F()` is TS7009.
#[test]
fn new_of_a_js_function_is_ts7009() {
    let reported = diagnostics("function F() {}\nconst o = new F();\n");
    assert!(reported.iter().any(|(code, text)| *code == 7009 && text == "new F()"));
}
