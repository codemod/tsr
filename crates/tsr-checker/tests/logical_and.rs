//! What `&&` must get right.
//!
//! `a && b` is **not** the union of the operands, and it is not the right
//! operand either. It is `getUnionType([extractDefinitelyFalsyTypes(left),
//! right])`, gated on the left operand being possibly truthy
//! (`checker.go:12496`). Every expected string below was taken from a real
//! `.types` baseline under
//! `vendor/typescript-go/testdata/baselines/reference/submodule` before it was
//! written down — the two naive designs score 401 and 377 of 659 corpus lines
//! respectively (`docs/architecture/checker-notes-armsplit.md` §3.2), which is
//! exactly the range where a fixture chosen by intuition agrees with both.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the `index`th statement.
///
/// Indexed so a fixture can declare annotated variables first and then combine
/// them, which is the only way to get operands of chosen types.
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

/// The head of the corpus row: 218 of the 659 lines are exactly this.
///
/// `boolean` is the union `false | true`. Mapping the definitely-falsy part
/// gives `false | never`, which the union worker reduces to `false`; unioning
/// that with `boolean` gives `boolean` back.
#[test]
fn boolean_and_boolean_is_boolean() {
    let source = "let a: boolean; let b: boolean; let c = a && b;";
    assert_eq!(type_of_initialiser_at(source, 2), "boolean");
}

/// **The discriminating fixture.** `conformance/logicalAndOperatorWithEveryType`
/// and `compiler/complexNarrowingWithAny` both carry this shape, and it is the
/// only one here that separates all three candidate designs at once:
///
/// | design | answer |
/// |---|---|
/// | the left operand | `number` |
/// | the right operand | `"a"` |
/// | `left \| right` | `number \| "a"` |
/// | **upstream** | **`"a" \| 0`** |
///
/// The constituent order is upstream's, not a choice: `CompareTypes`
/// (`utilities.go:415`) sorts by increasing flag value, and
/// `TypeFlagsStringLiteral` is `1 << 10` against `TypeFlagsNumberLiteral`'s
/// `1 << 11`. The corpus agrees — `>authToken && { authToken } : "" | { authToken: string; }`
/// and `>x && y : 0 | false`. This expectation was written the wrong way round
/// first, from intuition, and the baselines corrected it.
///
/// The corpus's own head triple for this shape is `number && number[] ->
/// 0 | number[]` (13 lines). It is not used here because `number[]` needs the
/// global `Array` symbol and this harness parses one file with no `lib.d.ts` —
/// the fixture would gap for a reason that has nothing to do with `&&`, which
/// is exactly the vacuous-test failure this file exists to avoid.
///
/// `docs/conventions.md` records a mutation that passed because the fixture
/// `{ z, m, a }` happened to hash into declaration order. The corrective there
/// was to run the mutation against candidate inputs and read the outputs rather
/// than reason about which fixture would discriminate. This fixture was chosen
/// the same way — from the corpus's own `(left, right) -> want` triples, where
/// it is 13 lines — and not by reasoning about the falsy switch.
#[test]
fn the_falsy_part_of_the_left_operand_survives() {
    let source = "let n: number; let s: \"a\"; let c = n && s;";
    assert_eq!(type_of_initialiser_at(source, 2), "\"a\" | 0");
}

/// A left operand that can never be truthy short-circuits: the right operand is
/// never evaluated, so the result is the left type **unchanged**
/// (`checker.go:12497` — the `if` is a gate, not a fast path).
///
/// Deleting the gate answers `undefined | string` here, which is the whole
/// union rather than the left type, so this test bites the branch directly.
#[test]
fn a_never_truthy_left_operand_is_the_answer_on_its_own() {
    let source = "let u: undefined; let s: string; let c = u && s;";
    assert_eq!(type_of_initialiser_at(source, 2), "undefined");
}

/// The union arm added to `get_type_facts` for this caller. `undefined | null`
/// is a union whose every constituent is falsy, so the gate must see `FALSY`
/// alone — and before that arm existed a union fell to the `_ => both` default
/// and would have reported `TRUTHY`, producing a union with `string` in it.
///
/// **The asserted string records a divergence this test found and does not
/// own.** Upstream prints `undefined | null` (`TypeFlagsUndefined` is `1 << 2`,
/// `TypeFlagsNull` `1 << 3`, and `CompareTypes` sorts ascending). This port
/// prints `null | undefined`, and it does so for the bare declared type with no
/// `&&` anywhere — `let x: undefined | null` and `let x: null | undefined` both
/// normalise to `null | undefined`, while `string | number` comes out right.
/// So it is a pre-existing defect in the union order for exactly this pair, not
/// something `&&` introduced. `bd tsr-iiu`.
#[test]
fn a_union_of_falsy_constituents_is_never_truthy() {
    let source = "let u: undefined | null; let s: string; let c = u && s;";
    assert_eq!(type_of_initialiser_at(source, 2), "null | undefined");
}

/// `any && x` is `any`, 56 corpus lines. `any`'s definitely-falsy part is
/// `any` itself (`AnyOrUnknown` maps to `t`), and the union absorbs the right
/// operand.
///
/// This is the pair that makes the `is_error` test in `check_logical_and`
/// load-bearing rather than decorative: `errorType` also carries
/// `TypeFlags::ANY`, so without the identity test an unported left operand
/// would answer `any` here — a confident wrong line where the port's whole
/// gap/wrong discipline wants a gap.
#[test]
fn any_absorbs_the_right_operand() {
    let source = "let a: any; let s: string; let c = a && s;";
    assert_eq!(type_of_initialiser_at(source, 2), "any");
}

/// `string`'s falsy part is the **empty string literal**, not `string`, so the
/// result keeps a literal the left operand did not have. `number` sorts before
/// it — `TypeFlagsNumber` is `1 << 6` and `TypeFlagsStringLiteral` `1 << 10` —
/// which the corpus confirms at `>-a.x && b.y() : void | ""`.
#[test]
fn the_falsy_part_of_string_is_the_empty_string_literal() {
    let source = "let s: string; let n: number; let c = s && n;";
    assert_eq!(type_of_initialiser_at(source, 2), "number | \"\"");
}

/// A gap in is a gap out, and the test is on **identity** rather than on
/// `TypeFlags::ANY` — `errorType` carries `ANY` too.
///
/// The unported form to hand is `||`, which this module still gaps, so
/// `(a || b) && s` reaches `check_logical_and` with an `errorType` left
/// operand. Without the `is_error` guard the falsy switch would map it through
/// `ANY_OR_UNKNOWN` to itself and the union would answer `error` anyway — by
/// accident of a flag, and only for as long as `||` is the gap. Removing the
/// guard therefore does **not** turn this red today, which is stated rather
/// than dressed up: what it defends is the day `||` lands and some *other*
/// form is the gap. The corpus is the real evidence, at 0 lines lost across
/// 9,538 cases.
#[test]
fn a_gapped_left_operand_gaps_the_whole_expression() {
    let source = "let a: boolean; let b: boolean; let s: string; let c = (a || b) && s;";
    assert_eq!(type_of_initialiser_at(source, 3), "error");
}
