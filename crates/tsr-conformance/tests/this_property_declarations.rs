//! This-property assignment declarations (`isConstructorDeclaredThisProperty`,
//! `getFlowTypeInConstructor`, `getTypeOfPropertyInBaseClass`). Every expected
//! sequence below is the pinned tsgo (5b1047d1) `.types` baseline for the
//! same source, produced by its own compiler test runner.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(name: &str, source: &str) -> Vec<String> {
    let case = TestCase::parse(name, "this_property.ts", source);
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

fn assert_sequence(name: &str, source: &str, expected: &str) {
    let got = lines(name, source);
    let want: Vec<&str> = expected.lines().collect();
    assert_eq!(got, want, "{name}");
}

/// Constructor flow, typed declarations, method-only `T | undefined` under
/// strict, and inherited property precedence in a derived method.
#[test]
fn constructor_flow_typed_and_inherited_this_properties() {
    assert_sequence(
        "probe/this-property-flow",
        r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: a.js
class Base {
    constructor() {
        /** @type {number[]} */
        this.data = [1, 2, 3];
        this.p = 1;
        this.q = undefined;
        this.r = null;
        if (Math.random()) this.s = "a"; else this.s = 2;
        this.p;
    }
    m() {
        this.p;
        this.data;
        this.s;
        this.t = 1;
        this.t;
        this.q;
    }
}
class Derived extends Base {
    m() {
        this.p = 1;
        this.p;
        this.u = "x";
    }
}
var d = new Derived();
d.p; d.u; d.t;
"#,
        r#"Base : Base
this.data = [1, 2, 3] : number[]
this.data : number[]
this : this
data : number[]
[1, 2, 3] : number[]
1 : 1
2 : 2
3 : 3
this.p = 1 : 1
this.p : any
this : this
p : any
1 : 1
this.q = undefined : undefined
this.q : any
this : this
q : any
undefined : undefined
this.r = null : null
this.r : any
this : this
r : any
Math.random() : number
Math.random : () => number
Math : Math
random : () => number
this.s = "a" : "a"
this.s : any
this : this
s : any
"a" : "a"
this.s = 2 : 2
this.s : any
this : this
s : any
2 : 2
this.p : number
this : this
p : number
m : () => void
this.p : number
this : this
p : number
this.data : number[]
this : this
data : number[]
this.s : string | number
this : this
s : string | number
this.t = 1 : 1
this.t : number | undefined
this : this
t : number | undefined
1 : 1
this.t : number
this : this
t : number
this.q : any
this : this
q : any
Derived : Derived
Base : Base
m : () => void
this.p = 1 : 1
this.p : number
this : this
p : number
1 : 1
this.p : number
this : this
p : number
this.u = "x" : "x"
this.u : string | undefined
this : this
u : string | undefined
"x" : "x"
d : Derived
new Derived() : Derived
Derived : typeof Derived
d.p : number
d : Derived
p : number
d.u : string | undefined
d : Derived
u : string | undefined
d.t : number | undefined
d : Derived
t : number | undefined"#,
    );
}

