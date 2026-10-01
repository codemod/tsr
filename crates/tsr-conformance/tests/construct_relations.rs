//! Construct relations checked against pinned tsgo declaration output.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn class_construct_relations_preserve_inheritance_and_visibility() {
    let source = r#"// @strict: true
// @target: es2020
declare function pick(value: new (x:number)=>{value:number}): "construct";
declare function pick(value:any): "fallback";
declare function pickAbstract(value: abstract new (x:number)=>{value:number}): "abstract";
declare function pickAbstract(value:any): "fallback";
export class Public {constructor(public value:number){}}
export class Private {private constructor(public value:number){}}
export class Protected {protected constructor(public value:number){}}
export abstract class Abstract {constructor(public value:number){}}
export class Inherited extends Public {}
export class Default {value=1;}
export class Generic<T> {constructor(public value:T){}}
export class GenericInherited extends Generic<number> {}
export const publicResult=pick(Public);
export const privateResult=pick(Private);
export const protectedResult=pick(Protected);
export const abstractResult=pick(Abstract);
export const inheritedResult=pick(Inherited);
export const defaultResult=pick(Default);
export const genericResult=pick(Generic);
export const genericInheritedResult=pick(GenericInherited);
export const abstractAccepted=pickAbstract(Abstract);
export const concreteAccepted=pickAbstract(Public);
export const privateAbstract=pickAbstract(Private);

class PrivateInherited extends Private {}
class ProtectedInherited extends Protected {}
class GenericDefault<T = number> { constructor(public value:T) {} }
class DefaultInherited extends GenericDefault { value = 0; }
class Overloaded { value = 0; constructor(x:number); constructor(x:string); constructor(x:unknown) {} }
class InheritedOverloaded extends Overloaded {}
class Wrong { constructor(public value:string) {} }
class StaticBase { static required = 1; value = 0; }
class StaticInherited extends StaticBase {}
class MissingStatic { value = 0; }
declare function pickStatic(value:typeof StaticBase):"static";
declare function pickStatic(value:any):"fallback";
declare function pickPrivate(value:typeof Private):"private";
declare function pickPrivate(value:any):"fallback";
declare function pickProtected(value:typeof Protected):"protected";
declare function pickProtected(value:any):"fallback";
export const inheritedPrivate = pick(PrivateInherited);
export const inheritedProtected = pick(ProtectedInherited);
export const defaultInherited = pick(DefaultInherited);
export const ownOverloads = pick(Overloaded);
export const inheritedOverloads = pick(InheritedOverloaded);
export const wrongInstance = pick(Wrong);
export const inheritedStatic = pickStatic(StaticInherited);
export const missingStatic = pickStatic(MissingStatic);
export const publicToPrivate = pickPrivate(Public);
export const protectedToPrivate = pickPrivate(Protected);
export const publicToProtected = pickProtected(Public);
export const privateToProtected = pickProtected(Private);
// @strict: true
// @target: es2020
class Required { static [Symbol.iterator]() {return 0;} }
class Same { static [Symbol.iterator]() {return 0;} }
class Missing {}
declare function select(value:typeof Required):"match";
declare function select(value:any):"fallback";
export const present = select(Same);
export const absent = select(Missing);
class InstanceOnly { [Symbol.iterator]() {return 0;} }
export const instanceOnly = select(InstanceOnly);
"#;
    let case = TestCase::parse("probe/construct-relations", "construct.ts", source);
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
    for (name, expected) in [
        ("publicResult", "construct"),
        ("privateResult", "fallback"),
        ("protectedResult", "fallback"),
        ("abstractResult", "fallback"),
        ("inheritedResult", "construct"),
        ("defaultResult", "construct"),
        ("genericResult", "construct"),
        ("genericInheritedResult", "construct"),
        ("abstractAccepted", "abstract"),
        ("concreteAccepted", "abstract"),
        ("privateAbstract", "fallback"),
        ("inheritedPrivate", "fallback"),
        ("inheritedProtected", "fallback"),
        ("defaultInherited", "construct"),
        ("ownOverloads", "construct"),
        ("inheritedOverloads", "construct"),
        ("wrongInstance", "fallback"),
        ("inheritedStatic", "static"),
        ("missingStatic", "fallback"),
        ("publicToPrivate", "private"),
        ("protectedToPrivate", "private"),
        ("publicToProtected", "protected"),
        ("privateToProtected", "fallback"),
        ("present", "match"),
        ("absent", "fallback"),
        ("instanceOnly", "fallback"),
    ] {
        let wanted = format!("{name} : \"{expected}\"");
        assert!(lines.iter().any(|line| line == &wanted), "missing {wanted}: {lines:?}");
    }
}
