//! r6-relater's relater arms (`docs/parity/notes/r6-relater.md`): each
//! marked line reports TS2322 as tsgo does, and each unmarked line stays
//! silent. Every expectation was checked against a native tsgo built from
//! the pinned submodule.

use tsr_ast::{NodeMap, NodeTable};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, ScriptTarget, Tristate};

/// The 1-based lines of every TS2322 in `source`, checked under `strict`.
fn ts2322_lines(source: &str) -> Vec<usize> {
    lines_of(source, &[2322])
}

/// The 1-based lines of every diagnostic with one of `codes` in `source`.
fn lines_of(source: &str, codes: &[u32]) -> Vec<usize> {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut map = NodeMap::new();
    let globals = "interface Array<T> { [n: number]: T; length: number } \
        interface ReadonlyArray<T> { readonly [n: number]: T; readonly length: number } \
        interface String { length: number; [n: number]: string } \
        interface Number { toFixed(): string } interface Boolean {} \
        interface Object {} interface Function {} interface RegExp {} \
        interface IArguments {} interface Symbol {} \
        interface SymbolConstructor { (): symbol } declare var Symbol: SymbolConstructor; \
        type Partial<T> = { [P in keyof T]?: T[P] }; \
        type Required<T> = { [P in keyof T]-?: T[P] }; \
        type Readonly<T> = { readonly [P in keyof T]: T[P] }; \
        type Record<K extends keyof any, T> = { [P in K]: T }; \
        type Pick<T, K extends keyof T> = { [P in K]: T[P] }; \
        type Capitalize<S extends string> = intrinsic; \
        type Uncapitalize<S extends string> = intrinsic;";
    let global = tsr_parser::parse_into(
        &arena,
        globals,
        tsr_parser::ParseOptions::for_file("global.d.ts"),
        &mut nodes,
        &mut map,
    );
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        global.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "global.d.ts", text: globals },
    );
    let parsed = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut map,
    );
    let root = parsed.source_file.node_id.expect("registered source file");
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &map);
    checker.apply_compiler_options(&CompilerOptions {
        strict: Tristate::from_bool(true),
        target: ScriptTarget::ES2015,
        ..CompilerOptions::default()
    });
    checker.check_source_file(
        root,
        FileContext { ambient: false, has_parse_errors: !parsed.diagnostics.is_empty() },
    );
    let mut lines: Vec<usize> = checker
        .diagnostics()
        .iter()
        .filter(|(file, diagnostic)| *file == root && codes.contains(&diagnostic.message.code()))
        .map(|(_, diagnostic)| {
            source[..usize::try_from(diagnostic.span.start).unwrap()].matches('\n').count() + 1
        })
        .collect();
    lines.sort_unstable();
    lines.dedup();
    lines
}

/// The lines of `source` that carry a `// error` marker.
fn marked(source: &str) -> Vec<usize> {
    source
        .lines()
        .enumerate()
        .filter(|(_, line)| line.contains("// error"))
        .map(|(index, _)| index + 1)
        .collect()
}

#[test]
fn the_non_primitive_object_meets_a_target_through_its_apparent_empty_type() {
    let source = r#"function foo<T extends object>(x: { a?: string }, y: T & { a: boolean }) {
  x = y; // error
}
function bar<T extends object>(x: { a?: string }, y: T & { a: string }) {
  x = y;
}
declare let o: object;
const p: { a?: string } = o;
const q: { a: string } = o; // error
const r: { (): void } = o; // error
const s: { [x: string]: any } = o;
const t: { [x: string]: number } = o; // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn an_unreliable_variance_falls_back_to_the_structure() {
    // `P`'s mapped constraint reports Unreliable: the failed covariant
    // `{ a: 1 } -> { a: 1; b: 2 }` falls back to `{ a?: 1 } -> { a?: 1; b?: 2 }`.
    let source = r#"type P<T> = { [K in keyof T]?: T[K] };
declare let p1: P<{ a: 1 }>;
declare let p2: P<{ a: 1; b: 2 }>;
p2 = p1;
p1 = p2;
"#;
    assert_eq!(lines_of(source, &[2322, 2741]), marked(source));
}

