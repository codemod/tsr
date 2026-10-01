//! Generic construct signature inference compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn single_construct_signatures_infer_before_using_defaults() {
    let source = r#"// @strict: true
// @target: es2020
interface Box<T> { value:T; }
interface MakeBox { new<T=number>(value?:T):Box<T>; }
declare const MakeBox:MakeBox;
export const defaultBox=new MakeBox();
export const stringBox=new MakeBox("value");
export const explicitBox=new MakeBox<string>();
export const explicitArgument=new MakeBox<string>("value");
interface MakePair { new<T,U=T>(first:T,second?:U):[T,U]; }
declare const MakePair:MakePair;
export const pair=new MakePair(1);
export const mixedPair=new MakePair(1,"value");
export const partialPair=new MakePair<number>(1);
interface MakeCallback { new<T=number>(callback:()=>T):Box<T>; }
declare const MakeCallback:MakeCallback;
export const callbackBox=new MakeCallback(()=>"value");
export const contextualBox:Box<string>=new MakeBox();
"#;
    let case = TestCase::parse("probe/generic-constructors", "generic-constructors.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "defaultBox : Box<number>",
        "stringBox : Box<string>",
        "explicitBox : Box<string>",
        "explicitArgument : Box<string>",
        "pair : [number, number]",
        "mixedPair : [number, string]",
        "partialPair : [number, number]",
        "callbackBox : Box<string>",
        "contextualBox : Box<string>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn overloaded_and_contextual_construct_signatures_preserve_sources() {
    let source = r#"// @strict: true
// @target: es2020
interface Box<T> {value:T;}
interface MakeList {
 new<T=number>(values?:readonly T[]|null):T[];
 new<T=number>(values?:Iterable<T>|null):T[];
}
declare const MakeList:MakeList;
export const emptyList=new MakeList();
export const stringList=new MakeList(["a","b"]);
export const writtenList=new MakeList<string>();
interface MakeContextual {new<T>(callback:(value:T)=>T,initial:T):Box<T>;}
declare const MakeContextual:MakeContextual;
export const inferredCallback=new MakeContextual(value=>value,1);
interface MakeConst {new<const T>(value:T):Box<T>;}
declare const MakeConst:MakeConst;
export const constConstructor=new MakeConst({value:["a",1]});
"#;
    let case = TestCase::parse(
        "probe/generic-constructors-extra",
        "generic-constructors-extra.ts",
        source,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "emptyList : number[]",
        "stringList : string[]",
        "writtenList : string[]",
        "inferredCallback : Box<number>",
        "constConstructor : Box<{ readonly value: readonly [\"a\", 1]; }>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn written_constructor_parameters_contextually_type_callbacks() {
    let source = r"// @strict: true
// @target: es2020
interface Box<T> {value:T;}
interface MakeCallback {new<T=number>(callback:(value:T)=>T):Box<T>;}
declare const MakeCallback:MakeCallback;
export const explicitCallback=new MakeCallback<number>(value=>value);
export const optionalCallback=new MakeCallback<string>(value=>value);
";
    let case =
        TestCase::parse("probe/generic-written-callback", "generic-written-callback.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "explicitCallback : Box<number>",
        "optionalCallback : Box<string>",
        "value=>value : (value: number) => number",
        "value=>value : (value: string) => string",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn awaited_constructors_receive_the_awaited_return_context() {
    let source = r"// @strict: true
// @target: es2020
export async function awaited():Promise<{count:number}> {
 return await new Promise(resolve=>resolve({count:1}));
}
export async function immediate():Promise<{count:number}> {
 return new Promise(resolve=>resolve({count:1}));
}
export async function assigned() {
 const value:{count:number}=await new Promise(resolve=>resolve({count:1}));
 return value;
}
";
    let case = TestCase::parse("probe/await-constructor", "await-constructor.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    let wanted = "new Promise(resolve=>resolve({count:1})) : Promise<{ count: number; }>";
    assert_eq!(lines.iter().filter(|line| line.as_str() == wanted).count(), 3, "{lines:?}");
}
