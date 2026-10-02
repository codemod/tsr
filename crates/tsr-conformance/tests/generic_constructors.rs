//! Generic construct signature inference compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn own_class_constructors_infer_before_defaults_and_preserve_contexts() {
    let source = r#"// @strict: true
// @target: es2020
export class Box<T=string>{constructor(public value:T){}}
export const numeric=new Box(1);
export const written=new Box<number>(1);
export const writtenMember=written.value;
export class Pair<T=string,U=T>{constructor(public first:T,public second?:U){}}
export const pair=new Pair(1);
export const partial=new Pair<number>(1);
export const explicit=new Pair<number,boolean>(1,true);
export class Callback<T=string,U=number>{constructor(public value:T,public map:(value:T)=>U){}}
export const callback=new Callback(1,value=>value>0);
export const writtenCallback=new Callback<number,boolean>(1,value=>value>0);
export class ConstBox<const T>{constructor(public value:T){}}
export const literal=new ConstBox({name:"a",tuple:[1,2]});
"#;
    let case = TestCase::parse("probe/class-constructor-contexts", "classes.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "numeric : Box<number>",
        "written : Box<number>",
        "writtenMember : number",
        "pair : Pair<number, number>",
        "partial : Pair<number, number>",
        "explicit : Pair<number, boolean>",
        "callback : Callback<number, boolean>",
        "writtenCallback : Callback<number, boolean>",
        "value=>value>0 : (value: number) => boolean",
        "literal : ConstBox<{ readonly name: \"a\"; readonly tuple: readonly [1, 2]; }>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

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

/// Pinned tsgo 5b1047d1: a single generic construct signature supplies the
/// tuple context of a non-context-sensitive array argument, as the call road
/// does (getContextualTypeForArgument treats `NewExpression` like a call).
#[test]
fn single_generic_construct_signatures_supply_argument_contexts() {
    let source = r#"// @strict: true
// @target: es2015
declare class C<T> { constructor(x: [T, string]); }
export const c = new C([1, "a"]);
interface CC { new <T>(x: [T, string]): C<T>; }
declare const cc: CC;
export const c2 = new cc([1, "a"]);
declare class DMap<K, V> { constructor(entries?: readonly (readonly [K, V])[] | null); }
export const d = new DMap([["1", 2]]);
"#;
    let case = TestCase::parse("probe/construct-contexts", "construct-contexts.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["c : C<number>", "c2 : C<number>", "d : DMap<string, number>"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

/// Pinned tsgo 5b1047d1: an overloaded construct set gives each generic
/// candidate's array-literal argument that candidate's tuple context
/// (inferTypeArguments re-checks arguments per candidate).
#[test]
fn overloaded_construct_candidates_supply_tuple_contexts() {
    let source = r#"// @strict: true
// @target: es2023
export const m = new Map([[Symbol("key"), "value"]]);
export const m2 = new Map([["a", 1]]);
declare const s: symbol;
export const w = new WeakMap([[s, false]]);
export const st = new Set([[1, "a"]]);
"#;
    let case = TestCase::parse("probe/overloaded-construct", "overloaded-construct.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "m : Map<symbol, string>",
        "m2 : Map<string, number>",
        "w : WeakMap<symbol, boolean>",
        "st : Set<(string | number)[]>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
