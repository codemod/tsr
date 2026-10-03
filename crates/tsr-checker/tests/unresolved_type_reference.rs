//! A type reference whose name does not resolve prints **the name that was
//! written** — `bd tsr-eep`.
//!
//! This looks like the one thing this port refuses everywhere else: answering
//! something for a name it could not resolve. It is the opposite. Upstream
//! reports `TS2304 Cannot find name` *and prints the name anyway*
//! (`getUnresolvedSymbolForEntityName`, `checker.go:23102`; the
//! `CheckFlagsUnresolved` branch of `getTypeFromTypeAliasReference`,
//! `checker.go:23580`). `conformance/parserRealSource11` carries **1,006** of
//! those errors, records `>nodeType : NodeType` throughout, and contains
//! **zero** ` : any` lines. Answering `errorType` was the divergence.

use tsr_ast::Statement;
use tsr_checker::Checker;
use tsr_core::Arena;

fn type_of_annotation(source: &str) -> String {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let last = *parsed.source_file.statements.last().expect("a statement");
    let Statement::VariableStatement(statement) = last else { panic!("want a var statement") };
    let declaration = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .expect("one declaration");
    let annotation = declaration.r#type.expect("an annotation");
    let id = checker.get_type_from_type_node(annotation);
    checker.type_to_string(id)
}

/// Baseline: `conformance/parserRealSource11` records `>nodeType : NodeType`
/// with 348 `Cannot find name 'NodeType'` errors against it.
#[test]
fn an_unresolved_name_prints_itself() {
    assert_eq!(type_of_annotation("let x: NodeType;"), "NodeType");
}

/// A qualified name whose root **does not resolve** is minted here.
///
/// The discriminator is upstream's own control flow:
/// `getUnresolvedSymbolForEntityName` is reached *only* when `resolveEntityName`
/// failed, and that begins by resolving the leftmost name
/// (`resolveQualifiedName`, `checker.go:15829`).
///
/// Shipping this arm ungated gained 8,797 corpus lines against 4,645 gated —
/// and a `matched`-count bar could not tell how many of the extra 4,152 were
/// gap→**wrong**, because a gap that becomes wrong moves nothing the bar
/// watches. Measured through the gap count instead: the gated arm converts
/// 4,645 and turns 135 gaps into wrong lines, 34:1.
#[test]
fn an_unresolved_qualified_name_is_minted_when_its_root_does_not_resolve() {
    assert_eq!(type_of_annotation("let x: TypeScript.AST;"), "TypeScript.AST");
}

/// The other arm: the root **does** resolve, so upstream had a real symbol.
///
/// **This assertion was `"error"` until design W landed.** It stood for
/// *"`resolveEntityName` is unported"*, and the gap it recorded was priced at
/// 4,557 corpus lines by `docs/architecture/checker-notes-qualname.md`. The arm
/// now resolves the name through the namespace's exports and reprints the
/// written text; `tests/qualified_type_reference.rs` owns the rest of its
/// behaviour, including the one position where it still refuses.
#[test]
fn a_qualified_name_whose_root_resolves_prints_the_written_name() {
    let source = "namespace M { export interface I {} }\nlet x: M.I;";
    assert_eq!(type_of_annotation(source), "M.I");
}

/// Type arguments go on the alias upstream, so they are printed.
#[test]
fn type_arguments_are_printed() {
    assert_eq!(type_of_annotation("let x: Foo<string>;"), "Foo<string>");
}

/// A gap **inside** an argument makes the whole reference a gap: printing
/// `Foo<error>` would be a wrong line rather than a missing one.
/// The assertion here was first written as the *printed* form, contradicting
/// the test's own name — `Foo<…>` is exactly what must **not** come out. The
/// argument `Bar[]` gaps (no global `Array` in this harness), so the whole
/// reference gaps.
#[test]
fn a_gapped_type_argument_gaps_the_whole_reference() {
    assert_eq!(type_of_annotation("let x: Foo<Bar[]>;"), "error");
}

