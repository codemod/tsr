//! r6-names2: a local export specifier in a non-ambient namespace. Native's
//! walk does not skip a pure export-specifier alias there
//! (`binder/nameresolver.go:121-133`), so `export { inner }` finds its own
//! alias and `resolveAlias` reports TS2303, while `export { inner as x }`
//! finds nothing and reports TS2304. Expectations are native tsgo output at
//! the pinned 5b1047d (`--module commonjs --target es2022`).
//! `docs/parity/notes/r6-names2.md` §3.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names2", "a.ts", source);
    case.options.insert("module".into(), "commonjs".into());
    case.options.insert("target".into(), "es2022".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn a_namespace_export_of_an_undeclared_name_is_its_own_circular_alias() {
    let source = "namespace N { export { inner } }
namespace M { const inner2 = 1; export { inner2 } }
var o = 1;
namespace O { export { o } }
export {};";
    assert_eq!(
        diagnostics(source),
        [(1, 15, 1194), (1, 24, 2303), (2, 33, 1194), (4, 15, 1194), (4, 24, 2303)]
    );
}

/// The lookup is for the property name, which the namespace's exports do not
/// hold; an ambient namespace skips the alias as a source file does.
#[test]
fn a_renamed_or_ambient_export_is_a_failed_lookup() {
    let source = "namespace N1 { export { inner as x } }
declare namespace D { export { amb } }
export {};";
    assert_eq!(diagnostics(source), [(1, 16, 1194), (1, 25, 2304), (2, 32, 2304)]);
}

/// Two specifiers naming each other are both circular; an alias whose
/// target resolves is not, and a reference to it resolves.
#[test]
fn mutual_aliases_cycle_and_a_resolved_alias_does_not() {
    let source = "namespace P { export { q as r }; export { r as q } }
namespace R { var x = 1; export { x as y }; let z = y; }
namespace S { function f() {} export { f } }
export {};";
    assert_eq!(
        diagnostics(source),
        [(1, 15, 1194), (1, 24, 2303), (1, 34, 1194), (1, 43, 2303), (2, 26, 1194), (3, 31, 1194)]
    );
}
