//! Element access with union keys, the object-literal key fallback, and
//! narrowing `obj[key]` through an unassigned identifier key.
//!
//! Native expectations are the declaration output of a pinned tsgo build
//! (`5b1047d1`, `--strict`), and
//! `docs/architecture/checker-99-union-key-access.md` records the native rules.
//! The circular-alias control explicitly retains the port's unresolved gap.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the last statement's declaration or function.
fn type_of_last(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    checker.set_no_implicit_any(true);
    let last = parsed.source_file.statements.last().copied().expect("a statement");
    let id = match last {
        Statement::VariableStatement(statement) => statement
            .declaration_list
            .and_then(|list| list.declarations.first().copied())
            .and_then(|declaration| declaration.node_id),
        Statement::FunctionDeclaration(function) => function.node_id,
        _ => None,
    }
    .expect("a declaration");
    let symbol = bound.symbol_of(id).expect("bound");
    let ty = checker.get_type_of_symbol(symbol);
    checker.type_to_string(ty)
}

/// `getIndexedAccessTypeOrUndefined` (`checker.go:26975`) reads each key of a
/// non-boolean union and unions the property types: `x: string | number`.
#[test]
fn a_union_key_reads_every_named_property() {
    let source = "declare const o: { a: number, b: string, c: boolean };\n\
                  declare const k: \"a\" | \"b\";\n\
                  const x = o[k];";
    assert_eq!(type_of_last(source), "string | number");
}

/// `getPropertyTypeForIndexType`'s object-literal arm (`checker.go:27134`):
/// a `string` key reads every property type plus `undefined`; under
/// `noImplicitAny` a missing literal key reads `undefined`.
#[test]
fn a_fresh_object_literal_answers_missing_keys() {
    let wide = "declare const k: string;\nconst y = { a: 1, b: \"\", c: true }[k];";
    assert_eq!(type_of_last(wide), "string | number | boolean | undefined");
    let missing = "declare const k: \"a\" | \"z\";\nconst z = { a: 1, b: \"\" }[k];";
    assert_eq!(type_of_last(missing), "number | undefined");
}

/// `isMatchingReference` (`flow.go:1629`) matches `obj[key]` with `obj[key]`
/// when `key` is a parameter that is never assigned; an assignment removes
/// the match, so `g` keeps the declared `string | undefined`.
#[test]
fn an_unassigned_parameter_key_narrows_the_access() {
    let thing = "type Thing = { a?: string, b?: number, c?: number };\n";
    let narrowed = format!(
        "{thing}function f(obj: Thing, key: keyof Thing) \
         {{ if (obj[key] !== undefined) {{ return obj[key]; }} throw 0; }}"
    );
    assert_eq!(type_of_last(&narrowed), "(obj: Thing, key: keyof Thing) => string | number");
    let assigned = format!(
        "{thing}function g(obj: Thing, key: keyof Thing) \
         {{ key = \"a\"; if (obj[key] !== undefined) {{ return obj[key]; }} throw 0; }}"
    );
    assert_eq!(type_of_last(&assigned), "(obj: Thing, key: keyof Thing) => string | undefined");
}

/// `getLiteralTypeFromPropertyName` (`checker.go:26773`): a numeric-literal
/// property name is a NUMBER literal key of a concrete `keyof`, printed after
/// the string keys in native union order.
#[test]
fn a_concrete_keyof_keeps_numeric_names_numeric() {
    let source = "declare const k: keyof { 0: string; 10: boolean; 2: number; a: number };\n\
                  const v = k;";
    assert_eq!(type_of_last(source), "\"a\" | 0 | 2 | 10");
}

/// `getIndexTypeEx` (`checker.go:26684`) intersects the deferred keys of a
/// union and unions those of an intersection, including named alias operands.
#[test]
fn generic_compounds_distribute_keyof_without_erasing_written_aliases() {
    let union = "function f<T, U>(key: keyof (T | U)) { return key; }";
    assert_eq!(type_of_last(union), "<T, U>(key: keyof (T | U)) => keyof T & keyof U");

    let intersection = "function f<T, U>(key: keyof (T & U)) { return key; }";
    assert_eq!(type_of_last(intersection), "<T, U>(key: keyof (T & U)) => keyof T | keyof U");

    let empty = "function f<T>(key: keyof (T & {})) { return key; }";
    assert_eq!(type_of_last(empty), "<T>(key: keyof (T & {})) => keyof (T & {})");

    let alias = "type Wrapped<T, U> = T | U;\n\
                 function f<T, U>(key: keyof Wrapped<T, U>) { return key; }";
    assert_eq!(type_of_last(alias), "<T, U>(key: keyof Wrapped<T, U>) => keyof T & keyof U");
}

