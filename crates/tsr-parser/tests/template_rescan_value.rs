//! Rescanned templates publish cooked values without losing raw escape spelling.
use tsr_ast::{Expression, Statement};
use tsr_core::{Arena, Span};

#[test]
fn untagged_octal_template_rescan_updates_value_and_exact_errors() {
    for (source, cooked, expected) in [
        (
            "const x = `\\5`;",
            "\u{5}",
            vec![(
                1487,
                Span::new(11, 13),
                "Octal escape sequences are not allowed. Use the syntax '\\x05'.",
            )],
        ),
        ("const x = `\\u0061`;", "a", vec![]),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let errors: Vec<_> =
            parsed.diagnostics.iter().map(|d| (d.message.code(), d.span, d.text())).collect();
        assert_eq!(
            errors,
            expected
                .into_iter()
                .map(|(code, span, text)| (code, span, text.to_owned()))
                .collect::<Vec<_>>()
        );
        let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
            panic!("expected variable")
        };
        let Some(list) = statement.declaration_list else { panic!("expected declarations") };
        let Some(Expression::NoSubstitutionTemplateLiteral(template)) =
            list.declarations[0].initializer
        else {
            panic!("expected template")
        };
        assert_eq!(template.text, cooked);
        assert_eq!(template.raw_text, &source[10..source.len() - 1]);
    }
}
