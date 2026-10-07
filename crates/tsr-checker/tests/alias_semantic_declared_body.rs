//! Native 5b1047d getDeclaredTypeOfTypeAlias / getTypeAliasInstantiation:
//! constructors without an alias retain and instantiate their semantic body.

use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn alias_free_bodies_map_ordered_arguments_and_preserve_value_identity() {
    let source = r"
        declare const value: { item: string };
        type Value<T> = typeof value;
        type Keys<T> = keyof T;
        type NestedKeys<T> = (keyof T);
        declare const a: Value<number>;
        declare const b: Value<string>;
        declare const keys: Keys<{ left: number; right: string }>;
        declare const nested: NestedKeys<{ first: number }>;
        declare const distinct: Keys<{ other: boolean }>;
    ";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "alias-control.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut answers = Vec::new();
    // Cold reverse order prevents the declared-body cache seed from disguising
    // a factory that only returns its own parameter instantiation correctly.
    for name in ["distinct", "nested", "keys", "b", "a", "value", "keys"] {
        let symbol = bound.lookup_local(root, name).unwrap();
        let ty = checker.get_type_of_symbol(symbol);
        answers.push((ty, checker.type_to_string(ty)));
    }
    assert_eq!(answers[0].1, "\"other\"");
    assert_eq!(answers[1].1, "\"first\"");
    assert_eq!(answers[2].1, "\"left\" | \"right\"");
    assert_eq!(answers[3].0, answers[5].0);
    assert_eq!(answers[4].0, answers[5].0);
    assert_eq!(answers[6].0, answers[2].0);
}
