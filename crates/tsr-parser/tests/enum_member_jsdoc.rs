//! Native enum members own their leading JSDoc instead of losing link/deprecation tags.
use tsr_ast::Statement;
use tsr_core::Arena;

#[test]
fn enum_member_keeps_its_concrete_documentation_host() {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse(&arena, "enum E { /** @deprecated old */ A = 1, /** @see E */ B = 2 }");
    assert!(parsed.diagnostics.is_empty(), "{:?}", parsed.diagnostics);
    let Statement::EnumDeclaration(enumeration) = parsed.source_file.statements[0] else {
        panic!("expected enum")
    };
    let first = enumeration.members[0].node_id.unwrap();
    let second = enumeration.members[1].node_id.unwrap();
    assert!(matches!(parsed.jsdoc.get(first)[0].tags[0], tsr_ast::JSDocTag::JSDocDeprecatedTag(_)));
    assert!(matches!(parsed.jsdoc.get(second)[0].tags[0], tsr_ast::JSDocTag::JSDocSeeTag(_)));
    assert_ne!(first, second);
}
