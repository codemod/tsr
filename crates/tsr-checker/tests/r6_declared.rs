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
