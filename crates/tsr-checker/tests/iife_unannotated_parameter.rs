//! An IMMEDIATELY INVOKED function with unannotated parameters resolves. §809.
//!
//! ```ts
//! (jake => { })("build");            // void, and jake is string
//! (function (cats) { })("lol");      // void
//! ((a, b, c) => { })("foo", 1, true);
//! ```
//!
//! all answered `errorType`.
//!
//! # The guard that was refusing them
//!
//! `get_type_of_function_expression` (`signatures.rs`) declines a function with
//! an unannotated parameter when a contextual type exists and the contextual
//! SIGNATURE does not materialise — the 37-GAP→WRONG hazard its own comment
//! records, where an arrow whose contextual signature answers `None` types
//! standalone-`any`.
//!
//! **An IIFE is not in that domain.** Its parameter types come from a different
//! road: §768 reads them off the ARGUMENTS of the call that invokes the
//! function. That road resolves, and the guard was refusing the function above
//! parameters that were already correct.
//!
//! Verified rather than assumed. Probing `((j) => {})("build")`:
//! `get_type_of_symbol` on `j` answers **`string`** — through §768 — while this
//! guard answered `errorType` for the arrow. The parameter was right and the
//! function it belongs to was refused.
//!
//! # Why it took three entries to find
//!
//! - **§807** isolated the failure to one intersection (bare parameter × IIFE)
//!   by probe ladder, and traced that the contextual-parameter road was never
//!   entered.
//! - **§808** found and fixed a *different* unreachability — the PATTERN-named
//!   parameter arm, which has no symbol to reach that road through — and
//!   explicitly recorded that it did NOT fix this case.
//! - **§809** is this: the parameter road was fine all along for the identifier
//!   spelling, and the refusal was one level up, in the function.
//!
//! Two rounds of hand-placed traces said "the road is unreached"; what actually
//! located it was reading the guard that returns `error`.

use tsr_ast::{Expression, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The type of the file's last call expression.
fn last_call(source: &str) -> String {
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
    let mut out = String::from("NOTFOUND");
    for statement in parsed.source_file.statements {
        let Statement::ExpressionStatement(expression) = statement else { continue };
        let Some(call @ Expression::CallExpression(_)) = expression.expression else { continue };
        let id = checker.check_expression(call);
        out = checker.type_to_string(id);
    }
    out
}

/// `contextuallyTypedIife`'s first line.
#[test]
fn an_arrow_iife_with_a_bare_parameter_resolves() {
    assert_eq!(last_call("((j) => { })(\"build\");"), "void");
}

/// The function-expression spelling, its second line.
#[test]
fn a_function_expression_iife_resolves() {
    assert_eq!(last_call("(function (cats) { })(\"lol\");"), "void");
}

/// Several parameters, and the extra parens the fixture piles on.
#[test]
fn parenthesised_and_multi_parameter_iifes_resolve() {
    assert_eq!(last_call("((((function (y) { }))))(\"-\");"), "void");
    assert_eq!(last_call("((a, b, c) => { })(\"foo\", 1, true);"), "void");
}

/// The parameter itself takes the ARGUMENT's type — §768's road, which was
/// working the whole time and is what makes lifting the guard sound.
#[test]
fn the_parameter_takes_the_arguments_type() {
    let source = "((j) => j)(\"build\");";
    assert_eq!(last_call(source), "string");
}

/// The control: a NON-invoked arrow is untouched by §809.
///
/// `use((j) => j)` on `use<T>(f: (x: T) => T)` answers `(j: unknown) => unknown`
/// — the fixing mapper's `unknown`, through the guard's own
/// `single_generic_argument_context` branch. §809 adds an IIFE exemption and
/// must not change this: the arrow is an ARGUMENT here, not a callee.
///
/// Asserted on the ARGUMENT's own type, not the call's — `use(…)` returns
/// `void` either way, which is what made the first version of this test pass
/// for the wrong reason.
#[test]
fn a_non_invoked_arrow_under_a_contextual_type_still_declines() {
    let source = "declare function use<T>(f: (x: T) => T): void;\nuse((j) => j);";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
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
        assert_eq!(checker.type_to_string(id), "(j: unknown) => unknown");
        return;
    }
    panic!("the fixture must contain a call");
}
