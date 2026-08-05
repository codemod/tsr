//! What an array type must get right.
//!
//! Every fixture here needs a **global `Array`**, because an array type is a
//! reference to it rather than a type that prints `T[]`. So these tests bind two
//! files into one result the way a program does — a stand-in lib and the fixture
//! — which is also the only way `BindResult::globals` is non-empty.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Array types are a reference to the global `Array`".

use tsr_ast::{NodeMap, NodeTable, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The minimal globals an array type needs. Two arities of one type parameter,
/// which is all `getGlobalType("Array", 1)` requires.
const LIB: &str = "interface Array<T> {}\ninterface ReadonlyArray<T> {}\n";

/// Bind `LIB` and `source` into one result, then answer a question about the
/// annotation on `source`'s `index`th statement.
fn type_of_annotation_at(source: &str, index: usize) -> String {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();

    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );

    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );

    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let Statement::VariableStatement(statement) = file.source_file.statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

fn type_of_annotation(source: &str) -> String {
    type_of_annotation_at(source, 0)
}

#[test]
fn an_array_type_prints_the_shorthand() {
    assert_eq!(type_of_annotation("var x: string[];"), "string[]");
    assert_eq!(type_of_annotation("var x: number[][];"), "number[][]");
}

#[test]
fn the_shorthand_is_a_property_of_the_target_not_of_how_it_was_written() {
    // `Array<Base>` records `Base[]` upstream (`generatedContextualTyping`), so
    // the `T[]` spelling belongs to `typeReferenceToTypeNode`'s special case on
    // `globalArrayType` and not to the array *node*.
    assert_eq!(type_of_annotation("var x: Array<string>;"), "string[]");
    assert_eq!(type_of_annotation("var x: ReadonlyArray<string>;"), "readonly string[]");
}

#[test]
fn an_array_type_is_the_same_type_as_the_generic_reference() {
    // The point of reusing the reference machinery rather than printing alike:
    // `string[]` and `Array<string>` must be one type, or the first relation
    // check written compares two handles that should have been equal.
    let source = "var a: string[];\nvar b: Array<string>;\nvar c: number[];";
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let mut ids = Vec::new();
    for statement in file.source_file.statements {
        let Statement::VariableStatement(statement) = statement else { continue };
        let annotation = statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|declaration| declaration.r#type)
            .expect("an annotation");
        ids.push(checker.get_type_from_type_node(annotation));
    }
    assert_eq!(ids[0], ids[1], "`string[]` and `Array<string>` must be one type");
    assert_ne!(ids[0], ids[2], "`string[]` and `number[]` must not be");
}

#[test]
fn readonly_is_a_different_global_not_a_modifier() {
    assert_eq!(type_of_annotation("var x: readonly string[];"), "readonly string[]");
    // And therefore a *different type* from the mutable one, which a modifier
    // on the same type could not be.
    let source = "var a: string[];\nvar b: readonly string[];";
    assert_eq!(type_of_annotation_at(source, 0), "string[]");
    assert_eq!(type_of_annotation_at(source, 1), "readonly string[]");
}

#[test]
fn an_element_is_parenthesised_only_where_the_baselines_parenthesise_it() {
    // Wrapped: a union, a `typeof`, a signature.
    assert_eq!(type_of_annotation("var x: (string | number)[];"), "(string | number)[]");
    // Not wrapped: an object type, or a nested array.
    assert_eq!(type_of_annotation("var x: { a: string }[];"), "{ a: string; }[]");
    assert_eq!(type_of_annotation("var x: string[][];"), "string[][]");
    // Redundant parentheses in the *source* do not survive: `(string)[]` is
    // `string[]`, because a parenthesised type node is transparent.
    assert_eq!(type_of_annotation("var x: (string)[];"), "string[]");
}

#[test]
fn an_element_this_port_cannot_type_makes_the_array_a_gap() {
    // `Unported[]` is not `any[]` — the same call made for union constituents
    // and for a generic reference's type arguments.
    assert_eq!(type_of_annotation("var x: Unresolvable[];"), "error");
}

#[test]
fn a_tuple_is_a_gap_rather_than_a_plausible_array() {
    // Upstream reaches tuples through the same function with `globalTupleType`
    // and a per-element flags model. Answering `string[]` for `[string, number]`
    // would be a wrong line dressed as a right one. `bd tsr-cqi`.
    assert_eq!(type_of_annotation("var x: [string, number];"), "error");
}

#[test]
fn a_global_array_of_the_wrong_arity_is_not_the_array_target() {
    // `getGlobalType("Array", 1, true)` checks the arity and reports when it is
    // wrong. Without the check a program declaring `interface Array {}` would
    // have its zero-arity `Array` used as the array target, and `string[]` would
    // print from a reference whose argument the target cannot hold.
    //
    // This existed as an unobservable guard until this test: no mutation of the
    // production code could reach it, because every other fixture supplies an
    // `Array` of the right arity.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = "interface Array {}\n";
    let source = "var x: string[];";
    let lib_file = tsr_parser::parse_into(&arena, lib, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        lib_file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: lib },
    );
    let bound = tsr_binder::bind_into(
        bound,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let Statement::VariableStatement(statement) = file.source_file.statements[0] else {
        panic!("variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    assert_eq!(id, checker.intrinsics().error);
}

#[test]
fn without_a_global_array_an_array_type_is_a_gap() {
    // A file checked with no lib is exactly the situation `getGlobalType`
    // reports on and this port gaps. Bound alone, so `globals` is empty.
    let source = "var x: string[];";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    assert_eq!(id, checker.intrinsics().error);
}
