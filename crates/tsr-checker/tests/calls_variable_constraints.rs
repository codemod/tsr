//! Native checkTypeArguments checks bounded type variables through assignability.
use tsr_ast::{Node, NodeId};
use tsr_checker::{Checker, calls::InstantiationExpressionSignature};

#[test]
fn bounded_type_variables_follow_constraint_relation() {
    for (bound, rejected) in [("number", true), ("string", false)] {
        let source = format!(
            "declare function stringOnly<T extends string>(value: T): T; function use<U extends {bound}>(value: U) {{ return stringOnly<U>(value); }}"
        );
        let arena = tsr_core::Arena::new();
        let parsed = tsr_parser::parse(&arena, &source);
        assert!(parsed.diagnostics.is_empty());
        let root = parsed.source_file.node_id.unwrap();
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "variables.ts", text: &source },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let signature = checker
            .get_signatures_of_symbol(bound.lookup_local(root, "stringOnly").unwrap())
            .unwrap()
            .remove(0);
        let call = (0..parsed.nodes.len())
            .map(|index| NodeId::new(u32::try_from(index).unwrap()))
            .find_map(|id| match parsed.node_map.get(id) {
                Some(Node::CallExpression(call)) => Some(call),
                _ => None,
            })
            .unwrap();
        let outcome = checker
            .get_instantiation_expression_signature(&signature, call.type_arguments)
            .unwrap();
        if rejected {
            assert!(matches!(outcome, InstantiationExpressionSignature::ConstraintRejected));
            let (_, diagnostic) = &checker.diagnostics()[0];
            assert_eq!(diagnostic.code(), "TS2344");
        } else {
            assert!(matches!(outcome, InstantiationExpressionSignature::Instantiated(_)));
            assert!(checker.diagnostics().is_empty());
        }
    }
}
