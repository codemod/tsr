//! `getESSymbolLikeTypeForNode` (`checker.go:22982`): one `unique symbol`
//! per declaration symbol, and `symbol` outside a valid declaration.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The declared type of each variable, by `get_type_from_type_node` on its
/// annotation, as printed and as identities.
fn annotation_types(source: &str) -> Vec<(String, u32)> {
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::default();
    let mut node_map = tsr_ast::NodeMap::default();
    let file = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut node_map,
    );
    assert!(file.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let mut out = Vec::new();
    for statement in file.source_file.statements {
        let Statement::VariableStatement(variable) = statement else { continue };
        let Some(list) = variable.declaration_list else { continue };
        for declaration in list.declarations {
            if let Some(annotation) = declaration.r#type {
                let ty = checker.get_type_from_type_node(annotation);
                out.push((checker.type_to_string(ty), u32::try_from(ty.index()).unwrap()));
            }
        }
    }
    out
}

#[test]
fn a_const_declaration_mints_its_own_unique_symbol() {
    let types =
        annotation_types("declare const a: unique symbol;\ndeclare const b: unique symbol;\n");
    assert_eq!(types[0].0, "unique symbol");
    assert_eq!(types[1].0, "unique symbol");
    assert_ne!(types[0].1, types[1].1, "two declarations, two identities");
}

#[test]
fn parentheses_are_walked_up_to_the_declaration() {
    // `WalkUpParenthesizedTypes(node.Parent)`; a union is not a declaration.
    let types = annotation_types(
        "declare const a: (unique symbol);\ndeclare const b: unique symbol | unique symbol;\n",
    );
    assert_eq!(types[0].0, "unique symbol");
    assert_eq!(types[1].0, "symbol");
}

#[test]
fn an_invalid_position_is_plain_symbol() {
    let types = annotation_types(
        "interface Box<T> { v: T }\ndeclare let a: unique symbol;\ndeclare const b: Box<unique symbol>;\ndeclare const c: [unique symbol];\n",
    );
    let printed: Vec<_> = types.iter().map(|(text, _)| text.as_str()).collect();
    assert_eq!(printed, ["symbol", "Box<symbol>", "[symbol]"]);
}
