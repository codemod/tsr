//! getThisType container boundaries from pinned tsgo checker.go:22908.
use tsr_ast::{Node, NodeId, TypeNode};
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn this_type_requires_a_nonstatic_class_or_interface_member_container() {
    for (source, allowed) in [
        ("class C { method(this: this) {} }", true),
        ("class C { static method(this: this) {} }", false),
        ("class C { constructor(value: this) {} }", false),
        ("class C { constructor() { let value: this; } }", true),
        ("class C { method() { function inner() { let value: this; } } }", false),
        ("class C { method() { const arrow = () => { let value: this; }; } }", true),
        ("class C { static { let value: this; } }", false),
        ("class C { [null as this]() {} }", false),
        ("interface I { method(): this; }", true),
        ("class C { property?: this; }", true),
    ] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        assert!(parsed.diagnostics.is_empty(), "fixture must parse: {source}");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let node = (0..u32::try_from(parsed.nodes.len()).unwrap())
            .map(NodeId::new)
            .find_map(|id| match parsed.node_map.get(id) {
                Some(Node::ThisTypeNode(node)) => Some(node),
                _ => None,
            })
            .expect("fixture has a this type");
        let resolved = checker.get_type_from_type_node(TypeNode::ThisTypeNode(node));
        assert_eq!(resolved != checker.intrinsics().error, allowed, "{source}");
        if allowed {
            assert_eq!(checker.type_to_string(resolved), "this");
        }
    }
}
