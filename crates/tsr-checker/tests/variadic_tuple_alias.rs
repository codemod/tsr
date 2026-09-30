//! Generic tuple aliases normalize their resolved arguments after substitution.
//! These fixtures cover concrete, optional, array, union, and never spreads,
//! following `createNormalizedTupleTypeEx` in typescript-go.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the first variable declaration's annotation.
fn type_of_annotation(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        let Some(declaration) =
            node.declaration_list.and_then(|list| list.declarations.first().copied())
        else {
            continue;
        };
        let annotation = declaration.r#type.expect("an annotation");
        let id = checker.get_type_from_type_node(annotation);
        return checker.type_to_string(id);
    }
    panic!("the fixture must declare a variable");
}

/// `variadicTuples1`'s own aliases.
const ALIASES: &str = "interface Array<T> { length: number }\n\
                       type TV0<T extends unknown[]> = [string, ...T];\n\
                       type TV1<T extends unknown[]> = [string, ...T, number];\n";

/// The head case, from `variadicTuples1`.
#[test]
fn a_leading_rest_splices() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV0<[boolean]>;")),
        "[string, boolean]"
    );
}

/// Several elements splice in order.
#[test]
fn a_multi_element_argument_splices_in_order() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV0<[boolean, number]>;")),
        "[string, boolean, number]"
    );
}

/// A rest in the MIDDLE keeps the elements on both sides — this is the shape
/// element-wise substitution could never produce.
#[test]
fn a_middle_rest_keeps_both_sides() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<[boolean, string]>;")),
        "[string, boolean, string, number]"
    );
}

/// The EMPTY tuple argument collapses the rest away entirely — `TN2` in the
/// fixture.
#[test]
fn an_empty_tuple_argument_collapses_the_rest() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<[]>;")),
        "[string, number]"
    );
}

/// Array arguments normalize the variadic element into an unbounded rest.
#[test]
fn an_array_argument_normalizes_to_a_rest() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV0<string[]>;")),
        "[string, ...string[]]"
    );
}

#[test]
fn a_union_argument_distributes_the_alias_body() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<[boolean] | [number]>;")),
        "[string, number, number] | [string, boolean, number]"
    );
}

#[test]
fn a_never_argument_annihilates_the_alias_body() {
    assert_eq!(type_of_annotation(&format!("{ALIASES}declare let a: TV1<never>;")), "never");
}

#[test]
fn an_optional_spread_before_a_required_suffix_keeps_undefined() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<[boolean?]>;")),
        "[string, boolean | undefined, number]"
    );
}

#[test]
fn an_any_argument_becomes_an_any_array_rest() {
    assert_eq!(
        type_of_annotation(&format!("{ALIASES}declare let a: TV1<any>;")),
        "[string, ...any[], number]"
    );
}
