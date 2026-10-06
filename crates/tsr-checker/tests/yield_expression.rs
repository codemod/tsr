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
        &arena,
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
fn an_annotation_that_is_no_iterator_answers_any() {
    // `getIterationTypeOfGeneratorFunctionReturnType(Next, ..)` orElse
    // `anyType` (`checker.go:11002`). This fixture's `Generator<T>` has ONE
    // type parameter, so it is not the global the fast path reads at arity
    // three; it declares no `[Symbol.iterator]` and no `next`, so both the
    // iterable and the iterator queries complete with no types, and the yield
    // is `any`. §225's syntactic read answered `error` here, a gap where
    // upstream's answer is computable.
    assert_eq!(
        type_of_first_yield(
            "interface Generator<T> {}\nfunction* g(): Generator<number> { yield 1; }"
        ),
        "any"
    );
}

/// An annotated generator's yield type is the annotation's NEXT iteration
/// type (`getIterationTypesOfGeneratorFunctionReturnType`): a reference to one
/// of the iteration globals reads its resolved third type argument, default
/// included. `conformance/generatorTypeCheck13` and
/// `generatorReturnTypeFallback.5`.
#[test]
fn an_annotated_generator_reads_its_annotations_next_type() {
    // The DEFAULT road: the real `IterableIterator<T, TReturn = any, TNext =
    // any>` (`es2015.iterable.d.ts:37`) with only two arguments written.
    assert_eq!(
        type_of_first_yield(
            "interface IterableIterator<T, TReturn = any, TNext = any> {}\n\
             function* g(): IterableIterator<number, string> { yield 0; }"
        ),
        "any"
    );
    // The WRITTEN argument, which must win over the default.
    assert_eq!(
        type_of_first_yield(
            "interface Generator<T, R = any, N = any> {}\n\
             function* g(): Generator<number, void, string> { yield 0; }"
        ),
        "string"
    );
    // A user interface with the same shape is NOT read by slot: it is neither
    // an iteration global nor structurally an iterable or iterator, so the
    // query completes empty and the yield is `any` (§225's slot read answered
    // `string`, which upstream never does).
    assert_eq!(
        type_of_first_yield(
            "interface G<T, R = any, N = any> {}\n\
             function* g(): G<number, void, string> { yield 0; }"
        ),
        "any"
    );
}

/// The control that makes §225 a *reading* rather than an assumption, and the
/// one the superseded pin's comment was really about.
///
/// The two globals disagree in the third slot —
/// `Generator<T = unknown, TReturn = any, TNext = unknown>` against
/// `IterableIterator<T, TReturn = any, TNext = any>` — so a rule that answered
/// `any` for every annotated generator passes the first assertion above and
/// fails here. Nothing else in this file discriminates the two.
#[test]
fn the_next_slot_is_read_and_not_assumed() {
    assert_eq!(
        type_of_first_yield(
            "interface Generator<T = unknown, TReturn = any, TNext = unknown> {}\n\
             function* g(): Generator<number> { yield 1; }"
        ),
        "unknown"
    );
    // And a third parameter with NO default is a slot this port cannot fill,
    // which is a gap rather than `any`.
    assert_eq!(
        type_of_first_yield("interface G<T, R, N> {}\nfunction* g(): G<number> { yield 1; }"),
        "error"
    );
}

#[test]
fn yield_star_is_a_gap() {
    // Needs `getIterationTypeOfIterable`.
    assert_eq!(type_of_first_yield("function* g() { yield* [1]; }"), "error");
}

#[test]
fn a_generator_expression_in_an_unannotated_variable_is_not_contextually_typed() {
    // **A refusal whose stated reason names a capability the fixture does not
    // use.** This asserted `error` on the ground that "a function expression
    // *can* carry a contextual type, which would make
    // `getContextualIterationType` return something other than the `anyType`
    // fallback. A declaration cannot."
    //
    // True about the kind, and *can* is not *does*. §224: this variable has no
    // annotation, and `getContextualTypeForInitializerExpression`
    // (`checker.go:29356`'s arm) reads the declaration's type node — so there
    // is provably no contextual type here and upstream takes the fallback.
    // `compiler/generatorES6_3` is this fixture almost verbatim and wants
    // `any`.
    //
    // The fence stays on the kind for every other position; only the one
    // provably-uncontextualised shape is carved out. The annotated control
    // below is what keeps that honest.
    assert_eq!(type_of_first_yield("var f = function* () { yield 1; };"), "any");
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

/// The controls, and the first is the whole point of the arm being narrow.
#[test]
fn the_container_widening_stops_at_an_annotation() {
    // ANNOTATED: the declaration's type node IS the contextual type, and
    // the answer comes from it, not from the uncontextualised shortcut. An
    // `any` context has no call signature, so `getContextualReturnType` is
    // nil and upstream falls back to `anyType` (`checker.go:11005`); this
    // used to decline (`error`) before that function was ported. A
    // `Generator` context answers its NEXT slot, which is what proves the
    // annotation is read at all.
    assert_eq!(type_of_first_yield("var v: any = function* () { yield 0; };"), "any");
    assert_eq!(
        type_of_first_yield(
            "interface Generator<T, TReturn, TNext> {}\nvar v: () => Generator<number, void, string> = function* () { yield 0; };"
        ),
        "string"
    );
    // An arrow cannot be a generator, so the widening must not reach it — it
    // keeps taking the not-a-generator arm.
    assert_eq!(type_of_first_yield("var v = () => { yield 1; };"), "any");
    // And the pre-existing declaration case is untouched.
    assert_eq!(type_of_first_yield("function* g() { yield 1; }"), "any");
}
