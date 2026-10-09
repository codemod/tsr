//! `checkPropertyAccessExpressionOrQualifiedName`'s `prop == nil` exit
//! (`checker.go:11353-11369`) answers `errorType` where the port's receiver
//! is complete and is no flow reference
//! (`docs/parity/notes/r6-errorsplit2.md` §3).
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last property access expression, or
/// `gap`/`errorType` for the two error identities.
fn last_property_access(source: &str) -> String {
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
        .filter(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::PropertyAccessExpression)
        .last()
        .expect("a property access");
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

/// The widened write target `{}` names no `a`: `errorType`
/// (`propertyAccessWidening`).
#[test]
fn a_written_union_with_an_empty_literal_misses_as_error_type() {
    let source = "function foo(options?: { a: string, b: number }) {\n\
                  (options || {}).a = 1;\n\
                  }";
    assert_eq!(last_property_access(source), "errorType");
}

/// A call result is no flow reference: its miss is `errorType`.
#[test]
fn a_call_result_miss_is_error_type() {
    let source = "declare function f(): { a: string };\nf().b;";
    assert_eq!(last_property_access(source), "errorType");
}

/// A flow reference keeps the gap: its type is the port's narrowing at the
/// access, which can fall short of upstream's.
#[test]
fn a_reference_receiver_miss_stays_the_gap() {
    let source = "declare const o: { a: string };\no.b;";
    assert_eq!(last_property_access(source), "gap");
}

/// A found member is unchanged.
#[test]
fn a_found_member_is_its_type() {
    let source = "declare function f(): { a: string };\nf().a;";
    assert_eq!(last_property_access(source), "string");
}
