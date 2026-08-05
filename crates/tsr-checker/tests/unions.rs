//! What a union type must get right.
//!
//! Every assertion here is about a **printed** union, because the constituent
//! order is compared verbatim in every `.types` baseline. A test whose
//! constituents happen to be in source order proves nothing about the ordering
//! rule, so the fixtures below deliberately write them in the wrong order.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Unions, and `boolean` stops being an intrinsic".

use tsr_ast::Statement;
use tsr_checker::{Checker, TypeData, TypeFlags, TypeId};
use tsr_core::Arena;

/// Parse, bind and check one source, then answer a question about it.
///
/// A callback rather than a returned struct: the checker borrows the arena, the
/// parse result and the bind result, and all three have to outlive it.
fn with_checker<R>(
    source: &str,
    ask: impl for<'a> FnOnce(&mut Checker<'a, '_>, &tsr_binder::BindResult<'_>, &[Statement<'a>]) -> R,
) -> R {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    ask(&mut checker, &bound, parsed.source_file.statements)
}

/// The type of the annotation on the `index`th statement's first declaration.
fn annotation_type<'a>(
    checker: &mut Checker<'a, '_>,
    statements: &[Statement<'a>],
    index: usize,
) -> TypeId {
    let Statement::VariableStatement(statement) = statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    checker.get_type_from_type_node(annotation)
}

/// The printed type of the first statement's annotation.
fn type_of_annotation(source: &str) -> String {
    with_checker(source, |checker, _bound, statements| {
        let id = annotation_type(checker, statements, 0);
        checker.type_to_string(id)
    })
}

/// The printed type a type declaration — a type alias or an enum — declares.
fn declared_type(source: &str) -> String {
    with_checker(source, |checker, bound, statements| {
        let id = declared_type_id(checker, bound, statements, 0);
        checker.type_to_string(id)
    })
}

fn declared_type_id(
    checker: &mut Checker,
    bound: &tsr_binder::BindResult<'_>,
    statements: &[Statement<'_>],
    index: usize,
) -> TypeId {
    let node = match statements[index] {
        Statement::TypeAliasDeclaration(node) => node.node_id,
        Statement::EnumDeclaration(node) => node.node_id,
        _ => panic!("statement {index} must declare a type"),
    }
    .expect("a registered node");
    let symbol = bound.symbol_of(node).expect("a symbol");
    checker.get_declared_type_of_symbol(symbol)
}

#[test]
fn constituents_are_ordered_by_type_flags_and_not_by_source_order() {
    // `STRING` is `1 << 5` and `NUMBER` is `1 << 6`, so `string` comes first
    // whichever way round the source writes it. Upstream records exactly this:
    // `function fn(x: number | string)` produces `>x : string | number`
    // (`baselines/reference/submodule/compiler/implicitConstParameters.types:15`).
    assert_eq!(type_of_annotation("var x: number | string;"), "string | number");
    assert_eq!(type_of_annotation("var x: string | number;"), "string | number");
    // Three constituents spanning a wider range of the flag values, again
    // written backwards: `object` is `1 << 17`, `symbol` is `1 << 9`.
    assert_eq!(type_of_annotation("var x: object | symbol | string;"), "string | symbol | object");
}

#[test]
fn numeric_literal_constituents_are_ordered_by_value_and_not_by_spelling() {
    // The distinction only shows where the two disagree: by text `"10"` sorts
    // before `"9"`, by value it does not.
    assert_eq!(type_of_annotation("var x: 10 | 9 | 1;"), "1 | 9 | 10");
}

#[test]
fn a_literal_is_absorbed_by_its_base_primitive() {
    // `removeRedundantLiteralTypes` (`checker.go:25838`). Without it the answer
    // would be `string | "a"` — a plausible-looking line that fails.
    assert_eq!(type_of_annotation(r#"var x: string | "a";"#), "string");
    assert_eq!(type_of_annotation("var x: number | 1 | 2;"), "number");
    // …and only its *own* base: a string literal survives beside `number`.
    // The order here was predicted wrong and the implementation was right —
    // `NUMBER` is `1 << 6` and `STRING_LITERAL` is `1 << 10`, so the literal
    // comes second. Upstream records `>x : number | "bar"`.
    assert_eq!(type_of_annotation(r#"var x: number | "a";"#), r#"number | "a""#);
}

#[test]
fn a_repeated_constituent_is_one_constituent() {
    assert_eq!(type_of_annotation("var x: string | string;"), "string");
    assert_eq!(type_of_annotation("var x: string | number | string;"), "string | number");
}

#[test]
fn never_is_dropped_from_a_union() {
    // "We ignore 'never' types in unions" (`checker.go:25767`). `NEVER` is
    // `1 << 18`, so failing to drop it would print `string | never`.
    assert_eq!(type_of_annotation("var x: string | never;"), "string");
}

#[test]
fn a_nullable_constituent_is_dropped_because_strict_null_checks_is_assumed_off() {
    // Not an approximation: this is upstream's behaviour under its *default*
    // options (`checker.go:25793`), which is what 89% of the corpus runs with.
    // `UNDEFINED` is `1 << 2`, so keeping it would print `undefined | string`.
    // See the module docs of `crate::unions` for why the option is assumed
    // rather than read, and what would show the assumption to be wrong.
    assert_eq!(type_of_annotation("var x: string | undefined;"), "string");
    assert_eq!(type_of_annotation("var x: string | null;"), "string");
}

#[test]
fn boolean_is_the_union_of_the_two_boolean_literal_types() {
    with_checker("var x: true | false;", |checker, _bound, statements| {
        let id = annotation_type(checker, statements, 0);
        // It prints as the keyword…
        assert_eq!(checker.type_to_string(id), "boolean");
        // …and it *is* `booleanType`, not a lookalike: writing `true | false`
        // reaches the same interned union the checker built at startup.
        assert_eq!(id, checker.intrinsics().boolean);
        let ty = checker.type_of(id);
        assert!(ty.flags.contains(TypeFlags::UNION), "boolean must be a union");
        assert!(ty.flags.contains(TypeFlags::BOOLEAN), "and carry the keyword flag");
    });
}

#[test]
fn boolean_flattens_into_a_union_because_it_is_one() {
    // The observable consequence of `boolean` no longer being an intrinsic: its
    // constituents are merged into the surrounding union, so `true` is already
    // there. An intrinsic `boolean` would print `true | boolean` here.
    assert_eq!(type_of_annotation("var x: boolean | true;"), "boolean");
    assert_eq!(type_of_annotation("var x: string | boolean;"), "string | boolean");
}

#[test]
fn the_same_union_written_twice_is_one_type() {
    // Interning is not an optimisation: the first relation check written will
    // compare two handles that must be equal. Written in different orders, and
    // one of them with a redundant constituent, to prove the key is the reduced
    // sorted list rather than the source text.
    with_checker(
        "var a: string | number; var b: number | string; var c: string | number | string;",
        |checker, _bound, statements| {
            let first = annotation_type(checker, statements, 0);
            let second = annotation_type(checker, statements, 1);
            let third = annotation_type(checker, statements, 2);
            assert_eq!(first, second, "order in source must not create a second type");
            assert_eq!(first, third, "nor must a duplicated constituent");
        },
    );
}

#[test]
fn a_constituent_this_port_cannot_type_makes_the_whole_union_a_gap() {
    // `A | Unported` is not `A | any`. This is upstream's own reduction rather
    // than a deviation: `errorType` carries `TypeFlagsAny`, and a union that
    // includes an error answers `errorType` (`checker.go:25659`).
    with_checker("var x: string | [number, number];", |checker, _bound, statements| {
        let id = annotation_type(checker, statements, 0);
        assert_eq!(id, checker.intrinsics().error, "an unported constituent must gap the union");
        assert_ne!(id, checker.intrinsics().any, "and must not be anyType");
    });
}

#[test]
fn a_type_alias_names_its_union_where_a_bare_union_prints_its_constituents() {
    // `type T8 = string | boolean` records `>T8 : T8` while a variable annotated
    // with the same constituents records them
    // (`baselines/reference/submodule/conformance/typeAliases.types:76`).
    assert_eq!(declared_type("type T = string | number;"), "T");
    assert_eq!(type_of_annotation("var x: string | number;"), "string | number");
    // The two are *different types* that a naive port would intern together:
    // the alias symbol is part of the key.
    with_checker(
        "type T = string | number; var x: string | number;",
        |checker, bound, statements| {
            let alias = declared_type_id(checker, bound, statements, 0);
            let bare = annotation_type(checker, statements, 1);
            assert_ne!(alias, bare, "the alias symbol must be part of the union's identity");
        },
    );
    // A parenthesised body still finds its alias (`checker.go:23721`).
    assert_eq!(declared_type("type T = (string | number);"), "T");
    // A non-union alias body stays transparent, which the alias handling here
    // must not have disturbed: `type T1 = number` records `>T1 : number`.
    assert_eq!(declared_type("type T = number;"), "number");
}

#[test]
fn an_enum_declares_a_real_union_that_prints_as_the_enum_name() {
    // The divergence this replaces printed `E` from a type with no constituents.
    // Printing alone therefore cannot tell the two apart — the assertion has to
    // be about the type.
    with_checker("enum E { A, B }", |checker, bound, statements| {
        let id = declared_type_id(checker, bound, statements, 0);
        assert_eq!(checker.type_to_string(id), "E", "an enum still prints as its name");
        let ty = checker.type_of(id);
        assert!(ty.flags.contains(TypeFlags::UNION), "and is now a union");
        assert!(ty.flags.contains(TypeFlags::ENUM_LITERAL), "flagged as an enum union");
        let TypeData::Union { types, symbol, .. } = &ty.data else {
            panic!("an enum's declared type must be a union");
        };
        assert_eq!(types.len(), 2, "one constituent per member");
        assert!(symbol.is_some(), "printing as `E` must come from the symbol");
        let members = types.iter().map(|&id| checker.type_to_string(id)).collect::<Vec<_>>();
        assert_eq!(members, ["E.A", "E.B"], "members print qualified, in declaration order");
    });
}

#[test]
fn a_union_containing_a_named_union_is_a_gap_rather_than_an_expansion() {
    // Upstream keeps `E` unexpanded through a denormalised `origin`
    // (`checker.go:25705`), which this port does not build. Expanding it would
    // print `E.A | E.B | string` where upstream prints `E | string` — a wrong
    // line rather than a missing one.
    with_checker("enum E { A, B } var x: E | string;", |checker, _bound, statements| {
        let id = annotation_type(checker, statements, 1);
        assert_eq!(id, checker.intrinsics().error);
    });
}
