//! Contextual receiver controls checked against pinned tsgo 5b1047d1.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/contextual-this", "this.ts", source);
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
    for wanted in wanted {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn plain_and_generic_contexts_substitute_the_polymorphic_receiver() {
    expect(
        r"// @strict: true
interface Box { value: number; accept(this: this, value: string): this; }
interface Generic<T> { value: T; accept(this: this, value: T): this; }
declare function capture(value: Box): Box;
declare function generic<T>(value: Generic<T>): Generic<T>;
const box = capture({ value: 1, accept(value) { return this; } });
const strings = generic<string>({ value: 'ok', accept(value) { return this; } });
declare const original: Box;
const method = original.accept;
const returned = original.accept('value');
",
        &[
            "accept : (this: Box, value: string) => Box",
            "accept : (this: Generic<string>, value: string) => Generic<string>",
            "method : (this: Box, value: string) => Box",
            "returned : Box",
        ],
    );
}

#[test]
fn apparent_constraints_preserve_receiver_identity_and_unmapped_templates() {
    expect(
        r"// @strict: true
class Fluent { identity(): this { return this; } }
class Derived extends Fluent { own = 1; }
const base = new Fluent().identity();
const derived = new Derived().identity();
function constrained<T extends Fluent>(value: T) { return value.identity(); }
class Receiver<T extends { a?: number; b: string }> {
    data!: { [P in keyof T]: T[P] };
    self(): this { return this; }
    forward() { return this.self(); }
    read() { return this.data; }
}
",
        &[
            "base : Fluent",
            "derived : Derived",
            "constrained : <T extends Fluent>(value: T) => T",
            "forward : () => this",
            "read : () => { [P in keyof T]: T[P]; }",
        ],
    );
}

#[test]
fn object_contexts_supply_this_to_methods_and_function_properties() {
    expect(
        r#"// @strict: true
interface Model { value: number; method(): number; }
const plain: Model = { value: 42, method() { return this.value; } };
interface FunctionModel { label: string; method(): string; }
const functionValue: FunctionModel = { label: "ok", method: function() { return this.label; } };
"#,
        &["this : Model", "this.value : number", "this : FunctionModel", "this.label : string"],
    );
}

#[test]
fn markers_reach_nested_literals_and_explicit_signatures_take_precedence() {
    expect(
        r"// @strict: true
const marked: ThisType<{ x: number }> & { read(): number } = { read() { return this.x; } };
const nested: ThisType<{ nestedValue: string }> & { inner: { read(): string } } = {
  inner: { read() { return this.nestedValue; } }
};
const explicit: ThisType<{ x: number }> & { read(this: { y: string }): string } = {
  read() { return this.y; }
};
",
        &[
            "this : { x: number; }",
            "this.x : number",
            "this : { nestedValue: string; }",
            "this.nestedValue : string",
            "this : { y: string; }",
            "this.y : string",
        ],
    );
}

#[test]
fn marker_aliases_and_inferred_object_fields_use_the_active_mapper() {
    expect(
        r"// @strict: true
declare function make<T>(options: { data: T; methods: { read(): unknown } & ThisType<{ data: T }> }): T;
const result = make({ data: { value: 42 }, methods: { read() { return this.data.value; } } });
type Alias<T> = ThisType<T> & { read(): number };
const aliased: Alias<{ x: number }> = { read() { return this.x; } };
type Marker<T> = ThisType<T>;
const nestedAlias: Marker<{ label: string }> & { read(): string } = { read() { return this.label; } };
",
        &[
            "result : { value: number; }",
            "this : { data: { value: number; }; }",
            "this.data.value : number",
            "this.x : number",
            "this.label : string",
        ],
    );
}

#[test]
fn a_local_interface_named_this_type_is_not_the_global_marker() {
    expect(
        r#"// @strict: true
namespace Local {
    export interface ThisType<T> { tag: string; }
    export const value: ThisType<{ x: number }> & { read(): string } = {
        tag: "ok", read() { return this.tag; }
    };
}
"#,
        &["this.tag : string"],
    );
}

#[test]
fn a_marker_alias_reduced_to_any_exposes_its_semantic_type() {
    expect(
        r"// @strict: true
type Receiver<D> = D & { tag: string };
const reduced: ThisType<Receiver<any>> & { read(): any } = { read() { return this; } };
",
        &["this : any", "read : () => any"],
    );
}

#[test]
fn marker_context_does_not_rebind_lexical_arrows_or_enable_no_implicit_this() {
    expect(
        r"// @strict: true
const lexical: ThisType<{ x: number }> & { read(): unknown } = { read: () => this };
",
        &["this : typeof globalThis"],
    );
    expect(
        r"// @strict: false
// @noImplicitThis: false
const loose: ThisType<{ x: number }> & { read(): unknown } = { read() { return this; } };
",
        &["this : any"],
    );
}

#[test]
fn computed_names_keep_the_enclosing_receiver() {
    expect(
        r"// @strict: true
class Enclosing {
    key() { return 1; }
    build() { return { [this.key()]() {} }; }
}",
        &["this.key() : number", "this : this"],
    );
}

#[test]
fn a_marker_shared_by_union_branches_is_not_a_recursion_cycle() {
    expect(
        r"// @strict: true
type First = ThisType<{ x: number }>;
type Second = ThisType<{ x: string }>;
const value: (First & { a?: string; read(): number }) | (First & Second & { b?: string; read(): number }) = {
    read() { return this.x; }
};
",
        &["this.x : number", "this : { x: number; }"],
    );
}

#[test]
fn assignment_receivers_cover_dot_element_and_parenthesized_functions() {
    expect(
        r"// @strict: true
interface Dot { count: number; run(): number; }
interface Indexed { label: string; run(): string; }
declare const dot: Dot;
declare const indexed: Indexed;
dot.run = function() { return this.count; };
indexed['run'] = ((function() { return this.label; }));
interface Logical { enabled: boolean; run(): boolean; }
declare const logical: Logical;
logical.run ||= function() { return this.enabled; };
",
        &[
            "this : Dot",
            "this.count : number",
            "this : Indexed",
            "this.label : string",
            "this : Logical",
            "this.enabled : boolean",
        ],
    );
}

#[test]
fn assignment_receiver_inference_preserves_explicit_and_lexical_this() {
    expect(
        r"// @strict: true
interface Receiver { value: number; run(this: { label: string }): string; }
declare const receiver: Receiver;
receiver.run = function() { return this.label; };
const lexical: { run(): unknown } = { run() { return 0; } };
lexical.run = () => this;
",
        &["this.label : string", "this : { label: string; }", "this : typeof globalThis"],
    );
    expect(
        r"// @strict: false
interface Receiver { value: number; run(): number; }
declare const receiver: Receiver;
receiver.run = function() { const loose = this; return loose.value; };
",
        &["loose : any", "loose.value : any"],
    );
}

#[test]
fn javascript_assignment_receivers_distinguish_local_exports() {
    expect(
        r#"// @allowJs: true
// @checkJs: true
// @strict: false
// @filename: assignment.js
const object = { count: 1, run() { return 0; } };
object.run = function() { return this.count; };
const exports = { label: "ok", run() { return ""; } };
exports.run = function() { return this.label; };
"#,
        &["this.count : number", "this.label : string"],
    );
    expect(
        r"// @allowJs: true
// @checkJs: true
// @strict: false
// @filename: commonjs.js
exports.Point = function(x) { this.x = x; const instance = this; };
",
        &["instance : any"],
    );
}

#[test]
fn this_flow_narrows_discriminants_and_truthiness() {
    expect(
        r#"// @strict: true
function discriminant(this: { kind: "left"; value: number } | { kind: "right"; value: string }) {
    if (this.kind === "left") { const left = this; return left.value; }
    const right = this;
    return right.value;
}
function optional(this: { value: number } | undefined) {
    if (this) { const present = this; return present.value; }
    const absent = this;
}
"#,
        &[
            "left : { kind: \"left\"; value: number; }",
            "right : { kind: \"right\"; value: string; }",
            "present : { value: number; }",
            "absent : undefined",
        ],
    );
}

#[test]
fn this_flow_applies_assertion_predicates() {
    expect(
        r"// @strict: true
declare function assertValue(value: unknown): asserts value is { value: number };
function checked(this: unknown) {
    assertValue(this);
    const asserted = this;
    return asserted.value;
}
",
        &["asserted : { value: number; }", "asserted.value : number"],
    );
}

#[test]
fn instanceof_preserves_polymorphic_and_generic_receiver_identity() {
    expect(
        r"// @strict: true
class Base {
    inspect() {
        if (this instanceof Derived) { const derived = this; return derived.own; }
        return 0;
    }
}
class Derived extends Base { own = 1; }
function generic<T extends Base>(value: T) {
    if (value instanceof Derived) { return value; }
}
",
        &[
            "derived : this & Derived",
            "derived.own : number",
            "generic : <T extends Base>(value: T) => (T & Derived) | undefined",
        ],
    );
}

#[test]
fn parameter_initializers_skip_contextual_assignment_receivers() {
    expect(
        r"// @strict: true
interface Target { value: number; run(value?: unknown): unknown; }
declare const target: Target;
target.run = function(initial = this) { const body = this; return body.value; };
",
        &["this : any", "body : Target", "body.value : number"],
    );
    expect(
        r"// @strict: true
class Defaulted { field = 1; run(value = this) { return value.field; } }
class Explicit { run(this: { label: string }, annotated = this) { return annotated.label; } }
",
        &["value : this", "annotated : { label: string; }", "annotated.label : string"],
    );
}

#[test]
fn unannotated_this_uses_its_contextual_slot_or_implicit_any() {
    expect(
        r"// @strict: true
let contextual: (this: { count: number }, value: number) => number;
contextual = function(this, value) { const assigned = this; return assigned.count + value; };
class Unannotated { run(this, implicit = this) { const inner = this; return inner; } }
",
        &["assigned : { count: number; }", "implicit : any", "inner : any"],
    );
}

#[test]
fn jsdoc_this_precedes_assignment_context_and_obeys_native_hosts() {
    expect(
        r#"// @allowJs: true
// @checkJs: true
// @strict: true
// @filename: explicit.js
const receiver = { label: "", run() { return 0; } };
/** @this {{ count: number }} */
receiver.run ??= function() { const annotated = this; return annotated.count; };
/** @this {{ nested: string }} */
const direct = function() { return this.nested; };
/** @this {{ ignored: boolean }} */
const wrapped = (function() { const lexicalHost = this; return lexicalHost; });
"#,
        &[
            "annotated : { count: number; }",
            "annotated.count : number",
            "this.nested : string",
            "lexicalHost : any",
        ],
    );
}
