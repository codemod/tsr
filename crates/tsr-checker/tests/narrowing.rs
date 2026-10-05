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
    type_of_last_expression_in_file(source, "test.ts")
}

fn type_of_last_expression_in_file(source: &str, name: &str) -> String {
    type_of_last_expression_with_null_checks(source, name, true)
}

fn type_of_last_expression_with_null_checks(
    source: &str,
    name: &str,
    strict_null_checks: bool,
) -> String {
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    // Match the compiler loader: a JS parse dialect does not stamp its root.
    if std::path::Path::new(name).extension().is_some_and(|ext| ext.eq_ignore_ascii_case("js")) {
        parsed.nodes.add_flags(
            parsed.source_file.node_id.expect("a parsed file has an id"),
            tsr_ast::NodeFlags::JAVASCRIPT_FILE,
        );
    }
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        tsr_binder::BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
    // §740: the auto road (`let x;` → evolving `undefined`/assigned types)
    // exists only under `noImplicitAny` (`checker.go:16697`), and the flow
    // predicate now reads the flag. These fixtures pin auto behaviour, so
    // the harness states the flag rather than inheriting the optionless
    // default (off), matching the corpus fixtures they mirror
    // (`@noImplicitAny: true` / `@strict: true`).
    let options = tsr_core::CompilerOptions {
        no_implicit_any: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    checker.set_strict_null_checks(strict_null_checks);

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
            Statement::WhileStatement(node) => {
                found = last_expression_statement(std::slice::from_ref(&node.statement)).or(found);
            }
            // §758: a function body, so a fixture can introduce a TYPE
            // PARAMETER — which needs a generic signature and therefore
            // cannot be written at the top level.
            Statement::FunctionDeclaration(node) => {
                if let Some(tsr_ast::FunctionBody::Block(block)) = node.body {
                    found = last_expression_statement(block.statements).or(found);
                }
            }
            // §754: a switch clause's body, so a fixture can put the
            // reference under test inside `case "a":`.
            Statement::SwitchStatement(node) => {
                for clause in node.case_block.iter().flat_map(|block| block.clauses) {
                    found = last_expression_statement(clause.statements).or(found);
                }
            }
            Statement::TryStatement(node) => {
                for block in [
                    node.try_block,
                    node.catch_clause.and_then(|clause| clause.block),
                    node.finally_block,
                ]
                .into_iter()
                .flatten()
                {
                    found = last_expression_statement(block.statements).or(found);
                }
            }
            _ => {}
        }
    }
    found
}

#[test]
fn assignment_conditions_match_the_assigned_reference_in_both_operand_orders() {
    for condition in
        ["((x = next())) !== null", "null !== (x = next())", "(touch(), x = next()) !== null"]
    {
        assert_eq!(
            type_of_last_expression(&format!(
                "declare function next(): string | null;
                 declare function touch(): void;
                 let x: string | null; if ({condition}) {{ x; }}"
            )),
            "string",
            "condition: {condition}"
        );
    }
}

#[test]
fn assignment_conditions_keep_the_null_branch_and_leave_other_references_alone() {
    assert_eq!(
        type_of_last_expression(
            "declare function next(): string | null;
             let x: string | null; if ((x = next()) === null) { x; }"
        ),
        "null"
    );
    assert_eq!(
        type_of_last_expression(
            "declare function next(): string | null;
             function f(y: string | null) {
                 let x: string | null; if ((x = next()) !== null) { y; }
             }"
        ),
        "string | null"
    );
}

#[test]
fn assignment_conditions_apply_truthiness_and_rhs_guards() {
    assert_eq!(
        type_of_last_expression(
            "declare function next(): string | null;
             let x: string | null; if (x = next()) { x; }"
        ),
        "string"
    );
    assert_eq!(
        type_of_last_expression(
            "function f(y: string | number) {
                 let flag: boolean; if (flag = typeof y === 'string') { y; }
             }"
        ),
        "string"
    );
    assert_eq!(
        type_of_last_expression(
            "function f(x: 'a' | null) { if (!(x = null as 'a' | null)) { x; } }"
        ),
        "null"
    );
}

#[test]
fn assignment_conditions_narrow_logical_assignment_results() {
    for operator in ["||=", "&&=", "??="] {
        assert_eq!(
            type_of_last_expression(&format!(
                "function f(x: 'a' | null, y: 'a' | null) {{
                    if (x {operator} y) {{ x; }}
                 }}"
            )),
            "\"a\"",
            "operator: {operator}"
        );
    }
}

#[test]
fn assignment_conditions_normalize_discriminant_accesses() {
    assert_eq!(
        type_of_last_expression(
            "function f(x: {kind: 'a'; a: number} | {kind: 'b'; b: number}) {
                 if ((x.kind = x.kind) === 'a') { x; }
             }"
        ),
        "{ kind: 'a'; a: number; }"
    );
}

#[test]
fn assignment_conditions_match_access_references_and_predicate_arguments() {
    assert_eq!(
        type_of_last_expression(
            "declare function next(): string | null;
             function f(obj: { value: string | null }) {
                 if ((obj.value = next()) !== null) { obj.value; }
             }"
        ),
        "string"
    );
    assert_eq!(
        type_of_last_expression(
            "declare function next(): string | number;
             declare function isString(value: string | number): value is string;
             let x: string | number; if (isString((x = next()))) { x; }"
        ),
        "string"
    );
}

#[test]
fn comma_conditions_inherit_rhs_guards() {
    assert_eq!(
        type_of_last_expression(
            "declare function touch(): void;
             function f(y: string | number) {
                 if ((touch(), typeof y === 'string')) { y; }
             }"
        ),
        "string"
    );
}

#[test]
fn a_jsdoc_cast_stops_reference_matching_through_parentheses() {
    assert_eq!(
        type_of_last_expression_in_file(
            "let value = ''; switch (/** @type {'foo' | 'bar'} */ (value)) {
                 case 'foo': value; break;
             }",
            "test.js"
        ),
        "string"
    );
    assert_eq!(
        type_of_last_expression_in_file(
            "let value = ''; switch ((value)) { case 'foo': value; break; }",
            "test.js"
        ),
        "\"foo\""
    );
    assert_eq!(
        type_of_last_expression_in_file(
            "/** @param {{value: number | null}} obj */
             function f(obj) {
                 if (obj.value) { (/** @type {{value: number | null}} */ (obj)).value; }
             }",
            "test.js"
        ),
        "number | null"
    );
}

#[test]
fn assignment_conditions_narrow_loop_body_reads() {
    assert_eq!(
        type_of_last_expression(
            "declare function next(): string | null;
             let x: string | null; while ((x = next()) !== null) { x; }"
        ),
        "string"
    );
}

#[test]
fn assignment_filtering_preserves_a_named_union_when_every_member_survives() {
    assert_eq!(
        type_of_last_expression("type Choice = 'a' | 'b'; let value: Choice = undefined; value;"),
        "Choice"
    );
}

#[test]
fn a_read_after_finally_uses_only_normal_try_completion() {
    assert_eq!(
        type_of_last_expression("function f(x: string | number) { try { x = 1; } finally {} x; }"),
        "number"
    );
}

#[test]
fn a_read_inside_finally_retains_exception_paths() {
    assert_eq!(
        type_of_last_expression("function f(x: string | number) { try { x = 1; } finally { x; } }"),
        "string | number"
    );
}

#[test]
fn a_read_after_finally_excludes_pending_returns() {
    assert_eq!(
        type_of_last_expression(
            "function f(x: string | number, stop: boolean) {
                try { if (stop) return; x = 1; } finally {} x;
            }"
        ),
        "number"
    );
}

#[test]
fn nested_finally_reductions_preserve_the_normal_assignment() {
    assert_eq!(
        type_of_last_expression(
            "function f(x: string | number) {
                try { try { x = 1; } finally {} } finally {} x;
            }"
        ),
        "number"
    );
}

