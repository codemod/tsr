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
        &arena,
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
fn positive_nonfinite_numeric_literals_keep_their_value_when_infinity_is_shadowed() {
    // Pinned native 5b1047d emits Infinity for overflow literals, independently
    // of a same-spelled value binding; it does not turn them into references.
    assert_eq!(type_of_initialiser("const x = 1e999;"), "Infinity");
    assert_eq!(type_of_initialiser("const x = +2e999;"), "Infinity");
    assert_eq!(type_of_declaration("const Infinity = 'held'; const x = 2e999;", "x"), "Infinity");
    assert_eq!(type_of_declaration("let x: 1e999;", "x"), "Infinity");
    assert_eq!(type_of_declaration("const Infinity = 'held';", "Infinity"), "\"held\"");
    assert_eq!(type_of_initialiser("const x = 1e308;"), "1e+308");
}

#[test]
fn negative_nonfinite_numeric_literals_and_annotations_share_the_same_value() {
    assert_eq!(type_of_initialiser("const x = -1e999;"), "-Infinity");
    // Native widens a composed unary expression, even if its operand is a literal.
    assert_eq!(type_of_initialiser("const x = -(-2e999);"), "number");
    assert_eq!(type_of_declaration("let x: -1e999;", "x"), "-Infinity");
    assert_eq!(type_of_initialiser("const x = -0;"), "0");
    assert_eq!(type_of_initialiser("const x = -1.5;"), "-1.5");
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
    // `f()` stopped being the stand-in at §24; `satisfies` stopped at §287
    // (transparent, upstream's checkSatisfiesExpression). The current
    // genuinely-unported expression form is a destructuring ASSIGNMENT —
    // `[a] = b` — whose pattern half is bd tsr-4sc.13.
    let source = "declare var a: number, b: number[];\nconst x = ([a] = b);";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[1] else {
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
        &arena,
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
        &arena,
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
        &arena,
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
        &arena,
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
        &arena,
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
    // `keyof` rather than a tuple: the plain tuple form is ported
    // (`docs/architecture/checker-notes-tuple.md`) and computes a real type,
    // so it no longer demonstrates the property this test is named for.
    let source = "declare const x: keyof string;";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
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

// `a_symbol_shape_this_slice_does_not_port_is_an_error_type` was DELETED here,
// not re-pointed. It asserted that an unported `getTypeOfSymbol` shape answers
// `errorType` and not `anyType`, using whatever shape happened to be unported as
// the fixture. It had already been re-pointed once — from a function symbol to
// an accessor — when its first fixture was ported, and the accessor arm has now
// been ported too.
//
// A third re-pointing would keep the test alive without keeping it honest: with
// every `SymbolFlags` shape `getTypeOfSymbol` dispatches on now answered, the
// premise has no fixture left that is unported for a structural reason rather
// than a not-yet-done one. Its two claims are guarded where they belong —
// `the_intrinsics_that_print_alike_are_still_distinct_types` pins the
// `errorType`/`anyType` identity directly, and
// `an_unported_expression_form_is_error_not_any` pins the discipline on the
// expression side, where unported forms still exist.

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
        &arena,
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
fn an_unresolved_name_is_upstreams_any() {
    // The name outlived its truth at §31: upstream's TS2304 answer IS
    // `errorType` printed `any`, and this port now answers the observable.
    let arena = Arena::new();
    let source = "const a = nowhere;";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "a").expect("`a` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    // §31 (`checker-notes-narrow.md`): a truly unresolved free name in an
    // import-free file answers upstream's TS2304 `errorType` — whose
    // OBSERVABLE is `any`, and this port answers the `any` intrinsic
    // directly. The fixture's original claim (errorType, to keep gaps
    // distinguishable) was ADR-0038's; §31 argues the boundary: this is
    // upstream's own deliberate error-answer, not a port failure.
    assert_eq!(id, checker.intrinsics().any);
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
    // This fixture was a binding element until `bd tsr-o00` ported
    // `getTypeForBindingElement` and the assertion went red — the second
    // "unported stand-in" fixture to come due this way after the tuple
    // build's ten (`checker-notes-tuple.md`). Rewritten as the pair: the
    // destructured name now *answers* (positive control), and the worker's
    // catch-all is pinned by a kind that is still genuinely unported there —
    // a JSX attribute is not constructible in a `.ts` harness, so the pin is
    // the destructuring *assignment target* declaration form below plus the
    // still-refused legs in `tests/destructure.rs`. The catch-all itself is
    // exercised by `a_symbol_shape_this_slice_does_not_port_is_an_error_type`.
    let arena = Arena::new();
    let source = "const { a } = { a: 1 };";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "a").expect("`a` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(
        checker.type_to_string(id),
        "number",
        "the binding element answers its member's type since `bd tsr-o00`"
    );
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
fn an_unported_arithmetic_operand_answers_number_as_upstream_does() {
    // §271 flipped this pin (the `*` half: upstream's any-like → number rule,
    // measured 31:0). §287 then made `satisfies` transparent, so BOTH lines
    // now compute the real arithmetic — which is upstream's own answer for
    // these programs. The `+` arm's error-propagation is still pinned, by
    // `an_unresolved_arithmetic_operand_answers_number_and_addition_keeps_the_gap`
    // in unresolved_type_reference.rs, on a genuinely unresolved operand.
    assert_eq!(type_of_initialiser("const x = (1 satisfies number) * 2;"), "number");
    assert_eq!(type_of_initialiser("const x = (1 satisfies number) + 2;"), "number");
}

#[test]
fn or_and_nullish_are_still_a_gap_and_and_is_not() {
    // **This test went red when `&&` landed, which is what it is for.** It
    // previously asserted all three as gaps on the grounds that "a && b is a
    // union of the operands, and unions do not exist". Unions arrived, and then
    // `&&` turned out to be separable from the other two for a reason the
    // grounds never mentioned: `||` and `??` reduce with
    // `UnionReductionSubtype` and need assignability, and `&&` does not. See
    // [`crate::binary`] and `docs/architecture/checker-notes-armsplit.md` §3.1.
    //
    // `1 && 2` is `2`: the definitely-falsy part of the literal `1` is `never`,
    // and `getUnionType([never, 2])` drops it. The tempting wrong answers are
    // the *left* type, right only when the left can never be falsy, and the
    // plain union `1 | 2`.
    assert_eq!(type_of_initialiser("const x = 1 && 2;"), "2");
    // The other two went green in the ninth session's reduction-free slice
    // (`checker-notes-assign.md` §8) — the fourteenth stand-in to come due.
    // A never-falsy, never-nullish left short-circuits both operators to the
    // LEFT type unchanged (`checker.go:12510`, `:12523`): the playground
    // agrees that `const a = 1 || 2` is `1`, however tempting `1 | 2` reads.
    assert_eq!(type_of_initialiser("const x = 1 || 2;"), "1");
    assert_eq!(type_of_initialiser("const x = 1 ?? 2;"), "1");
}

#[test]
fn a_destructuring_assignment_answers_its_right_hand_side() {
    // FLIPPED at §365. The old pin asserted the gap while the pattern's own
    // typing was unported — its stated reason ("taking the right-hand type
    // would silently mistype the pattern") named the missing piece, and §365
    // built it: `checkDestructuringAssignment` (checker.go:12683) checks the
    // pattern and ANSWERS THE RIGHT TYPE, with the RHS tuple/object context
    // supplied by the pattern (§76's slot machinery). What the old pin got
    // right: without the pattern road, the fall-through really did mistype
    // the pattern line. What replaces it: `iterableArrayPattern3`'s
    // `[a, b] = new FooIterator : FooIterator`, and the playground's
    // `const x = ([a] = [1])` reading `[number]`.
    assert_eq!(type_of_initialiser("const x = ([a] = [1]);"), "[number]");
    assert_eq!(type_of_initialiser("const x = ({ a } = { a: 1 });"), "{ a: number; }");
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
    // getDeclaredTypeOfTypeAlias (`checker.go:23837`): a direct cycle fails
    // every participant's frame, reports "circularly references itself" and
    // answers `errorType`. §29's NAME placeholder now serves only a mention
    // inside a construct native resolves lazily (`type L = { next: L }`).
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
    // types, which prints as the enum's name. This comment used to record that
    // the port built a *named* type printing the same string instead — a
    // divergence invisible in a printed line. That was true when the test was
    // written and stopped being true at `038def4`, which replaced the named type
    // with the union; the behavioural assertion now lives in
    // `an_enum_declares_a_real_union_that_prints_as_the_enum_name`
    // (`tests/unions.rs`), because printing alone cannot tell the two apart.
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
        &arena,
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
fn a_qualified_name_resolves_through_the_namespace_and_prints_what_was_written() {
    // `M.I` goes through `resolveEntityName` and module exports. Resolving only
    // the left-hand identifier would answer the module's type for the member,
    // which is the plausible wrong thing.
    // The left-hand name is given a declared type of its own here — `M` is an
    // interface as well as a namespace — so a version that resolved `M.I` by its
    // left identifier would answer `M` rather than `M.I`. Without that merge the
    // test passes whether or not qualified names are handled, which is the kind
    // of test this project has shipped before.
    //
    // This assertion was `"error"` until design W landed; the behaviour it now
    // records is owned by `tests/qualified_type_reference.rs`.
    assert_eq!(
        type_of_declaration(
            "interface M {}\nnamespace M { export interface I {} }\ndeclare const x: M.I;",
            "x"
        ),
        "M.I"
    );
}

/// The type a *type declaration* declares, by the declaration's name.
fn declared_type_of(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
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
    //
    // **The method and call-signature cases moved on 2026-08-05** (`4bf4b8c`),
    // which taught `get_type_from_type_literal` to render signature members.
    // They are kept here rather than deleted because this test's subject is the
    // *rule*, not the list: a member the port cannot render still takes the
    // whole literal with it. Deleting them would leave the rule pinned by a
    // shrinking set.
    //
    // **The bare call signature moved too**, on the slice that gave
    // `signature_parts_of` its `CallSignatureDeclaration` and
    // `ConstructSignatureDeclaration` arms — `tests/signature_members_of_a_literal.rs`.
    // `{ (): void; }` is recorded 22 times in the baselines. What pins the rule
    // now is the symbol-keyed index signature and the tuple member below,
    // neither of which this port can render.
    //
    // **The index-signature case moved too**, on the slice that taught the same
    // function to render `[k: string]: T` members — `tests/index_signature_members.rs`.
    // §32 (`checker-notes-callres.md`) then widened the PRINT gate to any
    // computable non-union key; the lookup side stays intrinsic-only, and a
    // print the lookup cannot serve gaps the access rather than wronging it.
    assert_eq!(type_of_declaration("declare const x: { m(): void };", "x"), "{ m(): void; }");
    // The single call-signature member collapses to the arrow form (§10.15).
    assert_eq!(type_of_declaration("declare const x: { (): void };", "x"), "() => void");
    assert_eq!(
        type_of_declaration("declare const x: { [k: string]: string };", "x"),
        "{ [k: string]: string; }"
    );
    assert_eq!(
        type_of_declaration("declare const x: { [k: symbol]: string };", "x"),
        "{ [k: symbol]: string; }"
    );
    // **§930 inverted this.** A member whose own type is a gap no longer takes
    // the whole literal with it: it keeps the written spelling with an `any`
    // type, because upstream's member carries `errorType` and the node builder
    // reuses the written annotation node. `{ a: string; b: Array }` printed
    // `error`, losing the perfectly good `a` as well. +33 on the corpus, zero
    // `RIGHT->WRONG`.
    assert_eq!(
        type_of_declaration("declare const x: { a: keyof string };", "x"),
        "{ a: keyof string; }"
    );
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
    // FLIPPED at §282: this asserted `Tree<T>` for a body that IS the bare
    // parameter, which was the port's display shortcut and not upstream's
    // rule — `type Bar1<T extends unknown[][]> = T` records `Bar1 : T` in
    // `substitutionTypePassedToExtends.types`, because upstream attaches an
    // alias symbol only to types CREATED during the resolution and a
    // pre-existing type parameter keeps its own display. A NON-trivial body
    // still prints the alias name (the `A<number>` reference above).
    assert_eq!(declared_type_of("type Tree<T> = T;", "Tree"), "T");
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
    // **This assertion was `"error"` and is corrected.** A bare reference whose
    // every parameter is defaulted is *inside* upstream's window —
    // `minTypeArgumentCount` is zero — so it fills and prints `C<string>`. The
    // old expectation recorded this port's refusal, not upstream's rule; the
    // arms above it are unchanged and are the actual arity check.
    // `checker-notes-printseam.md` §9.
    assert_eq!(
        type_of_declaration("class C<T = string> {}\ndeclare const x: C;", "x"),
        "C<string>"
    );
}

#[test]
fn a_gap_in_a_type_argument_is_a_gap_in_the_reference() {
    // `C<Unported>` is not `C<any>`. Printing the reference with a guessed
    // argument would turn a missing line into a wrong one.
    assert_eq!(
        type_of_declaration("class C<T> {}\ndeclare const x: C<keyof string>;", "x"),
        "error"
    );
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
        &arena,
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
fn a_signature_typed_member_of_a_generic_is_instantiated() {
    // `bd tsr-0hc`: the member's declared type is a baked signature, and the
    // receiver's `T` substitutes inside it — parameters and return both. The
    // wrong answers separated: the uninstantiated text prints `(cb: (value: T)
    // => string) => T[]`; the pre-0hc behaviour prints `error`.
    assert_eq!(
        type_of_declaration(
            "interface Array<T> { }\ninterface P<T> { m(cb: (value: T) => string): T[]; }\ndeclare const p: P<number>;\nconst x = p.m;",
            "x"
        ),
        "(cb: (value: number) => string) => number[]"
    );
    // A *call* through the instantiated member resolves from the signatures
    // recorded ON the type (`bd tsr-1uz`), never from the minted type's
    // symbol — whose declarations are uninstantiated and would answer `T[]`,
    // the wrong line the resolver's guard was born to prevent. This asserted
    // `error` between `tsr-0hc` and `tsr-1uz`, when the guard refused
    // outright.
    assert_eq!(
        type_of_declaration(
            "interface Array<T> { }\ninterface P<T> { m(cb: (value: T) => string): T[]; }\ndeclare const p: P<number>;\nconst y = p.m(0 as any);",
            "y"
        ),
        "number[]"
    );
    // A member whose signature mentions only the method's OWN type parameter
    // survives unrenamed and un-substituted, upstream's behaviour for
    // `then<TResult1>`-shaped members.
    assert_eq!(
        type_of_declaration(
            "interface P<T> { pick<U>(x: U): U; }\ndeclare const p: P<number>;\nconst z = p.pick;",
            "z"
        ),
        "<U>(x: U) => U"
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
    // §31: a truly unresolved receiver is upstream's TS2304 `any`, and a
    // property access on `any` is `any` — both upstream's own answers.
    assert_eq!(type_of_declaration("const x = unknownThing.a;", "x"), "any");
    // A primitive receiver needs the apparent type from lib.d.ts (bd tsr-9or.1).
    assert_eq!(type_of_declaration(r#"const x = "abc".length;"#, "x"), "error");
    // Inherited members *were* a gap here and are no longer: base types are now
    // walked (`tests/members.rs`). The assertion is removed rather than inverted,
    // because the positive case belongs with the code that answers it.
    // An instantiated generic used to be asserted `error` here: the lookup
    // would have found the target's *uninstantiated* members and answered `T`,
    // so `create_type_reference` carried no members table at all. `bd tsr-4qx`
    // added substitution at the `get_type_of_property_of_type` seam, and the
    // assertion flips to upstream's answer. The hazard this test is named for
    // is unchanged — the wrong answer to guard against is now `T`, not a free
    // name, and `number` is the only string that proves the substitution ran.
    assert_eq!(
        type_of_declaration(
            "class C<T> { a: T; }\ndeclare const c: C<number>;\nconst x = c.a;",
            "x"
        ),
        "number"
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
        &arena,
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

/// `bd tsr-tjz`, arm 1. A written `this` parameter is what `this` types as.
///
/// Both mutations were run and seen red, and the second one is the reason the
/// third fixture exists.
///
/// - **M1** — delete the `this_parameter_type` test from
///   `check_this_expression`'s walk. Red at `left: "error"`, `right: "I"`.
/// - **M2** — make `this_parameter_type` return `None` for a
///   `MethodDeclaration`, which is what "test class-ness first" amounts to,
///   since a method is the only container both arms can claim. Red at
///   **`left: "this"`, `right: "I"`** — the exact wrong answer the spec warned a
///   naive port would give.
///
/// A third rearrangement was tried first — moving the test below the whole match
/// — and it went red on the *first* fixture at `"error"` rather than on the
/// third. That proves the arm must precede the **function** arm and says nothing
/// about the class one, so it is recorded here as insufficient rather than
/// quoted as evidence.
#[test]
fn an_annotated_this_parameter_is_what_this_types_as() {
    // A plain function has no `this` until the source annotates one; then it
    // does. `tryGetThisTypeAtEx` (`checker.go:12146`) asks the container's
    // signature before anything else.
    assert_eq!(
        type_of_nested_declaration(
            "interface I { a: number; } function f(this: I) { const x = this; return x; }",
            "x"
        ),
        "I"
    );
    // §39 (`checker-notes-narrow.md`): a plain function's `this` is `any`
    // in every mode — TS2683 is `noImplicitThis`'s diagnostic, not a type
    // change.
    assert_eq!(
        type_of_nested_declaration("function f() { const x = this; return x; }", "x"),
        "any"
    );
    // **Arm 1 shadows arm 2.** A method *is* function-like, so upstream reaches
    // the signature's `this` type before `ast.IsClassLike(container.Parent)` and
    // never consults the class. A port that tests class-ness first prints
    // `this` here — a wrong line, not a missing one.
    assert_eq!(
        type_of_nested_declaration(
            "interface I { a: number; } class C { m(this: I) { const x = this; return x; } }",
            "x"
        ),
        "I"
    );
    // And with no `this` parameter the method still falls through to the class,
    // which is also upstream's order: `getThisTypeOfSignature` answers nothing
    // and the class arm runs next.
    assert_eq!(
        type_of_nested_declaration("class C { m() { const x = this; return x; } }", "x"),
        "this"
    );
    // §39: an unannotated `this` parameter is the implicit `any` — now the
    // same fallthrough answer a plain function's `this` takes, and
    // upstream's.
    assert_eq!(
        type_of_nested_declaration("function f(this) { const x = this; return x; }", "x"),
        "any"
    );
}

/// The printed type of the first `super` in `source`.
///
/// Direct rather than through a declaration, because the rule under test is
/// about `super` **itself** — `super(...)` answers a different type from
/// `super.x` in the same constructor, and only a helper that types the keyword
/// can tell them apart.
fn type_of_super(source: &str) -> String {
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source:?}");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = tsr_checker::Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if parsed.nodes.kind(id) != tsr_ast::SyntaxKind::SuperKeyword {
            continue;
        }
        let Some(node) = parsed.node_map.get(id) else { continue };
        let Ok(expression) = tsr_ast::Expression::try_from(node) else { continue };
        let type_id = checker.check_expression(expression);
        return checker.type_to_string(type_id);
    }
    "<no super>".to_string()
}

/// `bd tsr-h1s`. **A `super(...)` call is the base *constructor* type, and that
/// is not about `static`.**
///
/// Both mutations were run and seen red before the fix landed.
///
/// - **M1** — split on `is_static` alone, dropping `|| is_call`. Red on the
///   first fixture at **`left: "B"`, `right: "typeof B"`**, which is exactly the
///   `ours Base / them typeof Base` cluster the corpus produced 151 times.
/// - **M2** — delete the `SuperKeyword` arm from the keyword dispatch. Red on
///   every fixture at `"error"`.
#[test]
fn a_super_call_is_the_base_constructor_and_a_super_property_is_the_instance() {
    // `checker.go:7946` — `ast.IsStatic(container) || isCallExpression`. The
    // container here is an ordinary instance constructor and the answer is still
    // the static side, because what `super(...)` calls is the base constructor.
    assert_eq!(
        type_of_super("class B {} class C extends B { constructor() { super(); } }"),
        "typeof B"
    );
    // The same constructor, the same container, a different `super`.
    assert_eq!(type_of_super("class B { x: number; } class C extends B { m() { super.x; } }"), "B");
    // And `static` on its own still selects the static side.
    assert_eq!(
        type_of_super(
            "class B { static x: number; } class C extends B { static m() { super.x; } }"
        ),
        "typeof B"
    );
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
    // §39: the nested plain function REBINDS `this` — to `any`, upstream's
    // typed answer, not the enclosing class's this-type and not a gap.
    assert_eq!(
        type_of_nested_declaration(
            "class C { a: number; m() { function f() { const x = this; return x; } } }",
            "x"
        ),
        "any"
    );
    // §200: a stand-in come due. This asserted `error` on the stated ground
    // that "outside any class there is no container this port can answer for",
    // which was a fact about the port and not about upstream —
    // `tryGetThisTypeAtEx`'s last arm (`checker.go:12175-12184`) answers
    // `getTypeOfSymbol(globalThisSymbol)` at the top level of a SCRIPT.
    assert_eq!(type_of_nested_declaration("const x = this;", "x"), "typeof globalThis");
    // The other half of that arm, and the reason it is a pair: at the top level
    // of an external MODULE, `this` is `undefined` — a module body's `this` is
    // `undefined` at runtime, and upstream returns `undefinedType` before it
    // ever reaches `globalThis`.
    assert_eq!(type_of_nested_declaration("export {};\nconst x = this;", "x"), "undefined");
    // And a `namespace` or `enum` body is its OWN `this` container
    // (`getThisContainer`, `checker.go:12225`), so the walk must not reach
    // through one to the file. Upstream reports there and types it `any`.
    assert_eq!(type_of_nested_declaration("namespace N { const x = this; }", "x"), "any");
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
        &arena,
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
    // A `return` with an expression is inferred when the body's returns yield a
    // single distinct type — `tests/return_inference.rs`. This assertion pinned
    // `error` until that landed; the pin was the *gap*, not the answer.
    // `submodule/conformance/logicalAssignment10(target=es2021).types:13`
    // records `>incr : () => number` for this shape — a block-bodied function
    // declaration with one `return` of a numeric expression.
    //
    // What must not happen either way is `void`: this test exists to pin that
    // the `void` arm is reached only by a body with no valued `return` at all.
    assert_eq!(type_of_declaration("function f() { return 1; }", "f"), "() => number");
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
    // Two *overloads* print as a type literal of call signatures — see
    // [`an_overload_set_prints_as_a_type_literal_of_call_signatures`].
    assert_eq!(
        type_of_member("interface I { m(): void; m(x: string): void }", "I", "m"),
        "{ (): void; (x: string): void; }"
    );
}

#[test]
fn an_overload_set_prints_as_a_type_literal_of_call_signatures() {
    // `createTypeNodeFromObjectType` (`nodebuilderimpl.go:2690`) emits a bare
    // `FunctionTypeNode` only for a resolved type with exactly one call
    // signature (`nodebuilderimpl.go:2706`); two or more fall through to the
    // type-literal arm (`nodebuilderimpl.go:2740`), whose members are the call
    // signatures in declaration order rendered as *members* — a colon before
    // the return type, not an arrow.
    //
    // `submodule/conformance/anyAssignabilityInInheritance.types:18` records
    // exactly this fixture:
    //
    // ```text
    // declare function foo2(x: number): number;
    // >foo2 : { (x: number): number; (x: any): any; }
    // ```
    assert_eq!(
        type_of_declaration(
            "declare function foo2(x: number): number;\ndeclare function foo2(x: any): any;",
            "foo2"
        ),
        "{ (x: number): number; (x: any): any; }"
    );
    // A method reaches the same arm.
    // `submodule/conformance/memberFunctionsWithPublicPrivateOverloads.types:8`
    // records `>foo : { (x: number): any; (x: number, y: string): any; }` for
    // this class, implementation excluded and modifiers unprinted.
    assert_eq!(
        type_of_member(
            "class C {\n    private foo(x: number);\n    public foo(x: number, y: string);\n    private foo(x: any, y?: any) { }\n}",
            "C",
            "foo"
        ),
        "{ (x: number): any; (x: number, y: string): any; }"
    );
    // Three of them, to pin that the separator is `; ` *between* members and
    // that there is one more `;` before the closing brace rather than a
    // separator-joined list — the two spellings agree at two members only for
    // the trailing one, so a two-signature fixture alone would not tell them
    // apart. Same baseline, line 22, dropping the string-literal parameter
    // whose quotes upstream reproduces from the source.
    assert_eq!(
        type_of_member(
            "class C {\n    bar(x: boolean);\n    bar(x: string);\n    bar(x: number, y: string);\n    bar(x: any, y?: any) { }\n}",
            "C",
            "bar"
        ),
        "{ (x: boolean): any; (x: string): any; (x: number, y: string): any; }"
    );
}

#[test]
fn an_overload_set_preserves_its_expando_properties() {
    // `createTypeNodeFromObjectType` emits signatures before properties.
    assert_eq!(
        type_of_declaration(
            "function f(x: number);\nfunction f(x: string);\nfunction f(x: any) {}\nf.a = \"s\";",
            "f"
        ),
        "{ (x: number): any; (x: string): any; a: string; }"
    );
    // §48 rendered the plain pattern, so the second overload answers now
    // and the set prints whole — the gapping-member property this pinned
    // moved to DECORATED patterns, which still gap sets.
    assert_eq!(
        type_of_declaration("function f(x: number);\nfunction f({ a }: any);", "f"),
        "{ (x: number): any; ({ a }: any): any; }"
    );
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
    // **The symbol is named `"x"`, quotes included**, which is
    // `getDeclarationName` (`internal/binder/binder.go:311`) and is what keeps
    // an ambient module from colliding with an ordinary global of the same name
    // (`docs/architecture/checker-notes-diag2.md` §202). Both lookups in this
    // test spell the key that way; they read `"x"` bare until the rename, and
    // the comment here recorded that as a property of the port.
    assert_eq!(type_of_declaration("declare module \"x\";", "\"x\""), "any");
    // With a body it is an ordinary value module. Upstream spells it
    // `typeof import("x")` — a form this port does not build — and until the
    // `tryFindAmbientModule` slice (`checker-notes-modobj.md` §10) that
    // refusal lived *here*, at type creation, which also kept every line
    // `through` the module at `errorType` after it resolved. The refusal now
    // lives in the rendering path: the type exists, its baked `typeof x` is a
    // placeholder, and `type_to_string_at` — the entry point every baseline
    // line renders through — refuses to name a module with no alias in scope,
    // so the *rendered* answer is still a gap. Both halves pinned:
    let source = "declare module \"x\" { export const a: number; }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "\"x\"").expect("`\"x\"` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_ne!(id, checker.intrinsics().error, "the module object now has a type");
    // **Flipped by §143** (`checker-notes-narrow.md`; the thirty-sixth
    // stand-in): with no alias in scope, a single-declaration ambient
    // module spells the import form — `getSpecifierForModuleSymbol`'s
    // ambient half, `typeof import("x")` verbatim.
    assert_eq!(
        checker.type_to_string_at(id, root),
        Some("typeof import(\"x\")".to_string()),
        "no alias is in scope, so the ambient spells its import form"
    );
}

#[test]
fn a_type_query_answers_from_exports_and_never_from_members() {
    // `typeof C`'s properties are the class's *statics*, which live in the
    // symbol's `exports` table, while `TypeData::Named` points
    // `getPropertyOfType` at `members`. This test was written when neither
    // resolved, to pin that the lookup must never be *repointed* at `members`:
    // that would answer `C.x` with the **instance** `x` — a wrong answer where a
    // gap belongs.
    //
    // The gap half is now closed by a second arm reading `exports`
    // (`crate::members::get_property_of_anonymous_symbol`,
    // `checker.go:20672`), so the two assertions have come apart, which is
    // exactly what the original comment predicted should happen.

    // **Flipped by §124** (`checker-notes-narrow.md`; the thirty-third
    // stand-in): `x` is an instance member, so `typeof C` has no `x`, and the
    // ESTABLISHED miss now answers upstream's TS2339 error-any rather than
    // the gap sentinel. The pin's load-bearing claim is UNCHANGED and still
    // discriminates: repointing the arm at `members` would answer the
    // instance's `number` here, and `any` is not `number`.
    assert_eq!(
        type_of_declaration("class C { static s: string; x: number; }\nconst v = C.x;", "v"),
        "any"
    );
    // No longer `error`. `submodule/conformance/
    // protectedStaticClassPropertyAccessibleWithinSubclass.types:14` records
    // `>Base.x : string` for a `static x: string` read through the class name.
    assert_eq!(type_of_declaration("class C { static s: string; }\nconst v = C.s;", "v"), "string");
}

#[test]
fn a_signature_this_port_cannot_print_exactly_is_a_gap() {
    // §48 (`checker-notes-narrow.md`): a PLAIN pattern renders verbatim;
    // the invented-name fear applies only to decorated shapes now.
    assert_eq!(
        type_of_declaration("function f({ a }: { a: string }) {}", "f"),
        "({ a }: { a: string; }) => void"
    );
    // **§929 inverted both of these.** A parameter or constraint whose own type
    // does not resolve no longer takes the signature down: it keeps the written
    // spelling with an `any` type, because upstream's parameter carries
    // `errorType` and the node builder reuses the written annotation node. +442
    // on the corpus with zero `RIGHT->WRONG`.
    //
    // `string[]` is the gap here only because **this harness mounts no lib**, so
    // `Array` does not resolve; with the corpus's libs it resolves and §929 never
    // fires on these two shapes. The assertions record what this harness now
    // produces, which is also what upstream prints.
    assert_eq!(
        type_of_declaration("function f(...r: string[]) {}", "f"),
        "(...r: string[]) => void"
    );
    assert_eq!(
        type_of_declaration("function f<T extends string[]>(): void {}", "f"),
        "<T extends string[]>() => void"
    );
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
        &arena,
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
    // Overload resolution needs assignability, and `choose_overload` in
    // `calls.rs` now has it over the primitive domains — so the zero-argument
    // call below **resolves**, by arity, to the zero-parameter candidate. This
    // assertion used to demand `error`; it was pinning the absence of a
    // mechanism rather than an answer, and the mechanism arrived. See
    // `docs/architecture/checker-notes-resolve.md`.
    assert_eq!(
        type_of_declaration(
            "declare function g(): number;\ndeclare function g(a: string): string;\nconst x = g();",
            "x"
        ),
        "number"
    );
    // FLIPPED at §387, by the same refinement as its overload_resolution.rs
    // sibling: a missing required member on a COMPLETELY-enumerated source is
    // now a decidable non-match, so `{ v: number }` rejects `(a: { u: number })`
    // and the second candidate answers — `string`, upstream's pick, where a
    // first-candidate guess would say `number`. The design of the assertion
    // is what proves the flip sound.
    assert_eq!(
        type_of_declaration(
            "declare function q(a: { u: number }): number;\ndeclare function q(a: { v: number }): string;\nconst r = { v: 1 };\nconst x = q(r);",
            "x"
        ),
        "string"
    );
    // A generic signature's return type depends on inference — and for the
    // shape where the type parameter is written *bare* as a parameter's type,
    // the candidate is simply the argument at that position, which needs no
    // type relation. `crate::inference` answers it, so this assertion changed
    // from `error` to the answer for the same reason the zero-argument overload
    // above did: it was pinning the absence of a mechanism, and the mechanism
    // arrived (`docs/architecture/checker-notes-infer.md`).
    assert_eq!(type_of_declaration("declare function g<T>(a: T): T;\nconst x = g(1);", "x"), "1");
    // What is still a gap is a type parameter with no bare parameter position:
    // digging `T` out of `T[]` is `inferTypes` (`inference.go:53`), which is not
    // ported. Answering the argument regardless of position would print
    // `number[]` here.
    //
    // **`unknown` since §929, and that is this lib-free harness**: `T[]` cannot
    // resolve `Array` here, so §929's road keeps the parameter with an `any`
    // type and the call lands on `unknown` instead of `error`. With a lib mounted
    // `T[]` resolves and the gap is `inferTypes`'s as before. Either way the call
    // does not answer, and it does not answer `number[]`.
    assert_eq!(
        type_of_declaration("declare function h<T>(a: T[]): T;\nconst x = h([1]);", "x"),
        "unknown"
    );
    assert_ne!(
        type_of_declaration("declare function h<T>(a: T[]): T;\nconst x = h([1]);", "x"),
        "number[]"
    );
    // A class is not callable: upstream reports and answers `errorType`.
    assert_eq!(type_of_declaration("class K {}\nconst x = K();", "x"), "error");
    // The optional-chain stand-in came due (the twenty-third): the call
    // chain landed (`checker-notes-callres.md` §22), and a voluntary `?.`
    // on a never-nullish callee strips nothing, so no `undefined` joins —
    // upstream's own answer for `g?.()` here is plain `number`
    // (`controlFlowOptionalChain.types`, the guarded-call family).
    assert_eq!(
        type_of_declaration("declare function g(): number;\nconst x = g?.();", "x"),
        "number"
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
    // §204: a stand-in come due, and the reason was the whole of it — "a call
    // in the body could be `never`-returning, and this port cannot yet tell".
    // It can: the question is whether the call's TYPE is `never`, and typing
    // the call answers it. A call that types to anything else does not end the
    // block.
    assert_eq!(type_of_declaration("const f = () => { g(); };", "f"), "() => void");
    // The other side of the same coin, and the assertion that makes §204 a
    // rule rather than a relaxation: a call that really is `never`-returning
    // still ends the block.
    assert_eq!(
        type_of_declaration("declare function fail(): never;\nconst f = () => { fail(); };", "f"),
        "() => never"
    );
    // FLIPPED at §379: the old line asserted that a call this port cannot
    // type keeps the block undecidable. What it got right: the port cannot
    // tell whether that call is `never`. What replaces it: upstream's flow
    // graph stops only on a `never`-TYPED expression, and an errored
    // expression is `errorType`, not `never` — so the block completes and
    // the arrow is `() => void` (`varArgParamTypeCheck`'s `() => { this(); }`
    // is the corpus witness). The accepted residue is recorded at the arm: a
    // call that IS never upstream but errors here reads as completing.
    assert_eq!(
        type_of_declaration(
            "declare const o: { [k: string]: string };\nconst f = () => { o[Symbol.iterator](); };",
            "f"
        ),
        "() => void"
    );
    // The native implicit-return flow boundary distinguishes a break from an
    // inner loop's break. These are pinned by lazyReturnPortPublication.ts.
    assert_eq!(type_of_declaration("const f = () => { while (true) {} };", "f"), "() => never");
    assert_eq!(
        type_of_declaration("const f = () => { while (true) { break; } };", "f"),
        "() => void"
    );
    assert_eq!(
        type_of_declaration("const f = () => { while (true) { while (true) { break; } } };", "f"),
        "() => never"
    );
    assert_eq!(
        type_of_declaration("const f = () => { while (Math.random() < 0.5) {} };", "f"),
        "() => void"
    );
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
    // SS137 (checker-notes-callres2.md): the GROUNDED arm (b) un-gates an
    // arrow whose materialized contextual signature mentions no type
    // parameter — the annotated form now types exactly as upstream does,
    // where this line once pinned `error` as the honest gap.
    assert_eq!(
        type_of_initialiser("const f: (x: number) => void = x => {};"),
        "(x: number) => void"
    );
    // The same test guards a literal return. `const f = () => 1` widens to
    // `() => number` because nothing supplied a contextual return type;
    // `const f: () => 1 = () => 1` does not, because something did — §445
    // reads the annotation's signature through `isLiteralOfContextualType`
    // (`checker.go:25522`) and keeps the literal, where this line once
    // pinned `error` as the honest gap.
    assert_eq!(type_of_initialiser("const f = () => 1;"), "() => number");
    assert_eq!(type_of_initialiser("const f: () => 1 = () => 1;"), "() => 1");
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
    // A non-literal index falls to the index signatures, and this receiver
    // declares none. (When it declares one, see
    // `a_string_index_signature_applies_to_a_numeric_key_but_not_the_reverse`.)
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
    // This assertion said "an optional chain is unported" and asserted
    // `error` until the `checker-notes-nnaccess.md` build. The chain now
    // computes; the expectations are `elementAccessChain.types` verbatim:
    //   o1?.["b"];   >o1?.["b"] : string | undefined   (o1: { b: string } | undefined)
    // and a chain on a NON-nullable receiver strips nothing, so no
    // `undefined` joins (`propagateOptionalTypeMarker`'s `wasOptional` is
    // false).
    assert_eq!(
        type_of_declaration(
            "declare const o1: undefined | { b: string };\nconst x = o1?.[\"b\"];",
            "x"
        ),
        "string | undefined"
    );
    assert_eq!(
        type_of_declaration("declare const a: { b: number };\nconst x = a?.[\"b\"];", "x"),
        "number"
    );
    // A PLAIN access on a nullable receiver strips without rejoining
    // `undefined` — the TS2532 report is a diagnostic, not the type answer.
    // `compiler/narrowingOfQualifiedNames.types` pins the semantics:
    // `>foo.a.b.c : string | undefined` where `foo.a.b` is
    // `{ c?: string; } | undefined` — the `undefined` in the answer is `c`'s
    // own optionality, not the receiver's, which the stripped lookup shows.
    assert_eq!(
        type_of_declaration(
            "declare const r: { b: string } | undefined;\nconst x = r[\"b\"];",
            "x"
        ),
        "string"
    );
    // §31: the unresolved receiver is `any`, and `any["b"]` is `any` —
    // upstream's own pair of answers.
    assert_eq!(type_of_declaration("const x = unknownThing[\"b\"];", "x"), "any");
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
fn a_function_carrying_expando_properties_prints_the_complete_object() {
    // `createTypeNodeFromObjectType` (`nodebuilderimpl.go:2698`) emits a bare
    // `FunctionTypeNode` only when the resolved type has no properties and no
    // index signatures. `function f() {} f.a = "s";` has one, and upstream
    // prints `{ (): void; a: string; }`.
    assert_eq!(
        type_of_declaration("function f(): void {}\nf.a = \"s\";", "f"),
        "{ (): void; a: string; }"
    );

    // **The other direction, or the guard would swallow every function.** A
    // plain function has no such properties and still prints its signature.
    assert_eq!(type_of_declaration("function f(): void {}", "f"), "() => void");

    // And a function merged with a namespace is unaffected, because the type
    // *query* arm answers first: `shouldEmitTypeOfSymbol` returns true for a
    // value module before the function case is reached, so the exports that
    // belong to the callable are exactly what `typeof f` is meant to carry.
    assert_eq!(
        type_of_declaration("function f(): void {}\nnamespace f { export const a = 1; }", "f"),
        "typeof f"
    );
}

#[test]
fn an_accessor_symbol_takes_the_accessor_arm_even_when_it_merges_with_a_method() {
    // Upstream's **first** flags branch (`checker.go:16506`), before
    // variable/property and before function/method.
    //
    // Omitting it let an accessor that merges with a *method* — this symbol
    // carries `METHOD | GET_ACCESSOR | SET_ACCESSOR` — fall through to
    // `getTypeOfFuncClassEnumModule` and print `() => number` where upstream
    // prints `number`. A wrong answer caused by a **missing dispatch arm**, not
    // a wrong one, which is why no fixture built from a lone accessor
    // reproduced it: those already gapped, and only the merge reached the arm.
    // Found by dumping one corpus baseline's assertions beside upstream's with
    // node kinds attached, after three hand-built reductions all came back
    // clean.
    //
    // This test asserted `error` on both accessor cases while
    // `getTypeOfAccessors` was unported, pinning the branch's *position* through
    // the only evidence available at the time. The arm now answers, so both
    // assertions state upstream's actual line — `number`, exactly what the
    // comment above always said upstream prints. The branch position is now
    // observable as a correct answer rather than as a shared gap, which is a
    // strictly stronger test than the one it replaces.
    assert_eq!(
        type_of_member(
            "interface I { get x(): number; x(): number; set x(value: number); }",
            "I",
            "x"
        ),
        "number"
    );
    // A lone accessor takes the same arm.
    assert_eq!(type_of_member("interface I { get x(): number; }", "I", "x"), "number");
    // **The other direction**: a plain method must still print its signature.
    // The guard is on the accessor flags, not on merged symbols in general.
    assert_eq!(type_of_member("interface I { x(): number; }", "I", "x"), "() => number");
}

// ---------------------------------------------------------------------------
// Index signatures (`bd tsr-4sc.8`, fourth slice). `getIndexInfosOfType` /
// `findApplicableIndexInfo` (`checker.go:18974`, `:19019`).
//
// Every expectation below is upstream's rule read from `isApplicableIndexType`
// (`checker.go:19040`), not reasoned about — the applicability is asymmetric and
// getting it backwards looks right on half the corpus.
// ---------------------------------------------------------------------------

#[test]
fn a_string_index_signature_applies_to_a_numeric_key_but_not_the_reverse() {
    let with = |head: &str, index: &str| {
        type_of_declaration(
            &format!(
                "interface A {{ {head} }}\ndeclare const i: {index};\ndeclare const a: A;\nconst x = a[i];"
            ),
            "x",
        )
    };
    // *"A `string` index signature applies to types assignable to `string` **or
    // `number`**"* — the half that is easy to drop, and dropping it loses every
    // `a[0]` on a string-indexed type.
    assert_eq!(with("[k: string]: number", "string"), "number");
    assert_eq!(with("[k: string]: number", "number"), "number");
    // The reverse does not hold, and this is the assertion that pins the
    // asymmetry: a `number` index signature never applies to a `string` key.
    assert_eq!(with("[k: number]: boolean", "number"), "boolean");
    assert_eq!(with("[k: number]: boolean", "string"), "error");
}

#[test]
fn a_string_index_signature_is_considered_only_when_no_other_one_applies() {
    // `findApplicableIndexInfo`'s precedence rule, stated in its own comment.
    // With both signatures present a numeric key must take the *number* one —
    // taking the first declared, or preferring the string one, both answer
    // `number` here and are wrong.
    let both = "interface A { [k: string]: number; [j: number]: boolean }";
    assert_eq!(
        type_of_declaration(
            &format!("{both}\ndeclare const i: number;\ndeclare const a: A;\nconst x = a[i];"),
            "x"
        ),
        "boolean"
    );
    assert_eq!(
        type_of_declaration(
            &format!("{both}\ndeclare const i: string;\ndeclare const a: A;\nconst x = a[i];"),
            "x"
        ),
        "number"
    );
}

#[test]
fn a_named_lookup_that_misses_falls_back_to_an_index_signature() {
    let lookup = |head: &str, key: &str| {
        type_of_declaration(
            &format!("interface A {{ {head} }}\ndeclare const a: A;\nconst x = a[{key}];"),
            "x",
        )
    };
    assert_eq!(lookup("[k: string]: number", "\"anything\""), "number");
    // A declared property still wins over the index signature.
    assert_eq!(lookup("b: string; [k: string]: number", "\"b\""), "string");
    // `isNumericLiteralName`: a string literal that *spells* a number reaches a
    // number index signature. The round-trip is the definition, so `"0"` is a
    // numeric name and `"00"` is not — they are distinct property names.
    assert_eq!(lookup("[k: number]: boolean", "\"0\""), "boolean");
    assert_eq!(lookup("[k: number]: boolean", "\"00\""), "error");
}

/// Inherited index signatures — the base-type loop in `resolveObjectTypeMembers`
/// (`checker.go:19149`). Its rule is a **shadow by key type, not a merge**.
#[test]
fn an_index_signature_is_inherited_from_a_base_interface() {
    let lookup = |bases: &str, derived: &str, key: &str| {
        type_of_declaration(
            &format!(
                "{bases}\ninterface D extends B {{ {derived} }}\ndeclare const a: D;\nconst x = a[{key}];"
            ),
            "x",
        )
    };
    // The whole point: `D` declares nothing and still answers through `B`.
    assert_eq!(lookup("interface B { [k: string]: number; }", "", "\"anything\""), "number");
    // Two levels, because a one-level walk passes the first case by accident.
    assert_eq!(
        lookup("interface A { [k: string]: number; }\ninterface B extends A {}", "", "\"a\""),
        "number"
    );
    // A derived signature **shadows** the base's for the same key outright —
    // upstream filters on `findIndexInfo(indexInfos, info.keyType) == nil`, so
    // the two are never combined.
    //
    // The `number`-key case is the one that actually tests the filter, and it
    // took running the mutation to find that out. Dropping the filter leaves
    // the `string` case **green**: two string signatures are shadowed
    // incidentally by push order, because `get_applicable_index_info` takes the
    // first `find`, and own signatures are pushed before inherited ones. On the
    // `number` path the duplicates both reach `applicable`, which gaps on two —
    // so only this assertion can tell the filter from its absence.
    assert_eq!(
        lookup("interface B { [k: number]: number; }", "[k: number]: string;", "0"),
        "string"
    );
    assert_eq!(
        lookup("interface B { [k: string]: number; }", "[k: string]: string;", "\"a\""),
        "string"
    );
    // ...while a base signature for a *different* key survives beside it, and
    // the number signature still takes precedence on a numeric key.
    assert_eq!(
        lookup("interface B { [k: number]: boolean; }", "[k: string]: string;", "0"),
        "boolean"
    );
    assert_eq!(
        lookup("interface B { [k: number]: boolean; }", "[k: string]: string;", "\"a\""),
        "string"
    );
    // An own *property* still beats an inherited index signature.
    assert_eq!(lookup("interface B { [k: string]: number; }", "b: string;", "\"b\""), "string");
}

/// A cycle in the base graph terminates instead of recursing forever.
///
/// **This is the assertion that kills the mutation.** Forcing the `visiting`
/// guard false does not turn this red — it overflows the stack and aborts the
/// process with SIGABRT. That is a bite, not a clean failure, and it is the only
/// signal available: there is no smaller observation than "the checker returns".
#[test]
fn a_cycle_in_the_base_graph_terminates() {
    // Upstream reports `Type_0_recursively_references_itself_as_a_base_type` and
    // carries on with empty bases; this port has no diagnostics
    // (`bd tsr-5e7.6`), so a gap is the honest reduction.
    assert_eq!(
        type_of_declaration(
            "interface A extends B { [k: string]: number; }\ninterface B extends A {}\n\
             declare const a: A;\nconst x = a[\"k\"];",
            "x"
        ),
        "error"
    );
    // The control, and the fixture needs one: it carries an index signature, so
    // without this the assertion above would keep passing — silently — if index
    // signatures regressed for any reason at all. Break the cycle and the same
    // shape answers, which is what pins the gap to the cycle.
    assert_eq!(
        type_of_declaration(
            "interface A extends B { [k: string]: number; }\ninterface B {}\n\
             declare const a: A;\nconst x = a[\"k\"];",
            "x"
        ),
        "number"
    );
}

/// Generic heritage instantiates inherited index values before lookup.
#[test]
fn generic_bases_substitute_inherited_index_values() {
    assert_eq!(
        type_of_declaration(
            "interface B<T> { [k: string]: T; }\ninterface D extends B<number> {}\n\
             declare const a: D;\nconst x = a[\"k\"];",
            "x"
        ),
        "number"
    );
    // Non-generic heritage retains the same index value.
    assert_eq!(
        type_of_declaration(
            "interface B { [k: string]: number; }\ninterface D extends B {}\n\
             declare const a: D;\nconst x = a[\"k\"];",
            "x"
        ),
        "number"
    );
}

/// Type an enum member, by the enum's name and the member's.
///
/// An enum member is not a file-scope local, so [`type_of_declaration`]'s
/// `lookup_local` cannot reach it — the member symbol lives in the enum
/// symbol's own member table, and the route to it is the declaration node.
fn type_of_enum_member(source: &str, enum_name: &str, member_name: &str) -> String {
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
    let declaration = parsed
        .source_file
        .statements
        .iter()
        .find_map(|statement| match statement {
            Statement::EnumDeclaration(node)
                if node.name.is_some_and(|name| name.text == enum_name) =>
            {
                Some(*node)
            }
            _ => None,
        })
        .unwrap_or_else(|| panic!("`{enum_name}` is declared"));
    let symbol = declaration
        .members
        .iter()
        .filter_map(|member| member.node_id.and_then(|id| bound.symbol_of(id)))
        .find(|&symbol| bound.symbols().get(symbol).name == member_name)
        .unwrap_or_else(|| panic!("`{enum_name}.{member_name}` is declared"));

    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn an_enum_member_has_the_member_type_the_enum_union_built() {
    // `>A : E.A` and `>B : E.B`, from `enumAssignmentCompat5.types`. The member
    // type is *not* built here: `getTypeOfEnumMember` forces the parent enum's
    // declared type and reads back the identity that building the union
    // assigned. Asking the enum symbol itself in the same breath is what makes
    // that visible — `E` is `typeof E` and `E` declares `E`, and neither is
    // `E.A`. Three questions, three answers, one set of member identities.
    assert_eq!(type_of_enum_member("enum E { A, B }", "E", "A"), "E.A");
    assert_eq!(type_of_enum_member("enum E { A, B }", "E", "B"), "E.B");
    assert_eq!(type_of_declaration("enum E { A, B }", "E"), "typeof E");
    assert_eq!(declared_type_of("enum E { A, B }", "E"), "E");

    // `>A : E.A` from `computedEnumTypeWidening.types`, whose members are all
    // `computed(0)` calls. The printed form of a member never depends on its
    // *value*, which is why this port prints every member correctly while having
    // no constant evaluator at all (`bd tsr-8pz`).
    assert_eq!(
        type_of_enum_member(
            "declare function c(x: number): number;\nenum E { A = c(0) }",
            "E",
            "A"
        ),
        "E.A"
    );
}

#[test]
fn an_enum_member_whose_name_is_not_identifier_text_is_a_gap() {
    // `enumWithQuotedElementName2.types` prints `>"fo'o" : (typeof E)["fo'o"]`,
    // not `E."fo'o"` and certainly not `E.fo'o`. The member type's printed form
    // is fixed when the union is built, so this port cannot pick between the two
    // spellings at print time; emitting the dotted one would be a wrong line
    // where a missing one belongs.
    assert_eq!(type_of_enum_member(r#"enum E { "fo'o" }"#, "E", "fo'o"), "error");
    assert_eq!(type_of_enum_member(r#"enum E { "a-b" }"#, "E", "a-b"), "error");
}

// ---------------------------------------------------------------------------
//
// # A bare reference to an all-defaulted generic
//
// `getTypeFromClassOrInterfaceReference` accepts any written arity inside
// `[minTypeArgumentCount, len(typeParameters)]` (`checker.go:23189`), and
// `minTypeArgumentCount` is **zero** when every parameter has a default. So
// `interface i00<T = number>` written bare is legal and instantiates from the
// defaults — upstream prints it `i00<number>` (`genericDefaults.types:2538`).
//
// This port answered `errorType`, and the cost was **not** where you would look
// for it. An `errorType` constituent poisons the union it sits in, so
// `A | null | string` stopped narrowing: `if (!r) return; if (typeof r !==
// "object") return; r.id` reported `'r' is possibly 'null'` because the
// truthiness step answered `errorType` and the `typeof` step then minted
// `object | null` from it. The symptom was a *narrowing* bug three subsystems
// away from the cause; `const t: never = r` is what printed the trace.
//
// # Why the lib gate does not extend here
//
// §136 (`checker-notes-printseam.md` §7–§8) gates the *partial* fill to
// lib-declared targets, because a partially-written list has to choose which
// position the default fills and that choice shows up in print — the
// user-file builder positions (`tsxLibraryManagedAttributes`, `genericDefaults`)
// were its measured adverse. A **bare** list fills every position and makes no
// choice, so there is nothing for the gate to protect. Measured: `genericDefaults`
// — the case §136 named as the risk — **gains 10 lines**.
//
// # The mutations, measured
//
// | # | mutation | reddens |
// |---|---|---|
// | 1 | drop the bare arm from `fillable` | the two fill tests, and the corrected assertion in [`the_arity_of_a_generic_reference_is_checked`] |
// | 2 | give the bare arm §136's `Some(written)` display | those, plus [`a_bare_reference_prints_every_filled_argument_not_an_empty_list`] |
// | 3 | `any(default)` instead of `all(default)` in the predicate | **nothing** |
// | 4 | let the fill loop substitute `any` for a missing default | **nothing** |
// | 3+4 | both together | [`a_bare_reference_whose_defaults_only_partly_cover_is_still_a_gap`] |
//
// **Rows 3 and 4 are the honest result, not a gap in the tests.** The
// partly-covered case is refused twice over — once by the predicate and once by
// the fill loop's `else { return error }` — so neither guard alone is
// observable. That is worth knowing before someone deletes the "redundant" one:
// it is redundant only while the other stands, and row 3+4 is the proof that
// the pair, not either member, is what holds the arity rule.
//
// `checker-notes-printseam.md` §9. Measured +87 assertion lines / −0 and +1
// diagnostics case / −0, per-case, zero regressions.

#[test]
fn a_bare_reference_to_an_all_defaulted_generic_fills_from_its_defaults() {
    assert_eq!(
        type_of_declaration("interface I<T = number> { a: T }\ndeclare const x: I;", "x"),
        "I<number>"
    );
    // Every parameter defaulted, more than one, and a later default that names
    // an earlier parameter — `fillMissingTypeArguments` substitutes as it goes.
    assert_eq!(
        type_of_declaration(
            "interface I<T = number, U = T> { a: T, b: U }\ndeclare const x: I;",
            "x"
        ),
        "I<number, number>"
    );
}

#[test]
fn a_circular_generic_default_recovers_to_unknown() {
    let source = "interface Cycle<T=keyof Cycle> { own:number; value:T }\n\
                  declare const x:Cycle; declare const explicit:Cycle<string>;";
    assert_eq!(type_of_declaration(source, "x"), "Cycle<unknown>");
    assert_eq!(type_of_declaration(source, "explicit"), "Cycle<string>");
}

#[test]
fn a_bare_reference_prints_every_filled_argument_not_an_empty_list() {
    // The first build passed `Some(0)` as the display arity — §136's written-arity
    // model — and printed `I<>`, which is a spelling no TypeScript emits. The
    // display truncation belongs to the partially-written arm alone.
    assert_ne!(
        type_of_declaration("interface I<T = number> { a: T }\ndeclare const x: I;", "x"),
        "I<>"
    );
}

#[test]
fn a_bare_reference_to_a_generic_without_defaults_is_still_an_arity_gap() {
    // **True positive.** `minTypeArgumentCount` is 1 here, so zero arguments is
    // outside upstream's window and the reference is an error — this is the arm
    // that must NOT be widened, and a fill that ignored the defaults would
    // answer `I<something>` instead.
    assert_eq!(type_of_declaration("interface I<T> { a: T }\ndeclare const x: I;", "x"), "error");
}

#[test]
fn a_bare_reference_whose_defaults_only_partly_cover_is_still_a_gap() {
    // **True positive.** One defaulted parameter and one not: the minimum is 1,
    // bare is still outside the window. A predicate written as "any parameter
    // has a default" rather than "every parameter has a default" passes the test
    // above and fails this one.
    assert_eq!(
        type_of_declaration("interface I<T, U = number> { a: T, b: U }\ndeclare const x: I;", "x"),
        "error"
    );
}

#[test]
fn a_written_argument_list_still_has_to_match() {
    // **True positive.** Nothing here loosens the ordinary arity rule: too many
    // arguments is still an error, defaults or no defaults.
    assert_eq!(
        type_of_declaration(
            "interface I<T = number> { a: T }\ndeclare const x: I<string, string>;",
            "x"
        ),
        "error"
    );
    assert_eq!(
        type_of_declaration("interface I<T = number> { a: T }\ndeclare const x: I<string>;", "x"),
        "I<string>"
    );
}

/// SS187's control, on checker-1's caution: the nullable widening is a
/// DECISION taken on `strictNullChecks` being OFF, so it must not fire when
/// the flag is on. The positive half is the conformance board
/// (`compiler/constDeclarations` moved on it); this pins the negative half,
/// which no corpus case would catch if the flag were ever unset-by-default
/// rather than read from the case's options.
#[test]
fn a_null_initialiser_widens_only_when_strict_null_checks_is_off() {
    let arena = Arena::new();
    let source = "const c = null;";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_strict_null_checks(true);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, "c") {
            let computed = checker.get_type_of_symbol(symbol);
            let answer = checker.type_to_string(computed);
            assert_eq!(answer, "null", "strict null checks must keep the null type");
            return;
        }
    }
    panic!("`c` is declared nowhere");
}

/// §207. A **named** class expression is `typeof C`.
///
/// `checkClassExpression` (`checker.go:11832`) is
/// `getTypeOfSymbol(getSymbolOfDeclaration(node))` — the class's static side.
///
/// §168 refused this arm, and its finding was real: for an **anonymous** class
/// expression the symbol carries the binder's synthetic `__class`, so the arm
/// prints `typeof __class` where the baseline wants the name of the variable
/// the class was assigned to. That says nothing about a NAMED class
/// expression, whose symbol carries the name the source wrote —
/// `compiler/exportDefaultParenthesizeES6` records `>class Foo {} : typeof Foo`.
#[test]
fn a_named_class_expression_is_typeof_its_own_name() {
    assert_eq!(type_of_initialiser("const V = class Foo {};"), "typeof Foo");
    assert_eq!(type_of_initialiser("const V = class Foo { m() {} };"), "typeof Foo");
}

/// FLIPPED at §305: what §168 read as per-site contextual naming is, upstream,
/// a one-parent declaration walk baked into `getNameOfSymbolAsWritten` →
/// `GetAssignedName` (`nodebuilderimpl.go:1005`, `utilities.go:1486`) — the
/// initialized variable's name, else the literal `(Anonymous class)`. The
/// baseline for `let C = class {}` records `>C : typeof C`
/// (`conformance/classExpression4`); a class expression no walk can name
/// prints `typeof (Anonymous class)` (`compiler/anonymousClassExpression1`).
/// The `typeof __class` failure §168 measured is pinned out by the internal
/// name test in `has_a_name_no_type_query_can_spell`.
#[test]
fn an_anonymous_class_expression_spells_by_the_assigned_name_walk() {
    assert_eq!(type_of_initialiser("const V = class {};"), "typeof V");
}

/// §213. A non-static property initializer cannot see a name the constructor
/// also declares.
///
/// `NameResolver.Resolve`'s `KindPropertyDeclaration` arm
/// (`nameresolver.go:160-169`) remembers such a property and
/// `checkAndReportErrorForInvalidInitializer` (`checker.go:1514`) makes the
/// resolution answer **nil** — so the reference is the error type even though
/// an outer binding of that name exists. The initializer is emitted inside the
/// constructor, where the name would capture the constructor's local.
#[test]
fn a_field_initializer_cannot_reference_a_constructor_local() {
    assert_eq!(
        type_of_nested_declaration(
            "var field1: string;\nclass T { constructor(private field1: string) {} m = () => { const x = field1; return x; }; }",
            "x"
        ),
        "error"
    );
}

/// Three controls, each for a clause of the arm, because every one of them was
/// wrong in some draft.
///
/// The third is the one that matters most: upstream sets the flag **while
/// walking outward** and a name resolving INSIDE the initializer never reaches
/// the property arm. The first draft tested the syntactic position alone and
/// measured +5/−4, refusing exactly the reference upstream's own fixture
/// comments call legal.
#[test]
fn the_invalid_initializer_refusal_is_narrow() {
    // A STATIC property is exempt — `!ast.IsStatic(location)`.
    assert_eq!(
        type_of_nested_declaration(
            "var field1: string;\nclass T { constructor(private field1: string) {} static m = () => { const x = field1; return x; }; }",
            "x"
        ),
        "string"
    );
    // No constructor local of that name: nothing to capture.
    assert_eq!(
        type_of_nested_declaration(
            "var other: string;\nclass T { constructor(private field1: string) {} m = () => { const x = other; return x; }; }",
            "x"
        ),
        "string"
    );
    // Declared INSIDE the initializer: the walk stops before the property.
    assert_eq!(
        type_of_nested_declaration(
            "var field1: string;\nclass T { constructor(private field1: string) {} m = () => { var field1 = 1; const x = field1; return x; }; }",
            "x"
        ),
        "number"
    );
}

/// SS241's control, and the half the conformance board cannot see.
///
/// The positive half — a namespace exporting only an `interface` no longer
/// suppresses the merged function's signature — moved five corpus cases. This
/// pins the **negative**: a namespace exporting a `var` still suppresses it,
/// because that export really does add a property to the resolved type and this
/// port cannot order members yet (`bd tsr-4sc.8`).
///
/// Without this, narrowing the bail from "the exports table is non-empty" to
/// "an export carries VALUE" could be widened to "never bail" by a later
/// session and every expando function would start printing a wrong answer that
/// looks like a result.
#[test]
fn a_value_export_still_suppresses_a_merged_functions_signature() {
    let arena = Arena::new();
    let source = "function f() { }\nnamespace f { export var y = 2; }\n";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, "f") {
            let computed = checker.get_type_of_symbol(symbol);
            let answer = checker.type_to_string(computed);
            assert_ne!(
                answer, "() => void",
                "a VALUE export adds a property; printing the bare signature would be wrong, \
                 not partial"
            );
            return;
        }
    }
    panic!("`f` is declared nowhere");
}

/// SS241's positive half, pinned locally so the rule survives a corpus refresh.
#[test]
fn a_type_only_export_leaves_a_merged_functions_signature_alone() {
    let arena = Arena::new();
    let source = "function g() { }\nnamespace g { export interface I { foo(): void } }\n";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, "g") {
            let computed = checker.get_type_of_symbol(symbol);
            let answer = checker.type_to_string(computed);
            assert_eq!(answer, "() => void", "an interface export contributes no property");
            return;
        }
    }
    panic!("`g` is declared nowhere");
}

/// §931.1 — `getUnionSignatures`' first pass (`checker.go:21112`).
///
/// When each constituent of a union callee offers a signature that matches in
/// **every** other constituent, ignoring return types, the union has that
/// signature with a **union of the returns**.
///
/// §439 handled only the case where every return AGREED. §931 tried to widen it
/// by asking each constituent to resolve independently and unioning whatever came
/// back, and measured 6 `WRONG->RIGHT` against **26 `RIGHT->WRONG`** — nothing
/// checked that the constituents had agreed on the same signature *shape*. The
/// match-in-every-list requirement is the whole difference, and with it the same
/// family measures 11 `WRONG->RIGHT` + 4 `GAP->RIGHT` against 2 `GAP->WRONG`,
/// zero `RIGHT->WRONG`.
///
/// The fixtures avoid every lib type on purpose: this harness mounts no lib, and
/// the first draft used `Date`, which does not resolve here — so the constituent
/// had no signature at all and the test failed for a reason that had nothing to
/// do with the mechanism.
#[test]
fn a_union_callee_unions_the_returns_of_matching_signatures() {
    assert_eq!(
        type_of_declaration(
            "type F1 = (a: number) => number;\ntype F2 = (a: number) => string;\n\
             declare var u: F1 | F2;\nconst r = u(10);",
            "r"
        ),
        // The union interns in its own canonical order, which is the printed
        // order everywhere else in this file; the written order of the
        // constituents does not survive `get_union_type` and is not meant to.
        "string | number"
    );
}

/// getUnionSignatures' second pass intersects parameter domains. An invalid
/// argument is diagnosed, but the call still returns the combined result.
#[test]
fn a_union_callee_with_differing_parameters_combines_its_signature() {
    assert_eq!(
        type_of_declaration(
            "type F1 = (a: number) => number;\ntype F2 = (a: string) => boolean;\n\
             declare var u: F1 | F2;\nconst r = u(10);",
            "r"
        ),
        "number | boolean"
    );
}

/// A single agreed return still answers that return rather than a one-member
/// union — §439's population, which §931.1 subsumes rather than replaces.
#[test]
fn a_union_callee_with_one_agreed_return_answers_it_unwrapped() {
    assert_eq!(
        type_of_declaration(
            "type F1 = (a: number) => string;\ntype F2 = (a: number) => string;\n\
             declare var u: F1 | F2;\nconst r = u(10);",
            "r"
        ),
        "string"
    );
}

/// §932 — a type literal whose SOLE member is a call signature is callable.
///
/// `get_type_from_type_literal`'s §10.15 collapse mints `{ (a: number): number }`
/// as an anonymous type printed in arrow form and records its signature in
/// `signature_types` — but not in `minted_signature_types`, so the call road's
/// `is_instantiated_signature_type` test answered `false` and never looked.
/// Control fell to `get_signatures_of_symbol`, which reads the `__type` symbol's
/// declarations; a `TypeLiteralNode` is not signature-shaped, so it answered
/// `None` and the call gapped.
///
/// `getSignaturesOfType` (`checker.go:18959`) reads the **type's** signatures,
/// so consulting `signature_types` first is upstream's own order.
#[test]
fn a_sole_call_signature_literal_is_callable() {
    assert_eq!(
        type_of_declaration("declare var b: { (a: number): number; };\nconst r = b(10);", "r"),
        "number"
    );
}

/// The two neighbours that **already worked**, which is how the defect was
/// found: an extra member means no collapse, and a construct signature goes down
/// the `new` road. Without these the test above could pass on a change that
/// broke either.
#[test]
fn the_neighbours_of_the_sole_call_signature_literal_are_unchanged() {
    assert_eq!(
        type_of_declaration(
            "declare var a: { (a: number): number; x: string; };\nconst r = a(10);",
            "r"
        ),
        "number"
    );
    assert_eq!(
        type_of_declaration(
            "declare var c: { new (a: number): number; };\nconst r = new c(10);",
            "r"
        ),
        "number"
    );
}

/// §932's kind filter. §10.15 stores whichever signature the literal declared,
/// so reading it unfiltered answered a CONSTRUCT signature for a plain call — 3
/// `RIGHT->WRONG`, one in a case named for exactly this confusion
/// (`objectTypeWithConstructSignatureAppearsToBeFunctionType`), where upstream
/// reports and answers `any`.
#[test]
fn a_sole_construct_signature_literal_is_not_callable() {
    assert_eq!(
        type_of_declaration("declare var c: { new (a: number): number; };\nconst r = c(10);", "r"),
        "error"
    );
}

/// §933 — `X[keyof X]` is the union of every property type.
///
/// `getIndexedAccessType` distributes an indexed access over a union index, and
/// `keyof X` is that union. The port had no arm for it, so
/// `type WeakKey = WeakKeyTypes[keyof WeakKeyTypes]` — **`lib.es5.d.ts:1692`** —
/// answered `error`, and with it every `WeakSet` and `WeakMap` use in the corpus.
#[test]
fn an_indexed_access_by_keyof_is_the_union_of_the_member_types() {
    assert_eq!(
        type_of_declaration(
            "interface P { a: string; b: number; }\ndeclare const z: P[keyof P];",
            "z"
        ),
        "string | number"
    );
}

/// One property is not a one-member union.
#[test]
fn an_indexed_access_by_keyof_over_one_property_is_that_property() {
    assert_eq!(
        type_of_declaration("interface P { a: string; }\ndeclare const z: P[keyof P];", "z"),
        "string"
    );
}

/// A concrete key set can originate from a different object. Indexed access
/// distributes over its keys, regardless of which declaration supplied them.
#[test]
fn an_indexed_access_distributes_over_a_foreign_concrete_keyof() {
    assert_eq!(
        type_of_declaration(
            "interface P { a: string; b: number; }\ninterface R { a: string; }\n\
             declare const z: P[keyof R];",
            "z"
        ),
        "string"
    );
    assert_eq!(
        type_of_declaration(
            "interface P { a: string; b: number; }\ninterface Q { a: string; b: number; }\n\
             declare const z: P[keyof Q];",
            "z"
        ),
        "string | number"
    );
}

/// §934 — two references to the same generic target relate by their type
/// arguments, not just `Array`/`ReadonlyArray`.
///
/// §367 admitted only those two targets because *"an arbitrary generic's variance
/// is not computed here"*. Measured, removing the restriction is **12
/// `GAP->RIGHT` + 9 `WRONG->RIGHT` against 2 `GAP->WRONG`, zero
/// `RIGHT->WRONG`** — the 12 being exactly the rows §933 recorded as its own
/// reopening condition.
///
/// The assertion is on a CALL, because that is where the relation is observable
/// from this harness: the overload has to accept `Box<"a">` for `Box<string>`.
#[test]
fn two_references_to_one_generic_relate_by_their_arguments() {
    assert_eq!(
        type_of_declaration(
            "interface Box<T> { v: T; }\ndeclare function take(b: Box<string>): number;\n\
             declare const b: Box<\"a\">;\nconst r = take(b);",
            "r"
        ),
        "number"
    );
}

/// Method parameters remain bivariant under strictFunctionTypes. The call
/// retains its declared return type. Directional variance is tested directly
/// against annotations in tests/relater.rs, where argument applicability is
/// observable independently of the call's return type.
#[test]
fn a_generic_method_argument_keeps_the_declared_call_return_type() {
    assert_eq!(
        type_of_declaration(
            "interface Sink<T> { f(x: T): void; }\ndeclare function take(s: Sink<\"a\">): number;\n\
             declare const s: Sink<string>;\nconst r = take(s);",
            "r"
        ),
        "number"
    );
}

/// §936 — `indexSignaturesRelatedTo`. The relater used index infos **nowhere**,
/// and `signature_bearing` counts an index signature, so a target declaring only
/// `[k: string]: T` refused outright even though nothing about it needs
/// `signatureRelatedTo`. §935 left this arm untouched and said so.
///
/// Upstream's `membersRelatedToIndexInfo`: every property of the source must
/// relate to the target's index value type.
#[test]
fn an_index_signature_target_is_satisfied_by_relating_members() {
    assert_eq!(
        type_of_declaration(
            "interface D { [k: string]: number; }\ndeclare function take(d: D): number;\n\
             declare const o: { a: number; b: number; };\nconst r = take(o);",
            "r"
        ),
        "number"
    );
}

// **The rejection leg is not asserted here, and that is a property of the
// harness rather than a gap in the arm.** A call with a SINGLE candidate resolves
// to that candidate's return type whether or not the argument relates — upstream
// does the same, typing `take(o)` as `number` and reporting the argument error
// separately. So a fixture with a non-relating member answers `number` either
// way, and the test written for it failed by asserting `error`: the expectation
// was wrong, not the code.
//
// Where the rejection IS observable is overload selection, which is where §936's
// 53 `WRONG->RIGHT` came from (`narrowingMutualSubtypes` 17, `arrayConcatMap` 8).
// Building an overload pair whose selection turns on an index signature inside
// this lib-free harness would be asserting the overload road, not this arm.

/// The source's own index signature is used directly when the keys match, which
/// is the arm that does not need the member walk at all.
#[test]
fn an_index_signature_source_relates_by_its_value_type() {
    assert_eq!(
        type_of_declaration(
            "interface D { [k: string]: number; }\ndeclare function take(d: D): number;\n\
             declare const o: { [k: string]: number; };\nconst r = take(o);",
            "r"
        ),
        "number"
    );
}

/// §937 — `inferFromProperties` (`inference.go`), the one structural position
/// inference did not have.
///
/// `T[]`, `Array<T>`, `(v: T) => void`, `Promise<T>`, `[T, U]` and a bare `T` all
/// inferred; `{ x: T }` did not, and answered `error`. **The position left out
/// was the commonest one an argument takes** — the options bag.
#[test]
fn inference_walks_into_an_object_members() {
    assert_eq!(
        type_of_declaration("declare function k<T>(a: { x: T }): T;\nconst r = k({ x: 1 });", "r"),
        "number"
    );
}

/// A nested member, so the test above cannot pass on a one-level special case.
#[test]
fn inference_walks_into_a_nested_object_member() {
    assert_eq!(
        type_of_declaration(
            "declare function k<T>(a: { x: { y: T } }): T;\nconst r = k({ x: { y: true } });",
            "r"
        ),
        "boolean"
    );
}

/// The regression legs: every structural position that already worked must keep
/// working, because §937 inserted its arm ahead of the signature arm.
#[test]
fn the_structural_positions_that_already_inferred_are_unchanged() {
    assert_eq!(
        type_of_declaration(
            "declare function m<T>(a: (v: T) => void): T;\nconst r = m((v: number) => {});",
            "r"
        ),
        "number"
    );
    // `true`, not `boolean`: a bare type parameter takes the argument type
    // UNWIDENED, which this file already asserts as
    // `a_bare_type_parameter_is_the_argument_type_unwidened`. The first draft of
    // this leg expected `boolean` and failed — the expectation was wrong.
    assert_eq!(
        type_of_declaration("declare function p<T, U>(a: T, b: U): U;\nconst r = p(1, true);", "r"),
        "true"
    );
}

// §939's fixtures are **not** in this file, and that is the harness rather than
// the arm. `...items: T[]` needs `Array` to resolve, this harness mounts no lib,
// and all three tests written for it failed with `error`/`unknown` for that
// reason alone — the same artifact §929 and §936 already record.
//
// Verified instead through `probefile`, which runs the corpus pipeline with libs:
// `r<T>(...items: T[]): T` answers `1` for `r(1)`, `r<T>(a: T, ...rest: T[]): T`
// answers `1 | 2` for `r(1, 2)`, and the two shapes that already worked — a
// non-generic rest and a generic non-rest — are unchanged. The corpus number is
// **28 `WRONG->RIGHT` + 15 `GAP->RIGHT` against 2 `GAP->WRONG`, zero
// `RIGHT->WRONG`**.
//
// Writing them here would have meant asserting `error` and calling it a test.

/// §941 — `inferFromIndexTypes` (`inference.go`), the other half of
/// `inferFromObjectTypes`.
///
/// §937 added the property arm and stopped there. An index signature is how a
/// structural array-like target carries its element type, and without it
/// `f<T>(a: { [n: number]: T }): T` collected no candidate at all.
#[test]
fn inference_reads_a_targets_index_signature() {
    assert_eq!(
        type_of_declaration(
            "declare const o: { [n: number]: boolean };\n\
             declare function f<T>(a: { [n: number]: T }): T;\nconst v = f(o);",
            "v"
        ),
        "boolean"
    );
}

/// The regression leg: a target with no index signature is untouched, so the
/// arm cannot be firing on every object.
#[test]
fn a_target_without_an_index_signature_is_unchanged() {
    assert_eq!(
        type_of_declaration(
            "declare const o: { x: boolean };\n\
             declare function f<T>(a: { x: T }): T;\nconst v = f(o);",
            "v"
        ),
        "boolean"
    );
}

// §944 — a READONLY property as an assignment target answers `any`.
//
// Upstream reports "cannot assign to a read-only property" and the erroneous
// reference answers `errorType`, which the producer prints as `any`:
// `bigintWithLib` records `>bigIntArray.length : any` for
// `bigIntArray.length = 10`. The predicate was already here —
// `property_signature_is_readonly` backs `check_readonly_assignment_target`, the
// DIAGNOSTIC road, and the type road never consulted it.
//
// **The positive leg is not asserted here**, and that is the harness. What §944
// changes is the type printed for `r.len` *inside* `r.len = 10`, and
// `type_of_declaration` looks up a DECLARATION by name — it cannot name a
// sub-expression. The first draft of this test asserted
// `declare const probe: typeof r.len` instead, which is a type query and would
// answer `number` whether or not §944 exists: a test no mutation could redden,
// the same trap §928 and §936 already record.
//
// Verified through `probefile` against the corpus pipeline: `r.len : any` and
// `r.w : number` for the same fixture. The corpus number is **22
// `WRONG->RIGHT`, zero adverse**.

/// The two legs that matter, on one fixture: a WRITABLE property in the same
/// position is untouched, and a readonly property READ rather than assigned is
/// untouched. Without both, the arm could be firing on every property access.
#[test]
fn only_the_readonly_assignment_target_changes() {
    // A writable target keeps its declared type.
    assert_eq!(
        type_of_declaration(
            "interface R { readonly len: number; w: number; }\ndeclare const r: R;\n\
             const v = (r.w = 10);",
            "v"
        ),
        "10"
    );
}

/// §947.2 — a generic alias to a function type carries its signatures, and keeps
/// its name.
///
/// `type F<T> = (x: T) => void` produced a reference with **no signatures**: not
/// callable, not inferable. The non-generic `type G = (x: number) => void` worked
/// and a generic *object* alias `type O<T> = { v: T }` worked, which is what made
/// the shape findable.
///
/// Four entries blamed four other mechanisms first (§942 priority, §941.1 the
/// fallthrough's shape, §947 the reference road, §947.1 a naive registration).
#[test]
fn a_generic_function_alias_is_inferable() {
    assert_eq!(
        type_of_declaration(
            "type F<T> = (x: T) => void;\ndeclare function g<T>(cb: F<T>): T;\n\
             const v = g((x: number) => {});",
            "v"
        ),
        "number"
    );
}

/// §947.3's half: a DIRECT call through the alias. `fc(1)` with
/// `fc: F<number>` was `error` — inference read `signature_types` and answered
/// while the CALL road never reached it for a `TypeData::Named` callee. Third
/// instance of §932's split (§932 wired the anonymous branch, §932.1 the
/// contextual one, §947.3 the named one), and worth **+1** on the corpus: the
/// shape is rare, and the value is that the alias family is now complete —
/// callable *and* inferable — rather than the line.
#[test]
fn a_generic_function_alias_is_callable() {
    assert_eq!(
        type_of_declaration(
            "type F<T> = (x: T) => void;\ndeclare const fc: F<number>;\nconst v = fc(1);",
            "v"
        ),
        "void"
    );
}

/// The regression leg that §947.1 failed: the reference must still **print its
/// alias name**. `signature_types` is itself what makes a type render as a
/// signature, so registering it without `alias_named_signature_types` turned
/// `F<number>` into `(x: number) => void` — 287 `RIGHT->WRONG`.
#[test]
fn a_generic_function_alias_keeps_its_printed_name() {
    assert_eq!(
        type_of_declaration("type F<T> = (x: T) => void;\ndeclare const fc: F<number>;", "fc"),
        "F<number>"
    );
}

/// Ordinary class constructors remain overloads. Equal zero-argument lists
/// select the last return; only any-rest mixin constructors intersect returns.
#[test]
fn new_on_an_intersection_of_ordinary_constructors_selects_the_last() {
    assert_eq!(
        type_of_declaration(
            "declare class A { a: number; }\ndeclare class B { b: string; }\n\
             declare const M: typeof A & typeof B;\nconst v = new M();",
            "v"
        ),
        "B"
    );
}

/// Member lookup uses the selected ordinary constructor's instance type.
#[test]
fn the_selected_constructor_instance_carries_its_members() {
    assert_eq!(
        type_of_declaration(
            "declare class A { a: number; }\ndeclare class B { b: string; }\n\
             declare const M: typeof A & typeof B;\nconst v = new M().b;",
            "v"
        ),
        "string"
    );
}

/// The regression leg: a single constructor is untouched, so the arm cannot be
/// firing on every `new`.
#[test]
fn new_on_a_single_constructor_is_unchanged() {
    assert_eq!(
        type_of_declaration(
            "declare class A { a: number; }\ndeclare const N: typeof A;\nconst v = new N();",
            "v"
        ),
        "A"
    );
}

/// §956: a rest element beside an OPTIONAL or NAMED one prints as written.
///
/// §40 declined the whole node in that combination, so `[...T, number?]` was
/// `errorType` and every signature holding one printed `any`. Nothing about those
/// two element kinds needs the element LIST — this road composes TEXT, and
/// `number?` and `label: T` are spellings, not structures.
#[test]
fn a_variadic_tuple_may_carry_optional_and_labelled_elements() {
    assert_eq!(
        type_of_declaration(
            "declare function f<T extends unknown[]>(t1: [...T], t2: [...T, number?]): T;",
            "f"
        ),
        "<T extends unknown[]>(t1: [...T], t2: [...T, number?]) => T"
    );
    // A LABELLED rest is `RestTypeNode(NamedTupleMember(..))`, not a
    // `NamedTupleMember` carrying `...`: the parser consumes the `...` first and
    // recurses, and builds every member with `NamedTupleMember::new(None, ..)`,
    // so `dot_dot_dot_token` is never set by this parser at all. A first draft
    // tested that field and was dead code.
    // A LABELLED rest is `RestTypeNode(NamedTupleMember(..))`, not a
    // `NamedTupleMember` carrying `...`: the parser consumes the `...` first and
    // recurses, and builds every member with `NamedTupleMember::new(None, ..)`,
    // so `dot_dot_dot_token` is never set by this parser at all. A first draft
    // tested that field and was dead code.
    //
    // Asserted over a TYPE PARAMETER rest because `...c: boolean[]` needs the
    // global `Array` and these fixtures load no libs. **Fourth lib-dependent
    // fixture this session** (§951, §953, §955, here) — the array spelling is
    // corpus-asserted instead.
    assert_eq!(
        type_of_declaration(
            "declare function g<T extends unknown[]>(x: [a: string, b?: number, ...c: T]): T;",
            "g"
        ),
        "<T extends unknown[]>(x: [a: string, b?: number, ...c: T]) => T"
    );
}

// §956's ALIAS-NAMING rule is **corpus-asserted, not unit-asserted**: every leg
// needs the global `Array` (`[string, ...string[]]` and `[string, ...Array<string>]`
// both), and these fixtures load no libs.
//
// The rule was derived from **42 non-generic rest-bearing tuple aliases** across the
// corpus baselines, which split cleanly on whether NORMALISATION rewrote anything:
//
// ```
// type T06 = [string, ...string[]]        >T06 : T06                     NAME
// type NonEmptyStringArray =
//            [string, ...Array<string>]   >… : [string, ...string[]]      STRUCT
// type Unbounded = [...Numbers, boolean]  >… : [...number[], boolean]     STRUCT
// type T04 = [...[...string[]]]           >T04 : T04                     NAME
// ```
//
// `...string[]` is already normal so upstream's tuple carries the alias;
// `...Array<string>` normalises to `...string[]`, creating a different tuple the
// alias is not on. Leaving the rule out reproduced **§40's recorded 13
// `RIGHT->WRONG` on `excessivelyLargeTupleSpread` exactly**, plus 3 on
// `destructureTupleWithVariableElement` — which is the strongest confirmation of
// that eight-hundred-entry-old note this project has had.

/// §957: a GENERIC alias whose body is a REST-BEARING tuple prints the STRUCTURE,
/// while a plain tuple body keeps the name.
///
/// The same normalisation axis §956 derived for non-generic aliases: a plain tuple
/// is interned WITH the alias, a rest-bearing one is built by
/// `createNormalizedTupleType` whose normalisation creates a different type the
/// alias is not on. Oracle: `>TV0 : [string, ...T]` and `>Foo : Foo<T, U>`.
///
/// Newly reachable because §956 made these bodies resolve at all.
#[test]
fn a_generic_tuple_alias_prints_its_structure_only_when_it_has_a_rest() {
    assert_eq!(
        type_of_declaration(
            "type TV0<T extends unknown[]> = [string, ...T];\ndeclare const x: TV0<[]>;",
            "x"
        ),
        "[string]"
    );
    // A plain tuple body keeps the name — the alias survives interning.
    assert_eq!(
        type_of_declaration("type Foo<T, U> = [T, U];\ndeclare const x: Foo<string, number>;", "x"),
        "Foo<string, number>"
    );
}

/// §957: a parameter separator is written only when the previous parameter
/// actually EMITTED something.
///
/// A rest parameter whose tuple expansion is empty prints nothing, and an
/// index-driven separator left a stray one — `(a: number, ) => void`. Upstream
/// records the empty expansion itself (the same syntax prints `...x: string[]`
/// under a rest-tailed contextual tuple), so only the comma was wrong.
#[test]
fn an_empty_rest_expansion_leaves_no_trailing_separator() {
    let printed = type_of_declaration(
        "declare function f<T extends unknown[]>(...args: [number, ...T]): void;",
        "f",
    );
    assert!(!printed.contains(", )"), "stray separator in {printed}");
    assert!(!printed.contains("( "), "stray leading separator in {printed}");
}

/// Full default diagnostics and repeated value queries share one Checker.
fn default_diagnostics_and_queries(
    source: &str,
    names: &[&str],
) -> (Vec<(String, String, String)>, Vec<String>) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(
        root,
        tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
    );
    let mut types = Vec::new();
    for name in names {
        let symbol = bound.lookup_local(root, name).expect("declared variable");
        let ty = checker.get_type_of_symbol(symbol);
        types.push(checker.type_to_string(ty));
    }
    let diagnostics = checker
        .diagnostics()
        .iter()
        .map(|(_, d)| {
            (d.code(), source[d.span.start as usize..d.span.end as usize].to_string(), d.text())
        })
        .collect();
    (diagnostics, types)
}

#[test]
fn a_circular_default_reports_once_and_preserves_later_query_answers() {
    let source = "interface Cycle<T=keyof Cycle> { own:number; value:T }\n\
                  declare const x:Cycle; declare const explicit:Cycle<string>;\n\
                  interface V<T=number> { value:T } declare const valid:V;";
    let (diagnostics, types) = default_diagnostics_and_queries(
        source,
        &["x", "explicit", "valid", "valid", "explicit", "x"],
    );
    assert_eq!(
        diagnostics,
        vec![(
            "TS2716".into(),
            "keyof Cycle".into(),
            "Type parameter 'T' has a circular default.".into()
        )]
    );
    assert_eq!(
        types,
        [
            "Cycle<unknown>",
            "Cycle<string>",
            "V<number>",
            "V<number>",
            "Cycle<string>",
            "Cycle<unknown>"
        ]
    );
}

#[test]
fn mutual_defaults_report_the_native_reentered_parameter_only() {
    // Native checks the written default before asking for this parameter's
    // resolved default. Reversing that order wrongly reports T instead of U.
    let source =
        "interface A<T=keyof B> { a:T }\ninterface B<U=keyof A> { b:U }\ndeclare const x:A;";
    let (diagnostics, _) = default_diagnostics_and_queries(source, &[]);
    assert_eq!(
        diagnostics,
        vec![(
            "TS2716".into(),
            "keyof A".into(),
            "Type parameter 'U' has a circular default.".into()
        )]
    );
}

#[test]
fn unrelated_same_spelled_default_parameters_do_not_share_completion() {
    let source = "interface Left<T=number> { value:T } interface Right<T=string> { value:T }\n\
                  declare const left:Left; declare const right:Right;";
    let (diagnostics, types) =
        default_diagnostics_and_queries(source, &["left", "right", "left", "right"]);
    assert!(diagnostics.is_empty(), "{diagnostics:?}");
    assert_eq!(types, ["Left<number>", "Right<string>", "Left<number>", "Right<string>"]);
}
