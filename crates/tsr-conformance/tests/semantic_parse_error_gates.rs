//! Pinned semantic error gates are per node, not a file-wide syntax decline.
//!
//! Native 5b1047d10d32e7d5b446be4de56b126ff42f82bb controls call both Program
//! syntactic and semantic diagnostic APIs, as the baseline harness does.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/semantic-gates", "gates.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "false".into());
    case.options.insert("usedefineforclassfields".into(), "false".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn native_super_constructor_and_member_errors_survive_an_unrelated_parse_error() {
    let valid = "class Base { value() {} static staticValue() {} }
class Plain {
    constructor() { super(); }
    method() { super(); }
}
class Derived extends Base {
    constructor() { super(); }
    method() { super(); super.staticValue; super.missing; }
}
class Missing extends Base { constructor() {} }
class Signature extends Base { constructor(); }";
    let malformed = "class Base { value() {} static staticValue() {} }
class Plain {
    constructor() { super(); }
    method() { super(); }
}
class Derived extends Base {
    constructor() { super(); }
    method() { super(); super.staticValue; super.missing; }
    recovery() { super; }
}
class Missing extends Base { constructor() {} }
class Signature extends Base { constructor(); }";
    assert_eq!(
        diagnostics(valid),
        [
            (3, 21, 2335),
            (4, 16, 2337),
            (8, 16, 2337),
            (8, 31, 2576),
            (8, 50, 2339),
            (10, 30, 2377),
            (11, 32, 2390),
        ],
    );
    assert_eq!(
        diagnostics(malformed),
        [
            (3, 21, 2335),
            (4, 16, 2337),
            (8, 16, 2337),
            (8, 31, 2576),
            (8, 50, 2339),
            (9, 23, 1034),
            (11, 30, 2377),
            (12, 32, 2390),
        ],
    );
}

#[test]
fn missing_property_names_decline_without_silencing_real_member_errors() {
    let source = "class Declared { actual = 1; static staticValue() {} }\n\
declare const receiver: Declared;\n\
receiver.;\n\
receiver.actual;\n\
receiver.missing;\n\
receiver.staticValue;\n\
receiver?.actual;";
    assert_eq!(diagnostics(source), [(3, 10, 1003), (5, 10, 2339), (6, 10, 2576)]);
}

#[test]
fn instantiation_expression_receivers_are_values_but_their_type_arguments_are_not() {
    let source = "interface TypeOnly<T> { value: T; }\n\
interface Shape extends TypeOnly<number> {}\n\
class Implements implements TypeOnly<number> { value = 1; }\n\
declare function f<T>(): T;\n\
f<TypeOnly<number>>;\n\
Missing<number> || f<number>;\n\
MissingProperty<number>.member;\n\
type Alias = TypeOnly<number>;";
    let diagnostics = diagnostics(source);
    // Native also reports grammar TS1477 at (7,16), an existing parser gap.
    // This consumer gate checks the semantic identifier positions independently.
    assert_eq!(
        diagnostics.iter().copied().filter(|(_, _, code)| *code == 2304).collect::<Vec<_>>(),
        [(6, 1, 2304), (7, 1, 2304)],
    );
    assert!(diagnostics.iter().all(|(_, _, code)| *code != 2693), "type-only use became a value");
}

#[test]
fn missing_constructor_bodies_do_not_report_the_empty_body_super_error() {
    let source = "class Base {}\n\
class EmptyBody extends Base { constructor() {} }\n\
class SignatureOnly extends Base { constructor(); }\n\
class Valid extends Base { constructor() { super(); } }\n\
class Malformed extends Base { constructor() , }";
    // Native reports TS2377 only for the written, empty body. Signature and
    // missing-brace bodies decline; other recovery diagnostics are not this rule.
    assert_eq!(
        diagnostics(source).into_iter().filter(|(_, _, code)| *code == 2377).collect::<Vec<_>>(),
        [(2, 32, 2377)],
    );
}

#[test]
fn recovery_misses_require_checked_value_roles_and_complete_receiver_ownership() {
    // Expected TS2339 positions come from fresh pinned Program semantic APIs.
    // Unrelated parser recovery differences remain outside this consumer gate.
    let heritage = "namespace Names { export interface Shape {} export class Base {} }
interface Typed extends Names.Shape {}
class Implements implements Names.Shape {}
class Derived extends Names.Missing {}
declare const value: Names.Base;
value.missing;
interface Generic<T> {}
interface Other extends Generic<typeof missingValue> {}
class Recovery extends Names.Base { method() { super; } }";
    // The heritage-name decline must not suppress a type argument's value slot.
    assert!(diagnostics(heritage).contains(&(8, 40, 2304)));
    for (source, expected) in [
        (heritage, &[(4, 29, 2339), (6, 7, 2339)][..]),
        (
            "class Known { actual = 1; constructor() { this.actual = 2; this.missing; } }
class Recovered { constructor() { public this.p1 = 0; } }",
            &[(1, 65, 2339)][..],
        ),
        (
            "declare namespace stable.child { var actual: number; }
export const ok = stable.child.actual;
export const wrong = stable.child.missing;
declare namespace recovered.debugger { var actual: number; }
export const unknown = recovered.debugger.actual;",
            &[(3, 35, 2339)][..],
        ),
        (
            "class Base { assertDerived(): asserts this is Derived {} }
class Derived extends Base { z = 1; }
function typed(receiver: Base) { receiver.missing; }
function untyped(values: Base[]) { for (let receiver of values) { receiver.assertDerived(); receiver.z; } }
class Recovery extends Base { method() { super; } }",
            &[(3, 43, 2339)][..],
        ),
    ] {
        assert_eq!(
            diagnostics(source).into_iter().filter(|(_, _, code)| *code == 2339).collect::<Vec<_>>(),
            expected,
            "{source}",
        );
    }
}
