//! Per-case guard for the corpus dumps: wall time, peak memory, panic capture
//! and a watchdog for a case that runs away (`tsr-2zk.1041`).
//!
//! # Why this exists
//!
//! The integrator's zero-loss gate joins two dumps on their key and verdict
//! columns. A verdict compares diagnostics or type text only, so at `99b337b`
//! `varianceProblingAndZeroOrderIndexSignatureRelationsAlign` scored
//! `EMPTY_RIGHT` in both full dumps while the same case, run alone, grew without
//! bound until the kernel killed it. Its cost was invisible to the gate. The
//! reasoning, the alternatives and their measured costs are in
//! `docs/parity/notes/r5-harness.md`.
//!
//! # What it records
//!
//! [`run_case`] wraps one case's work:
//!
//! - **wall time**, an `Instant` around the closure;
//! - **peak memory**, the bytes the case's thread allocated and still held at
//!   its high point. Counted only when the binary installs the counting
//!   allocator (`examples/support/counting_alloc.rs`), which calls [`charge`];
//!   otherwise reported as 0. A case runs on one rayon worker from start to
//!   finish (the checker spawns no threads and these closures start no nested
//!   parallel work), so a per-thread counter is a per-case counter;
//! - **a panic**. The workspace's release profile sets `panic = "abort"`
//!   (`Cargo.toml`), so in the dumps a panic aborts the process (SIGABRT,
//!   exit 134) before any row is printed, and nothing can catch it. The panic
//!   hook [`install`] adds therefore writes the case's `PANIC` marker row to
//!   stdout first, then lets the abort proceed: the run still fails, but
//!   names its case. Under `panic = "unwind"` (tests, debug builds) the
//!   closure runs under `catch_unwind` and the case returns `Err(message)`,
//!   which the dumps print as the same `PANIC` row and carry on.
//!
//! `TSR_CASE_INJECT=panic:<case>` (or `grow:<case>`) makes the named case
//! panic, or allocate without bound, inside its guard: the self-test that
//! the markers and the watchdog work in the build the gate actually runs
//! (`docs/parity/notes/r5-harness.md` §1).
//!
//! `TSR_CASE_TRACE=1` also prints `START <case>` and `END <case> <ms>` to
//! stderr, so a death no hook sees (a stack overflow's SIGSEGV handler, the
//! kernel's OOM killer) leaves its in-flight cases in the log as the `START`s
//! with no `END`.
//!
//! # The watchdog
//!
//! [`install`] starts one thread that looks at the in-flight cases every
//! 100 ms:
//!
//! - past `TSR_CASE_WARN_S` (default 10 s) it prints `SLOW <case> <s>s
//!   <MiB>MiB` to stderr once, so a later kernel OOM kill or hang has a named
//!   suspect in the log;
//! - past `TSR_CASE_MEM_MIB` (default [`DEFAULT_MEMORY_MIB`]) peak memory, or
//!   past `TSR_CASE_TIMEOUT_S` (default 600 s) wall time, or when the cases in
//!   flight together hold more than `TSR_DUMP_MEM_MIB` (default three quarters
//!   of `MemTotal`; the largest of them is blamed), it writes the case's
//!   marker row (`OOM` or `TIMEOUT`) to stdout, names every other in-flight
//!   case on stderr, and exits the process with status 3. A thread cannot be
//!   stopped from outside and a case that will not finish can never contribute
//!   a real row, so the run ends loudly and attributed rather than being
//!   OOM-killed with an empty dump.

use std::cell::Cell;
use std::io::Write as _;
use std::panic::AssertUnwindSafe;
use std::sync::atomic::{AtomicI64, Ordering::Relaxed};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

/// Per-case memory budget when `TSR_CASE_MEM_MIB` is unset, in MiB.
///
/// The largest cases on the full corpus at `1252ab9` peak at 3,456 MiB (the
/// two `varianceProbling…` cases; `docs/parity/notes/r5-harness.md` §1 has
/// the distribution), so this leaves them 1.8× headroom while still acting
/// well before the kernel would on the 15 GiB box the dumps run on. It is a
/// tripwire for unbounded growth, not the gate's budget: `slowcases` flags a
/// case past 1 GiB.
pub const DEFAULT_MEMORY_MIB: u64 = 6144;

