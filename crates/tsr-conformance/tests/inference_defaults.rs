//! Native outcomes from pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

fn expect(source: &str, wanted: &[&str]) {
    let case = TestCase::parse("probe/inference_defaults", "probe.ts", source);
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

#[test]
fn candidate_free_constraints_and_defaults() {
    expect(
        r"// @strict: true
// @target: esnext
declare function plain<T>(): T;
declare function constrained<T extends string>(): T;
declare function backward<A, B extends A>(value: A): B;
declare function forward<A extends B, B extends string>(): [A, B];
declare function defaultBack<A=number, B=A>(): [A,B];
declare function defaultForward<A=B, B=string>(): [A,B];
declare function defaultSelf<A=A>(): A;
declare function defaultInvalid<A extends string=number>(): A;
declare function defaultValid<A extends string='x'>(): A;
declare function selfConstraint<A extends { value: A }>(): A;
declare function earlierConstraint<A extends B, B=string>(): [A,B];
export const noCandidate=plain();
export const closed=constrained();
export const previous=backward(1);
export const later=forward();
export const defaults=defaultBack();
export const forwardDefault=defaultForward();
export const selfDefault=defaultSelf();
export const invalidDefault=defaultInvalid();
export const validDefault=defaultValid();
export const recursiveConstraint=selfConstraint();
export const dependentDefault=earlierConstraint();
",
        &[
            "noCandidate : unknown",
            "closed : string",
            "previous : number",
            "later : [string, string]",
            "defaults : [number, number]",
            "forwardDefault : [unknown, string]",
            "selfDefault : unknown",
            "invalidDefault : string",
            "validDefault : \"x\"",
            "recursiveConstraint : { value: unknown; }",
            "dependentDefault : [string, string]",
        ],
    );
}

#[test]
fn recursive_constraints_and_candidate_backreferences() {
    expect(
        r"// @strict: true
// @target: esnext
declare function chain<A extends B, B extends C, C extends string>(): [A,B,C];
declare function defaults<A=B,B=string>(value:B):[A,B];
declare function source<A,B=A>(value:A):[A,B];
declare function pair<A extends {b:B},B extends {a:A}>():[A,B];
declare function functionConstraint<A extends () => string>():A;
export const deep=chain();
export const futureCandidate=defaults(2);
export const earlierCandidate=source(2);
export const mutual=pair();
export const callable=functionConstraint();
",
        &[
            "deep : [string, string, string]",
            "futureCandidate : [unknown, number]",
            "earlierCandidate : [number, number]",
            "mutual : [{ b: { a: unknown; }; }, { a: unknown; }]",
            "callable : () => string",
        ],
    );
}

#[test]
fn javascript_fallback_and_forward_defaults() {
    expect(
        r"// @strict: true
// @allowJs: true
// @checkJs: true
// @module: commonjs
// @filename: api.d.ts
export declare function plain<T>():T;
export declare function constrained<T extends string>():T;
export declare function recursive<T extends {value:T}>():T;
export declare function defaults<A=B,B=string>():[A,B];
// @filename: index.js
import {plain,constrained,recursive,defaults} from './api';
export const empty = plain();
export const closed = constrained();
export const self = recursive();
export const forward = defaults();
",
        &[
            "empty : any",
            "closed : string",
            "self : { value: any; }",
            "forward : [unknown, string]",
        ],
    );
}

#[test]
fn failed_argument_with_unresolved_conditional_receiver() {
    expect(
        r"// @strict: true
function baz<T extends 1 | 2>(callback: (this: 1, ...args: T extends 1 ? [unknown] : [unknown, unknown]) => void) {
 const good = callback.bind(1);
 const bad = callback.bind(2);
}
",
        &[
            "callback.bind(1) : (...args: T extends 1 ? [unknown] : [unknown, unknown]) => void",
            "callback.bind(2) : (...args: T extends 1 ? [unknown] : [unknown, unknown]) => void",
        ],
    );
}
