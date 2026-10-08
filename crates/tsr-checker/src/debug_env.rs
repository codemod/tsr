//! Developer trace switches, read from the environment once per process.
//!
//! The checker's diagnostic probes (`TSR_PROJ_TRACE`, `TSR_CTX_DEBUG`, …) sit
//! on hot paths: `TSR_PROJ_TRACE` alone was tested on every property read,
//! 95,917 `std::env::var` calls and 2.5% of all instructions on a 100-module
//! project (`docs/parity/notes/perf.md` §8 C2). `std::env::var` takes the
//! process environment lock and scans every variable, and it allocates the
//! value. These switches are only ever set when the process starts, so one
//! read is equivalent. No typescript-go counterpart: these are port-only
//! probes and do not affect output.

use std::sync::OnceLock;

/// Every switch read through [`var`]. A name missing here is a programming
/// error caught in debug builds.
const NAMES: [&str; 5] =
    ["TSR_PROJ_TRACE", "TSR_CTX_DEBUG", "TSR_TRACE_LOOP", "TSR_DBG", "TSR_DEBUG_832"];

/// The value of the developer switch `name`, read once per process.
pub(crate) fn var(name: &'static str) -> Option<&'static str> {
    static VALUES: OnceLock<[Option<String>; NAMES.len()]> = OnceLock::new();
    let values = VALUES.get_or_init(|| NAMES.map(|name| std::env::var(name).ok()));
    let index = NAMES.iter().position(|&known| known == name);
    debug_assert!(index.is_some(), "unregistered debug switch {name}");
    values[index?].as_deref()
}

/// Whether the developer switch `name` is set.
pub(crate) fn is_set(name: &'static str) -> bool {
    var(name).is_some()
}
