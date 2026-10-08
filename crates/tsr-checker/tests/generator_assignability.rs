//! Generator assignability: `checkYieldExpression`'s check for `yield*` (the
//! delegated iterable's iterated type against the annotation's yield type,
//! `getYieldedTypeOfYieldExpression`, `checker.go:11019`) and
//! `checkSignatureDeclaration`'s generator arm (`checker.go:2757`). The
//! expected texts are native tsgo's (vendor `5b1047d`) for the same source.

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;
use tsr_diagnostics::{Diagnostic, format::write_flattened_diagnostic_message};

/// The iteration protocol a generator annotation needs, written as a script
/// so its declarations are global.
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
            (diagnostic.message.code(), text)
        })
        .collect()
}

#[test]
fn yield_star_relates_the_delegated_element_type_to_the_yield_type() {
    // generatorTypeCheck20: the head swaps to the missing-property message.
    let actual = diagnostics(
        "class Foo { x!: number }\n\
         class Baz { z!: number }\n\
         function* g(): IterableIterator<Foo> { yield* [new Baz]; }\n",
    );
    assert_eq!(
        actual,
        [(2741, "Property 'x' is missing in type 'Baz' but required in type 'Foo'.".to_string())]
    );
}

#[test]
fn yield_star_of_an_assignable_iterable_is_silent() {
    let actual = diagnostics(
        "class Foo { x!: number }\n\
         function* g(): IterableIterator<Foo> { yield* [new Foo]; }\n",
    );
    assert_eq!(actual, []);
}

/// Native chains `Property 'x' is missing in type 'Baz'…` under the head;
/// the per-constituent chain of a union source is not built by this
/// reporter for any site, so only the head is pinned.
#[test]
fn yield_star_reports_the_element_union_at_the_operand() {
    let actual = diagnostics(
        "class Foo { x!: number }\n\
         class Baz { z!: number }\n\
         function* g(): IterableIterator<Foo> { yield* [new Baz, new Foo]; }\n",
    );
    let heads: Vec<(u32, &str)> =
        actual.iter().map(|(code, text)| (*code, text.lines().next().unwrap_or(""))).collect();
    assert_eq!(heads, [(2322, "Type 'Baz | Foo' is not assignable to type 'Foo'.")]);
}

#[test]
fn a_void_generator_annotation_is_ts2505() {
    let actual = diagnostics("function* g(): void { }\n");
    assert_eq!(actual, [(2505, "A generator cannot have a 'void' type annotation.".to_string())]);
}

#[test]
fn the_generator_instantiation_must_be_assignable_to_the_annotation() {
    // generatorTypeCheck6: reported at the annotation.
    let actual = diagnostics("function* g(): number { }\n");
    assert_eq!(
        actual,
        [(
            2322,
            "Type 'Generator<any, any, unknown>' is not assignable to type 'number'.".to_string()
        )]
    );
}

#[test]
fn an_annotation_the_instantiation_satisfies_is_silent() {
    let actual = diagnostics(
        "function* g(): Iterator<number> { yield 1; }\n\
         function* h(): Generator<number, string, boolean> { return \"\"; }\n\
         function* k(): {} { }\n",
    );
    assert_eq!(actual, []);
}

#[test]
fn a_bodiless_generator_is_not_checked() {
    // `FunctionFlagsInvalid`: an overload signature has no body, so only the
    // grammar error (TS1222) is reported, not TS2322 at `number`.
    let actual = diagnostics(
        "function* g(): number;\n\
         function* g(): any { }\n",
    );
    let codes: Vec<u32> = actual.iter().map(|(code, _)| *code).collect();
    assert_eq!(codes, [1222]);
}
