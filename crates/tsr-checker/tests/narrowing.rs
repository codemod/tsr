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
    // gives the answer it gave before narrowing existed. This fixture used a
    // `typeof` guard as its stand-in until `bd tsr-q9g` landed that arm and
    // it came due — the third such expiry in one session, which is exactly why
    // the conventions require the PAIR. The stand-in is now **comparability**
    // (`x === "a"` needs `areTypesComparable`, unported, `bd tsr-97d`), and
    // the ported half of the pair is asserted beside it in
    // `a_typeof_guard_narrows_by_the_named_primitive`.
    assert_eq!(
        type_of_last_expression("let x: string | undefined;\nif (x === \"a\") { x; }"),
        "string | undefined"
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
    // The comparability branch of `narrowTypeByEquality` is not ported — it
    // needs `areTypesComparable`. `x === "a"` therefore leaves the declared
    // type, which is `narrow_type`'s standing default rather than a new
    // guess, and this test is what fails if someone routes a non-nullable
    // operand into the facts filter: `string | undefined` has no constituent
    // carrying a `"a"`-comparability fact, so it would collapse to `never`.
    assert_eq!(
        type_of_last_expression("declare const x: string | undefined;\nif (x === \"a\") { x; }"),
        "string | undefined"
    );
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
    assert_eq!(
        type_of_last_expression(
            "declare const o: { [k: string]: string | undefined };\ndeclare const i: string;\nif (o[i] !== undefined) { o[i]; }"
        ),
        "string | undefined",
        "a non-literal index is not a decidable reference"
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
