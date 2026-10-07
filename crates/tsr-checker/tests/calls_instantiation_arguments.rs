//! Native checkTypeArguments/getSignatureInstantiation defaults and rejection.
use tsr_ast::{Statement, TypeNode};
use tsr_checker::Checker;

#[test]
fn written_arguments_fill_defaults_and_substitute_signature() {
    let source = "declare function f<T, U = T>(value: U): T; type Arg = string;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "args.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let symbol = bound.lookup_local(root, "f").unwrap();
    let signature = checker.get_signatures_of_symbol(symbol).unwrap().remove(0);
    let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[1] else { panic!() };
    assert!(Checker::signature_accepts_type_argument_count(&signature, 1));
    assert!(!Checker::signature_accepts_type_argument_count(&signature, 0));
    assert!(!Checker::signature_accepts_type_argument_count(&signature, 3));
    let image = checker
        .instantiate_signature_with_type_arguments(&signature, &[alias.r#type.unwrap()])
        .unwrap()
        .unwrap();
    assert_eq!(checker.type_to_string(image.r#type), "string");
    let parameter = checker.parameter_type(&image.parameters[0]);
    assert_eq!(checker.type_to_string(parameter), "string");
}

#[test]
fn rejected_constraint_is_reported_without_an_instantiated_image() {
    let source = "declare function f<T extends string>(value: T): T; type Arg = number;";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "args.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let signature =
        checker.get_signatures_of_symbol(bound.lookup_local(root, "f").unwrap()).unwrap().remove(0);
    let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[1] else { panic!() };
    let argument: TypeNode<'_> = alias.r#type.unwrap();
    assert!(
        checker
            .instantiate_signature_with_type_arguments(&signature, &[argument])
            .unwrap()
            .is_none()
    );
    assert_eq!(
        checker.diagnostics().iter().map(|(_, diagnostic)| diagnostic.text()).collect::<Vec<_>>(),
        ["Type 'number' does not satisfy the constraint 'string'."]
    );
}
