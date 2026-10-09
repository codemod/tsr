//! A generic reference written outside its arity window answers upstream's
//! `errorType`, which composes (`getTypeFromClassOrInterfaceReference`,
//! `checker.go:23169`; `getTypeFromTypeAliasReference`, `checker.go:23596`).
//! `docs/parity/notes/r6-typesroots.md` §2.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The last variable statement's initializer: whether it is the native
/// `errorType`, the port's gap, or what it prints.
fn initializer(file: &str, source: &str) -> String {
    let arena = Arena::new();
    let parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(file));
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: file, text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let initializer = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initializer");
    let id = checker.check_expression(initializer);
    let intrinsics = checker.intrinsics();
    if id == intrinsics.native_error {
        "<native error>".to_owned()
    } else if id == intrinsics.error {
        "<gap>".to_owned()
    } else {
        checker.type_to_string(id)
    }
}

const DECLS: &str =
    "class C<T> { x: T }\ninterface I<T> { y: T }\ntype A<T, U = T> = { a: T; u: U };\n";

/// `var c1: C;` — too few arguments: `errorType`, not the gap.
#[test]
fn a_bare_generic_class_reference_is_native_error() {
    let source = format!("{DECLS}declare var c1: C;\nvar x = c1;");
    assert_eq!(initializer("t.ts", &source), "<native error>");
}

/// `var c2: C<I>;` — the argument's `errorType` composes into the reference.
#[test]
fn an_arity_error_argument_composes() {
    let source = format!("{DECLS}declare var c2: C<I>;\nvar x = c2;");
    assert_eq!(initializer("t.ts", &source), "C<any>");
}

/// Too many arguments on an alias: `errorType` too (`checker.go:23596`).
#[test]
fn too_many_alias_arguments_is_native_error() {
    let source = format!("{DECLS}declare var a: A<1, 2, 3>;\nvar x = a;");
    assert_eq!(initializer("t.ts", &source), "<native error>");
}

/// The control: a count inside the window (the defaulted tail omitted) is
/// not an arity error.
#[test]
fn a_defaulted_tail_is_not_an_arity_error() {
    let source = format!("{DECLS}declare var a: A<1>;\nvar x = a;");
    let printed = initializer("t.ts", &source);
    assert_ne!(printed, "<native error>");
    assert_ne!(printed, "<gap>");
}

/// The JS leg never answers `errorType` for a class reference
/// (`if !isJs { return c.errorType }`).
#[test]
fn a_javascript_class_reference_is_not_native_error() {
    let source = "/** @template T */\nclass C { }\n/** @type {C} */\nvar c;\nvar x = c;";
    assert_ne!(initializer("t.js", source), "<native error>");
}
