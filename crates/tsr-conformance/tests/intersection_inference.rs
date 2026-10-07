//! Intersection inference controls verified against pinned tsgo declarations.
use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn structured_intersection_candidates_and_reverse_mapping() {
    let source = r#"// @strict: true
// @target: es2020
declare function combine<T, U>(value: { left: T } & { right: U }): [T, U];
export const pair=combine({left:1,right:"x"});
declare function withExtra<T, U>(value: T & { extra: U }): [T,U];
export const extra=withExtra({left:1,extra:"x"});
declare function merge<T>(value: {left:T} & {right:T}):T;
export const merged=merge({left:1,right:2});
// @target: es2015
// @strict: true
// @noEmit: true

type Results<T> = {
  [K in keyof T]: {
    data: T[K];
    onSuccess: (data: T[K]) => void;
  };
};

type Errors<E> = {
  [K in keyof E]: {
    error: E[K];
    onError: (data: E[K]) => void;
  };
};

declare function withKeyedObj<T, E>(
  arg: Results<T> & Errors<E>
): [T, E];

export const res = withKeyedObj({
  a: {
    data: "foo",
    onSuccess: (dataArg) => {
      dataArg;
    },
    error: 404,
    onError: (errorArg) => {
      errorArg;
    },
  },
  b: {
    data: true,
    onSuccess: (dataArg) => {
      dataArg;
    },
    error: 500,
    onError: (errorArg) => {
      errorArg;
    },
  },
});

"#;
    let case = TestCase::parse("probe/intersection-inference", "inference.ts", source);
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
    for wanted in [
        "pair : [number, string]",
        "extra : [{ left: number; extra: string; }, string]",
        "merged : number",
        "dataArg : string",
        "dataArg : boolean",
        "errorArg : number",
        "res : [{ a: string; b: boolean; }, { a: number; b: number; }]",
    ] {
        assert!(lines.iter().any(|line| line == wanted), "missing {wanted}: {lines:?}");
    }
}
