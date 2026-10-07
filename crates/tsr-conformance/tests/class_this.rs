//! Class this annotation controls compared with pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};
#[test]
fn class_this_annotations_share_receiver_identity_in_method_signatures() {
    let source = r#"// @strict: true
// @target: es2015
class Base {
    value = 1;
    fluent(this: this): this { return this; }
    method(this: this, value: number): string { return String(value); }
}
class Derived extends Base { extra = "derived"; }
declare const base: Base;
declare const derived: Derived;
const baseMethod = base.method;
const derivedMethod = derived.method;
const fluent = derived.fluent();
export function result() { return { baseMethod, derivedMethod, fluent }; }
"#;
    let case = TestCase::parse("probe/class-this", "class-this.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in [
        "baseMethod : (this: Base, value: number) => string",
        "derivedMethod : (this: Derived, value: number) => string",
        "fluent : Derived",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}

#[test]
fn global_this_exposes_runtime_globals_and_recovers_missing_lexical_properties_with_any() {
    let source = r"// @strict: true
// @target: es2015
var runtime = 1;
let lexical = 2;
class LexicalClass {}
const visible = globalThis.runtime;
const hidden = globalThis.lexical;
const hiddenClass = globalThis.LexicalClass;
const missing = globalThis.absent;
";
    let case = TestCase::parse("probe/global-this", "global-this.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for wanted in ["visible : number", "hidden : any", "hiddenClass : any", "missing : any"] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
