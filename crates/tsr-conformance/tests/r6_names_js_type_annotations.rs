//! r6-names: a TypeScript-only type annotation in a JavaScript file is still
//! resolved (`crates/tsr-checker/src/name_slots.rs`, `names_in_jsdoc`).
//! Expectations are native tsgo's compiler-test harness output at the pinned
//! 5b1047d (`allowJs`, `checkJs`, `target es2015`).

use tsr_conformance::{TestCase, diagnostics_suite};

fn diagnostics(file: &str, source: &str) -> Vec<(u32, u32, u32)> {
    let mut case = TestCase::parse("probe/r6-names", file, source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("allowjs".into(), "true".into());
    case.options.insert("checkjs".into(), "true".into());
    case.options.insert("noemit".into(), "true".into());
    let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    diagnostics.sort_unstable();
    diagnostics
}

#[test]
fn typescript_annotations_in_javascript_resolve_their_names() {
    let source = "type A = Missing1;
function f(x: Missing2): Missing3 { return x; }
let v: Missing4 = 1;
class C<T extends Missing5> { p: Missing6; }
interface I { a: Missing7 }
var r = (): Missing12 => 1;";
    assert_eq!(
        diagnostics("a.js", source),
        [
            (1, 6, 8008),
            (1, 10, 2304),
            (2, 15, 2304),
            (2, 15, 8010),
            (2, 26, 2304),
            (2, 26, 8010),
            (3, 8, 2304),
            (3, 8, 8010),
            (4, 9, 8004),
            (4, 19, 2304),
            (4, 34, 2304),
            (4, 34, 8010),
            (5, 11, 8006),
            (5, 18, 2304),
            (6, 13, 2304),
            (6, 13, 8010),
        ],
    );
}
