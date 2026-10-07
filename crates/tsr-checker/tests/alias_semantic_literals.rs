//! Native `getDeclaredTypeOfTypeAlias` resolves pre-existing literal and parameter
//! bodies unchanged; a generic alias must not turn them into nominal objects.

use tsr_ast::{Node, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

fn declared_and_reference(source: &str, name: &str) -> (String, String) {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let symbol = bound.lookup_local(root, name).unwrap();
    let annotation = parsed
        .source_file
        .statements
        .iter()
        .find_map(|statement| {
            let Statement::VariableStatement(statement) = statement else { return None };
            statement.declaration_list?.declarations.first()?.r#type
        })
        .unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let declared = checker.get_declared_type_of_symbol(symbol);
    let reference = checker.get_type_from_type_node(annotation);
    (checker.type_to_string(declared), checker.type_to_string(reference))
}

#[test]
fn generic_literal_aliases_retain_their_body_type() {
    for literal in ["\"fixed\"", "42", "-42", "true", "false", "null", "123n"] {
        let source = format!("type Fixed<T> = ({literal}); declare let value: Fixed<string>;");
        assert_eq!(declared_and_reference(&source, "Fixed"), (literal.into(), literal.into()));
    }
}

#[test]
fn parenthesized_keyword_bodies_remain_intrinsic_types() {
    for keyword in [
        "any",
        "unknown",
        "string",
        "number",
        "bigint",
        "boolean",
        "symbol",
        "void",
        "undefined",
        "never",
        "object",
    ] {
        let source =
            format!("type Primitive<T> = (({keyword})); declare let value: Primitive<string>;");
        assert_eq!(declared_and_reference(&source, "Primitive"), (keyword.into(), keyword.into()));
    }
}

#[test]
fn parenthesized_indexed_alias_body_substitutes_arguments() {
    assert_eq!(
        declared_and_reference(
            "type Get<T, K extends keyof T> = ((T[K])); declare let value: Get<{ item: string }, \"item\">;",
            "Get",
        ).1,
        "string",
    );
}

#[test]
fn parenthesized_conditional_alias_body_uses_native_branch_resolution() {
    assert_eq!(
        declared_and_reference(
            "type Select<T> = ((T extends string ? number : boolean)); declare let value: Select<string>;",
            "Select",
        ).1,
        "number",
    );
}

#[test]
fn qualified_conditional_alias_chain_resolves_its_native_target() {
    assert_eq!(
        declared_and_reference(
            "namespace N { export type Select<T> = T extends string ? number : boolean; } type Outer<T> = N.Select<T>; declare let value: Outer<string>;",
            "Outer",
        ).1,
        "number",
    );
}

#[test]
fn parenthesized_keyword_body_retains_intrinsic_under_outer_alias_mapper() {
    assert_eq!(
        declared_and_reference(
            "type Primitive<T> = ((string)); type Outer<T> = { item: Primitive<T> }[\"item\"]; declare let value: Outer<number>;",
            "Outer",
        ).1,
        "string",
    );
}

#[test]
fn parenthesized_variadic_alias_normalizes_ordered_tuple_arguments() {
    assert_eq!(
        declared_and_reference(
            "type Prepend<T extends unknown[]> = (([string, ...T])); declare let value: Prepend<[number, boolean]>;",
            "Prepend",
        ).1,
        "[string, number, boolean]",
    );
}

#[test]
fn parenthesized_identity_mapped_alias_preserves_primitive_argument() {
    assert_eq!(
        declared_and_reference(
            "type Copy<T> = (({ [K in keyof T]: T[K] })); declare let value: Copy<string>;",
            "Copy",
        )
        .1,
        "string",
    );
}

#[test]
fn parenthesized_mapped_arguments_keep_distinct_ordered_tuple_images() {
    let source = "type Copy<T> = (({ [K in keyof T]: T[K] })); declare let first: Copy<[string, number]>; declare let reversed: Copy<[number, string]>; declare let single: Copy<[boolean]>;";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let mut images = Vec::new();
    for (name, expected) in [
        ("first", "[string, number]"),
        ("reversed", "[number, string]"),
        ("single", "[boolean]"),
        ("first", "[string, number]"),
    ] {
        let symbol = bound.lookup_local(root, name).unwrap();
        let ty = checker.get_type_of_symbol(symbol);
        assert_eq!(checker.type_to_string(ty), expected);
        images.push(ty);
    }
    assert_ne!(images[0], images[1]);
    assert_ne!(images[0], images[2]);
    assert_eq!(images[0], images[3]);
}

#[test]
fn mapped_identity_uses_resolved_parenthesized_parameter_operands() {
    assert_eq!(
        declared_and_reference(
            "type Copy<T> = { [K in keyof (T)]: (T)[(K)] }; declare let value: Copy<string>;",
            "Copy",
        )
        .1,
        "string",
    );
}

#[test]
fn parameter_body_alias_maps_its_actual_ordered_owner() {
    assert_eq!(
        declared_and_reference(
            "type Second<A, B> = ((B)); declare let value: Second<number, string>;",
            "Second",
        ),
        ("B".into(), "string".into()),
    );
}

#[test]
fn mapped_identity_parenthesized_constraint_and_template_preserve_primitive() {
    assert_eq!(
        declared_and_reference(
            "type Copy<T> = { [K in (keyof T)]: ((T[K])) }; declare let value: Copy<string>;",
            "Copy",
        )
        .1,
        "string",
    );
}

#[test]
fn identity_mapped_object_images_reuse_only_the_same_argument_identity() {
    let source = "type Copy<T> = { [K in keyof T]: T[K] }; declare const a: Copy<{ item: string }>; declare const b: Copy<{ item: number }>;";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let a = bound.lookup_local(root, "a").unwrap();
    let b = bound.lookup_local(root, "b").unwrap();
    let first = checker.get_type_of_symbol(a);
    let distinct = checker.get_type_of_symbol(b);
    assert_ne!(first, distinct);
    assert_eq!(checker.get_type_of_symbol(a), first);
    assert_eq!(checker.type_to_string(first), "Copy<{ item: string; }>");
    assert_eq!(checker.type_to_string(distinct), "Copy<{ item: number; }>");
}

#[test]
fn array_constrained_homomorphic_any_maps_to_array() {
    assert_eq!(
        declared_and_reference(
            "interface Array<T> {} type Copy<T extends unknown[]> = { [K in keyof T]: T[K] }; declare let value: Copy<any>;",
            "Copy",
        ).1,
        "any[]",
    );
}

#[test]
fn unique_operator_requires_symbol_operand() {
    assert_eq!(
        declared_and_reference(
            "declare const value: unique number; type Alias = typeof value;",
            "Alias"
        )
        .1,
        "error",
    );
}

#[test]
fn resolved_literal_groups_call_before_construct_signatures() {
    assert_eq!(
        declared_and_reference(
            "type Unused = string; declare let value: { new (x: number): number; (x: string): string; new (x: boolean): boolean; (x: number): number; item: string };",
            "Unused",
        ).1,
        "{ (x: string): string; (x: number): number; new (x: number): number; new (x: boolean): boolean; item: string; }",
    );
}

#[test]
fn semantic_signature_vector_groups_calls_before_constructors() {
    let source = "declare const f: { new(x: number): number; (x: string): string; new(x: boolean): boolean; (x: number): number };";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let ty = checker.get_type_of_symbol(bound.lookup_local(root, "f").unwrap());
    let kinds: Vec<_> =
        checker.signatures_of_type(ty).unwrap().iter().map(|signature| signature.kind).collect();
    use tsr_checker::signatures::SignatureKind::{Call, Construct};
    assert_eq!(kinds, [Call, Call, Construct, Construct]);
}

#[test]
fn keyof_parenthesized_parameter_retains_deferred_operand() {
    assert_eq!(
        declared_and_reference(
            "type Keys<T> = keyof ((T)); declare let value: Keys<{ item: string }>;",
            "Keys",
        ),
        ("keyof T".into(), "\"item\"".into()),
    );
}

#[test]
fn tuple_spread_never_normalizes_to_never() {
    assert_eq!(
        declared_and_reference(
            "type Unused = string; declare let value: [number, ...never, boolean];",
            "Unused",
        )
        .1,
        "never",
    );
}

#[test]
fn source_tuple_variadic_union_distributes_in_branch_order() {
    assert_eq!(
        declared_and_reference(
            "type Unused = string; declare let value: [number, ...([string] | [boolean])];",
            "Unused",
        )
        .1,
        "[number, string] | [number, boolean]",
    );
}

#[test]
fn concrete_optional_spread_before_required_tail_normalizes_slot() {
    assert_eq!(
        declared_and_reference(
            "type Unused = string; declare let value: [...[string?], number];",
            "Unused",
        )
        .1,
        "[string | undefined, number]",
    );
}

#[test]
fn nested_tuple_spread_flattens_fixed_prefix_and_array_rest() {
    assert_eq!(
        declared_and_reference(
            "interface Array<T> {} type Unused = string; declare let value: [...[string, ...number[]], boolean];",
            "Unused",
        ).1,
        "[string, ...number[], boolean]",
    );
}

#[test]
fn parenthesized_parameter_body_retains_parameter_identity() {
    assert_eq!(
        declared_and_reference("type Id<T> = ((T)); declare let value: Id<string>;", "Id"),
        ("T".into(), "string".into()),
    );
}

#[test]
fn explicit_literal_union_keeps_its_alias_identity() {
    assert_eq!(
        declared_and_reference(
            "type Choice<T> = \"a\" | \"b\"; declare let value: Choice<string>;",
            "Choice"
        ),
        ("Choice<T>".into(), "Choice<string>".into()),
    );
}
