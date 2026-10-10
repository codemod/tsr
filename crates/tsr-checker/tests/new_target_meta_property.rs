//! `checkMetaProperty`'s type half (`checker.go:10753`): `new.target` is the
//! class's or the function's own type inside a constructor or a function,
//! and `errorType` outside one; `import.<other>` is `errorType`
//! (`docs/parity/notes/r6-errorsplit2.md` §8).
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last meta-property, or `gap`/`errorType`
/// for the two error identities.
fn last_meta_property(source: &str) -> String {
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
        .rfind(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::MetaProperty)
        .expect("a meta-property");
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

#[test]
fn new_target_in_a_constructor_is_the_class() {
    let source = "class C2 {\n  constructor() { const t = new.target; }\n}";
    assert_eq!(last_meta_property(source), "typeof C2");
}

#[test]
fn new_target_in_a_function_is_the_function() {
    let source = "function f() { return new.target; }";
    assert_eq!(last_meta_property(source), "() => any");
}

#[test]
fn new_target_outside_a_function_is_error_type() {
    let source = "const a = new.target;";
    assert_eq!(last_meta_property(source), "errorType");
}

#[test]
fn new_target_in_an_arrow_at_top_level_is_error_type() {
    let source = "const a = () => new.target;";
    assert_eq!(last_meta_property(source), "errorType");
}
