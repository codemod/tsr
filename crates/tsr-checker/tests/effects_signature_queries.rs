//! Declaration returns copied from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
//! `fixtures/effects-signature-queries.native.d.ts` is the native strict-mode
//! declaration output for the exact input. Exercise cold and reversed queries
//! in independent private checkers before and after the ordinary full-file pass.

use tsr_ast::Node;
use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;

const SOURCE: &str = include_str!("fixtures/effects-signature-queries.ts");

#[test]
fn effects_preserve_native_returns_across_query_order_and_checker_owners() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, SOURCE);
    assert!(parsed.diagnostics.is_empty());
    let root = Node::SourceFile(parsed.source_file).node_id().unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "effects-signature-queries.ts", text: SOURCE },
    );
    let names = [
        "ordinaryRepeat",
        "assertion",
        "predicateBranch",
        "annotatedNever",
        "inferredNever",
        "dottedAssertion",
        "dottedOrdinary",
        "optionalOrdinary",
        "genericOrdinary",
        "genericAssertion",
        "loop",
        "contextual",
        "latePredicate",
        "recursive",
    ];
    for reverse in [false, true] {
        for precheck in [false, true] {
            let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
            checker.set_strict_null_checks(true);
            checker.set_no_implicit_any(true);
            if precheck {
                checker.check_source_file(
                    root,
                    FileContext { ambient: false, has_parse_errors: false },
                );
            }
            let mut order = names;
            if reverse {
                order.reverse();
            }
            for _ in 0..2 {
                for name in order {
                    let symbol = bound.lookup_local(root, name).unwrap();
                    checker.get_type_of_symbol(symbol);
                    let signature = checker.get_signatures_of_symbol(symbol).unwrap().remove(0);
                    let returned = signature.r#type;
                    match name {
                        "ordinaryRepeat" | "assertion" | "predicateBranch" => {
                            let expected = if name == "ordinaryRepeat" {
                                checker.intrinsics().unknown
                            } else {
                                checker.intrinsics().string
                            };
                            for member in ["left", "right"] {
                                assert_eq!(
                                    checker.get_type_of_property_of_type(returned, member),
                                    Some(expected),
                                    "{name}.{member}, reverse={reverse}, precheck={precheck}"
                                );
                            }
                        }
                        "dottedAssertion" | "genericAssertion" | "latePredicate" => {
                            assert_eq!(returned, checker.intrinsics().string, "{name}");
                        }
                        "recursive" => assert_eq!(returned, checker.intrinsics().boolean, "{name}"),
                        "annotatedNever" | "inferredNever" => assert_eq!(
                            checker.type_to_string(returned),
                            "string | number",
                            "{name}"
                        ),
                        _ => assert_eq!(returned, checker.intrinsics().unknown, "{name}"),
                    }
                }
                order.reverse();
            }
            checker
                .check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
            assert!(
                checker.diagnostics().is_empty(),
                "reverse={reverse}, precheck={precheck}: {:?}",
                checker.diagnostics()
            );
        }
    }
}