/// Memory a case's thread holds, in bytes, relative to the case's start.
///
/// Only the owning thread writes (load + store rather than read-modify-write,
/// so an allocation pays no locked instruction); the watchdog reads.
#[derive(Default)]
struct CaseMemory {
    live: AtomicI64,
    peak: AtomicI64,
}

thread_local! {
    /// The case this thread is running, if any. `const`-initialised and
    /// without a destructor, so the allocator's access never allocates and
    /// never registers anything. The counters are leaked (16 bytes per case,
    /// about 200 KiB over the corpus) so the reference is `'static`.
    static CURRENT: Cell<Option<&'static CaseMemory>> = const { Cell::new(None) };
}

/// Record `delta` bytes allocated (positive) or freed (negative) on this
/// thread against the case it is running. Called by the counting allocator on
/// every allocation; a no-op outside a case.
///
/// `try_with` because the allocator also runs while thread-local storage is
/// torn down at thread exit.
#[inline]
pub fn charge(delta: i64) {
    let _ = CURRENT.try_with(|current| {
        let Some(memory) = current.get() else { return };
        let live = memory.live.load(Relaxed) + delta;
        memory.live.store(live, Relaxed);
        if live > memory.peak.load(Relaxed) {
            memory.peak.store(live, Relaxed);
        }
    });
}

/// One case's measured outcome.
pub struct Measured<T> {
    /// The closure's value, or the panic message it unwound with.
    pub value: Result<T, String>,
    /// Wall time in milliseconds.
    pub wall_ms: u128,
    /// Peak memory held by the case's thread, in MiB (0 without the counting
    /// allocator).
    pub peak_mib: i64,
}

impl<T> Measured<T> {
    /// The two trailing columns every guarded dump row carries:
    /// `ms=<wall>\tmib=<peak>`. Tagged so a reader finds them from the right
    /// without guessing whether a type-text column is a number.
    #[must_use]
    pub fn columns(&self) -> String {
        format!("ms={}\tmib={}", self.wall_ms, self.peak_mib)
    }
}

/// A case the watchdog can see.
struct InFlight {
    id: u64,
    name: String,
    thread: std::thread::ThreadId,
    start: Instant,
    memory: &'static CaseMemory,
    warned: bool,
}

/// The marker row a fatal watchdog verdict writes to stdout, by dump format.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RowShape {
    /// `examples/diagverdictdump.rs`: `case<TAB>VERDICT<TAB>expected<TAB>actual`.
    Diagnostics,
    /// `examples/verdictdump.rs`: `case:file:position<TAB>VERDICT<TAB>want<TAB>got`.
    Types,
}

impl RowShape {
    /// The row for `case` ending with `verdict` (`PANIC`, `OOM`, `TIMEOUT`).
    /// A types marker is keyed `case:*:*`: the case has no aligned rows, and
    /// the key still splits back to the case name as a positional key does.
    #[must_use]
    pub fn marker(self, case: &str, verdict: &str, detail: &str, columns: &str) -> String {
        let detail = detail.replace(['\t', '\n'], " ");
        match self {
            RowShape::Diagnostics => format!("{case}\t{verdict}\t-\t{detail}\t{columns}"),
            RowShape::Types => format!("{case}:*:*\t{verdict}\t-\t{detail}\t{columns}"),
        }
    }
}

struct Watchdog {
    shape: RowShape,
    warn: Duration,
    timeout: Duration,
    memory_bytes: i64,
    total_bytes: i64,
    in_flight: Mutex<Vec<InFlight>>,
}

static WATCHDOG: OnceLock<Watchdog> = OnceLock::new();
static NEXT_ID: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

fn env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name).ok().and_then(|value| value.trim().parse().ok()).unwrap_or(default)
}

/// Start the watchdog for a dump whose rows have `shape`. Idempotent; a run
/// without it still measures every case but has no watchdog.
pub fn install(shape: RowShape) {
    let mut started = false;
    let watchdog = WATCHDOG.get_or_init(|| {
        started = true;
        Watchdog {
            shape,
            warn: Duration::from_secs(env_u64("TSR_CASE_WARN_S", 10)),
            timeout: Duration::from_secs(env_u64("TSR_CASE_TIMEOUT_S", 600)),
            memory_bytes: i64::try_from(env_u64("TSR_CASE_MEM_MIB", DEFAULT_MEMORY_MIB))
                .unwrap_or(i64::MAX)
                .saturating_mul(1 << 20),
            total_bytes: std::env::var("TSR_DUMP_MEM_MIB")
                .ok()
                .and_then(|value| value.trim().parse::<i64>().ok())
                .map_or_else(default_total_bytes, |value| value.saturating_mul(1 << 20)),
            in_flight: Mutex::new(Vec::new()),
        }
    });
    if started {
        if cfg!(panic = "abort") {
            let previous = std::panic::take_hook();
            std::panic::set_hook(Box::new(move |info| {
                mark_panicking_case(watchdog, &panic_message(info.payload()));
                previous(info);
            }));
        }
        std::thread::Builder::new()
            .name("case-watchdog".into())
            .spawn(move || watch(watchdog))
            .expect("spawning the case watchdog");
    }
}

