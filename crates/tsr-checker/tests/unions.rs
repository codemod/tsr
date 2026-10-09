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
        &arena,
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
fn a_nullable_constituent_is_kept_and_printed_last() {
    // Two rules at once, and the second is the one a port gets wrong.
    //
    // Kept, because `strictNullChecks` is assumed **on** — upstream's behaviour
    // whenever `strict` is unset, which is 82.6% of the corpus
    // (`GetStrictOptionValue`, `internal/core/compileroptions.go:294`). The
    // first version of this module assumed off and printed `string` here, which
    // is what an explicitly `@strict: false` case records
    // (`predicateSemantics.ts` → `>opt : number`) and not what the default does
    // (`useRegexpGroups.ts` → `>result : RegExpExecArray | null`).
    //
    // Printed **last**, against the sort: `UNDEFINED` is `1 << 2` and `NULL` is
    // `1 << 3`, so both sort to the *front* of the constituent list and upstream
    // appends them at the end (`printer.go:407`). Sorting alone would print
    // `undefined | string`, and 6,811 baseline lines end in `| undefined`.
    assert_eq!(type_of_annotation("var x: string | undefined;"), "string | undefined");
    assert_eq!(type_of_annotation("var x: string | null;"), "string | null");
    assert_eq!(type_of_annotation("var x: undefined | string;"), "string | undefined");
    // `null` before `undefined`, never the reverse — `>d : object | null | undefined`.
    assert_eq!(
        type_of_annotation("var x: undefined | object | null;"),
        "object | null | undefined"
    );
    // Only nullable constituents: everything is appended, nothing precedes.
    assert_eq!(type_of_annotation("var x: undefined | null;"), "null | undefined");
    // The boolean collapse still fires with a nullable constituent in the list,
    // which it would not if the two passes were fused.
    assert_eq!(type_of_annotation("var x: boolean | undefined;"), "boolean | undefined");
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
    // `keyof` rather than a tuple: plain tuples are ported and compute
    // (`docs/architecture/checker-notes-tuple.md`), so one no longer stands in
    // for "a constituent this port cannot type".
    with_checker("var x: string | keyof string;", |checker, _bound, statements| {
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
fn a_union_containing_a_named_union_keeps_the_origin_spelling() {
    // RE-POINTED by §53 (`checker-notes-narrow.md`): the port now builds
    // upstream's denormalised `origin` (`checker.go:25705`) for the
    // enum/plain entry shapes — the SPELLING keeps `E` unexpanded while the
    // constituent list is the flattened members, which is what lets the
    // narrowing filters project subsets.
    with_checker("enum E { A, B } var x: E | string;", |checker, _bound, statements| {
        let id = annotation_type(checker, statements, 1);
        // The origin's entries are in `CompareTypes` order (`insertType`,
        // `checker.go:25724`): `string`'s flag sorts below a union's.
        assert_eq!(checker.type_to_string(id), "string | E");
        let ty = checker.type_of(id);
        let TypeData::Union { types, .. } = &ty.data else {
            panic!("the origin union still IS a union of the flattened members");
        };
        assert_eq!(types.len(), 3, "E.A, E.B, string — members stay flattened");
    });
}

#[test]
fn origin_entries_follow_compare_types_and_print_through_format_union_types() {
    // r5-unionorder: getUnionTypeWorker's origin is `reducedTypes` plus each
    // named union placed by `insertType` (`checker.go:25724`) — a
    // `CompareTypes` order, where a type parameter (1 << 19) and an object
    // (1 << 20) sort before any union (1 << 27). An unnamed union is no
    // entry: `boolean`'s two literals join as members and formatUnionTypes
    // (`printer.go:383`) collapses them, and puts `null` then `undefined`
    // last (`unknownControlFlow.types:62`, `typeInferenceLiteralUnion`'s
    // `(T | Primitive)[]`).
    let source = "type P = undefined | null | string; enum E { A, B } \
                  function f<T>(a: P | T, b: E | null | undefined, c: boolean | E | undefined) {}";
    with_checker(source, |checker, _bound, statements| {
        let Statement::FunctionDeclaration(function) = statements[2] else {
            panic!("statement 2 must be the function");
        };
        let printed: Vec<String> = function
            .parameters
            .iter()
            .map(|parameter| {
                let id = checker.get_type_from_type_node(parameter.r#type.expect("annotated"));
                checker.type_to_string(id)
            })
            .collect();
        assert_eq!(printed, ["T | P", "E | null | undefined", "boolean | E | undefined"]);
    });
}

#[test]
fn nested_alias_origins_keep_the_outer_alias_at_reference_sites() {
    // Native nodebuilderimpl.go:3362 chooses the alias before its origin;
    // addNamedUnions likewise treats an aliased union as an atomic entry.
    with_checker(
        "type Word = 'east' | 'west'; type Route = Word | 13;
         var direct: Route; var nullable: Route | undefined;",
        |checker, bound, statements| {
            let route = declared_type_id(checker, bound, statements, 1);
            let direct = annotation_type(checker, statements, 2);
            assert_eq!(route, direct);
            let nullable = annotation_type(checker, statements, 3);
            for (id, index, expected) in [(nullable, 3, "Route | undefined"), (direct, 2, "Route")]
            {
                let reference = statements[index].node_id().expect("a reference scope");
                assert_eq!(checker.type_to_string_at(id, reference).as_deref(), Some(expected));
                assert_eq!(checker.type_to_string(id), expected);
                assert_eq!(checker.type_to_string_at(id, reference).as_deref(), Some(expected));
            }
        },
    );
}

/// Check the last variable's expression without declaration widening. This
/// exercises `checkConditionalExpression`'s `UnionReductionSubtype` road.
fn last_initializer(checker: &mut Checker, statements: &[Statement<'_>]) -> TypeId {
    let Statement::VariableStatement(statement) = statements.last().expect("a statement") else {
        panic!("last statement must declare a variable");
    };
    let initializer = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initializer");
    checker.check_expression(initializer)
}

#[test]
fn subtype_elimination_precedes_named_origin_construction() {
    // Native removeSubtypes compares flattened constituents, then
    // getUnionTypeWorker restores Choice only when all its members survive.
    // An unreduced Choice | Rich origin used to gap before comparison.
    for branches in ["choice : rich", "rich : choice"] {
        let source = format!(
            "interface A {{ tag: 'a' }} interface B {{ tag: 'b' }}
             interface Rich extends A {{ extra: number }}
             type Choice = A | B;
             declare const choice: Choice; declare const rich: Rich;
             const result = true ? {branches};"
        );
        with_checker(&source, |checker, bound, statements| {
            let alias = declared_type_id(checker, bound, statements, 3);
            let result = last_initializer(checker, statements);
            assert_eq!(result, alias, "fully surviving members recover the alias identity");
            assert_eq!(checker.type_to_string(result), "Choice");
        });
    }
}

#[test]
fn an_enum_is_a_named_union_in_a_subtype_reduced_origin() {
    // getDeclaredTypeOfEnum gives the enum's union an alias
    // (checker.go:23899), so addNamedUnions keeps `E` as an origin entry and
    // insertType places it after the object (`1 << 20` before `1 << 27`):
    // `logicalOrOperatorWithEveryType.types:373` records `{ a: string; } | E`.
    with_checker(
        "enum E { a, b, c }
         declare const o: { a: string } | undefined; declare const e: E;
         const result = o || e;",
        |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), "{ a: string; } | E");
        },
    );
}

#[test]
fn a_partly_removed_named_union_does_not_resurrect_its_alias() {
    with_checker(
        "interface Broad { tag: 'a' }
         interface Narrow extends Broad { extra: number }
         interface Other { tag: 'b' }
         type Choice = Narrow | Other;
         declare const choice: Choice; declare const broad: Broad;
         const result = true ? choice : broad;",
        |checker, bound, statements| {
            let narrow = checker.get_declared_type_of_symbol(
                bound.symbol_of(statements[1].node_id().unwrap()).unwrap(),
            );
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), "Broad | Other");
            let TypeData::Union { types, .. } = &checker.type_of(result).data else {
                panic!("two survivors must remain");
            };
            assert_eq!(types.len(), 2);
            assert!(!types.contains(&narrow), "the eliminated alias member must stay eliminated");
        },
    );
}

