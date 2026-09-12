//! An IIFE called with a SPREAD of a concrete tuple types its parameters
//! positionally. §795.
//!
//! ```ts
//! declare const t1: [number, boolean, string];
//! (function (a, b, c) {})(...t1);   // a: number, b: boolean, c: string
//! (function (...x) {})(...t1);      // x: [number, boolean, string]
//! ```
//!
//! These are `getSpreadArgumentType`'s legs at `checker.go:29504` and `:29528`
//! — §771's recorded residue, which declined every spread argument outright.
//!
//! # This is the third entry in one chain, and the chain is the point
//!
//! - **§769** wrote the IIFE REST arm, measured it at **21:9 adverse**, and
//!   reverted it. The arm was correct; what broke was downstream — a tuple
//!   inherited no `Array<T>` members, so giving a parameter its true tuple type
//!   took `noNumbers.some(…)` from RIGHT to GAP. The refusal named the
//!   prerequisite: *a tuple's apparent type must include the members of
//!   `Array<union of its elements>`*.
//! - **§770** landed exactly that.
//! - **§771** re-applied §769's arm, as its refusal had predicted.
//! - **§795** is the residue §771 itself recorded.
//!
//! A refusal that names its prerequisite precisely enough is a work item, not a
//! dead end. This one was cashed twice, months apart, by sessions that only had
//! to re-read it.
//!
//! # What is admitted
//!
//! Exactly one spread over a CONCRETE tuple. A spread mixed with plain
//! arguments needs the position arithmetic `getSpreadArgumentType` does over
//! several sources; a spread of an array has no element to land on. Both keep
//! declining.

use tsr_ast::{Expression, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed types of the IIFE's parameters, in order.
fn parameter_types(source: &str) -> Vec<String> {
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
        let mut callee = call.expression.expect("a callee");
        while let Expression::ParenthesizedExpression(parenthesized) = callee {
            callee = parenthesized.expression.expect("an inner expression");
        }
        let Expression::FunctionExpression(function) = callee else { continue };
        return function
            .parameters
            .iter()
            .map(|parameter| {
                let id = parameter.node_id.expect("registered");
                let symbol = bound.symbol_of(id).expect("a parameter symbol");
                let t = checker.get_type_of_symbol(symbol);
                checker.type_to_string(t)
            })
            .collect();
    }
    panic!("the fixture must contain an IIFE");
}

const T1: &str = "interface Array<T> { length: number }\n\
                  declare const t1: [number, boolean, string];\n";

/// `restTuplesFromContextualTypes.types:11-13`.
#[test]
fn positional_parameters_take_the_tuple_elements() {
    let source = format!("{T1}(function (a, b, c) {{}})(...t1);");
    assert_eq!(parameter_types(&source), vec!["number", "boolean", "string"]);
}

/// `restTuplesFromContextualTypes.types:21` — a rest parameter takes the whole
/// remainder as a tuple.
#[test]
fn a_rest_parameter_takes_the_remaining_elements_as_a_tuple() {
    let source = format!("{T1}(function (...x) {{}})(...t1);");
    assert_eq!(parameter_types(&source), vec!["[number, boolean, string]"]);
}

/// A rest parameter AFTER positional ones takes only the tail.
#[test]
fn a_trailing_rest_takes_only_the_tail() {
    let source = format!("{T1}(function (a, ...rest) {{}})(...t1);");
    assert_eq!(parameter_types(&source), vec!["number", "[boolean, string]"]);
}

/// The decline: a spread of an ARRAY has no element to land on, so the
/// parameters keep whatever the ordinary road gives them rather than being
/// handed a position that does not exist.
#[test]
fn a_spread_of_an_array_still_declines() {
    let source = "interface Array<T> { length: number }\n\
                  declare const xs: number[];\n\
                  (function (a, b) {})(...xs);";
    assert_ne!(parameter_types(source), vec!["number", "number"]);
}
