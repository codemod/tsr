//! What `x as T` and `<T>x` must get right.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Type assertions, and the two rules behind one node kind".

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the `index`th statement's first declaration's initialiser.
fn type_of_initialiser_at(source: &str, index: usize) -> String {
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
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

fn type_of_initialiser(source: &str) -> String {
    type_of_initialiser_at(source, 0)
}

#[test]
fn an_assertion_answers_the_asserted_type_and_discards_the_operand() {
    // The operand's type is not consulted at all. `1 as string` is `string` —
    // upstream reports that it is not comparable and still answers `string`,
    // because the diagnostic does not change the type.
    assert_eq!(type_of_initialiser("const x = 1 as string;"), "string");
    assert_eq!(type_of_initialiser(r#"const x = "a" as number;"#), "number");
    // The angle-bracket spelling is the same construct.
    assert_eq!(type_of_initialiser("const x = <string>1;"), "string");
}

#[test]
fn an_assertion_resolves_its_target_through_the_full_type_node_machinery() {
    // Not just keywords: whatever `getTypeFromTypeNode` can answer.
    assert_eq!(type_of_initialiser_at("interface I {}\nconst x = 1 as I;", 1), "I");
    assert_eq!(type_of_initialiser("const x = 1 as string | number;"), "string | number");
    assert_eq!(type_of_initialiser("const x = 1 as { a: string };"), "{ a: string; }");
    assert_eq!(type_of_initialiser(r#"const x = 1 as "a";"#), r#""a""#);
    // An asserted type this port cannot *compute* is a gap, not the operand
    // type — answering `1` here would be a wrong line dressed as a right one.
    // A tuple was the example until the plain form was ported
    // (`docs/architecture/checker-notes-tuple.md`); it now computes and the
    // assertion answers it, which is upstream's line. `keyof` is the example
    // that still exercises the rule, and the property under test — that the
    // answer is never the *operand* type — is what both assertions pin.
    assert_eq!(type_of_initialiser("const x = 1 as [string];"), "[string]");
    assert_eq!(type_of_initialiser("const x = 1 as keyof string;"), "error");
    // An **unresolved** name is a different case since `bd tsr-eep`: upstream
    // reports `Cannot find name` and prints the name, so the assertion answers
    // `Unresolvable`. The thing this line guards — that the answer is never the
    // *operand* type — still holds, and is what would break if the assertion
    // arm fell back to its operand.
    assert_eq!(type_of_initialiser("const x = 1 as Unresolvable;"), "Unresolvable");
}

#[test]
fn a_const_assertion_takes_the_operand_and_makes_it_regular() {
    // The opposite rule: no type node to resolve, and the operand's type is the
    // answer. `1 as const` stays `1` where `const n = 1` would too, but
    // `let n = 1 as const` also stays `1` — the assertion is what stops the
    // widening, which is the whole point of the form.
    assert_eq!(type_of_initialiser("const x = 1 as const;"), "1");
    assert_eq!(type_of_initialiser("let x = 1 as const;"), "1");
    assert_eq!(type_of_initialiser(r#"let x = "a" as const;"#), r#""a""#);
    assert_eq!(type_of_initialiser("let x = true as const;"), "true");
    // Upstream records `>"b" as const : "b"` for exactly this shape.
    assert_eq!(type_of_initialiser("let x = <const>1;"), "1");
}

#[test]
fn const_is_recognised_before_the_type_node_is_resolved() {
    // The ordering is the rule. If `const` were resolved as a name first there
    // is no type called `const`, so every const assertion in the corpus would
    // answer `error`. This is the assertion that distinguishes the two orders —
    // `1 as const` is `1`, and a genuinely unresolvable name is `error`.
    assert_eq!(type_of_initialiser("let x = 1 as const;"), "1");
    // Written as `error` while this test was `#[ignore]`d, before §31 landed
    // unresolved-name minting: `consts` now answers its minted name, the same
    // rule `an_assertion_answers_the_asserted_type` pins for `Unresolvable`.
    // The distinguishing claim is unchanged — the answer is not `1`.
    assert_eq!(type_of_initialiser("let x = 1 as consts;"), "consts");
    // …and a type *named* `const` with arguments would be a reference, not an
    // assertion — `isConstTypeReference` tests the arity for this reason. That
    // claim is unassertable here: the parser's ConstKeyword arm consumes the
    // keyword without ever parsing type arguments, so `1 as const<number>`
    // fails to PARSE ("Expression expected") where upstream reads a reference
    // to a type named `const`. A parse-level divergence on a pathological
    // shape, recorded rather than silently dropped; the checker-side arity
    // gate stays for the identifier-spelled encoding.
}

#[test]
fn a_const_assertion_on_an_object_literal_is_readonly_and_unwidened() {
    // §105 slice 2a: `isConstContext` is ported into `checkObjectLiteral`,
    // so the members are readonly and keep their regular literals. This test
    // previously pinned the pre-slice GAP ("error") and went red the day the
    // slice landed — the failure was the feature arriving.
    assert_eq!(type_of_initialiser("let x = { a: 1 } as const;"), "{ readonly a: 1; }");
    // §109: the value-spelling carriage — a SINGLE-QUOTED member value
    // prints single-quoted inside the object type (upstream reuses the
    // source node), while its standalone line stays double-quoted. This
    // assertion pinned the pre-carriage gap and went red the day §109
    // landed — the second same-day instance of a test one build behind
    // its own feature, both caught by the tee-log grep.
    assert_eq!(type_of_initialiser("let x = { a: 'b' } as const;"), "{ readonly a: 'b'; }");
    // A double-quoted member has no spelling question and answers.
    assert_eq!(type_of_initialiser("let x = { a: \"b\" } as const;"), "{ readonly a: \"b\"; }");
    // Without `as const` the same literal widens, so the const arm is
    // specific rather than a new default.
    assert_eq!(type_of_initialiser("let x = { a: 1 };"), "{ a: number; }");
    // A non-const assertion *to* an object type is unaffected too.
    assert_eq!(type_of_initialiser("let x = { a: 1 } as { a: number };"), "{ a: number; }");
}

#[test]
fn an_operand_this_port_cannot_type_still_gives_the_asserted_type() {
    // The operand is discarded, so a gap in it must not propagate — this is the
    // one place an assertion pays even where nothing else does.
    assert_eq!(type_of_initialiser("const x = unknownThing as string;"), "string");
    // A const assertion has nothing else to answer with, so a gap in the operand
    // does propagate there — asserted in the ignored test above rather than here,
    // where today it would pass for the wrong reason (`bd tsr-0ao`).
}
