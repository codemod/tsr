//! Native's generic-mapped relation arms and the decidable endings around
//! them (`docs/parity/notes/r5-relater5.md` §1-§2): each marked line reports
//! TS2322 as tsgo does, and each unmarked line stays silent.

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
fn generic_mapped_targets_end_false_after_their_arms() {
    let source = r"type Denullified<T> = { [P in keyof T]-?: NonNullable<T[P]> };
type NonNullable<T> = T & {};
function f1<T>(x: Required<T>, y: T, z: Partial<T>, w: Denullified<T>) {
    x = y; // error
    x = z; // error
    y = x;
    y = z; // error
    z = x;
    z = y;
    w = x; // error
    x = {}; // error
    z = {};
}
";
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn generic_mapped_sources_meet_string_indexes_through_their_template() {
    let source = r"type N2<T> = { [K in keyof T]: number };
function f<T, K extends string>(x: { [key: string]: T }, y: Record<K, T>, m: N2<T>) {
    x = y;
    const d: { [name: string]: number } = m;
    y = x; // error
}
";
    assert_eq!(ts2322_lines(source), marked(source));
}

#[test]
fn decidable_pairs_no_longer_fall_off_the_worker() {
    let source = r"function f3<T, U extends T>(x: T, y: U, k: keyof T) {
    x[k] = y[k];
    y[k] = x[k]; // error
}
function f4<T, K extends keyof T, J extends keyof T>(tk: T[K], tj: T[J]) {
    tj = tk; // error
}
function f5<T, U>(k: keyof T | keyof U, j: keyof T & keyof U) {
    k = j;
    j = k; // error
}
";
    assert_eq!(ts2322_lines(source), marked(source));
}

/// getSimplifiedIndexedAccessType's generic-mapped arm
/// (`docs/parity/notes/r5-relater6.md` §1): `Partial<T>[K]` is related as
/// `T[K] | undefined` and `Readonly<U>[K]` as `U[K]`.
#[test]
fn indexed_access_of_a_generic_mapped_type_is_substituted() {
    let source = r"function f10<T>(x: T, y: Partial<T>, k: keyof T) {
    x[k] = y[k]; // error
    y[k] = x[k];
}
function f12<T, U extends T, K extends keyof T>(x: T, y: Partial<U>, k: K) {
    x[k] = y[k]; // error
    y[k] = x[k]; // error
}
function f20<T, U extends T, K extends keyof T>(x: T, y: Readonly<U>, k: K) {
    x[k] = y[k];
    y[k] = x[k]; // error
}
function f30<T, K extends keyof T>(y: Required<Partial<T>>, k: K) {
    const v: T[K] = y[k]; // error
}
";
    assert_eq!(ts2322_lines(source), marked(source));
}
