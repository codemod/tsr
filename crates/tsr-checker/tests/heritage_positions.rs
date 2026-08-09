//! A heritage clause that emits nothing is a type position — and which rules
//! that silences.
//!
//! `isIdentifierInNonEmittingHeritageClause` (`ast/utilities.go:3132`) names the
//! two: a class's `implements`, and **either clause of an `interface`**. Only a
//! class's `extends` is a value — it is the base constructor and it survives to
//! the output. The `extends` keyword does not separate them, because an
//! interface's clause is spelled `extends` too.
//!
//! # Two rules, two different gates, and that is the finding
//!
//! It is tempting to answer this once in `is_value_reference` and have every
//! rule inherit it. **That is wrong, and the corpus said so.**
//!
//! - **TS1361** (`import type` used as a value) is guarded upstream by
//!   `IsValidTypeOnlyAliasUseSite` (`:3124`), whose third clause is exactly this
//!   predicate. Silencing it here is upstream's own rule.
//! - **TS2686** (a UMD global in a module) is guarded by
//!   `meaning&SymbolFlagsValue == SymbolFlagsValue` (`checker.go:1841`), and an
//!   interface heritage entry resolves at type meaning — so it too is silent.
//! - **TS2304** (`Cannot find name`) is **not**. Upstream reports it for an
//!   unresolved interface-heritage name through `resolveEntityName`'s failure
//!   at type meaning. Declining in `is_value_reference` — which TS2304 is keyed
//!   on here — silently lost `compiler/protoAssignment`
//!   (`interface Number extends Comparable<number>`), reported by the suite
//!   only as `1,994 → 1,993`.
//!
//! # The mutations
//!
//! | # | mutation | reddens |
//! |---|---|---|
//! | 1 | drop the clause from `is_valid_type_only_alias_use_site` | [`an_interfaces_extends_over_a_type_only_import_is_silent`] and [`a_typeof_inside_heritage_type_arguments_is_still_a_value_position`] |
//! | 2 | decline in `is_value_reference` instead (the tempting one fix) | [`an_unresolved_name_in_an_interfaces_extends_still_reports`] **only** |
//!
//! `docs/architecture/checker-notes-diag2.md` §502.

use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_binder::BindResult;
use tsr_checker::Checker;
use tsr_checker::check::FileContext;

/// Every diagnostic code the last fixture produces, sorted and deduplicated.
fn codes(files: &[(&str, &str)]) -> Vec<String> {
    let arena = tsr_core::Arena::new();
    let mut nodes = NodeTable::new();
    let mut node_map = NodeMap::new();
    let mut parsed = Vec::new();
    for (name, source) in files {
        let source: &str = arena.alloc_str(source);
        let name: &str = arena.alloc_str(name);
        let file = tsr_parser::parse_into(
            &arena,
            source,
            tsr_parser::ParseOptions::for_file(name),
            &mut nodes,
            &mut node_map,
        );
        assert!(file.diagnostics.is_empty(), "fixture {name} must parse");
        parsed.push((name, source, file.source_file));
    }
    let mut bound = BindResult::empty();
    let mut roots: Vec<(&str, NodeId)> = Vec::new();
    for (name, source, source_file) in parsed {
        roots.push((name, source_file.node_id.expect("a parsed file has an id")));
        bound = tsr_binder::bind_into(
            bound,
            &arena,
            source_file,
            &nodes,
            tsr_binder::FileInfo { name, text: source },
        );
    }
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    let (name, root) = *roots.last().expect("at least one file");
    checker.check_source_file(
        root,
        FileContext { ambient: name.ends_with(".d.ts"), has_parse_errors: false },
    );
    let mut out: Vec<String> =
        checker.diagnostics().iter().map(|(_, d)| d.code()).collect::<Vec<_>>();
    out.sort();
    out.dedup();
    out
}

/// A type-only import of two names, one generic.
const LIB: (&str, &str) =
    ("/lib.ts", "export interface Base { a: number }\nexport type Variant<T> = { v: T };\n");
const TYPE_IMPORT: &str = "import type { Base, Variant } from \"./lib\";\n";

#[test]
fn an_interfaces_extends_over_a_type_only_import_is_silent() {
    // The shape every `cva`-styled React component is written in:
    // `interface P extends VariantProps<typeof variants>` over an
    // `import type { VariantProps }`. It reported TS1361 on all of them.
    //
    // Red under: dropping `identifier_in_non_emitting_heritage_clause` from
    // `is_valid_type_only_alias_use_site`.
    assert_eq!(
        codes(&[
            LIB,
            (
                "/a.ts",
                &format!(
                    "{TYPE_IMPORT}export interface X extends Base, Variant<string> {{ b: number }}\n"
                )
            ),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_class_implements_over_a_type_only_import_is_silent() {
    // The other non-emitting clause. This one was already silent — its arm
    // keyed on the `implements` keyword — and it is here so that a future
    // simplification of the predicate cannot quietly drop it.
    assert_eq!(
        codes(&[
            LIB,
            ("/a.ts", &format!("{TYPE_IMPORT}export class C implements Base {{ a = 1; }}\n")),
        ]),
        Vec::<String>::new()
    );
}

#[test]
fn a_class_extends_over_a_type_only_import_still_reports() {
    // The value position, and the control for both tests above: a class's
    // `extends` is the base constructor and it *is* emitted, so a type-only
    // import there is exactly what TS1361 exists for.
    assert_eq!(
        codes(&[LIB, ("/a.ts", &format!("{TYPE_IMPORT}export class C extends Base {{}}\n")),]),
        vec!["TS1361".to_string()]
    );
}

#[test]
fn an_unresolved_name_in_an_interfaces_extends_still_reports() {
    // `compiler/protoAssignment`, reduced. Upstream reports `Cannot find name`
    // here through `resolveEntityName` at **type** meaning — a heritage clause
    // being a type position does not make an undeclared name legal.
    //
    // This is the test that says the two gates must stay separate: it is red
    // under the "obvious" fix of declining in `is_value_reference`, which is
    // what made TS1361 and TS2686 silent in one place.
    assert_eq!(
        codes(&[(
            "/a.ts",
            "interface Number extends Comparable<number> { compareTo(o: number): void }\n"
        )]),
        vec!["TS2304".to_string()]
    );
}

#[test]
fn a_typeof_inside_heritage_type_arguments_is_still_a_value_position() {
    // The loop in `isIdentifierInNonEmittingHeritageClause` climbs
    // `PropertyAccessExpression` and `ExpressionWithTypeArguments` and nothing
    // else, which is what keeps the *type arguments* out of the exemption.
    // `typeof nope` is a value position (§79) and its name must still resolve.
    //
    // **Measured, and it is a guard rather than an arm test.** Widening the
    // walk to climb `TypeQueryNode` too does *not* redden it: TS2304 is keyed on
    // `is_value_reference`, whose own type-query arm answers independently of
    // the heritage walk. It stays because the invariant is worth pinning and
    // because a future rule keyed on the use-site predicate would need it — not
    // because a mutation was found for it. It *is* red under M1 (dropping the
    // clause), which adds a TS1361 for `Variant` beside the TS2304.
    assert_eq!(
        codes(&[
            LIB,
            (
                "/a.ts",
                &format!(
                    "{TYPE_IMPORT}export interface X extends Variant<typeof nope> {{ b: number }}\n"
                )
            ),
        ]),
        vec!["TS2304".to_string()]
    );
}
