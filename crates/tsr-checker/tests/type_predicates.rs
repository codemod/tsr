//! Type predicates in a signature's return position.
//!
//! **Every expected string below was copied out of a `.types` baseline before
//! it was written down**, and the source line above each one is the baseline's
//! own source line. This is not a formality: five expectations written from
//! intuition across earlier sessions were all wrong and the port was right
//! every time (`docs/conventions.md`). The baselines used here are
//! `conformance/controlFlow/assertionTypePredicates1`,
//! `conformance/expressions/typeGuards/typeGuardFunctionOfFormThis`,
//! `conformance/expressions/typeGuards/typeGuardOfFormIsType` and
//! `compiler/inferTypePredicates`.
//!
//! The reasoning is in `docs/architecture/checker-notes-typepred.md`.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named top-level declaration.
///
/// Through `type_to_string` for the reason `tests/types.rs` gives: the printed
/// form is what a `.types` baseline compares, and a type that is internally
/// right but prints wrong fails conformance identically.
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
fn a_written_predicate_prints_in_the_signatures_return_position() {
    // `conformance/controlFlow/assertionTypePredicates1.types`, verbatim:
    //
    // ```text
    // declare function isString(value: unknown): value is string;
    // >isString : (value: unknown) => value is string
    // ```
    //
    // and `conformance/expressions/typeGuards/typeGuardOfFormIsType.types`
    // for a predicate naming a declared type:
    //
    // ```text
    // >isC2 : (x: any) => x is C2
    // ```
    assert_eq!(
        type_of_declaration(
            "declare function isString(value: unknown): value is string;",
            "isString"
        ),
        "(value: unknown) => value is string"
    );
    assert_eq!(
        type_of_declaration(
            "interface C2 { p2: number }\ndeclare function isC2(x: any): x is C2;",
            "isC2"
        ),
        "(x: any) => x is C2"
    );
}

#[test]
fn the_asserts_forms_print_with_and_without_an_is_clause() {
    // Same baseline:
    //
    // ```text
    // declare function assertIsString(value: unknown): asserts value is string;
    // >assertIsString : (value: unknown) => asserts value is string
    //
    // const assert: (value: unknown) => asserts value = value => {}
    // >assert : (value: unknown) => asserts value
    // ```
    //
    // The bare form is the one worth pinning: upstream records a nil predicate
    // type and emits **no** `is` clause, so a renderer that always wrote
    // ` is <type>` would print `asserts value is void` — plausible, and wrong
    // on every line of this family.
    assert_eq!(
        type_of_declaration(
            "declare function assertIsString(value: unknown): asserts value is string;",
            "assertIsString"
        ),
        "(value: unknown) => asserts value is string"
    );
    assert_eq!(
        type_of_declaration(
            "declare function assertWeird(value: unknown): asserts value;",
            "assertWeird"
        ),
        "(value: unknown) => asserts value"
    );
}

#[test]
fn a_this_predicate_prints_this_and_not_the_parameter_name() {
    // `conformance/expressions/typeGuards/typeGuardFunctionOfFormThis.types`:
    //
    // ```text
    // isLeader(): this is LeadGuard {
    // >isLeader : () => this is LeadGuard
    // ```
    //
    // Asserted through a **type literal in annotation position** rather than
    // through the baseline's class: `get_type_of_symbol` on an interface name
    // is a type-meaning lookup this harness does not do, and the member
    // renderer under test (`objects::signature_member_text`) is the same one
    // either way.
    assert_eq!(
        type_of_declaration(
            "interface LeadGuard { lead(): void }\n\
             declare const guard: { isLeader(): this is LeadGuard };",
            "guard"
        ),
        "{ isLeader(): this is LeadGuard; }"
    );
}

#[test]
fn a_predicate_is_the_declarations_print_and_boolean_is_the_calls() {
    // The two halves of the arm, on one declaration. Upstream records both in
    // `conformance/expressions/typeGuards/typeGuardOfFormIsType.types`:
    //
    // ```text
    // >isFunction : (x: any) => x is Function
    // >isFunction(x) : boolean
    // ```
    //
    // `getTypeFromTypeNode` answers `boolean` for the predicate node
    // (`checker.go:22858`) and the node builder overrides that in return
    // position only (`nodebuilderimpl.go:1748`). **The pair is the test**: a
    // build that printed the predicate everywhere would pass the first
    // assertion and fail the second, which is exactly what the first run of
    // `examples/predgap.rs` forecast before it was corrected.
    let source = "\
declare function isString(x: unknown): x is string;
declare const v: unknown;
const called = isString(v);
";
    assert_eq!(type_of_declaration(source, "isString"), "(x: unknown) => x is string");
    assert_eq!(type_of_declaration(source, "called"), "boolean");
}

#[test]
fn an_asserts_signature_returns_void_where_a_plain_predicate_returns_boolean() {
    // `conformance/controlFlow/assertionTypePredicates1.types` records the
    // call side of the `asserts` form as `void`:
    //
    // ```text
    // >assertIsString(x) : void
    // ```
    //
    // which is `getTypeFromTypeNode`'s `voidType` branch — the half of
    // `checker.go:22858` the plain form does not exercise.
    let source = "\
declare function assertIsString(x: unknown): asserts x is string;
declare const v: unknown;
const called = assertIsString(v);
";
    assert_eq!(type_of_declaration(source, "called"), "void");
}

#[test]
fn a_predicate_whose_type_is_unported_gaps_and_a_ported_one_does_not() {
    // **A pair, deliberately.** The refused half here is not "predicates are
    // unported" — that stand-in has come due twelve times on this project and
    // stops discriminating the moment the arm lands. It is the standing rule
    // that a construct refuses *whole*: a predicate whose own type node this
    // port cannot resolve is a gap, not `x is error` and not `x is any`.
    //
    // `keyof` is the unported type node used as the stand-in
    // (`declared.rs`'s `TypeOperatorNode` arm takes `readonly` only), and the
    // ported half beside it is the same declaration with a resolvable type. If
    // `keyof` lands, this test comes due and should be re-pointed at whatever
    // type node is still refused then — not deleted.
    assert_eq!(
        type_of_declaration("declare function f<T>(x: unknown, o: T): x is keyof T;", "f"),
        "error"
    );
    assert_eq!(
        type_of_declaration("declare function g<T>(x: unknown, o: T): x is T;", "g"),
        "<T>(x: unknown, o: T) => x is T"
    );
}

#[test]
fn a_return_type_on_its_own_line_is_not_glued_to_a_member_named_is() {
    // `conformance/expressions/typeGuards/typePredicateASI.ts`, verbatim, and
    // its baseline says `>foo : (callback: (a: any, b: any) => void) => I`.
    //
    // Upstream guards the predicate on `!p.hasPrecedingLineBreak()`
    // (`parser.go:3408`). Without it the interface's *next member's* name is
    // eaten as the predicate's `is`, which printed `=> I is any` and dropped
    // `is()` from the type. Found in this build's residual wrong lines.
    // Through a type literal for the same harness reason as the `this` test
    // above; the parser rule under test is indifferent to the enclosing node.
    assert_eq!(
        type_of_declaration(
            "interface I { q: number }\n\
             declare const v: {\n    foo(callback: (a: any, b: any) => void): I\n    is(): boolean;\n};",
            "v"
        ),
        "{ foo(callback: (a: any, b: any) => void): I; is(): boolean; }"
    );
}
