//! `getContextualTypeForAssignmentExpression`'s `F[xxx] = expr` arm
//! (`checker.go:29859-29863`): an expando assignment on a variable with a type
//! annotation reads its contextual type from the annotation's property named
//! by the key's type, so `bar[t] = true` keeps the literal `true`
//! (`expandoFunctionExpressionsWithDynamicNames2`,
//! `docs/parity/notes/r6-errorsplit2.md` §11).
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's first arrow function.
fn first_arrow(source: &str) -> String {
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
        .find(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ArrowFunction)
        .expect("an arrow function");
    let node = parsed.node_map.get(id).expect("a mapped node");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let type_id = checker.check_expression(expression);
    checker.type_to_string(type_id)
}

#[test]
fn a_const_key_reads_the_annotated_property() {
    let source = "interface Bar {\n  (): void;\n  test: true;\n}\n\
                  const t = \"test\" as const;\n\
                  const bar: Bar = () => {};\n\
                  bar[t] = true;";
    assert_eq!(first_arrow(source), "{ (): void; test: true; }");
}

#[test]
fn a_literal_key_still_reads_the_annotated_property() {
    let source = "interface Bar {\n  (): void;\n  test: true;\n}\n\
                  const bar: Bar = () => {};\n\
                  bar[\"test\"] = true;";
    assert_eq!(first_arrow(source), "{ (): void; test: true; }");
}