#[test]
fn a_truthiness_guard_removes_the_falsy_constituents() {
    // The narrowing this slice is for. `undefined` is falsy-only, so a truthy
    // guard drops it; `string` and `number` can be either, so they stay.
    assert_eq!(type_of_last_expression("let x: string | undefined;\nif (x) { x; }"), "string");
    assert_eq!(
        type_of_last_expression("let x: string | number | undefined;\nif (x) { x; }"),
        "string | number"
    );
    // Parenthesised, because `narrowType` recurses through it rather than
    // treating it as an unrecognised form.
    assert_eq!(type_of_last_expression("let x: string | undefined;\nif ((x)) { x; }"), "string");
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
        type_of_last_expression("let x: string | undefined;\nif (!x) { x; }"),
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
        type_of_last_expression("let x: string | undefined;\nif (x) { }\nx;"),
        "string | undefined"
    );
}

#[test]
fn an_unported_guard_leaves_the_declared_type_rather_than_a_wrong_one() {
    // The property that makes a partial port of `narrowType` safe: its default
    // arm returns the type unchanged, so a form this port does not recognise
    // gives the answer it gave before narrowing existed. This fixture's
    // stand-in has expired FIVE times now (`typeof` -> `bd tsr-q9g`;
    // comparability -> §52; `instanceof` -> §83, `checker-notes-narrow.md`),
    // which is exactly why the conventions require the PAIR. The stand-in is
    // now an `in` guard whose NAME is a variable — the ported `in` arm
    // requires a written string literal, and a computed name needs the
    // operand's type read mid-walk. The ported half of the pair is asserted
    // beside it in `a_typeof_guard_narrows_by_the_named_primitive`.
    assert_eq!(
        type_of_last_expression(
            "declare const k: string;\ndeclare const x: { a: string } | { b: number };\nif (k in x) { x; }"
        ),
        "{ a: string; } | { b: number; }"
    );
}

#[test]
fn an_assignment_reduces_a_union_wherever_it_appears() {
    // This test used to assert the *gap*: that `getTypeAtFlowAssignment` left a
    // union declared type unreduced because this checker had no assignability.
    // It is kept, inverted, because the fixture is the one that proves the arm
    // fires — and because the initialiser form below is why several truthiness
    // fixtures in this file had to drop their initialisers.
    assert_eq!(type_of_last_expression("let x: string | number = \"a\";\nx = 1;\nx;"), "number");
    // An **initialiser** is an assignment too: the binder records a flow node
    // against the declaration, so `getInitialType` reduces the union at the
    // declaration itself and the guard downstream sees the reduced type.
    assert_eq!(
        type_of_last_expression("let x: string | undefined = \"a\";\nif (!x) { x; }"),
        "string"
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

#[test]
fn an_unannotated_let_evolves_to_what_was_assigned() {
    // `any` evolution: the declaration is `any`, the *reference* is what the
    // assignment reaching it put there. This is `getTypeAtFlowAssignment`'s
    // automatic arm (`flow.go:232`), and it is why upstream prints `number` on
    // a line whose declaration says `any`.
    assert_eq!(type_of_last_expression("let x;\nx = 1;\nx;"), "number");
    assert_eq!(type_of_last_expression("let x;\nx = \"a\";\nx;"), "string");
    // Widened, as upstream widens: `getWidenedLiteralType` on the assigned type,
    // so it is `number` and not `1`. The declaration is not `const`, so there is
    // no freshness to keep.
    assert_eq!(type_of_last_expression("let x;\nx = true;\nx;"), "boolean");
    // The *last* assignment on the path wins, because the walk is backwards and
    // stops at the first assignment it reaches.
    assert_eq!(type_of_last_expression("let x;\nx = 1;\nx = \"a\";\nx;"), "string");
}

#[test]
fn an_unannotated_let_is_undefined_before_it_is_assigned() {
    // The initial type of an automatic declaration is `undefined`, not `any`
    // (`checker.go:11165`). A reference before any assignment therefore answers
    // `undefined` — this is upstream's answer, and it is the observable
    // consequence of the initial type, so it is the test that would catch the
    // initial type being left as the declared one.
    assert_eq!(type_of_last_expression("let x;\nx;"), "undefined");
}

#[test]
fn evolution_unions_across_a_branch() {
    // One path assigns, the other does not, so the branch label unions the
    // assigned type with the `undefined` from the top of the graph. Nothing
    // about this is special-cased: it falls out of the initial type meeting
    // `getTypeAtFlowBranchLabel`.
    assert_eq!(type_of_last_expression("let x;\nif (x) { x = 1; }\nx;"), "number | undefined");
    // Both paths assign, so `undefined` is unreachable at the join.
    assert_eq!(
        type_of_last_expression("let x;\nif (x) { x = 1; } else { x = \"a\"; }\nx;"),
        "string | number"
    );
}

#[test]
fn an_annotated_or_initialised_declaration_does_not_evolve() {
    // The boundary of the automatic arm, from both sides. An annotation means
    // the declared type is real and the assignment cannot replace it.
    assert_eq!(type_of_last_expression("let x: any;\nx = 1;\nx;"), "any");
    // An initialiser means the declaration is typed from it, and — this is the
    // one that looks wrong and is not — a later assignment does *not* change the
    // answer, because the declared type is neither automatic nor a union.
    assert_eq!(type_of_last_expression("let x = \"a\";\nx = 1;\nx;"), "string");
    // `const` is excluded upstream before the automatic arm is reached.
    assert_eq!(type_of_last_expression("const x = 1;\nx;"), "1");
}

#[test]
fn an_assignment_reduces_a_declared_union() {
    // The other half of `getTypeAtFlowAssignment`: a *declared* union keeps only
    // the constituents the assigned type could be
    // (`getAssignmentReducedType`, `flow.go:2399`).
    assert_eq!(type_of_last_expression("let x: string | number;\nx = 1;\nx;"), "number");
    assert_eq!(
        type_of_last_expression("let x: string | number | undefined;\nx = \"a\";\nx;"),
        "string"
    );
    // Upstream's give-up guard (`flow.go:2424`): when the assigned type is not
    // assignable to what the filter kept, the declared type is returned whole
    // rather than a type the assignment refutes.
    assert_eq!(
        type_of_last_expression("let x: string | number;\nx = true;\nx;"),
        "string | number"
    );
    // A **union** assigned type needs only one constituent to be assignable to a
    // declared constituent for that constituent to survive
    // (`typeMaybeAssignableTo`, `flow.go:2434`). Requiring all of them would
    // keep nothing here, and the give-up guard would then hand back the whole
    // declared union — the same printed answer as no reduction at all, which is
    // why this case needs its own fixture to be visible.
    assert_eq!(
        type_of_last_expression(
            "let x: string | number | boolean;\nlet y: string | number;\nx = y;\nx;"
        ),
        "string | number"
    );
}

/// Every expectation below comes from a **baseline**, not from intuition —
/// this file's own rule, and the one that caught three wrong guesses in
/// `logical_and.rs`. The two sources:
///
/// - `conformance/controlFlowGenericTypes.types:566` writes
///   `if (control !== undefined)` on a `control: T | undefined` and records
///   `>control : T` inside the block.
/// - `conformance/equalityStrictNulls.types:4` writes every one of the eight
///   operator/operand combinations against a **non-nullable** `x: string`,
///   which is the control: a type with no nullable constituent has nothing to
///   remove on the `!=` side.
#[test]
fn an_inequality_against_undefined_removes_the_undefined() {
    // The baseline shape, with `T` replaced by a concrete type so the fixture
    // needs no generic machinery to answer.
    assert_eq!(
        type_of_last_expression(
            "declare const control: string | undefined;\nif (control !== undefined) { control; }"
        ),
        "string"
    );
    // `!= null` removes both constituents under `==`'s coercion, which is the
    // whole reason `EQ_UNDEFINED_OR_NULL` is a separate bit from the two
    // single ones.
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | null | undefined;\nif (x != null) { x; }"
        ),
        "string"
    );
    // `!== null` removes **only** `null`: the strict operator does not see
    // `undefined`. An implementation that routed both operators through
    // `NE_UNDEFINED_OR_NULL` prints `string` here and is wrong.
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | null | undefined;\nif (x !== null) { x; }"
        ),
        "string | undefined"
    );
    // The reference on the right-hand side, which upstream reaches through
    // `getReferenceCandidate` normalising the operands.
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | undefined;\nif (undefined !== x) { x; }"
        ),
        "string"
    );
}

