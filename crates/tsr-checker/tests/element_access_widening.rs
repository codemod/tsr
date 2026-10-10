//! `checkElementAccessExpression` widens the receiver of an assignment
//! target or a called method before the lookup (`checker.go:8148`).
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last element access expression, or
/// `gap`/`errorType` for the two error identities.
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
    let intrinsics = checker.intrinsics();
    if type_id == intrinsics.native_error {
        "errorType".to_owned()
    } else if type_id == intrinsics.error {
        "gap".to_owned()
    } else {
        checker.type_to_string(type_id)
    }
}

/// `getWidenedType` of `{ a: string; b: number; } | {}` reduces to `{}`, so
/// the write target names no property: `errorType`
/// (`propertyAccessWidening`).
#[test]
fn a_written_union_with_an_empty_literal_is_widened_first() {
    let source = "function foo(options?: { a: string, b: number }) {\n\
                  (options || {})[\"a\"] = 1;\n\
                  }";
    assert_eq!(last_element_access(source), "errorType");
}

/// A write through a declared receiver is unchanged by widening.
#[test]
fn a_written_declared_receiver_keeps_its_member() {
    let source = "declare const o: { a: string };\no[\"a\"] = \"x\";";
    assert_eq!(last_element_access(source), "string");
}
