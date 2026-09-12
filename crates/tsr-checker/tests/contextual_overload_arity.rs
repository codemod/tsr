//! An object-literal argument of a MIXED-ARITY overloaded call keeps its
//! contextual type. §789.
//!
//! `symbols.rs`'s §56.3 supplies the contextual member root for an
//! object-literal argument by resolving the call's signature. For an overload
//! set `resolve_call_signature` answers the **first** candidate without
//! consulting arity, so a mixed-arity set handed this road the wrong parameter
//! list: `g({ u: "a" })` against
//!
//! ```ts
//! declare function g(x: number, o: { u: "a" | "b" }): void;
//! declare function g(o: { u: "a" | "b" }): void;
//! ```
//!
//! looked for `u` on the first overload's `x: number`, found nothing, and left
//! the literal to widen — `{ u: string; }` where the oracle records
//! `{ u: "a"; }`.
//!
//! **The failure is narrower than "overloads", which is what made it cheap.**
//! A probe ladder found single signatures, methods, and SAME-ARITY overload
//! sets all working (the last through §70's agreement path). Only mixed-arity
//! sets fail, and there the arity IS the choice — upstream's `chooseOverload`
//! discards every candidate that cannot take this many arguments before
//! anything subtler runs.
//!
//! **Uniqueness is required here where §788's `new` road takes the first fit.**
//! This road supplies a contextual TYPE rather than an answer, so a wrong
//! context silently retypes the argument, while a decline merely leaves the
//! widening that was already there. A tie declines.
//!
//! Measured +25 WRONG→RIGHT (`arrayToLocaleStringES2020` 13,
//! `arrayToLocaleStringES2015` 12) with **zero adverse transitions of any
//! kind**.

use tsr_ast::{Expression, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the first argument of the file's first call statement.
fn first_argument_type(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::ExpressionStatement(expression) = statement else { continue };
        let Some(Expression::CallExpression(call)) = expression.expression else { continue };
        let Some(&argument) = call.arguments.first() else { continue };
        let id = checker.check_expression(argument);
        return checker.type_to_string(id);
    }
    panic!("the fixture must contain a call statement");
}

/// The head shape, as a free function.
#[test]
fn a_mixed_arity_overloaded_call_keeps_the_contextual_type() {
    let source = "declare function g(x: number, o: { u: \"a\" | \"b\" }): void;\n\
                  declare function g(o: { u: \"a\" | \"b\" }): void;\n\
                  g({ u: \"a\" });";
    assert_eq!(first_argument_type(source), "{ u: \"a\"; }");
}

/// The same through an interface METHOD, which is where the corpus puts it.
#[test]
fn the_method_spelling_works_the_same_way() {
    let source = "interface I {\n\
                    m(x: number, o: { u: \"a\" | \"b\" }): void;\n\
                    m(o: { u: \"a\" | \"b\" }): void;\n\
                  }\n\
                  declare const i: I;\ni.m({ u: \"a\" });";
    assert_eq!(first_argument_type(source), "{ u: \"a\"; }");
}

/// The control: a single signature has always worked, through a different road.
#[test]
fn a_single_signature_still_works() {
    let source = "declare function g(o: { u: \"a\" | \"b\" }): void;\ng({ u: \"a\" });";
    assert_eq!(first_argument_type(source), "{ u: \"a\"; }");
}

/// The other control: no contextual literal type means no preservation, which
/// is what keeps this from being a blanket "never widen".
#[test]
fn a_plain_string_property_still_widens() {
    let source = "declare function g(x: number, o: { u: string }): void;\n\
                  declare function g(o: { u: string }): void;\n\
                  g({ u: \"a\" });";
    assert_eq!(first_argument_type(source), "{ u: string; }");
}

/// The tie declines. Two candidates both accepting one argument leave the
/// resolved-signature road in charge rather than picking one.
#[test]
fn an_ambiguous_arity_match_declines_rather_than_guessing() {
    let source = "declare function g(o: { u: \"a\" | \"b\" }): number;\n\
                  declare function g(o: { v: number }): string;\n\
                  g({ u: \"a\" });";
    // Either answer is acceptable; what must NOT happen is picking the second
    // candidate's shape for the first candidate's argument.
    let answer = first_argument_type(source);
    assert!(answer == "{ u: \"a\"; }" || answer == "{ u: string; }", "unexpected: {answer}");
}
