//! What `yield` must get right.
//!
//! `yield` is the one form in `expressions.rs` that answers `anyType`, so these
//! tests exist mostly to pin *which* paths are entitled to it. 430 of roughly
//! 540 `>yield` lines in the baselines under
//! `vendor/typescript-go/testdata/baselines/reference/submodule` are `any`, and
//! the two unconditional paths are the ones ported here — see
//! `check_yield_expression` for why that is a computed answer rather than a gap
//! wearing `any`.

use tsr_ast::{ConciseBody, Expression, FunctionBody, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Find the one `yield` expression in a fixture.
///
/// A structural walk rather than a scan of the node table, because the fixtures
/// deliberately nest the yield inside function bodies of several kinds and the
/// nesting is part of what is being tested.
fn find_yield<'a>(statements: &[Statement<'a>]) -> Option<&'a tsr_ast::YieldExpression<'a>> {
    // Nested rather than a closure so it can recurse into `find_yield`.
    fn in_body(body: Option<FunctionBody<'_>>) -> Option<&tsr_ast::YieldExpression<'_>> {
        match body {
            Some(FunctionBody::Block(block)) => find_yield(block.statements),
            None => None,
        }
    }
    for statement in statements {
        let found = match statement {
            Statement::ExpressionStatement(s) => match s.expression {
                Some(Expression::YieldExpression(y)) => Some(y),
                _ => None,
            },
            Statement::FunctionDeclaration(f) => in_body(f.body),
            Statement::VariableStatement(s) => s
                .declaration_list
                .and_then(|list| list.declarations.first().copied())
                .and_then(|declaration| declaration.initializer)
                .and_then(|initialiser| match initialiser {
                    Expression::FunctionExpression(f) => in_body(f.body),
                    Expression::ArrowFunction(f) => match f.body {
                        Some(ConciseBody::Block(block)) => find_yield(block.statements),
                        _ => None,
                    },
                    _ => None,
                }),
            _ => None,
        };
        if found.is_some() {
            return found;
        }
    }
    None
}

/// Type the one `yield` expression in the fixture.
fn type_of_first_yield(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);

    let found =
        find_yield(parsed.source_file.statements).expect("the fixture must contain a yield");
    let id = checker.check_expression(Expression::YieldExpression(found));
    checker.type_to_string(id)
}

#[test]
fn yield_in_a_generator_declaration_without_an_annotation_is_any() {
    // The common corpus shape, and upstream's own answer: with no return
    // annotation the function ends at `getContextualIterationType ?? anyType`,
    // and a function declaration cannot be contextually typed.
    assert_eq!(type_of_first_yield("function* g() { yield 1; }"), "any");
}

#[test]
fn yield_in_a_non_generator_is_any_whatever_the_container() {
    // `checker.go:10967` returns `anyType` before it reads the return
    // annotation or any contextual type, so this holds even for containers this
    // port otherwise refuses to reason about, and even with an annotation
    // present. That ordering is the whole reason these are safe.
    assert_eq!(type_of_first_yield("function g() { yield 1; }"), "any");
    assert_eq!(type_of_first_yield("function g(): string { yield 1; }"), "any");
    assert_eq!(type_of_first_yield("var f = function () { yield 1; };"), "any");
}

#[test]
fn yield_outside_any_function_is_any() {
    // `fn == nil` (`checker.go:10963`).
    assert_eq!(type_of_first_yield("yield 1;"), "any");
}

#[test]
fn a_generator_with_a_return_annotation_is_a_gap_and_notably_not_any() {
    // This is the case that makes the `any` above a real decision rather than a
    // blanket. Upstream computes the NEXT type of the annotation, and
    // `Generator<number>`'s next type is `unknown`, not `any` — so answering
    // `any` here would be a wrong line, not a conservative one.
    assert_eq!(
        type_of_first_yield(
            "interface Generator<T> {}\nfunction* g(): Generator<number> { yield 1; }"
        ),
        "error"
    );
}

#[test]
fn yield_star_is_a_gap() {
    // Needs `getIterationTypeOfIterable`.
    assert_eq!(type_of_first_yield("function* g() { yield* [1]; }"), "error");
}

#[test]
fn a_generator_expression_is_a_gap_because_it_could_be_contextually_typed() {
    // The fence is on the container's kind: a function expression can carry a
    // contextual type, which would make `getContextualIterationType` return
    // something other than the `anyType` fallback. A declaration cannot.
    assert_eq!(type_of_first_yield("var f = function* () { yield 1; };"), "error");
}

#[test]
fn a_yield_inside_an_arrow_inside_a_generator_is_not_the_generators_yield() {
    // `GetContainingFunction` does not treat an arrow as transparent, unlike the
    // `this` walk. The nearest containing function is the arrow, which is not a
    // generator, so this is the non-generator arm and answers `any`.
    //
    // The enclosing generator carries a return ANNOTATION deliberately. Without
    // it both readings answer `any` and this test pins nothing — which is what
    // it did until a mutation that removed `ArrowFunction` from the walk left it
    // green. With the annotation, treating the arrow as transparent would reach
    // the generator and gap, so the two readings are now distinguishable.
    assert_eq!(
        type_of_first_yield(
            "interface Generator<T> {}\nfunction* g(): Generator<number> { var h = () => { yield 1; }; }"
        ),
        "any"
    );
}
