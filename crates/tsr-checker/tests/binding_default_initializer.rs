//! Annotated variable binding defaults use their adjusted symbol target, not context.

use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, ScriptTarget, Tristate};

fn assignment_sites(
    source: &str,
    strict: bool,
    exact: bool,
    warm: bool,
    name: &str,
) -> Vec<(u32, u32)> {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let globals = "interface Array<T> { [n: number]: T; length: number } \
        interface ReadonlyArray<T> { readonly [n: number]: T; readonly length: number }";
    let global = tsr_parser::parse_into(
        &arena,
        globals,
        tsr_parser::ParseOptions::for_file("global.d.ts"),
        &mut nodes,
        &mut map,
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        global.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "global.d.ts", text: globals },
    );
    let parsed = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file(name),
        &mut nodes,
        &mut map,
    );
    let root = parsed.source_file.node_id.expect("registered source file");
    // Parsing a JS dialect alone does not stamp the compiler loader's flag.
    if std::path::Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("js")) {
        nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    }
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name, text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &map);
    checker.apply_compiler_options(&CompilerOptions {
        strict: Tristate::from_bool(strict),
        exact_optional_property_types: Tristate::from_bool(exact),
        target: ScriptTarget::ES2015,
        ..CompilerOptions::default()
    });
    if warm {
        for index in 0..nodes.len() {
            let id = NodeId::new(u32::try_from(index).unwrap());
            if matches!(map.get(id), Some(Node::TypeAliasDeclaration(_))) {
                checker.get_declared_type_of_symbol(bound.symbol_of(id).unwrap());
            }
        }
        for index in 0..nodes.len() {
            let id = NodeId::new(u32::try_from(index).unwrap());
            if matches!(map.get(id), Some(Node::BindingElement(_)))
                && let Some(symbol) = bound.symbol_of(id)
            {
                checker.get_type_of_symbol(symbol);
            }
        }
    }
    checker.check_source_file(
        root,
        FileContext {
            ambient: name.ends_with(".d.ts"),
            has_parse_errors: !parsed.diagnostics.is_empty(),
        },
    );
    let mut sites: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(file, diagnostic)| *file == root && diagnostic.message.code() == 2322)
        .map(|(_, diagnostic)| (diagnostic.span.start, diagnostic.span.end))
        .collect();
    sites.sort_unstable();
    sites
}

fn site(source: &str, name: &str) -> (u32, u32) {
    let start = u32::try_from(source.find(name).unwrap()).unwrap();
    (start, start + u32::try_from(name.len()).unwrap())
}

#[test]
fn annotated_binding_literals_report_at_the_leaf_after_aliases_and_holes() {
    for source in [
        r#"type Target = { right?: "right"; left: "left" }; type Linked = Target;
            declare const input: Linked;
            let { right: good = "right", left: bad = "right" }: Linked = input;"#,
        r#"type Target = ["skip", "right"?]; type Linked = Target;
            declare const input: Linked; let [, bad = "wrong"]: Linked = input;"#,
        r#"declare const input: { outer?: { x: "right" } };
            let { outer: { x: bad = "wrong" } = { x: "right" } }:
                { outer?: { x: "right" } } = input;"#,
        r#"type Target = [("left" | "right")?, ("middle" | "end")?]; type Linked = Target;
            declare const input: Linked; let [good = "left", bad = "other"]: Linked = input;"#,
    ] {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                assert_eq!(
                    assignment_sites(source, strict, exact, warm, "a.ts"),
                    vec![site(source, "bad")],
                    "strict={strict} exact={exact} warm={warm}: {source}"
                );
            }
        }
    }
}

#[test]
fn annotated_binding_defaults_preserve_null_and_undefined_target_adjustment() {
    for annotation in [r#"{ x?: "right" }"#, r#"["right"?]"#, r#"[("right" | undefined)?]"#] {
        let pattern = if annotation.starts_with('{') {
            "{ x: chosen = undefined }"
        } else {
            "[chosen = undefined]"
        };
        let source =
            format!("declare const input: {annotation}; let {pattern}: {annotation} = input;");
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                assert!(assignment_sites(&source, strict, exact, warm, "a.ts").is_empty());
            }
        }
    }
    for source in [
        r#"declare const input: ["right"]; let [chosen = undefined]: ["right"] = input;"#,
        r#"declare const input: { x?: "right" }; let { x: chosen = null }: { x?: "right" } = input;"#,
    ] {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                let expected = if strict { vec![site(source, "chosen")] } else { vec![] };
                assert_eq!(assignment_sites(source, strict, exact, warm, "a.ts"), expected);
            }
        }
    }
    let nullable = r#"declare const input: ["right" | null];
        let [chosen = null]: ["right" | null] = input;"#;
    assert!(assignment_sites(nullable, true, true, false, "a.ts").is_empty());
}

