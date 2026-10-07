//! Captured mapped modifiers must override their declaration provenance.
use tsr_checker::relater::{Relation, Ternary};
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r"// @strict: true
// @target: es2015
interface Mutable { a: number }
interface Optional { a?: number }
interface ReadonlyMember { readonly a: number }
interface RequiredAny { a: any }
interface PrototypeRequirement { toString: any }
type MutableAgain<T> = { -readonly [P in keyof T]: T[P] };
declare let optional: Optional;
declare let mutable: Mutable;
declare let requiredOptional: Required<Optional>;
declare let partialMutable: Partial<Mutable>;
declare let readonlyMutable: Readonly<Mutable>;
declare let readonlyPartial: Readonly<Partial<Mutable>>;
declare let requiredReadonly: Required<Readonly<Optional>>;
declare let mutableAgain: MutableAgain<ReadonlyMember>;
declare let readonlyRoot: ReadonlyMember;
declare let requiredAny: RequiredAny;
declare let prototypeRequirement: PrototypeRequirement;
declare let empty: {};
export function readonlySourceReduced(flag: boolean) { return flag ? readonlyMutable : mutable; }
export function readonlyTargetReduced(flag: boolean) { return flag ? mutable : readonlyMutable; }
export function readonlyMappedTarget(flag: boolean) { return flag ? readonlyMutable : readonlyRoot; }
";

