//! Native 5b1047d shouldWriteTypeOfFunctionSymbol / visitedTypes. Dependent on
//! stable callable publication and canonical declaration-owned return metadata.

use tsr_ast::{Node, NodeId, NodeMap, push_children};
use tsr_checker::Checker;
use tsr_core::Arena;

fn sites(map: &NodeMap<'_>, root: NodeId, name: &str) -> Vec<NodeId> {
    let mut stack = vec![map.get(root).unwrap()];
    let mut sites = Vec::new();
    while let Some(node) = stack.pop() {
        if let Node::Identifier(node) = node
            && node.text == name
        {
            sites.push(node.node_id.unwrap());
        }
        push_children(node, &mut stack);
    }
    sites.reverse();
    sites
}

fn views(source: &str, name: &str, expected: &[&str]) {
    for reverse in [false, true] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, source);
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "/recursive.ts", text: source },
        );
        let symbol = bound.lookup_local(root, name).unwrap();
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let ty = checker.get_type_of_symbol(symbol);
        let data = checker.type_of(ty).data.clone();
        let sites = sites(&parsed.node_map, root, name);
        assert_eq!(sites.len(), expected.len());
        for repeat in 0..4 {
            let mut order: Vec<_> = (0..sites.len()).collect();
            if reverse ^ (repeat % 2 == 1) {
                order.reverse();
            }
            for index in order {
                assert_eq!(
                    checker.type_to_string_at(ty, sites[index]).as_deref(),
                    Some(expected[index]),
                    "owner={:?}, data={data:?}",
                    bound.symbols().get(symbol),
                );
                assert_eq!(checker.get_type_of_symbol(symbol), ty);
                assert_eq!(checker.type_of(ty).data, data);
            }
        }
    }
}

#[test]
fn top_level_recursive_arrow_and_expression_follow_native_enclosing_declaration() {
    views(
        "const arrow = () => arrow; const copy = arrow;",
        "arrow",
        &["() => typeof arrow", "() => typeof arrow", "() => typeof arrow"],
    );
    views(
        "const expression = function named() { return named; }; const copy = expression;",
        "expression",
        &["() => typeof named", "() => typeof expression"],
    );
}

#[test]
fn recursive_function_and_mutual_callables_keep_identity_without_extra_print_layers() {
    views(
        "function recur() { return recur; } const copy = recur;",
        "recur",
        &["() => typeof recur", "() => typeof recur", "() => typeof recur"],
    );
    views(
        "function left() { return right; } function right() { return left; } const copy = left;",
        "left",
        &["() => () => typeof left", "() => () => typeof left", "() => () => typeof left"],
    );
}

#[test]
fn local_callable_cycle_elides_instead_of_inventing_a_typeof_local_name() {
    views(
        "function outer() { function local() { return local; } return local; }
         const copy = outer;",
        "outer",
        &["() => () => any", "() => () => any"],
    );
    views(
        "const annotated: () => typeof annotated = () => annotated; const copy = annotated;",
        "annotated",
        &[
            "() => typeof annotated",
            "() => typeof annotated",
            "() => typeof annotated",
            "() => typeof annotated",
        ],
    );
}
