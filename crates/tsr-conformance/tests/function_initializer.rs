//! Native 5b1047d1 Program syntax+semantic controls, including recovery-owned
//! subtrees. The owner witness admits no property, binding or signature default.
use tsr_conformance::{TestCase, diagnostics_suite::reported_for};

fn diagnostics(source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/function-initializer", "owner.ts", source);
    // These single-unit controls have no harness directives. Keep raw CRLF
    // bytes instead of testing TestCase's normalised LF source twice.
    assert_eq!(case.files.len(), 1);
    case.files[0].content = source.to_string();
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "false".into());
    let mut out =
        reported_for(&case).into_iter().map(|d| (d.line, d.column, d.code)).collect::<Vec<_>>();
    out.sort_unstable();
    out
}

#[test]
fn complete_named_function_and_plain_block_owners_keep_native_bags() {
    let source = include_str!("fixtures/function_initializer/Complete.ts");
    for source in [source.to_string(), source.replace('\n', "\r\n")] {
        assert_eq!(
            diagnostics(&source),
            [
                (3, 21, 1109),
                (4, 25, 2739),
                (5, 25, 2739),
                (5, 68, 1109),
                (5, 74, 2739),
                (6, 26, 2739),
                (7, 19, 2739),
                (8, 19, 2739),
                (8, 41, 2739),
                (9, 62, 2739),
                (10, 20, 1109),
                (11, 24, 2739),
            ]
        );
    }
}

#[test]
fn object_function_and_new_initializers_and_defaults_keep_native_positions() {
    // The published binding-token certificate also accepts `var object`.
    // Its native TS2739(6,9) must survive reuse of that leaf under this owner.
    assert_eq!(
        diagnostics(include_str!("fixtures/function_initializer/InitializerForms.ts")),
        [
            (4, 24, 1109),
            (6, 9, 2739),
            (7, 9, 2322),
            (8, 9, 2739),
            (9, 9, 2739),
            (11, 24, 2739),
            (12, 26, 2322),
            (13, 21, 2739),
            (14, 22, 2739),
        ]
    );
    // A separate, native-replayed ordinary identifier positive checks the object
    // initializer; the original keyword witness above is neither rewritten nor
    // removed from the native occurrence ledger.
    assert_eq!(
        diagnostics(include_str!("fixtures/function_initializer/Ordinary.ts")),
        [(3, 13, 1109), (4, 20, 2739),]
    );
    assert_eq!(
        diagnostics(include_str!("fixtures/function_initializer/ClosingTrivia.ts")),
        [(3, 13, 1109), (4, 20, 2739), (4, 52, 2739),]
    );
}

#[test]
fn original_local_and_parameter_occurrences_are_restored() {
    let actual = diagnostics(include_str!("fixtures/function_initializer/Context.ts"));
    for key in [(5, 25, 2739), (8, 19, 2739)] {
        assert!(actual.contains(&key), "missing native occurrence {key:?}: {actual:?}");
    }
}

#[test]
fn missing_header_body_close_and_swallowed_owners_do_not_certify_leaves() {
    for source in [
        include_str!("fixtures/function_initializer/MissingClose.ts"),
        include_str!("fixtures/function_initializer/MissingHeader.ts"),
        include_str!("fixtures/function_initializer/Signature.ts"),
        include_str!("fixtures/function_initializer/CommentClose.ts"),
        include_str!("fixtures/function_initializer/ParameterRecovery.ts"),
    ] {
        // Their complete native bags still contain errors this port misses.
        // A decline is not asserted to mean native semantic silence.
        assert!(diagnostics(source).iter().all(|(_, _, code)| *code != 2739 && *code != 2322));
    }
    let recovered = diagnostics(include_str!("fixtures/function_initializer/Recovery.ts"));
    let relations: Vec<_> =
        recovered.into_iter().filter(|(_, _, code)| *code == 2739 || *code == 2322).collect();
    assert_eq!(relations, [(4, 25, 2739), (5, 52, 2739)]);
}

#[test]
fn function_certification_preserves_source_identity_and_ambient_declines() {
    use tsr_checker::{Checker, check::FileContext, resolution::ModuleHost};
    use tsr_conformance::types_producer;
    use tsr_core::Arena;

    let case = TestCase::parse(
        "probe/function-source",
        "owner.ts",
        include_str!("fixtures/function_initializer/Complete.ts"),
    );
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let foreign_arena = Arena::new();
    let foreign = types_producer::program_for_case(&foreign_arena, &case);
    let root = program.source_file("owner.ts").unwrap().source_file().node_id.unwrap();
    assert_eq!(root, foreign.source_file("owner.ts").unwrap().source_file().node_id.unwrap());
    for (host, ambient) in [
        (None, false),
        (Some(&foreign as &dyn ModuleHost), false),
        (Some(&program as &dyn ModuleHost), true),
    ] {
        let mut checker =
            Checker::with_module_host(program.binder(), program.nodes(), program.node_map(), host);
        checker.check_source_file(root, FileContext { ambient, has_parse_errors: true });
        assert!(
            checker
                .diagnostics()
                .iter()
                .all(|(_, d)| d.message.code() != 2739 && d.message.code() != 2322)
        );
    }
}
