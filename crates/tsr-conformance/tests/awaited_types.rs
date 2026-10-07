//! Generic await controls checked against pinned tsgo 5b1047d declarations.
use tsr_conformance::{TestCase, diagnostics_suite, types_baseline::FileTypes, types_producer};

#[test]
fn generic_await_wraps_only_potentially_promise_like_constraints() {
    let source = r"// @strict: true
// @target: esnext
export async function free<T>(x: T) { const freeValue = await x; return { freeValue }; }
export async function primitive<T extends string | number>(x: T) { const primitiveValue = await x; return { primitiveValue }; }
export async function objectBound<T extends { tag: string }>(x: T) { const objectValue = await x; return { objectValue }; }
export async function emptyBound<T extends {}>(x: T) { const emptyValue = await x; return { emptyValue }; }
export async function promiseBound<T extends Promise<string>>(x: T) { const promiseValue = await x; return { promiseValue }; }
export async function mixedBound<T extends string | Promise<number>>(x: T) { const mixedValue = await x; return { mixedValue }; }
export async function concreteBox<T>(x: { tag: T }) { const boxValue = await x; return { boxValue }; }
export async function plainReturn<T>(x: T) { return x; }
export async function awaitReturn<T>(x: T) { return await x; }
export async function alreadyAwaited<T>(x: Awaited<T>) { const alreadyValue = await x; return { alreadyValue }; }
type Select<T> = T extends number ? Promise<1> : Promise<2>;
export async function conditional<T>(x: Select<T>) { const conditionalValue = await x; return { conditionalValue }; }
export async function nullable<T>(x: T | undefined) { const nullableValue = await x; return { nullableValue }; }
export async function conditionalReturn<T>(x: Select<T>) { const selected = await x; return selected; }
export async function unionReturn<T>(x: T | undefined) { const selectedUnion = await x; return selectedUnion; }
export function capture<T>(x: T) { return async function inner<U>(y: U) { return x; }; }
export async function higherOrder<T, U>(x: Awaited<T> | U) { const higherValue = await x; return { higherValue }; }
export async function brandedNumber<T extends number & { then(): void }>(x: T) { const brandedNumberValue = await x; return { brandedNumberValue }; }
export async function brandedString<T extends string & { then(onfulfilled: (value: number) => void): void }>(x: T) { const brandedStringValue = await x; return { brandedStringValue }; }
export async function objectThen<T extends { then(): void }>(x: T) { const objectThenValue = await x; return { objectThenValue }; }
export async function mixedThen<T extends (number | { tag: string }) & { then(): void }>(x: T) { const mixedThenValue = await x; return { mixedThenValue }; }
";
    let case = TestCase::parse("probe/generic-awaited", "generic-awaited.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    // A generic argument on a concrete object does not make it a generic
    // object, and nonpromise primitive/object bounds retain the parameter.
    // A previously written Awaited<T> must not become Awaited<Awaited<T>>.
    for wanted in [
        "freeValue : Awaited<T>",
        "primitiveValue : T",
        "objectValue : T",
        "emptyValue : Awaited<T>",
        "promiseValue : Awaited<T>",
        "mixedValue : Awaited<T>",
        "boxValue : { tag: T; }",
        "plainReturn : <T>(x: T) => Promise<T>",
        "awaitReturn : <T>(x: T) => Promise<T>",
        "alreadyValue : Awaited<T>",
        "conditionalValue : Awaited<Select<T>>",
        "nullableValue : Awaited<T> | undefined",
        "conditionalReturn : <T>(x: Select<T>) => Promise<Select<T>>",
        "unionReturn : <T>(x: T | undefined) => Promise<T | undefined>",
        "capture : <T>(x: T) => <U>(y: U) => Promise<T>",
        "higherValue : Awaited<T> | Awaited<U>",
        "brandedNumberValue : T",
        "brandedStringValue : T",
        "objectThenValue : Awaited<T>",
        "mixedThenValue : Awaited<T>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}

#[test]
fn awaited_operands_retain_constraints_before_unwrapping_intersection_thenables() {
    // Fresh native 5b1047d TestLocal/awaitedOperandsNativeControls controls.
    // Different literal values distinguish generic retention from eager bound
    // unwrapping, intersected callbacks from callback overloads, and this-filtered
    // candidates from the first/all overloads. Repeated queries follow cycles to
    // expose a recursion stack accidentally retained across separate operands.
    let source = r#"// @strict: true
// @target: esnext
interface Tagged { tag: "left" }
interface Then<T> { then(onfulfilled: (value: T) => void): void }
interface OwnThis { tag: "left"; then(this: OwnThis, onfulfilled: (value: "receiver") => void): void }
interface WrongThis { then(this: { missing: "right" }, onfulfilled: (value: 91) => void): void }
interface Overloads { then(this: { missing: "right" }, onfulfilled: (value: 91) => void): void; then(this: Tagged, onfulfilled: (value: "selected") => void): void }
interface CycleA { then(onfulfilled: (value: CycleB) => void): void }
interface CycleB { then(onfulfilled: (value: CycleA) => void): void }
interface SelfCycle { then(onfulfilled: (value: SelfCycle) => void): void }
async function operands<T, P extends string, O extends Tagged, F extends Then<17>, K extends keyof T, I extends { value: Promise<31> }, Q extends { value: 67 }>(
  free: T, primitive: P, object: O, constrainedThen: F, key: K, indexed: I["value"], directKey: keyof T, primitiveIndexed: Q["value"],
  both: Then<23> & Tagged, plain: Tagged & { other: 42 }, branded: number & Then<29>,
  nested: Then<Promise<37>> & Tagged, own: OwnThis & { other: 42 }, wrong: WrongThis & Tagged,
  overloads: Overloads & Tagged, self: SelfCycle & Tagged, cycle: CycleA & Tagged,
  nullable: (Then<41> & Tagged) | undefined, optional: { then?: (cb: (value: 43) => void) => void } & Tagged,
  anyThen: { then: any } & Tagged, badCallback: { then(cb: 47): void } & Tagged,
  distinct: Then<"left"> & { then(cb: (value: 53) => void): void },
  primitiveBound: P & Then<59>, genericIntersection: T & Tagged
) {
  const freeValue = await free;
  const primitiveValue = await primitive;
  const objectValue = await object;
  const constrainedThenValue = await constrainedThen;
  const keyValue = await key;
  const indexedValue = await indexed;
  const directKeyValue = await directKey;
  const primitiveIndexedValue = await primitiveIndexed;
  const bothValue = await both;
  const plainValue = await plain;
  const brandedValue = await branded;
  const nestedValue = await nested;
  const ownValue = await own;
  const wrongValue = await wrong;
  const overloadsValue = await overloads;
  const selfValue = await self;
  const cycleValue = await cycle;
  const nullableValue = await nullable;
  const optionalValue = await optional;
  const anyThenValue = await anyThen;
  const badCallbackValue = await badCallback;
  const distinctValue = await distinct;
  const primitiveBoundValue = await primitiveBound;
  const genericIntersectionValue = await genericIntersection;
}
async function independent(left: Then<61> & Tagged, right: Then<"right"> & Tagged) {
  const leftValue = await left;
  const rightValue = await right;
  const leftAgainValue = await left;
}
"#;
    let case = TestCase::parse("probe/awaited-operands", "awaited-operands.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in [
        "freeValue : Awaited<T>",
        "primitiveValue : P",
        "objectValue : O",
        "constrainedThenValue : Awaited<F>",
        "keyValue : K",
        "indexedValue : Awaited<I[\"value\"]>",
        "directKeyValue : keyof T",
        "primitiveIndexedValue : Q[\"value\"]",
        "bothValue : 23",
        "plainValue : Tagged & { other: 42; }",
        "brandedValue : number & Then<29>",
        "nestedValue : 37",
        "ownValue : \"receiver\"",
        "overloadsValue : \"selected\"",
        "nullableValue : 41 | undefined",
        "anyThenValue : { then: any; } & Tagged",
        "distinctValue : never",
        "primitiveBoundValue : P & Then<59>",
        "genericIntersectionValue : T & Tagged",
        "leftValue : 61",
        "rightValue : \"right\"",
        "leftAgainValue : 61",
        // Native error recovery prints any with TS1320/TS1062. These remain
        // unsupported error images, not claimed native-matching answers.
        "wrongValue : error",
        "selfValue : error",
        "cycleValue : error",
        "optionalValue : error",
        "badCallbackValue : error",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn async_contextual_calls_do_not_publish_foreign_uninstantiated_parameters() {
    // Native infers Count through the contextual union. Our call mapper still
    // declines that case; preserving no-alias parameters must not turn that
    // gap into a confident return containing Promise.reject's foreign T.
    let source = r#"// @strict: true
// @target: esnext
declare class StateMachine<T> { onDone: (a: T) => void; }
declare function createMachine<T>(implementations: {
  services: Record<string, () => Promise<T> | StateMachine<T>>;
}): void;
createMachine<{ count: number }>({
  services: {
    test: async () => Promise.reject("some err"),
    async test2() { return Promise.reject("some err"); },
  },
});
"#;
    let case =
        TestCase::parse("probe/async-foreign-parameter", "async-foreign-parameter.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for text in ["test", "test2", "async () => Promise.reject(\"some err\")"] {
        assert!(lines.iter().any(|line| line.starts_with(&format!("{text} : "))), "{lines:?}");
        assert!(
            !lines.iter().any(|line| line == &format!("{text} : () => Promise<T>")),
            "foreign parameter escaped in {lines:?}"
        );
    }
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}

#[test]
fn custom_global_awaited_alias_does_not_gain_conditional_distribution() {
    // Pinned native with --noLib preserves Awaited<T | undefined> for this
    // tuple alias. Mapping each constituent would instead yield
    // Awaited<T> | Awaited<undefined>, a different tuple/union contract.
    let source = r"// @strict: true
// @target: esnext
// @noLib: true
interface Object {}
interface Function {}
interface CallableFunction {}
interface NewableFunction {}
interface IArguments {}
interface String {}
interface Number {}
interface Boolean {}
interface RegExp {}
interface Array<T> { length: number; [n: number]: T }
interface Promise<T> {}
type Awaited<T> = [T];
async function custom<T>(x: T | undefined) { const customValue = await x; return { customValue }; }
";
    let case = TestCase::parse("probe/custom-awaited", "custom-awaited.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    assert!(lines.iter().any(|line| line == "customValue : Awaited<T | undefined>"), "{lines:?}");
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}

#[test]
fn generic_await_recognizes_only_resolved_empty_named_constraints() {
    // Pinned native declarations wrap the first three values in Awaited<T>.
    // No own property names is insufficient: inherited/private properties,
    // either signature kind and index signatures all make a bound nonempty.
    let source = r"// @strict: true
// @target: esnext
interface Empty {}
interface EmptyChild extends Empty {}
interface Tagged { tag: string }
interface TaggedChild extends Tagged {}
interface Callable { (): number }
interface Constructible { new(): Tagged }
interface Indexed { [key: string]: number }
declare class EmptyClass {}
declare class PrivateClass { private value: number }
declare class TaggedClass { tag: string }
export async function emptyInterface<T extends Empty>(x: T) { const emptyInterfaceValue = await x; return { emptyInterfaceValue }; }
export async function emptyInherited<T extends EmptyChild>(x: T) { const emptyInheritedValue = await x; return { emptyInheritedValue }; }
export async function emptyClass<T extends EmptyClass>(x: T) { const emptyClassValue = await x; return { emptyClassValue }; }
export async function taggedInherited<T extends TaggedChild>(x: T) { const taggedInheritedValue = await x; return { taggedInheritedValue }; }
export async function privateClass<T extends PrivateClass>(x: T) { const privateClassValue = await x; return { privateClassValue }; }
export async function callable<T extends Callable>(x: T) { const callableValue = await x; return { callableValue }; }
export async function constructible<T extends Constructible>(x: T) { const constructibleValue = await x; return { constructibleValue }; }
export async function indexed<T extends Indexed>(x: T) { const indexedValue = await x; return { indexedValue }; }
export async function taggedClass<T extends TaggedClass>(x: T) { const taggedClassValue = await x; return { taggedClassValue }; }
";
    let case = TestCase::parse("probe/awaited-named-empty", "awaited-named-empty.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in [
        "emptyInterfaceValue : Awaited<T>",
        "emptyInheritedValue : Awaited<T>",
        "emptyClassValue : Awaited<T>",
        "taggedInheritedValue : T",
        "privateClassValue : T",
        "callableValue : T",
        "constructibleValue : T",
        "indexedValue : T",
        "taggedClassValue : T",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
    assert!(diagnostics_suite::reported_for(&case).is_empty());
}
