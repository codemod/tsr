//! The relation half of `checkDestructuringAssignment` (`checker.go:12552`):
//! each leaf target of an array or object literal on the left of `=` is
//! related to its slice of the right operand by `checkReferenceAssignment`
//! (`checker.go:12704`). The expected texts are native tsgo's (vendor
//! `5b1047d`) for the same source.

use tsr_checker::{Checker, check::FileContext};
use tsr_core::Arena;
use tsr_diagnostics::{Diagnostic, format::write_flattened_diagnostic_message};

const LIB: &str = "interface SymbolConstructor { readonly iterator: unique symbol; }\n\
    declare var Symbol: SymbolConstructor;\n\
    interface IteratorYieldResult<T> { done?: false; value: T; }\n\
    interface IteratorReturnResult<T> { done: true; value: T; }\n\
    type IteratorResult<T, R = any> = IteratorYieldResult<T> | IteratorReturnResult<R>;\n\
    interface Iterator<T, R = any, N = any> { next(...[value]: [] | [N]): IteratorResult<T, R>; }\n\
    interface IterableIterator<T, R = any, N = any> extends Iterator<T, R, N> {\n\
        [Symbol.iterator](): IterableIterator<T, R, N>;\n\
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
fn array_and_object_targets_relate_their_slices() {
    let actual = diagnostics(
        "let a: string = \"\";\n\
         let o = { x: 1, y: \"s\" };\n\
         [a] = [1];\n\
         ({ x: a } = o);\n\
         ({ y: a } = o);\n",
    );
    assert_eq!(actual, [ts2322("number", "string"), ts2322("number", "string")]);
}

#[test]
fn a_shorthand_default_is_assigned_to_its_name_and_keeps_undefined() {
    // destructuringAssignmentWithDefault2: `= undefined` does not strip
    // `undefined` from the slice, and is itself assigned to `x`.
    let actual = diagnostics(
        "declare const a: { x?: number };\n\
         let x: number = 0;\n\
         ({ x = undefined } = a);\n\
         ({ x = 0 } = a);\n",
    );
    assert_eq!(actual, [ts2322("undefined", "number"), ts2322("number | undefined", "number")]);
}

#[test]
fn a_tuple_rest_target_receives_the_tuple_slice() {
    let actual = diagnostics(
        "declare let t: [number, string];\n\
         let n: number = 0;\n\
         let rest: [string] = [\"\"];\n\
         [...rest] = t;\n\
         [n, ...rest] = t;\n",
    );
    assert_eq!(actual, [ts2322("[number, string]", "[string]")]);
}

#[test]
fn for_of_heads_relate_the_iterated_element() {
    let actual = diagnostics(
        "let a: string = \"\";\n\
         declare const rows: number[][];\n\
         for ([a] of rows) {}\n",
    );
    assert_eq!(actual, [ts2322("number", "string")]);
}

/// `getFlowTypeOfDestructuring` would narrow `obj[0]` under the guard; a
/// slice whose name is accessed on the right operand in its scope is
/// declined, and an unguarded one is still related.
#[test]
fn a_possibly_narrowed_slice_is_declined() {
    let actual = diagnostics(
        "function f(obj: [number, string] | null[], other: [number | null]) {\n\
             let c0: number = 0;\n\
             if (obj[0]) { [c0] = obj; }\n\
             [c0] = other;\n\
         }\n",
    );
    assert_eq!(actual, [ts2322("number | null", "number")]);
}
