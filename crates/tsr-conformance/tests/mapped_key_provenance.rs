//! resolveMappedTypeMembers reads semantic property key types from source origins.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @target: es2020
declare const computed: 13;
interface Numeric {
    42: string;
    "12": number;
    [-3]: bigint;
    [13]: boolean;
    "08": Date;
    2.0: null;
    readonly 0?: "optional";
}
interface Quoted { "42": string; "2": null }
interface LateBound { [computed]: boolean }
type Project<T> = { [K in keyof T]: { key: K; value: T[K] } };
declare const numeric: Project<Numeric>;
declare const quoted: Project<Quoted>;
declare const lateBound: Project<LateBound>;
const unresolvedKey = lateBound[13].key;
const numberKey = numeric[42].key;
const stringKey = quoted["42"].key;
const quotedIndex = numeric["12"].key;
const negativeKey = numeric[-3].key;
const computedKey = numeric[13].key;
const nonCanonicalKey = numeric["08"].key;
const canonicalKey = numeric[2].key;
const canonicalQuotedKey = quoted["2"].key;
const numberValue = numeric[42].value;
const quotedValue = numeric["12"].value;
const negativeValue = numeric[-3].value;
const maybe = numeric[0];
type Second<T> = { [K in keyof T]: { key: K; prior: T[K] } };
declare const nested: Second<Project<Numeric>>;
const nestedKey = nested[42].key;
const priorKey = nested[42].prior.key;
type Combined<T> = { [K in keyof T as "all"]: { key: K; value: T[K] } };
declare const combined: Combined<{ 7: string; "09": number }>;
const collisionKeys = combined.all.key;
const collisionValues = combined.all.value;
type Indexes = { [key: string]: number; [key: number]: 1 | 2 };
declare const indexes: Project<Indexes>;
const stringDomain = indexes.text.key;
const numberDomain = indexes[8].key;
const numberIndexValue = indexes[8].value;
"#;

#[test]
fn numeric_and_quoted_names_keep_distinct_iteration_types() {
    assert_types(&[
        "numberKey : 42",
        "stringKey : \"42\"",
        "quotedIndex : \"12\"",
        "negativeKey : -3",
        "computedKey : 13",
        "nonCanonicalKey : \"08\"",
        "canonicalKey : 2",
        "canonicalQuotedKey : \"2\"",
        "numberValue : string",
        "quotedValue : number",
        "negativeValue : bigint",
        "maybe : { key: 0; value: \"optional\" | undefined; } | undefined",
    ]);
}

#[test]
fn mapped_origins_survive_nested_images_and_remapped_collisions() {
    assert_types(&[
        "nestedKey : 42",
        "priorKey : 42",
        "collisionKeys : \"09\" | 7",
        "collisionValues : string | number",
    ]);
}

#[test]
fn index_signature_keys_remain_domains_not_property_name_literals() {
    assert_types(&["stringDomain : string", "numberDomain : number", "numberIndexValue : 1 | 2"]);
}

#[test]
fn unenumerated_late_bound_members_still_decline() {
    // Native resolves this key to 13, but the port's source member enumeration
    // has not admitted the declared-identifier name. This consumer must not
    // fabricate that producer's missing member or expand helper ownership.
    assert_types(&["unresolvedKey : error"]);
}

fn assert_types(wanted: &[&str]) {
    let case = TestCase::parse("probe/mapped-key-provenance", "keys.ts", SOURCE);
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
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
