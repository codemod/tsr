//! A deferred conditional type's typed print is made when it is printed
//! (`docs/parity/notes/r6-lazytext.md` §2). Native's
//! conditionalTypeToTypeNode (`nodebuilderimpl.go:2916`) reads
//! getTrueTypeFromConditionalType and getFalseTypeFromConditionalType while it
//! prints; the mint keeps the written text.

use tsr_checker::Checker;
use tsr_core::Arena;

/// The baked print and the print at its declaration of the symbol `name`.
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
            let ty = checker.get_type_of_symbol(symbol);
            return (checker.type_to_string(ty), checker.type_to_string_at(ty, site));
        }
    }
    panic!("`{name}` is declared nowhere");
}

#[test]
fn a_deferred_conditional_prints_its_typed_branches_when_printed() {
    // wideningWithTopLevelTypeParameter: tsgo prints the false branch as the
    // union type `string | T`, in union order, not as written.
    let (baked, at_site) =
        prints("function f<T>(t: T extends undefined ? never : T | string) {}", "t");
    assert_eq!(baked, "T extends undefined ? never : T | string");
    assert_eq!(at_site.as_deref(), Some("T extends undefined ? never : string | T"));
}
