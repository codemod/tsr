//! A reused `typeof f<…>` query re-emits its type arguments as written
//! nodes, JSDoc ones through the existing-node visitor's rewrites
//! (`docs/parity/notes/r7-printer.md` §23). Every expected line was read
//! from pinned native tsgo 5b1047d's `expressionWithJSDocTypeArguments`
//! baseline.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn a_jsdoc_type_argument_of_a_reused_type_query_prints_as_written() {
    let case = TestCase::parse(
        "probe/type_query_jsdoc_arguments",
        "a.ts",
        r"// @target: es2015
// @strict: true
function foo<T>(x: T): T { return x }
class Bar<T> { constructor(public x: T) { } }
type TWhatFoo = typeof foo<?>;
type THuhFoo = typeof foo<string?>;
type TComeOnFoo = typeof foo<?string?>;
type TComeOnBar = typeof Bar<?string?>;
",
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
        "TWhatFoo : typeof foo<any | null>",
        "THuhFoo : typeof foo<string | null>",
        "TComeOnFoo : typeof foo<(string | null) | null>",
        "TComeOnBar : typeof Bar<(string | null) | null>",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