#[test]
fn the_assume_false_branch_keeps_only_the_nullable_part() {
    // The `else` of the first fixture above. `filter_type` on the negated
    // assumption keeps exactly the constituents the `if` removed, which is
    // what makes the two branches a partition rather than two guesses.
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | undefined;\nif (x !== undefined) { 0; } else { x; }"
        ),
        "undefined"
    );
}

#[test]
fn a_non_nullable_type_is_left_alone_by_the_inequality() {
    // `equalityStrictNulls`' control: `x: string` under `!= undefined` and
    // `!== null` records `>x : string` throughout. This is the assertion that
    // fails if the `Base*StrictFacts` bits are wrong — a `string` missing
    // `NE_UNDEFINED` would filter to `never` here.
    for guard in ["x != undefined", "x !== undefined", "x != null", "x !== null"] {
        assert_eq!(
            type_of_last_expression(&format!("declare const x: string;\nif ({guard}) {{ x; }}")),
            "string",
            "guard: {guard}"
        );
    }
}

#[test]
fn an_equality_against_a_non_nullable_operand_narrows_nothing() {
    // RE-POINTED by §52 (`checker-notes-narrow.md`): the comparability
    // branch of `narrowTypeByEquality` is now ported, so `x === "a"`
    // filters to the comparable constituent and
    // `replacePrimitivesWithLiterals` spells it as the literal — the
    // answer upstream gives. The never-collapse this test feared is pinned
    // by the filter's kept-empty guard instead.
    assert_eq!(
        type_of_last_expression("declare const x: string | undefined;\nif (x === \"a\") { x; }"),
        "\"a\""
    );
}

/// `unknownControlFlow.types`' #50706 repro distinguishes the empty-object
/// operand from the nullable and direct-unknown roads. After `!== undefined`,
/// an `unknown` is represented as `{} | null`; strict equality to a primitive
/// still narrows to that primitive (`someType(t,
/// IsEmptyAnonymousObjectType)` in native `narrowTypeByEquality`).
#[test]
fn strict_equality_narrows_a_filtered_unknown_through_its_empty_object_part() {
    assert_eq!(
        type_of_last_expression(
            "declare let x: unknown;\n\
             if (x !== undefined && x !== \"utf8\") { throw 0; }\n\
             x;"
        ),
        "\"utf8\" | undefined"
    );

    // Equal and unequal syntax must reach the same assume-true equality after
    // operator polarity is normalized.
    for source in [
        "declare let x: unknown;\nif (x !== undefined) { if (x === 42) { x; } }",
        "declare let x: unknown;\nif (x !== undefined) { if (x !== 42) {} else { x; } }",
    ] {
        assert_eq!(type_of_last_expression(source), "42");
    }

    // Nullable operands keep their dedicated facts path; the empty-object arm
    // must not turn either comparison into a primitive-literal comparison.
    assert_eq!(
        type_of_last_expression("declare let x: unknown;\nif (x === undefined) { x; }"),
        "undefined"
    );
    assert_eq!(type_of_last_expression("declare let x: unknown;\nif (x === null) { x; }"), "null");
}

/// `narrowByEquality.types` pins native `isCoercibleUnderDoubleEquals`:
/// broad number/string/boolean comparands keep every coercible primitive
/// constituent, while unit comparands still narrow to that unit. The object
/// comparand control needs the corpus libs and remains in the native baseline;
/// the strict-literal control is
/// `an_equality_against_a_non_nullable_operand_narrows_nothing` above.
#[test]
fn loose_equality_filters_with_native_coercible_primitive_pairs() {
    for (comparand, expected) in [
        ("declare let n: number", "string | number | boolean"),
        ("declare let n: string", "string | number | boolean"),
        ("declare let n: boolean", "string | number | boolean"),
        ("const n = 1", "1"),
        ("const n = \"foo\"", "\"foo\""),
        ("const n = true", "true"),
    ] {
        assert_eq!(
            type_of_last_expression(&format!(
                "declare let x: number | string | boolean;\n{comparand};\nif (x == n) {{ x; }}"
            )),
            expected,
            "comparand: {comparand}"
        );
    }
}

#[test]
fn a_shadowed_undefined_is_not_the_literal() {
    // `undefined` is an identifier, not a keyword, so the operand is matched
    // against the synthesised global's **symbol** and not by name. A local
    // binding of the same name is an ordinary reference and lands in the
    // unported comparability branch, leaving the type alone.
    // A block rather than a function body, because this file's helper walks
    // blocks and `if`s and deliberately does not descend into functions.
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | undefined;\n{ let undefined: string = \"\"; if (x !== undefined) { x; } }"
        ),
        "string | undefined"
    );
}

/// Property-reference narrowing — `bd tsr-6ka`. Until this landed,
/// `is_matching_reference` compared resolved **symbols**, so no guard on
/// `a.b` narrowed anything and `check_property_access_expression` never
/// reached the flow walk at all.
///
/// Ground truth: `conformance/controlFlowOptionalChain.types:973` guards
/// `o?.foo !== undefined` on a `string | number | undefined` property, and
/// `conformance/controlFlowGenericTypes.types:566` is the union shape.
#[test]
fn a_guard_on_a_property_access_narrows_that_property() {
    assert_eq!(
        type_of_last_expression(
            "declare const o: { foo: string | undefined };\nif (o.foo !== undefined) { o.foo; }"
        ),
        "string"
    );
    // Truthiness through the same matcher — the arm that has been ported
    // longest and has been unreachable on this form the whole time.
    assert_eq!(
        type_of_last_expression(
            "declare const o: { foo: string | undefined };\nif (o.foo) { o.foo; }"
        ),
        "string"
    );
    // Nested receivers, so the match is recursive rather than one level.
    assert_eq!(
        type_of_last_expression(
            "declare const o: { a: { b: string | undefined } };\nif (o.a.b !== undefined) { o.a.b; }"
        ),
        "string"
    );
}

#[test]
fn a_guard_on_one_property_does_not_narrow_a_sibling() {
    // The failure mode this module's header calls the only way narrowing
    // produces a *wrong* answer rather than a gap: a match that is too loose.
    // Same receiver, different property.
    assert_eq!(
        type_of_last_expression(
            "declare const o: { a: string | undefined, b: string | undefined };\nif (o.a !== undefined) { o.b; }"
        ),
        "string | undefined",
        "a guard on `o.a` must leave `o.b` alone"
    );
    // Same property name, different receiver — the half that a name-only
    // comparison would get wrong, and the reason the receiver match recurses.
    assert_eq!(
        type_of_last_expression(
            "declare const x: { a: string | undefined };\ndeclare const y: { a: string | undefined };\nif (x.a !== undefined) { y.a; }"
        ),
        "string | undefined",
        "a guard on `x.a` must leave `y.a` alone"
    );
}

#[test]
fn an_element_access_matches_only_on_a_literal_argument() {
    // `a["b"]` names a property and matches `a.b`'s rule; `a[i]` does not,
    // because upstream matches it only when `i` is provably constant
    // (`isSymbolAssigned`), which this port cannot decide — so it refuses
    // rather than matching on text.
    assert_eq!(
        type_of_last_expression(
            "declare const o: { b: string | undefined };\nif (o[\"b\"] !== undefined) { o[\"b\"]; }"
        ),
        "string"
    );
    // FLIPPED at §423: the refusal's stated missing piece was
    // `isSymbolAssigned`, and a CONST needs no assignment analysis —
    // upstream's element arm matches on the same argument SYMBOL plus
    // `isConstantVariable`, whatever the symbol's type
    // (`typeGuardNarrowsIndexedAccessOfKnownProperty3/5/6` are the corpus
    // witnesses). A mutable index still refuses.
    assert_eq!(
        type_of_last_expression(
            "declare const o: { [k: string]: string | undefined };\ndeclare const i: string;\nif (o[i] !== undefined) { o[i]; }"
        ),
        "string",
        "a const index is a decidable reference"
    );
    assert_eq!(
        type_of_last_expression(
            "declare const o: { [k: string]: string | undefined };\nlet j: string = \"a\";\nif (o[j] !== undefined) { o[j]; }"
        ),
        "string | undefined",
        "a mutable index is not"
    );
}

