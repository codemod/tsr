//! Contextual generic return inference controls.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn generic_return_annotations_supply_callback_parameters() {
    let source = r"// @strict: true
// @target: es2020
type Mapper<T,U>=(value:T)=>U;
declare function wrap<T,U>(callback:Mapper<T,U>):Mapper<T,U>;
declare function arrayize<T,U>(callback:Mapper<T,U>):Mapper<T,U[]>;
export const mapper:Mapper<string,number>=wrap(value=>value.length);
export const nested:Mapper<string,number[]>=arrayize(wrap(nestedValue=>nestedValue.length));
declare function combine<A,B,C>(first:(value:A)=>B,second:(value:B)=>C):(value:A)=>C;
export const combined:Mapper<string,boolean>=combine(wrap(first=>first.length),wrap(second=>second>10));
export const twice:Mapper<string,number[][]>=arrayize(arrayize(wrap(deep=>deep.length)));
";
    let case = TestCase::parse(
        "probe/contextual-generic-returns",
        "contextual-generic-returns.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    assert!(
        lines.iter().any(|line| line == "value=>value.length : (value: string) => number"),
        "{lines:?}"
    );
    assert!(lines.iter().any(|line| line=="nestedValue=>nestedValue.length : (nestedValue: string) => number"), "{lines:?}");
    assert!(
        lines.iter().any(|line| line == "deep=>deep.length : (deep: string) => number"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line == "second=>second>10 : (second: number) => boolean"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line == "first=>first.length : (first: string) => number"),
        "{lines:?}"
    );
    assert!(
        lines.iter().any(|line| line == "wrap(second=>second>10) : Mapper<number, boolean>"),
        "{lines:?}"
    );
}

/// Pinned tsgo 5b1047d1 controls: checkExpressionWithContextualType keeps a
/// literal argument when the return mapper's contextual parameter type is that
/// literal, and createOuterReturnMapper snapshots an outer context once.
#[test]
fn return_mapper_literal_contexts_regularize_arguments() {
    let source = r"// @strict: true
interface Wrap<T> { value: T; }
declare function wrap<T>(value: T): Wrap<T>;
function f2(): Wrap<'foo'> { return wrap('foo'); }
const inner = (() => { let x: Wrap<'bar'> = wrap('bar'); return wrap('baz'); })();
enum Enum { A, B }
type Func<T> = (x: T) => T;
declare function makeFoo<T>(x: T): Func<T>;
declare function baz<U>(x: Func<U>, y: Func<U>): [U];
const z = baz(makeFoo(Enum.A), makeFoo(Enum.B));
declare function pair<T>(a: T): { a: T; b: boolean };
const g: { a: true; b: boolean } = pair(true);
";
    let case = TestCase::parse("probe/return-mapper-literals", "return-mapper-literals.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "wrap('foo') : Wrap<\"foo\">",
        "wrap('bar') : Wrap<\"bar\">",
        "wrap('baz') : Wrap<string>",
        "inner : Wrap<string>",
        "makeFoo(Enum.A) : Func<Enum>",
        "makeFoo(Enum.B) : Func<Enum>",
        "z : [Enum]",
        "pair(true) : { a: true; b: boolean; }",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
