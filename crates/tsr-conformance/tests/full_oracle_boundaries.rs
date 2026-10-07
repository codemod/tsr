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
    let dir = tsr_conformance::repo_root().join("target/recovery/oracle/controls");
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
    let actual = tsr_conformance::full_oracle_actual::produce(&clean.configuration).expect("real TSR producer");
    assert_eq!(full_oracle::compare(clean.output.as_ref().unwrap(), &actual), Verdict::Exact);
    let actual_comma = tsr_conformance::full_oracle_actual::produce(&comma.configuration).expect("real TSR comma producer");
    assert_eq!(actual_comma.errors, output.errors);
    assert_eq!(actual_comma.types, output.types);
    assert!(actual_comma.diagnostics.contains("unavailable:skippedOnNoEmit"));
    assert!(matches!(full_oracle::compare(output, &actual_comma), Verdict::Different { diagnostics: true, .. }));
}

#[test]
fn config_only_options_keep_source_identity_and_complete_artifacts() {
    let dir = tsr_conformance::repo_root().join("target/recovery/oracle/config");
    let population = full_oracle::native_population(
        &tsr_conformance::repo_root().join("vendor/typescript-go"), &dir,
        Some("^TestFullOracle$/^local$/^compiler$/^tsconfigSimpleTest\\.ts$"),
    ).expect("pinned native config control");
    let native = population.configurations.values().next().expect("config control retained");
    let actual = tsr_conformance::full_oracle_actual::produce(&native.configuration).expect("real TSR config producer");
    let expected = native.output.as_ref().expect("native artifact");
    assert_eq!(actual.errors, expected.errors);
    assert_eq!(actual.types, expected.types);
    assert!(actual.diagnostics.contains("unavailable:skippedOnNoEmit"));
    assert!(matches!(full_oracle::compare(expected, &actual), Verdict::Different { diagnostics: true, .. }));
}

#[test]
fn native_disabled_type_output_does_not_hide_semantic_metadata_failure() {
    let dir = tsr_conformance::repo_root().join("target/recovery/oracle/no-types");
    let population = full_oracle::native_population(
        &tsr_conformance::repo_root().join("vendor/typescript-go"), &dir,
        Some(r"^TestFullOracle$/^local$/^compiler$/^tslibImportDefaultHelperCommonJS\.ts$"),
    ).expect("native noTypesAndSymbols control");
    let native = population.configurations.values().next().expect("configured control retained");
    let expected = native.output.as_ref().expect("native artifacts");
    let actual = tsr_conformance::full_oracle_actual::produce(&native.configuration).expect("real TSR producer");
    assert_eq!(expected.types, b"<no content>");
    assert_eq!(actual.types, expected.types);
    assert_eq!(actual.errors, expected.errors);
    assert!(matches!(full_oracle::compare(expected, &actual), Verdict::Different { diagnostics: true, errors: false, types: false }));
}

#[test]
fn duplicate_configuration_and_orphan_results_cannot_replace_manifest_identity() {
    let path = std::env::temp_dir().join(format!("full-oracle-identities-{}.tsv", std::process::id()));
    let record = |fields: &[&str]| fields.iter().map(|field| full_oracle::hex(field)).collect::<Vec<_>>().join("\t") + "\n";
    let configuration = record(&["C", "local/compiler/control.ts(strict=true)", "/control.ts", "strict=true", "strict=74727565\n"]);
    std::fs::write(&path, format!("{configuration}{configuration}")).unwrap();
    assert!(full_oracle::read_population(&path).unwrap_err().to_string().contains("duplicate native configuration"));
    let failure = record(&["R", "local/compiler/control.ts(strict=true)", "panic", "crash", "", "", ""]);
    std::fs::write(&path, &failure).unwrap();
    assert!(full_oracle::read_population(&path).unwrap_err().to_string().contains("native result without configuration"));
    std::fs::write(&path, format!("{configuration}{failure}{failure}")).unwrap();
    assert!(full_oracle::read_population(&path).unwrap_err().to_string().contains("duplicate native result"));
    std::fs::write(&path, &configuration).unwrap();
    let unpublished = full_oracle::read_population(&path).unwrap();
    assert!(unpublished.configurations.values().next().unwrap().output.is_err());
    std::fs::remove_file(path).unwrap();
}

#[test]
fn process_deadline_reaps_hung_compiler_and_retains_output() {
    let path = std::env::temp_dir().join(format!("full-oracle-deadline-{}", std::process::id()));
    let status = full_oracle::run_bounded(std::process::Command::new("sh").args(["-c", "echo started; exec sleep 10"]),
        &path, std::time::Duration::from_millis(30)).unwrap();
    assert!(status.starts_with("timeout after 30 ms; reaped"), "{status}");
    assert_eq!(std::fs::read_to_string(path.with_extension("stdout")).unwrap(), "started\n");
    std::fs::remove_file(path.with_extension("stdout")).unwrap();
    std::fs::remove_file(path.with_extension("stderr")).unwrap();
}
