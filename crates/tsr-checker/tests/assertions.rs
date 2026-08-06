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
#[ignore = "blocked on bd tsr-0ao: the parser rejects `const` as a type reference name, \
            so `is_const_type_reference` can never match. The checker arm is ported and \
            correct; this turns green when the parser does. Deliberately NOT rewritten to \
            assert today's `error`, which would pin the inferior answer."]
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
#[ignore = "blocked on bd tsr-0ao, as above"]
fn const_is_recognised_before_the_type_node_is_resolved() {
    // The ordering is the rule. If `const` were resolved as a name first there
    // is no type called `const`, so every const assertion in the corpus would
    // answer `error`. This is the assertion that distinguishes the two orders —
    // `1 as const` is `1`, and a genuinely unresolvable name is `error`.
    assert_eq!(type_of_initialiser("let x = 1 as const;"), "1");
    assert_eq!(type_of_initialiser("let x = 1 as consts;"), "error");
    // …and a type *named* `const` with arguments is a reference, not an
    // assertion. `isConstTypeReference` tests the arity for this reason.
    assert_eq!(type_of_initialiser("let x = 1 as const<number>;"), "error");
}

#[test]
#[ignore = "blocked on bd tsr-0ao: every const assertion is already a gap for the parser's \
            reason, so this guard cannot be observed until that lands"]
fn a_const_assertion_on_an_object_literal_is_a_gap() {
    // `{ a: 1 } as const` is `{ readonly a: 1; }`, and this port would answer
    // `{ a: number; }`: the readonly-and-unwidened members come from
    // `isConstContext` inside `checkObjectLiteral`, not from here, and even with
    // that ported a string member would print with the wrong quotes — upstream
    // preserves the source's quote style inside an object type while
    // normalising it everywhere else. `bd tsr-7ja` owns both halves.
    assert_eq!(type_of_initialiser("let x = { a: 1 } as const;"), "error");
    // Without `as const` the same literal is answered, so the guard is specific
    // rather than a blanket refusal of object literals.
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
