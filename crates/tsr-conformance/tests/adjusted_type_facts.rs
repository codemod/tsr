//! Outcomes checked against pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/adjusted_type_facts", "probe.ts", source);
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
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn nullable_generic_flow_types_remain_semantic_through_instantiation() {
    expect(
        r"// @strict: true
export function notNull<T>(value:T){if(value===null)throw 0;return value;}
export function notUndefined<T>(value:T){if(value===undefined)throw 0;return value;}
export function both<T>(value:T){if(value==null)throw 0;return value;}
export function sequential<T>(value:T){return notUndefined(notNull(value));}
export function undefinedConstraint<T extends {}|undefined>(value:T){if(value===undefined)throw 0;return value;}
export function nonNullConstraint<T extends {}|undefined>(value:T){if(value===null)throw 0;return value;}
export function definedConstraint<T extends {}>(value:T){if(!value)throw 0;return value;}
export function explicitNullable<T>(value:T|null|undefined){if(value==null)throw 0;return value;}
export function unknownDefined(value:unknown){if(value===undefined)throw 0;return value;}
export function unknownTruthy(value:unknown){if(!value)throw 0;return value;}
declare const maybeString:string|null|undefined;
export const stringResult=sequential(maybeString);
declare const maybeNumber:number|null|undefined;
export const numberResult=sequential(maybeNumber);
",
        &[
            "notNull : <T>(value: T) => T & ({} | undefined)",
            "notUndefined : <T>(value: T) => T & ({} | null)",
            "both : <T>(value: T) => NonNullable<T>",
            "sequential : <T>(value: T) => T & {}",
            "undefinedConstraint : <T extends {} | undefined>(value: T) => T & {}",
            "nonNullConstraint : <T extends {} | undefined>(value: T) => T",
            "definedConstraint : <T extends {}>(value: T) => T",
            "explicitNullable : <T>(value: T | null | undefined) => NonNullable<T>",
            "unknownDefined : (value: unknown) => {} | null",
            "unknownTruthy : (value: unknown) => {}",
            "stringResult : string",
            "numberResult : number",
        ],
    );
}

#[test]
fn opposite_nullable_and_unknown_boundaries() {
    expect(
        r"// @strict: true
export function withNull<T>(v:T|null){if(v===undefined)throw 0;return v;}
export function withUndefined<T>(v:T|undefined){if(v===null)throw 0;return v;}
export function repeated<T>(v:T){if(v==null)throw 0;if(v==null)throw 0;return v;}
export function falsyUnknown(v:unknown){if(v)throw 0;return v;}
export function onlyNull(v:unknown){if(v!==null)throw 0;return v;}
export function onlyUndefined(v:unknown){if(v!==undefined)throw 0;return v;}
export function indexed<T,K extends keyof T>(v:T[K]){if(v===undefined)throw 0;return v;}
export function direct<T>(v:T){return v!;}
export function nested<T>(v:NonNullable<T>){return v!;}
",
        &[
            "withNull : <T>(v: T | null) => (T & {}) | null",
            "withUndefined : <T>(v: T | undefined) => (T & {}) | undefined",
            "repeated : <T>(v: T) => NonNullable<T>",
            "falsyUnknown : (v: unknown) => unknown",
            "onlyNull : (v: unknown) => null",
            "onlyUndefined : (v: unknown) => undefined",
            "indexed : <T, K extends keyof T>(v: T[K]) => T[K] & ({} | null)",
            "direct : <T>(v: T) => NonNullable<T>",
            "nested : <T>(v: NonNullable<T>) => NonNullable<T>",
        ],
    );
}

#[test]
fn non_strict_checks_do_not_add_intersections() {
    expect(
        r"// @strict: false
export function both<T>(v:T){if(v==null)throw 0;return v;}
export function direct<T>(v:T){return v!;}
export function onlyNull(v:unknown){if(v!==null)throw 0;return v;}
",
        &["both : <T>(v: T) => T", "direct : <T>(v: T) => T", "onlyNull : (v: unknown) => unknown"],
    );
}

#[test]
fn absent_global_alias_uses_the_intersection() {
    expect(
        r"// @strict: true
// @noLib: true
export function g<T extends {x:string}|undefined>(obj:T){if(obj!=null)return obj;throw 0;}
export function h<T>(obj:T){if(obj)return obj;throw 0;}
",
        &[
            "g : <T extends { x: string; } | undefined>(obj: T) => T & {}",
            "h : <T>(obj: T) => T & {}",
        ],
    );
}

