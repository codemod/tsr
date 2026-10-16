//! Contextual-return inference controls checked against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

/// Pinned tsgo 5b1047d1: a written return context supplies the generic call's
/// return mapper while its object, tuple, and callback arguments are checked.
/// The same literals without that context retain ordinary widening.
#[test]
fn contextual_return_mapper_reaches_argument_literal_members() {
    let source = r#"// @strict: true
// @target: es2015
interface Wrap<T> { value:T }
interface Box<T> { item:T }
declare function wrap<T>(value:T):Wrap<T>;
declare function box<T>(value:T):Box<T>;
function objectContext():Wrap<{kind:"ctx"}>{ return wrap({kind:"ctx"}); }
const objectNoContext=wrap({kind:"free"});
function tupleContext():Box<[string,number]>{ return box(["x",1]); }
const tupleNoContext=box(["x",1]);
interface Diagnostic { severity:1|2; message:string }
function callbackContext():Diagnostic[]{
  return [0].map((value:any)=>({severity:1,message:"m"}));
}
const callbackNoContext=[0].map((value:any)=>({severity:1,message:"m"}));
"#;
    let case = TestCase::parse("probe/contextual-return-inference", "contextual-return.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in [
        "wrap({kind:\"ctx\"}) : Wrap<{ kind: \"ctx\"; }>",
        "wrap({kind:\"free\"}) : Wrap<{ kind: string; }>",
        "box([\"x\",1]) : Box<[string, number]>",
        "box([\"x\",1]) : Box<(string | number)[]>",
        "[0].map((value:any)=>({severity:1,message:\"m\"})) : { severity: 1; message: string; }[]",
        "[0].map((value:any)=>({severity:1,message:\"m\"})) : { severity: number; message: string; }[]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// Native 5b1047d1 infers the returned function object before demanding its
/// return. Its eventual contextual signature is its own original signature;
/// a written broad return or no context must still widen.
#[test]
fn returned_callables_keep_literals_only_in_their_resolved_context() {
    let source = r"// @strict: true
// @target: es2015
declare function useState<S>(initial: (() => S) | (S extends (() => any) ? never : S)): S;
declare function ordinary<T>(initial: () => T): T;
export const numeric = useState(() => () => 0);
export const boolean = ordinary(() => () => false);
export const block = useState(() => () => { return 0; });
export const outerBlock = ordinary(() => { return () => 0; });
export const outerFunction = ordinary(function () { return () => 0; });
export const innerFunction = ordinary(() => function () { return 0; });
export const broad = ordinary<() => number>(() => () => 0);
export const unknown = ordinary<() => unknown>(() => () => 0);
export const any = ordinary<() => any>(() => () => false);
export const standalone = () => 0;
export const freeNested = () => () => 0;
export const conditional = (flag: boolean) => flag ? 0 : 1;
export const wrongLiteral: () => 1 = useState(() => () => 0);
export const wrongPrimitive: () => string = ordinary(() => () => false);
export const wrongParameter: (value: string) => number = ordinary(() => (value: number) => 0);
declare function accept(factory: () => () => string): void;
declare function accept(factory: () => () => number): void;
accept(() => () => true);
";
    let case = TestCase::parse("probe/returned-callable-literals", "returned-callables.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in [
        "numeric : () => 0",
        "boolean : () => false",
        "block : () => 0",
        "outerBlock : () => 0",
        "outerFunction : () => 0",
        "innerFunction : () => 0",
        "broad : () => number",
        "unknown : () => unknown",
        "any : () => any",
        "standalone : () => number",
        "freeNested : () => () => number",
        "conditional : (flag: boolean) => 0 | 1",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
    let diagnostics: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| diagnostic.code)
        .collect();
    assert_eq!(diagnostics, [2322, 2322, 2322, 2769]);
}

#[test]
fn returned_callable_completion_reports_genuine_return_cycles_without_printing() {
    let source = r"// @strict: true
declare function ordinary<T>(initial: () => T): T;
export function recursiveNumber(flag: boolean) { return flag ? 0 : recursiveNumber(false); }
export function recursiveReturned() { return ordinary(() => () => recursiveReturned()()); }
";
    let case =
        TestCase::parse("probe/returned-callable-cycles", "returned-callable-cycles.ts", source);
    let diagnostics = tsr_conformance::diagnostics_suite::reported_for(&case);
    assert!(diagnostics.iter().any(|diagnostic| diagnostic.line == 2 && diagnostic.code == 7023));
    let anonymous_cycles: Vec<_> = diagnostics
        .iter()
        .filter(|diagnostic| diagnostic.code == 7024)
        .map(|diagnostic| (diagnostic.line, diagnostic.column))
        .collect();
    // Native reports only the inner returned callback, not its outer factory.
    assert_eq!(anonymous_cycles, [(3, 61)]);
}
