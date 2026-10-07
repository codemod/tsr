//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/declared_computed_members", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

#[test]
fn declared_computed_keys() {
    expect(
        r"// @strict: true
// @target: esnext
enum Key { Bare='bare', Quoted='a b', Numeric='123123' }
const negative = -2;
const fraction = 1.5;
declare const unique: unique symbol;
interface Source {
 [Key.Bare]: number;
 [Key.Quoted]: string;
 [Key.Numeric]: boolean;
 [negative]: Date;
 [fraction]: number;
 [unique]: boolean;
}
declare const source: Source;
export const bare = source.bare;
export const quoted = source['a b'];
export const numeric = source[123123];
export const negativeRead = source[-2];
export const fractionRead = source[1.5];
export const uniqueRead = source[unique];
export const copy = {...source};
export class Base {
 [Key.Bare] = 1;
 [Key.Quoted] = 'x';
 static [Key.Numeric] = true;
}
export class Derived extends Base {}
declare const derived: Derived;
export const inherited = derived['a b'];
export const staticRead = Derived['123123'];
export const instanceCopy = {...derived};
export const staticCopy = {...Derived};
",
        &[
            "bare : number",
            "quoted : string",
            "numeric : boolean",
            "negativeRead : Date",
            "fractionRead : number",
            "uniqueRead : boolean",
            "copy : { bare: number; \"a b\": string; \"123123\": boolean; [-2]: Date; 1.5: number; [unique]: boolean; }",
            "inherited : string",
            "staticRead : boolean",
            "instanceCopy : { bare: number; \"a b\": string; }",
        ],
    );
}

#[test]
fn nonlocal_enum_key_spread() {
    expect(
        r"// @module: commonjs
// @target: es2015
// @declaration: true
// @filename: class.ts
export const enum TestEnum {
    Test1 = '123123',
    Test2 = '12312312312',
}

export interface ITest {
    [TestEnum.Test1]: string;
    [TestEnum.Test2]: string;
}

export class A {
    getA(): ITest {
        return {
            [TestEnum.Test1]: '123',
            [TestEnum.Test2]: '123',
        };
    }
}
// @filename: index.ts
import { A } from './class';

export class B extends A {
    getA() { // TS4053 error
        return {
            ...super.getA(),
            a: '123',
        };
    }
}",
        &["getA : () => { \"123123\": string; \"12312312312\": string; a: string; }"],
    );
}

#[test]
fn static_and_instance_computed_accessors() {
    expect(
        r"// @strict: true
// @target: esnext
const key = 'a b';
class Pair {
 get [key]() { return 1; }
 set [key](value: number) {}
 static get [key]() { return 'x'; }
 static set [key](value: string) {}
}
export const instance = new Pair();
export const instanceRead = instance[key];
export const staticRead = Pair[key];
export const instanceWrite = instance[key] = 2;
export const staticWrite = Pair[key] = 'y';
class Methods {
 [key](value: number): number;
 [key](value: string): string;
 [key](value: number | string) { return value; }
 static [key](value: boolean) { return value; }
}
export const methodInstance = new Methods();
export const numberCall = methodInstance[key](1);
export const stringCall = methodInstance[key]('x');
export const staticCall = Methods[key](true);
",
        &[
            "instanceRead : number",
            "staticRead : string",
            "instanceWrite : 2",
            "staticWrite : \"y\"",
            "numberCall : number",
            "stringCall : string",
            "staticCall : boolean",
        ],
    );
}
