//! Physical host controls: fresh sidecars, actual process IDs and aborts.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

struct Project(PathBuf);

impl Project {
    fn new() -> Self {
        let nonce = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
        let path =
            std::env::temp_dir().join(format!("tsr-work-trace-{}-{nonce}", std::process::id()));
        fs::create_dir(&path).unwrap();
        fs::write(
            path.join("tsconfig.json"),
            r#"{"compilerOptions":{"noLib":true,"noEmit":true},"files":["index.ts"]}"#,
        )
        .unwrap();
        fs::write(path.join("index.ts"), "const value: string = 42;\n").unwrap();
        Self(path)
    }

    fn command(&self, trace: Option<&Path>) -> Command {
        let mut command = Command::new(env!("CARGO_BIN_EXE_tsr"));
        command
            .current_dir(&self.0)
            .args(["--pretty", "false", "--listFiles"])
            .env_remove("TSR_WORK_TRACE")
            .env_remove("TSR_WORK_TRACE_BINARY_SHA256");
        if let Some(trace) = trace {
            command.env("TSR_WORK_TRACE", trace);
        }
        command
    }
}

impl Drop for Project {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn os_trace_uses_actual_pid_and_refuses_to_overwrite_existing_sidecars() {
    let project = Project::new();
    let off = project.command(None).output().unwrap();
    let trace_path = project.0.join("work.ndjson");
    let child = project
        .command(Some(&trace_path))
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let pid = child.id();
    let on = child.wait_with_output().unwrap();
    assert_eq!(on.status, off.status);
    assert_eq!(on.stdout, off.stdout);
    assert_eq!(on.stderr, off.stderr);
    let bytes = fs::read(&trace_path).unwrap();
    let text = String::from_utf8(bytes.clone()).unwrap();
    let header = text.lines().next().unwrap();
    assert!(header.contains(&format!("\"pid\":{pid}")));
    assert!(header.contains("\"schema_version\":2"));
    assert!(text.lines().last().unwrap().contains("\"state\":\"complete\""));
    assert!(text.lines().last().unwrap().contains("\"actual_work_equivalence_verified\":false"));

    let reused = project.command(Some(&trace_path)).output().unwrap();
    assert_eq!(reused.status, off.status);
    assert_eq!(reused.stdout, off.stdout);
    assert!(String::from_utf8_lossy(&reused.stderr).contains("work trace not started"));
    assert_eq!(fs::read(&trace_path).unwrap(), bytes);
}

#[test]
fn killed_compilation_cannot_publish_a_completed_invocation() {
    let project = Project::new();
    // Keep the compiler busy while the parent observes the flushed header.
    // No wall-time threshold is being used as performance evidence.
    fs::write(
        project.0.join("index.ts"),
        "export interface A { value: string; }\n".repeat(100_000),
    )
    .unwrap();
    let trace_path = project.0.join("killed.ndjson");
    let mut child = project
        .command(Some(&trace_path))
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    let started = Instant::now();
    loop {
        if fs::read_to_string(&trace_path).is_ok_and(|text| text.contains("invocation_start")) {
            break;
        }
        if child.try_wait().unwrap().is_some() || started.elapsed() > Duration::from_secs(5) {
            let _ = child.kill();
            let _ = child.wait();
            panic!("probe did not flush its invocation header before termination");
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    child.kill().unwrap();
    let status = child.wait().unwrap();
    assert!(!status.success());
    let trace = fs::read_to_string(&trace_path).unwrap();
    assert!(trace.contains("invocation_start"));
    assert!(!trace.contains("invocation_end"));
}
