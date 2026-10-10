//! `lateBindMember` (`checker.go:16005`) gathers every late-bound assignment
//! with one late name into one symbol, typed over all of its declarations by
//! `getWidenedTypeForAssignmentDeclaration`
//! (`docs/parity/notes/r6-errorsplit2.md` §12).
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last element access expression. The reads
/// sit in a function, where no assignment's flow narrows them.
fn last_element_access(source: &str) -> String {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file("t.ts"));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    #[allow(clippy::cast_possible_truncation)]
    let id = (0..parsed.nodes.len() as u32)
        .map(tsr_ast::NodeId::new)
        .rfind(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ElementAccessExpression)
        .expect("an element access");
    let node = parsed.node_map.get(id).expect("a mapped node");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let type_id = checker.check_expression(expression);
    checker.type_to_string(type_id)
}

#[test]
fn two_late_assignments_to_one_name_union_their_types() {
    let source = "function foo() {}\n\
                  const k = \"x\";\n\
                  foo[k] = 1;\n\
                  foo[k] = \"s\";\n\
                  function read() { return foo[k]; }";
    let answer = last_element_access(source);
    assert!(answer == "string | number" || answer == "number | string", "{answer}");
}

#[test]
fn one_late_assignment_keeps_its_type() {
    let source =
        "function foo() {}\nconst k = \"x\";\nfoo[k] = 1;\nfunction read() { return foo[k]; }";
    assert_eq!(last_element_access(source), "number");
}
