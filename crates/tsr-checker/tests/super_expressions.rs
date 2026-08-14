//! What `super` must get right.
//!
//! `checkSuperExpression` (`checker.go:7854`) answers two different things for
//! the same keyword: inside a `super(...)` call or a static member it is the
//! base's **static** side, `typeof B`; elsewhere it is the base's instance
//! type. `getBaseConstructorTypeOfClass` (`:17434`) is the static half, and it
//! reads `getEffectiveBaseTypeNode` — **this class node's** heritage — and
//! types its *expression*.

use tsr_ast::{Node, SyntaxKind};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the file's single `super` keyword.
///
/// Found by walking the tree rather than by position, because the fixtures put
/// it at different depths and a positional helper would silently type the wrong
/// node when one of them changes.
fn type_of_super(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);

    let mut stack: Vec<Node<'_>> = vec![Node::SourceFile(parsed.source_file)];
    let mut children = Vec::new();
    let mut found = None;
    while let Some(node) = stack.pop() {
        if let Node::KeywordExpression(keyword) = node
            && keyword.kind == SyntaxKind::SuperKeyword
        {
            found = Some(tsr_ast::Expression::KeywordExpression(keyword));
            break;
        }
        children.clear();
        tsr_ast::push_children(node, &mut children);
        stack.extend(children.iter().copied());
    }
    let expression = found.expect("the fixture must contain a `super`");
    let id = checker.check_expression(expression);
    checker.type_to_string(id)
}

/// §202. `super` on the STATIC side ignores the heritage entry's type
/// arguments, and reads only the CLASS's own heritage.
///
/// `getBaseConstructorTypeOfClass` (`checker.go:17434`) types
/// `getEffectiveBaseTypeNode`'s *expression*, so `B` is `typeof B` whatever
/// follows it in angle brackets. This port shared one helper with the instance
/// road, which refuses a heritage entry carrying type arguments because it
/// cannot instantiate the member table — a refusal the static road does not
/// need.
#[test]
fn super_in_a_constructor_ignores_the_bases_type_arguments() {
    assert_eq!(
        type_of_super(
            "declare class B<T> { }\nclass D extends B<any> { constructor() { super(); } }"
        ),
        "typeof B"
    );
}

/// The instance side still refuses, because there the arguments really do
/// decide the answer. Without this the test above passes for a checker that
/// dropped the refusal everywhere.
#[test]
fn super_as_a_receiver_still_refuses_an_instantiated_base() {
    assert_eq!(
        type_of_super(
            "declare class B<T> { m(): void }\nclass D extends B<any> { n() { super.m(); } }"
        ),
        "error"
    );
}

/// Only the CLASS's own heritage counts. A class merged with an interface that
/// extends something is still a class with no base — upstream reports on the
/// `super()` and answers `any`, not the interface's base.
/// `conformance/superCallFromClassThatHasNoBaseTypeButWithSameSymbolInterface`
/// was the single loss on this arm's first measurement.
#[test]
fn an_interface_merged_with_the_class_contributes_no_base_to_super() {
    assert_eq!(
        type_of_super(
            "interface Foo extends Array<number> {}\nclass Foo { constructor() { super(); } }"
        ),
        "error"
    );
}

/// A heritage entry with MORE type arguments than the base declares is an
/// upstream error and the base type becomes `errorType`, so `super` is not
/// `typeof A`. Measured: without this, two cases lose a line.
#[test]
fn too_many_type_arguments_on_the_base_make_super_a_gap() {
    assert_eq!(
        type_of_super(
            "class A { }\nclass B extends A<number, string> { constructor() { super(); } }"
        ),
        "error"
    );
}

/// §481: upstream skips arrows only for a NON-CALL `super`
/// (`checker.go:7860`), so a `super()` whose container is an arrow fails
/// `IsConstructorDeclaration` and errors — `errorType`, printed `any`
/// (`derivedClassConstructorWithoutSuperCall` records `>super : any` for
/// `() => super()` inside a constructor).
#[test]
fn a_super_call_through_an_arrow_is_the_deliberate_error_any() {
    assert_eq!(
        type_of_super(
            "class Base { }\nclass D extends Base { constructor() { var r = () => super(); } }"
        ),
        "any"
    );
}

/// The control §481 must not break: a super PROPERTY access through an
/// arrow keeps the transparent-arrow behaviour and answers the instance
/// side.
#[test]
fn a_super_property_access_through_an_arrow_still_answers() {
    assert_eq!(
        type_of_super(
            "class Base { m() {} }\nclass D extends Base { m() { var r = () => super.m; } }"
        ),
        "Base"
    );
}

/// §481's second arm: `super` inside a COMPUTED PROPERTY NAME
/// (`checker.go:7893`'s FindAncestor check) — `computedPropertyNames27_ES6`
/// records `>super : any` in `[super.toString()]`.
#[test]
fn a_super_in_a_computed_property_name_is_the_deliberate_error_any() {
    assert_eq!(
        type_of_super("class Base { }\nclass D extends Base { [super.toString()]() { } }"),
        "any"
    );
}

/// The skip is the rule, not the position: a computed name is not inside
/// the member it names, so the search continues OUTSIDE it — through the
/// object literal and even through a nested class — and a legal outer
/// member answers. `computedPropertyNames25_ES6` records `>super : Base`
/// for the object-literal shape;
/// `superPropertyAccessInComputedPropertiesOfNestedType_ES6` for the
/// nested-class one.
#[test]
fn a_computed_name_in_an_object_literal_inside_a_method_is_legal() {
    assert_eq!(
        type_of_super(
            "class Base { bar() { return 0; } }\nclass D extends Base { foo() { var obj = { [super.bar()]() { } }; } }"
        ),
        "Base"
    );
}

#[test]
fn a_computed_name_on_a_nested_class_member_answers_the_outer_base() {
    assert_eq!(
        type_of_super(
            "class A { foo() { return 1; } }\nclass B extends A { bar() { return class { [super.foo()]() { } }; } }"
        ),
        "A"
    );
}