#[test]
fn an_unmeasurable_variance_relates_only_identical_arguments() {
    let source = r#"declare let r1: Required<{ a?: 1; x: 1 }>;
declare let r2: Required<{ b?: 1; x: 1 }>;
r1 = r2; // error
r2 = r1; // error
declare let r3: Required<{ a?: 1; x: 1 }>;
r3 = r1;
"#;
    assert_eq!(lines_of(source, &[2322, 2741]), marked(source));
}

#[test]
fn an_unwitnessed_parameter_of_a_reference_alias_is_independent() {
    let source = r#"type A<T> = B<T>;
interface B<T> { prop: A<T>; }
declare let a: A<number>;
declare let b: A<3>;
b = a;
"#;
    assert_eq!(lines_of(source, &[2322, 2741]), marked(source));
}

#[test]
fn nested_homomorphic_applications_keep_their_own_recursion_identity() {
    let source = r#"type Id<T> = { [K in keyof T]: T[K] };
declare let d1: Id<{ x: Id<{ y: Id<{ z: Id<{ w: number }> }> }> }>;
declare let d2: Id<{ x: Id<{ y: Id<{ z: Id<{ w: string }> }> }> }>;
d1 = d2; // error
"#;
    assert_eq!(lines_of(source, &[2322, 2741]), marked(source));
}

#[test]
fn a_homomorphic_alias_over_a_primitive_is_that_primitive() {
    let source = r#"type RequiredDeep<T> = { [K in keyof T]-?: RequiredDeep<T[K]> };
type A = { a?: { b: { c: 1 | { d: 2000 } } } };
type B = { a?: { b: { c: { d: { e: { f: { g: 2 } } } }; x: 1000 } } };
type C = RequiredDeep<A>;
type D = RequiredDeep<B>;
const t2: C extends D ? true : false = true; // error
const u2: RequiredDeep<1 | undefined> extends RequiredDeep<2 | undefined> ? true : false = true; // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn an_alias_written_as_an_interface_reference_keeps_no_alias_variance() {
    let source = r#"interface Shape<V> { value: V }
type VarianceShape<in out V> = Shape<V>;
declare let vs1: VarianceShape<1>;
declare let vs12: VarianceShape<1 | 2>;
vs1 = vs12; // error
vs12 = vs1;
class A { x: string = ""; y: number = 0; }
class B { x: string = ""; z: boolean = true; }
type T<X extends { x: any }> = Pick<X, "x">;
const ta: T<A> extends T<B> ? true : false = true; // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn an_extract_source_relates_through_its_inferred_true_type() {
    let source = r#"type Extract<T, U> = T extends U ? T : never;
function f3<T, K extends Extract<keyof T, string>, U extends T, J extends K>(tk: T[K], tj: T[J], uk: U[K], uj: U[J]): void {
    tk = uk;
    uk = tk; // error
    tj = uj;
    uj = tj; // error
    tk = tj;
    tj = tk; // error
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

/// The lines of every TS2321 in `source`.
fn ts2321_lines(source: &str) -> Vec<usize> {
    lines_of(source, &[2321])
}

#[test]
fn a_literal_alias_expanding_through_its_arguments_is_cut_not_overflowed() {
    // tsgo reports nothing here: every `Bar<…>` instantiation shares the
    // literal's symbol, so isDeeplyNestedType cuts the walk as expanding.
    let source = r#"type Foo<T> = { x: Foo<T> };
type Bar<T> = { x: Bar<T[]> };
function f2<U>() {
    let x: Foo<U> = 0 as any as Bar<U>;
}
"#;
    assert_eq!(ts2321_lines(source), Vec::<usize>::new());
    assert_eq!(ts2322_lines(source), Vec::<usize>::new());
}

#[test]
fn a_related_primitive_source_is_not_reported() {
    let source = r#"interface Boolean { doStuff(): string; }
interface NotBoolean { doStuff(): string; }
var x = true;
declare var a: Boolean;
declare var b: NotBoolean;
b = a;
b = x;
x = b; // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}
