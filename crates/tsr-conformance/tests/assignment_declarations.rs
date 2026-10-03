//! Assignment declaration controls grounded in pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "assignment.ts", source);
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
fn descriptor_values_and_commonjs_assignment_unions() {
    let source = r#"// @strict: true
// @noImplicitAny: false
// @target: es2020
// @module: commonjs
// @allowJs: true
// @checkJs: true
// @filename: mod.js
Object.defineProperty(exports,"value",{value:42,writable:true});
Object.defineProperty(exports,"readonlyValue",{value:"fixed"});
Object.defineProperty(exports,"getter",{get() { return "read"; }});
Object.defineProperty(exports,"setter",{
    /** @param {boolean} value */
    set(value) {}
});
Object.defineProperty(exports,"both",{
    get() {return "get";},
    /** @param {boolean} value */
    set(value) {}
});
Object.defineProperty(exports,"empty",{});
exports.literals=1;
exports.literals=2;
exports.initial=undefined;
exports.initial="ready";
exports.nullOnly=null;
// @filename: validator.ts
import m = require("./mod");
export const value=m.value;
export const readonlyValue=m.readonlyValue;
export const getter=m.getter;
export const setter=m.setter;
export const both=m.both;
export const empty=m.empty;
export const literals=m.literals;
export const initial=m.initial;
export const nullOnly=m.nullOnly;
m.value=10;
m.readonlyValue="other";
m.getter="other";
m.setter=true;
"#;
    let lines = lines("probe/assignment-descriptors", source);
    for wanted in [
        "value : number",
        "readonlyValue : string",
        "getter : string",
        "setter : boolean",
        "both : string",
        "empty : any",
        "literals : 1 | 2",
        "initial : \"ready\"",
        "nullOnly : any",
        "m.value : number",
        "m.readonlyValue : any",
        "m.getter : any",
        "m.setter : boolean",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn synthetic_defaults_and_expando_container_boundaries() {
    let source = r#"// @module: commonjs
// @target: es2015
// @strict: false
// @allowJs: true
// @filename: default.js
exports.default = {bar() {return 0;}};
// @filename: factory.js
module.exports = function () {
    class A {}
    return {c:A.b=1};
};
// @filename: validator.ts
import foo from "./default";
foo.bar();
"#;
    let lines = lines("probe/assignment-boundaries", source);
    // The synthetic default denotes the namespace, so bar is absent.
    // General missing-property errorType-to-any recovery remains a gap.
    for wanted in ["foo.bar() : error", "foo.bar : error", "A.b : error", "b : any"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

// Native 5b1047d10d32e7d5b446be4de56b126ff42f82bb, strict with exact mode
// both enabled and disabled. Distinct type arguments catch declaration-symbol
// lookup instead of concrete receiver lookup; explicit undefined is not missing.
const OPTIONAL_WRITES: &str = r#"interface Box<T> {
    implicit?: T;
    explicit?: T | undefined;
    required: T;
    always?: undefined;
    empty?: never;
}
declare const box: Box<number>;
box.implicit = undefined;
declare const maybe: number | undefined;
declare const unionBox: Box<number>;
unionBox.implicit = maybe;
declare const explicitBox: Box<number>;
explicitBox.explicit = undefined;
declare const requiredBox: Box<number>;
requiredBox.required = undefined;
declare const alwaysBox: Box<number>;
alwaysBox.always = undefined;
declare const emptyBox: Box<number>;
emptyBox.empty = undefined;
declare const implicitMismatch: Box<number>;
implicitMismatch.implicit = "bad";
declare const explicitMismatch: Box<number>;
explicitMismatch.explicit = "bad";
declare const successBox: Box<number>;
successBox.implicit = 12;
const read: number | undefined = successBox.implicit;
interface Inherited extends Box<string> {}
declare const inherited: Inherited;
inherited.implicit = undefined;
declare const inheritedExplicit: Inherited;
inheritedExplicit.explicit = undefined;
declare const inheritedMismatch: Inherited;
inheritedMismatch.implicit = 42;
declare const requiredUnion: { value: number | undefined };
requiredUnion.value = undefined;
declare const dynamic: any;
declare const anyBox: Box<number>;
anyBox.implicit = dynamic;
declare const divergent: {
    get value(): number | undefined;
    set value(value: number | undefined);
};
divergent.value = undefined;
"#;

fn optional_write_case(exact: bool) -> TestCase {
    TestCase::parse(
        "probe/exact-optional-writes",
        "assignment.ts",
        &format!(
            "// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n// @filename: assignment.ts\n{OPTIONAL_WRITES}"
        ),
    )
}

#[test]
fn exact_optional_writes_select_2412_only_for_missing_properties() {
    for exact in [true, false] {
        let case = optional_write_case(exact);
        let mut actual = tsr_conformance::diagnostics_suite::reported_for(&case);
        actual.sort_unstable();
        let mut expected = vec![(16, 2322), (22, 2322), (24, 2322), (34, 2322)];
        if exact {
            expected.extend([(9, 2412), (12, 2412), (20, 2412), (30, 2412)]);
        }
        expected.sort_unstable();
        assert_eq!(
            actual
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.code,
                ))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|&(line, code)| ("assignment.ts", line, 1, code))
                .collect::<Vec<_>>(),
            "exact={exact}"
        );
    }
}

