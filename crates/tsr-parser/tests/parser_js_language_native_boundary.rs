//! Native JavaScript JSX boundaries and nested type-list publication.
use tsr_ast::{Expression, Statement};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_with_options};

#[test]
fn javascript_unary_jsx_recovery_preserves_diagnostics() {
    for name in ["a.js", "a.mjs", "a.cjs", "a.jsx"] {
        for (source, expected) in [
            ("~< <\n", vec![(1003, 3, 4), (1109, 4, 4)]),
            ("~<></> <\n", vec![(1109, 8, 8)]),
            ("!< {:>\n\n", vec![(17008, 2, 2), (1003, 3, 4), (1005, 4, 5), (1005, 8, 8)]),
        ] {
            let arena = Arena::new();
            let parsed = parse_with_options(&arena, source, ParseOptions::for_file(name));
            let actual: Vec<_> = parsed
                .diagnostics
                .iter()
                .map(|d| (d.message.code(), d.span.start, d.span.end))
                .collect();
            assert_eq!(actual, expected, "{name}: {source:?}");
            if source.starts_with('!') {
                let messages: Vec<_> = parsed.diagnostics.iter().map(|d| d.text()).collect();
                assert_eq!(
                    messages,
                    [
                        "JSX element '' has no corresponding closing tag.",
                        "Identifier expected.",
                        "'...' expected.",
                        "'</' expected.",
                    ]
                );
            }
        }
    }
}

#[test]
fn javascript_call_brackets_are_relational_not_type_arguments() {
    for name in ["a.js", "a.mjs", "a.cjs", "a.jsx", "a.ts", "a.tsx"] {
        let arena = Arena::new();
        let parsed = parse_with_options(&arena, "f<T>(x);", ParseOptions::for_file(name));
        assert!(parsed.diagnostics.is_empty(), "{name}: {:?}", parsed.diagnostics);
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected expression statement");
        };
        if ParseOptions::for_file(name).script_kind.is_javascript() {
            assert!(matches!(statement.expression, Some(Expression::BinaryExpression(_))));
        } else {
            let Some(Expression::CallExpression(call)) = statement.expression else {
                panic!("expected typed call in {name}");
            };
            assert!(matches!(call.type_arguments, [tsr_ast::TypeNode::TypeReferenceNode(_)]));
        }
    }
}

#[test]
fn parenthesized_jsx_and_typescript_assertions_remain_distinct() {
    for name in ["a.js", "a.jsx", "a.tsx"] {
        let arena = Arena::new();
        let parsed = parse_with_options(&arena, "(<A/>);", ParseOptions::for_file(name));
        assert!(parsed.diagnostics.is_empty(), "{name}: {:?}", parsed.diagnostics);
        let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected expression statement");
        };
        let Some(Expression::ParenthesizedExpression(parent)) = statement.expression else {
            panic!("expected parenthesized JSX");
        };
        assert!(matches!(parent.expression, Some(Expression::JsxSelfClosingElement(_))));
    }
    let arena = Arena::new();
    let parsed = parse_with_options(&arena, "<T>(x);", ParseOptions::for_file("a.ts"));
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Statement::ExpressionStatement(statement) = parsed.source_file.statements[0] else {
        panic!("expected expression statement");
    };
    assert!(matches!(statement.expression, Some(Expression::TypeAssertion(_))));
}

#[test]
fn published_type_arguments_survive_the_private_parser_owner() {
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::new();
    let mut map = tsr_ast::NodeMap::new();
    // Reserve a distinct preceding file so private IDs must actually relocate.
    let prefix = tsr_parser::ParsedFile::parse("const prefix = 0;".to_owned());
    prefix.publish(&arena, "const prefix = 0;", &mut nodes, &mut map);
    let source = "const x = <A<B<C>>/>; f<B<C>>(x);";
    let private = tsr_parser::ParsedFile::parse_with_options(
        source.to_owned(),
        ParseOptions::for_file("a.tsx"),
    );
    assert!(private.diagnostics().is_empty(), "{:?}", private.diagnostics());
    let published = private.publish(&arena, source, &mut nodes, &mut map);
    drop(private);
    let mut references = Vec::new();
    for index in published.node_range.clone() {
        let id = tsr_ast::NodeId::new(index);
        if let Some(tsr_ast::Node::TypeReferenceNode(reference)) = map.get(id) {
            let Some(tsr_ast::EntityName::Identifier(name)) = reference.type_name else {
                panic!("expected identifier type name");
            };
            if name.text == "B" {
                let [tsr_ast::TypeNode::TypeReferenceNode(argument)] = reference.type_arguments
                else {
                    panic!("B must preserve its nested type argument");
                };
                let Some(tsr_ast::EntityName::Identifier(argument_name)) = argument.type_name
                else {
                    panic!("expected nested identifier");
                };
                assert_eq!(argument_name.text, "C");
                use tsr_ast::HasNodeId;
                let argument_id = argument.node_id().unwrap();
                assert!(published.node_range.contains(&argument_id.as_u32()));
                assert_eq!(nodes.parent(argument_id), Some(id));
                references.push(name.text);
            }
        }
    }
    assert_eq!(references, ["B", "B"]);
}
