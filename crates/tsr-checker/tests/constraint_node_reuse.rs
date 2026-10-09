//! A signature's type-parameter constraint reuses its written node.
//!
//! `typeParameterToDeclaration` (`nodebuilderimpl.go:1611`) prints the
//! constraint through `typeToTypeNodeHelperWithPossibleReusableTypeNode`
//! (`:1597`): when `getTypeFromTypeNode` of the first written `extends` node is
//! the constraint, the node is re-emitted, so its parentheses and its union
//! order survive. A mapped node is one type per node
//! (`getTypeFromMappedTypeNode`'s `typeNodeLinks`), which is what lets the
//! identity hold for `{ [P in K]: X }`. `docs/parity/notes/r5-mapped4.md` §2.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the final expression statement, printed.
fn type_of_last_expression(source: &str) -> String {
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
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
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

#[test]
fn a_written_constraint_keeps_its_parentheses_and_union_order() {
    // `typeParameterConstraints1`: `<T extends (1)>`.
    assert_eq!(
        type_of_last_expression("declare function f<T extends (1)>(test: T): void;\nf;"),
        "<T extends (1)>(test: T) => void"
    );
    // `cannotIndexGenericWritingError`: the written `number | string`, not the
    // sorted `string | number`.
    assert_eq!(
        type_of_last_expression(
            "declare function f<T extends { a: 1 } & { [s: string]: number | string }>(t: T): void;\nf;"
        ),
        "<T extends { a: 1; } & { [s: string]: number | string; }>(t: T) => void"
    );
}

#[test]
fn a_mapped_constraint_is_one_type_per_node() {
    // `mappedTypeContextualTypesApplied`: a non-generic mapped constraint
    // prints its written node, not its resolved members. (Pins the reuse for
    // r5-mapped3's declared-route diff, under which the node resolves to its
    // members; without it the written mint already prints this text.)
    assert_eq!(
        type_of_last_expression(
            "type TakeString = (s: string) => any;\n\
             declare function f<T extends { [P in string]: TakeString }>(obj: T): void;\nf;"
        ),
        "<T extends { [P in string]: TakeString; }>(obj: T) => void"
    );
}
