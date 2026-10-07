//! Behavioral masked projections of native getTypeFactsWorker domains.
use crate::{Checker, flow::TypeFacts};
use tsr_ast::Statement;

#[test]
fn masked_truthiness_preserves_empty_object_scalar_union_and_callable_boundaries() {
    for strict in [false, true] {
        for (body, truthy, falsy) in [
            ("{}", true, true),
            ("{ p: string }", true, !strict),
            ("{ (x: string): string; p: string }", true, !strict),
            ("`prefix${string}`", true, !strict),
            ("0n", false, true),
            ("1n", true, !strict),
            ("0n | 1n", true, true),
            ("symbol", true, !strict),
        ] {
            let source = format!("type Current = {body};");
            let arena = tsr_core::Arena::new();
            let parsed = tsr_parser::parse(&arena, &source);
            let bound = tsr_binder::bind(
                &arena,
                parsed.source_file,
                &parsed.nodes,
                tsr_binder::FileInfo { name: "masked.ts", text: &source },
            );
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            checker.set_strict_null_checks(strict);
            let Statement::TypeAliasDeclaration(alias) = parsed.source_file.statements[0] else {
                panic!("type alias required");
            };
            let ty = checker.get_type_from_type_node(alias.r#type.unwrap());
            let absent = checker.get_type_facts_with_mask(ty, TypeFacts::IS_UNDEFINED);
            assert!(!absent.contains(TypeFacts::IS_UNDEFINED), "{body}");
            let facts = checker.get_type_facts_with_mask(ty, TypeFacts::TRUTHY | TypeFacts::FALSY);
            assert_eq!(facts.contains(TypeFacts::TRUTHY), truthy, "{body}, strict={strict}");
            assert_eq!(facts.contains(TypeFacts::FALSY), falsy, "{body}, strict={strict}");
            // The same callable must still have function typeof facts when
            // the requested mask distinguishes object/function categories.
            if body.starts_with("{ (") {
                assert!(
                    checker
                        .get_type_facts_with_mask(ty, TypeFacts::TYPEOF_EQ_FUNCTION)
                        .contains(TypeFacts::TYPEOF_EQ_FUNCTION)
                );
            }
        }
    }
}
