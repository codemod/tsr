//! A mapped instance that is still generic prints from its parts
//! (createTypeNodeFromObjectType's isGenericMappedType arm,
//! nodebuilderimpl.go:2690), not from the members of its key domain's
//! constraint. Expectations are the pinned tsgo's (5b1047d), probed with
//! `scripts/offline-cargo/build-tsgo.sh` (`docs/parity/notes/r6-mapped.md` §1).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn type_lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/generic-mapped-instance", "instance.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

#[test]
fn a_generic_homomorphic_instance_keeps_its_mapped_form() {
    // `T := U` with `U extends readonly unknown[]`: the instance's key
    // domain `keyof U` is generic, so it prints as a mapped type. The base
    // listed `ReadonlyArray`'s members (`concat: { value: U["concat"]; }`…).
    let source = r#"// @strict: true
// @target: es2020
declare function box<T extends readonly unknown[]>(value: T): { -readonly [K in keyof T]: { value: T[K] } };
function f<U extends readonly unknown[]>(u: U) {
    const boxed = box(u);
    return boxed;
}
"#;
    let lines = type_lines(source);
    let wanted = "boxed : { -readonly [K in keyof U]: { value: U[K]; }; }";
    assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
}

#[test]
fn a_mapped_node_under_alias_bindings_maps_a_tuple_image_elementwise() {
    // `mappedArrayTupleIntersections`: the true branch's mapped node is
    // evaluated with `T := [3, 4, 5]`; instantiateMappedType sends a tuple
    // image to instantiateMappedTupleType (`docs/parity/notes/r6-mapped.md`
    // §2). The base printed the tuple's members (`0: 1; …; concat: 1; …`).
    let source = r#"// @strict: true
// @target: es2015
type MustBeArray<T extends any[]> = T;
type Hmm<T extends any[]> = T extends number[] ? MustBeArray<{ [I in keyof T]: 1 }> : never;
type Plain<T extends any[]> = T extends number[] ? { [I in keyof T]: 1 } : never;
type X = Hmm<[3, 4, 5]>;
declare let y: Plain<[3, 4, 5]>;
"#;
    let lines = type_lines(source);
    for wanted in ["X : [1, 1, 1]", "y : [1, 1, 1]"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
