//! r6-relater2's relater arms (`docs/parity/notes/r6-relater2.md`): each
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
fn a_mapped_generic_indexed_access_meets_a_union_through_its_index_constraint() {
    // `correlatedUnions` 181 and 299: the failed union walk falls through to
    // the type-variable arm, whose substitution (`Partial<Foo1>[K]` is
    // `Foo1[K] | undefined`) or `Funcs[keyof ArgMap]` step relates.
    let source = r#"type ArgMap = { a: number, b: string };
type Func<K extends keyof ArgMap> = (x: ArgMap[K]) => void;
type Funcs = { [K in keyof ArgMap]: Func<K> };
function f4<K extends keyof ArgMap>(x: Funcs[keyof ArgMap], y: Funcs[K]) {
    x = y;
}
function f5<K extends keyof ArgMap>(y: Funcs[K]) {
    const z: Func<"a"> | number = y; // error
}
type Foo1 = { x: number; y: string };
function getValueConcrete<K extends keyof Foo1>(o: Partial<Foo1>, k: K): Foo1[K] | undefined {
    return o[k];
}
function getValueWrong<K extends keyof Foo1>(o: Partial<Foo1>, k: K): Foo1[K] | null {
    return o[k]; // error
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn a_write_to_a_generic_intersection_object_is_not_related_through_a_constraint() {
    // `undefinedAssignableToGenericMappedIntersection`: the base of
    // `Errors<T>` is an intersection holding a generic mapped type, so
    // isGenericObjectType skips the write-constraint step and `S -> T[K]`
    // fails.
    let source = r#"type Errors<T> = { [P in keyof T]: string | undefined } & { all: string | undefined };
function foo<T>() {
    let obj!: Errors<T>;
    let x!: keyof T;
    obj[x] = undefined; // error
}
type Plain = { a: string | undefined } & { b: string | undefined };
function bar(obj: Plain, k: keyof Plain) {
    obj[k] = undefined;
}
"#;
    assert_eq!(ts2322_lines(source), marked(source));
}
