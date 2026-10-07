//! The CLI's checker pool: several checkers over one program, each owning a
//! fixed share of its files.
//!
//! Ported from `internal/compiler/checkerpool.go` at the pinned commit:
//! `newCheckerPoolWithTracing` (`:40`) sizes the pool, `createCheckers`
//! (`:98`) constructs every checker concurrently and associates program file
//! `i` with checker `i % checkerCount`, and `forEachCheckerGroupDo` (`:148`)
//! runs one task per checker that visits, in program order, only the files
//! associated with it.
//!
//! # Ownership and work boundary
//!
//! - **Key identity and owner.** A file belongs to exactly one checker, chosen
//!   by its index in [`tsr_compiler::Program::source_files`] (native
//!   `program.files`, libraries first). The association is a pure function of
//!   that index and the pool size, so it does not depend on scheduling.
//! - **Publication.** Each checker is private to its worker thread: its type
//!   store, links and caches are never shared or merged. The program, binder
//!   and node tables are shared read-only (`Sync`).
//! - **Diagnostics.** Native `GetSemanticDiagnostics(file)` asks the file's
//!   *associated* checker for that file's diagnostics. A checker that records
//!   a diagnostic against a file owned by another checker while it resolves a
//!   cross-file declaration does not publish it; that file's own checker
//!   reports what it finds. [`check_program_files`] keeps exactly the
//!   `(file, diagnostic)` pairs whose file the producing checker owns.
//! - **Expensive work.** Construction plus `check_source_file` per owned file,
//!   on one thread per checker. Lazily resolved declarations in another
//!   checker's files (globals, imported types) are resolved again by each
//!   checker that needs them, as natively.
//!
//! `singleThreaded` sizes the pool at one checker and runs it on the calling
//! thread, as native `core.NewWorkGroup(true)` does.

use std::time::{Duration, Instant};

use tsr_core::CompilerOptions;
use tsr_diagnostics::Diagnostic;

use crate::compile::full_check_exclusion;

/// Stack for a checker worker thread. The recursive walks grow on demand
/// (`tsr_core::stack`); this matches the main thread's default and the
/// conformance harness workers.
const WORKER_STACK: usize = 8 * 1024 * 1024;

/// Native default pool size (`checkerpool.go:41`).
const DEFAULT_CHECKERS: usize = 4;

/// Native upper bound on the pool size (`checkerpool.go:48`).
const MAX_CHECKERS: usize = 256;

/// The number of checkers a program gets (`newCheckerPoolWithTracing`).
#[must_use]
pub fn checker_count(options: &CompilerOptions, file_count: usize) -> usize {
    let requested = if options.single_threaded.is_true() {
        1
    } else if let Some(count) = options.checkers {
        usize::try_from(count).unwrap_or(0)
    } else {
        DEFAULT_CHECKERS
    };
    requested.min(file_count).clamp(1, MAX_CHECKERS)
}

/// `(program file index, diagnostic)` pairs.
type Located = Vec<(usize, Diagnostic)>;

/// What a pool run produced.
pub struct PoolOutcome {
    /// `(program file index, diagnostic)`, in no particular order.
    pub diagnostics: Vec<(usize, Diagnostic)>,
    /// `SourceFile.JSDiagnostics()` of every JavaScript file, checked or not:
    /// syntactic upstream, produced by this port's checker
    /// (`Checker::js_syntax_diagnostics`).
    pub js_syntax: Vec<(usize, Diagnostic)>,
    /// Files actually passed to `check_source_file`.
    pub checked_files: usize,
    /// Wall time until the last checker finished construction.
    pub construction: Duration,
}

