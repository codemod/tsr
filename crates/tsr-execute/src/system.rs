//! What the driver needs from the world outside the compiler.
//!
//! Ported from `tsc.System` (`internal/execute/tsc/compile.go:17`) and the
//! implementation `cmd/tsgo/sys.go` provides for it.
//!
//! # Why an interface rather than calling `std` directly
//!
//! Because the CLI's oracle is a set of baselines that run a whole compilation
//! against a virtual filesystem, a fixed current directory, a pinned version
//! string and a clock that does not tick (see `STATUS-cli.md` §2). Every one of
//! those is a method here. A driver that reached for `std::env::current_dir()`
//! could not be baselined at all, and the baselines are the only thing that can
//! say whether this port is right.

use std::time::Duration;

use tsr_vfs::FileSystem;

/// The host a `tsc` invocation runs against (`tsc.System`).
pub trait System {
    /// Where output goes. One stream: upstream writes diagnostics, help and
    /// status to the same `Writer()`, and splitting them would change what a
    /// baseline captures.
    fn write(&mut self, text: &str);

    /// The files to compile against.
    fn fs(&self) -> &dyn FileSystem;

    /// Where the bundled `lib.*.d.ts` live (`DefaultLibraryPath`).
    fn default_library_path(&self) -> &str;

    /// What a relative path is resolved against.
    fn current_directory(&self) -> &str;

    /// Whether output is a terminal, which is what `--pretty` defaults to.
    fn write_output_is_tty(&self) -> bool;

    /// How wide the terminal is, for the pretty formatter's frame.
    fn width_of_terminal(&self) -> usize;

    /// An environment variable, or the empty string.
    fn environment_variable(&self, name: &str) -> String;

    /// How long since the process started, for `--diagnostics`.
    ///
    /// Upstream has `Now()` and `SinceStart()`; only the elapsed form is ported,
    /// because nothing here yet prints a wall-clock time and a `Now()` that no
    /// caller uses is a method whose correctness nothing checks.
    fn since_start(&self) -> Duration;

    /// The version this compiler reports.
    ///
    /// A method rather than a constant because upstream's test harness pins it
    /// to `FakeTSVersion` — every `tsc` baseline that prints a banner contains
    /// that string — and a compiler whose version cannot be substituted cannot
    /// be baselined at all. Real hosts take the default.
    fn version(&self) -> &str {
        VERSION
    }
}

/// The version this compiler reports.
///
/// Upstream's `core.Version()`. The baselines pin it to `FakeTSVersion` through
/// the test harness rather than through the compiler, so this is a constant here
/// and the harness substitutes.
pub const VERSION: &str = "5.9.0";
