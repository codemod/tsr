//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/contextual_initializers", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &expected, false)
        .iter()
        .flatten()
        .map(types_producer::Assertion::line)
        .collect();
    for line in wanted {
        assert!(lines.iter().any(|actual| actual == line), "missing {line}: {lines:?}");
    }
}

const SOURCE: &str = r#"// @strict: true
// @target: esnext
declare function capture<T>(cb:(x:1|2)=>T):T;
declare function bottom<T>(cb:(x:never)=>T):T;
declare function top<T>(cb:(x:unknown)=>T):T;
declare function optional<T>(cb:(x?:1|2)=>T):T;
export const widened=capture((wideParam=3)=>wideParam);
export const compatible=capture((compatibleParam=1)=>compatibleParam);
export const incompatible=capture((incompatibleParam="bad")=>incompatibleParam);
export const explicit=capture((explicitParam:number=3)=>explicitParam);
export const impossible=bottom((bottomParam=3)=>bottomParam);
export const unknownContext=top((unknownParam=3)=>unknownParam);
export const optionalDefault=optional((optionalParam=3)=>optionalParam);
export const optionalCompatible=optional((optionalCompatibleParam=1)=>optionalCompatibleParam);
function requiredAfter(this:void, x:1|2=1, y:number):1|2 { return x; }
export const replay:typeof requiredAfter=function(beforeRequired=3,last){ return beforeRequired; };
"#;

#[test]
fn contextual_initializers_strict() {
    expect(
        SOURCE,
        &[
            "widened : number",
            "compatible : 1 | 2",
            "incompatible : 1 | 2",
            "explicit : number",
            "impossible : number",
            "unknownContext : unknown",
            "optionalDefault : 1 | 2",
            "optionalCompatible : 1 | 2",
            "beforeRequired : 1 | 2",
        ],
    );
}

#[test]
fn contextual_initializers_without_strict_null_checks() {
    expect(
        &SOURCE.replace("@strict: true", "@strict: false"),
        &[
            "widened : number",
            "compatible : 1 | 2",
            "incompatible : 1 | 2",
            "explicit : number",
            "impossible : number",
            "unknownContext : unknown",
            "optionalDefault : number",
            "optionalCompatible : 1 | 2",
            "beforeRequired : number",
        ],
    );
}

/// Equal separately declared inputs contextualize a union callback; merely
/// mutually assignable inputs do not. Binding defaults project a contextual
/// parameter without typing genuine uncontextual parameters.
#[test]
fn callback_inputs_and_binding_defaults_preserve_context_and_real_errors() {
    let source = r#"// @strict: true
type Equal = ((value: { x: number }) => void) | ((other: { x: number }) => number);
const equal: Equal = value => { const bad: string = value.x; };
type Different = ((value: { x: number }) => void) | ((other: { x: number; extra?: boolean }) => number);
const different: Different = value => 0;
const nested: (arg: { fn?: (value: "right") => void }) => void = ({ fn = value => { const bad: "left" = value; } }) => {};
const array: (arg: [(value: "right") => void]) => void = ([fn = value => { const bad: "left" = value; }]) => {};
const untyped = value => value;
const binding = ({ missing }) => missing;
"#;
    let case = TestCase::parse("probe/callback-binding-context", "probe.ts", source);
    let actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .iter()
        .map(|d| (d.line, d.column, d.code))
        .collect();
    assert_eq!(
        actual,
        [(2, 39, 2322), (4, 30, 7006), (5, 91, 2322), (6, 82, 2322), (7, 17, 7006), (8, 20, 7031),]
    );
}

/// The skipped callback's completed empty member table must reject a
/// non-callable configuration overload in the subtype pass, before the callback
/// is context-checked. Wrong configuration and callback bodies stay errors.
#[test]
fn configuration_overloads_contextualize_destructured_callbacks() {
    let source = r"// @strict: true
interface Config { root?: string }
interface Env { mode: string; command: string }
type ConfigFn = (env: Env) => Config | Promise<Config>;
type ConfigExport = Config | Promise<Config> | ConfigFn;
declare function defineConfig(config: Config): Config;
declare function defineConfig(config: Promise<Config>): Promise<Config>;
declare function defineConfig(config: ConfigFn): ConfigFn;
declare function defineConfig(config: ConfigExport): ConfigExport;
defineConfig(({ mode }) => ({ root: mode }));
defineConfig(({ mode }) => { const bad: number = mode; return { root: mode }; });
defineConfig({ root: 123 });
const untyped = value => value;
const binding = ({ missing }) => missing;
";
    let case = TestCase::parse("probe/config-callback-context", "probe.ts", source);
    let actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .iter()
        .map(|d| (d.line, d.column, d.code))
        .collect();
    assert_eq!(actual, [(10, 36, 2322), (11, 16, 2769), (12, 17, 7006), (13, 20, 7031)]);
}

/// Syntax indices include `this`; signature indices exclude it, including
/// trailing defaults and rest positions. The typed receiver also exercises
/// this contract independently of the unannotated-this diagnostic lane.
#[test]
fn leading_this_does_not_erase_or_shift_ordinary_callback_parameters() {
    let source = r#"// @strict: true
const typed: (this: { tag: string }, first: number, second: string) => void = function(this: { tag: string }, first, second) {
  const badFirst: string = first;
  const badSecond: number = second;
  const goodReceiver: string = this.tag;
};
const defaulted: (this: { tag: string }, first: number, second?: string) => void = function(this: { tag: string }, first, second = "ok") {
  const badFirst: string = first;
  const badSecond: number = second;
};
const rest: (this: { tag: string }, first: number, ...rest: string[]) => void = function(this: { tag: string }, first, ...rest) {
  const badFirst: string = first;
  const badRest: number[] = rest;
};
const missing = function(this: { tag: string }, first, second) {};
"#;
    let case = TestCase::parse("probe/this-callback-parameters", "probe.ts", source);
    let actual: Vec<_> = tsr_conformance::diagnostics_suite::reported_for(&case)
        .iter()
        .map(|d| (d.line, d.column, d.code))
        .collect();
    assert_eq!(
        actual,
        [
            (2, 9, 2322),
            (3, 9, 2322),
            (7, 9, 2322),
            (8, 9, 2322),
            (11, 9, 2322),
            (12, 9, 2322),
            (14, 49, 7006),
            (14, 56, 7006),
        ]
    );
}
