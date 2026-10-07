//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/contextual_initializers", "probe.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let lines: Vec<_> = types_producer::assertions_for_case(&case, &case.files.as_slice(), false)
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