#[test]
fn disjoint_named_unions_keep_aliases_in_native_union_order() {
    with_checker(
        "type Words = 'a' | 'b'; type Numbers = 1 | 2;
         declare const words: Words; declare const numbers: Numbers;
         const result = true ? words : numbers;",
        |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            // Both origins have UNION flags: CompareTypes sorts their alias
            // names, not their first flattened primitive's flag value.
            assert_eq!(checker.type_to_string(result), "Numbers | Words");
            let TypeData::Union { types, .. } = &checker.type_of(result).data else {
                panic!("origins must retain flattened semantic members");
            };
            assert_eq!(types.len(), 4);
        },
    );
}

#[test]
fn overlapping_named_unions_have_no_denormalized_origin() {
    with_checker(
        "type AB = 'a' | 'b'; type BC = 'b' | 'c';
         declare const ab: AB; declare const bc: BC;
         const result = true ? ab : bc;",
        |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), "\"a\" | \"b\" | \"c\"");
            let TypeData::Union { types, .. } = &checker.type_of(result).data else {
                panic!("three distinct members");
            };
            assert_eq!(types.len(), 3, "shared b must occur once");
        },
    );
}

#[test]
fn a_named_object_union_survives_beside_an_unrelated_literal() {
    with_checker(
        "type Choice = { tag: 'red'; payload: string } | { tag: 'blue'; payload: number };
         declare const choice: Choice;
         const result = true ? choice : 'other';",
        |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), "\"other\" | Choice");
            let TypeData::Union { types, .. } = &checker.type_of(result).data else {
                panic!("the origin is not the member set");
            };
            assert_eq!(types.len(), 3);
        },
    );
}

