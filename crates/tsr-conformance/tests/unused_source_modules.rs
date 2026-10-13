//! Native 5b1047d: augmented source-file modules are not unused locals.

use tsr_conformance::{TestCase, diagnostics_suite};

const MODULE: &str = "// @strict: true
// @target: es2022
// @module: nodenext
// @noUnusedLocals: true
// @noUnusedParameters: true
// @filename: mod.ts
/**
 * Module under augmentation.
 */
export interface Shape { value: number }
export const token = 1;
// @filename: use.ts
";

#[test]
fn augmented_source_modules_are_not_unused_locals() {
    for (body, expected) in [
        (
            "export {};
declare module './mod.js' { interface Shape { extra: string } }
",
            vec![],
        ),
        (
            "import { token } from './mod.js';
declare module './mod.js' { interface Shape { extra: string } }
const local = 1;
function hidden() {}
namespace Hidden { export const member = 1; }
export function read(parameter: number) { return 1; }
",
            vec![(1, 10, 6133), (3, 7, 6133), (4, 10, 6133), (5, 11, 6133), (6, 22, 6133)],
        ),
    ] {
        let source = format!("{MODULE}{body}");
        let mut case = TestCase::parse("probe/unused-source-modules", "fixture.ts", &source);
        for _ in 0..2 {
            let mut diagnostics: Vec<_> = diagnostics_suite::reported_for(&case)
                .into_iter()
                .map(|d| (d.file, d.line, d.column, d.code))
                .collect();
            diagnostics.sort_unstable();
            let expected: Vec<_> = expected
                .iter()
                .map(|&(line, column, code)| ("use.ts".to_string(), line, column, code))
                .collect();
            assert_eq!(diagnostics, expected, "{source}");
            case.files.reverse();
        }
    }
}
