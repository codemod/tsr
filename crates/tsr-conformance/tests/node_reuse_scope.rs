//! The structural pseudo types' visitor scope (`tsr-2zk.1118`,
//! `docs/parity/notes/r6-nodereuse.md` §1). Expectations from the pinned
//! tsgo `5b1047d` (`tsgo --declaration`, built by
//! `scripts/offline-cargo/build-tsgo.sh`), whose declaration emit prints
//! the same nodes the types baseline prints at the declaration.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn lines(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/node_reuse_scope", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

fn expect(source: &str, wanted: &[&str]) {
    let lines = lines(source);
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

/// A returned function's type parameter is its own `reuseNode`: a
/// constraint naming an alias local to the printed function's body is
/// tracked at the site, fails there, and the visitor's per-node recovery
/// prints the alias's type. tsgo: `<U extends T = T>(local: U) => T | U`.
#[test]
fn a_body_local_alias_in_an_entered_constraint_is_tracked_at_the_site() {
    expect(
        r"// @strict: true
// @target: es2015
function noShadow<T>(value: T) {
    type Outer = T;
    return function inner<U extends Outer = Outer>(local: U): Outer | U { return Math.random() ? value : local; };
}
",
        &["noShadow : <T>(value: T) => <U extends T = T>(local: U) => T | U"],
    );
}

/// An entered type parameter whose written name an enclosing parameter
/// already holds is renamed by `typeParameterToName`; the visitor does not
/// model that allocation, so the slot is serialized from its type, which
/// renames. tsgo: `<T_1 extends T = T>(local: T_1) => T | T_1`.
#[test]
fn a_shadowed_entered_type_parameter_declines_to_the_type() {
    expect(
        r"// @strict: true
// @target: es2015
function constraints<T>(value: T) {
    type Outer = T;
    return function inner<T extends Outer = Outer>(local: T): Outer | T { return Math.random() ? value : local; };
}
",
        &["constraints : <T>(value: T) => <T_1 extends T = T>(local: T_1) => T | T_1"],
    );
}
