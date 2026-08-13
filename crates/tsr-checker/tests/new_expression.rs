//! What `new C()` must get right.
//!
//! The interesting property is that this answers without ever building a
//! construct signature — see `check_new_expression` for why that is a sound
//! reduction for a class callee and nothing else. These tests pin both the case
//! it answers and each case it refuses.
//!
//! Expected strings come from `.types` baselines under
//! `vendor/typescript-go/testdata/baselines/reference/submodule`: `>new C() : C`
//! appears 160 times, `>new C<number>() : C<number>` 12 times.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement, after any set-up declarations.
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
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initialiser = declaration.initializer.expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn new_on_a_class_is_the_instance_type() {
    // `>new C() : C`, the single most common `new` line in the corpus. Note that
    // the answer is the *instance* type `C` and not the static side, which is
    // what the callee expression itself has.
    assert_eq!(type_of_last("class C {}\nconst x = new C();"), "C");
    assert_eq!(type_of_last("class C { a: string; }\nconst x = new C();"), "C");
}

#[test]
fn a_declared_constructor_does_not_change_the_answer() {
    // The reduction rests on a constructor being unable to return anything but
    // the instance type — it cannot carry a return type annotation. A class with
    // an explicit constructor, including a parameterised one, must therefore
    // give the same answer as one without.
    assert_eq!(type_of_last("class C { constructor(a: string) {} }\nconst x = new C(\"a\");"), "C");
}

#[test]
fn arguments_do_not_reach_the_answer() {
    // Same reduction as `check_call_expression`: the arguments are not checked
    // against the parameters, and for a non-generic class they cannot affect the
    // result type.
    assert_eq!(type_of_last("class C { constructor(a: string) {} }\nconst x = new C();"), "C");
}

#[test]
fn a_generic_class_is_a_gap_rather_than_the_uninstantiated_type() {
    // FLIPPED at §389. The old line asserted the gap; zero-source inference
    // has upstream's own fallback — every parameter is `unknown`
    // (`recursiveBaseCheck4/5/6` record `M<unknown>`). What the old pin got
    // right: `C<T>` and bare `C` really are wrong; the fallback is neither.
    assert_eq!(type_of_last("class C<T> {}\nconst x = new C();"), "C<unknown>");
}

#[test]
fn explicit_type_arguments_instantiate_the_class() {
    // This asserted `error` until `bd tsr-tgov` built the arm, and going red is
    // what it is for — the twelfth "unported stand-in" fixture in this project
    // to come due when its frontier moved. Rewritten as the **pair**: the
    // written-argument form answers, and the inferred form beside it still
    // gaps, so the test keeps discriminating instead of merely flipping.
    // `conformance/genericSetterInClassType.types` records
    // `>new C<number>() : C<number>`. The wider suite is `tests/new_generic.rs`.
    assert_eq!(type_of_last("class C<T> {}\nconst x = new C<number>();"), "C<number>");
    // §389: the inferred form's zero-source case answers the `unknown`
    // fallback now — the pair still discriminates, on the VALUE rather than
    // on answer-vs-gap.
    assert_eq!(type_of_last("class C<T> {}\nconst x = new C();"), "C<unknown>");
}

#[test]
fn an_abstract_class_is_a_gap_which_is_also_upstreams_answer() {
    // `Cannot_create_an_instance_of_an_abstract_class` — upstream reports and
    // answers `errorType` (`checker.go:8620`), so this agrees with upstream
    // rather than diverging from it.
    assert_eq!(type_of_last("abstract class C {}\nconst x = new C();"), "error");
}

#[test]
fn new_on_a_non_class_callee_is_a_gap() {
    // FLIPPED at §298 for the TS half: `new f()` on a plain TS function is
    // upstream's TS7009 — "'new' expression, whose target lacks a construct
    // signature, implicitly has an 'any' type" — and the answer is ANY, the
    // deliberate error-any (`avoid.ts` records `new f() : any` beside the
    // error; `anyAsReturnTypeForNewOnCall` likewise). The old rationale
    // ("upstream is the function's instance type") describes the JS
    // constructor-function pattern, which the arm therefore EXCLUDES: a JS
    // function's `new` keeps the gap until instance types land.
    assert_eq!(type_of_last("function f() {}\nconst x = new f();"), "any");
    // **A twelfth-and-then-some stand-in fixture came due.** The second half of
    // this test used to assert that `interface Ctor { new (): string; }` was a
    // gap "because it needs real construct signatures". It has them now
    // (`bd tsr-4sa`,
    // `docs/architecture/checker-notes-namedcallee.md`), so the assertion is
    // rewritten as the **pair** rather than flipped: the plain construct
    // signature answers, and the generic one beside it — a gap until §74
    // (`checker-notes-narrow.md`) — now INFERS through `check_generic_call`.
    // §162 corrected the literal half: "keeping the fresh literal exactly as
    // the call road does" was an INDUCTION from `id(1)`, whose return IS the
    // parameter at top level. `Box<T>` is not, so
    // `getCovariantInference` WIDENS (`inference.go:1442`,
    // `isTypeParameterAtTopLevelInReturnType` at `:1501`) — the corpus
    // records `>box(42) : Box<number>` (isomorphicMappedTypeInference).
    assert_eq!(
        type_of_last("interface Ctor { new (): string; }\nvar C: Ctor;\nconst x = new C();"),
        "string"
    );
    assert_eq!(
        type_of_last(
            "interface Box<T> { v: T; }\n\
             interface Ctor { new <T>(v: T): Box<T>; }\n\
             var C: Ctor;\n\
             const x = new C(1);"
        ),
        "Box<number>"
    );
}
