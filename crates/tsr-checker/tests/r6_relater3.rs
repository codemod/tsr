//! r6-relater3's commits (`docs/parity/notes/r6-relater3.md`): each marked
//! line reports the named code as tsgo does, and each unmarked line stays
//! silent. Every expectation was checked against a native tsgo built from
//! the pinned submodule.

use tsr_ast::{NodeMap, NodeTable};
use tsr_checker::{Checker, check::FileContext};
use tsr_core::{Arena, CompilerOptions, ScriptTarget, Tristate};

/// The 1-based lines of every diagnostic with one of `codes` in `source`.
fn lines_of(source: &str, codes: &[u32]) -> Vec<usize> {
    let file = "a.ts";
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

/// The 1-based lines of every diagnostic with one of `codes` in the
/// JavaScript file `source` (JSDoc collected, as the corpus checks a JS file),
/// under `strict`.
fn js_lines_of(source: &str, codes: &[u32]) -> Vec<usize> {
    let name = "a.js";
    let arena = Arena::new();
    let mut parsed =
        tsr_parser::parse_with_options(&arena, source, tsr_parser::ParseOptions::for_file(name));
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("source file");
    parsed.nodes.add_flags(root, tsr_ast::NodeFlags::JAVASCRIPT_FILE);
    let jsdoc: Vec<_> = parsed.jsdoc.iter().collect();
    let bound = tsr_binder::bind_into_with_jsdoc(
        tsr_binder::BindResult::empty(),
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name, text: source },
        &jsdoc,
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_jsdoc(parsed.jsdoc.iter());
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
fn a_js_function_returns_against_its_jsdoc_return_type() {
    // `arrowExpressionBodyJSDoc`, `importTag24`, `jsdocBracelessTypeTag1` 3:
    // getReturnTypeFromAnnotation reads the reparsed `@returns` and a
    // function's `@type` full signature, and a JS `@type` cast's parentheses
    // are the effective check node.
    let source = r#"/** @returns {number} */
function f() { return "s"; } // error
/** @returns {number} */
const g = () => "s"; // error
/** @type {(x: number) => string} */
function h(x) { return x; } // error
/** @returns {string} */
function ok() { return "s"; }
/** @returns {number} */
const cast = () => /** @type {string} */ ("s"); // error
"#;
    assert_eq!(js_lines_of(source, &[2322]), marked(source));
}

#[test]
fn a_new_non_generic_alias_relates_structurally_not_by_alias_variance() {
    // `genericIndexedAccessVarianceComparisonResultCorrect`: `type C = T<A>`
    // carries alias `C` with no type arguments, so `c = d` relates
    // structurally (`{ x: string }` both ways); `b = a` keeps `T`'s
    // variances. An alias written as an interface reference keeps the
    // reference's variances.
    let source = r#"class A { x: string = 'A'; y: number = 0; }
class B { x: string = 'B'; z: boolean = true; }
type T<X extends { x: any }> = Pick<X, 'x'>;
type C = T<A>;
type D = T<B>;
declare let a: T<A>;
declare let b: T<B>;
declare let c: C;
declare let d: D;
b = a; // error
c = d;
type FooBase = string | false;
type FooArray = FooBase[];
declare let fa: FooArray;
declare let ba: boolean[];
ba = fa; // error
"#;
    assert_eq!(lines_of(source, &[2322]), marked(source));
}
