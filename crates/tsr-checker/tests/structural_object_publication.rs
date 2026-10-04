//! Native createAnonymousTypeNodeEx: only completed original object-literal
//! images are eligible, and active anonymous-object identity elides to any.

use tsr_ast::{Node, push_children};
use tsr_checker::Checker;
use tsr_core::Arena;

#[test]
fn completed_property_only_image_names_slots_at_each_cold_and_warm_viewer() {
    let source = "namespace N { export class C {} export const value = {
        item: new C(), callback: () => new C() }; const inside = value; }
        import Alias = N; const outside = N.value;";
    for reverse in [false, true] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/object.ts", text: source },
        );
        let namespace = *bound.globals().get("N").unwrap();
        let symbol = *bound.symbols().get(namespace).exports.get("value").unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let ty = checker.get_type_of_symbol(symbol);
        let data = checker.type_of(ty).data.clone();
        let mut nodes = vec![parsed.node_map.get(root).unwrap()];
        let mut sites = Vec::new();
        while let Some(node) = nodes.pop() {
            if let Node::Identifier(node) = node {
                let expected = match node.text {
                    "inside" => Some("{ item: C; callback: () => C; }"),
                    "outside" => Some("{ item: Alias.C; callback: () => Alias.C; }"),
                    _ => None,
                };
                if let Some(expected) = expected {
                    sites.push((node.node_id.unwrap(), expected));
                }
            }
            push_children(node, &mut nodes);
        }
        assert_eq!(sites.len(), 2);
        if reverse {
            sites.reverse();
        }
        for _ in 0..4 {
            for &(site, expected) in &sites {
                assert_eq!(checker.type_to_string_at(ty, site).as_deref(), Some(expected));
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(checker.type_of(ty).data, data);
            }
            sites.reverse();
        }
    }
}

#[test]
fn published_captured_object_and_fresh_expression_keep_distinct_recursive_images() {
    let source = "let x = { value: 17, recur: () => x }; const copied = x;";
    for reverse in [false, true] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/capture.ts", text: source },
        );
        let symbol = bound.lookup_local(root, "x").unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut nodes = vec![parsed.node_map.get(root).unwrap()];
        let mut sites = Vec::new();
        let mut literal = None;
        while let Some(node) = nodes.pop() {
            match node {
                Node::Identifier(node) if node.text == "x" => sites.push(node.node_id.unwrap()),
                Node::ObjectLiteralExpression(node) => literal = Some(node),
                _ => {}
            }
            push_children(node, &mut nodes);
        }
        let ty = checker.get_type_of_symbol(symbol);
        let data = checker.type_of(ty).data.clone();
        let literal = literal.unwrap();
        let fresh = checker.check_expression(tsr_ast::Expression::ObjectLiteralExpression(literal));
        assert_ne!(fresh, ty, "native regularization retains fresh expression identity");
        let fresh_data = checker.type_of(fresh).data.clone();
        assert_eq!(sites.len(), 3);
        if reverse {
            sites.reverse();
        }
        for _ in 0..4 {
            for &site in &sites {
                assert_eq!(
                    checker.type_to_string_at(ty, site).as_deref(),
                    Some("{ value: number; recur: () => any; }")
                );
                assert_eq!(
                    checker.type_to_string_at(fresh, literal.node_id.unwrap()).as_deref(),
                    Some("{ value: number; recur: () => { value: number; recur: () => any; }; }")
                );
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(checker.type_of(ty).data, data);
                assert_eq!(checker.type_of(fresh).data, fresh_data);
            }
            sites.reverse();
        }
        assert!(checker.diagnostics().is_empty());
    }
}

#[test]
fn mixed_member_keys_accessor_pairs_and_surviving_duplicates_keep_query_order_identity() {
    let source = "const quoted = { label: 'kept', callback: () => 7 } as const;
        const mixed = { before: 11, get value() { return 19; },
            set value(input: number) {}, method() { return 'method'; },
            callback: () => 29, after: true };
        const setterFirst = { set value(input: number) {}, marker: 'middle',
            get value() { return 31; }, callback: function () { return false; } };
        const duplicates = { ['same']: 41, ['same']: 'last' };
        const numberLast = { item: 'first', item: 19,
            callback: () => false, callback: () => 31 };
        const booleanLast = { item: 17, item: 'last',
            callback: () => 23, callback: () => true };
        function mixedCaptured() { let x = {
            get value() { return 47; }, method() {}, recur: () => x }; return x; }";
    let mut expected = vec![
        ("quoted", "{ readonly label: 'kept'; readonly callback: () => number; }"),
        (
            "mixed",
            "{ before: number; value: number; method(): string; callback: () => number; after: boolean; }",
        ),
        ("setterFirst", "{ value: number; marker: string; callback: () => boolean; }"),
        ("duplicates", "{ same: string; }"),
        ("numberLast", "{ item: number; callback: () => number; }"),
        ("booleanLast", "{ item: string; callback: () => boolean; }"),
        ("mixedCaptured", "() => { readonly value: number; method(): void; recur: () => any; }"),
    ];
    for reverse in [false, true] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/mixed.ts", text: source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let mut nodes = vec![parsed.node_map.get(root).unwrap()];
        let mut sites = std::collections::HashMap::new();
        while let Some(node) = nodes.pop() {
            if let Node::Identifier(node) = node
                && expected.iter().any(|(name, _)| *name == node.text)
            {
                sites.insert(node.text, node.node_id.unwrap());
            }
            push_children(node, &mut nodes);
        }
        if reverse {
            expected.reverse();
        }
        let mut snapshots = Vec::new();
        for &(name, expected) in &expected {
            let symbol = bound.lookup_local(root, name).unwrap();
            let ty = checker.get_type_of_symbol(symbol);
            let data = checker.type_of(ty).data.clone();
            assert_eq!(checker.type_to_string_at(ty, sites[name]).as_deref(), Some(expected));
            assert_eq!(checker.type_of(ty).data, data);
            snapshots.push((symbol, ty, data, sites[name], expected));
        }
        for _ in 0..4 {
            snapshots.reverse();
            for &(symbol, ty, ref data, site, expected) in &snapshots {
                assert_eq!(checker.type_to_string_at(ty, site).as_deref(), Some(expected));
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(&checker.type_of(ty).data, data);
            }
        }
    }
}