#[test]
fn an_assignment_to_the_receiver_resets_the_property_narrowing() {
    // `conformance/destructuringControlFlow.ts:4` — the corpus case that
    // named this arm when the first run of `bd tsr-6ka` shipped without it.
    // The baseline records `string | undefined` for the inner `obj.a`, not
    // the narrowed `string`: assigning to `obj` invalidates everything
    // narrowed about `obj.a`, because it is a different object now.
    //
    // This is the *over-narrowing* direction — a confident wrong line rather
    // than a gap — which is the one failure mode this module is most able to
    // cause, so it is pinned in both spellings upstream records.
    assert_eq!(
        type_of_last_expression(
            "declare let obj: { a?: string };\nif (obj.a) { obj = {}; obj.a; }"
        ),
        "string | undefined"
    );
    assert_eq!(
        type_of_last_expression(
            "declare let obj: { a?: string };\nif (obj[\"a\"]) { obj = {}; obj[\"a\"]; }"
        ),
        "string | undefined"
    );
    // The control: with no intervening assignment the narrowing stands, so the
    // reset is about the assignment and not about property narrowing being
    // switched off.
    assert_eq!(
        type_of_last_expression("declare let obj: { a?: string };\nif (obj.a) { obj.a; }"),
        "string"
    );
}

/// `typeof` guards — `bd tsr-q9g`, `checker-notes-narrow.md` §6. Every
/// expectation is a `conformance/typeGuardOfFormTypeOf*.types` line:
///   if (typeof strOrNum === "string") { strOrNum; }   >strOrNum : string
///   if (typeof strOrNum !== "string") { strOrNum; }   >strOrNum : number
/// and from `typeGuardOfFormTypeOfBoolean` / `...Number` the same pattern for
/// the other primitives.
#[test]
fn a_typeof_guard_narrows_by_the_named_primitive() {
    assert_eq!(
        type_of_last_expression(
            "declare var strOrNum: string | number;\nif (typeof strOrNum === \"string\") { strOrNum; }"
        ),
        "string"
    );
    assert_eq!(
        type_of_last_expression(
            "declare var strOrNum: string | number;\nif (typeof strOrNum !== \"string\") { strOrNum; }"
        ),
        "number"
    );
    assert_eq!(
        type_of_last_expression(
            "declare var strOrBool: string | boolean;\nif (typeof strOrBool === \"boolean\") { strOrBool; }"
        ),
        "boolean"
    );
    // The operand orders are one rule (`narrowTypeByTypeof` flips on the
    // operator, not the sides).
    assert_eq!(
        type_of_last_expression(
            "declare var strOrNum: string | number;\nif (\"number\" === typeof strOrNum) { strOrNum; }"
        ),
        "number"
    );
}

/// `typeGuardTypeOfUndefined.types`:
///   if (typeof x === "undefined") { x; }  — on `boolean | undefined` shapes
/// the true branch keeps `undefined`, the false branch removes it.
#[test]
fn a_typeof_undefined_guard_splits_the_nullable() {
    assert_eq!(
        type_of_last_expression(
            "declare var b: boolean | undefined;\nif (typeof b === \"undefined\") { b; }"
        ),
        "undefined"
    );
    assert_eq!(
        type_of_last_expression(
            "declare var b: boolean | undefined;\nif (typeof b !== \"undefined\") { b; }"
        ),
        "boolean"
    );
}

/// The refused pair: a guard whose target the matcher does not match narrows
/// nothing, and a non-union reference under a FALSE typeof test collapses to
/// `never` only when the facts say it must — `typeGuardOfFormTypeOfString`:
///   if (typeof strOrNum === "string") {} else { strOrNum; }  >strOrNum : number
#[test]
fn the_else_branch_removes_the_named_primitive() {
    assert_eq!(
        type_of_last_expression(
            "declare var strOrNum: string | number;\nif (typeof strOrNum === \"string\") {} else { strOrNum; }"
        ),
        "number"
    );
}

/// `'p' in x` — `bd tsr-q9g` §6.1, `narrowTypeByInKeyword`'s known-property
/// half. The semantics are `conformance/controlFlowInOperator.types` (its
/// fixtures use computed names; the members here are plain, the shape is
/// identical):
///   declare const c: A | B;
///   if ('a' in c) { c; }   >c : A
#[test]
fn an_in_guard_filters_by_property_presence() {
    let source = "interface A { x: number }\ninterface B { y: string }\ndeclare const c: A | B;\nif (\"x\" in c) { c; }";
    assert_eq!(type_of_last_expression(source), "A");
    let source = "interface A { x: number }\ninterface B { y: string }\ndeclare const c: A | B;\nif (\"x\" in c) {} else { c; }";
    assert_eq!(type_of_last_expression(source), "B");
    // An unknown name narrows nothing on the true branch (the Record-
    // intersection half is upstream's own no-op when the global alias is
    // missing, which a lib-less fixture guarantees).
    let source = "interface A { x: number }\ninterface B { y: string }\ndeclare const c: A | B;\nif (\"z\" in c) { c; }";
    assert_eq!(type_of_last_expression(source), "A | B");
}

/// The pair the first run of the `in` arm lost 13 corpus lines to
/// (`compiler/strictOptionalProperties1.types`): the ELSE branch of a guard
/// over an OPTIONAL property keeps the object — absence is possible — where a
/// required property's else branch removes it.
///   if ('a' in obj) {} else { obj; }   >obj : { a?: string; ... }
#[test]
fn an_in_guard_over_an_optional_property_keeps_the_else_branch() {
    let source = "interface A { x?: number }\ndeclare const c: A;\nif (\"x\" in c) {} else { c; }";
    assert_eq!(type_of_last_expression(source), "A");
}

/// `typeof x === "object"` builds `object | null` — and leaves the any-flagged
/// types alone.
///
/// `narrowTypeByTypeName` (`flow.go:670`) guards this arm with
/// `t.flags&TypeFlagsAny != 0`, a **flags** test. This port compared identity
/// against `anyType`, which excludes `errorType` — the type it answers for every
/// gap — so an unresolved receiver came out of a `typeof` guard as
/// `object | null` and the next access reported a `null` no program contains.
/// Three real-repo reports; `real_repo_regressions.rs` holds the diagnostic
/// half. The assertions here are the type half, because a diagnostic test
/// cannot tell "narrowed correctly" from "not narrowed at all" — measured: the
/// mutation that returns `t` for *every* type reddens nothing over there.
#[test]
fn typeof_object_keeps_null_and_drops_the_primitives() {
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | { a: number } | null;\nif (typeof x === \"object\") { x; }\n"
        ),
        "{ a: number; } | null"
    );
    // With the truthiness guard in front, the `null` is gone.
    assert_eq!(
        type_of_last_expression(
            "declare const x: string | { a: number } | null;\nif (x && typeof x === \"object\") { x; }\n"
        ),
        "{ a: number; }"
    );
    // `any` is returned unchanged — upstream's guard, and the arm the identity
    // comparison did cover.
    assert_eq!(
        type_of_last_expression("declare const x: any;\nif (typeof x === \"object\") { x; }\n"),
        "any"
    );
    // And `errorType` — what the flags test admits and the identity test did
    // not. It prints `error` through this helper (`type_to_string`; the `any`
    // spelling of ADR-0038 is the *baseline* renderer's), and the assertion
    // that matters is that it is **not** `object | null`.
    assert_eq!(
        type_of_last_expression(
            "declare const x: Unresolved | null;\nif (typeof x === \"object\") { x; }\n"
        ),
        "error"
    );
}

