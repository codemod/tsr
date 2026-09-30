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
    // ported, so the call cannot answer `number`. The guard has to keep gapping
    // through the *same* path it now answers through, which is the half of the
    // wiring the positive test above cannot check.
    //
    // **`unknown` since §929, and that is this harness rather than the
    // checker.** This file binds one LIB-FREE source, so `T[]` cannot resolve
    // `Array` and is itself a gap; §929's road therefore fires on the parameter,
    // keeping the written `T[]` with an `any` type, and the call lands on
    // `unknown` where it used to land on `error`. With a lib mounted — which is
    // what the corpus scores through — `T[]` resolves and §929 never fires here;
    // the corpus measured **zero `RIGHT->WRONG`** across the whole change.
    //
    // Either way the assertion holds what the test is for: **the call does not
    // answer**, and it does not answer the argument type either.
    assert_eq!(
        type_of_last("function f<T>(x: T[]): T { return x[0]; }\nconst a = f([1]);"),
        "unknown"
    );
    assert_ne!(
        type_of_last("function f<T>(x: T[]): T { return x[0]; }\nconst a = f([1]);"),
        "number"
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
    // SURPLUS arity FLIPPED at §690, on the same derivation §375 corrected for
    // the assertion below — and for the same reason. The old line asserted
    // `error` because *"`checkTypeArguments` fails the call, so answering the
    // one written argument would be a wrong answer"*. Upstream reports the
    // arity error and **still instantiates**: `getSignatureInstantiation`
    // proceeds past the diagnostic, dropping the surplus. The corpus witness is
    // `callGenericFunctionWithIncorrectNumberOfTypeArguments`, whose
    // `f<number, string, number>(1, '')` on `f<T, U>` records `number` — three
    // arguments, two parameters, resolved. Measured at 9 WRONG→RIGHT, zero
    // adverse.
    //
    // §690 deliberately covers the SURPLUS half only. The MISSING half is left
    // to ordinary inference, which already answers it — `f<number>(1, 2)` on
    // `f<T, U>` infers `T = number` from the argument and records `number`,
    // upstream's answer too. Routing it through the written-argument path
    // instead would need `fillMissingTypeArguments`' defaults, which this
    // port's `TypeParameter` does not carry; filling with `any` measured **62
    // RIGHT→WRONG in `genericDefaults`**. Pinned so that the next attempt sees
    // the missing half is not broken and needs no arm.
    assert_eq!(
        type_of_last("function f<T>(x: T): T { return x; }\nconst a = f<string, number>(\"s\");"),
        "string"
    );
    assert_eq!(
        type_of_last("function f<T, U>(x: T, y: U): T { return x; }\nconst a = f<number>(1, 2);"),
        "number"
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

/// §951: the NON-ARRAY rest parameter — `getNonArrayRestType`
/// (`relater.go:1858`) names the shape this port used to decline whole, and
/// `inferTypeArguments` (`checker.go:9489`) infers a TUPLE of the arguments at
/// the rest position against the rest parameter's own type.
#[test]
fn a_rest_parameter_typed_by_a_type_parameter_infers_a_tuple() {
    // The primary leg. Widened, not literal: `getSpreadArgumentType` applies
    // `getWidenedLiteralType` per element (`checker.go:29500`), so `["a", 1]`
    // would be a wrong answer that prints plausibly.
    assert_eq!(
        type_of_last(
            "declare function f<T extends unknown[]>(...args: T): T;\nconst a = f(\"a\", 1);"
        ),
        "[string, number]"
    );
    // The falsifier leg: no arguments is the EMPTY tuple, which is a distinct
    // print from `never[]` and from a gap. This is also the corpus's
    // `[]`-versus-`never[]` row.
    assert_eq!(
        type_of_last("declare function h<T extends unknown[]>(...args: T): T;\nconst a = h();"),
        "[]"
    );
    // The regression leg — §939's ARRAY-shaped rest keeping its own road — is
    // **not expressible in this harness and is asserted by the corpus instead**
    // (`scorepair` reports zero `RIGHT->WRONG` for §951). This fixture loads no
    // libs, so `T[]` is not a resolvable `Array` reference, `rest_element`
    // reports non-array, and the arm above fires for a shape that takes the
    // other road under the corpus. Use `examples/probefile` for that leg.
    //
    // Recorded because the first draft of this test asserted `number` here and
    // that was **my expectation, not upstream's or this port's**: under the
    // corpus the line reads `r(1, 2) : 1 | 2`, and upstream's own baseline for
    // the same shape is `makeArrayG(1, "") : number[]` (`genericRestArgs`) —
    // inference candidates are WIDENED (`getWidenedLiteralType` in
    // `getInferredType`) and this port does not widen them. That is a real
    // pre-existing defect, outside §951's road, and it now has a witness.
    // A SPREAD argument at the rest position still declines. Upstream gives the
    // element `ElementFlagsVariadic`/`ElementFlagsRest` and this port's tuple
    // side table carries no per-element flags at all (§950), so a tuple built
    // here would claim a required element where upstream records a variadic
    // one. Pinned so the decline is a recorded choice rather than an omission
    // the next reader silently lifts.
    assert_eq!(
        type_of_last(
            "declare function f<T extends unknown[]>(...args: T): T;\nconst xs: string[] = [];\nconst a = f(...xs);"
        ),
        "error"
    );
}
