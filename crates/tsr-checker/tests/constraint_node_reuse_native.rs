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
fn nested_generic_scopes_preserve_distinct_written_type_parameters() {
    assert_eq!(
        reused_return(
            "type Keys = 'a' | 'b'; declare function source(): \
             <T>(value: <T extends Keys = Keys>(x: T) => T) => T;",
        ),
        "<T>(value: <T extends Keys = Keys>(x: T) => T) => T",
    );
}
