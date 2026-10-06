use super::{Checker, TypeData, TypeId};
use tsr_core::{Arena, CompilerOptions, Tristate};

fn leaves(checker: &Checker<'_, '_>, id: TypeId) -> Vec<TypeId> {
    match &checker.store.get(id).data {
        TypeData::Union { types, .. } => types.iter().flat_map(|&t| leaves(checker, t)).collect(),
        _ => vec![id],
    }
}

fn image(checker: &Checker<'_, '_>, id: TypeId) -> String {
    let ty = checker.store.get(id);
    let parts = match &ty.data {
        TypeData::Union { types, .. } => {
            types.iter().map(|&t| image(checker, t)).collect::<Vec<_>>().join(",")
        }
        _ => String::new(),
    };
    let origin = checker
        .union_origin
        .get(&id)
        .map(|entries| entries.iter().map(|&t| image(checker, t)).collect::<Vec<_>>().join(","))
        .unwrap_or_default();
    let alias = match &ty.data {
        TypeData::Union { symbol: Some(symbol), .. } => {
            format!("{:?}", checker.binder.symbols().get(*symbol).name)
        }
        _ => "null".to_owned(),
    };
    format!(
        "{{\"id\":{},\"flags\":{},\"text\":{:?},\"missing\":{},\"undefined\":{},\"void\":{},\"never\":{},\"any\":{},\"error\":{},\"unknown\":{},\"alias\":{},\"parts\":[{}],\"origin\":[{}]}}",
        id.index(),
        ty.flags.bits(),
        checker.type_to_string(id),
        id == checker.intrinsics.missing,
        id == checker.intrinsics.undefined,
        id == checker.intrinsics.void,
        id == checker.intrinsics.never,
        id == checker.intrinsics.any,
        id == checker.intrinsics.error,
        id == checker.intrinsics.unknown,
        alias,
        parts,
        origin
    )
}