/// Check every eligible file of `program` with `count` checkers.
///
/// The optional observer preserves pool selection and private checker ownership.
/// `checked_files` is the program-wide eligible set every checker
/// is told about before its first check (`Checker::set_checked_files`).
#[must_use]
pub fn check_program_files(
    program: &tsr_compiler::Program<'_>,
    options: &CompilerOptions,
    count: usize,
    #[cfg(feature = "work-trace")] trace: Option<&std::sync::Arc<crate::work_trace::WorkTrace>>,
) -> PoolOutcome {
    let started = Instant::now();
    let files = program.source_files();
    #[cfg(feature = "work-trace")]
    if let Some(trace) = trace {
        trace.pool_selected(options, count, files.len());
    }
    let eligible: Vec<_> = files
        .iter()
        .enumerate()
        .filter(|(index, _)| full_check_exclusion(program, *index).is_none())
        .filter_map(|(_, file)| file.source_file().node_id)
        .collect();
    let file_index: rustc_hash::FxHashMap<_, _> = files
        .iter()
        .enumerate()
        .filter_map(|(index, file)| file.source_file().node_id.map(|id| (id, index)))
        .collect();

    let run = |owner: usize| -> (Located, Located, usize, Duration) {
        #[cfg(feature = "work-trace")]
        if let Some(trace) = trace {
            trace.checker_construction_started(owner);
        }
        let mut checker = tsr_checker::Checker::with_module_host(
            program.binder(),
            program.nodes(),
            program.node_map(),
            Some(program),
        );
        #[cfg(feature = "work-trace")]
        if let Some(trace) = trace {
            trace.checker_created(owner);
        }
        checker.apply_compiler_options(options);
        for file in program.root_and_referenced_files() {
            checker.set_jsdoc(file.jsdoc().iter());
        }
        #[cfg(feature = "work-trace")]
        if let Some(trace) = trace {
            checker.set_work_observer(trace.observer(owner));
        }
        checker.set_checked_files(eligible.iter().copied());
        let constructed = started.elapsed();
        let mut checked_count = 0;
        let mut js_syntax = Vec::new();
        for (index, file) in files.iter().enumerate() {
            if index % count != owner {
                continue;
            }
            if let Some(id) = file.source_file().node_id {
                js_syntax
                    .extend(checker.js_syntax_diagnostics(id).into_iter().map(|(_, d)| (index, d)));
            }
        }
        if !options.no_check.is_true() {
            for (index, file) in files.iter().enumerate() {
                if index % count != owner || full_check_exclusion(program, index).is_some() {
                    continue;
                }
                let Some(id) = file.source_file().node_id else { continue };
                // Upstream's parser sets `NodeFlagsAmbient` on every node of a
                // declaration file; this port's does not, so the bit is
                // supplied here, the same way the conformance harness does.
                let ambient = tsr_path::is_declaration_file_name(file.file_name());
                checker.check_source_file(
                    id,
                    tsr_checker::check::FileContext {
                        ambient,
                        has_parse_errors: !file.diagnostics().is_empty(),
                    },
                );
                checked_count += 1;
            }
        }
        let diagnostics = checker
            .diagnostics()
            .iter()
            .filter_map(|(file_id, diagnostic)| {
                let index = *file_index.get(file_id)?;
                (index % count == owner).then(|| (index, diagnostic.clone()))
            })
            .collect();
        (diagnostics, js_syntax, checked_count, constructed)
    };

    let results: Vec<_> = if count == 1 {
        vec![run(0)]
    } else {
        std::thread::scope(|scope| {
            let workers: Vec<_> = (0..count)
                .map(|owner| {
                    std::thread::Builder::new()
                        .name(format!("checker-{owner}"))
                        .stack_size(WORKER_STACK)
                        .spawn_scoped(scope, move || run(owner))
                        .expect("spawn a checker worker")
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| match worker.join() {
                    Ok(result) => result,
                    Err(panic) => std::panic::resume_unwind(panic),
                })
                .collect()
        })
    };

    let mut outcome = PoolOutcome {
        diagnostics: Vec::new(),
        js_syntax: Vec::new(),
        checked_files: 0,
        construction: Duration::ZERO,
    };
    for (diagnostics, js_syntax, checked_count, constructed) in results {
        outcome.diagnostics.extend(diagnostics);
        outcome.js_syntax.extend(js_syntax);
        outcome.checked_files += checked_count;
        outcome.construction = outcome.construction.max(constructed);
    }
    outcome
}
