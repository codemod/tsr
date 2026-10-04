//! Object mapped values and tuple templates have different native `StripOptional` roads.
use tsr_conformance::{TestCase, diagnostics_suite};

const SOURCE: &str = r#"interface Source { real?: number | undefined; missing?: number; required: number | undefined }
type ValueNeed<T> = { [P in keyof T]-?: T[P] | null };
declare let need: ValueNeed<Source>;
need = { real: undefined, missing: 5, required: 7 };
need = { real: 9, missing: undefined, required: 4 };
need = { real: 9, missing: 6, required: undefined };
need = { real: null, missing: null, required: null };
need = { real: "wrong", missing: 6, required: 7 };
declare const absent: Source;
need = absent;
type TupleNeed<T> = { [P in keyof T]-?: T[P] };
declare let tuple: TupleNeed<[z?: number | undefined]>;
tuple = [undefined];
tuple = [31];
"#;

#[test]
fn mapped_object_values_remove_missing_but_preserve_real_undefined_in_exact_mode() {
    // Pinned tsgo 5b1047d10d32e7d5b446be4de56b126ff42f82bb, strict in both modes.
    // The null template forces synthesized object members rather than the
    // identity-map fallback. A required origin preserves undefined in both
    // modes; an optional origin preserves only real undefined in exact mode.
    // Tuple templates intentionally still strip undefined in exact mode.
    for exact in [false, true] {
        let case = TestCase::parse(
            "probe/mapped-property-optionality",
            "mapped.ts",
            &format!("// @strict: true\n// @exactOptionalPropertyTypes: {exact}\n{SOURCE}"),
        );
        let actual: Vec<_> = diagnostics_suite::reported_for(&case)
            .into_iter()
            .map(|diagnostic| (diagnostic.line, diagnostic.column, diagnostic.code))
            .collect();
        // This unit does not port tuple-element diagnostic elaboration: native
        // locates that error at the element, this reporter at the assignment.
        // Pin rejection without blessing the existing position difference.
        assert_eq!(
            actual.iter().filter(|row| row.0 == 13).map(|row| row.2).collect::<Vec<_>>(),
            [2322],
            "tuple, exact={exact}"
        );
        let actual: Vec<_> = actual.into_iter().filter(|row| row.0 != 13).collect();
        let mut expected = vec![(5, 19, 2322), (8, 10, 2322), (10, 1, 2322)];
        if !exact {
            expected.insert(0, (4, 10, 2322));
        }
        assert_eq!(actual, expected, "exact={exact}");
    }
}
