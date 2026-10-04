//! Pinned tsgo 5b1047d nodebuilderimpl.go: typeToTypeNode formats union
//! slots and createTypeNodesFromResolvedType renders completed member images
//! from the viewing declaration. Native control: structuralReferenceSite.ts.

use tsr_ast::{Node, NodeId, NodeMap, push_children};
use tsr_checker::Checker;
use tsr_core::Arena;

fn identifier(map: &NodeMap<'_>, node: NodeId, name: &str) -> Option<NodeId> {
    let current = map.get(node)?;
    if matches!(current, Node::Identifier(id) if id.text == name) {
        return Some(node);
    }
    let mut children = Vec::new();
    push_children(current, &mut children);
    children.into_iter().find_map(|child| identifier(map, child.node_id()?, name))
}

/// Read one completed namespace member, then serialize its SAME semantic
/// identity at asymmetrical sites in forward/reverse/repeated order. A global
/// text rewrite or first-site cache cannot satisfy both views. Snapshot the
/// payload and reference arguments to catch a print mutating semantic types.
fn views(source: &str, member: &str, expected: &[(&str, &str)]) {
    for reverse in [false, true] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "t.ts", text: source },
        );
        let namespace = bound.merged_symbol(*bound.globals().get("N").unwrap());
        let symbol = *bound.symbols().get(namespace).exports.get(member).unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let ty = checker.get_type_of_symbol(symbol);
        let data = checker.type_of(ty).data.clone();
        let reference = checker.type_reference_target(ty).cloned();
        let sites: Vec<_> = expected
            .iter()
            .map(|(site, text)| {
                (
                    identifier(&parsed.node_map, parsed.source_file.node_id.unwrap(), site)
                        .unwrap(),
                    text,
                )
            })
            .collect();
        for repetition in 0..4 {
            let mut order: Vec<_> = (0..sites.len()).collect();
            if reverse ^ (repetition % 2 == 1) {
                order.reverse();
            }
            for index in order {
                let (site, text) = sites[index];
                assert_eq!(checker.type_to_string_at(ty, site).as_deref(), Some(*text));
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(checker.type_of(ty).data, data);
                assert_eq!(checker.type_reference_target(ty), reference.as_ref());
            }
        }
    }
}

#[test]
fn inferred_nullable_union_uses_inside_outside_and_shadowed_alias_views() {
    views(
        "namespace N { export class C {} export declare const v: C | null; const inside = v; }
         import Alias = N; const outside = N.v; function shadow<N>() { const hidden = Alias.v; }",
        "v",
        &[("inside", "C | null"), ("outside", "Alias.C | null"), ("hidden", "Alias.C | null")],
    );
}

#[test]
fn generic_argument_slots_visit_union_structure_without_changing_reference_identity() {
    views(
        "namespace N { export class C {} export interface Box<T> { item: T }
         export declare const v: Box<C | null>; const inside = v; }
         import Alias = N; const outside = N.v;",
        "v",
        &[("inside", "Box<C | null>"), ("outside", "Alias.Box<Alias.C | null>")],
    );
}

#[test]
fn completed_construct_method_and_property_slots_render_at_the_viewer() {
    views(
        "namespace N { export class C {} export interface Box<T> { item: T }
         export declare const v: { new(x: C): C; readonly prototype: C;
         make(x: C | null, options?: { nested: Box<C> }): C | null; };
         const inside = v; } import Alias = N; const outside = N.v;",
        "v",
        &[
            (
                "inside",
                "{ new (x: C): C; readonly prototype: C; make(x: C | null, options?: { nested: Box<C>; }): C | null; }",
            ),
            (
                "outside",
                "{ new (x: Alias.C): Alias.C; readonly prototype: Alias.C; make(x: Alias.C | null, options?: { nested: Alias.Box<Alias.C>; }): Alias.C | null; }",
            ),
        ],
    );
}

#[test]
fn alias_selection_follows_inner_scope_and_declaration_order() {
    for aliases in
        ["import Earlier = N; import Later = N;", "import Later = N; import Earlier = N;"]
    {
        let winner = if aliases.starts_with("import Earlier") { "Earlier" } else { "Later" };
        views(
            &format!(
                "namespace N {{ export class C {{}} export declare const v: C | null; const inside = v; }}
                 {aliases} const outside = N.v; function scope() {{ import Inner = N; const nested = N.v; }}"
            ),
            "v",
            &[("inside", "C | null"), ("outside", &format!("{winner}.C | null")), ("nested", "Inner.C | null")],
        );
    }
}

#[test]
fn boolean_collapse_nullable_order_and_written_slots_remain_distinct() {
    views(
        "namespace N { export declare const v: { value: boolean | null | undefined;
         order: number | string; spelling: 'left' | 'right'; }; const inside = v; }
         const outside = N.v;",
        "v",
        &[
            (
                "inside",
                "{ value: boolean | null | undefined; order: number | string; spelling: 'left' | 'right'; }",
            ),
            (
                "outside",
                "{ value: boolean | null | undefined; order: number | string; spelling: 'left' | 'right'; }",
            ),
        ],
    );
    views(
        "namespace N { export declare const v: boolean | null | undefined; const inside = v; }
         const outside = N.v;",
        "v",
        &[("inside", "boolean | null | undefined"), ("outside", "boolean | null | undefined")],
    );
}

#[test]
fn erased_primitive_aliases_are_retained_only_by_annotated_slots() {
    views(
        "interface Array<T> { [index: number]: T; length: number; }
         namespace N { export type Text = string; export class C {}
         export declare function v(x: Text, y: Text | C, z: { text: Text }): Text[];
         const inside = v; } import Alias = N; const outside = N.v;",
        "v",
        &[
            ("inside", "(x: Text, y: Text | C, z: { text: Text; }) => Text[]"),
            (
                "outside",
                "(x: Alias.Text, y: Alias.Text | Alias.C, z: { text: Alias.Text; }) => Alias.Text[]",
            ),
        ],
    );
    views(
        "interface Array<T> { [index: number]: T; length: number; }
         namespace N { export type Text = string; export class C {}
         export declare const v: { new (x: Text | C): C; prototype: C;
         make(x: Text, y: { text: Text }): Text[]; };
         const inside = v; } import Alias = N; const outside = N.v;",
        "v",
        &[
            (
                "inside",
                "{ new (x: Text | C): C; prototype: C; make(x: Text, y: { text: Text; }): Text[]; }",
            ),
            (
                "outside",
                "{ new (x: Alias.Text | Alias.C): Alias.C; prototype: Alias.C; make(x: Alias.Text, y: { text: Alias.Text; }): Alias.Text[]; }",
            ),
        ],
    );
    // No written return slot certifies this inferred primitive. A renderer
    // which globally replays an alias from another annotated slot is wrong.
    views(
        "namespace N { export type Text = string;
         export function v(x: string) { return x; } const inside = v; }
         import Alias = N; const outside = N.v;",
        "v",
        &[("inside", "(x: string) => string"), ("outside", "(x: string) => string")],
    );
}
