//! `getApparentType`'s head (`checker.go:21731`): an instantiable type with
//! no base constraint is read as `unknownType`, whose apparent type without
//! `strictNullChecks` is the empty object, so `Object`'s members answer.
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last expression of `kind`, checked with
/// `strictNullChecks` as given.
fn last_of(source: &str, kind: tsr_ast::SyntaxKind, strict: bool) -> String {
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
    checker.set_strict_null_checks(strict);
    #[allow(clippy::cast_possible_truncation)]
    let id = (0..parsed.nodes.len() as u32)
        .map(tsr_ast::NodeId::new)
        .rfind(|&id| parsed.nodes.kind(id) == kind)
        .expect("an expression of the kind");
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

const SOURCE: &str = "interface Object { toString(): string; }\n\
                      function f<T>(x: T) {\n\
                      x['toString'];\n\
                      x.toString;\n\
                      }";

/// Without `strictNullChecks` an unconstrained `T` reads `Object`'s members
/// on both roads (`propertyAccessOnTypeParameterWithoutConstraints`).
#[test]
fn an_unconstrained_type_parameter_reads_object_members() {
    use tsr_ast::SyntaxKind as K;
    assert_eq!(last_of(SOURCE, K::ElementAccessExpression, false), "() => string");
    assert_eq!(last_of(SOURCE, K::PropertyAccessExpression, false), "() => string");
}
