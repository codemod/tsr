//! That a generic call is **reached**, which the unit tests cannot prove.
//!
//! `crate::inference`'s own tests call `check_generic_call` directly, so they
//! measure inference and say nothing about whether anything routes to it.
//! `Checker::check_call_expression` (`calls.rs`) returned `errorType` for every
//! generic signature until the guard there was pointed at the module, and an
//! arm that is never entered has tests that pass without ever running it. This
//! file goes through `check_expression`, which is the only path a `.types`
//! baseline ever takes.
//!
//! The oracle is
//! `vendor/typescript-go/testdata/baselines/reference/submodule/conformance/callGenericFunctionWithZeroTypeArguments.types:10`:
//!
//! ```text
//! var r = f(1);
//! >r : number
//! >f(1) : 1
//! >f : <T>(x: T) => T
//! ```
//!
//! Note which line is asserted. `>r : number` is the *variable*, widened at the
//! declaration by machinery that already existed; `>f(1) : 1` is the **call**,
//! and it is unwidened. A port that widens the inference candidate gets the
//! variable right and the call wrong, so asserting on the variable would hide
//! the error this file exists to catch.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement — the **call expression**, not
/// the variable it initialises.
fn type_of_last(source: &str) -> String {
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

    let index = parsed.source_file.statements.len() - 1;
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("the last statement must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_generic_call_reaches_inference_through_check_expression() {
    // The baseline line, verbatim: `>f(1) : 1`. Before the guard in
    // `check_call_expression` was pointed at `crate::inference` this printed
    // `error`, which is what makes this an end-to-end assertion rather than a
    // restatement of the unit tests.
    assert_eq!(type_of_last("function f<T>(x: T): T { return x; }\nconst a = f(1);"), "1");
    // Widening would print `number` here and would still be right about `a`.
    assert_ne!(type_of_last("function f<T>(x: T): T { return x; }\nconst a = f(1);"), "number");
}

#[test]
fn a_generic_call_this_slice_cannot_answer_is_still_a_gap_at_the_call_site() {
    // Digging `T` out of `T[]` is `inferTypes` (`inference.go:53`) and is not
    // ported, so the call is `errorType` — not the uninstantiated `T`, and not
    // the argument type. The guard has to keep gapping through the *same* path
    // it now answers through, which is the half of the wiring the positive test
    // above cannot check.
    assert_eq!(
        type_of_last("function f<T>(x: T[]): T { return x[0]; }\nconst a = f([1]);"),
        "error"
    );
}

#[test]
fn a_written_type_argument_is_used_instead_of_inference() {
    // `f<string>("s")` is `string`, NOT `"s"`. This is the fixture that
    // separates the two paths: inference from the argument would answer the
    // literal `"s"`, which is what the same call without type arguments gives
    // one line down. An implementation that ignores the written arguments and
    // infers anyway passes every other test in this file.
    assert_eq!(
        type_of_last("function f<T>(x: T): T { return x; }\nconst a = f<string>(\"s\");"),
        "string"
    );
    assert_eq!(type_of_last("function f<T>(x: T): T { return x; }\nconst a = f(\"s\");"), "\"s\"");
}

#[test]
fn type_arguments_that_do_not_check_are_a_gap() {
    // Wrong arity: `checkTypeArguments` (`checker.go:9269`) fails the call, so
    // answering the one written argument would be a wrong answer.
    assert_eq!(
        type_of_last("function f<T>(x: T): T { return x; }\nconst a = f<string, number>(\"s\");"),
        "error"
    );
    // Type arguments on a signature that takes none — `checkNoTypeArguments`.
    // FLIPPED at §375: the old line asserted `error` on the derivation that
    // answering would "quietly drop what the caller wrote" — but upstream
    // REPORTS TS2558 and still resolves the error call through the
    // candidate; the type is the return. The corpus witness is
    // `typeAssertions`' `fn2<string>(4) : void`. What the old pin got
    // right: the construct IS an error — that half lives in the
    // diagnostics lane, not in this type.
    assert_eq!(
        type_of_last("function g(x: number): number { return x; }\nconst a = g<string>(1);"),
        "number"
    );
    // The stand-in came due (the twenty-seventh): §31 made a truly
    // unresolved NAME answer upstream's TS2304 `errorType`, printed `any` —
    // so `typeof missing` is `any` and the instantiated call answers it,
    // exactly as upstream's baseline would. An unresolved TYPE reference
    // (`f<Unported>()`) still gaps; only value-name resolution changed.
    assert_eq!(
        type_of_last(
            "function f<T>(x: T): T { return x; }\nconst a = f<typeof missing>(1 as any);"
        ),
        "any"
    );
}
