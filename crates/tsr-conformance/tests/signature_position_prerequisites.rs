//! Signature-position consumers, pinned against native 5b1047d1.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn initialized_contextual_parameters_narrow_reads_without_changing_writes() {
    let source = r"declare function contextual<T extends (x?: number | undefined) => any>(input: T): T;
const body = contextual((p = 1) => { const observedBody = p; return p; });
const optional = contextual(p => { const observedOptional = p; return p; });
const write = contextual((p = 1) => { p = undefined; const observedWrite = p; });
const undefinedDefault = contextual((p = undefined) => { const observedUndefined = p; return p; });
";
    for strict in [false, true] {
        let source = format!("// @strictNullChecks: {strict}\n{source}");
        let case = TestCase::parse("probe/signature-position-consumers", "consumer.ts", &source);
        let expected = [FileTypes { file: "consumer.ts".into(), assertions: Vec::new() }];
        let actual = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
        for (name, expected) in [
            ("observedBody", "number"),
            ("observedOptional", if strict { "number | undefined" } else { "number" }),
            ("observedWrite", if strict { "undefined" } else { "number" }),
            ("observedUndefined", if strict { "number | undefined" } else { "number" }),
        ] {
            let actual = actual[0].iter().find(|a| a.text == name).expect(name);
            assert_eq!(actual.type_string, expected, "{name}, strict={strict}");
        }
        // The contextual write target keeps its declared type, even though
        // the default narrowed entry reads. The diagnostics traversal's
        // pre-existing missing contextual assignment is audited separately.
        let write = actual[0].windows(2).find(|pair| pair[0].text == "p = undefined").unwrap();
        assert_eq!(write[1].text, "p");
        assert_eq!(write[1].type_string, if strict { "number | undefined" } else { "number" });
    }
}

#[test]
fn primitive_array_mismatch_does_not_skip_naked_or_unsupported_generics() {
    let source = r#"declare function arrayFirst<T>(value: T[]): "array";
declare function arrayFirst(value: string): "string";
const afterArray = arrayFirst("abc");
const actualArray = arrayFirst([1]);
declare function stringFirst(value: string): "string";
declare function stringFirst<T>(value: T[]): "array";
const beforeArray = stringFirst("abc");
declare function readonlyFirst<T>(value: ReadonlyArray<T>): "readonly";
declare function readonlyFirst(value: string): "string";
const afterReadonly = readonlyFirst("abc");
const actualReadonly = readonlyFirst([1]);
declare function nakedFirst<T>(value: T): "generic";
declare function nakedFirst(value: string): "string";
const naked = nakedFirst("abc");
namespace shadow {
    export type Array<T> = T;
    export declare function select<T>(value: Array<T>): "shadow";
    export declare function select(value: string): "string";
    export const shadowed = select("abc");
}
declare let uncertain: unknown;
declare function arrayOrUnknown<T>(value: T[]): "array";
declare function arrayOrUnknown(value: unknown): "unknown";
const unsupported = arrayOrUnknown(uncertain);
"#;
    for strict in [false, true] {
        let source = format!("// @strictNullChecks: {strict}\n{source}");
        let case = TestCase::parse("probe/primitive-array-overloads", "consumer.ts", &source);
        let expected = [FileTypes { file: "consumer.ts".into(), assertions: Vec::new() }];
        let actual = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
        for (name, expected) in [
            ("afterArray", "\"string\""),
            ("actualArray", "\"array\""),
            ("beforeArray", "\"string\""),
            ("afterReadonly", "\"string\""),
            ("actualReadonly", "\"readonly\""),
            ("naked", "\"generic\""),
            // The identity alias exposes its instantiable body, so native's
            // first generic overload succeeds without treating it as an array.
            ("shadowed", "\"shadow\""),
            // Native accepts unknown, but its inference prerequisite remains
            // unsupported. It must not be skipped to manufacture a winner.
            ("unsupported", "error"),
        ] {
            let actual = actual[0].iter().find(|a| a.text == name).expect(name);
            assert_eq!(actual.type_string, expected, "{name}, strict={strict}");
        }
    }
}

#[test]
fn empty_global_arrays_do_not_prove_a_primitive_mismatch() {
    let source = r#"// @noLib: true
interface Array<T> {}
interface ReadonlyArray<T> {}
interface Boolean {}
interface CallableFunction {}
interface Function {}
interface IArguments {}
interface NewableFunction {}
interface Number {}
interface Object {}
interface RegExp {}
interface String {}
declare function emptyArrayFirst<T>(value: T[]): "array";
declare function emptyArrayFirst(value: string): "string";
const afterEmptyArray = emptyArrayFirst("abc");
declare function emptyReadonlyFirst<T>(value: ReadonlyArray<T>): "readonly";
declare function emptyReadonlyFirst(value: string): "string";
const afterEmptyReadonly = emptyReadonlyFirst("abc");
"#;
    for strict in [false, true] {
        let source = format!("// @strictNullChecks: {strict}\n{source}");
        let case = TestCase::parse("probe/empty-global-arrays", "consumer.ts", &source);
        let expected = [FileTypes { file: "consumer.ts".into(), assertions: Vec::new() }];
        let actual = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
        for name in ["afterEmptyArray", "afterEmptyReadonly"] {
            let actual = actual[0].iter().find(|a| a.text == name).expect(name);
            // Native selects the generic candidate. Existing inference cannot
            // decide it; array identity must not fabricate the later winner.
            assert_eq!(actual.type_string, "error", "{name}, strict={strict}");
        }
    }
}

#[test]
fn initialized_annotated_parameter_writes_still_use_the_declared_type() {
    for strict in [false, true] {
        let source = format!(
            "// @strictNullChecks: {strict}\nfunction write(p: number | undefined = 1) {{ p = undefined; }}"
        );
        let case = TestCase::parse("probe/initialized-parameter-write", "consumer.ts", &source);
        assert!(tsr_conformance::diagnostics_suite::reported_for(&case).is_empty());
    }
}

#[test]
fn optional_and_rest_union_overloads_keep_all_branch_orders() {
    let source = r#"interface A { (x: number): number; (x: string, y?: string): boolean; (x: Date): void; <T>(x: T[]): T[]; }
interface B { (x: number): number; (x: string): string; (x: Date): void; <T>(x: T[]): T[]; }
interface C { (x: string, ...y: string[]): number; (x: number, s?: string): number; <T>(x: T[]): T[]; }
declare let first: A | B | C;
declare let reversed: C | B | A;
declare let rotated: B | A | C;
const stringFirst = first("abc");
const stringReversed = reversed("abc");
const stringRotated = rotated("abc");
const numeric = first(1);
const array = first([true, false]);
"#;
    for strict in [false, true] {
        let source = format!("// @strictNullChecks: {strict}\n{source}");
        let case = TestCase::parse("probe/optional-union-overloads", "consumer.ts", &source);
        let expected = [FileTypes { file: "consumer.ts".into(), assertions: Vec::new() }];
        let actual = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
        for (name, expected) in [
            ("stringFirst", "string | number | boolean"),
            ("stringReversed", "string | number | boolean"),
            ("stringRotated", "string | number | boolean"),
            ("numeric", "number"),
            ("array", "boolean[]"),
        ] {
            let actual = actual[0].iter().find(|a| a.text == name).expect(name);
            assert_eq!(actual.type_string, expected, "{name}, strict={strict}");
        }
    }
}
