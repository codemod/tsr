//! r6-names: regions native never checks (`crates/tsr-checker/src/name_slots.rs`,
//! `names_in_unchecked_region`): the expression of an empty `for…of`
//! declaration list, and a decorator on a node `NodeCanBeDecorated` rejects.
//! Expectations are native tsgo output at the pinned 5b1047d.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(file: &str, source: &str, legacy: bool) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names", file, source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "false".into());
    if legacy {
        case.options.insert("experimentaldecorators".into(), "true".into());
    }
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

/// `checkForOfStatement` reaches the expression only through a declaration,
/// so an empty list leaves it unchecked; a declared one and `for…in` do not.
#[test]
fn an_empty_for_of_declaration_list_leaves_its_expression_unchecked() {
    let source = "for (var of missingOf) { }
for (var x of missingDeclared) { }
for (var in missingIn) { }";
    assert_eq!(
        diagnostics("for_of.ts", source, false),
        [(1, 9, 1123), (2, 15, 2304), (3, 9, 1123), (3, 13, 2304)],
    );
}

const SOURCE: &str = "var v = @missingA class C { static p = 1 };
class D { @missingB m() {} }
function f(@missingC x: number) {}
class E { m(@missingD x: number) {} }
class F { @missingE.member() p = 1; }";

#[test]
fn legacy_decorators_on_a_class_expression_are_not_resolved() {
    assert_eq!(
        diagnostics("d.ts", SOURCE, true),
        [(1, 9, 1206), (2, 12, 2304), (3, 12, 1206), (4, 14, 2304), (5, 12, 2304)],
    );
}

#[test]
fn standard_decorators_on_a_parameter_are_not_resolved() {
    assert_eq!(
        diagnostics("d.ts", SOURCE, false),
        [(1, 10, 2304), (2, 12, 2304), (3, 12, 1206), (4, 13, 1206), (5, 12, 2304)],
    );
}
