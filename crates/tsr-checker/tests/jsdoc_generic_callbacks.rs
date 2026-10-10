//! A generic JS `@callback` (or function-type `@typedef`) alias gives its
//! instantiation a signature, pinned to 5b1047d1: the reparser makes either
//! a `JSTypeAliasDeclaration` (`reparseUnhosted`, `parser/reparser.go:70`),
//! whose instantiation `getTypeAliasInstantiation` reads like a written
//! alias's; and `gatherTypeParameters` (`:293`) gives no comment carrying a
//! typedef or callback to the host it documents. `docs/parity/notes/r6-jsdoc.md`
//! §4.
use tsr_ast::{Expression, Node, NodeId, SyntaxKind};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of every arrow function's parameters and of every arrow
/// initializer, in source order.
fn arrow_types(source: &str) -> Vec<String> {
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
    let mut out = Vec::new();
    for index in 0..parsed.nodes.len() {
        let id = NodeId::new(u32::try_from(index).unwrap());
        if parsed.nodes.kind(id) != SyntaxKind::VariableDeclaration {
            continue;
        }
        let Some(Node::VariableDeclaration(declaration)) = parsed.node_map.get(id) else {
            continue;
        };
        let Some(initializer @ Expression::ArrowFunction(arrow)) = declaration.initializer else {
            continue;
        };
        let ty = checker.check_expression(initializer);
        out.push(checker.type_to_string(ty));
        for parameter in arrow.parameters {
            let symbol = bound.symbol_of(parameter.node_id.unwrap()).expect("parameter symbol");
            let ty = checker.get_type_of_symbol(symbol);
            out.push(checker.type_to_string(ty));
        }
    }
    out
}

/// `callbackTag2`: `@type {Id<string>}` contextually types the arrow.
#[test]
fn a_generic_callback_instantiation_types_the_arrow() {
    let source = "/** @template T
 * @callback Id
 * @param {T} t
 * @returns {T}
 */
var x = 1;
/** @type {Id<string>} */
var one = s => s;
";
    assert_eq!(arrow_types(source), ["(s: string) => string", "string"]);
}

/// A function-type `@typedef` with `@template` reads the same way.
#[test]
fn a_generic_function_typedef_instantiation_types_the_arrow() {
    let source = "/** @template T
 * @typedef {(t: T) => T} F
 */
var x = 1;
/** @type {F<number>} */
var f = n => n;
";
    assert_eq!(arrow_types(source), ["(n: number) => number", "number"]);
}

/// The callback's `@template` belongs to its alias, not to the statement
/// the comment happens to precede.
#[test]
fn a_callback_comment_does_not_parameterize_its_host() {
    let source = "/**
 * @template V
 * @callback One
 * @param {V} barts
 * @return {V}
 */
/** @type {One<string>} */
var b = t => t;
";
    assert_eq!(arrow_types(source), ["(t: string) => string", "string"]);
}
