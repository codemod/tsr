//! A TSR refusal-state control: an alias frame cannot finish an original
//! pending signature. Its `None` is not native's completed unknown effects slot.

use crate::{Checker, signatures::LazyReturnState};
use tsr_ast::{Node, Statement};

#[test]
fn effects_refusal_in_foreign_alias_frame_is_not_a_completed_negative() {
    let source = "type Frame<T> = T; function inferred(value: unknown) { return typeof value === 'string'; } declare let value: unknown; inferred(value);";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "effects-refusal.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let symbol = bound.lookup_local(root, "inferred").unwrap();
    checker.defer_typeof_function_return(symbol);
    let ty = checker.get_type_of_symbol(symbol);
    let signature = checker.signature_types[&ty][0].clone();
    let key = checker.type_literal_key(signature.declaration);
    assert!(checker.pending_signature_returns[&key] == LazyReturnState::Pending);
    let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[0] else {
        panic!("alias");
    };
    let parameter = bound.symbol_of(alias.type_parameters[0].node_id.unwrap()).unwrap();
    let call = (0..checker.nodes.len())
        .find_map(|index| {
            let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
            match checker.node_map.get(id) {
                Some(Node::CallExpression(call)) => Some((id, call)),
                _ => None,
            }
        })
        .unwrap();
    let mut frame = rustc_hash::FxHashMap::default();
    frame.insert(parameter, checker.intrinsics.number);
    checker.alias_evaluation_bindings.push(frame);
    for _ in 0..2 {
        assert!(checker.get_effects_signature(call.0, call.1).is_none());
        assert!(checker.pending_signature_returns[&key] == LazyReturnState::Pending);
        assert!(!checker.signature_returns.contains_key(&key));
    }
    checker.alias_evaluation_bindings.pop();
    for _ in 0..2 {
        let completed = checker
            .get_effects_signature(call.0, call.1)
            .expect("inferred predicate after original-frame completion");
        assert_eq!(completed.predicate.unwrap().r#type, Some(checker.intrinsics.string));
        assert!(!checker.pending_signature_returns.contains_key(&key));
        assert_eq!(checker.get_type_of_symbol(symbol), ty);
    }
    assert!(checker.diagnostics().is_empty());
}

#[test]
fn completed_unknown_effects_are_private_and_exclude_predicates_and_never() {
    let source = "declare function ordinary(): void; declare function predicate(value: unknown): value is string; declare function stop(): never; function inferred(value: unknown) { return typeof value === 'string'; } declare let value: unknown; ordinary(); predicate(value); stop(); inferred(value);";
    let arena = tsr_core::Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty());
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "effects-completed.ts", text: source },
    );
    let calls: Vec<_> = (0..parsed.nodes.len())
        .filter_map(|index| {
            let id = tsr_ast::NodeId::new(u32::try_from(index).unwrap());
            let Some(Node::CallExpression(call)) = parsed.node_map.get(id) else { return None };
            let Some(tsr_ast::Expression::Identifier(callee)) = call.expression else {
                return None;
            };
            Some((id, call, callee.text))
        })
        .collect();
    for reverse in [false, true] {
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        assert!(checker.completed_no_effects_calls.is_empty());
        let mut calls = calls.clone();
        if reverse {
            calls.reverse();
        }
        let (ordinary, call, _) = calls.iter().find(|(_, _, name)| *name == "ordinary").unwrap();
        // A captured frame is not this call's original completion context.
        checker.alias_evaluation_bindings.push(rustc_hash::FxHashMap::default());
        assert!(checker.get_effects_signature(*ordinary, call).is_none());
        assert!(checker.completed_no_effects_calls.is_empty());
        checker.alias_evaluation_bindings.pop();
        for _ in 0..2 {
            for (id, call, name) in &calls {
                let effects = checker.get_effects_signature(*id, call);
                assert_eq!(effects.is_none(), *name == "ordinary", "{name}");
                assert_eq!(
                    checker.completed_no_effects_calls.contains(id),
                    *name == "ordinary",
                    "{name}"
                );
            }
        }
        assert_eq!(checker.completed_no_effects_calls.len(), 1);
        assert!(checker.diagnostics().is_empty());
    }
}
