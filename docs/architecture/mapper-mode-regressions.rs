// Insert inside inference.rs::tests in a frozen archive before production edits.
#[test]
fn print_created_object_does_not_complete_semantic_instantiation() {
    use crate::flags::TypeFlags;

    for print_first in [true, false] {
        let arena = Arena::new();
        let source = "declare function f<T,U>(x: {value: T; other: U}): U;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapper.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let Statement::FunctionDeclaration(node) = parsed.source_file.statements[0] else {
            panic!("generic function");
        };
        let signature = checker
            .get_signatures_of_symbol(bound.symbol_of(node.node_id.unwrap()).unwrap())
            .unwrap()
            .remove(0);
        let parameters = checker.type_parameter_types(&signature).unwrap();
        let original = signature.parameters[0].r#type;
        let map = [(parameters[0], checker.intrinsics.string)];
        for printing in [print_first, !print_first] {
            checker.identity_unmapped_type_parameters = printing;
            let result = checker.instantiate_type(original, &map, &parameters, &["T", "U"]);
            if printing {
                assert_ne!(
                    result, checker.intrinsics.error,
                    "semantic refusal poisoned print cache"
                );
                assert!(checker.store.get(result).flags.contains(TypeFlags::OBJECT));
            } else {
                assert_eq!(
                    result, checker.intrinsics.error,
                    "print result bypassed semantic refusal"
                );
            }
        }
        checker.identity_unmapped_type_parameters = false;
        assert_eq!(
            checker.instantiate_type(original, &map, &parameters, &["T", "U"]),
            checker.intrinsics.error
        );
    }
}

#[test]
fn print_created_signature_does_not_complete_semantic_instantiation() {
    use crate::flags::TypeFlags;

    for print_first in [true, false] {
        let arena = Arena::new();
        let source = "declare function f<T,U>(x: T): U;";
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty());
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "mapper.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let Statement::FunctionDeclaration(node) = parsed.source_file.statements[0] else {
            panic!("generic function");
        };
        let owner = bound.symbol_of(node.node_id.unwrap()).unwrap();
        let mut signature = checker.get_signatures_of_symbol(owner).unwrap().remove(0);
        let parameters = checker.type_parameter_types(&signature).unwrap();
        signature.type_parameters.clear();
        let original =
            checker.store.new_anonymous(TypeFlags::OBJECT, "(x: T) => U".into(), owner, true);
        checker.signature_types.insert(original, vec![signature]);
        let map = [(parameters[0], checker.intrinsics.string)];
        for printing in [print_first, !print_first] {
            checker.identity_unmapped_type_parameters = printing;
            let result = checker.instantiate_type(original, &map, &parameters, &["T", "U"]);
            if printing {
                assert_ne!(result, checker.intrinsics.error);
                assert!(checker.store.get(result).flags.contains(TypeFlags::OBJECT));
            } else {
                assert_eq!(result, checker.intrinsics.error);
            }
        }
        checker.identity_unmapped_type_parameters = false;
        assert_eq!(
            checker.instantiate_type(original, &map, &parameters, &["T", "U"]),
            checker.intrinsics.error
        );
    }
}
