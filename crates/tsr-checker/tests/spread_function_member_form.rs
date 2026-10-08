//! A spread property copied from a `Function`-flagged symbol prints in method
//! form (r5-typetriage, `docs/parity/notes/r5-typetriage.md` §8).
//!
//! `getSpreadSymbol` (`checker.go:13585`, pinned 5b1047d) returns the property
//! symbol itself, so a spread of a namespace keeps each exported function's
//! `Function` flag. `addPropertyToElementList` (`nodebuilderimpl.go:2581`)
//! prints a `Function | Method` property whose type has call signatures and
//! no own properties as a method signature. A function merged with a
//! namespace has own properties, so it stays a property.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The printed type of the top-level `const name`.
fn type_of(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string(ty);
        }
    }
    panic!("`{name}` is declared nowhere");
}

#[test]
fn a_spread_exported_function_prints_as_a_method() {
    assert_eq!(
        type_of(
            "namespace N { export function f(): void {} export let x = 1; }\nconst s = { ...N };",
            "s"
        ),
        "{ f(): void; x: number; }"
    );
}

#[test]
fn a_spread_function_value_that_is_not_a_function_symbol_stays_a_property() {
    assert_eq!(
        type_of("namespace N { export const f = (): void => {}; }\nconst s = { ...N };", "s"),
        "{ f: () => void; }"
    );
}

#[test]
fn a_spread_function_merged_with_a_namespace_stays_a_property() {
    // Native prints `typeof N.g`; the qualifier is the symbol-chain printer's
    // (tsr-2zk.39), so only the property form is pinned here.
    let printed = type_of(
        "namespace N { export function g(): void {} export namespace g { export const y = 1; } }\nconst s = { ...N };",
        "s",
    );
    assert!(printed.starts_with("{ g: typeof "), "{printed}");
}
