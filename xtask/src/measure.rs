//! `cargo xtask measure` — run the conformance suites, but **only** if
//! `clippy` is clean first.
//!
//! # Why this exists
//!
//! `docs/architecture/checker-notes-diag2.md` §140, §231, §244 and §259 are one
//! defect, four times: a new rule adds a `match` arm for a node kind that
//! already has one further down, Rust takes the first and silently deletes the
//! second, and the coverage number that comes back is measuring a compiler with
//! a rule switched off. The first occurrence read as a **590-case collapse**
//! and was nearly reported as a failed hypothesis.
//!
//! `clippy` catches it every time — `unreachable pattern` — and the correction
//! written after each occurrence was *"run clippy before coverage"*. §259
//! recorded why that kept failing:
//!
//! > Knowing the rule is not the same as having the rule. The correction lived
//! > only in prose, and batching the two commands into one shell invocation is
//! > the ergonomic default; the lint count lands three lines above the number
//! > you are waiting for, and goes unread.
//!
//! So the ordering is a **program** rather than a habit. Coverage does not run
//! unless clippy exits zero, and there is no argument that skips it.
//!
//! # The second thing it prints, and why
//!
//! §283 lost a falsifier to a *silent partial build*: three edits were applied,
//! the third failed its own assertion and left the file untouched, and the run
//! still produced a plausible `+4`. The number moved, so nothing about the
//! measurement said the build was incomplete — the traceback had scrolled past,
//! above the number it was waiting for.
//!
//! > An edit that fails its own assertion is a silent partial build, and the
//! > measurement cannot tell you.
//!
//! It can now: this prints `git diff --stat` immediately before the coverage
//! run, in the same output. A build the author expected to touch three files
//! and sees touch two is stopped by reading one line, not by remembering to
//! check.

use std::process::Command;

use anyhow::{Result, bail};

/// Run `clippy`; if it is clean, run the conformance binary.
pub fn run(root: &std::path::Path) -> Result<()> {
    eprintln!("[measure] clippy --workspace --all-targets -- -D warnings");
    let clippy = Command::new("cargo")
        .args(["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"])
        .current_dir(root)
        .status()?;
    if !clippy.success() {
        bail!(
            "clippy is not clean — refusing to measure.\n\
             A coverage number taken over a failing lint has been wrong before: \
             see checker-notes-diag2.md §231 and §244, where an `unreachable pattern` \
             on a new dispatch arm silently deleted an existing rule and the run \
             reported a 590-case collapse."
        );
    }
    eprintln!("[measure] clippy clean");
    // §283 — what is actually being measured, printed next to the number.
    eprintln!("[measure] working tree:");
    let _ = Command::new("git").args(["diff", "--stat", "HEAD"]).current_dir(root).status();
    eprintln!("[measure] running coverage");
    let coverage = Command::new("cargo")
        .args(["run", "--release", "-q", "-p", "tsr-conformance", "--bin", "coverage"])
        .current_dir(root)
        .status()?;
    if !coverage.success() {
        bail!("coverage run failed");
    }
    Ok(())
}
