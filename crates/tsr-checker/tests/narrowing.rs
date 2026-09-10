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
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
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
    // **The intersection is the residue, and it is pinned rather than wished
    // away.** Upstream answers `() => void`; this port answers
    // `() => void & Function`, and the reason is one rung up the ladder rather
    // than in this arm. `narrow_type_by_type_facts` is a faithful
    // transcription of `narrowTypeByTypeFacts` (`flow.go:685`), whose first
    // rung is `isTypeRelatedTo(t, impliedType, strictSubtypeRelation)`.
    // Upstream answers **true** for a function type against `Function`,
    // because an object type carrying call signatures resolves its members
    // with the global `Function` as an implicit base; this port has no such
    // step, the first rung fails, and the third rung intersects instead.
    //
    // So the residue is a MEMBERS gap, independent of §231 and present before
    // it. Fixing that rung turns this assertion into upstream's answer and
    // turns §231's one adverse line into zero — see the commit for the 18:1
    // split.
    assert_eq!(
        type_of_last_expression(
            "interface Function {}\n\
             declare var x: string | (() => void);\n\
             if (typeof x === \"function\") { x; }"
        ),
        // §594 CORRECTED the PARENTHESES here, not the residue. A function type
        // is below `Intersection` on the precedence ladder, so upstream prints
        // `(() => void) & Function`; this port omitted the parentheses on the
        // intersection road and the assertion pinned that. The MEMBERS gap this
        // test is really about — the first rung of `narrowTypeByTypeFacts`
        // failing, so the third rung intersects instead of narrowing — is
        // unchanged and still the reason an intersection is printed at all.
        "(() => void) & Function"
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

/// §758's other half: `getAdjustedTypeWithFacts` (`checker.go:31159`) maps
/// surviving constituents through `getGlobalNonNullableTypeInstantiation` for
/// **`NEUndefinedOrNull` as well as `Truthy`**. §85 had spelled the utility
/// for `Truthy` only, so a type parameter under a non-null fact printed
/// `T & {}` where upstream prints `NonNullable<T>`.
///
/// Reddened by: restoring `NonNullKind::Both` for a `TYPE_PARAMETER` under
/// `NE_UNDEFINED_OR_NULL`.
#[test]
fn a_type_parameter_under_a_non_null_fact_spells_the_nonnullable_utility() {
    assert_eq!(
        type_of_last_expression(
            "function g<T extends { x: string } | undefined>(obj: T) {\n\
             if (obj != null) { obj; }\n}"
        ),
        "NonNullable<T>"
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
