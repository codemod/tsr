//! `cargo xtask gate` — the five checks, run in one command that **fails**.
//!
//! # Why this exists
//!
//! Every build in the `diagnostics` workstream ends with five checks: `fmt`,
//! `clippy -D warnings`, `cargo test`, `xtask anchors`, `xtask issue-ids`.
//! Four of them only *print*. At §922 a commit went out with clippy red — the
//! number was on screen, in the same command block as the commit, and it was
//! read as part of a block that had printed `0` for four hundred builds.
//!
//! `measure` already refuses to run on a red clippy, which is why every
//! coverage number in that session is trustworthy. This is the same idea for
//! the rest: **the check that matters is the one that stops the pipeline.**
//!
//! `docs/architecture/checker-notes-diag2.md` §923.
use std::process::Command;

use anyhow::{Result, bail};

/// Run the five checks in order, stopping at the first failure.
pub fn run(root: &std::path::Path) -> Result<()> {
    let steps: [(&str, &[&str]); 5] = [
        ("fmt", &["fmt", "--all", "--check"]),
        ("clippy", &["clippy", "--workspace", "--all-targets", "--", "-D", "warnings"]),
        ("test", &["test", "--workspace"]),
        ("anchors", &["run", "-q", "-p", "xtask", "--", "anchors"]),
        ("issue-ids", &["run", "-q", "-p", "xtask", "--", "issue-ids"]),
    ];
    for (name, args) in steps {
        eprintln!("[gate] {name}");
        let status = Command::new("cargo").args(args).current_dir(root).status()?;
        if !status.success() {
            bail!(
                "gate `{name}` failed.\n\
                 Four of these five only printed their result until §923, and a commit \
                 went out with clippy red because a number that had read `0` for four \
                 hundred builds stopped being read. See checker-notes-diag2.md §923."
            );
        }
    }
    eprintln!("[gate] all five clean");
    Ok(())
}
