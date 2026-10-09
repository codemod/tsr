//! r6-names2: `resolveName`'s `useOuterVariableScopeInParameter` arm
//! (`crates/tsr-binder/src/scope_change.rs`). A parameter initializer does not
//! see the function body's variables unless the parameters require a scope
//! change. Expectations are native tsgo output at the pinned 5b1047d
//! (`--strict`). `docs/parity/notes/r6-names2.md` §1.

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(source: &str, target: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names2", "s.ts", source);
    case.options.insert("target".into(), target.into());
    case.options.insert("strict".into(), "true".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

const SOURCE: &str = "export function bar(func = () => foo) { let foo = \"in\"; }
let outer = \"\";
export function f1(p = outer) { var outer: number = 2; return p; }
export function nullish(a = c ?? 1) { var c: number | undefined; return a; }
export function plain(a = d) { var d = 1; return a; }";

/// Below ES2020 `??` in a parameter requires a scope change, so `c` is the
/// body's (TS2373); the other initializers look outside the function.
#[test]
fn an_es2015_nullish_initializer_keeps_the_body_scope() {
    assert_eq!(diagnostics(SOURCE, "es2015"), [(1, 34, 2304), (4, 29, 2373), (5, 27, 2304)]);
}

#[test]
fn from_es2020_every_initializer_looks_outside_the_function() {
    assert_eq!(diagnostics(SOURCE, "es2022"), [(1, 34, 2304), (4, 29, 2304), (5, 27, 2304)]);
}

/// Every resolution road answers the same: the flow road behind TS2454
/// resolves through `BindResult::resolve_name`, which gets no options, and
/// still finds no body variable for a parameter list without a trigger
/// (`optionalParamReferencingOtherParams2`'s native baseline).
#[test]
fn a_road_without_options_agrees_when_nothing_requires_a_scope_change() {
    let source = "var a = 1;
function strange(x = a, y = b) {
    var b = \"\";
    return y;
}";
    assert_eq!(diagnostics(source, "es2015"), [(2, 29, 2304)]);
}
