//! Feature-gated observations of one CLI invocation's actual checker entries.
//!
//! Records describe this producer's boundaries, not native work equivalence.
//! Query spans include cache hits; variable-worker spans do not. Source/binary
//! claims need external verification, and unobserved forcing remains explicit.

use std::collections::HashMap;
use std::io::Write;
use std::sync::{Mutex, MutexGuard};
use std::time::Instant;

use serde_json::{Value, json};
use tsr_checker::work_trace::{NodeId, Operation, WorkObserver};
use tsr_core::{CompilerOptions, OrderedMap};
use tsr_tsoptions::value::ConfigValue;

/// Claims supplied by the host, rather than identities verified by the checker.
pub struct TraceIdentity {
    /// The actual process ID for an OS-hosted invocation.
    pub pid: u32,
    /// Unique within the host; a fresh sidecar must be used for each invocation.
    pub invocation_id: String,
    /// Optional build source claim, requiring external tree verification.
    pub source_sha_claim: Option<String>,
    /// Optional binary claim, requiring an external hash of the executed file.
    pub binary_sha256_claim: Option<String>,
}

/// A sink for one serial CLI checker. Its mutex protects output, not type stores.
///
/// The writer must not panic. Failed writes disable further records and surface
/// through `finish`, without changing compiler diagnostics or exit status.
pub struct WorkTrace {
    identity: TraceIdentity,
    state: Mutex<State>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Lifecycle {
    New,
    Running,
    Finished,
}

struct State {
    writer: Box<dyn Write + Send>,
    failure: Option<String>,
    lifecycle: Lifecycle,
    program_observed: bool,
    file_ids: HashMap<u32, usize>,
    next_span: u64,
    active: HashMap<u64, Operation>,
    full_checks: usize,
    peak_full_checks: usize,
    checker_instances: usize,
    started_at: Instant,
    construction_started_at_ns: Option<u64>,
    peak_constructing_checkers: usize,
    peak_covered_semantic_checkers: usize,
    peak_full_checkers: usize,
    peak_observed_checkers: usize,
    saw_unwind: bool,
}

impl State {
    fn fail(&mut self, message: impl Into<String>) {
        if self.failure.is_none() {
            self.failure = Some(message.into());
        }
    }

    fn elapsed_ns(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }

    fn observe_activity(&mut self) {
        // This producer supports one private checker. Nested spans on that
        // instance are activity, not additional workers or CPU utilization.
        let constructing = usize::from(self.construction_started_at_ns.is_some());
        let semantic = usize::from(!self.active.is_empty());
        let full = usize::from(self.full_checks != 0);
        self.peak_constructing_checkers = self.peak_constructing_checkers.max(constructing);
        self.peak_covered_semantic_checkers = self.peak_covered_semantic_checkers.max(semantic);
        self.peak_full_checkers = self.peak_full_checkers.max(full);
        self.peak_observed_checkers = self.peak_observed_checkers.max(constructing.max(semantic));
    }

    fn record(&mut self, mut value: Value) {
        if self.failure.is_some() {
            return;
        }
        value["recorded_at_ns"] = self.elapsed_ns().into();
        let result = serde_json::to_writer(&mut self.writer, &value)
            .map_err(std::io::Error::other)
            .and_then(|()| self.writer.write_all(b"\n"));
        if let Err(error) = result {
            self.fail(error.to_string());
        }
    }

    fn flush(&mut self) {
        if self.failure.is_none() {
            if let Err(error) = self.writer.flush() {
                self.fail(error.to_string());
            }
        }
    }
}

impl WorkTrace {
    /// Construct a single-use collector; call through `command_line` to finish it.
    #[must_use]
    pub fn new(writer: Box<dyn Write + Send>, identity: TraceIdentity) -> Self {
        Self {
            identity,
            state: Mutex::new(State {
                writer,
                failure: None,
                lifecycle: Lifecycle::New,
                program_observed: false,
                file_ids: HashMap::new(),
                next_span: 0,
                active: HashMap::new(),
                full_checks: 0,
                peak_full_checks: 0,
                checker_instances: 0,
                started_at: Instant::now(),
                construction_started_at_ns: None,
                peak_constructing_checkers: 0,
                peak_covered_semantic_checkers: 0,
                peak_full_checkers: 0,
                peak_observed_checkers: 0,
                saw_unwind: false,
            }),
        }
    }

    fn state(&self) -> MutexGuard<'_, State> {
        self.state.lock().unwrap_or_else(|poison| {
            let mut state = poison.into_inner();
            state.fail("observer mutex poisoned");
            state
        })
    }

