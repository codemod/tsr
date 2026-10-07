//! Native-backed full-population and exact-comparison controls.
use tsr_conformance::full_oracle::{self, Artifacts, Verdict};

#[test]
fn full_oracle_rejects_consumer_visible_diagnostic_and_type_changes() {
    let expected = Artifacts {
        diagnostics: "a.ts,0,2,0,2,2322,error,chain(string,number)\n".into(),
        errors: b"a.ts(1,1): error TS2322: Type 'string' is not assignable to type 'number'.\r\n".to_vec(),
        types: b"=== a.ts ===\r\n>x : string\r\n>y : number\r\n".to_vec(),
    };
    assert_eq!(full_oracle::compare(&expected, &expected), Verdict::Exact);
    for replacement in [
        "a.ts,0,1,0,1,2322,error,chain(string,number)\n",
        "a.ts,0,2,0,2,2322,error,chain(number,string)\n",
        "a.ts,0,2,0,2,2322,error,chain(string,number)\na.ts,3,1,3,1,2322,error,extra\n",
    ] {
        let mut actual = expected.clone();
        actual.diagnostics = replacement.into();
        assert!(matches!(full_oracle::compare(&expected, &actual), Verdict::Different { diagnostics: true, .. }));
    }
    let mut actual = expected.clone();
    actual.types = b"=== a.ts ===\r\n>y : number\r\n>x : string\r\n".to_vec();
    assert!(matches!(full_oracle::compare(&expected, &actual), Verdict::Different { types: true, .. }));
    let empty = Artifacts { diagnostics: String::new(), errors: b"<no content>".to_vec(), types: b"<no content>".to_vec() };
    assert!(matches!(full_oracle::compare(&empty, &actual), Verdict::Different { .. }));
}

#[test]
fn full_oracle_native_cartesian_clean_and_span_controls() {
    let dir = std::env::temp_dir().join(format!("full-oracle-controls-{}", std::process::id()));
    let population = full_oracle::native_population(
        &tsr_conformance::repo_root().join("vendor/typescript-go"), &dir,
        Some("^TestFullOracle$/^(local|submodule)$/^compiler$/^(settingsSimpleTest|2dArrays|commaOperator1)\\.ts$"),
    ).expect("pinned native oracle");
    assert!(population.discovery_failures.is_empty(), "{:?}", population.discovery_failures);
    let configs: Vec<_> = population.configurations.values().filter(|r| r.configuration.source.ends_with("settingsSimpleTest.ts")).collect();
    assert_eq!(configs.len(), 2);
    assert!(configs.iter().any(|r| r.configuration.name == "strict=true"));
    assert!(configs.iter().any(|r| r.configuration.name == "strict=false"));
    let clean = population.configurations.values().find(|r| r.configuration.source.ends_with("2dArrays.ts")).unwrap();
    let output = clean.output.as_ref().unwrap();
    assert_eq!(output.errors, b"<no content>");
    let reference = std::fs::read(tsr_conformance::repo_root().join(
        "vendor/typescript-go/testdata/baselines/reference/submodule/compiler/2dArrays.types",
    )).unwrap();
    assert_eq!(output.types, reference);
    let comma = population.configurations.values().find(|r| r.configuration.source.ends_with("commaOperator1.ts")).unwrap();
    let output = comma.output.as_ref().unwrap();
    assert!(output.errors.windows(6).any(|window| window == b"TS2695"));
    // Actual native diagnostics at the same start remain separate by length.
    let records: Vec<_> = output.diagnostics.lines().map(|line| line.split(',').collect::<Vec<_>>()).collect();
    assert!(records.iter().enumerate().any(|(i,a)| records[i+1..].iter().any(|b| a[0] == b[0] && a[1] == b[1] && a[5] == b[5] && a[2] != b[2])));
    std::fs::remove_dir_all(dir).unwrap();
}
