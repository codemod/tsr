//! Check-site targets of `checkReturnStatement`, `checkAssignmentOperator`,
//! `checkForOfStatement` and `checkVariableLikeDeclaration`
//! (`docs/parity/notes/r5-ts2322.md` §2.2). The expected texts are native
//! tsgo's (vendor `5b1047d`) for the same source.

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;
use tsr_diagnostics::{Diagnostic, format::write_flattened_diagnostic_message};

const LIB: &str = "interface SymbolConstructor { readonly iterator: unique symbol; }\n\
    declare var Symbol: SymbolConstructor;\n\
    interface IteratorYieldResult<T> { done?: false; value: T; }\n\
    interface IteratorReturnResult<T> { done: true; value: T; }\n\
    type IteratorResult<T, R = any> = IteratorYieldResult<T> | IteratorReturnResult<R>;\n\
    interface Iterator<T, R = any, N = any> { next(...[value]: [] | [N]): IteratorResult<T, R>; }\n\
    interface Iterable<T, R = any, N = any> { [Symbol.iterator](): Iterator<T, R, N>; }\n\
    interface IterableIterator<T, R = any, N = any> extends Iterator<T, R, N> {\n\
        [Symbol.iterator](): IterableIterator<T, R, N>;\n\
    }\n\
    interface Generator<T = unknown, R = any, N = any> extends Iterator<T, R, N> {\n\
        [Symbol.iterator](): Generator<T, R, N>;\n\
    }\n\
    interface Array<T> { length: number; [n: number]: T; [Symbol.iterator](): IterableIterator<T>; }\n";

/// `(code, first line of the message)` per diagnostic.
fn diagnostics(body: &str) -> Vec<(u32, String)> {
    let source = format!("{LIB}{body}");
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, &source);
    assert!(parsed.diagnostics.is_empty());
    let root = parsed.source_file.node_id.unwrap();
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "a.ts", text: &source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker
        .diagnostics()
        .iter()
        .map(|(_, diagnostic): &(_, Diagnostic)| {
            let mut text = String::new();
            write_flattened_diagnostic_message(&mut text, diagnostic, "\n");
            let head = text.lines().next().unwrap_or_default().to_string();
            (diagnostic.message.code(), head)
        })
        .collect()
}

fn ts2322(source: &str, target: &str) -> (u32, String) {
    (2322, format!("Type '{source}' is not assignable to type '{target}'."))
}

#[test]
fn a_generator_return_relates_to_the_return_iteration_type() {
    // generatorExplicitReturnType: `unwrapReturnType`'s generator arm.
    let actual = diagnostics(
        "function* g1(): Generator<number, boolean, string> { return 10; }\n\
         function* g2(): Generator<number, boolean, string> { return true; }\n\
         function* g3(): Iterable<number> { return 1; }\n",
    );
    assert_eq!(actual, [ts2322("number", "boolean")]);
}

#[test]
fn a_late_bound_getter_returns_its_setters_annotation() {
    // constEnumPropertyAccess1: `get [G.B]()` pairs with `set [G.B](x: number)`.
    let actual = diagnostics(
        "const enum G { A = 1, B = 2 }\n\
         class C {\n\
             get [G.B]() { return true; }\n\
             set [G.B](x: number) { }\n\
             static get [G.A]() { return true; }\n\
             set [G.A](x: number) { }\n\
         }\n",
    );
    assert_eq!(actual, [ts2322("boolean", "number")]);
}

#[test]
fn a_compound_like_assignment_writes_into_the_literal_base() {
    // literalWideningWithCompoundLikeAssignments.
    let actual = diagnostics(
        "declare const numLiteral: 0;\n\
         let t1 = numLiteral;\n\
         t1 = t1 + 42;\n\
         t1 = 42;\n",
    );
    assert_eq!(actual, [ts2322("42", "0")]);
}

#[test]
fn a_default_relates_to_the_contextual_parameter_type() {
    // defaultArgsInFunctionExpressions 17:41.
    let actual = diagnostics(
        "var f4: (a: number) => void = function (a = \"\") { };\n\
         function foo(a = bar()) { }\n\
         function bar(a = foo()) { }\n",
    );
    assert_eq!(actual, [ts2322("string", "number")]);
}

#[test]
fn an_any_source_fails_only_against_never() {
    // intersectionReduction 80:1.
    let actual = diagnostics(
        "declare let n: never;\n\
         n = 1 as any;\n\
         let s: string = 1 as any;\n",
    );
    assert_eq!(actual, [ts2322("any", "never")]);
}
