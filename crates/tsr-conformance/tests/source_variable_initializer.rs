//! Narrow variable-consumer prerequisite; native Program syntax + semantic APIs
//! at 5b1047d1 establish the positions independently of TSR's error flags.
use tsr_checker::{Checker, check::FileContext, resolution::ModuleHost};
use tsr_conformance::{TestCase, diagnostics_suite::reported_for, types_producer};
use tsr_core::Arena;

const SOURCE: &str = "export {};\ninterface Required { tag: string; count: number }\nclass Empty {}\nvar beforeObject: Required = {};\nvar beforeFunction: Required = function () {};\nvar beforeNewCall: Required = new Empty();\nvar beforeNewBare: Required = new Empty;\nconst broken = ;\nvar afterObject: Required = {};\nvar afterFunction: Required = function () {};\nvar afterNewCall: Required = new Empty();\nvar afterNewBare: Required = new Empty;\nvar primitive: number = {};\nconst assignable: Empty = {};";

fn case(source: &str) -> TestCase {
    let mut case = TestCase::parse("probe/source-variable-initializer", "initializer.ts", source);
    // These single-unit controls have no harness directives. Preserve CRLF
    // bytes instead of exercising TestCase's normalised LF source twice.
    assert_eq!(case.files.len(), 1);
    case.files[0].content = source.to_string();
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
fn contextual_binding_keywords_keep_native_initializer_bags() {
    // Pinned native isBindingIdentifier accepts keywords after LastReservedWord.
    // These are AST Identifier bindings, but their source tokens are keywords.
    let source = "export {};\ninterface Required { tag: string; count: number }\nclass Empty {}\nvar object: Required = {};\nvar type: Required = function () {};\nvar readonly: Required = new Empty();\nvar as: number = {};\nconst broken = ;\nvar unknown: Required = {};\nvar from: Required = function () {};\nvar of: Required = new Empty;\nvar satisfies: Empty = {};\n";
    for source in [source.to_string(), source.replace('\n', "\r\n")] {
        let mut actual: Vec<_> =
            reported_for(&case(&source)).into_iter().map(|d| (d.line, d.column, d.code)).collect();
        actual.sort_unstable();
        assert_eq!(
            actual,
            [
                (4, 5, 2739),
                (5, 5, 2322),
                (6, 5, 2739),
                (7, 5, 2322),
                (8, 16, 1109),
                (9, 5, 2739),
                (10, 5, 2322),
                (11, 5, 2739),
            ]
        );
    }
}

#[test]
fn last_reserved_word_and_escaped_bindings_keep_their_source_declines() {
    let source = "interface BoundaryRequired { tag: string; count: number }\nconst broken = ;\nvar implements: BoundaryRequired = {};\nvar with: BoundaryRequired = {};\nvar after: BoundaryRequired = {};\nvar escaped: BoundaryRequired = {};\nvar \\u006fbject: BoundaryRequired = {};\n";
    // `with` is LastReservedWord, while `implements` is the first permitted
    // binding keyword. Native has no written variable owner for `var with`.
    // The escaped object binding retains the frozen Unicode-escape decline;
    // its native TS2739 at (7,5) remains a known unsupported occurrence.
    for source in [source.to_string(), source.replace('\n', "\r\n")] {
        let actual: Vec<_> = reported_for(&case(&source))
            .into_iter()
            .filter(|d| d.code == 2322 || d.code == 2739)
            .map(|d| (d.line, d.column, d.code))
            .collect();
        assert_eq!(actual, [(3, 5, 2739), (5, 5, 2739), (6, 5, 2739)]);
    }
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
    // Native reports at the two later declarations. The bodiless function
    // expression's body is empty (`parseBlock` without its `{`), so `swallowed`
    // is an ordinary declaration and its TS2739 is native's; the TS2322 at
    // (4,5) on the recovered function owner is still a pending parity gap, not
    // native silence.
    let source = "export {};\ninterface Required { tag: string; count: number }\nvar before: Required = {};\nvar recovered: Required = function () ;\nvar swallowed: Required = {};";
    let actual: Vec<_> = reported_for(&case(source))
        .into_iter()
        .filter(|d| d.code == 2322 || d.code == 2739)
        .map(|d| (d.line, d.column, d.code))
        .collect();
    assert_eq!(actual, [(3, 5, 2739), (5, 5, 2739)]);
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

/// The pinned native test runner (`TestLocal`, `allowJs` + `checkJs`) reports
/// the same initializer errors in a `.js` file as in `.ts`, beside each TS8010:
/// a type annotation in JavaScript is still the declared type.
#[test]
fn javascript_reports_native_initializer_errors() {
    let mut case = TestCase::parse("probe/source-variable-js", "initializer.js", SOURCE);
    case.options.insert("allowjs".into(), "true".into());
    case.options.insert("checkjs".into(), "true".into());
    let mut actual: Vec<_> = reported_for(&case)
        .into_iter()
        .filter(|d| d.code == 2322 || d.code == 2739)
        .map(|d| (d.line, d.column, d.code))
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            (4, 5, 2739),
            (5, 5, 2322),
            (6, 5, 2739),
            (7, 5, 2739),
            (9, 5, 2739),
            (10, 5, 2322),
            (11, 5, 2739),
            (12, 5, 2739),
            (13, 5, 2322),
        ]
    );
}