#[test]
fn mapped_optionality_overrides_origin_on_both_sides() {
    for exact in [false, true] {
        let source = format!("// @exactOptionalPropertyTypes: {exact}\n{SOURCE}");
        let case = TestCase::parse("probe/mapped-property-relations", "mapped.ts", &source);
        let arena = tsr_core::Arena::new();
        let program = types_producer::program_for_case(&arena, &case);
        let mut checker = types_producer::configured_checker(&program);
        let file = program.source_file("mapped.ts").expect("fixture");
        let mut types = std::collections::BTreeMap::new();
        for statement in file.source_file().statements {
            let tsr_ast::Statement::VariableStatement(statement) = statement else { continue };
            for declaration in statement.declaration_list.expect("declarations").declarations {
                let tsr_ast::BindingName::Identifier(name) = declaration.name.expect("name") else {
                    panic!("identifier");
                };
                let ty = checker.get_type_from_type_node(declaration.r#type.expect("annotation"));
                types.insert(name.text, ty);
            }
        }
        // Pinned native 5b1047d1 accepts Required<Optional> -> Mutable,
        // rejects both optional sources -> required targets, and accepts the
        // absent optional target property. RequiredAny masks value differences
        // to isolate the mandatory-property rule, including a composed map.
        for (source, target, expected) in [
            ("requiredOptional", "mutable", Ternary::Related),
            ("partialMutable", "mutable", Ternary::NotRelated),
            ("optional", "requiredOptional", Ternary::NotRelated),
            ("empty", "partialMutable", Ternary::Related),
            ("empty", "requiredOptional", Ternary::NotRelated),
            ("partialMutable", "requiredAny", Ternary::NotRelated),
            ("readonlyPartial", "requiredAny", Ternary::NotRelated),
            ("partialMutable", "prototypeRequirement", Ternary::Related),
            ("requiredReadonly", "mutable", Ternary::Related),
            ("mutableAgain", "mutable", Ternary::Related),
            ("readonlyMutable", "mutable", Ternary::Related),
            ("mutable", "readonlyMutable", Ternary::Related),
        ] {
            assert_eq!(
                checker.relate_ternary(types[source], types[target], Relation::Assignable),
                expected,
                "{source} -> {target}, exact={exact}"
            );
        }
    }
}

// Native 5b1047d10d32e7d5b446be4de56b126ff42f82bb in both exact modes.
// Non-alphabetical names discriminate native declaration order and the 5/6
// boundary. Global augmentation supplies source presence, not mapped own keys.
const MISSING_HEADS: &str = r#"interface Shape { z: number; a?: string; m?: boolean }
type Identity<T> = { [P in keyof T]: T[P] };
type Need<T> = { [P in keyof T]-?: T[P] };
type Maybe<T> = { [P in keyof T]?: T[P] };
type Frozen<T> = { readonly [P in keyof T]: T[P] };
interface One { z: number }
interface Two { z: number; a: string }
interface Five { z: number; a: string; m: boolean; b: number; q: string }
interface Six { z: number; a: string; m: boolean; b: number; q: string; c: boolean }
declare const empty: {};
declare const full: { z: number; a: string; m: boolean };
declare const shape: Shape;
declare const partial: Maybe<Shape>;
declare const required: Need<Shape>;
declare const nested: Frozen<Maybe<Shape>>;
declare let one: One;
declare let two: Two;
declare let five: Five;
declare let six: Six;
declare let need: Need<Shape>;
declare let maybe: Maybe<Shape>;
declare let identity: Identity<Shape>;
one = empty;
two = empty;
five = empty;
six = empty;
need = empty;
need = shape;
need = full;
maybe = empty;
identity = shape;
identity = required;
need = partial;
need = nested;
declare let prototype: { toString: any };
prototype = empty;
prototype = partial;
interface Object { globalPresence: any }
declare let global: { globalPresence: any };
global = empty;
global = nested;
declare let ownGlobal: Need<{ globalPresence?: any; z?: any }>;
ownGlobal = empty;
declare let ownString: Need<{ toString?: any; z?: any }>;
ownString = empty;
declare let nestedNeed: Need<Maybe<Shape>>;
nestedNeed = empty;
nestedNeed = full;
declare let explicit: Need<{ z: any; a?: string }>;
declare const explicitFull: { z: undefined; a: string };
explicit = explicitFull;
declare let optionalTarget: Maybe<{ z: number; a: string }>;
optionalTarget = empty;
declare const list: number[];
declare const tuple: [number];
declare let sequence: [number, number];
sequence = list;
sequence = tuple;
interface Base { b: number; z: number; q: string }
interface Derived extends Base { m: boolean; z: number; a: string }
declare let derived: Derived;
derived = empty;
declare let composedDerived: Need<Maybe<Derived>>;
composedDerived = empty;
interface Merged { q: string; z: number }
interface Merged { a: boolean; q: string; m: number }
declare let composedMerged: Need<Maybe<Merged>>;
composedMerged = empty;
need = { z: 1 };
need = { z: 1, a: "ok" };
need = { z: 1, a: "ok", m: true };
maybe = {};
two = {};
six = {};
prototype = {};
global = {};
class Hidden { private secret = 0; visible = 0; protected guard = 0; #hash = 0; }
declare let mappedHidden: Need<Hidden>;
mappedHidden = {};
"#;

fn missing_heads_case(exact: bool) -> TestCase {
    TestCase::parse(
        "probe/mapped-missing-heads",
        "missing.ts",
        &format!(
            "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{MISSING_HEADS}"
        ),
    )
}

#[test]
fn complete_mapped_tables_select_missing_heads_without_losing_prototype_presence() {
    for exact in [false, true] {
        let mut actual =
            tsr_conformance::diagnostics_suite::reported_for(&missing_heads_case(exact));
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
            [
                (23, 2741),
                (24, 2739),
                (25, 2739),
                (26, 2740),
                (27, 2739),
                (28, 2322),
                (33, 2322),
                (34, 2322),
                (43, 2741),
                (45, 2741),
                (47, 2739),
                (57, 2322),
                (58, 2322),
                (62, 2739),
                (64, 2739),
                (68, 2739),
                (69, 2739),
                (70, 2741),
                (73, 2739),
                (74, 2740),
                (79, 2741),
            ]
            .map(|(line, code)| ("missing.ts", line, 1, code)),
            "exact={exact}"
        );
    }
}

#[test]
fn missing_heads_keep_native_order_and_six_property_truncation() {
    let case = missing_heads_case(true);
    let arena = tsr_core::Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let mut checker = types_producer::configured_checker(&program);
    let file = program.source_file("missing.ts").expect("fixture");
    let id = file.source_file().node_id.expect("registered file");
    checker.set_checked_files([id]);
    checker.check_source_file(
        id,
        tsr_checker::check::FileContext { ambient: false, has_parse_errors: false },
    );
    let messages: Vec<_> = checker
        .diagnostics()
        .iter()
        .filter(|(_, diagnostic)| [2739, 2740, 2741].contains(&diagnostic.message.code()))
        .map(|(_, diagnostic)| {
            let (line, _) = tsr_conformance::symbols_baseline::line_and_character(
                file.text(),
                diagnostic.span.start,
            );
            let args: Vec<_> = diagnostic.args.iter().map(String::as_str).collect();
            (line + 1, diagnostic.message.format(&args))
        })
        .collect();
    for (line, expected) in [
        (23, "Property 'z' is missing in type '{}' but required in type 'One'."),
        (24, "Type '{}' is missing the following properties from type 'Two': z, a"),
        (25, "Type '{}' is missing the following properties from type 'Five': z, a, m, b, q"),
        (
            26,
            "Type '{}' is missing the following properties from type 'Six': z, a, m, b, and 2 more.",
        ),
        (27, "Type '{}' is missing the following properties from type 'Need<Shape>': z, a, m"),
        (
            47,
            "Type '{}' is missing the following properties from type 'Need<Maybe<Shape>>': z, a, m",
        ),
        (62, "Type '{}' is missing the following properties from type 'Derived': m, z, a, b, q"),
        (
            64,
            "Type '{}' is missing the following properties from type 'Need<Maybe<Derived>>': b, q, m, z, a",
        ),
        (
            68,
            "Type '{}' is missing the following properties from type 'Need<Maybe<Merged>>': q, z, a, m",
        ),
        (
            69,
            "Type '{ z: number; }' is missing the following properties from type 'Need<Shape>': a, m",
        ),
        (
            70,
            "Property 'm' is missing in type '{ z: number; a: string; }' but required in type 'Need<Shape>'.",
        ),
        (79, "Property 'visible' is missing in type '{}' but required in type 'Need<Hidden>'."),
    ] {
        assert!(
            messages.iter().any(|(at, text)| *at == line && text == expected),
            "{line}: {messages:?}"
        );
    }
}

#[test]
fn mapped_readonly_orders_subtype_reduction_in_both_directions() {
    for exact in [false, true] {
        let source = format!("// @exactOptionalPropertyTypes: {exact}\n{SOURCE}");
        let case = TestCase::parse("probe/mapped-readonly-reduction", "mapped.ts", &source);
        let expected = vec![FileTypes { file: "mapped.ts".to_owned(), assertions: Vec::new() }];
        let rows = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
        for name in ["readonlySourceReduced", "readonlyTargetReduced", "readonlyMappedTarget"] {
            let row =
                rows.iter().flatten().find(|row| row.text == name).expect("function assertion");
            assert_eq!(row.type_string, "(flag: boolean) => Readonly<Mutable>", "{name}");
        }
    }
}

const GUARDED_HEADS: &str = r#"interface Shape { z: number; a?: string }
type Need<T> = { [P in keyof T]-?: T[P] };
declare let need: Need<Shape>;
declare const inherited: { foreign: number };
need = { ...inherited };
need = { ...inherited, z: 1 };
need = { foreign: 1, ...inherited };
need = { ...inherited, foreign: 1 };
need = { z: 1, forign: 1 };
need = { zz: 1 };
declare let indexed: { z: number; [key: string]: number };
indexed = { foreign: 1 };
declare let numeric: { z: number; [key: number]: number };
numeric = { 1: 1 };
numeric = { foreign: 1 };
declare let prototype: { z: number };
prototype = { toString: () => "" };
interface Object { globalPresence: number }
prototype = { globalPresence: 1 };
class OptionalParameter { constructor(public z?: number) {} }
class RequiredParameter { constructor(public z: number) {} }
class DefaultedParameter { constructor(public z = 0) {} }
declare const empty: {};
declare let optionalParameter: OptionalParameter;
declare let requiredParameter: RequiredParameter;
declare let defaultedParameter: DefaultedParameter;
optionalParameter = {};
optionalParameter = empty;
requiredParameter = empty;
defaultedParameter = empty;
"#;

#[test]
fn missing_heads_require_known_written_keys_and_certified_parameter_metadata() {
    for exact in [false, true] {
        let case = TestCase::parse(
            "probe/guarded-missing-heads",
            "guarded.ts",
            &format!(
                "// @strict: true\n// @target: es2015\n// @exactOptionalPropertyTypes: {exact}\n{GUARDED_HEADS}"
            ),
        );
        // Native 5b1047d: spread-only foreign keys do not take the excess path,
        // but a written foreign key does. Index keys are known; global Object
        // augmentation supplies source presence, not known fresh target keys.
        // This pins only missing-head selection: excess spelling/shorthand
        // elaboration and native-valid optional-parameter assignment acceptance
        // remain independent gaps, whose full diagnostic identities are audited.
        let mut heads: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
            .into_iter()
            .filter(|diagnostic| [2739, 2740, 2741].contains(&diagnostic.code))
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        heads.sort_unstable();
        assert_eq!(
            heads,
            [
                (5, 1, 2739),
                (6, 1, 2741),
                (7, 1, 2739),
                (12, 1, 2741),
                (14, 1, 2741),
                (29, 1, 2741),
                (30, 1, 2741),
            ],
            "exact={exact}"
        );
    }
}
