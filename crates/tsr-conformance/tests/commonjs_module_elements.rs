//! Complete pinned-native module element diagnostics, including strict/nonstrict
//! dot/bracket, suggestion, applicable index and class-clone boundaries. Every
//! native type assertion is reported too; missing-access error/any display gaps
//! are outside this diagnostic-only prerequisite and remain explicit.
//! TS exports and namespace imports certify the original `SourceFile` receiver
//! without the separate JS require/plain-exports producer port. The original
//! Strict/Loose JS fixtures remain retained for that producer's later controls.

use tsr_conformance::{
    TestCase, diagnostics_suite, errors_baseline, types_baseline, types_producer,
};

#[test]
fn commonjs_module_element_diagnostics_match_every_native_occurrence() {
    let mut failures = Vec::new();
    let mut assertions = 0;
    let mut type_mismatches = 0;
    let mut occurrences = 0;
    for (name, source, types, errors) in [
        (
            "strict",
            include_str!("fixtures/commonjs_module_element/SourceModuleStrict.ts"),
            include_str!("fixtures/commonjs_module_element/SourceModuleStrict.types"),
            include_str!("fixtures/commonjs_module_element/SourceModuleStrict.errors.txt"),
        ),
        (
            "loose",
            include_str!("fixtures/commonjs_module_element/SourceModuleLoose.ts"),
            include_str!("fixtures/commonjs_module_element/SourceModuleLoose.types"),
            include_str!("fixtures/commonjs_module_element/SourceModuleLoose.errors.txt"),
        ),
    ] {
        let case = TestCase::parse(&format!("probe/module-element-{name}"), "control.ts", source);
        let expected = types_baseline::parse(types);
        let actual = types_producer::assertions_for_case(&case, &expected, false);
        if actual.len() != expected.len() {
            failures.push(format!(
                "{name}: file population {} != {}",
                actual.len(),
                expected.len()
            ));
        }
        for (wanted, got) in expected.iter().zip(&actual) {
            assertions += wanted.assertions.len();
            let actual = got.iter().map(types_producer::Assertion::line).collect::<Vec<_>>();
            let expected = wanted
                .assertions
                .iter()
                .map(|assertion| assertion.text.clone())
                .collect::<Vec<_>>();
            assert_eq!(actual.len(), expected.len(), "{name}: assertion population");
            for (position, (expected, actual)) in expected.iter().zip(&actual).enumerate() {
                type_mismatches += usize::from(expected != actual);
                println!("{name}:{}:{position}\t{expected}\t{actual}", wanted.file);
            }
        }
        let arena = tsr_core::Arena::new();
        let (program, _, ids) =
            types_producer::assertions_for_case_with_ids(&arena, &case, &expected);
        let mut checker = types_producer::configured_checker(&program);
        let mut module_sites = Vec::new();
        for id in ids.into_iter().flatten() {
            let ty = types_producer::type_id_at_location(
                &mut checker,
                program.binder(),
                program.nodes(),
                program.node_map(),
                id,
            );
            if let tsr_checker::types::TypeData::Anonymous { symbol, .. } = checker.type_of(ty).data
                && program
                    .binder()
                    .symbols()
                    .get(symbol)
                    .flags
                    .contains(tsr_binder::SymbolFlags::VALUE_MODULE)
            {
                let record = program.binder().symbols().get(symbol);
                println!(
                    "{name}: original module identity node={id:?} type={ty:?} symbol={symbol:?} flags={:?} declaration={:?}",
                    record.flags,
                    record.value_declaration.map(|declaration| program.nodes().kind(declaration))
                );
                assert_eq!(record.flags, tsr_binder::SymbolFlags::VALUE_MODULE);
                let file = record.value_declaration.expect("published module declaration");
                assert_eq!(program.nodes().kind(file), tsr_ast::SyntaxKind::SourceFile);
                assert_eq!(record.declarations.as_slice(), &[file]);
                assert_eq!(program.binder().symbol_of(file), Some(symbol));
                assert_eq!(checker.get_type_of_symbol(symbol), ty, "original module TypeId");
                module_sites.push((id, symbol, ty, checker.type_of(ty).data.clone()));
            }
        }
        assert!(!module_sites.is_empty(), "{name}: canonical module receiver must be exercised");
        for _ in 0..3 {
            module_sites.reverse();
            for &(id, symbol, ty, ref data) in &module_sites {
                assert_eq!(checker.get_type_of_symbol(symbol), ty, "warm/reversed publication");
                assert_eq!(
                    types_producer::type_id_at_location(
                        &mut checker,
                        program.binder(),
                        program.nodes(),
                        program.node_map(),
                        id,
                    ),
                    ty,
                    "expression reuses the original image"
                );
                assert_eq!(&checker.type_of(ty).data, data);
            }
        }
        let mut expected = errors_baseline::parse(errors);
        let mut actual = diagnostics_suite::reported_for(&case);
        expected.sort_unstable();
        actual.sort_unstable();
        println!(
            "{name}: complete diagnostic occurrences\nexpected: {expected:?}\nactual: {actual:?}"
        );
        occurrences += expected.len();
        if actual != expected {
            failures.push(format!("{name}: complete duplicate-aware diagnostics\nactual: {actual:?}\nexpected: {expected:?}"));
        }
    }
    println!(
        "complete native element controls: {assertions} recorded assertions ({type_mismatches} explicit mismatches), {occurrences} diagnostic occurrences"
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