/// Sources upstream accepts: a structured default typed under a nullable
/// element context (the port declines it, see `check_binding_element_initializer`),
/// a self reference, an unannotated root (the default joins the element type)
/// and an `unknown` root.
#[test]
fn binding_default_reporter_accepts_what_upstream_accepts() {
    for source in [
        r#"interface I { tag: "right" } declare const input: [I?];
            let [chosen = { tag: "right" }]: [I?] = input;"#,
        r#"interface I { tag: "right" } declare const input: [(I | undefined)?];
            let [chosen = { tag: "right" }]: [(I | undefined)?] = input;"#,
        r#"interface J { x: { a: "right" } } declare const input: [J?];
            let [chosen = class { static x = { a: "right" } }]: [J?] = input;"#,
        r#"interface J { x: { a: "right" } } declare const input: [(J | undefined)?];
            let [chosen = class { static x = { a: "right" } }]: [(J | undefined)?] = input;"#,
        r#"declare const input: ["right"]; let [chosen = chosen]: ["right"] = input;"#,
        r#"declare const input: ["right"]; let [chosen = "wrong"] = input;"#,
        r#"declare const input: unknown; let [chosen = "wrong"]: unknown = input;"#,
    ] {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                assert!(
                    assignment_sites(source, strict, exact, warm, "a.ts").is_empty(),
                    "strict={strict} exact={exact} warm={warm}: {source}"
                );
            }
        }
    }
}

/// `checkVariableLikeDeclaration` checks every default of a primary binding
/// element against its element type, whatever the source expression, and
/// reports at the element (pinned tsgo reports each of these at `chosen`).
#[test]
fn binding_default_reporter_checks_every_source_expression() {
    for source in [
        r#"declare const input: ["right"]; const other: "wrong" = "wrong";
            let [chosen = other]: ["right"] = input;"#,
        r#"declare const input: ["right"]; let [chosen = ("wrong")]: ["right"] = input;"#,
        r#"declare const input: { x: "right" }; let { ["x"]: chosen = "wrong" }: { x: "right" } = input;"#,
        r#"declare const input: [number]; let [chosen = "wrong"]: [number] = input;"#,
        r#"declare const input: ["right" | number];
            let [chosen = "wrong"]: ["right" | number] = input;"#,
        r#"declare const input: ["right"]; function f(undefined: "wrong") {
            let [chosen = undefined]: ["right"] = input; }"#,
    ] {
        for (strict, exact) in [(false, false), (true, false), (true, true)] {
            for warm in [false, true] {
                assert_eq!(
                    assignment_sites(source, strict, exact, warm, "a.ts"),
                    vec![site(source, "chosen")],
                    "strict={strict} exact={exact} warm={warm}: {source}"
                );
            }
        }
    }
}

#[test]
fn binding_default_reporter_checks_parameters_and_keeps_file_declines() {
    let parameter = r#"function f([chosen = "wrong"]: ["right"?]) {}"#;
    assert_eq!(
        assignment_sites(parameter, true, true, false, "a.ts"),
        vec![site(parameter, "chosen")]
    );
    for source in [
        r#"declare function f([chosen = "wrong"]: ["right"?]): void;"#,
        r#"declare const input: { x: "right" };
            var { x: chosen }: { x: "right" } = input;
            var { x: chosen = "wrong" }: { x: "right" } = input;"#,
        r#"declare const input: ["head", ..."middle"[], "end"];
            let [, ...chosen = "wrong"]: ["head", ..."middle"[], "end"] = input;"#,
    ] {
        assert!(assignment_sites(source, true, true, false, "a.ts").is_empty(), "{source}");
    }
    let source = r#"declare const input: ["right"]; let [chosen = "wrong"]: ["right"] = input;"#;
    assert!(assignment_sites(source, true, true, false, "a.js").is_empty());
    assert!(assignment_sites(source, true, true, false, "a.d.ts").is_empty());
    let malformed = r#"declare const input: ["right"]; let [chosen = ]: ["right"] = input;"#;
    assert!(assignment_sites(malformed, true, true, false, "a.ts").is_empty());
}
