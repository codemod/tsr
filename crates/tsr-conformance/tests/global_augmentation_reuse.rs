//! A written reference to a global declared inside a module file's
//! `declare global` is reused in a signature print
//! (`docs/parity/notes/r6-printer.md` §1). Expectations from pinned native
//! tsgo 5b1047d.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

const SOURCE: &str = r#"// @strict: true
// @filename: a.ts
export {};
declare global {
    interface Pair<T, U = any> { first: T; second: U; }
    namespace Outer { interface Inner<T, U = any> { v: T; w: U; } }
}
// @filename: b.ts
function g(): Pair<number> { return null as any; }
function h(p: Outer.Inner<string>) { return p; }
declare const d: Pair<string>;
"#;

fn assertions(source: &str) -> Vec<String> {
    let case = TestCase::parse("probe/global_augmentation_reuse", "a.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect()
}

#[test]
fn a_global_augmentation_member_is_visible_to_node_reuse() {
    let lines = assertions(SOURCE);
    // `IsExternalModuleAugmentation` holds for `declare global` in a module
    // file, so `Pair` is accessible and the written `Pair<number>` is reused;
    // the type itself prints every argument.
    for wanted in ["g : () => Pair<number>", "d : Pair<string, any>"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
