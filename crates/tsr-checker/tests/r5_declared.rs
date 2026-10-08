//! r5-declared: `declared.rs` producers the relater and narrowing consume.
//!
//! Each fixture's native answer was read from the pinned reference baseline
//! (`vendor/typescript-go/testdata/baselines/reference/submodule`) or from the
//! pinned Go source named beside it. See `docs/parity/notes/r5-declared.md`.

use tsr_ast::{NodeId, NodeMap, NodeTable};
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

/// The diagnostic codes a fixture reports, in report order.
fn codes(source: &str) -> Vec<String> {
    let arena = Arena::new();
    let mut nodes = NodeTable::default();
    let mut node_map = NodeMap::default();
    let file = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut node_map,
    );
    assert!(
        file.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        file.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let root: NodeId = file.source_file.node_id.expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    checker.diagnostics().iter().map(|(_, d)| d.code().to_string()).collect()
}

/// getConstraintOfDistributiveConditionalType (checker.go:17286) for an
/// inline conditional: `T extends A` with `A` unrelated to `B` instantiates
/// `T extends B ? string : number` at `A` to `number`, so the deferred
/// conditional is assignable to `number` and not to `string`
/// (`distributiveConditionalTypeConstraints`).
#[test]
fn an_inline_distributive_conditional_relates_through_its_constraint() {
    let prelude = "interface A { foo(): void }\ninterface B { bar(): void }\n";
    let ok = format!(
        "{prelude}function f<T extends A>(y: T extends B ? string : number) {{ const n: number = y; }}"
    );
    assert_eq!(codes(&ok), Vec::<String>::new());
    let bad = format!(
        "{prelude}function f<T extends A>(y: T extends B ? string : number) {{ const s: string = y; }}"
    );
    assert_eq!(codes(&bad), vec!["TS2322".to_string()]);
}

/// getConditionalType's `forConstraint` extra (checker.go:24383): `string`
/// is not assignable to `"abc" | 42`, but `"abc"` is assignable to `string`,
/// so the constraint of `Foo<T>` for `T extends string` is `boolean`, not
/// `false` (`conditionalTypes1` f20/f21).
#[test]
fn for_constraint_adds_the_true_branch_of_an_overlapping_extends_type() {
    let prelude = "type Foo<T> = T extends \"abc\" | 42 ? true : false;\n";
    let widened =
        format!("{prelude}function f<T extends string>(x: Foo<T>) {{ let t: boolean = x; }}");
    assert_eq!(codes(&widened), Vec::<String>::new());
    let narrow =
        format!("{prelude}function f<T extends string>(x: Foo<T>) {{ let t: false = x; }}");
    assert_eq!(codes(&narrow), vec!["TS2322".to_string()]);
}

/// getTypeFromTypeAliasReference (checker.go:23580) through a qualified name:
/// `N.Yep` is the alias's declared object type, so a fresh literal with the
/// wrong discriminant is not assignable to either constituent
/// (`namespaceDisambiguationInUnion`). The print-only mint had no members and
/// the relation was undecided (`tsr-2zk.979`).
#[test]
fn a_qualified_alias_reference_relates_as_its_declared_type() {
    let prelude = "namespace Foo { export type Yep = { type: \"foo.yep\" } }\n\
        namespace Bar { export type Yep = { type: \"bar.yep\" } }\n";
    let wrong = format!("{prelude}const x = {{ type: \"wat\" }};\nconst v: Foo.Yep | Bar.Yep = x;");
    assert_eq!(codes(&wrong), vec!["TS2322".to_string()]);
    let right = format!(
        "{prelude}const y = {{ type: \"foo.yep\" as const }};\nconst v: Foo.Yep | Bar.Yep = y;"
    );
    assert_eq!(codes(&right), Vec::<String>::new());
}

/// getTypeAliasInstantiation over an intersection body (instantiateTypeWithAlias
/// → getIntersectionTypeEx with the alias, checker.go:26043): the reference is
/// the alias-carrying intersection of its instantiated constituents, so its
/// members are the constituents' members. A print-only mint enumerated no
/// properties and accepted any object (`tsr-2zk.1010`).
#[test]
fn an_intersection_alias_reference_has_its_constituents_members() {
    let prelude = "interface ClassAttributes<T> { ref?: T }\n\
        interface HTMLAttributes<T> { className?: string; onClick?: (t: T) => void }\n\
        type DetailedHTMLProps<E extends HTMLAttributes<T>, T> = ClassAttributes<T> & E;\n";
    let excess = format!(
        "{prelude}const y: DetailedHTMLProps<HTMLAttributes<number>, number> = {{ class: \"\" }};"
    );
    assert_eq!(codes(&excess), vec!["TS2353".to_string()]);
    let known = format!(
        "{prelude}const y: DetailedHTMLProps<HTMLAttributes<number>, number> = {{ className: \"\" }};"
    );
    assert_eq!(codes(&known), Vec::<String>::new());
}
