//! r6-declared: `declared.rs` instantiation keys.
//!
//! Each fixture's native answer was read from the pinned reference baseline
//! (`vendor/typescript-go/testdata/baselines/reference/submodule`) or the
//! native tsgo probe. See `docs/parity/notes/r6-declared.md`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the last statement's declaration.
fn type_of_last(source: &str) -> String {
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
    checker.set_strict_null_checks(true);
    let last = parsed.source_file.statements.last().copied().expect("a statement");
    let id = match last {
        Statement::VariableStatement(statement) => statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|declaration| declaration.node_id),
        _ => None,
    }
    .expect("a declaration");
    let symbol = bound.symbol_of(id).expect("bound");
    let ty = checker.get_type_of_symbol(symbol);
    checker.type_to_string(ty)
}

/// getObjectTypeInstantiation (checker.go:22304) keys a type literal's
/// instance on the outer type parameters isTypeParameterPossiblyReferenced
/// admits; with none, the instance is the written literal, whose member
/// keeps its annotation's order (`number | string`, not a re-minted
/// `string | number`).
#[test]
fn a_literal_naming_no_bound_parameter_is_the_written_literal() {
    assert_eq!(
        type_of_last(
            "type W<T> = { inner: { id?: number | string }; t: T };\n\
             declare let w: W<boolean>;\n\
             let x = w.inner;"
        ),
        "{ id?: number | string; }"
    );
}

/// A literal that names the bound parameter is still instantiated.
#[test]
fn a_literal_naming_the_bound_parameter_is_instantiated() {
    assert_eq!(
        type_of_last(
            "type W<T> = { inner: { id?: T | number }; t: T };\n\
             declare let w: W<string>;\n\
             let x = w.inner;"
        ),
        "{ id?: string | number; }"
    );
}

/// The resolved branch of a conditional alias is the written literal when it
/// names no bound parameter: getConditionalType instantiates the branch with
/// the conditional's mapper, and getObjectTypeInstantiation answers the
/// literal itself, whose member reuses its written annotation.
#[test]
fn a_resolved_branch_naming_no_parameter_is_the_written_literal() {
    assert_eq!(
        type_of_last(
            "type C<T> = T extends string ? { id?: number | string } : never;\n\
             declare let c: C<\"a\">;\n\
             let x = c;"
        ),
        "{ id?: number | string; }"
    );
}

/// getIndexedAccessType reads only the property the literal index names
/// (getPropertyTypeForIndexType -> getTypeOfSymbol), so a recursive arm the
/// index does not select is never resolved: `Count<3>` is `3` (native tsgo
/// probe), and an unresolvable sibling does not poison `["a"]`.
#[test]
fn an_indexed_type_literal_resolves_only_the_selected_member() {
    assert_eq!(
        type_of_last(
            "interface Array<T> { length: number; [n: number]: T }\n\
             type Count<N extends number, Acc extends unknown[] = []> =\n\
             { 0: Count<N, [...Acc, 0]>; 1: Acc[\"length\"] }[Acc[\"length\"] extends N ? 1 : 0];\n\
             declare let three: Count<3>;\n\
             let x = three;"
        ),
        "3"
    );
    assert_eq!(
        type_of_last(
            "type Pick1 = { a: string; b: Missing }[\"a\"];\n\
             declare let a: Pick1;\n\
             let x = a;"
        ),
        "string"
    );
}
