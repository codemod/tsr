//! Type-parameter names at a print site (`typeParameterToName`,
//! `nodebuilderimpl.go:1404`; `docs/parity/notes/r7-printer.md` §33, §34, §46).
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

/// A shadowed overload whose substitution declines still renames its
/// declaration, matching its by-id slots (`inferenceContextualReturnTypeUnion3`;
/// r7-printer §37).
#[test]
fn a_renamed_overload_declares_the_name_its_slots_use() {
    let lines = lines(
        r#"// @target: es2015
// @strict: true
// @lib: esnext
declare function deprecate<T extends Function>(fn: T, msg: string): T;
const f = <T extends object>(obj: T): T => {
  return new Proxy(obj, {
    set: deprecate((target, property, value, receiver) => Reflect.set(target, property, value, receiver), "m"),
  });
};
"#,
    );
    assert_has(
        &lines,
        &[
            "Reflect.set : { <T_1 extends object, P extends PropertyKey>(target: T_1, propertyKey: P, value: P extends keyof T_1 ? T_1[P] : any, receiver?: any): boolean; (target: object, propertyKey: PropertyKey, value: any, receiver?: any): boolean; }",
        ],
    );
}

/// r7-printer §46: a reused node's own type-parameter declarations are
/// renamed where the render already holds the name (native-read lines).
#[test]
fn a_reused_generic_annotation_renames_a_taken_name() {
    let lines = lines(
        r"function foo4<T extends <T>(x: T) => void>(x: T): T { return null as any; }
function h<T>(cb: <T>(x: T) => T): T { return null as any; }
",
    );
    assert_has(
        &lines,
        &[
            "foo4 : <T extends <T_1>(x: T_1) => void>(x: T) => T",
            "h : <T>(cb: <T_1>(x: T_1) => T_1) => T",
        ],
    );
}

/// r7-printer §48: a reused indexed access keeps a written type-query
/// operand unparenthesized (`emitPostfixTypeOperand`; native-read lines).
#[test]
fn a_reused_type_query_operand_keeps_its_spelling() {
    let lines = lines(
        r"const a = { a: 1 } as const;
export const o2 = (o: (typeof a)['a']) => {};
export const o3 = (o:  typeof a['a']) => {};
",
    );
    assert_has(&lines, &["o2 : (o: (typeof a)['a']) => void", "o3 : (o: typeof a['a']) => void"]);
}

/// r7-printer §55: a merged interface's type parameter declared by another
/// declaration shadows at the site (native-read line).
#[test]
fn a_merged_declarations_type_parameter_shadows_a_global() {
    let lines = lines(
        r"interface A { a: number; }
interface i07 { a: A; }
interface i07<A = number> { b: A; }
",
    );
    assert_has(&lines, &["a : globalThis.A", "b : A"]);
}
