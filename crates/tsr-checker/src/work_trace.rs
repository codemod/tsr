//! Opt-in observations of actual checker entries, without querying their types.
//!
//! These hooks are absent from ordinary builds. Query spans include cache hits;
//! worker spans have a narrower boundary. File IDs belong to this Program's node
//! table, not a global identity space or a type-store/cache ownership domain.

use std::sync::Arc;

pub use tsr_ast::NodeId;
use tsr_binder::SymbolId;

use crate::Checker;

/// A precisely named entry point, rather than a generic "checked" counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Operation {
    /// `Checker::check_source_file`, including this port's unused pass.
    SourceFileCheck,
    /// `get_type_of_symbol`; includes dispatches that return cached types.
    SymbolTypeQuery,
    /// `get_declared_type_of_symbol`; includes cache hits.
    DeclaredTypeQuery,
    /// Variable/parameter/property worker after its caller's cache lookup.
    VariableTypeWorker,
}

impl Operation {
    /// Stable spelling in a versioned observation record.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SourceFileCheck => "source_file_check",
            Self::SymbolTypeQuery => "symbol_type_query",
            Self::DeclaredTypeQuery => "declared_type_query",
            Self::VariableTypeWorker => "variable_type_worker",
        }
    }
}

/// A probe sink attached to one checker instance.
///
/// Implementations must not panic or re-enter the checker. They own their span
/// tokens, file-identity mapping and I/O failures. No `TypeId` crosses this seam.
pub trait WorkObserver: Send + Sync {
    /// Observe entry and return a token private to this observer/invocation.
    fn begin(&self, operation: Operation, declaration_files: &[NodeId]) -> u64;
    /// Observe return or unwinding. A panicking span is not completed work.
    fn end(&self, token: u64, panicking: bool);
}

pub(crate) struct WorkSpan {
    observer: Arc<dyn WorkObserver>,
    token: u64,
}

impl WorkSpan {
    fn new(observer: Arc<dyn WorkObserver>, operation: Operation, files: &[NodeId]) -> Self {
        let token = observer.begin(operation, files);
        Self { observer, token }
    }
}

impl Drop for WorkSpan {
    fn drop(&mut self) {
        self.observer.end(self.token, std::thread::panicking());
    }
}

impl Checker<'_, '_> {
    /// Attach a sink to this private checker, after construction and before work.
    /// Initialization forcing before attachment is not observed.
    pub fn set_work_observer(&mut self, observer: Arc<dyn WorkObserver>) {
        self.work_observer = Some(observer);
    }

    pub(crate) fn trace_file_work(&self, file: NodeId) -> Option<WorkSpan> {
        let observer = self.work_observer.as_ref()?;
        Some(WorkSpan::new(Arc::clone(observer), Operation::SourceFileCheck, &[file]))
    }

    pub(crate) fn trace_symbol_work(
        &self,
        operation: Operation,
        symbol: SymbolId,
    ) -> Option<WorkSpan> {
        let observer = self.work_observer.as_ref()?;
        let mut files: Vec<_> = self
            .binder
            .symbols()
            .get(symbol)
            .declarations
            .iter()
            .filter_map(|&node| self.source_file_of_for_diagnostics(node))
            .collect();
        files.sort_unstable();
        files.dedup();
        Some(WorkSpan::new(Arc::clone(observer), operation, &files))
    }
}
