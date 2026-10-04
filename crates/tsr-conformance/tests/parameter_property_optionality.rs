//! Constructor property metadata follows the written question token, not call optionality.
use tsr_conformance::{TestCase, diagnostics_suite};

const SOURCE: &str = r"class OptionalParameter { constructor(public z?: number) {} }
class RequiredParameter { constructor(public z: number) {} }
class DefaultedParameter { constructor(public z = 7) {} }
class ExplicitParameter { constructor(public z: number | undefined) {} }
class PlainParameter { constructor(z?: number) {} }
class InheritedOptional extends OptionalParameter {}
class PrivateOptional { constructor(private z?: number) {} }
class ProtectedOptional { constructor(protected z?: number) {} }
class GenericOptional<T> { constructor(public z?: T) {} }
declare const empty: {};
declare const optional: OptionalParameter;
declare const genericNumber: GenericOptional<number>;
declare const genericString: GenericOptional<string>;
declare let optionalTarget: OptionalParameter;
declare let requiredTarget: RequiredParameter;
declare let defaultedTarget: DefaultedParameter;
declare let explicitTarget: ExplicitParameter;
declare let requiredAny: { z: any };
optionalTarget = {};
optionalTarget = empty;
optionalTarget = { z: undefined };
requiredTarget = empty;
defaultedTarget = empty;
explicitTarget = empty;
explicitTarget = { z: undefined };
requiredAny = optional;
const inherited: InheritedOptional = empty;
const privateEmpty: PrivateOptional = empty;
const protectedEmpty: ProtectedOptional = empty;
const genericEmpty: GenericOptional<number> = empty;
const genericPresent: GenericOptional<number> = { z: 31 };
const genericWrong: GenericOptional<number> = genericString;
requiredAny = genericNumber;
const plainEmpty: PlainParameter = empty;
type Need<T> = { [P in keyof T]-?: T[P] };
type RebuiltNeed<T> = { [P in keyof T]-?: T[P] | undefined };
type RebuiltMaybe<T> = { [P in keyof T]?: T[P] | undefined };
const mappedRequiredEmpty: Need<OptionalParameter> = empty;
const rebuiltRequiredUndefined: RebuiltNeed<OptionalParameter> = { z: undefined };
const rebuiltOptionalEmpty: RebuiltMaybe<OptionalParameter> = empty;
optionalTarget.z = undefined;
function genericLocal<T>(source: GenericOptional<T>, value: T) {
    const absent: GenericOptional<T> = empty;
    const present: GenericOptional<T> = { z: value };
    const required: { z: any } = source;
}
";

#[test]
fn optional_constructor_fields_preserve_presence_and_required_source_rules() {
    // Pinned native 5b1047d10d32e7d5b446be4de56b126ff42f82bb. Required-any
    // targets mask value compatibility and expose optional-source metadata.
    // The synthesized map also pins the previously delivered exact-mode
    // value prerequisite when a newly recognized parameter origin is optional.
    for (strict, exact) in [(true, false), (true, true), (false, false)] {
        let case = TestCase::parse(
            "probe/parameter-property-optionality",
            "parameters.ts",
            &format!("// @strict: {strict}\n// @exactOptionalPropertyTypes: {exact}\n{SOURCE}"),
        );
        let actual: Vec<_> = diagnostics_suite::reported_for(&case)
            .into_iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        let mut expected = vec![
            (22, 1, 2741),
            (23, 1, 2741),
            (24, 1, 2741),
            (26, 1, 2322),
            (32, 7, 2322),
            (33, 1, 2322),
            (38, 7, 2741),
        ];
        if strict && !exact {
            expected.push((39, 68, 2322));
        }
        expected.push((45, 11, 2322));
        assert_eq!(actual, expected, "strict={strict}, exact={exact}");
    }
}
