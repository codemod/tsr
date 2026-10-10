//! `ScanJSDocToken` (`scanner/scanner.go:1490`, `:1515`) at the pinned
//! 5b1047d: a unicode escape that starts or continues a JSDoc name is part of
//! it, and `parseJSDocIdentifierName` reads the decoded token value
//! (`docs/parity/notes/r6-jsdoc2.md` §2).
use tsr_ast::{EntityName, JSDocTag};
use tsr_core::Arena;

fn param_names(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    parsed
        .jsdoc
        .iter()
        .flat_map(|(_, docs)| docs.iter().flat_map(|doc| doc.tags.iter()))
        .map(|tag| {
            let JSDocTag::JSDocParameterOrPropertyTag(tag) = tag else { panic!("expected @param") };
            let Some(EntityName::Identifier(name)) = tag.name else { panic!("a simple name") };
            name.text.to_string()
        })
        .collect()
}

#[test]
fn escapes_in_param_names_are_decoded() {
    let source = "/**\n * @param {number} \\u0061\n * @param {number} a\\u0061\n \
                  * @param {number} \\u{0062}c\n */\nfunction foo(a, aa, bc) {}";
    assert_eq!(param_names(source), ["a", "aa", "bc"]);
}
