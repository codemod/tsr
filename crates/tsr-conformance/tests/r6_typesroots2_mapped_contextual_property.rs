//! getTypeOfPropertyOfContextualType (`checker.go`) reads a mapped instance's
//! property through `getTypeOfSymbol` of the MAPPED symbol, the template
//! instantiated for that key, so `Readonly<FC<V>>`'s `validate` is
//! `(props: V) => void` for the call's `V`, which the inference mapper then
//! fixes. `docs/parity/notes/r6-typesroots2.md` §2.
//!
//! Expectations read from the pinned tsgo (assign the parameter to `never`:
//! `Type '{ foo: string; }' is not assignable to type 'never'`).
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r6_typesroots2_mapped_contextual", "probe.ts", source);
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
fn a_homomorphic_mapped_member_is_read_through_the_instance() {
    let lines = lines(
        r#"// @target: es2015
// @strict: true
interface FC<V> { initialValues: V; validate?: (props: V) => void; }
declare function F4<V = object>(x: Readonly<FC<V>>): void;
F4({ initialValues: { foo: "" }, validate: p4 => { p4; } });
"#,
    );
    assert!(lines.iter().any(|line| line == "p4 : { foo: string; }"), "{lines:?}");
}
