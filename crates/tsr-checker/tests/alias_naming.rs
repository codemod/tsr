//! A type literal under a type alias prints the **alias's name**.
//!
//! `type Obj = { code: string }` used as `const arr: Obj[]` records
//! `>arr : Obj[]` (`conformance/literalTypeWidening.types:427`), never the
//! expanded members. Until this slice the port printed `{ code: string; }[]` —
//! a wrong answer rather than a gap, and one that was invisible while the
//! literal itself gapped for other reasons.
//!
//! # The rule is `get_type_from_union_type_node`'s, extended
//!
//! `getAliasForTypeNode` (`checker.go:23711`) attaches the enclosing alias to
//! whatever the node produced, and the union and intersection arms already did
//! this. The three arms have to stay identical across all three spellings,
//! which is why the unaliased case is asserted here too rather than assumed:
//! `>alpha : { a: string; }` is a real baseline line, so an alias name must not
//! be invented where no alias encloses the literal.
//!
//! # What this does *not* unblock
//!
//! Alias naming only pays for an alias whose body the port can already compute.
//! A body containing a tuple member, a mapped member, or a member whose type is
//! unresolved still gaps — there is nothing to name. That boundary is asserted
//! below, because it is the difference between "the item is done" and "the item
//! is done and the frontier behind it is what remains".

use tsr_ast::{NodeMap, NodeTable, Statement};
use tsr_checker::Checker;
use tsr_core::Arena;

/// The minimal globals an array type needs, as `tests/arrays.rs` uses.
const LIB: &str = "interface Array<T> {}\ninterface ReadonlyArray<T> {}\n";

/// Bind `LIB` and `source` as one program, then print the annotation on
/// `source`'s last variable statement.
fn type_of_last_annotation(source: &str) -> String {
    let arena = Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let options = tsr_parser::ParseOptions::default();

    let lib = tsr_parser::parse_into(&arena, LIB, options, &mut nodes, &mut node_map);
    let file = tsr_parser::parse_into(&arena, source, options, &mut nodes, &mut node_map);
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );

    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        lib.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "lib.d.ts", text: LIB },
    );
    let bound = tsr_binder::bind_into(
        bound,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );

    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let statement = file
        .source_file
        .statements
        .iter()
        .rev()
        .find_map(|statement| match statement {
            Statement::VariableStatement(statement) => Some(statement),
            _ => None,
        })
        .expect("a variable statement");
    let annotation = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.r#type)
        .expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

#[test]
fn a_literal_under_a_non_generic_alias_prints_the_alias_name() {
    // `>ExampleState : ExampleState` and `>arr : Obj[]` are both real baseline
    // lines; the expanded members appear on neither.
    assert_eq!(type_of_last_annotation("type X = { a: string };\nvar v: X;"), "X");
    assert_eq!(type_of_last_annotation("type X = { a: string };\nvar v: X[];"), "X[]");
    // The name survives every position the population count found it in, not
    // just the array suffix — 2,311 of the 6,077 lines are embedded like this.
    assert_eq!(
        type_of_last_annotation("type X = { a: string };\nvar v: X | undefined;"),
        "X | undefined"
    );
    assert_eq!(
        type_of_last_annotation("type X = { a: string };\nvar v: (p: X) => void;"),
        "(p: X) => void"
    );
}

#[test]
fn an_unaliased_literal_still_renders_structurally() {
    // The half that is easy to lose by over-applying the rule. `>alpha : { a:
    // string; }` is a real baseline line, so a literal with no enclosing alias
    // must still print its members.
    assert_eq!(type_of_last_annotation("var v: { a: string };"), "{ a: string; }");
    // A literal nested *inside* an aliased one is not itself aliased — its host
    // is the property signature, not the alias — so only the outer name applies.
    assert_eq!(type_of_last_annotation("type X = { a: { b: string } };\nvar v: X;"), "X");
    assert_eq!(type_of_last_annotation("var v: { a: { b: string } };"), "{ a: { b: string; }; }");
}

#[test]
fn naming_pays_only_where_the_body_computes() {
    // Alias naming has nothing to name when the body gaps, so these are
    // unchanged by this slice. They are the frontier behind it, and the reason
    // the population number is not the payoff number.
    assert_eq!(type_of_last_annotation("type X = { a: [string] };\nvar v: X[];"), "error");
    // `type X = { a: Nope }` used to be here asserting `error`, and it now
    // computes: an **unresolved** type reference prints the name that was
    // written (`bd tsr-eep`, `Checker::unresolved_type_reference`), so the body
    // no longer gaps and the alias has something to name. Upstream prints
    // `X[]` here too — it reports `Cannot find name 'Nope'` *and* renders the
    // name. The line moved rather than being deleted, because the frontier this
    // test describes is real and just no longer runs through an unresolved
    // name.
    assert_eq!(type_of_last_annotation("type X = { a: Nope };\nvar v: X[];"), "X[]");
    assert_eq!(
        type_of_last_annotation(
            "type K = \"a\" | \"b\";\ntype X = { a: { [P in K]: string } };\nvar v: X[];"
        ),
        "error"
    );
    // But a member that *does* compute carries the naming through, including a
    // generic reference — which is what makes the reachable set larger than the
    // gapped bodies suggest.
    assert_eq!(
        type_of_last_annotation("interface G<T> {}\ntype X = { a: G<string> };\nvar v: X[];"),
        "X[]"
    );
    assert_eq!(type_of_last_annotation("type X = { a: string[] };\nvar v: X[];"), "X[]");
}