/// Aliases substitute arguments before computing keys, including reordered
/// alias chains. A reduced body and a written parameter have distinct renders.
#[test]
fn generic_alias_keys_follow_substituted_bodies() {
    let intersection = "type Wrapped<T, U> = T & U;\n\
                        function f<T, U>(key: keyof Wrapped<T, U>) { return key; }";
    assert_eq!(type_of_last(intersection), "<T, U>(key: keyof Wrapped<T, U>) => keyof T | keyof U");
    let chain = "type Pair<A, B> = A | B; type Wrapped<T, U> = Pair<U, T>;\n\
                 function f<T, U>(key: keyof Wrapped<T, U>) { return key; }";
    // CompareTypes sorts named union constituents before getIndexTypeEx
    // intersects their keys, independently of the written argument order.
    assert_eq!(type_of_last(chain), "<T, U>(key: keyof Wrapped<T, U>) => keyof T & keyof U");
    let any = "type Wrapped<T> = T | any;\n\
               function f<T>(key: keyof Wrapped<T>) { return key; }";
    assert_eq!(type_of_last(any), "<T>(key: keyof Wrapped<T>) => string | number | symbol");
    let never = "type Wrapped<T> = T & never;\n\
                 function f<T>(key: keyof Wrapped<T>) { return key; }";
    assert_eq!(type_of_last(never), "<T>(key: keyof Wrapped<T>) => string | number | symbol");
    let unknown = "type Wrapped<T> = T & unknown;\n\
                   function f<T>(key: keyof Wrapped<T>) { return key; }";
    assert_eq!(type_of_last(unknown), "<T>(key: keyof Wrapped<T>) => keyof T");
}

/// shouldDeferIndexType tests semantic empty objects, including through an
/// alias. It must not replace `keyof (T & {})` with `keyof T`.
#[test]
fn generic_alias_keys_preserve_empty_object_deferral() {
    let alias = "type Wrapped<T> = T & {};\n\
                 function f<T>(key: keyof Wrapped<T>) { return key; }";
    assert_eq!(type_of_last(alias), "<T>(key: keyof Wrapped<T>) => keyof Wrapped<T>");
    let named_empty = "type Empty = {};\n\
                       function f<T>(key: keyof (T & Empty)) { return key; }";
    assert_eq!(type_of_last(named_empty), "<T>(key: keyof (T & Empty)) => keyof (T & Empty)");
    let nonempty = "type Tag = { a: number };\n\
                    function f<T>(key: keyof (T & Tag)) { return key; }";
    assert_eq!(type_of_last(nonempty), "<T>(key: keyof (T & Tag)) => \"a\" | keyof T");
}

/// instantiateTypeWorker recomputes the deferred index after substituting its
/// operand; retaining only the alias's printed name would leave this as error.
#[test]
fn deferred_alias_keys_resolve_after_call_instantiation() {
    let source = "type NonNull<T> = T & {};\n\
                  function f<T>(key: keyof NonNull<T>) { return key; }\n\
                  const value = f<{ a: number; b: string }>(\"a\");";
    assert_eq!(type_of_last(source), "\"a\" | \"b\"");
}

/// Invalid circular aliases are still an unresolved-type gap in this port.
/// Exposing their semantic union must not infinitely recurse computing keys.
#[test]
fn circular_alias_key_queries_remain_a_gap_without_overflowing() {
    let source = "type Cycle<T> = T | Cycle<T>;\n\
                  function f<T>(key: keyof Cycle<T>) { return key; }";
    assert_eq!(type_of_last(source), "<T>(key: keyof Cycle<T>) => any");
}

/// getLiteralTypeFromProperties preserves the index origin of an aliased
/// object, but a single-key union still collapses to its literal.
#[test]
fn generic_object_alias_keys_retain_their_origin() {
    let pair = "type Box<T> = { value: T; extra: number };\n\
                function f<T>(key: keyof Box<T>) { return key; }";
    assert_eq!(type_of_last(pair), "<T>(key: keyof Box<T>) => keyof Box<T>");
    let single = "type Box<T> = { value: T };\n\
                  function f<T>(key: keyof Box<T>) { return key; }";
    assert_eq!(type_of_last(single), "<T>(key: keyof Box<T>) => \"value\"");
}

/// `shouldDeferIndexType` requires an instantiable constituent as well as an
/// empty anonymous object. Concrete and concretely substituted mapped operands
/// therefore resolve their keys, while redundant parentheses do not change a
/// generic compound's semantic distribution.
#[test]
fn empty_object_deferral_requires_an_instantiable_compound() {
    let concrete = "declare const key: keyof ({ a: string } & {});\nconst value = key;";
    assert_eq!(type_of_last(concrete), "\"a\"");

    let mapped = "type Copy<T> = { [K in keyof T]: T[K] };\n\
                  declare const key: keyof (Copy<{ a: string }> & {});\n\
                  const value = key;";
    assert_eq!(type_of_last(mapped), "\"a\"");

    let compound = "declare const key: keyof ({ a: string } & { b: number });\n\
                    const value = key;";
    assert_eq!(type_of_last(compound), "\"a\" | \"b\"");

    let double_union = "function f<T, U>(key: keyof (((T | U)))) { return key; }";
    assert_eq!(type_of_last(double_union), "<T, U>(key: keyof (T | U)) => keyof T & keyof U");

    let double_intersection = "function f<T, U>(key: keyof (((T & U)))) { return key; }";
    assert_eq!(
        type_of_last(double_intersection),
        "<T, U>(key: keyof (T & U)) => keyof T | keyof U"
    );
}

/// `getIndexTypeEx` reduces a compound before distributing over any surviving
/// union or intersection. Absorbing `any`, `never`, and `unknown` constituents
/// therefore cannot be handled by folding the written operands independently.
#[test]
fn direct_compounds_reduce_before_key_distribution() {
    let any_union = "declare const key: keyof (any | { a: string });\nconst value = key;";
    assert_eq!(type_of_last(any_union), "string | number | symbol");

    let never_intersection =
        "declare const key: keyof (never & { a: string });\nconst value = key;";
    assert_eq!(type_of_last(never_intersection), "string | number | symbol");

    let unknown_intersection =
        "declare const key: keyof (unknown & { a: string });\nconst value = key;";
    assert_eq!(type_of_last(unknown_intersection), "\"a\"");
}