/// §231. `typeof x === "function"` narrows — the arm existed and had never run.
///
/// `narrowTypeByTypeName`'s function arm reads `c.globalFunctionType`, and this
/// port fetched it with `global_type_symbol("Function")`. That convenience
/// wrapper is `global_type_symbol_with_arity(name, 1)`, while `interface
/// Function` (`es5.d.ts:257`) takes **no** type parameters — so the lookup
/// answered `None` for every program and the arm's decline was unconditional.
/// The comment beside it described that decline as the lib-less case; it was
/// true of nothing.
///
/// Worth **+17 lines across four cases and zero cases**, which is why it needed
/// looking at rather than dismissing: `narrowingByTypeofInSwitch` (+7),
/// `typeGuardConstructorClassAndNumber` (+5), `typeGuardOfFormTypeOfFunction`
/// (+4), `narrowingTypeofFunction` (+1) all carry other blockers. Conventions
/// corollary 24 as `checker-2`'s §250 sharpened it: a per-case tally
/// understates as readily as it hides.
#[test]
fn typeof_function_narrows_a_union_to_its_callable_member() {
    // Anonymous callables now satisfy Function structurally, so the strict
    // subtype branch preserves the callable without an extra intersection.
    assert_eq!(
        type_of_last_expression(
            "interface Function {}\n\
             declare var x: string | (() => void);\n\
             if (typeof x === \"function\") { x; }"
        ),
        "() => void"
    );
}

/// An ALIASED function type is still a structured callable. `type L` prints as
/// `L`, so its `Anonymous` record says `signature: false` (the printed node
/// kind), and the relater used that print flag as its "has members" test:
/// `L -> Function` was never compared structurally, the strict-subtype branch
/// of `narrowTypeByTypeFacts` (`flow.go`) missed, and the intersection arm
/// answered `L & Function` where `narrowingByTypeofInSwitch.types` records `L`.
/// The generic form (`X extends L`) relates through its constraint the same way.
#[test]
fn typeof_function_keeps_an_aliased_callable_without_an_intersection() {
    assert_eq!(
        type_of_last_expression(
            "interface Function {}\n\
             type L = (x: number) => string;\n\
             type R = { x: string };\n\
             declare var x: L | R;\n\
             if (typeof x === \"function\") { x; }"
        ),
        "L"
    );
}

/// The lib's `Function` interface declares no call signatures, yet native
/// gives it `FunctionStrictFacts`: `isFunctionObjectType` (checker.go:31140)
/// also accepts a type with a `bind` member that is a subtype of the global
/// `Function`. Without that half, a `typeof x !== "function"` branch (and a
/// switch's default clause) kept `Function`, as in
/// `narrowingByTypeofInSwitch.types`' `string | object | undefined` rows.
#[test]
fn typeof_not_function_removes_the_function_interface() {
    assert_eq!(
        type_of_last_expression(
            "interface Function { bind(this: Function, thisArg: any): any; }\n\
             declare var x: string | Function;\n\
             if (typeof x !== \"function\") { x; }"
        ),
        "string"
    );
}

/// The control, and it is the one that would have caught the original defect:
/// with **no** `Function` in scope the decline is correct, so a fixture that
/// only tests the narrowing cannot tell "declines when it should" from
/// "declines always". Both assertions together can.
#[test]
fn typeof_function_still_declines_when_the_program_has_no_function_interface() {
    assert_eq!(
        type_of_last_expression(
            "declare var x: string | (() => void);\n\
             if (typeof x === \"function\") { x; }"
        ),
        "string | (() => void)"
    );
}

/// §612's probe, made executable.
///
/// `compiler/incrementAndDecrement` writes `var e = E.B;` and records `>e : E`
/// at every later reference. This port prints `E.B` there — 8 lines, and they
/// are that case's entire deficit.
///
/// Three mechanisms are ruled out in `checker-notes-enums.md` §610–§612: the
/// fresh/regular twin at the access site, the initial flow type, and the
/// structure of `get_assignment_reduced_type` (which already carries upstream's
/// give-up path). What is left is the single assignability query that CHOOSES
/// that path — `isTypeAssignableTo(assignedType, reducedType)` with a FRESH
/// enum literal against its REGULAR twin (`flow.go:2429`).
///
/// **§613 sharpens the target, and the fixture is why.** All eight of
/// `incrementAndDecrement`'s wrong positions are `e++`, `e--`, `++e`, `--e` —
/// **every one an assignment TARGET**, and the file contains no plain read of
/// `e` at all. So the hypothesis is not "this port narrows enum initialisers
/// wrongly" but the narrower **"this port narrows a reference that is being
/// WRITTEN"**, where upstream answers the declared type because narrowing a
/// write target is meaningless.
///
/// That reframing matters: the assertion below is a plain READ, and this port's
/// `E.B` there may well be correct — upstream's baseline does not contain the
/// case, so it is pinned as CURRENT BEHAVIOUR OF UNKNOWN CORRECTNESS rather
/// than as a known defect. The known defect is the write-target one, and it
/// needs a fixture with an increment, which this harness cannot type (the
/// operand of `++` is not an expression statement).
#[test]
fn an_enum_initialised_var_narrows_to_the_member_at_a_read() {
    assert_eq!(
        type_of_last_expression("enum E { A, B, C }\nvar e = E.B;\ne;"),
        "E.B",
        "pinned as current behaviour; upstream's answer at a plain READ is not \
         in the corpus. The MEASURED defect is at a write target — see \
         checker-notes-enums.md §613"
    );
}

/// §744: upstream's `unreachableNeverType` sentinel. A `never`-returning
/// call cuts its path off; at a JOIN the cut path contributes nothing (so the
/// guard's other arm survives), while a read that is itself unreachable
/// answers the declared type (`flow.go:111`).
#[test]
fn a_never_call_drops_its_path_at_a_join_and_reads_declared_when_unreachable() {
    assert_eq!(
        type_of_last_expression(
            "declare function fail(m?: string): never;\ndeclare let x: string | undefined;\nif (x === undefined) fail();\nx;"
        ),
        "string"
    );
    // §128's observable, kept: the read AFTER the call is unreachable and
    // prints the declared type, not `never`.
    assert_eq!(
        type_of_last_expression(
            "declare function fail(m?: string): never;\ndeclare let x: string | undefined;\nfail();\nx;"
        ),
        "string | undefined"
    );
}

/// §752: the equality arm reaches discriminant narrowing through
/// `getDiscriminantPropertyAccess`/`narrowTypeByDiscriminant` (`flow.go:496`)
/// rather than through §51.1's inline property-access test.
///
/// The inline form matched a `PropertyAccessExpression` only, so `u["kind"]`
/// narrowed nothing. `getAccessedPropertyName` (`flow.go:1727`) accepts the
/// ELEMENT access with a string-literal argument too, which is what
/// `compiler/discriminantElementAccessCheck` wants.
///
/// Reddened by: restoring the inline `access_pair` test.
#[test]
fn a_discriminant_equality_narrows_through_a_property_or_element_access() {
    let union = "type A = { kind: \"a\"; a: string };\n\
                 type B = { kind: \"b\"; b: number };\n\
                 declare let u: A | B;\n";
    // The property-access road §51.1 already had — a regression guard on the
    // swap, not a new capability.
    assert_eq!(type_of_last_expression(&format!("{union}if (u.kind === \"a\") {{ u; }}")), "A");
    // The element-access road, which the inline form could not reach.
    assert_eq!(
        type_of_last_expression(&format!("{union}if (u[\"kind\"] === \"a\") {{ u; }}")),
        "A"
    );
    // The negative branch drops the matched constituent, both ways round.
    assert_eq!(type_of_last_expression(&format!("{union}if (u.kind !== \"a\") {{ u; }}")), "B");
    // The reference may sit on either side of the operator (upstream's
    // `leftAccess` then `rightAccess`).
    assert_eq!(type_of_last_expression(&format!("{union}if (\"b\" === u.kind) {{ u; }}")), "B");
}

