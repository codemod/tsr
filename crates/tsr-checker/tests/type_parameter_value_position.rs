//! §475 — an identifier that resolves ONLY to a TYPE PARAMETER, read in
//! value position: upstream's TS2693 ("'T' only refers to a type") and
//! `errorType`, printed `any`. This is deterministic, not a port resolution
//! gap — a type parameter can never carry a value meaning in an unloaded
//! file, which is what separates it from the §31 gate's honest `error`.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the extends-clause entry's expression.
fn type_of_heritage_expression(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut stack = vec![tsr_ast::Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    while let Some(node) = stack.pop() {
        if let tsr_ast::Node::ExpressionWithTypeArguments(entry) = node
            && let Some(expression) = entry.expression
        {
            let ty = checker.check_expression(expression);
            return checker.type_to_string(ty);
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    panic!("the fixture must contain a heritage entry");
}

/// `typeParameterAsBaseClass` records `>T : any` for `class C<T> extends T`.
/// Deleting the §475 type-parameter arm in the identifier road's unresolved
/// exit reddens this to `error`.
#[test]
fn a_type_parameter_read_as_a_value_is_the_deliberate_error_any() {
    assert_eq!(type_of_heritage_expression("class C<T> extends T {}"), "any");
}

/// The control: an ordinary class base still answers through the value road,
/// so the arm is keyed to the SYMBOL kind, not to heritage position.
#[test]
fn an_ordinary_base_class_still_answers() {
    assert_eq!(type_of_heritage_expression("class A {}\nclass B extends A {}"), "typeof A");
}