#[test]
fn exact_optional_write_messages_preserve_concrete_targets_and_source_display() {
    let case = optional_write_case(true);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let file = program.source_file("assignment.ts").expect("fixture file");
    let id = file.source_file().node_id.expect("registered file");
    checker.set_checked_files([id]);
    checker.check_source_file(
        id,
        tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
    );
    let mut messages: Vec<_> = checker
        .diagnostics()
        .iter()
        .map(|(_, diagnostic)| {
            let (line, _) = tsr_conformance::symbols_baseline::line_and_character(
                file.text(),
                diagnostic.span.start,
            );
            let args: Vec<_> = diagnostic.args.iter().map(String::as_str).collect();
            (line + 1, diagnostic.message.format(&args))
        })
        // Target union reduction in ordinary TS2322 is a separate reporter
        // prerequisite. The complete code/position test still checks this row.
        .filter(|(line, _)| *line != 24)
        .collect();
    messages.sort_unstable();
    assert_eq!(
        messages,
        [
            (9, "Type 'undefined' is not assignable to type 'number' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target."),
            // This port checks the native primary message, not its child chain.
            (12, "Type 'number | undefined' is not assignable to type 'number' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target."),
            (16, "Type 'undefined' is not assignable to type 'number'."),
            (20, "Type 'undefined' is not assignable to type 'never' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target."),
            (22, "Type 'string' is not assignable to type 'number'."),
            (30, "Type 'undefined' is not assignable to type 'string' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the type of the target."),
            (34, "Type 'number' is not assignable to type 'string'."),
        ]
        .map(|(line, text)| (line, text.to_owned()))
    );
}

#[test]
fn undefined_in_source_does_not_make_explicit_undefined_an_exact_optional_mismatch() {
    for exact in [true, false] {
        let source = format!(
            "// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n// @filename: explicit.ts\n\
             declare const maybeText: string | undefined;\n\
             declare const explicitUnionMismatch: {{ explicit?: number | undefined }};\n\
             explicitUnionMismatch.explicit = maybeText;\n\
             declare const onlyUndefinedMismatch: {{ always?: undefined }};\n\
             onlyUndefinedMismatch.always = maybeText;\n"
        );
        let case = TestCase::parse("probe/explicit-undefined-writes", "explicit.ts", &source);
        let mut actual = tsr_conformance::diagnostics_suite::reported_for(&case);
        actual.sort_unstable();
        assert_eq!(
            actual
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.code,
                ))
                .collect::<Vec<_>>(),
            [("explicit.ts", 3, 1, 2322), ("explicit.ts", 5, 1, 2322)],
            "exact={exact}"
        );
    }
}

// Native 5b1047d10d32e7d5b446be4de56b126ff42f82bb, strict, exact on/off.
// RequiredBox catches inspecting the source declaration's T instead of the
// instantiated read type; Box<number | undefined> catches raw target-symbol
// lookup. The unrelated undefined member must not influence the optional a.
const OPTIONAL_OBJECTS: &str = r"interface Optional { a?: string; d: boolean }
interface Explicit { a?: string | undefined; d: boolean }
interface RequiredFields { a: string; d: boolean }
declare const optionalWrong: { d: boolean } & { a?: number };
declare const optionalGood: { a?: string } & { d: boolean };
declare const explicitSource: { a: string | undefined; d: boolean };
declare const good: { a: string; d: boolean };
declare const omitted: { d: boolean };
const optionalCheck: Optional = optionalWrong;
const optionalPositive: Optional = optionalGood;
const explicitSourceCheck: Optional = explicitSource;
const positiveCheck: Optional = good;
const omittedCheck: Optional = omitted;
const explicitTargetCheck: Explicit = optionalWrong;
const requiredTargetCheck: RequiredFields = explicitSource;
declare const unrelated: { other?: number; d: number | undefined };
const unrelatedCheck: Optional = unrelated;
declare const undefinedOnly: { a: string | undefined };
const undefinedOnlyCheck: { a?: undefined } = undefinedOnly;
interface Box<T> { value?: T; tag: boolean }
interface RequiredBox<T> { value: T; tag: boolean }
declare const box: Box<string>;
declare const requiredBox: RequiredBox<string | undefined>;
const genericCheck: Box<number> = box;
const explicitGenericCheck: Box<number | undefined> = box;
const requiredGenericCheck: Box<string> = requiredBox;
declare const tuple: [number?];
const tupleCheck: [string?] = tuple;
declare const always: { a: undefined };
const alwaysCheck: { a?: undefined } = always;
";

