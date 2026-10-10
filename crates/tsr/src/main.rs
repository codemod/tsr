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

/// The binary's allocator: mimalloc v2 rather than the system `malloc`.
/// Allocation was about 11% of domain-model's and 18% of jsTyping's
/// single-threaded profile under glibc, and mimalloc's per-thread heaps
/// also serve the checker pool without arena contention. Only this binary
/// links it. See ADR-0055 for the measurements and the refused alternatives.
#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

/// `mi_option_purge_delay` in mimalloc v2's `mi_option_e`
/// (`c_src/mimalloc/v2/include/mimalloc.h`), which `libmimalloc-sys` 0.1.49
/// does not name.
const MI_OPTION_PURGE_DELAY: libmimalloc_sys::mi_option_t = 15;

/// Never return freed pages to the OS while the command runs. A `tsr`
/// process lives for one compilation and exits (`System::exits_after_command`),
/// so purging after mimalloc's default 10 ms delay only re-faults the same
/// pages. On generic-imports that cost +3–8% CPU over glibc; with purging off
/// it is below glibc (ADR-0055).
fn configure_allocator() {
    // SAFETY: `mi_option_set` takes an option index and a value and has no
    // pointer arguments. It is called once, first thing in `main`, before
    // any thread is spawned, and mimalloc reads options lazily and
    // thread-safely, so no allocation in flight can observe a torn value.
    // The index is checked against the pinned header above. ADR-0011's
    // exception list records this call (ADR-0055).
    #[allow(unsafe_code)]
    unsafe {
        libmimalloc_sys::mi_option_set(MI_OPTION_PURGE_DELAY, -1);
    }
}

fn main() -> std::process::ExitCode {
    configure_allocator();
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
