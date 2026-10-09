//! The contextual type of an operand typescript-go's reparser wraps in a cast,
//! pinned to 5b1047d1: `reparseHosted`'s `KindJSDocTypeTag` arm turns a
//! `@type` on a `return` or a parenthesized expression into an
//! `AsExpression` around the operand (`parser/reparser.go:378`), and
//! `getContextualType`'s `KindAsExpression` arm answers the asserted type.
//! This port answers it from the JSDoc side table (`jsdoc_annotations.rs`);
//! `docs/parity/notes/r5-jsdoc5.md` §4.1.
use tsr_ast::{Node, NodeId};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of every function-expression or arrow parameter named `name` in a
/// JS file, in source order (not the comment's own signature parameters).
fn parameter_types(source: &str, name: &str) -> Vec<String> {
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
        let Some(Node::ParameterDeclaration(parameter)) = parsed.node_map.get(id) else {
            continue;
        };
        if !matches!(parameter.name, Some(tsr_ast::BindingName::Identifier(n)) if n.text == name)
            || !parsed.nodes.parent(id).is_some_and(|function| {
                matches!(
                    parsed.nodes.kind(function),
                    tsr_ast::SyntaxKind::FunctionExpression | tsr_ast::SyntaxKind::ArrowFunction
                )
            })
        {
            continue;
        }
        let symbol = bound.symbol_of(id).expect("bound parameter");
        let ty = checker.get_type_of_symbol(symbol);
        out.push(checker.type_to_string(ty));
    }
    out
}

/// `jsdocSignatureOnReturnedFunction`'s `f4`: the `@type` on a `return`
/// casts the function expression, whose parameters take the cast's
/// signature as their context.
#[test]
fn a_returned_function_takes_its_return_cast_as_context() {
    let source = "function f() {\n  /** @type {(a: number) => number} */\n  return function (a) { return a; };\n}\n";
    assert_eq!(parameter_types(source, "a"), ["number"]);
}

/// The same cast on a parenthesized expression (`makeNewCast` for
/// `KindParenthesizedExpression`).
#[test]
fn a_parenthesized_arrow_takes_its_cast_as_context() {
    let source = "var g = /** @type {(a: string) => string} */ ((a) => a);\n";
    assert_eq!(parameter_types(source, "a"), ["string"]);
}

/// `@type {const}` is not a contextual type (`isConstTypeReference`).
#[test]
fn a_const_cast_gives_no_context() {
    let source = "function f() {\n  /** @type {const} */\n  return (a) => a;\n}\n";
    assert_eq!(parameter_types(source, "a"), ["any"]);
}

/// A `return` whose comment carries no `@type` gives no cast context.
#[test]
fn an_untyped_return_comment_gives_no_context() {
    let source = "function f() {\n  /** returns a function */\n  return (a) => a;\n}\n";
    assert_eq!(parameter_types(source, "a"), ["any"]);
}
