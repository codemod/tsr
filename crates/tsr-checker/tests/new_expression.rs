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
    // `>new C<number>() : C<number>` upstream. Without inference this would
    // print `C<T>` or bare `C`, both wrong lines, so it gaps.
    assert_eq!(type_of_last("class C<T> {}\nconst x = new C();"), "error");
}

#[test]
fn explicit_type_arguments_are_a_gap() {
    assert_eq!(type_of_last("class C<T> {}\nconst x = new C<number>();"), "error");
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
    // A function callee has a construct signature upstream and this port has no
    // construct signatures at all, so it must not guess. `new f()` upstream is
    // the function's instance type, which is emphatically not `f`'s return type.
    assert_eq!(type_of_last("function f() {}\nconst x = new f();"), "error");
    // An interface with a construct signature member is the shape `new Date()`
    // really takes, and it needs real construct signatures.
    assert_eq!(
        type_of_last("interface Ctor { new (): string; }\nvar C: Ctor;\nconst x = new C();"),
        "error"
    );
}
