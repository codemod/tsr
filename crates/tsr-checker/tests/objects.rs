//! What an object literal **expression** must get right.
//!
//! The printed form is the same one `{ a: string }` produces as a type node, so
//! every assertion here has to be about a fixture the *type-node* path cannot
//! reach — an expression. See
//! [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Object literal expressions, and the two widenings".

use tsr_ast::Statement;
use tsr_checker::{Checker, TypeFlags};
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
fn an_object_literal_prints_structurally_in_upstreams_exact_form() {
    // Whole-line comparison, so the spaces and the trailing `; ` are the answer
    // and not a style choice.
    assert_eq!(type_of_initialiser("const o = { a: 1 };"), "{ a: number; }");
    assert_eq!(type_of_initialiser("const o = {};"), "{}");
    assert_eq!(type_of_initialiser(r#"const o = { a: 1, b: "s" };"#), "{ a: number; b: string; }");
    // Members keep **source** order, unlike a union's constituents. Upstream
    // builds a symbol table in declaration order and never sorts it.
    assert_eq!(type_of_initialiser(r#"const o = { b: "s", a: 1 };"#), "{ b: string; a: number; }");
}

#[test]
fn a_member_widens_where_a_bare_const_does_not() {
    // The property boundary is where freshness stops, and it is the single
    // easiest thing to get wrong in an object-literal port.
    //
    // `const n = 1` is `1` — `getWidenedLiteralTypeForInitializer` returns the
    // initialiser's type unchanged for a constant. `const o = { a: 1 }` is
    // `{ a: number; }`, because `checkExpressionForMutableLocation` widens each
    // member as the literal is checked (`checker.go:13885`). Upstream records
    // `>obj1 : { a: number; }` for exactly this source.
    assert_eq!(type_of_initialiser("const n = 1;"), "1");
    assert_eq!(type_of_initialiser("const o = { a: 1 };"), "{ a: number; }");
    assert_eq!(type_of_initialiser(r#"const o = { a: "s" };"#), "{ a: string; }");
    assert_eq!(type_of_initialiser("const o = { a: true };"), "{ a: boolean; }");
    // `let` versus `const` makes no difference *inside* a literal — the member
    // widened before the declaration was ever consulted.
    assert_eq!(type_of_initialiser("let o = { a: 1 };"), "{ a: number; }");
}

#[test]
fn a_nested_object_literal_is_typed_by_the_same_rule() {
    assert_eq!(type_of_initialiser("const o = { a: { b: 1 } };"), "{ a: { b: number; }; }");
}

#[test]
fn a_string_named_property_prints_unquoted_only_when_it_is_an_identifier() {
    assert_eq!(type_of_initialiser(r#"const o = { "a": 1 };"#), "{ a: number; }");
    // A name that is not an identifier is re-quoted the way upstream's printer
    // does. This asserted `error` until quoting landed; the fixture was found by
    // grepping the suite for stand-ins BEFORE the work rather than after, which
    // is the gap-fixture rule applied in the direction that actually helps.
    assert_eq!(type_of_initialiser(r#"const o = { "a-b": 1 };"#), r#"{ "a-b": number; }"#);
}

#[test]
fn a_member_this_port_cannot_type_makes_the_whole_literal_a_gap() {
    // The same rule the type-node path already follows: a partial object type is
    // a wrong answer that looks like a right one. §31 changed the MEMBER's
    // answer: a truly unresolved name is upstream's TS2304 `any`, so the
    // literal builds `{ a: any; }` — upstream's own baseline shape here.
    assert_eq!(type_of_initialiser("const o = { a: unknownThing };"), "{ a: any; }");
    // A method needs a signature. A gap, not faked.
    //
    // The shorthand `{ a }` used to be asserted here as a second gap. It is now
    // ported (see `tests/shorthand_properties.rs`) and answers `{ a: number; }`,
    // so the line was removed rather than updated: this test is about members
    // that CANNOT be typed, and a member that can no longer belongs in it. That
    // is the gap-fixture hazard — a fixture standing in for "unported" must be a
    // failure, not a form, or it silently asserts the opposite of its name.
    //
    // **`{ ...{ a: 1 } }` was removed for the same reason and by the same rule**
    // when object spread landed (`bd tsr-sps`); it now answers
    // `{ a: number; }` and is covered by `tests/members_object_spread.rs`. The
    // precedent set two lines above is what said to delete rather than update,
    // and this is the second time this fixture file has paid for having it
    // written down.
    // **And a third time, for the method fixture.** `{ m() { return 1; } }`
    // now answers `{ m(): number; }` — object-literal methods are ported
    // (`crate::objects`' `MethodDeclaration` arm) — so by the rule stated
    // above it is removed here rather than updated, and covered positively in
    // `a_method_member_prints_as_a_signature` below.
    //
    // The rule has now been applied three times in this one file. What is left
    // must be a member that genuinely cannot be typed: a computed name.
    assert_eq!(type_of_initialiser("const o = { [1]: 1 };"), "error");
    // A method whose *signature* cannot be built keeps the whole literal a
    // gap, which is the property the removed line was really testing.
    assert_eq!(type_of_initialiser("const o = { m(x: keyof string) {} };"), "error");
}

#[test]
fn a_nullable_member_is_a_gap_because_the_declaration_would_need_widening() {
    // Upstream records **two different types** for one source line:
    //
    //     var c = {x: null};
    //     >c : { x: any; }            ← getWidenedType, at the declaration
    //     >{x: null} : { x: null; }   ← checkObjectLiteral
    //
    // This port has no call site for the first (`crate::symbols` is another
    // workstream, `bd tsr-mli`), so answering the second alone would make the
    // declaration line wrong. A gap until both can be right.
    assert_eq!(type_of_initialiser("var c = { x: null };"), "error");
    assert_eq!(type_of_initialiser("var c = { x: undefined };"), "error");
    // A non-nullable member is unaffected — the guard must not be a blanket one.
    assert_eq!(type_of_initialiser("var c = { x: 1 };"), "{ x: number; }");
}

#[test]
fn an_object_literal_type_carries_the_symbol_a_property_access_looks_in() {
    // Without it `o.a` cannot resolve, which is the point of computing the type.
    let source = "const o = { a: 1 };";
    let arena = Arena::new();
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
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    let ty = checker.type_of(id);
    assert!(ty.flags.contains(TypeFlags::OBJECT), "an object literal type is an object type");
    let tsr_checker::TypeData::Named { members, .. } = &ty.data else {
        panic!("an object literal type prints structurally, like a type literal");
    };
    assert!(members.is_some(), "and carries the symbol its properties live in");
}

#[test]
fn a_method_member_prints_as_a_signature() {
    // `m(): void`, not `m: () => void` — the distinction `Member`'s own doc
    // calls out and the reason `Member::Signature` exists. The corpus prints
    // 3,012 object types carrying a method; `{ fn(): void; }` (68 instances)
    // and `{ log(msg: any): void; }` (366) are the head of that population.
    assert_eq!(type_of_initialiser("const o = { m() {} };"), "{ m(): void; }");
    assert_eq!(type_of_initialiser("const o = { m() { return 1; } };"), "{ m(): number; }");
    assert_eq!(
        type_of_initialiser("const o = { log(msg: any): void {} };"),
        "{ log(msg: any): void; }"
    );
    // Beside a property, so the two member spellings are rendered by one pass
    // and the ordering is the literal's own.
    assert_eq!(type_of_initialiser("const o = { a: 1, m() {} };"), "{ a: number; m(): void; }");
}