    pub(crate) fn start(&self, args: &[String]) {
        let mut state = self.state();
        if state.lifecycle != Lifecycle::New {
            state.fail("collector reused across invocations");
            return;
        }
        state.lifecycle = Lifecycle::Running;
        state.started_at = Instant::now();
        state.record(json!({
            "event": "invocation_start", "schema_version": 1,
            "worker_activity_schema_version": 1,
            "activity_clock": "monotonic_elapsed_ns",
            "producer": "tsr-work-trace", "pid": self.identity.pid,
            "invocation_id": self.identity.invocation_id, "args": args,
            "source_sha_claim": self.identity.source_sha_claim,
            "binary_sha256_claim": self.identity.binary_sha256_claim,
            "complete_provenance_verified": false,
            "all_forcing_observed": false,
            "observed_operations": ["source_file_check", "symbol_type_query",
                "declared_type_query", "variable_type_worker"],
            "initialization_forcing_observed": false,
        }));
        // Even an aborted buffered run leaves an invocation header, never a
        // successful completion marker inherited from a previous invocation.
        state.flush();
    }

    pub(crate) fn program(
        &self,
        program: &tsr_compiler::Program<'_>,
        options: &CompilerOptions,
        roots: &[String],
        raw: &OrderedMap<ConfigValue>,
        current_directory: &str,
        case_sensitive: bool,
    ) {
        let mut state = self.state();
        if state.lifecycle != Lifecycle::Running || state.program_observed {
            state.fail("program observed outside a single active invocation");
            return;
        }
        state.program_observed = true;
        let option = |value: tsr_core::Tristate| {
            if value.is_unknown() { None } else { Some(value.is_true()) }
        };
        state.record(json!({
            "event": "program", "current_directory": current_directory,
            "case_sensitive": case_sensitive, "root_files": roots,
            "compiler_options_debug": format!("{options:?}"),
            "show_config": crate::show_config::show_config(
                options, roots, raw, current_directory, case_sensitive),
            "complete_input_equivalence_verified": false,
            "native_eligibility_equivalence_verified": false,
            // Immutable facts let a consumer validate the exclusion rule,
            // without inferring library membership from names or directives
            // from source text. Older schema-1 readers ignore these additions.
            "default_library_file_count": program.lib_files().len(),
            "full_check_options": {
                "no_check": option(options.no_check),
                "skip_lib_check": option(options.skip_lib_check),
                "skip_default_lib_check": option(options.skip_default_lib_check),
                "check_js": option(options.check_js),
            },
        }));
        for (file_id, file) in program.source_files().iter().enumerate() {
            let source_node_id = file.source_file().node_id.map(NodeId::as_u32);
            if let Some(node_id) = source_node_id {
                state.file_ids.insert(node_id, file_id);
            }
            let name = file.file_name();
            let exclusion = crate::compile::full_check_exclusion(program, file_id);
            state.record(json!({
                "event": "program_file", "file_id": file_id, "path": name,
                "source_node_id": source_node_id, "text_bytes": file.text().len(),
                "full_check_eligible": exclusion.is_none(),
                "full_check_exclusion": exclusion,
                "declaration_file": tsr_path::is_declaration_file_name(name),
                "javascript_source": tsr_compiler::program_diagnostics::is_source_file_js(name),
                "json_source": tsr_parser::ScriptKind::from_file_name(name) == tsr_parser::ScriptKind::Json,
                "check_js_directive": file.file_references().check_js_directive.map(|directive| directive.enabled),
            }));
        }
    }

    pub(crate) fn checker_construction_started(&self) {
        let mut state = self.state();
        if state.lifecycle != Lifecycle::Running
            || !state.program_observed
            || state.checker_instances != 0
            || state.construction_started_at_ns.is_some()
        {
            state.fail("unsupported checker construction in serial CLI producer");
            return;
        }
        state.construction_started_at_ns = Some(state.elapsed_ns());
        state.observe_activity();
    }

    pub(crate) fn checker_created(&self, options: &CompilerOptions) {
        let mut state = self.state();
        if state.lifecycle != Lifecycle::Running
            || !state.program_observed
            || state.checker_instances != 0
        {
            state.fail("unsupported checker lifetime in serial CLI producer");
            return;
        }
        let Some(construction_started_at_ns) = state.construction_started_at_ns.take() else {
            state.fail("checker created without observed construction");
            return;
        };
        let construction_finished_at_ns = state.elapsed_ns();
        state.checker_instances += 1;
        state.record(json!({
            "event": "checker_created", "checker_id": 0,
            "construction_started_at_ns": construction_started_at_ns,
            "construction_finished_at_ns": construction_finished_at_ns,
            "requested_checkers": options.checkers,
            "requested_single_threaded": if options.single_threaded.is_unknown() {
                None
            } else { Some(options.single_threaded.is_true()) },
            "effective_serial_checker_limit": 1,
            "requested_checkers_matches_actual_instances": options.checkers.map(|count| count == 1),
            "worker_options_applied_by_driver": false,
            "memory_admission_budget": null,
            "initialization_forcing_observed": false,
        }));
    }

