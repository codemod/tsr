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

/// **A LIMIT OF THIS HARNESS, not of the checker — corrected after it was first
/// written up the other way.**
///
/// `Action<ActionType.Bar, number>` answers `error` here while
/// `Action<string, number>` answers `number`, and §823 first recorded that as a
/// checker defect: *"an enum member as a type argument does not resolve"*.
/// **That claim is false**, and the corpus refutes it flatly: **5,723 RIGHT
/// lines carry a dotted enum-member answer** (`ambientEnum1` → `E1.y`,
/// `assignToEnum` → `A.foo`, and so on), and 14,133 RIGHT lines carry a dotted
/// answer of any kind.
///
/// So what fails is this fixture, not the road it was meant to probe. This
/// harness is `Checker::new` over **one file with no `lib.d.ts` and no
/// `ModuleHost`**, and it does not go through `types_producer`'s position rules
/// either — so it is a usable oracle for *"does this arm fire"* and **not** for
/// *"can the port express this"*.
///
/// Kept, with the caveat, because the asymmetry against the test above is still
/// the thing to re-check if anyone touches the argument road — but the next
/// question is what THIS fixture lacks, not what the checker lacks.
#[test]
fn an_enum_argument_fails_in_this_harness_only() {
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
/// bare `P`/`T`. Unlike the enum fixture above, this one is corroborated by the
/// corpus: `recursiveArrayNotCircular`'s five wrong lines answer exactly this
/// bare `P`/`T`, so the road really does hand back an uninstantiated branch.
#[test]
fn blocker_the_alias_declared_road_does_not_substitute() {
    let source = "type Action<T, P> = P extends void ? { type: T } : { type: T, payload: P };\n\
         type Bar = Action<string, number>;\n\
         declare const a: Bar;\n\
         a.payload;\n";
    assert_eq!(type_of_last_expression(source), "P");
}

/// §830: a GENERIC METHOD reached through an instantiated reference keeps its own
/// type parameters — the class's substitute, the method's shadow and survive.
///
/// `instantiate_for_reference` mapped the class's parameters by NAME, so `foo`'s
/// own `U` was substituted with the class's argument. +214 lines corpus-wide.
#[test]
fn a_generic_method_keeps_its_own_type_parameters() {
    let source = "class C<T, U> { foo<U>(t: T, u: U): T { return t; } }\n\
         declare const c: C<string, number>;\n\
         c.foo;\n";
    assert_eq!(type_of_last_expression(source), "<U>(t: string, u: U) => string");
}

/// §830.1: the same shadowing rule at the PROPERTY spelling —
/// `foo: <U>(t: T, u: U) => T` is the same two sets of parameters written the
/// other way. Measured at **zero** corpus-wide; this test is what says the
/// mechanism is correct rather than merely unexercised.
#[test]
fn a_generic_function_typed_property_keeps_its_own_type_parameters() {
    let source = "class C<T, U> { foo: <U>(t: T, u: U) => T; }\n\
         declare const c: C<string, number>;\n\
         c.foo;\n";
    assert_eq!(type_of_last_expression(source), "<U>(t: string, u: U) => string");
}