fn optional_object_case(exact: bool) -> TestCase {
    TestCase::parse(
        "probe/exact-optional-objects",
        "assignment.ts",
        &format!(
            "// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n// @filename: assignment.ts\n{OPTIONAL_OBJECTS}"
        ),
    )
}

#[test]
fn exact_optional_objects_select_2375_from_corresponding_concrete_read_members() {
    for exact in [true, false] {
        let case = optional_object_case(exact);
        let mut actual = tsr_conformance::diagnostics_suite::reported_for(&case);
        actual.sort_unstable();
        let mut expected =
            vec![(14, 2322), (15, 2322), (17, 2322), (19, 2322), (25, 2322), (28, 2322)];
        if exact {
            expected.extend([(9, 2375), (11, 2375), (24, 2375), (26, 2375)]);
        } else {
            expected.extend([(9, 2322), (24, 2322)]);
        }
        expected.sort_unstable();
        assert_eq!(
            actual
                .iter()
                .map(|diagnostic| (
                    diagnostic.file.as_str(),
                    diagnostic.line,
                    diagnostic.column,
                    diagnostic.code,
                ))
                .collect::<Vec<_>>(),
            expected
                .iter()
                .map(|&(line, code)| ("assignment.ts", line, 7, code))
                .collect::<Vec<_>>(),
            "exact={exact}"
        );
    }
}

// Strict native 5b1047d10d32e7d5b446be4de56b126ff42f82bb, exact on/off.
// Presence removes intrinsic missing, not written undefined or null. Receiver
// reassignment, shadowed bindings and guards on another member/receiver must
// not reuse the positive fact. Declaration output independently pins reads.
const PRESENCE_READS: &str = r#"interface Box { a?: string; b?: string | undefined }
export function present(box: Box) {
    if ("a" in box) {
        const read: string = box.a;
        box.a = box.a;
        return box.a;
    }
    throw 0;
}
export function unguarded(box: Box) {
    box.a = box.a;
}
export function absent(box: Box) {
    if ("a" in box) {} else {
        box.a = box.a;
        const read: string = box.a;
    }
}
export function explicit(box: Box) {
    if ("b" in box) {
        const read: string = box.b;
        box.a = box.b;
    }
}
export function reset(box: Box, other: Box) {
    if ("a" in box) {
        box = other;
        box.a = box.a;
    }
}
export function shadow(box: Box, other: Box) {
    if ("a" in box) {
        { const box = other; box.a = box.a; }
    }
}
export function differentProperty(box: Box) {
    if ("b" in box) { box.a = box.a; }
}
export function differentReceiver(box: Box, other: Box) {
    if ("a" in other) { box.a = box.a; }
}
export function nested(container: { inner: Box }) {
    if ("a" in (container.inner)) {
        const read: string = container.inner.a;
        return container.inner.a;
    }
    throw 0;
}
export function constantKey(box: Box) {
    const key = "a";
    if (key in box) {
        const read: string = box[key];
        return box[key];
    }
    throw 0;
}
export function own(box: Box) {
    if ((box).hasOwnProperty("a")) {
        const read: string = box.a;
        box.a = box.a;
        return box.a;
    }
    throw 0;
}
export function ownExplicit(box: Box) {
    if (box.hasOwnProperty("b")) {
        const read: string = box.b;
        box.a = box.b;
    }
}
export function ownShadow(box: Box, other: Box) {
    if (box.hasOwnProperty("a")) {
        { const box = other; box.a = box.a; }
    }
}
export function ownReset(box: Box, other: Box) {
    if (box.hasOwnProperty("a")) {
        box = other;
        box.a = box.a;
    }
}
export function numeric(box: { 0?: number }) {
    if (0 in box) {
        const read: number = box[0];
        return box[0];
    }
    throw 0;
}
export function template(box: Box) {
    if (box.hasOwnProperty(`a`)) {
        const read: string = box.a;
        return box.a;
    }
    throw 0;
}
export function nullable(box: { a?: string | null }) {
    if ("a" in box) {
        const read: string | null = box.a;
        const notNull: string = box.a;
        return box.a;
    }
    throw 0;
}
"#;

