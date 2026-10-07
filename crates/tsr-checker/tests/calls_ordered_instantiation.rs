//! Ordered argument maps remain distinct for one canonical parsed signature.
use tsr_checker::Checker;

#[test]
fn swapped_argument_vectors_keep_distinct_cold_and_warm_returns() {
    let source = "declare function select<A, B>(first: A, second: B): A;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "ordered.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let signature = checker
        .get_signatures_of_symbol(bound.lookup_local(root, "select").unwrap())
        .unwrap()
        .remove(0);
    let string = checker.intrinsics().string;
    let number = checker.intrinsics().number;
    let string_first = checker.get_signature_instantiation(&signature, &[string, number]).unwrap();
    let number_first = checker.get_signature_instantiation(&signature, &[number, string]).unwrap();
    for (arguments, expected) in [([string, number], "string"), ([number, string], "number")] {
        let warm = checker.get_signature_instantiation(&signature, &arguments).unwrap();
        let returned = checker.mapped_signature_return(&warm).unwrap();
        assert_eq!(checker.type_to_string(returned), expected);
    }
    let returned = checker.mapped_signature_return(&string_first).unwrap();
    assert_eq!(checker.type_to_string(returned), "string");
    let returned = checker.mapped_signature_return(&number_first).unwrap();
    assert_eq!(checker.type_to_string(returned), "number");
}