#[test]
fn disjoint_unit_properties_bypass_an_undecidable_protected_relation() {
    // removeSubtypes' key precheck (checker.go:25970) rejects the pair without
    // needing isValidOverrideOf for the protected member. The interface sources
    // deliberately do not take the reducer's separate class-derivation gate.
    with_checker(
        "class Red { protected tag: 'red' = 'red' }
         class Blue { protected tag: 'blue' = 'blue' }
         interface RedLike extends Red {} interface BlueLike extends Blue {}
         declare const red: RedLike; declare const blue: BlueLike;
         const result = true ? red : blue;",
        |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), "BlueLike | RedLike");
            let TypeData::Union { types, .. } = &checker.type_of(result).data else {
                panic!("different unit properties preserve both constituents");
            };
            assert_eq!(types.len(), 2);
        },
    );
}

#[test]
fn an_alias_covering_every_survivor_keeps_its_original_identity() {
    // Native's one-named-union/no-uncovered-types exit precedes its origin
    // count check. Requiring every original member to survive loses Choice.
    with_checker(
        "interface A { id: number } interface Rich extends A { extra: string }
         type Choice = A | Rich;
         declare const choice: Choice; declare const a: A;
         const result = true ? choice : a;",
        |checker, bound, statements| {
            let alias = declared_type_id(checker, bound, statements, 2);
            let result = last_initializer(checker, statements);
            assert_eq!(result, alias);
            assert_eq!(checker.type_to_string(result), "Choice");
        },
    );
}

#[test]
fn resolved_empty_interfaces_enable_primitive_subtype_elimination() {
    // String's apparent wrapper is a real prerequisite for the primitive
    // relation in this one-file harness. Its nonempty member list also ensures
    // that only Empty, not the source wrapper, licenses primitive candidacy.
    with_checker(
        "interface String { length: number }
         interface Empty {} interface DerivedEmpty extends Empty {}
         declare const text: string; declare const empty: DerivedEmpty;
         const result = true ? text : empty;",
        |checker, _bound, statements| {
            let empty = annotation_type(checker, statements, 4);
            let result = last_initializer(checker, statements);
            assert_eq!(result, empty, "eligibility must not canonicalize the interface to {{}}");
            assert_eq!(checker.type_to_string(result), "DerivedEmpty");
        },
    );
}

#[test]
fn signatures_and_indexes_make_zero_property_objects_nonempty() {
    for (name, body) in [
        ("CallOnly", "(): number"),
        ("ConstructOnly", "new(): { n: number }"),
        ("Indexed", "[key: string]: number"),
    ] {
        let source = format!(
            "interface String {{ length: number }} interface {name} {{ {body} }}
             declare const text: string; declare const other: {name};
             const result = true ? text : other;"
        );
        with_checker(&source, |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), format!("string | {name}"));
            let TypeData::Union { types, .. } = &checker.type_of(result).data else {
                panic!("nonempty objects must not absorb the primitive");
            };
            assert_eq!(types.len(), 2);
        });
    }
}

#[test]
fn resolved_empty_interfaces_do_not_absorb_strict_nullable_types() {
    for (nullable, expected) in [("null", "Empty | null"), ("undefined", "Empty | undefined")] {
        let source = format!(
            "interface Empty {{}} declare const empty: Empty;
             declare const nullable: {nullable}; const result = true ? nullable : empty;"
        );
        with_checker(&source, |checker, _bound, statements| {
            let result = last_initializer(checker, statements);
            assert_eq!(checker.type_to_string(result), expected);
        });
    }
}

