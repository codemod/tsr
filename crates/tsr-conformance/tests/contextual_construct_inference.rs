//! Contextual construct inference checked against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn contextual_construct_inference_uses_rest_inputs_and_propagates_generics() {
    let source = r#"// @target: es2015
// @strict: true

// Repros from #32976

declare class Banana<T extends string> { constructor(a: string, property: T) }

declare function fruitFactory1<TFruit>(Fruit: new (...args: any[]) => TFruit): TFruit
export const banana1 = fruitFactory1(Banana) // Banana<any>

declare function fruitFactory2<TFruit>(Fruit: new (a: string, ...args: any[]) => TFruit): TFruit
export const banana2 = fruitFactory2(Banana) // Banana<any>

declare function fruitFactory3<TFruit>(Fruit: new (a: string, s: "foo", ...args: any[]) => TFruit): TFruit
export const banana3 = fruitFactory3(Banana) // Banana<"foo">

declare function fruitFactory4<TFruit>(Fruit: new (a: string, ...args: "foo"[]) => TFruit): TFruit
export const banana4 = fruitFactory4(Banana) // Banana<"foo">

declare function fruitFactory5<TFruit>(Fruit: new (...args: "foo"[]) => TFruit): TFruit
export const banana5 = fruitFactory5(Banana) // Banana<"foo">

export declare class Bag<T> { constructor(...args:T[]); contains(value:T):boolean; static tag:string; }
export declare function asFunction<A extends any[], B>(cf:new(...args:A)=>B):(...args:A)=>B;
export const newBag = asFunction(Bag);
export const bag = newBag("a", "b");

export declare class Comp<P> { props:P; constructor(props:P); }
export type CompClass<P> = new(props:P)=>Comp<P>;
export declare function myHoc<P>(C:CompClass<P>):CompClass<P>;
export type GenericProps<T> = {foo:number;stuff:T};
export declare class GenericComp<T> extends Comp<GenericProps<T>> {}
export const GenericComp2 = myHoc(GenericComp);

export declare const genericCall: {<T>(x:T):T;tag:string;[key:string]:unknown};
export declare function wrapFunction<A,B>(f:(x:A)=>B):(x:A)=>B;
export declare function wrapNullable<A,B>(f:((x:A)=>B)|undefined):(x:A)=>B;
export declare function wrapMember<A,B>(f:{(x:A):B;tag?:string}):(x:A)=>B;
export declare function wrapIndexed<A,B>(f:{(x:A):B;[key:string]:unknown}):(x:A)=>B;
export const sourceMembers = wrapFunction(genericCall);
export const nullable = wrapNullable(genericCall);
export const targetMember = wrapMember(genericCall);
export const targetIndex = wrapIndexed(genericCall);

export declare const hybrid: {<T>(x:T):T;new<T>(x:T):T};
export const bothKinds = wrapFunction(hybrid);
"#;
    let case = TestCase::parse("probe/contextual-construct-inference", "construct.ts", source);
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
        "banana1 : Banana<any>",
        "banana2 : Banana<any>",
        "banana3 : Banana<\"foo\">",
        "banana4 : Banana<\"foo\">",
        "banana5 : Banana<\"foo\">",
        "newBag : <T>(...args: T[]) => Bag<T>",
        "bag : Bag<string>",
        "GenericComp2 : new <T>(props: GenericProps<T>) => Comp<GenericProps<T>>",
        "sourceMembers : <T>(x: T) => T",
        "nullable : <T>(x: T) => T",
        "targetMember : (x: unknown) => unknown",
        "targetIndex : (x: unknown) => unknown",
        "bothKinds : (x: unknown) => unknown",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
