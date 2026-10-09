//! A type literal's property prints its written annotation when
//! `serializeTypeForDeclaration`'s reuse arm holds (`nodebuilderimpl.go:2231`,
//! from `addPropertyToElementList`): the annotation's type is the property's,
//! optionality forgiven. The re-emitted node is the existing-node visitor's,
//! so an unannotated parameter of a written function type prints `: any`
//! (`nodecopy.go:660`). `docs/parity/notes/r5-printer2.md` §3.

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

/// `emitRestParametersFunctionPropertyES6`: the written function type, not
/// the signature's `any[]` rest type.
#[test]
fn a_written_function_type_keeps_its_untyped_rest_parameter() {
    assert_eq!(
        type_of_last_expression(
            "declare var obj: { func1: (...rest) => void; n: number; };\nobj;",
            false
        ),
        "{ func1: (...rest: any) => void; n: number; }"
    );
}

/// An optional property's annotation is reused although the property's type
/// carries the added `undefined`.
#[test]
fn an_optional_property_reuses_its_annotation() {
    assert_eq!(
        type_of_last_expression("declare var obj: { f?: (a) => void; n: number; };\nobj;", true),
        "{ f?: (a: any) => void; n: number; }"
    );
}
