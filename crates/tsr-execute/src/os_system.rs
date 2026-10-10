//! The `System` a real `tsr` process runs against.
//!
//! Ported from `cmd/tsgo/sys.go`. Everything here is a syscall or an
//! environment read; the interesting decisions are where the default library
//! lives and how terminal width is discovered, both of which upstream gets from
//! Go's standard library and neither of which Rust's provides.

use std::io::{IsTerminal, Write};
use std::time::{Duration, Instant};

use tsr_vfs::{BundledFileSystem, FileSystem, OsFileSystem};

use crate::system::System;

/// A `tsr` process: the real disk, the real terminal, the real clock.
pub struct OsSystem {
    fs: BundledFileSystem<OsFileSystem>,
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
            fs: BundledFileSystem::new(OsFileSystem::new()),
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

    /// The `tsr` binary runs one command per process.
    fn exits_after_command(&self) -> bool {
        true
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

/// Where `lib.*.d.ts` is found (`bundled.LibPath`, `cmd/tsgo/sys.go:72`).
///
/// The libraries are compiled into the binary, as native's default `!noembed`
/// build does (`internal/bundled/embed.go`), and mounted at
/// `bundled:///libs` by [`tsr_vfs::BundledFileSystem`]. That makes the binary
/// relocatable, keeps library loading free of file I/O, and prints the same
/// library paths native prints (`--listFiles`, diagnostics located in a
/// library).
///
/// `TSR_LIB_PATH` overrides it with an on-disk directory. There is no
/// upstream counterpart (native's equivalent is a `noembed` build); it exists
/// so a caller can check against libraries other than the pinned ones.
fn default_library_path() -> String {
    match std::env::var("TSR_LIB_PATH") {
        Ok(explicit) if !explicit.is_empty() => tsr_path::normalize_slashes(&explicit),
        _ => tsr_vfs::bundled::LIB_PATH.to_string(),
    }
}
