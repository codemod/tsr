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
    // `errorType` and `anyType` both print `any`, and conflating them is how a
    // gap becomes an assertion. Everything unported must land on `error`.
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
    // A type reference has no implementation yet. It must yield `errorType`, so
    // that `checker_types` sees a gap rather than a confident `any`. Both print
    // `any`, which is exactly why this is asserted on identity, not on text.
    let arena = Arena::new();
    let source = "interface I {}\ndeclare const x: I;";
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
    // A function symbol goes to `getTypeOfFuncClassEnumModule`, unported. It must
    // read as a gap rather than as `any`.
    let arena = Arena::new();
    let source = "function f() {}";
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, "f").expect("`f` is declared");
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    assert_eq!(id, checker.intrinsics().error);
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
