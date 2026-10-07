//! Native checkTypeArguments/getSignatureInstantiation defaults and rejection.
use tsr_ast::{Statement, TypeNode};
use tsr_checker::{Checker, calls::InstantiationExpressionSignature};

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
    assert!(Checker::signature_accepts_type_argument_count(&signature, 0));
    assert!(!Checker::signature_accepts_type_argument_count(&signature, 3));
    let empty_arguments =
        checker.check_signature_type_arguments(&signature, &[], true).unwrap().unwrap();
    let empty_image = checker.get_signature_instantiation(&signature, &empty_arguments).unwrap();
    let empty_return = checker.mapped_signature_return(&empty_image).unwrap();
    assert_eq!(checker.type_to_string(empty_return), "unknown");
    let empty_parameter = checker.parameter_type(&empty_image.parameters[0]);
    assert_eq!(checker.type_to_string(empty_parameter), "unknown");
    let arguments = checker
        .check_signature_type_arguments(&signature, &[alias.r#type.unwrap()], true)
        .unwrap()
        .unwrap();
    let image = checker.get_signature_instantiation(&signature, &arguments).unwrap();
    assert!(image.type_parameters.is_empty());
    assert!(image.target.as_ref().unwrap().type_parameters.is_empty());
    let returned = checker.mapped_signature_return(&image).unwrap();
    assert_eq!(checker.type_to_string(returned), "string");
    let parameter = checker.parameter_type(&image.parameters[0]);
    assert_eq!(checker.type_to_string(parameter), "string");
}

#[test]
fn constructor_signature_substitutes_instance_and_parameters() {
    let source = "interface Box<T> { value: T } declare const C: new <T>(value: T) => Box<T>; type Arg = number;";
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
    let ty = checker.get_type_of_symbol(bound.lookup_local(root, "C").unwrap());
    let tsr_checker::types::TypeData::Anonymous { symbol, .. } = checker.type_of(ty).data else {
        panic!("constructor type")
    };
    let signature = checker.get_signatures_of_symbol(symbol).unwrap().remove(0);
    let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[2] else { panic!() };
    let InstantiationExpressionSignature::Instantiated(image) = checker
        .get_instantiation_expression_signature(&signature, &[alias.r#type.unwrap()])
        .unwrap()
    else {
        panic!("constructor must instantiate")
    };
    let returned = checker.mapped_signature_return(&image).unwrap();
    assert_eq!(checker.type_to_string(returned), "Box<number>");
    let parameter = checker.parameter_type(&image.parameters[0]);
    assert_eq!(checker.type_to_string(parameter), "number");
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
        checker.check_signature_type_arguments(&signature, &[argument], false).unwrap().is_none()
    );
    assert!(checker.diagnostics().is_empty());
    assert!(matches!(
        checker.get_instantiation_expression_signature(&signature, &[argument]).unwrap(),
        InstantiationExpressionSignature::ConstraintRejected
    ));
    assert_eq!(signature.type_parameters.len(), 1);
    assert_eq!(checker.type_to_string(signature.r#type), "T");
    assert_eq!(
        checker.diagnostics().iter().map(|(_, diagnostic)| diagnostic.text()).collect::<Vec<_>>(),
        ["Type 'number' does not satisfy the constraint 'string'."]
    );
}
