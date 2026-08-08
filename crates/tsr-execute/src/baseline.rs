//! Running upstream's `tsc` baselines, and saying how many pass.
//!
//! Ported from the parts of `internal/execute/tsctests` that a *replay* needs:
//! `newTestSys` (`tsctests/sys.go:117`) for the virtual host, and the baseline
//! file format the runner writes.
//!
//! # Why this exists before most of what it measures
//!
//! `STATUS-cli.md` phase 0 argues the case at length. In short: the CLI's
//! correctness is not a matter of opinion — upstream ships 194 `tsc` baselines
//! that pin exact stdout bytes and an exit status by name — and the project has
//! twice learned that building the instrument first is what moves a stalled
//! number. A suite reading 0/194 honestly is more useful than a driver nobody
//! can grade.
//!
//! # The format
//!
//! ```text
//! currentDirectory::/home/src/workspaces/project
//! useCaseSensitiveFileNames::true
//! Input::
//! //// [/home/src/workspaces/project/a.ts] *new*
//! export const a = 10;
//!
//! tsgo -p . a.ts
//! ExitStatus:: DiagnosticsPresent_OutputsSkipped
//! Output::
//! <the exact bytes, escapes included>
//! ```
//!
//! Anything after the output — emitted files, `.tsbuildinfo` dumps — belongs to
//! the 144 cases that need emit and is not parsed.

use tsr_vfs::InMemoryFileSystem;

use crate::compile::ExitStatus;
use crate::system::System;

/// The environment a scenario runs under, by baseline file stem.
///
/// **Not in the baseline file.** A `tsc` baseline records the current directory,
/// the input files, the argv, the exit status and the output — but the
/// environment lives in the Go *test source*, as a `map[string]string` on the
/// `tscInput` literal keyed by `subScenario` (`tsctests/tsc_test.go:17`). Three
/// baselines depend on it: two set `NO_COLOR`/`FORCE_COLOR`, one sets
/// `TS_TEST_TERMINAL_WIDTH` to select `--help`'s wide layout.
///
/// Parsing Go source is not something this project does elsewhere and is worth
/// justifying. The alternative was to infer the environment from the file
/// *name* — `does-not-add-color-when-NO_COLOR-is-set` does say so — which is a
/// guess dressed as a rule and would silently mis-run any scenario upstream
/// renamed. Reading the declaration is narrow, it fails loudly when the shape
/// changes (the regex stops matching and every scenario gets an empty
/// environment, which those three baselines then fail on), and it keeps the
/// oracle's inputs complete.
#[derive(Debug, Default)]
pub struct ScenarioEnvironments {
    /// Baseline stem (`show-help-…`) to its environment.
    by_scenario: std::collections::HashMap<String, Vec<(String, String)>>,
}

impl ScenarioEnvironments {
    /// Parse `tsc_test.go`'s `tscInput` literals.
    ///
    /// Deliberately not a Go parser: it walks the text looking for
    /// `subScenario: "…"` and, if an `env: map[string]string{` follows before
    /// the next `subScenario`, the `"KEY": "VALUE"` pairs inside it.
    #[must_use]
    pub fn parse(source: &str) -> Self {
        let mut by_scenario = std::collections::HashMap::new();
        let mut current: Option<String> = None;
        let mut in_env = false;
        let mut pairs: Vec<(String, String)> = Vec::new();

        for line in source.lines() {
            let trimmed = line.trim();
            if let Some(rest) = trimmed.strip_prefix("subScenario:") {
                if let Some(name) = current.take() {
                    by_scenario.insert(name, std::mem::take(&mut pairs));
                }
                in_env = false;
                current = quoted(rest).map(|name| name.replace(' ', "-"));
                continue;
            }
            if trimmed.starts_with("env:") {
                in_env = true;
                continue;
            }
            if in_env {
                if trimmed.starts_with('}') {
                    in_env = false;
                    continue;
                }
                let mut parts = trimmed.splitn(2, ':');
                if let (Some(key), Some(value)) = (parts.next(), parts.next())
                    && let (Some(key), Some(value)) = (quoted(key), quoted(value))
                {
                    pairs.push((key, value));
                }
            }
        }
        if let Some(name) = current {
            by_scenario.insert(name, pairs);
        }

        Self { by_scenario }
    }

