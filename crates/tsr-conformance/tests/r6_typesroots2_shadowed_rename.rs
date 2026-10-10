//! `typeParameterToName`'s shadow rename (`nodebuilderimpl.go:1404`), resolved
//! from the node builder's enclosing declaration with `enterNewScope`'s
//! synthesized signature scopes, and `resolveNameHelper`'s function-like and
//! computed-name visibility rules (`binder/nameresolver.go:54-70, 216`).
//! `docs/parity/notes/r6-typesroots2.md` §5.
//!
//! Every expectation is a line of the pinned baseline
//! (`testdata/baselines/reference/submodule/…` @ 5b1047d) named at each test.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/r6_typesroots2_shadowed_rename", "probe.ts", source);
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

fn expect(source: &str, wanted: &[&str]) {
    let lines = lines(source);
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

/// `conformance/subtypesOfTypeParameter.types`: an arrow's own `T` printed
/// where the enclosing function's `T` is in scope is renamed.
#[test]
fn an_inner_signature_parameter_shadowed_at_the_site_is_renamed() {
    expect(
        r"// @target: es2015
function f<T>(x: T) {
    var r8 = true ? <T>(x: T) => { return x } : x;
}
",
        &["r8 : T | (<T_1>(x: T_1) => T_1)", "<T>(x: T) => { return x } : <T_1>(x: T_1) => T_1"],
    );
}

/// `conformance/computedPropertyNames32_ES6.types`: a class's type
/// parameters are not in scope in its member's computed name, so `foo`'s own
/// `T` prints as written there.
#[test]
fn a_computed_name_does_not_see_its_class_type_parameters() {
    expect(
        r"// @target: es2015
function foo<T>() { return '' }
class C<T> {
    bar() {
        return 0;
    }
    [foo<T>()]() { }
}
",
        &["foo : <T>() => string"],
    );
}

/// `compiler/typeParametersAndParametersInComputedNames.types`: nor are the
/// method's own type parameters.
#[test]
fn a_computed_name_does_not_see_its_method_type_parameters() {
    expect(
        r"// @target: es2015
function foo<T>(a: T) : string {
    return '';
}
class A {
    [foo<T>(a)]<T>(a: T) {
    }
}
",
        &["foo : <T>(a: T) => string"],
    );
}

/// `compiler/declarationEmitTypeParameterNameInOuterScope.types`: inside the
/// printed signature's scope `A` is its type parameter, so the class `A`
/// takes its `globalThis.` qualifier, at the declaration's name and at the
/// arrow expression alike.
#[test]
fn a_signature_scope_shadows_an_outer_class_while_it_prints() {
    expect(
        r"// @target: es2015
class A { }
var a3 = <A,>(x: A) => new A();
function a4<A,>(x: A) { return new A() }
",
        &["a4 : <A>(x: A) => globalThis.A", "<A,>(x: A) => new A() : <A>(x: A) => globalThis.A"],
    );
}

/// `compiler/conditionalTypeAssignabilityWhenDeferred.types`: a renamed
/// parameter's written constraint is reused under the rename.
#[test]
fn a_renamed_parameter_keeps_its_written_constraint() {
    expect(
        r"// @target: es2015
declare function onlyNullablePlease2<
  T extends [null] extends [T] ? any : never
>(value: T): void;
function f<T>(t: T) {
  var x: T | null = Math.random() > 0.5 ? null : t;
  onlyNullablePlease2(x);
}
",
        &[
            "onlyNullablePlease2 : <T_1 extends [null] extends [T_1] ? any : never>(value: T_1) => void",
        ],
    );
}
