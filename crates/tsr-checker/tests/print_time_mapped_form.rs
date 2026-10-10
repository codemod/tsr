//! A generic mapped type, a deferred `keyof` and a deferred indexed access
//! are printed from their parts when they are printed
//! (`docs/parity/notes/r6-printer4.md` §1, ADR-0052). Native's
//! createMappedTypeNodeFromType (`nodebuilderimpl.go:1458`) and typeToTypeNode's
//! index and indexed-access arms print each part at the print site; the mint
//! keeps their site-free text.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The baked print and the print at its declaration of the type alias or
/// variable `name`.
fn prints(source: &str, name: &str) -> (String, Option<String>) {
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
    let options = tsr_core::CompilerOptions {
        strict_null_checks: tsr_core::Tristate::True,
        ..Default::default()
    };
    checker.apply_compiler_options(&options);
    for index in 0..parsed.nodes.len() {
        #[allow(clippy::cast_possible_truncation)]
        let id = tsr_ast::NodeId::new(index as u32);
        if let Some(symbol) = bound.lookup_local(id, name) {
            let site = bound.symbols().get(symbol).declarations[0];
            let flags = bound.symbols().get(symbol).flags;
            let ty = if flags.contains(tsr_binder::SymbolFlags::TYPE_ALIAS) {
                checker.get_declared_type_of_symbol(symbol)
            } else {
                checker.get_type_of_symbol(symbol)
            };
            return (checker.type_to_string(ty), checker.type_to_string_at(ty, site));
        }
    }
    panic!("`{name}` is declared nowhere");
}

#[test]
fn a_keyof_over_a_generic_mapped_type_prints_its_as_clause_when_printed() {
    // mappedTypeAsClauses 0:106: the as clause's deferred conditional is
    // printed from its typed parts inside the mapped form inside `keyof`.
    let (baked, at_site) = prints(
        "type TN2<T> = keyof { [P in keyof T as 'a' extends P ? 'x' : 'y']: string };",
        "TN2",
    );
    assert_eq!(baked, "keyof { [P in keyof T as 'a' extends P ? 'x' : 'y']: string; }");
    assert_eq!(
        at_site.as_deref(),
        Some("keyof { [P in keyof T as \"a\" extends P ? \"x\" : \"y\"]: string; }")
    );
}

#[test]
fn a_shadowed_iteration_parameter_is_renamed_in_every_part() {
    // tsgo: `>q : keyof { [P_1 in keyof P as P_1]: P[P_1]; }`; the
    // indexed access in the template takes the same allocation.
    let (baked, at_site) = prints(
        "function h<P>(p: P) { type Q<T> = keyof { [P in keyof T as P]: T[P] }; let q!: Q<P>; return q; }",
        "q",
    );
    assert_eq!(baked, "keyof { [P in keyof P as P]: P[P]; }");
    assert_eq!(at_site.as_deref(), Some("keyof { [P_1 in keyof P as P_1]: P[P_1]; }"));
}
