//! `import q = M` — what an alias symbol's type is, and where it stops.
//!
//! Assertions go through `Checker::type_to_string`, because the printed form is
//! what `.types` baselines compare. The expectations here are quoted from
//! `compiler/aliasBug.types` and `compiler/aliasErrors.types` rather than
//! reasoned about: an alias is one of the few places where the printed name is
//! *not* the name you wrote, and guessing it would be easy and wrong.

use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the symbol declared at file scope under `name`.
fn type_of_declaration(source: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(
        parsed.diagnostics.is_empty(),
        "fixture must parse: {:?}",
        parsed.diagnostics.iter().map(tsr_diagnostics::Diagnostic::text).collect::<Vec<_>>()
    );
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: source },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));

    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

#[test]
fn an_alias_to_a_same_file_namespace_has_the_targets_type_and_not_its_own_name() {
    // `compiler/aliasBug.types` and `compiler/aliasErrors.types` both record
    //
    //     import provide = foo;
    //     >provide : typeof foo
    //
    // `typeof foo`, NOT `typeof provide`. The alias contributes no type of its
    // own; `getTypeOfAlias` answers with the *target's* type, which is the whole
    // content of the arm. An implementation that named the alias would pass a
    // "does it resolve" test and fail every baseline line.
    assert_eq!(
        type_of_declaration(
            "namespace foo { export class Provide {} }\nimport provide = foo;",
            "provide"
        ),
        "typeof foo"
    );

    // The same through an enum, to show the answer is whatever
    // `getTypeOfSymbol` says about the target rather than a namespace special
    // case. An enum qualifies because `SymbolFlags::NAMESPACE` includes `ENUM`;
    // a *class* does not, and `import q = C` correctly resolves to nothing —
    // which is why there is no class case here. That asymmetry is upstream's
    // `SymbolFlagsNamespace` meaning at `checker.go:14486`, not an omission.
    assert_eq!(type_of_declaration("enum E { A }\nimport q = E;", "q"), "typeof E");
}

#[test]
fn an_alias_to_a_non_value_or_an_unresolvable_target_is_error_not_any() {
    // An interface target answers `error` — but **not** for the reason it is
    // tempting to write here. `SymbolFlags::INTERFACE` is not in
    // `SymbolFlags::NAMESPACE`, so this fails at the meaning filter in
    // `resolve_alias` and never reaches the `VALUE` test in `get_type_of_alias`.
    //
    // The distinction is not pedantry: the `VALUE` test (`checker.go:18612`) is
    // currently **unreachable** in this port. `NAMESPACE` is
    // `VALUE_MODULE | NAMESPACE_MODULE | ENUM`; the binder never assigns
    // `NAMESPACE_MODULE` at all, and `VALUE_MODULE` and `ENUM` are both inside
    // `SymbolFlags::VALUE` — so everything that survives the meaning filter is a
    // value by construction. The guard is upstream's line, kept because it is
    // upstream's and because it stops a stack overflow there, and no fixture
    // here exercises it.
    //
    // How you would know this changed: when the binder starts distinguishing a
    // type-only namespace as `NAMESPACE_MODULE`, `import q = N` for
    // `namespace N { export interface I {} }` becomes the fixture that reaches
    // the guard, and it needs its own test at that point. Today that source
    // binds `N` as `VALUE_MODULE`, which was checked rather than assumed.
    assert_eq!(type_of_declaration("interface I {}\nimport q = I;", "q"), "error");

    // A bare identifier resolves in NAMESPACE meaning only
    // (`checker.go:14486`), so a plain value target is not found — and that is
    // upstream's answer, not a gap. Resolving in VALUE too would turn this into
    // a wrong line.
    assert_eq!(type_of_declaration("const x = 1;\nimport q = x;", "q"), "error");

    // Nothing to resolve at all.
    assert_eq!(type_of_declaration("import q = Missing;", "q"), "error");
}

#[test]
fn a_qualified_alias_is_a_gap_because_its_printed_name_is_its_own() {
    // Resolving `foo.bar.baz` is a walk over `exports` and would succeed. The
    // answer would still be wrong, because `compiler/aliasBug.types` records
    //
    //     import booz = foo.bar.baz;
    //     >booz : typeof booz
    //
    // the ALIAS's name, where the bare form one line above prints the TARGET's.
    // Upstream emits the shortest accessible chain to the symbol and an alias
    // declaration is always a one-link chain: `foo` is already one link and
    // wins, `foo.bar.baz` is three and loses to `booz`. This port has no symbol
    // accessibility, so it would print `typeof baz` — a wrong line where a
    // missing one belongs.
    assert_eq!(
        type_of_declaration(
            "namespace foo { export namespace bar { export namespace baz { export class boo {} } } }\nimport booz = foo.bar.baz;",
            "booz"
        ),
        "error"
    );
}

#[test]
fn an_alias_through_an_external_module_stays_a_gap() {
    // `import q = require("m")` and every ES import reach their target through
    // module resolution, and cross-file targets are blocked on globals never
    // being merged across files — `crates/tsr-compiler/src/lib.rs:24`,
    // ADR-0034, `bd tsr-9or.1`. Not a gap this module can close, which is why
    // it is pinned here rather than left to be rediscovered.
    assert_eq!(type_of_declaration(r#"import q = require("m");"#, "q"), "error");
}
