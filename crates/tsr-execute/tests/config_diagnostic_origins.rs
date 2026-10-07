//! Extended configurations retain file identity and do not suppress semantic errors.
use tsr_execute::baseline::{Baseline, BaselineSystem};

#[test]
fn identical_unknown_options_in_distinct_bases_remain_distinct_diagnostics() {
    let baseline = Baseline {
        current_directory: "/project".into(),
        use_case_sensitive_file_names: true,
        files: vec![
            (
                "/project/tsconfig.json".into(),
                r#"{"extends":["./a.json","./b.json"],"files":["main.ts"]}"#.into(),
            ),
            ("/project/a.json".into(), r#"{"compilerOptions":{"frobnicate":true}}"#.into()),
            ("/project/b.json".into(), r#"{"compilerOptions":{"frobnicate":true}}"#.into()),
            ("/project/main.ts".into(), "export const value: number = 'wrong';".into()),
        ],
        ..Baseline::default()
    };
    let mut system = BaselineSystem::new(&baseline);
    let args = ["--project", "tsconfig.json", "--noEmit", "--pretty", "false"].map(str::to_owned);
    assert_eq!(
        tsr_execute::command_line(&mut system, &args),
        tsr_execute::ExitStatus::DiagnosticsPresentOutputsSkipped,
    );
    assert_eq!(
        system.output(),
        "a.json(1,21): error TS5023: Unknown compiler option 'frobnicate'.\nb.json(1,21): error TS5023: Unknown compiler option 'frobnicate'.\nmain.ts(1,14): error TS2322: Type 'string' is not assignable to type 'number'.\n",
    );
}

#[test]
fn list_only_keeps_config_and_syntax_but_skips_semantic_errors() {
    for (source, syntax) in
        [("export const bad: number = 'x';", false), ("export const bad = ;", true)]
    {
        let baseline = Baseline {
            current_directory: "/project".into(),
            use_case_sensitive_file_names: true,
            files: vec![
                (
                    "/project/tsconfig.json".into(),
                    r#"{"compilerOptions":{"frobnicate":true},"files":["main.ts"]}"#.into(),
                ),
                ("/project/main.ts".into(), source.into()),
            ],
            ..Baseline::default()
        };
        let mut system = BaselineSystem::new(&baseline);
        let args =
            ["--project", "tsconfig.json", "--noEmit", "--listFilesOnly", "--pretty", "false"]
                .map(str::to_owned);
        assert_eq!(
            tsr_execute::command_line(&mut system, &args),
            tsr_execute::ExitStatus::DiagnosticsPresentOutputsSkipped
        );
        let output = system.output();
        assert!(output.contains("error TS5023: Unknown compiler option 'frobnicate'."), "{output}");
        assert_eq!(output.contains("error TS1109:"), syntax, "{output}");
        assert!(!output.contains("error TS2322:"), "{output}");
        assert!(
            output.find("error TS5023:").unwrap() < output.rfind("/project/main.ts\n").unwrap(),
            "{output}"
        );
    }
}

#[test]
fn declaration_only_emit_is_unskipped_unless_an_earlier_policy_blocks_it() {
    let baseline = Baseline {
        current_directory: "/project".into(),
        use_case_sensitive_file_names: true,
        files: vec![
            (
                "/project/tsconfig.json".into(),
                r#"{"compilerOptions":{"frobnicate":true},"files":["main.d.ts"]}"#.into(),
            ),
            ("/project/main.d.ts".into(), "export declare const value: number;".into()),
        ],
        ..Baseline::default()
    };
    for (policy, status) in [
        ("--noEmit", tsr_execute::ExitStatus::DiagnosticsPresentOutputsGenerated),
        ("--noEmitOnError", tsr_execute::ExitStatus::DiagnosticsPresentOutputsSkipped),
        ("--listFilesOnly", tsr_execute::ExitStatus::DiagnosticsPresentOutputsSkipped),
    ] {
        let mut system = BaselineSystem::new(&baseline);
        let args = ["--project", "tsconfig.json", policy, "--pretty", "false"].map(str::to_owned);
        assert_eq!(tsr_execute::command_line(&mut system, &args), status);
        assert!(system.output().contains("error TS5023: Unknown compiler option 'frobnicate'."));
    }
}