/// §753: the DISCRIMINANT half of `narrowTypeByTypeof` (`flow.go:624`-`:629`),
/// the third and last of that function's three halves to be ported.
///
/// `typeof u.kind === "string"` discriminates a union whose `kind` types are
/// literals in different typeof domains — `compiler/narrowingTypeofDiscriminant`'s
/// `f1`. Its `f2` is the chain form, which needs upstream's ASSIGN-and-fall-through
/// at `flow.go:622` rather than the early return this port had: the nullish
/// removal has to COMPOSE with the discriminant filter.
///
/// Reddened by: returning `t` after the chain strip instead of assigning.
#[test]
fn a_typeof_on_a_discriminant_property_narrows_the_union() {
    let union = "type A = { kind: \"a\"; data: string };\n\
                 type B = { kind: 1; data: number };\n";
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}declare let u: A | B;\nif (typeof u.kind === \"string\") {{ u; }}"
        )),
        "A"
    );
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}declare let u: A | B;\nif (typeof u.kind === \"number\") {{ u; }}"
        )),
        "B"
    );
    // The chain form: `undefined` is removed by the containment strip AND the
    // union is filtered by the discriminant, in one branch.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}declare let u: A | B | undefined;\nif (typeof u?.kind === \"string\") {{ u; }}"
        )),
        "A"
    );
}

/// §754: the SWITCH arm swapped onto the discriminant pair
/// (`flow.go:1083`-`:1086`), the last of §4.-4's five.
///
/// `switch (u.kind)` narrowed through §51's inline property-access test
/// before; the pair also takes an ELEMENT access, and — because upstream's
/// default arm ASSIGNS the optional-chain containment and falls through
/// rather than returning — composes the nullish strip WITH the
/// discrimination.
///
/// Reddened by: restoring §51's inline block, or returning after the
/// containment strip instead of assigning.
#[test]
fn a_switch_on_a_discriminant_property_narrows_the_union() {
    let union = "type A = { kind: \"a\"; a: string };\n\
                 type B = { kind: \"b\"; b: number };\n";
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}declare let u: A | B;\nswitch (u.kind) {{ case \"a\": u; }}"
        )),
        "A"
    );
    // The element-access road, which §51's `PropertyAccessExpression`-only
    // test could not reach.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}declare let u: A | B;\nswitch (u[\"kind\"]) {{ case \"b\": u; }}"
        )),
        "B"
    );
    // The chain form: the clause range excludes `undefined`, so the base
    // strips it AND the union is discriminated, in one clause.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}declare let u: A | B | undefined;\nswitch (u?.kind) {{ case \"a\": u; }}"
        )),
        "A"
    );
}

/// §755: `getCandidateDiscriminantPropertyAccess`'s alias arm
/// (`flow.go:1473`-`:1482`) — given `const k = u.kind`, `k` narrows `u`.
///
/// The candidate returned is the INITIALIZER, so everything downstream sees
/// an ordinary access. The annotation check is the soundness half: a
/// declaration with a type annotation is typed by that annotation, not by the
/// access, so it must NOT alias.
///
/// Reddened by: dropping the identifier arm, or dropping the
/// `declaration.r#type.is_some()` guard (which reddens the third assertion).
#[test]
fn a_const_alias_of_a_discriminant_narrows_through_the_alias() {
    let union = "type A = { kind: \"a\"; a: string };\n\
                 type B = { kind: \"b\"; b: number };\n\
                 declare let u: A | B;\n";
    assert_eq!(
        type_of_last_expression(&format!("{union}const k = u.kind;\nif (k === \"a\") {{ u; }}")),
        "A"
    );
    // The switch arm reaches the same alias, through the same candidate.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}const k = u.kind;\nswitch (k) {{ case \"b\": u; }}"
        )),
        "B"
    );
    // ANNOTATED: `k` is typed by the annotation rather than by the access, so
    // upstream's `getCandidateVariableDeclarationInitializer` declines and the
    // union is left whole.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}const k: \"a\" | \"b\" = u.kind;\nif (k === \"a\") {{ u; }}"
        )),
        "A | B"
    );
    // A `let` alias is not constant, so it is not an alias either.
    assert_eq!(
        type_of_last_expression(&format!("{union}let k = u.kind;\nif (k === \"a\") {{ u; }}")),
        "A | B"
    );
}

/// §756: the destructuring alias arm (`flow.go:1483`-`:1489`) — given
/// `const { kind } = u`, `kind` narrows `u`.
///
/// The candidate returned is the BINDING ELEMENT, so this half also needs
/// `getAccessedPropertyName`'s binding-element arm
/// (`getDestructuringPropertyName`, `flow.go:1792`), whose name is
/// `PropertyNameOrName()` — the shorthand and the renamed form answer the
/// same property.
///
/// It also needs `isConstantVariable` to see the CONST through a binding
/// pattern, which is §756's other half: `getCombinedNodeFlags` starts at
/// `GetRootDeclaration`, and this port had walked one parent.
///
/// Reddened by: dropping the root-declaration walk from
/// `combined_node_flags` (the whole test), or the binding-element arm of
/// `get_accessed_property_name`.
#[test]
fn a_destructured_alias_of_a_discriminant_narrows_through_the_alias() {
    let union = "type A = { kind: \"a\"; a: string };\n\
                 type B = { kind: \"b\"; b: number };\n\
                 declare let u: A | B;\n";
    // Shorthand: `PropertyNameOrName()` falls back to the name.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}const {{ kind }} = u;\nif (kind === \"a\") {{ u; }}"
        )),
        "A"
    );
    // Renamed: the PROPERTY name is what discriminates, not the local.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}const {{ kind: k }} = u;\nif (k === \"b\") {{ u; }}"
        )),
        "B"
    );
    // The switch arm reaches it through the same candidate.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}const {{ kind }} = u;\nswitch (kind) {{ case \"a\": u; }}"
        )),
        "A"
    );
    // A `let` destructuring is not constant, so it is not an alias.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}let {{ kind }} = u;\nif (kind === \"a\") {{ u; }}"
        )),
        "A | B"
    );
}

/// §758: `narrowTypeByOptionality` (`flow.go:415`), reached from
/// `narrowType`'s pre-dispatch branch (`:378`-`:381`). For `a?.b` and for
/// `a ?? b`'s left operand, upstream emulates a synthetic
/// `a !== null && a !== undefined` condition rather than a truthiness one.
///
/// **This test does NOT redden when the branch is removed, and it is not
/// claimed to.** It pins the chain-root answers the branch must preserve —
/// SS151/SS152 already reach those through the chain-containment strip, so
/// the branch is a fidelity change there, not a behaviour change.
///
/// The branch's ONE observable effect on this corpus is the narrowing of a
/// `??` left operand inside the RIGHT operand, to the NULLISH part rather
/// than the falsy part (`conformance/nullishCoalescingOperator11`, 1 line).
/// This harness cannot see it: the effect is on a sub-expression, and
/// `type_of_last_expression` types the whole statement. Making it visible
/// needs overload resolution the port answers `error` for today. **That case
/// is the regression guard, not this test.**
#[test]
fn a_nullish_operand_narrows_by_presence_and_not_by_truth() {
    assert_eq!(
        type_of_last_expression("declare let o: { a: string } | undefined;\nif (o?.a) { o; }"),
        "{ a: string; }"
    );
    assert_eq!(
        type_of_last_expression("declare let s: string | undefined;\ns ?? \"d\";\ns;"),
        "string | undefined"
    );
}

/// Native getGlobalNonNullableTypeInstantiation falls back to T & {} when
/// no `NonNullable` alias is declared (checker.go:31207). This harness has no lib.
#[test]
fn without_a_global_alias_a_non_null_fact_uses_an_intersection() {
    assert_eq!(
        type_of_last_expression(
            "function g<T extends { x: string } | undefined>(obj: T) {\n\
             if (obj != null) { obj; }\n}"
        ),
        "T & {}"
    );
}

