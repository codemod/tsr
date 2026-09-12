//! `Record<K, V>` with a PRIMITIVE key carries an index signature. §785.
//!
//! `Record<K, V>` is `{ [P in K]: V }`, and upstream resolves it in
//! `resolveMappedTypeMembers` (`checker.go`): it walks
//! `getLowerBoundOfKeyType(constraintType)` and, for a key that is *not* usable
//! as a property name — which is exactly `string`, `number`, `symbol` and the
//! pattern types — creates an **index info** rather than a property.
//!
//! Mapped-type member resolution is not ported (see the gap list at the top of
//! `crates/tsr-checker/src/index_signatures.rs`), so `Record` alone is
//! special-cased, which is the scoping decision §45 already made for the
//! PROPERTY road in `Checker::record_string_value`. This is the half that road
//! could not cover: `m[i]` never asks for a property, so the property-road
//! special case never ran and every element access on a `Record` answered
//! `errorType` — printed `any`.
//!
//! Found by ranking the baseline's WRONG lines by case (STATUS §4.-5).
//! `compiler/temporal` heads that board at 339 wrong lines against 6,258
//! right, and four of its sites are spelled `monthsByDays[zdt.daysInMonth]`
//! over `Record<number, Temporal.ZonedDateTime[]>`, whose `any` then cascades
//! through the `Array<T>` members read off each one.
//!
//! **That case's 339 is NOT this arm's population**, and an earlier draft of
//! this comment said it was. Clustering temporal's wrong indices gives 35
//! separate blocks and at least three causes — 117 lines where the port
//! answers `any` (these among them), 78 contextual-literal-widening lines, 16
//! optional-parameter printing lines. The size of this arm is whatever
//! `scorepair` says it is; see the §785 row in STATUS §7.
//!
//! **Two risks to watch in the measurement, recorded before it is run.**
//! First, `get_index_infos_of_type` is not only the element-access road — the
//! relater consults it too, so handing back an index signature for `Record`
//! can move ASSIGNABILITY results, and an adverse count there would be this
//! arm's, not noise. Second, the new arm returns EARLY, ahead of the
//! `TypeData::Named` members road; that is safe only while a `Record`
//! reference carries no members table of its own, which is true today because
//! `Record` is an alias to a mapped type and this port does not resolve mapped
//! members at all. If mapped-type member resolution is ever ported, this early
//! return becomes a shadow and must move below it.
//!
//! **The literal-union key is excluded on purpose.** `Record<"a" | "b", V>`
//! must produce PROPERTIES, and handing an index signature back for it would
//! make `r.c` answer `V` where upstream errors — a confident wrong answer in
//! place of a missing one, which is the trade this project refuses everywhere.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the initialiser of the `index`th statement.
fn type_of_at(source: &str, index: usize) -> String {
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
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|declaration| declaration.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    checker.type_to_string(id)
}

/// The harness has no lib, so `Record` is declared in the fixture — the
/// special case resolves it through `binder.global("Record")` either way.
const RECORD: &str = "type Record<K extends keyof any, T> = { [P in K]: T };\n";

/// The `compiler/temporal` shape, reduced: a numeric index expression on a
/// `Record<number, V>`.
#[test]
fn a_numeric_index_on_a_numeric_key_record_answers_the_value() {
    let source = format!(
        "{RECORD}declare const m: Record<number, string>;\ndeclare const i: number;\nconst a = m[i];\n"
    );
    assert_eq!(type_of_at(&source, 3), "string");
}

/// A numeric LITERAL index reads the same signature — `monthsByDays[30]`.
#[test]
fn a_literal_numeric_index_reads_the_same_signature() {
    let source = format!("{RECORD}declare const m: Record<number, string>;\nconst a = m[30];\n");
    assert_eq!(type_of_at(&source, 2), "string");
}

/// The `string` key is the other primitive upstream turns into an index info,
/// and the element-access spelling is the half §45's property road missed.
#[test]
fn a_string_key_record_answers_the_value_through_element_access() {
    let source = format!(
        "{RECORD}declare const m: Record<string, number>;\ndeclare const k: string;\nconst a = m[k];\n"
    );
    assert_eq!(type_of_at(&source, 3), "number");
}

/// The exclusion, and the reason the arm is restricted rather than general:
/// a literal-union key is a set of PROPERTIES, so no index signature applies
/// and an unlisted key must stay a miss rather than answer `V`.
#[test]
fn a_literal_union_key_record_is_not_an_index_signature() {
    let source = format!(
        "{RECORD}declare const m: Record<\"a\" | \"b\", number>;\ndeclare const k: string;\nconst a = m[k];\n"
    );
    assert_ne!(type_of_at(&source, 3), "number");
}
