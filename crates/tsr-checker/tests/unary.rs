//! What the unary expression forms must get right.
//!
//! `typeof`, the prefix operators (`-`, `+`, `~`, `!`, `++`, `--`) and the
//! postfix ones. Every expected string here was taken from a real `.types`
//! baseline under `vendor/typescript-go/testdata/baselines/reference/submodule`
//! before it was written down, because the whole point of these forms is that
//! the *obvious* answer is wrong for a large minority of cases: `-1` is not
//! `number`, `-0` is not `-0`, and `!true` is not `boolean`.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the `index`th statement, which must be a variable
/// statement with an initialiser.
///
/// Indexed so a fixture can declare an annotated variable first and then use it,
/// which is the only way to get an operand of a chosen type.
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
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initialiser = declaration.initializer.expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

/// Type the initialiser of the last statement, after any set-up declarations.
fn type_of_last(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let last = parsed.source_file.statements.len() - 1;
    type_of_initialiser_at(source, last)
}

fn type_of_initialiser(source: &str) -> String {
    type_of_initialiser_at(source, 0)
}

// ---------------------------------------------------------------- `typeof` ---

/// The exact string 265 baseline lines record for `>typeof x`.
const TYPEOF: &str = r#""bigint" | "boolean" | "function" | "number" | "object" | "string" | "symbol" | "undefined""#;

#[test]
fn typeof_is_the_sorted_union_of_the_eight_result_strings() {
    // `typeofType` is built from `slices.Sorted(maps.Keys(typeofNEFacts))`
    // (`checker.go:1052`), so the order is alphabetical rather than any order a
    // human would choose. Getting the set right but the order wrong still fails
    // conformance, which is why this asserts the whole string.
    assert_eq!(type_of_last("var x: string;\nconst t = typeof x;"), TYPEOF);
}

#[test]
fn typeof_does_not_inherit_a_gap_from_its_operand() {
    // `checkTypeOfExpression` (`checker.go:10617`) discards the operand's type
    // entirely, so this is the one form where an operand this port cannot type
    // does *not* make the answer a gap. The operand here is a variable whose
    // annotation names a type that does not resolve, so its own type is `error`.
    assert_eq!(type_of_last("var u: Unresolved;\nconst t = typeof u;"), TYPEOF);
}

// ------------------------------------------------- prefix `-` and `+` ---

#[test]
fn a_negated_numeric_literal_is_a_literal_type_not_number() {
    // `>-1 : -1` in the baselines. Upstream special-cases the literal operand
    // before the operator's general rule (`checker.go:10862`); without that this
    // would be `number`, which is the plausible wrong answer.
    assert_eq!(type_of_initialiser("const x = -1;"), "-1");
    assert_eq!(type_of_initialiser("const x = -1.5;"), "-1.5");
}

#[test]
fn unary_plus_on_a_numeric_literal_keeps_the_literal() {
    // `>+1 : 1` — 14 baseline lines.
    assert_eq!(type_of_initialiser("const x = +1;"), "1");
}

#[test]
fn negative_zero_prints_as_zero() {
    // `>-0 : 0`, four baseline lines. This is the one input on which negating
    // the *value* differs from putting a `-` in front of the text, and IEEE 754
    // makes it easy to get wrong in the other direction too.
    assert_eq!(type_of_initialiser("const x = -0;"), "0");
}

#[test]
fn negation_of_a_non_literal_is_number() {
    // `>-x : number` and `>-true : number`: the literal special case is on the
    // *operand's syntax*, not on its type, so a `true` operand does not take it.
    assert_eq!(type_of_last("var n: number;\nconst x = -n;"), "number");
    assert_eq!(type_of_initialiser("const x = -true;"), "number");
    assert_eq!(type_of_initialiser("const x = ~1;"), "number");
}

// ------------------------------------------------------------ prefix `!` ---

#[test]
fn not_on_a_primitive_is_boolean_but_on_a_literal_is_the_decided_answer() {
    // `>!x : boolean` when the operand could be either truthy or falsy, but
    // `getTypeFacts` decides a unit type: `!true` is `false`, not `boolean`.
    // Answering `boolean` everywhere is the plausible wrong answer this pins.
    assert_eq!(type_of_last("var s: string;\nconst x = !s;"), "boolean");
    assert_eq!(type_of_initialiser("const x = !true;"), "false");
    assert_eq!(type_of_initialiser("const x = !false;"), "true");
    assert_eq!(type_of_initialiser("const x = !0;"), "true");
    assert_eq!(type_of_initialiser("const x = !1;"), "false");
    assert_eq!(type_of_initialiser(r#"const x = !"";"#), "true");
    assert_eq!(type_of_initialiser(r#"const x = !"a";"#), "false");
}

#[test]
fn not_on_a_nullable_is_true() {
    assert_eq!(type_of_initialiser("const x = !null;"), "true");
}

// ------------------------------------------------------- postfix, and gaps ---

#[test]
fn a_postfix_increment_is_number_even_on_a_literal_operand() {
    // `>i++ : number`. There is no literal special case on the postfix path, so
    // an operand of literal type `0` still gives `number`.
    assert_eq!(type_of_last("var i = 0;\nconst x = i++;"), "number");
    assert_eq!(type_of_last("var i: number;\nconst x = i--;"), "number");
}

#[test]
fn a_prefix_increment_is_number() {
    assert_eq!(type_of_last("var i: number;\nconst x = ++i;"), "number");
}

#[test]
fn a_bigint_operand_is_a_gap_rather_than_number() {
    // `getUnaryResultType` (`checker.go:10923`) answers `bigint` or
    // `number | bigint` for a bigint-like operand, both of which need an
    // assignability question this port cannot ask. Answering `number` would be
    // right for the rest of the corpus and wrong for every bigint.
    assert_eq!(type_of_last("var b: bigint;\nconst x = b++;"), "error");
    assert_eq!(type_of_last("var b: bigint;\nconst x = -b;"), "error");
}

#[test]
fn a_negated_bigint_literal_is_a_negative_bigint_literal() {
    assert_eq!(type_of_initialiser("const x = -1n;"), "-1n");
}

#[test]
fn an_operand_this_port_cannot_type_makes_the_result_a_gap() {
    // The only thing the answer depends on is whether the operand is
    // bigint-like, and an `error` operand is exactly the case where that is
    // unknown — so `number` would be a guess.
    //
    // The operand is a variable with an unresolvable annotation rather than an
    // unported *expression* form, deliberately: this fixture used `new C()`
    // until `new` was ported, at which point it started asserting the opposite
    // of what it says. An unresolved name stays a gap however much of the
    // expression grammar lands later.
    assert_eq!(type_of_last("var u: Unresolved;\nconst x = -u;"), "error");
}
