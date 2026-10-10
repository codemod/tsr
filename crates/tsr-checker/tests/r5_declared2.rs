//! r5-declared2: `declared.rs` tuple-alias and keyof producers.
//!
//! Each fixture's native answer was read from the pinned reference baseline
//! (`vendor/typescript-go/testdata/baselines/reference/submodule`, the case
//! named beside it). See `docs/parity/notes/r5-declared2.md`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the last statement's declaration.
fn type_of_last(source: &str) -> String {
    // A unit test has no lib files; the array targets a tuple normalizes to
    // are declared in the fixture's own global scope.
    let source = &format!(
        "interface Array<T> {{ length: number; [n: number]: T }}\n\
         interface ReadonlyArray<T> {{ readonly length: number; readonly [n: number]: T }}\n{source}"
    );
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

/// getTypeFromArrayOrTupleTypeNode (checker.go:24121): a tuple with a
/// variadic element is normalized and takes no alias; one whose rests are all
/// over array type nodes is a deferred reference that keeps it
/// (`namedTupleMembersErrors`, `restTupleElements1`).
#[test]
fn only_a_variadic_tuple_body_drops_its_alias() {
    assert_eq!(
        type_of_last("type List = [item: any, ...any];\ndeclare let x: List;"),
        "[item: any, ...any[]]"
    );
    assert_eq!(type_of_last("type T06 = [string, ...string[]];\ndeclare let x: T06;"), "T06");
}

/// TupleNormalizer.normalize (checker.go:23374): a variadic primitive operand
/// becomes a rest of errorType (`restTupleElements1`: `[...string]` is
/// `any[]`; `namedTupleMembersErrors`: `...rest: number` is `...rest: any[]`).
#[test]
fn a_variadic_primitive_is_an_error_rest() {
    assert_eq!(type_of_last("declare let x: [...string];"), "any[]");
}

/// An alias declared as a reference to a normalized-tuple alias is the
/// normalized instantiation, without the new alias (`variadicTuples2` V30).
#[test]
fn an_alias_of_a_normalized_tuple_alias_prints_the_structure() {
    let source = "type Tup3<T extends unknown[], U extends unknown[], V extends unknown[]> = [...T, ...U, ...V];\n\
                  type V30<A extends unknown[]> = Tup3<A, string[], number[]>;\n\
                  declare let x: V30<[boolean]>;";
    assert_eq!(type_of_last(source), "[boolean, ...(string | number)[]]");
}

/// A variadic body whose rest is a deferred conditional instantiates through
/// the conditional (`partiallyNamedTuples` `AddMixedConditional`).
#[test]
fn a_conditional_rest_instantiates_with_its_alias_arguments() {
    let source = "type S<T> = [T, ...(T extends 0 ? [x: \"c\"] : [])];\n\
                  declare let x: S<0>;";
    assert_eq!(type_of_last(source), "[0, x: \"c\"]");
}
