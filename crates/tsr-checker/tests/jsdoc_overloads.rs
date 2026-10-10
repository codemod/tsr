//! JS `@overload` tags are declarations, pinned to 5b1047d1:
//! `reparseUnhosted`'s `KindJSDocOverloadTag` arm (`parser/reparser.go:134`)
//! puts a body-less declaration per tag before its host, which
//! `getSignaturesOfSymbol` (`checker.go:19806`) reads as an overload and
//! whose host it skips as the implementation. `docs/parity/notes/r6-jsdoc.md`
//! §2.
use tsr_ast::{Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

struct Checked {
    /// The type of every named declaration of the asked kind, in order.
    types: Vec<String>,
    /// Every diagnostic, as `(code, text at its span)`.
    diagnostics: Vec<(u32, String)>,
}

fn check(source: &str, kind: SyntaxKind, name: &str) -> Checked {
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
    checker.set_no_implicit_any(true);
    let mut types = Vec::new();
    for index in 0..parsed.nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        if parsed.nodes.kind(id) != kind
            || parsed
                .nodes
                .parent(id)
                .is_some_and(|parent| matches!(parsed.nodes.kind(parent), SyntaxKind::FunctionType))
        {
            continue;
        }
        let named = match parsed.node_map.get(id) {
            Some(Node::ParameterDeclaration(node)) => {
                matches!(node.name, Some(tsr_ast::BindingName::Identifier(n)) if n.text == name)
            }
            Some(Node::FunctionDeclaration(node)) => node.name.is_some_and(|n| n.text == name),
            _ => false,
        };
        if named {
            let symbol = bound.symbol_of(id).expect("bound declaration");
            let ty = checker.get_type_of_symbol(symbol);
            types.push(checker.type_to_string(ty));
        }
    }
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let diagnostics = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            (d.message.code(), source[d.span.start as usize..d.span.end as usize].to_string())
        })
        .collect();
    Checked { types, diagnostics }
}

const OVERLOADED: &str = "/**
 * @overload
 * @param {number} a
 * @returns {number}
 *
 * @overload
 * @param {string} a
 * @returns {string}
 *
 * @param {string | number} a
 * @returns {string | number}
 */
export function f(a) { return a; }
";

/// `overloadTag1`: the overloads are the function's signatures, and the
/// implementation's parameter reads the comment's own `@param`.
#[test]
fn overload_tags_are_the_signatures_and_the_host_is_the_implementation() {
    assert_eq!(
        check(OVERLOADED, SyntaxKind::FunctionDeclaration, "f").types,
        ["{ (a: number): number; (a: string): string; }"]
    );
    assert_eq!(check(OVERLOADED, SyntaxKind::Parameter, "a").types, ["string | number"]);
}

/// `jsFileMethodOverloads3`: a signature without `@returns` is TS7012 at
/// the tag's name under `noImplicitAny`.
#[test]
fn an_overload_without_a_return_is_ts7012_at_the_tag_name() {
    let source = "/**\n * @overload\n * @param {number} x\n */\n/**\n * @param {number} x\n * @returns {number}\n */\nfunction id(x) { return x; }\n";
    let checked = check(source, SyntaxKind::FunctionDeclaration, "id");
    assert_eq!(checked.types, ["(x: number) => any"]);
    assert!(checked.diagnostics.contains(&(7012, "overload".to_string())));
}

/// `overloadTag1`: TS2394 lands on the incompatible overload's tag name.
#[test]
fn an_incompatible_overload_is_ts2394_at_its_tag_name() {
    let source = "/**
 * @overload
 * @param {number} a
 * @returns {number}
 *
 * @overload
 * @param {boolean} a
 * @returns {string}
 *
 * @param {string | number} a
 * @returns {string | number}
 */
export function g(a) { return a; }
";
    let checked = check(source, SyntaxKind::FunctionDeclaration, "g");
    assert_eq!(
        checked.diagnostics.iter().filter(|(code, _)| *code == 2394).collect::<Vec<_>>(),
        [&(2394, "overload".to_string())]
    );
}