    /// The environment for a baseline, by its file stem.
    #[must_use]
    pub fn for_scenario(&self, stem: &str) -> &[(String, String)] {
        self.by_scenario.get(stem).map_or(&[], Vec::as_slice)
    }
}

/// The first double-quoted run in `text`.
fn quoted(text: &str) -> Option<String> {
    let start = text.find('"')? + 1;
    let rest = &text[start..];
    let end = rest.find('"')?;
    Some(rest[..end].to_string())
}

/// One parsed baseline.
#[derive(Debug, Default)]
pub struct Baseline {
    /// What the baseline calls itself, for reporting.
    pub name: String,
    /// The directory the command runs in.
    pub current_directory: String,
    /// Whether the virtual filesystem distinguishes case.
    pub use_case_sensitive_file_names: bool,
    /// The files that exist before the command runs.
    pub files: Vec<(String, String)>,
    /// The arguments, with the `tsgo` removed.
    pub args: Vec<String>,
    /// The exit status the baseline expects, by upstream's name.
    pub expected_status: String,
    /// The exact bytes the baseline expects on stdout.
    pub expected_output: String,
    /// Whether the case expects emitted artifacts, and so cannot pass until
    /// there is an emitter.
    pub expects_emit: bool,
    /// The environment, from the Go test source. See [`ScenarioEnvironments`].
    pub environment: Vec<(String, String)>,
}

/// Parse a baseline file.
///
/// Returns `None` for a file that does not look like one at all, which keeps a
/// stray `README` in the directory from being counted as a failure.
#[must_use]
pub fn parse_baseline(name: &str, text: &str) -> Option<Baseline> {
    let mut baseline = Baseline {
        name: name.to_string(),
        use_case_sensitive_file_names: true,
        ..Baseline::default()
    };

    let mut lines = text.lines().peekable();
    baseline.current_directory =
        lines.next()?.strip_prefix("currentDirectory::")?.trim().to_string();
    if let Some(rest) = lines.peek().and_then(|l| l.strip_prefix("useCaseSensitiveFileNames::")) {
        baseline.use_case_sensitive_file_names = rest.trim() == "true";
        lines.next();
    }
    if lines.next()? != "Input::" {
        return None;
    }

    // The input files, until the blank line that precedes the command.
    let mut current: Option<(String, Vec<String>)> = None;
    let mut command_line = None;
    for line in lines.by_ref() {
        if let Some(rest) = line.strip_prefix("//// [") {
            if let Some((path, content)) = current.take() {
                baseline.files.push((path, content.join("\n")));
            }
            let path = rest.split(']').next().unwrap_or_default().to_string();
            current = Some((path, Vec::new()));
            continue;
        }
        if line.starts_with("tsgo") {
            command_line = Some(line.to_string());
            break;
        }
        if line.trim().is_empty() && current.is_some() {
            // A blank line inside a file's content is content; a blank line
            // before the command is a separator. Only the *last* one before
            // `tsgo` separates, so blanks are buffered and trimmed below.
            if let Some((_, content)) = current.as_mut() {
                content.push(String::new());
            }
            continue;
        }
        if let Some((_, content)) = current.as_mut() {
            content.push(line.to_string());
        }
    }
    if let Some((path, mut content)) = current.take() {
        while content.last().is_some_and(String::is_empty) {
            content.pop();
        }
        baseline.files.push((path, content.join("\n")));
    }

    let command_line = command_line?;
    baseline.args =
        command_line.split_whitespace().skip(1).map(std::string::ToString::to_string).collect();

    for line in lines.by_ref() {
        if let Some(rest) = line.strip_prefix("ExitStatus::") {
            baseline.expected_status = rest.trim().to_string();
            continue;
        }
        if line == "Output::" {
            break;
        }
    }

    let mut output = Vec::new();
    for line in lines {
        // An emitted-artifact marker ends the output section and tells us the
        // case is behind the emitter.
        if line.starts_with("//// [") {
            baseline.expects_emit = true;
            break;
        }
        output.push(line.to_string());
    }
    while output.last().is_some_and(String::is_empty) {
        output.pop();
    }
    baseline.expected_output = output.join("\n");

    Some(baseline)
}

