//! An optional parameter prints the type `serializeTypeForDeclaration`
//! (`nodebuilderimpl.go:2181`) serializes: the written annotation while it is
//! equivalent to the symbol's type, else the symbol's type, which carries the
//! `undefined` a `?` adds under strictNullChecks
//! (`getTypeForVariableLikeDeclaration`, `checker.go:16675`).
//! `docs/parity/notes/r5-printer2.md` §2.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the final expression statement, printed.
fn type_of_last_expression(source: &str, strict_null_checks: bool) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let options = tsr_core::CompilerOptions {
        strict_null_checks: if strict_null_checks {
            tsr_core::Tristate::True
        } else {
            tsr_core::Tristate::False
        },
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let last = parsed
        .source_file
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            Statement::ExpressionStatement(node) => node.expression,
            _ => None,
        })
        .expect("the fixture must end with an expression statement");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

const SORT: &str = "interface Box<T> { sort(compareFn?: (a: T, b: T) => number): Box<T>; }\n\
                    declare const box: Box<number>;\n\
                    box.sort;";

/// `arrayconcat`: the instantiated parameter no longer matches its
/// annotation, so the symbol's type prints, `undefined` included.
#[test]
fn an_instantiated_optional_parameter_prints_its_undefined() {
    assert_eq!(
        type_of_last_expression(SORT, true),
        "(compareFn?: ((a: number, b: number) => number) | undefined) => Box<number>"
    );
}

/// Without strictNullChecks `addOptionalityEx` adds nothing.
#[test]
fn without_strict_null_checks_nothing_is_added() {
    assert_eq!(
        type_of_last_expression(SORT, false),
        "(compareFn?: (a: number, b: number) => number) => Box<number>"
    );
}

/// The annotation itself is reused: `pseudoTypeEquivalentToType` forgives
/// the optionality (`nodebuilderimpl.go:2249`).
#[test]
fn a_written_optional_parameter_prints_as_written() {
    assert_eq!(
        type_of_last_expression("declare function f(x?: string): void;\nf;", true),
        "(x?: string) => void"
    );
}

/// A declaration's unannotated binding pattern takes its implied type with
/// no optionality (`checker.go:16791`). (An arrow's parameter is assigned by
/// `assignParameterType`, which adds it, `checker.go:10418`; its witness,
/// `parserParameterList11`'s `any[] | undefined`, needs lib's `Array`.)
#[test]
fn an_unannotated_declaration_pattern_takes_no_undefined() {
    assert_eq!(
        type_of_last_expression("function d([a, b]?) {}\nd;", true),
        "([a, b]?: [any, any]) => void"
    );
}
