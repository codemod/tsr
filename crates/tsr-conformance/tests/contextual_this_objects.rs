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