/// **FLIPPED at §271, and the half that stays is the `+` half.** This pinned
/// `error` for `a * 2` on the argument that answering `number` for an
/// unresolvable operand is ADR-0038's forbidden rendering. §271 measured the
/// blanket claim: upstream computes `number` for ANY-like operands including
/// `errorType` (`checker.go:12358`), and the corpus population where that
/// converts a gap into a WRONG number measured EMPTY (31 right, 0 adverse).
/// So the multiplicative arm now answers `number` — upstream's own answer
/// for exactly this program — while `check_addition` keeps propagating (its
/// upstream answers errorType there, `checker.go:12452`, and §272 measured
/// the signature-level analogue at 405 GAP→WRONG; the question is per-site).
///
/// The minted type still answers `is_error` for every OTHER consumer.
#[test]
fn an_unresolved_arithmetic_operand_answers_number_and_addition_keeps_the_gap() {
    let source = "let a: NodeType; let x = a * 2;";
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let Statement::VariableStatement(statement) = parsed.source_file.statements[1] else {
        panic!("want a var statement")
    };
    let initialiser = statement
        .declaration_list
        .and_then(|list| list.declarations.first().copied())
        .and_then(|d| d.initializer)
        .expect("an initialiser");
    let id = checker.check_expression(initialiser);
    assert_eq!(checker.type_to_string(id), "number");
}

fn annotation_queries(source: &str, order: &[usize]) -> Vec<String> {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, source);
    assert!(parsed.diagnostics.is_empty(), "fixture must parse");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: source },
    );
    let annotations: Vec<_> = parsed
        .source_file
        .statements
        .iter()
        .filter_map(|statement| {
            let Statement::VariableStatement(statement) = statement else { return None };
            statement.declaration_list?.declarations.first()?.r#type
        })
        .collect();
    let mut checker = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    order
        .iter()
        .map(|&index| {
            let id = checker.get_type_from_type_node(annotations[index]);
            checker.type_to_string(id)
        })
        .collect()
}

// Native completion interns unresolved symbols by full parent path; type
// arguments belong to the type reference, not that static symbol identity.
#[test]
fn unresolved_paths_and_arguments_survive_repeated_and_reordered_queries() {
    let source = "let a: Missing.Child; let b: Other.Child; let c: Missing.Child; \
                  let d: MissingFoo<string>; let e: MissingFoo<number>;";
    assert_eq!(
        annotation_queries(source, &[0, 1, 2, 3, 4, 0, 4, 3]),
        [
            "Missing.Child",
            "Other.Child",
            "Missing.Child",
            "MissingFoo<string>",
            "MissingFoo<number>",
            "Missing.Child",
            "MissingFoo<number>",
            "MissingFoo<string>",
        ]
    );
    assert_eq!(
        annotation_queries(source, &[4, 3, 2, 1, 0]),
        [
            "MissingFoo<number>",
            "MissingFoo<string>",
            "Missing.Child",
            "Other.Child",
            "Missing.Child"
        ]
    );
}

// Both instantiations visit the same alias-body reference to T. Symbol
// completion must leave its current substitution outside the static memo.
#[test]
#[ignore = "tsr-6.57: generic identity alias retains its reference instead of native primitive result"]
fn a_repeated_alias_body_reference_uses_each_instantiations_binding() {
    let source = "type Identity<T> = T; let a: Identity<string>; let b: Identity<number>;";
    assert_eq!(annotation_queries(source, &[0, 1, 0, 1]), ["string", "number", "string", "number"]);
    assert_eq!(annotation_queries(source, &[1, 0, 1, 0]), ["number", "string", "number", "string"]);
}

#[test]
fn checker_unknown_symbols_are_private_even_when_the_bound_program_is_shared() {
    let arena = Arena::new();
    let parsed = tsr_parser::parse(&arena, "let x: Missing.Child<string>;");
    let bound = tsr_binder::bind(
        &arena,
        parsed.source_file,
        &parsed.nodes,
        tsr_binder::FileInfo { name: "t.ts", text: "let x: Missing.Child<string>;" },
    );
    let first = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let second = Checker::new(&bound, &parsed.nodes, &parsed.node_map);
    let handle = first.symbol_access().unknown();
    assert_eq!(first.symbol_access().view(&handle).unwrap().name(), "unknown");
    assert!(matches!(
        second.symbol_access().view(&handle),
        Err(tsr_checker::symbol_access::SymbolAccessError::ForeignChecker)
    ));
}
