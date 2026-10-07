//! Only a `default` function declaration may omit its name (`parseFunctionDeclaration`).
use tsr_core::{Arena, Span};

#[test]
fn nameless_function_declaration_requires_default() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "function () {}");
    assert_eq!(
        parsed.diagnostics.iter().map(|d| (d.message.code(), d.span)).collect::<Vec<_>>(),
        [(1003, Span::new(9, 10))]
    );
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "export default function () {}");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
}
