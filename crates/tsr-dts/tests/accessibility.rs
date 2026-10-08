//! The written-name walk (`tsr_dts::accessibility`) against a stub resolver:
//! every declaration visible, and exactly the identifier `hidden`
//! inaccessible. What is pinned is the walk — which written names upstream's
//! `DeclarationTransformer` visits and which diagnostic context names them —
//! not the checker's visibility answers, which the conformance suite judges.

use tsr_ast::{Node, NodeId, NodeMap, NodeTable};
use tsr_dts::accessibility::{
    AccessibilityResolver, EntityNameVisibility, WalkOptions, declaration_walk_diagnostics,
    written_name_diagnostics,
};

struct Stub<'n, 'a> {
    map: &'n NodeMap<'a>,
}

impl AccessibilityResolver for Stub<'_, '_> {
    fn precalculate_declaration_emit_visibility(&mut self, _file: NodeId) {}

    fn is_declaration_visible(&mut self, _node: NodeId) -> bool {
        true
    }

    fn is_entity_name_visible(
        &mut self,
        entity_name: NodeId,
        _enclosing: NodeId,
    ) -> EntityNameVisibility {
        let mut node = entity_name;
        loop {
            match self.map.get(node) {
                Some(Node::QualifiedName(name)) => {
                    node = name.left.and_then(|l| l.node_id()).unwrap()
                }
                Some(Node::PropertyAccessExpression(access)) => {
                    node = access.expression.and_then(|e| e.node_id()).unwrap();
                }
                Some(Node::Identifier(identifier)) if identifier.text == "hidden" => {
                    return EntityNameVisibility::NotAccessible {
                        symbol_name: "hidden".into(),
                        error_node: node,
                    };
                }
                _ => return EntityNameVisibility::Accessible(Vec::new()),
            }
        }
    }

    fn is_implementation_of_overload(&mut self, _node: NodeId) -> bool {
        false
    }
}

/// `(code, column)` of each diagnostic on a one-line source, 1-based.
fn diagnostics(source: &str) -> Vec<(u32, u32)> {
    let arena = tsr_core::Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let parsed = tsr_parser::parse_into(&arena, source, Default::default(), &mut nodes, &mut map);
    let file = parsed.source_file.node_id.unwrap();
    let mut stub = Stub { map: &map };
    let mut out: Vec<(u32, u32)> = written_name_diagnostics(file, &nodes, &map, source, &mut stub)
        .into_iter()
        .map(|d| (d.message.code(), d.span.start + 1))
        .collect();
    out.sort_unstable();
    out
}

#[test]
fn a_variable_annotation_reports_through_the_variable_context() {
    // TS4025 at the name, as `declarationEmitVarInElidedBlock` records.
    assert_eq!(diagnostics("export let b: typeof hidden;"), [(4025, 22)]);
    // A type literal under a variable stays in the variable's context.
    assert_eq!(diagnostics("export let b: { a: hidden };"), [(4025, 20)]);
}

#[test]
fn each_declaration_kind_selects_its_message() {
    assert_eq!(diagnostics("export function f(x: hidden): void {}"), [(4078, 22)]);
    assert_eq!(diagnostics("export function f(): hidden {}"), [(4060, 22)]);
    assert_eq!(diagnostics("export interface I extends hidden {}"), [(4022, 28)]);
    assert_eq!(diagnostics("export type T = hidden;"), [(4081, 17)]);
    // A type literal under a type alias opens new contexts for its members.
    assert_eq!(diagnostics("export type T = { a: hidden };"), [(4033, 22)]);
}

#[test]
fn what_the_declaration_file_drops_is_not_walked() {
    // Private member types, initializers and bodies never reach the `.d.ts`.
    assert!(
        diagnostics(
            "export class C { private p: hidden; q = null as unknown as hidden; m() { let z: hidden; } }"
        )
        .is_empty()
    );
    // An unannotated declaration's type is inferred by the node builder,
    // which this walk declines; its initializer is never visited.
    assert!(diagnostics("export const x = null as unknown as hidden;").is_empty());
}

/// A resolver for the transform's own arms: every declaration visible except
/// import bindings, and every import required by an augmentation.
struct Tracking<'n, 'a> {
    map: &'n NodeMap<'a>,
}

impl AccessibilityResolver for Tracking<'_, '_> {
    fn precalculate_declaration_emit_visibility(&mut self, _file: NodeId) {}

    fn is_declaration_visible(&mut self, node: NodeId) -> bool {
        !matches!(self.map.get(node), Some(Node::ImportSpecifier(_) | Node::ImportClause(_)))
    }

    fn is_entity_name_visible(
        &mut self,
        _entity_name: NodeId,
        _enclosing: NodeId,
    ) -> EntityNameVisibility {
        EntityNameVisibility::Accessible(Vec::new())
    }

    fn is_implementation_of_overload(&mut self, _node: NodeId) -> bool {
        false
    }

    fn is_import_required_by_augmentation(&mut self, _import: NodeId) -> bool {
        true
    }
}

/// `(code, column)` of each diagnostic the transform's own arms produce.
fn tracked(source: &str, isolated_declarations: bool) -> Vec<(u32, u32)> {
    let arena = tsr_core::Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let parsed = tsr_parser::parse_into(&arena, source, Default::default(), &mut nodes, &mut map);
    let file = parsed.source_file.node_id.unwrap();
    let mut resolver = Tracking { map: &map };
    let options = WalkOptions { isolated_declarations };
    let mut out: Vec<(u32, u32)> =
        declaration_walk_diagnostics(file, &nodes, &map, source, options, &mut resolver)
            .into_iter()
            .map(|d| (d.message.code(), d.span.start + 1))
            .collect();
    out.sort_unstable();
    out
}

#[test]
fn an_import_kept_for_an_augmentation_reports_only_under_isolated_declarations() {
    assert_eq!(tracked("import { a } from \"./m\";", true), [(9026, 1)]);
    assert!(tracked("import { a } from \"./m\";", false).is_empty());
    // Default-only and namespace imports return before the augmentation arm.
    assert!(tracked("import d from \"./m\";", true).is_empty());
    assert!(tracked("import * as ns from \"./m\";", true).is_empty());
    assert!(tracked("import \"./m\";", true).is_empty());
}