#[test]
fn reduced_singleton_aliases_reuse_the_existing_constituent_identity() {
    // Native 5b1047d returns the sole reduced constituent before attaching
    // the new alias. Primitive and nominal singletons need no wrapper;
    // repeated inputs only collapse when their normalized set is singleton.
    for (body, bare, expected) in [
        ("number | 17", "number", "number"),
        ("17 & number", "17", "17"),
        ("'east' & string", "'east'", "\"east\""),
        ("Shape | never", "Shape", "Shape"),
        ("Shape & Shape", "Shape", "Shape"),
        ("17 | 17", "17", "17"),
        ("'east' | 'east'", "'east'", "\"east\""),
        ("('east' & 'west') | Shape", "Shape", "Shape"),
    ] {
        for reverse in [false, true] {
            let source = format!(
                "interface Shape {{ tag: 'shape' }}
                 type Word = 'east' | 'west';
                 type Brand = 'east' & {{ readonly side: 'east' }};
                 type Single = {body};
                 declare let bare: {bare}; declare let aliased: Single;"
            );
            with_checker(&source, |checker, bound, statements| {
                let (first, second) = if reverse { (5, 4) } else { (4, 5) };
                let left = annotation_type(checker, statements, first);
                let right = annotation_type(checker, statements, second);
                assert_eq!(left, right, "{body}, reverse={reverse}");
                assert_eq!(checker.type_to_string(left), expected, "{body}");
                assert_eq!(declared_type_id(checker, bound, statements, 3), left);
                assert_eq!(annotation_type(checker, statements, first), left);
                assert_eq!(annotation_type(checker, statements, second), left);
            });
        }
    }
}

#[test]
fn source_brand_identity_stays_distinct_from_its_reduced_primitive() {
    for primitive in ["string", "number", "bigint", "`west-${string}`"] {
        for reverse in [false, true] {
            let source = format!(
                "type Brand = {primitive} & {{}};
                 declare let bare: {primitive}; declare let branded: Brand;"
            );
            with_checker(&source, |checker, bound, statements| {
                let (first, second) = if reverse { (2, 1) } else { (1, 2) };
                let first_id = annotation_type(checker, statements, first);
                let second_id = annotation_type(checker, statements, second);
                assert_ne!(first_id, second_id, "{primitive}, reverse={reverse}");
                let brand = declared_type_id(checker, bound, statements, 0);
                assert_eq!(checker.type_to_string(brand), "Brand");
                assert!(
                    matches!(&checker.type_of(brand).data, TypeData::Intersection { types, .. } if types.len() == 2)
                );
                for _ in 0..3 {
                    assert_eq!(annotation_type(checker, statements, first), first_id);
                    assert_eq!(annotation_type(checker, statements, second), second_id);
                }
            });
        }
    }
}

#[test]
fn repeated_multi_constituent_inputs_keep_the_new_alias_after_flattening() {
    // Native's singleton check is after flattening/reduction: Word | Word
    // and Brand & Brand still have two constituents, unlike literal repeats.
    for (body, bare) in [("Word | Word", "Word"), ("Brand & Brand", "Brand")] {
        let source = format!(
            "type Word = 'east' | 'west';
             type Brand = 'east' & {{ readonly side: 'east' }};
             type Repeated = {body};
             declare let bare: {bare}; declare let aliased: Repeated;"
        );
        with_checker(&source, |checker, _bound, statements| {
            let bare = annotation_type(checker, statements, 3);
            let aliased = annotation_type(checker, statements, 4);
            assert_ne!(bare, aliased, "{body}");
            assert_eq!(checker.type_to_string(aliased), "Repeated", "{body}");
        });
    }
}

#[test]
fn singleton_reduction_preserves_written_signature_aliases() {
    // The semantic alias becomes number/17, while native's declaration
    // serializer still reuses the written annotation at signature slots.
    with_checker(
        "interface Shape { tag: 'shape' }
         type NumberUnion = number | 17;
         type LiteralIntersection = 17 & number;
         type ShapeUnion = Shape | never;
         declare function preserveSource(value: NumberUnion, item: LiteralIntersection): ShapeUnion;",
        |checker, bound, statements| {
            let Statement::FunctionDeclaration(function) = statements[4] else {
                panic!("function")
            };
            let symbol = bound.symbol_of(function.node_id.unwrap()).unwrap();
            let ty = checker.get_type_of_symbol(symbol);
            assert_eq!(
                checker.type_to_string_at(ty, function.node_id.unwrap()).as_deref(),
                Some("(value: NumberUnion, item: LiteralIntersection) => ShapeUnion"),
            );
        },
    );
}
