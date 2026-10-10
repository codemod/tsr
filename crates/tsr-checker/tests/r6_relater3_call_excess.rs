//! r6-relater3's generic-call object-literal argument gates
//! (`docs/parity/notes/r6-relater3.md` §3): each
//! marked line reports TS2353 as tsgo does, and each unmarked line stays
//! silent. Every expectation was checked against a native tsgo built from
//! the pinned submodule.

use tsr_ast::{NodeMap, NodeTable};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, ScriptTarget, Tristate};

/// The 1-based lines of every diagnostic with one of `codes` in `source`.
fn lines_of(source: &str, codes: &[u32]) -> Vec<usize> {
    lines_in("a.ts", source, codes)
}

/// [`lines_of`] for a file named `file` (`a.tsx` parses JSX).
fn lines_in(file: &str, source: &str, codes: &[u32]) -> Vec<usize> {
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
        tsr_parser::ParseOptions::for_file(file),
        &mut nodes,
        &mut map,
    );
    let root = parsed.source_file.node_id.expect("registered source file");
    let bound = tsr_binder::bind_into(
        bound,
        &arena,
        parsed.source_file,
        &nodes,
        tsr_binder::FileInfo { name: file, text: source },
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
fn an_instantiated_parameter_reports_a_literal_excess_property_first() {
    // `excessPropertyCheckWithEmptyObject`, `reverseMappedTypeLimitedConstraint`:
    // hasExcessProperties runs before the structural comparison and reads
    // only the literal's names, so a target mentioning a literal type (or
    // an intersection with `ThisType<any>`) does not stop the report.
    let source = r#"interface ThisType<T> {}
interface Desc { value?: any; writable?: boolean; }
declare function define<T>(o: T, attributes: Desc & ThisType<any>): T;
define(1, { value: "v", readonly: false }); // error
declare function k<T extends number>(p: { x: T }): T;
k({ x: 1, y: 2 }); // error
k({ x: 1 });
"#;
    assert_eq!(lines_of(source, &[2353]), marked(source));
}
