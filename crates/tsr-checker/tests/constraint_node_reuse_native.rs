//! Written constraint subtrees follow pinned `nodecopy.go`, independently of
//! the parent-owned top-level type-parameter serialization cutover.

use tsr_checker::Checker;
use tsr_core::Arena;

fn reused_return(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "constraint.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let symbol = bound.lookup_local(root, "source").unwrap();
    let signature = checker.get_signatures_of_symbol(symbol).unwrap().remove(0);
    let written = signature.written_return.unwrap();
    let text = checker.site_free_annotation_text(written, signature.r#type).unwrap();
    // The node's written spelling cannot survive replacement by an unrelated
    // semantic image, even though the same retained candidate is supplied.
    assert!(checker.site_free_annotation_text(written, checker.intrinsics().number).is_none());
    text
}

#[test]
fn nested_constraints_keep_aliases_and_fill_untyped_rest_with_keyword_any() {
    // Native nodecopy.go:660 inserts an AnyKeyword node, not the rest
    // parameter's semantic any[] type. Both callable syntaxes need the visit.
    assert_eq!(
        reused_return(
            "type Keys = 'a' | 'b'; declare function source(): \
             <K extends Keys, F extends (...args) => void, G extends { (...args): void }>() => K;",
        ),
        "<K extends Keys, F extends (...args: any) => void, G extends { (...args: any): void; }>() => K",
    );
}

#[test]
fn existing_node_defaults_and_constraints_keep_their_written_qualified_alias() {
    // nodecopy.go:560 visits BOTH fields of a reused type-parameter node.
    // This is distinct from typeParameterToDeclarationWithConstraint, which
    // serializes a semantic default and may shorten its alias to Keys.
    assert_eq!(
        reused_return(
            "namespace Public { export type Keys = 'a' | 'b'; } \
             declare function source(): <K extends Public.Keys = Public.Keys>(x: K) => K;",
        ),
        "<K extends Public.Keys = Public.Keys>(x: K) => K",
    );
}

#[test]
fn written_constraint_operators_do_not_collapse_to_their_semantic_image() {
    // These are the constraint shapes in cannotIndexGenericWritingError and
    // correlatedUnions. Reusing their AST must preserve constituent order,
    // keyof, and indexed access even when semantic resolution sorts/reduces.
    assert_eq!(
        reused_return(
            "declare function source(): \
             <T extends number[] & { [s: string]: number | string }, \
             K extends keyof { sum: [a: number, b: number]; concat: [a: string, b: string, c: string] }, \
             V extends { key: number | string }['key']>() => V;",
        ),
        "<T extends number[] & { [s: string]: number | string; }, K extends keyof { sum: [a: number, b: number]; concat: [a: string, b: string, c: string]; }, V extends { key: number | string; }['key']>() => V",
    );
}

#[test]
fn written_parenthesized_constraints_are_not_replaced_by_semantic_types() {
    // typeParameterConstraints1 writes (1), not 1. The simple operand visitor
    // must also preserve the emitted parentheses in these actual controls.
    assert_eq!(
        reused_return(
            "interface Obj { key: string } declare function source(): \
             <K extends keyof (Obj), V extends (Obj)['key'], L extends (1)>() => V;",
        ),
        "<K extends keyof (Obj), V extends (Obj)['key'], L extends (1)>() => V",
    );
}

#[test]
fn constraint_query_uses_source_symbol_identity_at_shadowed_print_sites() {
    let source = "const keys = { a: 1, b: 2 }; \
                  declare function source(): <K extends keyof typeof keys>(key: K) => K; \
                  const original = source; \
                  function shadow() { const keys = { c: 3 }; const shadowView = source; } \
                  namespace Other { export const keys = { d: 4 }; const siblingView = source; }";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "constraint.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let symbol = bound.lookup_local(parsed.source_file.node_id.unwrap(), "source").unwrap();
    let signature = checker.get_signatures_of_symbol(symbol).unwrap().remove(0);
    for (name, expected) in [
        ("original", "<K extends keyof typeof keys>(key: K) => K"),
        ("shadowView", "<K extends keyof typeof globalThis.keys>(key: K) => K"),
        ("siblingView", "<K extends keyof typeof globalThis.keys>(key: K) => K"),
    ] {
        let site = (0..u32::try_from(parsed.nodes.len()).unwrap())
            .find_map(|index| {
                let id = tsr_ast::NodeId::new(index);
                matches!(parsed.node_map.get(id), Some(tsr_ast::Node::Identifier(identifier)) if identifier.text == name)
                    .then_some(id)
            })
            .unwrap();
        assert_eq!(
            checker
                .written_annotation_text_at(
                    signature.written_return.unwrap(),
                    signature.r#type,
                    site
                )
                .unwrap(),
            expected,
        );
    }
}

#[test]
fn property_constraints_preserve_written_intersection_and_optional_annotation() {
    // spreadObjectOrFalsy's z resolves semantically to T. Reusing the
    // containing constraint still prints (T | undefined) & T, and does not
    // add the optional property's semantic undefined to its annotation.
    assert_eq!(
        reused_return(
            "declare function source(): \
             <T extends {}, A extends { z: (T | undefined) & T; readonly q?: T | null }>(a: A) => T;",
        ),
        "<T extends {}, A extends { z: (T | undefined) & T; readonly q?: T | null; }>(a: A) => T",
    );
}

#[test]
fn nested_generic_scopes_preserve_distinct_written_type_parameters() {
    assert_eq!(
        reused_return(
            "type Keys = 'a' | 'b'; declare function source(): \
             <T>(value: <T extends Keys = Keys>(x: T) => T) => T;",
        ),
        "<T>(value: <T extends Keys = Keys>(x: T) => T) => T",
    );
}