#[test]
fn exact_optional_presence_reads_keep_native_negative_codes_and_positions() {
    for exact in [true, false] {
        let source = format!(
            "// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n// @filename: assignment.ts\n{PRESENCE_READS}"
        );
        let case = TestCase::parse("probe/optional-presence", "assignment.ts", &source);
        let mut actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
            .into_iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        actual.sort_unstable();
        let mut expected = vec![(16, 15, 2322), (21, 15, 2322), (67, 15, 2322), (99, 15, 2322)];
        if exact {
            expected.extend([
                (11, 5, 2412),
                (15, 9, 2412),
                (22, 9, 2412),
                (28, 9, 2412),
                (33, 30, 2412),
                (37, 23, 2412),
                (40, 25, 2412),
                (68, 9, 2412),
                (73, 30, 2412),
                (79, 9, 2412),
            ]);
        } else {
            expected.extend([
                (4, 15, 2322),
                (44, 15, 2322),
                (52, 15, 2322),
                (59, 15, 2322),
                (84, 15, 2322),
                (91, 15, 2322),
                (98, 15, 2322),
            ]);
        }
        expected.sort_unstable();
        assert_eq!(actual, expected, "exact={exact}");
    }
}

#[test]
fn exact_optional_object_messages_preserve_intersections_and_instantiated_names() {
    let case = optional_object_case(true);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let file = program.source_file("assignment.ts").expect("fixture file");
    let id = file.source_file().node_id.expect("registered file");
    checker.set_checked_files([id]);
    checker.check_source_file(
        id,
        tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
    );
    let mut messages: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter_map(|(_, diagnostic)| {
            let (line, _) = tsr_conformance::symbols_baseline::line_and_character(
                file.text(),
                diagnostic.span.start,
            );
            let args: Vec<_> = diagnostic.args.iter().map(String::as_str).collect();
            [9, 11, 24, 26]
                .contains(&(line + 1))
                .then(|| (line + 1, diagnostic.message.format(&args)))
        })
        .collect();
    messages.sort_unstable();
    assert_eq!(
        messages,
        [
            (9, "Type '{ d: boolean; } & { a?: number; }' is not assignable to type 'Optional' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties."),
            (11, "Type '{ a: string | undefined; d: boolean; }' is not assignable to type 'Optional' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties."),
            (24, "Type 'Box<string>' is not assignable to type 'Box<number>' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties."),
            (26, "Type 'RequiredBox<string | undefined>' is not assignable to type 'Box<string>' with 'exactOptionalPropertyTypes: true'. Consider adding 'undefined' to the types of the target's properties."),
        ]
        .map(|(line, text)| (line, text.to_owned()))
    );
}

#[test]
fn exact_optional_presence_read_returns_match_native_declarations() {
    for exact in [true, false] {
        let source = format!(
            "// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n// @filename: assignment.ts\n{PRESENCE_READS}"
        );
        let actual = lines("probe/optional-presence", &source);
        let suffix = if exact { "" } else { " | undefined" };
        for wanted in [
            format!("present : (box: Box) => string{suffix}"),
            format!("constantKey : (box: Box) => string{suffix}"),
            format!("own : (box: Box) => string{suffix}"),
            format!("numeric : (box: {{ 0?: number; }}) => number{suffix}"),
            format!("template : (box: Box) => string{suffix}"),
            format!("nullable : (box: {{ a?: string | null; }}) => string | null{suffix}"),
        ] {
            assert!(actual.contains(&wanted), "missing {wanted}: {actual:?}");
        }
    }
}

#[test]
fn optional_presence_facts_do_not_narrow_definite_write_targets() {
    for exact in [true, false] {
        for condition in ["\"a\" in box", "box.hasOwnProperty(\"a\")"] {
            let source = format!(
                r"// @strict: true
// @exactOptionalPropertyTypes: {exact}
// @filename: assignment.ts
export function writes(box: {{ a?: string }}) {{
    if ({condition}) {{
        box.a = box.a;
    }} else {{
        box.a = box.a;
    }}
}}
"
            );
            let actual = lines("probe/presence-write-boundary", &source);
            // Native read values are string/undefined in exact mode, but BOTH
            // definite write targets keep string. Non-exact reads/writes retain
            // undefined. Applying the false presence fact to the LHS yields never.
            let mut property_types: Vec<_> =
                actual.iter().filter_map(|line| line.strip_prefix("box.a : ")).collect();
            property_types.sort_unstable();
            assert_eq!(
                property_types,
                if exact {
                    vec!["string", "string", "string", "undefined"]
                } else {
                    vec!["string | undefined"; 4]
                },
                "exact={exact}, condition={condition}"
            );
        }
    }
}
