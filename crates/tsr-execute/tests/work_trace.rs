//! Actual CLI work observations, independent of diagnostic output.

use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use serde_json::Value;
use tsr_execute::baseline::{Baseline, BaselineSystem};
use tsr_execute::work_trace::{TraceIdentity, WorkTrace};
use tsr_execute::{ExitStatus, System};
use tsr_vfs::FileSystem;

#[derive(Clone, Default)]
struct Capture(Arc<Mutex<Vec<u8>>>);

impl Write for Capture {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap().extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

struct Host {
    baseline: BaselineSystem,
    trace: Option<Arc<WorkTrace>>,
    warnings: Vec<String>,
}

impl System for Host {
    fn write(&mut self, text: &str) {
        self.baseline.write(text);
    }
    fn fs(&self) -> &dyn FileSystem {
        self.baseline.fs()
    }
    fn default_library_path(&self) -> &str {
        self.baseline.default_library_path()
    }
    fn current_directory(&self) -> &str {
        self.baseline.current_directory()
    }
    fn write_output_is_tty(&self) -> bool {
        false
    }
    fn width_of_terminal(&self) -> usize {
        80
    }
    fn environment_variable(&self, name: &str) -> String {
        self.baseline.environment_variable(name)
    }
    fn since_start(&self) -> Duration {
        Duration::ZERO
    }
    fn work_trace(&self) -> Option<Arc<WorkTrace>> {
        self.trace.clone()
    }
    fn work_trace_warning(&mut self, message: &str) {
        self.warnings.push(message.into());
    }
}

fn fixture() -> Baseline {
    let config = r#"{"compilerOptions":{"target":"es2022","module":"esnext","moduleResolution":"bundler","strict":true,"skipLibCheck":true,"noLib":true,"resolveJsonModule":true,"esModuleInterop":true,"noEmit":true},"files":["index.ts"]}"#;
    Baseline {
        current_directory: "/project".into(),
        use_case_sensitive_file_names: true,
        files: [
            ("tsconfig.json", config),
            ("index.ts", "import data from './data.json'; import type { A } from './a'; import type { Decl } from './types'; const wrong: string = data.value; export const use: A<Decl> | null = null;"),
            ("a.ts", "import type { B } from './b'; export interface A<T> { other?: B<T>; }"),
            ("b.ts", "import type { A } from './a'; export interface B<T> { other?: A<T>; }"),
            ("types.d.ts", "export interface Decl { value: string; }"),
            ("data.json", "{\"value\":17}"),
        ].into_iter().map(|(name, text)| (format!("/project/{name}"), text.into())).collect(),
        ..Baseline::default()
    }
}

fn run(flags: &[&str], enabled: bool) -> (ExitStatus, String, Vec<Value>) {
    run_fixture(flags, enabled, fixture())
}

fn run_fixture(
    flags: &[&str],
    enabled: bool,
    fixture: Baseline,
) -> (ExitStatus, String, Vec<Value>) {
    let capture = Capture::default();
    let trace = enabled.then(|| {
        Arc::new(WorkTrace::new(
            Box::new(capture.clone()),
            TraceIdentity {
                pid: 7,
                invocation_id: "public-control".into(),
                source_sha_claim: None,
                binary_sha256_claim: None,
            },
        ))
    });
    let mut host = Host { baseline: BaselineSystem::new(&fixture), trace, warnings: Vec::new() };
    let args: Vec<_> = ["--project", "/project/tsconfig.json", "--pretty", "false", "--listFiles"]
        .into_iter()
        .chain(flags.iter().copied())
        .map(str::to_owned)
        .collect();
    let status = tsr_execute::command_line(&mut host, &args);
    assert!(host.warnings.is_empty());
    let bytes = capture.0.lock().unwrap();
    let records = String::from_utf8_lossy(&bytes)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    (status, host.baseline.output().into(), records)
}

fn file_id(records: &[Value], suffix: &str) -> u64 {
    records
        .iter()
        .find(|row| {
            row["event"] == "program_file" && row["path"].as_str().unwrap().ends_with(suffix)
        })
        .unwrap()["file_id"]
        .as_u64()
        .unwrap()
}

fn without_timing(records: &[Value]) -> Vec<Value> {
    records
        .iter()
        .cloned()
        .map(|mut row| {
            let object = row.as_object_mut().unwrap();
            for key in
                ["recorded_at_ns", "construction_started_at_ns", "construction_finished_at_ns"]
            {
                object.remove(key);
            }
            row
        })
        .collect()
}

fn assert_intervals(records: &[Value]) {
    assert_eq!(records[0]["worker_activity_schema_version"], 2);
    assert_eq!(records[0]["activity_clock"], "monotonic_elapsed_ns");
    let mut previous = 0;
    for row in records {
        let now = row["recorded_at_ns"].as_u64().expect("direct monotonic event timestamp");
        assert!(now >= previous);
        previous = now;
    }
    for created in records.iter().filter(|row| row["event"] == "checker_created") {
        let start = created["construction_started_at_ns"].as_u64().unwrap();
        let end = created["construction_finished_at_ns"].as_u64().unwrap();
        assert!(start <= end && end <= created["recorded_at_ns"].as_u64().unwrap());
        for row in records.iter().filter(|row| {
            row["event"] == "work_begin" && row["checker_id"] == created["checker_id"]
        }) {
            assert!(row["recorded_at_ns"].as_u64().unwrap() >= end);
        }
    }
}

#[test]
fn tracing_preserves_requested_pool_and_modulo_file_ownership() {
    for (flags, count) in [
        (&[][..], 4),
        (&["--singleThreaded"][..], 1),
        (&["--checkers", "2"][..], 2),
        (&["--checkers", "4"][..], 4),
        (&["--checkers", "2", "--noCheck"][..], 2),
        (&["--skipLibCheck", "false"][..], 4),
    ] {
        let (off_status, off_output, _) = run(flags, false);
        let (status, output, records) = run(flags, true);
        assert_eq!((status, output), (off_status, off_output), "{flags:?}");
        let (repeat_status, repeat_output, repeat) = run(flags, true);
        let (off_status, off_output, _) = run(flags, false);
        assert_eq!((repeat_status, repeat_output), (off_status, off_output), "{flags:?}");
        // Global chronology/span tokens vary with scheduling; private-owner
        // work and ordered Program inventory must remain the same.
        let work = |rows: &[Value]| {
            let mut entries: Vec<_> = rows
                .iter()
                .filter(|row| row["event"] == "work_begin")
                .map(|row| {
                    serde_json::to_string(&serde_json::json!([
                        row["checker_id"],
                        row["operation"],
                        row["file_ids"],
                        row["unmapped_source_node_ids"]
                    ]))
                    .unwrap()
                })
                .collect();
            entries.sort();
            entries
        };
        assert_eq!(work(&records), work(&repeat), "{flags:?}");
        let inventory = |rows: &[Value]| {
            rows.iter()
                .filter(|row| row["event"] == "program_file")
                .map(|row| {
                    let mut row = row.clone();
                    row.as_object_mut().unwrap().remove("recorded_at_ns");
                    row
                })
                .collect::<Vec<_>>()
        };
        assert_eq!(inventory(&records), inventory(&repeat), "{flags:?}");
        assert_intervals(&records);
        assert_intervals(&repeat);
        assert_eq!(records.last().unwrap()["checker_instances_created"], count, "{flags:?}");
        let created: std::collections::BTreeSet<_> = records
            .iter()
            .filter(|row| row["event"] == "checker_created")
            .map(|row| row["checker_id"].as_u64().unwrap())
            .collect();
        assert_eq!(created, (0..count).collect(), "{flags:?}");
        for begin in records
            .iter()
            .filter(|row| row["event"] == "work_begin" && row["operation"] == "source_file_check")
        {
            assert_eq!(
                begin["checker_id"].as_u64().unwrap(),
                begin["file_ids"][0].as_u64().unwrap() % count
            );
        }
    }
}

#[test]
fn cli_trace_preserves_output_and_distinguishes_lazy_work_from_file_checks() {
    let (off_status, off_output, off_records) = run(&["--singleThreaded"], false);
    let (status, output, records) = run(&["--singleThreaded"], true);
    let (_, repeat_output, repeat) = run(&["--singleThreaded"], true);
    assert!(off_records.is_empty());
    assert_eq!(status, off_status);
    assert_eq!(output, off_output);
    assert_eq!(repeat_output, off_output);
    assert_eq!(without_timing(&records), without_timing(&repeat));
    assert_intervals(&records);
    assert_intervals(&repeat);
    assert!(output.contains("error TS2322:"));
    assert_eq!(records.first().unwrap()["event"], "invocation_start");
    assert_eq!(records.last().unwrap()["event"], "invocation_end");
    assert_eq!(records.last().unwrap()["state"], "complete");
    assert_eq!(records.last().unwrap()["exit_code"], status.code());
    let decl = file_id(&records, "/types.d.ts");
    let json = file_id(&records, "/data.json");
    let full_checks: Vec<_> = records
        .iter()
        .filter(|row| row["event"] == "work_begin" && row["operation"] == "source_file_check")
        .collect();
    assert!(!full_checks.iter().any(|row| row["file_ids"] == serde_json::json!([json])));
    assert_eq!(
        records
            .iter()
            .find(|row| row["event"] == "program_file" && row["file_id"] == json)
            .unwrap()["full_check_exclusion"],
        "json_source"
    );
    assert!(!full_checks.iter().any(|row| row["file_ids"] == serde_json::json!([decl])));
    assert!(records.iter().any(|row| row["event"] == "work_begin"
        && row["operation"] == "variable_type_worker"
        && row["file_ids"] == serde_json::json!([json])));
    assert!(records.iter().any(|row| row["event"] == "work_begin"
        && row["operation"] == "declared_type_query"
        && row["file_ids"].as_array().unwrap().contains(&serde_json::json!(decl))));
    let mut active = Vec::new();
    let mut peak_spans = 0;
    for row in &records {
        if row["event"] == "work_begin" {
            active.push(row["span_id"].as_u64().unwrap());
            peak_spans = peak_spans.max(active.len());
        } else if row["event"] == "work_end" {
            assert_eq!(active.pop(), row["span_id"].as_u64());
            assert_eq!(row["outcome"], "returned");
        }
    }
    assert!(active.is_empty());
    assert!(peak_spans > 1, "the control must exercise nested private-checker work");
    assert_eq!(records.last().unwrap()["all_forcing_observed"], false);
    assert_eq!(records.last().unwrap()["complete_provenance_verified"], false);
    // The fixture has nested queries, but they share one private checker.
    assert_eq!(records.last().unwrap()["peak_covered_semantic_checkers"], 1);
    assert_eq!(records.last().unwrap()["peak_observed_checkers"], 1);
    assert_eq!(records.last().unwrap()["peak_full_checkers"], 1);
}

#[test]
fn eligible_identities_equal_full_workers_started_and_returned() {
    for flags in [&[][..], &["--skipLibCheck", "false"][..], &["--noCheck"][..]] {
        let (_, _, records) = run(flags, true);
        let program = records.iter().find(|row| row["event"] == "program").unwrap();
        assert_eq!(program["default_library_file_count"], 0);
        assert!(program["full_check_options"].is_object());
        for file in records.iter().filter(|row| row["event"] == "program_file") {
            assert!(file["declaration_file"].is_boolean());
            assert!(file["javascript_source"].is_boolean());
            assert!(file["json_source"].is_boolean());
            assert!(file["check_js_directive"].is_null());
        }
        let eligible: std::collections::BTreeSet<_> = records
            .iter()
            .filter(|row| row["event"] == "program_file" && row["full_check_eligible"] == true)
            .map(|row| row["file_id"].as_u64().unwrap())
            .collect();
        let begun: Vec<_> = records
            .iter()
            .filter(|row| row["event"] == "work_begin" && row["operation"] == "source_file_check")
            .collect();
        let observed: std::collections::BTreeSet<_> =
            begun.iter().map(|row| row["file_ids"][0].as_u64().unwrap()).collect();
        assert_eq!(observed, eligible, "{flags:?}");
        assert_eq!(begun.len(), eligible.len(), "no duplicate full workers");
        for begin in begun {
            assert!(records.iter().any(|row| row["event"] == "work_end"
                && row["span_id"] == begin["span_id"]
                && row["outcome"] == "returned"));
        }
    }
}

#[test]
fn no_check_and_worker_requests_do_not_become_performed_work() {
    for flags in [
        &["--noCheck"][..],
        &["--singleThreaded", "--noCheck"][..],
        &["--checkers", "2", "--noCheck"][..],
    ] {
        let (status, output, records) = run(flags, true);
        let (off_status, off_output, _) = run(flags, false);
        assert_eq!((status, output), (off_status, off_output));
        assert!(!records.iter().any(|row| row["event"] == "work_begin"));
        assert!(
            records
                .iter()
                .filter(|row| row["event"] == "program_file")
                .all(|row| row["full_check_exclusion"] == "no_check")
        );
        let count = if flags.contains(&"--singleThreaded") {
            1
        } else if flags.contains(&"--checkers") {
            2
        } else {
            4
        };
        assert_eq!(records.iter().filter(|row| row["event"] == "checker_created").count(), count);
        assert_eq!(records.last().unwrap()["checker_instances_created"], count);
        assert_eq!(records.last().unwrap()["peak_full_checks"], 0);
        assert_intervals(&records);
        let peak = records.last().unwrap()["peak_constructing_checkers"].as_u64().unwrap();
        assert!((1..=count as u64).contains(&peak));
        assert_eq!(records.last().unwrap()["peak_covered_semantic_checkers"], 0);
        assert_eq!(records.last().unwrap()["peak_full_checkers"], 0);
        assert_eq!(records.last().unwrap()["peak_observed_checkers"], peak);
    }
    for request in ["1", "2"] {
        let (_, _, records) = run(&["--checkers", request], true);
        let workers = records.iter().find(|row| row["event"] == "pool_selected").unwrap();
        assert_eq!(workers["requested_checkers"], request.parse::<u64>().unwrap());
        assert_eq!(workers["selected_count"], request.parse::<u64>().unwrap());
        assert_eq!(workers["worker_options_applied_by_driver"], true);
        assert!(workers["memory_admission_budget"].is_null());
    }
}

#[test]
fn enabling_declaration_checks_changes_observed_work_without_changing_output() {
    let (status, output, records) = run(&["--skipLibCheck", "false"], true);
    let (off_status, off_output, _) = run(&["--skipLibCheck", "false"], false);
    assert_eq!((status, output), (off_status, off_output));
    let decl = file_id(&records, "/types.d.ts");
    assert!(records.iter().any(|row| row["event"] == "work_begin"
        && row["operation"] == "source_file_check"
        && row["file_ids"] == serde_json::json!([decl])));
}

#[test]
fn missing_project_finishes_as_no_semantic_program() {
    let (_, _, records) = run(&["--project", "/missing/tsconfig.json"], true);
    assert!(!records.iter().any(|row| row["event"] == "program_file"));
    assert_eq!(records.last().unwrap()["semantic_program_observed"], false);
    assert_ne!(records.last().unwrap()["exit_code"], 0);
}

#[test]
fn list_files_only_loads_a_program_without_creating_a_checker() {
    let (status, output, records) = run(&["--listFilesOnly"], true);
    let (off_status, off_output, _) = run(&["--listFilesOnly"], false);
    assert_eq!((status, output), (off_status, off_output));
    assert!(records.iter().any(|row| row["event"] == "program_file"));
    assert!(!records.iter().any(|row| row["event"] == "work_begin"));
    assert_eq!(records.last().unwrap()["semantic_program_observed"], true);
    assert_eq!(records.last().unwrap()["checker_instances_created"], 0);
    assert_intervals(&records);
    assert_eq!(records.last().unwrap()["peak_observed_checkers"], 0);
    assert_eq!(records.last().unwrap()["peak_constructing_checkers"], 0);
}

#[test]
fn list_only_js_syntax_observes_checker_lifetime_without_semantic_work() {
    let baseline = Baseline {
        current_directory: "/project".into(),
        use_case_sensitive_file_names: true,
        files: vec![
            (
                "/project/tsconfig.json".into(),
                r#"{"compilerOptions":{"allowJs":true},"files":["main.js"]}"#.into(),
            ),
            ("/project/main.js".into(), "const value: number = 1;".into()),
        ],
        ..Baseline::default()
    };
    let (status, output, records) = run_fixture(&["--listFilesOnly"], true, baseline);
    assert_eq!(status, ExitStatus::DiagnosticsPresentOutputsSkipped);
    assert!(output.contains("TS8010:"), "{output}");
    let end = records.last().unwrap();
    assert_eq!(end["checker_instances_created"], 1);
    assert_eq!(end["state"], "complete");
    assert!(
        !records
            .iter()
            .any(|row| row["event"] == "work_begin" && row["operation"] == "source_file_check")
    );
    assert_intervals(&records);
}

/// Fail after the flushed header, while preserving the bytes already written.
struct FailingWriter {
    capture: Capture,
    header_flushed: bool,
    fail_on_final_flush: bool,
}

impl Write for FailingWriter {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if self.header_flushed && !self.fail_on_final_flush {
            Err(io::Error::other("injected sidecar failure"))
        } else {
            self.capture.write(bytes)
        }
    }
    fn flush(&mut self) -> io::Result<()> {
        if self.header_flushed && self.fail_on_final_flush {
            return Err(io::Error::other("injected final flush failure"));
        }
        self.header_flushed = true;
        Ok(())
    }
}