#[test]
fn semantic_narrowing_reaches_flow_consumers() {
    expect(
        r#"// @strict: true
export function compound<T>(v:T){if(v!==undefined && v!==null)return v;throw 0;}
export function joined<T>(v:T|null|undefined){if(v!==undefined)v;if(v!==null)v;return v;}
function isDefined<T>(v:T):v is NonNullable<T>{return v!=null;}
function assertDefined<T>(v:T):asserts v is NonNullable<T>{if(v==null)throw 0;}
export function predicate(v:string|undefined){if(isDefined(v))return v;throw 0;}
export function assertion(v:string|null){assertDefined(v);return v;}
export function optional(v:{x?:string}){if(isDefined(v?.x))return v.x;throw 0;}
export function instance(v:unknown){if(v && v instanceof Object)return v;throw 0;}
export function rejoined(v:unknown){if(typeof v==="object"){if(v)v;}return v;}
export function forIn<T>(v:T){for(const k in v)return v;throw 0;}
"#,
        &[
            "compound : <T>(v: T) => T & {}",
            "joined : <T>(v: T | null | undefined) => T | null | undefined",
            "predicate : (v: string | undefined) => string",
            "assertion : (v: string | null) => string",
            "optional : (v: { x?: string; }) => string",
            "instance : (v: unknown) => Object",
            "rejoined : (v: unknown) => unknown",
            "forIn : <T>(v: T) => T",
        ],
    );
}

#[test]
fn written_aliases_and_explicit_predicate_arguments_keep_their_meaning() {
    expect(
        r"// @strict: true
function defined<T>(v:T):v is NonNullable<T>{return v!=null;}
function assertDefined<T>(v:T):asserts v is NonNullable<T>{if(v==null)throw 0;}
export function explicit(v:string|null){if(defined<string|null>(v))return v;throw 0;}
export function asserted(v:string|null){assertDefined<string|null>(v);return v;}
export function written<T>(x:T){if(x)x;let u:T|NonNullable<T>;return u!;}
export function returned<T>(x:T){if(x)return x;return x;}
",
        &[
            "explicit : (v: string | null) => string",
            "asserted : (v: string | null) => string",
            "u : T | NonNullable<T>",
            "written : <T>(x: T) => NonNullable<T>",
            "returned : <T>(x: T) => T",
        ],
    );
}

#[test]
fn empty_objects_do_not_include_callable_or_indexed_shapes() {
    expect(
        r"// @strict: true
declare function make<T>(v:T):()=>T;
export function callable(){const f=make(1);if(!f)return f;throw 0;}
export function noncallable<T extends {}>(v:T){if(!v)return v;throw 0;}
export function mapped<T>(v:Record<string,T>){if(!v)return v;throw 0;}
",
        &[
            "callable : () => never",
            "noncallable : <T extends {}>(v: T) => T",
            "mapped : <T>(v: Record<string, T>) => never",
        ],
    );
}

#[test]
fn apparent_empty_objects_retain_object_methods_without_narrowing_free_unknown() {
    // Pinned getApparentType/getPropertyOfTypeEx controls distinguish the
    // canonical empty-object images from free unknown and missing members.
    let source = r#"// @strict: true
// @target: es2015
function nonprimitive(value: object) {
    value.toString();
    value.hasOwnProperty("own");
    value.missing;
    return value;
}
function empty(value: {}) {
    value.toString();
    value.hasOwnProperty("own");
    value.missing;
    return value;
}
function narrowed(value: unknown) {
    if (value) {
        value.toString();
        value.hasOwnProperty("own");
        value.missing;
    }
    if (typeof value === "object" && value) {
        value.toString();
        value.hasOwnProperty("own");
    }
    return value;
}
function constrained<T extends object>(value: T) {
    value.toString();
    value.hasOwnProperty("own");
    return value;
}
function objectText(value: object) { return value.toString(); }
function unknownText(value: unknown) { if (value) return value.toString(); throw 0; }
function genericOwn<T extends object>(value: T) { return value.hasOwnProperty("own"); }
function freeUnknown(value: unknown) { return value.toString(); }
"#;
    expect(
        source,
        &[
            "nonprimitive : (value: object) => object",
            "empty : (value: {}) => {}",
            "narrowed : (value: unknown) => unknown",
            "constrained : <T extends object>(value: T) => T",
            "value.toString() : string",
            "value.toString : () => string",
            "value.hasOwnProperty(\"own\") : boolean",
            "value.hasOwnProperty : (v: PropertyKey) => boolean",
            "objectText : (value: object) => string",
            "unknownText : (value: unknown) => string",
            "genericOwn : <T extends object>(value: T) => boolean",
            "freeUnknown : (value: unknown) => any",
        ],
    );
    let case = TestCase::parse("probe/apparent-empty", "apparent-empty.ts", source);
    let diagnostics: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    // Preserve the native diagnostic on the declared {} receiver. Raw object
    // and narrowed-unknown missing-member diagnostics still decline at the
    // separate receiver/completeness gate (tracked under tsr-6.47.6); free
    // unknown's TS18046 is also unported. This is a lookup control, not a claim
    // of complete diagnostic equality for this source.
    assert!(diagnostics.contains(&(10, 11, 2339)), "{diagnostics:?}");
}

#[test]
fn loose_unknown_augments_object_without_becoming_callable() {
    expect(
        r#"// @strictNullChecks: false
function unknownText(value: unknown) { return value.toString(); }
function unknownOwn(value: unknown) { return value.hasOwnProperty("own"); }
function unknownApply(value: unknown) { return value.apply(); }
function objectApply(value: object) { return value.apply(); }
"#,
        &[
            "unknownText : (value: unknown) => string",
            "unknownOwn : (value: unknown) => boolean",
            "unknownApply : (value: unknown) => any",
            "objectApply : (value: object) => any",
        ],
    );
}
