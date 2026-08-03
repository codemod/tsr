//! The conformance harness.
//!
//! Runs the TypeScript conformance corpus against whichever compiler stages exist,
//! and writes a committed snapshot per suite so regressions surface as reviewable
//! diffs. See `docs/architecture/conformance.md`.
//!
//! The design constraint that shapes everything here: **a stage that does not
//! exist reports 0%, loudly.** It does not skip and it does not vanish from the
//! summary. A missing row and a zero row look the same in a table, and only one of
//! them tells you where you are.

pub mod case;
pub mod corpus;
pub mod scanner_suite;
pub mod snapshot;
pub mod suite;
pub mod suites;

use std::path::{Path, PathBuf};

pub use case::{TestCase, TestFile};
pub use corpus::{CaseEntry, Corpus};
pub use suite::{Outcome, Suite, SuiteResult, run_suite};

/// The repository root, derived from this crate's manifest directory.
#[must_use]
pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crates/tsr-conformance is two levels below the repo root")
        .to_path_buf()
}

/// Read the pinned upstream commit, so a snapshot records what it measured.
///
/// Falls back to `unknown` rather than failing: a snapshot with an unknown pin is
/// still more useful than no snapshot, and the fallback is visible in the output.
#[must_use]
pub fn upstream_commit(root: &Path) -> String {
    let output = std::process::Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .current_dir(root.join("vendor/typescript-go"))
        .output();

    match output {
        Ok(out) if out.status.success() => String::from_utf8_lossy(&out.stdout).trim().to_string(),
        _ => "unknown".to_string(),
    }
}
