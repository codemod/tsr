//! Control-flow narrowing: what a *reference* is worth, as opposed to what its
//! declaration says.
//!
//! `bd tsr-4sc.11`. Upstream's `getFlowTypeOfReference` (`checker/flow.go:77`)
//! walks the flow graph backwards from a reference to find the type in effect
//! there. Assertions go through `Checker::type_to_string`, as
//! `crates/tsr-checker/tests/types.rs` does and for the same reason: the printed
//! form is what a `.types` baseline compares.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the expression of the **last** expression statement in the source.
///
/// The last one, because a narrowing fixture is a declaration, then whatever
/// narrows it, then the reference under test — and the reference is what the
/// question is about. `types.rs`'s helper types the first *initialiser*, which
/// cannot reach a reference at all.
fn type_of_last_expression(source: &str) -> String {
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

    let last = last_expression_statement(parsed.source_file.statements)
        .expect("the fixture must end with an expression statement");
    let id = checker.check_expression(last);
    checker.type_to_string(id)
}

/// The expression of the last `ExpressionStatement` anywhere in `statements`,
/// including inside a block, so a fixture can put the reference under test in
/// an `if` body.
fn last_expression_statement<'a>(statements: &[Statement<'a>]) -> Option<tsr_ast::Expression<'a>> {
    let mut found = None;
    for statement in statements {
        match statement {
            Statement::ExpressionStatement(node) => found = node.expression,
            Statement::Block(block) => {
                found = last_expression_statement(block.statements).or(found);
            }
            Statement::IfStatement(node) => {
                for branch in [node.then_statement, node.else_statement].into_iter().flatten() {
                    found = last_expression_statement(std::slice::from_ref(&branch)).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

#[test]
fn a_truthiness_guard_removes_the_falsy_constituents() {
    // The narrowing this slice is for. `undefined` is falsy-only, so a truthy
    // guard drops it; `string` and `number` can be either, so they stay.
    assert_eq!(
        type_of_last_expression("let x: string | undefined = \"a\";\nif (x) { x; }"),
        "string"
    );
    assert_eq!(
        type_of_last_expression("let x: string | number | undefined;\nif (x) { x; }"),
        "string | number"
    );
    // Parenthesised, because `narrowType` recurses through it rather than
    // treating it as an unrecognised form.
    assert_eq!(
        type_of_last_expression("let x: string | undefined = \"a\";\nif ((x)) { x; }"),
        "string"
    );
}

#[test]
fn a_falsy_guard_does_not_narrow_a_type_that_can_be_either() {
    // **This looks like a gap and is the correct answer.** `if (!x)` narrows to
    // the falsy constituents, and `string` is one of them — an empty string is
    // falsy — so `string | undefined` survives whole. TypeScript does not narrow
    // `string` to `""` here either.
    //
    // Recorded as a test rather than left implicit because it was reached by
    // measurement after the opposite was expected, and a later reader looking at
    // `if (!x)` will expect `undefined` for exactly the same wrong reason.
    assert_eq!(
        type_of_last_expression("let x: string | undefined = \"a\";\nif (!x) { x; }"),
        "string | undefined"
    );
    // The `!` arm is still doing something: on a type whose only falsy
    // constituent is `undefined`, it keeps that one and drops the rest.
    assert_eq!(
        type_of_last_expression("let x: number | undefined;\nif (!x) { x; }"),
        "number | undefined"
    );
}

#[test]
fn narrowing_a_falsy_only_type_by_truthiness_leaves_never() {
    // `filterType` on a non-union either keeps it whole or yields `never`, which
    // is upstream's behaviour and is what makes an impossible branch visible.
    assert_eq!(type_of_last_expression("let x: undefined;\nif (x) { x; }"), "never");
}

#[test]
fn the_narrowing_does_not_escape_the_branch_it_belongs_to() {
    // The branch label unions what each path says, so a reference *after* the
    // `if` sees both. Without this, narrowing would leak forward and print a
    // plausible wrong type at every later reference — the failure mode this
    // module is most able to cause.
    assert_eq!(
        type_of_last_expression("let x: string | undefined = \"a\";\nif (x) { }\nx;"),
        "string | undefined"
    );
}

#[test]
fn an_unported_guard_leaves_the_declared_type_rather_than_a_wrong_one() {
    // The property that makes a partial port of `narrowType` safe: its default
    // arm returns the type unchanged, so a form this port does not recognise
    // gives the answer it gave before narrowing existed. `typeof` guards are the
    // largest such form and are not ported.
    assert_eq!(
        type_of_last_expression(
            "let x: string | undefined = \"a\";\nif (typeof x === \"string\") { x; }"
        ),
        "string | undefined"
    );
}

#[test]
fn an_assignment_narrowing_is_not_ported_and_says_so() {
    // `getTypeAtFlowAssignment` reduces a union declared type to the
    // constituents the assigned type could be, through `getAssignmentReducedType`
    // -> `typeMaybeAssignableTo`. This checker has no assignability, so the
    // assignment arm returns the declared type unreduced.
    //
    // Asserted rather than left undocumented so that the day assignability
    // lands, this test fails and points at the one function that has to change.
    assert_eq!(
        type_of_last_expression("let x: string | number = \"a\";\nx = 1;\nx;"),
        "string | number",
        "upstream answers `number` here; see `Checker::get_type_at_flow_assignment`"
    );
}

#[test]
fn a_declaration_with_an_initialiser_is_not_a_narrowing_gap() {
    // `let x = "a"` is `string` here **and upstream**, because
    // `getTypeAtFlowAssignment` reduces only when the *declared* type is a
    // union. It reads exactly like a missing narrowing and is not one.
    assert_eq!(type_of_last_expression("let x = \"a\";\nx;"), "string");
}

#[test]
fn a_guard_on_one_variable_does_not_narrow_another() {
    // The single failure mode in this module that produces a *plausible wrong
    // answer* rather than a gap: a reference match that is too loose narrows
    // whatever the guard mentioned, whichever variable is being asked about.
    // `is_matching_reference` is identifier-only and compares resolved symbols
    // for exactly this reason.
    assert_eq!(
        type_of_last_expression(
            "let x: string | undefined;\nlet y: string | undefined;\nif (y) { x; }"
        ),
        "string | undefined",
        "a guard on `y` must leave `x` alone"
    );
}
