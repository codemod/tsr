//! Late-bound assignment declarations (`foo[k] = v`): the binder files them
//! under the target's `InternalSymbolNameAssignmentDeclaration` export
//! (`binder.go:1000`) and `getResolvedMembersOrExportsOfSymbol` late-binds
//! them (`checker.go:15963`), so the function's type carries the members.
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's last expression of `kind`, or
/// `gap`/`errorType` for the two error identities.
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

/// A late-bound string key reads back its assigned member.
#[test]
fn a_late_bound_assignment_is_a_member_of_the_function() {
    let source = "function foo() {}\n\
                  const strMem = \"strMemName\";\n\
                  foo[strMem] = \"ok\";\n\
                  foo[strMem];";
    assert_eq!(last_of(source, tsr_ast::SyntaxKind::ElementAccessExpression), "string");
}

/// The function's type prints the late member after the early ones, named by
/// its key's type (`declarationEmitLateBoundAssignments`).
#[test]
fn the_function_type_prints_its_late_bound_members() {
    let source = "function foo() {}\n\
                  foo.bar = 12;\n\
                  const dashStrMem = \"dashed-str-mem\";\n\
                  foo[dashStrMem] = \"ok\";\n\
                  const numMem = 42;\n\
                  foo[numMem] = \"ok\";\n\
                  foo;";
    assert_eq!(
        last_of(source, tsr_ast::SyntaxKind::Identifier),
        "{ (): void; bar: number; \"dashed-str-mem\": string; 42: string; }"
    );
}

/// With the members bound, a key the function lacks is a complete miss:
/// `errorType` (`checker.go:8176`).
#[test]
fn a_key_the_function_lacks_is_error_type() {
    let source = "function foo() {}\n\
                  const strMem = \"strMemName\";\n\
                  foo[strMem] = \"ok\";\n\
                  const other = \"other\";\n\
                  foo[other];";
    assert_eq!(last_of(source, tsr_ast::SyntaxKind::ElementAccessExpression), "errorType");
}
