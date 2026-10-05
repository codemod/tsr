//! Narrow variable-consumer prerequisite; native Program syntax + semantic APIs
//! at 5b1047d1 establish the positions independently of TSR's error flags.
use tsr_checker::{Checker, check::FileContext, resolution::ModuleHost};
use tsr_conformance::{TestCase, diagnostics_suite::reported_for, types_producer};
use tsr_core::Arena;

const SOURCE: &str = "export {};\ninterface Required { tag: string; count: number }\nclass Empty {}\nvar beforeObject: Required = {};\nvar beforeFunction: Required = function () {};\nvar beforeNewCall: Required = new Empty();\nvar beforeNewBare: Required = new Empty;\nconst broken = ;\nvar afterObject: Required = {};\nvar afterFunction: Required = function () {};\nvar afterNewCall: Required = new Empty();\nvar afterNewBare: Required = new Empty;\nvar primitive: number = {};\nconst assignable: Empty = {};";

fn case(source: &str) -> TestCase {
    let mut case = TestCase::parse("probe/source-variable-initializer", "initializer.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "false".into());
    case
}

#[test]
fn complete_root_initializers_on_both_sides_of_an_error_keep_native_occurrences() {
    let mut actual: Vec<_> =
        reported_for(&case(SOURCE)).into_iter().map(|d| (d.line, d.column, d.code)).collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            (4, 5, 2739),
            (5, 5, 2322),
            (6, 5, 2739),
            (7, 5, 2739),
            (8, 16, 1109),
            (9, 5, 2739),
            (10, 5, 2322),
            (11, 5, 2739),
            (12, 5, 2739),
            (13, 5, 2322),
        ]
    );
}

#[test]
fn unavailable_or_foreign_source_does_not_certify_a_declaration() {
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case(SOURCE));
    let foreign_arena = Arena::new();
    let foreign = types_producer::program_for_case(&foreign_arena, &case(SOURCE));
    let file = program.source_file("initializer.ts").unwrap().source_file().node_id.unwrap();
    assert_eq!(file, foreign.source_file("initializer.ts").unwrap().source_file().node_id.unwrap());
    for host in [None, Some(&foreign as &dyn ModuleHost)] {
        let mut checker =
            Checker::with_module_host(program.binder(), program.nodes(), program.node_map(), host);
        checker.check_source_file(file, FileContext { ambient: false, has_parse_errors: true });
        assert!(
            checker
                .diagnostics()
                .iter()
                .all(|(_, d)| d.message.code() != 2322 && d.message.code() != 2739)
        );
    }
}

#[test]
fn trivia_unicode_asi_and_crlf_preserve_written_child_extents() {
    let source = "export {};\ninterface Required { tag: string; count: number }\nclass Empty {}\nconst broken = ;\nvar café /* name */ : /* type */ Required = { /* object */ };\nvar fn: Required = function /* parameters */ ( /* empty */ ) { /* body */ };\nvar call: Required = new /* callee */ Empty /* args */ ( /* empty */ );\nvar bare: Required = new /* callee */ Empty;\nvar asi: Required = {}\nvar after: Required = new Empty()";
    for source in [source.to_string(), source.replace('\n', "\r\n")] {
        let mut actual: Vec<_> =
            reported_for(&case(&source)).into_iter().map(|d| (d.line, d.column, d.code)).collect();
        actual.sort_unstable();
        assert_eq!(
            actual,
            [
                (4, 16, 1109),
                (5, 5, 2739),
                (6, 5, 2322),
                (7, 5, 2739),
                (8, 5, 2739),
                (9, 5, 2739),
                (10, 5, 2739),
            ]
        );
    }
}

#[test]
fn packed_repeated_declarations_keep_each_occurrence() {
    let source = "export {};\ninterface Required { tag: string; count: number } class Empty {}\nvar repeated: Required = {}; var repeated: Required = {}; const broken = ; var repeated: Required = {}; var repeated: Required = {};\nvar packedObject: Required = {}, packedFunction: Required = function () {}, packedNew: Required = new Empty(), packedBare: Required = new Empty;";
    let mut actual: Vec<_> =
        reported_for(&case(source)).into_iter().map(|d| (d.line, d.column, d.code)).collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            (3, 5, 2739),
            (3, 34, 2739),
            (3, 74, 1109),
            (3, 80, 2739),
            (3, 109, 2739),
            (4, 5, 2739),
            (4, 34, 2322),
            (4, 77, 2739),
            (4, 112, 2739),
        ]
    );
}

#[test]
fn swallowed_body_is_not_a_source_file_owned_positive() {
    // Native still reports at the two later declarations; this certificate does
    // not admit their recovered TSR function owner. The pending parity gap is
    // retained in the complete authored-control bags, not called native silence.
    let source = "export {};\ninterface Required { tag: string; count: number }\nvar before: Required = {};\nvar recovered: Required = function () ;\nvar swallowed: Required = {};";
    let actual: Vec<_> = reported_for(&case(source))
        .into_iter()
        .filter(|d| d.code == 2322 || d.code == 2739)
        .map(|d| (d.line, d.column, d.code))
        .collect();
    assert_eq!(actual, [(3, 5, 2739)]);
}

#[test]
fn ambient_context_keeps_its_existing_decline() {
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case(SOURCE));
    let file = program.source_file("initializer.ts").unwrap().source_file().node_id.unwrap();
    let mut checker = types_producer::configured_checker(&program);
    checker.check_source_file(file, FileContext { ambient: true, has_parse_errors: true });
    assert!(
        checker
            .diagnostics()
            .iter()
            .all(|(_, d)| d.message.code() != 2322 && d.message.code() != 2739)
    );
}

#[test]
fn javascript_keeps_its_existing_decline() {
    let mut case = TestCase::parse("probe/source-variable-js", "initializer.js", SOURCE);
    case.options.insert("allowjs".into(), "true".into());
    case.options.insert("checkjs".into(), "true".into());
    let arena = Arena::new();
    let program = types_producer::program_for_case(&arena, &case);
    let file = program.source_file("initializer.js").unwrap().source_file().node_id.unwrap();
    let mut checker = types_producer::configured_checker(&program);
    checker.check_source_file(file, FileContext { ambient: false, has_parse_errors: true });
    assert!(
        checker
            .diagnostics()
            .iter()
            .all(|(_, d)| d.message.code() != 2322 && d.message.code() != 2739)
    );
}
