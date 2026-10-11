//! Every node but the file and a JSDoc root has a parent: a node the parse
//! made and then dropped stays in the table, where a scan of the file's node
//! range finds it (docs/parity/notes/r7-parser.md §6).

use tsr_ast::SyntaxKind;
use tsr_core::Arena;
use tsr_parser::ParseOptions;

fn orphans(path: &str, source: &str) -> Vec<SyntaxKind> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse_with_options(&arena, source, ParseOptions::for_file(path));
    (0..parsed.nodes.len())
        .map(|i| tsr_ast::NodeId::new(u32::try_from(i).unwrap()))
        .filter(|&id| parsed.nodes.parent(id).is_none())
        .map(|id| parsed.nodes.kind(id))
        .filter(|kind| !matches!(kind, SyntaxKind::SourceFile | SyntaxKind::JSDoc))
        .collect()
}

#[test]
fn class_expressions_make_one_node() {
    assert_eq!(orphans("a.ts", "const x = class {};"), []);
    assert_eq!(orphans("a.ts", "const x = (@d class C {});"), []);
}

#[test]
fn export_forms_without_a_modifier_make_no_export_token() {
    for source in [
        "export = x;",
        "export default 1;",
        "export { a };",
        "export * from \"m\";",
        "export as namespace N;",
    ] {
        assert_eq!(orphans("a.ts", source), [], "{source}");
    }
}

#[test]
fn an_import_equals_declaration_makes_no_type_token() {
    assert_eq!(orphans("a.ts", "import type A = require(\"a\");"), []);
}

#[test]
fn typeof_import_makes_one_import_type() {
    assert_eq!(orphans("a.ts", "type T = typeof import(\"m\").A;"), []);
}