    /// Finish normal control flow. This does not certify complete semantic work.
    pub(crate) fn finish(&self, exit_code: i32) -> Option<String> {
        let mut state = self.state();
        if state.lifecycle != Lifecycle::Running {
            state.fail("collector finished outside a single active invocation");
        }
        state.lifecycle = Lifecycle::Finished;
        let record = json!({
            "event": "invocation_end", "exit_code": exit_code,
            "state": if state.active.is_empty() && state.construction_started_at_ns.is_none() && !state.saw_unwind {
                "complete"
            } else { "incomplete" },
            "semantic_program_observed": state.program_observed,
            "checker_instances_created": state.checker_instances,
            "peak_full_checks": state.peak_full_checks,
            "peak_constructing_checkers": state.peak_constructing_checkers,
            "peak_covered_semantic_checkers": state.peak_covered_semantic_checkers,
            "peak_full_checkers": state.peak_full_checkers,
            "peak_observed_checkers": state.peak_observed_checkers,
            "unfinished_constructions": usize::from(state.construction_started_at_ns.is_some()),
            "construction_started_at_ns": state.construction_started_at_ns,
            "unfinished_spans": state.active.len(),
            "all_forcing_observed": false,
            "complete_provenance_verified": false,
            "actual_work_equivalence_verified": false,
            "memory_admission_budget": null,
        });
        state.record(record);
        state.flush();
        state.failure.clone()
    }
}

impl WorkObserver for WorkTrace {
    fn begin(&self, operation: Operation, declaration_files: &[NodeId]) -> u64 {
        let mut state = self.state();
        if state.lifecycle != Lifecycle::Running || state.checker_instances != 1 {
            state.fail("checker entry outside its observed lifetime");
        }
        let token = state.next_span;
        state.next_span += 1;
        let mut files = Vec::new();
        let mut unmapped = Vec::new();
        for file in declaration_files {
            if let Some(&id) = state.file_ids.get(&file.as_u32()) {
                files.push(id);
            } else {
                unmapped.push(file.as_u32());
            }
        }
        state.active.insert(token, operation);
        if operation == Operation::SourceFileCheck {
            state.full_checks += 1;
            state.peak_full_checks = state.peak_full_checks.max(state.full_checks);
        }
        state.observe_activity();
        state.record(json!({
            "event": "work_begin", "span_id": token, "operation": operation.name(),
            "checker_id": 0, "file_ids": files, "unmapped_source_node_ids": unmapped,
        }));
        token
    }

    fn end(&self, token: u64, panicking: bool) {
        let mut state = self.state();
        let Some(operation) = state.active.remove(&token) else {
            state.fail("unmatched work completion");
            return;
        };
        if operation == Operation::SourceFileCheck {
            state.full_checks -= 1;
        }
        state.saw_unwind |= panicking;
        state.record(json!({
            "event": "work_end", "span_id": token,
            "outcome": if panicking { "panicking" } else { "returned" },
        }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[derive(Clone, Default)]
    struct Capture(Arc<Mutex<Vec<u8>>>);

    impl Write for Capture {
        fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
            self.0.lock().unwrap().extend_from_slice(bytes);
            Ok(bytes.len())
        }
        fn flush(&mut self) -> std::io::Result<()> {
            Ok(())
        }
    }

    fn construction() -> (WorkTrace, Capture) {
        let capture = Capture::default();
        let trace = WorkTrace::new(
            Box::new(capture.clone()),
            TraceIdentity {
                pid: 7,
                invocation_id: "collector-failure-control".into(),
                source_sha_claim: None,
                binary_sha256_claim: None,
            },
        );
        trace.start(&[]);
        // Collector failure tests isolate lifecycle bookkeeping. Actual Program
        // and constructor wiring is exercised through command_line integration.
        trace.state().program_observed = true;
        trace.checker_construction_started();
        (trace, capture)
    }

    fn last(capture: &Capture) -> Value {
        let bytes = capture.0.lock().unwrap();
        serde_json::from_slice(
            bytes.split(|&byte| byte == b'\n').rfind(|line| !line.is_empty()).unwrap(),
        )
        .unwrap()
    }

    #[test]
    fn unfinished_construction_cannot_finish_as_complete() {
        let (trace, capture) = construction();
        assert!(trace.finish(1).is_none());
        let end = last(&capture);
        assert_eq!(end["state"], "incomplete");
        assert_eq!(end["unfinished_constructions"], 1);
        assert_eq!(end["checker_instances_created"], 0);
        assert_eq!(end["peak_observed_checkers"], 1);
        assert_eq!(end["peak_covered_semantic_checkers"], 0);
        assert!(
            end["construction_started_at_ns"].as_u64().unwrap()
                <= end["recorded_at_ns"].as_u64().unwrap()
        );
    }

    #[test]
    fn nested_unwinding_work_is_one_checker_and_incomplete() {
        let (trace, capture) = construction();
        trace.checker_created(&CompilerOptions::default());
        let file = trace.begin(Operation::SourceFileCheck, &[]);
        let query = trace.begin(Operation::SymbolTypeQuery, &[]);
        trace.end(query, true);
        trace.end(file, true);
        assert!(trace.finish(1).is_none());
        let end = last(&capture);
        assert_eq!(end["state"], "incomplete");
        assert_eq!(end["unfinished_spans"], 0);
        assert_eq!(end["peak_full_checkers"], 1);
        assert_eq!(end["peak_covered_semantic_checkers"], 1);
        assert_eq!(end["peak_observed_checkers"], 1);
    }
}
