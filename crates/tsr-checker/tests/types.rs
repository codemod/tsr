//! What the checker must get right, stated as questions about printed types.
//!
//! Assertions go through [`Checker::type_to_string`] rather than through
//! `TypeId`s, because the printed form is what `.types` baselines compare and a
//! type that is internally right but prints wrong fails conformance identically.

use tsr_ast::{Expression, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the first `const`/`let` in the source.
fn type_of_initialiser(source: &str) -> String {
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

    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let initialiser = declaration.initializer.expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

#[test]
fn a_literal_expression_has_its_own_literal_type() {
    // The expression `"a"` is `"a"`, not `string` — the distinction the whole
    // literal-type machinery exists for.
    assert_eq!(type_of_initialiser(r#"const x = "a";"#), r#""a""#);
    assert_eq!(type_of_initialiser("const x = 1;"), "1");
    assert_eq!(type_of_initialiser("const x = true;"), "true");
    assert_eq!(type_of_initialiser("const x = false;"), "false");
    assert_eq!(type_of_initialiser("const x = null;"), "null");
    assert_eq!(type_of_initialiser("const x = 1n;"), "1n");
}

#[test]
fn a_numeric_literal_type_prints_its_value_not_its_spelling() {
    // `1.0` and `0x1` are both the type `1`. Getting this from the source text
    // would be wrong in a way every numeric baseline would catch.
    assert_eq!(type_of_initialiser("const x = 1.0;"), "1");
    assert_eq!(type_of_initialiser("const x = 0x10;"), "16");
    assert_eq!(type_of_initialiser("const x = 1_000;"), "1000");
    assert_eq!(type_of_initialiser("const x = 1.5;"), "1.5");
}

#[test]
fn a_string_literal_type_is_quoted_and_escaped_as_typescript_prints_it() {
    // Whole-line comparison means the quoting is under test, not cosmetic.
    assert_eq!(type_of_initialiser(r"const x = 'a';"), r#""a""#);
    assert_eq!(type_of_initialiser(r#"const x = "a\"b";"#), r#""a\"b""#);
    assert_eq!(type_of_initialiser(r#"const x = "a\\b";"#), r#""a\\b""#);
}

#[test]
fn parentheses_do_not_change_a_type() {
    assert_eq!(type_of_initialiser(r#"const x = ("a");"#), r#""a""#);
    assert_eq!(type_of_initialiser("const x = ((1));"), "1");
}

#[test]
fn an_unported_expression_form_is_error_not_any() {
    // `errorType` and `anyType` are distinguished by identity, not by printing,
    // and conflating them is how a gap becomes an assertion. Everything unported
    // must land on `error`.
    let arena = Arena::new();
    let source = "const x = f();";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    assert_eq!(id, checker.intrinsics().error, "an unported form must be errorType");
    assert_ne!(id, checker.intrinsics().any, "and must not be anyType");
}

#[test]
fn identical_literals_are_one_interned_type() {
    // Literal identity is TypeId equality, which is what makes `"a" === "a"`
    // decidable without comparing strings on every relation check.
    let arena = Arena::new();
    let source = r#"const a = "x"; const b = "x"; const c = "y";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);

    let mut ids = Vec::new();
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(statement) = statement else { continue };
        let initialiser = statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|d| d.initializer)
            .expect("an initialiser");
        ids.push(checker.check_expression(initialiser));
    }
    assert_eq!(ids[0], ids[1], r#"two occurrences of "x" must be one type"#);
    assert_ne!(ids[0], ids[2], r#""x" and "y" must not be"#);
}

#[test]
fn the_intrinsics_that_print_alike_are_still_distinct_types() {
    // `anyType` and `errorType` both print `any`; merging them would silently
    // change which errors cascade. Upstream keeps them apart by pointer identity.
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: "" },
    );
    let checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let intrinsics = checker.intrinsics();

    assert_eq!(checker.type_to_string(intrinsics.any), "any");
    assert_eq!(checker.type_to_string(intrinsics.error), "error");
    assert_ne!(intrinsics.any, intrinsics.error);
}

#[test]
fn a_literal_type_widens_to_its_primitive() {
    // The rule behind `let x = "a"` being `string` while `const x = "a"` is `"a"`.
    let arena = Arena::new();
    let source = r#"const x = "a";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");

    let literal = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(literal), r#""a""#);
    let widened = checker.get_widened_literal_type(literal);
    assert_eq!(checker.type_to_string(widened), "string");

    // Widening a non-literal is the identity, not an error.
    let string = checker.intrinsics().string;
    let again = checker.get_widened_literal_type(string);
    assert_eq!(again, string);
}

#[test]
fn an_expression_is_typed_once_however_many_times_it_is_asked_for() {
    // The memo is the whole reason `&mut self` returning `TypeId` was chosen; if
    // it were bypassed the checker would be exponential on nested expressions.
    let arena = Arena::new();
    let source = r#"const x = "a";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("variable statement");
    };
    let initialiser: Expression<'_> = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");

    // Counting *types* cannot test this: interning already means a repeated
    // literal produces no new type, so that assertion passes with the memo
    // removed. Counting computations is what distinguishes the two.
    let first = checker.check_expression(initialiser);
    assert_eq!(checker.computations(), 1, "the first ask computes");
    let second = checker.check_expression(initialiser);

    assert_eq!(first, second);
    assert_eq!(checker.computations(), 1, "the second ask is served from the memo");
}

// ---------------------------------------------------------------------------
// Declaration types: `getTypeOfSymbol` (`bd tsr-4sc.2`).
//
// Assertions go through the printed form, because that is what a `.types`
// baseline compares. `>x : string` is a declaration line.
// ---------------------------------------------------------------------------

/// The printed type of the top-level declaration named `name`.
fn type_of_declaration(source: &str, name: &str) -> String {
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
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));

    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn a_const_keeps_its_literal_type_and_a_let_widens_it() {
    // The rule the freshness pair exists for, and the most frequently surprising
    // line in any `.types` baseline: same initialiser, two different types.
    assert_eq!(type_of_declaration(r#"const x = "a";"#, "x"), r#""a""#);
    assert_eq!(type_of_declaration(r#"let x = "a";"#, "x"), "string");
    assert_eq!(type_of_declaration("const n = 1;", "n"), "1");
    assert_eq!(type_of_declaration("let n = 1;", "n"), "number");
    assert_eq!(type_of_declaration("const b = true;", "b"), "true");
    assert_eq!(type_of_declaration("let b = true;", "b"), "boolean");
    assert_eq!(type_of_declaration("const g = 1n;", "g"), "1n");
    assert_eq!(type_of_declaration("let g = 1n;", "g"), "bigint");
}

#[test]
fn a_var_widens_like_a_let() {
    // `var` is not block-scoped but it is not `const` either, and the rule keys
    // on `NodeFlags::CONSTANT` rather than on block scoping.
    assert_eq!(type_of_declaration(r#"var x = "a";"#, "x"), "string");
}

#[test]
fn an_annotation_beats_the_initialiser() {
    assert_eq!(type_of_declaration(r#"const x: string = "a";"#, "x"), "string");
    // And it is the annotation, not a widening of the initialiser: a literal
    // *type* stays literal even on a `let`, because a literal type node yields
    // the regular form and only fresh literals widen.
    assert_eq!(type_of_declaration(r#"let x: "a" = "a";"#, "x"), r#""a""#);
}

#[test]
fn every_keyword_type_node_is_its_intrinsic() {
    for (annotation, printed) in [
        ("any", "any"),
        ("unknown", "unknown"),
        ("string", "string"),
        ("number", "number"),
        ("bigint", "bigint"),
        ("boolean", "boolean"),
        ("symbol", "symbol"),
        ("void", "void"),
        ("undefined", "undefined"),
        ("never", "never"),
        ("object", "object"),
    ] {
        let source = format!("declare const x: {annotation};");
        assert_eq!(type_of_declaration(&source, "x"), printed, "for `{annotation}`");
    }
}

#[test]
fn a_parenthesised_type_is_its_inner_type() {
    assert_eq!(type_of_declaration("declare const x: (string);", "x"), "string");
}

#[test]
fn a_declaration_with_neither_annotation_nor_initialiser_is_the_implicit_any() {
    // `anyType`, deliberately, and not `errorType`: upstream's
    // `widenTypeForVariableLikeDeclaration` returns `anyType` here
    // (`checker.go:18264`). It is a computed answer — the implicit any — rather
    // than a gap, and the two must stay distinguishable even though they print
    // the same.
    let source = "declare let x;";
    assert_eq!(type_of_declaration(source, "x"), "any");
}

#[test]
fn an_unported_type_node_is_an_error_type_not_an_any() {
    // A tuple type has no implementation. It must yield `errorType`, so that
    // `checker_types` sees a gap rather than a confident `any` — asserted on
    // identity, since `errorType` and `anyType` are different types.
    //
    // The fixture has been changed twice as the thing it named became ported —
    // first `interface I {}`, then `{ a: string }`. The *assertion* has never
    // changed, because what it tests is the discipline and not the form.
    let arena = Arena::new();
    let source = "declare const x: [string, number];";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "x").expect("`x` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(id, checker.intrinsics().error, "an unported type node must be errorType");
    assert_ne!(id, checker.intrinsics().any, "and must not be anyType, though both print `any`");
}

#[test]
fn a_self_referential_initialiser_resolves_instead_of_hanging() {
    // `const a = a` — the circularity the resolution stack exists for. Upstream
    // returns `anyType` for a circular *initialiser* (`checker.go:18831`), and
    // `errorType` only when the cycle runs through a type annotation.
    assert_eq!(type_of_declaration("const a = a;", "a"), "any");
}

#[test]
fn a_symbol_shape_this_slice_does_not_port_is_an_error_type() {
    // An accessor symbol goes to `getTypeOfAccessors`, which is unported. It must
    // read as a gap rather than as `any`.
    //
    // This test used to name a *function* symbol, which now has a type
    // (`bd tsr-4sc.8`). The shape moved; the discipline did not.
    let arena = Arena::new();
    let source = "class C { get a() { return 1; } }";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let class = bound.lookup_local(root, "C").expect("`C` is declared");
    let symbol = *bound.symbols().get(class).members.get("a").expect("`a` is a member");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(id, checker.intrinsics().error);
    assert_ne!(id, checker.intrinsics().any);
}

#[test]
fn a_symbols_type_is_computed_once_and_memoised() {
    // The memo is ADR-0013's read-drop-recurse-write. Asserted with a
    // computation counter rather than by counting types, because interning
    // already makes a repeated literal produce no new type — a test that counted
    // types would pass with the memo deleted. That exact no-op test shipped once
    // in this crate already.
    let arena = Arena::new();
    let source = r#"const x = "a";"#;
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "x").expect("`x` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);

    let first = checker.get_type_of_symbol(symbol);
    let after_first = checker.computations();
    let second = checker.get_type_of_symbol(symbol);
    assert_eq!(first, second);
    assert_eq!(
        checker.computations(),
        after_first,
        "the second ask must be served from the memo, computing nothing"
    );
}

#[test]
fn a_reference_takes_the_type_of_what_it_resolves_to() {
    assert_eq!(type_of_declaration(r#"const a = "x"; const b = a;"#, "b"), r#""x""#);
    // **Freshness propagates through a `const`.** `getWidenedLiteralTypeForInitializer`
    // returns the initialiser's type *unchanged* for a const (`checker.go:16897`),
    // and that type is the fresh literal `checkExpression` produced — so `a` is
    // fresh `"x"`, and the `let` below still has something to widen. This test
    // originally asserted `"x"` here, on the assumption that a reference could
    // not be fresh; the implementation was right and the assumption was wrong.
    // `compiler/literalFreshnessPropagationOnNarrowing.ts` is the corpus case
    // named for this behaviour.
    assert_eq!(type_of_declaration(r#"const a = "x"; let b = a;"#, "b"), "string");
    // The other direction needs no widening: `a` is already `string`.
    assert_eq!(type_of_declaration(r#"let a = "x"; const b = a;"#, "b"), "string");
}

#[test]
fn a_cycle_through_two_symbols_resolves_instead_of_hanging() {
    // The case a per-symbol "in progress" flag gets wrong, end to end rather
    // than as a unit test of the stack: both participants must resolve.
    assert_eq!(type_of_declaration("const a = b; const b = a;", "a"), "any");
    assert_eq!(type_of_declaration("const a = b; const b = a;", "b"), "any");
}

#[test]
fn an_unresolved_name_is_an_error_type() {
    // Not `any`: we could not compute it, as opposed to computing that it is
    // `any`. `checker_types` has to be able to tell those apart.
    let arena = Arena::new();
    let source = "const a = nowhere;";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "a").expect("`a` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(id, checker.intrinsics().error);
}

#[test]
fn a_literal_that_came_from_an_annotation_does_not_widen_when_referenced() {
    // The case that makes freshness load-bearing, and the one the rest of this
    // file missed: `a`'s type comes from a type *node*, so it is the regular
    // literal, and `getWidenedLiteralType` widens only fresh ones
    // (`checker.go:25491`). So `b` stays `"x"` even though it is a `let`.
    //
    // Every other path here happens to agree whether or not freshness is
    // checked — an initialiser literal is always fresh, and an annotation is
    // returned without widening — so deleting the freshness check turned no test
    // red until this one existed.
    assert_eq!(type_of_declaration(r#"let a: "x" = "x"; let b = a;"#, "b"), r#""x""#);
    // Contrast: the same shape with the literal coming from an initialiser does
    // widen, because that literal is fresh.
    assert_eq!(type_of_declaration(r#"const a = "x"; let b = a;"#, "b"), "string");
}

#[test]
fn a_declaration_kind_this_slice_does_not_port_is_an_error_type() {
    // A binding element. The symbol *is* a variable, so the dispatch in
    // `getTypeOfSymbol` sends it down the variable path, and only the worker's
    // kind match rejects it — a different gate from
    // `a_symbol_shape_this_slice_does_not_port_is_an_error_type`, which is
    // rejected one level earlier. Destructuring needs `getTypeForBindingElement`,
    // which is unported.
    let arena = Arena::new();
    let source = "const { a } = { a: 1 };";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "a").expect("`a` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(id, checker.intrinsics().error, "an unported declaration kind must be errorType");
    assert_ne!(id, checker.intrinsics().any);
}

/// The binary operators (`bd tsr-4sc.13`).
///
/// Ranked into this order by `examples/types_shapes.rs`: of the 39,035 corpus
/// assertion lines lost on a `BinaryExpression`, assignment is 18,743, `+` is
/// 3,537, the comparisons together are 6,336 and the arithmetic and bitwise
/// family 6,886. The logical operators — 1,860 lines — need unions and are
/// deliberately still `errorType`.
#[test]
fn an_assignment_has_the_type_of_its_right_hand_side_including_its_freshness() {
    // The commonest binary line in the corpus by a factor of five. `x = "a"` is
    // `"a"` and not `string`: assignment returns the right operand's type
    // *unchanged*, so the fresh literal survives. Widening here would be
    // invisible in a test that only asked for `string`.
    assert_eq!(type_of_initialiser(r#"const x = (y = "a");"#), r#""a""#);
    assert_eq!(type_of_initialiser("const x = (y = 1);"), "1");
}

#[test]
fn a_comma_expression_has_the_type_of_its_right_hand_side() {
    assert_eq!(type_of_initialiser(r#"const x = (1, "a");"#), r#""a""#);
}

#[test]
fn arithmetic_bitwise_and_shift_are_number() {
    // One arm covers twenty-one operators upstream, which is why the test names
    // one from each family rather than all of them.
    assert_eq!(type_of_initialiser("const x = 1 * 2;"), "number");
    assert_eq!(type_of_initialiser("const x = 1 - 2;"), "number");
    assert_eq!(type_of_initialiser("const x = 1 / 2;"), "number");
    assert_eq!(type_of_initialiser("const x = 1 << 2;"), "number");
    assert_eq!(type_of_initialiser("const x = 1 | 2;"), "number");
    assert_eq!(type_of_initialiser("const x = 1 ** 2;"), "number");
    // Not `1`: the operands are literal types and the result is the primitive.
    assert_ne!(type_of_initialiser("const x = 1 * 1;"), "1");
}

#[test]
fn bigint_arithmetic_is_bigint_and_mixing_it_is_not_a_guess() {
    assert_eq!(type_of_initialiser("const x = 1n * 2n;"), "bigint");
    // Upstream reports on a mixed operation and answers `errorType`. Answering
    // `number` would be the plausible wrong thing.
    assert_eq!(type_of_initialiser("const x = 1n * 2;"), "error");
}

#[test]
fn addition_is_numeric_before_it_is_textual() {
    // Upstream's order is load-bearing: both number-like first, then both
    // bigint-like, then *either* string-like. A test that only checked
    // `"a" + "b"` would pass with the arms in any order.
    assert_eq!(type_of_initialiser("const x = 1 + 2;"), "number");
    assert_eq!(type_of_initialiser("const x = 1n + 2n;"), "bigint");
    assert_eq!(type_of_initialiser(r#"const x = "a" + "b";"#), "string");
    assert_eq!(type_of_initialiser(r#"const x = 1 + "b";"#), "string");
    assert_eq!(type_of_initialiser(r#"const x = "a" + 1;"#), "string");
}

#[test]
fn every_comparison_is_boolean_even_when_an_operand_is_a_gap() {
    // These are the arms that answer *without* their operands: upstream returns
    // `booleanType` whatever they are, computing them only to report on them. So
    // an unported operand does not stop the answer, and that is a computed
    // result rather than a guess.
    assert_eq!(type_of_initialiser("const x = 1 < 2;"), "boolean");
    assert_eq!(type_of_initialiser("const x = 1 >= 2;"), "boolean");
    assert_eq!(type_of_initialiser("const x = 1 === 2;"), "boolean");
    assert_eq!(type_of_initialiser("const x = 1 != 2;"), "boolean");
    assert_eq!(type_of_initialiser(r#"const x = "a" in b;"#), "boolean");
    assert_eq!(type_of_initialiser("const x = a instanceof B;"), "boolean");
    assert_eq!(type_of_initialiser("const x = f() === g();"), "boolean");
}

#[test]
fn an_unported_operand_propagates_rather_than_becoming_number() {
    // The deliberate deviation, stated as a test. `errorType` carries
    // `TypeFlagsAny`, so upstream's rule would answer `number` here — sound for
    // upstream, where `errorType` means an error was reported, and a claim in
    // this port, where it also means an unported form.
    assert_eq!(type_of_initialiser("const x = f() * 2;"), "error");
    assert_eq!(type_of_initialiser("const x = f() + 2;"), "error");
}

#[test]
fn the_logical_operators_are_still_a_gap() {
    // `a && b` is a union of the operands, and unions do not exist (bd
    // tsr-4sc.9). The tempting wrong answer is the *left* type, which is right
    // only when the left operand can never be falsy.
    assert_eq!(type_of_initialiser("const x = 1 && 2;"), "error");
    assert_eq!(type_of_initialiser("const x = 1 || 2;"), "error");
    assert_eq!(type_of_initialiser("const x = 1 ?? 2;"), "error");
}

#[test]
fn a_destructuring_assignment_is_a_gap_and_not_its_right_hand_side() {
    // `[a] = [1]` binds a *pattern*; taking the right-hand type would be nearly
    // right for the expression and wrong for everything inside the pattern, so
    // upstream leaves the function before checking either operand.
    assert_eq!(type_of_initialiser("const x = ([a] = [1]);"), "error");
    assert_eq!(type_of_initialiser("const x = ({ a } = { a: 1 });"), "error");
}

/// Named types (`bd tsr-4sc.7`, first slice): `getDeclaredTypeOfSymbol` and the
/// type references that reach it.
#[test]
fn a_type_reference_to_an_interface_or_class_is_that_named_type() {
    assert_eq!(type_of_declaration("interface I {}\ndeclare const x: I;", "x"), "I");
    assert_eq!(type_of_declaration("class C {}\ndeclare const x: C;", "x"), "C");
}

#[test]
fn a_type_alias_is_transparent() {
    // `conformance/typeAliases.types` records `type T1 = number; var x1: T1` as
    // `>x1 : number`. The alias name does not survive — printing `T1` here would
    // look more informative and be wrong.
    assert_eq!(type_of_declaration("type T = number;\ndeclare const x: T;", "x"), "number");
    assert_eq!(
        type_of_declaration(
            r#"type T = "a";
declare const x: T;"#,
            "x"
        ),
        r#""a""#
    );
    // Through two aliases, which is what makes this a recursion rather than a
    // lookup.
    assert_eq!(
        type_of_declaration("type A = string;\ntype B = A;\ndeclare const x: B;", "x"),
        "string"
    );
}

#[test]
fn a_circular_type_alias_answers_rather_than_hanging() {
    // `type T = T` resolves through `getDeclaredTypeOfTypeAlias` forever without
    // the resolution stack. Upstream reports "Type alias 0 circularly references
    // itself" and answers `errorType`.
    assert_eq!(type_of_declaration("type T = T;\ndeclare const x: T;", "x"), "error");
    assert_eq!(type_of_declaration("type A = B;\ntype B = A;\ndeclare const x: A;", "x"), "error");
}

#[test]
fn a_generic_type_is_printed_with_its_type_parameters() {
    // The declared type of `class C<T>` prints `C<T>` — upstream's baselines
    // record `class A { }` as `>A : A` and `class C<T> {}` as `>C : C<T>`.
    assert_eq!(declared_type_of("class C<T> {}", "C"), "C<T>");
    assert_eq!(declared_type_of("interface I<T, U> {}", "I"), "I<T, U>");
    assert_eq!(declared_type_of("class C {}", "C"), "C");
    // A generic type referenced with *no* arguments is an error upstream, which
    // answers `errorType` too.
    assert_eq!(type_of_declaration("class C<T> {}\ndeclare const x: C;", "x"), "error");
    // Type arguments on a *non*-generic type are `checkNoTypeArguments`, and
    // they must not be ignored: without this line the arguments could be
    // dropped silently and every assertion above would still pass.
    assert_eq!(type_of_declaration("interface I {}\ndeclare const x: I<number>;", "x"), "error");
}

#[test]
fn an_enum_declares_a_type_that_prints_its_name() {
    // Upstream's declared type of an enum is the union of its members' literal
    // types, which prints as the enum's name; this port has no unions and builds
    // a named type that prints the same string. A divergence in behaviour that
    // is invisible in a printed line — see `getDeclaredTypeOfSymbol`.
    assert_eq!(declared_type_of("enum E { A }", "E"), "E");
    assert_eq!(type_of_declaration("enum E { A }\ndeclare const x: E;", "x"), "E");
}

#[test]
fn a_type_parameter_is_its_own_named_type() {
    // Looked up inside the function\'s own scope, because that is the only place
    // a type parameter is in scope — which is also why it cannot go through
    // `type_of_declaration`.
    let arena = Arena::new();
    let source = "declare function f<T>(p: T): void;";
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let Statement::FunctionDeclaration(function) = parsed.source_file.statements[0] else {
        panic!("the fixture is a function declaration");
    };
    let scope = function.node_id.expect("registered");
    let symbol = bound.lookup_local(scope, "T").expect("the type parameter is in the function");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_declared_type_of_symbol(symbol);
    assert_eq!(checker.type_to_string(id), "T");
}

#[test]
fn a_qualified_name_is_still_a_gap() {
    // `M.I` needs `resolveEntityName` through module exports. Resolving only the
    // left-hand identifier would answer the module's type for the member, which
    // is the plausible wrong thing.
    // The left-hand name is given a declared type of its own here — `M` is an
    // interface as well as a namespace — so a version that resolved `M.I` by its
    // left identifier would answer `M` rather than failing. Without that
    // merge the test passes whether or not qualified names are handled, which is
    // the kind of test this project has shipped before.
    assert_eq!(
        type_of_declaration(
            "interface M {}\nnamespace M { export interface I {} }\ndeclare const x: M.I;",
            "x"
        ),
        "error"
    );
}

/// The type a *type declaration* declares, by the declaration's name.
fn declared_type_of(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).expect("the declaration is in scope");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_declared_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn an_anonymous_object_type_prints_its_members() {
    // Compared character for character against 26,686 corpus lines, so the
    // spaces and the trailing semicolon are part of the answer.
    assert_eq!(type_of_declaration("declare const x: {};", "x"), "{}");
    assert_eq!(type_of_declaration("declare const x: { a: string };", "x"), "{ a: string; }");
    assert_eq!(
        type_of_declaration("declare const x: { a: string, b: number };", "x"),
        "{ a: string; b: number; }"
    );
    assert_eq!(type_of_declaration("declare const x: { a?: string };", "x"), "{ a?: string; }");
    assert_eq!(
        type_of_declaration("declare const x: { readonly a: string };", "x"),
        "{ readonly a: string; }"
    );
    // Nested, which is where a member type has to go back through
    // `getTypeFromTypeNode` rather than being printed from the node.
    assert_eq!(
        type_of_declaration("interface I {}\ndeclare const x: { a: { b: I } };", "x"),
        "{ a: { b: I; }; }"
    );
}

#[test]
fn an_object_type_with_a_member_this_port_cannot_render_is_a_gap() {
    // Not a partial object: printing the members that *are* understood would be
    // a wrong answer dressed as a right one, and it would fail the line either
    // way.
    assert_eq!(type_of_declaration("declare const x: { m(): void };", "x"), "error");
    assert_eq!(type_of_declaration("declare const x: { (): void };", "x"), "error");
    assert_eq!(type_of_declaration("declare const x: { [k: string]: string };", "x"), "error");
    // A member whose own type is a gap takes the whole literal with it.
    assert_eq!(type_of_declaration("declare const x: { a: [string] };", "x"), "error");
}

/// Generic references (`bd tsr-4sc.7`, third slice).
#[test]
fn a_reference_to_a_generic_type_carries_its_arguments() {
    assert_eq!(type_of_declaration("class C<T> {}\ndeclare const x: C<number>;", "x"), "C<number>");
    assert_eq!(
        type_of_declaration("interface I<T, U> {}\ndeclare const x: I<string, number>;", "x"),
        "I<string, number>"
    );
    // Nested, so the argument itself goes back through `getTypeFromTypeNode`.
    assert_eq!(
        type_of_declaration("class C<T> {}\ndeclare const x: C<C<string>>;", "x"),
        "C<C<string>>"
    );
    // A generic alias keeps its name, unlike the transparent non-generic case.
    assert_eq!(
        type_of_declaration("type A<T> = T;\ndeclare const x: A<number>;", "x"),
        "A<number>"
    );
    assert_eq!(declared_type_of("type Tree<T> = T;", "Tree"), "Tree<T>");
}

#[test]
fn the_arity_of_a_generic_reference_is_checked() {
    // Outside `[minTypeArgumentCount, len(parameters)]` upstream reports and
    // answers `errorType`. Inside it — fewer arguments than parameters, filled
    // from defaults — needs substitution and is a gap rather than a guess.
    assert_eq!(type_of_declaration("class C<T, U> {}\ndeclare const x: C<number>;", "x"), "error");
    assert_eq!(
        type_of_declaration("class C<T> {}\ndeclare const x: C<number, string>;", "x"),
        "error"
    );
    assert_eq!(type_of_declaration("class C<T = string> {}\ndeclare const x: C;", "x"), "error");
}

#[test]
fn a_gap_in_a_type_argument_is_a_gap_in_the_reference() {
    // `C<Unported>` is not `C<any>`. Printing the reference with a guessed
    // argument would turn a missing line into a wrong one.
    assert_eq!(type_of_declaration("class C<T> {}\ndeclare const x: C<[string]>;", "x"), "error");
}

#[test]
fn a_self_referential_generic_alias_terminates() {
    // `type Tree<T> = T | { left: Tree<T> }` is the shape upstream guards with
    // an instantiation depth of 100. Nothing here substitutes into the body, so
    // it terminates for a different reason than upstream's — which is why the
    // limits are documented as belonging with `instantiateType` rather than
    // ported here.
    assert_eq!(declared_type_of("type Tree<T> = { left: Tree<T> };", "Tree"), "Tree<T>");
    assert_eq!(
        type_of_declaration(
            "type Tree<T> = { left: Tree<T> };\ndeclare const x: Tree<number>;",
            "x"
        ),
        "Tree<number>"
    );
}

#[test]
fn the_same_instantiation_written_twice_is_one_type() {
    // Identity, not printing: two `C<number>` annotations must produce the same
    // `TypeId`, the way two `"a"` literals do. Nothing looks inside these types
    // yet, so this is the only place the interning is observable — and the first
    // relation check written would compare two handles that should be equal.
    let arena = Arena::new();
    let source = "class C<T> {}\ndeclare const x: C<number>;\ndeclare const y: C<number>;\n\
                  declare const z: C<string>;";
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let type_of = |checker: &mut Checker<'_, '_>, name: &str| {
        checker.get_type_of_symbol(bound.lookup_local(root, name).expect("declared"))
    };
    let x = type_of(&mut checker, "x");
    let y = type_of(&mut checker, "y");
    let z = type_of(&mut checker, "z");
    assert_eq!(x, y, "`C<number>` twice is one type");
    assert_ne!(x, z, "`C<number>` and `C<string>` are not");
}

/// Members and property access (`bd tsr-4sc.7` fourth slice, `bd tsr-tl8`).
#[test]
fn a_property_access_has_the_type_of_the_property() {
    assert_eq!(
        type_of_declaration("interface I { a: string }\ndeclare const i: I;\nconst x = i.a;", "x"),
        "string"
    );
    assert_eq!(
        type_of_declaration("declare const o: { a: number };\nconst x = o.a;", "x"),
        "number"
    );
    // Through two accesses, so the receiver of the second is itself computed.
    assert_eq!(
        type_of_declaration(
            "interface Inner { b: string }\ninterface Outer { a: Inner }\n\
             declare const o: Outer;\nconst x = o.a.b;",
            "x"
        ),
        "string"
    );
    assert_eq!(
        type_of_declaration("class C { a: number; }\ndeclare const c: C;\nconst x = c.a;", "x"),
        "number"
    );
}

#[test]
fn a_property_that_is_not_there_is_a_gap_and_not_a_free_name() {
    // The `bd tsr-tl8` hazard, as a test: a local called `a` in scope must not
    // become the type of `o.a`. This is the one shape where this port could
    // answer *wrongly* where a gap belongs.
    assert_eq!(
        type_of_declaration(
            "declare const a: string;\ndeclare const o: { b: number };\nconst x = o.a;",
            "x"
        ),
        "error"
    );
    // A receiver this port cannot type takes the access with it.
    assert_eq!(type_of_declaration("const x = unknownThing.a;", "x"), "error");
    // A primitive receiver needs the apparent type from lib.d.ts (bd tsr-9or.1).
    assert_eq!(type_of_declaration(r#"const x = "abc".length;"#, "x"), "error");
    // Inherited members *were* a gap here and are no longer: base types are now
    // walked (`tests/members.rs`). The assertion is removed rather than inverted,
    // because the positive case belongs with the code that answers it.
    // An instantiated generic would find its target's *uninstantiated* members,
    // which would answer `T` where upstream answers `number`.
    assert_eq!(
        type_of_declaration(
            "class C<T> { a: T; }\ndeclare const c: C<number>;\nconst x = c.a;",
            "x"
        ),
        "error"
    );
}

/// The type of a declaration named `name`, wherever it is declared.
///
/// `type_of_declaration` looks only at the file's own locals, and `this` is only
/// interesting inside a class body — so these fixtures declare their `x` in a
/// method.
fn type_of_nested_declaration(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("`{name}` is declared nowhere");
}

#[test]
fn this_inside_a_class_is_the_class_this_type() {
    // Printed `this`, not `C` — upstream models it as a type parameter
    // constrained to the class, and the corpus records `>this : this`.
    assert_eq!(
        type_of_nested_declaration("class C { a: number; m() { const x = this; return x; } }", "x"),
        "this"
    );
    // Its members are the class's, which is what makes `this.a` work.
    assert_eq!(
        type_of_nested_declaration(
            "class C { a: number; m() { const x = this.a; return x; } }",
            "x"
        ),
        "number"
    );
    // An arrow function is transparent to `this`; a plain function is not, and
    // what `this` becomes there is upstream's signature machinery.
    assert_eq!(
        type_of_nested_declaration(
            "class C { a: number; m() { const f = () => { const x = this.a; return x; }; } }",
            "x"
        ),
        "number"
    );
    assert_eq!(
        type_of_nested_declaration(
            "class C { a: number; m() { function f() { const x = this; return x; } } }",
            "x"
        ),
        "error"
    );
    // Outside any class there is no container this port can answer for.
    assert_eq!(type_of_nested_declaration("const x = this;", "x"), "error");
}

// ---------------------------------------------------------------------------
// Function, class, enum and module symbols: `getTypeOfFuncClassEnumModule`
// (`bd tsr-4sc.8`).
//
// Ranked first by `examples/types_shapes.rs` at `78cfcba`: 35,488 gap lines are
// a symbol kind `getTypeOfSymbol` did not handle, and this is the only route to
// the two largest answer buckets still reading 0.00% — function/signature
// (55,421 lines) and `typeof X` (15,912), which is what a class or module
// symbol's type *prints* as rather than a separate feature.
//
// Every expected string below was taken from a corpus baseline under
// `vendor/typescript-go/testdata/baselines/reference/`, not from intuition: the
// whole line is compared verbatim, so the spelling is the thing under test.
// ---------------------------------------------------------------------------

/// The printed type of a member named `member` of the top-level declaration
/// named `owner`.
///
/// [`type_of_declaration`] reaches only a file's own locals, and a method lives
/// in its class's or interface's members table.
fn type_of_member(source: &str, owner: &str, member: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let owner = bound.lookup_local(root, owner).unwrap_or_else(|| panic!("`{owner}` is declared"));
    let symbol = *bound
        .symbols()
        .get(owner)
        .members
        .get(member)
        .unwrap_or_else(|| panic!("`{member}` is a member"));
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn a_function_symbol_is_its_call_signature() {
    // `conformance/typedefModuleExportsIndirect2.types` records `>f : () => void`
    // for a body with no `return`, and
    // `submodule/conformance/parserFunctionDeclaration1.types` records
    // `>F : () => any` for `declare function F();` — a body-less declaration's
    // return type is `any`, which is a computed answer and not a gap.
    assert_eq!(type_of_declaration("function f() {}", "f"), "() => void");
    assert_eq!(type_of_declaration("function f(x: string) {}", "f"), "(x: string) => void");
    assert_eq!(type_of_declaration("declare function f();", "f"), "() => any");
    assert_eq!(type_of_declaration("function f(): number { return 1; }", "f"), "() => number");
    // A method reaches the same arm: `getTypeOfSymbol` sends `SymbolFlagsMethod`
    // to `getTypeOfFuncClassEnumModule` alongside `SymbolFlagsFunction`.
    assert_eq!(type_of_member("interface I { m(): void }", "I", "m"), "() => void");
    assert_eq!(
        type_of_member("class C { m(x: number): void {} }", "C", "m"),
        "(x: number) => void"
    );
}

#[test]
fn a_void_return_is_inferred_only_where_no_return_statement_exists() {
    // Upstream's `checkAndAggregateReturnExpressionTypes` yields no types and is
    // not never-returning for a *function declaration*, so `getReturnTypeFromBody`
    // answers `void` — and it does so even for a body that only throws, because
    // `mayReturnNever` is false for this kind.
    assert_eq!(type_of_declaration("function f() { throw 1; }", "f"), "() => void");
    // A `return` with an expression needs a union of the return types, which is
    // `bd tsr-4sc.9`. It must read as a gap, not as `void`.
    assert_eq!(type_of_declaration("function f() { return 1; }", "f"), "error");
    // Async and generator return types are `Promise<T>` and `Generator<...>`,
    // references to globals that do not exist here (`bd tsr-9or.1`).
    assert_eq!(type_of_declaration("async function f() {}", "f"), "error");
    assert_eq!(type_of_declaration("function* f() {}", "f"), "error");
    // A `return` inside a *nested* function belongs to that function, so the
    // outer one still infers `void`. This is `ForEachReturnStatement`'s contract.
    assert_eq!(
        type_of_declaration("function f() { function g() { return 1; } }", "f"),
        "() => void"
    );
}

#[test]
fn an_overload_set_takes_the_overload_signature_and_not_the_implementation() {
    // `submodule/conformance/functionOverloadErrorsSyntax.types` records
    // `>fn4a : (x?: number, y: string) => any` for exactly this shape: the
    // implementation contributes no signature, so the printed type is the
    // overload's — including its `any` return, since the overload has no body.
    assert_eq!(
        type_of_declaration("function f(x?: number, y: string);\nfunction f() {}", "f"),
        "(x?: number, y: string) => any"
    );
    // Two *overloads* print as a type literal of call signatures,
    // `{ (): void; (x: string): void; }` (`nodebuilderimpl.go:2690` emits a bare
    // function type only for a single call signature). That rendering is
    // unported, and the first signature dressed up as the whole would be a wrong
    // answer where a gap belongs.
    assert_eq!(type_of_member("interface I { m(): void; m(x: string): void }", "I", "m"), "error");
}

#[test]
fn a_defaulted_parameter_is_optional_only_from_the_minimum_argument_count() {
    // `submodule/conformance/callSignaturesWithParameterInitializers.types`:
    // `function foo(x = 1) { }` records `>foo : (x?: number) => void`. The
    // parameter's own type is the *widened* initialiser, `number` and not `1`.
    assert_eq!(type_of_declaration("function f(x = 1) {}", "f"), "(x?: number) => void");
    // With a required parameter after it, `x` is at index 0 and the minimum
    // argument count is 2, so upstream's `isOptionalParameter` says no —
    // `parameterIndex >= minArgumentCount` is false. Treating every defaulted
    // parameter as optional would print `(x?: number, y: number)` here.
    assert_eq!(
        type_of_declaration("function f(x = 1, y: number) {}", "f"),
        "(x: number, y: number) => void"
    );
    // A `?` token is optional outright, whatever follows it.
    assert_eq!(
        type_of_declaration("function f(x?: number, y: number) {}", "f"),
        "(x?: number, y: number) => void"
    );
}

#[test]
fn a_this_parameter_is_printed_first() {
    // Upstream keeps `this` out of `signature.parameters` and the node builder
    // prepends it (`nodebuilderimpl.go:1792`). `docs/architecture/
    // checker-oracle.md` quotes `(this: any) => void` as a real baseline shape.
    assert_eq!(type_of_declaration("function f(this: any) {}", "f"), "(this: any) => void");
    assert_eq!(
        type_of_declaration("function f(this: any, x = 1) {}", "f"),
        "(this: any, x?: number) => void"
    );
    // **The exclusion itself is unobservable today**, and that is said here
    // rather than pinned by a test that would not bite. Folding `this` into
    // `parameters` shifts `minArgumentCount` by exactly one *and* shifts every
    // value parameter's list index by one, and the only thing either feeds is
    // `parameterIndex >= minArgumentCount` — so both sides move together and no
    // printed line changes. It becomes load-bearing with `getTypeAtPosition`
    // and call-argument checking, which is why the field is kept rather than
    // flattened; the same reasoning as the two guards recorded in
    // `docs/architecture/checker.md` under the binary operators.
}

#[test]
fn a_generic_function_prints_its_type_parameters() {
    assert_eq!(type_of_declaration("function f<T>(p: T): T { return p; }", "f"), "<T>(p: T) => T");
    assert_eq!(
        type_of_declaration("function f<T extends string>(p: T): void {}", "f"),
        "<T extends string>(p: T) => void"
    );
    assert_eq!(
        type_of_declaration("function f<T = number>(p: T): void {}", "f"),
        "<T = number>(p: T) => void"
    );
}

#[test]
fn a_class_enum_or_namespace_symbol_prints_as_a_type_query() {
    // The whole `typeof X` answer bucket — 15,912 corpus lines — is this arm.
    // A class's *declaration name* still records the instance type `C`
    // (`get_declared_type_of_symbol`); this is what a *reference* to it is.
    assert_eq!(type_of_declaration("class C {}", "C"), "typeof C");
    assert_eq!(type_of_declaration("enum E { A }", "E"), "typeof E");
    assert_eq!(type_of_declaration("namespace M { export const x = 1; }", "M"), "typeof M");
    // A merged function-and-namespace symbol takes the `typeof` form:
    // `shouldEmitTypeOfSymbol` (`nodebuilderimpl.go:2801`) tests enum and value
    // module as an `||` before falling through to the function case.
    assert_eq!(
        type_of_declaration("function f() {}\nnamespace f { export const x = 1; }", "f"),
        "typeof f"
    );
}

#[test]
fn a_shorthand_ambient_module_is_any_and_not_a_gap() {
    // `isShorthandAmbientModuleSymbol` (`utilities.go:198`) is the first branch of
    // `getTypeOfFuncClassEnumModuleWorker`, and `anyType` there is a computed
    // answer — a module with no body genuinely has no known shape.
    // The binder stores a string module's name unquoted, which is why the guard
    // below is on the declaration's name *node* and not on the stored text.
    assert_eq!(type_of_declaration("declare module \"x\";", "x"), "any");
    // With a body it is an ordinary value module, and upstream spells it
    // `typeof import("x")` — a form this port does not build, so a gap rather
    // than the `typeof x` its stored name would produce.
    assert_eq!(
        type_of_declaration("declare module \"x\" { export const a: number; }", "x"),
        "error"
    );
}

#[test]
fn a_type_query_carries_no_members_so_a_static_access_is_a_gap() {
    // `typeof C`'s properties are the class's *statics*, which live in the
    // symbol's `exports` table; `TypeData::Named` points `getPropertyOfType` at
    // `members`. Pointing it there would answer `C.x` with the *instance* `x` —
    // a wrong answer where a gap belongs, which is the one failure the
    // `errorType`-not-`anyType` rule exists to prevent.
    assert_eq!(
        type_of_declaration("class C { static s: string; x: number; }\nconst v = C.x;", "v"),
        "error"
    );
    assert_eq!(type_of_declaration("class C { static s: string; }\nconst v = C.s;", "v"), "error");
}

#[test]
fn a_signature_this_port_cannot_print_exactly_is_a_gap() {
    // A destructuring parameter: `parameterToParameterDeclarationName` invents a
    // name for a binding pattern, and an invented name compared verbatim is a
    // guess.
    assert_eq!(type_of_declaration("function f({ a }: { a: string }) {}", "f"), "error");
    // A parameter whose own type is a gap makes the whole signature a gap.
    // `string[]` is an array type node, still unported (`bd tsr-9or.1`).
    assert_eq!(type_of_declaration("function f(...r: string[]) {}", "f"), "error");
    // Likewise a constraint that does not resolve.
    assert_eq!(type_of_declaration("function f<T extends string[]>(): void {}", "f"), "error");
}

#[test]
fn a_function_symbols_type_is_memoised_on_the_symbol() {
    // The same memo upstream uses — `valueSymbolLinks.resolvedType` — so asking
    // twice must give the same *identity* and not two types that print alike.
    // Identity is what a relation check will compare.
    let arena = Arena::new();
    let source = "function f(x: string) {}";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "f").expect("`f` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let first = checker.get_type_of_symbol(symbol);
    let count = checker.type_count();
    let second = checker.get_type_of_symbol(symbol);
    assert_eq!(first, second, "one type per symbol");
    assert_eq!(checker.type_count(), count, "the second ask creates no type");
}

// ---------------------------------------------------------------------------
// Calls, function expressions and arrows (`bd tsr-4sc.8`, second slice).
//
// `CallExpression` is the largest single unported form in the corpus at 15,867
// gap lines; arrows and function expressions are a further 7,014. All three are
// the same machinery reached from three directions — a signature, and the
// anonymous object type that carries it.
// ---------------------------------------------------------------------------

#[test]
fn a_call_has_the_return_type_of_the_signature_it_resolves_to() {
    assert_eq!(type_of_declaration("declare function g(): number;\nconst x = g();", "x"), "number");
    assert_eq!(type_of_declaration("declare function g(): void;\nconst x = g();", "x"), "void");
    // A body-less declaration returns `any`, and a call to it therefore does too
    // — a computed answer that must not be confused with a gap.
    assert_eq!(type_of_declaration("declare function g();\nconst x = g();", "x"), "any");
    // An inferred `void` return survives the call.
    assert_eq!(type_of_declaration("function g() {}\nconst x = g();", "x"), "void");
    // Arguments are not checked and do not change the answer for a
    // non-generic signature, which is the whole reason skipping them is sound.
    assert_eq!(
        type_of_declaration("declare function g(a: number): string;\nconst x = g(1);", "x"),
        "string"
    );
}

#[test]
fn a_call_through_a_variable_resolves_because_the_type_carries_its_symbol() {
    // The reason `TypeData::Anonymous` exists rather than a lookup from the
    // callee's *name*: the callee here is a variable, and upstream asks the
    // callee's **type** for its signatures.
    assert_eq!(
        type_of_declaration("function g(): void {}\nconst h = g;\nconst x = h();", "x"),
        "void"
    );
    assert_eq!(type_of_declaration("const f = () => {};\nconst x = f();", "x"), "void");
}

#[test]
fn a_call_this_slice_cannot_resolve_is_a_gap_and_not_the_first_candidate() {
    // Overload resolution needs assignability. Taking the first candidate would
    // answer `number` here and be wrong for every call that picks a later one.
    //
    // **This line is currently enforced one level earlier than it reads.** An
    // overload set has no printed type yet, so the callee is already `errorType`
    // before the call is resolved, and mutating the single-candidate test in
    // `calls.rs` turns nothing red. That guard is kept rather than deleted
    // because it becomes the only thing standing between a call and a guess the
    // moment an overload set prints as `{ (): void; (x: string): void; }` — a
    // named future change, which is the same standard the `this`-parameter
    // exclusion is held to.
    assert_eq!(
        type_of_declaration(
            "declare function g(): number;\ndeclare function g(a: string): string;\nconst x = g();",
            "x"
        ),
        "error"
    );
    // A generic signature's return type depends on inference, so the
    // uninstantiated `T` would be a wrong answer rather than a missing one.
    assert_eq!(
        type_of_declaration("declare function g<T>(a: T): T;\nconst x = g(1);", "x"),
        "error"
    );
    // A class is not callable: upstream reports and answers `errorType`.
    assert_eq!(type_of_declaration("class K {}\nconst x = K();", "x"), "error");
    // An optional chain and explicit type arguments are both unported forms.
    assert_eq!(
        type_of_declaration("declare function g(): number;\nconst x = g?.();", "x"),
        "error"
    );
}

#[test]
fn a_function_expression_and_an_arrow_are_their_signature() {
    assert_eq!(type_of_declaration("const f = () => {};", "f"), "() => void");
    assert_eq!(type_of_declaration("const f = function () {};", "f"), "() => void");
    assert_eq!(type_of_declaration("const f = (x: number) => {};", "f"), "(x: number) => void");
    // A concise body is its expression's type, widened —
    // `getReturnTypeFromBody`'s `!ast.IsBlock(body)` arm.
    assert_eq!(type_of_declaration("const f = () => 1;", "f"), "() => number");
    assert_eq!(type_of_declaration("const f = (): number => 1;", "f"), "() => number");
}

#[test]
fn an_arrow_that_cannot_complete_is_never_and_one_that_can_is_void() {
    // The distinction upstream draws with `functionHasImplicitReturn` — the flow
    // graph — and this port draws syntactically, answering only where the grammar
    // forces it. `mayReturnNever` is what makes an arrow different from a
    // function declaration, whose empty body is `void` either way.
    assert_eq!(type_of_declaration("const f = () => { throw 1; };", "f"), "() => never");
    assert_eq!(type_of_declaration("const f = () => { let a = 1; };", "f"), "() => void");
    // Both halves of the `if` end the block, so the block ends.
    assert_eq!(
        type_of_declaration(
            "const f = (b: boolean) => { if (b) { throw 1; } else { throw 2; } };",
            "f"
        ),
        "(b: boolean) => never"
    );
    // Only one half does, so control can still reach the end.
    assert_eq!(
        type_of_declaration("const f = (b: boolean) => { if (b) { throw 1; } };", "f"),
        "(b: boolean) => void"
    );
    // A call in the body could be `never`-returning, and this port cannot yet
    // tell. It refuses rather than assuming the block completes.
    assert_eq!(type_of_declaration("const f = () => { g(); };", "f"), "error");
    // A loop is decidable and needs the real analysis to be decided correctly.
    assert_eq!(type_of_declaration("const f = () => { while (true) {} };", "f"), "error");
}

#[test]
fn an_unannotated_parameter_is_any_only_where_no_contextual_type_can_supply_one() {
    // The assertions go through the *initialiser* rather than the declaration,
    // because a declaration with an annotation takes the annotation and would
    // report `error` for an unrelated reason — a function **type node** is
    // itself unported, which is what blocks real contextual typing.
    //
    // The one place in this port where the implicit `any` is not safe: upstream
    // types `x` from the contextual signature, so answering `any` would be a
    // wrong line dressed as a computed one.
    assert_eq!(type_of_initialiser("const f = x => {};"), "(x: any) => void");
    assert_eq!(type_of_initialiser("const f: (x: number) => void = x => {};"), "error");
    // The same test guards a literal return. `const f = () => 1` widens to
    // `() => number` because nothing supplied a contextual return type;
    // `const f: () => 1 = () => 1` does not, because something did — so the
    // annotated form is a gap rather than the widened guess.
    assert_eq!(type_of_initialiser("const f = () => 1;"), "() => number");
    assert_eq!(type_of_initialiser("const f: () => 1 = () => 1;"), "error");
}

// ---------------------------------------------------------------------------
// Element access (`bd tsr-4sc.8`, third slice).
//
// 13,549 gap lines, the largest single unported form left once calls landed.
// It is not a second kind of lookup: upstream derives a property *name* from the
// index's **type** (`getPropertyNameFromIndex`, `checker.go:21786`) and then
// calls the same `getPropertyOfType` property access calls.
// ---------------------------------------------------------------------------

#[test]
fn a_literal_index_is_a_property_lookup_by_name() {
    assert_eq!(
        type_of_declaration("declare const a: { b: number };\nconst x = a[\"b\"];", "x"),
        "number"
    );
    // Nesting, and a method reached through an index then called — the three
    // slices of this issue meeting.
    assert_eq!(
        type_of_declaration(
            "declare const a: { b: { c: string } };\nconst x = a[\"b\"][\"c\"];",
            "x"
        ),
        "string"
    );
    assert_eq!(
        type_of_declaration(
            "class C { m(): number { return 1; } }\ndeclare const c: C;\nconst x = c[\"m\"]();",
            "x"
        ),
        "number"
    );
}

#[test]
fn the_index_name_comes_from_the_types_of_the_index_not_its_syntax() {
    // Upstream's choice, and it is worth more than it looks: `k` is an
    // *identifier*, so a syntactic reading would gap here. Its type is the
    // literal `"b"` — because a `const` keeps its literal type — so the
    // type-directed reading resolves it. This is the assertion that distinguishes
    // the two implementations.
    assert_eq!(
        type_of_declaration(
            "declare const a: { b: number };\nconst k = \"b\";\nconst x = a[k];",
            "x"
        ),
        "number"
    );
    // Parentheses do not change a type, so they do not change an index either.
    assert_eq!(
        type_of_declaration("declare const a: { b: number };\nconst x = a[(\"b\")];", "x"),
        "number"
    );
    // A **numeric** literal index names a property by its normalised text, which
    // is the same string `Number::toString` gives upstream — so `c[1]` and
    // `c[1.0]` name the same property `1`. (Reached through a class, because a
    // *type literal* with a numeric member name is still a gap on the
    // annotation side.)
    assert_eq!(
        type_of_declaration("class C { 1: boolean; }\ndeclare const c: C;\nconst x = c[1];", "x"),
        "boolean"
    );
    assert_eq!(
        type_of_declaration("class C { 1: boolean; }\ndeclare const c: C;\nconst x = c[1.0];", "x"),
        "boolean"
    );
    // A `let` widens to `string`, which names no property — the same rule read
    // from the other side, and a gap rather than a guess.
    assert_eq!(
        type_of_declaration(
            "declare const a: { b: number };\nlet k = \"b\";\nconst x = a[k];",
            "x"
        ),
        "error"
    );
}

#[test]
fn an_element_access_this_slice_cannot_resolve_is_a_gap() {
    // A non-literal index needs index signatures, which no type here has.
    assert_eq!(
        type_of_declaration(
            "declare const a: { b: number };\ndeclare const i: string;\nconst x = a[i];",
            "x"
        ),
        "error"
    );
    // No such property: upstream reports and answers `errorType`.
    assert_eq!(
        type_of_declaration("declare const a: { b: number };\nconst x = a[\"c\"];", "x"),
        "error"
    );
    // An optional chain is unported.
    assert_eq!(
        type_of_declaration("declare const a: { b: number };\nconst x = a?.[\"b\"];", "x"),
        "error"
    );
    // A receiver we cannot type takes the access with it.
    assert_eq!(type_of_declaration("const x = unknownThing[\"b\"];", "x"), "error");
}

#[test]
fn a_signature_prints_a_parameters_annotation_as_written_not_the_symbols_type() {
    // The two differ for an optional parameter, and upstream records both
    // spellings of the same one (`compiler/assertionWithNoArgument.types`, a
    // `@strict: true` case):
    //
    // ```text
    // export function assertWeird(value?: string): asserts value {
    // >assertWeird : (value?: string) => asserts value
    // >value : string | undefined
    // ```
    //
    // The declaration line prints the symbol's type, which carries the
    // `| undefined` a `?` adds; the signature reuses the annotation node.
    // `serializeTypeForDeclaration` (`nodebuilderimpl.go:2216`) is where upstream
    // makes that swap, and `(value?: string | undefined)` appears nowhere in the
    // corpus.
    //
    // **Both directions, on the same declaration.** Asserting only the signature
    // would pass under a printer that dropped `| undefined` *everywhere*,
    // including from the parameter's own line — where upstream wants it. The two
    // are different questions with different right answers, and it is the pair
    // that pins the rule.
    //
    // Worth being exact about what each half covers, because they are not
    // symmetric: the first assertion goes through `get_type_of_symbol` and does
    // **not** touch the signature printer at all, so it guards the *pair* against
    // being collapsed by a change on either side rather than guarding this
    // crate's printer. The printer itself is pinned by the second, and both
    // branches of its rule are mutation-verified — using the symbol type turns
    // this red, and dropping the unannotated branch turns
    // `an_unannotated_parameter_is_any_only_where_no_contextual_type_can_supply_one`
    // red.
    assert_eq!(type_of_nested_declaration("function f(x?: number) {}", "x"), "number | undefined");
    assert_eq!(type_of_declaration("function f(x?: number) {}", "f"), "(x?: number) => void");
    assert_eq!(
        type_of_declaration("declare function f(x?: number): void;", "f"),
        "(x?: number) => void"
    );
    assert_eq!(
        type_of_declaration("declare function f(x?: string, y?: number): void;", "f"),
        "(x?: string, y?: number) => void"
    );
}

#[test]
fn a_function_carrying_expando_properties_is_a_gap_not_its_bare_signature() {
    // `createTypeNodeFromObjectType` (`nodebuilderimpl.go:2698`) emits a bare
    // `FunctionTypeNode` only when the resolved type has no properties and no
    // index signatures. `function f() {} f.a = "s";` has one, and upstream
    // prints `{ (): void; a: string; }`.
    //
    // Printing `() => void` there is a *wrong* answer, not a partial one — it
    // looks like a result. Found by a bounded differential over 302 corpus
    // baselines: it was the largest identified pattern left inside
    // `getTypeOfFuncClassEnumModule`, and gapping it took that sample's wrong
    // lines from 281 to 253.
    assert_eq!(type_of_declaration("function f(): void {}\nf.a = \"s\";", "f"), "error");

    // **The other direction, or the guard would swallow every function.** A
    // plain function has no such properties and still prints its signature.
    assert_eq!(type_of_declaration("function f(): void {}", "f"), "() => void");

    // And a function merged with a namespace is unaffected, because the type
    // *query* arm answers first: `shouldEmitTypeOfSymbol` returns true for a
    // value module before the function case is reached, so the exports that
    // would gap a signature are exactly what `typeof f` is meant to carry.
    assert_eq!(
        type_of_declaration("function f(): void {}\nnamespace f { export const a = 1; }", "f"),
        "typeof f"
    );
}
