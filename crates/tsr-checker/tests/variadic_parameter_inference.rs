//! Variadic tuple inference and substitution, following `inferFromObjectTypes`
//! and `instantiateTypeWorker` in typescript-go. Fixed prefixes and suffixes
//! constrain the inferred middle; indexed accesses resolve after substitution.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the named variable's initialiser.
fn type_of_initialiser(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for statement in parsed.source_file.statements {
        let Statement::VariableStatement(node) = statement else { continue };
        for declaration in node.declaration_list.map(|list| list.declarations).unwrap_or_default() {
            let Some(tsr_ast::BindingName::Identifier(identifier)) = declaration.name else {
                continue;
            };
            if identifier.text != name {
                continue;
            }
            let initialiser = declaration.initializer.expect("an initialiser");
            let id = checker.check_expression(initialiser);
            return checker.type_to_string(id);
        }
    }
    panic!("no declaration named {name}");
}

const LIB: &str = "interface Array<T> { length: number }\n\
                   interface ReadonlyArray<T> { length: number }\n";

#[test]
fn generic_array_spreads_keep_their_indexed_element_type() {
    let source = format!(
        "{LIB}function f<T extends readonly unknown[]>(t: T) {{ return [...t]; }}\n\
         declare const t: readonly [1, 2]; const a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "(1 | 2)[]");
}

#[test]
fn function_parameters_infer_a_named_rest_tuple() {
    let source = format!(
        "{LIB}declare function f<A extends unknown[], R>(fn: (...args: A) => R): A;\n\
         const a = f((x: number, y: string) => true);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[x: number, y: string]");
}

#[test]
fn an_array_rest_in_a_function_remains_unbounded_during_inference() {
    let source = format!(
        "{LIB}declare function f<A extends unknown[]>(fn: (...args: A) => void): A;\n\
         const a = f((x: number, ...rest: string[]) => {{}});"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[x: number, ...rest: string[]]");
}

#[test]
fn a_fixed_function_prefix_is_not_included_in_the_inferred_rest() {
    let source = format!(
        "{LIB}declare function f<A extends unknown[]>(fn: (first: boolean, ...args: A) => void): A;\n\
         const a = f((first: boolean, x: number, y: string) => {{}});"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[x: number, y: string]");
}

#[test]
fn a_constrained_generic_function_infers_from_its_base_signature() {
    let source = format!(
        "{LIB}declare function f<T>(fn: (x: T) => T): T;\n\
         declare function g<U extends number>(x: U): U; const a = f(g);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

#[test]
fn a_generic_callback_reads_fixed_outer_inferences_before_its_own_inference() {
    let source =
        "declare function apply<T, U, V>(first: T, second: U, callback: (a: T, b: U) => V): V;
        declare function same<W>(first: W, second: W): W;
        const observed = apply(1, 1, same);";
    assert_eq!(type_of_initialiser(source, "observed"), "number");
}

#[test]
fn a_generic_contextual_return_preserves_its_own_type_parameter() {
    let source = format!(
        "{LIB}declare function wrap<A, B>(f: (a: A) => B): (a: A) => B;\n\
         const a: <A>(x: A) => A[] = wrap(x => [x]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "(a: A) => A[]");
}

#[test]
fn a_wrapper_propagates_its_function_arguments_type_parameter() {
    let source = format!(
        "{LIB}declare function wrap<A, B>(f: (a: A) => B): (a: A) => B;\n\
         declare function list<T>(x: T): T[]; const a = wrap(list);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "<T>(a: T) => T[]");
}

#[test]
fn a_propagated_function_can_be_called_with_a_concrete_argument() {
    let source = format!(
        "{LIB}declare function wrap<A, B>(f: (a: A) => B): (a: A) => B;\n\
         declare function list<T>(x: T): T[]; const wrapped = wrap(list); const a = wrapped(1);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "number[]");
}

#[test]
fn a_generic_composition_context_types_its_later_callback() {
    let source = format!(
        "{LIB}declare function pipe<A extends unknown[], B, C>(f: (...args: A) => B, g: (b: B) => C): (...args: A) => C;\n\
         declare function list<T>(x: T): T[]; const a = pipe(list, x => x.length);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "<T>(x: T) => number");
}

#[test]
fn an_overloaded_generic_composition_context_types_its_later_callback() {
    let source = format!(
        "{LIB}declare function pipe<A extends unknown[], B>(f: (...args: A) => B): (...args: A) => B;\n\
         declare function pipe<A extends unknown[], B, C>(f: (...args: A) => B, g: (b: B) => C): (...args: A) => C;\n\
         declare function list<T>(x: T): T[]; const a = pipe(list, x => x.length);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "<T>(x: T) => number");
}

#[test]
fn returned_variadic_tuple_splices_concrete_type_arguments() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [string, ...T, number];\n\
         const a = f<[boolean, string]>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[string, boolean, string, number]");
}

#[test]
fn returned_variadic_tuple_substitutes_array_type_arguments() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [string, ...T];\n\
         const a = f<number[]>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[string, ...number[]]");
}

#[test]
fn returned_variadic_tuple_preserves_optional_suffix() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [...T, boolean?];\n\
         const a = f<[string, number]>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[string, number, boolean?]");
}

#[test]
fn returned_variadic_tuple_distributes_a_union_argument() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [number, ...T];\n\
         const a = f<[string] | [boolean]>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string] | [number, boolean]");
}

#[test]
fn returned_variadic_tuple_with_never_argument_is_never() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [number, ...T];\n\
         const a = f<never>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "never");
}

#[test]
fn a_variadic_tuple_index_type_resolves_after_substitution() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [string, ...T][1];\n\
         const a = f<[number]>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

#[test]
fn a_variadic_tuple_fixed_index_type_is_already_known() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(): [string, ...T][0];\n\
         const a = f<[number]>();"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "string");
}

#[test]
fn a_generic_variadic_element_access_resolves_after_inference() {
    let source = format!(
        "{LIB}function f<T extends unknown[]>(t: [string, ...T]) {{ return t[1]; }}\n\
         declare const t: [string, number]; const a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

/// The head case. Both halves are needed: without the contextual half this
/// answers `number[]`, without the inference half it answers `error`.
#[test]
fn a_variadic_parameter_infers_the_argument_as_a_tuple() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [...T]): T;\nconst a = f([1, 2]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, number]");
}

/// Mixed element types keep their positions, which is what distinguishes a
/// tuple answer from the widened array.
#[test]
fn the_inferred_tuple_keeps_element_positions() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [...T]): T;\n\
         const a = f([1, \"x\", true]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string, boolean]");
}

/// A concrete tuple argument worked before this and must keep working — it
/// reaches `T` through the ordinary reference road, not through either half.
#[test]
fn a_concrete_tuple_argument_still_infers() {
    let source = format!(
        "{LIB}declare const t: [number, string];\n\
         declare function f<T extends unknown[]>(x: T): T;\nconst a = f(t);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string]");
}

/// The plain array parameter is untouched — `T[]` infers the ELEMENT, and
/// nothing here may change that.
#[test]
fn a_plain_array_parameter_still_infers_the_element() {
    let source = format!("{LIB}declare function f<T>(t: T[]): T;\nconst a = f([1, 2]);");
    assert_eq!(type_of_initialiser(&source, "a"), "number");
}

/// `inferFromObjectTypes` infers the slice between fixed prefix/suffix elements.
#[test]
fn a_variadic_with_a_leading_element_infers_the_remaining_slice() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [string, ...T]): T;\n\
         const a = f([\"s\", 1]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number]");
}

#[test]
fn a_variadic_with_fixed_prefix_and_suffix_infers_the_middle() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [string, ...T, boolean]): T;\n\
         const a = f([\"s\", 1, \"x\", true]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[number, string]");
}

#[test]
fn an_empty_middle_infers_an_empty_tuple() {
    let source = format!(
        "{LIB}declare function f<T extends unknown[]>(t: [string, ...T, boolean]): T;\n\
         const a = f([\"s\", true]);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "[]");
}

#[test]
fn a_generic_function_argument_uses_the_contextual_return_parameters() {
    let source = format!(
        "{LIB}declare function wrap<A, B>(f: (a: A) => B): (a: A) => B;\n\
         declare function list<T>(x: T): T[]; const a: <A>(x: A) => A[] = wrap(list);"
    );
    assert_eq!(type_of_initialiser(&source, "a"), "(a: A) => A[]");
}

#[test]
fn conflicting_contextual_signature_inputs_keep_the_error_recovery_type() {
    let source = "declare function f<U>(cb: (a: number, b: string) => U): U;\n\
                  declare function g<T>(a: T, b: T): T; const a = f(g);";
    assert_eq!(type_of_initialiser(source, "a"), "unknown");
}
