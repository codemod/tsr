//! Syntax tuple spreads share tsgo's expansion bound.
use std::fmt::Write;
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn concrete_tuple_spreads_stop_at_the_expansion_limit() {
    let mut source = String::from("// @strict: true\ntype T0=[any];\n");
    for index in 1..14 {
        writeln!(source, "type T{index}=[...T{},...T{}];", index - 1, index - 1).unwrap();
    }
    source.push_str(
        "type Below=[...T13,...T10,...T9,...T8,...T3,...T2,...T1,...T0];\n\
         type Above=[...T13,...T10,...T9,...T8,...T4];\n\
         declare const below:Below;\n\
         declare const above:Above;\n\
         export const belowLength=below.length;\n\
         export const aboveLength=above.length;\n\
         type BuildTuple<L extends number,T extends any[]=[any]>=\
             T['length'] extends L ? T : BuildTuple<L,[...T,...T]>;\n\
         type Impossible=BuildTuple<3>;\n\
         declare const impossible:Impossible;\n\
         export const recursiveLength=impossible.length;\n",
    );
    let case = TestCase::parse("probe/tuple-expansion-limit", "tuple-limit.ts", &source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &case.files.as_slice(), false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    assert!(lines.iter().any(|line| line == "belowLength : 9999"));
    // The oversized type is rejected. tsgo's later error-to-any member recovery
    // is still unported here; keep that gap instead of expanding an invalid tuple.
    assert!(lines.iter().any(|line| line == "aboveLength : error"));
    assert!(lines.iter().any(|line| line == "recursiveLength : error"));
}
