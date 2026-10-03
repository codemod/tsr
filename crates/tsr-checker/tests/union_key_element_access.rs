//! Element access with union keys, the object-literal key fallback, and
//! narrowing `obj[key]` through an unassigned identifier key.
//!
//! Every expectation is the declaration output of a pinned tsgo build
//! (`5b1047d1`, `--strict`), and
//! `docs/architecture/checker-99-union-key-access.md` records the native rules.

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
