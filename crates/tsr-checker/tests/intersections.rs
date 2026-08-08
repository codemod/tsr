//! What an intersection type must get right.
//!
//! The rule that separates a correct port from a plausible one is that an
//! intersection is **not** ordered like a union: its constituents stay in source
//! order. So every fixture here writes them in an order a union would reorder.
//!
//! See [`docs/architecture/checker.md`](../../../docs/architecture/checker.md),
//! "Intersections: source order, and the reduction that needs no assignability".

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn type_of_annotation_at(source: &str, index: usize) -> String {
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
    let Statement::VariableStatement(statement) = parsed.source_file.statements[index] else {
        panic!("statement {index} must be a variable statement");
    };
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

/// Two interfaces, then the annotation under test.
fn with_two_interfaces(annotation: &str) -> String {
    type_of_annotation_at(&format!("interface A {{}}\ninterface B {{}}\nvar x: {annotation};"), 2)
}

#[test]
fn constituents_keep_source_order_where_a_union_would_sort_them() {
    // The whole point. `B & A` stays `B & A`; the same names in a union sort by
    // `TypeFlags` and then by name. Both spellings appear in the baselines,
    // which a sort would make impossible.
    assert_eq!(with_two_interfaces("A & B"), "A & B");
    assert_eq!(with_two_interfaces("B & A"), "B & A");
    // A primitive does not migrate to the front either — `STRING` is `1 << 5`
    // and would sort before an object type. Upstream records `>x : string & T`.
    assert_eq!(with_two_interfaces("A & string"), "A & string");
    assert_eq!(with_two_interfaces("string & A"), "string & A");
}

#[test]
fn a_union_constituent_is_parenthesised() {
    // `T & ({} | null)` is the shape the baselines record. Without the
    // parentheses the printed form reassociates and means something else.
    assert_eq!(with_two_interfaces("A & (string | number)"), "A & (string | number)");
    // …and the union inside is still sorted by *its* rule, so the two orders
    // coexist in one printed type.
    assert_eq!(with_two_interfaces("A & (number | string)"), "A & (string | number)");
}

#[test]
fn disjoint_domains_reduce_to_never_without_needing_assignability() {
    // Pure flag arithmetic over `TypeFlagsDisjointDomains`. Upstream records
    // `never` for exactly this source (`switchCaseWithIntersectionType`).
    assert_eq!(with_two_interfaces("string & number"), "never");
    assert_eq!(with_two_interfaces("number & string"), "never");
    assert_eq!(with_two_interfaces("string & boolean"), "never");
    assert_eq!(with_two_interfaces("object & string"), "never");
    // Two distinct *unit* types are the same rule, reached by upstream's trick
    // of adding `NON_PRIMITIVE` to `includes` so the disjoint test fires.
    assert_eq!(with_two_interfaces(r#""a" & "b""#), "never");
    // …but one unit type twice is not two distinct ones.
    assert_eq!(with_two_interfaces(r#""a" & "a""#), r#""a""#);
    // And an object type beside a primitive is *not* disjoint — `OBJECT` is not
    // one of the domains, which is why `A & string` survives above.
    assert_ne!(with_two_interfaces("A & string"), "never");
}

#[test]
fn never_anywhere_makes_the_whole_intersection_never() {
    assert_eq!(with_two_interfaces("A & never"), "never");
    assert_eq!(with_two_interfaces("never & A"), "never");
}

#[test]
fn a_primitive_is_removed_beside_its_own_literal() {
    // `removeRedundantSupertypes` — the mirror of the union rule, which drops
    // the *literal* beside its primitive. Here the primitive goes.
    assert_eq!(with_two_interfaces(r#"string & "a""#), r#""a""#);
    assert_eq!(with_two_interfaces(r#""a" & string"#), r#""a""#);
    assert_eq!(with_two_interfaces("number & 1"), "1");
}

#[test]
fn duplicates_collapse_and_nested_intersections_flatten() {
    assert_eq!(with_two_interfaces("A & A"), "A");
    assert_eq!(with_two_interfaces("A & B & A"), "A & B");
    // A parenthesised intersection is flattened into its parent, and the order
    // of first appearance is what survives.
    assert_eq!(with_two_interfaces("A & (B & A)"), "A & B");
}

#[test]
fn a_constituent_this_port_cannot_type_makes_the_whole_intersection_a_gap() {
    // `A & Unported` must not print as `A`. This is upstream's own reduction
    // rather than a deviation: `errorType` carries `ANY`, and `IncludesError`
    // wins over `IncludesAny` (`checker.go:26092`).
    // `bd tsr-eep`: an **unresolved** name now prints itself, because
    // upstream reports `Cannot find name` and renders the name anyway. So an
    // unresolved reference is no longer an example of "a type this port cannot
    // compute"; a **tuple** still is, and is used instead. The rule under test
    // is unchanged.
    assert_eq!(with_two_interfaces("A & keyof string"), "error");
    // These still gap, and that is the `bd tsr-eep` design rather than an
    // oversight: the minted type answers `Checker::is_error`, so every
    // *consumer* keeps propagating and only the line rendering the reference
    // itself changes. Upstream prints `A & Unresolvable` here; this port does
    // not, and the divergence is confined to the consumers.
    assert_eq!(with_two_interfaces("A & Unresolvable"), "error");
    assert_eq!(with_two_interfaces("Unresolvable & A"), "error");
}

#[test]
fn an_empty_object_constituent_is_a_gap() {
    // `X & {}` has reduction rules of its own upstream — it deliberately skips
    // supertype reduction, and `{}` is removed beside a definitely-non-nullable
    // type. Neither is ported, so the intersection is a gap rather than a
    // plausible `A & {}` or `A`.
    assert_eq!(with_two_interfaces("A & {}"), "error");
    // A *non*-empty object literal type is unaffected.
    assert_eq!(with_two_interfaces("A & { a: string }"), "A & { a: string; }");
}

#[test]
fn a_type_alias_names_its_intersection() {
    assert_eq!(
        type_of_annotation_at("interface A {}\ninterface B {}\ntype T = A & B;\nvar x: T;", 3),
        "T"
    );
}
