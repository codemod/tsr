"""Public source controls for pinned member publication; no private code."""
def fixtures():
    cases = {}
    for count in (1, 32):
        cases[f"repeat-{count}"] = {"input.ts":
            "interface MC_Box<T> { value:T }\ndeclare const box:MC_Box<number>;\n"
            + "".join(f"const read{i}:number=box.value;\n" for i in range(count))}
    cases["argument-order"] = {"input.ts": """
interface MC_Box<T> { value:T }
declare const n:MC_Box<number>; declare const s:MC_Box<string>;
const n1:number=n.value; const s1:string=s.value;
const s2:string=s.value; const n2:number=n.value;
"""}
    cases["argument-reversed"] = {"input.ts": """
interface MC_Box<T> { value:T }
declare const s:MC_Box<string>; declare const n:MC_Box<number>;
const s1:string=s.value; const n1:number=n.value;
const n2:number=n.value; const s2:string=s.value;
"""}
    cases["argument-negative"] = {"input.ts": """
interface MC_Box<T> { value:T }
declare const n:MC_Box<number>; declare const s:MC_Box<string>;
const wrong:MC_Box<number>=s;
"""}
    cases["empty-interface"] = {"input.ts": """
interface MC_Empty {}
type MC_Keys=keyof MC_Empty;
const names:MC_Keys[]=[];
declare const empty:MC_Empty;
const first:{}=empty;const second:{}=empty;
"""}
    cases["empty-mapped"] = {"input.ts": """
type MC_Empty={ [P in never]:number };
type MC_Keys=keyof MC_Empty;
const names:MC_Keys[]=[];
declare const empty:MC_Empty;
const first:{}=empty;const second:{}=empty;
"""}
    cases["diamond-order"] = {"input.ts": """
interface MC_Root { base:number }
interface MC_Left extends MC_Root { left:string }
interface MC_Right extends MC_Root { right:boolean }
interface MC_Derived extends MC_Left,MC_Right { ownA:number;ownB:string }
declare const d:MC_Derived;
const a:number=d.ownA;const b:string=d.ownB;const c:number=d.base;
const e:string=d.left;const f:boolean=d.right;
type MC_Keys=keyof MC_Derived;const key:MC_Keys="base";
"""}
    cases["receiver-static"] = {"input.ts": """
class MC_Base { value=1;self():this{return this};static marker="static" }
class MC_Child extends MC_Base { extra="child" }
declare const child:MC_Child;
const a:MC_Child=child.self();const b:string=child.self().extra;
const c:string=MC_Child.marker;
"""}
    cases["receiver-negative"] = {"input.ts": """
class MC_Base { self():this{return this} }
class MC_Child extends MC_Base { extra="child" }
declare const base:MC_Base;
const wrong:MC_Child=base.self();
"""}
    cases["mapped-arguments"] = {"input.ts": """
type MC_Copy<T>={ [P in keyof T]:T[P] };
type MC_One=MC_Copy<{left:number}>;type MC_Two=MC_Copy<{right:string}>;
declare const one:MC_One;declare const two:MC_Two;
const a:number=one.left;const b:string=two.right;
type MC_K1=keyof MC_One;type MC_K2=keyof MC_Two;
const k1:MC_K1="left";const k2:MC_K2="right";
"""}
    cases["mapped-alias-negative"] = {"input.ts": """
type MC_Copy<T>={ [P in keyof T]:T[P] };
type MC_One=MC_Copy<{left:number}>;type MC_Two=MC_Copy<{right:string}>;
declare const two:MC_Two;const wrong:MC_One=two;
"""}
    cases["anonymous-signature-reentry"] = {"input.ts": """
function MC_fn():typeof MC_fn.field{return MC_fn.field}
namespace MC_fn { export const field=1 }
const a:number=MC_fn();const b:number=MC_fn.field;
"""}
    cases["mapped-cycle"] = {"input.ts": """
type MC_Cycle={ [P in keyof MC_Cycle]:MC_Cycle[P] };
declare const cycle:MC_Cycle;const a=cycle;
"""}
    cases["inheritance-cycle"] = {"input.ts": """
interface MC_Left extends MC_Right { left:number }
interface MC_Right extends MC_Left { right:string }
declare const x:MC_Left;const left:number=x.left;
"""}
    cases["failed-base"] = {"input.ts": """
interface MC_Derived extends MC_Missing { value:number }
declare const x:MC_Derived;const a:number=x.value;
"""}
    cases["recursive-default"] = {"input.ts": """
interface MC_Base<T> { base:T }
interface MC_Derived<T=keyof MC_Derived> extends MC_Base<T> { own:number }
declare const x:MC_Derived;
const a:number=x.own;type MC_Keys=keyof MC_Derived;
"""}
    cases["same-spelled"] = {"input.ts": """
namespace MC_Left {export interface MC_Shape{left:number}}
namespace MC_Right {export interface MC_Shape{right:string}}
declare const left:MC_Left.MC_Shape;declare const right:MC_Right.MC_Shape;
const a:number=left.left;const b:string=right.right;
"""}
    cases["recursive-imports"] = {
        "a.ts": 'import type {MC_B} from "./b";export interface MC_A<T>{value:T;peer?:MC_B<T>}\n',
        "b.ts": 'import type {MC_A} from "./a";export interface MC_B<T>{value:T;peer?:MC_A<T>}\n',
        "input.ts": 'import type {MC_A} from "./a";const good:MC_A<number>={value:1};const bad:MC_A<number>={value:"bad"};\n'}
    cases["augmentation"] = {
        "model.ts": 'export interface MC_Service{base:number}\n',
        "augment.ts": 'import "./model";declare module "./model"{interface MC_Service{extra:string}}\n',
        "input.ts": 'import type {MC_Service} from "./model";const good:MC_Service={base:1,extra:"ok"};const bad:MC_Service={base:1,extra:3};\n'}
    cases["mapped-alias-canonical-negative"] = {"input.ts": """
type Keys<K extends string> = { [P in K]:number };
type One=Keys<"one">;type Two=Keys<"two">;
declare const one:One;const bad:Two=one;
"""}
    cases["shadow-order"] = {"input.ts": """
interface MC_Base { base:number;shared:string }
interface MC_Derived extends MC_Base { shared:string;own:number }
declare const d:MC_Derived;
const a:number=d.base;const b:string=d.shared;const c:number=d.own;
const wrong:number=d.shared;
"""}
    return {name:{path:source.lstrip() for path,source in files.items()}
            for name,files in cases.items()}
