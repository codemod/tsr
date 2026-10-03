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
