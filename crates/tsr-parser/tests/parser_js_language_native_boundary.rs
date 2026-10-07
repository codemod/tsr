use tsr_ast::{Expression, Statement};
use tsr_core::Arena;
use tsr_parser::{ParseOptions, parse_with_options};

#[test]
fn parser_js_language_native_unary_recovery() {
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
fn parser_js_language_native_parenthesized_ambiguity() {
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
