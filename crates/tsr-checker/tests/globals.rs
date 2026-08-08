//! The globals upstream synthesises rather than reading from a file.
//!
//! `undefined` is the only global with no declaration anywhere — it is in no
//! `lib.*.d.ts` — so the lib-merging machinery that makes `Array` and `String`
//! resolve never produced it, and every `undefined` reference answered
//! `errorType`.

use tsr_checker::Checker;
use tsr_core::Arena;

/// Type the file-scope declaration `name`.
fn type_of_declaration(src: &str, name: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, src);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: src },
    );
    let root = tsr_ast::Node::SourceFile(parsed.source_file).node_id().expect("registered");
    let symbol = bound.lookup_local(root, name).unwrap_or_else(|| panic!("`{name}` is declared"));
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let id = checker.get_type_of_symbol(symbol);
    checker.type_to_string(id)
}

/// Type the initialiser of the first statement.
fn type_of_initialiser(src: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, src);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "test.ts", text: src },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let tsr_ast::Statement::VariableStatement(statement) = parsed.source_file.statements[0] else {
        panic!("the fixture must start with a variable statement");
    };
    let declaration =
        statement.declaration_list.and_then(|l| l.declarations.first().copied()).expect("one");
    let id = checker.check_expression(declaration.initializer.expect("an initialiser"));
    checker.type_to_string(id)
}

#[test]
fn the_name_undefined_resolves_to_the_undefined_type() {
    // Every enum baseline records `>undefined : undefined`, 1,738 lines across
    // 464 baselines. Upstream synthesises the symbol in `initializeChecker`
    // (`checker.go:955`) and sets its type directly (`checker.go:1345`),
    // because a symbol with no declaration reaches no dispatch arm.
    assert_eq!(type_of_initialiser("const q = undefined;"), "undefined");
    // `const` does not widen, so the declaration keeps it too.
    assert_eq!(type_of_declaration("const x = undefined;", "x"), "undefined");
}

#[test]
fn a_program_declaring_its_own_undefined_keeps_its_declaration() {
    // **The bug this test exists for**, found by probing rather than by reading:
    // the first version seeded a type for whatever symbol occupied the
    // `globals["undefined"]` slot. `merge_globals` puts a script's own
    // top-level names there, so a file declaring `var undefined: string` had
    // its declared type CLOBBERED and answered `undefined`.
    //
    // The binder now records *which* symbol it synthesised
    // (`BindResult::undefined_symbol`, `None` when the program declared its
    // own), so the checker seeds only the symbol it is entitled to. Upstream
    // has the same precedence by a different route: `mergeGlobalSymbol` merges
    // a file's declaration into whatever is already there.
    assert_eq!(type_of_declaration("var undefined: string;\nvar y = undefined;", "y"), "string");
}

#[test]
fn a_let_initialised_with_undefined_records_a_known_divergence() {
    // **Upstream says `any` here and this port says `undefined`. This test
    // records a wrong answer, deliberately, and must not be read as pinning
    // one.**
    //
    // Upstream's `undefinedSymbol` carries `undefinedWideningType`, whose whole
    // purpose is that `getWidenedType` turns it into `any`; `const` keeps
    // `undefined` and `let`/`var` widen. `crate::intrinsics` has exactly one
    // `undefined` (`intrinsics.rs:108`) and no widening variant, so the
    // distinction upstream draws between two types that PRINT ALIKE cannot be
    // drawn here — the same shape as the enum divergence replaced in `038def4`.
    //
    // Measured exposure: 22 `let`/`var` sites in the corpus against ~1,675
    // lines the symbol makes right. Accepted as a net-positive divergence
    // rather than hidden, and recorded here so that **adding a widening
    // `undefined` reddens this test and forces it to be deleted** rather than
    // leaving the divergence to be rediscovered.
    assert_eq!(type_of_declaration("let x = undefined;", "x"), "undefined");
    assert_eq!(type_of_declaration("var x = undefined;", "x"), "undefined");
}