#[test]
fn ordinary_undefined_dominates_missing_only_in_reduced_unions() {
    let source = "type V = { a: \"value\" }; type Alias = V | void; type WithU = V | undefined;";
    let mut failures = Vec::new();
    let raw = std::env::var_os("TSR_3TV_RAW").is_some();
    for (mode, strict, exact) in
        [("strict", true, false), ("exact", true, true), ("loose", false, false)]
    {
        for warm in [false, true] {
            for diagnostics_first in [false, true] {
                for reverse in [false, true] {
                    let arena = Arena::new();
                    let parsed = tsr_parser::parse(&arena, source);
                    assert!(parsed.diagnostics.is_empty());
                    let bound = tsr_binder::bind(
                        &arena,
                        parsed.source_file,
                        &parsed.nodes,
                        tsr_binder::FileInfo { name: "probe.ts", text: source },
                    );
                    let locals = bound.locals(parsed.source_file.node_id.unwrap()).unwrap();
                    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                    checker.apply_compiler_options(&CompilerOptions {
                        strict: Tristate::from_bool(strict),
                        exact_optional_property_types: Tristate::from_bool(exact),
                        ..Default::default()
                    });
                    if diagnostics_first {
                        checker.check_source_file(
                            parsed.source_file.node_id.unwrap(),
                            crate::check::FileContext { ambient: false, has_parse_errors: false },
                        );
                    }
                    if warm {
                        for name in ["V", "Alias", "WithU"] {
                            checker.get_declared_type_of_symbol(*locals.get(name).unwrap());
                        }
                    }
                    let value = checker.get_declared_type_of_symbol(*locals.get("V").unwrap());
                    let alias = checker.get_declared_type_of_symbol(*locals.get("Alias").unwrap());
                    let with_u = checker.get_declared_type_of_symbol(*locals.get("WithU").unwrap());
                    let undefined = checker.intrinsics.undefined;
                    let missing = checker.intrinsics.missing;
                    let void = checker.intrinsics.void;
                    let never = checker.intrinsics.never;
                    let any = checker.intrinsics.any;
                    let error = checker.intrinsics.error;
                    let unknown = checker.intrinsics.unknown;
                    let nested = checker.get_union_type_without_reduction(&[missing, value]);
                    let named_nested = checker.get_union_type(&[alias, missing]);
                    // Expected leaves are specified independently of the union factory.
                    // Loose nullable-empty and origin presentation discrepancies remain held.
                    let mut cases = vec![
                        ("pair", vec![undefined, missing], vec![undefined]),
                        ("sole-u", vec![undefined], vec![undefined]),
                        ("sole-m", vec![missing], vec![missing]),
                        ("empty", vec![], vec![never]),
                        ("m-never", vec![missing, never], vec![missing]),
                        ("u-never", vec![undefined, never], vec![undefined]),
                        ("m-v", vec![missing, value], vec![missing, value]),
                        ("m-void", vec![missing, void], vec![missing, void]),
                        ("pair-v", vec![undefined, missing, value], vec![undefined, value]),
                        ("pair-void", vec![undefined, missing, void], vec![undefined, void]),
                        (
                            "pair-v-void",
                            vec![undefined, missing, value, void],
                            vec![undefined, value, void],
                        ),
                        ("nested-u", vec![nested, undefined], vec![undefined, value]),
                        (
                            "alias-pair",
                            vec![alias, undefined, missing],
                            vec![undefined, value, void],
                        ),
                        ("alias-with-u-m", vec![with_u, missing], vec![undefined, value]),
                        (
                            "named-nested-u",
                            vec![named_nested, undefined],
                            vec![undefined, value, void],
                        ),
                        ("pair-any", vec![undefined, missing, any], vec![any]),
                        ("pair-error", vec![undefined, missing, error], vec![error]),
                        ("pair-unknown", vec![undefined, missing, unknown], vec![unknown]),
                        ("pair-any-error", vec![undefined, missing, any, error], vec![error]),
                    ];
                    if reverse {
                        cases.reverse();
                    }
                    for (case, inputs, reduced_expected) in cases {
                        for order in 0..2 {
                            let mut inputs = inputs.clone();
                            if order == 1 {
                                inputs.reverse();
                            }
                            for reduce in [true, false] {
                                let mut expected = match (case, reduce) {
                                    ("pair", false) => vec![undefined, missing],
                                    ("pair-v" | "nested-u" | "alias-with-u-m", false) => {
                                        vec![undefined, missing, value]
                                    }
                                    ("pair-void", false) => vec![undefined, missing, void],
                                    ("pair-v-void" | "alias-pair" | "named-nested-u", false) => {
                                        vec![undefined, missing, value, void]
                                    }
                                    ("pair-any", false) => vec![undefined, missing, any],
                                    ("pair-unknown", false) => vec![undefined, missing, unknown],
                                    _ => reduced_expected.clone(),
                                };
                                if !strict && inputs.len() > 1 {
                                    expected.retain(|&t| t != undefined && t != missing);
                                    // getUnionTypeWorker's empty-set arm
                                    // (checker.go:25692) answers the plain
                                    // undefined for these non-widening
                                    // inputs; without reduction the empty
                                    // sorted list is never.
                                    if expected.is_empty() {
                                        expected = vec![if reduce { undefined } else { never }];
                                    }
                                    // Existing literal never filtering returns the singleton unchanged.
                                    if reduce && matches!(case, "m-never" | "u-never") {
                                        expected = reduced_expected.clone();
                                    }
                                }
                                if inputs.is_empty() {
                                    expected = vec![never];
                                }
                                expected.sort_by_key(|t| t.index());
                                expected.dedup();
                                let mut previous = None;
                                for step in 0..2 {
                                    let actual = if reduce {
                                        checker.get_union_type(&inputs)
                                    } else {
                                        checker.get_union_type_without_reduction(&inputs)
                                    };
                                    if let Some(previous) = previous {
                                        assert_eq!(actual, previous, "repeat identity: {case}");
                                    }
                                    previous = Some(actual);
                                    let mut got = leaves(&checker, actual);
                                    got.sort_by_key(|t| t.index());
                                    got.dedup();
                                    // Legacy named origins can retain a one-leaf union identity.
                                    // Pin scalar identity on the canonical pair and singleton controls;
                                    // named origin identities/presentation are audited separately.
                                    let scalar_control = matches!(
                                        case,
                                        "pair"
                                            | "sole-u"
                                            | "sole-m"
                                            | "empty"
                                            | "m-never"
                                            | "u-never"
                                            | "pair-any"
                                            | "pair-error"
                                            | "pair-unknown"
                                            | "pair-any-error"
                                    );
                                    if got != expected
                                        || (scalar_control
                                            && expected.len() == 1
                                            && actual != expected[0])
                                    {
                                        failures.push(format!("{case} {mode} warm={warm} diagnostics_first={diagnostics_first} reverse={reverse} order={order} reduce={reduce} step={step}: got={got:?} expected={expected:?}"));
                                    }
                                    if raw {
                                        let input_images = inputs
                                            .iter()
                                            .map(|&t| image(&checker, t))
                                            .collect::<Vec<_>>()
                                            .join(",");
                                        println!(
                                            "RUST_3TV\t{{\"mode\":{mode:?},\"warm\":{warm},\"diagnosticsFirst\":{diagnostics_first},\"reverse\":{reverse},\"case\":{case:?},\"order\":{order},\"reduce\":{reduce},\"step\":{step},\"inputs\":[{input_images}],\"result\":{}}}",
                                            image(&checker, actual)
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} mismatches:\n{}", failures.len(), failures.join("\n"));
}
