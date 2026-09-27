//! What a property access answers on a reference to an alias whose body is a
//! CONDITIONAL. §823's probe, pinned.
//!
//! §822 was built on a guess about this and measured at zero corpus-wide. The
//! guess was that `evaluate_conditional_alias`'s frame could not reach the
//! branch's members; the refutation was that the failing lines never go through
//! that function at all — they are property accesses on a type REFERENCE, and
//! they resolve through `type_reference_targets` on the `bd tsr-4qx` member seam.
//!
//! This file is the probe that should have come first. It asserts what the port
//! answers today, so the next change to this road has a before-picture that is
//! measured rather than recalled.
//!
//! The corpus shape is `recursiveArrayNotCircular`:
//!
//! ```ts
//! type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P }
//! ```
//!
//! and upstream answers `number` for `payload` on
//! `Action<ActionType.Bar, number>` where this port answers `P`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the expression of the last expression statement, descending into
/// function bodies so a fixture can introduce type parameters.
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
                found = last_expression_statement(block.statements).or(found);
            }
            Statement::FunctionDeclaration(node) => {
                if let Some(tsr_ast::FunctionBody::Block(block)) = node.body {
                    found = last_expression_statement(block.statements).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

const ACTION: &str = "enum ActionType { Foo, Bar }\n\
     type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P }\n";

/// The corpus shape, and the line `recursiveArrayNotCircular` gets wrong:
/// upstream answers `number`.
#[test]
fn a_property_of_a_conditional_alias_reference() {
    let source = format!("{ACTION}declare const a: Action<ActionType.Bar, number>;\na.payload;\n");
    // Pinned as it behaves: **`error`**, a gap. Upstream answers `number`.
    //
    // Note the corpus answers `P` rather than `error` for the same shape, and
    // that difference is itself informative: in `recursiveArrayNotCircular` the
    // access goes through a UNION of these references narrowed by
    // `switch (action.type)`, so the `P` arrives on the narrowed-union road. The
    // direct reference declines outright.
    assert_eq!(type_of_last_expression(&source), "error");
}

/// The sibling property fails identically — upstream answers `ActionType.Bar`.
#[test]
fn the_other_property_fails_identically() {
    let source = format!("{ACTION}declare const a: Action<ActionType.Bar, number>;\na.type;\n");
    assert_eq!(type_of_last_expression(&source), "error");
}

/// **The control that localises the defect.** The same member access on an alias
/// whose body is a plain type literal — no conditional — substitutes correctly.
///
/// This is what says the defect is the CONDITIONAL body rather than the member
/// seam: `bd tsr-4qx`'s substitution works, and it is the conditional that gives
/// it nothing to substitute through.
#[test]
fn a_plain_generic_alias_substitutes_its_member() {
    let source = "type Plain<T, P> = { type: T, payload: P };\n\
         declare const a: Plain<string, number>;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "number");
}

/// **§823's mechanism, working.** The same shape with ordinary arguments: the
/// conditional's chosen branch supplies the member table and substitution
/// proceeds exactly as it does for `Plain`.
///
/// This is the test that says §823 is correct even though it moved **zero**
/// corpus lines — every corpus instance is blocked by one of the two defects
/// pinned below, not by this road.
#[test]
fn a_conditional_alias_reference_answers_from_its_branch() {
    let source = "type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P };\n\
         declare const a: Action<string, number>;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "number");
}

/// **Blocker 1, pinned: an ENUM MEMBER as a type argument does not resolve.**
///
/// `Action<ActionType.Bar, number>` still answers `error` where
/// `Action<string, number>` answers `number`, so the difference is the argument
/// rather than the road. `recursiveArrayNotCircular` — the case that motivated
/// §821–§823 — uses `Action<ActionType.Bar, number>` throughout, which is why
/// §823 gains nothing there.
#[test]
fn blocker_an_enum_member_type_argument_does_not_resolve() {
    let source = format!("{ACTION}declare const a: Action<ActionType.Bar, number>;\na.payload;\n");
    assert_eq!(type_of_last_expression(&source), "error");
}

/// **Blocker 2, pinned: the alias-declared road answers the UNINSTANTIATED
/// branch.**
///
/// Reached through an intermediate alias, the same reference answers a bare `P`
/// rather than `number` — so `in_alias_declared_position`'s road
/// (`declared.rs:2143`) evaluates the conditional and hands back a branch whose
/// parameters were never substituted. This is the road that gives the corpus its
/// bare `P`/`T`, and it is a different defect from blocker 1.
#[test]
fn blocker_the_alias_declared_road_does_not_substitute() {
    let source = "type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P };\n\
         type Bar = Action<string, number>;\n\
         declare const a: Bar;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "P");
}