/// §759: `narrowTypeByBooleanComparison` (`flow.go:806`), reached from the
/// equality dispatch's last two arms (`:510`-`:515`).
///
/// `isA(x) === true` re-enters `narrowType` on the NON-boolean operand with
/// the assumption folded in, so a condition that narrows on its own — here a
/// type-predicate call — keeps narrowing when it is compared to a boolean
/// literal. The fold is a three-way XOR of the assumption, the literal's
/// polarity and the operator's negation; all four combinations are asserted,
/// because getting one of them backwards is this arm's failure mode.
///
/// The operand must not itself be the matching reference — `x === true` is
/// answered by `narrowTypeByEquality` at `flow.go:483`, before this arm, and
/// filters by comparability instead. That is upstream's order and the port's.
///
/// Reddened by: any single sign flip in the fold.
#[test]
fn a_comparison_against_a_boolean_literal_narrows_the_other_operand() {
    // `compiler/narrowByBooleanComparison`, reduced to two constituents.
    let d = "type A = { type: \"A\" };\n\
             type B = { type: \"B\" };\n\
             declare function isA(x: A | B): x is A;\n\
             declare let x: A | B;\n";
    assert_eq!(type_of_last_expression(&format!("{d}if (isA(x) === true) {{ x; }}")), "A");
    assert_eq!(type_of_last_expression(&format!("{d}if (isA(x) === false) {{ x; }}")), "B");
    assert_eq!(type_of_last_expression(&format!("{d}if (isA(x) !== true) {{ x; }}")), "B");
    assert_eq!(type_of_last_expression(&format!("{d}if (isA(x) !== false) {{ x; }}")), "A");
    // Loose operators fold the same way.
    assert_eq!(type_of_last_expression(&format!("{d}if (isA(x) != true) {{ x; }}")), "B");
    // The boolean may sit on either side.
    assert_eq!(type_of_last_expression(&format!("{d}if (true === isA(x)) {{ x; }}")), "A");
}

/// §760: `narrowTypeByConstructor` (`flow.go:760`), dispatched at `:504`.
///
/// Only the CLASS road is asserted here. The primitive road needs `String`,
/// `Number` and friends from the lib, and this harness builds no program — the
/// same limitation §751 recorded for `let a: E.A`. The corpus carries the
/// primitive road (`typeGuardConstructorPrimitiveTypes`,
/// `typeGuardConstructorNarrowAny`).
#[test]
fn a_constructor_comparison_narrows_the_union() {
    assert_eq!(
        type_of_last_expression(
            "class C1 { p!: string }\ndeclare let x: C1 | number;\n\
             if (x.constructor === C1) { x; }"
        ),
        "C1"
    );
    // The element-access spelling is the same reference.
    assert_eq!(
        type_of_last_expression(
            "class C1 { p!: string }\ndeclare let x: C1 | number;\n\
             if (x[\"constructor\"] === C1) { x; }"
        ),
        "C1"
    );
    // INEQUALITY does not narrow: `x.constructor !== C1` does not prove the
    // constituent is not a subclass (`flow.go:762`).
    assert_eq!(
        type_of_last_expression(
            "class C1 { p!: string }\ndeclare let x: C1 | number;\n\
             if (x.constructor !== C1) { x; }"
        ),
        "number | C1"
    );
}

/// §761 (`flow.go:1077`-`:1080`), §754's residue: the SECOND switch
/// optional-chain containment variant.
///
/// `switch (typeof o?.x)` proves the chain defined when no clause in the range
/// is the string `"undefined"`. The clause check differs from the direct
/// form's, which looks for a nullish clause TYPE — here every clause type is a
/// string literal, so a nullish test would never fire and the base would never
/// be stripped.
///
/// Reddened by: removing the `else if` variant, or reusing
/// `switch_clause_range_covers_nullish` for it.
#[test]
fn a_switch_on_typeof_a_chain_strips_the_base() {
    let d = "declare let o: { x: string } | undefined;\n";
    assert_eq!(
        type_of_last_expression(&format!("{d}switch (typeof o?.x) {{ case \"string\": o; }}")),
        "{ x: string; }"
    );
    // A clause that IS `"undefined"` proves nothing, so the base survives.
    assert_eq!(
        type_of_last_expression(&format!("{d}switch (typeof o?.x) {{ case \"undefined\": o; }}")),
        "{ x: string; } | undefined"
    );
}

/// §764: two of `getNarrowedTypeOfSymbol`'s entry guards
/// (`checker.go:13772`, `:13775`) that §762 left unported.
///
/// The `isSomeSymbolAssigned` guard moved ZERO lines on the corpus and is
/// kept for SOUNDNESS: once any symbol the parameter's pattern binds is
/// reassigned, the siblings stop being projections of a single parent value,
/// so discriminating them against each other would be a WRONG answer rather
/// than a missing one. This test is the only thing that pins it.
///
/// Note the guard asks about EVERY symbol the root's name binds, not the one
/// being read — assigning `kind` is what withdraws the narrowing for `v`.
///
/// Reddened by: removing the guard.
#[test]
fn a_reassigned_destructured_parameter_does_not_discriminate() {
    let union = "type A = { kind: \"a\"; v: string };\n\
                 type B = { kind: \"b\"; v: number };\n";
    // The control: an un-reassigned destructured parameter DOES discriminate.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}function f({{ kind, v }}: A | B) {{ if (kind === \"a\") {{ v; }} }}"
        )),
        "string"
    );
    // Assigning a SIBLING withdraws the pseudo-reference: `v` keeps its whole
    // declared type rather than being discriminated by `kind`.
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}function f({{ kind, v }}: A | B) {{ kind = \"b\"; \
             if (kind === \"a\") {{ v; }} }}"
        )),
        "string | number"
    );
}

/// §765 (`checker.go:13752`/`:13761`): the const-like test is on the ROOT
/// declaration, reached by `GetRootDeclaration`, not on the pattern's
/// immediate holder.
///
/// For a NESTED pattern the immediate holder is a `BindingElement`, which is
/// neither a parameter nor a variable declaration, so a one-hop test declined
/// and the whole pseudo-reference road was unreachable for nested
/// destructuring. This moved ZERO on the corpus — it holds no nested
/// discriminated destructuring — so this test is the only thing that pins it.
///
/// Reddened by: testing `holder` instead of `root`.
#[test]
fn a_nested_destructuring_still_reaches_the_discriminant_road() {
    let union = "type A = { p: { kind: \"a\"; v: string } };\n\
                 type B = { p: { kind: \"b\"; v: number } };\n\
                 declare let x: A | B;\n";
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}const {{ p: {{ kind, v }} }} = x;\nif (kind === \"a\") {{ v; }}"
        )),
        "string"
    );
}

/// §766 (`checker.go:13768`): `mapType(parentType, getBaseConstraintOrType)`.
///
/// The union test at the pseudo-reference entry is on the CONSTRAINT, not on
/// the written type. A destructured parameter typed `T extends A | B` has a
/// written type that is a TYPE PARAMETER, so testing it declined every
/// generic destructuring.
///
/// Reddened by: testing `parent_type` instead of `parent_constraint`.
#[test]
fn a_generic_destructured_parameter_discriminates_through_its_constraint() {
    let union = "type A = { kind: \"a\"; v: string };\n\
                 type B = { kind: \"b\"; v: number };\n";
    assert_eq!(
        type_of_last_expression(&format!(
            "{union}function f<T extends A | B>({{ kind, v }}: T) \
             {{ if (kind === \"a\") {{ v; }} }}"
        )),
        "string"
    );
}

/// §779: an AMBIENT `declare var` with no annotation is `any`, not the auto
/// road's initial `undefined`.
///
/// Upstream's `autoType` IS `any` (`checker.go:976`), and a reference whose
/// flow type is still auto converts through `convertAutoToAny`
/// (`checker.go:11182`). A `declare var` can never be assigned, so its flow
/// type never leaves auto — where a non-ambient `let x;` read BEFORE its first
/// assignment genuinely is `undefined`, which is the distinction the auto road
/// exists to make and which the test below it pins.
///
/// Reddened by: removing the ambient guard from `is_auto_typed_declaration`.
#[test]
fn an_ambient_var_with_no_annotation_is_any() {
    assert_eq!(type_of_last_expression("declare var a;\na;"), "any");
    // The non-ambient road is unchanged: still `undefined` before assignment.
    assert_eq!(type_of_last_expression("let b;\nb;"), "undefined");
    // And still evolving after one.
    assert_eq!(type_of_last_expression("let c;\nc = 1;\nc;"), "number");
}