/// Three quarters of `MemTotal`: the whole-process budget when
/// `TSR_DUMP_MEM_MIB` is unset. Unlimited where `/proc/meminfo` is absent.
fn default_total_bytes() -> i64 {
    let total_kib = std::fs::read_to_string("/proc/meminfo").ok().and_then(|text| {
        let line = text.lines().find(|line| line.starts_with("MemTotal:"))?;
        line.split_whitespace().nth(1)?.parse::<i64>().ok()
    });
    total_kib.map_or(i64::MAX, |kib| kib.saturating_mul(1024) / 4 * 3)
}

fn mib(bytes: i64) -> i64 {
    bytes >> 20
}

fn watch(watchdog: &'static Watchdog) {
    loop {
        std::thread::sleep(Duration::from_millis(100));
        let mut in_flight =
            watchdog.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let mut fatal = None;
        for case in in_flight.iter_mut() {
            let elapsed = case.start.elapsed();
            let peak = case.memory.peak.load(Relaxed);
            if !case.warned && elapsed >= watchdog.warn {
                case.warned = true;
                eprintln!("SLOW {} {}s {}MiB", case.name, elapsed.as_secs(), mib(peak));
            }
            if peak > watchdog.memory_bytes {
                fatal = Some((case.id, "OOM"));
            } else if elapsed >= watchdog.timeout {
                fatal = Some((case.id, "TIMEOUT"));
            }
        }
        // Several cases can each stay under their own budget and still
        // exhaust the host together; the kernel would then kill the run
        // unattributed. Stop the one holding the most.
        let live: i64 = in_flight.iter().map(|case| case.memory.live.load(Relaxed).max(0)).sum();
        if fatal.is_none() && live > watchdog.total_bytes {
            fatal = in_flight
                .iter()
                .max_by_key(|case| case.memory.live.load(Relaxed))
                .map(|case| (case.id, "OOM"));
        }
        let Some((id, verdict)) = fatal else { continue };
        let mut stderr = std::io::stderr().lock();
        for case in in_flight.iter() {
            let report = format!(
                "{} {} after {}s at {}MiB",
                if case.id == id { verdict } else { "IN_FLIGHT" },
                case.name,
                case.start.elapsed().as_secs(),
                mib(case.memory.peak.load(Relaxed))
            );
            let _ = writeln!(stderr, "{report}");
        }
        let case = in_flight.iter().find(|case| case.id == id).expect("fatal case is in flight");
        let detail = format!(
            "watchdog: {}MiB peak (budget {}MiB), {}MiB live in all cases (budget {}MiB), \
             {}s wall (timeout {}s)",
            mib(case.memory.peak.load(Relaxed)),
            mib(watchdog.memory_bytes),
            mib(live),
            mib(watchdog.total_bytes),
            case.start.elapsed().as_secs(),
            watchdog.timeout.as_secs()
        );
        let columns = format!(
            "ms={}\tmib={}",
            case.start.elapsed().as_millis(),
            mib(case.memory.peak.load(Relaxed))
        );
        let row = watchdog.shape.marker(&case.name, verdict, &detail, &columns);
        let mut stdout = std::io::stdout().lock();
        let _ = writeln!(stdout, "{row}");
        let _ = stdout.flush();
        let _ = stderr.flush();
        std::process::exit(3);
    }
}

/// Under `panic = "abort"`: write the `PANIC` marker row of the case this
/// thread is running, before the runtime aborts. `try_lock` because the panic
/// may have come from inside the registry's own critical section.
fn mark_panicking_case(watchdog: &Watchdog, message: &str) {
    let Ok(in_flight) = watchdog.in_flight.try_lock() else { return };
    let thread = std::thread::current().id();
    let Some(case) = in_flight.iter().find(|case| case.thread == thread) else { return };
    let columns = format!(
        "ms={}\tmib={}",
        case.start.elapsed().as_millis(),
        mib(case.memory.peak.load(Relaxed))
    );
    let row = watchdog.shape.marker(&case.name, "PANIC", message, &columns);
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{row}");
    let _ = stdout.flush();
    eprintln!("PANIC {} after {}ms", case.name, case.start.elapsed().as_millis());
}

