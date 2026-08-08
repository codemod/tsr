//! The `tsr` binary.
//!
//! Ported from `cmd/tsgo/main.go`. Deliberately thin: everything a command line
//! means lives in `tsr-execute`, so the baseline runner can drive the same code
//! this binary does without a process.
//!
//! Subcommand dispatch is a `match` on the first argument rather than a parser
//! crate, which is upstream's shape (`main.go` is 32 lines) and is recorded as a
//! decision in `STATUS-cli.md` §6: a dependency here invites routing compiler
//! options through it later, and compiler options have a parser of their own.
//! Upstream's `cmd/tsgo/main.go` is 32 lines for the same reason.

use tsr_execute::{OsSystem, command_line};

fn main() -> std::process::ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    // `--lsp` and `--api` are upstream's other two entry points. Neither exists
    // here; naming them explicitly means they report as unimplemented rather
    // than as an unknown compiler option, which would send a user looking for a
    // typo.
    if let Some(first) = args.first() {
        if matches!(first.as_str(), "--lsp" | "--api") {
            eprintln!("error TS0: '{first}' is not implemented in this port.");
            return std::process::ExitCode::from(5);
        }
    }

    let mut sys = OsSystem::new();
    let status = command_line(&mut sys, &args);
    // Exit codes are upstream's `ExitStatus` and are part of the interface: a
    // build script distinguishes 1 (errors, nothing written) from 2 (errors,
    // output written anyway).
    std::process::ExitCode::from(u8::try_from(status.code()).unwrap_or(1))
}
