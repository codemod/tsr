//! Mapped tuple inference with the bundled libraries used by the corpus.

use tsr_conformance::{TestCase, types_baseline::FileTypes, types_producer};

#[test]
fn reflect_apply_rechecks_an_empty_array_under_the_inferred_tuple_context() {
    let source = "// @target: es2015\n
        const noOp = () => {};
        const observed = Reflect.apply(noOp, this, []);";
    let case = TestCase::parse("probe/mapped-tuple", "mapped-tuple.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    // The pinned doYouNeedToChangeYourTargetLibraryES2015 baseline records
    // both the return and the array's contextual tuple type.
    assert!(lines.iter().any(|line| line == "Reflect.apply(noOp, this, []) : void"), "{lines:?}");
    assert!(lines.iter().any(|line| line == "[] : []"), "{lines:?}");
}

#[test]
fn generic_tuple_indexes_distinguish_guaranteed_positions_from_deferred_reads() {
    let source = "// @strict: true\n// @target: es2015\n
        function read<T extends unknown[]>(tuple: [string, ...T, number], index: number) {
            const prefix = tuple[0];
            const guaranteed = tuple[1];
            const deferred = tuple[2];
            const numeric = tuple[index];
            const spread = [...tuple];
        }";
    let case = TestCase::parse("probe/generic-tuple-index", "generic-tuple-index.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for expected in [
        "tuple[0] : string",
        "tuple[1] : number | T[number]",
        "tuple[2] : [string, ...T, number][2]",
        "tuple[index] : [string, ...T, number][number]",
        "...tuple : string | number | T[number]",
    ] {
        assert!(lines.iter().any(|line| line == expected), "missing {expected}: {lines:?}");
    }
}

#[test]
fn iteration_reads_the_resolved_array_constraint_of_a_type_parameter() {
    let source = "// @strict: true\n// @target: es2015\n
        function copy<T extends unknown[], U extends T>(values: U) {
            const copied = [...values];
        }
        function read<T extends readonly string[]>(values: T) {
            for (const item of values) {}
        }";
    let case = TestCase::parse("probe/array-constraint-iteration", "array-constraint.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for expected in ["...values : unknown", "item : string"] {
        assert!(lines.iter().any(|line| line == expected), "missing {expected}: {lines:?}");
    }
}

#[test]
fn tuple_destructuring_slices_flags_and_labels_and_removes_readonly() {
    let source = "// @strict: true\n// @target: es2015\n
        function slices<T extends unknown[]>(values: readonly [string, ...T, number]) {
            const [...whole] = values;
            const [head, ...tail] = values;
            const [first, second, ...rest] = values;
        }
        function optional(values: readonly [a: number, b?: string]) {
            const [first, ...optionalTail] = values;
        }";
    let case = TestCase::parse("probe/tuple-destructuring-slices", "tuple-slices.ts", source);
    let expected: Vec<_> = case
        .files
        .iter()
        .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
        .collect();
    let assertions = types_producer::assertions_for_case(&case, &expected, false);
    let lines: Vec<_> = assertions.iter().flatten().map(types_producer::Assertion::line).collect();
    for expected in [
        "whole : [string, ...T, number]",
        "tail : [...T, number]",
        "second : number | T[number]",
        "rest : (number | T[number])[]",
        "optionalTail : [b?: string | undefined]",
    ] {
        assert!(lines.iter().any(|line| line == expected), "missing {expected}: {lines:?}");
    }
}

#[test]
fn tuple_slice_optional_arguments_follow_null_and_exact_optional_options() {
    for (options, wanted) in [
        ("// @strict: true\n", "[b?: string | undefined]"),
        ("// @strict: true\n// @exactOptionalPropertyTypes: true\n", "[b?: string]"),
        ("// @strictNullChecks: false\n", "[b?: string]"),
    ] {
        let source = format!(
            "{options}// @target: es2015\nfunction optional(values: readonly [a: number, b?: string]) {{ const [first, ...tail] = values; }}"
        );
        let case =
            TestCase::parse("probe/tuple-slice-optionality", "tuple-slice-optionality.ts", &source);
        let expected: Vec<_> = case
            .files
            .iter()
            .map(|unit| FileTypes { file: unit.name.clone(), assertions: Vec::new() })
            .collect();
        let assertions = types_producer::assertions_for_case(&case, &expected, false);
        let lines: Vec<_> =
            assertions.iter().flatten().map(types_producer::Assertion::line).collect();
        let wanted = format!("tail : {wanted}");
        assert!(lines.iter().any(|line| line == &wanted), "{options}: missing {wanted}: {lines:?}");
    }
}
