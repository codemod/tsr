//! `getContextualTypeForAssignmentExpression` (`checker.go:29843`) answers
//! no contextual type for an assignment declaration whose receiver is not an
//! annotated variable, so a context-sensitive function on its right types
//! from its own parameters (`G.z = (a = 1) => a`). `docs/parity/notes/r5-js.md`
//! §3.4.
use tsr_ast::{Node, SyntaxKind};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's first arrow function.
fn type_of_first_arrow(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut stack: Vec<Node<'_>> = vec![Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        if let Node::ArrowFunction(arrow) = node {
            let id = checker.check_expression(tsr_ast::Expression::ArrowFunction(arrow));
            return checker.type_to_string(id);
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    panic!("the fixture must contain an arrow function ({:?})", SyntaxKind::ArrowFunction)
}

#[test]
fn an_expando_assignment_gives_its_function_no_context() {
    assert_eq!(
        type_of_first_arrow("function G() {}\nG.z = (a = 1) => a;\n"),
        "(a?: number) => number"
    );
}

#[test]
fn a_nested_expando_literal_member_has_no_context_either() {
    assert_eq!(
        type_of_first_arrow("function G() {}\nG.y = { f: (a = 1) => a };\n"),
        "(a?: number) => number"
    );
}
