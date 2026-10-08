//! A function declaration's type is its signature, printed. These are the
//! signature-construction shapes `docs/parity/notes/r5-funcdecl.md` ports:
//! each used to decline the whole signature, so the function's type answered
//! `error` where native prints the signature.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// `cloneBindingName` (`nodebuilderimpl.go:1713`) keeps holes and the
/// source list's trailing comma, and drops only initializers.
#[test]
fn an_array_pattern_keeps_its_holes_and_trailing_comma() {
    assert_eq!(
        type_of_function("function f([, a, , b, , , , s, , , ]: any) {}", "f"),
        "([, a, , b, , , , s, , ,]: any) => void"
    );
    assert_eq!(
        type_of_function("function f([, , b] = [1, 2, 3] as any) {}", "f"),
        "([, , b]?: any) => void"
    );
}

/// Literal and computed property names print as the clone writes them.
#[test]
fn an_object_pattern_prints_literal_and_computed_keys() {
    assert_eq!(
        type_of_function(
            "declare const k: any;\nfunction f({ [\"a\"]: x, 'b': y, 2: z, [k]: w, [k.m()]: v }: any) {}",
            "f"
        ),
        "({ [\"a\"]: x, 'b': y, 2: z, [k]: w, [k.m()]: v }: any) => void"
    );
}

/// `getTupleElementLabelFromBindingElement` (`relater.go:1959`) names a
/// pattern rest's expanded elements from the pattern, not from its text.
#[test]
fn a_pattern_rest_expands_to_element_labels() {
    assert_eq!(
        type_of_function("function d(...[a, , , d]: [boolean, string, number]) {}", "d"),
        "(a: boolean, arg_1: string, arg_2: number) => void"
    );
    assert_eq!(
        type_of_function("function c(...{0: a, length}: [boolean, string]) {}", "c"),
        "(arg_0: boolean, arg_1: string) => void"
    );
}

/// An unresolvable `typeof` annotation keeps the parameter, printed as
/// written (`compiler/arguments`).
#[test]
fn an_unresolved_type_query_annotation_prints_as_written() {
    assert_eq!(
        type_of_function("declare function h(args: typeof missing): void;", "h"),
        "(args: typeof missing) => void"
    );
}

/// The printed type of the function declaration named `name`.
fn type_of_function(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let function = parsed
        .source_file
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::FunctionDeclaration(function)
                if function.name.is_some_and(|identifier| identifier.text == name) =>
            {
                Some(function)
            }
            _ => None,
        })
        .expect("the named function");
    let symbol = bound.symbol_of(function.node_id.expect("registered")).expect("a symbol");
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}
