//! What an array literal **expression** must get right.
//!
//! These need a global `Array` for the same reason the array *type* tests do —
//! the result is a reference to it — so they bind a stand-in lib alongside the
//! fixture.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Array literals: two pieces of machinery meeting".

use tsr_ast::{NodeMap, NodeTable, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

const LIB: &str = "interface Array<T> {}\ninterface ReadonlyArray<T> {}\n";

/// The printed type of the first statement's first declaration's initialiser.
fn type_of_initialiser(source: &str) -> String {
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
    let Statement::VariableStatement(statement) = file.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn an_empty_array_literal_is_never_and_not_a_special_case_of_nothing() {
    // `implicitNeverType` under `strictNullChecks` (`checker.go:8098`), which
    // the harness defaults on; the corpus splits 461 `never[]` to 297
    // `undefined[]` on exactly that option. The cheapest test that separates a
    // real implementation from one that only handles the non-empty path.
    assert_eq!(type_of_initialiser("const a = [];"), "never[]");
}

#[test]
fn an_empty_array_literal_is_undefined_when_strict_null_checks_is_off() {
    // The other branch of `checker.go:8098` — `undefinedWideningType`. From
    // `typedArrays.types:177` (a non-strict case): `>[] : undefined[]`. The
    // 212-line `undefined[] -> never[]` W2 row, ninth session.
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();
    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let source = "const a = [];";
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
    checker.set_strict_null_checks(false);
    let Statement::VariableStatement(statement) = file.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(id), "undefined[]");
}

#[test]
fn the_element_type_is_the_union_of_the_elements() {
    // Written so the union's own ordering rule is exercised: `STRING` is
    // `1 << 5` and `NUMBER` is `1 << 6`, so a number-first source still prints
    // string-first. Upstream records `>[1, "a"] : (string | number)[]`.
    assert_eq!(type_of_initialiser(r#"const a = [1, "a"];"#), "(string | number)[]");
    assert_eq!(type_of_initialiser(r#"const a = ["a", 1];"#), "(string | number)[]");
    // One constituent after deduplication needs no parentheses.
    assert_eq!(type_of_initialiser("const a = [1, 2, 3];"), "number[]");
}

#[test]
fn elements_widen_where_a_bare_const_does_not() {
    // Freshness stops at the element boundary, exactly as it stops at the
    // property boundary: `const n = 1` is `1`, `const a = [1]` is `number[]`.
    assert_eq!(type_of_initialiser("const a = [1];"), "number[]");
    assert_eq!(type_of_initialiser(r#"const a = ["x"];"#), "string[]");
    assert_eq!(type_of_initialiser("let a = [1];"), "number[]");
}

#[test]
fn a_nested_array_literal_is_typed_by_the_same_rule() {
    assert_eq!(type_of_initialiser("const a = [[1]];"), "number[][]");
}

#[test]
fn an_array_literal_is_the_same_type_as_the_annotation_would_give() {
    // The payoff of the result being a real `Array` reference rather than a
    // lookalike: the inferred type and the written one are one type.
    let source = "const a = [1];\nvar b: number[];";
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
    let Statement::VariableStatement(first) = file.source_file.statements[0] else {
        panic!("variable statement")
    };
    let Statement::VariableStatement(second) = file.source_file.statements[1] else {
        panic!("variable statement")
    };
    let inferred = checker.check_expression(
        first
            .declaration_list
            .and_then(|l| l.declarations.first().copied())
            .and_then(|d| d.initializer)
            .expect("an initialiser"),
    );
    let written = checker.get_type_from_type_node(
        second
            .declaration_list
            .and_then(|l| l.declarations.first().copied())
            .and_then(|d| d.r#type)
            .expect("an annotation"),
    );
    assert_eq!(inferred, written, "`[1]` and `number[]` must be one type");
}

#[test]
fn an_element_this_port_cannot_type_makes_the_literal_a_gap() {
    assert_eq!(type_of_initialiser("const a = [unknownThing];"), "error");
    assert_eq!(type_of_initialiser("const a = [1, unknownThing];"), "error");
}

#[test]
fn a_spread_or_an_omitted_element_makes_the_literal_a_gap() {
    // The spread half came due (the twenty-sixth stand-in): the §6 arm
    // (`checker-notes-arrays.md`) spreads an `Array<T>` operand as `T`, so
    // `[...[1]]` is `number[]` — upstream's own answer. An omission still
    // needs the tuple element flags and stays a gap.
    assert_eq!(type_of_initialiser("const a = [...[1]];"), "number[]");
    assert_eq!(type_of_initialiser("const a = [1, , 2];"), "error");
}

#[test]
fn two_object_typed_elements_are_a_gap_because_subtype_reduction_is_missing() {
    // The eighteenth unported-stand-in fixture to come due: this asserted
    // `error` for the exact mechanism the ninth session built. In the one
    // provably-uncontextual position — an un-annotated variable initialiser —
    // the §9 decidability-gated reduction now runs upstream's
    // `UnionReductionSubtype` call (`checker.go:8096`,
    // `checker-notes-assign.md` §13), and two mutual-subtype object types
    // collapse to one exactly as upstream's do.
    assert_eq!(type_of_initialiser("const a = [{ a: 1 }, { a: 1 }];"), "{ a: number; }[]");
    // One object-typed element is unaffected — subtype reduction would not have
    // merged these either, so the guard is specific rather than a blanket
    // refusal of object elements.
    assert_eq!(type_of_initialiser("const a = [{ a: 1 }];"), "{ a: number; }[]");
    // Order predicted wrong, implementation right, again: `NUMBER` is `1 << 6`
    // and `OBJECT` is `1 << 20`, so the object type sorts *second*.
    assert_eq!(type_of_initialiser("const a = [{ a: 1 }, 1];"), "(number | { a: number; })[]");
}