/// Removes the case from the watchdog's view and clears this thread's slot,
/// on return and on unwind alike.
struct Running {
    id: u64,
}

impl Drop for Running {
    fn drop(&mut self) {
        let _ = CURRENT.try_with(|current| current.set(None));
        if let Some(watchdog) = WATCHDOG.get() {
            let mut in_flight =
                watchdog.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
            in_flight.retain(|case| case.id != self.id);
        }
    }
}

fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(message) = payload.downcast_ref::<&str>() {
        (*message).to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

/// The `TSR_CASE_INJECT` fault, if it names this case.
fn inject(name: &str) {
    static FAULT: OnceLock<Option<(String, String)>> = OnceLock::new();
    let fault = FAULT.get_or_init(|| {
        let value = std::env::var("TSR_CASE_INJECT").ok()?;
        let (kind, case) = value.split_once(':')?;
        Some((kind.to_string(), case.to_string()))
    });
    let Some((kind, case)) = fault else { return };
    if case != name {
        return;
    }
    match kind.as_str() {
        "panic" => panic!("TSR_CASE_INJECT panic in {name}"),
        "grow" => {
            let mut held: Vec<Vec<u8>> = Vec::new();
            loop {
                held.push(vec![1; 64 << 20]);
                std::thread::sleep(Duration::from_millis(10));
            }
        }
        _ => {}
    }
}

fn trace_enabled() -> bool {
    static TRACE: OnceLock<bool> = OnceLock::new();
    *TRACE.get_or_init(|| std::env::var_os("TSR_CASE_TRACE").is_some_and(|value| value != "0"))
}

/// Run one case's work under the guard. See the module docs.
pub fn run_case<T>(name: &str, work: impl FnOnce() -> T) -> Measured<T> {
    let memory: &'static CaseMemory = Box::leak(Box::default());
    let id = NEXT_ID.fetch_add(1, Relaxed);
    let start = Instant::now();
    if let Some(watchdog) = WATCHDOG.get() {
        let mut in_flight =
            watchdog.in_flight.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        in_flight.push(InFlight {
            id,
            name: name.to_string(),
            thread: std::thread::current().id(),
            start,
            memory,
            warned: false,
        });
    }
    let running = Running { id };
    let trace = trace_enabled();
    if trace {
        eprintln!("START {name}");
    }
    CURRENT.with(|current| current.set(Some(memory)));
    let value = std::panic::catch_unwind(AssertUnwindSafe(|| {
        inject(name);
        work()
    }))
    .map_err(|payload| panic_message(payload.as_ref()));
    drop(running);
    if trace {
        eprintln!("END {name} {}", start.elapsed().as_millis());
    }
    Measured {
        value,
        wall_ms: start.elapsed().as_millis(),
        peak_mib: mib(memory.peak.load(Relaxed)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_panicking_case_is_returned_not_propagated() {
        let measured = run_case("probe", || -> u32 { panic!("boom") });
        assert_eq!(measured.value.err().as_deref(), Some("boom"));
        // The slot is cleared on unwind: charging after the case is a no-op.
        charge(1 << 30);
        assert!(CURRENT.with(|current| current.get().is_none()));
    }

    #[test]
    fn charges_count_against_the_running_case_only() {
        let measured = run_case("probe", || {
            charge(3 << 20);
            charge(-(2 << 20));
            charge(1 << 20);
            7
        });
        assert_eq!(measured.value, Ok(7));
        assert_eq!(measured.peak_mib, 3);
    }

    #[test]
    fn markers_keep_the_join_key_shape() {
        let diag = RowShape::Diagnostics.marker("compiler/a", "OOM", "x\ty", "ms=1\tmib=2");
        assert_eq!(diag, "compiler/a\tOOM\t-\tx y\tms=1\tmib=2");
        let types = RowShape::Types.marker("compiler/a", "PANIC", "boom", "ms=1\tmib=2");
        assert_eq!(types.split('\t').next(), Some("compiler/a:*:*"));
    }
}
