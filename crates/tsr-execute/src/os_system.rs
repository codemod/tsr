//! The `System` a real `tsr` process runs against.
//!
//! Ported from `cmd/tsgo/sys.go`. Everything here is a syscall or an
//! environment read; the interesting decisions are where the default library
//! lives and how terminal width is discovered, both of which upstream gets from
//! Go's standard library and neither of which Rust's provides.

use std::io::{IsTerminal, Write};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use tsr_vfs::{FileSystem, OsFileSystem};

use crate::system::System;

/// A `tsr` process: the real disk, the real terminal, the real clock.
pub struct OsSystem {
    fs: OsFileSystem,
    current_directory: String,
    default_library_path: String,
    started: Instant,
    output: std::io::Stdout,
    #[cfg(feature = "work-trace")]
    work_trace: Option<std::sync::Arc<crate::work_trace::WorkTrace>>,
}

impl OsSystem {
    /// Build the host for this process.
    ///
    /// # Panics
    ///
    /// If the current directory cannot be read, which upstream also treats as
    /// unrecoverable: every path the compiler constructs is resolved against it,
    /// so a guess would silently compile the wrong tree.
    #[must_use]
    pub fn new() -> Self {
        let current_directory = std::env::current_dir()
            .map(|path| tsr_path::normalize_slashes(&path.to_string_lossy()))
            .expect("the current directory must be readable");
        Self {
            fs: OsFileSystem::new(),
            current_directory,
            default_library_path: default_library_path(),
            started: Instant::now(),
            output: std::io::stdout(),
            #[cfg(feature = "work-trace")]
            work_trace: open_work_trace(),
        }
    }
}

impl Default for OsSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl System for OsSystem {
    fn write(&mut self, text: &str) {
        // Errors are dropped rather than reported. A closed stdout — `tsr | head`
        // — is the ordinary case, and upstream's `fmt.Fprint` discards the error
        // too; the alternative is a panic in a pipe.
        let _ = self.output.write_all(text.as_bytes());
        let _ = self.output.flush();
    }

    fn fs(&self) -> &dyn FileSystem {
        &self.fs
    }

    fn default_library_path(&self) -> &str {
        &self.default_library_path
    }

    fn current_directory(&self) -> &str {
        &self.current_directory
    }

    fn write_output_is_tty(&self) -> bool {
        std::io::stdout().is_terminal()
    }

    fn width_of_terminal(&self) -> usize {
        // Upstream asks the terminal. Rust's standard library cannot, and the
        // `terminal_size` crate is not in this workspace's bill of materials, so
        // the environment is consulted and 80 is the fallback — the same default
        // `tsc` uses when its own query fails.
        //
        // This is only read by the pretty formatter's frame, so being wrong
        // changes where long lines are elided and nothing else.
        std::env::var("COLUMNS").ok().and_then(|value| value.parse().ok()).unwrap_or(80)
    }

    fn environment_variable(&self, name: &str) -> String {
        std::env::var(name).unwrap_or_default()
    }

    fn since_start(&self) -> Duration {
        self.started.elapsed()
    }

    #[cfg(feature = "work-trace")]
    fn work_trace(&self) -> Option<std::sync::Arc<crate::work_trace::WorkTrace>> {
        self.work_trace.clone()
    }

    #[cfg(feature = "work-trace")]
    fn work_trace_warning(&mut self, message: &str) {
        let _ = writeln!(std::io::stderr(), "warning: TSR work trace incomplete: {message}");
    }
}

#[cfg(feature = "work-trace")]
fn open_work_trace() -> Option<std::sync::Arc<crate::work_trace::WorkTrace>> {
    use crate::work_trace::{TraceIdentity, WorkTrace};

    let path = std::env::var_os("TSR_WORK_TRACE")?;
    if path.is_empty() {
        return None;
    }
    // Never overwrite inputs or reuse a previous invocation's completion marker.
    let file = match std::fs::OpenOptions::new().write(true).create_new(true).open(path) {
        Ok(file) => file,
        Err(error) => {
            let _ = writeln!(std::io::stderr(), "warning: TSR work trace not started: {error}");
            return None;
        }
    };
    let pid = std::process::id();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    Some(std::sync::Arc::new(WorkTrace::new(
        Box::new(std::io::BufWriter::with_capacity(64 * 1024, file)),
        TraceIdentity {
            pid,
            invocation_id: format!("{pid}-{nonce}"),
            source_sha_claim: option_env!("TSR_WORK_TRACE_BUILD_SHA").map(str::to_owned),
            binary_sha256_claim: std::env::var("TSR_WORK_TRACE_BINARY_SHA256").ok(),
        },
    )))
}

/// Where `lib.*.d.ts` is expected to be found.
///
/// **STATUS-cli.md §7.1 asked whether these should be embedded in the binary or
/// found on disk; this answers "on disk", and the reasoning is worth keeping.**
///
/// Embedding via `include_str!` is what upstream effectively does — Go's
/// `bundled` package compiles them in — and it makes the binary relocatable and
/// startup free of I/O. It also adds ~3.9 MB to every build, of which
/// `lib.dom.d.ts` is 2.3 MB, and it makes the libraries un-swappable: a user who
/// wants a newer `lib.esnext.d.ts` than the binary shipped with cannot have one,
/// and `--lib` cannot be pointed anywhere.
///
/// On-disk keeps the choice open and matches the shape upstream's own
/// `DefaultLibraryPath` implies. The cost is a binary that is not self-contained
/// and a search order that has to be documented, which is this function.
///
/// The order, first hit wins:
///
/// 1. `TSR_LIB_PATH`, so a caller can be explicit. There is no upstream
///    counterpart; it exists because the two fallbacks below are guesses and a
///    guess needs an override.
/// 2. `<directory of the executable>/lib`, which is where an installed layout
///    puts them.
/// 3. The vendored submodule path recorded at build time, which is what makes
///    `cargo run` work in this repository without any setup.
///
/// **This would be wrong if** the binary were ever distributed without either
/// its sibling `lib/` directory or the environment variable — at which point
/// every compilation silently loses its global types, which surfaces as
/// thousands of `Cannot find name 'Array'` rather than as a missing-file error.
/// Making that failure loud is `bd`-less item: the driver should report when the
/// library path names nothing.
fn default_library_path() -> String {
    if let Ok(explicit) = std::env::var("TSR_LIB_PATH") {
        if !explicit.is_empty() {
            return tsr_path::normalize_slashes(&explicit);
        }
    }

    if let Ok(executable) = std::env::current_exe() {
        if let Some(directory) = executable.parent() {
            let beside = directory.join("lib");
            if beside.is_dir() {
                return tsr_path::normalize_slashes(&beside.to_string_lossy());
            }
        }
    }

    // The checkout's own submodule. `CARGO_MANIFEST_DIR` is this crate's
    // directory at build time, so two ancestors up is the workspace root.
    let vendored = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .map(|root| root.join("vendor/typescript-go/internal/bundled/libs"));
    vendored
        .filter(|path| path.is_dir())
        .map_or_else(String::new, |path| tsr_path::normalize_slashes(&path.to_string_lossy()))
}
