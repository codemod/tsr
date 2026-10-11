//! `getSymbolIfSameReference` resolves an `esModuleInterop` namespace import
//! to `resolveESModuleSymbol`'s module clone, so a written `moment.Moment`
//! from `moment`'s own file is not reused where `moment` is such an import;
//! the type serializes as `Moment` (`docs/parity/notes/r7-printer.md` §29).
//! Expected lines from pinned native tsgo 5b1047d's
//! `moduleAugmentationDuringSyntheticDefaultCheck` baseline.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn a_written_name_is_not_reused_through_a_namespace_import_clone() {
    let case = TestCase::parse(
        "probe/reuse_through_module_clone",
        "idx.ts",
        r#"// @target: es2015
// @module: commonjs
// @noImplicitReferences: true
// @esModuleInterop: true
// @filename: node_modules/moment/index.d.ts
declare function moment(): moment.Moment;
declare namespace moment {
  interface Moment extends Object {
    valueOf(): number;
  }
}
export = moment;
// @filename: node_modules/moment-timezone/index.d.ts
import * as moment from 'moment';
export = moment;
declare module "moment" {
    interface Moment {
        tz(): string;
    }
}
"#,
    );
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<String> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for wanted in [
        "moment : { default: () => moment.Moment; }",
        "\"moment\" : () => Moment",
        "moment : () => moment.Moment",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
    assert!(!lines.iter().any(|line| line == "\"moment\" : () => moment.Moment"), "{lines:?}");
}
