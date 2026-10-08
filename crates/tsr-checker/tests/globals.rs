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
fn a_let_initialised_with_undefined_widens_only_without_strict_null_checks() {
    // Upstream's `undefinedSymbol` carries `undefinedWideningType`
    // (`checker.go:1345`), which in strict mode *is* `undefinedType` and
    // otherwise is a twin `getWidenedType` turns into `any`. This test used
    // to record the opposite answer, as a known divergence, until the
    // widening twin existed (docs/parity/notes/contextual.md §7).
    assert_eq!(type_of_declaration("let x = undefined;", "x"), "undefined");
    assert_eq!(type_of_declaration("var x = undefined;", "x"), "undefined");
}

fn other_intrinsic_indices(i: &tsr_checker::Intrinsics) -> [usize; 24] {
    [
        i.any,
        i.error,
        i.unresolved,
        i.unknown,
        i.undefined,
        i.missing,
        i.null,
        i.string,
        i.number,
        i.bigint,
        i.regular_false,
        i.false_type,
        i.regular_true,
        i.true_type,
        i.boolean,
        i.empty_object,
        i.unknown_empty_object,
        i.unknown_union,
        i.es_symbol,
        i.void,
        i.never,
        i.implicit_never,
        i.unreachable_never,
        i.non_primitive,
    ]
    .map(tsr_checker::types::TypeId::index)
}

#[test]
fn intrinsic_construction_defaults_to_strict_without_moving_other_allocations() {
    let mut store = tsr_checker::types::TypeStore::new();
    let i = tsr_checker::Intrinsics::create(&mut store);
    // The loose identities occupy slots 5 (undefined) and 8 (null, created
    // right after nullType as upstream does, checker.go:990); the active
    // strict slots alias the ordinary types. Slots 17-19 are the regular
    // `""`, `0` and `0n` (`checker.go:1049`), interned after `boolean`.
    assert_eq!(store.len(), 29);
    assert_eq!(
        other_intrinsic_indices(&i),
        [0, 1, 2, 3, 4, 6, 7, 9, 10, 11, 12, 13, 14, 15, 16, 20, 21, 22, 23, 24, 25, 26, 27, 28]
    );
    assert_eq!(i.undefined_widening, i.undefined);
    assert_eq!(i.null_widening, i.null);
}

#[test]
fn undefined_slot_reselection_is_allocation_free_and_reseeds_the_global() {
    for apply_options in [false, true] {
        let arena = Arena::new();
        let parsed = tsr_parser::parse(&arena, "");
        let bound = tsr_binder::bind(
            &arena,
            parsed.source_file,
            &parsed.nodes,
            tsr_binder::FileInfo { name: "test.ts", text: "" },
        );
        let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
        let ordinary = checker.intrinsics().undefined;
        let other_ids = other_intrinsic_indices(checker.intrinsics());
        assert_eq!(checker.intrinsics().undefined_widening, ordinary);
        for strict in [true, false, false, true, true, false, true] {
            if apply_options {
                checker.apply_compiler_options(&tsr_core::CompilerOptions {
                    strict_null_checks: if strict {
                        tsr_core::Tristate::True
                    } else {
                        tsr_core::Tristate::False
                    },
                    ..tsr_core::CompilerOptions::default()
                });
            } else {
                checker.set_strict_null_checks(strict);
            }
            let i = checker.intrinsics();
            assert_eq!(i.undefined_widening.index(), if strict { 4 } else { 5 });
            assert_eq!(i.null_widening.index(), if strict { 7 } else { 8 });
            assert_eq!(checker.type_count(), 29);
            assert_eq!(other_intrinsic_indices(i), other_ids);
            assert_eq!(
                checker.type_of(i.undefined_widening).flags,
                tsr_checker::TypeFlags::UNDEFINED
            );
            assert_eq!(checker.type_of(i.null_widening).flags, tsr_checker::TypeFlags::NULL);
        }
        // Selecting loose re-seeds the global with the widening twin
        // (`checker.go:1345`); strict puts the ordinary type back.
        checker.set_strict_null_checks(false);
        let widening = checker.intrinsics().undefined_widening;
        assert_ne!(widening, ordinary);
        assert_eq!(checker.get_type_of_symbol(bound.undefined_symbol().unwrap()), widening);
        checker.set_strict_null_checks(true);
        assert_eq!(checker.get_type_of_symbol(bound.undefined_symbol().unwrap()), ordinary);
    }
}