/// Upstream's name for an exit status (`ExitStatus` in the baseline header).
#[must_use]
pub const fn status_name(status: ExitStatus) -> &'static str {
    match status {
        ExitStatus::Success => "Success",
        ExitStatus::DiagnosticsPresentOutputsSkipped => "DiagnosticsPresent_OutputsSkipped",
        ExitStatus::DiagnosticsPresentOutputsGenerated => "DiagnosticsPresent_OutputsGenerated",
        ExitStatus::InvalidProjectOutputsSkipped => "InvalidProject_OutputsSkipped",
        ExitStatus::ProjectReferenceCycleOutputsSkipped => "ProjectReferenceCycle_OutputsSkipped",
        ExitStatus::NotImplemented => "NotImplemented",
    }
}

/// The virtual host a baseline runs against (`tsctests.TestSys`).
pub struct BaselineSystem {
    fs: InMemoryFileSystem,
    current_directory: String,
    output: String,
    environment: Vec<(String, String)>,
}

/// Where the harness puts the fake default library (`tscLibPath`).
pub const TSC_LIB_PATH: &str = "/home/src/tslibs/TS/Lib";

impl BaselineSystem {
    /// Build a host from a parsed baseline.
    #[must_use]
    pub fn new(baseline: &Baseline) -> Self {
        Self {
            fs: InMemoryFileSystem::new(
                baseline.files.iter().cloned(),
                [],
                baseline.use_case_sensitive_file_names,
            ),
            current_directory: baseline.current_directory.clone(),
            output: String::new(),
            environment: baseline.environment.clone(),
        }
    }

    /// Everything written so far.
    #[must_use]
    pub fn output(&self) -> &str {
        &self.output
    }
}

impl System for BaselineSystem {
    fn write(&mut self, text: &str) {
        self.output.push_str(text);
    }

    fn fs(&self) -> &dyn tsr_vfs::FileSystem {
        &self.fs
    }

    fn default_library_path(&self) -> &str {
        TSC_LIB_PATH
    }

    fn current_directory(&self) -> &str {
        &self.current_directory
    }

    fn write_output_is_tty(&self) -> bool {
        // **True**, and this is the single most consequential line in the file:
        // upstream's harness captures the *pretty* output, escapes and all, so a
        // replay that answered `false` would differ from every baseline that
        // prints a diagnostic no matter how right the compiler was.
        true
    }

    fn width_of_terminal(&self) -> usize {
        // `tsctests/sys.go:227`: the scenario's `TS_TEST_TERMINAL_WIDTH` if it
        // set one, and **zero** otherwise — which selects `--help`'s narrow
        // layout and is why most help baselines show it.
        self.environment_variable("TS_TEST_TERMINAL_WIDTH").parse().unwrap_or(0)
    }

    fn environment_variable(&self, name: &str) -> String {
        self.environment
            .iter()
            .find(|(key, _)| key == name)
            .map_or_else(String::new, |(_, value)| value.clone())
    }

    fn since_start(&self) -> std::time::Duration {
        // Fixed, so a baseline carrying a duration is reproducible.
        std::time::Duration::ZERO
    }

    fn version(&self) -> &'static str {
        // What upstream's harness pins (`tsctests`), and what every banner in
        // the baselines says.
        "FakeTSVersion"
    }
}

/// What happened when one baseline was replayed.
#[derive(Debug)]
pub enum Verdict {
    /// Output and exit status both matched.
    Passed,
    /// Ran and disagreed.
    Failed {
        /// A short description of the first difference.
        reason: String,
    },
    /// Behind the emitter, so it cannot pass yet and is not a defect.
    NeedsEmit,
}

