//! A mapped member keyed by a unique symbol that lives on an interface is
//! spelled through getSymbolChain's containers (`nodebuilderimpl.go:1087`,
//! `getContainersOfSymbol` and `getWithAlternativeContainers`,
//! `symbolaccessibility.go:280`, `:117`): the interface itself, or a variable
//! of its type (the variable-match arm, `:137`), whichever `sortByBestName`
//! orders first (`docs/parity/notes/r6-printer4.md` §3). Native lines read
//! from the pinned tsgo.

use tsr_checker::Checker;
use tsr_core::Arena;

fn print_at_declaration(source: &str, name: &str) -> Option<String> {
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
            let site = bound.symbols().get(symbol).declarations[0];
            let ty = checker.get_type_of_symbol(symbol);
            return checker.type_to_string_at(ty, site);
        }
    }
    panic!("`{name}` is declared nowhere");
}

#[test]
fn an_interface_member_key_is_qualified_by_its_first_container() {
    // tsgo: `>m : { [Ctor.it]: number; }` — `Ctor` is declared first.
    let source = "interface Ctor { readonly it: unique symbol }
        declare var Sym: Ctor;
        declare function f<T>(t: T): { [K in typeof Sym.it]: T };
        const m = f(1);";
    assert_eq!(print_at_declaration(source, "m").as_deref(), Some("{ [Ctor.it]: number; }"));
}

#[test]
fn a_variable_of_the_interface_type_acts_as_its_namespace() {
    // tsgo: `>m : { [Sym.it]: number; }` — the variable match sorts first
    // (`Symbol.iterator` is this arm).
    let source = "declare var Sym: Ctor;
        interface Ctor { readonly it: unique symbol }
        declare function f<T>(t: T): { [K in typeof Sym.it]: T };
        const m = f(1);";
    assert_eq!(print_at_declaration(source, "m").as_deref(), Some("{ [Sym.it]: number; }"));
}
