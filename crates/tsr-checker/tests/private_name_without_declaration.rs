//! `checkPropertyAccessExpressionOrQualifiedName`'s private-name arm when no
//! enclosing class declares the name (`checker.go:11284-11310`):
//! `anyType` for an any-like receiver outside every class body, otherwise
//! `errorType` (`docs/parity/notes/r6-errorsplit2.md` §4).
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
        .rfind(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::PropertyAccessExpression)
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

/// Outside every class body, an `any` receiver answers `anyType`
/// (`privateNameBadAssignment`'s `A.prototype.#no`).
#[test]
fn an_any_receiver_outside_a_class_is_any() {
    let source = "declare const a: any;\na.#nope;";
    assert_eq!(last_property_access(source), "any");
}

/// Inside a class that does not declare the name: `errorType`.
#[test]
fn an_undeclared_name_inside_a_class_is_error_type() {
    let source = "class C {\n  #bar = 6;\n  m() { return this.#foo; }\n}";
    assert_eq!(last_property_access(source), "errorType");
}

/// Outside the declaring class, the type's own private member is reported
/// as not accessible: `errorType`.
#[test]
fn a_private_member_read_outside_its_class_is_error_type() {
    let source = "class C {\n  #bar = 6;\n}\ndeclare const c: C;\nc.#bar;";
    assert_eq!(last_property_access(source), "errorType");
}

/// The declaring class still reads its member.
#[test]
fn a_declared_name_reads_its_member() {
    let source = "class C {\n  #bar = 6;\n  m() { return this.#bar; }\n}";
    assert_eq!(last_property_access(source), "number");
}
