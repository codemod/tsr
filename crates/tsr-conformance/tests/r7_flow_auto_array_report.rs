//! TS7034/TS7005 print the flow type's identity: `autoArrayType` is `any[]`
//! (`checkIdentifier`, pinned checker.go:11182; `createFinalArrayType`,
//! flow.go:1582).
//!
//! Expected positions are pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb
//! on the same source (the 7034/7005 messages print `any[]` for `r`, `x` and
//! `C`, and `any` for the outer read of `s`).

use tsr_conformance::{TestCase, diagnostics_suite};

#[test]
fn auto_array_reads_report_any_array() {
    let source = "declare const c: boolean;
function a() {
    let r;
    if (c) r ??= []; else r = [1];
    r;
}
function d() {
    let x = [];
    let y = x;
    x.push(5);
    let z = x;
}
function g2() {
    let x = [];
    x.push(5);
    function g() {
        x;
    }
}
function g3() {
    let s;
    function g() { s; }
    s = [];
}
var C = [];
for (var k in C) {
    if (C.hasOwnProperty(k)) {
    }
}";
    let mut case = TestCase::parse("probe/auto-array-report", "probe.ts", source);
    case.options.insert("target".into(), "es2015".into());
    case.options.insert("strict".into(), "true".into());
    case.options.insert("noImplicitAny".into(), "true".into());
    let mut reported: Vec<_> = diagnostics_suite::reported_for(&case)
        .into_iter()
        .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
        .collect();
    reported.sort_unstable();
    assert_eq!(
        reported,
        [
            (3, 9, 7034),
            (5, 5, 7005),
            (8, 9, 7034),
            (9, 13, 7005),
            (14, 9, 7034),
            (17, 9, 7005),
            (21, 9, 7034),
            (22, 20, 7005),
            (25, 5, 7034),
            (26, 15, 7005),
            (27, 9, 7005),
        ]
    );
}
