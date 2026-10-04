//! Native checker 5b1047d: `StripOptional` requires a strict, optional source.
//! Expectations distinguish intrinsic missing from genuine undefined/void/null.

use tsr_checker::{
    Checker,
    types::{TypeData, TypeId},
};
use tsr_core::{Arena, CompilerOptions, Tristate};

#[derive(Clone, Copy)]
enum Leaf {
    Value,
    Undefined,
    Null,
    Void,
    Never,
    Any,
}

const V: &[Leaf] = &[Leaf::Value];
const VU: &[Leaf] = &[Leaf::Value, Leaf::Undefined];
const VN: &[Leaf] = &[Leaf::Value, Leaf::Null];
const VV: &[Leaf] = &[Leaf::Value, Leaf::Void];
const U: &[Leaf] = &[Leaf::Undefined];
const NULL: &[Leaf] = &[Leaf::Null];
const VOID: &[Leaf] = &[Leaf::Void];
const NEVER: &[Leaf] = &[Leaf::Never];
const ANY: &[Leaf] = &[Leaf::Any];

fn leaves(checker: &Checker<'_, '_>, id: TypeId) -> Vec<TypeId> {
    if let TypeData::Union { types, .. } = &checker.type_of(id).data {
        types.clone()
    } else {
        vec![id]
    }
}

#[test]
fn identity_removal_uses_source_optionality_and_not_non_nullability() {
    // Native raw ordinary-property expectations, ordered strict/exact/loose.
    // Loose null-only preserves the current Rust ordinary-null identity; native
    // uses its distinct loose null identity. That producer discrepancy is held.
    let cases: &[(&str, &str, [&[Leaf]; 3])] = &[
        ("type Source = { x: V | undefined };", "Need<Source>", [VU, VU, V]),
        ("type Source = { x?: V | undefined };", "Need<Source>", [V, VU, V]),
        ("type Source = { x: undefined };", "Need<Source>", [U, U, U]),
        ("type Source = { x?: undefined };", "Need<Source>", [NEVER, U, U]),
        ("type Source = { x: void };", "Need<Source>", [VOID, VOID, VOID]),
        ("type Source = { x?: void };", "Need<Source>", [NEVER, VOID, VOID]),
        ("type Source = { x: V | void };", "Need<Source>", [VV, VV, VV]),
        ("type Source = { x?: V | void };", "Need<Source>", [V, VV, VV]),
        ("type Source = { x: V | null };", "Need<Source>", [VN, VN, V]),
        ("type Source = { x?: V | null };", "Need<Source>", [VN, VN, V]),
        ("type Source = { x: null };", "Need<Source>", [NULL, NULL, NULL]),
        ("type Source = { x?: null };", "Need<Source>", [NULL, NULL, NULL]),
        ("type Source = { x: never };", "Need<Source>", [NEVER, NEVER, NEVER]),
        ("type Source = { x?: never };", "Need<Source>", [NEVER, NEVER, NEVER]),
        ("type Source = { x: any };", "Need<Source>", [ANY, ANY, ANY]),
        ("type Source = { x?: any };", "Need<Source>", [ANY, ANY, ANY]),
        ("type Source = { x: V | undefined };", "Need<Part<Source>>", [V, VU, V]),
        ("type Source = { x: V | undefined };", "Need<Read<Source>>", [VU, VU, V]),
        ("type Source = { x?: V | undefined };", "Need<Read<Source>>", [V, VU, V]),
        ("type Source = { x: V | undefined };", "Need<Read<Part<Source>>>", [V, VU, V]),
        ("type Source = { x?: V | undefined };", "Need<Read<Part<Source>>>", [V, VU, V]),
        ("type Source = { x: V | undefined };", "Need<Read<Need<Source>>>", [VU, VU, V]),
        ("type Source = { x?: V | undefined };", "Need<Read<Need<Source>>>", [V, VU, V]),
        (
            "interface Base { x?: V | undefined } interface Source extends Base {}",
            "Need<Source>",
            [V, VU, V],
        ),
        (
            "interface Base { x: V | undefined } interface Source extends Base {}",
            "Need<Source>",
            [VU, VU, V],
        ),
        ("type Box<T> = { x?: T }; type Source = Box<V | undefined>;", "Need<Source>", [V, VU, V]),
        ("type Source = { [Q in \"x\"]?: V | undefined };", "Need<Source>", [V, VU, V]),
        (
            "type Source = { x?: V | undefined };",
            "{ [Q in keyof Source]-?: Source[Q] }",
            [V, VU, V],
        ),
        (
            "type Source = { x: V | undefined };",
            "{ [Q in keyof Source]-?: Source[Q] }",
            [VU, VU, V],
        ),
    ];
    let mut failures = Vec::new();
    for &(declarations, target, expected) in cases {
        let source = format!(
            "type V = {{ a: \"value\" }}; type Part<T> = {{ [P in keyof T]?: T[P] }}; \
             type Read<T> = {{ readonly [P in keyof T]: T[P] }}; \
             type Need<T> = {{ [P in keyof T]-?: T[P] }}; {declarations} type Result = {target};"
        );
        for (mode, strict, exact) in [(0, true, false), (1, true, true), (2, false, false)] {
            for warm in [false, true] {
                for source_first in [false, true] {
                    let arena = Arena::new();
                    let parsed = tsr_parser::parse(&arena, &source);
                    assert!(parsed.diagnostics.is_empty());
                    let bound = tsr_binder::bind(
                        &arena,
                        parsed.source_file,
                        &parsed.nodes,
                        tsr_binder::FileInfo { name: "probe.ts", text: &source },
                    );
                    let locals = bound.locals(parsed.source_file.node_id.unwrap()).unwrap();
                    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
                    checker.apply_compiler_options(&CompilerOptions {
                        strict: Tristate::from_bool(strict),
                        exact_optional_property_types: Tristate::from_bool(exact),
                        ..Default::default()
                    });
                    if warm {
                        for name in ["V", "Part", "Read", "Need", "Source"] {
                            checker.get_declared_type_of_symbol(locals[name]);
                        }
                    }
                    if source_first {
                        let source_type = checker.get_declared_type_of_symbol(locals["Source"]);
                        let _ = checker.get_type_of_property_of_type(source_type, "x");
                    }
                    let result = checker.get_declared_type_of_symbol(locals["Result"]);
                    let value = checker.get_declared_type_of_symbol(locals["V"]);
                    let intrinsics = checker.intrinsics();
                    let mut want = expected[mode]
                        .iter()
                        .map(|leaf| match leaf {
                            Leaf::Value => value,
                            Leaf::Undefined => intrinsics.undefined,
                            Leaf::Null => intrinsics.null,
                            Leaf::Void => intrinsics.void,
                            Leaf::Never => intrinsics.never,
                            Leaf::Any => intrinsics.any,
                        })
                        .collect::<Vec<_>>();
                    want.sort_unstable();
                    for _ in 0..2 {
                        let actual = checker.get_type_of_property_of_type(result, "x").unwrap();
                        let mut got = leaves(&checker, actual);
                        got.sort_unstable();
                        if got != want {
                            failures.push(format!(
                                "{declarations} {target}; strict={strict} exact={exact} warm={warm} source_first={source_first}: got={got:?}, want={want:?}"
                            ));
                        }
                    }
                }
            }
        }
    }
    assert!(failures.is_empty(), "{} raw mismatches:\n{}", failures.len(), failures.join("\n"));
}
