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
        type Record<K extends keyof any, T> = { [P in K]: T };";
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