/// Replay one baseline.
#[must_use]
pub fn run_baseline(baseline: &Baseline) -> Verdict {
    if baseline.expects_emit {
        return Verdict::NeedsEmit;
    }

    let mut sys = BaselineSystem::new(baseline);
    let status = crate::command_line(&mut sys, &baseline.args);

    let actual_status = status_name(status);
    if actual_status != baseline.expected_status {
        return Verdict::Failed {
            reason: format!("exit status {actual_status}, expected {}", baseline.expected_status),
        };
    }

    let actual = sys.output().trim_end_matches('\n');
    let expected = baseline.expected_output.trim_end_matches('\n');
    if actual != expected {
        return Verdict::Failed { reason: first_difference(expected, actual) };
    }

    Verdict::Passed
}

/// The first line on which two outputs differ, rendered readably.
///
/// Escapes are shown as `<ESC>` because a raw one in a report re-colours the
/// terminal reading it, which has cost more than one debugging session.
fn first_difference(expected: &str, actual: &str) -> String {
    let mut expected_lines = expected.lines();
    let mut actual_lines = actual.lines();
    let mut line = 1;
    loop {
        match (expected_lines.next(), actual_lines.next()) {
            (None, None) => return "outputs differ only in trailing whitespace".to_string(),
            (want, got) if want != got => {
                return format!(
                    "line {line}: want {:?}, got {:?}",
                    want.unwrap_or("<end>").replace('\u{1b}', "<ESC>"),
                    got.unwrap_or("<end>").replace('\u{1b}', "<ESC>")
                );
            }
            _ => line += 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "currentDirectory::/home/src/workspaces/project\n\
useCaseSensitiveFileNames::true\n\
Input::\n\
//// [/home/src/workspaces/project/a.ts] *new* \n\
export const a = 10;\n\
\n\
tsgo -p . a.ts\n\
ExitStatus:: DiagnosticsPresent_OutputsSkipped\n\
Output::\n\
some output\n";

    #[test]
    fn a_baseline_parses_into_its_parts() {
        let baseline = parse_baseline("sample", SAMPLE).expect("parses");
        assert_eq!(baseline.current_directory, "/home/src/workspaces/project");
        assert!(baseline.use_case_sensitive_file_names);
        assert_eq!(
            baseline.files,
            [("/home/src/workspaces/project/a.ts".to_string(), "export const a = 10;".to_string())]
        );
        assert_eq!(baseline.args, ["-p", ".", "a.ts"]);
        assert_eq!(baseline.expected_status, "DiagnosticsPresent_OutputsSkipped");
        assert_eq!(baseline.expected_output, "some output");
        assert!(!baseline.expects_emit);
    }

    #[test]
    fn a_baseline_with_emitted_files_is_marked() {
        let text =
            format!("{SAMPLE}//// [/home/src/workspaces/project/a.js] *new* \nvar a = 10;\n");
        let baseline = parse_baseline("sample", &text).expect("parses");
        assert!(baseline.expects_emit);
        assert!(matches!(run_baseline(&baseline), Verdict::NeedsEmit));
    }

    #[test]
    fn something_that_is_not_a_baseline_is_not_one() {
        assert!(parse_baseline("readme", "# notes\n").is_none());
    }

    #[test]
    fn a_replay_reports_an_exit_status_mismatch_before_an_output_one() {
        // Ordering matters for readability: a wrong status usually explains the
        // wrong output, and reporting the output first buries the cause.
        let text = SAMPLE.replace("DiagnosticsPresent_OutputsSkipped", "Success");
        let baseline = parse_baseline("sample", &text).expect("parses");
        match run_baseline(&baseline) {
            Verdict::Failed { reason } => assert!(reason.starts_with("exit status"), "{reason}"),
            other => panic!("expected a failure, got {other:?}"),
        }
    }
}