#[test]
fn unknown_switch_uses_ground_clause_values_and_object_identity() {
    // Pinned flow.go:1102 maps object cases to nonPrimitive rather than to
    // the compared object's specific members or callable signature.
    for (prefix, clauses, expected) in [
        ("", "case 13:", "13"),
        ("", "case \"west\":", "\"west\""),
        ("", "case true:", "true"),
        ("", "case null:", "null"),
        ("", "case undefined:", "undefined"),
        ("", "case 13: case \"west\":", "\"west\" | 13"),
        ("enum Mode { Words = 'words', Counts = 'counts' }", "case Mode.Counts:", "Mode.Counts"),
        ("declare const token: unique symbol;", "case token:", "unique symbol"),
        ("declare const callback: () => void;", "case callback:", "object"),
        ("declare const value: { property: number };", "case value:", "object"),
        ("declare const value: object;", "case value:", "object"),
    ] {
        assert_eq!(
            type_of_last_expression(&format!(
                "{prefix} declare let x: unknown; switch (x) {{ {clauses} x; }}"
            )),
            expected,
            "{prefix} {clauses}",
        );
    }
}

#[test]
fn unknown_switch_defaults_and_ungrounded_cases_keep_unknown() {
    for source in [
        "declare let x: unknown; switch (x) { case 13: default: x; }",
        "declare let x: unknown; switch (x) { case 13: break; } x;",
        "declare let x: unknown; declare const dynamic: any; switch (x) { case 13: case dynamic: x; }",
        "function f<T>(x: unknown, generic: T) { switch (x) { case 'west': case generic: x; } }",
    ] {
        assert_eq!(type_of_last_expression(source), "unknown", "{source}");
    }
}

#[test]
fn named_union_equality_projects_members_and_surviving_alias_origins() {
    // Pinned flow.go:555/filterType: aliases do not prevent equality
    // narrowing. Keep a complete nested Word origin, but decompose a
    // partially surviving Route. Unchanged reads keep their original alias.
    let declarations = "type Word = 'east' | 'west'; type Route = Word | 13;";
    for (body, expected) in [
        ("if (value === 'west') { value; }", "\"west\""),
        ("if (value === 'west') {} else { value; }", "\"east\" | 13"),
        ("if (value !== 13) { value; }", "Word"),
        ("if (value === 13) { value; }", "13"),
        ("if (value === 'west') {} value;", "Route"),
    ] {
        assert_eq!(
            type_of_last_expression(&format!(
                "{declarations} function f(value: Route) {{ {body} }}"
            )),
            expected,
            "{body}",
        );
    }
    for operator in ["==", "==="] {
        assert_eq!(
            type_of_last_expression(&format!(
                "type Wide = string | number; function f(value: Wide) {{
                    if (value {operator} 13) {{ value; }}
                 }}"
            )),
            "13",
            "{operator}",
        );
    }
    assert_eq!(
        type_of_last_expression(
            "type Wide = string | number; function f(value: Wide) {
                if (value == 13) {} else { value; }
             }"
        ),
        "Wide",
    );
}

#[test]
fn named_union_equality_keeps_branded_members_and_distinct_receivers() {
    let declarations =
        "type Right = 'right' & { readonly side: 'right' }; type Either = 'left' | Right;";
    for (body, expected) in [
        ("if (value === 'left') { value; }", "\"left\""),
        ("if (value === 'left') {} else if (value === 'right') { value; }", "Right"),
        ("if (value === 'left') {} else if (value === 'right') {} else { value; }", "never"),
    ] {
        assert_eq!(
            type_of_last_expression(&format!(
                "{declarations} function f(value: Either) {{ {body} }}"
            )),
            expected,
            "{body}",
        );
    }
    assert_eq!(
        type_of_last_expression(
            "type Fields = { kind: 'east' } | { kind: 'west' };
             type Route = 'east' | 'west' | 13;
             function f(value: Fields, other: Route) {
                if (other === 'east') { value; }
             }"
        ),
        "Fields",
    );
}

#[test]
fn typed_nullable_equality_operands_use_strict_and_loose_facts() {
    // Pinned 5b1047d typedNullableEqualityWave47: flags of the computed
    // comparand select nullable facts, even when its syntax is not a literal.
    let declarations = "const nullValue: null = null; const undefinedValue: undefined = undefined;
         type Empty = null; declare const aliasedNull: Empty;";
    for (comparison, yes, no) in [
        ("value === nullValue", "null", "number | undefined"),
        ("nullValue !== value", "number | undefined", "null"),
        ("value === aliasedNull", "null", "number | undefined"),
        ("value === undefinedValue", "undefined", "number | null"),
        ("undefinedValue !== value", "number | null", "undefined"),
        ("value == nullValue", "null | undefined", "number"),
        ("nullValue != value", "number", "null | undefined"),
        ("value == undefinedValue", "null | undefined", "number"),
    ] {
        for (branch, expected) in [("{ value; }", yes), ("{} else { value; }", no)] {
            let source = format!(
                "{declarations} function f(value: number | null | undefined) {{
                     if ({comparison}) {branch}
                 }}"
            );
            assert_eq!(type_of_last_expression(&source), expected, "{comparison} {branch}");
            assert_eq!(
                type_of_last_expression_with_null_checks(&source, "test.ts", false),
                "number",
                "loose mode: {comparison} {branch}",
            );
        }
    }
    for (body, expected) in [
        ("if (value.item === nullValue) { value.item; }", "null"),
        ("if (undefinedValue !== value.item) { value.item; }", "number | null"),
    ] {
        assert_eq!(
            type_of_last_expression(&format!(
                "{declarations} function f(value: {{ item: number | null | undefined }}) {{
                     {body}
                 }}"
            )),
            expected,
            "{body}",
        );
    }
}

#[test]
fn nullable_unions_and_any_do_not_take_scalar_nullable_facts() {
    for (comparison, yes) in [
        ("value === nullOrUndefined", "null | undefined"),
        ("value == nullOrUndefined", "null | undefined"),
        ("value === nullOrNumber", "number | null"),
    ] {
        for (branch, expected) in
            [("{ value; }", yes), ("{} else { value; }", "number | null | undefined")]
        {
            assert_eq!(
                type_of_last_expression(&format!(
                    "declare const nullOrUndefined: null | undefined;
                     declare const nullOrNumber: null | number;
                     function f(value: number | null | undefined) {{
                         if ({comparison}) {branch}
                     }}"
                )),
                expected,
                "{comparison} {branch}",
            );
        }
    }
    assert_eq!(
        type_of_last_expression(
            "const nullValue: null = null;
             function f(value: any) { if (value === nullValue) { value; } }"
        ),
        "any",
    );
    assert_eq!(
        type_of_last_expression(
            "function f(undefined: string, value: string | null) {
                 if (value === undefined) { value; }
             }"
        ),
        "string",
    );
}

#[test]
fn javascript_loop_flow_keeps_assigned_error_and_completed_numeric_writes() {
    // Pinned 5b1047d jsErrorFlowWave47: the loop worker has no JS-file
    // exclusion. The assignment's error TypeId is not ordinary any, and a
    // later complete assignment must still replace it rather than stick.
    for (body, expected) in [
        (
            "var v, T, h = 0; v = missing, T = v;
             for (; h < 4; h = h + 1) {} T;",
            "error",
        ),
        (
            "var v, T, h = 0; v = missing, T = v;
             for (; h < 4; h = h + 1) {} T = 17; T;",
            "number",
        ),
        (
            "var v, T; v = missing, T = v;
             (function() { var h = 0; for (; h < 4; h = h + 1) {} })(); T;",
            "error",
        ),
    ] {
        assert_eq!(
            type_of_last_expression_in_file(&format!("function f() {{ {body} }}"), "test.js"),
            expected,
            "{body}",
        );
    }
}
