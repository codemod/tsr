//! `createUnionOrIntersectionProperty`'s object-literal arm
//! (`checker.go:21545`): a spread-free object-literal constituent without
//! the name contributes `undefined` to the union property.
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last expression of `kind`.
fn last_of(source: &str, kind: tsr_ast::SyntaxKind) -> String {
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
        .filter(|&id| parsed.nodes.kind(id) == kind)
        .last()
        .expect("an access");
    let node = parsed.node_map.get(id).expect("a mapped node");
    let expression = tsr_ast::Expression::try_from(node).expect("an expression");
    let type_id = checker.check_expression(expression);
    checker.type_to_string(type_id)
}

#[test]
fn an_empty_literal_constituent_reads_undefined() {
    let source = "function foo(options?: { a: string, b: number }) {\n\
                  (options || {})[\"a\"];\n\
                  (options || {}).a;\n\
                  }";
    assert_eq!(last_of(source, tsr_ast::SyntaxKind::ElementAccessExpression), "string | undefined");
    assert_eq!(
        last_of(source, tsr_ast::SyntaxKind::PropertyAccessExpression),
        "string | undefined"
    );
}
