//! Instantiation expressions (`getInstantiationExpressionType`,
//! `checker.go:10660`): the minted object keeps the receiver's members and
//! carries only the applicable signatures, instantiated.

use tsr_ast::{NodeId, Statement};
use tsr_checker::Checker;
use tsr_checker::check::FileContext;
use tsr_core::Arena;

/// The printed type of each `const` initializer, and the diagnostic codes.
fn check(source: &str) -> (Vec<String>, Vec<String>) {
    let arena = Arena::new();
    let mut nodes = tsr_ast::NodeTable::default();
    let mut node_map = tsr_ast::NodeMap::default();
    let file = tsr_parser::parse_into(
        &arena,
        source,
        tsr_parser::ParseOptions::for_file("a.ts"),
        &mut nodes,
        &mut node_map,
    );
    assert!(file.diagnostics.is_empty(), "fixture must parse");
    let root: NodeId = file.source_file.node_id.expect("a parsed file has an id");
    let bound = tsr_binder::bind_into(
        tsr_binder::BindResult::empty(),
        &arena,
        file.source_file,
        &nodes,
        tsr_binder::FileInfo { name: "a.ts", text: source },
    );
    let mut checker = Checker::new(&bound, &nodes, &node_map);
    checker.set_strict_null_checks(true);
    let mut printed = Vec::new();
    for statement in file.source_file.statements {
        let Statement::VariableStatement(variable) = statement else { continue };
        let Some(list) = variable.declaration_list else { continue };
        for declaration in list.declarations {
            if let Some(initializer) = declaration.initializer {
                let ty = checker.check_expression(initializer);
                printed.push(checker.type_to_string(ty));
            }
        }
    }
    checker.check_source_file(root, FileContext { ambient: false, has_parse_errors: false });
    let codes = checker.diagnostics().iter().map(|(_, d)| d.code().to_string()).collect();
    (printed, codes)
}

#[test]
fn applicable_signatures_are_instantiated_and_members_kept() {
    let (printed, codes) = check(
        "declare let f: { <T>(): T, g<U>(): U };\n\
         const a1 = f<number>;\n\
         const a2 = f.g<number>;\n",
    );
    assert_eq!(printed, ["{ (): number; g<U>(): U; }", "() => number"]);
    assert!(codes.is_empty(), "{codes:?}");
}

#[test]
fn no_applicable_signature_reports_ts2635_and_drops_the_signatures() {
    let (printed, codes) = check(
        "declare let f: { <T>(): T, g<U>(): U };\n\
         const a9 = (f<number>)<number>;\n",
    );
    assert_eq!(printed, ["{ g<U>(): U; }"]);
    assert_eq!(codes, ["TS2635"]);
}

#[test]
fn union_constituents_are_mapped() {
    let (printed, _) = check(
        "declare let g: (<T>(x: T) => T) | undefined;\n\
         const c = g<string>;\n",
    );
    assert_eq!(printed, ["((x: string) => string) | undefined"]);
}

#[test]
fn a_type_query_in_its_own_return_answers_the_gap_without_a_cycle() {
    // Native reads `h`'s signatures lazily and never re-enters its return.
    let (_, codes) = check("declare function h<T>(): typeof h<T>;\nconst x = h;\n");
    assert!(codes.is_empty(), "{codes:?}");
}
