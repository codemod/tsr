//! `processPragmasIntoFields`' parse diagnostics (`parser.go:6581`): TS1084
//! for a `reference` pragma naming none of `types`, `lib` and `path`, and
//! TS1453 for a `resolution-mode` other than `require` or `import`.
//! Expectations were checked against a native `tsgo` built from the pinned
//! submodule. Needs `docs/parity/notes/r6-smallcodes4-pragma-diagnostics.diff`.

use tsr_core::Arena;

fn reports(source: &str) -> Vec<(String, u32, u32)> {
    let arena = Arena::new();
    let source = arena.alloc_str(source);
    tsr_parser::parse(&arena, source)
        .diagnostics
        .iter()
        .map(|d| (d.code().to_string(), d.span.start, d.span.end))
        .collect()
}

#[test]
fn an_invalid_resolution_mode_is_ts1453_on_its_value() {
    // `/index.ts(1,45)` in `nodeModulesTripleSlashReferenceModeOverrideModeError`.
    let source = "/// <reference types=\"pkg\" resolution-mode=\"esm\"/>\nexport {};\n";
    assert_eq!(reports(source), [("TS1453".to_string(), 44, 47)]);
}

#[test]
fn a_reference_naming_no_target_is_ts1084() {
    let source = "/// <reference foo=\"x\" />\nexport {};\n";
    let reports = reports(source);
    assert_eq!(reports.len(), 1);
    assert_eq!(reports[0].0, "TS1084");
}

#[test]
fn valid_references_do_not_report() {
    let source = "/// <reference types=\"pkg\" resolution-mode=\"import\"/>\n\
                  /// <reference types=\"pkg\" resolution-mode=\"require\"/>\n\
                  /// <reference path=\"a.ts\" />\n/// <reference lib=\"es2015\" />\n";
    assert!(reports(source).is_empty());
}
