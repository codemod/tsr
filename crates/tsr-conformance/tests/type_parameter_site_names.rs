//! Type-parameter names at a print site (`typeParameterToName`,
//! `nodebuilderimpl.go:1404`; `docs/parity/notes/r7-printer.md` §33, §34).
//! Every expected line was read from pinned native tsgo 5b1047d on the same
//! source.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/type_parameter_site_names", "a.ts", source);
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

fn assert_has(lines: &[String], wanted: &[&str]) {
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// An instantiated object image prints its members at the site, inside the
/// enclosing render's names: `c`'s renamed `T_1` is taken, so the inner
/// `a<T>` is `T_2` (`typeParametersAvailableInNestedScope3`).
#[test]
fn a_nested_signature_takes_the_next_name_after_its_enclosing_render() {
    let lines = lines(
        r"// @target: es2015
function foo<T>(v: T) {
    function a<T>(a: T) { return a; }
    function c<T>(v: T) {
        function a<T>(a: T) { return a; }
        function b(): T { return v; }
        return { a, b };
    }
    return { a, c };
}
",
    );
    assert_has(
        &lines,
        &[
            "foo : <T>(v: T) => { a: <T_1>(a: T_1) => T_1; c: <T_1>(v: T_1) => { a: <T_2>(a: T_2) => T_2; b: () => T_1; }; }",
        ],
    );
}

/// An instantiated signature's type parameter is shadowed by its own
/// same-named value parameter (`inferredReturnTypeIncorrectReuse1`); the
/// declared signature and a differently named parameter are not.
#[test]
fn an_instantiated_type_parameter_named_like_its_parameter_is_renamed() {
    let lines = lines(
        r"// @target: es2015
// @strict: true
interface C<t> { m<fn extends (In: t) => unknown>(fn: fn): fn }
interface D<t> { m<fn extends (In: t) => unknown>(x: fn): fn }
declare function f<fn>(fn: fn): fn;
declare const c: C<string>; declare const d: D<string>;
c.m; d.m; f;
",
    );
    assert_has(
        &lines,
        &[
            "c.m : <fn_1 extends (In: string) => unknown>(fn: fn_1) => fn_1",
            "d.m : <fn extends (In: string) => unknown>(x: fn) => fn",
            "f : <fn>(fn: fn) => fn",
        ],
    );
}