/// Same-named reads, literal element names, arrow bodies, a prototype method
/// replacing `this.m = this.m.bind(this)`, an ambient property declaration
/// taking value-declaration precedence, braceless `@type`, and a static block
/// filing its property on the class's static side (strict).
#[test]
fn this_property_precedence_and_static_blocks_strict() {
    assert_sequence(
        "probe/this-property-precedence-strict",
        r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @target: es2020
// @filename: a.js
class A {
    constructor() {
        this.count = this.count || 0;
        this["label"] = "a";
        /** @type number */
        this.typed = 0;
        this.later = null;
        const f = () => { this.arrow = 1; this.arrow; };
        this.m = this.m.bind(this);
        this.prop = {};
    }
    declare prop: string;
    m() { return 1; }
    n() {
        this.later = "x";
        this.count; this.label; this.typed; this.later; this.arrow; this.prop;
        this.self = this.self;
    }
    static {
        this.st = 1;
    }
}
const a = new A();
a.count; a.label; a.typed; a.later; a.arrow; a.m; a.self; A.st;
"#,
        r#"A : A
this.count = this.count || 0 : 0
this.count : any
this : this
count : any
this.count || 0 : 0
this.count : undefined
this : this
count : undefined
0 : 0
this["label"] = "a" : "a"
this["label"] : any
this : this
"label" : "label"
"a" : "a"
this.typed = 0 : 0
this.typed : number
this : this
typed : number
0 : 0
this.later = null : null
this.later : any
this : this
later : any
f : () => void
() => { this.arrow = 1; this.arrow; } : () => void
this.arrow = 1 : 1
this.arrow : number
this : this
arrow : number
1 : 1
this.arrow : number
this : this
arrow : number
this.m = this.m.bind(this) : () => number
this.m : () => number
this : this
m : () => number
this.m.bind(this) : () => number
this.m.bind : { <T>(this: T, thisArg: ThisParameterType<T>): OmitThisParameter<T>; <T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R, thisArg: T, ...args: A): (...args: B) => R; }
this.m : () => number
this : this
m : () => number
bind : { <T>(this: T, thisArg: ThisParameterType<T>): OmitThisParameter<T>; <T, A extends any[], B extends any[], R>(this: (this: T, ...args: [...A, ...B]) => R, thisArg: T, ...args: A): (...args: B) => R; }
this : this
this.prop = {} : {}
this.prop : string
this : this
prop : string
{} : {}
prop : string
m : () => number
1 : 1
n : () => void
this.later = "x" : "x"
this.later : string | null
this : this
later : string | null
"x" : "x"
this.count : number
this : this
count : number
this.label : string
this : this
label : string
this.typed : number
this : this
typed : number
this.later : string
this : this
later : string
this.arrow : number
this : this
arrow : number
this.prop : string
this : this
prop : string
this.self = this.self : any
this.self : any
this : this
self : any
this.self : any
this : this
self : any
this.st = 1 : 1
this.st : number | undefined
this : typeof A
st : number | undefined
1 : 1
a : A
new A() : A
A : typeof A
a.count : number
a : A
count : number
a.label : string
a : A
label : string
a.typed : number
a : A
typed : number
a.later : string | null
a : A
later : string | null
a.arrow : number
a : A
arrow : number
a.m : () => number
a : A
m : () => number
a.self : any
a : A
self : any
A.st : number | undefined
A : typeof A
st : number | undefined"#,
    );
}

/// The non-strict twin: method-only declarations add no `undefined`.
#[test]
fn this_property_precedence_and_static_blocks_non_strict() {
    assert_sequence(
        "probe/this-property-precedence-loose",
        r#"// @allowJs: true
// @checkJs: true
// @strict: false
// @target: es2020
// @filename: a.js
class A {
    constructor() {
        this.count = this.count || 0;
        this["label"] = "a";
        /** @type number */
        this.typed = 0;
        this.later = null;
        const f = () => { this.arrow = 1; this.arrow; };
        this.m = this.m.bind(this);
        this.prop = {};
    }
    declare prop: string;
    m() { return 1; }
    n() {
        this.later = "x";
        this.count; this.label; this.typed; this.later; this.arrow; this.prop;
        this.self = this.self;
    }
    static {
        this.st = 1;
    }
}
const a = new A();
a.count; a.label; a.typed; a.later; a.arrow; a.m; a.self; A.st;
"#,
        r#"A : A
this.count = this.count || 0 : 0
this.count : any
this : this
count : any
this.count || 0 : 0
this.count : undefined
this : this
count : undefined
0 : 0
this["label"] = "a" : "a"
this["label"] : any
this : this
"label" : "label"
"a" : "a"
this.typed = 0 : 0
this.typed : number
this : this
typed : number
0 : 0
this.later = null : null
this.later : any
this : this
later : any
f : () => void
() => { this.arrow = 1; this.arrow; } : () => void
this.arrow = 1 : 1
this.arrow : number
this : this
arrow : number
1 : 1
this.arrow : number
this : this
arrow : number
this.m = this.m.bind(this) : any
this.m : () => number
this : this
m : () => number
this.m.bind(this) : any
this.m.bind : (this: Function, thisArg: any, ...argArray: any[]) => any
this.m : () => number
this : this
m : () => number
bind : (this: Function, thisArg: any, ...argArray: any[]) => any
this : this
this.prop = {} : {}
this.prop : string
this : this
prop : string
{} : {}
prop : string
m : () => number
1 : 1
n : () => void
this.later = "x" : "x"
this.later : string
this : this
later : string
"x" : "x"
this.count : number
this : this
count : number
this.label : string
this : this
label : string
this.typed : number
this : this
typed : number
this.later : string
this : this
later : string
this.arrow : number
this : this
arrow : number
this.prop : string
this : this
prop : string
this.self = this.self : any
this.self : any
this : this
self : any
this.self : any
this : this
self : any
this.st = 1 : 1
this.st : number
this : typeof A
st : number
1 : 1
a : A
new A() : A
A : typeof A
a.count : number
a : A
count : number
a.label : string
a : A
label : string
a.typed : number
a : A
typed : number
a.later : string
a : A
later : string
a.arrow : number
a : A
arrow : number
a.m : () => number
a : A
m : () => number
a.self : any
a : A
self : any
A.st : number
A : typeof A
st : number"#,
    );
}
