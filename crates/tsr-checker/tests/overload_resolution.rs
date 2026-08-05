//! What overload selection must get right.
//!
//! `resolveCall` (`checker.go:8843`) and `chooseOverload` (`checker.go:9025`)
//! pick the **first** candidate, in declaration order, that `hasCorrectArity`
//! (`checker.go:9107`) admits and that every argument is assignable to. The port
//! runs that selection only over argument and parameter types where a *negative*
//! assignability answer is trustworthy; everywhere else it answers `errorType`,
//! which prints `error`.
//!
//! The discriminating tests here are the ones that select a candidate other than
//! the first. A fixture where every candidate returns the same type would pass
//! under a first-candidate-always implementation and prove nothing, so each test
//! below gives the candidates *different* return types.
//!
//! `>x : number` and friends are the baseline shape, under
//! `vendor/typescript-go/testdata/baselines/reference/submodule`; the corpus
//! files this mirrors are `compiler/overloadingOnConstants2.types` and
//! `conformance/overloadResolution.types`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the last statement.
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
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

/// Two overloads separated by parameter type, with an implementation signature.
///
/// The implementation signature is deliberately present: upstream does **not**
/// put it in the candidate list when there are overloads, and a port that did
/// would answer `boolean` for both calls below.
const BY_TYPE: &str = "\
function f(x: string): string;
function f(x: number): number;
function f(x: any): any { return x; }
";

#[test]
fn a_later_candidate_wins_when_the_first_does_not_accept_the_argument() {
    // THE DISCRIMINATING CASE. `f(1)` must reach the SECOND candidate: the
    // first takes `string`, and `1` is not assignable to it. An implementation
    // that took the first candidate — or the last, or the one with the widest
    // parameter — answers `string` here.
    assert_eq!(type_of_last(&format!("{BY_TYPE}const x = f(1);")), "number");
    // …and the first candidate still wins when it is the one that matches, so
    // the test above is not passing by preferring later candidates.
    assert_eq!(type_of_last(&format!("{BY_TYPE}const x = f(\"a\");")), "string");
}

#[test]
fn arity_alone_selects_when_it_is_what_separates_the_candidates() {
    // `hasCorrectArity` runs before assignability, so a call with one argument
    // skips a two-parameter candidate outright. Declaring the two-parameter
    // candidate FIRST is what makes this discriminate.
    let source = "\
function g(a: number, b: number): string;
function g(a: number): number;
function g(a: number, b?: number): any { return a; }
const x = g(1);
";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn an_optional_parameter_widens_the_arity_a_candidate_admits() {
    // The required-parameter count stops at the first optional one, so a
    // candidate with one required and one optional parameter admits both one
    // and two arguments. Selecting on `parameters.len()` alone would answer
    // `error` for the one-argument call.
    let source = "\
function h(a: string, b?: string): string;
function h(a: number): number;
function h(a: any): any { return a; }
const x = h(\"a\");
";
    assert_eq!(type_of_last(source), "string");
}

#[test]
fn a_literal_argument_selects_by_its_literal_type_not_its_widened_one() {
    // `overloadingOnConstants2.types` is the corpus shape: candidates separated
    // by string-literal parameter types. `"b"` is not assignable to `"a"`, so
    // the second candidate wins — and a port that widened the argument to
    // `string` before comparing would match NEITHER candidate and answer
    // `error`.
    let source = "\
function k(x: \"a\"): string;
function k(x: \"b\"): number;
function k(x: any): any { return x; }
const x = k(\"b\");
";
    assert_eq!(type_of_last(source), "number");
}

#[test]
fn an_undecidable_argument_is_error_and_never_a_plausible_first_candidate() {
    // Object-typed parameters are outside what a `false` from this port's
    // relater can be trusted about, so the whole call is a gap. This is the
    // test that fails FIRST if someone relaxes the guard to "just try it": a
    // first-candidate answer here would be `string`.
    let source = "\
function m(x: { a: number }): string;
function m(x: { b: number }): number;
function m(x: any): any { return x; }
const value = { b: 1 };
const x = m(value);
";
    assert_eq!(type_of_last(source), "error");

    // A generic candidate anywhere in the set is a gap too — selecting it needs
    // inference, and its return type would print `T`.
    let generic = "\
function n(x: number): number;
function n<T>(x: T): T;
function n(x: any): any { return x; }
const x = n(1);
";
    assert_eq!(type_of_last(generic), "error");
}

#[test]
fn no_candidate_matching_is_a_gap_rather_than_the_nearest_miss() {
    // Upstream reports against the candidate with the fewest problems and
    // answers that candidate's return type. This port has no error reporting,
    // so it answers `error` rather than pinning one of the two as "closest".
    let source = "\
function p(x: string): string;
function p(x: number): number;
function p(x: any): any { return x; }
const x = p(true);
";
    assert_eq!(type_of_last(source), "error");
}
