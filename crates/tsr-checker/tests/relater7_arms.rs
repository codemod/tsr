//! r5-relater7's relater arms (`docs/parity/notes/r5-relater7.md`): each
//! marked line reports TS2322 as tsgo does, and each unmarked line stays
//! silent.

use tsr_ast::{NodeMap, NodeTable};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, ScriptTarget, Tristate};

/// The 1-based lines of every TS2322 in `source`, checked under `strict`.
fn ts2322_lines(source: &str) -> Vec<usize> {
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
        .filter(|(file, diagnostic)| *file == root && diagnostic.message.code() == 2322)
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
fn a_unique_symbol_against_a_decidable_target_is_decided() {
    let source = r#"declare const u: unique symbol;
const s: string = u; // error
const o: object = u; // error
const y: symbol = u;
const n: number | boolean = u; // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn a_private_identifier_of_another_class_is_an_absent_property() {
    let source = r#"class A { #foo: number = 1; }
interface A2 extends A {}
class B { #foo: number = 1; }
class C extends A {}
const b: A2 = new B(); // error
const c: A2 = new C();
const a: A = new C();
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn only_the_target_class_privacy_is_nominal() {
    let source = r#"class C { private x = 1 }
class D {}
class E { #y = 1 }
class F { x = 1 }
const d: D = new C();
const d2: D = new E();
const f: F = new C(); // error
const c: C = new F(); // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn variadic_tuple_elements_relate_by_their_flags() {
    let source = r#"function f<T extends unknown[], U extends unknown[]>(a: [...T, ...U], b: [string, ...T], c: [...T, number], d: [...T, ...string[]], e: string[], g: [...T]) {
  const x1: [...T, ...U] = a;
  const x2: [string, ...T] = b;
  const x3: [...T, number] = c;
  const x4: [...T, ...string[]] = d;
  const x5: [...T, ...string[]] = e; // error
  const x6: [...T, ...unknown[]] = g;
  const x7: [string, ...T] = c; // error
  const x8: [...T, number] = b; // error
  const x9: unknown[] = a;
  const x10: [...T, ...unknown[]] = c;
  const x11: [...T, ...U] = [] as any[]; // error
}
class I<SS extends string> {
  f() {
    let w: [...args: { [S in SS]: [a: number] }[SS]] = [1];
  }
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn a_string_mapping_meets_a_template_through_its_base_constraint() {
    let source = r#"function f(t: `A${string}`, c: Capitalize<string>, u: Uncapitalize<string>, l: `a${string}`) {
  t = c; // error
  l = u; // error
  c = t;
  u = l;
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn a_function_merged_with_a_namespace_relates_over_its_exports() {
    let source = r#"function Point() { return 0; }
namespace Point { export var Origin = 1; }
const f: () => number = Point;
const o: { Origin: number } = Point;
const p: { Origin: string } = Point; // error
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn union_fast_paths_keep_their_answers() {
    let source = r#"type L = "a" | "b" | "c" | undefined;
type O = { a: string } | { b: number };
function f(x: L, y: L & O, s: string | number, t: "a" | "b") {
  x = y;
  s = t;
  const u: "a" | "b" = "c"; // error
  const v: string | number = "c";
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn an_overflowing_check_is_not_reported_as_ts2322() {
    // relationComplexityError's f2: native reports TS2859 (the reporter half
    // is held, docs/parity/notes/r5-relater7.md section 9); the relater's
    // overflow answer must not surface as a TS2322 meanwhile.
    let source = r#"type Digits = '0' | '1' | '2' | '3' | '4' | '5' | '6' | '7';
type T1 = `${Digits}${Digits}${Digits}${Digits}` | undefined;
type T2 = { a: string } | { b: number };
function f1(x: T1, y: T1 & T2) {
    x = y;
}
function f2(x: T1 | null, y: T1 & T2) {
    x = y;
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}
