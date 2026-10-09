//! `getPropertyNameFromType`'s unique-symbol arm on the element-access road.
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
        .filter(|&id| parsed.nodes.kind(id) == tsr_ast::SyntaxKind::ElementAccessExpression)
        .last()
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

/// `o[N["s"]]`: the index is no entity name, and still names the member
/// declared `[N.s]` (`uniqueSymbols`).
#[test]
fn a_unique_symbol_index_names_its_member_whatever_spells_it() {
    let source = "declare const s: unique symbol;\n\
                  declare namespace N { const s: unique symbol; }\n\
                  declare const o: { [s]: \"a\", [N.s]: \"b\" };\n\
                  o[N[\"s\"]];";
    assert_eq!(last_element_access(source), "\"b\"");
}

/// A unique symbol the receiver has no member for: `getPropertyNameFromType`
/// names no property and no index signature applies, so `nil`, `errorType`
/// (`checker.go:8176`). Before the arm was ported the miss was the gap.
#[test]
fn a_unique_symbol_the_receiver_lacks_is_error_type() {
    let source = "declare const s: unique symbol;\n\
                  declare const t: unique symbol;\n\
                  declare const o: { [s]: \"a\" };\n\
                  o[t];";
    assert_eq!(last_element_access(source), "errorType");
}

/// `typeof globalThis` reads the globals that are not block-scoped, so a
/// `let` misses, and the miss is upstream's `errorType`
/// (`globalThisBlockscopedProperties`).
#[test]
fn a_block_scoped_global_through_global_this_is_error_type() {
    let source = "var x = 1;\nlet y = 2;\nglobalThis['y'];";
    assert_eq!(last_element_access(source), "errorType");
    let source = "var x = 1;\nlet y = 2;\nglobalThis['x'];";
    assert_eq!(last_element_access(source), "number");
}