#[test]
fn failed_sidecar_preserves_diagnostics_and_never_claims_completion() {
    let capture = Capture::default();
    let trace = Arc::new(WorkTrace::new(
        Box::new(FailingWriter {
            capture: capture.clone(),
            header_flushed: false,
            fail_on_final_flush: false,
        }),
        TraceIdentity {
            pid: 7,
            invocation_id: "failed-control".into(),
            source_sha_claim: None,
            binary_sha256_claim: None,
        },
    ));
    let mut host = Host {
        baseline: BaselineSystem::new(&fixture()),
        trace: Some(trace),
        warnings: Vec::new(),
    };
    let args = ["--project", "/project/tsconfig.json", "--pretty", "false", "--listFiles"]
        .map(str::to_owned);
    let status = tsr_execute::command_line(&mut host, &args);
    let (off_status, off_output, _) = run(&[], false);
    assert_eq!(status, off_status);
    assert_eq!(host.baseline.output(), off_output);
    assert_eq!(host.warnings, ["injected sidecar failure"]);
    let bytes = capture.0.lock().unwrap();
    let records: Vec<Value> = String::from_utf8_lossy(&bytes)
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(records.len(), 1);
    assert_eq!(records[0]["event"], "invocation_start");
}

#[test]
fn final_flush_failure_warns_even_when_complete_records_are_visible() {
    let capture = Capture::default();
    let trace = Arc::new(WorkTrace::new(
        Box::new(FailingWriter {
            capture: capture.clone(),
            header_flushed: false,
            fail_on_final_flush: true,
        }),
        TraceIdentity {
            pid: 7,
            invocation_id: "flush-control".into(),
            source_sha_claim: None,
            binary_sha256_claim: None,
        },
    ));
    let mut host = Host {
        baseline: BaselineSystem::new(&fixture()),
        trace: Some(trace),
        warnings: Vec::new(),
    };
    let args = ["--project", "/project/tsconfig.json", "--pretty", "false", "--listFiles"]
        .map(str::to_owned);
    let status = tsr_execute::command_line(&mut host, &args);
    let (off_status, off_output, _) = run(&[], false);
    assert_eq!(status, off_status);
    assert_eq!(host.baseline.output(), off_output);
    assert_eq!(host.warnings, ["injected final flush failure"]);
    let bytes = capture.0.lock().unwrap();
    let end: Value =
        serde_json::from_str(String::from_utf8_lossy(&bytes).lines().last().unwrap()).unwrap();
    assert_eq!(end["event"], "invocation_end");
    // A marker is insufficient: a consumer must reject the host's trace warning.
    assert_eq!(end["actual_work_equivalence_verified"], false);
}
