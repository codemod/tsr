//! §839: `checkIdentifier`'s uninitialized-variable arm returns the **declared**
//! type, discarding the narrowing.
//!
//! `checker.go:11189-11192`:
//!
//! ```go
//! } else if !assumeInitialized && !c.containsUndefinedType(t) && c.containsUndefinedType(flowType) {
//!     c.error(node, diagnostics.Variable_0_is_used_before_being_assigned, c.symbolToString(symbol))
//!     // Return the declared type to reduce follow-on errors
//!     return t
//! }
//! ```
//!
//! An uninitialized annotated `var` under `strictNullChecks` starts the flow
//! walk at `declared | undefined`. In the `then` branch of
//! `typeof x === "string"` the narrowing removes `undefined`, so the arm does
//! not fire and the narrowed type stands. In the `else` branch `undefined`
//! survives — `typeof undefined` is not `"string"` — so the arm fires and the
//! declared union replaces the narrowing.
//!
//! That asymmetry is the whole of the row, and it is observable in upstream's
//! own baselines, which is what settled it rather than a reading of the source:
//! `typeGuardOfFormTypeOfString.types` records `>strOrNum : string` in the
//! `then` branch and `>strOrNum : string | number` in the `else` branch of the
//! same guard, while `.errors.txt` reports both the TS2454 *and* a TS2322
//! assignability error that only an un-narrowed union could produce.
//!
//! Corpus effect when this landed: `WRONG->RIGHT 126`, `RIGHT->WRONG 20`.
//!
//! # The caveat this file carries
//!
//! This harness is not a faithful oracle — it has no `lib.d.ts`, no
//! `ModuleHost`, and none of `types_producer`'s position rules. It pins the
//! *mechanism* only; the corpus is what pinned the gain.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the expression of the last expression statement, descending into
/// blocks, function bodies **and both branches of an `if`** — the else branch
/// is where this rule is observable.
fn type_of_last_expression(source: &str) -> String {
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
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    let last = last_expression_statement(parsed.source_file.statements)
        .expect("the fixture must end with an expression statement");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

fn last_expression_statement<'a>(statements: &[Statement<'a>]) -> Option<tsr_ast::Expression<'a>> {
    let mut found = None;
    for statement in statements {
        match statement {
            Statement::ExpressionStatement(node) => found = node.expression,
            Statement::Block(block) => {
                if let Some(inner) = last_expression_statement(block.statements) {
                    found = Some(inner);
                }
            }
            Statement::IfStatement(node) => {
                for branch in [node.then_statement, node.else_statement].into_iter().flatten() {
                    if let Some(inner) = last_expression_statement(std::slice::from_ref(&branch)) {
                        found = Some(inner);
                    }
                }
            }
            _ => {}
        }
    }
    found
}

/// The `then` branch narrows: `undefined` does not survive `typeof x === "string"`,
/// so the arm does not fire.
#[test]
fn then_branch_of_typeof_narrows() {
    assert_eq!(
        type_of_last_expression(
            "var strOrNum: string | number;
             if (typeof strOrNum === \"string\") { strOrNum; }"
        ),
        "string"
    );
}

/// The `else` branch does **not**: `undefined` survives, the arm fires, and the
/// declared union comes back in place of `number`. This is the assertion the
/// whole of §839 turns on.
#[test]
fn else_branch_of_typeof_reads_the_declared_type() {
    assert_eq!(
        type_of_last_expression(
            "var strOrNum: string | number;
             if (typeof strOrNum === \"string\") { strOrNum; } else { strOrNum; }"
        ),
        "string | number"
    );
}

/// The control that proves the rule is about *initialization* and not about
/// `else` branches: with an initializer, `assumeInitialized` holds, no
/// `undefined` ever enters the walk, and the else branch narrows normally.
#[test]
fn an_initialized_variable_narrows_in_the_else_branch() {
    assert_eq!(
        type_of_last_expression(
            "var strOrNum: string | number = 0;
             if (typeof strOrNum === \"string\") { strOrNum; } else { strOrNum; }"
        ),
        "number"
    );
}

/// The second control: a declared type that already contains `undefined` is
/// excluded by `!c.containsUndefinedType(t)`, so the arm cannot fire and the
/// else branch narrows.
#[test]
fn a_declared_type_containing_undefined_narrows_in_the_else_branch() {
    assert_eq!(
        type_of_last_expression(
            "var strOrNum: string | number | undefined;
             if (typeof strOrNum === \"string\") { strOrNum; } else { strOrNum; }"
        ),
        "number | undefined"
    );
}

/// §839.1: the guard. A condition that names the reference *and* uses a
/// narrowing this port does not model suppresses the rule, because upstream
/// removes `undefined` there and this port does not. Without this the type road
/// measured `RIGHT->WRONG 67` instead of 20, most of it the follow-on from a
/// property access off an un-narrowed union.
#[test]
fn a_call_guard_suppresses_the_rule() {
    assert_eq!(
        type_of_last_expression(
            "declare function isString(x: unknown): x is string;
             var strOrNum: string | number;
             if (isString(strOrNum)) { strOrNum; } else { strOrNum; }"
        ),
        "number"
    );
}
